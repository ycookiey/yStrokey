use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// アプリケーション全体の設定
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(skip)]
    pub last_modified: Option<std::time::SystemTime>,
    pub display: DisplayConfig,
    pub style: StyleConfig,
    pub behavior: BehaviorConfig,
    pub shortcuts: Vec<ShortcutDef>,
    pub privacy: PrivacyConfig,
    pub hotkey: HotkeyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// 表示位置
    pub position: Position,
    /// 基準位置からのオフセット (px)
    pub offset_x: i32,
    pub offset_y: i32,
    /// モニタ別ウィンドウ位置 (デバイス名 → [x, y])
    #[serde(default)]
    pub monitor_positions: HashMap<String, [i32; 2]>,
    /// 最大同時表示アイテム数
    pub max_items: usize,
    /// 表示時間 (ms)
    pub display_duration_ms: u64,
    /// フェードアウト時間 (ms)
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
#[serde(default)]
pub struct StyleConfig {
    pub font_family: String,
    pub font_size: f32,
    /// "#RRGGBB" or "#RRGGBBAA"
    pub text_color: String,
    pub background_color: String,
    pub border_radius: f32,
    pub padding: f32,
    /// ショートカットキーのハイライト色
    pub shortcut_color: String,
    /// キー押下中の色
    pub key_down_color: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// down/up表示を有効にする
    pub show_key_down_up: bool,
    /// 連打カウント表示
    pub show_repeat_count: bool,
    /// テンキー区別
    pub distinguish_numpad: bool,
    /// IME変換前文字列表示
    pub show_ime_composition: bool,
    /// クリップボード表示
    pub show_clipboard: bool,
    /// クリップボードの最大表示文字数
    pub clipboard_max_chars: usize,
    /// Lock状態インジケータ表示
    pub show_lock_indicators: bool,
    /// 連打判定タイムアウト (ms)
    pub repeat_timeout_ms: u64,
    /// 連続入力グルーピング閾値 (ms)。0で無効。
    pub group_timeout_ms: u64,
    /// 1グループの最大キー数
    pub max_group_size: usize,
    pub ignored_keys: Vec<String>,
    /// 画面キャプチャからOSDを除外 (Win10 v2004+)
    pub exclude_from_capture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutDef {
    /// "Ctrl+C" 形式
    pub keys: String,
    /// 表示ラベル（例: "Copy"）
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    pub enabled: bool,
    /// OSD無効化対象のexe名
    pub blocked_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// OSD切替ホットキー（例: "Ctrl+Alt+F12"）
    pub toggle: String,
}

// --- Default impls ---

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_modified: None,
            display: DisplayConfig::default(),
            style: StyleConfig::default(),
            behavior: BehaviorConfig::default(),
            shortcuts: default_shortcuts(),
            privacy: PrivacyConfig::default(),
            hotkey: HotkeyConfig::default(),
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
            show_key_down_up: true,
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
    /// 指定パスから設定を読み込み（ファイルが存在しない/不正なら エラー）。
    pub fn load(config_path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(config_path)?;
        let mut config: Self = serde_json::from_str(&content)?;
        config.last_modified = std::fs::metadata(config_path)?.modified().ok();
        Ok(config)
    }

    /// 指定パスから設定を読み込み。存在しなければデフォルト作成。
    pub fn load_or_create(config_path: &Path) -> Result<Self, ConfigError> {
        if config_path.exists() {
            Self::load(config_path)
        } else {
            let mut config = Self::default();
            let json = serde_json::to_string_pretty(&config)?;
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(config_path, &json)?;
            config.last_modified = std::fs::metadata(config_path)?.modified().ok();
            Ok(config)
        }
    }

    /// ファイルの更新日時を確認し、変更があれば再読み込み
    pub fn check_reload(&self, path: &Path) -> Option<AppConfig> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let should_reload = match self.last_modified {
            Some(last) => modified > last,
            None => true,
        };
        if should_reload {
            let content = std::fs::read_to_string(path).ok()?;
            let mut config: AppConfig = serde_json::from_str(&content).ok()?;
            config.last_modified = Some(modified);
            return Some(config);
        }
        None
    }

    /// 設定をファイルに保存
    pub fn save(&self, config_path: &Path) -> Result<(), ConfigError> {
        let json = serde_json::to_string_pretty(self)?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(config_path, json)?;
        Ok(())
    }
}
