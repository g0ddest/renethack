//! The translator: a shown English text to Russian.
//!
//! 1. A message with its format (P7): the template with that format; a
//!    template derived from it that matches the text says more (a literal
//!    argument put in), so it goes first.
//! 2. Else the text matched against every template (the Sprintf pieces
//!    among them).
//! 3. Else the English text, reported as unknown.
//!
//! An argument of a template becomes, in this order: a translated piece of
//! the catalog ("digging"), a name the lexicon parses ("the newt"), a text
//! another template makes (a buffer the engine built), any name, or the
//! English text (the translation is then partial).

use crate::catalog::{Catalog, Channel, Match, Template};
use crate::format::{ConvKind, Segment, convs};
use crate::grammar::Gender;
use crate::phrase::{NameKind, Names};
use crate::russian::Russian;
use crate::template::{Part, Placeholder, RuTemplate, Target, Value, capitalize};

/// How deep a text made of texts is followed.
const MAX_NESTING: usize = 3;
/// A template with fewer letters of its own than this says almost nothing
/// ("%s of %s", "%s (%s)"): a name the lexicon reads goes first.
const STRONG_LETTERS: usize = 3;

/// An argument of a message as P7 sends it.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// `%s` (as printed), `%c`.
    Str(String),
    /// The integer conversions.
    Int(i64),
    /// The floating conversions.
    Num(f64),
    /// `%s` given a null pointer.
    Null,
}

/// How much of a text is Russian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// The template and every argument.
    Translated,
    /// The template; an argument stayed English (a name the lexicon does
    /// not know, a text no template makes).
    Partial,
    /// A template matched, but it has no translation yet.
    Untranslated,
    /// No template matched.
    Unknown,
}

/// A translated text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub text: String,
    pub status: Status,
    /// The id of the template that matched.
    pub template: Option<String>,
}

impl Output {
    fn english(text: &str, status: Status, template: Option<&Template>) -> Output {
        Output {
            text: text.to_string(),
            status,
            template: template.map(|t| t.id.clone()),
        }
    }
}

/// The catalog, its Russian translations and a lexicon.
pub struct Translator {
    catalog: Catalog,
    russian: Russian,
    names: Box<dyn Names + Send + Sync>,
    hero: Gender,
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Translator({} templates, {} translated)",
            self.catalog.len(),
            self.russian.len()
        )
    }
}

impl Translator {
    pub fn new(
        catalog: Catalog,
        russian: Russian,
        names: Box<dyn Names + Send + Sync>,
    ) -> Translator {
        Translator {
            catalog,
            russian,
            names,
            hero: Gender::Masc,
        }
    }

    /// The hero's gender, for `{hero:gender|...}`.
    pub fn set_hero(&mut self, gender: Gender) {
        self.hero = gender;
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn russian(&self) -> &Russian {
        &self.russian
    }

    /// A message of the message window: its format and arguments when the
    /// engine sent them (P7), and its text.
    pub fn message(&self, fmt: Option<&str>, args: &[Arg], text: &str) -> Output {
        if let Some(i) = fmt
            .filter(|f| *f != "%s")
            .and_then(|f| self.catalog.by_fmt(f))
        {
            let derived = self.catalog.derived(i);
            let found = self.catalog.best_of(derived, text, Channel::Message);
            if let Some(m) = &found
                && let Some(out) = self.render_match(m, 0)
            {
                return out;
            }
            // the generic template with P7's arguments: a literal argument
            // is a piece, translated on its own
            if let Some(out) = self.render_args(i, args) {
                return out;
            }
            let shown = found
                .as_ref()
                .map_or(&self.catalog.templates()[i], |m| m.template);
            return Output::english(text, Status::Untranslated, Some(shown));
        }
        self.by_text(text, Channel::Message)
    }

    /// A line of a menu, a question, a heading.
    pub fn text(&self, text: &str) -> Output {
        self.by_text(text, Channel::Window)
    }

    /// The lines of a text window, joined by newlines: a text of the
    /// catalog as a whole (a quest message, an oracle), else line by line.
    pub fn window(&self, text: &str) -> Output {
        // the blank lines around a text are not part of it
        let text = text.trim_matches(|c: char| c == '\n' || c == '\r');
        let whole = self.by_text(text, Channel::Window);
        if whole.status != Status::Unknown || !text.contains('\n') {
            return whole;
        }
        let lines: Vec<Output> = text.split('\n').map(|l| self.line(l)).collect();
        let worst = lines
            .iter()
            .map(|o| o.status)
            .max_by_key(|s| match s {
                Status::Translated => 0,
                Status::Partial => 1,
                Status::Untranslated => 2,
                Status::Unknown => 3,
            })
            .unwrap_or(Status::Translated);
        Output {
            text: lines
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            status: worst,
            template: None,
        }
    }

    /// One line of a window: an empty or blank line is translated as it is.
    fn line(&self, line: &str) -> Output {
        if line.trim().is_empty() {
            return Output::english(line, Status::Translated, None);
        }
        self.by_text(line, Channel::Window)
    }

    fn by_text(&self, text: &str, channel: Channel) -> Output {
        let found = self.catalog.find(text, channel);
        if found
            .as_ref()
            .is_none_or(|m| m.template.letters() < STRONG_LETTERS)
        {
            // a template that says almost nothing ("%s of %s") loses to a
            // name the lexicon reads whole ("a scroll of identify"), and to
            // a text the catalog knows without its last mark ("The Gnomish
            // Mines:")
            if let Some(p) = self.names.parse(NameKind::Any, text) {
                return Output {
                    text: p.form(crate::grammar::Case::Nom),
                    status: Status::Translated,
                    template: None,
                };
            }
            if let Some(out) = self.without_mark(text, channel) {
                return out;
            }
        }
        match found {
            Some(m) => self
                .render_or_base(&m, text, 0)
                .unwrap_or_else(|| Output::english(text, Status::Untranslated, Some(m.template))),
            None => Output::english(text, Status::Unknown, None),
        }
    }

    /// `text` but its last ':', '.', '!' or '?', translated by a template
    /// with words of its own, the mark put back.
    fn without_mark(&self, text: &str, channel: Channel) -> Option<Output> {
        let trimmed = text.trim_end();
        let mark = trimmed.chars().last().filter(|c| ":.!?".contains(*c))?;
        let inner = &trimmed[..trimmed.len() - mark.len_utf8()];
        if inner.is_empty() || inner.ends_with(mark) {
            return None;
        }
        let out = self.by_text(inner, channel);
        let strong = out
            .template
            .as_deref()
            .and_then(|id| self.catalog.by_id(id))
            .is_some_and(|t| t.letters() >= STRONG_LETTERS);
        (strong || out.status == Status::Translated).then(|| Output {
            text: format!("{}{mark}", out.text),
            ..out
        })
    }

    /// A derived template without a translation falls back to the
    /// template it derives from, matched on the same text.
    fn render_or_base(&self, m: &Match, text: &str, depth: usize) -> Option<Output> {
        self.render_match(m, depth).or_else(|| {
            let base = self.catalog.match_index(m.template.from?, text)?;
            self.render_match(&base, depth)
        })
    }

    /// The Russian of a matched template, None when it has none.
    fn render_match(&self, m: &Match, depth: usize) -> Option<Output> {
        let t = m.template;
        let ru = self.russian_of(t)?;
        let mut whole = true;
        let values: Vec<Value> = convs(&t.segments)
            .zip(&m.captures)
            .enumerate()
            .map(|(i, (c, cap))| {
                let (v, ok) = self.value(c.kind, cap, t.name_kind(i), depth);
                whole &= ok;
                v
            })
            .collect();
        Some(self.output(t, &ru, &values, whole, depth))
    }

    /// The Russian of template `index` with the arguments P7 sent; None
    /// when it has none or the arguments do not fit its conversions.
    fn render_args(&self, index: usize, args: &[Arg]) -> Option<Output> {
        let t = &self.catalog.templates()[index];
        let ru = self.russian_of(t)?;
        let cs: Vec<_> = convs(&t.segments).collect();
        let stars: usize = cs.iter().map(|c| c.stars).sum();
        // a `*` width or precision takes an argument of its own, unless the
        // host left it out
        let with_stars = args.len() == cs.len() + stars && stars > 0;
        if !with_stars && args.len() != cs.len() {
            return None;
        }
        let mut whole = true;
        let mut k = 0;
        let mut values = Vec::new();
        for (i, c) in cs.iter().enumerate() {
            if with_stars {
                k += c.stars;
            }
            let shown = match &args[k] {
                Arg::Str(s) => s.clone(),
                Arg::Int(n) => n.to_string(),
                Arg::Num(x) => x.to_string(),
                Arg::Null => "(null)".to_string(),
            };
            k += 1;
            let (v, ok) = self.value(c.kind, &shown, t.name_kind(i), 0);
            whole &= ok;
            values.push(v);
        }
        Some(self.output(t, &ru, &values, whole, 0))
    }

    fn output(
        &self,
        t: &Template,
        ru: &RuTemplate,
        values: &[Value],
        whole: bool,
        depth: usize,
    ) -> Output {
        let text = ru.render(values, self.hero);
        Output {
            text: if depth == 0 { capitalize(&text) } else { text },
            status: if whole {
                Status::Translated
            } else {
                Status::Partial
            },
            template: Some(t.id.clone()),
        }
    }

    /// A template's Russian: its translation, or for a format of
    /// punctuation and conversions only ("%c - %s.") the format itself.
    fn russian_of(&self, t: &Template) -> Option<RuTemplate> {
        if let Some(tr) = self.russian.get(&t.id) {
            return Some(tr.template.clone());
        }
        let wordless = t.segments.iter().all(|s| match s {
            Segment::Lit(l) => !l.chars().any(char::is_alphabetic),
            Segment::Conv(_) => true,
        });
        wordless.then(|| identity(t))
    }

    /// An argument's value, and whether it is Russian.
    fn value(&self, kind: ConvKind, shown: &str, name: NameKind, depth: usize) -> (Value, bool) {
        match kind {
            ConvKind::Int => match shown.trim().parse::<i64>() {
                Ok(n) => (Value::Number(n), true),
                Err(_) => (Value::Text(shown.to_string()), true),
            },
            ConvKind::Str => self.text_value(shown, name, depth),
            ConvKind::Char | ConvKind::Float | ConvKind::Other => {
                (Value::Text(shown.to_string()), true)
            }
        }
    }

    fn text_value(&self, shown: &str, name: NameKind, depth: usize) -> (Value, bool) {
        if shown.is_empty() {
            return (Value::Text(String::new()), true);
        }
        // a piece of the catalog that is translated
        if let Some(i) = self.catalog.by_fmt(&shown.replace('%', "%%")) {
            let t = &self.catalog.templates()[i];
            if t.arity() == 0
                && let Some(tr) = self.russian.get(&t.id)
            {
                return (Value::Phrase(tr.phrase()), true);
            }
        }
        if name != NameKind::Any
            && let Some(p) = self.names.parse(name, shown)
        {
            return (Value::Phrase(p), true);
        }
        if depth < MAX_NESTING
            && let Some(m) = self.catalog.find(shown, Channel::Any)
            && m.template.arity() > 0
            && let Some(out) = self.render_or_base(&m, shown, depth + 1)
        {
            let ok = out.status == Status::Translated;
            return (Value::Text(out.text), ok);
        }
        if let Some(p) = self.names.parse(NameKind::Any, shown) {
            return (Value::Phrase(p), true);
        }
        (Value::Text(shown.to_string()), false)
    }
}

/// A Russian template that is the format itself: "%c - %s." → "{1} - {2}.".
fn identity(t: &Template) -> RuTemplate {
    let mut parts = Vec::new();
    let mut i = 0;
    for s in &t.segments {
        match s {
            Segment::Lit(l) => parts.push(Part::Text(l.clone())),
            Segment::Conv(_) => {
                parts.push(Part::Place(Placeholder {
                    target: Target::Arg(i),
                    case: None,
                    cap: false,
                    select: None,
                    count_by: None,
                    source: (i + 1).to_string(),
                }));
                i += 1;
            }
        }
    }
    RuTemplate { parts }
}
