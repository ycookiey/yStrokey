use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use ystrokey_core::config::PrivacyConfig;

/// Get the exe name of the foreground window process
pub fn get_foreground_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        let handle = match handle {
            Ok(h) => h,
            Err(_) => return None,
        };
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        path.rsplit("\\").next().map(|s| s.to_string())
    }
}

/// Check if the foreground app is a privacy target
pub fn is_privacy_target(config: &PrivacyConfig) -> bool {
    if !config.enabled || config.blocked_apps.is_empty() {
        return false;
    }
    match get_foreground_process_name() {
        Some(name) => config
            .blocked_apps
            .iter()
            .any(|app| app.eq_ignore_ascii_case(&name)),
        None => false,
    }
}
