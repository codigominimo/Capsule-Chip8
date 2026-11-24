//! Mapa de teclado persistido en posiciones fijas de la cabecera `CAPSULE1`.
//!
//! Teclas anfitrionas asignables: `0-9`, `A-Z`, flechas, espacio, modificadores
//! y otras teclas comunes. `F1`, `Enter` y `Escape` quedan reservadas para la
//! interfaz.
//! Cada opción cabe en un byte por ranura CHIP-8.

use minifb::Key;

pub const HOST_KEY_COUNT: usize = 95;

const KEYS: [Key; HOST_KEY_COUNT] = [
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4, Key::Key5, Key::Key6, Key::Key7,
    Key::Key8, Key::Key9, Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S,
    Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z, Key::Space, Key::Down, Key::Left,
    Key::Right, Key::Up, Key::Tab, Key::LeftShift, Key::RightShift, Key::LeftCtrl, Key::RightCtrl,
    Key::LeftAlt, Key::RightAlt, Key::Backspace, Key::Delete, Key::End, Key::Home, Key::Insert,
    Key::Menu, Key::PageDown, Key::PageUp, Key::Pause, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6,
    Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12, Key::Apostrophe, Key::Backquote,
    Key::Backslash, Key::Comma, Key::Equal, Key::LeftBracket, Key::Minus, Key::Period,
    Key::RightBracket, Key::Semicolon, Key::Slash, Key::NumPad0, Key::NumPad1, Key::NumPad2,
    Key::NumPad3, Key::NumPad4, Key::NumPad5, Key::NumPad6, Key::NumPad7, Key::NumPad8, Key::NumPad9,
    Key::NumPadDot, Key::NumPadSlash, Key::NumPadAsterisk, Key::NumPadMinus, Key::NumPadPlus,
    Key::NumPadEnter,
];

const LABELS: [&str; HOST_KEY_COUNT] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F", "G", "H",
    "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
    "SP", "DN", "LT", "RT", "UP", "TB", "LS", "RS", "LC", "RC", "LA", "RA", "BK", "DE", "END",
    "HM", "IN", "MN", "PDN", "PUP", "PA", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10",
    "F11", "F12", "AP", "BQ", "BS", "CM", "EQ", "LB", "MN", "PD", "RB", "SC", "SL", "N0", "N1",
    "N2", "N3", "N4", "N5", "N6", "N7", "N8", "N9", "ND", "NS", "NA", "NM", "NP", "NE",
];

pub fn key_from_id(id: u8) -> Option<Key> {
    KEYS.get(id as usize).copied()
}

pub fn id_from_key(key: Key) -> Option<u8> {
    KEYS.iter().position(|&k| k == key).map(|i| i as u8)
}

pub fn label(id: u8) -> &'static str {
    LABELS.get(id as usize).copied().unwrap_or("?")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keymap {
    keys: [u8; 16],
}

impl Default for Keymap {
    fn default() -> Self {
        Self::factory()
    }
}

impl Keymap {
    /// `1 2 3 C / 4 5 6 D / 7 8 9 E / A 0 B F` sobre `1234/qwer/asdf/zxcv`.
    pub const fn factory() -> Self {
        Self {
            keys: [
                33, 1, 2, 3, // CHIP 0,1,2,3 -> X,1,2,3
                26, 32, 14, 10, // CHIP 4,5,6,7 -> Q,W,E,A
                28, 13, 35, 12, // CHIP 8,9,A,B -> S,D,Z,C
                4, 27, 15, 31, // CHIP C,D,E,F -> 4,R,F,V
            ],
        }
    }

    pub fn from_bytes(bytes: &[u8; 16]) -> Option<Self> {
        if bytes.iter().all(|&id| (id as usize) < HOST_KEY_COUNT) {
            Some(Self { keys: *bytes })
        } else {
            None
        }
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.keys
    }

    pub fn chip_key(&self, key: Key) -> Option<usize> {
        let id = id_from_key(key)?;
        self.keys.iter().position(|&k| k == id)
    }

    pub fn host_key(&self, slot: usize) -> Option<Key> {
        key_from_id(*self.keys.get(slot)?)
    }

    pub fn label(&self, slot: usize) -> &'static str {
        self.keys.get(slot).copied().map(label).unwrap_or("?")
    }

    pub fn assign(&mut self, slot: usize, id: u8) -> bool {
        if slot >= 16 || id as usize >= HOST_KEY_COUNT {
            return false;
        }
        let previous = self.keys[slot];
        for j in 0..16 {
            if j != slot && self.keys[j] == id {
                self.keys[j] = previous;
            }
        }
        self.keys[slot] = id;
        true
    }

    pub fn cycle(&mut self, slot: usize, delta: i32) {
        if slot >= 16 {
            return;
        }
        let current = self.keys[slot] as i32;
        let n = HOST_KEY_COUNT as i32;
        let next = (current + delta).rem_euclid(n) as u8;
        self.assign(slot, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_map_matches_classic_layout() {
        let m = Keymap::factory();
        assert_eq!(m.chip_key(Key::Key1), Some(1));
        assert_eq!(m.chip_key(Key::Q), Some(4));
        assert_eq!(m.chip_key(Key::A), Some(7));
        assert_eq!(m.chip_key(Key::Z), Some(10));
        assert_eq!(m.chip_key(Key::V), Some(15));
    }

    #[test]
    fn bytes_roundtrip_and_validation() {
        let m = Keymap::factory();
        let again = Keymap::from_bytes(m.as_bytes()).unwrap();
        assert_eq!(again, m);
        let mut bad = *m.as_bytes();
        bad[0] = HOST_KEY_COUNT as u8;
        assert!(Keymap::from_bytes(&bad).is_none());
    }

    #[test]
    fn assigning_a_taken_key_swaps_slots() {
        let mut m = Keymap::factory();
        m.assign(1, m.as_bytes()[4]);
        assert_eq!(m.as_bytes()[1], 26);
        assert_eq!(m.as_bytes()[4], 1);
    }

    #[test]
    fn cycle_wraps_around_the_full_key_list() {
        let mut m = Keymap::factory();
        m.assign(0, 0);
        m.cycle(0, -1);
        assert_eq!(m.label(0), "NE");
        m.cycle(0, 1);
        assert_eq!(m.label(0), "0");
    }

    #[test]
    fn arrows_and_space_are_assignable() {
        assert_eq!(id_from_key(Key::Up), Some(40));
        assert_eq!(id_from_key(Key::Space), Some(36));
        assert_eq!(label(40), "UP");
        assert_eq!(label(36), "SP");
    }

    #[test]
    fn cycling_into_a_taken_key_swaps_it() {
        let mut m = Keymap::factory();
        m.assign(1, 36);
        let previous = m.as_bytes()[0];
        m.assign(0, 36);
        assert_eq!(m.as_bytes()[0], 36);
        assert_eq!(m.as_bytes()[1], previous);
    }
}
