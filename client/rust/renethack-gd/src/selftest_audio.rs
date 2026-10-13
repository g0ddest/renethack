//! `audio`: the game's sound, heard through the player's record of what
//! it was asked to play (a counting dummy: headless, Godot plays nothing).
//! At the title: the buses, each sending where it should, and every file
//! of the manifest loaded. The mouse over a button ticks, a click clicks
//! and opens the settings page with its own sound; a volume set there is
//! the bus's at once (0 mutes it) and the profile keeps it when the page
//! closes. The voices: forty steps at once take no more than a step's
//! voices, a sound waits for its moment, a cue the manifest lacks plays
//! nothing. Loops of the test's own (jingles that go round): the title's
//! ambience and music fade in, and the dungeon's and the exploring take
//! over when the game begins; a newt made beside the hero brings the
//! fight's music once the fight has lasted, the ambience lowered under
//! it; the level's jingle holds the music down while it plays. Then a game (debug mode): the
//! hero's step sounds where they
//! stand and the pet's where it walks; a newt fought to its death sounds
//! the swing of the Valkyrie's spear, the blows or misses and its fall
//! (heard where the hero stands: a newt leaves nothing to place it by); a
//! potion of gain level drunk gulps and plays the level's jingle. The
//! engine's lines sound by the manifest's rules: the dagger dropped and
//! picked up again, gold wished for and dropped, a door's line at the
//! hero, a "You hear" far off and unplaced. Blindfolded with an amulet
//! of ESP the hero senses the pet walking, and hears none of its steps.
//! The blindfold off, a wand of cold zapped at the nearest wall sounds
//! its bolt where it is first seen and its bounce where it last lit a
//! cell; a scroll of fire's explosion sounds once. A fountain wished up
//! under the hero sounds its loop there once the hero steps off it and
//! sees it (a loop of the test's own). The sound's own time a frame stays
//! small.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use godot::classes::{
    AudioServer, BaseButton, Input, InputEventMouseButton, InputEventMouseMotion,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_world::Profile;

use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt};

use super::{
    DialogEvent, Step, UiEvent, command, ctrl_key, key, letter_of, map_view, screen, start,
};
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

/// The game's seed: the hero's first step and the pet in sight.
const SEED: u64 = 7;
/// Turns searched at most while waiting for the pet to walk.
const SEARCHES: u32 = 40;
/// The pet's steps the blind hero must sense (and not hear).
const SENSED_STEPS: u32 = 3;

/// The directions and their keys (the scenarios' vi-keys).
pub(super) const STEPS: [((i32, i32), char); 8] = [
    ((-1, 0), 'h'),
    ((1, 0), 'l'),
    ((0, -1), 'k'),
    ((0, 1), 'j'),
    ((-1, -1), 'y'),
    ((1, -1), 'u'),
    ((-1, 1), 'b'),
    ((1, 1), 'n'),
];

/// The record's length when a step began: what it checks came after.
static MARK: AtomicU64 = AtomicU64::new(0);
/// The map's steps of others when the blind hero began to search, and the
/// turns searched since.
static OTHERS: AtomicU32 = AtomicU32::new(0);
static SEARCHED: AtomicU32 = AtomicU32::new(0);
/// The fight's F was sent: the next command takes its direction.
static FIGHTING: AtomicBool = AtomicBool::new(false);
/// The search's m was sent: the next command is the search it forces.
static FORCING: AtomicBool = AtomicBool::new(false);
/// Where the settings button is on the screen.
static BUTTON: Mutex<Option<Vector2>> = Mutex::new(None);
/// The cell of the fountain wished up.
static FOUNTAIN: Mutex<Option<(i32, i32)>> = Mutex::new(None);

pub(super) fn audio() -> Vec<Step> {
    let mut steps = title();
    steps.extend(game());
    steps
}

fn title() -> Vec<Step> {
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
                let heard: Vec<String> = player(g)?
                    .heard()
                    .filter(|h| !h.asked.starts_with("amb.") && !h.asked.starts_with("music."))
                    .map(|h| h.asked.clone())
                    .collect();
                if !heard.is_empty() {
                    return Err(format!("heard at start-up: {heard:?}"));
                }
                for loop_ in ["amb.title", "music.title"] {
                    if !player(g)?.heard().any(|h| h.asked == loop_) {
                        return Err(format!("{loop_} was not asked for"));
                    }
                }
                Ok(())
            },
        ),
        Step::Call("loops of the test's own: jingles that go round", |g| {
            let a = &mut g.ui.as_mut().ok_or("no UI")?.audio;
            let file = |cue: &str| a.manifest().cues.get(cue).map(|c| c.files[0].clone());
            let (Some(title), Some(main)) = (file("jingle.levelup"), file("jingle.death")) else {
                return Err("no jingles to make loops of".into());
            };
            a.add_cue("amb.title", test_loop(&title, "Ambience")?);
            a.add_cue("amb.main", test_loop(&main, "Ambience")?);
            a.add_cue("music.title", test_loop(&main, "Music")?);
            a.add_cue("music.explore", test_loop(&title, "Music")?);
            a.add_cue("music.combat", test_loop(&main, "Music")?);
            Ok(())
        }),
        Step::Wait("the title's ambience and music faded in", |g| {
            let a = player(g)?;
            Ok(a.ambience_now() == (Some("amb.title"), 1.0)
                && a.music_now() == (Some("music.title"), 1.0))
        }),
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
            Ok(())
        }),
    ]
}

fn game() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        g.seed = Some(SEED);
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the dungeon's ambience and the exploring faded in", |g| {
            let a = player(g)?;
            let main = a.heard().any(|h| h.asked == "amb.main" && h.voiced);
            Ok(main
                && a.ambience_now() == (Some("amb.main"), 1.0)
                && a.music_now() == (Some("music.explore"), 1.0))
        }),
        Step::Call("", mark),
        Step::KeyFrom("a step onto open floor", |g| open_way(g).map(plain)),
        Step::Request("a command after the step", command),
        Step::Wait("the hero's step, where they stand", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            Ok(steps_heard(g)?.iter().any(|h| near(h, hero)))
        }),
        Step::Call("", mark),
        Step::KeysUntil("searching until the pet's step is heard", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            if steps_heard(g)?
                .iter()
                .any(|h| h.at.is_some() && !near(h, hero))
            {
                return Ok(None);
            }
            search(g)
        }),
    ]);
    steps.extend(create("newt"));
    steps.extend([
        Step::Wait("the fight's music, once the fight has lasted", |g| {
            let fight = player(g)?.music_now().0 == Some("music.combat");
            Ok(g.score.fighting() && fight)
        }),
        Step::Wait("the ambience lowered under the fight's music", |g| {
            let (music, ambience) = player(g)?.ducks();
            Ok(music == 1.0 && ambience <= 0.5)
        }),
        Step::Call("", mark),
        Step::KeysUntil("fighting the newt", fight),
        Step::Wait("its death heard", |g| Ok(heard_like(g, "die")?.is_some())),
        Step::Call(
            "the fight heard: each of the spear's swings, a blow or a miss",
            |g| {
                let mark = MARK.load(Ordering::Relaxed);
                let swings: Vec<Heard> = player(g)?
                    .heard()
                    .filter(|h| h.seq > mark && h.asked.starts_with("swing"))
                    .cloned()
                    .collect();
                let each = swings.iter().all(|h| h.asked == "swing.spear" && h.voiced);
                if swings.is_empty() || !each {
                    return Err(format!("the spear's swings: {swings:?}"));
                }
                if heard_like(g, "hit")?.is_none() && heard_like(g, "miss")?.is_none() {
                    return Err("neither a blow nor a miss".into());
                }
                Ok(())
            },
        ),
    ]);
    steps.extend(wish("uncursed potion of gain level"));
    steps.extend([
        Step::Call("", mark),
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the potion", |g| letter(g, "potion")),
        Step::Request("a command after the drink", command),
        Step::Wait("the drink and the level heard", |g| {
            let level = heard_like(g, "jingle.levelup")?;
            Ok(heard_like(g, "use.quaff")?.is_some()
                && level.is_some_and(|h| h.at.is_none() && h.voiced))
        }),
        Step::Wait("the music held down under the jingle", |g| {
            Ok(player(g)?.ducks().0 <= 0.25)
        }),
        Step::Call("", mark),
        key('d'),
        Step::Request("what to drop", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the dagger", |g| letter(g, "dagger")),
        Step::Request("a command after the drop", command),
        Step::Wait("the drop heard where the hero stands", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            Ok(heard_like(g, "drop")?.is_some_and(|h| near(&h, hero) && h.voiced))
        }),
        Step::Call("", mark),
        key(','),
        Step::Request("a command after the pickup", command),
        Step::Wait("the pickup heard", |g| {
            Ok(heard_like(g, "pickup")?.is_some_and(|h| h.asked == "pickup" && h.voiced))
        }),
        Step::Call("", mark),
    ]);
    steps.extend(wish("100 gold pieces"));
    steps.extend([
        Step::Wait("the gold heard", |g| {
            Ok(heard_like(g, "pickup.gold")?.is_some_and(|h| h.voiced))
        }),
        Step::Call("", mark),
        key('d'),
        Step::Request("what to drop", |p| !matches!(p, Prompt::Command)),
        key('$'),
        Step::Request("a command after the gold dropped", command),
        Step::Wait("the gold dropped heard", |g| {
            Ok(heard_like(g, "drop.gold")?.is_some_and(|h| h.voiced))
        }),
        Step::Call("a door's line and a \"You hear\" in the log", |g| {
            mark(g)?;
            let turn = g.world.turn();
            g.world.log.push("The door opens.".into(), 0, turn, false);
            g.world
                .log
                .push("You hear bubbling water.".into(), 0, turn, false);
            Ok(())
        }),
        Step::Wait("the door at the hero, the water far off", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            let door = heard_like(g, "door.open")?;
            let water = heard_like(g, "distant.water")?;
            Ok(door.is_some_and(|d| near(&d, hero) && d.voiced)
                && water.is_some_and(|w| {
                    w.at.is_none() && w.voiced && w.cue.is_some_and(|c| c.starts_with("distant"))
                }))
        }),
    ]);
    steps.extend(wish("blessed amulet of ESP"));
    steps.extend(put_on(|g| letter(g, "amulet")));
    steps.extend(wish("uncursed blindfold"));
    steps.extend(put_on(|g| letter(g, "blindfold")));
    steps.extend([
        Step::Wait("the hero blind", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let names = g.world.status.condition_names(cat);
            Ok(names.iter().any(|n| n == "Blind"))
        }),
        // a turn more: what moved while the status caught up is past
        key('m'),
        Step::Request("the search to force", command),
        key('s'),
        Step::Request("a command after a turn blind", command),
        Step::Call("", |g| {
            mark(g)?;
            OTHERS.store(map_view(g)?.motion_stats().other_steps, Ordering::Relaxed);
            SEARCHED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("searching while the pet walks, sensed", |g| {
            let walked = map_view(g)?.motion_stats().other_steps - OTHERS.load(Ordering::Relaxed);
            if walked >= SENSED_STEPS && !FORCING.load(Ordering::Relaxed) {
                return Ok(None);
            }
            forced_search(g)
        }),
        Step::Call("none of its steps heard", |g| {
            let heard = steps_heard(g)?;
            if !heard.is_empty() {
                return Err(format!(
                    "steps heard of what the blind hero senses: {heard:?}"
                ));
            }
            Ok(())
        }),
        key('R'),
        Step::Request("what to take off", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the blindfold", |g| letter(g, "blindfold")),
        Step::Request("a command, seeing again", command),
    ]);
    // a Valkyrie stands her own bolt of cold
    steps.extend(wish("wand of cold"));
    steps.extend([
        Step::Call("", mark),
        key('z'),
        Step::Request("what to zap", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the wand", |g| letter(g, "wand")),
        Step::Request("the zap's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        Step::KeyFrom("toward the nearest wall", |g| wall_way(g).map(plain)),
        Step::AnswerUntil('n', "a command after the zap", |g| {
            Ok(matches!(pending(g), Some(Prompt::Command)))
        }),
        Step::Wait("the bolt and its bounce heard", |g| {
            let bolt = heard_like(g, "ray.cold")?;
            let bounce = heard_like(g, "ray.bounce")?;
            Ok(bolt.is_some_and(|h| placed(&h))
                && bounce.is_some_and(|h| h.asked == "ray.bounce.cold" && placed(&h)))
        }),
    ]);
    steps.extend(wish("uncursed scroll of fire"));
    steps.extend([
        Step::Call("", mark),
        key('r'),
        Step::Request("what to read", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the scroll", |g| letter(g, "scroll")),
        Step::AnswerUntil('n', "a command after the fire", |g| {
            Ok(matches!(pending(g), Some(Prompt::Command)))
        }),
        Step::Wait("the explosion heard, once", |g| {
            let mark = MARK.load(Ordering::Relaxed);
            let blasts: Vec<Heard> = player(g)?
                .heard()
                .filter(|h| h.seq > mark && h.asked.starts_with("blast"))
                .cloned()
                .collect();
            if blasts.len() > 1 {
                return Err(format!(
                    "one explosion heard {} times: {blasts:?}",
                    blasts.len()
                ));
            }
            Ok(blasts
                .first()
                .is_some_and(|h| h.asked == "blast.fiery" && placed(h)))
        }),
        Step::Call("a fountain's loop of the test's own", |g| {
            let a = &mut g.ui.as_mut().ok_or("no UI")?.audio;
            let file = a
                .manifest()
                .cues
                .get("jingle.levelup")
                .map(|c| c.files[0].clone());
            a.add_cue(
                "emit.fountain",
                test_loop(&file.ok_or("no jingle")?, "Ambience")?,
            );
            mark(g)
        }),
    ]);
    // onto bare floor first: what lies on a cell hides its floor from
    // the map (the gold dropped here)
    steps.extend([
        Step::KeyFrom("a step onto bare floor", |g| open_way(g).map(plain)),
        Step::Request("a command on bare floor", command),
    ]);
    steps.extend(wish("fountain"));
    steps.extend([
        Step::Call("where the fountain is", |g| {
            *FOUNTAIN.lock().unwrap() = g.world.map.hero();
            Ok(())
        }),
        Step::KeyFrom("a step off it", |g| open_way(g).map(plain)),
        Step::Request("a command beside the fountain", command),
        Step::Wait("the fountain sounding where it stands", |g| {
            let fountain = FOUNTAIN.lock().unwrap().ok_or("no fountain")?;
            let asked = heard_like(g, "emit.fountain")?.is_some_and(|h| near(&h, fountain));
            Ok(asked && player(g)?.emitters_now() == [("emit.fountain", fountain)])
        }),
        Step::Call("the sound's time a frame", |g| {
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
    ]);
    steps
}

/// A loop of the test's own: `file` going round on `bus`.
fn test_loop(file: &str, bus: &str) -> Result<crate::cues::Cue, String> {
    let cue = serde_json::json!({"files": [file], "bus": bus, "loop": true});
    serde_json::from_value(cue).map_err(|e| e.to_string())
}

fn plain(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

/// A heard sound placed on the hero's cell (or next to it, as a step
/// lands while the model is still on its way).
fn near(h: &Heard, (x, y): (i32, i32)) -> bool {
    h.at.is_some_and(|p| (p.x - x as f32).abs() <= 1.0 && (p.z - y as f32).abs() <= 1.0)
}

/// Asked for at a place, and playing if the manifest has a sound for it
/// (rays and blasts may have none yet).
fn placed(h: &Heard) -> bool {
    h.at.is_some() && (h.cue.is_none() || h.voiced)
}

/// The steps heard since the mark.
fn steps_heard(game: &RenethackGame) -> Result<Vec<Heard>, String> {
    let mark = MARK.load(Ordering::Relaxed);
    Ok(player(game)?
        .heard()
        .filter(|h| h.seq > mark && (h.asked == "step" || h.asked.starts_with("step.")))
        .cloned()
        .collect())
}

/// The last sound asked for since the mark whose name is `kind` or begins
/// with it and a dot.
fn heard_like(game: &RenethackGame, kind: &str) -> Result<Option<Heard>, String> {
    let mark = MARK.load(Ordering::Relaxed);
    let dotted = format!("{kind}.");
    Ok(player(game)?
        .heard()
        .rev()
        .take_while(|h| h.seq > mark)
        .find(|h| h.asked == kind || h.asked.starts_with(&dotted))
        .cloned())
}

/// A turn searched at a command, else Esc; at most `SEARCHES`.
fn search(game: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if !matches!(pending(game), Some(Prompt::Command)) {
        return Ok(Some(KeyInput::plain(Key::Escape)));
    }
    if SEARCHED.fetch_add(1, Ordering::Relaxed) >= SEARCHES {
        return Err(format!("{SEARCHES} turns searched and nothing walked"));
    }
    Ok(Some(plain('s')))
}

/// As `search`, each search forced by `m` (the engine refuses one while
/// a hostile it shows stands beside the hero).
fn forced_search(game: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if matches!(pending(game), Some(Prompt::Command)) && !FORCING.swap(false, Ordering::Relaxed) {
        FORCING.store(true, Ordering::Relaxed);
        return Ok(Some(plain('m')));
    }
    search(game)
}

/// The key of a step onto open floor next to the hero.
pub(super) fn open_way(game: &RenethackGame) -> Result<char, String> {
    let cat = game.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = game.world.map.hero().ok_or("no hero")?;
    STEPS
        .iter()
        .find_map(|&((dx, dy), key)| {
            let cell = game.world.map.cell(hx + dx, hy + dy)?;
            let open = matches!(
                nh_world::cell_terrain(cell, cat),
                Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
            ) && cell.entity().is_none();
            open.then_some(key)
        })
        .ok_or_else(|| format!("no open floor next to the hero at {:?}", (hx, hy)))
}

/// The key of the straight way to the wall nearest the hero (a bolt
/// bounces there).
fn wall_way(game: &RenethackGame) -> Result<char, String> {
    let cat = game.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = game.world.map.hero().ok_or("no hero")?;
    let cardinal = STEPS.iter().filter(|((dx, dy), _)| dx * dy == 0);
    cardinal
        .filter_map(|&((dx, dy), key)| {
            (1..30).find_map(|n| {
                let cell = game.world.map.cell(hx + dx * n, hy + dy * n)?;
                let wall = matches!(
                    nh_world::cell_terrain(cell, cat),
                    Some(nh_world::Terrain::Wall | nh_world::Terrain::Stone)
                );
                wall.then_some((n, key))
            })
        })
        .min()
        .map(|(_, key)| key)
        .ok_or_else(|| "no wall in a straight line from the hero".into())
}

/// The newt's way from the hero: a monster next to them, not their pet.
fn foe(game: &RenethackGame) -> Option<char> {
    let (hx, hy) = game.world.map.hero()?;
    STEPS.iter().find_map(|&((dx, dy), key)| {
        let g = game.world.map.cell(hx + dx, hy + dy)?.glyph.as_ref()?;
        let foe = g.kind == GlyphKind::Mon && g.flags & (mg::HERO | mg::PET) == 0;
        foe.then_some(key)
    })
}

/// F and the newt's direction until no foe is left next to the hero.
fn fight(game: &RenethackGame) -> Result<Option<KeyInput>, String> {
    match pending(game) {
        Some(Prompt::Command) => {
            let Some(way) = foe(game) else {
                return Ok(None);
            };
            if FIGHTING.swap(false, Ordering::Relaxed) {
                Ok(Some(plain(way)))
            } else {
                FIGHTING.store(true, Ordering::Relaxed);
                Ok(Some(plain('F')))
            }
        }
        // "Really attack?"
        Some(Prompt::Choice { .. }) => Ok(Some(plain('y'))),
        _ => Ok(Some(KeyInput::plain(Key::Escape))),
    }
}

fn pending(game: &RenethackGame) -> Option<&Prompt> {
    game.pending.as_ref().map(|(_, p)| p)
}

/// A monster made by debug mode's ^G.
fn create(name: &'static str) -> [Step; 4] {
    [
        Step::Key(ctrl_key('g')),
        Step::Request(
            "the kind of monster to make",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
        Step::Request("a command after the monster", command),
    ]
}

fn wish(what: &'static str) -> [Step; 4] {
    [
        Step::Key(ctrl_key('w')),
        Step::Request(
            "the wish",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("wish")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::Request("a command after the wish", command),
    ]
}

/// `P` and the letter `which` picks.
fn put_on(which: fn(&RenethackGame) -> Result<KeyInput, String>) -> [Step; 4] {
    [
        key('P'),
        Step::Request("what to put on", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the thing to put on", which),
        Step::Request("a command after putting it on", command),
    ]
}

/// The key of the pack's thing whose name has `what`.
fn letter(game: &RenethackGame, what: &str) -> Result<KeyInput, String> {
    letter_of(game, what)
        .map(plain)
        .ok_or_else(|| format!("no {what} in the pack"))
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
