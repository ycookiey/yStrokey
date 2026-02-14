use std::ffi::c_void;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::Input::Ime::{
    GCS_COMPSTR, GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmGetOpenStatus,
    ImmReleaseContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

use ystrokey_core::{ImeEvent, ImeEventKind, InputEvent};

struct InputAttachGuard {
    current_tid: u32,
    target_tid: u32,
    attached: bool,
}

impl InputAttachGuard {
    fn maybe_attach(hwnd: HWND) -> Self {
        unsafe {
            let current_tid = GetCurrentThreadId();
            let target_tid = GetWindowThreadProcessId(hwnd, None);
            let attached = target_tid != 0
                && target_tid != current_tid
                && AttachThreadInput(current_tid, target_tid, true).as_bool();
            Self {
                current_tid,
                target_tid,
                attached,
            }
        }
    }
}

impl Drop for InputAttachGuard {
    fn drop(&mut self) {
        if self.attached {
            unsafe {
                let _ = AttachThreadInput(self.current_tid, self.target_tid, false);
            }
        }
    }
}

fn resolve_ime_focus_hwnd() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }

        let tid = GetWindowThreadProcessId(fg, None);
        if tid == 0 {
            return Some(fg);
        }

        let current_tid = GetCurrentThreadId();
        if tid != current_tid {
            let attached = AttachThreadInput(current_tid, tid, true).as_bool();
            let focus = GetFocus();
            if attached {
                let _ = AttachThreadInput(current_tid, tid, false);
            }
            if !focus.is_invalid() {
                return Some(focus);
            }
        } else {
            let focus = GetFocus();
            if !focus.is_invalid() {
                return Some(focus);
            }
        }

        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(tid, &mut info).is_ok() {
            if !info.hwndFocus.is_invalid() {
                return Some(info.hwndFocus);
            }
            if !info.hwndActive.is_invalid() {
                return Some(info.hwndActive);
            }
        }

        Some(fg)
    }
}

fn collect_ime_targets() -> Vec<HWND> {
    let mut out = Vec::with_capacity(3);

    if let Some(focus) = resolve_ime_focus_hwnd() {
        out.push(focus);
    }

    let fg = unsafe { GetForegroundWindow() };
    if !fg.is_invalid() && !out.iter().any(|h| *h == fg) {
        out.push(fg);
    }

    out
}

/// IME変換中の文字列（ひらがな等）を取得
pub fn get_composition_string(hwnd: HWND) -> Option<String> {
    unsafe {
        let _attach = InputAttachGuard::maybe_attach(hwnd);
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
        let _attach = InputAttachGuard::maybe_attach(hwnd);
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
        let _attach = InputAttachGuard::maybe_attach(hwnd);
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

    let targets = collect_ime_targets();
    if targets.is_empty() {
        return;
    }

    // IME ON/OFF状態チェック
    let mut comp = String::new();
    let mut ime_open = false;

    for hwnd in &targets {
        ime_open |= is_ime_open(*hwnd);

        if comp.is_empty() {
            if let Some(s) = get_composition_string(*hwnd) {
                if !s.is_empty() {
                    comp = s;
                }
            }
        }
    }

    let prev_open = PREV_IME_OPEN.with(|c| c.get());
    if ime_open != prev_open {
        PREV_IME_OPEN.with(|c| c.set(ime_open));
        let event = InputEvent::Ime(ImeEvent {
            kind: ImeEventKind::StateChanged { enabled: ime_open },
            timestamp: Instant::now(),
        });
        let _ = tx.try_send(event);
    }

    // 変換中文字列チェック（IME ON/OFF判定に依存せず文字列変化で更新）
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
