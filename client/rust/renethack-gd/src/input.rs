//! Godot key events → `KeyInput`, independent of the keyboard layout:
//! commands must work with a Russian layout active, and Ctrl/Alt need the
//! Latin letter, which `unicode` does not carry.

use godot::classes::InputEventKey;
use godot::global::Key as GKey;
use godot::obj::EngineEnum;
use godot::prelude::*;
use nh_world::{Key, KeyInput, Mods};

use crate::ui_events::UiEvent;

/// The parts of an InputEventKey the translation looks at.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RawKey {
    /// Latin label of the key in the current layout (Godot `keycode`).
    pub keycode: i32,
    /// The key's position as a US-layout code.
    pub physical: i32,
    /// The character typed (layout-dependent; 0 with Ctrl).
    pub unicode: u32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd / Super.
    pub meta: bool,
    pub echo: bool,
    pub pressed: bool,
}

/// Keys that are not characters.
fn special(keycode: i32) -> Option<Key> {
    let table = [
        (GKey::ESCAPE, Key::Escape),
        (GKey::TAB, Key::Tab),
        (GKey::BACKSPACE, Key::Backspace),
        (GKey::ENTER, Key::Enter),
        (GKey::KP_ENTER, Key::KeypadEnter),
        (GKey::DELETE, Key::Delete),
        (GKey::HOME, Key::Home),
        (GKey::END, Key::End),
        (GKey::LEFT, Key::Left),
        (GKey::UP, Key::Up),
        (GKey::RIGHT, Key::Right),
        (GKey::DOWN, Key::Down),
        (GKey::PAGEUP, Key::PageUp),
        (GKey::PAGEDOWN, Key::PageDown),
        (GKey::KP_PERIOD, Key::KeypadDot),
        (GKey::KP_ADD, Key::Char('+')),
        (GKey::KP_SUBTRACT, Key::Char('-')),
        (GKey::KP_MULTIPLY, Key::Char('*')),
        (GKey::KP_DIVIDE, Key::Char('/')),
    ];
    if let Some((_, k)) = table.iter().find(|(g, _)| g.ord() == keycode) {
        return Some(*k);
    }
    let kp0 = GKey::KP_0.ord();
    if (kp0..=GKey::KP_9.ord()).contains(&keycode) {
        return Some(Key::Keypad((keycode - kp0) as u8));
    }
    let f1 = GKey::F1.ord();
    if (f1..=GKey::F12.ord()).contains(&keycode) {
        return Some(Key::F((keycode - f1 + 1) as u8));
    }
    None
}

fn printable_ascii(c: u32) -> Option<char> {
    char::from_u32(c).filter(|c| c.is_ascii_graphic() || *c == ' ')
}

/// A letter key's character: the capital with Shift.
fn letter(keycode: i32, shift: bool) -> Option<char> {
    let c = u32::try_from(keycode).ok().and_then(char::from_u32)?;
    c.is_ascii_alphabetic().then(|| {
        if shift {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        }
    })
}

/// What the key at US position `code` types, with or without Shift.
fn us_char(code: i32, shift: bool) -> Option<char> {
    if let Some(c) = letter(code, shift) {
        return Some(c);
    }
    let c = printable_ascii(u32::try_from(code).ok()?)?;
    if !shift {
        return Some(c);
    }
    const PAIRS: [(char, char); 21] = [
        ('1', '!'),
        ('2', '@'),
        ('3', '#'),
        ('4', '$'),
        ('5', '%'),
        ('6', '^'),
        ('7', '&'),
        ('8', '*'),
        ('9', '('),
        ('0', ')'),
        ('`', '~'),
        ('-', '_'),
        ('=', '+'),
        ('[', '{'),
        (']', '}'),
        ('\\', '|'),
        (';', ':'),
        ('\'', '"'),
        (',', '<'),
        ('.', '>'),
        ('/', '?'),
    ];
    Some(PAIRS.iter().find(|(k, _)| *k == c).map_or(c, |(_, s)| *s))
}

/// Translate one key event. `text_ok`: a non-ASCII character is kept as
/// typed (text fields) instead of being folded to the Latin key. `mac`:
/// Option types layout characters ('[', '|', '@'), so it is Meta only when
/// the character typed is not printable ASCII.
pub fn translate(raw: &RawKey, text_ok: bool, mac: bool) -> Option<KeyInput> {
    if !raw.pressed {
        return None;
    }
    let mods = Mods {
        shift: raw.shift,
        ctrl: raw.ctrl,
        alt: raw.alt,
    };
    let make = |key, mods| {
        Some(KeyInput {
            key,
            mods,
            echo: raw.echo,
        })
    };
    if let Some(key) = special(raw.keycode) {
        return make(key, mods);
    }
    if raw.keycode >= GKey::SPECIAL.ord() || raw.meta {
        // modifiers alone, media keys, OS shortcuts
        return None;
    }
    let typed = printable_ascii(raw.unicode);
    if raw.ctrl {
        let c = letter(raw.keycode, false).or_else(|| us_char(raw.keycode, false))?;
        return make(Key::Char(c), mods);
    }
    if raw.alt {
        if mac && let Some(c) = typed {
            let plain = Mods { alt: false, ..mods };
            return make(Key::Char(c), plain);
        }
        let c = letter(raw.keycode, raw.shift)
            .or_else(|| {
                let digit = u32::try_from(raw.keycode).ok().and_then(char::from_u32);
                digit.filter(|d| d.is_ascii_digit() && !raw.shift)
            })
            .or(typed)
            .or_else(|| us_char(raw.physical, raw.shift))?;
        return make(Key::Char(c), mods);
    }
    if let Some(c) = typed {
        return make(Key::Char(c), mods);
    }
    let other = char::from_u32(raw.unicode).filter(|c| *c != '\0' && !c.is_control());
    if text_ok && let Some(c) = other {
        return make(Key::Char(c), mods);
    }
    // another layout (or no character at all): the Latin letter, else the
    // US character at the key's position ("Shift+3" is '#', not '№')
    let c = letter(raw.keycode, raw.shift).or_else(|| us_char(raw.physical, raw.shift))?;
    make(Key::Char(c), mods)
}

/// Keys the client keeps for itself (never sent to the engine): F9 the
/// message log, F5 rest until HP and Pw are full, Ctrl+`-` / Ctrl+`=`
/// (`+`) zoom, F8 or Ctrl+`0` the whole level. NetHack binds none of them.
pub fn client_key(k: &KeyInput) -> Option<UiEvent> {
    if k.mods.alt {
        return None;
    }
    match (k.key, k.mods.ctrl) {
        (Key::F(9), _) => Some(UiEvent::ToggleFullLog),
        (Key::F(5), _) => Some(UiEvent::Rest),
        (Key::F(8), _) | (Key::Char('0'), true) => Some(UiEvent::ToggleOverview),
        (Key::Char('-' | '_'), true) => Some(UiEvent::Zoom(1.0)),
        (Key::Char('=' | '+'), true) => Some(UiEvent::Zoom(-1.0)),
        _ => None,
    }
}

/// The key a release event lets go of (the same translation as its press).
pub fn key_release(ev: &Gd<InputEventKey>) -> Option<KeyInput> {
    if ev.is_pressed() {
        return None;
    }
    let raw = RawKey {
        keycode: ev.get_keycode().ord(),
        physical: ev.get_physical_keycode().ord(),
        unicode: ev.get_unicode(),
        shift: ev.is_shift_pressed(),
        ctrl: ev.is_ctrl_pressed(),
        alt: ev.is_alt_pressed(),
        meta: ev.is_meta_pressed(),
        echo: false,
        pressed: true,
    };
    translate(&raw, false, cfg!(target_os = "macos"))
}

/// `translate` for a Godot event.
pub fn key_input(ev: &Gd<InputEventKey>, text_ok: bool) -> Option<KeyInput> {
    let raw = RawKey {
        keycode: ev.get_keycode().ord(),
        physical: ev.get_physical_keycode().ord(),
        unicode: ev.get_unicode(),
        shift: ev.is_shift_pressed(),
        ctrl: ev.is_ctrl_pressed(),
        alt: ev.is_alt_pressed(),
        meta: ev.is_meta_pressed(),
        echo: ev.is_echo(),
        pressed: ev.is_pressed(),
    };
    translate(&raw, text_ok, cfg!(target_os = "macos"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(keycode: i32, unicode: char) -> RawKey {
        RawKey {
            keycode,
            physical: keycode,
            unicode: unicode as u32,
            pressed: true,
            ..RawKey::default()
        }
    }

    fn key(raw: RawKey) -> Option<Key> {
        translate(&raw, false, false).map(|k| k.key)
    }

    const A: i32 = 'A' as i32;

    #[test]
    fn input_release_is_ignored() {
        let raw = RawKey {
            pressed: false,
            ..press(A, 'a')
        };
        assert_eq!(translate(&raw, false, false), None);
    }

    #[test]
    fn input_ascii_comes_from_unicode() {
        assert_eq!(key(press(A, 'a')), Some(Key::Char('a')));
        let shifted = RawKey {
            shift: true,
            ..press(A, 'A')
        };
        assert_eq!(key(shifted), Some(Key::Char('A')));
        // symbols exist only as unicode ('#' is Shift+3)
        let hash = RawKey {
            shift: true,
            ..press('3' as i32, '#')
        };
        assert_eq!(key(hash), Some(Key::Char('#')));
        assert_eq!(key(press(' ' as i32, ' ')), Some(Key::Char(' ')));
    }

    #[test]
    fn input_ctrl_takes_the_latin_letter() {
        let raw = RawKey {
            ctrl: true,
            ..press('P' as i32, '\0')
        };
        let k = translate(&raw, false, false).unwrap();
        assert_eq!(k.key, Key::Char('p'));
        assert!(k.mods.ctrl);
        // with a Russian layout the keycode is still Latin
        let ru = RawKey {
            ctrl: true,
            ..press('T' as i32, 'е')
        };
        assert_eq!(key(ru), Some(Key::Char('t')));
    }

    #[test]
    fn input_alt_is_meta_with_the_latin_key() {
        let raw = RawKey {
            alt: true,
            ..press('P' as i32, 'п')
        };
        let k = translate(&raw, false, false).unwrap();
        assert_eq!((k.key, k.mods.alt), (Key::Char('p'), true));
        let shifted = RawKey {
            alt: true,
            shift: true,
            ..press('E' as i32, '\0')
        };
        assert_eq!(key(shifted), Some(Key::Char('E')));
        let digit = RawKey {
            alt: true,
            ..press('2' as i32, '\0')
        };
        assert_eq!(key(digit), Some(Key::Char('2')));
    }

    #[test]
    fn input_mac_option_types_layout_characters() {
        // German Mac layout: Option+5 is '['
        let bracket = RawKey {
            alt: true,
            ..press('5' as i32, '[')
        };
        let k = translate(&bracket, false, true).unwrap();
        assert_eq!((k.key, k.mods.alt), (Key::Char('['), false));
        // Option+p types 'π': that is Meta
        let pi = RawKey {
            alt: true,
            ..press('P' as i32, 'π')
        };
        let k = translate(&pi, false, true).unwrap();
        assert_eq!((k.key, k.mods.alt), (Key::Char('p'), true));
    }

    #[test]
    fn input_russian_layout_gives_latin_commands() {
        // 'i' under the Russian layout types 'ш'
        assert_eq!(key(press('I' as i32, 'ш')), Some(Key::Char('i')));
        let shifted = RawKey {
            shift: true,
            ..press('D' as i32, 'В')
        };
        assert_eq!(key(shifted), Some(Key::Char('D')));
        // Shift+3 types '№': the US table gives '#'
        let hash = RawKey {
            shift: true,
            ..press('3' as i32, '№')
        };
        assert_eq!(key(hash), Some(Key::Char('#')));
        // unshifted punctuation by position: 'б' sits on ','
        let comma = RawKey {
            physical: ',' as i32,
            ..press(0, 'б')
        };
        assert_eq!(key(comma), Some(Key::Char(',')));
    }

    #[test]
    fn input_text_fields_keep_what_was_typed() {
        let raw = press('I' as i32, 'ш');
        assert_eq!(
            translate(&raw, true, false).map(|k| k.key),
            Some(Key::Char('ш'))
        );
    }

    #[test]
    fn input_special_keys_by_keycode() {
        let k = |g: GKey| key(press(g.ord(), '\0'));
        assert_eq!(k(GKey::ENTER), Some(Key::Enter));
        assert_eq!(k(GKey::KP_ENTER), Some(Key::KeypadEnter));
        assert_eq!(k(GKey::ESCAPE), Some(Key::Escape));
        assert_eq!(k(GKey::BACKSPACE), Some(Key::Backspace));
        assert_eq!(k(GKey::UP), Some(Key::Up));
        assert_eq!(k(GKey::PAGEDOWN), Some(Key::PageDown));
        assert_eq!(k(GKey::KP_7), Some(Key::Keypad(7)));
        assert_eq!(k(GKey::KP_PERIOD), Some(Key::KeypadDot));
        assert_eq!(k(GKey::F9), Some(Key::F(9)));
        assert_eq!(k(GKey::F12), Some(Key::F(12)));
        // a modifier alone is nothing
        assert_eq!(k(GKey::SHIFT), None);
        assert_eq!(k(GKey::CTRL), None);
        // Cmd shortcuts belong to the OS
        let cmd = RawKey {
            meta: true,
            ..press('Q' as i32, 'q')
        };
        assert_eq!(key(cmd), None);
    }

    #[test]
    fn client_keys_are_the_log_zoom_and_overview() {
        let ctrl = |c| KeyInput {
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
            ..KeyInput::plain(Key::Char(c))
        };
        let plain = |k| client_key(&KeyInput::plain(k));
        assert_eq!(plain(Key::F(9)), Some(UiEvent::ToggleFullLog));
        assert_eq!(plain(Key::F(8)), Some(UiEvent::ToggleOverview));
        assert_eq!(plain(Key::F(5)), Some(UiEvent::Rest));
        assert_eq!(client_key(&ctrl('0')), Some(UiEvent::ToggleOverview));
        assert_eq!(client_key(&ctrl('-')), Some(UiEvent::Zoom(1.0)));
        assert_eq!(client_key(&ctrl('=')), Some(UiEvent::Zoom(-1.0)));
        assert_eq!(client_key(&ctrl('+')), Some(UiEvent::Zoom(-1.0)));
        // NetHack's own keys stay NetHack's
        assert_eq!(plain(Key::Char('-')), None);
        assert_eq!(plain(Key::Char('+')), None);
        assert_eq!(plain(Key::Char('0')), None);
        assert_eq!(client_key(&ctrl('p')), None);
        assert_eq!(plain(Key::PageUp), None);
    }

    #[test]
    fn input_echo_is_marked_for_the_context_to_decide() {
        let raw = RawKey {
            echo: true,
            ..press('L' as i32, 'l')
        };
        let k = translate(&raw, false, false).unwrap();
        assert!(k.echo);
        let arrow = RawKey {
            echo: true,
            ..press(GKey::LEFT.ord(), '\0')
        };
        assert!(translate(&arrow, false, false).unwrap().echo);
    }
}
