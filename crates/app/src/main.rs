mod autostart;
mod logger;
mod resources;
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
use windows::Win32::Storage::FileSystem::{
    FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use ystrokey_input::{
    install_keyboard_hook, is_privacy_target, poll_ime_state, set_hook_enabled, set_wake_window,
    ClipboardListener, GameList,
};
use ystrokey_render::{get_monitor_device_name, D2DRenderer, OsdWindow};

use tray::{
    show_context_menu, ID_TRAY_AUTOSTART, ID_TRAY_EXIT, ID_TRAY_EXPORT, ID_TRAY_IMPORT,
    ID_TRAY_SETTINGS, ID_TRAY_TOGGLE, WM_TRAYICON,
};

const HOTKEY_TOGGLE_ID: i32 = 1;

/// wnd_procからイベント送信用のグローバルチャネル
static EVENT_TX: OnceLock<SyncSender<InputEvent>> = OnceLock::new();

/// OSDの有効/無効。トレイメニューから切り替える
static OSD_ENABLED: AtomicBool = AtomicBool::new(true);

/// 設定ファイルのパス。wnd_procから参照する
static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 現在の設定。wnd_procから参照する
static CURRENT_CONFIG: OnceLock<Mutex<AppConfig>> = OnceLock::new();

/// Ghost-modeでインタラクティブ状態かどうか。ドラッグの判定に使う
static GHOST_INTERACTIVE: AtomicBool = AtomicBool::new(false);

// クリップボードの重複検知用。wnd_procはメインスレッドからだけ呼ばれる
thread_local! {
    static LAST_CLIPBOARD: RefCell<String> = const { RefCell::new(String::new()) };
}

/// WM_CLIPBOARDUPDATE。Windows Vista以降で使える
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

/// 前面のウィンドウがゲームか、プライバシーの対象のアプリかを判定し直し、OSDを前面のウィンドウのモニターに移し、IMEの状態を読み始めるか。
/// 前面のウィンドウ・ゲームの一覧・設定が変わった可能性があるときに立てる。
/// 起動直後にも1回判定するため、初期値はtrue
static FOREGROUND_DIRTY: AtomicBool = AtomicBool::new(true);

/// キー入力と前面のウィンドウの切り替えの後、IMEの状態を読み続ける時間。
/// キー入力からしばらく待って自動で変換し、未確定の文字列を書き換えるIMEがあるので、IMEの待ち時間と変換にかかる時間を見込む
const IME_FOLLOW: Duration = Duration::from_secs(2);

/// WINEVENT_OUTOFCONTEXTなので、メインスレッドのメッセージ処理の中で呼ばれる。
/// トレイのメニューや設定ウィンドウなど、自分のウィンドウへの切り替えでも判定し直すため、ほかのプロセスだけでなく自プロセスのイベントも受ける
unsafe extern "system" fn on_foreground_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    FOREGROUND_DIRTY.store(true, Ordering::SeqCst);
}

/// FindFirstChangeNotificationWはフォルダ単位でしか監視できないので、設定ファイルのあるフォルダを監視する。
/// 設定の保存は一時ファイルからReplaceFileWで置き換えるので、書き込みに加えて名前の変更も拾う
fn watch_directory(file: &Path) -> Option<HANDLE> {
    let dir = file.parent()?;
    let dir = HSTRING::from(dir.as_os_str());
    unsafe {
        FindFirstChangeNotificationW(
            &dir,
            false,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME,
        )
        .ok()
    }
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
        WM_SETTINGCHANGE => {
            // タスクバーのライト/ダークが切り替わったらトレイアイコンを差し替える
            if tray::is_color_set_change(lparam) {
                tray::update_icon_for_theme(hwnd);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
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

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let config_dir = user_dir("APPDATA").unwrap_or_else(|| exe_dir.clone());
    let log_base_dir = user_dir("LOCALAPPDATA").unwrap_or_else(|| exe_dir.clone());
    let _ = std::fs::create_dir_all(&config_dir);
    let config_path = config_dir.join("config.json");
    let migrated = migrate_legacy_config(&exe_dir.join("config.json"), &config_path);

    let mut saved_config = load_config_with_recovery(&config_path);
    let mut effective_config = saved_config.clone();
    let mut preview_mode_active = false;
    let mut preview_draft_config: Option<AppConfig> = None;

    logger::init(&log_base_dir, &saved_config.diagnostics);
    logger::log(DiagnosticsLevel::Info, "Application startup");
    match migrated {
        Ok(true) => logger::log(
            DiagnosticsLevel::Info,
            &format!("Copied the config next to the executable to {}", config_path.display()),
        ),
        Ok(false) => {}
        Err(e) => logger::log(
            DiagnosticsLevel::Warn,
            &format!("Failed to copy the config next to the executable: {e}"),
        ),
    }

    let _ = CONFIG_PATH.set(config_path.clone());
    let _ = CURRENT_CONFIG.set(Mutex::new(saved_config.clone()));

    let mut window = OsdWindow::create(
        saved_config.performance.osd_width,
        saved_config.performance.osd_height,
        &saved_config.display,
    )
    .unwrap_or_else(|e| fatal_error(&format!("OSD window creation failed: {e}")));

    unsafe {
        SetWindowLongPtrW(window.hwnd(), GWL_WNDPROC, app_wnd_proc as usize as isize);
    }

    let mut renderer = D2DRenderer::new(&saved_config.style)
        .unwrap_or_else(|e| fatal_error(&format!("D2D renderer creation failed: {e}")));
    renderer.update_dpi(window.dpi);

    let mut state = DisplayState::new(&saved_config);
    let mut intervals = RuntimeIntervals {
        frame_duration: Duration::from_millis(saved_config.performance.frame_interval_ms),
        ime_poll_interval: Duration::from_millis(saved_config.performance.ime_poll_interval_ms),
        config_reload_interval: Duration::from_millis(saved_config.performance.config_reload_interval_ms),
    };

    apply_config(
        ApplyReason::Startup,
        &saved_config,
        &mut state,
        &mut renderer,
        &mut window,
        &mut intervals,
    );

    let (tx, rx) = mpsc::sync_channel::<InputEvent>(256);
    let _ = EVENT_TX.set(tx.clone());

    set_wake_window(window.hwnd());
    let _hook_thread = install_keyboard_hook(tx.clone());

    let mut games = GameList::load();
    logger::log(
        DiagnosticsLevel::Info,
        &format!("game list: {} executables", games.len()),
    );
    let game_event = unsafe { CreateEventW(None, false, false, None) }.ok();
    let game_watch = match game_event {
        Some(event) => games.watch(event),
        None => false,
    };
    let config_watch = watch_directory(&config_path);
    let foreground_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    let poll_foreground = foreground_hook.is_invalid();
    if poll_foreground {
        logger::log(
            DiagnosticsLevel::Warn,
            "SetWinEventHook failed: checking the foreground window by polling instead",
        );
    }

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
    let mut config_changed = false;
    let mut privacy_active = false;
    let mut was_rendering = false;
    let mut last_foreground_hwnd = HWND::default();
    let mut last_foreground_poll = Instant::now();
    let mut in_game = false;
    let mut ime_follow_until: Option<Instant> = None;

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
            match event {
                InputEvent::DpiChanged { dpi, suggested_rect } => {
                    let rect = RECT {
                        left: suggested_rect[0],
                        top: suggested_rect[1],
                        right: suggested_rect[2],
                        bottom: suggested_rect[3],
                    };
                    window.update_for_dpi(dpi, &rect);
                    renderer.update_dpi(dpi);
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
                                saved_config = new_config;

                                if preview_mode_active {
                                    if let Some(draft) = preview_draft_config.as_ref() {
                                        effective_config =
                                            merge_preview_config(&saved_config, draft);
                                        apply_visual_config(
                                            &effective_config,
                                            &mut state,
                                            &mut renderer,
                                            &mut window,
                                        );
                                    } else {
                                        effective_config = saved_config.clone();
                                    }
                                } else {
                                    effective_config = saved_config.clone();
                                }
                            }
                            Err(e) => logger::log(
                                DiagnosticsLevel::Warn,
                                &format!("ConfigChanged reload failed: {e}"),
                            ),
                        }
                    }
                    continue;
                }
                InputEvent::PreviewMode { enabled } => {
                    preview_mode_active = enabled;
                    if enabled {
                        state.set_preview_active(true, Instant::now());
                    } else {
                        preview_draft_config = None;
                        state.set_preview_active(false, Instant::now());
                        effective_config = saved_config.clone();
                        apply_config(
                            ApplyReason::UiEdit,
                            &saved_config,
                            &mut state,
                            &mut renderer,
                            &mut window,
                            &mut intervals,
                        );
                    }
                    continue;
                }
                InputEvent::PreviewConfig { config } => {
                    if preview_mode_active {
                        preview_draft_config = Some(config);
                        if let Some(draft) = preview_draft_config.as_ref() {
                            effective_config = merge_preview_config(&saved_config, draft);
                            apply_visual_config(
                                &effective_config,
                                &mut state,
                                &mut renderer,
                                &mut window,
                            );
                        }
                    }
                    continue;
                }
                other => {
                    if enabled && !privacy_active && !in_game {
                        if matches!(other, InputEvent::Key(_)) {
                            ime_follow_until = Some(Instant::now() + IME_FOLLOW);
                        }
                        state.process_event(other);
                    }
                }
            }
        }

        let now = Instant::now();
        if poll_foreground && now.duration_since(last_foreground_poll) >= intervals.ime_poll_interval {
            last_foreground_poll = now;
            FOREGROUND_DIRTY.store(true, Ordering::SeqCst);
        }
        // 通知は遅れて届くことがあるので、通知の対象のウィンドウではなく今の前面のウィンドウで判定する
        if FOREGROUND_DIRTY.swap(false, Ordering::SeqCst) {
            let game = saved_config.behavior.pause_in_games && games.foreground_is_game();
            logger::log(DiagnosticsLevel::Debug, &format!("foreground changed: game={game}"));
            if game != in_game {
                in_game = game;
                set_hook_enabled(!game);
                // フックを外すとキーを離したイベントが届かないので、Alt+TabのAltのように、押されたまま表示しているキーを消しておく
                state.clear();
                logger::log(
                    DiagnosticsLevel::Info,
                    if game { "game: enter (OSD, IME polling and keyboard hook off)" } else { "game: leave" },
                );
            }
            let fg = unsafe { GetForegroundWindow() };
            if fg != last_foreground_hwnd {
                last_foreground_hwnd = fg;
                let prev_privacy = privacy_active;
                privacy_active = is_privacy_target(&saved_config.privacy);
                if privacy_active && !prev_privacy {
                    state.clear();
                }
                if !fg.0.is_null() && !in_game {
                    window.reposition_to_monitor(fg, &effective_config.display);
                }
            }
            ime_follow_until = Some(now + IME_FOLLOW);
        }
        if let Some(until) = ime_follow_until {
            if now >= until || in_game {
                ime_follow_until = None;
            } else if now.duration_since(last_ime_poll) >= intervals.ime_poll_interval {
                if enabled && !privacy_active {
                    poll_ime_state(&tx);
                }
                last_ime_poll = now;
            }
        }

        let config_due = match config_watch {
            Some(_) => std::mem::take(&mut config_changed),
            None => now.duration_since(last_config_check) >= intervals.config_reload_interval,
        };
        if config_due {
            match saved_config.check_reload(&config_path) {
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
                    saved_config = new_config;

                    if preview_mode_active {
                        if let Some(draft) = preview_draft_config.as_ref() {
                            effective_config = merge_preview_config(&saved_config, draft);
                            apply_visual_config(
                                &effective_config,
                                &mut state,
                                &mut renderer,
                                &mut window,
                            );
                        } else {
                            effective_config = saved_config.clone();
                        }
                    } else {
                        effective_config = saved_config.clone();
                    }
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
        let has_any = has_items || state.preview_active();

        if has_any || was_rendering {
            let live_items = state.active_items();
            let preview_items = state.preview_items();
            let ghost_opacity = calculate_ghost_opacity(&window, &effective_config);
            let interactive = ghost_opacity > 0.0 && is_cursor_in_rect(&window.get_rect());
            GHOST_INTERACTIVE.store(interactive, Ordering::Relaxed);
            window.set_interactive(interactive);

            if let Err(e) = renderer.render(
                live_items,
                preview_items,
                &effective_config.style,
                window.mem_dc(),
                window.width() as u32,
                window.height() as u32,
                ghost_opacity,
            ) {
                logger::log(DiagnosticsLevel::Warn, &format!("Render error: {e}"));
                if let Ok(new_renderer) = D2DRenderer::new(&effective_config.style) {
                    renderer = new_renderer;
                    renderer.update_dpi(window.dpi);
                }
            }
            // 設定プレビュー中は、ユーザーが不透明度を一時的に0にしても元に戻せるよう、
            // ウィンドウをわずかに表示したままにする。
            let present_opacity = if state.preview_active() {
                effective_config.style.opacity.max(0.05)
            } else {
                effective_config.style.opacity
            };
            window.present(present_opacity);

            was_rendering = has_any;
            std::thread::sleep(intervals.frame_duration);
        } else {
            // 描画していない間は、キー入力と前面のウィンドウの切り替えを含むメッセージか、設定フォルダ・ゲームの一覧の変更通知が届くまで待つ。
            // タイムアウトを付けるのは、IMEか前面のウィンドウを定期的に確認している間と、設定の変更通知を作れなかったときだけ
            let mut timeout = INFINITE;
            if poll_foreground {
                let next = intervals.ime_poll_interval.saturating_sub(last_foreground_poll.elapsed());
                timeout = timeout.min(next.as_millis() as u32);
            }
            if ime_follow_until.is_some() {
                let next = intervals.ime_poll_interval.saturating_sub(last_ime_poll.elapsed());
                timeout = timeout.min(next.as_millis() as u32);
            }
            if config_watch.is_none() {
                let next = intervals
                    .config_reload_interval
                    .saturating_sub(last_config_check.elapsed());
                timeout = timeout.min(next.as_millis() as u32);
            }
            let mut handles: Vec<HANDLE> = Vec::with_capacity(2);
            if let Some(h) = config_watch {
                handles.push(h);
            }
            let game_slot = match (game_watch, game_event) {
                (true, Some(e)) => {
                    handles.push(e);
                    Some(handles.len() - 1)
                }
                _ => None,
            };
            let woke = unsafe {
                MsgWaitForMultipleObjectsEx(Some(&handles), timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
            };
            let index = woke.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
            if index < handles.len() {
                if Some(index) == game_slot {
                    games.reload();
                    if let Some(e) = game_event {
                        games.watch(e);
                    }
                    FOREGROUND_DIRTY.store(true, Ordering::SeqCst);
                    logger::log(
                        DiagnosticsLevel::Info,
                        &format!("game list changed: {} executables", games.len()),
                    );
                } else {
                    config_changed = true;
                    unsafe {
                        let _ = FindNextChangeNotification(handles[index]);
                    }
                }
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
    apply_visual_config(config, state, renderer, window);
    // pause_in_gamesとプライバシーの対象のアプリが変わったかもしれないので、判定し直す
    FOREGROUND_DIRTY.store(true, Ordering::SeqCst);

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

fn apply_visual_config(
    config: &AppConfig,
    state: &mut DisplayState,
    renderer: &mut D2DRenderer,
    window: &mut OsdWindow,
) {
    state.update_config(config);
    renderer.update_style(&config.style);
    window.set_display_affinity(config.behavior.exclude_from_capture);

    if window.width() != config.performance.osd_width || window.height() != config.performance.osd_height {
        window.resize(config.performance.osd_width, config.performance.osd_height);
    }

    // 見た目の設定は、設定ウィンドウで調整している間も即座に反映する。
    unsafe {
        let fg = GetForegroundWindow();
        if !fg.0.is_null() {
            window.reposition_to_monitor(fg, &config.display);
        }
    }
}

fn merge_preview_config(base: &AppConfig, draft: &AppConfig) -> AppConfig {
    // 実行時の値など表示に関係しない項目は、古い下書きの値で上書きしないよう`base`の値をそのまま使う。
    let mut cfg = base.clone();

    // 表示。モニターごとの保存済みの位置は`base`の値を使う。
    cfg.display.position = draft.display.position;
    cfg.display.offset_x = draft.display.offset_x;
    cfg.display.offset_y = draft.display.offset_y;
    cfg.display.max_items = draft.display.max_items;
    cfg.display.display_duration_ms = draft.display.display_duration_ms;
    cfg.display.fade_duration_ms = draft.display.fade_duration_ms;

    // スタイル。すべて見た目の項目。
    cfg.style = draft.style.clone();

    // 動作。表示に影響する項目だけ。
    cfg.behavior.key_transition_mode = draft.behavior.key_transition_mode;
    cfg.behavior.show_repeat_count = draft.behavior.show_repeat_count;
    cfg.behavior.distinguish_numpad = draft.behavior.distinguish_numpad;
    cfg.behavior.show_shift_for_typed_symbols = draft.behavior.show_shift_for_typed_symbols;
    cfg.behavior.show_ime_composition = draft.behavior.show_ime_composition;
    cfg.behavior.show_clipboard = draft.behavior.show_clipboard;
    cfg.behavior.clipboard_max_chars = draft.behavior.clipboard_max_chars;
    cfg.behavior.show_lock_indicators = draft.behavior.show_lock_indicators;

    // 性能。OSDのサイズ。
    cfg.performance.osd_width = draft.performance.osd_width;
    cfg.performance.osd_height = draft.performance.osd_height;

    // アニメーション。見た目の項目。
    cfg.animation = draft.animation.clone();

    cfg
}

/// `%<var>%\ystrokey`を返す。設定とログを実行ファイルと同じフォルダではなくユーザーのフォルダに置くのは、
/// Scoopなどのパッケージマネージャーがバージョンごとに別のフォルダへインストールし、
/// 実行ファイルと同じフォルダのファイルが更新のたびに古いバージョンのフォルダに残されるため
fn user_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(|v| PathBuf::from(v).join("ystrokey"))
}

/// 実行ファイルと同じフォルダの設定ファイル`from`を`to`にコピーする。`to`がすでにあればコピーしない。
/// 実行ファイルと同じフォルダで設定を読む古いバージョンを動かせるよう、`from`は消さない。コピーしたかを返す
fn migrate_legacy_config(from: &Path, to: &Path) -> std::io::Result<bool> {
    if from == to || to.exists() || !from.exists() {
        return Ok(false);
    }
    std::fs::copy(from, to)?;
    Ok(true)
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
            match AppConfig::create_default(config_path) {
                Ok(cfg) => cfg,
                Err(create_err) => {
                    eprintln!(
                        "failed to persist recovered defaults: {}; using in-memory defaults",
                        create_err
                    );
                    default_runtime_config(config_path)
                }
            }
        }
    }
}

fn default_runtime_config(config_path: &Path) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.last_modified = std::fs::metadata(config_path)
        .and_then(|m| m.modified())
        .ok();
    cfg
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

/// 修飾キーが押されているかと、カーソルからOSDのウィンドウまでの距離から、ゴーストの不透明度を決める。
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

/// カーソルから矩形までの距離。カーソルが矩形の内側にあるときは0。
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

/// ホットキー文字列を解析し、RegisterHotKeyで登録する。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let dir = std::env::temp_dir().join(format!("ystrokey-{name}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn migrate_copies_the_legacy_config_when_none_exists() {
        let dir = temp_dir("migrate-copy");
        let (from, to) = (dir.join("old.json"), dir.join("new.json"));
        std::fs::write(&from, "{}").unwrap();
        assert!(migrate_legacy_config(&from, &to).unwrap());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "{}");
        assert!(from.exists());
    }

    #[test]
    fn migrate_keeps_an_existing_config() {
        let dir = temp_dir("migrate-keep");
        let (from, to) = (dir.join("old.json"), dir.join("new.json"));
        std::fs::write(&from, "old").unwrap();
        std::fs::write(&to, "new").unwrap();
        assert!(!migrate_legacy_config(&from, &to).unwrap());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
    }

    #[test]
    fn migrate_does_nothing_without_a_legacy_config() {
        let dir = temp_dir("migrate-none");
        assert!(!migrate_legacy_config(&dir.join("old.json"), &dir.join("new.json")).unwrap());
    }
}
