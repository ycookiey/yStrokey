/// キーコード。テンキーとメインキーを別値として定義。
/// Win32 VK_*コードをベースに、テンキーEnterを0x200|0x0Dで区別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u32);

impl KeyCode {
    // --- 数字キー (メインキーボード) ---
    pub const KEY_0: Self = Self(0x30);
    pub const KEY_1: Self = Self(0x31);
    pub const KEY_2: Self = Self(0x32);
    pub const KEY_3: Self = Self(0x33);
    pub const KEY_4: Self = Self(0x34);
    pub const KEY_5: Self = Self(0x35);
    pub const KEY_6: Self = Self(0x36);
    pub const KEY_7: Self = Self(0x37);
    pub const KEY_8: Self = Self(0x38);
    pub const KEY_9: Self = Self(0x39);

    // --- アルファベットキー ---
    pub const KEY_A: Self = Self(0x41);
    pub const KEY_B: Self = Self(0x42);
    pub const KEY_C: Self = Self(0x43);
    pub const KEY_D: Self = Self(0x44);
    pub const KEY_E: Self = Self(0x45);
    pub const KEY_F: Self = Self(0x46);
    pub const KEY_G: Self = Self(0x47);
    pub const KEY_H: Self = Self(0x48);
    pub const KEY_I: Self = Self(0x49);
    pub const KEY_J: Self = Self(0x4A);
    pub const KEY_K: Self = Self(0x4B);
    pub const KEY_L: Self = Self(0x4C);
    pub const KEY_M: Self = Self(0x4D);
    pub const KEY_N: Self = Self(0x4E);
    pub const KEY_O: Self = Self(0x4F);
    pub const KEY_P: Self = Self(0x50);
    pub const KEY_Q: Self = Self(0x51);
    pub const KEY_R: Self = Self(0x52);
    pub const KEY_S: Self = Self(0x53);
    pub const KEY_T: Self = Self(0x54);
    pub const KEY_U: Self = Self(0x55);
    pub const KEY_V: Self = Self(0x56);
    pub const KEY_W: Self = Self(0x57);
    pub const KEY_X: Self = Self(0x58);
    pub const KEY_Y: Self = Self(0x59);
    pub const KEY_Z: Self = Self(0x5A);

    // --- テンキー ---
    pub const NUMPAD_0: Self = Self(0x60);
    pub const NUMPAD_1: Self = Self(0x61);
    pub const NUMPAD_2: Self = Self(0x62);
    pub const NUMPAD_3: Self = Self(0x63);
    pub const NUMPAD_4: Self = Self(0x64);
    pub const NUMPAD_5: Self = Self(0x65);
    pub const NUMPAD_6: Self = Self(0x66);
    pub const NUMPAD_7: Self = Self(0x67);
    pub const NUMPAD_8: Self = Self(0x68);
    pub const NUMPAD_9: Self = Self(0x69);
    pub const NUMPAD_MULTIPLY: Self = Self(0x6A);
    pub const NUMPAD_ADD: Self = Self(0x6B);
    pub const NUMPAD_SEPARATOR: Self = Self(0x6C);
    pub const NUMPAD_SUBTRACT: Self = Self(0x6D);
    pub const NUMPAD_DECIMAL: Self = Self(0x6E);
    pub const NUMPAD_DIVIDE: Self = Self(0x6F);
    /// テンキーEnter（通常Enterと区別するため 0x200 | 0x0D）
    pub const NUMPAD_ENTER: Self = Self(0x200 | 0x0D);

    // --- 修飾キー ---
    pub const L_SHIFT: Self = Self(0xA0);
    pub const R_SHIFT: Self = Self(0xA1);
    pub const L_CTRL: Self = Self(0xA2);
    pub const R_CTRL: Self = Self(0xA3);
    pub const L_ALT: Self = Self(0xA4);
    pub const R_ALT: Self = Self(0xA5);
    pub const L_WIN: Self = Self(0x5B);
    pub const R_WIN: Self = Self(0x5C);

    // --- 特殊キー ---
    pub const BACKSPACE: Self = Self(0x08);
    pub const TAB: Self = Self(0x09);
    pub const ENTER: Self = Self(0x0D);
    pub const PAUSE: Self = Self(0x13);
    pub const CAPS_LOCK: Self = Self(0x14);
    pub const ESCAPE: Self = Self(0x1B);
    pub const SPACE: Self = Self(0x20);
    pub const PAGE_UP: Self = Self(0x21);
    pub const PAGE_DOWN: Self = Self(0x22);
    pub const END: Self = Self(0x23);
    pub const HOME: Self = Self(0x24);
    pub const LEFT: Self = Self(0x25);
    pub const UP: Self = Self(0x26);
    pub const RIGHT: Self = Self(0x27);
    pub const DOWN: Self = Self(0x28);
    pub const PRINT_SCREEN: Self = Self(0x2C);
    pub const INSERT: Self = Self(0x2D);
    pub const DELETE: Self = Self(0x2E);
    pub const NUM_LOCK: Self = Self(0x90);
    pub const SCROLL_LOCK: Self = Self(0x91);

    // --- ファンクションキー ---
    pub const F1: Self = Self(0x70);
    pub const F2: Self = Self(0x71);
    pub const F3: Self = Self(0x72);
    pub const F4: Self = Self(0x73);
    pub const F5: Self = Self(0x74);
    pub const F6: Self = Self(0x75);
    pub const F7: Self = Self(0x76);
    pub const F8: Self = Self(0x77);
    pub const F9: Self = Self(0x78);
    pub const F10: Self = Self(0x79);
    pub const F11: Self = Self(0x7A);
    pub const F12: Self = Self(0x7B);

    /// 表示用ラベルを返す
    pub fn label(&self) -> &'static str {
        match *self {
            // 数字キー
            Self::KEY_0 => "0",
            Self::KEY_1 => "1",
            Self::KEY_2 => "2",
            Self::KEY_3 => "3",
            Self::KEY_4 => "4",
            Self::KEY_5 => "5",
            Self::KEY_6 => "6",
            Self::KEY_7 => "7",
            Self::KEY_8 => "8",
            Self::KEY_9 => "9",
            // アルファベットキー
            Self::KEY_A => "A",
            Self::KEY_B => "B",
            Self::KEY_C => "C",
            Self::KEY_D => "D",
            Self::KEY_E => "E",
            Self::KEY_F => "F",
            Self::KEY_G => "G",
            Self::KEY_H => "H",
            Self::KEY_I => "I",
            Self::KEY_J => "J",
            Self::KEY_K => "K",
            Self::KEY_L => "L",
            Self::KEY_M => "M",
            Self::KEY_N => "N",
            Self::KEY_O => "O",
            Self::KEY_P => "P",
            Self::KEY_Q => "Q",
            Self::KEY_R => "R",
            Self::KEY_S => "S",
            Self::KEY_T => "T",
            Self::KEY_U => "U",
            Self::KEY_V => "V",
            Self::KEY_W => "W",
            Self::KEY_X => "X",
            Self::KEY_Y => "Y",
            Self::KEY_Z => "Z",
            // テンキー
            Self::NUMPAD_0 => "Num0",
            Self::NUMPAD_1 => "Num1",
            Self::NUMPAD_2 => "Num2",
            Self::NUMPAD_3 => "Num3",
            Self::NUMPAD_4 => "Num4",
            Self::NUMPAD_5 => "Num5",
            Self::NUMPAD_6 => "Num6",
            Self::NUMPAD_7 => "Num7",
            Self::NUMPAD_8 => "Num8",
            Self::NUMPAD_9 => "Num9",
            Self::NUMPAD_MULTIPLY => "Num*",
            Self::NUMPAD_ADD => "Num+",
            Self::NUMPAD_SEPARATOR => "NumSep",
            Self::NUMPAD_SUBTRACT => "Num-",
            Self::NUMPAD_DECIMAL => "Num.",
            Self::NUMPAD_DIVIDE => "Num/",
            Self::NUMPAD_ENTER => "NumEnter",
            // 修飾キー
            Self::L_CTRL | Self::R_CTRL => "Ctrl",
            Self::L_SHIFT | Self::R_SHIFT => "Shift",
            Self::L_ALT | Self::R_ALT => "Alt",
            Self::L_WIN | Self::R_WIN => "Win",
            // 特殊キー
            Self::BACKSPACE => "BS",
            Self::TAB => "Tab",
            Self::ENTER => "Enter",
            Self::PAUSE => "Pause",
            Self::CAPS_LOCK => "CapsLock",
            Self::ESCAPE => "Esc",
            Self::SPACE => "Space",
            Self::PAGE_UP => "PgUp",
            Self::PAGE_DOWN => "PgDn",
            Self::END => "End",
            Self::HOME => "Home",
            Self::LEFT => "Left",
            Self::UP => "Up",
            Self::RIGHT => "Right",
            Self::DOWN => "Down",
            Self::PRINT_SCREEN => "PrtSc",
            Self::INSERT => "Ins",
            Self::DELETE => "Del",
            Self::NUM_LOCK => "NumLock",
            Self::SCROLL_LOCK => "ScrLk",
            // ファンクションキー
            Self::F1 => "F1",
            Self::F2 => "F2",
            Self::F3 => "F3",
            Self::F4 => "F4",
            Self::F5 => "F5",
            Self::F6 => "F6",
            Self::F7 => "F7",
            Self::F8 => "F8",
            Self::F9 => "F9",
            Self::F10 => "F10",
            Self::F11 => "F11",
            Self::F12 => "F12",
            _ => "?",
        }
    }

    /// テンキー区別なしのラベルを返す（Numプレフィクスなし）
    pub fn label_plain(&self) -> &'static str {
        match *self {
            Self::NUMPAD_0 => "0",
            Self::NUMPAD_1 => "1",
            Self::NUMPAD_2 => "2",
            Self::NUMPAD_3 => "3",
            Self::NUMPAD_4 => "4",
            Self::NUMPAD_5 => "5",
            Self::NUMPAD_6 => "6",
            Self::NUMPAD_7 => "7",
            Self::NUMPAD_8 => "8",
            Self::NUMPAD_9 => "9",
            Self::NUMPAD_MULTIPLY => "*",
            Self::NUMPAD_ADD => "+",
            Self::NUMPAD_SEPARATOR => "Sep",
            Self::NUMPAD_SUBTRACT => "-",
            Self::NUMPAD_DECIMAL => ".",
            Self::NUMPAD_DIVIDE => "/",
            Self::NUMPAD_ENTER => "Enter",
            _ => self.label(),
        }
    }

    /// 修飾キーかどうか
    pub fn is_modifier(&self) -> bool {
        matches!(
            *self,
            Self::L_CTRL
                | Self::R_CTRL
                | Self::L_SHIFT
                | Self::R_SHIFT
                | Self::L_ALT
                | Self::R_ALT
                | Self::L_WIN
                | Self::R_WIN
        )
    }

    /// テンキー由来か
    pub fn is_numpad(&self) -> bool {
        matches!(self.0, 0x60..=0x6F) || *self == Self::NUMPAD_ENTER
    }
}
