//! The English names the engine prints, parsed, and their Russian.
//!
//! An object name follows doname()'s order: `[count | article | your |
//! owner's] [states] [+N] name [called X] [named Y] [containing N items]
//! [(status)...]`, where the name is one the lexicon knows ("long sword",
//! "bubbly potion", "scroll labeled ZELGO MER") or one the engine builds
//! from a monster ("newt corpse", "statue of a newt", "tin of newt meat").
//! A monster name follows x_monnam(): `[the | a | your] [adjectives]
//! kind`, the kind being a monster of the lexicon, a name ("Fido"), "dog
//! called Fido", "Asidonhopo the invisible shopkeeper", "priest of Mitra",
//! "Bob's ghost", or a pronoun ("it", "you").
//!
//! [`Lexicon`] implements [`Names`]: the translator hands it a name as it
//! stood in a message and gets a [`RuName`] it can put in any case.

mod english;
mod monster;
mod object;
mod ru;
mod status;

use std::collections::HashMap;
use std::sync::Arc;

pub use english::makeplural;
pub use monster::{MonsterKind, MonsterName};
pub use object::{Base, ObjectName, Owner, Tin};
pub use ru::{Count, RuName, Tail};
pub use status::{Hand, Slots, Status};

use crate::grammar::Gender;
use crate::lexicon::{Lexicon, Noun};
use crate::phrase::{NameKind, Names, Phrase};

/// The sections an object's name comes from, in the order a name is looked
/// up.
const OBJECT_SECTIONS: [&str; 4] = ["object", "appearance", "class", "artifact"];

/// The sections of words: what a message names that is neither a thing
/// nor a creature, in the order a word is looked up.
const WORD_SECTIONS: [&str; 22] = [
    "terrain",
    "trap",
    "place",
    "role",
    "rank",
    "god",
    "race",
    "alignment",
    "monclass",
    "objclass",
    "heading",
    "condition",
    "skill",
    "spell",
    "shop",
    "gender",
    "term",
    "currency",
    "color",
    "bodypart",
    "adjective",
    "label",
];

/// The lookups the parsers need, made once per lexicon.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// Every object name, singular and plural: (section, key, plural).
    objects: HashMap<String, (&'static str, String, bool)>,
    /// Every monster name, singular and plural: (key, plural).
    monsters: HashMap<String, (String, bool)>,
    /// Ranks and roles by their lower-case spelling (a player monster's
    /// title: "the stripling").
    titles: HashMap<String, (&'static str, String)>,
    bodypart_plurals: HashMap<String, String>,
    currency_plurals: HashMap<String, String>,
    /// The longest object name, in words.
    max_words: usize,
}

impl Index {
    pub fn new(lex: &Lexicon) -> Index {
        let mut objects = HashMap::new();
        let mut max_words = 1;
        for section in OBJECT_SECTIONS {
            for (key, entry) in lex.entries(section) {
                if entry.noun().is_none() {
                    continue;
                }
                max_words = max_words.max(key.split(' ').count());
                objects
                    .entry(key.to_string())
                    .or_insert((section, key.to_string(), false));
                let many = makeplural(key);
                if many != key {
                    objects
                        .entry(many)
                        .or_insert((section, key.to_string(), true));
                }
            }
        }
        let mut monsters = HashMap::new();
        for (key, entry) in lex.entries("monster") {
            if entry.noun().is_none() {
                continue;
            }
            monsters
                .entry(key.to_string())
                .or_insert((key.to_string(), false));
            let many = makeplural(key);
            if many != key {
                monsters.entry(many).or_insert((key.to_string(), true));
            }
        }
        let mut titles = HashMap::new();
        for section in ["rank", "role", "monster"] {
            for (key, entry) in lex.entries(section) {
                if entry.noun().is_some() {
                    titles
                        .entry(key.to_lowercase())
                        .or_insert((section, key.to_string()));
                }
            }
        }
        Index {
            objects,
            monsters,
            titles,
            bodypart_plurals: status::plurals(lex, "bodypart"),
            currency_plurals: status::plurals(lex, "currency"),
            max_words,
        }
    }
}

impl Lexicon {
    fn index(&self) -> &Index {
        self.names_index.get_or_init(|| Index::new(self))
    }

    /// An object's name as the engine printed it, parsed: None when it is
    /// not one the lexicon knows (a player's fruit, a text that is no name).
    pub fn parse_object(&self, english: &str) -> Option<ObjectName> {
        object::parse_object(self, self.index(), english)
    }

    /// A monster's name as the engine printed it, parsed. A name the
    /// lexicon does not know is taken for a personal name ("Fido") unless
    /// `strict`.
    pub fn parse_monster(&self, english: &str, strict: bool) -> Option<MonsterName> {
        monster::parse_monster(self, self.index(), english, !strict)
    }

    /// A closed-set word ("fountain", "the Astral Plane", "Valkyrie",
    /// "lawful") as a name.
    pub fn word(&self, english: &str) -> Option<RuName> {
        let candidates = [
            english.to_string(),
            english::uncapitalized(english),
            english::strip_word(english, "the")
                .map(str::to_string)
                .unwrap_or_default(),
        ];
        for text in candidates.iter().filter(|t| !t.is_empty()) {
            for section in WORD_SECTIONS {
                let Some(entry) = self.get(section, text) else {
                    continue;
                };
                if let Some(n) = entry.noun() {
                    return Some(RuName::new(n.clone()));
                }
                if let Some(a) = entry.adjective() {
                    return Some(RuName::new(Arc::new(a.as_noun(Gender::Masc))));
                }
                if let Some(t) = entry.fixed() {
                    return Some(RuName::new(Arc::new(Noun::fixed(t, Gender::Masc))));
                }
            }
        }
        None
    }
}

impl Names for Lexicon {
    fn parse(&self, kind: NameKind, english: &str) -> Option<Box<dyn Phrase>> {
        let english = english.trim();
        if english.is_empty() {
            return None;
        }
        let object = || self.parse_object(english).map(|o| o.ru(self));
        let monster = |lenient| {
            monster::parse_monster(self, self.index(), english, lenient).map(|m| m.ru(self))
        };
        let found = match kind {
            NameKind::Object => object(),
            NameKind::Monster => monster(true),
            NameKind::Word => self.word(english),
            NameKind::Any => object()
                .or_else(|| monster(false))
                .or_else(|| self.word(english)),
        };
        found.map(|n| Box::new(n) as Box<dyn Phrase>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::{Case, Number};

    fn parse(kind: NameKind, english: &str) -> Box<dyn Phrase> {
        Lexicon::ru()
            .parse(kind, english)
            .unwrap_or_else(|| panic!("{english:?} does not parse"))
    }

    #[test]
    fn the_translator_gets_phrases_for_every_kind() {
        let p = parse(NameKind::Monster, "The newt");
        assert_eq!(p.form(Case::Acc), "тритона");
        let p = parse(NameKind::Object, "a +1 long sword (weapon in hand)");
        assert_eq!(p.form(Case::Ins), "+1 длинным мечом (оружие в руке)");
        let p = parse(NameKind::Word, "fountain");
        assert_eq!(p.form(Case::Prep), "фонтане");
        let p = parse(NameKind::Word, "the Astral Plane");
        assert_eq!(p.form(Case::Prep), "Астральном плане");
        let p = parse(NameKind::Any, "2 uncursed potions of healing");
        assert_eq!(
            (p.form(Case::Nom).as_str(), p.number()),
            ("2 непроклятых зелья лечения", Number::Plur)
        );
        assert!(
            Lexicon::ru()
                .parse(NameKind::Any, "You feel better.")
                .is_none()
        );
    }

    #[test]
    fn a_place_takes_its_locative() {
        let p = parse(NameKind::Word, "ice");
        assert_eq!(
            (p.form(Case::Loc), p.form(Case::Prep)),
            ("льду".into(), "льде".into())
        );
        let p = parse(NameKind::Word, "lowered drawbridge");
        assert_eq!(p.form(Case::Loc), "опущенном подъёмном мосту");
        let p = parse(NameKind::Object, "an uncursed grappling hook");
        assert_eq!(p.form(Case::Loc), "непроклятом абордажном крюку");
        let p = parse(NameKind::Object, "2 grappling hooks");
        assert_eq!(p.form(Case::Loc), "2 абордажных крюках");
        let p = parse(NameKind::Monster, "the newt");
        assert_eq!(p.form(Case::Loc), "тритоне");
    }

    #[test]
    fn an_unknown_monster_is_a_name_only_when_one_is_expected() {
        let lex = Lexicon::ru();
        assert_eq!(parse(NameKind::Monster, "Fido").form(Case::Dat), "Fido");
        assert!(lex.parse(NameKind::Any, "Fido").is_none());
    }
}
