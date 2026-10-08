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
mod feature;
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
use crate::lexicon::{Adjective, Lexicon, Noun};
use crate::phrase::{NameKind, Names, Phrase};

/// The sections an object's name comes from, in the order a name is looked
/// up.
const OBJECT_SECTIONS: [&str; 4] = ["object", "appearance", "class", "artifact"];

/// The sections of words: what a message names that is neither a thing
/// nor a creature, in the order a word is looked up.
const WORD_SECTIONS: [&str; 27] = [
    "terrain",
    "surface",
    "liquid",
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
    "greeting",
    "note",
    "word",
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
    /// The plurals of the words: "feet", "hyphae" -> (section, key).
    word_plurals: HashMap<String, (&'static str, String)>,
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
        let mut word_plurals = HashMap::new();
        for section in WORD_SECTIONS {
            for (key, entry) in lex.entries(section) {
                let many = makeplural(key);
                if entry.noun().is_some_and(|n| n.has_plural()) && many != key {
                    word_plurals
                        .entry(many)
                        .or_insert((section, key.to_string()));
                }
            }
        }
        Index {
            objects,
            monsters,
            titles,
            word_plurals,
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
    /// "lawful", "feet", "your feet") or a feature of the map as the
    /// engine describes it ("a staircase up to level 3", "altar to Tyr
    /// (lawful)", "thin ice") as a name.
    pub fn word(&self, english: &str) -> Option<RuName> {
        let text = english.trim();
        if let Some(rest) = english::strip_word(text, "your") {
            let mut name = self.word(rest)?;
            name.possessive = self.adjective("your").cloned();
            name.own = self.adjective("own").cloned();
            return Some(name);
        }
        let bare = ["the", "an", "a"]
            .iter()
            .find_map(|a| english::strip_word(text, a))
            .unwrap_or(text);
        self.word_entry(text)
            .or_else(|| self.word_entry(bare))
            .or_else(|| self.word_plural(bare))
            .or_else(|| feature::parse(self, bare))
            .or_else(|| self.juice(bare))
            .or_else(|| self.owned(text))
    }

    /// A word after its owner, as s_suffix() and body_part() make it:
    /// "the newt's face" is лицо тритона.
    fn owned(&self, text: &str) -> Option<RuName> {
        let (owner, rest) = text.rsplit_once("'s ").or_else(|| {
            text.rsplit_once("s' ")
                .map(|(o, r)| (&text[..o.len() + 1], r))
        })?;
        let mut name = self.word(rest)?;
        let owner = self.parse_monster(owner, false)?;
        name.tails.push(Tail::Genitive(Box::new(owner.ru(self))));
        Some(name)
    }

    /// The juice of a fruit, as fruitname(TRUE) names it: "slime mold
    /// juice". A fruit the player named stays as typed.
    fn juice(&self, text: &str) -> Option<RuName> {
        // objects.h: a fruit's name is under PL_FSIZ characters
        const PL_FSIZ: usize = 32;
        let fruit = text.strip_suffix(" juice").filter(|f| !f.is_empty())?;
        if fruit.len() >= PL_FSIZ {
            return None;
        }
        let mut name = RuName::new(self.noun("word", "juice")?.clone());
        name.tails.push(match self.parse_object(fruit) {
            Some(o) => Tail::Genitive(Box::new(o.ru(self))),
            None => Tail::Text(fruit.to_string()),
        });
        Some(name)
    }

    /// One name of `kind`.
    fn one(&self, kind: NameKind, english: &str) -> Option<RuName> {
        let object = || self.parse_object(english).map(|o| o.ru(self));
        let monster = |lenient| {
            monster::parse_monster(self, self.index(), english, lenient).map(|m| m.ru(self))
        };
        match kind {
            NameKind::Object => object(),
            // an article marks a common noun the monsters lack ("a cat"
            // in Schroedinger's box), not a name someone was given
            NameKind::Monster => monster(false)
                .or_else(|| {
                    ["the", "an", "a"]
                        .iter()
                        .any(|a| english::strip_word(english, a).is_some())
                        .then(|| self.word(english))
                        .flatten()
                })
                .or_else(|| monster(true)),
            NameKind::Word => self.word(english),
            NameKind::Any => object()
                .or_else(|| monster(false))
                .or_else(|| self.word(english)),
        }
    }

    /// Names parted by "or", as farlook lists what a symbol may be
    /// (pager.c append_str()): "a doorway or the floor of a room or ice".
    /// Each is a name of any kind, the longest first ("a dog or other
    /// canine" is one).
    fn alternatives(&self, english: &str) -> Option<RuName> {
        const OR: &str = " or ";
        let cuts: Vec<usize> = english.match_indices(OR).map(|(i, _)| i).collect();
        cuts.iter().rev().find_map(|&i| {
            let (head, rest) = (&english[..i], &english[i + OR.len()..]);
            let mut first = self.one(NameKind::Any, head)?;
            let next = self
                .one(NameKind::Any, rest)
                .or_else(|| self.alternatives(rest))?;
            first.tails.push(Tail::Or(Box::new(next)));
            Some(first)
        })
    }

    /// Two names joined by "and": "you and your pony" (a rider knocked
    /// back with the steed).
    fn pair(&self, english: &str) -> Option<RuName> {
        let (one, other) = english.split_once(" and ")?;
        let mut first = self.one(NameKind::Any, one)?;
        let second = self.one(NameKind::Any, other)?;
        first.tails.push(Tail::And(Box::new(second)));
        Some(first)
    }

    /// The entry of a word section, as spelled or with its first letter
    /// changed ("the Gnomish Mines" for "The Gnomish Mines").
    fn word_entry(&self, text: &str) -> Option<RuName> {
        let spellings = [
            text.to_string(),
            english::uncapitalized(text),
            english::capitalized(text),
        ];
        for t in spellings.iter().filter(|t| !t.is_empty()) {
            for section in WORD_SECTIONS {
                let Some(entry) = self.get(section, t) else {
                    continue;
                };
                let mut name = if let Some(n) = entry.noun() {
                    RuName::new(n.clone())
                } else if let Some(a) = entry.adjective() {
                    RuName::new(Arc::new(a.as_noun(Gender::Masc)))
                } else if let Some(t) = entry.fixed() {
                    RuName::new(Arc::new(Noun::fixed(t, Gender::Masc)))
                } else {
                    continue;
                };
                name.as_adjective = self.adjective_reading(t);
                if matches!(section, "role" | "rank") {
                    name.female = self.noun("female", t).cloned();
                }
                return Some(name);
            }
        }
        None
    }

    /// The word as an adjective, in whatever section has it so: "lawful",
    /// "elven", and "human" (a race as a noun, an adjective in races[].adj).
    pub(super) fn adjective_reading(&self, key: &str) -> Option<Arc<Adjective>> {
        ["adjective", "race", "alignment", "color", "gender"]
            .iter()
            .find_map(|s| self.get(s, key)?.adjective().cloned())
    }

    /// A word in the plural makeplural() gives it: "feet", "hyphae".
    fn word_plural(&self, text: &str) -> Option<RuName> {
        let (section, key) = self.index().word_plurals.get(text)?;
        let mut name = RuName::new(self.noun(section, key)?.clone());
        name.count = Count::Some;
        Some(name)
    }
}

impl Names for Lexicon {
    fn parse(&self, kind: NameKind, english: &str) -> Option<Box<dyn Phrase>> {
        let english = english.trim();
        if english.is_empty() {
            return None;
        }
        // a monster read leniently is any name at all: no alternatives
        let found = self.one(kind, english).or_else(|| {
            matches!(kind, NameKind::Word | NameKind::Any)
                .then(|| self.alternatives(english).or_else(|| self.pair(english)))
                .flatten()
        });
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
    fn words_agree_with_a_heroine() {
        let lex = Lexicon::ru();
        let her = |w: &str| {
            lex.word(w)
                .unwrap()
                .agreeing(Gender::Fem, Number::Sing, Case::Nom)
        };
        assert_eq!(her("lawful"), "законопослушная");
        assert_eq!(her("elven"), "эльфийская");
        assert_eq!(her("human"), "человеческая");
        assert_eq!(her("Healer"), "Целительница");
        assert_eq!(her("Archeologist"), "Женщина-археолог");
        let axe = Lexicon::ru().parse(NameKind::Object, "your axe").unwrap();
        assert_eq!(axe.own(Case::Ins), "своим топором");
        assert_eq!(axe.form(Case::Ins), "вашим топором");
        let him = lex.word("Healer").unwrap();
        assert_eq!(
            him.agreeing(Gender::Masc, Number::Sing, Case::Ins),
            "Целителем"
        );
    }

    #[test]
    fn a_word_takes_your_before_it() {
        let lex = Lexicon::ru();
        let feet = lex.word("your feet").unwrap();
        assert_eq!(feet.form(Case::Nom), "ваши ноги");
        assert_eq!(feet.own(Case::Acc), "свои ноги");
        assert_eq!(feet.number(), Number::Plur);
        assert_eq!(
            lex.word("Your hand").unwrap().form(Case::Ins),
            "вашей рукой"
        );
        assert!(lex.word("your no such thing").is_none());
    }

    #[test]
    fn the_names_the_engine_passes_without_their_unit() {
        // gloves_simple_name(), suit_simple_name(), b_trapped()'s item, zap.c
        for (english, case, russian) in [
            ("your gauntlets", Case::Nom, "ваши рукавицы"),
            ("gloves", Case::Gen, "перчаток"),
            ("your boots", Case::Acc, "ваши сапоги"),
            ("dragon scales", Case::Nom, "драконья чешуя"),
            ("a scroll of mail", Case::Acc, "свиток почты"),
            ("The door", Case::Nom, "дверь"),
            ("the secret door", Case::Gen, "потайной двери"),
            ("a cat", Case::Acc, "кошку"),
            ("The spell", Case::Nom, "заклинание"),
        ] {
            assert_eq!(
                parse(NameKind::Any, english).form(case),
                russian,
                "{english}"
            );
        }
        assert_eq!(
            parse(NameKind::Any, "your gauntlets").number(),
            Number::Plur
        );
    }

    #[test]
    fn alternatives_are_names_parted_by_or() {
        // what farlook says a symbol may be (pager.c append_str())
        let p = parse(
            NameKind::Any,
            "a doorway or the floor of a room or the dark part of a room or ice",
        );
        assert_eq!(
            p.form(Case::Nom),
            "дверной проём или пол комнаты или тёмная часть комнаты или лёд"
        );
        assert_eq!(
            parse(NameKind::Any, "a human or elf or you").form(Case::Nom),
            "человек или эльф или вы"
        );
        assert_eq!(
            parse(NameKind::Word, "the interior of a monster or a wall").form(Case::Nom),
            "нутро монстра или стена"
        );
        // no alternatives of things the lexicon does not know
        assert!(
            Lexicon::ru()
                .parse(NameKind::Any, "to be or not to be")
                .is_none()
        );
    }

    #[test]
    fn a_word_after_its_owner() {
        // body_part() after s_suffix(mon_nam())
        assert_eq!(
            parse(NameKind::Any, "the newt's face").form(Case::Dat),
            "лицу тритона"
        );
        assert_eq!(
            parse(NameKind::Word, "the gnome lord's hands").form(Case::Acc),
            "руки лорда гномов"
        );
        // a ghost keeps its owner's name
        assert_eq!(
            parse(NameKind::Any, "Fred's ghost").form(Case::Nom),
            "привидение Fred"
        );
    }

    #[test]
    fn two_names_joined_by_and() {
        let p = parse(NameKind::Any, "you and your pony");
        assert_eq!(p.form(Case::Acc), "вас и вашего пони");
        assert_eq!(p.number(), Number::Plur);
        assert!(
            Lexicon::ru()
                .parse(NameKind::Any, "this and that")
                .is_none()
        );
    }

    #[test]
    fn words_of_the_engine_that_name_no_thing_of_the_game() {
        for (english, case, russian) in [
            // weapon_descr()
            ("your polearm", Case::Nom, "ваше древковое оружие"),
            ("tool", Case::Ins, "инструментом"),
            ("chain", Case::Nom, "цепь"),
            // mpoisons_subj()
            ("sting", Case::Nom, "жало"),
            ("bite", Case::Nom, "укус"),
            ("gaze", Case::Nom, "взгляд"),
            ("contact", Case::Nom, "касание"),
            // pager.c
            ("demons", Case::Acc, "демонов"),
            ("unseen creature", Case::Nom, "невидимое существо"),
            ("a trap", Case::Nom, "ловушка"),
            ("land", Case::Nom, "суша"),
            ("unknown", Case::Nom, "неизвестно что"),
        ] {
            assert_eq!(
                parse(NameKind::Any, english).form(case),
                russian,
                "{english}"
            );
        }
        // the status condition keeps its own word
        assert_eq!(parse(NameKind::Word, "Trap").form(Case::Nom), "В ловушке");
    }

    #[test]
    fn the_juice_of_a_fruit() {
        let lex = Lexicon::ru();
        let juice = lex.word("slime mold juice").unwrap();
        assert_eq!(juice.form(Case::Nom), "сок слизевого гриба");
        assert_eq!(juice.form(Case::Ins), "соком слизевого гриба");
        // a fruit the player named stays as typed; the liquid has its own name
        assert_eq!(
            lex.word("pineapple juice").unwrap().form(Case::Gen),
            "сока pineapple"
        );
        assert_eq!(
            lex.word("fruit juice").unwrap().form(Case::Nom),
            "фруктовый сок"
        );
    }

    #[test]
    fn an_unknown_monster_is_a_name_only_when_one_is_expected() {
        let lex = Lexicon::ru();
        assert_eq!(parse(NameKind::Monster, "Fido").form(Case::Dat), "Fido");
        assert!(lex.parse(NameKind::Any, "Fido").is_none());
        // a common noun where a monster is expected: zap.c's an("cat")
        assert_eq!(parse(NameKind::Monster, "a cat").form(Case::Gen), "кошки");
        // a pet named as a word of the lexicon keeps its name
        assert_eq!(parse(NameKind::Monster, "Door").form(Case::Nom), "Door");
    }
}
