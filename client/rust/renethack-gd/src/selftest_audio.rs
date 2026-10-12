//! `audio`: the game's sound. The buses are made at start-up, each sending
//! where it should; a volume the settings set is the bus's at once (0
//! mutes it) and the profile keeps it when the page closes.

use godot::classes::AudioServer;
use godot::prelude::*;
use nh_world::Profile;

use super::{Step, UiEvent, screen};
use crate::audio::{self, Bus, DISTANT};
use crate::game::RenethackGame;

/// Each bus the game plays on, and the bus it sends to.
const SENDS: [(&str, &str); 5] = [
    ("Music", "Master"),
    ("Ambience", "Master"),
    ("SFX", "Master"),
    ("UI", "Master"),
    (DISTANT, "SFX"),
];

pub(super) fn audio() -> Vec<Step> {
    vec![
        Step::Wait("the title", |g| Ok(screen(g) == Some("title"))),
        Step::Call("the buses", |_| buses()),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page", |g| Ok(screen(g) == Some("settings"))),
        Step::Push(UiEvent::SetVolume(Bus::Music, 0.25)),
        Step::Push(UiEvent::SetVolume(Bus::Ui, 0.0)),
        Step::Wait("the volumes set", |_| {
            Ok((audio::volume(Bus::Music) - 0.25).abs() < 0.005 && audio::volume(Bus::Ui) == 0.0)
        }),
        Step::Call("a volume of 0 mutes its bus", |_| {
            let server = AudioServer::singleton();
            let ui = server.get_bus_index("UI");
            let music = server.get_bus_index("Music");
            if !server.is_bus_mute(ui) || server.is_bus_mute(music) {
                return Err("UI at 0 is not muted, or Music at 0.25 is".into());
            }
            Ok(())
        }),
        Step::Push(UiEvent::CloseSettings),
        Step::Wait("back at the title", |g| Ok(screen(g) == Some("title"))),
        Step::Call("the profile keeps them", kept),
        // back as they were: what follows is heard at the defaults
        Step::Push(UiEvent::SetVolume(Bus::Music, Bus::Music.default_volume())),
        Step::Push(UiEvent::SetVolume(Bus::Ui, Bus::Ui.default_volume())),
        Step::Wait("the defaults again", |_| {
            Ok(Bus::ALL
                .iter()
                .all(|&b| (audio::volume(b) - b.default_volume()).abs() < 0.005))
        }),
    ]
}

fn buses() -> Result<(), String> {
    let server = AudioServer::singleton();
    for (name, send) in SENDS {
        let i = server.get_bus_index(name);
        if i < 0 {
            return Err(format!("no {name} bus"));
        }
        let to = server.get_bus_send(i).to_string();
        if to != send {
            return Err(format!("{name} sends to {to:?}, not {send}"));
        }
    }
    for bus in Bus::ALL {
        let v = audio::volume(bus);
        if (v - bus.default_volume()).abs() > 0.005 {
            return Err(format!("{} starts at {v}, not its default", bus.name()));
        }
    }
    Ok(())
}

fn kept(game: &mut RenethackGame) -> Result<(), String> {
    let text = nh_link::read_profile(&game.playground()).ok_or("no profile")?;
    let volumes = Profile::from_json(&text).volumes;
    match (volumes.get("Music"), volumes.get("UI"), volumes.get("SFX")) {
        (Some(25), Some(0), Some(100)) => Ok(()),
        _ => Err(format!("the profile keeps {volumes:?}")),
    }
}
