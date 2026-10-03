//! The checks of the Russian translations against the catalog and the
//! glossary: every placeholder names an argument the English format has,
//! with a modifier that fits it; every argument is used; the canonical
//! Russian term stands where the English names a glossary term; no English
//! word is left behind.

use crate::catalog::Catalog;
use crate::format::{ConvKind, convs};
use crate::russian::{Russian, Translation};
use crate::template::{Select, Target};

/// A term of the glossary: its English name and the Russian forms a
/// translation may use (any case or number; a stem matches the words it
/// begins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub en: String,
    pub ru: Vec<String>,
}

/// The canonical names the translations must use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Glossary {
    pub terms: Vec<Term>,
}

impl Glossary {
    pub fn new(terms: Vec<Term>) -> Glossary {
        Glossary { terms }
    }
}

/// One thing wrong with a translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub file: String,
    pub id: String,
    pub what: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: [{}]: {}", self.file, self.id, self.what)
    }
}

/// Every problem of every translation, by file and id.
pub fn lint(catalog: &Catalog, russian: &Russian, glossary: &Glossary) -> Vec<Problem> {
    let mut out = Vec::new();
    for t in russian.iter() {
        for what in check(catalog, t, glossary) {
            out.push(Problem {
                file: t.file.clone(),
                id: t.id.clone(),
                what,
            });
        }
    }
    out.sort_by(|a, b| (&a.file, &a.id).cmp(&(&b.file, &b.id)));
    out
}

fn check(catalog: &Catalog, tr: &Translation, glossary: &Glossary) -> Vec<String> {
    let mut out = Vec::new();
    let Some(t) = catalog.by_id(&tr.id) else {
        return vec![format!("no template {} in the catalog: {:?}", tr.id, tr.en)];
    };
    if t.fmt != tr.en {
        out.push(format!("en is {:?}, the catalog says {:?}", tr.en, t.fmt));
    }
    let kinds: Vec<ConvKind> = convs(&t.segments).map(|c| c.kind).collect();
    let mut used = vec![false; kinds.len()];
    for p in tr.template.placeholders() {
        let Target::Arg(i) = p.target else {
            continue;
        };
        let n = i + 1;
        let Some(&kind) = kinds.get(i) else {
            out.push(format!(
                "{{{}}}: the format has {} argument(s)",
                p.source,
                kinds.len()
            ));
            continue;
        };
        used[i] = true;
        let number = kind == ConvKind::Int;
        match &p.select {
            Some(Select::Plural(_)) if !number => {
                out.push(format!(
                    "{{{}}}: plural forms follow a number, argument {n} is not one",
                    p.source
                ));
            }
            Some(Select::Gender(_) | Select::Num(_)) if number => {
                out.push(format!(
                    "{{{}}}: argument {n} is a number: use plural",
                    p.source
                ));
            }
            _ => {}
        }
        if number && (p.case.is_some() || p.cap) {
            out.push(format!(
                "{{{}}}: argument {n} is a number: it has no case",
                p.source
            ));
        }
        if let Some(j) = p.count_by {
            if kinds.get(j) != Some(&ConvKind::Int) {
                out.push(format!(
                    "{{{}}}: argument {} is not a number to count by",
                    p.source,
                    j + 1
                ));
            } else {
                used[j] = true;
            }
            if number {
                out.push(format!("{{{}}}: a number is not counted", p.source));
            }
        }
    }
    for (i, u) in used.iter().enumerate() {
        if !u {
            out.push(format!("argument {} ({}) is not used", i + 1, t.kinds[i]));
        }
    }
    let ru_text = tr.template.render(&[], crate::grammar::Gender::Masc);
    for word in latin_words(&ru_text) {
        if !tr.en.contains(word) {
            out.push(format!("English left in the translation: {word:?}"));
        }
    }
    let en_lower = tr.en.to_lowercase();
    let ru_lower = tr.ru.to_lowercase();
    for term in &glossary.terms {
        if has_word(&en_lower, &term.en.to_lowercase())
            && !term.ru.iter().any(|f| ru_lower.contains(&f.to_lowercase()))
        {
            out.push(format!(
                "{:?} is in the glossary: use {}",
                term.en,
                term.ru.first().map_or("its term", String::as_str)
            ));
        }
    }
    out
}

/// The words of Latin letters in a text.
fn latin_words(s: &str) -> Vec<&str> {
    s.split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() > 1)
        .collect()
}

/// Does `text` hold `word` with no letter right before or after it?
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"{"format": 1, "entries": [
{"id": "a1", "fmt": "You hit %s.", "args": ["monster"]},
{"id": "a2", "fmt": "You find %d %s.", "args": ["number", "object"]},
{"id": "a3", "fmt": "You write Elbereth in the dust.", "args": []}
]}"#;

    fn problems(toml: &str) -> Vec<String> {
        let catalog = Catalog::parse(CATALOG).unwrap();
        let russian = Russian::parse(&[("t.toml".into(), toml.into())]).unwrap();
        let glossary = Glossary::new(vec![Term {
            en: "Elbereth".into(),
            ru: vec!["Elbereth".into()],
        }]);
        lint(&catalog, &russian, &glossary)
            .into_iter()
            .map(|p| p.what)
            .collect()
    }

    #[test]
    fn a_good_translation_passes() {
        let p = problems(
            r#"
[a1]
en = "You hit %s."
ru = "Вы бьёте {1:acc}."
[a2]
en = "You find %d %s."
ru = "Вы находите {1} {2:by1:acc}."
[a3]
en = "You write Elbereth in the dust."
ru = "Вы пишете Elbereth в пыли."
"#,
        );
        assert!(p.is_empty(), "{p:?}");
    }

    #[test]
    fn mistakes_are_found() {
        let p = problems(
            r#"
[a1]
en = "You hit %s!"
ru = "Вы бьёте {2:acc} hard."
[a2]
en = "You find %d %s."
ru = "Вы находите {1:acc} {2:plural|а|б|в}."
[a3]
en = "You write Elbereth in the dust."
ru = "Вы пишете Эльберет в пыли."
[zz]
en = "gone"
ru = "нет"
"#,
        );
        let want = [
            "en is \"You hit %s!\", the catalog says \"You hit %s.\"",
            "{2:acc}: the format has 1 argument(s)",
            "argument 1 (monster) is not used",
            "English left in the translation: \"hard\"",
            "{1:acc}: argument 1 is a number: it has no case",
            "{2:plural|а|б|в}: plural forms follow a number, argument 2 is not one",
            "\"Elbereth\" is in the glossary: use Elbereth",
            "no template zz in the catalog: \"gone\"",
        ];
        assert_eq!(p, want);
    }
}
