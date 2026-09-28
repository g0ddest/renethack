use std::collections::VecDeque;

use crate::DEFAULT_DIRCHARS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character as typed (Shift already applied).
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// Keypad digit 0–9.
    Keypad(u8),
    KeypadEnter,
    KeypadDot,
    /// F1–F12.
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInput {
    pub key: Key,
    pub mods: Mods,
    /// Auto-repeat of a held key.
    pub echo: bool,
}

impl KeyInput {
    /// A key pressed without modifiers.
    pub fn plain(key: Key) -> KeyInput {
        KeyInput {
            key,
            mods: Mods::default(),
            echo: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    /// nh_poskey: commands; direction keys move.
    Command,
    /// getdir: direction keys give directions.
    Directions,
    /// getobj and other free-key prompts: direction keys are not letters.
    Letters,
}

/// Direction index in NetHack's dirchars order: W NW N NE E SE S SW.
fn direction(key: Key) -> Option<usize> {
    Some(match key {
        Key::Left | Key::Keypad(4) => 0,
        Key::Home | Key::Keypad(7) => 1,
        Key::Up | Key::Keypad(8) => 2,
        Key::PageUp | Key::Keypad(9) => 3,
        Key::Right | Key::Keypad(6) => 4,
        Key::PageDown | Key::Keypad(3) => 5,
        Key::Down | Key::Keypad(2) => 6,
        Key::End | Key::Keypad(1) => 7,
        _ => return None,
    })
}

/// NetHack key code or None.
/// Directions (Command and Directions contexts): arrows, Home/PgUp/End/PgDn and keypad
/// 1–9 map through `dirchars` (order W NW N NE E SE S SW); Shift = run: the capital
/// letter, or M-digit (0x80|d) with number_pad on; keypad 5 → '.' (Shift → 's').
/// Letters context (getobj etc.): arrows, Home/End/PgUp/PgDn, keypad → None.
/// Char(c): Ctrl → c & 0x1f (letters), Alt → 0x80 | c; else c (ASCII only).
/// Enter → None in Command, '\n' otherwise; Backspace → None in Command, 8 otherwise;
/// Esc → 27; Tab → 9; F-keys → None (client functions).
/// Auto-repeat (`echo`) only counts in Command: a held key must not answer a question.
pub fn nethack_key(
    input: &KeyInput,
    ctx: KeyContext,
    number_pad: bool,
    dirchars: &str,
) -> Option<i32> {
    if input.echo && ctx != KeyContext::Command {
        return None;
    }
    let mods = input.mods;
    let command = ctx == KeyContext::Command;
    if let Some(dir) = direction(input.key) {
        if ctx == KeyContext::Letters {
            return None;
        }
        let chars: Vec<char> = dirchars.chars().collect();
        let chars: Vec<char> = if chars.len() >= 8 {
            chars
        } else {
            DEFAULT_DIRCHARS.chars().collect()
        };
        let c = chars[dir];
        if !c.is_ascii() {
            return None;
        }
        return Some(match (mods.shift, number_pad) {
            (false, _) => c as i32,
            (true, false) => c.to_ascii_uppercase() as i32,
            (true, true) => 0x80 | c as i32,
        });
    }
    match input.key {
        Key::Keypad(5) if ctx == KeyContext::Letters => None,
        Key::Keypad(5) => Some(if mods.shift { 's' } else { '.' } as i32),
        Key::Keypad(_) | Key::KeypadDot if ctx == KeyContext::Letters => None,
        Key::KeypadDot => Some('.' as i32),
        Key::Keypad(_) => None,
        Key::Char(c) => char_code(c, mods),
        Key::Enter | Key::KeypadEnter => (!command).then_some('\n' as i32),
        Key::Backspace => (!command).then_some(8),
        Key::Escape => Some(27),
        Key::Tab => Some(9),
        Key::Delete | Key::F(_) => None,
        Key::Up
        | Key::Down
        | Key::Left
        | Key::Right
        | Key::Home
        | Key::End
        | Key::PageUp
        | Key::PageDown => None,
    }
}

fn char_code(c: char, mods: Mods) -> Option<i32> {
    if !c.is_ascii() || c.is_ascii_control() {
        return None;
    }
    let mut code = c as i32;
    if mods.ctrl {
        // ^A..^Z, and ^@ ^[ ^\ ^] ^^ ^_ like a terminal
        let upper = c.to_ascii_uppercase();
        if !('@'..='_').contains(&upper) {
            return None;
        }
        code = upper as i32 & 0x1f;
    }
    if mods.alt {
        code |= 0x80;
    }
    Some(code)
}

/// How many keys typed ahead are kept.
pub const TYPEAHEAD_MAX: usize = 8;

/// Keys pressed while the engine is busy, for the next Command/Key/FreeKey prompt.
/// At most 8 keys; auto-repeat is never kept (a held key would run on).
#[derive(Debug, Clone, Default)]
pub struct Typeahead {
    keys: VecDeque<KeyInput>,
}

impl Typeahead {
    pub fn new() -> Typeahead {
        Typeahead::default()
    }

    /// Keep `input`; false when it was dropped (echo or full).
    pub fn push(&mut self, input: KeyInput) -> bool {
        if input.echo || self.keys.len() >= TYPEAHEAD_MAX {
            return false;
        }
        self.keys.push_back(input);
        true
    }

    pub fn pop(&mut self) -> Option<KeyInput> {
        self.keys.pop_front()
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUMPAD: &str = "47896321><";
    const SWAP_YZ: &str = "hzkulnjb><";
    const PHONE: &str = "41236987><";

    fn with(key: Key, shift: bool, ctrl: bool, alt: bool) -> KeyInput {
        KeyInput {
            key,
            mods: Mods { shift, ctrl, alt },
            echo: false,
        }
    }

    fn cmd(key: Key) -> Option<i32> {
        nethack_key(
            &KeyInput::plain(key),
            KeyContext::Command,
            false,
            DEFAULT_DIRCHARS,
        )
    }

    /// W NW N NE E SE S SW by arrows and keys, then by keypad.
    fn eight(ctx: KeyContext, number_pad: bool, dirchars: &str, shift: bool) -> (String, String) {
        let keys = [
            Key::Left,
            Key::Home,
            Key::Up,
            Key::PageUp,
            Key::Right,
            Key::PageDown,
            Key::Down,
            Key::End,
        ];
        let pad = [4, 7, 8, 9, 6, 3, 2, 1].map(Key::Keypad);
        let run = |ks: &[Key]| {
            ks.iter()
                .map(|&k| {
                    let code =
                        nethack_key(&with(k, shift, false, false), ctx, number_pad, dirchars)
                            .unwrap();
                    char::from_u32(code as u32 & 0x7f).unwrap()
                })
                .collect()
        };
        (run(&keys), run(&pad))
    }

    #[test]
    fn keys_directions_follow_dirchars() {
        let plain = |d: &str, np| eight(KeyContext::Command, np, d, false);
        assert_eq!(
            plain(DEFAULT_DIRCHARS, false),
            ("hykulnjb".into(), "hykulnjb".into())
        );
        assert_eq!(plain(SWAP_YZ, false).0, "hzkulnjb");
        assert_eq!(plain(NUMPAD, true).0, "47896321");
        assert_eq!(plain(PHONE, true).1, "41236987");
        // getdir maps the same way
        assert_eq!(
            eight(KeyContext::Directions, false, DEFAULT_DIRCHARS, false).0,
            "hykulnjb"
        );
        // no dirchars yet: the default
        assert_eq!(plain("", false).0, "hykulnjb");
        // keypad 5 rests, Shift searches
        assert_eq!(cmd(Key::Keypad(5)), Some('.' as i32));
        let search = nethack_key(
            &with(Key::Keypad(5), true, false, false),
            KeyContext::Command,
            false,
            "",
        );
        assert_eq!(search, Some('s' as i32));
        assert_eq!(cmd(Key::KeypadDot), Some('.' as i32));
        assert_eq!(cmd(Key::Keypad(0)), None);
    }

    #[test]
    fn keys_shift_runs() {
        assert_eq!(
            eight(KeyContext::Command, false, DEFAULT_DIRCHARS, true).0,
            "HYKULNJB"
        );
        let run = |k| {
            nethack_key(
                &with(k, true, false, false),
                KeyContext::Command,
                true,
                NUMPAD,
            )
        };
        assert_eq!(run(Key::Left), Some(0x80 | '4' as i32));
        assert_eq!(run(Key::Keypad(9)), Some(0x80 | '9' as i32));
    }

    #[test]
    fn keys_enter_and_backspace_do_nothing_in_command() {
        assert_eq!(cmd(Key::Enter), None);
        assert_eq!(cmd(Key::KeypadEnter), None);
        assert_eq!(cmd(Key::Backspace), None);
        let other = |k, ctx| nethack_key(&KeyInput::plain(k), ctx, false, DEFAULT_DIRCHARS);
        assert_eq!(other(Key::Enter, KeyContext::Letters), Some(10));
        assert_eq!(other(Key::Backspace, KeyContext::Letters), Some(8));
        assert_eq!(other(Key::Enter, KeyContext::Directions), Some(10));
    }

    #[test]
    fn keys_letters_context_ignores_arrows() {
        let letters = |k| {
            nethack_key(
                &KeyInput::plain(k),
                KeyContext::Letters,
                false,
                DEFAULT_DIRCHARS,
            )
        };
        for k in [
            Key::Left,
            Key::Right,
            Key::Up,
            Key::Down,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::Keypad(5),
            Key::Keypad(1),
            Key::KeypadDot,
        ] {
            assert_eq!(letters(k), None, "{k:?}");
        }
        assert_eq!(letters(Key::Char('j')), Some('j' as i32));
        assert_eq!(letters(Key::Char('-')), Some('-' as i32));
        assert_eq!(letters(Key::Char('*')), Some('*' as i32));
        assert_eq!(letters(Key::Escape), Some(27));
    }

    #[test]
    fn keys_ctrl_alt_esc_and_function_keys() {
        let c = |ch, shift, ctrl, alt| {
            nethack_key(
                &with(Key::Char(ch), shift, ctrl, alt),
                KeyContext::Command,
                false,
                "",
            )
        };
        assert_eq!(c('d', false, true, false), Some(4)); // ^D kick
        assert_eq!(c('T', true, true, false), Some(20)); // ^T with Shift held
        assert_eq!(c('p', false, false, true), Some(0x80 | 'p' as i32)); // M-p pray
        assert_eq!(c('P', true, false, true), Some(0x80 | 'P' as i32));
        assert_eq!(c('#', true, false, false), Some('#' as i32));
        assert_eq!(c('1', false, true, false), None);
        assert_eq!(c('ж', false, false, false), None);
        assert_eq!(cmd(Key::Escape), Some(27));
        assert_eq!(cmd(Key::Tab), Some(9));
        assert_eq!(cmd(Key::F(9)), None);
        assert_eq!(cmd(Key::Delete), None);
    }

    #[test]
    fn keys_echo_only_repeats_commands() {
        let held = KeyInput {
            echo: true,
            ..KeyInput::plain(Key::Home)
        };
        assert_eq!(
            nethack_key(&held, KeyContext::Command, false, DEFAULT_DIRCHARS),
            Some('y' as i32)
        );
        assert_eq!(
            nethack_key(&held, KeyContext::Directions, false, DEFAULT_DIRCHARS),
            None
        );
        let y = KeyInput {
            echo: true,
            ..KeyInput::plain(Key::Char('y'))
        };
        assert_eq!(
            nethack_key(&y, KeyContext::Letters, false, DEFAULT_DIRCHARS),
            None
        );
    }

    #[test]
    fn typeahead_drops_echo_and_caps_at_8() {
        let mut t = Typeahead::new();
        let echo = KeyInput {
            echo: true,
            ..KeyInput::plain(Key::Char('l'))
        };
        assert!(!t.push(echo));
        assert!(t.is_empty());
        for c in "abcdefghij".chars() {
            let kept = t.push(KeyInput::plain(Key::Char(c)));
            assert_eq!(kept, c <= 'h', "{c}");
        }
        assert_eq!(t.len(), TYPEAHEAD_MAX);
        assert_eq!(t.pop(), Some(KeyInput::plain(Key::Char('a'))));
        assert!(t.push(KeyInput::plain(Key::Char('z'))));
        t.clear();
        assert_eq!(t.pop(), None);
    }
}
