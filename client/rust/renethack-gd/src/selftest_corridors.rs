//! `corridors`: a stretch of level laid out in place of the game's (two
//! rooms and corridors of every shape between them, as the gallery lays
//! out its pages), the hero stood in each kind of corridor and doorway
//! with small monsters next to them and off to a side. From the game's
//! camera, at its default distance and as close as a player may zoom,
//! nothing may stand between the eye and the feet of any of them: no rock
//! in front of a corridor, no wall below a doorway. With `--screenshots`,
//! a picture of each. RENETHACK_CORRIDORS_IN=mines (sokoban, gehennom,
//! vlad, ludios, quest, main): the same as a level of that branch, for
//! the pictures of its look (the checks hold in any); with ",unlit" after
//! it (mines,unlit), a level with no light but the hero's.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::{Step, camera_settled, map_view, quit, start};
use godot::prelude::*;
use nh_protocol::LevelNotice;

use crate::gallery::{CORRIDOR_SCENES, corridor_cell, lay_out_corridors};
use crate::game::RenethackGame;

/// The scene of `CORRIDOR_SCENES` now laid out (steps are plain functions).
static SCENE: AtomicUsize = AtomicUsize::new(0);
/// The camera's distance in a game (bits of an f32).
static DISTANCE: AtomicU32 = AtomicU32::new(0);

/// The dungeon RENETHACK_CORRIDORS_IN names, as a level notice names it,
/// and whether the level is to be unlit.
fn other_level() -> Result<Option<(&'static str, bool)>, String> {
    let Ok(name) = std::env::var("RENETHACK_CORRIDORS_IN") else {
        return Ok(None);
    };
    let (name, unlit) = match name.strip_suffix(",unlit") {
        Some(name) => (name, true),
        None => (name.as_str(), false),
    };
    let dungeon = match name {
        "main" => "The Dungeons of Doom",
        "mines" => "The Gnomish Mines",
        "sokoban" => "Sokoban",
        "gehennom" => "Gehennom",
        "vlad" => "Vlad's Tower",
        "ludios" => "Fort Ludios",
        "quest" => "The Quest",
        other => return Err(format!("RENETHACK_CORRIDORS_IN: no branch {other:?}")),
    };
    Ok(Some((dungeon, unlit)))
}

fn lay_out(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let other = other_level()?;
    let unlit = other.is_some_and(|(_, unlit)| unlit);
    lay_out_corridors(&mut g.world, &cat, SCENE.load(Ordering::Relaxed), unlit)?;
    if let Some((dungeon, _)) = other {
        g.world.level = Some(LevelNotice {
            dungeon: dungeon.to_string(),
            depth: 3,
            plane: None,
        });
    }
    let ui = g.ui.as_mut().ok_or("no UI")?;
    ui.map.set_showcase(false);
    ui.map
        .set_distance(f32::from_bits(DISTANCE.load(Ordering::Relaxed)), 0.0);
    Ok(())
}

/// The hero and everyone else of the scene stand in view.
fn all_in_view(g: &RenethackGame) -> Result<(), String> {
    let scene = &CORRIDOR_SCENES[SCENE.load(Ordering::Relaxed)];
    let map = map_view(g)?;
    let distance = map.camera_distance();
    let everyone = std::iter::once(("the hero", scene.hero)).chain(scene.others.iter().copied());
    for (who, at) in everyone {
        if !map.feet_in_view(corridor_cell(at)) {
            return Err(format!(
                "{}: {who} at {at:?} stands behind rock or a wall, the camera {distance} m away",
                scene.name
            ));
        }
    }
    godot_print!(
        "selftest: corridors: {}: everyone in view from {distance} m",
        scene.name
    );
    Ok(())
}

pub(super) fn corridors() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.push(Step::Call("the camera's distance in a game", |g| {
        let d = map_view(g)?.camera_distance();
        DISTANCE.store(d.to_bits(), Ordering::Relaxed);
        Ok(())
    }));
    for scene in &CORRIDOR_SCENES {
        let far: &'static str = Box::leak(format!("corridor-{}", scene.name).into_boxed_str());
        let close: &'static str =
            Box::leak(format!("corridor-{}-close", scene.name).into_boxed_str());
        steps.extend([
            Step::Call("lay out the corridors", lay_out),
            Step::Wait("the corridors drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on the hero", camera_settled),
            Step::Call("everyone in view", |g| all_in_view(g)),
            Step::Shot(far),
            // as close as a player may zoom: the camera looks more from
            // the side, and more could stand in the way
            Step::Call("closer", |g| {
                g.ui.as_mut().ok_or("no UI")?.map.zoom(-10.0);
                Ok(())
            }),
            Step::Wait("the camera closer", camera_settled),
            Step::Call("everyone in view from close", |g| all_in_view(g)),
            Step::Shot(close),
            Step::Call("the next scene", |_| {
                SCENE.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(quit());
    steps
}
