//! `doorways`: whoever stands in a doorway stands on its floor. A room
//! laid out in place of the game's (as the gallery lays out its pages),
//! one body after another in each of its doors: a humanoid (a gnome
//! leader), a big one (an ogre), a rigged quadruped (a jackal), one built
//! in code (a lizard) and a small one (a newt). The doors are in a wall
//! along a row and in one along a column, each seen shut (whoever stands
//! there opened it: the engine draws them with no ground under them, and
//! the map still remembers the door shut), open, and doorless (or
//! broken); the hero stands in a door too. Each one's feet are where the
//! same body's are on the room's floor, within a centimetre, and in the
//! middle of the opening: standing, and back on the very spot after
//! lunging at a foe inside the room and after reeling from the foe's blow.
//! Mid-reel it is knocked back through the opening, never across it into
//! the frame. With `--screenshots`, a picture of each body's room at
//! each.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Instant;

use godot::prelude::*;

use super::{Step, camera_settled, map_view, quit, start};
use crate::animator::REEL;
use crate::gallery::{
    DOORWAY_DOORS, DOORWAY_FLOOR, DOORWAY_FOES, DOORWAY_HERO, DOORWAY_HERO_FOE, DoorSeen,
    door_seen, doorway_cell, lay_out_doorways,
};
use crate::game::RenethackGame;

/// The bodies, a room each; the hero's door is seen as each of
/// `DoorSeen::ALL` in turn.
const BODIES: [&str; 5] = ["gnome leader", "ogre", "jackal", "lizard", "newt"];
static BODY: AtomicUsize = AtomicUsize::new(0);

/// How far feet may lie from the floor's height, or from the middle of
/// the opening (metres).
const TOLERANCE: f32 = 0.01;
/// The camera far enough for the whole room.
const DISTANCE: f32 = 11.0;
/// Where the blows stop for the checks and the pictures: half-way, the
/// lunge at its farthest.
const HALF_WAY: f32 = 0.5;
/// The blows told: one for each door and the hero's.
const BLOWS: u32 = DOORWAY_DOORS.len() as u32 + 1;
/// The strikes counted before a round's blows; when its motions were let
/// go (their clips played out a while after).
static STRIKES: AtomicU32 = AtomicU32::new(0);
static LET_GO: Mutex<Option<Instant>> = Mutex::new(None);
const CLIPS_SECS: f32 = 1.0;

fn body() -> &'static str {
    BODIES[BODY.load(Ordering::Relaxed) % BODIES.len()]
}

fn hero_door() -> DoorSeen {
    door_seen(BODY.load(Ordering::Relaxed))
}

fn lay_out(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    lay_out_doorways(&mut g.world, &cat, body(), hero_door())?;
    let ui = g.ui.as_mut().ok_or("no UI")?;
    // the gallery's light: the feet in the doorways plain to see
    ui.map.set_showcase(true);
    ui.map.set_distance(DISTANCE, 0.0);
    Ok(())
}

/// A round of blows told as the engine tells them.
#[derive(Clone, Copy)]
enum Round {
    /// Those in the doors strike their foes, the hero hers.
    Blows,
    /// The foes strike back: those in the doors reel, the hero too.
    Hits,
}

/// The round's blows told; the motions they start stop half-way.
fn fight(g: &mut RenethackGame, round: Round) -> Result<(), String> {
    let map = &mut g.ui.as_mut().ok_or("no UI")?.map;
    map.hold_motions(Some(HALF_WAY));
    STRIKES.store(map.motion_stats().strikes, Ordering::Relaxed);
    let body = body();
    let (rat, _) = DOORWAY_HERO_FOE;
    let mut lines: Vec<String> = DOORWAY_FOES
        .iter()
        .map(|(foe, _)| match round {
            Round::Blows => format!("The {body} hits the {foe}."),
            Round::Hits => format!("The {foe} hits the {body}."),
        })
        .collect();
    lines.push(match round {
        Round::Blows => format!("You hit the {rat}."),
        Round::Hits => format!("The {rat} bites!"),
    });
    let turn = g.world.turn();
    for line in lines {
        g.world.log.push(line, 0, turn, false);
    }
    Ok(())
}

/// Every blow of the round struck and stopped half-way.
fn fights_held(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    let struck = map.motion_stats().strikes >= STRIKES.load(Ordering::Relaxed) + BLOWS;
    Ok(struck && map.motions_held())
}

fn let_go(g: &mut RenethackGame) -> Result<(), String> {
    g.ui.as_mut().ok_or("no UI")?.map.hold_motions(None);
    *LET_GO.lock().map_err(|e| e.to_string())? = Some(Instant::now());
    Ok(())
}

/// The round's motions ended, and its clips played out.
fn fights_over(g: &RenethackGame) -> Result<bool, String> {
    let since = LET_GO.lock().map_err(|e| e.to_string())?;
    let played = since.is_some_and(|t| t.elapsed().as_secs_f32() >= CLIPS_SECS);
    Ok(played && map_view(g)?.motions_held())
}

/// What those in the doors are doing as their feet are looked at.
#[derive(Clone, Copy, PartialEq)]
enum Stance {
    /// In the middle of the opening: before a blow, and after one, back
    /// on the very spot.
    Standing,
    /// Lunging at a foe inside the room: off the middle.
    Lunging,
    /// Knocked back by the foe's blow: through the opening, away from the
    /// foe and no farther than a reel goes, never across it into the
    /// frame.
    Reeling,
}

/// Everyone in a door has their feet where the same body has them on the
/// room's floor (the hero where it has), and stands as `stance` has it.
fn on_the_floor(g: &RenethackGame, when: &str, stance: Stance) -> Result<(), String> {
    let map = map_view(g)?;
    let body = body();
    let (fx, fy) = doorway_cell(DOORWAY_FLOOR);
    let floor = map
        .entity_stands(fx, fy)
        .ok_or_else(|| format!("no {body} on the room's floor"))?
        .0
        .y;
    // who, where, the way to their foe (the opening's passage), feet
    let mut everyone = Vec::new();
    for (door, (&at, (_, foe))) in DOORWAY_DOORS.iter().zip(DOORWAY_FOES).enumerate() {
        let (x, y) = doorway_cell(at);
        let (p, _) = map
            .entity_stands(x, y)
            .ok_or_else(|| format!("no {body} in the door at {at:?}"))?;
        let who = format!("the {body} in the {:?} door at {at:?}", door_seen(door));
        everyone.push((who, (x, y), toward(at, foe), p));
    }
    let (p, _) = map.hero_stands().ok_or("the hero is not drawn")?;
    let who = format!("the hero in the {:?} door", hero_door());
    let foe = toward(DOORWAY_HERO, DOORWAY_HERO_FOE.1);
    everyone.push((who, doorway_cell(DOORWAY_HERO), foe, p));
    for (who, (x, y), foe, p) in everyone {
        let up = p.y - floor;
        let off = Vector2::new(p.x - x as f32, p.z - y as f32);
        let back = -off.dot(foe);
        let across = (off + foe * back).length();
        godot_print!(
            "selftest: doorways: {when}: {who}: feet {up:+.3} m over the floor, {:.3} m off the middle ({back:+.3} back, {across:.3} across)",
            off.length()
        );
        if up.abs() > TOLERANCE {
            return Err(format!(
                "{when}: {who} stands {up:+.3} m over the room's floor"
            ));
        }
        match stance {
            Stance::Standing if off.length() > TOLERANCE => {
                return Err(format!(
                    "{when}: {who} stands {:.3} m off the middle of the opening",
                    off.length()
                ));
            }
            Stance::Reeling if across > TOLERANCE => {
                return Err(format!(
                    "{when}: {who} is knocked {across:.3} m across the opening, into its frame"
                ));
            }
            Stance::Reeling if back <= TOLERANCE || back > REEL + TOLERANCE => {
                return Err(format!(
                    "{when}: {who} is knocked back {back:+.3} m through the opening (a reel goes {REEL} m)"
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// The flat unit vector from a cell of the room to its neighbour `to`.
fn toward(from: (i32, i32), to: (i32, i32)) -> Vector2 {
    Vector2::new((to.0 - from.0) as f32, (to.1 - from.1) as f32).normalized()
}

pub(super) fn doorways() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    for body in BODIES {
        let shot = |what: &str| -> &'static str {
            Box::leak(format!("doorways-{}{what}", body.replace(' ', "-")).into_boxed_str())
        };
        steps.extend([
            Step::Call("lay out the room", lay_out),
            Step::Wait("the room drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera over it", camera_settled),
            Step::Shot(shot("")),
            Step::Call("everyone in a door on its floor", |g| {
                on_the_floor(g, "standing", Stance::Standing)
            }),
            Step::Call("blows at the foes", |g| fight(g, Round::Blows)),
            Step::Wait("the blows half-way", fights_held),
            Step::Shot(shot("-blows")),
            Step::Call("on the floor mid-blow", |g| {
                on_the_floor(g, "mid-blow", Stance::Lunging)
            }),
            Step::Call("the blows go on", let_go),
            Step::Wait("the blows over", fights_over),
            Step::Call("back on the spot after the blows", |g| {
                on_the_floor(g, "after the blows", Stance::Standing)
            }),
            Step::Call("the foes strike back", |g| fight(g, Round::Hits)),
            Step::Wait("their blows half-way", fights_held),
            Step::Shot(shot("-hits")),
            Step::Call("on the floor reeling", |g| {
                on_the_floor(g, "reeling", Stance::Reeling)
            }),
            Step::Call("their blows go on", let_go),
            Step::Wait("their blows over", fights_over),
            Step::Call("back on the spot after the reel", |g| {
                on_the_floor(g, "after the reel", Stance::Standing)
            }),
            Step::Shot(shot("-back")),
            Step::Call("the next body", |_| {
                BODY.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(quit());
    steps
}
