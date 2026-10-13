//! The keyboard's commands a player moves to other keys (the settings'
//! controls): each profile's, by the key the game knows them by, with how
//! they read; the keys that stay as they are; what a key is taken by.

use nh_protocol::Catalog;
use nh_world::{Key, KeyInput, KeyProfile, Keymap, Mods, key_of_code, norm};

use crate::i18n::{self, EngineKind};
use crate::tr;

/// The list's groups, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Movement,
    Interface,
    Commands,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Movement, Group::Interface, Group::Commands];

    pub fn name(self) -> String {
        match self {
            Group::Movement => tr!("controls-group-movement"),
            Group::Interface => tr!("controls-group-interface"),
            Group::Commands => tr!("controls-group-commands"),
        }
    }
}

/// A command the player can move: the key the game knows it by in the
/// profile, how it reads, its group.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub key: KeyInput,
    pub name: String,
    pub group: Group,
}

fn plain(key: Key) -> KeyInput {
    KeyInput::plain(key)
}

fn ch(c: char) -> KeyInput {
    plain(Key::Char(c))
}

/// Ctrl and a letter: a command's key ("^X").
pub(crate) fn ctrl(c: char) -> KeyInput {
    KeyInput {
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        ..ch(c)
    }
}

fn alt(c: char) -> KeyInput {
    KeyInput {
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
        ..ch(c)
    }
}

/// The eight steps' keys, west first, round by north (as the catalog
/// lists the moves and the engine's dirchars run).
pub fn steps(profile: KeyProfile) -> [KeyInput; 8] {
    match profile {
        KeyProfile::Modern => [
            plain(Key::Left),
            plain(Key::Home),
            plain(Key::Up),
            plain(Key::PageUp),
            plain(Key::Right),
            plain(Key::PageDown),
            plain(Key::Down),
            plain(Key::End),
        ],
        KeyProfile::Classic => ['h', 'y', 'k', 'u', 'l', 'n', 'j', 'b'].map(ch),
    }
}

const MOVES: [&str; 8] = [
    "movewest",
    "movenorthwest",
    "movenorth",
    "movenortheast",
    "moveeast",
    "movesoutheast",
    "movesouth",
    "movesouthwest",
];

/// The letters number_pad gives NetHack's commands (Modern): their keys
/// in that profile instead of the catalog's.
const NUMBER_PAD: [(&str, char); 4] =
    [("kick", 'k'), ("jump", 'j'), ("loot", 'l'), ("untrap", 'u')];

/// The engine's words, as the player reads them, with a capital.
fn engine_words(desc: &str) -> String {
    let text = i18n::engine(EngineKind::Menu, desc).into_owned();
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

/// The commands of `profile` the player can move, in the list's order:
/// the eight steps, the client's own, then NetHack's that have a key.
pub fn commands(profile: KeyProfile, catalog: Option<&Catalog>) -> Vec<Command> {
    let mut out = Vec::new();
    let desc = |name: &str| {
        catalog
            .and_then(|c| c.extcmds.iter().find(|e| e.name == name))
            .map(|e| engine_words(&e.desc))
    };
    for (i, key) in steps(profile).into_iter().enumerate() {
        out.push(Command {
            key,
            name: desc(MOVES[i]).unwrap_or_else(|| tr!("controls-step")),
            group: Group::Movement,
        });
    }
    let mut interface = vec![
        (ch('i'), tr!("controls-inventory")),
        (ch('#'), tr!("controls-palette")),
        (plain(Key::F(5)), tr!("controls-rest")),
        (plain(Key::F(8)), tr!("controls-overview")),
        (ctrl('='), tr!("controls-zoom-in")),
        (ctrl('-'), tr!("controls-zoom-out")),
        (plain(Key::F(9)), tr!("controls-log")),
        (plain(Key::F(1)), tr!("controls-help")),
    ];
    if profile == KeyProfile::Modern {
        interface.push((ch('n'), tr!("controls-count-prefix")));
    }
    for (i, digit) in ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']
        .into_iter()
        .enumerate()
    {
        interface.push((ch(digit), tr!("controls-slot", n = i + 1)));
    }
    out.extend(interface.into_iter().map(|(key, name)| Command {
        key,
        name,
        group: Group::Interface,
    }));
    let Some(catalog) = catalog else {
        return out;
    };
    for e in &catalog.extcmds {
        if e.flags.iter().any(|f| f == "movement") || matches!(e.name.as_str(), "#" | "inventory") {
            continue;
        }
        let modern = (profile == KeyProfile::Modern)
            .then(|| NUMBER_PAD.iter().find(|(n, _)| *n == e.name))
            .flatten();
        let key = match modern {
            Some(&(_, c)) => Some(ch(c)),
            None => key_of_code(e.key),
        };
        let Some(key) = key else {
            continue;
        };
        // a key the client keeps for itself is not the engine's
        if out.iter().any(|c| c.key == key) {
            continue;
        }
        out.push(Command {
            key,
            name: engine_words(&e.desc),
            group: Group::Commands,
        });
    }
    out
}

/// What a key that stays as it is does (None: it is not one).
pub fn fixed(profile: KeyProfile, key: KeyInput) -> Option<String> {
    let key = norm(key);
    let m = key.mods;
    let bare = !m.ctrl && !m.alt && !m.shift;
    match key.key {
        Key::Escape => return Some(tr!("controls-fixed-escape")),
        Key::Keypad(_) | Key::KeypadDot | Key::KeypadEnter => {
            return Some(tr!("controls-fixed-keypad"));
        }
        _ => {}
    }
    let arrow = matches!(
        key.key,
        Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    );
    // running: Shift and a step (the capitals of the vi-keys)
    if arrow && m.shift && !m.ctrl && !m.alt {
        return Some(tr!("controls-fixed-run"));
    }
    match profile {
        KeyProfile::Classic => {
            if arrow && bare {
                return Some(tr!("controls-fixed-arrows"));
            }
            if let Key::Char(c) = key.key
                && !m.ctrl
                && !m.alt
                && "HJKLYUBN".contains(c)
            {
                return Some(tr!("controls-fixed-run"));
            }
            if let Key::Char('0'..='9') = key.key
                && m.alt
            {
                return Some(tr!("controls-fixed-count"));
            }
        }
        KeyProfile::Modern => {
            // NetHack's own keys of number_pad's letters
            let alternate = [ctrl('d'), alt('j'), alt('l'), alt('u')];
            if alternate.contains(&key) {
                return Some(tr!("controls-fixed-alternate"));
            }
        }
    }
    if key == ctrl('0') {
        return Some(tr!("controls-fixed-alternate"));
    }
    None
}

/// What a key a command would take is taken by.
#[derive(Debug, Clone, PartialEq)]
pub enum Taken {
    /// No command of the list (or the command itself): it takes it.
    Free,
    /// A key that stays as it is: what it does.
    Fixed(String),
    /// The `n`-th command of the list has it.
    Command(usize),
}

/// What `key` is taken by, for `by` (a command of `list`) to take it.
pub fn taken(
    list: &[Command],
    map: &Keymap,
    profile: KeyProfile,
    key: KeyInput,
    by: KeyInput,
) -> Taken {
    if let Some(what) = fixed(profile, key) {
        return Taken::Fixed(what);
    }
    match map.translate(key) {
        Some(c) if c != norm(by) => list
            .iter()
            .position(|cmd| cmd.key == c)
            .map_or(Taken::Free, Taken::Command),
        _ => Taken::Free,
    }
}

/// A key as NetHack's hints write it: "^D" Ctrl, "M-p" Alt, "F5", a
/// character as itself; others by their names ("PgUp").
pub fn short_name(k: &KeyInput) -> String {
    let k = norm(*k);
    let m = k.mods;
    match k.key {
        Key::Char(c) if m.ctrl && !m.alt && !m.shift => format!("^{}", c.to_ascii_uppercase()),
        Key::Char(c) if m.alt && !m.ctrl && !m.shift => format!("M-{c}"),
        Key::Char(c) if !m.ctrl && !m.alt && c != ' ' => c.to_string(),
        _ => nh_world::key_name(&k),
    }
}

/// The key of a NetHack-style hint ("s", "^D", "M-p", "F5"); None for a
/// command typed by its name ("#pray") or several keys.
pub fn hint_key(hint: &str) -> Option<KeyInput> {
    let mut chars = hint.chars();
    match (chars.next()?, chars.next(), chars.next()) {
        (c, None, None) => Some(ch(c)),
        ('^', Some(c), None) if c.is_ascii_alphabetic() => Some(ctrl(c.to_ascii_lowercase())),
        ('M', Some('-'), Some(c)) if hint.chars().count() == 3 => Some(alt(c)),
        ('F', Some(_), _) => hint[1..]
            .parse()
            .ok()
            .filter(|n| (1..=12).contains(n))
            .map(|n| plain(Key::F(n))),
        _ => None,
    }
}

/// A hint as the player presses it now: a key moved shows its new key, a
/// key moved away a dash; a command typed by its name stays.
pub fn bound_hint(map: &Keymap, hint: &str) -> String {
    match hint_key(hint) {
        Some(k) => map
            .key_for(k)
            .map_or_else(|| "—".to_string(), |p| short_name(&p)),
        None => hint.to_string(),
    }
}

/// A NetHack key code's key as the player presses it now ("" for none).
pub fn bound_code(map: &Keymap, code: i32) -> String {
    key_of_code(code)
        .and_then(|k| map.key_for(k))
        .map(|p| short_name(&p))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_and_the_client_s_own_keys_are_in_each_profile_s_list() {
        for profile in [KeyProfile::Modern, KeyProfile::Classic] {
            let list = commands(profile, None);
            assert_eq!(
                list.iter().filter(|c| c.group == Group::Movement).count(),
                8
            );
            for k in [ch('i'), ch('#'), plain(Key::F(5)), ch('1'), ch('0')] {
                assert!(list.iter().any(|c| c.key == k), "{profile:?} {k:?}");
            }
            for (i, c) in list.iter().enumerate() {
                assert!(
                    !list[i + 1..].iter().any(|o| o.key == c.key),
                    "{:?} twice",
                    c.key
                );
            }
        }
        // n counts in Modern; in Classic it is a step
        assert!(
            commands(KeyProfile::Modern, None)
                .iter()
                .any(|c| c.key == ch('n') && c.group == Group::Interface)
        );
        assert!(
            commands(KeyProfile::Classic, None)
                .iter()
                .any(|c| c.key == ch('n') && c.group == Group::Movement)
        );
    }

    #[test]
    fn a_hint_shows_the_key_the_player_presses_now() {
        let mut map = Keymap::default();
        assert_eq!(bound_hint(&map, "s"), "s");
        assert_eq!(bound_hint(&map, "^D"), "^D");
        assert_eq!(bound_hint(&map, "#pray"), "#pray");
        map.rebind(ch('s'), plain(Key::F(6)), false);
        map.rebind(ctrl('d'), ch('v'), true);
        assert_eq!(bound_hint(&map, "s"), "F6");
        assert_eq!(bound_hint(&map, "^D"), "v");
        assert_eq!(bound_code(&map, 4), "v");
        assert_eq!(bound_code(&map, 'e' as i32), "e");
        assert_eq!(hint_key("M-p"), Some(alt('p')));
        assert_eq!(hint_key("F5"), Some(plain(Key::F(5))));
        assert_eq!(short_name(&alt('p')), "M-p");
    }

    #[test]
    fn a_key_is_free_fixed_or_another_command_s() {
        let list = commands(KeyProfile::Classic, None);
        let mut map = Keymap::default();
        let search_like = ch('i');
        assert_eq!(
            taken(
                &list,
                &map,
                KeyProfile::Classic,
                plain(Key::F(6)),
                search_like
            ),
            Taken::Free
        );
        assert!(matches!(
            taken(
                &list,
                &map,
                KeyProfile::Classic,
                plain(Key::Escape),
                search_like
            ),
            Taken::Fixed(_)
        ));
        assert!(matches!(
            taken(&list, &map, KeyProfile::Classic, ch('H'), search_like),
            Taken::Fixed(_)
        ));
        let at = |k: KeyInput| list.iter().position(|c| c.key == k).unwrap();
        assert_eq!(
            taken(&list, &map, KeyProfile::Classic, ch('h'), search_like),
            Taken::Command(at(ch('h')))
        );
        // once moved, a key gives what was moved onto it
        map.rebind(ch('h'), plain(Key::F(6)), false);
        assert_eq!(
            taken(
                &list,
                &map,
                KeyProfile::Classic,
                plain(Key::F(6)),
                search_like
            ),
            Taken::Command(at(ch('h')))
        );
        assert_eq!(
            taken(&list, &map, KeyProfile::Classic, ch('h'), search_like),
            Taken::Free
        );
    }
}
