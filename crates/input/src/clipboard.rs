use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable,
    OpenClipboard, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// クリップボード変更リスナー
///
/// `AddClipboardFormatListener` で登録し、Drop時に `RemoveClipboardFormatListener` で解除。
/// WM_CLIPBOARDUPDATE メッセージで変更通知を受信する。
pub struct ClipboardListener {
    hwnd: HWND,
}

impl ClipboardListener {
    /// クリップボード監視を開始
    pub fn new(hwnd: HWND) -> windows::core::Result<Self> {
        unsafe {
            AddClipboardFormatListener(hwnd)?;
        }
        Ok(Self { hwnd })
    }

    /// クリップボードからUnicodeテキストを取得
    pub fn get_text(hwnd: HWND) -> Option<String> {
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
                return None;
            }

            if OpenClipboard(hwnd).is_err() {
                return None;
            }

            // CloseClipboard を確実に呼ぶため、クロージャで本体を実行
            let result = (|| -> Option<String> {
                let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
                let hglobal = HGLOBAL(handle.0);
                let size = GlobalSize(hglobal);
                let max_u16_len = if size > 0 { size / 2 } else { usize::MAX };
                let ptr = GlobalLock(hglobal) as *const u16;
                if ptr.is_null() {
                    return None;
                }

                // null終端までの長さを計算（GlobalSize上限付き）
                let mut len = 0;
                while len < max_u16_len && *ptr.add(len) != 0 {
                    len += 1;
                }

                let slice = std::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);

                let _ = GlobalUnlock(hglobal);

                Some(text)
            })();

            let _ = CloseClipboard();
            result
        }
    }
}

impl Drop for ClipboardListener {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveClipboardFormatListener(self.hwnd);
        }
    }
}
