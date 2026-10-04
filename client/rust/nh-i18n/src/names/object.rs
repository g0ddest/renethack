//! Object names as doname(), xname() and their wrappers print them:
//! "2 uncursed +1 elven daggers (weapon in hand)", "a scroll labeled
//! ELBERETH", "a potion called healing", "an uncursed figurine of a newt
//! named Bob", "the Amulet of Yendor", "a lichen corpse", "Medusa's
//! partly eaten corpse", "a tin of newt meat", "your 2 daggers".

use std::sync::Arc;

use crate::grammar::{Case, Gender, Number, counted_form};
use crate::lexicon::{Lexicon, Noun};
use crate::phrase::Phrase;

use super::Index;
use super::english::{strip_word, uncapitalized};
use super::monster::{MonsterName, link, parse_monster};
use super::ru::{Count, RuName, Tail};
use super::status::{self, Status};

/// The words doname() and xname() put before a name for an item's state,
/// longest first where one begins another.
const STATES: &[&str] = &[
    "thoroughly corroded",
    "thoroughly cracked",
    "thoroughly rotted",
    "thoroughly rusty",
    "thoroughly burnt",
    "very corroded",
    "very cracked",
    "very rotted",
    "very rusty",
    "very burnt",
    "very large",
    "partly eaten",
    "partly used",
    "uncursed",
    "blessed",
    "cursed",
    "empty",
    "trapped",
    "broken",
    "locked",
    "unlocked",
    "greased",
    "poisoned",
    "rusty",
    "burnt",
    "cracked",
    "corroded",
    "rotted",
    "fixed",
    "rustproof",
    "corrodeproof",
    "fireproof",
    "tempered",
    "rotproof",
    "diluted",
    "moist",
    "wet",
    "historic",
    "next",
    "small",
    "medium",
    "large",
    "rotten",
    "homemade",
    "unpaid",
];

/// The ways tin_details() says what a tin holds after "tin of".
const TIN_VARIETIES: [&str; 13] = [
    "soup made from",
    "french fried",
    "deep fried",
    "stir fried",
    "pickled",
    "boiled",
    "smoked",
    "dried",
    "szechuan",
    "broiled",
    "sauteed",
    "candied",
    "pureed",
];

/// Who an item belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// "your long sword".
    Your,
    /// "Asidonhopo's long sword", "the newt's long sword".
    Of(Box<MonsterName>),
}

/// What a tin holds, as far as the name says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tin {
    Spinach,
    /// "<monster> meat", or a vegetarian monster on its own; the variety
    /// ("pickled", "soup made from", "rotten", "homemade").
    Of {
        monster: MonsterName,
        meat: bool,
        variety: Option<String>,
    },
}

/// What the name names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// A name of the lexicon: its section and key.
    Thing {
        section: &'static str,
        key: String,
    },
    Corpse(MonsterName),
    Egg(MonsterName),
    Tin(Tin),
    Statue(MonsterName),
    Figurine(MonsterName),
    /// "scroll labeled X".
    Labeled(String),
}

/// An object's name, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectName {
    pub count: Count,
    pub owner: Option<Owner>,
    /// "the 2nd arrow".
    pub ordinal: Option<u32>,
    /// English keys of the "adjective" section, in order.
    pub states: Vec<String>,
    pub enchantment: Option<i32>,
    pub base: Base,
    pub called: Option<String>,
    pub named: Option<String>,
    pub containing: Option<u64>,
    /// The parenthesized groups, in order, without their parentheses.
    pub statuses: Vec<String>,
    /// A price quote: "buy 10-20 sell 5".
    pub quote: Option<String>,
    /// paydoname()'s "the contents of X" and "X and its contents".
    pub contents: Option<Contents>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    /// "the contents of your bag".
    Of,
    /// "an unpaid bag and its contents".
    And,
}

pub(super) fn parse_object(lex: &Lexicon, index: &Index, text: &str) -> Option<ObjectName> {
    let mut s = text.trim();
    let mut contents = None;
    if let Some(r) = strip_word(s, "the").and_then(|r| r.strip_prefix("contents of ")) {
        contents = Some(Contents::Of);
        s = r;
    } else if let Some(r) = s.strip_suffix(" and its contents") {
        contents = Some(Contents::And);
        s = r;
    }
    let mut quote = None;
    if s.ends_with('}')
        && let Some(open) = s.rfind(" {")
    {
        quote = Some(s[open + 2..s.len() - 1].to_string());
        s = &s[..open];
    }
    let mut statuses = Vec::new();
    while let Some(open) = last_group(s) {
        statuses.push(s[open + 2..s.len() - 1].to_string());
        s = s[..open].trim_end();
    }
    statuses.reverse();
    let mut containing = None;
    if let Some(at) = s.rfind(" containing ")
        && let Some((n, what)) = s[at + 12..].split_once(' ')
        && matches!(what, "item" | "items")
        && let Some(n) = number(n)
    {
        containing = Some(n);
        s = &s[..at];
    }

    // who owns it, how many there are
    let mut owner = None;
    if let Some(r) = strip_word(s, "your") {
        owner = Some(Owner::Your);
        s = r;
    } else if let Some((who, r)) = possessive(s)
        && !is_unique_corpse(index, who, r)
        && let Some(m) = parse_monster(lex, index, who, true)
    {
        owner = Some(Owner::Of(Box::new(m)));
        s = r;
    }
    let mut ordinal = None;
    let mut count = Count::One;
    if let Some(r) = strip_word(s, "the") {
        s = r;
        if let Some((n, r)) = ordinal_prefix(s) {
            ordinal = Some(n);
            s = r;
        }
    } else if let Some(r) = strip_word(s, "some") {
        count = Count::Some;
        s = r;
    } else if let Some(r) = strip_word(s, "an").or_else(|| strip_word(s, "a")) {
        s = r;
    } else if let Some((n, r)) = s.split_once(' ')
        && let Some(n) = number(n)
    {
        count = Count::Exactly(n);
        s = r;
    }

    // states and the name
    let mut states = Vec::new();
    let mut enchantment = None;
    let mut corpse_owner = None;
    if let Some((who, r)) = possessive(s)
        && is_unique_corpse(index, who, r)
    {
        // Medusa's partly eaten corpse; Medusa's 2 corpses
        corpse_owner = parse_monster(lex, index, who, false);
        s = r;
        if let Some((n, r)) = s.split_once(' ')
            && let Some(n) = number(n)
        {
            count = Count::Exactly(n);
            s = r;
        }
    }
    let words: Vec<&str> = s.split(' ').collect();
    let mut i = 0;
    let found = loop {
        if i >= words.len() {
            return None;
        }
        if let Some(found) = base(lex, index, &words[i..], corpse_owner.as_ref()) {
            break found;
        }
        if let Some((key, used)) = state(&words[i..]) {
            states.push(key.to_string());
            i += used;
            continue;
        }
        if let Some(e) = enchantment_of(words[i]) {
            enchantment = Some(e);
            i += 1;
            continue;
        }
        return None;
    };
    let (mut base, plural, called, named) = found;
    if plural && count == Count::One {
        count = Count::Some;
    }
    // the tin's "rotten" and "homemade" stand before "tin"
    if let Base::Tin(Tin::Of { variety, .. }) = &mut base
        && variety.is_none()
        && let Some(at) = states.iter().position(|s| s == "rotten" || s == "homemade")
    {
        *variety = Some(states.remove(at));
    }
    Some(ObjectName {
        count,
        owner,
        ordinal,
        states,
        enchantment,
        base,
        called,
        named,
        containing,
        statuses,
        quote,
        contents,
    })
}

/// A count as doname() writes it: digits only ("+1" is an enchantment).
fn number(word: &str) -> Option<u64> {
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// Where the last " (...)" group of `s` opens, if `s` ends with one.
fn last_group(s: &str) -> Option<usize> {
    if !s.ends_with(')') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return (i >= 1 && bytes[i - 1] == b' ').then(|| i - 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// "Asidonhopo's long sword" -> ("Asidonhopo", "long sword").
fn possessive(s: &str) -> Option<(&str, &str)> {
    let at = s.find("'s ").or_else(|| s.find("s' ").map(|i| i + 1))?;
    let who = &s[..at];
    let rest = s[at..].split_once(' ')?.1;
    Some((who, rest))
}

/// "Medusa's corpse": the owner is the corpse's monster.
fn is_unique_corpse(index: &Index, who: &str, rest: &str) -> bool {
    let who = strip_word(who, "the").unwrap_or(who);
    index.monsters.contains_key(who)
        && (rest.ends_with(" corpse")
            || rest.ends_with(" corpses")
            || rest == "corpse"
            || rest == "corpses")
}

/// "2nd " -> 2.
fn ordinal_prefix(s: &str) -> Option<(u32, &str)> {
    let (word, rest) = s.split_once(' ')?;
    let digits = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &word[digits.len()..];
    if digits.is_empty() || !matches!(suffix, "st" | "nd" | "rd" | "th") {
        return None;
    }
    Some((digits.parse().ok()?, rest))
}

/// "+1", "-2", "+0".
fn enchantment_of(word: &str) -> Option<i32> {
    let digits = word.strip_prefix('+').or_else(|| word.strip_prefix('-'))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// A state word or two at the start of `words`: (key, words used).
fn state(words: &[&str]) -> Option<(&'static str, usize)> {
    for &st in STATES {
        let n = st.split(' ').count();
        if words.len() > n && words[..n].join(" ") == st {
            return Some((st, n));
        }
    }
    None
}

type Found = (Base, bool, Option<String>, Option<String>);

/// The name at the start of `words`, what follows it ("called", "named")
/// and whether it was plural.
fn base(
    lex: &Lexicon,
    index: &Index,
    words: &[&str],
    corpse_owner: Option<&MonsterName>,
) -> Option<Found> {
    let text = words.join(" ");
    if let Some(m) = corpse_owner {
        return matches!(text.as_str(), "corpse" | "corpses")
            .then(|| (Base::Corpse(m.clone()), text == "corpses", None, None));
    }
    // "<monster> corpse", "<monster> egg"
    for (one, many, egg) in [("corpse", "corpses", false), ("egg", "eggs", true)] {
        for (word, plural) in [(one, false), (many, true)] {
            if let Some(m) = text.strip_suffix(word).and_then(|t| t.strip_suffix(' '))
                && let Some(mon) = parse_monster(lex, index, m, false)
            {
                let b = if egg {
                    Base::Egg(mon)
                } else {
                    Base::Corpse(mon)
                };
                return Some((b, plural, None, None));
            }
        }
    }
    // "statue of a newt", "figurine of the Oracle"
    for (lead, statue) in [("statue of ", true), ("figurine of ", false)] {
        if let Some(rest) = text.strip_prefix(lead) {
            let (who, named) = match rest.split_once(" named ") {
                Some((w, n)) => (w, Some(n.to_string())),
                None => (rest, None),
            };
            if let Some(mon) = parse_monster(lex, index, who, false) {
                let b = if statue {
                    Base::Statue(mon)
                } else {
                    Base::Figurine(mon)
                };
                return Some((b, false, None, named));
            }
        }
    }
    // "tin of newt meat", "tins of spinach", "tin of soup made from newt meat"
    for (lead, plural) in [("tin of ", false), ("tins of ", true)] {
        if let Some(rest) = text.strip_prefix(lead)
            && let Some(t) = tin(lex, index, rest)
        {
            return Some((Base::Tin(t), plural, None, None));
        }
    }
    // "scroll labeled ZELGO MER"
    for (lead, plural) in [("scroll labeled ", false), ("scrolls labeled ", true)] {
        if let Some(rest) = text.strip_prefix(lead) {
            let (label, named) = match rest.split_once(" named ") {
                Some((l, n)) => (l, Some(n.to_string())),
                None => (rest, None),
            };
            return Some((Base::Labeled(label.to_string()), plural, None, named));
        }
    }
    // a name of the lexicon, maybe called or named
    for k in (1..=index.max_words.min(words.len())).rev() {
        let candidate = words[..k].join(" ");
        let Some((section, key, plural)) = index.objects.get(&candidate) else {
            continue;
        };
        let rest = &words[k..];
        let (called, named) = match rest.first().copied() {
            None => (None, None),
            Some("called") => {
                let tail = rest[1..].join(" ");
                match tail.split_once(" named ") {
                    Some((c, n)) => (Some(c.to_string()), Some(n.to_string())),
                    None => (Some(tail), None),
                }
            }
            Some("named") => (None, Some(rest[1..].join(" "))),
            _ => continue,
        };
        let b = Base::Thing {
            section,
            key: key.clone(),
        };
        return Some((b, *plural, called, named));
    }
    None
}

/// What follows "tin of": "spinach", "newt meat", "lichen", "pickled newt
/// meat", "soup made from newt meat".
fn tin(lex: &Lexicon, index: &Index, rest: &str) -> Option<Tin> {
    if rest == "spinach" {
        return Some(Tin::Spinach);
    }
    let mut rest = rest;
    let mut variety = None;
    for v in TIN_VARIETIES {
        if let Some(r) = rest.strip_prefix(v).and_then(|r| r.strip_prefix(' ')) {
            variety = Some(v.to_string());
            rest = r;
            break;
        }
    }
    let (who, meat) = match rest.strip_suffix(" meat") {
        Some(w) => (w, true),
        None => (rest, false),
    };
    let monster = parse_monster(lex, index, who, false)?;
    Some(Tin::Of {
        monster,
        meat,
        variety,
    })
}

impl ObjectName {
    /// The name in Russian.
    pub fn ru(&self, lex: &Lexicon) -> RuName {
        let thing = |section: &str, key: &str| -> Arc<Noun> {
            lex.noun(section, key)
                .cloned()
                .unwrap_or_else(|| Arc::new(Noun::fixed(key, Gender::Masc)))
        };
        let genitive = |m: &MonsterName| Tail::Genitive(Box::new(m.ru(lex)));
        let mut name = match &self.base {
            Base::Thing { section, key } => RuName::new(thing(section, key)),
            Base::Corpse(m) => with_tail(RuName::new(thing("object", "corpse")), genitive(m)),
            Base::Egg(m) => with_tail(RuName::new(thing("object", "egg")), genitive(m)),
            Base::Statue(m) => with_tail(RuName::new(thing("object", "statue")), genitive(m)),
            Base::Figurine(m) => with_tail(RuName::new(thing("object", "figurine")), genitive(m)),
            Base::Tin(t) => with_tail(
                RuName::new(thing("object", "tin")),
                Tail::Genitive(Box::new(tin_ru(lex, t))),
            ),
            Base::Labeled(label) => {
                let label = lex
                    .get("label", label)
                    .and_then(|e| e.fixed())
                    .unwrap_or(label);
                with_tail(
                    RuName::new(thing("class", "scroll")),
                    Tail::Text(format!("{} {label}", link(lex, "labeled"))),
                )
            }
        };
        if let Some(unit) = &name.head.unit {
            name.unit = lex.noun("part", unit).cloned();
        }
        if let Some(Owner::Of(m)) = &self.owner {
            name.tails.insert(0, Tail::Genitive(Box::new(m.ru(lex))));
        }
        if let Some(c) = &self.called {
            name.tails
                .push(Tail::Text(format!("{} «{c}»", link(lex, "called"))));
        }
        if let Some(n) = &self.named {
            name.tails.push(Tail::Text(format!(
                "{} {}",
                link(lex, "named"),
                personal_name(lex, n)
            )));
        }
        if let Some(n) = self.containing
            && let Some(item) = lex.noun("part", "item")
        {
            let (num, case) = counted_form(n, Case::Ins, false);
            let word = match num {
                Number::Sing => item.singular(case),
                Number::Plur => item.plural(case),
            };
            name.tails.push(Tail::Text(format!("с {n} {word}")));
        }
        name.adjectives = self
            .states
            .iter()
            .filter_map(|s| lex.adjective(s).cloned())
            .collect();
        name.enchantment = self.enchantment;
        name.count = self.count;
        name.ordinal = self.ordinal;
        if self.owner == Some(Owner::Your) {
            name.possessive = lex.adjective("your").cloned();
        }
        let index = lex.index();
        name.statuses = self
            .statuses
            .iter()
            .map(|g| match status::parse(lex, index, g) {
                Status::English(e) => {
                    look(lex, &self.base, &e).map_or(Status::English(e), Status::Text)
                }
                s => s,
            })
            .collect::<Vec<Status>>();
        name.quote = self.quote.as_ref().map(|q| quote_ru(lex, q));
        match self.contents {
            None => name,
            Some(Contents::Of) => {
                let mut c = RuName::new(thing("part", "contents"));
                c.tails.push(Tail::Genitive(Box::new(name)));
                c
            }
            Some(Contents::And) => {
                let its = match (name.number(), name.gender()) {
                    (Number::Plur, _) => "их",
                    (_, Gender::Fem) => "её",
                    _ => "его",
                };
                let contents = thing("part", "contents");
                name.tails.push(Tail::Text(format!(
                    "и {its} {}",
                    contents.singular(Case::Nom)
                )));
                name
            }
        }
    }
}

/// The classes whose look obj_typename() writes without the class word:
/// "potion of healing (bubbly)".
const LOOK_CLASSES: [&str; 8] = [
    "potion",
    "scroll",
    "wand",
    "ring",
    "spellbook",
    "amulet",
    "gem",
    "stone",
];

/// The look obj_typename() puts in parentheses after a known name, in the
/// nominative: "potion of healing (bubbly)" -> пузырящееся, "elven shield
/// (blue and green shield)" -> сине-зелёный щит, "scroll of identify
/// (KIRJE)" -> KIRJE.
fn look(lex: &Lexicon, base: &Base, group: &str) -> Option<String> {
    if !matches!(base, Base::Thing { .. }) {
        return None;
    }
    if let Some(label) = lex.get("label", group).and_then(|e| e.fixed()) {
        return Some(label.to_string());
    }
    for class in LOOK_CLASSES {
        if let Some(n) = lex.noun("appearance", &format!("{group} {class}")) {
            let whole = n.singular(Case::Nom);
            let class_ru = lex
                .noun("class", class)
                .map_or("", |c| c.singular(Case::Nom));
            // пузырящееся (зелье), (книга заклинаний) с загнутыми уголками
            let bare = whole
                .strip_suffix(class_ru)
                .map(str::trim_end)
                .or_else(|| whole.strip_prefix(class_ru).map(str::trim_start))
                .filter(|b| !b.is_empty() && !class_ru.is_empty());
            return Some(bare.unwrap_or(whole).to_string());
        }
    }
    for key in [group.to_string(), format!("pair of {group}")] {
        if let Some(n) = lex
            .noun("appearance", &key)
            .or_else(|| lex.noun("object", &key))
        {
            return Some(n.singular(Case::Nom).to_string());
        }
    }
    None
}

fn with_tail(mut name: RuName, tail: Tail) -> RuName {
    name.tails.push(tail);
    name
}

/// What a tin holds, in Russian: мяса тритона, маринованного мяса тритона,
/// супа из мяса тритона, лишайника, шпината.
fn tin_ru(lex: &Lexicon, t: &Tin) -> RuName {
    let noun = |section: &str, key: &str| -> Arc<Noun> {
        lex.noun(section, key)
            .cloned()
            .unwrap_or_else(|| Arc::new(Noun::fixed(key, Gender::Masc)))
    };
    match t {
        Tin::Spinach => RuName::new(noun("part", "spinach")),
        Tin::Of {
            monster,
            meat,
            variety,
        } => {
            let mut content = if *meat {
                with_tail(
                    RuName::new(noun("part", "meat")),
                    Tail::Genitive(Box::new(monster.ru(lex))),
                )
            } else {
                monster.ru(lex)
            };
            match variety.as_deref() {
                Some("soup made from") => {
                    let mut soup = RuName::new(noun("part", "soup"));
                    soup.tails
                        .push(Tail::Text(format!("из {}", content.form(Case::Gen))));
                    soup
                }
                Some(v) => {
                    if let Some(a) = lex.adjective(v) {
                        content.adjectives.push(a.clone());
                    }
                    content
                }
                None => content,
            }
        }
    }
}

/// A personal name: an artifact's in Russian ("Excalibur", "the Orb of
/// Detection"), any other as it is.
fn personal_name(lex: &Lexicon, name: &str) -> String {
    let bare = strip_word(name, "the").unwrap_or(name);
    for key in [bare.to_string(), uncapitalized(bare)] {
        if let Some(n) = lex.noun("artifact", &key) {
            return n.singular(Case::Nom).to_string();
        }
    }
    name.to_string()
}

/// "buy 10-20 sell 5" -> "покупка 10-20 продажа 5".
fn quote_ru(lex: &Lexicon, q: &str) -> String {
    q.split(' ')
        .map(|w| lex.get("status", w).and_then(|e| e.fixed()).unwrap_or(w))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ObjectName {
        Lexicon::ru()
            .parse_object(text)
            .unwrap_or_else(|| panic!("{text:?} does not parse"))
    }

    fn ru(text: &str, case: Case) -> String {
        parse(text).ru(Lexicon::ru()).form(case)
    }

    #[test]
    fn the_strict_order_of_doname() {
        let n = parse("2 uncursed +1 elven daggers (weapon in hand)");
        assert_eq!(n.count, Count::Exactly(2));
        assert_eq!(n.states, ["uncursed"]);
        assert_eq!(n.enchantment, Some(1));
        assert_eq!(
            n.base,
            Base::Thing {
                section: "object",
                key: "elven dagger".into()
            }
        );
        assert_eq!(n.statuses, ["weapon in hand"]);
        assert_eq!(
            ru("2 uncursed +1 elven daggers (weapon in hand)", Case::Nom),
            "2 непроклятых +1 эльфийских кинжала (оружие в руке)"
        );
        assert_eq!(
            ru(
                "an uncursed very rusty greased +0 long sword (weapon in right hand)",
                Case::Acc
            ),
            "непроклятый очень ржавый смазанный +0 длинный меч (оружие в правой руке)"
        );
    }

    #[test]
    fn looks_labels_called_and_named() {
        assert_eq!(
            ru("a scroll labeled ELBERETH", Case::Nom),
            "свиток с надписью ELBERETH"
        );
        assert_eq!(
            ru("a potion called healing", Case::Gen),
            "зелья под названием «healing»"
        );
        assert_eq!(ru("3 bubbly potions", Case::Nom), "3 пузырящихся зелья");
        assert_eq!(
            ru("an uncursed figurine of a newt named Bob", Case::Ins),
            "непроклятой статуэткой тритона по имени Bob"
        );
        assert_eq!(
            ru("a +0 long sword named Excalibur", Case::Nom),
            "+0 длинный меч по имени Экскалибур"
        );
        assert_eq!(
            ru(
                "the blessed rustproof +5 Excalibur (weapon in hand)",
                Case::Dat
            ),
            "благословенному нержавеющему +5 Экскалибуру (оружие в руке)"
        );
        assert_eq!(ru("the Amulet of Yendor", Case::Acc), "Амулет Йендора");
        assert_eq!(ru("a pair of combat boots", Case::Nom), "армейские ботинки");
        assert_eq!(
            ru("2 pair of combat boots", Case::Gen),
            "2 пар армейских ботинок"
        );
    }

    #[test]
    fn things_made_of_monsters() {
        assert_eq!(ru("a lichen corpse", Case::Nom), "труп лишайника");
        assert_eq!(
            ru("2 uncursed partly eaten newt corpses", Case::Nom),
            "2 непроклятых надкусанных трупа тритона"
        );
        assert_eq!(
            ru("Medusa's partly eaten corpse", Case::Acc),
            "надкусанный труп Медузы"
        );
        assert_eq!(ru("the Oracle's corpse", Case::Nom), "труп Оракула");
        assert_eq!(ru("a tin of newt meat", Case::Nom), "банка мяса тритона");
        assert_eq!(
            ru("a tin of pickled newt meat", Case::Nom),
            "банка маринованного мяса тритона"
        );
        assert_eq!(ru("a tin of lichen", Case::Nom), "банка лишайника");
        assert_eq!(ru("a tin of spinach", Case::Nom), "банка шпината");
        assert_eq!(
            ru("a rotten tin of newt meat", Case::Nom),
            "банка тухлого мяса тритона"
        );
        assert_eq!(
            ru("an uncursed empty tin", Case::Nom),
            "непроклятая пустая банка"
        );
        assert_eq!(
            ru("a tin of soup made from newt meat", Case::Nom),
            "банка супа из мяса тритона"
        );
        assert_eq!(ru("a cockatrice egg", Case::Gen), "яйца кокатрикса");
        assert_eq!(
            ru("a historic statue of Medusa", Case::Nom),
            "историческая статуя Медузы"
        );
        assert_eq!(
            ru("a small glob of gray ooze", Case::Nom),
            "маленький комок серой слизи"
        );
    }

    #[test]
    fn owners_wrappers_and_ordinals() {
        assert_eq!(ru("your 2 daggers", Case::Nom), "ваши 2 кинжала");
        assert_eq!(
            ru("Asidonhopo's long sword", Case::Nom),
            "длинный меч Asidonhopo"
        );
        assert_eq!(
            ru("the newt's long sword", Case::Nom),
            "длинный меч тритона"
        );
        assert_eq!(ru("the 2nd arrow", Case::Nom), "2-я стрела");
        assert_eq!(
            ru("an uncursed bag containing 3 items", Case::Nom),
            "непроклятая сумка с 3 предметами"
        );
        assert_eq!(
            ru("the contents of your bag", Case::Nom),
            "содержимое вашей сумки"
        );
        assert_eq!(
            ru("an unpaid bag and its contents", Case::Nom),
            "неоплаченная сумка и её содержимое"
        );
        assert_eq!(
            ru("a +0 dagger (alternate weapon; not wielded)", Case::Nom),
            "+0 кинжал (запасное оружие; не в руках)"
        );
    }

    #[test]
    fn looks_in_the_discoveries() {
        assert_eq!(
            ru("potion of healing (bubbly)", Case::Nom),
            "зелье лечения (пузырящееся)"
        );
        assert_eq!(
            ru("elven shield (blue and green shield)", Case::Nom),
            "эльфийский щит (сине-зелёный щит)"
        );
        assert_eq!(
            ru("pair of elven boots (snow boots)", Case::Nom),
            "эльфийские сапоги (зимние сапоги)"
        );
        assert_eq!(
            ru("scroll of identify (KIRJE)", Case::Nom),
            "свиток опознания (KIRJE)"
        );
    }

    #[test]
    fn what_no_lexicon_knows_does_not_parse() {
        let lex = Lexicon::ru();
        assert!(lex.parse_object("2 uncursed slices of pizza").is_none());
        assert!(lex.parse_object("You feel better.").is_none());
    }
}
