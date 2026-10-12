//! The game's sound: the mixer's buses, whose volumes the player sets,
//! and the player of cues (`cues.rs`): pooled voices placed on the map or
//! unplaced, the ears at the hero, a record of what was asked for.
//!
//! Master takes Music, Ambience, SFX and UI; Distant ("You hear ..."
//! cues, unplaced) goes through SFX. The buses are made here at start-up,
//! not in a bus layout resource the project would have to name.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::time::{Duration, Instant};

use godot::classes::audio_stream_player_3d::AttenuationModel;
use godot::classes::{
    AudioEffectLowPassFilter, AudioEffectReverb, AudioListener3D, AudioServer, AudioStream,
    AudioStreamPlayer, AudioStreamPlayer3D, FileAccess, ResourceLoader,
};
use godot::prelude::*;

use crate::cues::{AUDIO_DIR, MANIFEST, Manifest, Picker, Size};

/// A bus whose volume the player sets (the settings' sliders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bus {
    Master,
    Music,
    Ambience,
    Sfx,
    Ui,
}

/// The bus of "You hear ..." cues, under SFX (not the player's to set).
pub const DISTANT: &str = "Distant";

/// The buses made at start-up, each with the bus it sends to.
const MADE: [(&str, &str); 5] = [
    ("Music", "Master"),
    ("Ambience", "Master"),
    ("SFX", "Master"),
    ("UI", "Master"),
    (DISTANT, "SFX"),
];

/// Quieter than this is silence: the bus is muted.
const SILENT: f32 = 0.001;

/// What "You hear ..." loses of its highs.
const DISTANT_CUTOFF_HZ: f32 = 1600.0;
/// Voices for sounds placed on the map, and for unplaced ones (the
/// interface, "You hear ...", jingles).
const PLACED_VOICES: usize = 16;
const FLAT_VOICES: usize = 8;
/// Files loaded a frame, ahead of their first play.
const PRELOAD_STEP: usize = 6;
/// The record keeps this many of the sounds asked for, and the sound's
/// time of this many frames.
const HEARD_KEPT: usize = 4096;
const COSTS_KEPT: usize = 1024;
/// Metres within which a placed sound is as loud as beside the hero.
const UNIT_SIZE: f32 = 4.0;
/// The ears are this high over where the hero stands.
pub const EAR_HEIGHT: f32 = 1.2;

/// The manifest built into the client, used when the project's copy
/// cannot be read.
const BUILT_IN_MANIFEST: &str = include_str!("../../../godot/audio/manifest.json");

impl Bus {
    /// Top to bottom, as the settings list them.
    pub const ALL: [Bus; 5] = [Bus::Master, Bus::Music, Bus::Ambience, Bus::Sfx, Bus::Ui];

    /// Its name in Godot's mixer (and in the profile).
    pub fn name(self) -> &'static str {
        match self {
            Bus::Master => "Master",
            Bus::Music => "Music",
            Bus::Ambience => "Ambience",
            Bus::Sfx => "SFX",
            Bus::Ui => "UI",
        }
    }

    /// Its volume until the player sets one: music under the game's own
    /// sounds.
    pub fn default_volume(self) -> f32 {
        match self {
            Bus::Music => 0.7,
            Bus::Ambience | Bus::Ui => 0.8,
            Bus::Master | Bus::Sfx => 1.0,
        }
    }
}

/// A slider's linear volume in decibels (as Godot's `linear_to_db`).
pub fn db_of(linear: f32) -> f32 {
    20.0 * linear.max(SILENT).log10()
}

pub fn linear_of(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Make the buses the game plays on, once (Master is Godot's own).
/// Distant is far off and through walls: muffled, with a little room.
pub fn ensure_buses() {
    let mut server = AudioServer::singleton();
    for (name, send) in MADE {
        if server.get_bus_index(name) >= 0 {
            continue;
        }
        let i = server.get_bus_count();
        server.add_bus();
        server.set_bus_name(i, name);
        server.set_bus_send(i, send);
        if name == DISTANT {
            let mut low = AudioEffectLowPassFilter::new_gd();
            low.set_cutoff(DISTANT_CUTOFF_HZ);
            server.add_bus_effect(i, &low);
            let mut room = AudioEffectReverb::new_gd();
            room.set_room_size(0.6);
            room.set_wet(0.25);
            room.set_dry(0.9);
            server.add_bus_effect(i, &room);
        }
    }
}

/// The volume `bus` plays at now, linear 0..=1.
pub fn volume(bus: Bus) -> f32 {
    let server = AudioServer::singleton();
    let i = server.get_bus_index(bus.name());
    if i < 0 {
        return bus.default_volume();
    }
    if server.is_bus_mute(i) {
        return 0.0;
    }
    linear_of(server.get_bus_volume_db(i)).clamp(0.0, 1.0)
}

/// Set `bus` to `linear` (0..=1); 0 mutes it.
pub fn set_volume(bus: Bus, linear: f32) {
    let linear = linear.clamp(0.0, 1.0);
    let mut server = AudioServer::singleton();
    let i = server.get_bus_index(bus.name());
    if i < 0 {
        return;
    }
    server.set_bus_volume_db(i, db_of(linear));
    server.set_bus_mute(i, linear < SILENT);
}

/// Every bus's volume in percent, by name (the profile's).
pub fn volumes() -> BTreeMap<String, u8> {
    Bus::ALL
        .into_iter()
        .map(|b| (b.name().to_string(), percent(volume(b))))
        .collect()
}

/// The volumes a profile keeps, each bus it has none for at its default.
pub fn apply_volumes(kept: &BTreeMap<String, u8>) {
    for bus in Bus::ALL {
        set_volume(bus, kept_volume(kept, bus));
    }
}

fn percent(linear: f32) -> u8 {
    (linear.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn kept_volume(kept: &BTreeMap<String, u8>, bus: Bus) -> f32 {
    kept.get(bus.name())
        .map_or(bus.default_volume(), |&p| f32::from(p) / 100.0)
        .clamp(0.0, 1.0)
}

/// A sound asked for: cue names, most specific first (the manifest plays
/// the first it has, else the first one's parents along its dots).
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    pub cues: Vec<String>,
    /// Where on the map it sounds; None: unplaced.
    pub at: Option<Vector3>,
    /// Seconds from now (a blow sounds when it lands).
    pub delay: f32,
    /// The creature it is of (`sized` cues).
    pub size: Option<Size>,
    /// Only the names given, not the first one's parents.
    pub exact: bool,
    /// Its claim on a voice when all are busy; None: its cue's.
    pub priority: Option<u8>,
}

impl Sound {
    pub fn new(cue: impl Into<String>) -> Sound {
        Sound {
            cues: vec![cue.into()],
            at: None,
            delay: 0.0,
            size: None,
            exact: false,
            priority: None,
        }
    }

    /// Names to try in order, with no fallback past them.
    pub fn exactly(names: Vec<String>) -> Sound {
        Sound {
            cues: names,
            exact: true,
            ..Sound::new("")
        }
    }

    /// Another name to try, after those before it.
    pub fn or(mut self, cue: impl Into<String>) -> Sound {
        self.cues.push(cue.into());
        self
    }

    pub fn at(mut self, at: Vector3) -> Sound {
        self.at = Some(at);
        self
    }

    pub fn after(mut self, secs: f32) -> Sound {
        self.delay = secs;
        self
    }

    pub fn sized(mut self, size: Size) -> Sound {
        self.size = Some(size);
        self
    }

    /// Before others for a voice (the hero's own sounds).
    pub fn urgent(mut self) -> Sound {
        self.priority = Some(2);
        self
    }
}

/// A sound the player was asked for: the record the self-tests count
/// (headless, Godot's dummy driver plays nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// Numbered from 1 in the order asked.
    pub seq: u64,
    /// The most specific name asked.
    pub asked: String,
    /// The cue that played; None: the manifest has none for it.
    pub cue: Option<String>,
    pub at: Option<Vector3>,
    /// It got a voice (not held back by its cooldown, nor by every voice
    /// busy with more urgent sounds, nor by a file that would not load).
    pub voiced: bool,
}

enum Player {
    Placed(Gd<AudioStreamPlayer3D>),
    Flat(Gd<AudioStreamPlayer>),
}

struct Voice {
    player: Player,
    cue: String,
    priority: u8,
    started: f64,
    ends: f64,
}

impl Voice {
    fn busy(&self, now: f64) -> bool {
        now < self.ends
    }
}

/// How a voice plays a variant.
struct Take<'a> {
    stream: &'a Gd<AudioStream>,
    bus: &'a str,
    db: f32,
    pitch: f32,
    at: Option<Vector3>,
    range: f32,
}

impl Player {
    fn start(&mut self, t: &Take) {
        match self {
            Player::Placed(p) => {
                p.set_stream(t.stream);
                p.set_bus(t.bus);
                p.set_volume_db(t.db);
                p.set_pitch_scale(t.pitch);
                p.set_max_distance(t.range);
                if let Some(at) = t.at {
                    p.set_global_position(at);
                }
                p.play();
            }
            Player::Flat(p) => {
                p.set_stream(t.stream);
                p.set_bus(t.bus);
                p.set_volume_db(t.db);
                p.set_pitch_scale(t.pitch);
                p.play();
            }
        }
    }
}

/// The player of cues.
pub struct Audio {
    manifest: Manifest,
    ears: Gd<AudioListener3D>,
    placed: Vec<Voice>,
    flat: Vec<Voice>,
    /// Loaded files (None: it would not load; said once).
    streams: HashMap<String, Option<Gd<AudioStream>>>,
    /// Files still to load ahead of their first play.
    to_load: Vec<String>,
    /// When each cue last played, and which variant.
    last: HashMap<String, (f64, usize)>,
    /// Sounds waiting for their moment.
    waiting: Vec<(f32, Sound)>,
    picker: Picker,
    clock: f64,
    heard: VecDeque<Heard>,
    asked: u64,
    /// The sound's own time this frame (starting sounds, the waiting ones,
    /// the ears), and that of the last frames (files loading ahead aside).
    cost: Duration,
    costs: VecDeque<Duration>,
}

impl Audio {
    /// `root`: a node in the map's 3D world (the voices and the ears go
    /// under it).
    pub fn new(mut root: Gd<Node3D>) -> Audio {
        let text = FileAccess::get_file_as_string(MANIFEST).to_string();
        let manifest = Manifest::parse(&text).unwrap_or_else(|e| {
            godot_warn!("renethack: the audio manifest: {e}; using the one built into the client");
            Manifest::parse(BUILT_IN_MANIFEST).expect("the built-in audio manifest is valid")
        });
        let mut ears = AudioListener3D::new_alloc();
        ears.set_name("Ears");
        root.add_child(&ears);
        ears.make_current();
        let placed = (0..PLACED_VOICES)
            .map(|i| {
                let mut p = AudioStreamPlayer3D::new_alloc();
                p.set_name(&format!("Voice{i}"));
                p.set_unit_size(UNIT_SIZE);
                p.set_attenuation_model(AttenuationModel::INVERSE_DISTANCE);
                root.add_child(&p);
                Voice::new(Player::Placed(p))
            })
            .collect();
        let flat = (0..FLAT_VOICES)
            .map(|i| {
                let mut p = AudioStreamPlayer::new_alloc();
                p.set_name(&format!("Flat{i}"));
                root.add_child(&p);
                Voice::new(Player::Flat(p))
            })
            .collect();
        // the loops are music and ambience, loaded when they play
        let to_load: BTreeSet<String> = manifest
            .cues
            .values()
            .filter(|c| !c.looped)
            .flat_map(|c| c.files.iter().cloned())
            .collect();
        Audio {
            manifest,
            ears,
            placed,
            flat,
            streams: HashMap::new(),
            to_load: to_load.into_iter().rev().collect(),
            last: HashMap::new(),
            waiting: Vec::new(),
            picker: Picker::default(),
            clock: 0.0,
            heard: VecDeque::new(),
            asked: 0,
            cost: Duration::ZERO,
            costs: VecDeque::new(),
        }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Play `sound`, now or after its delay.
    pub fn play(&mut self, sound: Sound) {
        if sound.delay > 0.0 {
            self.waiting.push((sound.delay, sound));
        } else {
            self.start(sound);
        }
    }

    /// An unplaced cue (the interface's).
    pub fn ui(&mut self, cue: &str) {
        self.play(Sound::new(cue));
    }

    fn start(&mut self, sound: Sound) {
        let t = Instant::now();
        self.start_now(sound);
        self.cost += t.elapsed();
    }

    fn start_now(&mut self, sound: Sound) {
        self.asked += 1;
        let mut heard = Heard {
            seq: self.asked,
            asked: sound.cues.first().cloned().unwrap_or_default(),
            cue: None,
            at: sound.at,
            voiced: false,
        };
        let found = if sound.exact {
            self.manifest.resolve_exact(&sound.cues)
        } else {
            self.manifest.resolve(&sound.cues)
        };
        let Some((name, cue)) = found else {
            return self.record(heard);
        };
        let name = name.to_string();
        heard.cue = Some(name.clone());
        let last = self.last.get(&name).copied();
        if last.is_some_and(|(when, _)| self.clock - when < f64::from(cue.cooldown)) {
            return self.record(heard);
        }
        let variant = self.picker.variant(cue.files.len(), last.map(|(_, v)| v));
        let (pitch_k, db_k) = sound
            .size
            .filter(|_| cue.sized)
            .map_or((1.0, 0.0), Size::scale);
        let pitch = (cue.pitch.at(self.picker.unit()) * pitch_k).max(0.05);
        let db = cue.volume.at(self.picker.unit()) + db_k;
        let priority = sound.priority.unwrap_or(cue.priority);
        let (bus, voices, range) = (cue.bus.clone(), cue.voices, cue.range);
        let Some(file) = cue.files.get(variant).cloned() else {
            return self.record(heard);
        };
        let Some(stream) = self.stream(&file) else {
            return self.record(heard);
        };
        let placed = sound.at.is_some();
        let Some(i) = self.voice_for(placed, &name, voices, priority) else {
            return self.record(heard);
        };
        let now = self.clock;
        let voice = if placed {
            &mut self.placed[i]
        } else {
            &mut self.flat[i]
        };
        voice.player.start(&Take {
            stream: &stream,
            bus: &bus,
            db,
            pitch,
            at: sound.at,
            range,
        });
        voice.cue = name.clone();
        voice.priority = priority;
        voice.started = now;
        voice.ends = now + stream.get_length() / f64::from(pitch) + 0.05;
        self.last.insert(name, (now, variant));
        heard.voiced = true;
        self.record(heard);
    }

    /// A voice for a new sound of `cue`: the oldest of its own when it has
    /// its `limit` at once, else a free one, else the oldest of no higher
    /// priority.
    fn voice_for(&self, placed: bool, cue: &str, limit: u32, priority: u8) -> Option<usize> {
        let now = self.clock;
        let pool = if placed { &self.placed } else { &self.flat };
        let oldest = |pick: &dyn Fn(&Voice) -> bool| {
            pool.iter()
                .enumerate()
                .filter(|(_, v)| v.busy(now) && pick(v))
                .min_by(|a, b| a.1.started.total_cmp(&b.1.started))
                .map(|(i, _)| i)
        };
        let own = pool.iter().filter(|v| v.busy(now) && v.cue == cue).count();
        if own >= limit.max(1) as usize {
            return oldest(&|v| v.cue == cue);
        }
        pool.iter()
            .position(|v| !v.busy(now))
            .or_else(|| oldest(&|v| v.priority <= priority))
    }

    /// The file's stream, loaded once.
    fn stream(&mut self, file: &str) -> Option<Gd<AudioStream>> {
        if let Some(s) = self.streams.get(file) {
            return s.clone();
        }
        let path = format!("{AUDIO_DIR}/{file}");
        let mut loader = ResourceLoader::singleton();
        let stream = if loader.exists(&path) {
            loader
                .load(&path)
                .and_then(|r| r.try_cast::<AudioStream>().ok())
        } else {
            None
        };
        if stream.is_none() {
            godot_warn!("renethack: the sound {path} is missing (not imported?)");
        }
        self.streams.insert(file.to_string(), stream.clone());
        stream
    }

    fn record(&mut self, heard: Heard) {
        if self.heard.len() == HEARD_KEPT {
            self.heard.pop_front();
        }
        self.heard.push_back(heard);
    }

    /// A frame: the sounds whose moment came start, the ears go where the
    /// listener stands (the hero, else the camera), a few files load.
    pub fn update(&mut self, delta: f64, ears: Option<Transform3D>) {
        let t = Instant::now();
        self.clock += delta;
        let mut due = Vec::new();
        self.waiting.retain_mut(|(left, sound)| {
            *left -= delta as f32;
            if *left <= 0.0 {
                due.push(sound.clone());
                false
            } else {
                true
            }
        });
        for sound in due {
            self.start(sound);
        }
        if let Some(ears) = ears {
            self.ears.set_global_transform(ears);
        }
        if self.costs.len() == COSTS_KEPT {
            self.costs.pop_front();
        }
        self.costs
            .push_back(std::mem::take(&mut self.cost) + t.elapsed());
        for _ in 0..PRELOAD_STEP {
            let Some(file) = self.to_load.pop() else {
                break;
            };
            self.stream(&file);
        }
    }

    /// The sound's time a frame over the last frames (files loading ahead
    /// aside): the median, the 99th percentile and the most.
    pub fn frame_costs(&self) -> (Duration, Duration, Duration) {
        let mut c: Vec<Duration> = self.costs.iter().copied().collect();
        if c.is_empty() {
            return Default::default();
        }
        c.sort_unstable();
        let at = |q: f64| c[((c.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.99), c[c.len() - 1])
    }

    /// Every file of the cues played on the map is loaded.
    pub fn preloaded(&self) -> bool {
        self.to_load.is_empty()
    }

    /// Files that would not load.
    pub fn missing(&self) -> Vec<&str> {
        let mut m: Vec<&str> = self
            .streams
            .iter()
            .filter(|(_, s)| s.is_none())
            .map(|(f, _)| f.as_str())
            .collect();
        m.sort_unstable();
        m
    }

    /// The sounds asked for, oldest first (the last `HEARD_KEPT`).
    pub fn heard(&self) -> impl DoubleEndedIterator<Item = &Heard> {
        self.heard.iter()
    }

    /// How many sounds were asked for so far.
    pub fn asked(&self) -> u64 {
        self.asked
    }

    /// Voices busy now with `cue`.
    pub fn busy_with(&self, cue: &str) -> usize {
        let now = self.clock;
        let busy = |v: &&Voice| v.busy(now) && v.cue == cue;
        self.placed.iter().filter(busy).count() + self.flat.iter().filter(busy).count()
    }

    /// Voices busy now: placed, unplaced.
    pub fn voices_busy(&self) -> (usize, usize) {
        let now = self.clock;
        (
            self.placed.iter().filter(|v| v.busy(now)).count(),
            self.flat.iter().filter(|v| v.busy(now)).count(),
        )
    }

    /// The voices each pool has.
    pub fn voices(&self) -> (usize, usize) {
        (self.placed.len(), self.flat.len())
    }
}

/// What the interface shows that sounds when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shown {
    pub inventory: bool,
    pub help: bool,
    /// A question or a menu of the engine's.
    pub dialog: bool,
    /// A full-screen page (title, settings...).
    pub screen: Option<&'static str>,
    /// The button under the mouse.
    pub hovered: Option<InstanceId>,
}

/// The interface's sounds: what changed since the last frame.
#[derive(Debug, Default)]
pub struct UiWatch {
    last: Option<Shown>,
}

impl UiWatch {
    /// The cues of the changes from the last frame to `now` (none on the
    /// first: what shows at start-up was not opened).
    pub fn cues(&mut self, now: Shown) -> Vec<&'static str> {
        let Some(last) = self.last.replace(now) else {
            return Vec::new();
        };
        let mut cues = Vec::new();
        let mut opened = |was: bool, is: bool, open, close| {
            if is != was {
                cues.push(if is { open } else { close });
            }
        };
        opened(last.inventory, now.inventory, "inv.open", "inv.close");
        opened(last.help, now.help, "book.open", "book.close");
        if now.dialog && !last.dialog {
            cues.push("ui.open");
        }
        // a page over the title or the game opens; back to them closes
        match (last.screen, now.screen) {
            (a, b) if a == b => {}
            (Some(_), None | Some("title")) => cues.push("ui.close"),
            (_, Some(_)) => cues.push("ui.open"),
            _ => {}
        }
        if now.hovered.is_some() && now.hovered != last.hovered {
            cues.push("ui.hover");
        }
        cues
    }
}

impl Voice {
    fn new(player: Player) -> Voice {
        Voice {
            player,
            cue: String::new(),
            priority: 0,
            started: 0.0,
            ends: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buses_are_named_as_the_mixer_and_profile_know_them() {
        let names: Vec<_> = Bus::ALL.iter().map(|b| b.name()).collect();
        assert_eq!(names, ["Master", "Music", "Ambience", "SFX", "UI"]);
        assert!(!names.contains(&DISTANT), "not the player's to set");
        // every one the game makes (Master is Godot's own)
        for name in &names[1..] {
            assert!(MADE.iter().any(|(n, _)| n == name), "{name}");
        }
        // every bus made sends to one made before it (or Master)
        for (i, (name, send)) in MADE.iter().enumerate() {
            assert!(
                *send == "Master" || MADE[..i].iter().any(|(n, _)| n == send),
                "{name} sends to {send}"
            );
        }
    }

    #[test]
    fn the_interface_sounds_what_opens_and_closes() {
        let mut w = UiWatch::default();
        let shown = Shown {
            screen: Some("title"),
            ..Shown::default()
        };
        assert!(w.cues(shown).is_empty(), "what shows at start-up");
        assert!(w.cues(shown).is_empty(), "nothing changed");
        let game = Shown::default();
        assert_eq!(w.cues(game), ["ui.close"], "the title leaves");
        let bag = Shown {
            inventory: true,
            dialog: true,
            ..game
        };
        assert_eq!(w.cues(bag), ["inv.open", "ui.open"]);
        assert_eq!(
            w.cues(game),
            ["inv.close"],
            "a dialog closing is its click's"
        );
        let help = Shown { help: true, ..game };
        assert_eq!(w.cues(help), ["book.open"]);
        let button = InstanceId::from_i64(7);
        let hover = Shown {
            hovered: Some(button),
            ..help
        };
        assert_eq!(w.cues(hover), ["ui.hover"]);
        assert!(w.cues(hover).is_empty(), "still over the same button");
        let settings = Shown {
            screen: Some("settings"),
            ..hover
        };
        assert_eq!(w.cues(settings), ["ui.open"]);
        let title = Shown {
            screen: Some("title"),
            ..hover
        };
        assert_eq!(w.cues(title), ["ui.close"], "back to the title");
    }

    #[test]
    fn a_volume_goes_to_decibels_and_back() {
        for v in [1.0, 0.7, 0.5, 0.25, 0.05] {
            assert!((linear_of(db_of(v)) - v).abs() < 1e-4, "{v}");
        }
        assert_eq!(db_of(1.0), 0.0);
        assert!((db_of(0.5) + 6.02).abs() < 0.01);
        assert!(db_of(0.0) <= -60.0, "nothing is silence");
    }

    #[test]
    fn a_profile_keeps_the_volumes_set_and_the_rest_are_defaults() {
        let kept = BTreeMap::from([("Music".to_string(), 30), ("SFX".to_string(), 250)]);
        assert_eq!(kept_volume(&kept, Bus::Music), 0.3);
        assert_eq!(kept_volume(&kept, Bus::Sfx), 1.0, "clamped");
        assert_eq!(kept_volume(&kept, Bus::Ui), Bus::Ui.default_volume());
        assert_eq!(kept_volume(&kept, Bus::Master), 1.0);
        for v in [0.0, 0.05, 0.7, 1.0] {
            assert_eq!(f32::from(percent(v)) / 100.0, v, "a slider's step keeps");
        }
    }
}
