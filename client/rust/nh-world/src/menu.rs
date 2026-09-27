use nh_protocol::{Glyph, MenuItem, PickHow, Reply};

#[derive(Debug, Clone, PartialEq)]
pub struct MenuEntry {
    /// add_menu index; the reply names items by it.
    pub idx: i32,
    pub text: String,
    pub attr: i32,
    pub selectable: bool,
    /// Key that toggles it: its own `ch`, else one assigned a–z, A–Z.
    pub letter: Option<char>,
    /// Group accelerator (`gch`).
    pub group: Option<char>,
    pub glyph: Option<Glyph>,
    /// Bulk select and invert never turn it on.
    pub skipinvert: bool,
    pub selected: bool,
    /// Count typed before picking it; None = all.
    pub count: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuOutcome {
    Pending,
    Done(Reply),
    PageUp,
    PageDown,
}

/// A select_menu in progress, following tty's rules.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuState {
    pub how: PickHow,
    pub title: Option<String>,
    pub entries: Vec<MenuEntry>,
    /// Count being typed (only once it is not zero, as in tty).
    count: Option<i64>,
}

const ESC: char = '\u{1b}';

fn to_char(c: i32) -> Option<char> {
    u32::try_from(c)
        .ok()
        .filter(|&c| c != 0)
        .and_then(char::from_u32)
}

impl MenuState {
    /// Letters: the item's own `ch`; selectable items without one get a–z then A–Z in
    /// order, skipping letters taken explicitly in this menu (tty assigns by page position;
    /// the difference only shows in which letter a click-only item gets).
    pub fn new(how: PickHow, title: Option<String>, items: &[MenuItem]) -> MenuState {
        let taken: Vec<char> = items.iter().filter_map(|i| to_char(i.ch)).collect();
        let mut free = ('a'..='z').chain('A'..='Z').filter(|c| !taken.contains(c));
        let entries = items
            .iter()
            .map(|i| {
                let letter = match to_char(i.ch) {
                    Some(c) => Some(c),
                    None if i.selectable => free.next(),
                    None => None,
                };
                MenuEntry {
                    idx: i.idx,
                    text: i.str.clone().unwrap_or_default(),
                    attr: i.attr,
                    selectable: i.selectable,
                    letter,
                    group: to_char(i.gch),
                    glyph: i.glyph.clone(),
                    skipinvert: i.skipinvert,
                    selected: i.selectable && i.preselected,
                    count: None,
                }
            })
            .collect();
        MenuState {
            how,
            title,
            entries,
            count: None,
        }
    }

    /// tty precedence: (1) an item's own letter; (2) a group accelerator — in pick-any it
    /// toggles its group, in pick-one it picks only if exactly one item has it; a digit is
    /// a group key only while no count is being typed; (3) digits build a count (then the
    /// next pick gets it); (4) commands, pick-any only: '.' ',' select all, '-' none,
    /// '@' invert — none of them turns on a skipinvert item; '>' '<' page; Enter/Space
    /// confirm; Esc cancel. Pick-one: a pick replaces any preselection and finishes.
    pub fn key(&mut self, c: char) -> MenuOutcome {
        if self.how == PickHow::None {
            return match c {
                '\n' | '\r' | ' ' | ESC => MenuOutcome::Done(Reply::Ack),
                '>' => MenuOutcome::PageDown,
                '<' => MenuOutcome::PageUp,
                _ => MenuOutcome::Pending,
            };
        }
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.selectable && e.letter == Some(c))
        {
            return self.pick(i);
        }
        let counting = self.count.is_some();
        if !(c.is_ascii_digit() && counting)
            && let Some(outcome) = self.group(c)
        {
            return outcome;
        }
        if let Some(d) = c.to_digit(10) {
            let count = self
                .count
                .unwrap_or(0)
                .checked_mul(10)
                .and_then(|n| n.checked_add(i64::from(d)));
            // leading zeros are ignored; an overflow drops the count
            self.count = count.filter(|&n| n != 0 && n <= i64::from(i32::MAX));
            return MenuOutcome::Pending;
        }
        let any = self.how == PickHow::Any;
        match c {
            '.' | ',' if any => {
                for e in self.entries.iter_mut() {
                    if e.selectable && !e.selected && !e.skipinvert {
                        e.selected = true;
                    }
                }
            }
            '-' if any => {
                for e in self.entries.iter_mut().filter(|e| e.selectable) {
                    e.selected = false;
                    e.count = None;
                }
            }
            '@' if any => {
                for e in self.entries.iter_mut() {
                    if !e.selectable || (e.skipinvert && !e.selected) {
                        continue;
                    }
                    e.selected = !e.selected;
                    e.count = None;
                }
            }
            '>' => return MenuOutcome::PageDown,
            '<' => return MenuOutcome::PageUp,
            '\n' | '\r' | ' ' => return MenuOutcome::Done(self.confirm()),
            // Esc while typing a count only drops the count
            ESC if counting => self.count = None,
            ESC => return MenuOutcome::Done(Reply::Cancel),
            _ => {}
        }
        self.count = None;
        MenuOutcome::Pending
    }

    /// Group accelerator `c`, if it is one here (tty: an item's `gch` that is
    /// not its own letter).
    fn group(&mut self, c: char) -> Option<MenuOutcome> {
        let members: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                e.selectable && e.group == Some(c) && e.letter != Some(c)
            })
            .collect();
        if members.is_empty() {
            return None;
        }
        match self.how {
            PickHow::One if members.len() == 1 => Some(self.pick(members[0])),
            PickHow::One | PickHow::None => None,
            PickHow::Any => {
                let count = self.count.take();
                for i in members {
                    let e = &mut self.entries[i];
                    if e.selected {
                        e.selected = false;
                        e.count = None;
                    } else {
                        e.selected = true;
                        e.count = count;
                    }
                }
                Some(MenuOutcome::Pending)
            }
        }
    }

    /// Pick entry `i` with the count being typed.
    fn pick(&mut self, i: usize) -> MenuOutcome {
        let count = self.count.take();
        if self.how == PickHow::One {
            for e in self.entries.iter_mut() {
                e.selected = false;
                e.count = None;
            }
            let e = &mut self.entries[i];
            e.selected = true;
            e.count = count;
            return MenuOutcome::Done(self.confirm());
        }
        // tty toggle_menu_curr: a count (re)selects with it, otherwise toggle
        let e = &mut self.entries[i];
        if count.is_some() {
            e.selected = true;
            e.count = count;
        } else {
            e.selected = !e.selected;
            e.count = None;
        }
        MenuOutcome::Pending
    }

    /// A click on entry `i` (position in `entries`).
    pub fn click(&mut self, i: usize) -> MenuOutcome {
        match self.entries.get(i) {
            Some(e) if e.selectable && self.how != PickHow::None => self.pick(i),
            _ => MenuOutcome::Pending,
        }
    }

    /// The answer as the menu stands: selected items in order, count -1 for "all".
    pub fn confirm(&self) -> Reply {
        if self.how == PickHow::None {
            return Reply::Ack;
        }
        let picked = self
            .entries
            .iter()
            .filter(|e| e.selectable && e.selected)
            .filter_map(|e| {
                u32::try_from(e.idx)
                    .ok()
                    .map(|idx| (idx, e.count.unwrap_or(-1)))
            });
        match self.how {
            PickHow::One => Reply::Menu(picked.take(1).collect()),
            _ => Reply::Menu(picked.collect()),
        }
    }

    /// The count being typed, to show it.
    pub fn typed_count(&self) -> Option<i64> {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(idx: i32, ch: char, text: &str) -> MenuItem {
        MenuItem {
            win: 4,
            idx,
            glyph: None,
            selectable: true,
            ch: if ch == '\0' { 0 } else { ch as i32 },
            gch: 0,
            attr: 0,
            clr: 8,
            str: Some(text.to_string()),
            preselected: false,
            skipinvert: false,
        }
    }

    fn header(idx: i32, text: &str) -> MenuItem {
        MenuItem {
            selectable: false,
            attr: 7,
            ..item(idx, '\0', text)
        }
    }

    fn letters(m: &MenuState) -> String {
        m.entries.iter().map(|e| e.letter.unwrap_or('_')).collect()
    }

    fn done(o: MenuOutcome) -> Reply {
        match o {
            MenuOutcome::Done(r) => r,
            other => panic!("not done: {other:?}"),
        }
    }

    /// The 'D' menu: skipinvert items first, then classes.
    fn drop_types() -> Vec<MenuItem> {
        let skip = |idx, ch, text| MenuItem {
            skipinvert: true,
            ..item(idx, ch, text)
        };
        vec![
            skip(0, 'A', "Auto-select every relevant item"),
            header(1, ""),
            header(2, "Item types"),
            skip(3, 'a', "All types"),
            item(4, 'b', "Weapons"),
            item(5, 'c', "Armor"),
            item(6, 'd', "Comestibles"),
        ]
    }

    #[test]
    fn menu_letters_like_tty() {
        let items = vec![
            header(0, "Weapons"),
            item(1, '\0', "a spear"),
            item(2, 'a', "a dagger"),
            item(3, '\0', "a shield"),
            header(4, "Tools"),
        ];
        let m = MenuState::new(PickHow::Any, None, &items);
        assert_eq!(letters(&m), "_bac_");
        // 52 unlettered items use up a-z and A-Z; the rest are click-only
        let many: Vec<_> = (0..54).map(|i| item(i, '\0', "x")).collect();
        let m = MenuState::new(PickHow::Any, None, &many);
        let l = letters(&m);
        assert!(l.starts_with("abc") && l.ends_with("XYZ__"), "{l}");
    }

    #[test]
    fn menu_pick_one_finishes_and_replaces_preselection() {
        let mut items = vec![item(0, 'a', "one"), item(1, 'b', "two")];
        items[0].preselected = true;
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(m.confirm(), Reply::Menu(vec![(0, -1)]));
        assert_eq!(done(m.key('b')), Reply::Menu(vec![(1, -1)]));
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(done(m.click(1)), Reply::Menu(vec![(1, -1)]));
        // Enter keeps the preselection
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(done(m.key('\n')), Reply::Menu(vec![(0, -1)]));
        // bulk commands do nothing in pick-one
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(m.key('.'), MenuOutcome::Pending);
        assert_eq!(m.key('-'), MenuOutcome::Pending);
        assert_eq!(m.confirm(), Reply::Menu(vec![(0, -1)]));
    }

    #[test]
    fn menu_item_letter_beats_command() {
        // w, ?: 'your bare hands' is '-'
        let items = vec![
            header(0, "Weapons"),
            item(1, '-', "your bare hands"),
            item(2, 'a', "a +1 spear"),
        ];
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(done(m.key('-')), Reply::Menu(vec![(1, -1)]));
        // in pick-any too: '-' toggles the item instead of clearing
        let mut m = MenuState::new(PickHow::Any, None, &items);
        m.key('a');
        m.key('-');
        assert_eq!(m.confirm(), Reply::Menu(vec![(1, -1), (2, -1)]));
    }

    #[test]
    fn menu_digit_group_accel() {
        // O, number_pad: items a-f with group keys 0-5
        let items: Vec<_> = (0..6)
            .map(|i| MenuItem {
                gch: '0' as i32 + i,
                ..item(i, (b'a' + i as u8) as char, "mode")
            })
            .collect();
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(done(m.key('3')), Reply::Menu(vec![(3, -1)]));
        // pick-any: a digit toggles its group, unless a count is being typed
        let mut m = MenuState::new(PickHow::Any, None, &items);
        m.key('2');
        assert_eq!(m.confirm(), Reply::Menu(vec![(2, -1)]));
        assert_eq!(m.typed_count(), None);
        // without group keys digits are a count
        let plain = vec![item(0, 'a', "x"), item(1, 'b', "y")];
        let mut m = MenuState::new(PickHow::Any, None, &plain);
        m.key('2');
        assert_eq!(m.typed_count(), Some(2));
    }

    #[test]
    fn menu_pick_one_group_matching_two_is_ignored() {
        let items = vec![
            MenuItem {
                gch: ')' as i32,
                ..item(0, 'a', "a spear")
            },
            MenuItem {
                gch: ')' as i32,
                ..item(1, 'b', "a dagger")
            },
            MenuItem {
                gch: '[' as i32,
                ..item(2, 'c', "a shield")
            },
        ];
        let mut m = MenuState::new(PickHow::One, None, &items);
        assert_eq!(m.key(')'), MenuOutcome::Pending);
        assert_eq!(m.confirm(), Reply::Menu(vec![]));
        assert_eq!(done(m.key('[')), Reply::Menu(vec![(2, -1)]));
        // pick-any toggles the whole group
        let mut m = MenuState::new(PickHow::Any, None, &items);
        m.key(')');
        assert_eq!(m.confirm(), Reply::Menu(vec![(0, -1), (1, -1)]));
        m.key(')');
        assert_eq!(m.confirm(), Reply::Menu(vec![]));
    }

    #[test]
    fn menu_counts() {
        let items = vec![item(0, 'a', "12 arrows"), item(1, 'b', "a bow")];
        let mut m = MenuState::new(PickHow::Any, None, &items);
        m.key('1');
        m.key('2');
        assert_eq!(m.typed_count(), Some(12));
        m.key('a');
        assert_eq!(m.typed_count(), None);
        assert_eq!(m.confirm(), Reply::Menu(vec![(0, 12)]));
        // a new count changes it; the letter alone deselects
        m.key('5');
        m.key('a');
        assert_eq!(m.confirm(), Reply::Menu(vec![(0, 5)]));
        m.key('a');
        assert_eq!(m.confirm(), Reply::Menu(vec![]));
        // leading zeros are no count; Esc drops a count, then cancels
        m.key('0');
        assert_eq!(m.typed_count(), None);
        m.key('7');
        assert_eq!(m.key('\u{1b}'), MenuOutcome::Pending);
        assert_eq!(m.typed_count(), None);
        // pick-one takes the count along
        let mut m = MenuState::new(PickHow::One, None, &items);
        m.key('3');
        assert_eq!(done(m.key('b')), Reply::Menu(vec![(1, 3)]));
    }

    #[test]
    fn menu_select_all_skips_skipinvert() {
        let mut m = MenuState::new(PickHow::Any, None, &drop_types());
        m.key('.');
        assert_eq!(m.confirm(), Reply::Menu(vec![(4, -1), (5, -1), (6, -1)]));
        m.key('-');
        assert_eq!(m.confirm(), Reply::Menu(vec![]));
        m.key(',');
        m.key('A'); // a letter still selects it
        assert_eq!(
            m.confirm(),
            Reply::Menu(vec![(0, -1), (4, -1), (5, -1), (6, -1)])
        );
        // '-' clears skipinvert items too
        m.key('-');
        assert_eq!(m.confirm(), Reply::Menu(vec![]));
    }

    #[test]
    fn menu_invert_skips_skipinvert() {
        let mut m = MenuState::new(PickHow::Any, None, &drop_types());
        m.key('b');
        m.key('@');
        assert_eq!(m.confirm(), Reply::Menu(vec![(5, -1), (6, -1)]));
        // a selected skipinvert item may be inverted off
        m.key('a');
        m.key('@');
        assert_eq!(m.confirm(), Reply::Menu(vec![(4, -1)]));
    }

    #[test]
    fn menu_escape_cancels() {
        let mut m = MenuState::new(PickHow::Any, None, &drop_types());
        m.key('b');
        assert_eq!(done(m.key('\u{1b}')), Reply::Cancel);
        let mut m = MenuState::new(PickHow::One, None, &drop_types());
        assert_eq!(done(m.key('\u{1b}')), Reply::Cancel);
        // pages and confirm
        assert_eq!(m.key('>'), MenuOutcome::PageDown);
        assert_eq!(m.key('<'), MenuOutcome::PageUp);
        assert_eq!(done(m.key(' ')), Reply::Menu(vec![]));
        let mut m = MenuState::new(PickHow::None, None, &drop_types());
        assert_eq!(done(m.key('\u{1b}')), Reply::Ack);
        assert_eq!(m.click(0), MenuOutcome::Pending);
    }

    /// xorshift64*: deterministic randomness without a dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self, n: usize) -> usize {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as usize % n
        }
    }

    #[test]
    fn menu_reply_is_always_valid() {
        let mut items = drop_types();
        items.push(MenuItem {
            gch: '1' as i32,
            preselected: true,
            ..item(7, '\0', "grouped")
        });
        items.push(MenuItem {
            gch: '1' as i32,
            ..item(8, '-', "bare hands")
        });
        items.push(header(9, "not selectable"));
        let selectable: Vec<u32> = items
            .iter()
            .filter(|i| i.selectable)
            .map(|i| i.idx as u32)
            .collect();
        let keys: Vec<char> = "aAbcdef1230.,-@<>\n \u{1b}x9".chars().collect();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..3000 {
            let how = if round % 2 == 0 {
                PickHow::Any
            } else {
                PickHow::One
            };
            let mut m = MenuState::new(how, None, &items);
            let mut reply = None;
            for _ in 0..rng.next(30) {
                let outcome = if rng.next(4) == 0 {
                    m.click(rng.next(items.len() + 1))
                } else {
                    m.key(keys[rng.next(keys.len())])
                };
                if let MenuOutcome::Done(r) = outcome {
                    reply = Some(r);
                    break;
                }
            }
            match reply.unwrap_or_else(|| m.confirm()) {
                Reply::Cancel => {}
                Reply::Menu(picked) => {
                    if how == PickHow::One {
                        assert!(picked.len() <= 1, "{picked:?}");
                    }
                    for (n, (idx, count)) in picked.iter().enumerate() {
                        assert!(selectable.contains(idx), "{picked:?}");
                        assert!(*count == -1 || *count >= 1, "{picked:?}");
                        assert!(!picked[..n].iter().any(|(i, _)| i == idx), "{picked:?}");
                    }
                }
                other => panic!("{other:?}"),
            }
        }
    }
}
