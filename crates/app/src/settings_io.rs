use std::path::PathBuf;

use windows::core::HSTRING;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;

use ystrokey_core::AppConfig;

fn win32_err(e: impl std::fmt::Display) -> ystrokey_core::AppError {
    ystrokey_core::AppError::Win32(e.to_string())
}

struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    fn new() -> Self {
        let initialized = unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok()
        };
        Self { initialized }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() }
        }
    }
}

fn setup_json_filter(dialog: &IFileDialog) -> Result<(), ystrokey_core::AppError> {
    let filter_name = HSTRING::from("JSON files (*.json)");
    let filter_pattern = HSTRING::from("*.json");
    let filters = [COMDLG_FILTERSPEC {
        pszName: windows::core::PCWSTR(filter_name.as_ptr()),
        pszSpec: windows::core::PCWSTR(filter_pattern.as_ptr()),
    }];
    unsafe { dialog.SetFileTypes(&filters).map_err(win32_err) }
}

unsafe fn get_path_from_dialog(dialog: &IFileDialog) -> Result<Option<PathBuf>, ystrokey_core::AppError> {
    if dialog.Show(None).is_err() {
        return Ok(None); // user cancelled
    }
    let result = dialog.GetResult().map_err(win32_err)?;
    let path_raw = result.GetDisplayName(SIGDN_FILESYSPATH).map_err(win32_err)?;
    let path_str = path_raw.to_string().map_err(win32_err)?;
    Ok(Some(PathBuf::from(path_str)))
}

/// ファイル保存ダイアログで設定をエクスポート
pub fn export_config(config: &AppConfig) -> Result<(), ystrokey_core::AppError> {
    unsafe {
        let _com = ComGuard::new();

        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).map_err(win32_err)?;

        // IFileSaveDialog は IFileDialog を Deref で継承
        setup_json_filter(&dialog)?;
        let _ = dialog.SetDefaultExtension(&HSTRING::from("json"));
        let _ = dialog.SetFileName(&HSTRING::from("ystrokey_config.json"));

        if let Some(path) = get_path_from_dialog(&dialog)? {
            config.save(&path)?;
        }
        Ok(())
    }
}

/// ファイル選択ダイアログで設定をインポート
pub fn import_config() -> Result<Option<AppConfig>, ystrokey_core::AppError> {
    unsafe {
        let _com = ComGuard::new();

        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(win32_err)?;

        setup_json_filter(&dialog)?;

        match get_path_from_dialog(&dialog)? {
            Some(path) => {
                let config = AppConfig::load(&path)?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }
}
