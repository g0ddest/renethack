//! The translator: a shown English text to Russian.
//!
//! 1. A message with its format (P7): the template with that format; a
//!    template derived from it that matches the text says more (a literal
//!    argument put in), so it goes first. Either takes P7's arguments,
//!    which split the text truly where two conversions touch.
//! 2. Else the text matched against every template (the Sprintf pieces
//!    among them).
//! 3. Else the English text, reported as unknown.
//!
//! An argument of a template becomes, in this order: a translated piece of
//! the catalog ("digging"), a name the lexicon parses ("the newt"), a text
//! another template makes (a buffer the engine built), any name, or the
//! English text (the translation is then partial). A name the player typed
//! (the hero's, a fruit's) is shown as typed.

use crate::catalog::{Catalog, Channel, Match, Template, Use};
use crate::format::{ConvKind, Segment, convs};
use crate::grammar::Gender;
use crate::phrase::{NameKind, Names, Phrase};
use crate::russian::Russian;
use crate::template::{Part, Placeholder, RuTemplate, Target, Value, capitalize};

/// How deep a text made of texts is followed.
const MAX_NESTING: usize = 3;
/// A template with fewer letters of its own than this says almost nothing
/// ("%s of %s", "%s (%s)", "the %s", "%s and %s"): a name the lexicon reads
/// goes first.
const STRONG_LETTERS: usize = 4;

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
            if let Some(m) = &found {
                // the arguments P7 sent split the text truly ("the
                // gnome's" | "hand"); the text's captures only guess
                let base = &self.catalog.templates()[i];
                let rendered = match p7_captures(base, m.template, args) {
                    Some(captures) => self.render_captures(m.template, &captures, 0),
                    None => self.render_match(m, 0),
                };
                if let Some(out) = rendered {
                    return out;
                }
            }
            // a format of conversions alone ("%s %s%s%s") says nothing its
            // derived templates did not: the text decides
            if self.catalog.templates()[i].letters() == 0 {
                return self.by_text(text, Channel::Message);
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

    /// A name the engine printed alone (a role, a monster, an object, a
    /// word of the status line): the lexicon reads it, in the nominative,
    /// capitalised as the English was; else it is a text.
    pub fn name(&self, english: &str) -> Output {
        self.whole_name(english)
            .unwrap_or_else(|| self.text(english))
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
        // a window of paragraphs, one a text of the catalog (the Oracle's
        // words under its heading), else line by line
        let parts: Vec<Output> = if text.contains("\n\n") {
            text.split("\n\n").map(|p| self.paragraph(p)).collect()
        } else {
            text.split('\n').map(|l| self.line(l)).collect()
        };
        let sep = if text.contains("\n\n") { "\n\n" } else { "\n" };
        Output {
            text: parts
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join(sep),
            status: worst(&parts),
            template: None,
        }
    }

    /// Is a window one the client lays out from its English (the
    /// tombstone, #overview, the vanquished): a line of it a text the
    /// catalog knows only as layout?
    pub fn is_layout(&self, text: &str) -> bool {
        text.lines().filter(|l| !l.trim().is_empty()).any(|l| {
            self.catalog.find(l, Channel::Window).is_some_and(|m| {
                let uses = &m.template.uses;
                uses.contains(&Use::Layout)
                    && uses.iter().all(|u| matches!(u, Use::Layout | Use::Piece))
            })
        })
    }

    /// A paragraph of a window: a text of the catalog as a whole, else
    /// line by line.
    fn paragraph(&self, text: &str) -> Output {
        let whole = self.by_text(text, Channel::Window);
        if whole.status != Status::Unknown || !text.contains('\n') {
            return if text.trim().is_empty() {
                Output::english(text, Status::Translated, None)
            } else {
                whole
            };
        }
        let lines: Vec<Output> = text.split('\n').map(|l| self.line(l)).collect();
        Output {
            text: lines
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            status: worst(&lines),
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
            .is_none_or(|m| !says_more_than_a_name(m.template))
        {
            // a template that says almost nothing ("%s of %s") loses to a
            // name the lexicon reads whole ("a scroll of identify"), and to
            // a text the catalog knows without its last mark ("The Gnomish
            // Mines:")
            if let Some(out) = self.whole_name(text) {
                return out;
            }
            if let Some(out) = self.without_mark(text, channel) {
                return out;
            }
        }
        match found {
            Some(m) => self.render_or_base(&m, text, 0).unwrap_or_else(|| {
                // no Russian for the template yet: a heading the glossary
                // names ("Armor") still reads as a name
                match self.whole_name(text) {
                    Some(out) => out,
                    None => Output::english(text, Status::Untranslated, Some(m.template)),
                }
            }),
            None => Output::english(text, Status::Unknown, None),
        }
    }

    /// The whole text as a name the lexicon reads, capitalised as it was.
    fn whole_name(&self, text: &str) -> Option<Output> {
        let bare = text.trim();
        let p = self.names.parse(NameKind::Any, bare)?;
        let ru = p.form(crate::grammar::Case::Nom);
        let upper = bare.chars().next().is_some_and(char::is_uppercase);
        Some(Output {
            text: if upper { capitalize(&ru) } else { ru },
            status: Status::Translated,
            template: None,
        })
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
            // a base of conversions alone would say the English again
            (base.template.letters() > 0)
                .then(|| self.render_match(&base, depth))
                .flatten()
        })
    }

    /// The Russian of a matched template, None when it has none.
    fn render_match(&self, m: &Match, depth: usize) -> Option<Output> {
        self.render_captures(m.template, &m.captures, depth)
    }

    /// The Russian of template `t` with its conversions printed as
    /// `captures`, None when it has none.
    fn render_captures(&self, t: &Template, captures: &[String], depth: usize) -> Option<Output> {
        let ru = self.russian_of(t)?;
        let mut whole = true;
        let values: Vec<Value> = convs(&t.segments)
            .zip(captures)
            .enumerate()
            .map(|(i, (c, cap))| {
                let (v, ok) = self.arg_value(t, i, c.kind, cap, depth);
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
        self.render_captures(t, &shown_args(t, args)?, 0)
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
        // a text starts with a capital, but a line that goes on with a
        // sentence ("and 45 pieces of gold, after 678 moves.") does not
        let goes_on = t.fmt.chars().next().is_some_and(char::is_lowercase);
        Output {
            text: if depth == 0 && !goes_on {
                capitalize(&text)
            } else {
                text
            },
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

    /// Conversion `i` of template `t` printed as `shown`: its value, and
    /// whether it is Russian. A name the player typed is shown as typed.
    fn arg_value(
        &self,
        t: &Template,
        i: usize,
        kind: ConvKind,
        shown: &str,
        depth: usize,
    ) -> (Value, bool) {
        if t.is_typed(i) {
            return (Value::Text(shown.to_string()), true);
        }
        self.value(kind, shown, t.name_kind(i), depth)
    }

    /// An argument's value, and whether it is Russian.
    fn value(&self, kind: ConvKind, shown: &str, name: NameKind, depth: usize) -> (Value, bool) {
        match kind {
            // a number keeps its print where it says more than its digits
            // (a sign, zero padding: "+3", "18/02")
            ConvKind::Int => match shown.trim().parse::<i64>() {
                Ok(n) if n.to_string() == shown.trim() => (Value::Number(n), true),
                _ => (Value::Text(shown.to_string()), true),
            },
            ConvKind::Str => self.text_value(shown, name, depth),
            ConvKind::Char | ConvKind::Float | ConvKind::Other => {
                (Value::Text(shown.to_string()), true)
            }
        }
    }

    /// An argument printed with `%s`: the padding of a width (or the
    /// spaces the format put inside it) stays around what it becomes.
    fn text_value(&self, shown: &str, name: NameKind, depth: usize) -> (Value, bool) {
        let trimmed = shown.trim();
        if trimmed.len() == shown.len() {
            return self.bare_text_value(shown, name, depth);
        }
        let start = shown.len() - shown.trim_start().len();
        let (before, after) = (&shown[..start], &shown[start + trimmed.len()..]);
        match self.bare_text_value(trimmed, name, depth) {
            (Value::Phrase(p), ok) => (
                Value::Phrase(Box::new(Padded {
                    inner: p,
                    before: before.to_string(),
                    after: after.to_string(),
                })),
                ok,
            ),
            (Value::Text(t), ok) => (Value::Text(format!("{before}{t}{after}")), ok),
            (v, ok) => (v, ok),
        }
    }

    fn bare_text_value(&self, shown: &str, name: NameKind, depth: usize) -> (Value, bool) {
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
        // a name read whole is the surest reading, even of a slot that
        // says nothing of its kind: "the goblin" is no "the %s"
        if let Some(p) = self.names.parse(name, shown) {
            return (Value::Phrase(p), true);
        }
        let found = if depth < MAX_NESTING {
            self.catalog
                .find(shown, Channel::Any)
                .filter(|m| m.template.arity() > 0)
        } else {
            None
        };
        // a text another template makes, unless that template says almost
        // nothing and the lexicon reads the text as a name
        let strong = found
            .as_ref()
            .is_some_and(|m| says_more_than_a_name(m.template));
        if !strong && let Some(p) = self.names.parse(NameKind::Any, shown) {
            return (Value::Phrase(p), true);
        }
        if let Some(m) = found
            && let Some(out) = self.render_or_base(&m, shown, depth + 1)
        {
            let ok = out.status == Status::Translated;
            return (Value::Text(out.text), ok);
        }
        // what stays English: a name or a code of one word (a pet's name,
        // inventory letters "aefgh") is shown as it is; words are not
        (Value::Text(shown.to_string()), verbatim(shown))
    }
}

/// The least translated of some outputs.
fn worst(parts: &[Output]) -> Status {
    parts
        .iter()
        .map(|o| o.status)
        .max_by_key(|s| match s {
            Status::Translated => 0,
            Status::Partial => 1,
            Status::Untranslated => 2,
            Status::Unknown => 3,
        })
        .unwrap_or(Status::Translated)
}

/// The arguments P7 sent for template `t`, one printed text per
/// conversion; None when they do not fit its conversions. A `*` width or
/// precision takes an argument of its own, unless the host left it out.
fn shown_args(t: &Template, args: &[Arg]) -> Option<Vec<String>> {
    let cs: Vec<_> = convs(&t.segments).collect();
    let stars: usize = cs.iter().map(|c| c.stars).sum();
    let with_stars = args.len() == cs.len() + stars && stars > 0;
    if !with_stars && args.len() != cs.len() {
        return None;
    }
    let mut k = 0;
    let mut out = Vec::new();
    for c in &cs {
        if with_stars {
            k += c.stars;
        }
        out.push(match &args[k] {
            Arg::Str(s) => s.clone(),
            Arg::Int(n) => n.to_string(),
            Arg::Num(x) => x.to_string(),
            Arg::Null => "(null)".to_string(),
        });
        k += 1;
    }
    Some(out)
}

/// The printed conversions of `derived`, a template derived from `base`,
/// taken from P7's arguments for `base`: None when the two do not line
/// up. `derived` is `base` with conversions put in as literal text
/// ("%s %s to %s %s!" → "%s welds itself to %s %s!", P7's arguments "The
/// crossbow welds", "itself", "the gnome's", "hand"): each conversion of
/// `base` became a run of `derived`, literal text and conversions, and
/// its argument must read as that run.
fn p7_captures(base: &Template, derived: &Template, args: &[Arg]) -> Option<Vec<String>> {
    let shown = shown_args(base, args)?;
    let mut runs = Vec::new();
    search(
        &toks(&base.segments),
        &toks(&derived.segments),
        &mut runs,
        &shown,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Conv,
    Char(char),
}

fn toks(segments: &[Segment]) -> Vec<Tok> {
    segments
        .iter()
        .flat_map(|s| match s {
            Segment::Lit(l) => l.chars().map(Tok::Char).collect::<Vec<_>>(),
            Segment::Conv(_) => vec![Tok::Conv],
        })
        .collect()
}

/// Lines up the base's tokens `b` with the derived template's `d`, each
/// base conversion with a run of `d` (`runs`, in order); the derived
/// conversions' texts when every argument reads as its run.
fn search<'d>(
    b: &[Tok],
    d: &'d [Tok],
    runs: &mut Vec<&'d [Tok]>,
    shown: &[String],
) -> Option<Vec<String>> {
    let Some(first) = b.first() else {
        return d.is_empty().then(|| captures(runs, shown)).flatten();
    };
    if let Tok::Char(c) = first {
        return (d.first() == Some(&Tok::Char(*c)))
            .then(|| search(&b[1..], &d[1..], runs, shown))
            .flatten();
    }
    let arg = shown.get(runs.len())?;
    for n in 0..=d.len() {
        // a run the argument does not read as goes no further
        if fit(&d[..n], arg).is_none() {
            continue;
        }
        runs.push(&d[..n]);
        if let Some(found) = search(&b[1..], &d[n..], runs, shown) {
            return Some(found);
        }
        runs.pop();
    }
    None
}

/// Each argument read as its run: the texts of the run's conversions, in
/// order; None when an argument does not read so.
fn captures(runs: &[&[Tok]], shown: &[String]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for (run, arg) in runs.iter().zip(shown) {
        out.extend(fit(run, arg)?);
    }
    (runs.len() == shown.len()).then_some(out)
}

/// `text` read as `run`: its conversions' texts, the shortest first.
fn fit(run: &[Tok], text: &str) -> Option<Vec<String>> {
    match run.first() {
        None => text.is_empty().then(Vec::new),
        Some(Tok::Char(c)) => fit(&run[1..], text.strip_prefix(*c)?),
        Some(Tok::Conv) => text
            .char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .find_map(|end| {
                let mut rest = fit(&run[1..], &text[end..])?;
                rest.insert(0, text[..end].to_string());
                Some(rest)
            }),
    }
}

/// Is a text fine shown as it is in a Russian sentence: one word at most
/// (a name, letters, a number), not English words?
fn verbatim(s: &str) -> bool {
    s.split(|c: char| !c.is_alphabetic())
        .filter(|w| w.chars().count() >= 2)
        .count()
        <= 1
}

/// Does a template say more than a name the lexicon reads in the same
/// text? Not when it has almost no words of its own ("%s of %s"), nor when
/// it is only a piece of a longer text, as the pieces names are built of
/// are ("%s corpse").
fn says_more_than_a_name(t: &Template) -> bool {
    t.letters() >= STRONG_LETTERS && !t.uses.iter().all(|u| *u == crate::catalog::Use::Piece)
}

/// A phrase with the spaces that stood around its English.
struct Padded {
    inner: Box<dyn Phrase>,
    before: String,
    after: String,
}

impl Phrase for Padded {
    fn form(&self, case: crate::grammar::Case) -> String {
        format!("{}{}{}", self.before, self.inner.form(case), self.after)
    }

    fn gender(&self) -> Gender {
        self.inner.gender()
    }

    fn number(&self) -> crate::grammar::Number {
        self.inner.number()
    }

    fn counted(&self, n: u64, case: crate::grammar::Case) -> String {
        format!(
            "{}{}{}",
            self.before,
            self.inner.counted(n, case),
            self.after
        )
    }

    fn own(&self, case: crate::grammar::Case) -> String {
        format!("{}{}{}", self.before, self.inner.own(case), self.after)
    }

    fn agreeing(
        &self,
        gender: Gender,
        number: crate::grammar::Number,
        case: crate::grammar::Case,
    ) -> String {
        format!(
            "{}{}{}",
            self.before,
            self.inner.agreeing(gender, number, case),
            self.after
        )
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
                    skip: false,
                    agree: None,
                    own: false,
                    source: (i + 1).to_string(),
                }));
                i += 1;
            }
        }
    }
    RuTemplate { parts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::parse_format;

    fn lined_up(base: &str, derived: &str, args: &[&str]) -> Option<Vec<String>> {
        let shown: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let derived = toks(&parse_format(derived));
        search(
            &toks(&parse_format(base)),
            &derived,
            &mut Vec::new(),
            &shown,
        )
    }

    fn texts(v: &[&str]) -> Option<Vec<String>> {
        Some(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_derived_template_takes_p7_s_arguments() {
        // the verb put in with its object, the touching names split as P7 did
        assert_eq!(
            lined_up(
                "%s %s to %s %s!",
                "%s welds itself to %s %s!",
                &["The crossbow welds", "itself", "the gnome's", "hand"]
            ),
            texts(&["The crossbow", "the gnome's", "hand"])
        );
        assert_eq!(
            lined_up("%s %s%s%s", "%s bites!", &["The newt", "bites", "", "!"]),
            texts(&["The newt"])
        );
        // one argument holding several conversions of the derived template
        assert_eq!(
            lined_up(
                "%s %s, welcome!  You are a%s.",
                "%s %s, welcome!  You are a %s male %s %s.",
                &["Konnichi wa", "Hero", " lawful male human Samurai"]
            ),
            texts(&["Konnichi wa", "Hero", "lawful", "human", "Samurai"])
        );
        // arguments that do not read as the derived template says
        assert_eq!(
            lined_up("%s %s%s%s", "%s bites!", &["The newt", "stings", "", "!"]),
            None
        );
    }
}
