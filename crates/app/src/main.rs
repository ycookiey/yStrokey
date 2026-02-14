mod autostart;
mod logger;
mod settings_io;
mod settings_window;
mod tray;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::core::HSTRING;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::{
    AppConfig, ClipboardContent, ClipboardEvent, ConfigError, DiagnosticsLevel, DisplayState,
    GhostModifier, InputEvent, MenuLanguage,
};
use ystrokey_input::{install_keyboard_hook, is_privacy_target, poll_ime_state, ClipboardListener};
use ystrokey_render::{get_monitor_device_name, D2DRenderer, OsdWindow};

use tray::{
    show_context_menu, ID_TRAY_AUTOSTART, ID_TRAY_EXIT, ID_TRAY_EXPORT, ID_TRAY_IMPORT,
    ID_TRAY_SETTINGS, ID_TRAY_TOGGLE, WM_TRAYICON,
};

const HOTKEY_TOGGLE_ID: i32 = 1;

/// wnd_proc からイベント送信用のグローバルチャネル
static EVENT_TX: OnceLock<SyncSender<InputEvent>> = OnceLock::new();

/// OSD 有効/無効（トレイメニューから切替）
static OSD_ENABLED: AtomicBool = AtomicBool::new(true);

/// 設定ファイルパス（wnd_proc からアクセス用）
static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 現在の設定（wnd_proc からアクセス用）
static CURRENT_CONFIG: OnceLock<Mutex<AppConfig>> = OnceLock::new();

/// Ghost-mode でインタラクティブ状態かどうか（ドラッグ判定用）
static GHOST_INTERACTIVE: AtomicBool = AtomicBool::new(false);

// クリップボード重複検知用（wnd_proc はメインスレッドのみで呼ばれる）
thread_local! {
    static LAST_CLIPBOARD: RefCell<String> = const { RefCell::new(String::new()) };
}

/// WM_CLIPBOARDUPDATE (Windows Vista+)
const WM_CLIPBOARD_UPDATE: u32 = 0x031D;

enum ApplyReason {
    Startup,
    HotReload,
    UiEdit,
}

struct RuntimeIntervals {
    frame_duration: Duration,
    ime_poll_interval: Duration,
    config_reload_interval: Duration,
}

/// 致命的エラー時にメッセージボックスを表示して終了
fn fatal_error(msg: &str) -> ! {
    logger::log(DiagnosticsLevel::Error, msg);
    unsafe {
        let text = HSTRING::from(msg);
        let caption = HSTRING::from("yStrokey Error");
        MessageBoxW(None, &text, &caption, MB_OK | MB_ICONERROR);
    }
    std::process::exit(1);
}

unsafe extern "system" fn app_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        m if m == WM_TRAYICON => {
            let mouse_msg = lparam.0 as u32;
            if mouse_msg == WM_RBUTTONUP {
                let (menu_lang, osd_enabled) = current_tray_status();
                show_context_menu(
                    hwnd,
                    menu_lang,
                    osd_enabled,
                    autostart::is_autostart_enabled(),
                );
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd_id = (wparam.0 & 0xFFFF) as u32;
            match cmd_id {
                ID_TRAY_TOGGLE => {
                    let prev = OSD_ENABLED.load(Ordering::Relaxed);
                    OSD_ENABLED.store(!prev, Ordering::Relaxed);
                }
                ID_TRAY_AUTOSTART => {
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(mut cfg) = cfg_mutex.lock() {
                            let next = !cfg.startup.autostart_enabled;
                            if autostart::set_autostart(next).is_ok() {
                                cfg.startup.autostart_enabled = next;
                                if let Some(path) = CONFIG_PATH.get() {
                                    let _ = cfg.save_atomic(path);
                                }
                                if let Some(tx) = EVENT_TX.get() {
                                    let _ = tx.try_send(InputEvent::ConfigChanged);
                                }
                            } else {
                                logger::log(
                                    DiagnosticsLevel::Warn,
                                    "Failed to toggle auto start from tray",
                                );
                            }
                        }
                    }
                }
                ID_TRAY_SETTINGS => {
                    if let (Some(path), Some(cfg_mutex)) = (CONFIG_PATH.get(), CURRENT_CONFIG.get()) {
                        if let Ok(cfg) = cfg_mutex.lock() {
                            let notify_tx = EVENT_TX.get().cloned();
                            settings_window::open_settings_window(&cfg, path, notify_tx);
                        }
                    }
                }
                ID_TRAY_EXPORT => {
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        let cfg_clone = cfg_mutex.lock().ok().map(|c| c.clone());
                        if let Some(cfg) = cfg_clone {
                            if let Err(e) = settings_io::export_config(&cfg) {
                                logger::log(
                                    DiagnosticsLevel::Warn,
                                    &format!("Config export failed: {e}"),
                                );
                            }
                        }
                    }
                }
                ID_TRAY_IMPORT => {
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(Some(new_cfg)) = settings_io::import_config() {
                            if let Some(path) = CONFIG_PATH.get() {
                                if let Err(e) = new_cfg.save_atomic(path) {
                                    logger::log(
                                        DiagnosticsLevel::Error,
                                        &format!("Failed to persist imported config: {e}"),
                                    );
                                    return LRESULT(0);
                                }
                            }
                            if let Ok(mut cfg) = cfg_mutex.lock() {
                                *cfg = new_cfg;
                            }
                            if let Some(tx) = EVENT_TX.get() {
                                let _ = tx.try_send(InputEvent::ConfigChanged);
                            }
                        }
                    }
                }
                ID_TRAY_EXIT => {
                    if should_confirm_exit() {
                        let yes = MessageBoxW(
                            None,
                            &HSTRING::from(exit_confirm_text()),
                            &HSTRING::from("yStrokey"),
                            MB_ICONQUESTION | MB_YESNO,
                        );
                        if yes != IDYES {
                            return LRESULT(0);
                        }
                    }
                    PostQuitMessage(0);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLIPBOARD_UPDATE => {
            if let Some(tx) = EVENT_TX.get() {
                if let Some(text) = ClipboardListener::get_text(hwnd) {
                    let changed = LAST_CLIPBOARD.with(|cell| {
                        let prev = cell.borrow();
                        text != *prev
                    });
                    if changed {
                        LAST_CLIPBOARD.with(|cell| {
                            *cell.borrow_mut() = text.clone();
                        });
                        let event = InputEvent::Clipboard(ClipboardEvent {
                            content: ClipboardContent::Text(text),
                            timestamp: Instant::now(),
                        });
                        let _ = tx.try_send(event);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if GHOST_INTERACTIVE.load(Ordering::Relaxed) {
                let _ = ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            save_current_position(hwnd);
            LRESULT(0)
        }
        WM_HOTKEY => {
            if wparam.0 as i32 == HOTKEY_TOGGLE_ID {
                let prev = OSD_ENABLED.load(Ordering::Relaxed);
                OSD_ENABLED.store(!prev, Ordering::Relaxed);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let new_dpi = (wparam.0 >> 16) as u32;
            let suggested = lparam.0 as *const RECT;
            if !suggested.is_null() {
                let r = &*suggested;
                if let Some(tx) = EVENT_TX.get() {
                    let _ = tx.try_send(InputEvent::DpiChanged {
                        dpi: new_dpi,
                        suggested_rect: [r.left, r.top, r.right, r.bottom],
                    });
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn current_tray_status() -> (MenuLanguage, bool) {
    let menu_lang = CURRENT_CONFIG
        .get()
        .and_then(|m| m.lock().ok())
        .map(|cfg| cfg.tray.menu_language)
        .unwrap_or(MenuLanguage::Ja);
    let osd_enabled = OSD_ENABLED.load(Ordering::Relaxed);
    (menu_lang, osd_enabled)
}

fn should_confirm_exit() -> bool {
    CURRENT_CONFIG
        .get()
        .and_then(|m| m.lock().ok())
        .map(|cfg| cfg.tray.confirm_on_exit)
        .unwrap_or(false)
}

fn exit_confirm_text() -> &'static str {
    let lang = CURRENT_CONFIG
        .get()
        .and_then(|m| m.lock().ok())
        .map(|cfg| cfg.tray.menu_language)
        .unwrap_or(MenuLanguage::En);

    match lang {
        MenuLanguage::Ja => "yStrokey を終了しますか？",
        MenuLanguage::En => "Exit yStrokey?",
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")))
        .unwrap_or_else(|| PathBuf::from("config.json"));

    let base_dir = config_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut config = load_config_with_recovery(&config_path);

    logger::init(&base_dir, &config.diagnostics);
    logger::log(DiagnosticsLevel::Info, "Application startup");

    let _ = CONFIG_PATH.set(config_path.clone());
    let _ = CURRENT_CONFIG.set(Mutex::new(config.clone()));

    let mut window = OsdWindow::create(
        config.performance.osd_width,
        config.performance.osd_height,
        &config.display,
    )
    .unwrap_or_else(|e| fatal_error(&format!("OSD window creation failed: {e}")));

    unsafe {
        SetWindowLongPtrW(window.hwnd(), GWL_WNDPROC, app_wnd_proc as usize as isize);
    }

    let mut renderer = D2DRenderer::new(&config.style)
        .unwrap_or_else(|e| fatal_error(&format!("D2D renderer creation failed: {e}")));
    renderer.update_dpi(window.dpi);

    let mut state = DisplayState::new(&config);
    let mut intervals = RuntimeIntervals {
        frame_duration: Duration::from_millis(config.performance.frame_interval_ms),
        ime_poll_interval: Duration::from_millis(config.performance.ime_poll_interval_ms),
        config_reload_interval: Duration::from_millis(config.performance.config_reload_interval_ms),
    };

    apply_config(
        ApplyReason::Startup,
        &config,
        &mut state,
        &mut renderer,
        &mut window,
        &mut intervals,
    );

    let (tx, rx) = mpsc::sync_channel::<InputEvent>(256);
    let _ = EVENT_TX.set(tx.clone());

    let _hook_thread = install_keyboard_hook(tx.clone());

    let _clipboard_listener = match ClipboardListener::new(window.hwnd()) {
        Ok(listener) => Some(listener),
        Err(e) => {
            logger::log(DiagnosticsLevel::Warn, &format!("clipboard listener failed: {e}"));
            None
        }
    };

    let _tray = tray::TrayIcon::new(window.hwnd())
        .unwrap_or_else(|e| fatal_error(&format!("Tray icon creation failed: {e}")));

    let mut msg = MSG::default();
    let mut last_ime_poll = Instant::now();
    let mut last_config_check = Instant::now();
    let mut privacy_active = false;
    let mut was_rendering = false;
    let mut last_foreground_hwnd = HWND::default();

    loop {
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let enabled = OSD_ENABLED.load(Ordering::Relaxed);
        while let Ok(event) = rx.try_recv() {
            match &event {
                InputEvent::DpiChanged { dpi, suggested_rect } => {
                    let rect = RECT {
                        left: suggested_rect[0],
                        top: suggested_rect[1],
                        right: suggested_rect[2],
                        bottom: suggested_rect[3],
                    };
                    window.update_for_dpi(*dpi, &rect);
                    renderer.update_dpi(*dpi);
                    continue;
                }
                InputEvent::ConfigChanged => {
                    if let Some(path) = CONFIG_PATH.get() {
                        match AppConfig::load_strict(path) {
                            Ok(new_config) => {
                                apply_config(
                                    ApplyReason::UiEdit,
                                    &new_config,
                                    &mut state,
                                    &mut renderer,
                                    &mut window,
                                    &mut intervals,
                                );
                                if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                                    if let Ok(mut cfg) = cfg_mutex.lock() {
                                        *cfg = new_config.clone();
                                    }
                                }
                                config = new_config;
                            }
                            Err(e) => logger::log(
                                DiagnosticsLevel::Warn,
                                &format!("ConfigChanged reload failed: {e}"),
                            ),
                        }
                    }
                    continue;
                }
                _ => {}
            }
            if enabled && !privacy_active {
                state.process_event(event);
            }
        }

        let now = Instant::now();
        if now.duration_since(last_ime_poll) >= intervals.ime_poll_interval {
            let fg = unsafe { GetForegroundWindow() };
            if fg != last_foreground_hwnd {
                last_foreground_hwnd = fg;
                let prev_privacy = privacy_active;
                privacy_active = is_privacy_target(&config.privacy);
                if privacy_active && !prev_privacy {
                    state.clear();
                }
                if !fg.0.is_null() {
                    window.reposition_to_monitor(fg, &config.display);
                }
            }
            if enabled && !privacy_active {
                poll_ime_state(&tx);
            }
            last_ime_poll = now;
        }

        if now.duration_since(last_config_check) >= intervals.config_reload_interval {
            match config.check_reload(&config_path) {
                Ok(Some(new_config)) => {
                    apply_config(
                        ApplyReason::HotReload,
                        &new_config,
                        &mut state,
                        &mut renderer,
                        &mut window,
                        &mut intervals,
                    );
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(mut cfg) = cfg_mutex.lock() {
                            *cfg = new_config.clone();
                        }
                    }
                    config = new_config;
                }
                Ok(None) => {}
                Err(e) => logger::log(
                    DiagnosticsLevel::Warn,
                    &format!("Hot reload skipped (invalid config): {e}"),
                ),
            }
            last_config_check = now;
        }

        state.tick(Instant::now());

        let has_items = !state.active_items().is_empty();

        if has_items || was_rendering {
            let items = state.active_items();
            let ghost_opacity = calculate_ghost_opacity(&window, &config);
            let interactive = ghost_opacity > 0.0 && is_cursor_in_rect(&window.get_rect());
            GHOST_INTERACTIVE.store(interactive, Ordering::Relaxed);
            window.set_interactive(interactive);

            if let Err(e) = renderer.render(
                items,
                &config.style,
                window.mem_dc(),
                window.width() as u32,
                window.height() as u32,
                ghost_opacity,
            ) {
                logger::log(DiagnosticsLevel::Warn, &format!("Render error: {e}"));
                if let Ok(new_renderer) = D2DRenderer::new(&config.style) {
                    renderer = new_renderer;
                    renderer.update_dpi(window.dpi);
                }
            }
            window.present(config.style.opacity);

            was_rendering = has_items;
            std::thread::sleep(intervals.frame_duration);
        } else {
            unsafe {
                MsgWaitForMultipleObjects(None, false, 50, QS_ALLINPUT);
            }
        }
    }
}

fn apply_config(
    reason: ApplyReason,
    config: &AppConfig,
    state: &mut DisplayState,
    renderer: &mut D2DRenderer,
    window: &mut OsdWindow,
    intervals: &mut RuntimeIntervals,
) {
    state.update_config(config);
    renderer.update_style(&config.style);
    window.set_display_affinity(config.behavior.exclude_from_capture);

    if window.width() != config.performance.osd_width || window.height() != config.performance.osd_height {
        window.resize(config.performance.osd_width, config.performance.osd_height);
    }

    intervals.frame_duration = Duration::from_millis(config.performance.frame_interval_ms);
    intervals.ime_poll_interval = Duration::from_millis(config.performance.ime_poll_interval_ms);
    intervals.config_reload_interval =
        Duration::from_millis(config.performance.config_reload_interval_ms);

    unsafe {
        let _ = UnregisterHotKey(window.hwnd(), HOTKEY_TOGGLE_ID);
    }
    register_toggle_hotkey(window.hwnd(), &config.hotkey.toggle);

    logger::update_config(&config.diagnostics);

    if autostart::set_autostart(config.startup.autostart_enabled).is_err() {
        logger::log(
            DiagnosticsLevel::Warn,
            "Failed to apply startup.autostart_enabled",
        );
    }

    if matches!(reason, ApplyReason::Startup) {
        OSD_ENABLED.store(config.tray.start_osd_enabled, Ordering::Relaxed);
    }
}

fn load_config_with_recovery(config_path: &Path) -> AppConfig {
    match AppConfig::load_strict(config_path) {
        Ok(cfg) => cfg,
        Err(ConfigError::IoError(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            AppConfig::create_default(config_path).unwrap_or_else(|err| {
                fatal_error(&format!("Failed to create default config: {err}"))
            })
        }
        Err(err) => {
            let backup_result = backup_invalid_config(config_path);
            match &backup_result {
                Ok(path) => eprintln!("invalid config backed up to {}", path.display()),
                Err(e) => eprintln!("backup failed: {}", e),
            }

            eprintln!("invalid config recovered with defaults: {}", err);
            AppConfig::create_default(config_path).unwrap_or_else(|create_err| {
                fatal_error(&format!(
                    "Failed to recover invalid config (original: {err}, recover: {create_err})"
                ))
            })
        }
    }
}

fn backup_invalid_config(config_path: &Path) -> Result<PathBuf, std::io::Error> {
    if !config_path.exists() {
        return Ok(config_path.to_path_buf());
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let file_name = config_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    let backup_name = format!("{}.invalid.{}.json", file_name, stamp);
    let backup_path = config_path.with_file_name(backup_name);

    std::fs::rename(config_path, &backup_path)?;
    Ok(backup_path)
}

/// Modifier key + cursor distance determines ghost opacity.
fn calculate_ghost_opacity(window: &OsdWindow, config: &AppConfig) -> f32 {
    unsafe {
        let modifier_down = match config.animation.ghost_modifier {
            GhostModifier::Ctrl => (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16) & 0x8000 != 0,
            GhostModifier::Alt => (GetAsyncKeyState(VK_MENU.0 as i32) as u16) & 0x8000 != 0,
            GhostModifier::Shift => (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16) & 0x8000 != 0,
        };
        if !modifier_down {
            return 0.0;
        }

        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() {
            return 0.0;
        }

        let rect = window.get_rect();
        let distance = distance_to_rect(&cursor, &rect);

        let threshold = config.animation.ghost_threshold_px.max(1.0);
        let base = (1.0 - distance / threshold).clamp(0.0, 1.0);
        (base * config.animation.ghost_max_opacity).clamp(0.0, 1.0)
    }
}

/// Cursor distance to rectangle (0 when inside).
fn distance_to_rect(cursor: &POINT, rect: &RECT) -> f32 {
    let dx = if cursor.x < rect.left {
        rect.left - cursor.x
    } else if cursor.x > rect.right {
        cursor.x - rect.right
    } else {
        0
    };
    let dy = if cursor.y < rect.top {
        rect.top - cursor.y
    } else if cursor.y > rect.bottom {
        cursor.y - rect.bottom
    } else {
        0
    };
    ((dx * dx + dy * dy) as f32).sqrt()
}

/// Check whether cursor is inside rectangle.
fn is_cursor_in_rect(rect: &RECT) -> bool {
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            cursor.x >= rect.left
                && cursor.x <= rect.right
                && cursor.y >= rect.top
                && cursor.y <= rect.bottom
        } else {
            false
        }
    }
}

/// Save current window position to config file.
fn save_current_position(hwnd: HWND) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }

        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if let Some(device_name) = get_monitor_device_name(hmon) {
            if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                if let Ok(mut cfg) = cfg_mutex.lock() {
                    cfg.display
                        .monitor_positions
                        .insert(device_name, [rect.left, rect.top]);
                    if let Some(path) = CONFIG_PATH.get() {
                        if let Err(e) = cfg.save_atomic(path) {
                            logger::log(
                                DiagnosticsLevel::Warn,
                                &format!("Failed to save monitor position: {e}"),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Parse hotkey string and register with RegisterHotKey.
fn register_toggle_hotkey(hwnd: HWND, hotkey_str: &str) {
    if hotkey_str.is_empty() {
        return;
    }

    let Some((modifiers, vk)) = parse_hotkey(hotkey_str) else {
        logger::log(DiagnosticsLevel::Warn, &format!("invalid hotkey: {}", hotkey_str));
        return;
    };

    unsafe {
        if RegisterHotKey(hwnd, HOTKEY_TOGGLE_ID, modifiers, vk).is_err() {
            logger::log(
                DiagnosticsLevel::Warn,
                &format!("RegisterHotKey failed for: {}", hotkey_str),
            );
        }
    }
}

/// Convert hotkey string to (MOD_*, VK).
fn parse_hotkey(s: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut modifiers = MOD_NOREPEAT;
    let mut vk = None;

    for part in s.split('+') {
        match part.trim() {
            "Ctrl" => modifiers |= MOD_CONTROL,
            "Alt" => modifiers |= MOD_ALT,
            "Shift" => modifiers |= MOD_SHIFT,
            "Win" => modifiers |= MOD_WIN,
            key => vk = Some(key_name_to_vk(key)?),
        }
    }

    Some((modifiers, vk?))
}

/// Convert key name to Win32 virtual key code.
fn key_name_to_vk(name: &str) -> Option<u32> {
    let vk = match name {
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,
        "0" => 0x30,
        "1" => 0x31,
        "2" => 0x32,
        "3" => 0x33,
        "4" => 0x34,
        "5" => 0x35,
        "6" => 0x36,
        "7" => 0x37,
        "8" => 0x38,
        "9" => 0x39,
        "A" => 0x41,
        "B" => 0x42,
        "C" => 0x43,
        "D" => 0x44,
        "E" => 0x45,
        "F" => 0x46,
        "G" => 0x47,
        "H" => 0x48,
        "I" => 0x49,
        "J" => 0x4A,
        "K" => 0x4B,
        "L" => 0x4C,
        "M" => 0x4D,
        "N" => 0x4E,
        "O" => 0x4F,
        "P" => 0x50,
        "Q" => 0x51,
        "R" => 0x52,
        "S" => 0x53,
        "T" => 0x54,
        "U" => 0x55,
        "V" => 0x56,
        "W" => 0x57,
        "X" => 0x58,
        "Y" => 0x59,
        "Z" => 0x5A,
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Tab" => 0x09,
        "Esc" => 0x1B,
        "BS" => 0x08,
        "Del" => 0x2E,
        "Ins" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PgUp" => 0x21,
        "PgDn" => 0x22,
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        "Pause" => 0x13,
        "PrtSc" => 0x2C,
        _ => return None,
    };
    Some(vk)
}
