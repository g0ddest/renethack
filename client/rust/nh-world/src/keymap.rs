//! The keys a player rebinds (the settings' controls): a key the player
//! presses stands for the key the game knows a command by. Each key
//! profile has its own map, kept in the profile. The map applies only
//! where the engine waits for a command, and to the client's own actions;
//! where keys are data (a text, an item's letter, a menu, a yes or no, a
//! position on the map) a key is what it is.

use serde::{Deserialize, Serialize};

use crate::{Key, KeyInput, KeyProfile, Mods};

/// A key as compared and kept: no repeat, and Shift only on the keys
/// whose character does not already carry it.
pub fn norm(k: KeyInput) -> KeyInput {
    KeyInput {
        key: k.key,
        mods: Mods {
            shift: k.mods.shift && !matches!(k.key, Key::Char(_)),
            ..k.mods
        },
        echo: false,
    }
}

/// How a key reads, as the profile keeps it and the screen shows it:
/// "a", "Ctrl+d", "Alt+p", "F5", "Shift+Up", "Keypad 7", "Space".
pub fn key_name(k: &KeyInput) -> String {
    let k = norm(*k);
    let mut out = String::new();
    if k.mods.ctrl {
        out += "Ctrl+";
    }
    if k.mods.alt {
        out += "Alt+";
    }
    if k.mods.shift {
        out += "Shift+";
    }
    out += &match k.key {
        Key::Char(' ') => "Space".to_string(),
        Key::Char(c) => c.to_string(),
        Key::Enter => "Enter".into(),
        Key::Escape => "Esc".into(),
        Key::Backspace => "Backspace".into(),
        Key::Tab => "Tab".into(),
        Key::Delete => "Delete".into(),
        Key::Up => "Up".into(),
        Key::Down => "Down".into(),
        Key::Left => "Left".into(),
        Key::Right => "Right".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PgUp".into(),
        Key::PageDown => "PgDn".into(),
        Key::Keypad(d) => format!("Keypad {d}"),
        Key::KeypadEnter => "Keypad Enter".into(),
        Key::KeypadDot => "Keypad .".into(),
        Key::F(n) => format!("F{n}"),
    };
    out
}

/// A key from its name (`key_name`'s); None for one it does not read.
pub fn parse_key(name: &str) -> Option<KeyInput> {
    let mut mods = Mods::default();
    let mut rest = name;
    loop {
        if let Some(r) = rest.strip_prefix("Ctrl+") {
            mods.ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Alt+") {
            mods.alt = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("Shift+") {
            mods.shift = true;
            rest = r;
        } else {
            break;
        }
    }
    let key = match rest {
        "Space" => Key::Char(' '),
        "Enter" => Key::Enter,
        "Esc" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Delete" => Key::Delete,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "Home" => Key::Home,
        "End" => Key::End,
        "PgUp" => Key::PageUp,
        "PgDn" => Key::PageDown,
        "Keypad Enter" => Key::KeypadEnter,
        "Keypad ." => Key::KeypadDot,
        r => {
            if let Some(d) = r.strip_prefix("Keypad ") {
                Key::Keypad(d.parse().ok().filter(|d| *d <= 9)?)
            } else if let Some(n) = r.strip_prefix('F').filter(|n| !n.is_empty()) {
                Key::F(n.parse().ok().filter(|n| (1..=12).contains(n))?)
            } else {
                let mut chars = r.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                Key::Char(c)
            }
        }
    };
    Some(norm(KeyInput {
        key,
        mods,
        echo: false,
    }))
}

/// The key of a NetHack key code: ^X a Ctrl+letter, M-x an Alt+character.
pub fn key_of_code(code: i32) -> Option<KeyInput> {
    let (c, mods) = match code {
        1..=26 => (
            char::from(b'a' + code as u8 - 1),
            Mods {
                ctrl: true,
                ..Mods::default()
            },
        ),
        0x80..=0xff => (
            char::from((code & 0x7f) as u8),
            Mods {
                alt: true,
                ..Mods::default()
            },
        ),
        0x20..=0x7e => (char::from(code as u8), Mods::default()),
        _ => return None,
    };
    Some(KeyInput {
        key: Key::Char(c),
        mods,
        echo: false,
    })
}

/// A command's key moved: the player presses `key` (None: no key) for the
/// command the game knows by `command`. Names as `key_name` writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Moved {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// A profile's moved keys. Every command keeps one key: a key taken from
/// another command swaps with it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<Moved>,
}

impl Keymap {
    /// The moves read: (the command's key, the key pressed for it).
    fn pairs(&self) -> Vec<(KeyInput, Option<KeyInput>)> {
        self.moved
            .iter()
            .filter_map(|m| {
                let command = parse_key(&m.command)?;
                let key = match &m.key {
                    Some(k) => Some(parse_key(k)?),
                    None => None,
                };
                Some((command, key))
            })
            .collect()
    }

    fn set_pairs(&mut self, pairs: Vec<(KeyInput, Option<KeyInput>)>) {
        self.moved = pairs
            .into_iter()
            .filter(|(c, k)| *k != Some(*c))
            .map(|(c, k)| Moved {
                command: key_name(&c),
                key: k.map(|k| key_name(&k)),
            })
            .collect();
    }

    pub fn is_empty(&self) -> bool {
        self.moved.is_empty()
    }

    /// The command key `pressed` gives the game now: the one moved onto
    /// it, else itself; None when its own command moved away from it.
    pub fn translate(&self, pressed: KeyInput) -> Option<KeyInput> {
        let pressed = norm(pressed);
        let pairs = self.pairs();
        if let Some((c, _)) = pairs.iter().find(|(_, k)| *k == Some(pressed)) {
            return Some(*c);
        }
        if pairs.iter().any(|(c, _)| *c == pressed) {
            return None;
        }
        Some(pressed)
    }

    /// The key the player presses for the command `command` (None: none).
    pub fn key_for(&self, command: KeyInput) -> Option<KeyInput> {
        let command = norm(command);
        let pairs = self.pairs();
        if let Some((_, k)) = pairs.iter().find(|(c, _)| *c == command) {
            return *k;
        }
        // its own key, unless another command took it
        (!pairs.iter().any(|(_, k)| *k == Some(command))).then_some(command)
    }

    /// The player presses `key` for `command`. The command that had `key`
    /// takes `command`'s key when `swap` (a key of no command's needs
    /// none: `command`'s old key then gives nothing).
    pub fn rebind(&mut self, command: KeyInput, key: KeyInput, swap: bool) {
        let (command, key) = (norm(command), norm(key));
        let old = self.key_for(command);
        if old == Some(key) {
            return;
        }
        let holder = self.translate(key);
        let mut pairs: Vec<(KeyInput, Option<KeyInput>)> = self
            .pairs()
            .into_iter()
            .filter(|(c, k)| *c != command && *k != Some(key))
            .collect();
        pairs.push((command, Some(key)));
        if swap && let Some(h) = holder.filter(|h| *h != command) {
            pairs.retain(|(c, _)| *c != h);
            pairs.push((h, old));
        }
        self.set_pairs(pairs);
    }

    /// `command` back on its own key; a command that had the key takes
    /// `command`'s.
    pub fn reset(&mut self, command: KeyInput) {
        self.rebind(command, command, true);
    }
}

/// Both profiles' maps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymaps {
    #[serde(default, skip_serializing_if = "Keymap::is_empty")]
    pub modern: Keymap,
    #[serde(default, skip_serializing_if = "Keymap::is_empty")]
    pub classic: Keymap,
}

impl Keymaps {
    pub fn get(&self, profile: KeyProfile) -> &Keymap {
        match profile {
            KeyProfile::Modern => &self.modern,
            KeyProfile::Classic => &self.classic,
        }
    }

    pub fn get_mut(&mut self, profile: KeyProfile) -> &mut Keymap {
        match profile {
            KeyProfile::Modern => &mut self.modern,
            KeyProfile::Classic => &mut self.classic,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.modern.is_empty() && self.classic.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(c: char) -> KeyInput {
        KeyInput::plain(Key::Char(c))
    }

    fn ctrl(c: char) -> KeyInput {
        KeyInput {
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
            ..ch(c)
        }
    }

    #[test]
    fn a_key_reads_and_is_read_back() {
        let shift_up = KeyInput {
            mods: Mods {
                shift: true,
                ..Mods::default()
            },
            ..KeyInput::plain(Key::Up)
        };
        for k in [
            ch('a'),
            ch('A'),
            ch(' '),
            ctrl('d'),
            KeyInput::plain(Key::F(5)),
            KeyInput::plain(Key::Keypad(7)),
            KeyInput::plain(Key::PageDown),
            shift_up,
        ] {
            assert_eq!(parse_key(&key_name(&k)), Some(k), "{}", key_name(&k));
        }
        assert_eq!(key_name(&ctrl('d')), "Ctrl+d");
        assert_eq!(key_name(&shift_up), "Shift+Up");
        // a capital is its own key: Shift is in the character
        let shifted_a = KeyInput {
            mods: Mods {
                shift: true,
                ..Mods::default()
            },
            ..ch('A')
        };
        assert_eq!(norm(shifted_a), ch('A'));
        assert_eq!(parse_key("Ctrl+"), None);
        assert_eq!(key_of_code(4), Some(ctrl('d')));
        assert_eq!(key_name(&key_of_code(0x80 | 'p' as i32).unwrap()), "Alt+p");
    }

    #[test]
    fn a_free_key_takes_a_command_and_its_old_key_gives_nothing() {
        let mut m = Keymap::default();
        let f6 = KeyInput::plain(Key::F(6));
        m.rebind(ch('s'), f6, false);
        assert_eq!(m.translate(f6), Some(ch('s')));
        assert_eq!(m.translate(ch('s')), None, "search moved away from s");
        assert_eq!(m.translate(ch('e')), Some(ch('e')), "the rest as they were");
        assert_eq!(m.key_for(ch('s')), Some(f6));
    }

    #[test]
    fn a_taken_key_swaps_and_a_reset_swaps_back() {
        let mut m = Keymap::default();
        // search onto a: apply takes s
        m.rebind(ch('s'), ch('a'), true);
        assert_eq!(m.translate(ch('a')), Some(ch('s')));
        assert_eq!(m.translate(ch('s')), Some(ch('a')));
        assert_eq!(m.key_for(ch('a')), Some(ch('s')));
        // search onto e: eat takes search's a, apply keeps s
        m.rebind(ch('s'), ch('e'), true);
        assert_eq!(m.translate(ch('e')), Some(ch('s')));
        assert_eq!(m.translate(ch('a')), Some(ch('e')));
        assert_eq!(m.translate(ch('s')), Some(ch('a')));
        // apply back on a: eat (on a) takes apply's s
        m.reset(ch('a'));
        assert_eq!(m.translate(ch('a')), Some(ch('a')));
        assert_eq!(m.translate(ch('s')), Some(ch('e')));
        // every command keeps a key all along
        for c in ['a', 's', 'e'] {
            assert!(m.key_for(ch(c)).is_some(), "{c}");
        }
        m.reset(ch('s'));
        m.reset(ch('e'));
        assert!(m.is_empty(), "{m:?}");
    }

    #[test]
    fn the_maps_keep_in_the_profile_by_key_names() {
        let mut maps = Keymaps::default();
        maps.get_mut(KeyProfile::Classic)
            .rebind(ch('s'), KeyInput::plain(Key::F(6)), false);
        let text = serde_json::to_string(&maps).unwrap();
        assert_eq!(
            text,
            r#"{"classic":{"moved":[{"command":"s","key":"F6"}]}}"#
        );
        assert_eq!(serde_json::from_str::<Keymaps>(&text).unwrap(), maps);
        assert_eq!(serde_json::to_string(&Keymaps::default()).unwrap(), "{}");
    }
}
