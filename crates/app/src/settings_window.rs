use std::cell::Cell;
use std::path::Path;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::AppConfig;

/// 設定ウィンドウの状態（GWLP_USERDATA に格納、UIスレッド限定）
struct SettingsState {
    config: AppConfig,
    config_path: std::path::PathBuf,
    edit_font_size: HWND,
    edit_duration: HWND,
    edit_opacity: HWND,
}

// 設定ウィンドウが既に開いているかどうか（UIスレッド限定）
thread_local! {
    static SETTINGS_OPEN: Cell<bool> = const { Cell::new(false) };
}

const ID_BTN_SAVE: u16 = 100;
const ID_BTN_CANCEL: u16 = 101;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SettingsState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                let cmd_id = (wparam.0 & 0xFFFF) as u16;
                match cmd_id {
                    ID_BTN_SAVE => {
                        let font_size = get_edit_f32(state.edit_font_size)
                            .unwrap_or(state.config.style.font_size);
                        let duration = get_edit_u64(state.edit_duration)
                            .unwrap_or(state.config.display.display_duration_ms);
                        let opacity = get_edit_f32(state.edit_opacity)
                            .unwrap_or(state.config.style.opacity)
                            .clamp(0.0, 1.0);

                        state.config.style.font_size = font_size;
                        state.config.display.display_duration_ms = duration;
                        state.config.style.opacity = opacity;
                        let _ = state.config.save(&state.config_path);
                        let _ = DestroyWindow(hwnd);
                    }
                    ID_BTN_CANCEL => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SettingsState;
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            SETTINGS_OPEN.with(|c| c.set(false));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn get_edit_f32(hwnd: HWND) -> Option<f32> {
    let mut buf = [0u16; 64];
    let len = GetWindowTextW(hwnd, &mut buf) as usize;
    let text = String::from_utf16_lossy(&buf[..len]);
    text.trim().parse().ok()
}

unsafe fn get_edit_u64(hwnd: HWND) -> Option<u64> {
    let mut buf = [0u16; 64];
    let len = GetWindowTextW(hwnd, &mut buf) as usize;
    let text = String::from_utf16_lossy(&buf[..len]);
    text.trim().parse().ok()
}

unsafe fn create_label(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32) {
    let wide = to_wide(text);
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("STATIC"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x, y, w, h,
        parent, None, None, None,
    );
}

unsafe fn create_edit(parent: HWND, value: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let wide = to_wide(value);
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        windows::core::w!("EDIT"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        x, y, w, h,
        parent, None, None, None,
    ).unwrap_or_default()
}

unsafe fn create_button(parent: HWND, text: &str, id: u16, x: i32, y: i32, w: i32, h: i32) {
    let wide = to_wide(text);
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("BUTTON"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        x, y, w, h,
        parent,
        HMENU(id as *mut _),
        None, None,
    );
}

/// 設定ウィンドウを開く（UIスレッドから呼ぶこと）
pub fn open_settings_window(config: &AppConfig, config_path: &Path) {
    // 既に開いている場合は何もしない
    if SETTINGS_OPEN.with(|c| c.get()) {
        return;
    }

    unsafe {
        let class_name = to_wide("yStrokeySettings");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_wnd_proc),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let title = to_wide("yStrokey 設定");
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT, CW_USEDEFAULT, 350, 250,
            None, None, None, None,
        ).unwrap_or_default();

        if hwnd.0.is_null() {
            return;
        }

        let lx = 20;  // label x
        let ex = 170; // edit x
        let ew = 140; // edit width

        create_label(hwnd, "フォントサイズ:", lx, 20, 140, 22);
        let edit_font = create_edit(hwnd, &config.style.font_size.to_string(), ex, 18, ew, 24);

        create_label(hwnd, "表示時間 (ms):", lx, 56, 140, 22);
        let edit_dur = create_edit(hwnd, &config.display.display_duration_ms.to_string(), ex, 54, ew, 24);

        create_label(hwnd, "不透明度 (0-1):", lx, 92, 140, 22);
        let edit_opa = create_edit(hwnd, &format!("{:.2}", config.style.opacity), ex, 90, ew, 24);

        create_button(hwnd, "保存", ID_BTN_SAVE, 80, 140, 80, 30);
        create_button(hwnd, "キャンセル", ID_BTN_CANCEL, 180, 140, 80, 30);

        let state = Box::new(SettingsState {
            config: config.clone(),
            config_path: config_path.to_path_buf(),
            edit_font_size: edit_font,
            edit_duration: edit_dur,
            edit_opacity: edit_opa,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        SETTINGS_OPEN.with(|c| c.set(true));

        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}
