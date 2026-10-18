//! The top ten list at a game's end (topten.c outheader(), outentry()):
//!
//! ```text
//!  No  Points     Name                                                   Hp [max]
//!   1         78  Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom      -  [16]
//!                 on dungeon level 2.  Killed by a jackal.
//! ```
//!
//! An entry is its rank and points, the hero's name and code (kept as
//! they are: NetHack's geek code, "Val-Hum-Fem-Neu"), what became of the
//! hero and where, then the killer, wrapped over lines wherever the
//! English ran out of room, the hit points and their most at the end of
//! its last line. No line of it is a text of its own: each entry is read
//! whole, in the gender its code says, and wrapped again as the engine
//! wraps it.

use super::{Status, Translator};
use crate::catalog::Channel;
use crate::grammar::Gender;
use crate::template::capitalize;

/// The engine's columns (COLNO).
const COLUMNS: usize = 80;
/// Where an entry's words start on each of its lines.
const INDENT: usize = 16;
/// A line of an entry is wrapped once it reaches the hit points' column.
const TEXT_END: usize = COLUMNS - "  Hp [max]".len();
/// The header's start.
const HEADER: &str = " No  Points     Name";
/// What became of a hero, as outentry() says it; where follows.
const FATES: [&str; 11] = [
    "escaped the dungeon",
    "ascended to demigoddess-hood",
    "ascended to demigod-hood",
    "starved to death",
    "choked on his food",
    "choked on her food",
    "was crushed to death",
    "was poisoned",
    "turned to stone",
    "quit",
    "died",
];
/// The parts of where, each a text of the catalog (" in %s", " on level
/// %d", " [max %d]", " on the Plane of Earth"), and quit's kludge.
const PLACES: [&str; 5] = [" in ", " on level ", " [max ", " on the ", " while "];

impl Translator {
    /// The lines of a window that is the top ten list, in Russian: the
    /// header by its two cells, each entry read whole and wrapped again,
    /// the lines about it ("You made the top ten list!") one by one. None
    /// unless the lines have the list's header.
    pub fn topten(&self, lines: &[&str]) -> Option<Vec<String>> {
        if !lines.iter().any(|l| l.starts_with(HEADER)) {
            return None;
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let Some((head, first)) = first_line(line) else {
                out.push(if line.trim().is_empty() {
                    line.to_string()
                } else {
                    self.window_line(line).text
                });
                i += 1;
                continue;
            };
            // the entry: this line and the ones its words go on in
            let mut words = first.trim_end().to_string();
            let mut next = i + 1;
            while let Some(more) = lines.get(next).and_then(|l| continuation(l)) {
                words.push(' ');
                words.push_str(more.trim_end());
                next += 1;
            }
            let (words, hp) = split_hp(&words);
            match self.entry(words) {
                Some(ru) => out.extend(wrap(head, &ru, hp)),
                None => out.extend(lines[i..next].iter().map(|l| l.to_string())),
            }
            i = next;
        }
        Some(out)
    }

    /// An entry's words in Russian: the name's code as it is, the rest in
    /// the gender the code says. None when a part of it reads no text of
    /// the catalog.
    fn entry(&self, words: &str) -> Option<String> {
        let (code, rest) = words.split_once(' ')?;
        let gender = if code.contains("-Fem") {
            Gender::Fem
        } else {
            Gender::Masc
        };
        self.as_hero(gender, || {
            // the killer's own sentence after the fate's: ".  Killed by…"
            let (fate, killer) = match rest.find(".  ") {
                Some(k) => (&rest[..k], Some(rest[k + 3..].trim_end_matches('.'))),
                None => (rest.trim_end_matches('.'), None),
            };
            let mut out = format!("{code} {}.", self.fate(fate)?);
            if let Some(killer) = killer {
                out.push_str("  ");
                out.push_str(&self.killer(killer)?);
                out.push('.');
            }
            Some(out)
        })
    }

    /// What became of the hero and where: "died in The Dungeons of Doom
    /// on level 2 [max 3]", part by part.
    fn fate(&self, fate: &str) -> Option<String> {
        // an escape names its depth in its own words
        if fate.starts_with("escaped") || fate.starts_with("ascended") {
            return self.whole(fate);
        }
        let head = FATES.iter().find(|f| fate.starts_with(**f))?;
        let mut out = self.whole(head)?;
        let mut rest = &fate[head.len()..];
        while !rest.is_empty() {
            let end = PLACES
                .iter()
                .filter_map(|p| rest[1..].find(p).map(|k| k + 1))
                .min()
                .unwrap_or(rest.len());
            out.push_str(&self.whole(&rest[..end])?);
            rest = &rest[end..];
        }
        Some(out)
    }

    /// The killer's sentence, which outentry() capitalised: "Killed by a
    /// jackal, while helpless" is the death "killed by a jackal, while
    /// helpless".
    fn killer(&self, killer: &str) -> Option<String> {
        let mut chars = killer.chars();
        let first = chars.next()?;
        let death = format!("{}{}", first.to_lowercase(), chars.as_str());
        let out = self.whole(&death).or_else(|| self.whole(killer))?;
        Some(capitalize(&out))
    }

    /// A text the catalog translates whole, or None.
    fn whole(&self, text: &str) -> Option<String> {
        let out = self.by_text(text, Channel::Any);
        (out.status == Status::Translated).then_some(out.text)
    }
}

/// An entry's first line: its rank and points ("%3d %10ld  ", 16
/// characters) and its words.
fn first_line(line: &str) -> Option<(&str, &str)> {
    let head = line.get(..INDENT)?;
    let words = &line[INDENT..];
    let (rank, points) = (&head[..3], &head[4..14]);
    let digits = |s: &str| s.trim().chars().all(|c| c.is_ascii_digit());
    let fits = digits(rank)
        && &head[3..4] == " "
        && !points.trim().is_empty()
        && digits(points)
        && &head[14..] == "  "
        && words.starts_with(|c: char| c != ' ');
    fits.then_some((head, words))
}

/// A line an entry's words go on in ("%15s %s").
fn continuation(line: &str) -> Option<&str> {
    let words = line.strip_prefix(" ".repeat(INDENT).as_str())?;
    words.starts_with(|c: char| c != ' ').then_some(words)
}

/// An entry's words without the hit points and their most that end its
/// last line ("…a jackal.      -  [16]").
fn split_hp(words: &str) -> (&str, Option<(&str, &str)>) {
    let none = (words.trim_end(), None);
    let t = words.trim_end();
    let Some(most) = t.strip_suffix(']') else {
        return none;
    };
    let Some(open) = most.rfind('[') else {
        return none;
    };
    let (before, most) = (most[..open].trim_end(), &most[open + 1..]);
    let Some(gap) = before.rfind(' ') else {
        return none;
    };
    let (rest, hp) = (before[..gap].trim_end(), &before[gap + 1..]);
    let number = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !(number(most) && (hp == "-" || number(hp)) && rest.ends_with('.')) {
        return none;
    }
    (rest, Some((hp, most)))
}

/// An entry's head and words wrapped as outentry() wraps them: a line ends
/// at a space before the hit points' column, the next goes on after 16
/// spaces, and the last ends with the hit points and their most.
fn wrap(head: &str, words: &str, hp: Option<(&str, &str)>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line: Vec<char> = format!("{head}{words}").chars().collect();
    while line.len() >= TEXT_END {
        let mut at = (0..TEXT_END).rev().find(|&k| line[k] == ' ').unwrap_or(0);
        // a word too long for a line is cut in its middle
        if at <= 15 {
            at = TEXT_END - 1;
        }
        let rest: String = if line[at] == ' ' {
            line[at + 1..].iter().collect()
        } else {
            line[at..].iter().collect()
        };
        lines.push(line[..at].iter().collect::<String>());
        line = format!("{}{rest}", " ".repeat(INDENT)).chars().collect();
    }
    let mut last: String = line.iter().collect();
    if let Some((hp, most)) = hp {
        let column = COLUMNS - 7 - hp.chars().count();
        let len = last.chars().count();
        if len <= column {
            last.push_str(&" ".repeat(column - len));
            let pad = match most.len() {
                1 => "  ",
                2 => " ",
                _ => "",
            };
            last.push_str(&format!("{hp} {pad}[{most}]"));
        }
    }
    lines.push(last);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_s_parts() {
        let line = "  1         78  Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom on";
        let (head, words) = first_line(line).unwrap();
        assert_eq!(head, "  1         78  ");
        assert!(words.starts_with("Hero-Val"));
        let words = "Hero-Val died.  Killed by a jackal.      -  [16]";
        assert_eq!(
            split_hp(words),
            ("Hero-Val died.  Killed by a jackal.", Some(("-", "16")))
        );
        // a line its entry goes on after has no hit points
        let words = "Hero-Val died in The Dungeons of Doom on";
        assert_eq!(split_hp(words), (words, None));
        assert_eq!(
            continuation("                on level 2."),
            Some("on level 2.")
        );
        assert_eq!(first_line(" No  Points     Name"), None);
    }

    #[test]
    fn wrapped_as_the_engine_wraps() {
        let words = "Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom on dungeon level 2.  Killed by a jackal.";
        let lines = wrap("  1         78  ", words, Some(("-", "16")));
        assert_eq!(
            lines,
            [
                "  1         78  Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom on",
                "                dungeon level 2.  Killed by a jackal.                   -  [16]",
            ]
        );
    }
}
