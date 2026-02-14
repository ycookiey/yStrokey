use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::config::{AppConfig, FadeOutCurve, KeyTransitionMode, ShortcutDef};
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
    /// 単一セルモードでのDown/Up対応付け
    active_presses: HashMap<PressKey, PressTarget>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PressKey {
    scan_code: u32,
    is_numpad: bool,
}

impl PressKey {
    fn from_key_event(ke: &KeyEvent) -> Self {
        Self {
            scan_code: ke.scan_code,
            is_numpad: ke.is_numpad,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PressTarget {
    item_id: u64,
    group_index: Option<usize>,
}

impl PressTarget {
    fn item(item_id: u64) -> Self {
        Self {
            item_id,
            group_index: None,
        }
    }

    fn group(item_id: u64, group_index: usize) -> Self {
        Self {
            item_id,
            group_index: Some(group_index),
        }
    }
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
            active_presses: HashMap::new(),
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
                    let _ = self.add_item(
                        DisplayItemKind::Shortcut {
                            keys_label,
                            action_label,
                        },
                        now,
                    );
                    self.active_presses.remove(&PressKey::from_key_event(&ke));
                    return;
                }

                let target = if self.config.behavior.show_repeat_count {
                    let count = self.repeat_tracker.track(ke.key, ke.modifiers, now);
                    if count > 1 {
                        let force_down =
                            self.config.behavior.key_transition_mode == KeyTransitionMode::SingleCell;
                        self.update_repeat_count(count, now, force_down)
                            .unwrap_or_else(|| {
                                self.add_keystroke(
                                    display_label.to_string(),
                                    ke.modifiers,
                                    KeyAction::Down,
                                    now,
                                )
                            })
                    } else {
                        self.add_keystroke(
                            display_label.to_string(),
                            ke.modifiers,
                            KeyAction::Down,
                            now,
                        )
                    }
                } else {
                    self.add_keystroke(
                        display_label.to_string(),
                        ke.modifiers,
                        KeyAction::Down,
                        now,
                    )
                };

                // 連打カウント（show_repeat_count が有効な場合のみ追跡）
                if self.config.behavior.key_transition_mode == KeyTransitionMode::SingleCell {
                    self.active_presses
                        .insert(PressKey::from_key_event(&ke), target);
                }
            }
            KeyAction::Up => {
                if ke.key.is_modifier() {
                    return;
                }

                match self.config.behavior.key_transition_mode {
                    KeyTransitionMode::SingleCell => {
                        self.apply_key_up_to_existing(&ke, now);
                    }
                    KeyTransitionMode::SplitCells => {
                        self.active_presses.remove(&PressKey::from_key_event(&ke));
                        let _ = self.add_keystroke(
                            display_label.to_string(),
                            ke.modifiers,
                            KeyAction::Up,
                            now,
                        );
                    }
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
        let _ = self.add_item(
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
                    let _ = self.add_item(DisplayItemKind::ImeComposition { text }, ie.timestamp);
                }
            }
            ImeEventKind::CompositionEnd { .. } => {
                // IMEアイテムを除去
                self.items
                    .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
                self.prune_active_press_targets();
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

        let _ = self.add_item(DisplayItemKind::ClipboardPreview { text }, ce.timestamp);
    }

    fn process_lock_event(&mut self, ls: LockStateEvent) {
        if !self.config.behavior.show_lock_indicators {
            return;
        }

        let _ = self.add_item(
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
        self.prune_active_press_targets();
    }

    pub fn active_items(&self) -> &[DisplayItem] {
        &self.items
    }

    /// 全アイテムをクリア（privacy遷移時等）
    pub fn clear(&mut self) {
        self.items.clear();
        self.active_presses.clear();
    }

    pub fn has_animations(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.phase == DisplayPhase::FadingOut)
    }

    /// 設定を更新（ホットリロード用）
    pub fn update_config(&mut self, config: &AppConfig) {
        if self.config.behavior.key_transition_mode != config.behavior.key_transition_mode {
            self.active_presses.clear();
        }
        self.config = config.clone();
        self.repeat_tracker.timeout = Duration::from_millis(config.behavior.repeat_timeout_ms);
        self.prune_active_press_targets();
    }

    // --- private helpers ---

    fn add_item(&mut self, kind: DisplayItemKind, now: Instant) -> u64 {
        let item_id = self.next_id;
        let item = DisplayItem {
            id: item_id,
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

        self.prune_active_press_targets();
        item_id
    }

    fn add_keystroke(
        &mut self,
        label: String,
        modifiers: Modifiers,
        action: KeyAction,
        now: Instant,
    ) -> PressTarget {
        let group_timeout_ms = self.config.behavior.group_timeout_ms;
        if group_timeout_ms == 0 {
            let item_id = self.add_item(
                DisplayItemKind::KeyStroke {
                    label,
                    modifiers,
                    action,
                    repeat_count: 1,
                },
                now,
            );
            return PressTarget::item(item_id);
        }

        let group_timeout = Duration::from_millis(group_timeout_ms);
        let max_group = self.config.behavior.max_group_size;
        let new_entry = KeyStrokeEntry {
            label: label.clone(),
            modifiers,
            action,
            repeat_count: 1,
        };
        let mut grouped_target = None;
        let mut remap_item_id = None;

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
                        let item_id = last_item.id;
                        remap_item_id = Some(item_id);
                        grouped_target = Some(PressTarget::group(item_id, 1));
                    }
                    DisplayItemKind::KeyStrokeGroup { strokes } => {
                        if strokes.len() < max_group {
                            strokes.push(new_entry);
                            last_item.created_at = now;
                            grouped_target =
                                Some(PressTarget::group(last_item.id, strokes.len() - 1));
                        }
                        // max_group_size に達したら新行へ
                    }
                    _ => {}
                }
            }
        }

        if let Some(item_id) = remap_item_id {
            self.remap_item_target_to_group_first(item_id);
        }
        if let Some(target) = grouped_target {
            return target;
        }

        // グループ化できない場合は通常の新行
        let item_id = self.add_item(
            DisplayItemKind::KeyStroke {
                label,
                modifiers,
                action,
                repeat_count: 1,
            },
            now,
        );
        PressTarget::item(item_id)
    }

    fn match_shortcut(&self, ke: &KeyEvent) -> Option<&ShortcutDef> {
        if ke.action != KeyAction::Down || !ke.modifiers.any() {
            return None;
        }

        self.config.shortcuts.iter().find(|s| {
            shortcut_matches(&s.keys, ke)
        })
    }

    fn update_repeat_count(
        &mut self,
        count: u32,
        now: Instant,
        force_down_state: bool,
    ) -> Option<PressTarget> {
        // 最新のKeyStroke/KeyStrokeGroupアイテムの連打カウントを更新
        for item in self.items.iter_mut().rev() {
            let matched = match &mut item.kind {
                DisplayItemKind::KeyStroke {
                    action: ref mut a,
                    repeat_count: ref mut rc,
                    ..
                } => {
                    if *a == KeyAction::Down || force_down_state {
                        if force_down_state {
                            *a = KeyAction::Down;
                        }
                        *rc = count;
                        Some(PressTarget::item(item.id))
                    } else {
                        None
                    }
                }
                DisplayItemKind::KeyStrokeGroup { strokes } => {
                    if let Some(last) = strokes.last_mut() {
                        if last.action == KeyAction::Down || force_down_state {
                            if force_down_state {
                                last.action = KeyAction::Down;
                            }
                            last.repeat_count = count;
                            Some(PressTarget::group(item.id, strokes.len() - 1))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(target) = matched {
                Self::refresh_item(item, now);
                return Some(target);
            }
        }

        None
    }

    fn apply_key_up_to_existing(&mut self, ke: &KeyEvent, now: Instant) {
        let Some(target) = self.active_presses.remove(&PressKey::from_key_event(ke)) else {
            return;
        };

        let Some(item) = self.items.iter_mut().find(|item| item.id == target.item_id) else {
            return;
        };

        let updated = match &mut item.kind {
            DisplayItemKind::KeyStroke { action, .. } => {
                *action = KeyAction::Up;
                true
            }
            DisplayItemKind::KeyStrokeGroup { strokes } => match target.group_index {
                Some(idx) => {
                    if let Some(stroke) = strokes.get_mut(idx) {
                        stroke.action = KeyAction::Up;
                        true
                    } else {
                        false
                    }
                }
                None => false,
            },
            _ => false,
        };

        if updated {
            Self::refresh_item(item, now);
        }
    }

    fn remap_item_target_to_group_first(&mut self, item_id: u64) {
        for target in self.active_presses.values_mut() {
            if target.item_id == item_id && target.group_index.is_none() {
                target.group_index = Some(0);
            }
        }
    }

    fn prune_active_press_targets(&mut self) {
        let live_ids: HashSet<u64> = self.items.iter().map(|item| item.id).collect();
        self.active_presses
            .retain(|_, target| live_ids.contains(&target.item_id));
    }

    fn refresh_item(item: &mut DisplayItem, now: Instant) {
        item.created_at = now;
        item.opacity = 1.0;
        item.phase = DisplayPhase::Active;
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
