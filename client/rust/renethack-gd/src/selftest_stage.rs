//! Picture scenarios for motion on video (the movie writer records them;
//! the log says which frames to look at):
//!
//! - `bites`: a debug-mode Valkyrie (her pet slain first) at 7 m; a soldier
//!   ant, a sewer rat, a newt, a black pudding, a giant bat, a baby gray
//!   dragon and a raven are made beside her one at a time, each attacks
//!   her for a few turns (`selftest: bites: <foe> strikes at frame N`,
//!   where it and she stand in the window), then is slain by #wizkill.
//!   RENETHACK_BITES_DISTANCE: the camera's distance (11 m);
//!   RENETHACK_BITES_FOES=giant ant,fire ant: those foes instead;
//!   RENETHACK_BITES_WALK=4: she first steps that many cells away from
//!   each, so it is seen walking after her (`... walks at frame N`).
//! - `stride`: a debug-mode Valkyrie and her kitten, the level mapped; a
//!   travel (`_`) to the farthest floor eight steps off, round a corner
//!   (RENETHACK_STRIDE_STEPS: more), and a click back, every frame logged
//!   (`selftest: stride: frame N ...`: the hero's cell, where her model
//!   stands, its clip, the clip's speed and length), so a run's metres a
//!   second and strides a metre can be read off. RENETHACK_STRIDE_DISTANCE:
//!   the camera's distance; RENETHACK_STRIDE_KNIGHT=1: a Knight on his pony.
//!
//! - `catch-up` (in a window: headless, nothing is held): a debug-mode
//!   Valkyrie (her pet slain) walks to the farthest
//!   floor by the open ways (RENETHACK_CATCH_UP_LONG=1: some seventy steps
//!   off, opening the doors and fighting on the way, to time a travel
//!   across the level). Then she
//!   travels home, let run (`selftest: catch-up: a travel of ... m took
//!   ...`): the game's next question, and any word that came with her last
//!   leg, is never up while she has more than a scurry still to run. Then
//!   there again, and mid-run
//!   (her travel drawn at a run, the engine's later drawings held) a key
//!   (`:`, look here) cuts in: within 40 frames she stands on the engine's
//!   cell, nothing held, the camera's aim never jumping, and the key's
//!   answer comes after, on that picture. Then home again, cut into by a
//!   click on a cell near home: she dashes home and walks to that cell.
//!
//! Nothing is checked but that the scenes were played (but in `catch-up`).

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
/// The foes of this run: `FOES`, or RENETHACK_BITES_FOES's.
static CAST: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn cast() -> &'static [String] {
    CAST.get_or_init(|| match std::env::var("RENETHACK_BITES_FOES") {
        Ok(list) => list.split(',').map(|f| f.trim().to_string()).collect(),
        Err(_) => FOES.iter().map(|f| f.to_string()).collect(),
    })
}

/// The foe beside her now.
fn foe_now() -> &'static str {
    &cast()[FOE.load(Ordering::Relaxed).min(cast().len() - 1)]
}

/// Steps she has still to take away from the foe.
static AWAY: AtomicU32 = AtomicU32::new(0);

/// She steps away from the foe over open floor, a cell a turn, and it
/// comes after her (RENETHACK_BITES_WALK cells).
fn walk_away(cells: u32) -> Vec<Step> {
    static CELLS: AtomicU32 = AtomicU32::new(0);
    CELLS.store(cells, Ordering::Relaxed);
    vec![
        Step::Call("", |g| {
            AWAY.store(CELLS.load(Ordering::Relaxed), Ordering::Relaxed);
            *TARGET.lock().unwrap() = g.world.map.hero().unwrap_or((0, 0));
            SINCE.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("her steps away from the foe", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return to_command(g);
            }
            log_strikes(g, foe_now())?;
            let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
            // (asked every frame: a step is counted when she has moved)
            let mut last = TARGET.lock().unwrap();
            if *last != (hx, hy) {
                *last = (hx, hy);
                AWAY.store(
                    AWAY.load(Ordering::Relaxed).saturating_sub(1),
                    Ordering::Relaxed,
                );
                eprintln!("selftest: bites: {} walks at frame {}", foe_now(), frames());
            }
            if AWAY.load(Ordering::Relaxed) == 0 || frames() > SINCE.load(Ordering::Relaxed) + 900 {
                return Ok(None);
            }
            let (fx, fy) = foe_near(g, 8).unwrap_or((hx, hy));
            // the open cell beside her farthest from the foe
            let way = super::audio_tests::STEPS
                .iter()
                .filter(|((dx, dy), _)| open_floor(g, (hx + dx, hy + dy)))
                .max_by_key(|((dx, dy), _)| {
                    let (x, y) = (hx + dx - fx, hy + dy - fy);
                    // (a straight step before a diagonal one of the same reach)
                    (x.abs().max(y.abs()) * 4) + (x.abs() + y.abs())
                });
            Ok(way.map(|(_, k)| KeyInput::plain(Key::Char(*k))))
        }),
    ]
}

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
    log_strikes(g, foe_now())?;
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
            log_strikes(g, foe_now())?;
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
    let walk: u32 = std::env::var("RENETHACK_BITES_WALK")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);
    for foe in cast() {
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
                let foe = foe_now();
                eprintln!("selftest: bites: {foe} made at frame {}", frames());
                Ok(())
            }),
        ]);
        if walk > 0 {
            steps.extend(walk_away(walk));
        }
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

/// `passable` takes a shut door too (`catch-up` opens them on its way).
static THROUGH_DOORS: AtomicBool = AtomicBool::new(false);

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
        ) || (THROUGH_DOORS.load(Ordering::Relaxed)
            && nh_world::cell_terrain(c, cat) == Some(T::ClosedDoor))
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
/// `most` steps, one round a corner first (not straight ahead nor on a
/// diagonal): a travel long enough to take more than one of the engine's
/// drawings, turning on its way.
fn far_floor(g: &RenethackGame, most: i32) -> Option<(i32, i32)> {
    let from = g.world.map.hero()?;
    let mut dist = std::collections::HashMap::from([(from, 0)]);
    let mut queue = std::collections::VecDeque::from([from]);
    let mut best = None;
    let corner = |(x, y): (i32, i32)| {
        let (dx, dy) = ((x - from.0).abs(), (y - from.1).abs());
        dx != 0 && dy != 0 && dx != dy
    };
    while let Some(at) = queue.pop_front() {
        let d = dist[&at];
        let score = (corner(at), d);
        if open_floor(g, at) && best.is_none_or(|(b, _)| score > b) {
            best = Some((score, at));
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
    let (clip, speed, length) = map
        .hero_model()
        .and_then(|m| m.player())
        .filter(|p| p.is_instance_valid())
        .map(|p| {
            (
                p.get_current_animation().to_string(),
                p.get_playing_speed(),
                p.get_current_animation_length(),
            )
        })
        .unwrap_or_default();
    let walking = map.hero_walking();
    eprintln!(
        "selftest: stride: {leg} frame {} cell {:?} at {} clip {clip} speed {speed:.2} length {length:.3} walking {walking}",
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

/// `#ride` onto the pet beside him, the mount forced (debug mode).
fn ride() -> Vec<Step> {
    vec![
        Step::Wait("the pony beside him", |g| Ok(pet_way(g).is_some())),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("ride".into())),
        Step::Request("the way to the steed", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        Step::KeyFrom("towards the pony", |g| {
            Ok(KeyInput::plain(Key::Char(
                pet_way(g).ok_or("no pet beside him")?,
            )))
        }),
        Step::Request(
            "Force the mount?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Force")),
        ),
        key('y'),
        Step::AnswerUntil('n', "a command after mounting", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && !g.world.getpos)
        }),
        Step::Wait("in the saddle", |g| Ok(map_view(g)?.steed().is_some())),
    ]
}

/// The way to the pet beside the hero (a vi-key).
fn pet_way(g: &RenethackGame) -> Option<char> {
    let (hx, hy) = g.world.map.hero()?;
    super::audio_tests::STEPS.iter().find_map(|&((dx, dy), k)| {
        let gl = g.world.map.cell(hx + dx, hy + dy)?.glyph.as_ref()?;
        (gl.kind == GlyphKind::Mon && gl.flags & mg::PET != 0).then_some(k)
    })
}

pub(super) fn stride() -> Vec<Step> {
    // RENETHACK_STRIDE_KNIGHT=1: a Knight travels on his pony
    let knight = std::env::var_os("RENETHACK_STRIDE_KNIGHT").is_some();
    let mut steps = vec![Step::Call("debug mode, seed 11", |g| {
        g.debug_mode = true;
        g.seed = Some(11);
        Ok(())
    })];
    steps.extend(if knight {
        super::start_as(crate::ui_events::CharacterChoice {
            name: "wizard".into(),
            role: "knight".into(),
            gender: "male".into(),
            align: "lawful".into(),
            ..super::smoke_choice()
        })
    } else {
        start()
    });
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        // the level mapped: a far cell to travel to, in the dark too
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", command),
        Step::Call("7 m off, as in play (or the distance asked)", |g| {
            let d = std::env::var("RENETHACK_STRIDE_DISTANCE")
                .ok()
                .and_then(|d| d.parse().ok())
                .unwrap_or(7.0);
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(d, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    if knight {
        steps.extend(ride());
    }
    steps.extend([
        Step::Call("the travel's goal", |g| {
            let from = g.world.map.hero().ok_or("no hero")?;
            let most = std::env::var("RENETHACK_STRIDE_STEPS")
                .ok()
                .and_then(|d| d.parse().ok())
                .unwrap_or(8);
            let goal = far_floor(g, most).ok_or("no far floor")?;
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

static RUN_FROM: AtomicI64 = AtomicI64::new(0);
static CUT_AT: AtomicI64 = AtomicI64::new(0);
static LOG_MARK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CUT_MARK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static PREP_REQ: AtomicI64 = AtomicI64::new(-1);
static PREP_TRIES: AtomicU32 = AtomicU32::new(0);
static HOME: Mutex<(i32, i32)> = Mutex::new((0, 0));
static NEAR: Mutex<(i32, i32)> = Mutex::new((0, 0));
/// A leg's start, the way run so far (m, and where she was last), the top
/// speed (m/s) and the most a frame moved the camera's aim (m).
#[derive(Clone, Copy)]
struct Leg {
    began: Option<std::time::Instant>,
    last: Option<Vector3>,
    way: f32,
    top: f32,
    aim: Option<Vector3>,
    jolt: f32,
    /// How far from the engine's cell she was drawn when cut into (m).
    to_go: f32,
}
static LEG: Mutex<Leg> = Mutex::new(Leg {
    began: None,
    last: None,
    way: 0.0,
    top: 0.0,
    aim: None,
    jolt: 0.0,
    to_go: 0.0,
});

/// Where the hero's model stands is the engine's cell (to a few cm).
fn drawn_where_she_is(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    let (Some((x, y)), Some((p, _))) = (g.world.map.hero(), map.hero_stands()) else {
        return Ok(false);
    };
    Ok((p.x - x as f32).abs() < 0.05 && (p.z - y as f32).abs() < 0.05)
}

/// The way to `GOAL` travelled once, however often the engine stops her
/// (a door to open, a newt in view): `_` and a click on the goal at each
/// command prompt till she stands there.
fn open_the_way() -> Step {
    Step::Wait("the long way walked once, its doors opened", |g| {
        let Some((id, Prompt::Command)) = &g.pending else {
            return Ok(false);
        };
        let id = *id as i64;
        if PREP_REQ.load(Ordering::Relaxed) == id
            || map_view(g)?.hero_running()
            || !drawn_where_she_is(g)?
        {
            return Ok(false);
        }
        let hero = g.world.map.hero().ok_or("no hero")?;
        let (x, y) = *GOAL.lock().unwrap();
        if g.world.getpos {
            PREP_REQ.store(id, Ordering::Relaxed);
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            return Ok(false);
        }
        if hero == (x, y) {
            return Ok(true);
        }
        if PREP_TRIES.fetch_add(1, Ordering::Relaxed) > 120 {
            return Err(format!("she never got there: at {hero:?}"));
        }
        PREP_REQ.store(id, Ordering::Relaxed);
        // a travel does not open a door: a step into the shut one beside her
        // does (and again while it sticks)
        let cat = g.catalog.as_deref().ok_or("no catalog")?;
        let door = super::audio_tests::STEPS[..4].iter().find(|((dx, dy), _)| {
            g.world.map.cell(hero.0 + dx, hero.1 + dy).is_some_and(|c| {
                nh_world::cell_terrain(c, cat) == Some(nh_world::Terrain::ClosedDoor)
            })
        });
        // and a blow at the foe beside her
        let foe = foe_near(g, 1).and_then(|(fx, fy)| {
            super::audio_tests::STEPS
                .iter()
                .find(|((dx, dy), _)| (hero.0 + dx, hero.1 + dy) == (fx, fy))
        });
        let k = foe.or(door).map_or('_', |(_, k)| *k);
        g.push_ui(UiEvent::Key(KeyInput::plain(Key::Char(k))));
        Ok(false)
    })
}

/// A frame of a travel watched: its way and speed summed, and never a
/// message of the engine's up (one since the travel began) while she has
/// more than a scurry still to run once the engine is done with the travel
/// (a word of the dungeon's on her way is up as she runs on, as it came).
fn watch_leg(g: &RenethackGame) -> Result<(), String> {
    let map = map_view(g)?;
    let mut leg = LEG.lock().unwrap();
    if let Some((p, _)) = map.hero_stands() {
        if let Some(q) = leg.last {
            leg.way += Vector2::new(p.x - q.x, p.z - q.z).length();
        }
        leg.last = Some(p);
    }
    let aim = map.camera_aim();
    if let Some(a) = leg.aim {
        leg.jolt = leg.jolt.max(a.distance_to(aim));
    }
    leg.aim = Some(aim);
    let run = map.hero_run();
    if let Some((_, speed)) = run
        && !map.catching_up()
    {
        leg.top = leg.top.max(speed);
    }
    // a message that came this frame, the engine done with the travel
    let seen = SEEN.swap(g.world.log.last_seq(), Ordering::Relaxed);
    let over = g.pending.is_some() || g.world.map.hero() == Some(*GOAL.lock().unwrap());
    if let Some((left, _)) = run.filter(|(left, _)| *left > 0.45 && g.pending.is_some())
        && !map.catching_up()
        && !g.world.getpos
        && frames() - CUT_AT.load(Ordering::Relaxed) > 60
    {
        return Err(format!(
            "the game asks for the next command and she has {left:.2} s still to run"
        ));
    }
    if over
        && frames() - CUT_AT.load(Ordering::Relaxed) > 60
        && let Some((left, _)) = run.filter(|(left, _)| *left > 0.45)
        && let Some(m) = g.world.log.since(seen).find(|m| !m.from_history)
    {
        return Err(format!(
            "\"{}\" is up and she has {left:.2} s still to run",
            m.text
        ));
    }
    Ok(())
}

/// `_` to `GOAL`, picked by a click; her run under way and held.
fn travel() -> Vec<Step> {
    vec![
        key('_'),
        Step::Request("where to travel", command),
        Step::Call("the goal picked", |g| {
            let (x, y) = *GOAL.lock().unwrap();
            RUN_FROM.store(frames(), Ordering::Relaxed);
            LOG_MARK.store(g.world.log.last_seq(), Ordering::Relaxed);
            SEEN.store(g.world.log.last_seq(), Ordering::Relaxed);
            *LEG.lock().unwrap() = Leg {
                began: Some(std::time::Instant::now()),
                last: None,
                way: 0.0,
                top: 0.0,
                aim: None,
                jolt: 0.0,
                to_go: 0.0,
            };
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            Ok(())
        }),
        Step::Wait("her run under way, the engine's drawings held", |g| {
            watch_leg(g)?;
            Ok(map_view(g)?.hero_runs_on() && frames() >= RUN_FROM.load(Ordering::Relaxed) + 30)
        }),
    ]
}

/// The travel let run to its end, timed.
fn arrive(leg: &'static str) -> Vec<Step> {
    vec![
        Step::Wait("there, standing, the game waiting", |g| {
            watch_leg(g)?;
            Ok(!map_view(g)?.hero_running()
                && matches!(g.pending, Some((_, Prompt::Command)))
                && drawn_where_she_is(g)?)
        }),
        Step::Call(leg, |g| {
            let l = *LEG.lock().unwrap();
            let secs = l.began.map_or(0.0, |t| t.elapsed().as_secs_f32());
            eprintln!(
                "selftest: catch-up: a travel of {:.1} m took {secs:.2} s ({:.1} m/s), top speed {:.1} m/s, at {:?}",
                l.way,
                l.way / secs.max(0.01),
                l.top,
                g.world.map.hero()
            );
            let long = std::env::var_os("RENETHACK_CATCH_UP_LONG").is_some();
            if !long && g.world.map.hero() != Some(*GOAL.lock().unwrap()) {
                return Err("the travel ended short of its goal".into());
            }
            Ok(())
        }),
    ]
}

/// A cut is made now: how far she is drawn from where the game has her
/// by the time it has read all of the engine (measured in `caught_up`).
fn note_cut(g: &RenethackGame) -> Result<(), String> {
    let (p, _) = map_view(g)?.hero_stands().ok_or("no hero drawn")?;
    let (x, y) = *GOAL.lock().unwrap();
    CUT_AT.store(frames(), Ordering::Relaxed);
    CUT_MARK.store(g.world.log.last_seq(), Ordering::Relaxed);
    let mut leg = LEG.lock().unwrap();
    leg.jolt = 0.0;
    leg.to_go = Vector2::new(p.x - x as f32, p.z - y as f32).length();
    Ok(())
}

/// After a cut made at `CUT_AT`: within 40 frames she stands on the
/// engine's cell, nothing held; the camera's aim never jumped.
fn caught_up() -> Step {
    Step::Wait("caught up: on the engine's cell, nothing held", |g| {
        watch_leg(g)?;
        let n = frames() - CUT_AT.load(Ordering::Relaxed);
        if drawn_where_she_is(g)? && !map_view(g)?.hero_running() && g.pending.is_some() {
            let Leg { jolt, to_go, .. } = *LEG.lock().unwrap();
            eprintln!(
                "selftest: catch-up: on the engine's cell {n} frames after the cut, {to_go:.1} m dashed; the camera's aim moved {jolt:.2} m a frame at most"
            );
            if jolt > (0.3 * to_go).max(1.5) {
                return Err(format!("the camera jumped {jolt:.2} m in a frame"));
            }
            return Ok(true);
        }
        if n > 40 {
            let map = map_view(g)?;
            return Err(format!(
                "{n} frames after the cut she is still not drawn where the game has her: at {:?}, drawn at {:?}, run {:?}, pending {:?}",
                g.world.map.hero(),
                map.hero_stands().map(|(p, _)| (p.x, p.z)),
                map.hero_run(),
                g.pending.as_ref().map(|(_, p)| p),
            ));
        }
        Ok(false)
    })
}

pub(super) fn catch_up() -> Vec<Step> {
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
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", command),
        Step::Call("7 m off, as in play", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(7.0, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Call("home, a cell near it, and the far floor", |g| {
            let home = g.world.map.hero().ok_or("no hero")?;
            *HOME.lock().unwrap() = home;
            *NEAR.lock().unwrap() = far_floor(g, 3).ok_or("no floor near")?;
            // RENETHACK_CATCH_UP_LONG: across the level, through its doors
            // (a measure: foes on the way make it no test)
            let long = std::env::var_os("RENETHACK_CATCH_UP_LONG").is_some();
            THROUGH_DOORS.store(long, Ordering::Relaxed);
            let goal = far_floor(g, 75);
            THROUGH_DOORS.store(false, Ordering::Relaxed);
            let goal = goal.ok_or("no far floor")?;
            *FROM.lock().unwrap() = goal;
            *GOAL.lock().unwrap() = goal;
            eprintln!("selftest: catch-up: from {home:?} to {goal:?}");
            Ok(())
        }),
        open_the_way(),
    ]);
    // a whole travel, let run: how long it takes; the game's next question
    // (and any word of it) waits for her last leg
    let home = Step::Call("", |_| {
        *GOAL.lock().unwrap() = *HOME.lock().unwrap();
        Ok(())
    });
    let there = Step::Call("", |_| {
        *GOAL.lock().unwrap() = *FROM.lock().unwrap();
        Ok(())
    });
    steps.push(home);
    steps.extend(travel());
    steps.extend(arrive("home"));
    if std::env::var_os("RENETHACK_CATCH_UP_LONG").is_some() {
        // the measure: there and back twice more (a foe may stop one)
        for _ in 0..2 {
            steps.push(Step::Call("", |_| {
                *GOAL.lock().unwrap() = *FROM.lock().unwrap();
                Ok(())
            }));
            steps.extend(travel());
            steps.extend(arrive("there"));
            steps.push(Step::Call("", |_| {
                *GOAL.lock().unwrap() = *HOME.lock().unwrap();
                Ok(())
            }));
            steps.extend(travel());
            steps.extend(arrive("home"));
        }
        steps.extend(quit());
        return steps;
    }
    // there again, cut into mid-run by a key
    steps.push(there);
    steps.extend(travel());
    steps.extend([
        Step::Call("a key mid-run: look here", |g| {
            if drawn_where_she_is(g)? {
                return Err("she stands on the engine's cell before the key".into());
            }
            note_cut(g)?;
            g.push_ui(UiEvent::Key(KeyInput::plain(Key::Char(':'))));
            Ok(())
        }),
        caught_up(),
        Step::Wait("the key's answer, on that picture", |g| {
            let mark = CUT_MARK.load(Ordering::Relaxed);
            let answered = g
                .world
                .log
                .since(mark)
                .any(|m| !m.from_history && m.text.contains("here"));
            if answered && !drawn_where_she_is(g)? {
                return Err("the key was answered with her drawn elsewhere".into());
            }
            if frames() - CUT_AT.load(Ordering::Relaxed) > 240 {
                return Err("the key was never answered (eaten?)".into());
            }
            Ok(answered && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Call("where the travel took her", |g| {
            if g.world.map.hero() != Some(*GOAL.lock().unwrap()) {
                return Err("the key cut the travel itself short".into());
            }
            Ok(())
        }),
    ]);
    // home again, cut into by a click on a cell near home: she dashes home,
    // then walks to that cell (the click not eaten)
    steps.push(Step::Call("", |_| {
        *GOAL.lock().unwrap() = *HOME.lock().unwrap();
        Ok(())
    }));
    steps.extend(travel());
    steps.extend([
        Step::Call("a click mid-run", |g| {
            if drawn_where_she_is(g)? {
                return Err("she stands on the engine's cell before the click".into());
            }
            note_cut(g)?;
            let (x, y) = *NEAR.lock().unwrap();
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            Ok(())
        }),
        caught_up(),
        Step::Wait("she walks to the cell clicked", |g| {
            if frames() - CUT_AT.load(Ordering::Relaxed) > 900 {
                return Err(format!(
                    "the click was eaten: she is at {:?}",
                    g.world.map.hero()
                ));
            }
            Ok(g.world.map.hero() == Some(*NEAR.lock().unwrap()) && drawn_where_she_is(g)?)
        }),
    ]);
    steps.extend(quit());
    steps
}
