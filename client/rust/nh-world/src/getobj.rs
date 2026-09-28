//! getobj() questions ("What do you want to wield? [- ab or ?*]") read
//! from their text, and engine menus that list inventory items.

use nh_protocol::MenuItem;

use crate::{Pack, Prompt};

/// A getobj() question: which item the engine wants, and which ones it
/// suggests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemQuestion {
    /// "wield", "put on", "dip the dagger into", as the engine words it.
    pub verb: String,
    /// The letters the engine suggests, in its order ('$' for gold, '#'
    /// for the overflow letter); not '-'.
    pub allowed: Vec<char>,
    /// '-' is offered: bare hands, fingers, nothing (empty the quiver).
    pub hands: bool,
    /// Nothing is suggested ("[*]", or no list at all): any item may be
    /// chosen and the engine judges.
    pub all: bool,
}

impl ItemQuestion {
    /// Whether digits typed before the letter are a count (the commands
    /// that call getobj() with GETOBJ_ALLOWCNT, and drop).
    pub fn takes_count(&self) -> bool {
        matches!(
            self.verb.as_str(),
            "drop" | "throw" | "fire" | "ready" | "wield" | "adjust" | "charge"
        )
    }

    /// Is `letter` one the engine suggests?
    pub fn suggests(&self, letter: char) -> bool {
        self.allowed.contains(&letter)
    }
}

/// Read a getobj() question: "What do you want to <verb>?" and a bracket
/// list "[- a-d$ or ?*]" or "[*]". tty compacts runs of four and more
/// letters to "a-d" (and three or more '#' to "#-#").
pub fn parse_item_question(query: &str) -> Option<ItemQuestion> {
    let rest = query.trim().strip_prefix("What do you want to ")?;
    let (verb, list) = match rest.find("? [") {
        Some(at) => (&rest[..at], Some(rest[at + 3..].strip_suffix(']')?)),
        None => (rest.strip_suffix('?')?, None),
    };
    if verb.is_empty() {
        return None;
    }
    let mut q = ItemQuestion {
        verb: verb.to_string(),
        allowed: Vec::new(),
        hands: false,
        all: false,
    };
    let list = match list {
        None | Some("*") => {
            q.all = true;
            return Some(q);
        }
        Some(l) => l.strip_suffix(" or ?*")?,
    };
    let mut letters = list;
    if let Some(after) = letters.strip_prefix('-')
        && (after.is_empty() || after.starts_with(' '))
    {
        q.hands = true;
        letters = after.trim_start();
    }
    let chars: Vec<char> = letters.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !is_invlet(c) {
            return None;
        }
        if chars.get(i + 1) == Some(&'-') {
            let end = *chars.get(i + 2)?;
            if !is_invlet(end) || end < c {
                return None;
            }
            for r in c..=end {
                push_new(&mut q.allowed, r);
            }
            i += 3;
        } else {
            push_new(&mut q.allowed, c);
            i += 1;
        }
    }
    Some(q)
}

/// The getobj() question a prompt asks, if it is one.
pub fn item_question(prompt: &Prompt) -> Option<ItemQuestion> {
    match prompt {
        Prompt::FreeKey {
            query,
            directions: false,
        } => parse_item_question(query),
        _ => None,
    }
}

fn is_invlet(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '$' || c == '#'
}

fn push_new(v: &mut Vec<char>, c: char) {
    if !v.contains(&c) {
        v.push(c);
    }
}

/// How the UI shows an engine menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// Every entry to pick is an item of the pack, by its letter and its
    /// doname(): `D`, `A`, identify, charging... The inventory panel shows
    /// it (multi-select mode).
    Inventory,
    /// Anything else: the generic list.
    Generic,
}

/// An inventory menu has at least one entry to pick, and each has an
/// inventory letter as its key and the text of that item (the engine may
/// add to it, as "(unpaid, 5 zorkmids)" or a weight in wizard mode).
/// Container contents are not inventory items: their letters point at
/// other things, so their texts do not match.
pub fn menu_kind(items: &[MenuItem], pack: &Pack) -> MenuKind {
    let mut picks = items.iter().filter(|i| i.selectable).peekable();
    if picks.peek().is_none() || !pack.received() {
        return MenuKind::Generic;
    }
    let all_items = picks.all(|i| {
        let letter = u32::try_from(i.ch).ok().and_then(char::from_u32);
        let text = i.str.as_deref().unwrap_or("");
        letter
            .and_then(|l| pack.by_letter(l))
            .is_some_and(|item| !item.text.is_empty() && text.starts_with(&item.text))
    });
    if all_items {
        MenuKind::Inventory
    } else {
        MenuKind::Generic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{InvItem, Inventory};

    fn q(query: &str) -> ItemQuestion {
        parse_item_question(query).unwrap_or_else(|| panic!("not a getobj question: {query}"))
    }

    fn letters(q: &ItemQuestion) -> String {
        q.allowed.iter().collect()
    }

    #[test]
    fn a_question_names_its_verb_and_letters() {
        let w = q("What do you want to wield? [- ab or ?*]");
        assert_eq!(w.verb, "wield");
        assert_eq!(letters(&w), "ab");
        assert!(w.hands && !w.all);
        assert!(w.suggests('a') && !w.suggests('c') && !w.suggests('-'));

        let t = q("What do you want to throw? [$ab or ?*]");
        assert_eq!(t.verb, "throw");
        assert_eq!(letters(&t), "$ab");
        assert!(!t.hands);
        assert!(t.takes_count());

        let d = q("What do you want to drop? [$a-e or ?*]");
        assert_eq!(letters(&d), "$abcde");
        let r = q("What do you want to read? [a-dhkm-pA-C or ?*]");
        assert_eq!(letters(&r), "abcdhkmnopABC");
        assert!(!r.takes_count());
    }

    #[test]
    fn nothing_suggested_opens_on_all() {
        let e = q("What do you want to eat? [*]");
        assert!(e.all && e.allowed.is_empty() && !e.hands);
        // the prompt without a list (a repeated command, force_invmenu)
        let e = q("What do you want to eat?");
        assert!(e.all && e.allowed.is_empty());
        // only hands: the trailing space goes
        let w = q("What do you want to wield? [- or ?*]");
        assert!(w.hands && !w.all && w.allowed.is_empty());
    }

    #[test]
    fn verbs_come_as_the_callers_word_them() {
        for (query, verb, hands, list) in [
            ("What do you want to wear? [c or ?*]", "wear", false, "c"),
            (
                "What do you want to take off? [bc or ?*]",
                "take off",
                false,
                "bc",
            ),
            (
                "What do you want to put on? [fg or ?*]",
                "put on",
                false,
                "fg",
            ),
            (
                "What do you want to remove? [fg or ?*]",
                "remove",
                false,
                "fg",
            ),
            ("What do you want to drink? [h or ?*]", "drink", false, "h"),
            ("What do you want to read? [ij or ?*]", "read", false, "ij"),
            ("What do you want to eat? [d or ?*]", "eat", false, "d"),
            (
                "What do you want to sacrifice? [k or ?*]",
                "sacrifice",
                false,
                "k",
            ),
            ("What do you want to zap? [l or ?*]", "zap", false, "l"),
            (
                "What do you want to use or apply? [e or ?*]",
                "use or apply",
                false,
                "e",
            ),
            ("What do you want to ready? [- b or ?*]", "ready", true, "b"),
            ("What do you want to fire? [$b or ?*]", "fire", false, "$b"),
            (
                "What do you want to write with? [- abl or ?*]",
                "write with",
                true,
                "abl",
            ),
            (
                "What do you want to dip? [a-f or ?*]",
                "dip",
                false,
                "abcdef",
            ),
            (
                "What do you want to dip the dagger into? [h or ?*]",
                "dip the dagger into",
                false,
                "h",
            ),
            (
                "What do you want to dip into one of the potions of water? [a-c or ?*]",
                "dip into one of the potions of water",
                false,
                "abc",
            ),
            (
                "What do you want to adjust? [$a-e or ?*]",
                "adjust",
                false,
                "$abcde",
            ),
            (
                "What do you want to name? [a-e or ?*]",
                "name",
                false,
                "abcde",
            ),
            ("What do you want to call? [h or ?*]", "call", false, "h"),
            ("What do you want to rub? [e or ?*]", "rub", false, "e"),
            (
                "What do you want to rub on the stone? [$a-e or ?*]",
                "rub on the stone",
                false,
                "$abcde",
            ),
            (
                "What do you want to grease? [a-e or ?*]",
                "grease",
                false,
                "abcde",
            ),
            (
                "What do you want to invoke? [a or ?*]",
                "invoke",
                false,
                "a",
            ),
            (
                "What do you want to charge? [l or ?*]",
                "charge",
                false,
                "l",
            ),
            ("What do you want to tip? [m or ?*]", "tip", false, "m"),
            (
                "What do you want to stash? [a-d or ?*]",
                "stash",
                false,
                "abcd",
            ),
            (
                "What do you want to untrap with? [n or ?*]",
                "untrap with",
                false,
                "n",
            ),
            (
                "What do you want to write on? [i or ?*]",
                "write on",
                false,
                "i",
            ),
            (
                "What do you want to destroy? [c or ?*]",
                "destroy",
                false,
                "c",
            ),
            ("What do you want to open? [p or ?*]", "open", false, "p"),
        ] {
            let got = q(query);
            assert_eq!(got.verb, verb, "{query}");
            assert_eq!(got.hands, hands, "{query}");
            assert_eq!(letters(&got), list, "{query}");
        }
    }

    #[test]
    fn overflow_letters_and_odd_lists() {
        let d = q("What do you want to drop? [$a-zA-Z# or ?*]");
        assert_eq!(d.allowed.len(), 54);
        assert_eq!(d.allowed.last(), Some(&'#'));
        let d = q("What do you want to drop? [a#-# or ?*]");
        assert_eq!(letters(&d), "a#");
    }

    #[test]
    fn other_questions_are_not_getobj() {
        for query in [
            "In what direction?",
            "Adjust letter to what [a-zA-Z] (? see used letters)?",
            "Split 3 to what [f-zA-Z] (? see used letters)?",
            "What do you want to wield? [ab]",
            "What do you want to? [a or ?*]",
            "What do you want to wield? [a-? or ?*]",
            "What do you want to wield? [d-a or ?*]",
            "There is a lichen corpse here; eat it?",
            "Which ring-finger, Right or Left?",
        ] {
            assert_eq!(parse_item_question(query), None, "{query}");
        }
        let dir = Prompt::FreeKey {
            query: "What do you want to wield? [a or ?*]".into(),
            directions: true,
        };
        assert_eq!(item_question(&dir), None);
        let free = Prompt::FreeKey {
            query: "What do you want to wield? [a or ?*]".into(),
            directions: false,
        };
        assert_eq!(item_question(&free).unwrap().verb, "wield");
        assert_eq!(item_question(&Prompt::Command), None);
    }

    fn inv(letter: char, text: &str) -> InvItem {
        InvItem {
            letter,
            class: ')',
            tile: 1,
            quan: 1,
            slots: vec![],
            lit: false,
            text: text.into(),
        }
    }

    fn entry(ch: char, text: &str, selectable: bool) -> MenuItem {
        MenuItem {
            win: 5,
            idx: 0,
            glyph: None,
            selectable,
            ch: ch as i32,
            gch: 0,
            attr: 0,
            clr: 0,
            str: Some(text.into()),
            preselected: false,
            skipinvert: false,
        }
    }

    #[test]
    fn menus_of_pack_items_are_inventory_menus() {
        let mut pack = Pack::new();
        pack.replace(&Inventory {
            items: vec![
                inv('a', "a +1 spear (weapon in right hand)"),
                inv('d', "an uncursed food ration"),
            ],
            twoweap: false,
        });
        let drop = [
            entry('\0', "Weapons", false),
            entry('a', "a +1 spear (weapon in right hand)", true),
            entry('\0', "Comestibles", false),
            entry('d', "an uncursed food ration", true),
        ];
        assert_eq!(menu_kind(&drop, &pack), MenuKind::Inventory);
        // a shop adds the price
        let shop = [entry(
            'd',
            "an uncursed food ration (unpaid, 45 zorkmids)",
            true,
        )];
        assert_eq!(menu_kind(&shop, &pack), MenuKind::Inventory);
        // a container's contents: letters, but not the pack's items
        let bag = [entry('a', "a scroll labeled FOO", true)];
        assert_eq!(menu_kind(&bag, &pack), MenuKind::Generic);
        // D's first step: categories
        let types = [entry('a', "All types", true), entry('b', "Weapons", true)];
        assert_eq!(menu_kind(&types, &pack), MenuKind::Generic);
        // one foreign entry spoils it
        let mixed = [
            entry('a', "a +1 spear (weapon in right hand)", true),
            entry('A', "Auto-select every item", true),
        ];
        assert_eq!(menu_kind(&mixed, &pack), MenuKind::Generic);
        // nothing to pick, or no pack yet
        assert_eq!(menu_kind(&drop[..1], &pack), MenuKind::Generic);
        assert_eq!(menu_kind(&drop, &Pack::new()), MenuKind::Generic);
    }
}
