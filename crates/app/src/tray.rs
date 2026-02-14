use std::mem;

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::MenuLanguage;

pub const WM_TRAYICON: u32 = WM_USER + 1;
pub const ID_TRAY_TOGGLE: u32 = 1001;
pub const ID_TRAY_EXIT: u32 = 1002;
pub const ID_TRAY_AUTOSTART: u32 = 1003;
pub const ID_TRAY_SETTINGS: u32 = 1004;
pub const ID_TRAY_EXPORT: u32 = 1005;
pub const ID_TRAY_IMPORT: u32 = 1006;

/// システムトレイアイコン
pub struct TrayIcon {
    hwnd: HWND,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> windows::core::Result<Self> {
        unsafe {
            let icon = LoadIconW(None, IDI_APPLICATION)?;

            let mut nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAYICON,
                hIcon: icon,
                ..Default::default()
            };

            // ツールチップ（szTip: [u16; 128] 固定長配列）
            let tip: Vec<u16> = "yStrokey"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let len = tip.len().min(nid.szTip.len());
            nid.szTip[..len].copy_from_slice(&tip[..len]);

            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                return Err(windows::core::Error::from_win32());
            }

            Ok(Self { hwnd })
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

/// トレイ右クリックメニューを表示
pub fn show_context_menu(
    hwnd: HWND,
    menu_language: MenuLanguage,
    osd_enabled: bool,
    autostart_enabled: bool,
) {
    unsafe {
        let menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return,
        };

        let toggle_flags = if osd_enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        let autostart_flags = if autostart_enabled {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };

        let _ = AppendMenuW(
            menu,
            toggle_flags,
            ID_TRAY_TOGGLE as usize,
            match menu_language {
                MenuLanguage::Ja => w!("有効/無効 切替 (&T)"),
                MenuLanguage::En => w!("Toggle OSD (&T)"),
            },
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(
            menu,
            autostart_flags,
            ID_TRAY_AUTOSTART as usize,
            match menu_language {
                MenuLanguage::Ja => w!("自動起動 (&A)"),
                MenuLanguage::En => w!("Auto Start (&A)"),
            },
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            ID_TRAY_SETTINGS as usize,
            match menu_language {
                MenuLanguage::Ja => w!("設定 (&S)"),
                MenuLanguage::En => w!("Settings (&S)"),
            },
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            ID_TRAY_EXPORT as usize,
            match menu_language {
                MenuLanguage::Ja => w!("エクスポート (&E)"),
                MenuLanguage::En => w!("Export (&E)"),
            },
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            ID_TRAY_IMPORT as usize,
            match menu_language {
                MenuLanguage::Ja => w!("インポート (&I)"),
                MenuLanguage::En => w!("Import (&I)"),
            },
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            ID_TRAY_EXIT as usize,
            match menu_language {
                MenuLanguage::Ja => w!("終了 (&X)"),
                MenuLanguage::En => w!("Exit (&X)"),
            },
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        // TrackPopupMenu が正しくメニューを閉じるために必須
        let _ = SetForegroundWindow(hwnd);

        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);

        // メニュー後始末メッセージ
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}
