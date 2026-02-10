use std::time::Instant;

use crate::key::KeyCode;

/// 全入力イベントの統合型
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Ime(ImeEvent),
    Clipboard(ClipboardEvent),
    LockState(LockStateEvent),
    /// DPI変更通知 (モニタ移動等)
    DpiChanged {
        dpi: u32,
        /// suggested rect [left, top, right, bottom]
        suggested_rect: [i32; 4],
    },
    /// 設定がインポート等で外部から変更された通知
    ConfigChanged,
}

/// キーイベント
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// キーコード（VK_*に対応、テンキー区別済み）
    pub key: KeyCode,
    /// 押下 or 離上
    pub action: KeyAction,
    /// 同時押し修飾キー
    pub modifiers: Modifiers,
    /// テンキー由来か
    pub is_numpad: bool,
    /// Win32スキャンコード
    pub scan_code: u32,
    /// イベント発生時刻
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Down,
    Up,
}

/// 修飾キー状態
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Modifiers {
    pub fn any(&self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }
}

/// マウスイベント
#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub action: MouseAction,
    pub position: (i32, i32),
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Down,
    Up,
    Wheel(i16),
}

/// IMEイベント
#[derive(Debug, Clone)]
pub struct ImeEvent {
    pub kind: ImeEventKind,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub enum ImeEventKind {
    /// IME ON/OFF切替
    StateChanged { enabled: bool },
    /// 変換前文字列（ひらがな）の更新
    CompositionUpdate { text: String },
    /// 変換確定
    CompositionEnd { result: String },
}

/// クリップボードイベント
#[derive(Debug, Clone)]
pub struct ClipboardEvent {
    pub content: ClipboardContent,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image { width: u32, height: u32 },
    Other,
}

/// Lock状態イベント
#[derive(Debug, Clone)]
pub struct LockStateEvent {
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
    pub timestamp: Instant,
}
