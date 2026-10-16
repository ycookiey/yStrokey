use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::SyncSender;
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Win32::System::Threading::GetCurrentThreadId;

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, ToUnicodeEx, VK_CAPITAL, VK_CONTROL, VK_LCONTROL,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SCROLL, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::{InputEvent, KeyAction, KeyCode, KeyEvent, LockStateEvent, Modifiers};

thread_local! {
    static HOOK_SENDER: std::cell::RefCell<Option<SyncSender<InputEvent>>> =
        const { std::cell::RefCell::new(None) };
}

/// イベントを送った後にWM_NULLを送るウィンドウ。メインループはメッセージが届くまで待つので、WM_NULLで起こす。0なら送らない
static WAKE_HWND: AtomicIsize = AtomicIsize::new(0);
static HOOK_WANTED: AtomicBool = AtomicBool::new(true);
/// フックのスレッドのID。0ならスレッドはまだ起動していない
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// フックのスレッドに、HOOK_WANTEDの値に合わせてフックを付けさせるか外させるスレッドメッセージ
const WM_APP_HOOK_SYNC: u32 = WM_APP + 0x10;

pub fn set_wake_window(hwnd: HWND) {
    WAKE_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
}

/// キーボードのフックを付ける・外す。低レベルのフックを付けている間は、OSがすべてのキー入力でキーボードのフックの応答を待つので、不要な間は外す
pub fn set_hook_enabled(enabled: bool) {
    HOOK_WANTED.store(enabled, Ordering::SeqCst);
    let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_APP_HOOK_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

fn wake_main() {
    let hwnd = WAKE_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(HWND(hwnd as *mut _), WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

/// KBDLLHOOKSTRUCTからテンキーを区別してKeyCodeに変換
fn to_key_code(kb: &KBDLLHOOKSTRUCT) -> KeyCode {
    let vk = kb.vkCode;
    let scan = kb.scanCode;
    let extended = (kb.flags.0 & 0x01) != 0; // LLKHF_EXTENDED

    match vk {
        // テンキーの0-9
        0x60..=0x69 => KeyCode(vk),
        // Numpad演算子(*, +, separator, -, ., /)
        0x6A..=0x6F => KeyCode(vk),
        // Enter: extendedならテンキーのEnter
        0x0D if extended => KeyCode::NUMPAD_ENTER,
        0x0D => KeyCode::ENTER,
        // NumLockオフ時: スキャンコード0x47-0x53で非extended → テンキー由来
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

/// GetAsyncKeyStateで現在の修飾キー状態を取得
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

/// VKとscan_codeから実入力文字を取得する。
/// 低レベルのフックでは呼出スレッドがキーボードフォーカスを持たないため、
/// `GetKeyboardState`ではなく`GetAsyncKeyState`から状態を再構築する。
/// `ToUnicodeEx`のflagsに渡す値4は、キーボードの内部状態であるデッドキーのバッファを
/// 変えない指定で、Windows 10 1607以降で使える。値2は、押したキーだけでなく離したキーも文字に変える別の指定。
fn resolve_text(vk: u32, scan_code: u32) -> Option<String> {
    unsafe {
        let mut ks = [0u8; 256];
        let set_async = |arr: &mut [u8; 256], v: u16| {
            let down = (GetAsyncKeyState(v as i32) as u16 & 0x8000) != 0;
            if down {
                arr[v as usize] = 0x80;
            }
        };
        set_async(&mut ks, VK_LSHIFT.0);
        set_async(&mut ks, VK_RSHIFT.0);
        set_async(&mut ks, VK_LCONTROL.0);
        set_async(&mut ks, VK_RCONTROL.0);
        set_async(&mut ks, VK_LMENU.0);
        set_async(&mut ks, VK_RMENU.0);
        if ks[VK_LSHIFT.0 as usize] != 0 || ks[VK_RSHIFT.0 as usize] != 0 {
            ks[VK_SHIFT.0 as usize] = 0x80;
        }
        if ks[VK_LCONTROL.0 as usize] != 0 || ks[VK_RCONTROL.0 as usize] != 0 {
            ks[VK_CONTROL.0 as usize] = 0x80;
        }
        if ks[VK_LMENU.0 as usize] != 0 || ks[VK_RMENU.0 as usize] != 0 {
            ks[VK_MENU.0 as usize] = 0x80;
        }
        if (GetKeyState(VK_CAPITAL.0 as i32) & 1) != 0 {
            ks[VK_CAPITAL.0 as usize] = 0x01;
        }

        // 前面のウィンドウのキーボードレイアウトを使う（JIS/US等を反映）
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let hkl = GetKeyboardLayout(tid);

        let mut buf = [0u16; 8];
        let n = ToUnicodeEx(vk, scan_code, &ks, &mut buf, 4, hkl);
        if n <= 0 {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..n as usize]);
        if s.is_empty() || s.chars().any(|c| c.is_control()) {
            return None;
        }
        Some(s)
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
    // テンキーのEnter (VK_RETURN + extended)
    if vk == 0x0D && extended {
        return true;
    }
    // NumLockオフ時のスキャンコード範囲(非extended)
    if (0x47..=0x53).contains(&scan) && !extended {
        return true;
    }
    false
}


/// CapsLock・NumLock・ScrollLockのロックキー
fn is_lock_key(vk: u32) -> bool {
    vk == VK_CAPITAL.0 as u32 || vk == VK_NUMLOCK.0 as u32 || vk == VK_SCROLL.0 as u32
}

/// 現在のロック状態を取得する
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
        // Down時のみ実文字を解決（Upは不要、デッドキー副作用を避ける意味でも限定）
        let text = if action == KeyAction::Down {
            resolve_text(kb.vkCode, kb.scanCode)
        } else {
            None
        };

        let event = InputEvent::Key(KeyEvent {
            key: key_code,
            action,
            modifiers,
            is_numpad: is_numpad_key(kb),
            scan_code: kb.scanCode,
            text,
            timestamp: Instant::now(),
        });

        // try_send: バッファフルなら破棄（フックコールバックはブロック不可）
        HOOK_SENDER.with(|cell| {
            if let Some(ref tx) = *cell.borrow() {
                let _ = tx.try_send(event);
                // ロックキー: WM_KEYUP時にトグル状態を送る
                if action == KeyAction::Up && is_lock_key(kb.vkCode) {
                    let lock_event = InputEvent::LockState(get_lock_state_event());
                    let _ = tx.try_send(lock_event);
                }
            }
        });
        wake_main();
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
        let mut kb_hook: Option<HHOOK> = None;
        let sync =|hook: &mut Option<HHOOK>| {
            let wanted = HOOK_WANTED.load(Ordering::SeqCst);
            match (wanted, hook.is_some()) {
                (true, false) => {
                    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmod.unwrap_or_default(), 0) {
                        Ok(h) => *hook = Some(h),
                        Err(e) => eprintln!("keyboard hook install failed: {e}"),
                    }
                }
                (false, true) => {
                    if let Some(h) = hook.take() {
                        let _ = UnhookWindowsHookEx(h);
                    }
                }
                _ => {}
            }
        };

        // PostThreadMessageWはメッセージキューの無いスレッドに送れないので、IDを公開する前にキューを作る
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
        // IDを公開した後にHOOK_WANTEDを読むので、公開の前に変わった値を取りこぼさない
        sync(&mut kb_hook);

        // 低レベルのフックはメッセージループが必須
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.hwnd.0.is_null() && msg.message == WM_APP_HOOK_SYNC {
                sync(&mut kb_hook);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if let Some(h) = kb_hook {
            let _ = UnhookWindowsHookEx(h);
        }
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
