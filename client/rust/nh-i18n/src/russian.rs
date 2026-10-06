//! The Russian translations of the catalog's templates:
//! `client/i18n/ru/*.toml`, one table per template, keyed by its id.
//!
//! ```toml
//! [741e0f949397]
//! en = "You hit %s."
//! ru = "Вы бьёте {1:acc}."
//!
//! # a piece that is an argument of other messages may give its case
//! # forms and its gender, for {N:case} and agreement
//! [5b3c0d8e9f10]
//! en = "digging"
//! ru = "копание"
//! forms = ["копание", "копания", "копанию", "копание", "копанием", "копании"]
//! gender = "n"
//! ```
//!
//! A template with arguments may give its forms too, each a template
//! ("взрыв {1:gen}", "взрыва {1:gen}"…): a text it makes, the argument of
//! another, then takes that one's case ("убит взрывом газовой споры").
//!
//! `en` repeats the English format: a reviewer reads it, and the linter
//! tells when the catalog no longer has it. `gender` is "m", "f", "n" or
//! "pl" (a plural).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::Deserialize;

use crate::grammar::{Case, Gender, Number};
use crate::phrase::Phrase;
use crate::template::{RuTemplate, TemplateError};

/// Where the translations live, from the repository root.
pub const RU_DIR: &str = "client/i18n/ru";

#[derive(Debug, thiserror::Error)]
pub enum RussianError {
    #[error("{0}: cannot read: {1}")]
    Io(String, std::io::Error),
    #[error("{0}: not valid TOML for a translation file: {1}")]
    Toml(String, toml::de::Error),
    #[error("{file}: [{id}]: {err}")]
    Template {
        file: String,
        id: String,
        err: TemplateError,
    },
    #[error("{file}: [{id}]: {what}")]
    Invalid {
        file: String,
        id: String,
        what: String,
    },
    #[error("[{0}] is translated twice: in {1} and in {2}")]
    Twice(String, String, String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    en: String,
    ru: String,
    #[serde(default)]
    forms: Option<Vec<String>>,
    #[serde(default)]
    gender: Option<String>,
    #[serde(default)]
    not_terms: Vec<String>,
}

/// The translation of one template.
#[derive(Debug, Clone, PartialEq)]
pub struct Translation {
    pub id: String,
    /// The English format, as the translator saw it.
    pub en: String,
    /// The Russian template as written.
    pub ru: String,
    pub template: RuTemplate,
    /// Case forms, for a piece used as an argument.
    pub forms: Option<[String; 6]>,
    /// The case forms as templates, for a translation with arguments.
    pub form_templates: Option<Vec<RuTemplate>>,
    pub gender: Option<(Gender, Number)>,
    /// English words of the format that are not the glossary's terms there
    /// ("tin" as a verb, "rock" as the stuff): the linter lets them be.
    pub not_terms: Vec<String>,
    /// The file it came from.
    pub file: String,
}

impl Translation {
    /// The translation as an argument of another message: its case forms
    /// when it has them, else its text in every case.
    pub fn phrase(&self) -> Box<dyn Phrase> {
        let (gender, number) = self.gender.unwrap_or((Gender::Masc, Number::Sing));
        let forms = self.forms.clone().unwrap_or_else(|| {
            let plain = self.template.render(&[], Gender::Masc);
            std::array::from_fn(|_| plain.clone())
        });
        Box::new(Piece {
            forms,
            gender,
            number,
        })
    }
}

/// A translated piece used as an argument.
struct Piece {
    forms: [String; 6],
    gender: Gender,
    number: Number,
}

impl Phrase for Piece {
    fn form(&self, case: Case) -> String {
        self.forms[case.index()].clone()
    }

    fn gender(&self) -> Gender {
        self.gender
    }

    fn number(&self) -> Number {
        self.number
    }
}

/// Every translation, by template id.
#[derive(Debug, Clone, Default)]
pub struct Russian {
    entries: HashMap<String, Translation>,
}

impl Russian {
    /// The translations of the files given as (name, text).
    pub fn parse(files: &[(String, String)]) -> Result<Russian, RussianError> {
        let mut entries: HashMap<String, Translation> = HashMap::new();
        for (name, text) in files {
            for t in parse_file(name, text)? {
                if let Some(old) = entries.get(&t.id) {
                    return Err(RussianError::Twice(t.id, old.file.clone(), t.file));
                }
                entries.insert(t.id.clone(), t);
            }
        }
        Ok(Russian { entries })
    }

    /// Every `*.toml` of a directory, in name order.
    pub fn load_dir(dir: &Path) -> Result<Russian, RussianError> {
        let io = |e| RussianError::Io(dir.display().to_string(), e);
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .map_err(io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        names.sort();
        let mut files = Vec::new();
        for p in names {
            let text = std::fs::read_to_string(&p)
                .map_err(|e| RussianError::Io(p.display().to_string(), e))?;
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            files.push((name, text));
        }
        Russian::parse(&files)
    }

    pub fn get(&self, id: &str) -> Option<&Translation> {
        self.entries.get(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every translation, by id order.
    pub fn iter(&self) -> impl Iterator<Item = &Translation> {
        let mut v: Vec<&Translation> = self.entries.values().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v.into_iter()
    }
}

fn parse_file(name: &str, text: &str) -> Result<Vec<Translation>, RussianError> {
    let raw: BTreeMap<String, RawEntry> =
        toml::from_str(text).map_err(|e| RussianError::Toml(name.into(), e))?;
    let mut out = Vec::new();
    for (id, e) in raw {
        let invalid = |what: String| RussianError::Invalid {
            file: name.into(),
            id: id.clone(),
            what,
        };
        let template = RuTemplate::parse(&e.ru).map_err(|err| RussianError::Template {
            file: name.into(),
            id: id.clone(),
            err,
        })?;
        let forms = match e.forms {
            None => None,
            Some(f) => Some(
                <[String; 6]>::try_from(f)
                    .map_err(|f| invalid(format!("{} forms, not six", f.len())))?,
            ),
        };
        let form_templates = match &forms {
            Some(f) if f.iter().any(|x| x.contains('{')) => Some(
                f.iter()
                    .map(|x| RuTemplate::parse(x))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| RussianError::Template {
                        file: name.into(),
                        id: id.clone(),
                        err,
                    })?,
            ),
            _ => None,
        };
        let gender = match e.gender.as_deref() {
            None => None,
            Some("m") => Some((Gender::Masc, Number::Sing)),
            Some("f") => Some((Gender::Fem, Number::Sing)),
            Some("n") => Some((Gender::Neut, Number::Sing)),
            Some("pl") => Some((Gender::Masc, Number::Plur)),
            Some(other) => return Err(invalid(format!("gender {other:?}: m, f, n or pl"))),
        };
        out.push(Translation {
            id,
            en: e.en,
            ru: e.ru,
            template,
            forms,
            form_templates,
            gender,
            not_terms: e.not_terms,
            file: name.into(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(text: &str) -> Vec<(String, String)> {
        vec![("t.toml".into(), text.into())]
    }

    #[test]
    fn translations_with_forms_and_gender() {
        let ru = Russian::parse(&files(
            r#"
[a1]
en = "You hit %s."
ru = "Вы бьёте {1:acc}."

[d1]
en = "digging"
ru = "копание"
forms = ["копание", "копания", "копанию", "копание", "копанием", "копании"]
gender = "n"

[s1]
en = "searching"
ru = "поиски"
gender = "pl"
"#,
        ))
        .unwrap();
        assert_eq!(ru.len(), 3);
        assert_eq!(ru.get("a1").unwrap().en, "You hit %s.");
        let p = ru.get("d1").unwrap().phrase();
        assert_eq!(
            (p.form(Case::Ins), p.gender()),
            ("копанием".to_string(), Gender::Neut)
        );
        let plain = ru.get("s1").unwrap().phrase();
        assert_eq!(
            (plain.form(Case::Gen), plain.number()),
            ("поиски".to_string(), Number::Plur)
        );
    }

    #[test]
    fn forms_of_a_template_with_arguments() {
        let ru = Russian::parse(&files(
            r#"
[e1]
en = "%s explosion"
ru = "взрыв {1:gen}"
forms = ["взрыв {1:gen}", "взрыва {1:gen}", "взрыву {1:gen}", "взрыв {1:gen}", "взрывом {1:gen}", "взрыве {1:gen}"]
gender = "m"
"#,
        ))
        .unwrap();
        let t = ru.get("e1").unwrap();
        assert_eq!(t.form_templates.as_ref().map(Vec::len), Some(6));
        let err = Russian::parse(&files(
            "[x]\nen = \"%s\"\nru = \"{1}\"\nforms = [\"{1:вин}\", \"a\", \"a\", \"a\", \"a\", \"a\"]\n",
        ))
        .unwrap_err();
        assert!(err.to_string().starts_with("t.toml: [x]:"), "{err}");
    }

    #[test]
    fn bad_files_say_where() {
        let err = Russian::parse(&files("[x]\nen = \"a\"\nru = \"{1:вин}\"\n")).unwrap_err();
        assert!(err.to_string().starts_with("t.toml: [x]:"), "{err}");
        let err =
            Russian::parse(&files("[x]\nen = \"a\"\nru = \"b\"\nforms = [\"a\"]\n")).unwrap_err();
        assert!(err.to_string().contains("1 forms"), "{err}");
        let err = Russian::parse(&files("[x]\nen = \"a\"\nru = \"b\"\nnote = 1\n")).unwrap_err();
        assert!(matches!(err, RussianError::Toml(..)), "{err}");
        let twice = vec![
            (
                "a.toml".to_string(),
                "[x]\nen = \"a\"\nru = \"b\"\n".to_string(),
            ),
            (
                "b.toml".to_string(),
                "[x]\nen = \"a\"\nru = \"c\"\n".to_string(),
            ),
        ];
        assert!(matches!(
            Russian::parse(&twice),
            Err(RussianError::Twice(..))
        ));
    }
}
