//! The prologue's narration: a recorded voice reading each beat's words.
//!
//! A beat's clip is found by its exact text: `res://audio/vo/<lang>/<key>.ogg`,
//! the key the SHA-1 of `lang + "\n" + text` (UTF-8, hex), as the dump of
//! every beat's text for recording keys it (the prologue's `vo_texts_dump`
//! test). A text voiced in parts (the Hero beat: the sentence naming the
//! hero's rank and god, then the rest every hero shares) is found as a head
//! and a tail at a sentence's end, both voiced. A beat without a clip is
//! read in silence, as before. The clips are asked of the loader on its
//! threads when the prologue begins and taken when their beat starts:
//! nothing is loaded on the main thread then.

use godot::classes::{AudioStream, ResourceLoader};
use godot::prelude::*;

/// Where the clips are.
const VO_DIR: &str = "res://audio/vo";
/// The loader's answers (ResourceLoader.ThreadLoadStatus).
const IN_PROGRESS: i64 = 1;
const LOADED: i64 = 3;

/// The key of a text in a language: hex SHA-1 of `lang + "\n" + text`.
pub fn key(lang: &str, text: &str) -> String {
    sha1(format!("{lang}\n{text}").as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// SHA-1 (FIPS 180-4): the keys are the dump's, made by any tool; here
/// without the engine, so that tests can make them.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, t);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// A clip's path under the clips' folder (`res://audio/vo/`).
fn rel_path(lang: &str, text: &str) -> String {
    format!("{lang}/{}.ogg", key(lang, text))
}

/// The clips that read `text` in `lang`, in order, by `exists` (given a
/// path under the clips' folder): its own, else a head and a tail at a
/// sentence's end that are both recorded; none if neither.
pub fn clips_for(lang: &str, text: &str, mut exists: impl FnMut(&str) -> bool) -> Vec<String> {
    let whole = rel_path(lang, text);
    if exists(&whole) {
        return vec![whole];
    }
    partings(text)
        .into_iter()
        .map(|(h, t)| (rel_path(lang, &h), rel_path(lang, &t)))
        .find(|(h, t)| exists(h) && exists(t))
        .map_or_else(Vec::new, |(h, t)| vec![h, t])
}

/// Where `text` may be parted into a head and a tail: after each of its
/// sentences but the last (the two parts, trimmed).
pub fn partings(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (n, &(i, c)) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') && chars.get(n + 1).is_some_and(|&(_, next)| next == ' ') {
            let cut = i + c.len_utf8();
            let (head, tail) = (text[..cut].trim(), text[cut..].trim());
            if !head.is_empty() && !tail.is_empty() {
                out.push((head.to_string(), tail.to_string()));
            }
        }
    }
    out
}

/// The narration of one telling: the clips found for each beat, being
/// loaded.
pub struct Narration {
    /// The texts it was asked for.
    texts: [String; 6],
    beats: Vec<Vec<String>>,
}

impl Narration {
    /// The clips for `texts` (the six beats') in `lang` ("en", "ru"),
    /// asked of the loader; None when there is no voice in that language.
    pub fn new(texts: &[String; 6], lang: Option<&'static str>) -> Option<Narration> {
        let lang = lang?;
        let mut loader = ResourceLoader::singleton();
        let beats: Vec<Vec<String>> = texts
            .iter()
            .map(|text| {
                clips_for(lang, text, |p| loader.exists(&format!("{VO_DIR}/{p}")))
                    .into_iter()
                    .map(|p| format!("{VO_DIR}/{p}"))
                    .collect()
            })
            .collect();
        for p in beats.iter().flatten() {
            // (the threaded calls are not in the bindings: called by name)
            loader.call("load_threaded_request", &[p.to_variant()]);
        }
        Some(Narration {
            texts: texts.clone(),
            beats,
        })
    }

    /// The texts it was asked for.
    pub fn texts(&self) -> &[String; 6] {
        &self.texts
    }

    /// Whether `beat` has a voice at all (self-tests).
    pub fn voiced(&self, beat: usize) -> bool {
        self.beats.get(beat).is_some_and(|b| !b.is_empty())
    }

    /// The clips of `beat` once all are loaded (in order); None while one
    /// is still loading, unless `wait`: then it waits for them (a recording
    /// holds the beat for them: `RenethackGame::prologue_steady`); empty
    /// when it has none (or one would not load).
    pub fn ready(&self, beat: usize, wait: bool) -> Option<Vec<Gd<AudioStream>>> {
        let Some(paths) = self.beats.get(beat) else {
            return Some(Vec::new());
        };
        let mut loader = ResourceLoader::singleton();
        let mut out = Vec::new();
        for p in paths {
            let arg = [p.to_variant()];
            match loader
                .call("load_threaded_get_status", &arg)
                .try_to::<i64>()
            {
                Ok(IN_PROGRESS) if !wait => return None,
                // (the threaded get waits for a load still going)
                Ok(IN_PROGRESS | LOADED) => {}
                _ => {
                    godot_warn!("renethack: the narration's {p} would not load");
                    return Some(Vec::new());
                }
            }
            match loader
                .call("load_threaded_get", &arg)
                .try_to::<Gd<AudioStream>>()
            {
                Ok(s) => out.push(s),
                Err(_) => return Some(Vec::new()),
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_the_sha1_the_dump_gives() {
        assert_eq!(
            key("en", "It is written in the Book of Tyr:"),
            "e58ff383b1bdb900fcfb6d23c5dc868d351a2819"
        );
        // the standard's own vectors, the empty one and two blocks long
        let hex = |d: &[u8]| {
            sha1(d)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        // and a Russian text by its UTF-8 (Python's hashlib)
        assert_eq!(key("ru", "Тир"), "9f50c806c4f290a9e5f718ceaa3c531aa7896e56");
    }

    #[test]
    fn a_text_parts_after_each_sentence_but_the_last() {
        let hero = "You, a newly trained Stripling, have been heralded from birth as the \
                    instrument of Tyr. You are destined to recover the Amulet for your \
                    deity, or die in the attempt. Your hour of destiny has come.";
        let p = partings(hero);
        assert_eq!(p.len(), 2);
        assert!(p[0].0.ends_with("instrument of Tyr."));
        assert!(p[0].1.starts_with("You are destined"));
        assert!(p[1].1 == "Your hour of destiny has come.");
        // a colon or a comma parts nothing
        assert!(partings("For the sake of us all: Go bravely with Tyr!").is_empty());
        assert!(partings("Вы — новобранка, и обучение ваше завершилось. Пробил час.").len() == 1);
    }
}
