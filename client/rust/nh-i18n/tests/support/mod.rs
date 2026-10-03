//! A tiny lexicon for the tests: a few nouns with both paradigms, the
//! articles and "your" of the engine's names, proper names as they are.

#![allow(dead_code)]

use std::path::PathBuf;

use nh_i18n::{Case, Gender, NameKind, Names, Number, Phrase, counted_form};

/// client/i18n, from this crate.
pub fn i18n_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../i18n")
}

#[derive(Clone)]
pub struct Noun {
    pub en: &'static str,
    pub sg: [&'static str; 6],
    pub pl: [&'static str; 6],
    /// after 2, 3, 4 in the nominative
    pub few: &'static str,
    pub gender: Gender,
    pub animate: bool,
}

pub const NOUNS: &[Noun] = &[
    Noun {
        en: "newt",
        sg: [
            "тритон",
            "тритона",
            "тритону",
            "тритона",
            "тритоном",
            "тритоне",
        ],
        pl: [
            "тритоны",
            "тритонов",
            "тритонам",
            "тритонов",
            "тритонами",
            "тритонах",
        ],
        few: "тритона",
        gender: Gender::Masc,
        animate: true,
    },
    Noun {
        en: "sewer rat",
        sg: ["крыса", "крысы", "крысе", "крысу", "крысой", "крысе"],
        pl: ["крысы", "крыс", "крысам", "крыс", "крысами", "крысах"],
        few: "крысы",
        gender: Gender::Fem,
        animate: true,
    },
    Noun {
        en: "spear",
        sg: ["копьё", "копья", "копью", "копьё", "копьём", "копье"],
        pl: ["копья", "копий", "копьям", "копья", "копьями", "копьях"],
        few: "копья",
        gender: Gender::Neut,
        animate: false,
    },
    Noun {
        en: "arrow",
        sg: ["стрела", "стрелы", "стреле", "стрелу", "стрелой", "стреле"],
        pl: [
            "стрелы",
            "стрел",
            "стрелам",
            "стрелы",
            "стрелами",
            "стрелах",
        ],
        few: "стрелы",
        gender: Gender::Fem,
        animate: false,
    },
    Noun {
        en: "kitten",
        sg: [
            "котёнок",
            "котёнка",
            "котёнку",
            "котёнка",
            "котёнком",
            "котёнке",
        ],
        pl: ["котята", "котят", "котятам", "котят", "котятами", "котятах"],
        few: "котёнка",
        gender: Gender::Masc,
        animate: true,
    },
    Noun {
        en: "gold piece",
        sg: [
            "золотая монета",
            "золотой монеты",
            "золотой монете",
            "золотую монету",
            "золотой монетой",
            "золотой монете",
        ],
        pl: [
            "золотые монеты",
            "золотых монет",
            "золотым монетам",
            "золотые монеты",
            "золотыми монетами",
            "золотых монетах",
        ],
        few: "золотые монеты",
        gender: Gender::Fem,
        animate: false,
    },
    Noun {
        en: "dart",
        sg: [
            "дротик",
            "дротика",
            "дротику",
            "дротик",
            "дротиком",
            "дротике",
        ],
        pl: [
            "дротики",
            "дротиков",
            "дротикам",
            "дротики",
            "дротиками",
            "дротиках",
        ],
        few: "дротика",
        gender: Gender::Masc,
        animate: false,
    },
    Noun {
        en: "pick-axe",
        sg: ["кирка", "кирки", "кирке", "кирку", "киркой", "кирке"],
        pl: ["кирки", "кирок", "киркам", "кирки", "кирками", "кирках"],
        few: "кирки",
        gender: Gender::Fem,
        animate: false,
    },
];

const YOUR: [[&str; 6]; 4] = [
    ["ваш", "вашего", "вашему", "ваш", "вашим", "вашем"],
    ["ваша", "вашей", "вашей", "вашу", "вашей", "вашей"],
    ["ваше", "вашего", "вашему", "ваше", "вашим", "вашем"],
    ["ваши", "ваших", "вашим", "ваши", "вашими", "ваших"],
];

/// A noun, how many (0: a plural without its count, "arrows"), and
/// "your".
#[derive(Clone)]
pub struct Name {
    pub noun: Noun,
    pub count: u64,
    pub yours: bool,
}

impl Name {
    pub fn one(en: &str) -> Name {
        Name {
            noun: NOUNS
                .iter()
                .find(|n| n.en == en)
                .cloned()
                .expect("a test noun"),
            count: 1,
            yours: false,
        }
    }

    pub fn many(en: &str, count: u64) -> Name {
        Name {
            count,
            ..Name::one(en)
        }
    }

    fn your(&self, case: Case) -> String {
        let row = if self.number() == Number::Plur {
            3
        } else {
            match self.noun.gender {
                Gender::Masc => 0,
                Gender::Fem => 1,
                Gender::Neut => 2,
            }
        };
        let animate_acc = case == Case::Acc && self.noun.animate && row != 1 && row != 2;
        YOUR[row][if animate_acc { Case::Gen } else { case }.index()].to_string()
    }
}

impl Phrase for Name {
    fn form(&self, case: Case) -> String {
        let noun = match self.count {
            0 => {
                let case = if case == Case::Acc && self.noun.animate {
                    Case::Gen
                } else {
                    case
                };
                self.noun.pl[case.index()].to_string()
            }
            1 => self.noun.sg[case.index()].to_string(),
            n => format!("{n} {}", self.counted(n, case)),
        };
        if self.yours {
            format!("{} {noun}", self.your(case))
        } else {
            noun
        }
    }

    fn gender(&self) -> Gender {
        self.noun.gender
    }

    fn number(&self) -> Number {
        if self.count == 1 {
            Number::Sing
        } else {
            Number::Plur
        }
    }

    fn counted(&self, n: u64, case: Case) -> String {
        match counted_form(n, case, self.noun.animate) {
            // 2 золотые монеты: the phrase's own form after 2, 3, 4
            (Number::Sing, Case::Gen) if case != Case::Gen => self.noun.few.to_string(),
            (Number::Sing, c) => self.noun.sg[c.index()].to_string(),
            (Number::Plur, c) => self.noun.pl[c.index()].to_string(),
        }
    }
}

/// A proper name: the same in every case.
struct Proper(String);

impl Phrase for Proper {
    fn form(&self, _case: Case) -> String {
        self.0.clone()
    }

    fn gender(&self) -> Gender {
        Gender::Masc
    }

    fn number(&self) -> Number {
        Number::Sing
    }
}

/// Parses "the newt", "a dart", "your kitten", "3 arrows", "Slasher".
pub struct TestNames;

impl Names for TestNames {
    fn parse(&self, _kind: NameKind, english: &str) -> Option<Box<dyn Phrase>> {
        let mut rest = english;
        let mut yours = false;
        for article in ["the ", "The ", "a ", "A ", "an ", "An "] {
            if let Some(r) = rest.strip_prefix(article) {
                rest = r;
            }
        }
        for your in ["your ", "Your "] {
            if let Some(r) = rest.strip_prefix(your) {
                rest = r;
                yours = true;
            }
        }
        let (count, rest) = match rest.split_once(' ') {
            Some((n, r)) if n.parse::<u64>().is_ok() => (n.parse().unwrap_or(1), r),
            _ => (1, rest),
        };
        if let Some(noun) = NOUNS.iter().find(|n| n.en == rest) {
            return Some(Box::new(Name {
                noun: noun.clone(),
                count,
                yours,
            }));
        }
        // "3 arrows", or "arrows" with no count
        let singular = rest.strip_suffix('s').unwrap_or(rest);
        if let Some(noun) = NOUNS.iter().find(|n| n.en == singular) {
            return Some(Box::new(Name {
                noun: noun.clone(),
                count: if count > 1 { count } else { 0 },
                yours,
            }));
        }
        let proper = !rest.contains(' ') && rest.chars().next().is_some_and(char::is_uppercase);
        proper.then(|| Box::new(Proper(rest.to_string())) as Box<dyn Phrase>)
    }
}
