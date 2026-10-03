//! The catalog of the engine's English formats (`client/i18n/catalog.en.json`,
//! made by `tools/i18n/extract.py`) and the matching of a shown text
//! against them.

use std::collections::{HashMap, HashSet};

use aho_corasick::AhoCorasick;
use serde::Deserialize;

use crate::format::{ConvKind, Segment, convs, parse_format};
use crate::phrase::NameKind;

/// Where the catalog lives, from the repository root.
pub const CATALOG_PATH: &str = "client/i18n/catalog.en.json";
/// The catalog format this crate reads.
pub const CATALOG_FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("the catalog is not valid JSON for its schema: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the catalog has format {0}, this client reads {CATALOG_FORMAT}")]
    Format(u32),
    #[error("the catalog is inconsistent: {0}")]
    Invalid(String),
}

/// What shows a text: the message window, a menu, a question...
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Use {
    /// The pline family: the message window.
    Pline,
    /// A piece of a longer text (Sprintf, Strcpy, Strcat).
    Piece,
    /// A line of a text window.
    Window,
    /// A menu item or a menu's title.
    Menu,
    /// A question (yn_function, getlin...).
    Query,
    /// The verb of "What do you want to <verb>?".
    Getobj,
    /// What the hero is busy doing ("You stop <digging>.").
    Occupation,
    /// A cause of death.
    Death,
    /// A text of `dat/`: rumours, oracles, epitaphs, engravings, quest
    /// texts, level messages, hallucinatory names.
    Data,
    /// raw_printf and livelog lines.
    Other,
}

impl Use {
    fn parse(s: &str) -> Use {
        match s {
            "pline" => Use::Pline,
            "sprintf" => Use::Piece,
            "window" => Use::Window,
            "menu" => Use::Menu,
            "query" => Use::Query,
            "getobj" => Use::Getobj,
            "occupation" => Use::Occupation,
            "death" => Use::Death,
            "rumor" | "oracle" | "epitaph" | "engraving" | "bogusmon" | "quest" | "level"
            | "tutorial" => Use::Data,
            _ => Use::Other,
        }
    }
}

/// Where a text was shown, for the choice between equally good templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// The message window (a pline).
    Message,
    /// A text window, a menu, a question.
    Window,
    /// Anything.
    Any,
}

/// One format of the catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// The stable id: a hash of `fmt`.
    pub id: String,
    /// The printf format, as vpline sees it ("You hit %s.").
    pub fmt: String,
    pub segments: Vec<Segment>,
    /// For each conversion: what it is (monster, object, word, number,
    /// text, quest:p...); several joined by '|'.
    pub kinds: Vec<String>,
    pub uses: Vec<Use>,
    /// The generic template this one was derived from, by its index.
    pub from: Option<usize>,
    /// Derived templates exist: translate those instead.
    pub expanded: bool,
    pub sites: Vec<String>,
    /// Characters of literal text: the more, the better a match.
    literal_len: usize,
}

impl Template {
    /// How to parse conversion `i`'s text as a name.
    pub fn name_kind(&self, i: usize) -> NameKind {
        let Some(k) = self.kinds.get(i) else {
            return NameKind::Any;
        };
        let parts: Vec<&str> = k.split('|').filter(|p| *p != "text").collect();
        if parts.is_empty() {
            NameKind::Any
        } else if parts.iter().all(|p| matches!(*p, "monster" | "species")) {
            NameKind::Monster
        } else if parts.iter().all(|p| *p == "object") {
            NameKind::Object
        } else if parts.iter().all(|p| *p == "word") {
            NameKind::Word
        } else {
            NameKind::Any
        }
    }

    /// The number of conversions (placeholders {1}..{n} of a translation).
    pub fn arity(&self) -> usize {
        convs(&self.segments).count()
    }

    /// Is conversion `i` a name a lexicon declines (not a number, not a
    /// character)?
    pub fn is_name(&self, i: usize) -> bool {
        self.kinds.get(i).is_some_and(|k| {
            k.split('|')
                .any(|p| matches!(p, "monster" | "species" | "object"))
        })
    }

    pub fn literal_len(&self) -> usize {
        self.literal_len
    }

    pub fn has_use(&self, u: Use) -> bool {
        self.uses.contains(&u)
    }
}

#[derive(Deserialize)]
struct RawCatalog {
    format: u32,
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    id: String,
    fmt: String,
    #[serde(default)]
    uses: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    expanded: bool,
    #[serde(default)]
    sites: Vec<String>,
}

/// A text matched to a template: the template and what each conversion
/// printed.
#[derive(Debug, Clone)]
pub struct Match<'c> {
    pub template: &'c Template,
    pub index: usize,
    /// One per conversion, trimmed of the padding a width adds.
    pub captures: Vec<String>,
}

/// The catalog of templates and the indexes to find them.
pub struct Catalog {
    templates: Vec<Template>,
    by_id: HashMap<String, usize>,
    by_fmt: HashMap<String, usize>,
    derived: HashMap<usize, Vec<usize>>,
    /// Each template's longest literal run, for finding candidates.
    anchors: AhoCorasick,
    /// anchor pattern -> the templates it belongs to
    anchor_templates: Vec<Vec<usize>>,
}

impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Catalog({} templates)", self.templates.len())
    }
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Catalog, CatalogError> {
        let raw: RawCatalog = serde_json::from_str(json)?;
        if raw.format != CATALOG_FORMAT {
            return Err(CatalogError::Format(raw.format));
        }
        let mut templates = Vec::with_capacity(raw.entries.len());
        let mut by_id = HashMap::new();
        let mut by_fmt = HashMap::new();
        for (i, e) in raw.entries.iter().enumerate() {
            let segments = parse_format(&e.fmt);
            let arity = convs(&segments).count();
            if !e.args.is_empty() && e.args.len() != arity {
                return Err(CatalogError::Invalid(format!(
                    "{}: {} kinds for {} conversions of {:?}",
                    e.id,
                    e.args.len(),
                    arity,
                    e.fmt
                )));
            }
            let literal_len = segments
                .iter()
                .map(|s| match s {
                    Segment::Lit(l) => l.chars().count(),
                    Segment::Conv(_) => 0,
                })
                .sum();
            if by_id.insert(e.id.clone(), i).is_some() {
                return Err(CatalogError::Invalid(format!("id {} twice", e.id)));
            }
            by_fmt.insert(e.fmt.clone(), i);
            templates.push(Template {
                id: e.id.clone(),
                fmt: e.fmt.clone(),
                segments,
                kinds: if e.args.is_empty() {
                    vec!["text".into(); arity]
                } else {
                    e.args.clone()
                },
                uses: e.uses.iter().map(|u| Use::parse(u)).collect(),
                from: None,
                expanded: e.expanded,
                sites: e.sites.clone(),
                literal_len,
            });
        }
        let mut derived: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, e) in raw.entries.iter().enumerate() {
            if let Some(base) = &e.from {
                let b = *by_id.get(base).ok_or_else(|| {
                    CatalogError::Invalid(format!("{} derives from unknown {base}", e.id))
                })?;
                templates[i].from = Some(b);
                derived.entry(b).or_default().push(i);
            }
        }
        let mut patterns: Vec<String> = Vec::new();
        let mut pattern_index: HashMap<String, usize> = HashMap::new();
        let mut anchor_templates: Vec<Vec<usize>> = Vec::new();
        for (i, t) in templates.iter().enumerate() {
            let Some(anchor) = anchor(&t.segments).filter(|_| matchable(&t.segments)) else {
                continue;
            };
            let p = *pattern_index.entry(anchor.to_string()).or_insert_with(|| {
                patterns.push(anchor.to_string());
                anchor_templates.push(Vec::new());
                patterns.len() - 1
            });
            anchor_templates[p].push(i);
        }
        let anchors = AhoCorasick::new(&patterns)
            .map_err(|e| CatalogError::Invalid(format!("anchors: {e}")))?;
        Ok(Catalog {
            templates,
            by_id,
            by_fmt,
            derived,
            anchors,
            anchor_templates,
        })
    }

    pub fn len(&self) -> usize {
        self.templates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    pub fn get(&self, index: usize) -> Option<&Template> {
        self.templates.get(index)
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    pub fn by_id(&self, id: &str) -> Option<&Template> {
        self.index_of(id).map(|i| &self.templates[i])
    }

    /// The template whose format is exactly `fmt` (P7 sends it).
    pub fn by_fmt(&self, fmt: &str) -> Option<usize> {
        self.by_fmt.get(fmt).copied()
    }

    /// The templates derived from template `index`.
    pub fn derived(&self, index: usize) -> &[usize] {
        self.derived.get(&index).map_or(&[], |v| v.as_slice())
    }

    /// Every template `text` matches, the best first: the most literal
    /// text, then the fewest conversions, then the use that fits the
    /// channel.
    pub fn matches(&self, text: &str, channel: Channel) -> Vec<Match<'_>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for m in self.anchors.find_overlapping_iter(text) {
            for &i in &self.anchor_templates[m.pattern().as_usize()] {
                if !seen.insert(i) {
                    continue;
                }
                if let Some(captures) = match_template(&self.templates[i], text) {
                    out.push(Match {
                        template: &self.templates[i],
                        index: i,
                        captures,
                    });
                }
            }
        }
        out.sort_by_key(|m| rank(m, channel));
        out
    }

    /// The best template for `text`.
    pub fn find(&self, text: &str, channel: Channel) -> Option<Match<'_>> {
        self.matches(text, channel).into_iter().next()
    }

    /// Template `index` matched against `text`, when it matches.
    pub fn match_index(&self, index: usize, text: &str) -> Option<Match<'_>> {
        let t = self.templates.get(index)?;
        match_template(t, text).map(|captures| Match {
            template: t,
            index,
            captures,
        })
    }

    /// The best of templates `indexes` for `text`.
    pub fn best_of(&self, indexes: &[usize], text: &str, channel: Channel) -> Option<Match<'_>> {
        let mut found: Vec<Match> = indexes
            .iter()
            .filter_map(|&i| self.match_index(i, text))
            .collect();
        found.sort_by_key(|m| rank(m, channel));
        found.into_iter().next()
    }
}

/// Lower is better.
fn rank(m: &Match, channel: Channel) -> (std::cmp::Reverse<usize>, usize, u8, usize) {
    let t = m.template;
    let fits = match channel {
        Channel::Message => t.has_use(Use::Pline),
        Channel::Window => {
            t.has_use(Use::Window)
                || t.has_use(Use::Menu)
                || t.has_use(Use::Query)
                || t.has_use(Use::Data)
        }
        Channel::Any => true,
    };
    (
        std::cmp::Reverse(t.literal_len),
        t.arity(),
        u8::from(!fits),
        m.index,
    )
}

/// Can a text be told to be made by this format? Not when its literal
/// text is spaces and sentence punctuation ("%s %s%s%s", "%s."): those
/// match anything, and are found by their format only.
fn matchable(segments: &[Segment]) -> bool {
    segments.iter().any(|s| match s {
        Segment::Lit(l) => l
            .chars()
            .any(|c| c.is_alphabetic() || !(c.is_whitespace() || ".,!?;:'\"".contains(c))),
        Segment::Conv(_) => false,
    })
}

/// The longest literal run of a format.
fn anchor(segments: &[Segment]) -> Option<&str> {
    segments
        .iter()
        .filter_map(|s| match s {
            Segment::Lit(l) => Some(l.as_str()),
            Segment::Conv(_) => None,
        })
        .max_by_key(|l| l.len())
}

/// Match a whole text against a template: the text of each conversion, or
/// None. Conversions take the shortest text that lets the rest match;
/// a name's text is never empty nor "You", and no capture leaves a
/// bracket or a quote open.
pub fn match_template(t: &Template, text: &str) -> Option<Vec<String>> {
    let mut caps = Vec::new();
    if match_from(t, 0, text, &mut caps, 0) {
        Some(caps)
    } else {
        None
    }
}

fn match_from(t: &Template, seg: usize, text: &str, caps: &mut Vec<String>, conv: usize) -> bool {
    let Some(s) = t.segments.get(seg) else {
        return text.is_empty();
    };
    match s {
        Segment::Lit(l) => match text.strip_prefix(l.as_str()) {
            Some(rest) => match_from(t, seg + 1, rest, caps, conv),
            None => false,
        },
        Segment::Conv(c) => {
            let next_lit = match t.segments.get(seg + 1) {
                Some(Segment::Lit(l)) => Some(l.as_str()),
                _ => None,
            };
            for end in candidate_ends(c.kind, c.width, c.precision, text, next_lit) {
                let piece = &text[..end];
                if !plausible(t, conv, c.kind, piece) {
                    continue;
                }
                caps.push(piece.trim().to_string());
                if match_from(t, seg + 1, &text[end..], caps, conv + 1) {
                    return true;
                }
                caps.pop();
            }
            false
        }
    }
}

/// Where the text of a conversion may end, shortest first.
fn candidate_ends(
    kind: ConvKind,
    width: Option<usize>,
    precision: Option<usize>,
    text: &str,
    next_lit: Option<&str>,
) -> Vec<usize> {
    match kind {
        ConvKind::Int | ConvKind::Float => {
            let b = text.as_bytes();
            let mut i = 0;
            if width.is_some() {
                while i < b.len() && b[i] == b' ' {
                    i += 1;
                }
            }
            if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                i += 1;
            }
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_digit() || (kind == ConvKind::Float && b[i] == b'.'))
            {
                i += 1;
            }
            if i > start { vec![i] } else { vec![] }
        }
        ConvKind::Char => text
            .chars()
            .next()
            .map(|c| vec![c.len_utf8()])
            .unwrap_or_default(),
        ConvKind::Str | ConvKind::Other => {
            let mut ends: Vec<usize> = match next_lit {
                // the next literal must follow: only where it starts
                Some(l) => text.match_indices(l).map(|(i, _)| i).collect(),
                None => vec![text.len()],
            };
            if let Some(p) = precision {
                ends.retain(|&e| text[..e].chars().count() <= p);
            }
            let _ = width;
            ends
        }
    }
}

const NOT_NAMES: [&str; 4] = ["You", "you", "Your", "your"];

fn plausible(t: &Template, conv: usize, kind: ConvKind, piece: &str) -> bool {
    if kind != ConvKind::Str {
        return true;
    }
    if t.is_name(conv) {
        let p = piece.trim();
        if p.is_empty() || NOT_NAMES.contains(&p) {
            return false;
        }
    }
    balanced(piece)
}

/// No bracket closes before it opens or stays open, quotes come in pairs.
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && s.matches('"').count().is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"{"format": 1, "engine": "test", "count": 9, "entries": [
{"id": "a1", "fmt": "You hit %s.", "uses": ["pline"], "args": ["monster"], "sites": ["src/uhitm.c:1 f You(mon_nam(mon))"]},
{"id": "a2", "fmt": "%s %s%s%s", "uses": ["pline"], "args": ["monster", "text", "text", "text"], "expanded": true, "sites": []},
{"id": "a3", "fmt": "%s bites!", "uses": ["pline"], "args": ["monster"], "from": "a2", "sites": []},
{"id": "a4", "fmt": "%s hit the %s.", "uses": ["pline"], "args": ["object", "word"], "sites": []},
{"id": "a5", "fmt": "%c - %.*s.", "uses": ["sprintf"], "args": ["char", "object"], "sites": []},
{"id": "a6", "fmt": "%s in %s", "uses": ["sprintf"], "args": ["text", "text"], "sites": []},
{"id": "a7", "fmt": "You have %d gold pieces.", "uses": ["pline"], "args": ["number"], "sites": []},
{"id": "a8", "fmt": "There is %s here.", "uses": ["pline"], "args": ["text"], "sites": []},
{"id": "a9", "fmt": "a doorway", "uses": ["sprintf"], "sites": []}
]}"#;

    fn sample() -> Catalog {
        Catalog::parse(SAMPLE).unwrap()
    }

    #[test]
    fn the_most_literal_template_wins() {
        let c = sample();
        let m = c.find("You hit the newt.", Channel::Message).unwrap();
        assert_eq!(m.template.id, "a1");
        assert_eq!(m.captures, vec!["the newt"]);
        // "%s hit the %s." would take "You" for an object
        let all: Vec<_> = c
            .matches("You hit the newt.", Channel::Message)
            .iter()
            .map(|m| m.template.id.clone())
            .collect();
        assert!(!all.contains(&"a4".to_string()), "{all:?}");
    }

    #[test]
    fn derived_templates_and_their_base() {
        let c = sample();
        let m = c.find("The newt bites!", Channel::Message).unwrap();
        assert_eq!(m.template.id, "a3");
        assert_eq!(m.template.from, c.index_of("a2"));
        assert_eq!(
            c.derived(c.index_of("a2").unwrap()),
            &[c.index_of("a3").unwrap()]
        );
        assert_eq!(c.by_fmt("%s bites!"), c.index_of("a3"));
    }

    #[test]
    fn numbers_characters_and_open_brackets() {
        let c = sample();
        let m = c
            .find("You have 12 gold pieces.", Channel::Message)
            .unwrap();
        assert_eq!(m.captures, vec!["12"]);
        let none = c.find("You have many gold pieces.", Channel::Message);
        assert!(
            none.is_none(),
            "{:?}",
            none.map(|m| (m.template.fmt.clone(), m.captures))
        );
        let m = c
            .find(
                "e - a +0 pick-axe (weapon in right hand).",
                Channel::Message,
            )
            .unwrap();
        assert_eq!(m.template.id, "a5");
        assert_eq!(
            m.captures,
            vec!["e", "a +0 pick-axe (weapon in right hand)"]
        );
    }

    #[test]
    fn a_bad_catalog_is_refused() {
        let bad = SAMPLE.replace("\"format\": 1", "\"format\": 9");
        assert!(matches!(Catalog::parse(&bad), Err(CatalogError::Format(9))));
        let bad = SAMPLE.replace("\"from\": \"a2\"", "\"from\": \"zz\"");
        assert!(matches!(
            Catalog::parse(&bad),
            Err(CatalogError::Invalid(_))
        ));
    }
}
