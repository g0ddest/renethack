//! Russian input (localization phase R7). Where the engine asks for the
//! name of a thing, which it understands only in English (a wish, a
//! genocide, a polymorph, what to write with a marker, the debug mode's
//! monster to create), the player picks the thing by its name in their
//! language and the engine is sent the English. What the player engraves
//! goes in Latin letters: the engine wears an engraving away a byte at a
//! time, and a Cyrillic letter is two.
//!
//! No Godot here: the lists, their search and the wish's English; the
//! dialog is in `dialogs`.

use std::sync::OnceLock;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::names::makeplural;
use nh_i18n::{Case, Phrase};
use nh_protocol::Catalog;

use crate::i18n::{self, EngineKind, Lang};

/// What a question of the engine asks the name of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// An object, with a count, a blessing and an enchantment.
    Wish,
    /// A kind of monster (a genocide, a polymorph, a monster to create).
    Monster,
    /// A class of monsters; the engine is sent its symbol.
    MonsterClass,
    /// What to write with a magic marker: a scroll or a spellbook.
    Write(ObjClass),
}

/// The question's `Ask`, by the engine's English: None for a question
/// whose answer is free text (a name, an engraving, a number).
pub fn ask_of(query: &str) -> Option<Ask> {
    let q = query.trim_start();
    let starts = |p: &str| q.starts_with(p);
    if starts("For what do you wish") {
        Some(Ask::Wish)
    } else if starts("What type of monster do you want to genocide?")
        || starts("Become what kind of monster?")
        || starts("Create what kind of monster?")
    {
        Some(Ask::Monster)
    } else if starts("What class of monsters do you want to genocide?") {
        Some(Ask::MonsterClass)
    } else if starts("What type of scroll do you want to write?") {
        Some(Ask::Write(ObjClass::Scroll))
    } else if starts("What type of spellbook do you want to write?") {
        Some(Ask::Write(ObjClass::Spellbook))
    } else {
        None
    }
}

/// The engine asks what to engrave ("What do you want to write in the
/// dust here?", "... engrave on the floor here?").
pub fn is_engraving(query: &str) -> bool {
    let q = query.trim();
    q.starts_with("What do you want to ") && q.ends_with(" here?")
}

/// An object's class, as the glossary groups the objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjClass {
    Weapon,
    Armor,
    Ring,
    Amulet,
    Tool,
    Food,
    Potion,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
    /// Boulders and statues, the ball and the chain.
    Heavy,
}

impl ObjClass {
    /// In the order the class picker lists them (NetHack's).
    pub const ALL: [ObjClass; 13] = [
        ObjClass::Weapon,
        ObjClass::Armor,
        ObjClass::Ring,
        ObjClass::Amulet,
        ObjClass::Tool,
        ObjClass::Food,
        ObjClass::Potion,
        ObjClass::Scroll,
        ObjClass::Spellbook,
        ObjClass::Wand,
        ObjClass::Coin,
        ObjClass::Gem,
        ObjClass::Heavy,
    ];

    /// The glossary's heading of the class (`# weapon`); None for the
    /// groups no one wishes for by name (the strange object, venom, the
    /// Samurai's other names for things).
    fn from_heading(h: &str) -> Option<ObjClass> {
        Some(match h {
            "weapon" => ObjClass::Weapon,
            "armor" => ObjClass::Armor,
            "ring" => ObjClass::Ring,
            "amulet" => ObjClass::Amulet,
            "tool" => ObjClass::Tool,
            "food" => ObjClass::Food,
            "potion" => ObjClass::Potion,
            "scroll" => ObjClass::Scroll,
            "spellbook" => ObjClass::Spellbook,
            "wand" => ObjClass::Wand,
            "coin" => ObjClass::Coin,
            "gem" => ObjClass::Gem,
            "rock" | "ball" | "chain" => ObjClass::Heavy,
            _ => return None,
        })
    }

    /// The Fluent key of its name in the class picker.
    pub fn key(self) -> &'static str {
        match self {
            ObjClass::Weapon => "picker-class-weapon",
            ObjClass::Armor => "picker-class-armor",
            ObjClass::Ring => "picker-class-ring",
            ObjClass::Amulet => "picker-class-amulet",
            ObjClass::Tool => "picker-class-tool",
            ObjClass::Food => "picker-class-food",
            ObjClass::Potion => "picker-class-potion",
            ObjClass::Scroll => "picker-class-scroll",
            ObjClass::Spellbook => "picker-class-spellbook",
            ObjClass::Wand => "picker-class-wand",
            ObjClass::Coin => "picker-class-coin",
            ObjClass::Gem => "picker-class-gem",
            ObjClass::Heavy => "picker-class-heavy",
        }
    }
}

/// One thing a picker offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    /// What the engine is sent: an English name, a class's symbol.
    pub reply: String,
    /// The English the engine knows it by (dim beside the shown name).
    pub english: String,
    /// Its name as the player reads it.
    pub shown: String,
    /// The monster class's symbol (a monster, a class).
    pub symbol: Option<char>,
    /// The object's class.
    pub class: Option<ObjClass>,
    /// What a search matches: every Russian form and the English, in
    /// lower case, ё as е.
    forms: Vec<String>,
}

/// Lower case, ё as е, single spaces: how forms and searches compare.
fn fold(text: &str) -> String {
    let lower = text.to_lowercase().replace('ё', "е");
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `english`'s name as the player reads it: the lexicon's nominative in
/// Russian, else the engine's words as the language shows them.
fn shown_name(lex: &Lexicon, section: &str, english: &str) -> String {
    if i18n::lang() == Lang::Ru {
        if let Some(noun) = lex.noun(section, english) {
            return noun.singular(Case::Nom).to_string();
        }
        if let Some(text) = lex.get(section, english).and_then(|e| e.fixed()) {
            return text.to_string();
        }
    }
    i18n::engine(EngineKind::Name, english).into_owned()
}

/// The search forms of `english` in `section`: its Russian forms, the
/// English and its plural.
fn forms_of(lex: &Lexicon, section: &str, english: &str) -> Vec<String> {
    let mut out: Vec<String> = lex
        .get(section, english)
        .map(|e| e.forms().into_iter().map(fold).collect())
        .unwrap_or_default();
    for f in [fold(english), fold(&makeplural(english))] {
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

const GLOSSARY: &str = include_str!("../../../i18n/glossary.ru.toml");

/// The glossary's objects with their classes, in NetHack's order.
fn glossary_objects() -> &'static [(ObjClass, String)] {
    static OBJECTS: OnceLock<Vec<(ObjClass, String)>> = OnceLock::new();
    OBJECTS.get_or_init(|| {
        let mut out = Vec::new();
        let mut inside = false;
        let mut class = None;
        for line in GLOSSARY.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                inside = line == "[object]";
                continue;
            }
            if !inside {
                continue;
            }
            if let Some(heading) = line.strip_prefix('#') {
                class = ObjClass::from_heading(heading.trim());
                continue;
            }
            let (Some(c), Some(rest)) = (class, line.strip_prefix('"')) else {
                continue;
            };
            if let Some(end) = rest.find('"') {
                out.push((c, rest[..end].to_string()));
            }
        }
        out
    })
}

/// Every object a wish names, by class (the glossary's, NetHack's order).
pub fn objects(lex: &Lexicon) -> Vec<Pick> {
    glossary_objects()
        .iter()
        .filter(|(_, key)| lex.noun("object", key).is_some())
        .map(|(class, key)| Pick {
            reply: key.clone(),
            english: key.clone(),
            shown: shown_name(lex, "object", key),
            symbol: None,
            class: Some(*class),
            forms: forms_of(lex, "object", key),
        })
        .collect()
}

/// Every kind of monster, in NetHack's order when the engine's catalog is
/// there (with each one's class symbol and its gendered names), else the
/// lexicon's.
pub fn monsters(lex: &Lexicon, catalog: Option<&Catalog>) -> Vec<Pick> {
    let mut names: Vec<(String, Option<char>)> = Vec::new();
    match catalog {
        Some(cat) => {
            for m in &cat.monsters {
                let symbol = m.class.chars().next();
                for n in [Some(&m.name), m.male.as_ref(), m.female.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if !n.is_empty() && !names.iter().any(|(x, _)| x == n) {
                        names.push((n.clone(), symbol));
                    }
                }
            }
        }
        None => {
            let mut keys: Vec<&str> = lex.entries("monster").map(|(k, _)| k).collect();
            keys.sort_unstable();
            names.extend(keys.into_iter().map(|k| (k.to_string(), None)));
        }
    }
    names
        .into_iter()
        .map(|(name, symbol)| Pick {
            reply: name.clone(),
            shown: shown_name(lex, "monster", &name),
            forms: forms_of(lex, "monster", &name),
            english: name,
            symbol,
            class: None,
        })
        .collect()
}

/// Every class of monsters the catalog has; the engine is sent its symbol.
/// A class is found by its own name and by its members' ("кобольды").
pub fn monster_classes(lex: &Lexicon, catalog: Option<&Catalog>) -> Vec<Pick> {
    let Some(cat) = catalog else {
        return Vec::new();
    };
    let mut out: Vec<Pick> = Vec::new();
    for m in &cat.monsters {
        let Some(symbol) = m.class.chars().next() else {
            continue;
        };
        let member = forms_of(lex, "monster", &m.name);
        if let Some(p) = out.iter_mut().find(|p| p.symbol == Some(symbol)) {
            for f in member {
                if !p.forms.contains(&f) {
                    p.forms.push(f);
                }
            }
            continue;
        }
        let mut forms = forms_of(lex, "monclass", &m.class_name);
        forms.push(symbol.to_string());
        forms.extend(member);
        out.push(Pick {
            reply: symbol.to_string(),
            english: m.class_name.clone(),
            shown: shown_name(lex, "monclass", &m.class_name),
            symbol: Some(symbol),
            class: None,
            forms,
        });
    }
    out
}

/// How well `query` (folded) matches a form: 0 the form itself, 1 its
/// start, 2 the start of a word in it, 3 anywhere in it.
fn match_rank(form: &str, query: &str) -> Option<u8> {
    if form == query {
        Some(0)
    } else if form.starts_with(query) {
        Some(1)
    } else if form
        .match_indices(query)
        .any(|(i, _)| form[..i].ends_with([' ', '-', '(']))
    {
        Some(2)
    } else if form.contains(query) {
        Some(3)
    } else {
        None
    }
}

/// The picks that match `query` and are of `class` (None: any), best
/// first: by how well, then the shorter name, then alphabetically. An
/// empty query lists them all alphabetically.
pub fn search(picks: &[Pick], query: &str, class: Option<ObjClass>) -> Vec<usize> {
    let q = fold(query);
    let mut found: Vec<(u8, usize)> = picks
        .iter()
        .enumerate()
        .filter(|(_, p)| class.is_none() || p.class == class)
        .filter_map(|(i, p)| {
            if q.is_empty() {
                return Some((0, i));
            }
            p.forms
                .iter()
                .filter_map(|f| match_rank(f, &q))
                .min()
                .map(|r| (r, i))
        })
        .collect();
    found.sort_by(|(ra, a), (rb, b)| {
        let (pa, pb) = (&picks[*a], &picks[*b]);
        ra.cmp(rb)
            .then(pa.shown.chars().count().cmp(&pb.shown.chars().count()))
            .then_with(|| fold(&pa.shown).cmp(&fold(&pb.shown)))
    });
    if q.is_empty() {
        found.sort_by_key(|(_, i)| fold(&picks[*i].shown));
    }
    found.into_iter().map(|(_, i)| i).collect()
}

/// Blessed, uncursed or cursed, as a wish asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Buc {
    /// Not said: the engine decides.
    #[default]
    Any,
    Blessed,
    Uncursed,
    Cursed,
}

impl Buc {
    pub const ALL: [Buc; 4] = [Buc::Any, Buc::Blessed, Buc::Uncursed, Buc::Cursed];

    /// The engine's word for it.
    pub fn english(self) -> Option<&'static str> {
        match self {
            Buc::Any => None,
            Buc::Blessed => Some("blessed"),
            Buc::Uncursed => Some("uncursed"),
            Buc::Cursed => Some("cursed"),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Buc::Any => "wish-buc-any",
            Buc::Blessed => "wish-buc-blessed",
            Buc::Uncursed => "wish-buc-uncursed",
            Buc::Cursed => "wish-buc-cursed",
        }
    }

    /// The next one, round.
    pub fn next(self) -> Buc {
        let i = Buc::ALL.iter().position(|b| *b == self).unwrap_or(0);
        Buc::ALL[(i + 1) % Buc::ALL.len()]
    }
}

/// The wish's count, blessing and enchantment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wish {
    pub count: u32,
    pub buc: Buc,
    pub ench: Option<i32>,
}

impl Default for Wish {
    fn default() -> Wish {
        Wish {
            count: 1,
            buc: Buc::Any,
            ench: None,
        }
    }
}

/// The most a wish's count and enchantment go to in the builder.
pub const MAX_COUNT: u32 = 99;
pub const MAX_ENCH: i32 = 7;

impl Wish {
    /// The English the engine reads: "2 blessed +2 long swords".
    pub fn english(&self, name: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.count > 1 {
            parts.push(self.count.to_string());
        }
        if let Some(b) = self.buc.english() {
            parts.push(b.to_string());
        }
        if let Some(e) = self.ench {
            parts.push(format!("{e:+}"));
        }
        parts.push(if self.count > 1 {
            makeplural(name)
        } else {
            name.to_string()
        });
        parts.join(" ")
    }
}

/// What a wish typed into the search says beside the object: a leading
/// count ("2"), a blessing in either language ("благословенный",
/// "cursed"), an enchantment ("+2"); and the rest, the object's name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WishText {
    pub count: Option<u32>,
    pub buc: Option<Buc>,
    pub ench: Option<i32>,
    pub rest: String,
}

/// Every form of the blessings' adjectives, folded, with their meaning.
fn buc_words(lex: &Lexicon) -> &'static [(String, Buc)] {
    static WORDS: OnceLock<Vec<(String, Buc)>> = OnceLock::new();
    WORDS.get_or_init(|| {
        let mut out = Vec::new();
        for buc in [Buc::Blessed, Buc::Uncursed, Buc::Cursed] {
            let english = buc.english().unwrap_or_default();
            out.push((english.to_string(), buc));
            if let Some(a) = lex.adjective(english) {
                for f in a.forms() {
                    out.push((fold(f), buc));
                }
            }
        }
        out
    })
}

pub fn parse_wish(lex: &Lexicon, text: &str) -> WishText {
    let mut out = WishText::default();
    let mut rest: Vec<&str> = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        let folded = fold(w);
        if i == 0
            && let Ok(n) = w.parse::<u32>()
        {
            out.count = Some(n.clamp(1, MAX_COUNT));
            continue;
        }
        if (w.starts_with('+') || w.starts_with('-'))
            && let Ok(n) = w.parse::<i32>()
        {
            out.ench = Some(n.clamp(-MAX_ENCH, MAX_ENCH));
            continue;
        }
        if let Some((_, b)) = buc_words(lex).iter().find(|(f, _)| *f == folded) {
            out.buc = Some(*b);
            continue;
        }
        rest.push(w);
    }
    out.rest = rest.join(" ");
    out
}

/// The wish as the player reads it: the engine's English said back in
/// the language now ("2 благословенных +2 длинных меча").
pub fn wish_shown(lex: &Lexicon, english: &str) -> String {
    if i18n::lang() == Lang::Ru
        && let Some(name) = lex.parse_object(english)
    {
        return name.ru(lex).form(Case::Nom);
    }
    i18n::engine(EngineKind::Name, english).into_owned()
}

/// Latin letters for an engraving: Cyrillic transliterated (ж → zh, щ →
/// shch), any other letter beyond ASCII as `?`; "Элберет" is "Elbereth".
pub fn latin(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii() {
            out.push(c);
            continue;
        }
        let lower = c.to_lowercase().next().unwrap_or(c);
        let Some(latin) = cyrillic_latin(lower) else {
            out.push('?');
            continue;
        };
        if lower == c || latin.is_empty() {
            out.push_str(latin);
            continue;
        }
        // a capital: the whole of it in a word of capitals, else its
        // first letter
        let upper_near = |j: Option<usize>| {
            j.and_then(|j| chars.get(j))
                .is_some_and(|n| n.is_uppercase())
        };
        if upper_near(i.checked_sub(1)) || upper_near(Some(i + 1)) {
            out.push_str(&latin.to_uppercase());
        } else {
            let mut l = latin.chars();
            if let Some(first) = l.next() {
                out.extend(first.to_uppercase());
                out.push_str(l.as_str());
            }
        }
    }
    elbereth(&out)
}

/// The Latin of a lower-case Cyrillic letter (Russian, and the Ukrainian
/// and Belarusian letters besides).
fn cyrillic_latin(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' => "e",
        'ё' => "yo",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'й' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ъ' | 'ь' => "",
        'ы' => "y",
        'э' => "e",
        'ю' => "yu",
        'я' => "ya",
        'і' => "i",
        'ї' => "yi",
        'є' => "ye",
        'ґ' => "g",
        'ў' => "u",
        _ => return None,
    })
}

/// Elbereth as the engine knows it, however the player spelt it in
/// Cyrillic: the word only works in its own letters.
fn elbereth(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let bare = w.trim_matches(|c: char| !c.is_ascii_alphabetic());
            let folded = bare.to_ascii_lowercase();
            if !bare.is_empty() && matches!(folded.as_str(), "elberet" | "elberes") {
                w.replacen(bare, "Elbereth", 1)
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex() -> &'static Lexicon {
        Lexicon::ru()
    }

    fn shown_of(picks: &[Pick], found: &[usize]) -> Vec<String> {
        found.iter().map(|&i| picks[i].english.clone()).collect()
    }

    #[test]
    fn questions_say_what_they_ask_for() {
        assert_eq!(ask_of("For what do you wish?"), Some(Ask::Wish));
        assert_eq!(
            ask_of("For what do you wish (enter 'help' for assistance)?"),
            Some(Ask::Wish)
        );
        assert_eq!(
            ask_of(
                "What type of monster do you want to genocide? [enter the name of a type of monster, or '?']"
            ),
            Some(Ask::Monster)
        );
        assert_eq!(
            ask_of("Become what kind of monster? [type the name]"),
            Some(Ask::Monster)
        );
        assert_eq!(
            ask_of("What class of monsters do you want to genocide?"),
            Some(Ask::MonsterClass)
        );
        assert_eq!(
            ask_of("What type of scroll do you want to write?"),
            Some(Ask::Write(ObjClass::Scroll))
        );
        assert_eq!(ask_of("Hello stranger, who are you? -"), None);
        assert_eq!(ask_of("What do you want to name this dagger?"), None);
        assert!(is_engraving("What do you want to write in the dust here?"));
        assert!(is_engraving(
            "What do you want to add to the engraving in the floor here?"
        ));
        assert!(!is_engraving(
            "What do you want to call this dungeon level?"
        ));
    }

    #[test]
    fn every_class_has_its_objects() {
        let picks = objects(lex());
        assert!(picks.len() > 400, "{} objects", picks.len());
        for class in ObjClass::ALL {
            assert!(picks.iter().any(|p| p.class == Some(class)), "no {class:?}");
        }
        let sword = picks
            .iter()
            .find(|p| p.english == "long sword")
            .expect("long sword");
        assert_eq!(sword.class, Some(ObjClass::Weapon));
        assert!(picks.iter().all(|p| p.english != "strange object"));
        let genocide = picks
            .iter()
            .find(|p| p.english == "scroll of genocide")
            .expect("genocide");
        assert_eq!(genocide.class, Some(ObjClass::Scroll));
    }

    #[test]
    fn a_search_finds_any_form_best_first() {
        let picks = objects(lex());
        let found = search(&picks, "длинный меч", None);
        assert_eq!(shown_of(&picks, &found)[0], "long sword");
        // a case and a plural, ё typed as е
        let found = search(&picks, "длинных мечей", None);
        assert_eq!(shown_of(&picks, &found)[0], "long sword");
        let found = search(&picks, "свиток геноцида", None);
        assert_eq!(shown_of(&picks, &found)[0], "scroll of genocide");
        // the English too
        let found = search(&picks, "wand of wish", None);
        assert_eq!(shown_of(&picks, &found)[0], "wand of wishing");
        // a class narrows it
        let found = search(&picks, "", Some(ObjClass::Wand));
        assert!(
            found
                .iter()
                .all(|&i| picks[i].class == Some(ObjClass::Wand))
        );
        assert!(found.len() > 20);
    }

    #[test]
    fn kobolds_are_a_monster_and_a_class() {
        let cat: Option<&Catalog> = None;
        let picks = monsters(lex(), cat);
        let found = search(&picks, "кобольды", None);
        assert_eq!(shown_of(&picks, &found)[0], "kobold");
        let found = search(&picks, "Кобольд", None);
        assert_eq!(shown_of(&picks, &found)[0], "kobold");
    }

    #[test]
    fn a_typed_wish_fills_the_builder() {
        let w = parse_wish(lex(), "благословенный +2 длинный меч");
        assert_eq!(w.buc, Some(Buc::Blessed));
        assert_eq!(w.ench, Some(2));
        assert_eq!(w.count, None);
        assert_eq!(w.rest, "длинный меч");
        let w = parse_wish(lex(), "3 непроклятых свитка геноцида");
        assert_eq!(w.count, Some(3));
        assert_eq!(w.buc, Some(Buc::Uncursed));
        assert_eq!(w.rest, "свитка геноцида");
        let w = parse_wish(lex(), "cursed -1 ring mail");
        assert_eq!((w.buc, w.ench), (Some(Buc::Cursed), Some(-1)));
        assert_eq!(w.rest, "ring mail");
    }

    #[test]
    fn a_wish_goes_to_the_engine_in_english() {
        let w = Wish {
            count: 2,
            buc: Buc::Blessed,
            ench: Some(2),
        };
        assert_eq!(w.english("long sword"), "2 blessed +2 long swords");
        let w = Wish {
            count: 1,
            buc: Buc::Uncursed,
            ench: None,
        };
        assert_eq!(
            w.english("scroll of genocide"),
            "uncursed scroll of genocide"
        );
        assert_eq!(Wish::default().english("magic marker"), "magic marker");
        let w = Wish {
            count: 1,
            buc: Buc::Any,
            ench: Some(-1),
        };
        assert_eq!(w.english("ring mail"), "-1 ring mail");
    }

    #[test]
    fn a_wish_reads_back_in_russian() {
        crate::i18n::set_lang(Lang::Ru);
        let shown = wish_shown(lex(), "2 blessed +2 long swords");
        crate::i18n::set_lang(Lang::En);
        assert!(shown.contains("длинных меча"), "{shown}");
        assert!(shown.contains("благословенн"), "{shown}");
        assert!(shown.contains("+2"), "{shown}");
    }

    #[test]
    fn engravings_go_in_latin_letters() {
        assert_eq!(latin("Привет"), "Privet");
        assert_eq!(latin("ЖЁЛТЫЙ щит"), "ZHYOLTYY shchit");
        assert_eq!(latin("Элберет"), "Elbereth");
        assert_eq!(latin("ЭЛБЕРЕТ!"), "Elbereth!");
        assert_eq!(latin("Elbereth"), "Elbereth");
        assert_eq!(latin("x ✓ y"), "x ? y");
        assert_eq!(latin("съешь"), "sesh");
    }
}
