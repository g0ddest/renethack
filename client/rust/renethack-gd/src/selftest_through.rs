//! `through`: the hero is seen through a creature that stands between them
//! and the camera, as through a wall. The camera looks from the south: a
//! giant mummy in the cell south of the hero hid them whole. Now their
//! outline shows through it (the x-ray overlay walls already gave them);
//! a newt there, or the giant north of them, hides nothing and gets none.
//! And the other way: a newt north of the hero, behind their back, shows
//! through them in a hostile's red; the giant there, whose head shows over
//! theirs, does not.
//!
//! The creature is laid out on the client's map (the engine is not asked),
//! on the gallery's lit floor. Headless, the overlay's state is checked;
//! with `--screenshots`, in a window, a picture of each.

use std::sync::atomic::{AtomicU32, Ordering};

use godot::prelude::*;

use super::{Step, camera_settled, map_view, quit, start, zoom_by};
use crate::game::RenethackGame;

static FRAMES: AtomicU32 = AtomicU32::new(0);

/// Lay `name` out alone with the hero, then move it `dy` cells from the
/// hero (south: towards the camera).
fn beside_hero(g: &mut RenethackGame, name: &str, dy: i32) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let (x, y) = crate::gallery::lay_out_one(&mut g.world, &cat, name)?;
    let hero = g.world.map.hero().ok_or("no hero laid out")?;
    let glyph = |x: i32, y: i32| g.world.map.cell(x, y).and_then(|c| c.glyph.clone());
    let creature = glyph(x, y).ok_or("nothing laid out")?;
    let floor = glyph(x - 1, y).ok_or("no floor laid out")?;
    g.world.map.print(x, y, &floor, None);
    g.world.map.print(hero.0, hero.1 + dy, &creature, None);
    g.world.view_center = Some(hero);
    Ok(())
}

/// The creature drawn where it was put, the hero's model with it, the
/// camera there, and a few frames for the overlay to follow.
fn drawn() -> Step {
    Step::Wait("the creature and the hero drawn, the camera there", |g| {
        let map = map_view(g)?;
        let hero = g.world.map.hero().ok_or("no hero")?;
        let ready = map.drawn_generation() == Some(g.world.map.generation())
            && map.hero_model().is_some()
            && (map.entity_rim(hero.0, hero.1 + 1).is_some()
                || map.entity_rim(hero.0, hero.1 - 1).is_some())
            && camera_settled(g)?;
        if !ready {
            FRAMES.store(0, Ordering::Relaxed);
            return Ok(false);
        }
        Ok(FRAMES.fetch_add(1, Ordering::Relaxed) + 1 >= 10)
    })
}

/// The creature north of the hero is drawn through them (`want`, a
/// hostile's colour), or not (None).
fn behind_hero(g: &RenethackGame, want: Option<bool>, what: &str) -> Result<(), String> {
    let hero = g.world.map.hero().ok_or("no hero")?;
    let now = map_view(g)?.seen_through_hero(hero.0, hero.1 - 1);
    godot_print!("selftest: through: {what}: drawn through the hero (hostile): {now:?}");
    if now != want {
        return Err(format!(
            "{what}: drawn through the hero {now:?}, not {want:?} (Some: in a hostile's colour)"
        ));
    }
    Ok(())
}

fn seen_through(g: &RenethackGame, want: bool, what: &str) -> Result<(), String> {
    let now = map_view(g)?.hero_seen_through();
    godot_print!("selftest: through: {what}: the hero drawn through it: {now}");
    if now != want {
        return Err(format!(
            "{what}: the hero is {}drawn through what stands before them",
            if now { "" } else { "not " }
        ));
    }
    Ok(())
}

pub(super) fn through() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("a giant mummy in the cell south of the hero", |g| {
            beside_hero(g, "giant mummy", 1)
        }),
        drawn(),
        Step::Shot("through-giant"),
        Step::Call("the hero is drawn through the giant", |g| {
            seen_through(g, true, "a giant mummy south of the hero")
        }),
        Step::Call("closer", |g| zoom_by(g, -3.0)),
        Step::Wait("the camera there", camera_settled),
        Step::Shot("through-giant-close"),
        Step::Call("still drawn through it, close", |g| {
            seen_through(g, true, "close up")
        }),
        Step::Call("back", |g| zoom_by(g, 3.0)),
        Step::Call("a newt there instead", |g| beside_hero(g, "newt", 1)),
        drawn(),
        Step::Shot("through-newt"),
        Step::Call("a newt hides nothing", |g| {
            seen_through(g, false, "a newt south of the hero")
        }),
        Step::Call("the giant north of the hero", |g| {
            beside_hero(g, "giant mummy", -1)
        }),
        drawn(),
        Step::Shot("through-behind"),
        Step::Call("the giant behind the hero hides nothing", |g| {
            seen_through(g, false, "a giant mummy north of the hero")
        }),
        Step::Call("its head shows over theirs: not drawn through them", |g| {
            behind_hero(g, None, "a giant mummy north of the hero")
        }),
        Step::Call("a newt behind the hero", |g| beside_hero(g, "newt", -1)),
        drawn(),
        Step::Shot("through-newt-behind"),
        Step::Call("the newt shows through the hero, a hostile", |g| {
            behind_hero(g, Some(true), "a newt north of the hero")
        }),
    ]);
    steps.extend(quit());
    steps
}
