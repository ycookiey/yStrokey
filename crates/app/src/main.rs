mod autostart;
mod settings_io;
mod settings_window;
mod tray;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::{AppConfig, ClipboardContent, ClipboardEvent, DisplayState, InputEvent};
use ystrokey_input::{install_keyboard_hook, is_privacy_target, poll_ime_state, ClipboardListener};
use ystrokey_render::{get_monitor_device_name, D2DRenderer, OsdWindow};

use tray::{
    show_context_menu, ID_TRAY_AUTOSTART, ID_TRAY_EXIT, ID_TRAY_EXPORT, ID_TRAY_IMPORT,
    ID_TRAY_SETTINGS, ID_TRAY_TOGGLE, WM_TRAYICON,
};

const OSD_WIDTH: i32 = 600;
const OSD_HEIGHT: i32 = 300;
const IME_POLL_INTERVAL: Duration = Duration::from_millis(50);
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
                show_context_menu(hwnd);
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
                    let currently = autostart::is_autostart_enabled();
                    let _ = autostart::set_autostart(!currently);
                }
                ID_TRAY_SETTINGS => {
                    if let (Some(path), Some(cfg_mutex)) = (CONFIG_PATH.get(), CURRENT_CONFIG.get()) {
                        if let Ok(cfg) = cfg_mutex.lock() {
                            settings_window::open_settings_window(&cfg, path);
                        }
                    }
                }
                ID_TRAY_EXPORT => {
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(cfg) = cfg_mutex.lock() {
                            let _ = settings_io::export_config(&cfg);
                        }
                    }
                }
                ID_TRAY_IMPORT => {
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(Some(new_cfg)) = settings_io::import_config() {
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

fn main() {
    // DPI Awareness 設定
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // 設定ファイル読み込み（exe隣の config.json）
    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.json")))
        .unwrap_or_else(|| std::path::PathBuf::from("config.json"));

    let mut config = AppConfig::load_or_create(&config_path).unwrap_or_else(|e| {
        eprintln!("config load failed, using defaults: {:?}", e);
        AppConfig::default()
    });

    // グローバル状態にセット（wnd_proc からアクセス用）
    let _ = CONFIG_PATH.set(config_path.clone());
    let _ = CURRENT_CONFIG.set(Mutex::new(config.clone()));

    // OSD ウィンドウ作成
    let mut window = OsdWindow::create(OSD_WIDTH, OSD_HEIGHT, &config.display).expect("OSD window creation failed");

    // ウィンドウプロシージャをアプリ用に差し替え
    unsafe {
        SetWindowLongPtrW(window.hwnd(), GWL_WNDPROC, app_wnd_proc as usize as isize);
    }

    // キャプチャ防止設定
    window.set_display_affinity(config.behavior.exclude_from_capture);

    // Direct2D レンダラー作成
    let mut renderer =
        D2DRenderer::new(&config.style)
            .expect("D2D renderer creation failed");

    // 表示状態管理
    let mut state = DisplayState::new(&config);

    // イベントチャネル（hook thread → UI thread）
    let (tx, rx) = mpsc::sync_channel::<InputEvent>(256);
    let _ = EVENT_TX.set(tx.clone());

    // キーボードフック起動（別スレッド）
    let _hook_thread = install_keyboard_hook(tx.clone());

    // クリップボードリスナー登録（WM_CLIPBOARDUPDATE を受信可能にする）
    let _clipboard_listener = match ClipboardListener::new(window.hwnd()) {
        Ok(listener) => Some(listener),
        Err(e) => {
            eprintln!("clipboard listener failed: {}", e);
            None
        }
    };

    // グローバルホットキー登録
    register_toggle_hotkey(window.hwnd(), &config.hotkey.toggle);

    // システムトレイアイコン作成
    let _tray = tray::TrayIcon::new(window.hwnd()).expect("tray icon creation failed");

    // メインループ
    let mut msg = MSG::default();
    let frame_duration = Duration::from_millis(16);
    let mut last_ime_poll = Instant::now();
    let mut last_config_check = Instant::now();
    let config_check_interval = Duration::from_secs(1);
    let mut privacy_active = false;
    let mut was_rendering = false;

    loop {
        // Win32 メッセージ処理
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // イベント受信（OSD無効時はイベント破棄）
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
                    if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                        if let Ok(new_cfg) = cfg_mutex.lock() {
                            let new_config = new_cfg.clone();
                            drop(new_cfg);
                            state.update_config(&new_config);
                            renderer.update_style(&new_config.style);
                            window.set_display_affinity(new_config.behavior.exclude_from_capture);
                            config = new_config;
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

        // IME ポーリング（50ms 間隔）
        let now = Instant::now();
        if now.duration_since(last_ime_poll) >= IME_POLL_INTERVAL {
            privacy_active = is_privacy_target(&config.privacy);
            if enabled && !privacy_active {
                poll_ime_state(&tx);
            }
            // Multi-monitor: reposition OSD to foreground window monitor
            unsafe {
                let fg = GetForegroundWindow();
                if !fg.0.is_null() {
                    window.reposition_to_monitor(fg, &config.display);
                }
            }
            last_ime_poll = now;
        }

        // 設定ホットリロード（1秒間隔）
        if now.duration_since(last_config_check) >= config_check_interval {
            if let Some(new_config) = config.check_reload(&config_path) {
                state.update_config(&new_config);
                renderer.update_style(&new_config.style);
                window.set_display_affinity(new_config.behavior.exclude_from_capture);
                if let Some(cfg_mutex) = CURRENT_CONFIG.get() {
                    if let Ok(mut cfg) = cfg_mutex.lock() {
                        *cfg = new_config.clone();
                    }
                }
                config = new_config;
            }
            last_config_check = now;
        }

        // アニメーション更新
        state.tick(Instant::now());

        let has_items = !state.active_items().is_empty();

        if has_items || was_rendering {
            // 描画が必要（アクティブ、またはクリア用の最終フレーム）
            let items = state.active_items();
            let ghost_opacity = calculate_ghost_opacity(&window);
            let interactive = ghost_opacity > 0.0 && is_cursor_in_rect(&window.get_rect());
            GHOST_INTERACTIVE.store(interactive, Ordering::Relaxed);
            window.set_interactive(interactive);

            let _ = renderer.render(
                items,
                &config.style,
                window.mem_dc(),
                window.width() as u32,
                window.height() as u32,
                ghost_opacity,
            );
            window.present(config.style.opacity);

            was_rendering = has_items;
            std::thread::sleep(frame_duration);
        } else {
            // アイドル: 描画をスキップし、メッセージまたはタイムアウト(50ms)で待機
            unsafe {
                MsgWaitForMultipleObjects(None, false, 50, QS_ALLINPUT);
            }
        }
    }
}

/// Ctrl押下 + カーソル距離 (100px閾値) から ghost不透明度を計算
fn calculate_ghost_opacity(window: &OsdWindow) -> f32 {
    unsafe {
        // Ctrl キー押下チェック
        let ctrl_down = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16) & 0x8000 != 0;
        if !ctrl_down {
            return 0.0;
        }

        // カーソル位置取得
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() {
            return 0.0;
        }

        // ウィンドウ矩形までの距離
        let rect = window.get_rect();
        let distance = distance_to_rect(&cursor, &rect);

        // 100px以内でフェードイン (距離0で1.0、100pxで0.0)
        let threshold = 100.0_f32;
        (1.0 - distance / threshold).clamp(0.0, 1.0)
    }
}

/// カーソルからRECTまでの距離 (内側なら0)
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

/// カーソルがRECT内にあるか
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

/// 現在のウィンドウ位置をconfig.jsonに保存
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
                        let _ = cfg.save(path);
                    }
                }
            }
        }
    }
}

/// ホットキー文字列（例: "Ctrl+Alt+F12"）をパースして RegisterHotKey で登録
fn register_toggle_hotkey(hwnd: HWND, hotkey_str: &str) {
    if hotkey_str.is_empty() {
        return;
    }

    let Some((modifiers, vk)) = parse_hotkey(hotkey_str) else {
        eprintln!("invalid hotkey: {}", hotkey_str);
        return;
    };

    unsafe {
        if RegisterHotKey(hwnd, HOTKEY_TOGGLE_ID, modifiers, vk).is_err() {
            eprintln!("RegisterHotKey failed for: {}", hotkey_str);
        }
    }
}

/// ホットキー文字列をパースして (MOD_*, VKコード) に変換
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

/// キー名から Win32 仮想キーコードへの変換
fn key_name_to_vk(name: &str) -> Option<u32> {
    let vk = match name {
        "F1" => 0x70, "F2" => 0x71, "F3" => 0x72, "F4" => 0x73,
        "F5" => 0x74, "F6" => 0x75, "F7" => 0x76, "F8" => 0x77,
        "F9" => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
        "0" => 0x30, "1" => 0x31, "2" => 0x32, "3" => 0x33,
        "4" => 0x34, "5" => 0x35, "6" => 0x36, "7" => 0x37,
        "8" => 0x38, "9" => 0x39,
        "A" => 0x41, "B" => 0x42, "C" => 0x43, "D" => 0x44,
        "E" => 0x45, "F" => 0x46, "G" => 0x47, "H" => 0x48,
        "I" => 0x49, "J" => 0x4A, "K" => 0x4B, "L" => 0x4C,
        "M" => 0x4D, "N" => 0x4E, "O" => 0x4F, "P" => 0x50,
        "Q" => 0x51, "R" => 0x52, "S" => 0x53, "T" => 0x54,
        "U" => 0x55, "V" => 0x56, "W" => 0x57, "X" => 0x58,
        "Y" => 0x59, "Z" => 0x5A,
        "Space" => 0x20, "Enter" => 0x0D, "Tab" => 0x09,
        "Esc" => 0x1B, "BS" => 0x08, "Del" => 0x2E, "Ins" => 0x2D,
        "Home" => 0x24, "End" => 0x23, "PgUp" => 0x21, "PgDn" => 0x22,
        "Left" => 0x25, "Up" => 0x26, "Right" => 0x27, "Down" => 0x28,
        "Pause" => 0x13, "PrtSc" => 0x2C,
        _ => return None,
    };
    Some(vk)
}
