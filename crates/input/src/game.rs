//! 前面のアプリがゲームかを、WindowsがXbox Game Barのために記録しているゲームの一覧で判定する。
//!
//! 一覧は`HKCU\System\GameConfigStore\Children`にある。実行ファイルのパス（`MatchedExeFullPath`）を
//! 持つ項目だけを使う。パスの無い項目は、前面のプロセスと照合できない。
//! `HKCU\System\GameConfigStore\Children`は文書化されていないので、読めないときはどのアプリもゲームとみなさず、OSDを止めない。

use std::collections::HashSet;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetFinalPathNameByHandleW, FILE_FLAGS_AND_ATTRIBUTES, FILE_NAME_NORMALIZED,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY,
    HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
    RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const CHILDREN: PCWSTR = w!("System\\GameConfigStore\\Children");

/// ゲームの実行ファイルの実体のパスを小文字にした文字列
pub struct GameList {
    paths: HashSet<String>,
    notify_key: Option<HKEY>,
}

impl GameList {
    pub fn load() -> Self {
        let notify_key = unsafe {
            let mut key = HKEY::default();
            RegOpenKeyExW(HKEY_CURRENT_USER, CHILDREN, 0, KEY_NOTIFY, &mut key)
                .ok()
                .ok()
                .map(|_| key)
        };
        Self {
            paths: read_paths(),
            notify_key,
        }
    }

    pub fn reload(&mut self) {
        self.paths = read_paths();
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// 一覧が変わったら`event`をシグナル状態にするよう登録する。通知は1回きりなので、通知のたびに登録し直す。
    /// 登録できなければfalseを返す。登録できなかったときは、一覧を読み込んだときのまま使う。
    pub fn watch(&self, event: HANDLE) -> bool {
        let Some(key) = self.notify_key else {
            return false;
        };
        unsafe {
            RegNotifyChangeKeyValue(
                key,
                true,
                REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                event,
                true,
            )
            .is_ok()
        }
    }

    pub fn foreground_is_game(&self) -> bool {
        if self.paths.is_empty() {
            return false;
        }
        match foreground_process_path() {
            Some(path) => self.paths.contains(&normalize(&path)),
            None => false,
        }
    }
}

impl Drop for GameList {
    fn drop(&mut self) {
        if let Some(key) = self.notify_key.take() {
            unsafe {
                let _ = RegCloseKey(key);
            }
        }
    }
}

fn read_paths() -> HashSet<String> {
    let mut out = HashSet::new();
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, CHILDREN, 0, KEY_READ, &mut root).is_err() {
            return out;
        }
        let mut index = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            let result = RegEnumKeyExW(
                root,
                index,
                PWSTR(name.as_mut_ptr()),
                &mut len,
                None,
                PWSTR::null(),
                None,
                None,
            );
            if result.is_err() {
                break;
            }
            index += 1;
            if let Some(path) = read_string(root, &name[..len as usize], "MatchedExeFullPath") {
                if !path.is_empty() {
                    out.insert(normalize(&path));
                }
            }
        }
        let _ = RegCloseKey(root);
    }
    out
}

fn read_string(root: HKEY, subkey: &[u16], value: &str) -> Option<String> {
    let mut subkey: Vec<u16> = subkey.to_vec();
    subkey.push(0);
    let value: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let chars = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
}

/// 照合に使う形にする。junctionとシンボリックリンクをたどった実体のパスを小文字にする。
/// ファイルを開けなければ、渡されたパスを小文字にする。
/// ゲームの一覧と実行中のプロセスが、同じファイルをjunctionを挟んだ別のパスで示すことがある。
fn normalize(path: &str) -> String {
    final_path(path).unwrap_or_else(|| path.to_string()).to_lowercase()
}

fn final_path(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
        .ok()?;
        let mut buf = [0u16; 1024];
        let len = GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIZED) as usize;
        let _ = CloseHandle(handle);
        if len == 0 || len >= buf.len() {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..len]);
        Some(s.strip_prefix("\\\\?\\").unwrap_or(&s).to_string())
    }
}

fn foreground_process_path() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
        let _ = CloseHandle(process);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_paths_that_cannot_be_opened() {
        assert_eq!(normalize("C:\\No\\Such\\Game.EXE"), "c:\\no\\such\\game.exe");
    }

    #[test]
    fn normalize_resolves_an_existing_file_to_the_same_form_twice() {
        let exe = std::env::current_exe().unwrap();
        let a = normalize(exe.to_str().unwrap());
        let b = normalize(&exe.to_str().unwrap().to_uppercase());
        assert_eq!(a, b);
    }
}
