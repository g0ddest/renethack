//! The translator: a shown English text to Russian.
//!
//! 1. A message with its format (P7): the template with that format; a
//!    template derived from it that matches the text says more (a literal
//!    argument put in), so it goes first. Either takes P7's arguments,
//!    which split the text truly where two conversions touch.
//! 2. Else the text matched against every template (the Sprintf pieces
//!    among them). Where only a space parts two conversions, the lexicon
//!    says where the one name ends ("The pony's" | "saddle").
//! 3. Else the English text, reported as unknown.
//!
//! An argument of a template becomes, in this order: a translated piece of
//! the catalog ("digging", a hallucinated "jumbo shrimp" behind its
//! article), a name the lexicon parses ("the newt"), a text
//! another template makes (a buffer the engine built), any name, or the
//! English text (the translation is then partial). A name the player typed
//! (the hero's, a fruit's) is shown as typed.

use crate::catalog::{Catalog, Channel, Match, Template, Use, plausible};
use crate::format::{ConvKind, Segment, convs};
use crate::grammar::{Case, Gender};
use crate::phrase::{NameKind, Names, Phrase};
use crate::russian::{Russian, Translation};
use crate::template::{Part, Placeholder, RuTemplate, Target, Value, capitalize};

/// How deep a text made of texts is followed.
const MAX_NESTING: usize = 3;
/// A template with fewer letters of its own than this says almost nothing
/// ("%s of %s", "%s (%s)", "the %s", "%s and %s"): a name the lexicon reads
/// goes first.
const STRONG_LETTERS: usize = 4;
/// How many splits of the conversions that spaces part are weighed.
const MAX_SPLITS: usize = 256;

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
        let found = self.best_match(text, channel);
        let mut unmarked = None;
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
            match self.without_mark(text, channel) {
                Some(out) if out.status != Status::Untranslated => return out,
                other => unmarked = other,
            }
        }
        let Some(m) = found else {
            return unmarked
                .or_else(|| self.padded(text, channel))
                .unwrap_or_else(|| Output::english(text, Status::Unknown, None));
        };
        match self.render_or_base(&m, text, 0) {
            // a reading without the mark that has no Russian hides none of
            // this one's: "What do you want to burn into the %s here?", to
            // the catalog only a piece, is no "%s into %s"
            Some(out) if out.status == Status::Translated || unmarked.is_none() => out,
            // no Russian for the template yet: a heading the glossary
            // names ("Armor") still reads as a name, another template
            // may hold it all ("x - 12 gold pieces." is "%c - %s."
            // more than "%s gold %s.")
            _ => unmarked
                .or_else(|| self.whole_name(text))
                .or_else(|| self.fallback(text, channel))
                .unwrap_or_else(|| Output::english(text, Status::Untranslated, Some(m.template))),
        }
    }

    /// The best template for `text`; of those the catalog ranks alike (as
    /// much literal text, as many conversions), the one whose arguments
    /// the lexicon reads best: "killed by a gas spore's explosion" is
    /// "killed by %s" more than "%s explosion".
    fn best_match(&self, text: &str, channel: Channel) -> Option<Match<'_>> {
        let mut all = self.catalog.matches(text, channel);
        let key = |m: &Match| (m.template.literal_len(), m.template.arity());
        let top = key(all.first()?);
        let tied = all.iter().take_while(|m| key(m) == top).count();
        let best = if tied < 2 {
            0
        } else {
            (0..tied)
                .min_by_key(|&k| {
                    let m = &all[k];
                    self.unread_run(m.template, 0, &self.resplit(m.template, &m.captures))
                })
                .unwrap_or(0)
        };
        Some(all.swap_remove(best))
    }

    /// The text by another template it matches, when its best one has no
    /// Russian, and if all its arguments are Russian then: "x - 12 gold
    /// pieces." by "%c - %s." rather than "%s gold %s.", "drowned in a
    /// pool of water" by "drowned in %s" rather than "%spool of water".
    fn fallback(&self, text: &str, channel: Channel) -> Option<Output> {
        self.catalog
            .matches(text, channel)
            .iter()
            .filter_map(|m| self.render_match(m, 0))
            .find(|out| out.status == Status::Translated)
    }

    /// A text no template makes with its spaces, translated without them
    /// and put back between them: a menu's heading (" General   "), an
    /// option's help ("    can your character hear anything").
    fn padded(&self, text: &str, channel: Channel) -> Option<Output> {
        let bare = text.trim_matches(' ');
        if bare.is_empty() || bare.len() == text.len() {
            return None;
        }
        let out = self.by_text(bare, channel);
        (out.status != Status::Unknown).then(|| {
            let start = text.len() - text.trim_start_matches(' ').len();
            Output {
                text: format!(
                    "{}{}{}",
                    &text[..start],
                    out.text,
                    &text[start + bare.len()..]
                ),
                ..out
            }
        })
    }

    /// The whole text as a name the lexicon reads, capitalised as it was.
    fn whole_name(&self, text: &str) -> Option<Output> {
        let bare = text.trim();
        let p = self.names.parse(NameKind::Any, bare)?;
        let ru = p.form(Case::Nom);
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
        let captures = self.resplit(m.template, &m.captures);
        self.render_captures(m.template, &captures, depth)
    }

    /// The captures of a match by its text, the conversions that only
    /// spaces part ("%s %s falls to the %s.") split where the lexicon reads
    /// them best. The matcher gives the first of them the shortest text
    /// ("The" | "pony's saddle"); the split that leaves the fewest words
    /// unread wins ("The pony's" | "saddle"), an owner going with the name
    /// before it.
    fn resplit(&self, t: &Template, captures: &[String]) -> Vec<String> {
        let mut caps = captures.to_vec();
        let kinds: Vec<ConvKind> = convs(&t.segments).map(|c| c.kind).collect();
        if caps.len() != kinds.len() {
            return caps;
        }
        let joints = joints(&t.segments);
        let mut i = 0;
        while i < caps.len() {
            let mut j = i;
            while j + 1 < caps.len()
                && kinds[j] == ConvKind::Str
                && kinds[j + 1] == ConvKind::Str
                && joints[j].is_some()
            {
                j += 1;
            }
            if j > i {
                let seps: Vec<&str> = joints[i..j].iter().map(|s| s.unwrap_or("")).collect();
                let mut joined = caps[i].clone();
                for (sep, cap) in seps.iter().zip(&caps[i + 1..=j]) {
                    joined.push_str(sep);
                    joined.push_str(cap);
                }
                let mut best = (self.unread_run(t, i, &caps[i..=j]), caps[i..=j].to_vec());
                let mut tried = 0;
                splits(&joined, &seps, &mut Vec::new(), &mut |pieces| {
                    tried += 1;
                    let fits = pieces
                        .iter()
                        .enumerate()
                        .all(|(k, p)| plausible(t, i + k, ConvKind::Str, p));
                    if fits {
                        let unread = self.unread_run(t, i, pieces);
                        if unread < best.0 {
                            best = (unread, pieces.iter().map(|p| p.to_string()).collect());
                        }
                    }
                    tried < MAX_SPLITS
                });
                caps.splice(i..=j, best.1);
            }
            i = j + 1;
        }
        caps
    }

    /// The words of a run of conversions (from conversion `first`) the
    /// lexicon does not read, and one more for an owner ("the gnome's")
    /// left at the head of a name after another name: it is that one's.
    fn unread_run<S: AsRef<str>>(&self, t: &Template, first: usize, pieces: &[S]) -> usize {
        pieces
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let p = p.as_ref();
                let owner = k > 0 && t.is_name(first + k - 1) && has_owner(p);
                self.unread(t, first + k, p) + usize::from(owner)
            })
            .sum()
    }

    /// The words of conversion `i`'s text the lexicon does not read: none
    /// of a translated piece, those a name keeps English, all of a text.
    fn unread(&self, t: &Template, i: usize, text: &str) -> usize {
        let text = text.trim();
        let words = english_words(text);
        if words == 0 || t.is_typed(i) || self.piece(text).is_some() {
            return 0;
        }
        // a monster's name the lexicon knows (a monster read leniently is
        // any name at all)
        let kind = match t.name_kind(i) {
            NameKind::Object => NameKind::Object,
            NameKind::Word => NameKind::Word,
            NameKind::Monster | NameKind::Any => NameKind::Any,
        };
        match self.names.parse(kind, text) {
            Some(p) => english_words(&p.form(Case::Nom)).min(words),
            None => words,
        }
    }

    /// A translated piece of the catalog that is `text` ("digging"), one
    /// behind an article ("the jumbo shrimp": a hallucinated name; "a
    /// falling rock", "a bolt of lightning"), or one the engine
    /// lower-cased ("unskilled" of "Unskilled").
    fn piece(&self, text: &str) -> Option<Box<dyn Phrase>> {
        self.bare_piece(text).or_else(|| {
            // "your jumbo shrimp": a hallucinated pet
            let rest = text
                .strip_prefix("your ")
                .or_else(|| text.strip_prefix("Your "))?;
            self.bare_piece(rest)
                .map(|p| Box::new(Owned(p)) as Box<dyn Phrase>)
        })
    }

    /// A translated piece of the catalog that is `text`, behind an
    /// article, lower-cased, or in the plural ("the jumbo shrimps", when
    /// its translation gives the plural's forms).
    fn bare_piece(&self, text: &str) -> Option<Box<dyn Phrase>> {
        let translation = |s: &str| {
            let i = self.catalog.by_fmt(&s.replace('%', "%%"))?;
            let t = &self.catalog.templates()[i];
            if t.arity() > 0 {
                return None;
            }
            self.russian.get(&t.id)
        };
        let find = |s: &str| translation(s).map(Translation::phrase);
        let bare = ["the ", "The ", "a ", "A ", "an ", "An "]
            .iter()
            .find_map(|a| text.strip_prefix(a));
        find(text)
            .or_else(|| find(bare?))
            .or_else(|| {
                let first = text.chars().next().filter(|c| c.is_lowercase())?;
                let upper = capitalize(text);
                (!upper.starts_with(first))
                    .then(|| find(&upper))
                    .flatten()
                    .map(|p| Box::new(Lowered(p)) as Box<dyn Phrase>)
            })
            .or_else(|| {
                singulars(bare.unwrap_or(text))
                    .iter()
                    .find_map(|s| translation(s)?.plural_phrase())
            })
    }

    /// A matched template whose translation gives its case forms, as a
    /// phrase of them ("взрыв газовой споры", "взрывом газовой споры"…),
    /// and whether it is Russian.
    fn declined(&self, m: &Match, depth: usize) -> Option<(Value, bool)> {
        let t = m.template;
        let tr = self.russian.get(&t.id)?;
        let forms = tr.form_templates.as_ref()?;
        let captures = self.resplit(t, &m.captures);
        let mut whole = true;
        let values: Vec<Value> = convs(&t.segments)
            .zip(&captures)
            .enumerate()
            .map(|(i, (c, cap))| {
                let (v, ok) = self.arg_value(t, i, c.kind, cap, depth);
                whole &= ok;
                v
            })
            .collect();
        let forms: Vec<String> = forms.iter().map(|f| f.render(&values, self.hero)).collect();
        let (gender, number) = tr
            .gender
            .unwrap_or((Gender::Masc, crate::grammar::Number::Sing));
        Some((
            Value::Phrase(Box::new(Declined {
                forms,
                gender,
                number,
            })),
            whole,
        ))
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
        let upper = depth == 0 && starts_upper(t, captures);
        Some(self.output(t, &ru, &values, whole, upper))
    }

    /// The Russian of template `index` with the arguments P7 sent; None
    /// when it has none or the arguments do not fit its conversions.
    fn render_args(&self, index: usize, args: &[Arg]) -> Option<Output> {
        let t = &self.catalog.templates()[index];
        self.render_captures(t, &shown_args(t, args)?, 0)
    }

    /// The Russian of `t` with its values, a capital first where the
    /// English has one.
    fn output(
        &self,
        t: &Template,
        ru: &RuTemplate,
        values: &[Value],
        whole: bool,
        upper: bool,
    ) -> Output {
        let text = ru.render(values, self.hero);
        Output {
            text: if upper { capitalize(&text) } else { text },
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
        if let Some(p) = self.piece(shown) {
            return (Value::Phrase(p), true);
        }
        // a name read whole is the surest reading, even of a slot that
        // says nothing of its kind: "the goblin" is no "the %s"
        if let Some(p) = self.names.parse(name, shown) {
            return (Value::Phrase(p), true);
        }
        // an owner, read without its "'s" ("Shan Lai Ching's altar"): the
        // Russian says whose by a case
        if let Some(p) = owner(shown).and_then(|o| self.names.parse(name, o)) {
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
        if let Some(m) = found {
            // a text with case forms takes the case asked of it
            if let Some(declined) = self.declined(&m, depth + 1) {
                return declined;
            }
            if let Some(out) = self.render_or_base(&m, shown, depth + 1) {
                let ok = out.status == Status::Translated;
                return (Value::Text(out.text), ok);
            }
        }
        // what stays English: a name or a code of one word (a pet's name,
        // inventory letters "aefgh") is shown as it is; words are not
        (Value::Text(shown.to_string()), verbatim(shown))
    }
}

/// For each conversion but the last, the literal between it and the next
/// when only spaces are (or nothing is): there the matcher's split is a
/// guess.
fn joints(segments: &[Segment]) -> Vec<Option<&str>> {
    let mut out = Vec::new();
    for (k, s) in segments.iter().enumerate() {
        if !matches!(s, Segment::Conv(_)) {
            continue;
        }
        out.push(match (segments.get(k + 1), segments.get(k + 2)) {
            (Some(Segment::Conv(_)), _) => Some(""),
            (Some(Segment::Lit(l)), Some(Segment::Conv(_))) if l.chars().all(|c| c == ' ') => {
                Some(l.as_str())
            }
            _ => None,
        });
    }
    out.pop();
    out
}

/// Every split of `text` into one piece more than `seps`, piece k and k + 1
/// parted by `seps[k]` (anywhere for ""), shortest first; `each` says
/// whether to go on.
fn splits<'a>(
    text: &'a str,
    seps: &[&str],
    pieces: &mut Vec<&'a str>,
    each: &mut dyn FnMut(&[&'a str]) -> bool,
) -> bool {
    let Some((sep, rest)) = seps.split_first() else {
        pieces.push(text);
        let go_on = each(pieces);
        pieces.pop();
        return go_on;
    };
    let ends: Vec<usize> = if sep.is_empty() {
        text.char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect()
    } else {
        text.match_indices(sep).map(|(i, _)| i).collect()
    };
    for end in ends {
        pieces.push(&text[..end]);
        let go_on = splits(&text[end + sep.len()..], rest, pieces, each);
        pieces.pop();
        if !go_on {
            return false;
        }
    }
    true
}

/// The words of a text in Latin letters.
fn english_words(text: &str) -> usize {
    text.split(|c: char| !(c.is_ascii_alphabetic() || c == '\''))
        .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
        .count()
}

/// The name of an owner ("Shan Lai Ching's", "Croesus'"), without its
/// mark.
fn owner(text: &str) -> Option<&str> {
    text.strip_suffix("'s")
        .or_else(|| text.strip_suffix('\'').filter(|o| o.ends_with('s')))
        .filter(|o| !o.is_empty())
}

/// Does a text hold an owner before more words ("gnome's hand",
/// "the dogs' bowl")?
fn has_owner(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    words.len() > 1
        && words[..words.len() - 1]
            .iter()
            .any(|w| w.ends_with("'s") || (w.ends_with("s'") && w.len() > 2))
}

/// Does the English that template `t` printed as `captures` start with a
/// capital? A sentence does; a line that goes on with one ("and 45
/// pieces of gold, after 678 moves."), a menu's item ("a - a +1 long
/// sword", "autopickup [X]") and a name ("newt corpse") do not.
fn starts_upper(t: &Template, captures: &[String]) -> bool {
    let mut caps = captures.iter();
    t.segments
        .iter()
        .find_map(|s| {
            let text = match s {
                Segment::Lit(l) => l.as_str(),
                Segment::Conv(_) => caps.next().map_or("", String::as_str),
            };
            text.chars().find(|c| c.is_alphabetic())
        })
        .is_some_and(char::is_uppercase)
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

/// A text another template made, in the case forms its translation
/// gives.
struct Declined {
    forms: Vec<String>,
    gender: Gender,
    number: crate::grammar::Number,
}

impl Phrase for Declined {
    fn form(&self, case: Case) -> String {
        self.forms.get(case.index()).cloned().unwrap_or_default()
    }

    fn gender(&self) -> Gender {
        self.gender
    }

    fn number(&self) -> crate::grammar::Number {
        self.number
    }
}

/// What a regular English plural can be the plural of: "shrimps" of
/// "shrimp", "foxes" of "fox", "flies" of "fly", "firemen" of "fireman";
/// "bluebirds of happiness" of "bluebird of happiness" (makeplural()
/// makes the words before " of " plural).
fn singulars(text: &str) -> Vec<String> {
    if let Some((head, tail)) = text.split_once(" of ") {
        return singulars(head)
            .into_iter()
            .map(|h| format!("{h} of {tail}"))
            .collect();
    }
    let mut out = Vec::new();
    if let Some(s) = text.strip_suffix("ies") {
        out.push(format!("{s}y"));
    }
    if let Some(s) = text.strip_suffix("men") {
        out.push(format!("{s}man"));
    }
    if let Some(s) = text.strip_suffix("es") {
        out.push(s.to_string());
    }
    if let Some(s) = text.strip_suffix('s') {
        out.push(s.to_string());
    }
    out
}

/// A piece with "your" before it: ваш, agreeing, or свой where the hero
/// does the thing.
struct Owned(Box<dyn Phrase>);

impl Owned {
    /// The pronoun of `table` (ваш, свой: the stem and its endings by
    /// gender) agreeing with the piece in `case`.
    fn pronoun(&self, table: [&str; 4], case: Case) -> String {
        let row = match (self.0.number(), self.0.gender()) {
            (crate::grammar::Number::Plur, _) => table[3],
            (_, Gender::Masc) => table[0],
            (_, Gender::Fem) => table[1],
            (_, Gender::Neut) => table[2],
        };
        let forms: Vec<&str> = row.split(' ').collect();
        let k = case.index().min(5);
        // a creature's accusative is its genitive (вашего, ваших)
        let animate = self.0.form(Case::Acc) == self.0.form(Case::Gen);
        let k = if k == 3 && animate && row != table[1] && row != table[2] {
            1
        } else {
            k
        };
        forms[k].to_string()
    }
}

const VASH: [&str; 4] = [
    "ваш вашего вашему ваш вашим вашем",
    "ваша вашей вашей вашу вашей вашей",
    "ваше вашего вашему ваше вашим вашем",
    "ваши ваших вашим ваши вашими ваших",
];
const SVOJ: [&str; 4] = [
    "свой своего своему свой своим своём",
    "своя своей своей свою своей своей",
    "своё своего своему своё своим своём",
    "свои своих своим свои своими своих",
];

impl Phrase for Owned {
    fn form(&self, case: Case) -> String {
        format!("{} {}", self.pronoun(VASH, case), self.0.form(case))
    }

    fn own(&self, case: Case) -> String {
        format!("{} {}", self.pronoun(SVOJ, case), self.0.form(case))
    }

    fn gender(&self) -> Gender {
        self.0.gender()
    }

    fn number(&self) -> crate::grammar::Number {
        self.0.number()
    }
}

/// A piece the engine lower-cased: its forms with a small first letter.
struct Lowered(Box<dyn Phrase>);

impl Phrase for Lowered {
    fn form(&self, case: Case) -> String {
        lower_first(&self.0.form(case))
    }

    fn gender(&self) -> Gender {
        self.0.gender()
    }

    fn number(&self) -> crate::grammar::Number {
        self.0.number()
    }
}

fn lower_first(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_lowercase().chain(cs).collect(),
        None => String::new(),
    }
}

/// A phrase with the spaces that stood around its English.
struct Padded {
    inner: Box<dyn Phrase>,
    before: String,
    after: String,
}

impl Phrase for Padded {
    fn form(&self, case: Case) -> String {
        format!("{}{}{}", self.before, self.inner.form(case), self.after)
    }

    fn gender(&self) -> Gender {
        self.inner.gender()
    }

    fn number(&self) -> crate::grammar::Number {
        self.inner.number()
    }

    fn counted(&self, n: u64, case: Case) -> String {
        format!(
            "{}{}{}",
            self.before,
            self.inner.counted(n, case),
            self.after
        )
    }

    fn own(&self, case: Case) -> String {
        format!("{}{}{}", self.before, self.inner.own(case), self.after)
    }

    fn agreeing(&self, gender: Gender, number: crate::grammar::Number, case: Case) -> String {
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
    fn where_spaces_part_conversions() {
        let joined = |fmt: &str| {
            joints(&parse_format(fmt))
                .into_iter()
                .map(|j| j.map(str::to_string))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            joined("%s %s falls to the %s."),
            [Some(" ".to_string()), None]
        );
        assert_eq!(joined("%s%s is empty."), [Some(String::new())]);
        assert_eq!(joined("%s hits %s."), [None]);
        let mut all = Vec::new();
        splits("The giant beetle's", &[" "], &mut Vec::new(), &mut |p| {
            all.push(p.join("|"));
            true
        });
        assert_eq!(all, ["The|giant beetle's", "The giant|beetle's"]);
        assert!(has_owner("gnome's hand"));
        assert!(has_owner("the dogs' bowl"));
        assert!(!has_owner("The gnome's"));
        assert_eq!(english_words("плащ beetle"), 1);
    }

    #[test]
    fn what_an_english_plural_is_the_plural_of() {
        for (plural, one) in [
            ("jumbo shrimps", "jumbo shrimp"),
            ("giant pigmies", "giant pigmy"),
            ("frog princes", "frog prince"),
            ("particle men", "particle man"),
            ("bluebirds of happiness", "bluebird of happiness"),
        ] {
            assert!(
                singulars(plural).iter().any(|s| s == one),
                "{plural}: {:?}",
                singulars(plural)
            );
        }
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
