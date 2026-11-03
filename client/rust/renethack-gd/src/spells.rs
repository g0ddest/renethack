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

/// What a spell asks once it is picked (spell.c's `spelleffects`): nothing,
/// a direction (`getdir`, `.` for the hero), a place (`getpos`), or a
/// direction that becomes a place from Skilled on (fireball, cone of
/// cold: `throwspell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    None,
    Direction,
    Place,
    DirectionOrPlace,
}

/// One of 5.0's spells as objects.h has it: its school, level and what it
/// asks; the symbol its icon is drawn from (game-icons.net's author and
/// name, under `art/ccby/game-icons/`); and whether 5.0 leaves it out of
/// the game (`#if 0 /* DEFERRED */`: the two spheres).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Known {
    pub name: &'static str,
    pub school: School,
    pub level: i32,
    pub aim: Aim,
    pub art: &'static str,
    pub deferred: bool,
}

const fn known(
    name: &'static str,
    school: School,
    level: i32,
    aim: Aim,
    art: &'static str,
) -> Known {
    Known {
        name,
        school,
        level,
        aim,
        art,
        deferred: false,
    }
}

/// A spell 5.0 has under `#if 0 /* DEFERRED */`: a level 1 matter spell
/// that summons nothing yet.
const fn deferred(name: &'static str, art: &'static str) -> Known {
    Known {
        deferred: true,
        ..known(name, School::Matter, 1, Aim::None, art)
    }
}

/// objects.h's SPELL entries in its order, dig to freeze sphere; not
/// blank paper (no spell), nor the Book of the Dead (no SPELL).
pub const KNOWN: [Known; 43] = {
    use Aim::{Direction as D, DirectionOrPlace as DP, None as N, Place as P};
    use School::*;
    [
        known("dig", Matter, 5, D, "lorc/dig-dug"),
        known("magic missile", Attack, 2, D, "lorc/energy-arrow"),
        known("fireball", Attack, 4, DP, "lorc/burning-meteor"),
        known("cone of cold", Attack, 4, DP, "lorc/energy-breath"),
        known("sleep", Enchantment, 3, D, "delapouite/night-sleep"),
        known("finger of death", Attack, 7, D, "lorc/death-zone"),
        known("light", Divination, 1, N, "lorc/sun"),
        known("detect monsters", Divination, 1, N, "lorc/beast-eye"),
        known("healing", Healing, 1, D, "zeromancer/heart-plus"),
        known("knock", Matter, 1, D, "lorc/unlocking"),
        known("force bolt", Attack, 1, D, "lorc/punch-blast"),
        known(
            "confuse monster",
            Enchantment,
            1,
            N,
            "delapouite/knocked-out-stars",
        ),
        known("cure blindness", Healing, 2, N, "delapouite/blindfold"),
        known("drain life", Attack, 2, D, "lorc/life-tap"),
        known("slow monster", Enchantment, 2, D, "lorc/snail"),
        known("wizard lock", Matter, 2, D, "lorc/padlock"),
        known("create monster", Clerical, 2, N, "lorc/imp-laugh"),
        known("detect food", Divination, 2, N, "lorc/meat"),
        known("cause fear", Enchantment, 3, N, "lorc/terror"),
        known("clairvoyance", Divination, 3, N, "lorc/crystal-ball"),
        known("cure sickness", Healing, 3, N, "delapouite/medicines"),
        known("charm monster", Enchantment, 5, N, "lorc/puppet"),
        known("haste self", Escape, 3, N, "lorc/sprint"),
        known("detect unseen", Divination, 3, N, "delapouite/secret-door"),
        known("levitation", Escape, 4, N, "lorc/feather"),
        known("extra healing", Healing, 3, D, "sbed/health-increase"),
        known("restore ability", Healing, 4, N, "lorc/muscle-up"),
        known("invisibility", Escape, 4, N, "delapouite/invisible"),
        known(
            "detect treasure",
            Divination,
            4,
            N,
            "skoll/open-treasure-chest",
        ),
        known("remove curse", Clerical, 3, N, "skoll/breaking-chain"),
        known("magic mapping", Divination, 5, N, "lorc/treasure-map"),
        known("identify", Divination, 3, N, "lorc/magnifying-glass"),
        known("turn undead", Clerical, 6, D, "delapouite/shambling-zombie"),
        known("polymorph", Matter, 6, D, "lorc/frog"),
        known("teleport away", Escape, 6, D, "lorc/teleport"),
        known("create familiar", Clerical, 6, N, "lorc/owl"),
        known("cancellation", Matter, 7, D, "lorc/interdiction"),
        known("protection", Clerical, 1, N, "lorc/magic-shield"),
        known("jumping", Escape, 1, P, "delapouite/jump-across"),
        known("stone to flesh", Healing, 3, D, "delapouite/stone-bust"),
        known("chain lightning", Attack, 2, N, "lorc/lightning-branches"),
        deferred("flame sphere", "lorc/ball-glow"),
        deferred("freeze sphere", "lorc/frozen-orb"),
    ]
};

/// The spell named `name` (objects.h's English, "force bolt").
pub fn spell_known(name: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.name == name)
}

/// A spell's file name: "force bolt" is `force_bolt`.
pub fn slug(name: &str) -> String {
    name.replace([' ', '-'], "_")
}

/// The spell of a spellbook as the pack names it, only once its kind is
/// known: "a blessed spellbook of force bolt" is force bolt; "an orange
/// spellbook" (its kind unknown), "an orange spellbook called fire" (a
/// name the player gave it), blank paper and the Book of the Dead are
/// none, so an icon never tells what the hero has not learnt.
pub fn book_spell(item_text: &str) -> Option<&'static Known> {
    let stem = nh_world::parse_item_name(item_text).stem;
    spell_known(stem.strip_prefix("spellbook of ")?)
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

/// `+` when the hero knows a single spell: spell.c's list is then a menu
/// of no choice, which comes as a text window, its letters gone. Its lines
/// (each with whether it is a heading) made the menu's items again, the
/// spell's letter `a` (the only spell's), and the spellbook in them.
pub fn shown(title: Option<&str>, lines: &[(String, bool)]) -> Option<(Vec<MenuItem>, Spellbook)> {
    let mut letter = b'a';
    let items: Vec<MenuItem> = lines
        .iter()
        .enumerate()
        .filter(|(_, (text, _))| !text.trim().is_empty())
        .map(|(i, (text, heading))| {
            let pick = !*heading;
            let ch = if pick {
                letter += 1;
                i32::from(letter - 1)
            } else {
                0
            };
            MenuItem {
                win: 0,
                idx: i32::try_from(i).unwrap_or(0) + 1,
                glyph: None,
                selectable: pick,
                ch,
                gch: 0,
                attr: if pick { 0 } else { 1 },
                clr: 8,
                str: Some(text.clone()),
                preselected: false,
                skipinvert: false,
            }
        })
        .collect();
    let book = spellbook(title, &items)?;
    (book.purpose == Purpose::View).then_some((items, book))
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
    fn a_monk_s_one_spell_from_its_text_window() {
        // `+` with one spell: a menu of no choice, as a text window
        let lines = [
            (
                "    Name                 Level Category     Fail Retention".to_string(),
                true,
            ),
            (
                "protection             1   clerical       0%      100%".to_string(),
                false,
            ),
            (String::new(), false),
        ];
        let (items, b) = shown(Some("Currently known spells"), &lines).expect("a spellbook");
        assert_eq!(b.purpose, Purpose::View);
        assert_eq!(b.sort, None);
        let s = &b.spells[0];
        assert_eq!(
            (s.letter, s.name.as_str(), s.school, s.fail, s.retention),
            ('a', "protection", School::Clerical, 0, Retention::Full)
        );
        assert_eq!(b.spell(s.idx).map(|s| s.letter), Some('a'));
        assert_eq!(items.iter().filter(|i| i.selectable).count(), 1);
        // any other text window is not one
        assert!(shown(Some("Currently known spells"), &lines[1..]).is_none());
        assert!(shown(Some("Things that are here:"), &lines).is_none());
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
    fn every_spell_of_objects_h_once_its_symbol_its_own() {
        assert_eq!(KNOWN.len(), 43);
        let live = KNOWN.iter().filter(|k| !k.deferred).count();
        assert_eq!(live, 41, "two spheres are deferred in 5.0");
        for (i, k) in KNOWN.iter().enumerate() {
            assert!((1..=7).contains(&k.level), "{}", k.name);
            for other in &KNOWN[i + 1..] {
                assert_ne!(k.name, other.name);
                assert_ne!(k.art, other.art, "{} and {}", k.name, other.name);
            }
            assert_eq!(spell_known(k.name), Some(k));
        }
        let force = spell_known("force bolt").unwrap();
        assert_eq!(
            (force.school, force.level, force.aim),
            (School::Attack, 1, Aim::Direction)
        );
        assert_eq!(spell_known("jumping").unwrap().aim, Aim::Place);
        assert_eq!(spell_known("fireball").unwrap().aim, Aim::DirectionOrPlace);
        assert_eq!(spell_known("light").unwrap().aim, Aim::None);
        assert_eq!(spell_known("blank paper"), None);
        assert_eq!(slug("finger of death"), "finger_of_death");
    }

    #[test]
    fn a_book_tells_its_spell_only_once_known() {
        let spell = |t: &str| book_spell(t).map(|k| k.name);
        assert_eq!(
            spell("a blessed spellbook of force bolt"),
            Some("force bolt")
        );
        assert_eq!(
            spell("2 uncursed spellbooks of magic missile"),
            Some("magic missile")
        );
        assert_eq!(
            spell("a spellbook of stone to flesh (unpaid, 300 zorkmids)"),
            Some("stone to flesh")
        );
        // its kind not known: its look, never its spell
        assert_eq!(spell("an orange spellbook"), None);
        assert_eq!(spell("an orange spellbook called fire"), None);
        assert_eq!(
            spell("an orange spellbook named spellbook of fireball"),
            None
        );
        // no spell in it
        assert_eq!(spell("a spellbook of blank paper"), None);
        assert_eq!(spell("a plain spellbook"), None);
        assert_eq!(spell("the Book of the Dead"), None);
        assert_eq!(spell("a papyrus spellbook"), None);
        assert_eq!(spell("a paperback book"), None);
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
