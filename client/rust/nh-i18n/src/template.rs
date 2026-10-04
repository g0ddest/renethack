//! Russian templates: the translation of one English format, with
//! placeholders for its arguments.
//!
//! # Syntax
//!
//! Text, with placeholders in braces (`{{` and `}}` are literal braces). A
//! placeholder names an argument of the English format by its position,
//! 1 for the first conversion (`%s`, `%d`, `%c`...), or `hero`:
//!
//! | placeholder | renders |
//! |---|---|
//! | `{1}` | argument 1 in the nominative (a number as its digits, text as it is) |
//! | `{1:acc}` | in a case: `nom`, `gen`, `dat`, `acc`, `ins`, `prep`; `loc` after в/на of a place (на полу, the prepositional where a noun has no locative) |
//! | `{1:cap}`, `{1:ins:cap}` | with its first letter upper-cased |
//! | `{1:gender\|ударил\|ударила\|ударило\|ударили}` | the form that agrees with argument 1: masculine, feminine, neuter, plural |
//! | `{1:num\|кусает\|кусают}` | singular or plural, as argument 1 |
//! | `{1:plural\|монету\|монеты\|монет}` | after the number argument 1: one (1, 21), few (2–4, 22–24), many (5–20, 0) |
//! | `{2:by1}`, `{2:by1:acc}` | argument 2 as counted by the number argument 1: "стрелу", "стрелы", "стрел" |
//! | `{hero:gender\|сам\|сама}` | as the hero's gender: masculine, feminine |
//!
//! "You hit %s." → `"Вы бьёте {1:acc}."`; "%s bites!" →
//! `"{1} кусает!"` (the message's first letter is upper-cased anyway);
//! "%s dies!" → `"{1} {1:gender|умирает|умирает|умирает|умирают}!"`, or
//! shorter `"{1} {1:num|умирает|умирают}!"`; "You find %d gold pieces." →
//! `"Вы находите {1} {1:plural|золотую монету|золотые монеты|золотых монет}."`;
//! "You shoot %d %s." → `"Вы выпускаете {1} {2:by1:acc}."`.
//!
//! A message starts with an upper-case letter whatever its template says.

use std::fmt::Write as _;

use crate::grammar::{Case, Gender, Number, Plural, plural_category};
use crate::phrase::Phrase;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("unclosed placeholder at byte {0}")]
    Unclosed(usize),
    #[error("a lone '}}' at byte {0} (write '}}}}')")]
    LoneBrace(usize),
    #[error("placeholder {{{0}}}: not an argument number nor `hero`")]
    BadTarget(String),
    #[error("placeholder {{{0}}}: unknown modifier {1:?}")]
    BadModifier(String, String),
    #[error("placeholder {{{0}}}: {1} takes {2} forms, not {3}")]
    Forms(String, &'static str, usize, usize),
    #[error("placeholder {{{0}}}: forms without a selector (gender, num, plural)")]
    FormsWithoutSelector(String),
    #[error("placeholder {{{0}}}: a selector with a case or `cap`")]
    SelectorWithCase(String),
    #[error("placeholder {{{0}}}: `hero` only selects by gender")]
    Hero(String),
}

/// What a placeholder refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The argument at this index (0-based: `{1}` is `Arg(0)`).
    Arg(usize),
    Hero,
}

/// Forms chosen by agreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Select {
    /// masculine, feminine, neuter, plural
    Gender([String; 4]),
    /// the hero's: masculine, feminine
    HeroGender([String; 2]),
    /// singular, plural
    Num([String; 2]),
    /// one, few, many
    Plural([String; 3]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub target: Target,
    pub case: Option<Case>,
    pub cap: bool,
    pub select: Option<Select>,
    /// `{2:by1}`: argument 2 as counted by the number argument 1.
    pub count_by: Option<usize>,
    /// As written, without the braces.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Text(String),
    Place(Placeholder),
}

/// A parsed Russian template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuTemplate {
    pub parts: Vec<Part>,
}

/// An argument ready for a template.
pub enum Value {
    /// A name or a word, declined by its phrase.
    Phrase(Box<dyn Phrase>),
    /// `%d` and its kin.
    Number(i64),
    /// Text that does not decline (a `%c`, untranslated text).
    Text(String),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Phrase(p) => write!(f, "Phrase({:?})", p.form(Case::Nom)),
            Value::Number(n) => write!(f, "Number({n})"),
            Value::Text(t) => write!(f, "Text({t:?})"),
        }
    }
}

impl Value {
    fn form(&self, case: Case) -> String {
        match self {
            Value::Phrase(p) => p.form(case),
            Value::Number(n) => n.to_string(),
            Value::Text(t) => t.clone(),
        }
    }

    fn gender(&self) -> Gender {
        match self {
            Value::Phrase(p) => p.gender(),
            _ => Gender::Masc,
        }
    }

    fn number(&self) -> Number {
        match self {
            Value::Phrase(p) => p.number(),
            Value::Number(n) if n.unsigned_abs() != 1 => Number::Plur,
            _ => Number::Sing,
        }
    }

    fn plural(&self) -> Plural {
        match self {
            Value::Number(n) => plural_category(n.unsigned_abs()),
            _ if self.number() == Number::Plur => Plural::Many,
            _ => Plural::One,
        }
    }
}

impl RuTemplate {
    pub fn parse(s: &str) -> Result<RuTemplate, TemplateError> {
        let mut parts = Vec::new();
        let mut text = String::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < s.len() {
            match b[i] {
                b'{' if b.get(i + 1) == Some(&b'{') => {
                    text.push('{');
                    i += 2;
                }
                b'}' if b.get(i + 1) == Some(&b'}') => {
                    text.push('}');
                    i += 2;
                }
                b'{' => {
                    let end = s[i..].find('}').ok_or(TemplateError::Unclosed(i))? + i;
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    parts.push(Part::Place(placeholder(&s[i + 1..end])?));
                    i = end + 1;
                }
                b'}' => return Err(TemplateError::LoneBrace(i)),
                _ => {
                    let c = s[i..].chars().next().unwrap_or_default();
                    text.push(c);
                    i += c.len_utf8();
                }
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        Ok(RuTemplate { parts })
    }

    pub fn placeholders(&self) -> impl Iterator<Item = &Placeholder> {
        self.parts.iter().filter_map(|p| match p {
            Part::Place(p) => Some(p),
            Part::Text(_) => None,
        })
    }

    /// The template with its arguments put in. A placeholder for an
    /// argument that is missing renders as nothing.
    pub fn render(&self, args: &[Value], hero: Gender) -> String {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Place(p) => {
                    let _ = write!(out, "{}", render_placeholder(p, args, hero));
                }
            }
        }
        out
    }
}

fn render_placeholder(p: &Placeholder, args: &[Value], hero: Gender) -> String {
    if let Target::Hero = p.target {
        return match &p.select {
            Some(Select::HeroGender([m, f])) => {
                if hero == Gender::Fem {
                    f.clone()
                } else {
                    m.clone()
                }
            }
            _ => String::new(),
        };
    }
    let Target::Arg(i) = p.target else {
        unreachable!("the hero is handled above")
    };
    let Some(v) = args.get(i) else {
        return String::new();
    };
    match &p.select {
        Some(Select::Gender(forms)) => {
            let k = if v.number() == Number::Plur {
                3
            } else {
                match v.gender() {
                    Gender::Masc => 0,
                    Gender::Fem => 1,
                    Gender::Neut => 2,
                }
            };
            forms[k].clone()
        }
        Some(Select::Num([sg, pl])) => {
            if v.number() == Number::Plur {
                pl.clone()
            } else {
                sg.clone()
            }
        }
        Some(Select::Plural([one, few, many])) => match v.plural() {
            Plural::One => one.clone(),
            Plural::Few => few.clone(),
            Plural::Many => many.clone(),
        },
        Some(Select::HeroGender(_)) => String::new(),
        None => {
            let case = p.case.unwrap_or(Case::Nom);
            let form = match (p.count_by.and_then(|j| args.get(j)), v) {
                (Some(Value::Number(n)), Value::Phrase(ph)) => ph.counted(n.unsigned_abs(), case),
                _ => v.form(case),
            };
            if p.cap { capitalize(&form) } else { form }
        }
    }
}

/// The first letter upper-cased.
pub fn capitalize(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().chain(cs).collect(),
        None => String::new(),
    }
}

fn placeholder(src: &str) -> Result<Placeholder, TemplateError> {
    let mut forms: Vec<&str> = src.split('|').collect();
    let head = forms.remove(0);
    let mut words = head.split(':');
    let target = match words.next().unwrap_or("").trim() {
        "hero" => Target::Hero,
        n => match n.parse::<usize>() {
            Ok(k) if k >= 1 => Target::Arg(k - 1),
            _ => return Err(TemplateError::BadTarget(src.into())),
        },
    };
    let mut case = None;
    let mut cap = false;
    let mut selector = None;
    let mut count_by = None;
    for w in words {
        match w.trim() {
            "cap" => cap = true,
            s @ ("gender" | "num" | "plural") => selector = Some(s),
            by if by.starts_with("by") => match by[2..].parse::<usize>() {
                Ok(k) if k >= 1 => count_by = Some(k - 1),
                _ => return Err(TemplateError::BadModifier(src.into(), by.into())),
            },
            other => match other.parse::<Case>() {
                Ok(c) => case = Some(c),
                Err(_) => return Err(TemplateError::BadModifier(src.into(), other.into())),
            },
        }
    }
    let forms: Vec<String> = forms.into_iter().map(str::to_string).collect();
    let want = |name: &'static str, n: usize| {
        if forms.len() == n {
            Ok(())
        } else {
            Err(TemplateError::Forms(src.into(), name, n, forms.len()))
        }
    };
    let select = match (selector, target) {
        (None, _) if !forms.is_empty() => {
            return Err(TemplateError::FormsWithoutSelector(src.into()));
        }
        (None, Target::Hero) => return Err(TemplateError::Hero(src.into())),
        (None, _) => None,
        (Some("gender"), Target::Hero) => {
            want("gender of the hero", 2)?;
            Some(Select::HeroGender([forms[0].clone(), forms[1].clone()]))
        }
        (Some(_), Target::Hero) => return Err(TemplateError::Hero(src.into())),
        (Some("gender"), _) => {
            want("gender", 4)?;
            Some(Select::Gender([
                forms[0].clone(),
                forms[1].clone(),
                forms[2].clone(),
                forms[3].clone(),
            ]))
        }
        (Some("num"), _) => {
            want("num", 2)?;
            Some(Select::Num([forms[0].clone(), forms[1].clone()]))
        }
        (Some(_), _) => {
            want("plural", 3)?;
            Some(Select::Plural([
                forms[0].clone(),
                forms[1].clone(),
                forms[2].clone(),
            ]))
        }
    };
    if select.is_some() && (case.is_some() || cap || count_by.is_some()) {
        return Err(TemplateError::SelectorWithCase(src.into()));
    }
    Ok(Placeholder {
        target,
        case,
        cap,
        select,
        count_by,
        source: src.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phrase::Fixed;

    struct Noun {
        forms: [&'static str; 6],
        gender: Gender,
        number: Number,
    }

    impl Phrase for Noun {
        fn form(&self, case: Case) -> String {
            self.forms[case.index()].to_string()
        }
        fn gender(&self) -> Gender {
            self.gender
        }
        fn number(&self) -> Number {
            self.number
        }
    }

    fn newt() -> Value {
        Value::Phrase(Box::new(Noun {
            forms: [
                "тритон",
                "тритона",
                "тритону",
                "тритона",
                "тритоном",
                "тритоне",
            ],
            gender: Gender::Masc,
            number: Number::Sing,
        }))
    }

    fn rat() -> Value {
        Value::Phrase(Box::new(Noun {
            forms: ["крыса", "крысы", "крысе", "крысу", "крысой", "крысе"],
            gender: Gender::Fem,
            number: Number::Sing,
        }))
    }

    fn arrows() -> Value {
        Value::Phrase(Box::new(Noun {
            forms: [
                "3 стрелы",
                "3 стрел",
                "3 стрелам",
                "3 стрелы",
                "3 стрелами",
                "3 стрелах",
            ],
            gender: Gender::Fem,
            number: Number::Plur,
        }))
    }

    fn render(t: &str, args: &[Value]) -> String {
        RuTemplate::parse(t).unwrap().render(args, Gender::Masc)
    }

    #[test]
    fn cases_and_capitals() {
        assert_eq!(render("Вы бьёте {1:acc}.", &[newt()]), "Вы бьёте тритона.");
        assert_eq!(render("Вы бьёте {1:acc}.", &[rat()]), "Вы бьёте крысу.");
        assert_eq!(render("{1:cap} кусает!", &[rat()]), "Крыса кусает!");
        assert_eq!(
            render("С {1:ins}: {{ok}}", &[arrows()]),
            "С 3 стрелами: {ok}"
        );
    }

    #[test]
    fn agreement_and_plurals() {
        let t = "{1} {1:gender|умер|умерла|умерло|умерли}.";
        assert_eq!(render(t, &[newt()]), "тритон умер.");
        assert_eq!(render(t, &[rat()]), "крыса умерла.");
        assert_eq!(render(t, &[arrows()]), "3 стрелы умерли.");
        let t = "{1:num|Лежит|Лежат} {1}.";
        assert_eq!(render(t, &[arrows()]), "Лежат 3 стрелы.");
        let t = "{1} {1:plural|монета|монеты|монет}";
        let coins: Vec<String> = [1, 3, 5, 11, 21, 22, 0]
            .iter()
            .map(|&n| render(t, &[Value::Number(n)]))
            .collect();
        assert_eq!(
            coins,
            [
                "1 монета",
                "3 монеты",
                "5 монет",
                "11 монет",
                "21 монета",
                "22 монеты",
                "0 монет"
            ]
        );
        let t = RuTemplate::parse("Вы {hero:gender|сам|сама}.").unwrap();
        assert_eq!(t.render(&[], Gender::Fem), "Вы сама.");
        assert_eq!(
            render("{1:acc}", &[Value::Phrase(Box::new(Fixed::new("Fido")))]),
            "Fido"
        );
    }

    #[test]
    fn mistakes_are_reported() {
        let err = |t: &str| RuTemplate::parse(t).unwrap_err();
        assert_eq!(
            err("Вы {1:вин}"),
            TemplateError::BadModifier("1:вин".into(), "вин".into())
        );
        assert!(matches!(err("{0}"), TemplateError::BadTarget(_)));
        assert!(matches!(
            err("{1:gender|а|б}"),
            TemplateError::Forms(_, "gender", 4, 2)
        ));
        assert!(matches!(
            err("{1|а|б}"),
            TemplateError::FormsWithoutSelector(_)
        ));
        assert!(matches!(
            err("{1:acc:num|а|б}"),
            TemplateError::SelectorWithCase(_)
        ));
        assert!(matches!(err("{1"), TemplateError::Unclosed(0)));
        assert!(matches!(err("a } b"), TemplateError::LoneBrace(2)));
        assert!(matches!(err("{hero}"), TemplateError::Hero(_)));
    }
}
