use windows::core::PCWSTR;
use windows::Win32::System::Registry::*;

use ystrokey_core::AppError;

const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "yStrokey";

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Set or remove auto-start registry entry
pub fn set_autostart(enable: bool) -> Result<(), AppError> {
    unsafe {
        let key_wide = to_wide(RUN_KEY);
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_wide.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hkey,
        );
        if result.is_err() {
            return Err(AppError::Win32(format!("RegOpenKeyExW failed: {:?}", result)));
        }

        let name_wide = to_wide(APP_NAME);

        if enable {
            let exe_path = std::env::current_exe()
                .map_err(|e| AppError::Win32(e.to_string()))?;
            let path_str = exe_path.to_string_lossy().to_string();
            let path_wide = to_wide(&path_str);
            let bytes: &[u8] = std::slice::from_raw_parts(
                path_wide.as_ptr() as *const u8,
                path_wide.len() * 2,
            );
            let result = RegSetValueExW(
                hkey,
                PCWSTR(name_wide.as_ptr()),
                0,
                REG_SZ,
                Some(bytes),
            );
            let _ = RegCloseKey(hkey);
            if result.is_err() {
                return Err(AppError::Win32(format!("RegSetValueExW failed: {:?}", result)));
            }
        } else {
            let result = RegDeleteValueW(hkey, PCWSTR(name_wide.as_ptr()));
            let _ = RegCloseKey(hkey);
            if result.is_err() {
                // Ignore error if value does not exist
                return Ok(());
            }
        }

        Ok(())
    }
}

/// Check if auto-start is currently enabled
pub fn is_autostart_enabled() -> bool {
    unsafe {
        let key_wide = to_wide(RUN_KEY);
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_wide.as_ptr()),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        );
        if result.is_err() {
            return false;
        }

        let name_wide = to_wide(APP_NAME);
        let mut buf_size: u32 = 0;
        let result = RegQueryValueExW(
            hkey,
            PCWSTR(name_wide.as_ptr()),
            None,
            None,
            None,
            Some(&mut buf_size),
        );
        let _ = RegCloseKey(hkey);
        result.is_ok()
    }
}
