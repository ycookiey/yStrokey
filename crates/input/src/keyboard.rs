use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VK_CAPITAL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_NUMLOCK, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SCROLL,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::{InputEvent, KeyAction, KeyCode, KeyEvent, LockStateEvent, Modifiers};

thread_local! {
    static HOOK_SENDER: std::cell::RefCell<Option<SyncSender<InputEvent>>> =
        const { std::cell::RefCell::new(None) };
}

/// KBDLLHOOKSTRUCT からテンキーを区別して KeyCode に変換
fn to_key_code(kb: &KBDLLHOOKSTRUCT) -> KeyCode {
    let vk = kb.vkCode;
    let scan = kb.scanCode;
    let extended = (kb.flags.0 & 0x01) != 0; // LLKHF_EXTENDED

    match vk {
        // Numpad 0-9
        0x60..=0x69 => KeyCode(vk),
        // Numpad演算子 (*, +, separator, -, ., /)
        0x6A..=0x6F => KeyCode(vk),
        // Enter: extended = Numpad Enter
        0x0D if extended => KeyCode::NUMPAD_ENTER,
        0x0D => KeyCode::ENTER,
        // NumLockオフ時: スキャンコード 0x47-0x53 で非extended → テンキー由来
        _ if (0x47..=0x53).contains(&scan) && !extended => numpad_scan_to_key(scan),
        _ => KeyCode(vk),
    }
}

/// NumLockオフ時のスキャンコードからNumpadキーへの変換
fn numpad_scan_to_key(scan: u32) -> KeyCode {
    match scan {
        0x47 => KeyCode::NUMPAD_7,
        0x48 => KeyCode::NUMPAD_8,
        0x49 => KeyCode::NUMPAD_9,
        0x4B => KeyCode::NUMPAD_4,
        0x4C => KeyCode::NUMPAD_5,
        0x4D => KeyCode::NUMPAD_6,
        0x4F => KeyCode::NUMPAD_1,
        0x50 => KeyCode::NUMPAD_2,
        0x51 => KeyCode::NUMPAD_3,
        0x52 => KeyCode::NUMPAD_0,
        0x53 => KeyCode::NUMPAD_DECIMAL,
        _ => KeyCode(scan),
    }
}

/// GetAsyncKeyState で現在の修飾キー状態を取得
fn get_current_modifiers() -> Modifiers {
    unsafe {
        Modifiers {
            ctrl: GetAsyncKeyState(VK_LCONTROL.0 as i32) < 0
                || GetAsyncKeyState(VK_RCONTROL.0 as i32) < 0,
            shift: GetAsyncKeyState(VK_LSHIFT.0 as i32) < 0
                || GetAsyncKeyState(VK_RSHIFT.0 as i32) < 0,
            alt: GetAsyncKeyState(VK_LMENU.0 as i32) < 0
                || GetAsyncKeyState(VK_RMENU.0 as i32) < 0,
            win: GetAsyncKeyState(VK_LWIN.0 as i32) < 0
                || GetAsyncKeyState(VK_RWIN.0 as i32) < 0,
        }
    }
}

/// テンキー由来かどうかを判定
fn is_numpad_key(kb: &KBDLLHOOKSTRUCT) -> bool {
    let vk = kb.vkCode;
    let scan = kb.scanCode;
    let extended = (kb.flags.0 & 0x01) != 0;

    // VK_NUMPAD0-9, 演算子
    if (0x60..=0x6F).contains(&vk) {
        return true;
    }
    // Numpad Enter (VK_RETURN + extended)
    if vk == 0x0D && extended {
        return true;
    }
    // NumLockオフ時のスキャンコード範囲 (非extended)
    if (0x47..=0x53).contains(&scan) && !extended {
        return true;
    }
    false
}


/// Lock key (CapsLock/NumLock/ScrollLock)
fn is_lock_key(vk: u32) -> bool {
    vk == VK_CAPITAL.0 as u32 || vk == VK_NUMLOCK.0 as u32 || vk == VK_SCROLL.0 as u32
}

/// Get current lock state
fn get_lock_state_event() -> LockStateEvent {
    unsafe {
        LockStateEvent {
            caps_lock: (GetKeyState(VK_CAPITAL.0 as i32) & 1) != 0,
            num_lock: (GetKeyState(VK_NUMLOCK.0 as i32) & 1) != 0,
            scroll_lock: (GetKeyState(VK_SCROLL.0 as i32) & 1) != 0,
            timestamp: Instant::now(),
        }
    }
}

/// キーボードフックコールバック
unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let action = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => KeyAction::Down,
            WM_KEYUP | WM_SYSKEYUP => KeyAction::Up,
            _ => return CallNextHookEx(None, code, wparam, lparam),
        };

        let key_code = to_key_code(kb);
        let modifiers = get_current_modifiers();

        let event = InputEvent::Key(KeyEvent {
            key: key_code,
            action,
            modifiers,
            is_numpad: is_numpad_key(kb),
            scan_code: kb.scanCode,
            timestamp: Instant::now(),
        });

        // try_send: バッファフルなら破棄（フックコールバックはブロック不可）
        HOOK_SENDER.with(|cell| {
            if let Some(ref tx) = *cell.borrow() {
                let _ = tx.try_send(event);
                // Lock key: send toggle state on WM_KEYUP
                if action == KeyAction::Up && is_lock_key(kb.vkCode) {
                    let lock_event = InputEvent::LockState(get_lock_state_event());
                    let _ = tx.try_send(lock_event);
                }
            }
        });
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// フックスレッドを起動してメッセージループを実行
pub fn run_hook_thread(tx: SyncSender<InputEvent>) {
    HOOK_SENDER.with(|cell| {
        cell.replace(Some(tx));
    });

    unsafe {
        let hmod = GetModuleHandleW(None).ok().map(|h| HINSTANCE(h.0));
        let kb_hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmod.unwrap_or_default(), 0) {
            Ok(hook) => hook,
            Err(e) => {
                eprintln!("keyboard hook install failed: {e}");
                return;
            }
        };

        // LL hookはメッセージループが必須
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(kb_hook);
    }
}

/// キーボードフックを別スレッドで起動するヘルパー
pub fn install_keyboard_hook(tx: SyncSender<InputEvent>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("keyboard-hook".into())
        .spawn(move || run_hook_thread(tx))
        .unwrap_or_else(|e| {
            eprintln!("keyboard hook thread spawn failed: {e}");
            // フォールバック: 現在のスレッドでダミーハンドルを返す
            std::thread::spawn(|| {})
        })
}
