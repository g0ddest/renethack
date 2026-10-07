//! `rim`: the layer of the hero's rim light stays on the hero's own model,
//! and a model in view changes layers without upsetting the renderer. The
//! scenario needs a window: a headless run draws nothing, and what goes
//! wrong here shows only in the real renderer, as its "BUG, indexing did
//! not unpair geometries from light" on quit and a crash after it. Both
//! come after the PASS line; the Makefile fails the run for either.
//!
//! First the map is drawn as a swap with the pet looks between two frames
//! (the engine redraws one cell, then the other; a missile flying at that
//! moment puts a frame between them): the pet on the cell the hero left,
//! the hero on none. The hero's last known cell then holds the pet's
//! model, which must not take the hero's layer. Then the pet's model, in
//! view beside the rim light, is put on that layer in the frame it is
//! shown and taken off it again, as the hero's own is when it changes.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use godot::classes::{DisplayServer, Node3D, RenderingServer};
use godot::prelude::*;
use nh_protocol::{Glyph, mg};

use super::{Step, map_view, quit, start};
use crate::game::RenethackGame;
use crate::map_view::MapView;

/// The hero's and the pet's cells as the game drew them.
#[derive(Clone)]
struct Drawn {
    hero: (i32, i32),
    pet: (i32, i32),
    hero_glyph: Glyph,
    pet_glyph: Glyph,
    /// The node of the hero's model then.
    hero_node: InstanceId,
}

static DRAWN: Mutex<Option<Drawn>> = Mutex::new(None);
/// Checks in a row a state has held (a step is checked once a frame).
static HELD: AtomicU32 = AtomicU32::new(0);
/// Frames a state must hold: a model first built comes a few frames after
/// its cell.
const HOLD: u32 = 8;

fn drawn() -> Result<Drawn, String> {
    DRAWN
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "the cells are not remembered yet".to_string())
}

/// True once `now` has been true `HOLD` checks in a row.
fn held(now: bool) -> bool {
    if !now {
        HELD.store(0, Ordering::Relaxed);
        return false;
    }
    if HELD.fetch_add(1, Ordering::Relaxed) + 1 < HOLD {
        return false;
    }
    HELD.store(0, Ordering::Relaxed);
    true
}

/// Frames drawn whatever the window: macOS does not draw one it thinks
/// nobody sees, and the renderer pairs meshes with lights only when it
/// draws (or when something is freed, whenever that is).
fn draw_frames(what: &'static str) -> Step {
    Step::Wait(what, |_| {
        RenderingServer::singleton().force_draw();
        Ok(held(true))
    })
}

/// The model on a cell: its node, and whether it is on the rim layer.
fn model(g: &RenethackGame, at: (i32, i32)) -> Result<Option<(Gd<Node3D>, bool)>, String> {
    Ok(map_view(g)?.entity_rim(at.0, at.1))
}

fn pet_beside(g: &RenethackGame, (hx, hy): (i32, i32)) -> Option<(i32, i32)> {
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (hx + dx, hy + dy)))
        .filter(|&at| at != (hx, hy))
        .find(|&(x, y)| {
            let cell = g.world.map.cell(x, y);
            cell.and_then(|c| c.entity())
                .is_some_and(|e| e.flags & mg::PET != 0)
        })
}

/// Draw a glyph on a cell of the client's map, as a print_glyph of the
/// engine's would (the engine is not asked).
fn draw(g: &mut RenethackGame, (x, y): (i32, i32), glyph: &Glyph) {
    let bk = g.world.map.cell(x, y).and_then(|c| c.bk.clone());
    g.world.map.print(x, y, glyph, bk.as_ref());
}

fn pet_model(g: &RenethackGame) -> Result<Gd<Node3D>, String> {
    let d = drawn()?;
    let (node, _) = model(g, d.pet)?.ok_or("no model on the pet's cell")?;
    Ok(node)
}

pub(super) fn rim() -> Vec<Step> {
    let mut steps = vec![Step::Call("a window to draw in", |_| {
        if DisplayServer::singleton().get_name() == "headless" {
            return Err("the renderer must run: start it without --headless".into());
        }
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Wait("the hero's model on the rim layer, the pet's off it", |g| {
            let Some(hero) = g.world.map.hero() else {
                return Ok(false);
            };
            let Some(pet) = pet_beside(g, hero) else {
                return Err("no pet beside the hero".into());
            };
            let (Some((_, on)), Some((_, pet_on))) = (model(g, hero)?, model(g, pet)?) else {
                return Ok(false);
            };
            if pet_on {
                return Err("the pet's model is on the hero's rim layer".into());
            }
            Ok(held(on))
        }),
        Step::Call("the pet drawn on the hero's cell, the hero on none", |g| {
            let hero = g.world.map.hero().ok_or("the hero is not drawn")?;
            let pet = pet_beside(g, hero).ok_or("no pet beside the hero")?;
            let glyph = |(x, y): (i32, i32)| {
                let cell = g.world.map.cell(x, y);
                cell.and_then(|c| c.glyph.clone()).ok_or("an empty cell")
            };
            let d = Drawn {
                hero,
                pet,
                hero_glyph: glyph(hero)?,
                pet_glyph: glyph(pet)?,
                hero_node: model(g, hero)?
                    .ok_or("no model on the hero's cell")?
                    .0
                    .instance_id(),
            };
            draw(g, hero, &d.pet_glyph);
            *DRAWN.lock().map_err(|e| e.to_string())? = Some(d);
            Ok(())
        }),
        Step::Wait("the pet's model there, off the rim layer", |g| {
            let d = drawn()?;
            if g.world.map.hero().is_some() || g.world.hero() != Some(d.hero) {
                return Err("the hero's last known cell is not the one they left".into());
            }
            match model(g, d.hero)? {
                // the cell is not drawn again yet
                None => Ok(false),
                Some((node, _)) if node.instance_id() == d.hero_node => Ok(false),
                Some((_, true)) => {
                    Err("the model on the cell the hero left took the hero's rim layer".into())
                }
                Some((_, false)) => Ok(held(true)),
            }
        }),
        Step::Call("the hero drawn on the pet's cell", |g| {
            let d = drawn()?;
            draw(g, d.pet, &d.hero_glyph);
            Ok(())
        }),
        Step::Wait("the rim layer on the hero's model alone", |g| {
            let d = drawn()?;
            let (Some((_, on)), Some((_, pet_on))) = (model(g, d.pet)?, model(g, d.hero)?) else {
                return Ok(false);
            };
            if pet_on {
                return Err("the pet's model is on the hero's rim layer".into());
            }
            Ok(held(on && g.world.map.hero() == Some(d.pet)))
        }),
        Step::Call("both drawn where the engine has them", |g| {
            let d = drawn()?;
            draw(g, d.pet, &d.pet_glyph);
            draw(g, d.hero, &d.hero_glyph);
            Ok(())
        }),
        Step::Wait("the rim layer back with the hero, the steps over", |g| {
            let d = drawn()?;
            let (Some((_, on)), Some((_, pet_on))) = (model(g, d.hero)?, model(g, d.pet)?) else {
                return Ok(false);
            };
            let (hero_steps, others) = map_view(g)?.steps_under_way();
            Ok(held(on && !pet_on && !hero_steps && others == 0))
        }),
        // a model shown and put on the hero's layer in one frame is paired
        // with the rim light by that layer when the frame is drawn; taken
        // off the layer in view, it must not stay on the light's list
        Step::Call("the pet's model out of view", |g| {
            pet_model(g)?.set_visible(false);
            Ok(())
        }),
        draw_frames("frames drawn without it"),
        Step::Call("in view again, on the hero's layer", |g| {
            let mut node = pet_model(g)?;
            node.set_visible(true);
            MapView::set_rim(&node, true);
            Ok(())
        }),
        draw_frames("frames drawn with it in the rim light"),
        Step::Call("off the hero's layer, in view", |g| {
            MapView::set_rim(&pet_model(g)?, false);
            Ok(())
        }),
        draw_frames("frames drawn with it out of the rim light"),
    ]);
    steps.extend(quit());
    steps
}
