//! Picture scenarios for motion on video (the movie writer records them;
//! the log says which frames to look at):
//!
//! - `bites`: a debug-mode Valkyrie (her pet slain first) at 7 m; a soldier
//!   ant, a sewer rat, a newt, a black pudding, a giant bat, a baby gray
//!   dragon and a raven are made beside her one at a time, each attacks
//!   her for a few turns (`selftest: bites: <foe> strikes at frame N`,
//!   where it and she stand in the window), then is slain by #wizkill.
//!   RENETHACK_BITES_DISTANCE: the camera's distance (11 m).
//! - `stride`: a debug-mode Valkyrie and her kitten, the level mapped; a
//!   travel (`_`, a far cell picked)
//!   and a click back, every frame logged (`selftest: stride: frame N ...`:
//!   the hero's cell, where her model stands, its clip and the clip's
//!   speed), so a run's cells a second and strides a cell can be read off.
//!
//! Nothing is checked but that the scenes were played.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicUsize, Ordering};

use godot::classes::Engine;
use godot::obj::Singleton;
use godot::prelude::*;
use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, camera_settled, command, ctrl_key, key, map_view, quit, start};
use crate::game::RenethackGame;
use crate::ui_events::UiEvent;

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// The foes made in turn beside her.
/// (The raven last: its claws blind her, and a blind hero sees no
/// blows.)
const FOES: [&str; 7] = [
    "soldier ant",
    "sewer rat",
    "newt",
    "black pudding",
    "giant bat",
    "baby gray dragon",
    "raven",
];
/// Turns she waits beside each.
const WAIT_TURNS: u32 = 6;

static FOE: AtomicUsize = AtomicUsize::new(0);
static STRIKES: AtomicU32 = AtomicU32::new(0);
static TARGET: Mutex<(i32, i32)> = Mutex::new((0, 0));
static PICKED: AtomicBool = AtomicBool::new(false);
static SINCE: AtomicI64 = AtomicI64::new(0);

/// A hostile within `r` cells of the hero (the nearest).
fn foe_near(g: &RenethackGame, r: i32) -> Option<(i32, i32)> {
    let (hx, hy) = g.world.map.hero()?;
    let mut best: Option<(i32, (i32, i32))> = None;
    for y in hy - r..=hy + r {
        for x in hx - r..=hx + r {
            let Some(gl) = g.world.map.cell(x, y).and_then(|c| c.glyph.as_ref()) else {
                continue;
            };
            if gl.kind != GlyphKind::Mon || gl.flags & (mg::PET | mg::HERO) != 0 {
                continue;
            }
            let d = (x - hx).abs().max((y - hy).abs());
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, (x, y)));
            }
        }
    }
    best.map(|(_, at)| at)
}

/// The strikes the map has played since the last call, logged with the
/// frame they began on.
fn log_strikes(g: &RenethackGame, what: &str) -> Result<(), String> {
    let s = map_view(g)?.motion_stats().strikes;
    let before = STRIKES.swap(s, Ordering::Relaxed);
    if s > before {
        // where the foe and the hero stand in the window (the movie's
        // pixels), for the crop
        let window = |cell: Option<(i32, i32)>| -> String {
            let (Some((x, y)), Some(vp)) = (cell, g.base().get_viewport()) else {
                return "-".into();
            };
            let Some(cam) = vp.get_camera_3d() else {
                return "-".into();
            };
            let at = vp.get_screen_transform()
                * cam.unproject_position(Vector3::new(x as f32, 0.3, y as f32));
            format!("{:.0},{:.0}", at.x, at.y)
        };
        eprintln!(
            "selftest: bites: {what} strikes at frame {} ({} blow(s)); foe {} hero {}",
            frames(),
            s - before,
            window(foe_near(g, 2)),
            window(g.world.map.hero()),
        );
    }
    Ok(())
}

/// Past whatever a turn beside the foe asks ("Die?" is answered no) to
/// the next command, logging the blows.
fn to_command(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    log_strikes(g, FOES[FOE.load(Ordering::Relaxed)])?;
    match g.pending.as_ref().map(|(_, p)| p) {
        Some(Prompt::Command) => Ok(None),
        Some(Prompt::Choice { query, .. }) if query.contains("Die?") => {
            Ok(Some(KeyInput::plain(Key::Char('n'))))
        }
        _ => Ok(Some(KeyInput::plain(Key::Escape))),
    }
}

/// A turn waited beside the foe (`m` then `s`: a plain search beside a
/// hostile passes no turn), then a second for its blows to be played.
fn turn_beside() -> Vec<Step> {
    vec![
        key('m'),
        key('s'),
        Step::KeysUntil("the next command", to_command),
        Step::Call("", |_| {
            SINCE.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("its blows played", |g| {
            log_strikes(g, FOES[FOE.load(Ordering::Relaxed)])?;
            Ok(frames() >= SINCE.load(Ordering::Relaxed) + 30)
        }),
    ]
}

/// The foe near her picked by #wizkill's cursor and slain.
fn slay_foe() -> Vec<Step> {
    vec![
        Step::Call("the foe's cell", |g| {
            let at = foe_near(g, 6).unwrap_or((0, 0));
            *TARGET.lock().unwrap() = at;
            PICKED.store(false, Ordering::Relaxed);
            Ok(())
        }),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        Step::KeysUntil("the foe picked and slain", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            if !g.world.getpos {
                return Ok(PICKED
                    .load(Ordering::Relaxed)
                    .then_some(())
                    .map_or(Some(KeyInput::plain(Key::Escape)), |_| None));
            }
            let target = *TARGET.lock().unwrap();
            if PICKED.load(Ordering::Relaxed) || target == (0, 0) {
                PICKED.store(true, Ordering::Relaxed);
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let (cx, cy) = g.world.cursor.or(g.world.map.hero()).ok_or("no cursor")?;
            if (cx, cy) == target {
                PICKED.store(true, Ordering::Relaxed);
                return Ok(Some(KeyInput::plain(Key::Char('.'))));
            }
            let (dx, dy) = ((target.0 - cx).signum(), (target.1 - cy).signum());
            let way = super::audio_tests::STEPS
                .iter()
                .find(|(d, _)| *d == (dx, dy))
                .ok_or("no way to the foe")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }),
    ]
}

pub(super) fn bites() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 11", |g| {
        g.debug_mode = true;
        g.seed = Some(11);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend(super::motion_tests::slay_pet());
    steps.extend([
        Step::Call("7 m off, as in play (or the distance asked)", |g| {
            let d = std::env::var("RENETHACK_BITES_DISTANCE")
                .ok()
                .and_then(|d| d.parse().ok())
                .unwrap_or(7.0);
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(d, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    steps.push(Step::Call("", |_| {
        FOE.store(0, Ordering::Relaxed);
        Ok(())
    }));
    for foe in FOES {
        steps.extend([
            Step::Key(ctrl_key('g')),
            Step::Request(
                "the kind of monster to make",
                |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster")),
            ),
            // (a neutral Valkyrie meets many of them peaceful)
            Step::Dialog(DialogEvent::TextSubmitted(format!("hostile {foe}"))),
            Step::Request("a command after the monster", command),
            Step::Call("its turns counted from here", |g| {
                STRIKES.store(map_view(g)?.motion_stats().strikes, Ordering::Relaxed);
                let foe = FOES[FOE.load(Ordering::Relaxed)];
                eprintln!("selftest: bites: {foe} made at frame {}", frames());
                Ok(())
            }),
        ]);
        for _ in 0..WAIT_TURNS {
            steps.extend(turn_beside());
        }
        steps.extend(slay_foe());
        steps.extend([
            Step::Request("the next command", command),
            Step::Call("the next foe", |_| {
                FOE.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(quit());
    steps
}

/// Where the travel started, and where it goes.
static FROM: Mutex<(i32, i32)> = Mutex::new((0, 0));
static GOAL: Mutex<(i32, i32)> = Mutex::new((0, 0));
/// Frames the hero has stood still on her goal.
static STILL: AtomicU32 = AtomicU32::new(0);
static LEG_FRAMES: AtomicU32 = AtomicU32::new(0);

/// Cells a travel crosses: floor, corridors, doorways, stairs.
fn passable(g: &RenethackGame, (x, y): (i32, i32)) -> bool {
    use nh_world::Terrain as T;
    let Some(cat) = g.catalog.as_deref() else {
        return false;
    };
    g.world.map.cell(x, y).is_some_and(|c| {
        matches!(
            nh_world::cell_terrain(c, cat),
            Some(
                T::Floor
                    | T::DarkFloor
                    | T::Corridor
                    | T::Doorway
                    | T::OpenDoor
                    | T::BrokenDoor
                    | T::StairsUp
                    | T::StairsDown
            )
        )
    })
}

/// Open floor (no stairs, nobody on it).
fn open_floor(g: &RenethackGame, (x, y): (i32, i32)) -> bool {
    let Some(cat) = g.catalog.as_deref() else {
        return false;
    };
    g.world.map.cell(x, y).is_some_and(|c| {
        matches!(
            nh_world::cell_terrain(c, cat),
            Some(
                nh_world::Terrain::Floor
                    | nh_world::Terrain::DarkFloor
                    | nh_world::Terrain::Corridor
            )
        ) && c.entity().is_none()
    })
}

/// The open floor farthest from the hero by the known way, at most
/// `most` steps: a travel long enough to take several of the engine's
/// drawings, round a corner or two.
fn far_floor(g: &RenethackGame, most: i32) -> Option<(i32, i32)> {
    let from = g.world.map.hero()?;
    let mut dist = std::collections::HashMap::from([(from, 0)]);
    let mut queue = std::collections::VecDeque::from([from]);
    let mut best = None;
    while let Some(at) = queue.pop_front() {
        let d = dist[&at];
        if open_floor(g, at) && best.is_none_or(|(b, _)| d > b) {
            best = Some((d, at));
        }
        if d == most {
            continue;
        }
        for (dx, dy) in super::audio_tests::STEPS.iter().map(|(d, _)| *d) {
            let next = (at.0 + dx, at.1 + dy);
            if !dist.contains_key(&next) && passable(g, next) {
                dist.insert(next, d + 1);
                queue.push_back(next);
            }
        }
    }
    best.map(|(_, at)| at)
}

/// Log this frame of a leg: the hero's cell, her model's place, its clip
/// and the clip's speed; true once she has stood on `goal` for a while.
fn log_leg(g: &RenethackGame, leg: &str, goal: (i32, i32)) -> Result<bool, String> {
    let map = map_view(g)?;
    let cell = g.world.map.hero();
    let at = map.hero_stands().map(|(p, _)| p);
    let (clip, speed) = map
        .hero_model()
        .and_then(|m| m.player())
        .filter(|p| p.is_instance_valid())
        .map(|p| (p.get_current_animation().to_string(), p.get_playing_speed()))
        .unwrap_or_default();
    let walking = map.hero_walking();
    eprintln!(
        "selftest: stride: {leg} frame {} cell {:?} at {} clip {clip} speed {speed:.2} walking {walking}",
        frames(),
        cell,
        at.map_or("-".into(), |p| format!("{:.3},{:.3}", p.x, p.z)),
    );
    let n = LEG_FRAMES.fetch_add(1, Ordering::Relaxed);
    if n > 900 {
        return Err(format!("{leg}: she never got to {goal:?} (at {cell:?})"));
    }
    let done = !g.world.getpos && !walking && (cell == Some(goal) || n > 300);
    if done {
        return Ok(STILL.fetch_add(1, Ordering::Relaxed) >= 20);
    }
    STILL.store(0, Ordering::Relaxed);
    Ok(false)
}

pub(super) fn stride() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 11", |g| {
        g.debug_mode = true;
        g.seed = Some(11);
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        // the level mapped: a far cell to travel to, in the dark too
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", command),
        Step::Call("7 m off, as in play", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(7.0, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Call("the travel's goal", |g| {
            let from = g.world.map.hero().ok_or("no hero")?;
            let goal = far_floor(g, 18).ok_or("no far floor")?;
            *FROM.lock().unwrap() = from;
            *GOAL.lock().unwrap() = goal;
            eprintln!(
                "selftest: stride: from {from:?} to {goal:?} at frame {}",
                frames()
            );
            Ok(())
        }),
        key('_'),
        Step::Request("where to travel", command),
        Step::Call("the goal picked", |g| {
            let (x, y) = *GOAL.lock().unwrap();
            LEG_FRAMES.store(0, Ordering::Relaxed);
            STILL.store(0, Ordering::Relaxed);
            eprintln!("selftest: stride: travel picked at frame {}", frames());
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            Ok(())
        }),
        Step::Wait("the travel, frame by frame", |g| {
            log_leg(g, "travel", *GOAL.lock().unwrap())
        }),
        Step::Call("a click back where she came from", |g| {
            // (beside the stairs: a click on them climbs)
            let from = *FROM.lock().unwrap();
            let (x, y) = super::audio_tests::STEPS
                .iter()
                .map(|((dx, dy), _)| (from.0 + dx, from.1 + dy))
                .find(|&at| open_floor(g, at))
                .unwrap_or(from);
            *FROM.lock().unwrap() = (x, y);
            LEG_FRAMES.store(0, Ordering::Relaxed);
            STILL.store(0, Ordering::Relaxed);
            eprintln!("selftest: stride: click back at frame {}", frames());
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            Ok(())
        }),
        Step::Wait("the walk back, frame by frame", |g| {
            log_leg(g, "click", *FROM.lock().unwrap())
        }),
    ]);
    steps.extend(quit());
    steps
}
