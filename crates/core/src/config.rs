use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

pub const SCHEMA_VERSION: u32 = 2;

/// Strict configuration schema for the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(skip)]
    pub last_modified: Option<SystemTime>,
    pub display: DisplayConfig,
    pub style: StyleConfig,
    pub behavior: BehaviorConfig,
    pub shortcuts: Vec<ShortcutDef>,
    pub privacy: PrivacyConfig,
    pub hotkey: HotkeyConfig,
    pub performance: PerformanceConfig,
    pub diagnostics: DiagnosticsConfig,
    pub startup: StartupConfig,
    pub tray: TrayConfig,
    pub animation: AnimationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayConfig {
    pub position: Position,
    pub offset_x: i32,
    pub offset_y: i32,
    pub monitor_positions: HashMap<String, [i32; 2]>,
    pub max_items: usize,
    pub display_duration_ms: u64,
    pub fade_duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleConfig {
    pub font_family: String,
    pub font_size: f32,
    /// "#RRGGBB" or "#RRGGBBAA"
    pub text_color: String,
    pub background_color: String,
    pub border_radius: f32,
    pub padding: f32,
    pub shortcut_color: String,
    pub key_down_color: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorConfig {
    pub key_transition_mode: KeyTransitionMode,
    pub show_repeat_count: bool,
    pub distinguish_numpad: bool,
    pub show_ime_composition: bool,
    pub show_clipboard: bool,
    pub clipboard_max_chars: usize,
    pub show_lock_indicators: bool,
    pub repeat_timeout_ms: u64,
    pub group_timeout_ms: u64,
    pub max_group_size: usize,
    pub ignored_keys: Vec<String>,
    pub exclude_from_capture: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeyTransitionMode {
    SingleCell,
    SplitCells,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutDef {
    pub keys: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyConfig {
    pub enabled: bool,
    pub blocked_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotkeyConfig {
    pub toggle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceConfig {
    pub osd_width: i32,
    pub osd_height: i32,
    pub ime_poll_interval_ms: u64,
    pub frame_interval_ms: u64,
    pub config_reload_interval_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticsLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsConfig {
    pub level: DiagnosticsLevel,
    pub file_logging_enabled: bool,
    pub max_file_bytes: u64,
    pub max_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupConfig {
    pub autostart_enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MenuLanguage {
    Ja,
    En,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayConfig {
    pub start_osd_enabled: bool,
    pub menu_language: MenuLanguage,
    pub confirm_on_exit: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GhostModifier {
    Ctrl,
    Alt,
    Shift,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FadeOutCurve {
    Linear,
    EaseOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationConfig {
    pub ghost_modifier: GhostModifier,
    pub ghost_threshold_px: f32,
    pub ghost_max_opacity: f32,
    pub fade_out_curve: FadeOutCurve,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            last_modified: None,
            display: DisplayConfig::default(),
            style: StyleConfig::default(),
            behavior: BehaviorConfig::default(),
            shortcuts: default_shortcuts(),
            privacy: PrivacyConfig::default(),
            hotkey: HotkeyConfig::default(),
            performance: PerformanceConfig::default(),
            diagnostics: DiagnosticsConfig::default(),
            startup: StartupConfig::default(),
            tray: TrayConfig::default(),
            animation: AnimationConfig::default(),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            position: Position::BottomCenter,
            offset_x: 0,
            offset_y: -48,
            monitor_positions: HashMap::new(),
            max_items: 5,
            display_duration_ms: 2000,
            fade_duration_ms: 300,
        }
    }
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            font_family: "Yu Gothic UI".into(),
            font_size: 24.0,
            text_color: "#FFFFFF".into(),
            background_color: "#000000CC".into(),
            border_radius: 8.0,
            padding: 12.0,
            shortcut_color: "#4CAF50".into(),
            key_down_color: "#2196F3".into(),
            opacity: 0.95,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            key_transition_mode: KeyTransitionMode::SingleCell,
            show_repeat_count: true,
            distinguish_numpad: true,
            show_ime_composition: true,
            show_clipboard: true,
            clipboard_max_chars: 50,
            show_lock_indicators: true,
            repeat_timeout_ms: 500,
            group_timeout_ms: 300,
            max_group_size: 10,
            ignored_keys: Vec::new(),
            exclude_from_capture: false,
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            blocked_apps: vec!["KeePass.exe".into(), "1Password.exe".into()],
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+F12".into(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            osd_width: 600,
            osd_height: 300,
            ime_poll_interval_ms: 50,
            frame_interval_ms: 16,
            config_reload_interval_ms: 1000,
        }
    }
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            level: DiagnosticsLevel::Info,
            file_logging_enabled: true,
            max_file_bytes: 1024 * 1024,
            max_files: 3,
        }
    }
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            autostart_enabled: false,
        }
    }
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            start_osd_enabled: true,
            menu_language: MenuLanguage::Ja,
            confirm_on_exit: true,
        }
    }
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            ghost_modifier: GhostModifier::Ctrl,
            ghost_threshold_px: 100.0,
            ghost_max_opacity: 1.0,
            fade_out_curve: FadeOutCurve::Linear,
        }
    }
}

fn default_shortcuts() -> Vec<ShortcutDef> {
    vec![
        ShortcutDef { keys: "Ctrl+C".into(), label: "Copy".into() },
        ShortcutDef { keys: "Ctrl+V".into(), label: "Paste".into() },
        ShortcutDef { keys: "Ctrl+X".into(), label: "Cut".into() },
        ShortcutDef { keys: "Ctrl+Z".into(), label: "Undo".into() },
        ShortcutDef { keys: "Ctrl+Y".into(), label: "Redo".into() },
        ShortcutDef { keys: "Ctrl+S".into(), label: "Save".into() },
        ShortcutDef { keys: "Ctrl+A".into(), label: "SelectAll".into() },
        ShortcutDef { keys: "Ctrl+F".into(), label: "Find".into() },
        ShortcutDef { keys: "Alt+Tab".into(), label: "Switch".into() },
        ShortcutDef { keys: "Alt+F4".into(), label: "Close".into() },
        ShortcutDef { keys: "Win+D".into(), label: "Desktop".into() },
        ShortcutDef { keys: "Win+L".into(), label: "Lock".into() },
        ShortcutDef { keys: "Win+E".into(), label: "Explorer".into() },
        ShortcutDef { keys: "Win+Tab".into(), label: "TaskView".into() },
        ShortcutDef { keys: "Ctrl+Shift+Esc".into(), label: "TaskMgr".into() },
        ShortcutDef { keys: "Ctrl+N".into(), label: "New".into() },
        ShortcutDef { keys: "Ctrl+W".into(), label: "CloseTab".into() },
        ShortcutDef { keys: "Ctrl+T".into(), label: "NewTab".into() },
    ]
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::ValidationError(format!(
                "schema_version must be {}",
                SCHEMA_VERSION
            )));
        }

        if self.display.max_items == 0 {
            return Err(ConfigError::ValidationError("display.max_items must be > 0".into()));
        }
        if self.display.display_duration_ms == 0 {
            return Err(ConfigError::ValidationError(
                "display.display_duration_ms must be > 0".into(),
            ));
        }
        if self.display.fade_duration_ms == 0 {
            return Err(ConfigError::ValidationError(
                "display.fade_duration_ms must be > 0".into(),
            ));
        }

        if self.style.font_size <= 0.0 {
            return Err(ConfigError::ValidationError("style.font_size must be > 0".into()));
        }
        if !(0.0..=1.0).contains(&self.style.opacity) {
            return Err(ConfigError::ValidationError("style.opacity must be within 0..=1".into()));
        }

        if self.behavior.clipboard_max_chars == 0 {
            return Err(ConfigError::ValidationError(
                "behavior.clipboard_max_chars must be > 0".into(),
            ));
        }
        if self.behavior.repeat_timeout_ms == 0 {
            return Err(ConfigError::ValidationError(
                "behavior.repeat_timeout_ms must be > 0".into(),
            ));
        }
        if self.behavior.max_group_size == 0 {
            return Err(ConfigError::ValidationError(
                "behavior.max_group_size must be > 0".into(),
            ));
        }

        if self.performance.osd_width <= 0 || self.performance.osd_height <= 0 {
            return Err(ConfigError::ValidationError(
                "performance.osd_width/osd_height must be > 0".into(),
            ));
        }
        if self.performance.ime_poll_interval_ms == 0 {
            return Err(ConfigError::ValidationError(
                "performance.ime_poll_interval_ms must be > 0".into(),
            ));
        }
        if self.performance.frame_interval_ms == 0 {
            return Err(ConfigError::ValidationError(
                "performance.frame_interval_ms must be > 0".into(),
            ));
        }
        if self.performance.config_reload_interval_ms == 0 {
            return Err(ConfigError::ValidationError(
                "performance.config_reload_interval_ms must be > 0".into(),
            ));
        }

        if self.diagnostics.max_file_bytes < 1024 {
            return Err(ConfigError::ValidationError(
                "diagnostics.max_file_bytes must be >= 1024".into(),
            ));
        }
        if self.diagnostics.max_files == 0 {
            return Err(ConfigError::ValidationError(
                "diagnostics.max_files must be > 0".into(),
            ));
        }

        if self.animation.ghost_threshold_px <= 0.0 {
            return Err(ConfigError::ValidationError(
                "animation.ghost_threshold_px must be > 0".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.animation.ghost_max_opacity) {
            return Err(ConfigError::ValidationError(
                "animation.ghost_max_opacity must be within 0..=1".into(),
            ));
        }

        Ok(())
    }

    pub fn load_strict(config_path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(config_path)?;
        let mut config: Self = serde_json::from_str(&content)?;
        config.validate()?;
        config.last_modified = std::fs::metadata(config_path)?.modified().ok();
        Ok(config)
    }

    pub fn create_default(config_path: &Path) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        config.save_atomic(config_path)?;
        config.last_modified = std::fs::metadata(config_path)?.modified().ok();
        Ok(config)
    }

    pub fn check_reload(&self, path: &Path) -> Result<Option<AppConfig>, ConfigError> {
        let modified = std::fs::metadata(path)?.modified()?;
        let should_reload = match self.last_modified {
            Some(last) => modified > last,
            None => true,
        };
        if should_reload {
            let mut config = Self::load_strict(path)?;
            config.last_modified = Some(modified);
            return Ok(Some(config));
        }
        Ok(None)
    }

    pub fn save_atomic(&self, config_path: &Path) -> Result<(), ConfigError> {
        self.validate()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;

        let tmp_path = temp_path_for(config_path);
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
        }

        if config_path.exists() {
            if let Err(replace_err) = replace_file(config_path, &tmp_path) {
                // Fallback for environments where ReplaceFileW returns ACCESS_DENIED.
                let direct_write = (|| -> Result<(), std::io::Error> {
                    let mut file = File::create(config_path)?;
                    file.write_all(json.as_bytes())?;
                    file.sync_all()?;
                    Ok(())
                })();

                let _ = std::fs::remove_file(&tmp_path);

                if let Err(write_err) = direct_write {
                    return Err(ConfigError::IoError(std::io::Error::new(
                        write_err.kind(),
                        format!(
                            "replace failed: {replace_err}; fallback write failed: {write_err}"
                        ),
                    )));
                }
            }
        } else {
            std::fs::rename(&tmp_path, config_path)?;
        }

        Ok(())
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!("{}.{}.tmp", file_name, stamp))
}

#[cfg(windows)]
fn replace_file(target: &Path, replacement: &Path) -> Result<(), ConfigError> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_IGNORE_MERGE_ERRORS};

    unsafe {
        let target_w = HSTRING::from(target.to_string_lossy().to_string());
        let replacement_w = HSTRING::from(replacement.to_string_lossy().to_string());
        ReplaceFileW(
            &target_w,
            &replacement_w,
            PCWSTR::null(),
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            None,
            None,
        )
        .map_err(|e| {
            ConfigError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("ReplaceFileW failed: {}", e),
            ))
        })
    }
}

#[cfg(not(windows))]
fn replace_file(target: &Path, replacement: &Path) -> Result<(), ConfigError> {
    if target.exists() {
        std::fs::remove_file(target)?;
    }
    std::fs::rename(replacement, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("ystrokey-{}-{}.json", name, stamp))
    }

    #[test]
    fn strict_rejects_unknown_keys() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.insert("unknown".to_string(), serde_json::json!(123));

        let parsed = serde_json::from_value::<AppConfig>(value);
        assert!(parsed.is_err());
    }

    #[test]
    fn strict_rejects_missing_required_keys() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        let obj = value.as_object_mut().unwrap();
        obj.remove("display");

        let parsed = serde_json::from_value::<AppConfig>(value);
        assert!(parsed.is_err());
    }

    #[test]
    fn save_atomic_and_load_strict_roundtrip() {
        let path = temp_config_path("roundtrip");
        let mut cfg = AppConfig::default();
        cfg.style.font_size = 30.0;
        cfg.performance.frame_interval_ms = 20;
        cfg.save_atomic(&path).unwrap();

        let loaded = AppConfig::load_strict(&path).unwrap();
        assert_eq!(loaded.style.font_size, 30.0);
        assert_eq!(loaded.performance.frame_interval_ms, 20);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn default_uses_single_cell_key_transition_mode() {
        let cfg = AppConfig::default();
        assert_eq!(
            cfg.behavior.key_transition_mode,
            KeyTransitionMode::SingleCell
        );
    }
}
