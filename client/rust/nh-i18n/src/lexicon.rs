//! The Russian lexicon: every name of `client/i18n/glossary.ru.toml` with
//! all its forms, as `tools/i18n/lexicon_build.py` made them
//! (`client/i18n/lexicon.ru.toml`, embedded in the binary).
//!
//! The lexicon keeps the glossary's sections ("monster", "object",
//! "appearance", "terrain"...) and its English keys, exactly as the engine
//! prints the names. An entry is a noun phrase with its six cases in both
//! numbers, an adjective with its forms by gender, or a fixed text.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;

use crate::grammar::{Case, Gender, Number};

/// The lexicon the game ships.
const LEXICON_RU: &str = include_str!("../../../i18n/lexicon.ru.toml");

/// A noun phrase with all its forms: "длинный меч", "свиток телепортации",
/// "сапоги-скороходы".
#[derive(Debug, Clone, PartialEq)]
pub struct Noun {
    /// The head noun's gender; a plural-only noun says `Masc` (agreement in
    /// the plural ignores the gender).
    pub gender: Gender,
    /// Only plural forms: сапоги, очки, Подземелья Рока.
    pub plural_only: bool,
    /// A living thing: its accusative is the genitive (вижу тритона).
    pub animate: bool,
    sg: Option<[String; 6]>,
    pl: Option<[String; 6]>,
    few: Option<String>,
    /// The English key of the part ("pair", "set") a count of these is
    /// made of: 2 пары сапог.
    pub unit: Option<String>,
}

impl Noun {
    /// A noun that never changes (a user's name, a word no lexicon knows).
    pub fn fixed(text: &str, gender: Gender) -> Noun {
        Noun {
            gender,
            plural_only: false,
            animate: false,
            sg: Some(std::array::from_fn(|_| text.to_string())),
            pl: None,
            few: None,
            unit: None,
        }
    }

    /// The singular form in `case`; a plural-only noun gives its plural.
    pub fn singular(&self, case: Case) -> &str {
        match (&self.sg, &self.pl) {
            (Some(sg), _) => &sg[case.index()],
            (None, Some(pl)) => &pl[case.index()],
            (None, None) => "",
        }
    }

    /// The plural form in `case`; a noun with no plural (a proper name,
    /// a mass noun) gives its singular.
    pub fn plural(&self, case: Case) -> &str {
        match (&self.pl, &self.sg) {
            (Some(pl), _) => &pl[case.index()],
            (None, Some(sg)) => &sg[case.index()],
            (None, None) => "",
        }
    }

    /// The nominative after 2, 3 and 4: "2 длинных меча".
    pub fn few(&self) -> &str {
        match &self.few {
            Some(f) => f,
            None => self.singular(Case::Gen),
        }
    }

    /// The noun with every form starting in lower case: a rank title
    /// (Новобранец) used for a monster (новобранец).
    pub fn uncapitalized(&self) -> Noun {
        let lower = |f: &String| -> String {
            let mut c = f.chars();
            match c.next() {
                Some(first) => first.to_lowercase().chain(c).collect(),
                None => String::new(),
            }
        };
        Noun {
            sg: self.sg.as_ref().map(|a| a.each_ref().map(lower)),
            pl: self.pl.as_ref().map(|a| a.each_ref().map(lower)),
            few: self.few.as_ref().map(lower),
            ..self.clone()
        }
    }

    /// Whether the noun has plural forms of its own.
    pub fn has_plural(&self) -> bool {
        self.pl.is_some()
    }

    /// Every distinct form.
    pub fn forms(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let all = self
            .sg
            .iter()
            .chain(self.pl.iter())
            .flat_map(|a| a.iter().map(String::as_str))
            .chain(self.few.as_deref());
        for f in all {
            if !out.contains(&f) {
                out.push(f);
            }
        }
        out
    }
}

/// An adjective: six cases for each gender and for the plural. The
/// accusative given is the inanimate one; an animate masculine or plural
/// noun takes the genitive.
#[derive(Debug, Clone, PartialEq)]
pub struct Adjective {
    m: [String; 6],
    f: [String; 6],
    n: [String; 6],
    pl: [String; 6],
}

impl Adjective {
    /// The form agreeing with a noun of that gender, number and animacy.
    pub fn form(&self, gender: Gender, number: Number, animate: bool, case: Case) -> &str {
        let row = match (number, gender) {
            (Number::Plur, _) => &self.pl,
            (Number::Sing, Gender::Masc) => &self.m,
            (Number::Sing, Gender::Fem) => &self.f,
            (Number::Sing, Gender::Neut) => &self.n,
        };
        let case = match (case, number, gender) {
            (Case::Acc, Number::Plur, _) | (Case::Acc, Number::Sing, Gender::Masc) if animate => {
                Case::Gen
            }
            (c, _, _) => c,
        };
        &row[case.index()]
    }

    /// The adjective used as a noun of that gender (Тёмный, a colour named
    /// on its own).
    pub fn as_noun(&self, gender: Gender) -> Noun {
        let sg = match gender {
            Gender::Masc => self.m.clone(),
            Gender::Fem => self.f.clone(),
            Gender::Neut => self.n.clone(),
        };
        Noun {
            gender,
            plural_only: false,
            animate: false,
            sg: Some(sg),
            pl: Some(self.pl.clone()),
            few: Some(self.pl[if gender == Gender::Fem { 0 } else { 1 }].clone()),
            unit: None,
        }
    }

    /// Every distinct form.
    pub fn forms(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for f in [&self.m, &self.f, &self.n, &self.pl]
            .into_iter()
            .flat_map(|a| a.iter())
        {
            if !out.contains(&f.as_str()) {
                out.push(f);
            }
        }
        out
    }
}

/// One entry of the lexicon.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Noun(Arc<Noun>),
    Adjective(Arc<Adjective>),
    /// Words that never change: a scroll label, a heading, a status.
    Fixed(String),
}

impl Entry {
    pub fn noun(&self) -> Option<&Arc<Noun>> {
        match self {
            Entry::Noun(n) => Some(n),
            _ => None,
        }
    }

    pub fn adjective(&self) -> Option<&Arc<Adjective>> {
        match self {
            Entry::Adjective(a) => Some(a),
            _ => None,
        }
    }

    pub fn fixed(&self) -> Option<&str> {
        match self {
            Entry::Fixed(t) => Some(t),
            _ => None,
        }
    }

    /// Every distinct Russian form of the entry.
    pub fn forms(&self) -> Vec<&str> {
        match self {
            Entry::Noun(n) => n.forms(),
            Entry::Adjective(a) => a.forms(),
            Entry::Fixed(t) => vec![t.as_str()],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LexiconError {
    #[error("the lexicon is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{section}.{key:?}: {problem}")]
    Entry {
        section: String,
        key: String,
        problem: &'static str,
    },
}

/// An entry as the file spells it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    fixed: Option<String>,
    g: Option<String>,
    #[serde(default)]
    anim: bool,
    unit: Option<String>,
    sg: Option<[String; 6]>,
    pl: Option<[String; 6]>,
    few: Option<String>,
    m: Option<[String; 6]>,
    f: Option<[String; 6]>,
    n: Option<[String; 6]>,
    #[allow(dead_code)]
    src: String,
}

/// The lexicon: sections of entries by English key.
#[derive(Debug, Clone, Default)]
pub struct Lexicon {
    sections: HashMap<String, HashMap<String, Entry>>,
    /// The name parsers' lookups, made on first use.
    pub(crate) names_index: OnceLock<crate::names::Index>,
}

impl Lexicon {
    /// The Russian lexicon the game ships, read once.
    pub fn ru() -> &'static Lexicon {
        static RU: OnceLock<Lexicon> = OnceLock::new();
        RU.get_or_init(|| Lexicon::from_toml(LEXICON_RU).expect("client/i18n/lexicon.ru.toml"))
    }

    /// A lexicon from the text of a lexicon file.
    pub fn from_toml(text: &str) -> Result<Lexicon, LexiconError> {
        let raw: HashMap<String, HashMap<String, RawEntry>> = toml::from_str(text)?;
        let mut sections = HashMap::with_capacity(raw.len());
        for (section, entries) in raw {
            let mut out = HashMap::with_capacity(entries.len());
            for (key, e) in entries {
                let entry = convert(e).map_err(|problem| LexiconError::Entry {
                    section: section.clone(),
                    key: key.clone(),
                    problem,
                })?;
                out.insert(key, entry);
            }
            sections.insert(section, out);
        }
        Ok(Lexicon {
            sections,
            names_index: OnceLock::new(),
        })
    }

    /// The entry of `key` (English, as the engine prints it) in `section`.
    pub fn get(&self, section: &str, key: &str) -> Option<&Entry> {
        self.sections.get(section)?.get(key)
    }

    pub fn noun(&self, section: &str, key: &str) -> Option<&Arc<Noun>> {
        self.get(section, key)?.noun()
    }

    pub fn adjective(&self, key: &str) -> Option<&Arc<Adjective>> {
        self.get("adjective", key)?.adjective()
    }

    /// The section names.
    pub fn sections(&self) -> impl Iterator<Item = &str> {
        self.sections.keys().map(String::as_str)
    }

    /// The entries of a section.
    pub fn entries(&self, section: &str) -> impl Iterator<Item = (&str, &Entry)> {
        self.sections
            .get(section)
            .into_iter()
            .flat_map(|s| s.iter().map(|(k, e)| (k.as_str(), e)))
    }

    /// Every Russian form of an English term, whatever its section: what a
    /// translation may use where the English says `english`.
    pub fn forms(&self, english: &str) -> Option<Vec<&str>> {
        let mut out: Vec<&str> = Vec::new();
        for entries in self.sections.values() {
            if let Some(e) = entries.get(english) {
                for f in e.forms() {
                    if !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

fn convert(e: RawEntry) -> Result<Entry, &'static str> {
    if let Some(text) = e.fixed {
        return Ok(Entry::Fixed(text));
    }
    if let (Some(m), Some(f), Some(n), Some(pl)) = (e.m, e.f, e.n, e.pl.clone()) {
        return Ok(Entry::Adjective(Arc::new(Adjective { m, f, n, pl })));
    }
    let (gender, plural_only) = match e.g.as_deref() {
        Some("m") => (Gender::Masc, false),
        Some("f") => (Gender::Fem, false),
        Some("n") => (Gender::Neut, false),
        Some("p") => (Gender::Masc, true),
        _ => return Err("a noun needs g = \"m\", \"f\", \"n\" or \"p\""),
    };
    if e.sg.is_none() && e.pl.is_none() {
        return Err("a noun needs sg or pl forms");
    }
    if plural_only && e.pl.is_none() {
        return Err("a plural-only noun needs pl forms");
    }
    Ok(Entry::Noun(Arc::new(Noun {
        gender,
        plural_only,
        animate: e.anim,
        sg: e.sg,
        pl: e.pl,
        few: e.few,
        unit: e.unit,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_lexicon_loads_and_knows_the_newt() {
        let lex = Lexicon::ru();
        let newt = lex.noun("monster", "newt").unwrap();
        assert_eq!(newt.gender, Gender::Masc);
        assert!(newt.animate);
        assert_eq!(newt.singular(Case::Acc), "тритона");
        assert_eq!(newt.plural(Case::Gen), "тритонов");
        assert_eq!(newt.few(), "тритона");
    }

    #[test]
    fn plural_only_nouns_and_units() {
        let lex = Lexicon::ru();
        let boots = lex.noun("object", "pair of speed boots").unwrap();
        assert!(boots.plural_only);
        assert_eq!(boots.unit.as_deref(), Some("pair"));
        assert_eq!(boots.singular(Case::Nom), "сапоги-скороходы");
        assert_eq!(boots.plural(Case::Gen), "сапог-скороходов");
    }

    #[test]
    fn adjectives_agree_with_gender_number_and_animacy() {
        let lex = Lexicon::ru();
        let blessed = lex.adjective("blessed").unwrap();
        assert_eq!(
            blessed.form(Gender::Fem, Number::Sing, false, Case::Acc),
            "благословенную"
        );
        assert_eq!(
            blessed.form(Gender::Masc, Number::Sing, false, Case::Acc),
            "благословенный"
        );
        assert_eq!(
            blessed.form(Gender::Masc, Number::Sing, true, Case::Acc),
            "благословенного"
        );
        assert_eq!(
            blessed.form(Gender::Neut, Number::Plur, true, Case::Acc),
            "благословенных"
        );
        assert_eq!(
            blessed.form(Gender::Neut, Number::Plur, false, Case::Ins),
            "благословенными"
        );
    }

    #[test]
    fn forms_of_a_term_cover_every_section() {
        let lex = Lexicon::ru();
        let forms = lex.forms("dwarf").unwrap();
        assert!(
            forms.contains(&"дварф") && forms.contains(&"дварфов"),
            "{forms:?}"
        );
        assert!(lex.forms("no such thing").is_none());
    }

    #[test]
    fn a_broken_entry_says_where_it_is() {
        let err = Lexicon::from_toml(
            "[monster.\"newt\"]\nsg = [\"a\",\"b\",\"c\",\"d\",\"e\",\"f\"]\nsrc = \"hand\"\n",
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "monster.\"newt\": a noun needs g = \"m\", \"f\", \"n\" or \"p\""
        );
    }
}
