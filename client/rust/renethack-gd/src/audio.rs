//! The game's sound: the mixer's buses, whose volumes the player sets.
//!
//! Master takes Music, Ambience, SFX and UI; Distant ("You hear ..."
//! cues, unplaced) goes through SFX. The buses are made here at start-up,
//! not in a bus layout resource the project would have to name.

use std::collections::BTreeMap;

use godot::classes::AudioServer;
use godot::prelude::*;

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
