//! The sound's cues: the code asks for a sound by name, and the manifest
//! (`client/godot/audio/manifest.json`) says how to play it: its files,
//! its bus, how loud and how high, how many at once. A cue the manifest
//! lacks falls back along its dots (`die.humanoid` -> `die`), so a new
//! sound needs only a manifest line. Plain data and the rules for picking
//! (no Godot).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Cues the code asks for that must play (the manifest test checks them).
#[cfg(test)]
const ASKED: &[&str] = &[
    "ui.click",
    "ui.hover",
    "ui.open",
    "ui.close",
    "inv.open",
    "inv.close",
    "book.open",
    "book.close",
    "step",
    "swing",
    "hit",
    "miss",
    "hurt",
    "die",
    "hero.hurt",
    "use",
];

/// Where the manifest lies, and the folder its files are under.
pub const MANIFEST: &str = "res://audio/manifest.json";
pub const AUDIO_DIR: &str = "res://audio";

/// A value, or a range one is picked from at each play.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Span {
    One(f32),
    Two([f32; 2]),
}

impl Span {
    /// The value at `r` (0..1) along the range.
    pub fn at(self, r: f32) -> f32 {
        match self {
            Span::One(v) => v,
            Span::Two([a, b]) => a + (b - a) * r.clamp(0.0, 1.0),
        }
    }
}

/// How a cue plays.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    /// Its variants, under `res://audio/`: one is picked at each play,
    /// never the same twice running.
    pub files: Vec<String>,
    /// Its mixer bus.
    #[serde(default = "sfx")]
    pub bus: String,
    /// Decibels.
    #[serde(default = "unchanged")]
    pub volume: Span,
    /// A pitch scale.
    #[serde(default = "as_recorded")]
    pub pitch: Span,
    /// At most this many at once: one more takes the oldest's voice.
    #[serde(default = "three")]
    pub voices: u32,
    /// Seconds before it plays again (one event told twice in a frame).
    #[serde(default = "brief")]
    pub cooldown: f32,
    /// Metres it carries from where it is placed.
    #[serde(default = "far")]
    pub range: f32,
    /// Music and ambience go round.
    #[serde(default, rename = "loop")]
    pub looped: bool,
    /// A creature's size scales it: a dragon's step is low and loud, a
    /// newt's high and soft.
    #[serde(default)]
    pub sized: bool,
    /// When every voice is busy, a sound takes one from those of lower or
    /// equal priority (steps 0, most 1, the hero's own 2).
    #[serde(default = "usual")]
    pub priority: u8,
}

fn sfx() -> String {
    "SFX".to_string()
}

fn unchanged() -> Span {
    Span::One(0.0)
}

fn as_recorded() -> Span {
    Span::One(1.0)
}

fn three() -> u32 {
    3
}

fn brief() -> f32 {
    0.03
}

fn far() -> f32 {
    24.0
}

fn usual() -> u8 {
    1
}

/// The manifest: the cues, and what the code reads alongside them.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Manifest {
    pub cues: BTreeMap<String, Cue>,
}

impl Manifest {
    /// The manifest, each cue's files with their ranges written out.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut m: Manifest = serde_json::from_str(text).map_err(|e| e.to_string())?;
        for (name, cue) in &mut m.cues {
            let mut files = Vec::new();
            for f in &cue.files {
                files.extend(expand(f).map_err(|e| format!("cue {name}: {e}"))?);
            }
            if files.is_empty() {
                return Err(format!("cue {name} has no files"));
            }
            cue.files = files;
        }
        Ok(m)
    }

    /// The cue for a sound asked for by `names`, most specific first: the
    /// first of them the manifest has, else the first one's parents along
    /// its dots.
    pub fn resolve<'a, S: AsRef<str>>(&'a self, names: &[S]) -> Option<(&'a str, &'a Cue)> {
        self.resolve_exact(names).or_else(|| {
            let first = names.first()?.as_ref();
            parents(first)
                .find_map(|p| self.cues.get_key_value(p))
                .map(|(name, cue)| (name.as_str(), cue))
        })
    }

    /// The first of `names` the manifest has, without falling back.
    pub fn resolve_exact<'a, S: AsRef<str>>(&'a self, names: &[S]) -> Option<(&'a str, &'a Cue)> {
        names
            .iter()
            .find_map(|n| self.cues.get_key_value(n.as_ref()))
            .map(|(name, cue)| (name.as_str(), cue))
    }
}

/// `a_{000..004}.ogg`: `a_000.ogg` to `a_004.ogg`, as wide as the first
/// number is written; a name without braces is itself.
pub fn expand(file: &str) -> Result<Vec<String>, String> {
    let (Some(open), Some(close)) = (file.find('{'), file.find('}')) else {
        return Ok(vec![file.to_string()]);
    };
    let bad = || format!("{file}: a range is {{first..last}}");
    let (first, last) = file
        .get(open + 1..close)
        .and_then(|r| r.split_once(".."))
        .ok_or_else(bad)?;
    let (from, to) = match (first.parse::<u32>(), last.parse::<u32>()) {
        (Ok(a), Ok(b)) if a <= b => (a, b),
        _ => return Err(bad()),
    };
    let (head, tail, width) = (&file[..open], &file[close + 1..], first.len());
    Ok((from..=to)
        .map(|n| format!("{head}{n:0width$}{tail}"))
        .collect())
}

/// `a.b.c`, then `a.b`, then `a`.
pub fn parents(name: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(name);
    std::iter::from_fn(move || {
        let now = next?;
        next = now.rfind('.').map(|i| &now[..i]);
        Some(now)
    })
}

/// A creature's size, as the catalog names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Gigantic,
}

impl Size {
    pub fn of(name: &str) -> Size {
        match name {
            "tiny" => Size::Tiny,
            "small" => Size::Small,
            "large" => Size::Large,
            "huge" => Size::Huge,
            "gigantic" => Size::Gigantic,
            _ => Size::Medium,
        }
    }

    /// A sized cue's pitch scale and decibels for this size.
    pub fn scale(self) -> (f32, f32) {
        match self {
            Size::Tiny => (1.35, -7.0),
            Size::Small => (1.15, -3.0),
            Size::Medium => (1.0, 0.0),
            Size::Large => (0.86, 2.0),
            Size::Huge => (0.74, 4.0),
            Size::Gigantic => (0.64, 5.0),
        }
    }
}

/// Picks variants and values: a fixed sequence (runs repeat), never the
/// same variant twice running.
#[derive(Debug, Clone)]
pub struct Picker {
    state: u64,
}

impl Default for Picker {
    fn default() -> Picker {
        Picker {
            state: 0x9e37_79b9_7f4a_7c15,
        }
    }
}

impl Picker {
    /// 0..1.
    pub fn unit(&mut self) -> f32 {
        // xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        let r = x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
        r as f32 / (1u64 << 24) as f32
    }

    /// One of `n` variants, not `last` when there is another.
    pub fn variant(&mut self, n: usize, last: Option<usize>) -> usize {
        if n <= 1 {
            return 0;
        }
        let choices = match last {
            Some(l) if l < n => n - 1,
            _ => n,
        };
        let i = ((self.unit() * choices as f32) as usize).min(choices - 1);
        match last {
            Some(l) if l < n && i >= l => i + 1,
            _ => i,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest::parse(
            r#"{"cues": {
                "step": {"files": ["a.ogg"]},
                "step.stone": {"files": ["b.ogg", "c.ogg"], "volume": [-14, -10], "sized": true},
                "step.hoof": {"files": ["d.ogg"], "priority": 0},
                "die": {"files": ["e.ogg"], "bus": "SFX", "pitch": 0.9},
                "music.title": {"files": ["f.ogg"], "bus": "Music", "loop": true}
            }}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_cue_falls_back_along_its_dots() {
        let m = manifest();
        let got = |names: &[&str]| m.resolve(names).map(|(n, _)| n);
        assert_eq!(got(&["die.humanoid.large"]), Some("die"));
        assert_eq!(got(&["step.stone"]), Some("step.stone"));
        // a step: its floor and gait, its gait, its floor, then any step
        assert_eq!(
            got(&["step.stone.hoof", "step.hoof", "step.stone"]),
            Some("step.hoof")
        );
        assert_eq!(
            got(&["step.water.paw", "step.paw", "step.water"]),
            Some("step"),
            "nothing for water: a step still"
        );
        assert_eq!(got(&["hit.blade"]), None, "nothing at all: silence");
        let exact = |names: &[&str]| m.resolve_exact(names).map(|(n, _)| n);
        assert_eq!(exact(&["step.dirt.slither", "step.slither"]), None);
        assert_eq!(exact(&["step.ooze", "step"]), Some("step"));
        assert_eq!(got(&[] as &[&str]), None);
        assert_eq!(parents("a.b.c").collect::<Vec<_>>(), ["a.b.c", "a.b", "a"]);
    }

    #[test]
    fn a_cue_takes_the_defaults_it_does_not_set() {
        let m = manifest();
        let step = &m.cues["step"];
        assert_eq!(
            (
                step.bus.as_str(),
                step.voices,
                step.priority,
                step.looped,
                step.sized
            ),
            ("SFX", 3, 1, false, false)
        );
        assert_eq!(step.volume.at(0.5), 0.0);
        assert_eq!(step.pitch.at(0.5), 1.0);
        let stone = &m.cues["step.stone"];
        assert_eq!((stone.volume.at(0.0), stone.volume.at(1.0)), (-14.0, -10.0));
        assert!(stone.sized);
        assert!(m.cues["music.title"].looped);
        // a field the code does not know is a mistake in the manifest
        assert!(Manifest::parse(r#"{"cues": {"x": {"files": [], "volum": 3}}}"#).is_err());
    }

    #[test]
    fn a_range_of_files_is_written_out() {
        assert_eq!(
            expand("a_{000..002}.ogg").unwrap(),
            ["a_000.ogg", "a_001.ogg", "a_002.ogg"]
        );
        assert_eq!(
            expand("cloth{1..3}.ogg").unwrap(),
            ["cloth1.ogg", "cloth2.ogg", "cloth3.ogg"]
        );
        assert_eq!(expand("plain.ogg").unwrap(), ["plain.ogg"]);
        assert!(expand("x{3..1}.ogg").is_err());
        assert!(expand("x{a..b}.ogg").is_err());
        let m = Manifest::parse(r#"{"cues": {"c": {"files": ["s{1..2}.ogg", "t.ogg"]}}}"#).unwrap();
        assert_eq!(m.cues["c"].files, ["s1.ogg", "s2.ogg", "t.ogg"]);
        assert!(Manifest::parse(r#"{"cues": {"c": {"files": []}}}"#).is_err());
    }

    /// The project's manifest: every bus a cue names is one the game makes,
    /// every file is there, and every cue the code asks for plays (itself
    /// or a parent).
    #[test]
    fn the_projects_manifest_holds_together() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../godot/audio");
        let text = std::fs::read_to_string(format!("{dir}/manifest.json")).unwrap();
        let m = Manifest::parse(&text).unwrap();
        for (name, cue) in &m.cues {
            assert!(
                ["SFX", "Distant", "UI", "Music", "Ambience"].contains(&cue.bus.as_str()),
                "{name}: bus {}",
                cue.bus
            );
            for f in &cue.files {
                assert!(
                    std::path::Path::new(&format!("{dir}/{f}")).is_file(),
                    "{name}: {f} is missing"
                );
            }
            assert!(cue.voices >= 1 && cue.range > 0.0, "{name}");
        }
        for asked in ASKED {
            assert!(m.resolve(&[*asked]).is_some(), "nothing plays for {asked}");
        }
    }

    #[test]
    fn variants_never_repeat_and_values_stay_in_range() {
        let mut p = Picker::default();
        let mut last = None;
        let mut seen = [0; 4];
        for _ in 0..400 {
            let v = p.variant(4, last);
            assert_ne!(Some(v), last);
            seen[v] += 1;
            last = Some(v);
            let u = p.unit();
            assert!((0.0..1.0).contains(&u));
        }
        assert!(seen.iter().all(|&n| n > 60), "{seen:?}");
        assert_eq!(p.variant(1, Some(0)), 0);
        assert_eq!(p.variant(0, None), 0);
    }

    #[test]
    fn bigger_creatures_sound_lower_and_louder() {
        let all = [
            Size::Tiny,
            Size::Small,
            Size::Medium,
            Size::Large,
            Size::Huge,
            Size::Gigantic,
        ];
        for w in all.windows(2) {
            let (a, b) = (w[0].scale(), w[1].scale());
            assert!(a.0 > b.0 && a.1 < b.1, "{:?} {:?}", w[0], w[1]);
        }
        assert_eq!(Size::of("huge"), Size::Huge);
        assert_eq!(Size::of("whatever"), Size::Medium);
        assert_eq!(Size::Medium.scale(), (1.0, 0.0));
    }
}
