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
    /// IME変換中文字列がアクティブか
    ime_composing: bool,
    /// OSのIME APIから取得したネイティブ変換中表示か
    ime_native_composing: bool,
    /// IME ON/OFF のフォールバック状態
    ime_fallback_enabled: bool,
    /// IMEフォールバック用のローマ字バッファ
    ime_fallback_romaji: String,
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
            ime_composing: false,
            ime_native_composing: false,
            ime_fallback_enabled: false,
            ime_fallback_romaji: String::new(),
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

        // IME切替キーは常に捕捉（"?"表示を防ぐ）
        if self.handle_ime_toggle_key(&ke) {
            return;
        }

        // IMEフォールバック入力（Composition取得失敗時の救済）
        if self.handle_ime_fallback_key(&ke) {
            return;
        }

        // IME変換中はローマ字入力キーを抑制し、Composition表示を優先する
        if self.ime_composing && should_suppress_during_ime_composition(&ke) {
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
            ImeEventKind::StateChanged { enabled } => {
                if !enabled {
                    // IME OFF時のみフォールバックを明示的に停止する。
                    // ON側は半角/全角キーでのローカル判定を優先し、
                    // 誤検知時に日本語化し続ける問題を避ける。
                    self.ime_fallback_enabled = false;
                    self.ime_composing = false;
                    self.ime_native_composing = false;
                    self.ime_fallback_romaji.clear();
                    self.items.retain(|item| {
                        !matches!(item.kind, DisplayItemKind::ImeComposition { .. })
                    });
                    self.prune_active_press_targets();
                }
            }
            ImeEventKind::CompositionUpdate { text } => {
                self.ime_composing = true;
                self.ime_native_composing = true;
                self.ime_fallback_romaji.clear();
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
                // ネイティブIME由来の変換終了のみ確定的に終了扱いにする。
                // フォールバック入力中に空文字イベントが届いてもバッファを壊さない。
                if self.ime_native_composing {
                    self.ime_composing = false;
                    self.ime_native_composing = false;
                    self.ime_fallback_romaji.clear();
                    self.items
                        .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
                    self.prune_active_press_targets();
                } else if !self.ime_fallback_enabled {
                    self.ime_composing = false;
                    self.items
                        .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
                    self.prune_active_press_targets();
                }
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
        self.ime_composing = false;
        self.ime_native_composing = false;
        self.ime_fallback_romaji.clear();
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

    fn handle_ime_toggle_key(&mut self, ke: &KeyEvent) -> bool {
        const VK_KANA: u32 = 0x15;
        const VK_IME_ON: u32 = 0x16;
        const VK_KANJI: u32 = 0x19;
        const VK_IME_OFF: u32 = 0x1A;
        const VK_OEM_3: u32 = 0xC0;

        let vk = ke.key.0 & 0xFF;
        // 日本語キーボードの半角/全角は環境により VK が異なるため scan_code を最優先。
        let is_hankaku_zenkaku = ke.scan_code == 0x29 || (vk == VK_OEM_3 && ke.scan_code == 0x29);

        if !matches!(vk, VK_KANA | VK_IME_ON | VK_KANJI | VK_IME_OFF) && !is_hankaku_zenkaku {
            return false;
        }

        if ke.action == KeyAction::Down {
            match vk {
                VK_IME_ON => self.ime_fallback_enabled = true,
                VK_IME_OFF => self.ime_fallback_enabled = false,
                VK_KANA | VK_KANJI => self.ime_fallback_enabled = !self.ime_fallback_enabled,
                VK_OEM_3 if is_hankaku_zenkaku => {
                    self.ime_fallback_enabled = !self.ime_fallback_enabled
                }
                _ if is_hankaku_zenkaku => {
                    self.ime_fallback_enabled = !self.ime_fallback_enabled
                }
                _ => {}
            }

            if !self.ime_fallback_enabled {
                self.ime_composing = false;
                self.ime_native_composing = false;
                self.ime_fallback_romaji.clear();
                self.items.retain(|item| {
                    !matches!(item.kind, DisplayItemKind::ImeComposition { .. })
                });
                self.prune_active_press_targets();
            }
        }

        true
    }

    fn handle_ime_fallback_key(&mut self, ke: &KeyEvent) -> bool {
        if !self.config.behavior.show_ime_composition
            || !self.ime_fallback_enabled
            || self.ime_native_composing
            || ke.modifiers.ctrl
            || ke.modifiers.alt
            || ke.modifiers.win
        {
            return false;
        }

        let vk = ke.key.0 & 0xFF;
        let is_letter = (0x41..=0x5A).contains(&vk);
        let is_control_key = matches!(vk, 0x08 | 0x0D | 0x1B | 0x09 | 0x20); // BS/Enter/Esc/Tab/Space

        if ke.action == KeyAction::Up {
            return is_letter || is_control_key;
        }

        if is_letter {
            let c = (vk as u8 as char).to_ascii_lowercase();
            self.ime_fallback_romaji.push(c);
            self.apply_ime_fallback_text(ke.timestamp);
            return true;
        }

        if vk == 0x08 {
            let _ = self.ime_fallback_romaji.pop();
            self.apply_ime_fallback_text(ke.timestamp);
            return true;
        }

        if matches!(vk, 0x0D | 0x1B | 0x09 | 0x20) {
            self.ime_fallback_romaji.clear();
            self.ime_composing = false;
            self.items
                .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
            self.prune_active_press_targets();
            return true;
        }

        false
    }

    fn apply_ime_fallback_text(&mut self, now: Instant) {
        let text = romaji_to_hiragana(&self.ime_fallback_romaji);
        if text.is_empty() {
            self.ime_composing = false;
            self.ime_native_composing = false;
            self.items
                .retain(|item| !matches!(item.kind, DisplayItemKind::ImeComposition { .. }));
            self.prune_active_press_targets();
            return;
        }

        self.ime_composing = true;
        self.ime_native_composing = false;
        let updated = self.items.iter_mut().any(|item| {
            if let DisplayItemKind::ImeComposition { text: ref mut t } = item.kind {
                *t = text.clone();
                item.phase = DisplayPhase::Active;
                item.opacity = 1.0;
                item.created_at = now;
                true
            } else {
                false
            }
        });

        if !updated {
            let _ = self.add_item(DisplayItemKind::ImeComposition { text }, now);
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

fn should_suppress_during_ime_composition(ke: &KeyEvent) -> bool {
    if ke.modifiers.ctrl || ke.modifiers.alt || ke.modifiers.win {
        return false;
    }

    // VKの下位1byteを使用（拡張値は除外）
    let vk = ke.key.0 & 0xFF;
    (0x30..=0x5A).contains(&vk) || (0xBA..=0xE2).contains(&vk)
}

fn romaji_to_hiragana(romaji: &str) -> String {
    let s: String = romaji
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;

    while i < bytes.len() {
        // 促音（小さい「っ」）: 子音重複（nn除く）
        if i + 1 < bytes.len()
            && bytes[i] == bytes[i + 1]
            && is_romaji_consonant(bytes[i] as char)
            && bytes[i] != b'n'
        {
            out.push('っ');
            i += 1;
            continue;
        }

        // 「ん」処理
        if bytes[i] == b'n' {
            if i + 1 == bytes.len() {
                break; // 末尾nは確定待ち
            }
            let next = bytes[i + 1] as char;
            if next == 'n' {
                out.push('ん');
                i += 1;
                continue;
            }
            if !is_romaji_vowel(next) && next != 'y' {
                out.push('ん');
                i += 1;
                continue;
            }
        }

        if i + 3 <= bytes.len() {
            let chunk = &s[i..i + 3];
            if let Some(kana) = romaji_map_3(chunk) {
                out.push_str(kana);
                i += 3;
                continue;
            }
        }

        if i + 2 <= bytes.len() {
            let chunk = &s[i..i + 2];
            if let Some(kana) = romaji_map_2(chunk) {
                out.push_str(kana);
                i += 2;
                continue;
            }
        }

        if i + 1 <= bytes.len() {
            let chunk = &s[i..i + 1];
            if let Some(kana) = romaji_map_1(chunk) {
                out.push_str(kana);
                i += 1;
                continue;
            }
        }

        // 末尾の未確定1文字は待機し、それ以外の未知綴りは素通しで継続。
        // 例: "nihogngo" -> "にほgんご"
        if i + 1 >= bytes.len() {
            break;
        }
        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn is_romaji_vowel(c: char) -> bool {
    matches!(c, 'a' | 'i' | 'u' | 'e' | 'o')
}

fn is_romaji_consonant(c: char) -> bool {
    c.is_ascii_alphabetic() && !is_romaji_vowel(c)
}

fn romaji_map_3(s: &str) -> Option<&'static str> {
    let v = match s {
        "kya" => "きゃ",
        "kyu" => "きゅ",
        "kyo" => "きょ",
        "gya" => "ぎゃ",
        "gyu" => "ぎゅ",
        "gyo" => "ぎょ",
        "sha" | "sya" => "しゃ",
        "shu" | "syu" => "しゅ",
        "sho" | "syo" => "しょ",
        "cha" | "tya" | "cya" => "ちゃ",
        "chu" | "tyu" | "cyu" => "ちゅ",
        "cho" | "tyo" | "cyo" => "ちょ",
        "nya" => "にゃ",
        "nyu" => "にゅ",
        "nyo" => "にょ",
        "hya" => "ひゃ",
        "hyu" => "ひゅ",
        "hyo" => "ひょ",
        "mya" => "みゃ",
        "myu" => "みゅ",
        "myo" => "みょ",
        "rya" => "りゃ",
        "ryu" => "りゅ",
        "ryo" => "りょ",
        "bya" => "びゃ",
        "byu" => "びゅ",
        "byo" => "びょ",
        "pya" => "ぴゃ",
        "pyu" => "ぴゅ",
        "pyo" => "ぴょ",
        "ja" | "jya" | "zya" => "じゃ",
        "ju" | "jyu" | "zyu" => "じゅ",
        "jo" | "jyo" | "zyo" => "じょ",
        "shi" => "し",
        "chi" => "ち",
        "tsu" => "つ",
        "dya" => "ぢゃ",
        "dyu" => "ぢゅ",
        "dyo" => "ぢょ",
        _ => return None,
    };
    Some(v)
}

fn romaji_map_2(s: &str) -> Option<&'static str> {
    let v = match s {
        "ka" => "か",
        "ki" => "き",
        "ku" => "く",
        "ke" => "け",
        "ko" => "こ",
        "ga" => "が",
        "gi" => "ぎ",
        "gu" => "ぐ",
        "ge" => "げ",
        "go" => "ご",
        "sa" => "さ",
        "su" => "す",
        "se" => "せ",
        "so" => "そ",
        "za" => "ざ",
        "ji" => "じ",
        "zu" => "ず",
        "ze" => "ぜ",
        "zo" => "ぞ",
        "ta" => "た",
        "te" => "て",
        "to" => "と",
        "da" => "だ",
        "di" => "ぢ",
        "du" => "づ",
        "de" => "で",
        "do" => "ど",
        "na" => "な",
        "ni" => "に",
        "nu" => "ぬ",
        "ne" => "ね",
        "no" => "の",
        "ha" => "は",
        "hi" => "ひ",
        "fu" => "ふ",
        "he" => "へ",
        "ho" => "ほ",
        "ba" => "ば",
        "bi" => "び",
        "bu" => "ぶ",
        "be" => "べ",
        "bo" => "ぼ",
        "pa" => "ぱ",
        "pi" => "ぴ",
        "pu" => "ぷ",
        "pe" => "ぺ",
        "po" => "ぽ",
        "ma" => "ま",
        "mi" => "み",
        "mu" => "む",
        "me" => "め",
        "mo" => "も",
        "ya" => "や",
        "yu" => "ゆ",
        "yo" => "よ",
        "ra" => "ら",
        "ri" => "り",
        "ru" => "る",
        "re" => "れ",
        "ro" => "ろ",
        "wa" => "わ",
        "wo" => "を",
        "fa" => "ふぁ",
        "fi" => "ふぃ",
        "fe" => "ふぇ",
        "fo" => "ふぉ",
        "va" => "ゔぁ",
        "vi" => "ゔぃ",
        "vu" => "ゔ",
        "ve" => "ゔぇ",
        "vo" => "ゔぉ",
        _ => return None,
    };
    Some(v)
}

fn romaji_map_1(s: &str) -> Option<&'static str> {
    let v = match s {
        "a" => "あ",
        "i" => "い",
        "u" => "う",
        "e" => "え",
        "o" => "お",
        _ => return None,
    };
    Some(v)
}
