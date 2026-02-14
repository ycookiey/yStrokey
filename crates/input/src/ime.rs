use std::ffi::c_void;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::Ime::{
    GCS_COMPSTR, GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmGetOpenStatus,
    ImmReleaseContext,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use ystrokey_core::{ImeEvent, ImeEventKind, InputEvent};

/// IME変換中の文字列（ひらがな等）を取得
pub fn get_composition_string(hwnd: HWND) -> Option<String> {
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return None;
        }

        let byte_len = ImmGetCompositionStringW(himc, GCS_COMPSTR, None, 0);
        if byte_len <= 0 {
            let _ = ImmReleaseContext(hwnd, himc);
            return None;
        }

        let char_count = byte_len as usize / 2;
        let mut buf: Vec<u16> = vec![0u16; char_count];

        let copied = ImmGetCompositionStringW(
            himc,
            GCS_COMPSTR,
            Some(buf.as_mut_ptr() as *mut c_void),
            byte_len as u32,
        );

        let _ = ImmReleaseContext(hwnd, himc);

        if copied > 0 {
            let len = copied as usize / 2;
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}

/// IME確定文字列を取得
pub fn get_result_string(hwnd: HWND) -> Option<String> {
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return None;
        }

        let byte_len = ImmGetCompositionStringW(himc, GCS_RESULTSTR, None, 0);
        if byte_len <= 0 {
            let _ = ImmReleaseContext(hwnd, himc);
            return None;
        }

        let char_count = byte_len as usize / 2;
        let mut buf: Vec<u16> = vec![0u16; char_count];

        let copied = ImmGetCompositionStringW(
            himc,
            GCS_RESULTSTR,
            Some(buf.as_mut_ptr() as *mut c_void),
            byte_len as u32,
        );

        let _ = ImmReleaseContext(hwnd, himc);

        if copied > 0 {
            let len = copied as usize / 2;
            Some(String::from_utf16_lossy(&buf[..len]))
        } else {
            None
        }
    }
}

/// IME ON/OFF状態を取得
pub fn is_ime_open(hwnd: HWND) -> bool {
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.is_invalid() {
            return false;
        }
        let open = ImmGetOpenStatus(himc).as_bool();
        let _ = ImmReleaseContext(hwnd, himc);
        open
    }
}

/// IME状態をポーリングしてイベントを送信
///
/// フォアグラウンドウィンドウのIME状態と変換中文字列をチェックし、
/// 前回から変化があった場合にイベントを送信する。
pub fn poll_ime_state(tx: &SyncSender<InputEvent>) {
    thread_local! {
        static PREV_IME_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static PREV_COMPOSITION: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return;
    }

    // IME ON/OFF状態チェック
    let ime_open = is_ime_open(hwnd);
    let prev_open = PREV_IME_OPEN.with(|c| c.get());
    if ime_open != prev_open {
        PREV_IME_OPEN.with(|c| c.set(ime_open));
        let event = InputEvent::Ime(ImeEvent {
            kind: ImeEventKind::StateChanged { enabled: ime_open },
            timestamp: Instant::now(),
        });
        let _ = tx.try_send(event);
    }

    // 変換中文字列チェック（IME ONの場合のみ）
    if ime_open {
        let comp = get_composition_string(hwnd).unwrap_or_default();
        let changed = PREV_COMPOSITION.with(|c| {
            let prev = c.borrow();
            comp != *prev
        });
        if changed {
            let kind = if comp.is_empty() {
                ImeEventKind::CompositionEnd { result: String::new() }
            } else {
                ImeEventKind::CompositionUpdate { text: comp.clone() }
            };
            let _ = tx.try_send(InputEvent::Ime(ImeEvent {
                kind,
                timestamp: Instant::now(),
            }));
        }
        PREV_COMPOSITION.with(|c| {
            *c.borrow_mut() = comp;
        });
    }
}
