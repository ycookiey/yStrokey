
use std::cell::Cell;
use std::path::Path;
use std::sync::mpsc::SyncSender;

use windows::core::HSTRING;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::{
    AppConfig, DiagnosticsLevel, FadeOutCurve, GhostModifier, InputEvent, MenuLanguage, Position,
    ShortcutDef,
};

struct SettingsState {
    config: AppConfig,
    config_path: std::path::PathBuf,
    notify_tx: Option<SyncSender<InputEvent>>,
    category: Category,
    nav: HWND,
    status: HWND,
    dynamic_controls: Vec<HWND>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    Display,
    Style,
    Input,
    Privacy,
    Performance,
    Diagnostics,
    Startup,
    Tray,
    Animation,
}

impl Category {
    fn from_index(i: i32) -> Self {
        match i {
            0 => Self::General,
            1 => Self::Display,
            2 => Self::Style,
            3 => Self::Input,
            4 => Self::Privacy,
            5 => Self::Performance,
            6 => Self::Diagnostics,
            7 => Self::Startup,
            8 => Self::Tray,
            9 => Self::Animation,
            _ => Self::General,
        }
    }
}

thread_local! {
    static SETTINGS_OPEN: Cell<bool> = const { Cell::new(false) };
}

const ID_NAV: u16 = 100;
const ID_BTN_REVERT_SECTION: u16 = 101;
const ID_BTN_RESET_ALL: u16 = 102;
const ID_BTN_CLOSE: u16 = 103;

const ID_HOTKEY_TOGGLE: u16 = 1000;
const ID_SHORTCUTS: u16 = 1001;

const ID_DISPLAY_POSITION: u16 = 1100;
const ID_DISPLAY_OFFSET_X: u16 = 1101;
const ID_DISPLAY_OFFSET_Y: u16 = 1102;
const ID_DISPLAY_MAX_ITEMS: u16 = 1103;
const ID_DISPLAY_DURATION: u16 = 1104;
const ID_DISPLAY_FADE: u16 = 1105;

const ID_STYLE_FONT_FAMILY: u16 = 1200;
const ID_STYLE_FONT_SIZE: u16 = 1201;
const ID_STYLE_TEXT_COLOR: u16 = 1202;
const ID_STYLE_BACKGROUND_COLOR: u16 = 1203;
const ID_STYLE_BORDER_RADIUS: u16 = 1204;
const ID_STYLE_PADDING: u16 = 1205;
const ID_STYLE_SHORTCUT_COLOR: u16 = 1206;
const ID_STYLE_KEY_DOWN_COLOR: u16 = 1207;
const ID_STYLE_OPACITY: u16 = 1208;

const ID_BEHAVIOR_SHOW_KEY_DOWN_UP: u16 = 1300;
const ID_BEHAVIOR_SHOW_REPEAT_COUNT: u16 = 1301;
const ID_BEHAVIOR_DISTINGUISH_NUMPAD: u16 = 1302;
const ID_BEHAVIOR_SHOW_IME: u16 = 1303;
const ID_BEHAVIOR_SHOW_CLIPBOARD: u16 = 1304;
const ID_BEHAVIOR_CLIPBOARD_MAX_CHARS: u16 = 1305;
const ID_BEHAVIOR_SHOW_LOCK: u16 = 1306;
const ID_BEHAVIOR_REPEAT_TIMEOUT: u16 = 1307;
const ID_BEHAVIOR_GROUP_TIMEOUT: u16 = 1308;
const ID_BEHAVIOR_MAX_GROUP_SIZE: u16 = 1309;
const ID_BEHAVIOR_IGNORED_KEYS: u16 = 1310;
const ID_BEHAVIOR_EXCLUDE_CAPTURE: u16 = 1311;

const ID_PRIVACY_ENABLED: u16 = 1400;
const ID_PRIVACY_BLOCKED_APPS: u16 = 1401;

const ID_PERF_OSD_WIDTH: u16 = 1500;
const ID_PERF_OSD_HEIGHT: u16 = 1501;
const ID_PERF_IME_POLL: u16 = 1502;
const ID_PERF_FRAME_INTERVAL: u16 = 1503;
const ID_PERF_RELOAD_INTERVAL: u16 = 1504;

const ID_DIAG_LEVEL: u16 = 1600;
const ID_DIAG_FILE_ENABLED: u16 = 1601;
const ID_DIAG_MAX_BYTES: u16 = 1602;
const ID_DIAG_MAX_FILES: u16 = 1603;

const ID_STARTUP_AUTOSTART: u16 = 1700;

const ID_TRAY_START_OSD: u16 = 1800;
const ID_TRAY_MENU_LANGUAGE: u16 = 1801;
const ID_TRAY_CONFIRM_EXIT: u16 = 1802;

const ID_ANIM_GHOST_MODIFIER: u16 = 1900;
const ID_ANIM_GHOST_THRESHOLD: u16 = 1901;
const ID_ANIM_GHOST_MAX_OPACITY: u16 = 1902;
const ID_ANIM_FADE_CURVE: u16 = 1903;

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
            if ptr.is_null() {
                return LRESULT(0);
            }

            let state = &mut *ptr;
            let cmd_id = (wparam.0 & 0xFFFF) as u16;
            let notify = ((wparam.0 >> 16) & 0xFFFF) as u16;

            match cmd_id {
                ID_BTN_CLOSE => {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                ID_BTN_REVERT_SECTION => {
                    match AppConfig::load_strict(&state.config_path) {
                        Ok(cfg) => {
                            state.config = cfg;
                            rebuild_category(hwnd, state);
                            set_status(state, "Reverted this section.");
                        }
                        Err(e) => set_status(state, &format!("Reload failed: {e}")),
                    }
                    return LRESULT(0);
                }
                ID_BTN_RESET_ALL => {
                    let ans = MessageBoxW(
                        hwnd,
                        &HSTRING::from("Reset all settings to defaults?"),
                        &HSTRING::from("yStrokey"),
                        MB_ICONQUESTION | MB_YESNO,
                    );
                    if ans == IDYES {
                        let mut cfg = AppConfig::default();
                        match persist_and_notify(state, &mut cfg) {
                            Ok(()) => {
                                state.config = cfg;
                                rebuild_category(hwnd, state);
                                set_status(state, "Reset to defaults.");
                            }
                            Err(e) => set_status(state, &format!("Reset failed: {e}")),
                        }
                    }
                    return LRESULT(0);
                }
                ID_NAV if notify == LBN_SELCHANGE as u16 => {
                    let idx = SendMessageW(state.nav, LB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
                    state.category = Category::from_index(idx);
                    rebuild_category(hwnd, state);
                    return LRESULT(0);
                }
                _ => {}
            }

            let should_apply = notify == EN_KILLFOCUS as u16
                || notify == BN_CLICKED as u16
                || notify == CBN_SELCHANGE as u16;

            if should_apply {
                let mut new_cfg = state.config.clone();
                match apply_control_to_config(hwnd, cmd_id, &mut new_cfg) {
                    Ok(()) => match persist_and_notify(state, &mut new_cfg) {
                        Ok(()) => {
                            state.config = new_cfg;
                            set_status(state, "Saved.");
                        }
                        Err(e) => {
                            set_status(state, &format!("Save failed: {e}"));
                            rebuild_category(hwnd, state);
                        }
                    },
                    Err(e) => {
                        set_status(state, &format!("Invalid value: {e}"));
                        rebuild_category(hwnd, state);
                    }
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

fn persist_and_notify(state: &SettingsState, cfg: &mut AppConfig) -> Result<(), String> {
    cfg.validate().map_err(|e| e.to_string())?;
    cfg.save_atomic(&state.config_path)
        .map_err(|e| e.to_string())?;

    if let Some(tx) = &state.notify_tx {
        let _ = tx.try_send(InputEvent::ConfigChanged);
    }

    Ok(())
}

unsafe fn set_status(state: &SettingsState, msg: &str) {
    let w = to_wide(msg);
    let _ = SetWindowTextW(state.status, windows::core::PCWSTR(w.as_ptr()));
}
unsafe fn rebuild_category(hwnd: HWND, state: &mut SettingsState) {
    for ctrl in state.dynamic_controls.drain(..) {
        let _ = DestroyWindow(ctrl);
    }

    let cfg = state.config.clone();
    let mut y = 24;
    match state.category {
        Category::General => {
            add_edit_row(
                hwnd,
                state,
                "Toggle hotkey",
                ID_HOTKEY_TOGGLE,
                &cfg.hotkey.toggle,
                &mut y,
            );
            add_multiline_row(
                hwnd,
                state,
                "Shortcuts (keys=label per line)",
                ID_SHORTCUTS,
                &shortcuts_to_text(&cfg.shortcuts),
                &mut y,
                170,
            );
        }
        Category::Display => {
            add_combo_row(
                hwnd,
                state,
                "Position",
                ID_DISPLAY_POSITION,
                &["top-left", "top-center", "top-right", "bottom-left", "bottom-center", "bottom-right"],
                position_index(cfg.display.position),
                &mut y,
            );
            add_edit_row(hwnd, state, "Offset X", ID_DISPLAY_OFFSET_X, &cfg.display.offset_x.to_string(), &mut y);
            add_edit_row(hwnd, state, "Offset Y", ID_DISPLAY_OFFSET_Y, &cfg.display.offset_y.to_string(), &mut y);
            add_edit_row(hwnd, state, "Max items", ID_DISPLAY_MAX_ITEMS, &cfg.display.max_items.to_string(), &mut y);
            add_edit_row(hwnd, state, "Display duration (ms)", ID_DISPLAY_DURATION, &cfg.display.display_duration_ms.to_string(), &mut y);
            add_edit_row(hwnd, state, "Fade duration (ms)", ID_DISPLAY_FADE, &cfg.display.fade_duration_ms.to_string(), &mut y);
        }
        Category::Style => {
            add_edit_row(hwnd, state, "Font family", ID_STYLE_FONT_FAMILY, &cfg.style.font_family, &mut y);
            add_edit_row(hwnd, state, "Font size", ID_STYLE_FONT_SIZE, &cfg.style.font_size.to_string(), &mut y);
            add_edit_row(hwnd, state, "Text color", ID_STYLE_TEXT_COLOR, &cfg.style.text_color, &mut y);
            add_edit_row(hwnd, state, "Background color", ID_STYLE_BACKGROUND_COLOR, &cfg.style.background_color, &mut y);
            add_edit_row(hwnd, state, "Border radius", ID_STYLE_BORDER_RADIUS, &cfg.style.border_radius.to_string(), &mut y);
            add_edit_row(hwnd, state, "Padding", ID_STYLE_PADDING, &cfg.style.padding.to_string(), &mut y);
            add_edit_row(hwnd, state, "Shortcut color", ID_STYLE_SHORTCUT_COLOR, &cfg.style.shortcut_color, &mut y);
            add_edit_row(hwnd, state, "Key down color", ID_STYLE_KEY_DOWN_COLOR, &cfg.style.key_down_color, &mut y);
            add_edit_row(hwnd, state, "Opacity (0-1)", ID_STYLE_OPACITY, &cfg.style.opacity.to_string(), &mut y);
        }
        Category::Input => {
            add_check_row(hwnd, state, "Show key down/up", ID_BEHAVIOR_SHOW_KEY_DOWN_UP, cfg.behavior.show_key_down_up, &mut y);
            add_check_row(hwnd, state, "Show repeat count", ID_BEHAVIOR_SHOW_REPEAT_COUNT, cfg.behavior.show_repeat_count, &mut y);
            add_check_row(hwnd, state, "Distinguish numpad", ID_BEHAVIOR_DISTINGUISH_NUMPAD, cfg.behavior.distinguish_numpad, &mut y);
            add_check_row(hwnd, state, "Show IME composition", ID_BEHAVIOR_SHOW_IME, cfg.behavior.show_ime_composition, &mut y);
            add_check_row(hwnd, state, "Show clipboard", ID_BEHAVIOR_SHOW_CLIPBOARD, cfg.behavior.show_clipboard, &mut y);
            add_edit_row(hwnd, state, "Clipboard max chars", ID_BEHAVIOR_CLIPBOARD_MAX_CHARS, &cfg.behavior.clipboard_max_chars.to_string(), &mut y);
            add_check_row(hwnd, state, "Show lock indicators", ID_BEHAVIOR_SHOW_LOCK, cfg.behavior.show_lock_indicators, &mut y);
            add_edit_row(hwnd, state, "Repeat timeout (ms)", ID_BEHAVIOR_REPEAT_TIMEOUT, &cfg.behavior.repeat_timeout_ms.to_string(), &mut y);
            add_edit_row(hwnd, state, "Group timeout (ms)", ID_BEHAVIOR_GROUP_TIMEOUT, &cfg.behavior.group_timeout_ms.to_string(), &mut y);
            add_edit_row(hwnd, state, "Max group size", ID_BEHAVIOR_MAX_GROUP_SIZE, &cfg.behavior.max_group_size.to_string(), &mut y);
            add_check_row(hwnd, state, "Exclude from capture", ID_BEHAVIOR_EXCLUDE_CAPTURE, cfg.behavior.exclude_from_capture, &mut y);
            add_multiline_row(
                hwnd,
                state,
                "Ignored keys (one key label per line)",
                ID_BEHAVIOR_IGNORED_KEYS,
                &cfg.behavior.ignored_keys.join("\r\n"),
                &mut y,
                100,
            );
        }
        Category::Privacy => {
            add_check_row(hwnd, state, "Privacy filter enabled", ID_PRIVACY_ENABLED, cfg.privacy.enabled, &mut y);
            add_multiline_row(
                hwnd,
                state,
                "Blocked process names (one .exe per line)",
                ID_PRIVACY_BLOCKED_APPS,
                &cfg.privacy.blocked_apps.join("\r\n"),
                &mut y,
                200,
            );
        }
        Category::Performance => {
            add_edit_row(hwnd, state, "OSD width", ID_PERF_OSD_WIDTH, &cfg.performance.osd_width.to_string(), &mut y);
            add_edit_row(hwnd, state, "OSD height", ID_PERF_OSD_HEIGHT, &cfg.performance.osd_height.to_string(), &mut y);
            add_edit_row(hwnd, state, "IME poll interval (ms)", ID_PERF_IME_POLL, &cfg.performance.ime_poll_interval_ms.to_string(), &mut y);
            add_edit_row(hwnd, state, "Frame interval (ms)", ID_PERF_FRAME_INTERVAL, &cfg.performance.frame_interval_ms.to_string(), &mut y);
            add_edit_row(hwnd, state, "Config reload interval (ms)", ID_PERF_RELOAD_INTERVAL, &cfg.performance.config_reload_interval_ms.to_string(), &mut y);
        }
        Category::Diagnostics => {
            add_combo_row(
                hwnd,
                state,
                "Level",
                ID_DIAG_LEVEL,
                &["error", "warn", "info", "debug"],
                diag_level_index(cfg.diagnostics.level),
                &mut y,
            );
            add_check_row(hwnd, state, "Enable file logging", ID_DIAG_FILE_ENABLED, cfg.diagnostics.file_logging_enabled, &mut y);
            add_edit_row(hwnd, state, "Max file bytes", ID_DIAG_MAX_BYTES, &cfg.diagnostics.max_file_bytes.to_string(), &mut y);
            add_edit_row(hwnd, state, "Max files", ID_DIAG_MAX_FILES, &cfg.diagnostics.max_files.to_string(), &mut y);
        }
        Category::Startup => {
            add_check_row(hwnd, state, "Enable autostart", ID_STARTUP_AUTOSTART, cfg.startup.autostart_enabled, &mut y);
        }
        Category::Tray => {
            add_check_row(hwnd, state, "OSD enabled on startup", ID_TRAY_START_OSD, cfg.tray.start_osd_enabled, &mut y);
            add_combo_row(
                hwnd,
                state,
                "Menu language",
                ID_TRAY_MENU_LANGUAGE,
                &["ja", "en"],
                if cfg.tray.menu_language == MenuLanguage::Ja { 0 } else { 1 },
                &mut y,
            );
            add_check_row(hwnd, state, "Confirm on exit", ID_TRAY_CONFIRM_EXIT, cfg.tray.confirm_on_exit, &mut y);
        }
        Category::Animation => {
            add_combo_row(
                hwnd,
                state,
                "Ghost modifier",
                ID_ANIM_GHOST_MODIFIER,
                &["ctrl", "alt", "shift"],
                ghost_modifier_index(cfg.animation.ghost_modifier),
                &mut y,
            );
            add_edit_row(hwnd, state, "Ghost threshold (px)", ID_ANIM_GHOST_THRESHOLD, &cfg.animation.ghost_threshold_px.to_string(), &mut y);
            add_edit_row(hwnd, state, "Ghost max opacity", ID_ANIM_GHOST_MAX_OPACITY, &cfg.animation.ghost_max_opacity.to_string(), &mut y);
            add_combo_row(
                hwnd,
                state,
                "Fade out curve",
                ID_ANIM_FADE_CURVE,
                &["linear", "ease_out"],
                if cfg.animation.fade_out_curve == FadeOutCurve::Linear { 0 } else { 1 },
                &mut y,
            );
        }
    }
}

unsafe fn create_label(parent: HWND, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let wide = to_wide(text);
    CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("STATIC"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        h,
        parent,
        None,
        None,
        None,
    )
    .unwrap_or_default()
}

unsafe fn create_edit(parent: HWND, id: u16, value: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let wide = to_wide(value);
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        windows::core::w!("EDIT"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as usize as *mut _),
        None,
        None,
    )
    .unwrap_or_default()
}

unsafe fn create_multiline_edit(
    parent: HWND,
    id: u16,
    value: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let wide = to_wide(value);
    CreateWindowExW(
        WS_EX_CLIENTEDGE,
        windows::core::w!("EDIT"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WINDOW_STYLE(ES_MULTILINE as u32)
            | WINDOW_STYLE(ES_AUTOVSCROLL as u32)
            | WINDOW_STYLE(WS_VSCROLL.0),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as usize as *mut _),
        None,
        None,
    )
    .unwrap_or_default()
}

unsafe fn create_checkbox(parent: HWND, id: u16, text: &str, checked: bool, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let wide = to_wide(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("BUTTON"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as usize as *mut _),
        None,
        None,
    )
    .unwrap_or_default();

    let state = if checked { 1usize } else { 0usize };
    let _ = SendMessageW(hwnd, BM_SETCHECK, WPARAM(state), LPARAM(0));
    hwnd
}

unsafe fn create_combo(
    parent: HWND,
    id: u16,
    options: &[&str],
    selected_idx: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("COMBOBOX"),
        None,
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as usize as *mut _),
        None,
        None,
    )
    .unwrap_or_default();

    for opt in options {
        let wide = to_wide(opt);
        let _ = SendMessageW(
            hwnd,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(wide.as_ptr() as isize),
        );
    }

    let _ = SendMessageW(hwnd, CB_SETCURSEL, WPARAM(selected_idx as usize), LPARAM(0));
    hwnd
}

unsafe fn add_edit_row(
    hwnd: HWND,
    state: &mut SettingsState,
    label: &str,
    id: u16,
    value: &str,
    y: &mut i32,
) {
    let l = create_label(hwnd, label, 250, *y, 220, 22);
    let e = create_edit(hwnd, id, value, 480, *y - 2, 340, 24);
    state.dynamic_controls.push(l);
    state.dynamic_controls.push(e);
    *y += 30;
}

unsafe fn add_multiline_row(
    hwnd: HWND,
    state: &mut SettingsState,
    label: &str,
    id: u16,
    value: &str,
    y: &mut i32,
    height: i32,
) {
    let l = create_label(hwnd, label, 250, *y, 560, 22);
    let e = create_multiline_edit(hwnd, id, value, 250, *y + 22, 570, height);
    state.dynamic_controls.push(l);
    state.dynamic_controls.push(e);
    *y += height + 36;
}

unsafe fn add_check_row(
    hwnd: HWND,
    state: &mut SettingsState,
    label: &str,
    id: u16,
    checked: bool,
    y: &mut i32,
) {
    let c = create_checkbox(hwnd, id, label, checked, 250, *y, 480, 24);
    state.dynamic_controls.push(c);
    *y += 30;
}

unsafe fn add_combo_row(
    hwnd: HWND,
    state: &mut SettingsState,
    label: &str,
    id: u16,
    options: &[&str],
    selected_idx: i32,
    y: &mut i32,
) {
    let l = create_label(hwnd, label, 250, *y, 220, 22);
    let c = create_combo(hwnd, id, options, selected_idx, 480, *y - 2, 220, 300);
    state.dynamic_controls.push(l);
    state.dynamic_controls.push(c);
    *y += 30;
}
unsafe fn get_text(hwnd: HWND) -> String {
    let mut buf = vec![0u16; 4096];
    let len = GetWindowTextW(hwnd, &mut buf) as usize;
    String::from_utf16_lossy(&buf[..len])
}

unsafe fn get_edit_i32(parent: HWND, id: u16) -> Result<i32, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd)
        .trim()
        .parse::<i32>()
        .map_err(|_| format!("id {} expects i32", id))
}

unsafe fn get_edit_u64(parent: HWND, id: u16) -> Result<u64, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd)
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("id {} expects u64", id))
}

unsafe fn get_edit_u32(parent: HWND, id: u16) -> Result<u32, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd)
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("id {} expects u32", id))
}

unsafe fn get_edit_usize(parent: HWND, id: u16) -> Result<usize, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd)
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("id {} expects usize", id))
}

unsafe fn get_edit_f32(parent: HWND, id: u16) -> Result<f32, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd)
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("id {} expects f32", id))
}

unsafe fn get_edit_string(parent: HWND, id: u16) -> String {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    get_text(hwnd).trim().to_string()
}

unsafe fn get_checkbox(parent: HWND, id: u16) -> bool {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 as u32 != 0
}

unsafe fn get_combo_index(parent: HWND, id: u16) -> Result<i32, String> {
    let hwnd = GetDlgItem(parent, id as i32).unwrap_or_default();
    let idx = SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as i32;
    if idx < 0 {
        Err(format!("id {} expects combo selection", id))
    } else {
        Ok(idx)
    }
}

unsafe fn apply_control_to_config(parent: HWND, id: u16, cfg: &mut AppConfig) -> Result<(), String> {
    match id {
        ID_HOTKEY_TOGGLE => cfg.hotkey.toggle = get_edit_string(parent, id),
        ID_SHORTCUTS => {
            let text = get_edit_string(parent, id);
            cfg.shortcuts = parse_shortcuts(&text)?;
        }

        ID_DISPLAY_POSITION => {
            cfg.display.position = match get_combo_index(parent, id)? {
                0 => Position::TopLeft,
                1 => Position::TopCenter,
                2 => Position::TopRight,
                3 => Position::BottomLeft,
                4 => Position::BottomCenter,
                5 => Position::BottomRight,
                _ => return Err("invalid display.position".into()),
            }
        }
        ID_DISPLAY_OFFSET_X => cfg.display.offset_x = get_edit_i32(parent, id)?,
        ID_DISPLAY_OFFSET_Y => cfg.display.offset_y = get_edit_i32(parent, id)?,
        ID_DISPLAY_MAX_ITEMS => cfg.display.max_items = get_edit_usize(parent, id)?,
        ID_DISPLAY_DURATION => cfg.display.display_duration_ms = get_edit_u64(parent, id)?,
        ID_DISPLAY_FADE => cfg.display.fade_duration_ms = get_edit_u64(parent, id)?,

        ID_STYLE_FONT_FAMILY => cfg.style.font_family = get_edit_string(parent, id),
        ID_STYLE_FONT_SIZE => cfg.style.font_size = get_edit_f32(parent, id)?,
        ID_STYLE_TEXT_COLOR => cfg.style.text_color = get_edit_string(parent, id),
        ID_STYLE_BACKGROUND_COLOR => cfg.style.background_color = get_edit_string(parent, id),
        ID_STYLE_BORDER_RADIUS => cfg.style.border_radius = get_edit_f32(parent, id)?,
        ID_STYLE_PADDING => cfg.style.padding = get_edit_f32(parent, id)?,
        ID_STYLE_SHORTCUT_COLOR => cfg.style.shortcut_color = get_edit_string(parent, id),
        ID_STYLE_KEY_DOWN_COLOR => cfg.style.key_down_color = get_edit_string(parent, id),
        ID_STYLE_OPACITY => cfg.style.opacity = get_edit_f32(parent, id)?,

        ID_BEHAVIOR_SHOW_KEY_DOWN_UP => cfg.behavior.show_key_down_up = get_checkbox(parent, id),
        ID_BEHAVIOR_SHOW_REPEAT_COUNT => cfg.behavior.show_repeat_count = get_checkbox(parent, id),
        ID_BEHAVIOR_DISTINGUISH_NUMPAD => cfg.behavior.distinguish_numpad = get_checkbox(parent, id),
        ID_BEHAVIOR_SHOW_IME => cfg.behavior.show_ime_composition = get_checkbox(parent, id),
        ID_BEHAVIOR_SHOW_CLIPBOARD => cfg.behavior.show_clipboard = get_checkbox(parent, id),
        ID_BEHAVIOR_CLIPBOARD_MAX_CHARS => cfg.behavior.clipboard_max_chars = get_edit_usize(parent, id)?,
        ID_BEHAVIOR_SHOW_LOCK => cfg.behavior.show_lock_indicators = get_checkbox(parent, id),
        ID_BEHAVIOR_REPEAT_TIMEOUT => cfg.behavior.repeat_timeout_ms = get_edit_u64(parent, id)?,
        ID_BEHAVIOR_GROUP_TIMEOUT => cfg.behavior.group_timeout_ms = get_edit_u64(parent, id)?,
        ID_BEHAVIOR_MAX_GROUP_SIZE => cfg.behavior.max_group_size = get_edit_usize(parent, id)?,
        ID_BEHAVIOR_IGNORED_KEYS => {
            let text = get_edit_string(parent, id);
            cfg.behavior.ignored_keys = split_lines(&text);
        }
        ID_BEHAVIOR_EXCLUDE_CAPTURE => cfg.behavior.exclude_from_capture = get_checkbox(parent, id),

        ID_PRIVACY_ENABLED => cfg.privacy.enabled = get_checkbox(parent, id),
        ID_PRIVACY_BLOCKED_APPS => {
            let text = get_edit_string(parent, id);
            cfg.privacy.blocked_apps = split_lines(&text);
        }

        ID_PERF_OSD_WIDTH => cfg.performance.osd_width = get_edit_i32(parent, id)?,
        ID_PERF_OSD_HEIGHT => cfg.performance.osd_height = get_edit_i32(parent, id)?,
        ID_PERF_IME_POLL => cfg.performance.ime_poll_interval_ms = get_edit_u64(parent, id)?,
        ID_PERF_FRAME_INTERVAL => cfg.performance.frame_interval_ms = get_edit_u64(parent, id)?,
        ID_PERF_RELOAD_INTERVAL => cfg.performance.config_reload_interval_ms = get_edit_u64(parent, id)?,

        ID_DIAG_LEVEL => {
            cfg.diagnostics.level = match get_combo_index(parent, id)? {
                0 => DiagnosticsLevel::Error,
                1 => DiagnosticsLevel::Warn,
                2 => DiagnosticsLevel::Info,
                3 => DiagnosticsLevel::Debug,
                _ => return Err("invalid diagnostics.level".into()),
            }
        }
        ID_DIAG_FILE_ENABLED => cfg.diagnostics.file_logging_enabled = get_checkbox(parent, id),
        ID_DIAG_MAX_BYTES => cfg.diagnostics.max_file_bytes = get_edit_u64(parent, id)?,
        ID_DIAG_MAX_FILES => cfg.diagnostics.max_files = get_edit_u32(parent, id)?,

        ID_STARTUP_AUTOSTART => cfg.startup.autostart_enabled = get_checkbox(parent, id),

        ID_TRAY_START_OSD => cfg.tray.start_osd_enabled = get_checkbox(parent, id),
        ID_TRAY_MENU_LANGUAGE => {
            cfg.tray.menu_language = match get_combo_index(parent, id)? {
                0 => MenuLanguage::Ja,
                1 => MenuLanguage::En,
                _ => return Err("invalid tray.menu_language".into()),
            }
        }
        ID_TRAY_CONFIRM_EXIT => cfg.tray.confirm_on_exit = get_checkbox(parent, id),

        ID_ANIM_GHOST_MODIFIER => {
            cfg.animation.ghost_modifier = match get_combo_index(parent, id)? {
                0 => GhostModifier::Ctrl,
                1 => GhostModifier::Alt,
                2 => GhostModifier::Shift,
                _ => return Err("invalid animation.ghost_modifier".into()),
            }
        }
        ID_ANIM_GHOST_THRESHOLD => cfg.animation.ghost_threshold_px = get_edit_f32(parent, id)?,
        ID_ANIM_GHOST_MAX_OPACITY => cfg.animation.ghost_max_opacity = get_edit_f32(parent, id)?,
        ID_ANIM_FADE_CURVE => {
            cfg.animation.fade_out_curve = match get_combo_index(parent, id)? {
                0 => FadeOutCurve::Linear,
                1 => FadeOutCurve::EaseOut,
                _ => return Err("invalid animation.fade_out_curve".into()),
            }
        }
        _ => {}
    }

    Ok(())
}

fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn shortcuts_to_text(shortcuts: &[ShortcutDef]) -> String {
    shortcuts
        .iter()
        .map(|s| format!("{}={}", s.keys, s.label))
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn parse_shortcuts(text: &str) -> Result<Vec<ShortcutDef>, String> {
    let mut shortcuts = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((keys, label)) = trimmed.split_once('=') else {
            return Err(format!("shortcut line {} must be keys=label", i + 1));
        };

        let keys = keys.trim();
        let label = label.trim();
        if keys.is_empty() || label.is_empty() {
            return Err(format!("shortcut line {} must not be empty", i + 1));
        }

        shortcuts.push(ShortcutDef {
            keys: keys.to_string(),
            label: label.to_string(),
        });
    }

    Ok(shortcuts)
}

fn position_index(pos: Position) -> i32 {
    match pos {
        Position::TopLeft => 0,
        Position::TopCenter => 1,
        Position::TopRight => 2,
        Position::BottomLeft => 3,
        Position::BottomCenter => 4,
        Position::BottomRight => 5,
    }
}

fn ghost_modifier_index(m: GhostModifier) -> i32 {
    match m {
        GhostModifier::Ctrl => 0,
        GhostModifier::Alt => 1,
        GhostModifier::Shift => 2,
    }
}

fn diag_level_index(l: DiagnosticsLevel) -> i32 {
    match l {
        DiagnosticsLevel::Error => 0,
        DiagnosticsLevel::Warn => 1,
        DiagnosticsLevel::Info => 2,
        DiagnosticsLevel::Debug => 3,
    }
}

pub fn open_settings_window(
    config: &AppConfig,
    config_path: &Path,
    notify_tx: Option<SyncSender<InputEvent>>,
) {
    if SETTINGS_OPEN.with(|c| c.get()) {
        return;
    }

    unsafe {
        let class_name = to_wide("yStrokeySettingsV2");
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

        let title = to_wide("yStrokey Settings");
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            880,
            680,
            None,
            None,
            None,
            None,
        )
        .unwrap_or_default();

        if hwnd.0.is_null() {
            return;
        }

        let nav = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            windows::core::w!("LISTBOX"),
            None,
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(LBS_NOTIFY as u32),
            20,
            20,
            210,
            560,
            hwnd,
            HMENU(ID_NAV as usize as *mut _),
            None,
            None,
        )
        .unwrap_or_default();

        let categories = [
            "General",
            "Display",
            "Style",
            "Input",
            "Privacy",
            "Performance",
            "Diagnostics",
            "Startup",
            "Tray",
            "Animation",
        ];
        for c in categories {
            let w = to_wide(c);
            let _ = SendMessageW(nav, LB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
        }
        let _ = SendMessageW(nav, LB_SETCURSEL, WPARAM(0), LPARAM(0));

        let _ = create_button(hwnd, "Revert Section", ID_BTN_REVERT_SECTION, 250, 590, 140, 32);
        let _ = create_button(hwnd, "Reset Defaults", ID_BTN_RESET_ALL, 400, 590, 140, 32);
        let _ = create_button(hwnd, "Close", ID_BTN_CLOSE, 740, 590, 80, 32);

        let status = create_label(hwnd, "", 250, 628, 570, 20);

        let mut state = Box::new(SettingsState {
            config: config.clone(),
            config_path: config_path.to_path_buf(),
            notify_tx,
            category: Category::General,
            nav,
            status,
            dynamic_controls: Vec::new(),
        });

        rebuild_category(hwnd, &mut state);

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        SETTINGS_OPEN.with(|c| c.set(true));

        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

unsafe fn create_button(parent: HWND, text: &str, id: u16, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let wide = to_wide(text);
    CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        windows::core::w!("BUTTON"),
        windows::core::PCWSTR(wide.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as usize as *mut _),
        None,
        None,
    )
    .unwrap_or_default()
}
