//! pager.c's line about a map symbol, as `;` and `/` print it:
//!
//! ```text
//! d        a dog or other canine (tame little dog called Rex) [seen: telepathy]
//! ```
//!
//! The symbol, eight spaces, what the symbol can be (alternatives joined by
//! " or ", or "can be many things"), then what is there in parentheses and
//! how the hero sees it in brackets. No template holds the line: an
//! alternative has an " or " of its own ("dog or other canine", "boulder or
//! statue"), so each must reach the lexicon whole.
//!
//! - An alternative is, in this order: a word the line's own template
//!   names ("%s        a trap", translated "{1}        ловушка"), a word
//!   of the lexicon's closed sets (terrain, the classes of monsters and
//!   objects), any name, a piece.
//! - What is there is a name, then what pager.c says of it in clauses of
//!   their own: ", asleep", " embedded in a wall", " with remembered text:
//!   \"Elbereth\"".

use super::{Output, STRONG_LETTERS, Status, Translator, english_words};
use crate::catalog::{Channel, Template};
use crate::grammar::Case;
use crate::phrase::NameKind;
use crate::template::Value;

/// What parts the symbol from what it can be.
const GAP: &str = "        ";
/// What follows a warning's symbol that hides a boulder.
const BOULDER: &str = " co-located with a boulder";

/// A part of the line in Russian, and the English words left in it.
struct Read {
    text: String,
    unread: usize,
}

impl Read {
    fn english(text: &str) -> Read {
        Read {
            text: text.to_string(),
            unread: english_words(text).max(1),
        }
    }
}

impl Translator {
    /// `text` as pager.c's line about a map symbol, when it is one and
    /// some of it reads.
    pub(super) fn look(&self, text: &str) -> Option<Output> {
        let sym = text.chars().next()?;
        let body = text[sym.len_utf8()..].strip_prefix(GAP)?;
        if body.is_empty() || body.starts_with(' ') {
            return None;
        }
        let or = self.said(" or ")?;
        let (body, how) = seen(body);
        // what is there stands in the last parentheses, unless they are an
        // alternative's own: "a useful item (pick-axe, key, lamp...)"
        let (can_be, there) = match last_group(body) {
            Some((list, there)) => {
                let split = self.alternatives(list, &or);
                let whole = (split.unread > 0).then(|| self.alternatives(body, &or));
                match whole {
                    Some(whole) if whole.unread == 0 => (whole, None),
                    _ => (split, Some(self.there(there))),
                }
            }
            None => (self.alternatives(body, &or), None),
        };
        let how = how.map(|h| self.seen_by(h));
        let parts = [Some(&can_be), there.as_ref(), how.as_ref()];
        let unread: usize = parts.iter().flatten().map(|r| r.unread).sum();
        if unread >= english_words(body).max(1) {
            return None;
        }
        let mut out = format!("{sym}{GAP}{}", can_be.text);
        if let Some(there) = &there {
            out.push_str(&format!(" ({})", there.text));
        }
        if let Some(how) = &how {
            out.push_str(&how.text);
        }
        Some(Output {
            text: out,
            status: if unread == 0 {
                Status::Translated
            } else {
                Status::Partial
            },
            template: None,
        })
    }

    /// The Russian of a piece without arguments, by its English.
    fn said(&self, english: &str) -> Option<String> {
        let t = &self.catalog.templates()[self.catalog.by_fmt(english)?];
        let ru = self.russian.get(&t.id)?;
        Some(ru.template.render(&[], self.hero))
    }

    /// What the symbol can be: the alternatives " or " joins, split so
    /// that the fewest words stay unread, then into the fewest
    /// alternatives ("a human or elf" is one, not a human and an elf).
    fn alternatives(&self, list: &str, or: &str) -> Read {
        if let Some(list) = list.strip_suffix(BOULDER) {
            let mut read = self.alternatives(list, or);
            match self.said(BOULDER) {
                Some(ru) => read.text.push_str(&ru),
                None => {
                    read.text.push_str(BOULDER);
                    read.unread += english_words(BOULDER);
                }
            }
            return read;
        }
        let parts: Vec<&str> = list.split(" or ").collect();
        // best[i]: the first i parts read
        let mut best: Vec<Option<(usize, Vec<String>)>> = vec![None; parts.len() + 1];
        best[0] = Some((0, Vec::new()));
        for i in 1..=parts.len() {
            for j in 0..i {
                let Some((unread, read)) = &best[j] else {
                    continue;
                };
                let one = parts[j..i].join(" or ");
                let (ru, missed) = match self.alternative(&one) {
                    Some(ru) => (ru, 0),
                    // only a part alone stays as it is
                    None if i - j == 1 => (one.clone(), english_words(&one).max(1)),
                    None => continue,
                };
                let better = best[i]
                    .as_ref()
                    .is_none_or(|(u, r)| (unread + missed, read.len() + 1) < (*u, r.len()));
                if better {
                    let mut read = read.clone();
                    read.push(ru);
                    best[i] = Some((unread + missed, read));
                }
            }
        }
        match best.pop().flatten() {
            Some((unread, read)) => Read {
                text: read.join(or),
                unread,
            },
            None => Read::english(list),
        }
    }

    /// One alternative, read whole.
    fn alternative(&self, text: &str) -> Option<String> {
        // what the line's own template names: "%s        a trap"
        let own = self
            .catalog
            .by_fmt(&format!("%s{GAP}{}", text.replace('%', "%%")))
            .and_then(|i| self.render_captures(&self.catalog.templates()[i], &[String::new()], 1))
            .and_then(|out| out.text.strip_prefix(GAP).map(str::to_string));
        own.or_else(|| self.read_name(text))
            .or_else(|| self.piece(text).map(|p| p.form(Case::Nom)))
    }

    /// A name the lexicon reads whole: a word of its closed sets first
    /// ("stone" is the rock here, not a gem), then any name. A proper
    /// name stays ("shopkeeper Asidonhopo"), and what the player typed
    /// ("called rex").
    fn read_name(&self, text: &str) -> Option<String> {
        [NameKind::Word, NameKind::Any]
            .iter()
            .filter_map(|kind| self.names.parse(*kind, text))
            .map(|p| p.form(Case::Nom))
            .find(|ru| reads(ru, text))
    }

    /// A name, a piece, or a text the catalog makes ("lawful altar",
    /// "interior of the purple worm").
    fn named(&self, text: &str) -> Option<String> {
        self.read_name(text)
            .or_else(|| self.piece(text).map(|p| p.form(Case::Nom)))
            .or_else(|| self.made(text, |_| true))
    }

    /// A text a template of the catalog makes, one with words of its own
    /// (a format of conversions alone says nothing), all of it Russian
    /// but what the player typed.
    fn made(&self, text: &str, fits: impl Fn(&Template) -> bool) -> Option<String> {
        self.catalog
            .matches(text, Channel::Any)
            .iter()
            .filter(|m| m.template.letters() >= STRONG_LETTERS && fits(m.template))
            .filter_map(|m| self.render_match(m, 1))
            .find(|o| o.status != Status::Untranslated && reads(&o.text, text))
            .map(|o| o.text)
    }

    /// What is there: a name, the longest that reads, and after it the
    /// clauses pager.c adds.
    fn there(&self, text: &str) -> Read {
        let (text, how) = seen(text);
        let mut read = self.described(text);
        if let Some(how) = how {
            let how = self.seen_by(how);
            read.text.push_str(&how.text);
            read.unread += how.unread;
        }
        read
    }

    fn described(&self, text: &str) -> Read {
        if let Some(ru) = self.named(text) {
            return Read {
                text: ru,
                unread: 0,
            };
        }
        let mut best: Option<Read> = None;
        let cuts = text.match_indices([' ', ',']).map(|(i, _)| i);
        for i in cuts.collect::<Vec<_>>().into_iter().rev() {
            let (head, tail) = text.split_at(i);
            // a clause starts at its comma, not at the space after it
            if head.ends_with(',') || (tail.starts_with(',') && !tail.starts_with(", ")) {
                continue;
            }
            let Some(mut ru) = self.named(head) else {
                continue;
            };
            let rest = self.clauses(tail);
            ru.push_str(&rest.text);
            if rest.unread == 0 {
                return Read {
                    text: ru,
                    unread: 0,
                };
            }
            // a name cut at a space is half a name ("human" of "human
            // valkyrie"): only a clause after a comma may stay English
            if tail.starts_with(", ") && best.as_ref().is_none_or(|b| rest.unread < b.unread) {
                best = Some(Read {
                    text: ru,
                    unread: rest.unread,
                });
            }
        }
        best.unwrap_or_else(|| Read::english(text))
    }

    /// What follows a name, clause by clause, each as the catalog knows
    /// it with its comma or space.
    fn clauses(&self, tail: &str) -> Read {
        let mut out = Read {
            text: String::new(),
            unread: 0,
        };
        for clause in clauses(tail) {
            match self.clause(clause) {
                Some(ru) => out.text.push_str(&ru),
                None => {
                    out.text.push_str(clause);
                    out.unread += english_words(clause).max(1);
                }
            }
        }
        out
    }

    /// One clause, or one the engine put together of two (", hiding" and
    /// " on the ceiling").
    fn clause(&self, clause: &str) -> Option<String> {
        let own = |t: &Template| t.fmt.starts_with([',', ' ']);
        self.made(clause, own).or_else(|| {
            clause.match_indices(' ').skip(1).find_map(|(i, _)| {
                Some(self.made(&clause[..i], own)? + &self.clause(&clause[i..])?)
            })
        })
    }

    /// " [seen: infravision, telepathy]": how the hero sees it.
    fn seen_by(&self, how: &str) -> Read {
        let english = || Read::english(&format!(" [seen: {how}]"));
        let Some(i) = self.catalog.by_fmt(" [seen: %s]") else {
            return english();
        };
        let Some(ru) = self.russian_of(&self.catalog.templates()[i]) else {
            return english();
        };
        let mut unread = 0;
        let ways: Vec<String> = how
            .split(", ")
            .map(|way| {
                self.named(way).unwrap_or_else(|| {
                    unread += english_words(way).max(1);
                    way.to_string()
                })
            })
            .collect();
        Read {
            text: ru.render(&[Value::Text(ways.join(", "))], self.hero),
            unread,
        }
    }
}

/// The line without its " [seen: ...]", and what that says.
fn seen(body: &str) -> (&str, Option<&str>) {
    match body
        .strip_suffix(']')
        .and_then(|b| b.rsplit_once(" [seen: "))
    {
        Some((rest, how)) if !how.contains(['[', ']']) => (rest, Some(how)),
        _ => (body, None),
    }
}

/// The text before the parentheses that end `body`, and what they hold.
fn last_group(body: &str) -> Option<(&str, &str)> {
    let inner = body.strip_suffix(')')?;
    let mut depth = 1;
    for (i, c) in inner.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some((inner[..i].strip_suffix(' ')?, &inner[i + 1..]));
        }
    }
    None
}

/// The clauses of what follows a name: a new one at each ", " outside
/// parentheses and quotes.
fn clauses(tail: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut depth, mut quoted) = (0, 0i32, false);
    for (i, c) in tail.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 && i > start && tail[i..].starts_with(", ") => {
                out.push(&tail[start..i]);
                start = i;
            }
            _ => {}
        }
    }
    out.push(&tail[start..]);
    out
}

/// Is `ru` all Russian? The Latin words it may keep of `english` are
/// proper names ("shopkeeper Asidonhopo") and what the player typed: a
/// name after "called" or "named", a text in quotes.
fn reads(ru: &str, english: &str) -> bool {
    let typed = typed(english);
    latin_words(ru).all(|w| w.starts_with(|c: char| c.is_ascii_uppercase()) || typed.contains(&w))
}

fn latin_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphabetic() || c == '\''))
        .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
}

/// The words of a text the player typed.
fn typed(text: &str) -> Vec<&str> {
    let named = [" called ", " named "]
        .into_iter()
        .flat_map(|by| text.match_indices(by).map(move |(i, _)| i + by.len()))
        .flat_map(|i| latin_words(text[i..].split(',').next().unwrap_or("")));
    let quoted = text.split('"').skip(1).step_by(2).flat_map(latin_words);
    named.chain(quoted).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_s_parts() {
        assert_eq!(
            seen("a human or elf (peaceful watchman) [seen: telepathy]"),
            ("a human or elf (peaceful watchman)", Some("telepathy"))
        );
        assert_eq!(seen("a fountain (fountain)").1, None);
        assert_eq!(
            last_group("a useful item (pick-axe, key, lamp...) (pick-axe)"),
            Some(("a useful item (pick-axe, key, lamp...)", "pick-axe"))
        );
        assert_eq!(
            last_group(
                "a cat or other feline (tame kitten, can't move (paralyzed or sleeping or busy))"
            ),
            Some((
                "a cat or other feline",
                "tame kitten, can't move (paralyzed or sleeping or busy)"
            ))
        );
        assert_eq!(last_group("a fountain"), None);
        assert_eq!(
            clauses(", can't move (paralyzed or sleeping or busy), leashed to you"),
            [
                ", can't move (paralyzed or sleeping or busy)",
                ", leashed to you"
            ]
        );
        assert_eq!(
            clauses(" with remembered text: \"a, b\""),
            [" with remembered text: \"a, b\""]
        );
        assert_eq!(typed("tame little dog called rex, asleep"), ["rex"]);
        assert_eq!(
            typed("engraving with remembered text: \"ad aquarium\""),
            ["ad", "aquarium"]
        );
        assert!(reads(
            "ручная собачка по имени rex",
            "tame little dog called rex"
        ));
        assert!(reads("лавочник Asidonhopo", "shopkeeper Asidonhopo"));
        assert!(!reads(
            "ручной котёнок по имени Tom, can't",
            "tame kitten called Tom, can't"
        ));
    }
}
