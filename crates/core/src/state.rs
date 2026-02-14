use std::time::{Duration, Instant};

use crate::config::{AppConfig, FadeOutCurve, ShortcutDef};
use crate::event::*;
use crate::key::KeyCode;

/// アプリケーション全体の表示状態
pub struct DisplayState {
    /// 現在表示中のOSDアイテム
    items: Vec<DisplayItem>,
    /// 連打検出用
    repeat_tracker: RepeatTracker,
    /// 設定
    config: AppConfig,
    /// 次のアイテムID
    next_id: u64,
}

/// 表示アイテム（OSD上の1つの表示要素）
#[derive(Debug, Clone)]
pub struct DisplayItem {
    pub id: u64,
    pub kind: DisplayItemKind,
    pub created_at: Instant,
    /// 0.0（透明）〜 1.0（不透明）
    pub opacity: f32,
    /// フェーズ
    pub phase: DisplayPhase,
}

#[derive(Debug, Clone)]
pub enum DisplayItemKind {
    /// 通常キー入力
    KeyStroke {
        label: String,
        modifiers: Modifiers,
        action: KeyAction,
        repeat_count: u32,
    },
    /// 連続キー入力グループ（水平並列表示）
    KeyStrokeGroup {
        strokes: Vec<KeyStrokeEntry>,
    },
    /// ショートカットキー（特殊表示）
    Shortcut {
        keys_label: String,
        action_label: String,
    },
    /// IME変換中テキスト
    ImeComposition { text: String },
    /// クリップボード内容
    ClipboardPreview { text: String },
    /// Lock状態変更通知
    LockIndicator {
        caps: bool,
        num: bool,
        scroll: bool,
    },
}

/// グループ内の個別キーストローク
#[derive(Debug, Clone)]
pub struct KeyStrokeEntry {
    pub label: String,
    pub modifiers: Modifiers,
    pub action: KeyAction,
    pub repeat_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPhase {
    /// 表示中（フルopacity）
    Active,
    /// フェードアウト中
    FadingOut,
    /// 期限切れ（削除対象）
    Expired,
}

/// 連打検出
struct RepeatTracker {
    last_key: Option<KeyCode>,
    last_modifiers: Modifiers,
    count: u32,
    last_time: Instant,
    timeout: Duration,
}

impl RepeatTracker {
    fn new(timeout: Duration) -> Self {
        Self {
            last_key: None,
            last_modifiers: Modifiers::default(),
            count: 0,
            last_time: Instant::now(),
            timeout,
        }
    }

    fn track(&mut self, key: KeyCode, modifiers: Modifiers, now: Instant) -> u32 {
        if Some(key) == self.last_key
            && modifiers == self.last_modifiers
            && now.duration_since(self.last_time) < self.timeout
        {
            self.count += 1;
        } else {
            self.count = 1;
        }
        self.last_key = Some(key);
        self.last_modifiers = modifiers;
        self.last_time = now;
        self.count
    }
}

impl DisplayState {
    pub fn new(config: &AppConfig) -> Self {
        let timeout = Duration::from_millis(config.behavior.repeat_timeout_ms);
        Self {
            items: Vec::new(),
            repeat_tracker: RepeatTracker::new(timeout),
            config: config.clone(),
            next_id: 0,
        }
    }

    pub fn process_event(&mut self, event: InputEvent) {
        match event {
            InputEvent::Key(ke) => self.process_key_event(ke),
            InputEvent::Mouse(me) => self.process_mouse_event(me),
            InputEvent::Ime(ie) => self.process_ime_event(ie),
            InputEvent::Clipboard(ce) => self.process_clipboard_event(ce),
            InputEvent::LockState(ls) => self.process_lock_event(ls),
            InputEvent::DpiChanged { .. } | InputEvent::ConfigChanged => {} // main loopで処理
        }
    }

    fn process_key_event(&mut self, ke: KeyEvent) {
        // Key filter: skip ignored keys (case-insensitive, always use full label)
        let full_label = ke.key.label();
        if self.config.behavior.ignored_keys.iter().any(|k| k.eq_ignore_ascii_case(full_label)) {
            return;
        }

        let now = ke.timestamp;

        // 表示ラベル: distinguish_numpad に応じてテンキー区別を制御
        let display_label = if self.config.behavior.distinguish_numpad {
            ke.key.label()
        } else {
            ke.key.label_plain()
        };

        match ke.action {
            KeyAction::Down => {
                // 修飾キー単体は表示しない
                if ke.key.is_modifier() {
                    return;
                }

                // ショートカット判定
                if let Some(shortcut) = self.match_shortcut(&ke) {
                    let keys_label = shortcut.keys.clone();
                    let action_label = shortcut.label.clone();
                    self.add_item(
                        DisplayItemKind::Shortcut {
                            keys_label,
                            action_label,
                        },
                        now,
                    );
                    return;
                }

                // 連打カウント（show_repeat_count が有効な場合のみ追跡）
                if self.config.behavior.show_repeat_count {
                    let count = self.repeat_tracker.track(ke.key, ke.modifiers, now);
                    if count > 1 {
                        self.update_repeat_count(ke.key, count);
                    } else {
                        self.add_keystroke(display_label.to_string(), ke.modifiers, ke.action, now);
                    }
                } else {
                    self.add_keystroke(display_label.to_string(), ke.modifiers, ke.action, now);
                }
            }
            KeyAction::Up => {
                // show_key_down_up: Up イベントも表示（修飾キー単体は除外）
                if self.config.behavior.show_key_down_up && !ke.key.is_modifier() {
                    self.add_keystroke(display_label.to_string(), ke.modifiers, KeyAction::Up, now);
                }
            }
        }
    }

    fn process_mouse_event(&mut self, me: MouseEvent) {
        let label = match me.button {
            MouseButton::Left => "LClick",
            MouseButton::Right => "RClick",
            MouseButton::Middle => "MClick",
            MouseButton::X1 => "X1Click",
            MouseButton::X2 => "X2Click",
        };
        let action_label = match me.action {
            MouseAction::Down => label,
            MouseAction::Up => return,
            MouseAction::Wheel(delta) => {
                if delta > 0 {
                    "WheelUp"
                } else {
                    "WheelDown"
                }
            }
        };
        self.add_item(
            DisplayItemKind::KeyStroke {
                label: action_label.to_string(),
                modifiers: Modifiers::default(),
                action: KeyAction::Down,
                repeat_count: 1,
            },
            me.timestamp,
        );
    }

    fn process_ime_event(&mut self, ie: ImeEvent) {
        if !self.config.behavior.show_ime_composition {
            return;
        }

        match ie.kind {
            ImeEventKind::StateChanged { .. } => {}
            ImeEventKind::CompositionUpdate { text } => {
                // 既存のIMEアイテムを更新、なければ追加
                let updated = self.items.iter_mut().any(|item| {
                    if let DisplayItemKind::ImeComposition { text: ref mut t } = item.kind {
                        *t = text.clone();
                        item.phase = DisplayPhase::Active;
                        item.opacity = 1.0;
                        true
                    } else {
                        false
                    }
                });
                if !updated {
                    self.add_item(DisplayItemKind::ImeComposition { text }, ie.timestamp);
                }
            }
            ImeEventKind::CompositionEnd { .. } => {
                // IMEアイテムを除去
                self.items
                    .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
            }
        }
    }

    fn process_clipboard_event(&mut self, ce: ClipboardEvent) {
        if !self.config.behavior.show_clipboard {
            return;
        }

        let text = match ce.content {
            ClipboardContent::Text(ref s) => {
                let max = self.config.behavior.clipboard_max_chars;
                let char_count = s.chars().count();
                if char_count > max {
                    let truncated: String = s.chars().take(max).collect();
                    format!("{}...", truncated)
                } else {
                    s.clone()
                }
            }
            ClipboardContent::Image { width, height } => {
                format!("[Image {}x{}]", width, height)
            }
            ClipboardContent::Other => "[Clipboard]".to_string(),
        };

        self.add_item(DisplayItemKind::ClipboardPreview { text }, ce.timestamp);
    }

    fn process_lock_event(&mut self, ls: LockStateEvent) {
        if !self.config.behavior.show_lock_indicators {
            return;
        }

        self.add_item(
            DisplayItemKind::LockIndicator {
                caps: ls.caps_lock,
                num: ls.num_lock,
                scroll: ls.scroll_lock,
            },
            ls.timestamp,
        );
    }

    /// 時間経過処理（毎フレーム呼び出し）
    pub fn tick(&mut self, now: Instant) {
        let display_dur =
            Duration::from_millis(self.config.display.display_duration_ms);
        let fade_dur = Duration::from_millis(self.config.display.fade_duration_ms);

        for item in &mut self.items {
            match item.phase {
                DisplayPhase::Active => {
                    if now.duration_since(item.created_at) >= display_dur {
                        item.phase = DisplayPhase::FadingOut;
                    }
                }
                DisplayPhase::FadingOut => {
                    let fade_start = item.created_at + display_dur;
                    let fade_elapsed = now.duration_since(fade_start);
                    let progress =
                        (fade_elapsed.as_secs_f32() / fade_dur.as_secs_f32()).clamp(0.0, 1.0);
                    item.opacity = match self.config.animation.fade_out_curve {
                        FadeOutCurve::Linear => (1.0 - progress).max(0.0),
                        FadeOutCurve::EaseOut => {
                            let inv = 1.0 - progress;
                            (inv * inv).max(0.0)
                        }
                    };
                    if item.opacity <= 0.0 {
                        item.phase = DisplayPhase::Expired;
                    }
                }
                DisplayPhase::Expired => {}
            }
        }

        self.items.retain(|item| item.phase != DisplayPhase::Expired);
    }

    pub fn active_items(&self) -> &[DisplayItem] {
        &self.items
    }

    /// 全アイテムをクリア（privacy遷移時等）
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn has_animations(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.phase == DisplayPhase::FadingOut)
    }

    /// 設定を更新（ホットリロード用）
    pub fn update_config(&mut self, config: &AppConfig) {
        self.config = config.clone();
        self.repeat_tracker.timeout = Duration::from_millis(config.behavior.repeat_timeout_ms);
    }

    // --- private helpers ---

    fn add_item(&mut self, kind: DisplayItemKind, now: Instant) {
        let item = DisplayItem {
            id: self.next_id,
            kind,
            created_at: now,
            opacity: 1.0,
            phase: DisplayPhase::Active,
        };

        self.next_id += 1;
        self.items.push(item);

        while self.items.len() > self.config.display.max_items {
            self.items.remove(0);
        }
    }

    fn add_keystroke(
        &mut self,
        label: String,
        modifiers: Modifiers,
        action: KeyAction,
        now: Instant,
    ) {
        let group_timeout_ms = self.config.behavior.group_timeout_ms;
        if group_timeout_ms == 0 {
            self.add_item(
                DisplayItemKind::KeyStroke {
                    label,
                    modifiers,
                    action,
                    repeat_count: 1,
                },
                now,
            );
            return;
        }

        let group_timeout = Duration::from_millis(group_timeout_ms);
        let max_group = self.config.behavior.max_group_size;
        let new_entry = KeyStrokeEntry {
            label: label.clone(),
            modifiers,
            action,
            repeat_count: 1,
        };

        // 最終アイテムがActiveかつタイムアウト内ならグループ化
        if let Some(last_item) = self.items.last_mut() {
            if last_item.phase == DisplayPhase::Active
                && now.duration_since(last_item.created_at) < group_timeout
            {
                match &mut last_item.kind {
                    DisplayItemKind::KeyStroke {
                        label: ref l,
                        modifiers: ref m,
                        action: ref a,
                        repeat_count: ref rc,
                    } => {
                        // KeyStroke → KeyStrokeGroup に昇格
                        let first = KeyStrokeEntry {
                            label: l.clone(),
                            modifiers: *m,
                            action: *a,
                            repeat_count: *rc,
                        };
                        last_item.kind = DisplayItemKind::KeyStrokeGroup {
                            strokes: vec![first, new_entry],
                        };
                        last_item.created_at = now;
                        return;
                    }
                    DisplayItemKind::KeyStrokeGroup { strokes } => {
                        if strokes.len() < max_group {
                            strokes.push(new_entry);
                            last_item.created_at = now;
                            return;
                        }
                        // max_group_size に達したら新行へ
                    }
                    _ => {}
                }
            }
        }

        // グループ化できない場合は通常の新行
        self.add_item(
            DisplayItemKind::KeyStroke {
                label,
                modifiers,
                action,
                repeat_count: 1,
            },
            now,
        );
    }

    fn match_shortcut(&self, ke: &KeyEvent) -> Option<&ShortcutDef> {
        if ke.action != KeyAction::Down || !ke.modifiers.any() {
            return None;
        }

        self.config.shortcuts.iter().find(|s| {
            shortcut_matches(&s.keys, ke)
        })
    }

    fn update_repeat_count(&mut self, _key: KeyCode, count: u32) {
        // 最新のKeyStroke/KeyStrokeGroupアイテムの連打カウントを更新
        for item in self.items.iter_mut().rev() {
            let matched = match &mut item.kind {
                DisplayItemKind::KeyStroke {
                    repeat_count: ref mut rc,
                    ..
                } => {
                    *rc = count;
                    true
                }
                DisplayItemKind::KeyStrokeGroup { strokes } => {
                    if let Some(last) = strokes.last_mut() {
                        last.repeat_count = count;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if matched {
                let now = Instant::now();
                item.created_at = now;
                item.opacity = 1.0;
                item.phase = DisplayPhase::Active;
                break;
            }
        }
    }
}

/// ショートカット定義文字列がキーイベントにマッチするか判定
fn shortcut_matches(keys_str: &str, ke: &KeyEvent) -> bool {
    let parts: Vec<&str> = keys_str.split('+').collect();
    if parts.is_empty() {
        return false;
    }

    let mut need_ctrl = false;
    let mut need_shift = false;
    let mut need_alt = false;
    let mut need_win = false;
    let mut key_part = None;

    for part in &parts {
        match *part {
            "Ctrl" => need_ctrl = true,
            "Shift" => need_shift = true,
            "Alt" => need_alt = true,
            "Win" => need_win = true,
            other => key_part = Some(other),
        }
    }

    if ke.modifiers.ctrl != need_ctrl
        || ke.modifiers.shift != need_shift
        || ke.modifiers.alt != need_alt
        || ke.modifiers.win != need_win
    {
        return false;
    }

    let Some(expected_key) = key_part else {
        return false;
    };

    ke.key.label() == expected_key
}
