//! An engraving worn by wipeout_text() (engrave.c): its characters rubbed
//! out one at a time, a letter to a '?' or to a look-alike of rubouts[], a
//! small punctuation mark to a space ("Tne magic mar|er is mightier ?nan
//! hc sword."). No template matches it, but it is a worn copy of a text of
//! the catalog, a random engraving, a rumor, an epitaph: each of its
//! characters is the original's or what wiping, once or again, could make
//! of it, and wiping trims the spaces it leaves at the end.
//!
//! The original's Russian is shown worn as much: the same share of its
//! characters rubbed out, where a hash of the English says, so the same
//! engraving reads the same each time. An original kept in Latin letters
//! (Elbereth) stays as the engine wore it: whether it still reads matters
//! in play.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::Translator;
use crate::catalog::{Catalog, Use};
use crate::russian::Russian;

/// engrave.c's rubouts[]: the look-alikes wiping may make of a character.
const RUBOUTS: [(u8, &str); 48] = [
    (b'A', "^"),
    (b'B', "Pb["),
    (b'C', "("),
    (b'D', "|)["),
    (b'E', "|FL[_"),
    (b'F', "|-"),
    (b'G', "C("),
    (b'H', "|-"),
    (b'I', "|"),
    (b'K', "|<"),
    (b'L', "|_"),
    (b'M', "|"),
    (b'N', "|\\"),
    (b'O', "C("),
    (b'P', "F"),
    (b'Q', "C("),
    (b'R', "PF"),
    (b'T', "|"),
    (b'U', "J"),
    (b'V', "/\\"),
    (b'W', "V/\\"),
    (b'Z', "/"),
    (b'b', "|"),
    (b'd', "c|"),
    (b'e', "c"),
    (b'g', "c"),
    (b'h', "n"),
    (b'j', "i"),
    (b'k', "|"),
    (b'l', "|"),
    (b'm', "nr"),
    (b'n', "r"),
    (b'o', "c"),
    (b'q', "c"),
    (b'w', "v"),
    (b'y', "v"),
    (b':', "."),
    (b';', ",:"),
    (b',', "."),
    (b'=', "-"),
    (b'+', "-|"),
    (b'*', "+"),
    (b'@', "0"),
    (b'0', "C("),
    (b'1', "|"),
    (b'6', "o"),
    (b'7', "/"),
    (b'8', "3o"),
];

/// What wipeout_text() rubs out to a space.
const SMALL: &str = "?.,'`-|_";

/// A worn text shorter than this is no one's copy for sure.
const MIN_LEN: usize = 4;

/// How many characters the trimming may have taken off the end.
const TRIMMED: usize = 3;

/// The texts an engraving may be a worn copy of, by their length: the
/// English and its Russian.
pub(super) struct Worn {
    by_len: HashMap<usize, Vec<(Vec<u8>, String)>>,
}

impl Worn {
    pub(super) fn new(catalog: &Catalog, russian: &Russian) -> Worn {
        let mut by_len: HashMap<usize, Vec<(Vec<u8>, String)>> = HashMap::new();
        for t in catalog.templates() {
            if t.arity() > 0 || !t.uses.contains(&Use::Data) || !t.fmt.is_ascii() {
                continue;
            }
            let Some(tr) = russian.get(&t.id) else {
                continue;
            };
            // a data text has no conversions: its format is the text, but
            // for a doubled '%'
            let en = t.fmt.replace("%%", "%");
            by_len
                .entry(en.len())
                .or_default()
                .push((en.into_bytes(), tr.ru.clone()));
        }
        Worn { by_len }
    }

    /// The one text `shown` is a worn copy of: its English and Russian.
    fn original(&self, shown: &[u8]) -> Option<(&[u8], &str)> {
        let mut found = None;
        for n in shown.len()..=shown.len() + TRIMMED {
            for (en, ru) in self.by_len.get(&n).into_iter().flatten() {
                if worn_copy(en, shown) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((en.as_slice(), ru.as_str()));
                }
            }
        }
        found
    }
}

impl Translator {
    /// `shown` as the Russian of the engraving, rumor or epitaph it is a
    /// worn copy of, worn as much; None when it is no one's.
    pub(super) fn worn(&self, shown: &str) -> Option<String> {
        if shown.len() < MIN_LEN || !shown.is_ascii() {
            return None;
        }
        let index = self
            .worn
            .get_or_init(|| Worn::new(&self.catalog, &self.russian));
        let (en, ru) = index.original(shown.as_bytes())?;
        // Elbereth, a motto in Latin: as the engine wore it
        if ru.as_bytes() == en {
            return Some(shown.to_string());
        }
        Some(wear(ru, en, shown.as_bytes()))
    }
}

/// Is `shown` `original` worn: each character the original's or what
/// wiping could make of it, past its end only what wiping left as spaces,
/// and at least half of the original's characters still standing?
fn worn_copy(original: &[u8], shown: &[u8]) -> bool {
    if original == shown {
        return false;
    }
    let mut standing = 0;
    let mut chars = 0;
    for (k, &o) in original.iter().enumerate() {
        if o != b' ' {
            chars += 1;
        }
        match shown.get(k) {
            Some(&s) if s == o => standing += usize::from(o != b' '),
            Some(&s) if wears_to(o, s) => {}
            Some(_) => return false,
            // trimmed: a mark rubbed out to a space ("jackal" is no worn
            // "jackalope")
            None if o == b' ' || SMALL.as_bytes().contains(&o) => {}
            None => return false,
        }
    }
    2 * standing >= chars
}

/// Can wiping, once or again, make `to` of `from`?
fn wears_to(from: u8, to: u8) -> bool {
    static REACH: OnceLock<[u128; 128]> = OnceLock::new();
    let reach = REACH.get_or_init(|| {
        let mut table = [0u128; 128];
        for (c, set) in table.iter_mut().enumerate() {
            let mut todo = vec![c as u8];
            while let Some(x) = todo.pop() {
                if *set >> x & 1 == 1 {
                    continue;
                }
                *set |= 1 << x;
                todo.extend(wiped(x));
            }
        }
        table
    });
    from < 128 && to < 128 && reach[from as usize] >> to & 1 == 1
}

/// What one wipe makes of a character: a space stays, a small mark is
/// rubbed out to one, anything else turns to a look-alike or to a '?'.
fn wiped(c: u8) -> Vec<u8> {
    if c == b' ' {
        return Vec::new();
    }
    if SMALL.as_bytes().contains(&c) {
        return vec![b' '];
    }
    let mut out = vec![b'?'];
    if let Some((_, to)) = RUBOUTS.iter().find(|(from, _)| *from == c) {
        out.extend(to.bytes());
    }
    out
}

/// `ru` with as large a share of its characters rubbed out as `shown` has
/// of `en`'s: a small mark to a space, anything else to a '?', where a
/// hash of `shown` says; its last mark stays where the English kept its
/// own, and the spaces left at the end go.
fn wear(ru: &str, en: &[u8], shown: &[u8]) -> String {
    let chars = en.iter().filter(|&&c| c != b' ').count();
    let gone = en
        .iter()
        .enumerate()
        .filter(|&(k, &c)| c != b' ' && shown.get(k) != Some(&c))
        .count();
    let mut out: Vec<char> = ru.chars().collect();
    let kept_last = shown.len() == en.len() && shown.last() == en.last();
    let mut places: Vec<usize> = (0..out.len())
        .filter(|&k| out[k] != ' ' && !(kept_last && k + 1 == out.len()))
        .collect();
    let wanted = ((gone * places.len() + chars / 2) / chars.max(1))
        .max(usize::from(gone > 0))
        .min(places.len());
    // the same places for the same worn English
    let mut seed = shown.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    }) | 1;
    for k in 0..wanted {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let pick = k + (seed % (places.len() - k) as u64) as usize;
        places.swap(k, pick);
        let c = &mut out[places[k]];
        *c = if SMALL.contains(*c) { ' ' } else { '?' };
    }
    let text: String = out.into_iter().collect();
    text.trim_end_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_wiping_makes_of_a_character() {
        // a look-alike, a '?', and a '?' rubbed out again to a space
        assert!(wears_to(b'h', b'n'));
        assert!(wears_to(b'm', b'r'));
        assert!(wears_to(b'k', b'|'));
        assert!(wears_to(b'k', b' '));
        assert!(wears_to(b'P', b'?'));
        assert!(wears_to(b'.', b' '));
        assert!(wears_to(b'8', b'o'));
        // a space stays one, a mark is no letter
        assert!(!wears_to(b' ', b'?'));
        assert!(!wears_to(b'.', b'?'));
        assert!(!wears_to(b'h', b'x'));
    }

    #[test]
    fn a_worn_copy() {
        let en = b"The magic marker is mightier than the sword.";
        assert!(worn_copy(
            en,
            b"Tne magic mar|er is mightier ?nan  hc sword."
        ));
        // the trimming took the rubbed-out mark at the end
        assert!(worn_copy(
            en,
            b"The magic marker is mightier than the sword"
        ));
        assert!(!worn_copy(en, en));
        assert!(!worn_copy(b"jackalope", b"jackal"));
        assert!(!worn_copy(
            en,
            b"The magic marker is mightier than the swore."
        ));
        // hardly a letter left
        assert!(!worn_copy(b"Elbereth", b"?|???e??"));
    }

    #[test]
    fn the_russian_worn_as_much() {
        let en = b"Please don't feed the animals.";
        let shown = b"?lea?e don't feed the ar?mals.";
        let ru = "Пожалуйста, не кормите животных.";
        let worn = wear(ru, en, shown);
        assert_eq!(worn, wear(ru, en, shown), "the same each time");
        assert_eq!(worn.chars().count(), ru.chars().count());
        let rubbed = worn.chars().zip(ru.chars()).filter(|(w, r)| w != r);
        // "P", "s", the "n" worn to an "r" and the "i": four of 26
        assert_eq!(rubbed.count(), 4, "{worn}");
        assert!(worn.ends_with('.'), "{worn}");
    }
}
