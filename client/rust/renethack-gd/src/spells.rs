//! The spells' panel: spell.c's menus parsed from their English. `+`'s
//! "Currently known spells", `Z`'s "Choose which spell to cast" and the
//! swap's "Reordering spells; swap 'a' with" list each spell under the
//! column heading "Name Level Category Fail Retention" (and "turns" in
//! debug mode): its letter, name, level, school, chance of failure and
//! how well it is remembered. `+`'s also has its "[sort spells]".
//!
//! No Godot here: the view is in `spells_view`.

use nh_protocol::MenuItem;

/// A spell's school, as spell.c names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum School {
    Attack,
    Healing,
    Divination,
    Enchantment,
    Clerical,
    Escape,
    Matter,
}

impl School {
    fn named(name: &str) -> Option<School> {
        Some(match name {
            "attack" => School::Attack,
            "healing" => School::Healing,
            "divination" => School::Divination,
            "enchantment" => School::Enchantment,
            "clerical" => School::Clerical,
            "escape" => School::Escape,
            "matter" => School::Matter,
            _ => return None,
        })
    }
}

/// How well a spell is remembered: as freshly learnt, between two
/// percentages (as precisely as the school's skill tells), or forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    Full,
    Range(u8, u8),
    Gone,
}

impl Retention {
    fn read(text: &str) -> Option<Retention> {
        let pct = |t: &str| t.strip_suffix('%')?.parse::<u8>().ok();
        match text {
            "(gone)" => Some(Retention::Gone),
            "100%" => Some(Retention::Full),
            t => {
                let (lo, hi) = t.split_once('-')?;
                Some(Retention::Range(pct(lo)?, pct(hi)?))
            }
        }
    }
}

/// A spell's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spell {
    /// The menu item's index and letter.
    pub idx: i32,
    pub letter: char,
    /// Its English name, as the cast menu picks it ("force bolt").
    pub name: String,
    pub level: i32,
    pub school: School,
    /// The chance of failure, percent.
    pub fail: u8,
    pub retention: Retention,
    /// Turns until it is forgotten (debug mode says).
    pub turns: Option<i64>,
}

/// What a pick in the menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// `Z`: casts the spell.
    Cast,
    /// `+`: begins swapping two spells' letters (the panel casts instead,
    /// unless the player reorders).
    View,
    /// The spell to swap the one of this letter with.
    Swap(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spellbook {
    pub purpose: Purpose,
    pub spells: Vec<Spell>,
    /// `+`'s "[sort spells]": its menu item's index and letter.
    pub sort: Option<(i32, char)>,
}

impl Spellbook {
    /// The spell the menu item `idx` is.
    pub fn spell(&self, idx: i32) -> Option<&Spell> {
        self.spells.iter().find(|s| s.idx == idx)
    }
}

fn purpose(title: &str) -> Option<Purpose> {
    match title {
        "Choose which spell to cast" => Some(Purpose::Cast),
        "Currently known spells" => Some(Purpose::View),
        t => {
            let rest = t.strip_prefix("Reordering spells; swap '")?;
            let mut chars = rest.chars();
            let letter = chars.next()?;
            (chars.as_str() == "' with").then_some(Purpose::Swap(letter))
        }
    }
}

/// A row: "force bolt             1   attack         0%      100%" (with
/// the turns after it in debug mode), or the same with tabs between.
fn row(text: &str, turns: bool) -> Option<(String, i32, School, u8, Retention, Option<i64>)> {
    let mut f: Vec<&str> = if text.contains('\t') {
        text.split('\t').map(str::trim).collect()
    } else {
        text.split_whitespace().collect()
    };
    let turns = if turns {
        Some(f.pop()?.parse().ok()?)
    } else {
        None
    };
    let retention = Retention::read(f.pop()?)?;
    let fail = f.pop()?.strip_suffix('%')?.parse().ok()?;
    let school = School::named(f.pop()?)?;
    let level = f.pop()?.parse().ok()?;
    if f.is_empty() {
        return None;
    }
    // a tab-separated name is one field; a padded one, words
    Some((f.join(" "), level, school, fail, retention, turns))
}

/// The spellbook in a menu: None unless it is one of spell.c's.
pub fn spellbook(title: Option<&str>, items: &[MenuItem]) -> Option<Spellbook> {
    let purpose = purpose(title?.trim())?;
    let heading = items
        .iter()
        .find(|i| !i.selectable)
        .and_then(|i| i.str.as_deref())?;
    let words: Vec<&str> = heading.split_whitespace().collect();
    if !words.starts_with(&["Name", "Level", "Category", "Fail", "Retention"]) {
        return None;
    }
    let turns = words.last() == Some(&"turns");
    let mut book = Spellbook {
        purpose,
        spells: Vec::new(),
        sort: None,
    };
    for item in items.iter().filter(|i| i.selectable) {
        let text = item.str.as_deref().unwrap_or("");
        let letter = u32::try_from(item.ch).ok().and_then(char::from_u32)?;
        if text == "[sort spells]" {
            book.sort = Some((item.idx, letter));
            continue;
        }
        let (name, level, school, fail, retention, turns) = row(text, turns)?;
        book.spells.push(Spell {
            idx: item.idx,
            letter,
            name,
            level,
            school,
            fail,
            retention,
            turns,
        });
    }
    (!book.spells.is_empty()).then_some(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(idx: i32, ch: char, text: &str, selectable: bool) -> MenuItem {
        MenuItem {
            win: 0,
            idx,
            glyph: None,
            selectable,
            ch: if ch == '\0' { 0 } else { ch as i32 },
            gch: 0,
            attr: if selectable { 0 } else { 7 },
            clr: 8,
            str: Some(text.to_string()),
            preselected: false,
            skipinvert: false,
        }
    }

    /// A Healer's `+` as the engine wrote it.
    fn healer() -> Vec<MenuItem> {
        vec![
            item(
                0,
                '\0',
                "    Name                 Level Category     Fail Retention",
                false,
            ),
            item(
                1,
                'a',
                "healing                1   healing        0%      100%",
                true,
            ),
            item(
                2,
                'b',
                "extra healing          3   healing       62%      100%",
                true,
            ),
            item(
                3,
                'c',
                "stone to flesh         3   healing       71%  76%-100%",
                true,
            ),
            item(4, '+', "[sort spells]", true),
        ]
    }

    #[test]
    fn a_healer_s_spells_read() {
        let b = spellbook(Some("Currently known spells"), &healer()).expect("a spellbook");
        assert_eq!(b.purpose, Purpose::View);
        assert_eq!(b.sort, Some((4, '+')));
        let names: Vec<&str> = b.spells.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["healing", "extra healing", "stone to flesh"]);
        let s = &b.spells[2];
        assert_eq!(
            (s.letter, s.level, s.school, s.fail),
            ('c', 3, School::Healing, 71)
        );
        assert_eq!(s.retention, Retention::Range(76, 100));
        assert_eq!(b.spell(2).map(|s| s.fail), Some(62));
        // the same rows in the cast menu, and the swap's
        let cast = spellbook(Some("Choose which spell to cast"), &healer()).unwrap();
        assert_eq!(cast.purpose, Purpose::Cast);
        let swap = spellbook(Some("Reordering spells; swap 'a' with"), &healer()).unwrap();
        assert_eq!(swap.purpose, Purpose::Swap('a'));
    }

    #[test]
    fn debug_mode_s_turns_and_tabs_read() {
        let debug = [
            item(
                0,
                '\0',
                "    Name                 Level Category     Fail Retention  turns",
                false,
            ),
            item(
                1,
                'a',
                "force bolt             1   attack         0%      100%  20000",
                true,
            ),
            item(
                2,
                'b',
                "slow monster           2   enchantment    0%    (gone)      0",
                true,
            ),
        ];
        let b = spellbook(Some("Choose which spell to cast"), &debug).unwrap();
        assert_eq!(b.spells[0].turns, Some(20000));
        assert_eq!(b.spells[1].retention, Retention::Gone);
        let tabs = [
            item(0, '\0', "Name\tLevel\tCategory\tFail\tRetention", false),
            item(1, 'a', "magic missile\t2\tattack\t15%\t91%-100%", true),
        ];
        let b = spellbook(Some("Choose which spell to cast"), &tabs).unwrap();
        assert_eq!(b.spells[0].name, "magic missile");
        assert_eq!(b.spells[0].retention, Retention::Range(91, 100));
    }

    #[test]
    fn other_menus_are_no_spellbook() {
        assert_eq!(spellbook(Some("Pick up what?"), &healer()), None);
        assert_eq!(spellbook(None, &healer()), None);
        let odd = [
            item(0, '\0', "    Name                 Level Category", false),
            item(1, 'a', "force bolt 1 attack", true),
        ];
        assert_eq!(spellbook(Some("Choose which spell to cast"), &odd), None);
    }
}
