use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::MenuLanguage;

use crate::resources::{IDI_TRAY_DARK, IDI_TRAY_LIGHT};

pub const WM_TRAYICON: u32 = WM_USER + 1;
pub const ID_TRAY_TOGGLE: u32 = 1001;
pub const ID_TRAY_EXIT: u32 = 1002;
pub const ID_TRAY_AUTOSTART: u32 = 1003;
pub const ID_TRAY_SETTINGS: u32 = 1004;
pub const ID_TRAY_EXPORT: u32 = 1005;
pub const ID_TRAY_IMPORT: u32 = 1006;

/// トレイに表示中のアイコン。テーマの切替で差し替えるので、wnd_procから届く場所に置く
static CURRENT_ICON: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// タスクバーがライトのテーマか。値が無い・読めないときはダーク扱い
fn taskbar_uses_light_theme() -> bool {
    unsafe {
        let mut value: u32 = 0;
        let mut size = mem::size_of::<u32>() as u32;
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        );
        result.is_ok() && value == 1
    }
}

/// タスクバーのテーマに合うトレイアイコンを、exeのリソースから小さいアイコンの大きさで読み込む
fn load_tray_icon() -> windows::core::Result<HICON> {
    let id = if taskbar_uses_light_theme() {
        IDI_TRAY_LIGHT
    } else {
        IDI_TRAY_DARK
    };
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let handle = LoadImageW(
            HINSTANCE(instance.0),
            PCWSTR(id as usize as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            IMAGE_FLAGS(0),
        )?;
        Ok(HICON(handle.0))
    }
}

/// システムトレイアイコン
pub struct TrayIcon {
    hwnd: HWND,
}

impl TrayIcon {
    pub fn new(hwnd: HWND) -> windows::core::Result<Self> {
        unsafe {
            let icon = load_tray_icon()?;

            let mut nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAYICON,
                hIcon: icon,
                ..Default::default()
            };

            // ツールチップ。szTipは長さ128の[u16]の固定長配列
            let tip: Vec<u16> = "yStrokey"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let len = tip.len().min(nid.szTip.len());
            nid.szTip[..len].copy_from_slice(&tip[..len]);

            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                let err = windows::core::Error::from_win32();
                let _ = DestroyIcon(icon);
                return Err(err);
            }
            CURRENT_ICON.store(icon.0, Ordering::SeqCst);

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
            let icon = CURRENT_ICON.swap(ptr::null_mut(), Ordering::SeqCst);
            if !icon.is_null() {
                let _ = DestroyIcon(HICON(icon));
            }
        }
    }
}

/// WM_SETTINGCHANGEがライト/ダークの切替(lParamが"ImmersiveColorSet")か
pub fn is_color_set_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let name = PCWSTR(lparam.0 as *const u16);
    let expected: Vec<u16> = "ImmersiveColorSet".encode_utf16().collect();
    unsafe { name.as_wide() == expected.as_slice() }
}

/// タスクバーのテーマを読み直してトレイアイコンを差し替え、前のアイコンを破棄する
pub fn update_icon_for_theme(hwnd: HWND) {
    // トレイアイコンの作成前・破棄後は何もしない
    if CURRENT_ICON.load(Ordering::SeqCst).is_null() {
        return;
    }
    let Ok(icon) = load_tray_icon() else {
        return;
    };
    unsafe {
        let nid = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON,
            hIcon: icon,
            ..Default::default()
        };
        if !Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() {
            let _ = DestroyIcon(icon);
            return;
        }
        let old = CURRENT_ICON.swap(icon.0, Ordering::SeqCst);
        if !old.is_null() {
            let _ = DestroyIcon(HICON(old));
        }
    }
}

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

        // TrackPopupMenuが正しくメニューを閉じるために必須
        let _ = SetForegroundWindow(hwnd);

        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);

        // メニュー後始末メッセージ
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}
