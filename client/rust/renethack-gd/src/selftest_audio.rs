//! `audio`: the game's sound, heard through the player's record of what
//! it was asked to play (a counting dummy: headless, Godot plays nothing).
//! At the title: the buses, each sending where it should, and every file
//! of the manifest loaded. The mouse over a button ticks, a click clicks
//! and opens the settings page with its own sound; a volume set there is
//! the bus's at once (0 mutes it) and the profile keeps it when the page
//! closes. The voices: forty steps at once take no more than a step's
//! voices, a sound waits for its moment, a cue the manifest lacks plays
//! nothing. The sound's own time a frame stays small.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use godot::classes::{
    AudioServer, BaseButton, Input, InputEventMouseButton, InputEventMouseMotion,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_world::Profile;

use super::{Step, UiEvent, screen};
use crate::audio::{self, Audio, Bus, DISTANT, Heard, Sound};
use crate::cues::Size;
use crate::game::RenethackGame;
use crate::tr;

/// Each bus the game plays on, and the bus it sends to.
const SENDS: [(&str, &str); 5] = [
    ("Music", "Master"),
    ("Ambience", "Master"),
    ("SFX", "Master"),
    ("UI", "Master"),
    (DISTANT, "SFX"),
];

/// The sound's time in all but the busiest frames (a loaded machine
/// included), and forty steps started in one.
const FRAME_COST_MS: f64 = 1.0;
const FORTY_STEPS_MS: f64 = 5.0;

/// The record's length when a step began: what it checks came after.
static MARK: AtomicU64 = AtomicU64::new(0);
/// Where the settings button is on the screen.
static BUTTON: Mutex<Option<Vector2>> = Mutex::new(None);

pub(super) fn audio() -> Vec<Step> {
    vec![
        Step::Wait("the title, every sound loaded", |g| {
            Ok(screen(g) == Some("title") && player(g)?.preloaded())
        }),
        Step::Wait("the warm-up done", |g| Ok(g.warmed_up())),
        Step::Call(
            "the buses and the files; nothing heard of the warm-up",
            |g| {
                buses()?;
                let missing = player(g)?.missing();
                if !missing.is_empty() {
                    return Err(format!("sounds that would not load: {missing:?}"));
                }
                let heard: Vec<String> = player(g)?.heard().map(|h| h.asked.clone()).collect();
                if !heard.is_empty() {
                    return Err(format!("heard at start-up: {heard:?}"));
                }
                Ok(())
            },
        ),
        Step::Call("the mouse over the settings button", |g| {
            mark(g)?;
            let at = button_at(g, &tr!("title-settings"))?;
            *BUTTON.lock().unwrap() = Some(at);
            let mut ev = InputEventMouseMotion::new_gd();
            ev.set_position(at);
            ev.set_global_position(at);
            Input::singleton().parse_input_event(&ev);
            Ok(())
        }),
        Step::Wait("its tick", |g| Ok(heard_since(g, "ui.hover")?.is_some())),
        Step::Call("a click on it", |g| {
            mark(g)?;
            let at = BUTTON.lock().unwrap().ok_or("no button")?;
            for pressed in [true, false] {
                let mut ev = InputEventMouseButton::new_gd();
                ev.set_button_index(MouseButton::LEFT);
                ev.set_position(at);
                ev.set_global_position(at);
                ev.set_pressed(pressed);
                Input::singleton().parse_input_event(&ev);
            }
            Ok(())
        }),
        Step::Wait("the click, the page and its sound", |g| {
            Ok(heard_since(g, "ui.click")?.is_some()
                && screen(g) == Some("settings")
                && heard_since(g, "ui.open")?.is_some())
        }),
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
        Step::Call("", mark),
        Step::Push(UiEvent::CloseSettings),
        Step::Wait("back at the title, with its sound", |g| {
            Ok(screen(g) == Some("title") && heard_since(g, "ui.close")?.is_some())
        }),
        Step::Call("the profile keeps them", kept),
        // back as they were: what follows is heard at the defaults
        Step::Push(UiEvent::SetVolume(Bus::Music, Bus::Music.default_volume())),
        Step::Push(UiEvent::SetVolume(Bus::Ui, Bus::Ui.default_volume())),
        Step::Wait("the defaults again", |_| {
            Ok(Bus::ALL
                .iter()
                .all(|&b| (audio::volume(b) - b.default_volume()).abs() < 0.005))
        }),
        Step::Call("forty steps at once", |g| {
            mark(g)?;
            let a = &mut g.ui.as_mut().ok_or("no UI")?.audio;
            let t0 = std::time::Instant::now();
            for i in 0..40 {
                let size = ["tiny", "medium", "huge"][i % 3];
                let at = Vector3::new((i % 8) as f32, 0.0, (i / 8) as f32);
                a.play(
                    Sound::new("step.stone")
                        .or("step")
                        .at(at)
                        .sized(Size::of(size)),
                );
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            godot_print!("selftest: audio: forty steps started in {ms:.3} ms");
            if ms > FORTY_STEPS_MS {
                return Err(format!("forty steps took {ms:.2} ms to start"));
            }
            let limit = a.manifest().cues["step"].voices as usize;
            let busy = a.busy_with("step");
            if busy != limit {
                return Err(format!("{busy} steps sound at once, not {limit}"));
            }
            let (placed, _) = a.voices_busy();
            if placed > a.voices().0 {
                return Err(format!("{placed} voices busy of {}", a.voices().0));
            }
            let mark = MARK.load(Ordering::Relaxed);
            let all: Vec<&Heard> = a.heard().filter(|h| h.seq > mark).collect();
            if all.len() != 40
                || !all
                    .iter()
                    .all(|h| h.voiced && h.cue.as_deref() == Some("step"))
            {
                return Err(format!("the record: {all:?}"));
            }
            Ok(())
        }),
        Step::Call("a door after a while, and a cue that is not there", |g| {
            mark(g)?;
            let a = &mut g.ui.as_mut().ok_or("no UI")?.audio;
            a.play(
                Sound::new("door.open")
                    .at(Vector3::new(3.0, 0.0, 3.0))
                    .after(0.3),
            );
            a.play(Sound::new("no.such.cue"));
            let mark = MARK.load(Ordering::Relaxed);
            if a.heard().any(|h| h.asked == "door.open" && h.seq > mark) {
                return Err("the door sounded before its moment".into());
            }
            Ok(())
        }),
        Step::Wait("the door, in its moment", |g| {
            Ok(heard_since(g, "door.open")?.is_some())
        }),
        Step::Call("what was not there played nothing", |g| {
            let h = heard_since(g, "no.such.cue")?.ok_or("not asked")?;
            if h.cue.is_some() || h.voiced {
                return Err(format!("{h:?}"));
            }
            let door = heard_since(g, "door.open")?.ok_or("no door")?;
            if !door.voiced || door.at.is_none() {
                return Err(format!("{door:?}"));
            }
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            let (median, p99, most) = player(g)?.frame_costs();
            godot_print!(
                "selftest: audio: a frame of the sound: median {:.3} ms, 99% {:.3} ms, most {:.3} ms",
                ms(median),
                ms(p99),
                ms(most)
            );
            if ms(p99) > FRAME_COST_MS {
                return Err(format!("a frame of the sound took {:.2} ms", ms(p99)));
            }
            Ok(())
        }),
    ]
}

fn player(game: &RenethackGame) -> Result<&Audio, String> {
    Ok(&game.ui.as_ref().ok_or("no UI")?.audio)
}

/// From now on: what a check looks for must come after this.
fn mark(game: &mut RenethackGame) -> Result<(), String> {
    MARK.store(player(game)?.asked(), Ordering::Relaxed);
    Ok(())
}

/// The last sound asked for as `asked` since the mark.
fn heard_since(game: &RenethackGame, asked: &str) -> Result<Option<Heard>, String> {
    let mark = MARK.load(Ordering::Relaxed);
    Ok(player(game)?
        .heard()
        .rev()
        .take_while(|h| h.seq > mark)
        .find(|h| h.asked == asked)
        .cloned())
}

/// The middle of the shown button whose text begins with `text`, where the
/// window has it.
fn button_at(game: &RenethackGame, text: &str) -> Result<Vector2, String> {
    fn find(node: Gd<Node>, text: &str) -> Option<Gd<BaseButton>> {
        if let Ok(b) = node.clone().try_cast::<godot::classes::Button>()
            && b.is_visible_in_tree()
            && b.get_text().to_string().starts_with(text)
        {
            return Some(b.upcast());
        }
        node.get_children()
            .iter_shared()
            .find_map(|c| find(c, text))
    }
    let root = game.base().get_tree().get_root().ok_or("no window")?;
    let b = find(root.upcast(), text).ok_or_else(|| format!("no {text:?} button"))?;
    let vp = b.get_viewport().ok_or("no viewport")?;
    Ok(vp.get_screen_transform() * b.get_global_rect().center())
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
