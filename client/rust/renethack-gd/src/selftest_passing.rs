//! `passing`: what the engine draws in passing leaves who stands there.
//! A monster's missile is drawn on the hero's cell when it reaches them; a
//! ray, an explosion or a sparkle over whoever is in its way; and the cell
//! the hero steps off may be drawn a frame before the cell they step to.
//! None of it takes the hero's or the pet's model away: the model stays (or
//! walks), its ring under it, the hero's ring, light and doll with the hero.
//! Only a hero the engine asks a command of without drawing them (invisible,
//! hiding) is gone, and then what they stand on is not taken for them.
//!
//! The map is drawn here as the engine's print_glyph would draw it, in the
//! order and the frames the engine does; the engine is not asked, and has
//! its own map back before the game is quit. With `--screenshots`, a
//! picture of each moment. What went wrong is told at the end, all of it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use godot::prelude::*;
use nh_protocol::{Glyph, GlyphKind, mg};

use super::{Step, camera_settled, map_view, panel, quit, start, zoom_by};
use crate::game::RenethackGame;

/// The cells and models as the game drew them before anything passed.
#[derive(Clone)]
struct Drawn {
    hero: (i32, i32),
    /// A floor cell beside the hero, where they are drawn stepping to.
    next: (i32, i32),
    pet: (i32, i32),
    hero_glyph: Glyph,
    pet_glyph: Glyph,
    /// The floor of the cell beside (and, for the drawing, under the hero).
    floor: Glyph,
    hero_node: InstanceId,
    pet_node: InstanceId,
    hero_steps: u32,
}

static DRAWN: Mutex<Option<Drawn>> = Mutex::new(None);
/// What did not stay as it should, told at the end.
static WRONG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static FRAMES: AtomicU32 = AtomicU32::new(0);
/// Frames a drawing is left on screen before it is looked at: the map
/// takes it in the next frame, a model first built comes a few later, and
/// the doll renders every other frame.
const FRAMES_SHOWN: u32 = 10;

fn drawn() -> Result<Drawn, String> {
    DRAWN
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "the cells are not remembered yet".to_string())
}

fn wrong(what: String) {
    godot_print!("selftest: passing: WRONG: {what}");
    if let Ok(mut w) = WRONG.lock() {
        w.push(what);
    }
}

/// A few frames with the map as it was just drawn.
fn shown(what: &'static str) -> Step {
    Step::Wait(what, |_| {
        if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 < FRAMES_SHOWN {
            return Ok(false);
        }
        FRAMES.store(0, Ordering::Relaxed);
        Ok(true)
    })
}

/// Draw a glyph on a cell of the client's map, as a print_glyph of the
/// engine's would (the engine is not asked).
fn draw(g: &mut RenethackGame, (x, y): (i32, i32), glyph: &Glyph) {
    let bk = g.world.map.cell(x, y).and_then(|c| c.bk.clone());
    g.world.map.print(x, y, glyph, bk.as_ref());
}

/// The engine's own word of where the hero went, as a step sends it: the
/// view clipped around them, and the cursor put on them by the next
/// message (the monster's throw is told before the missile flies).
fn stands_on(g: &mut RenethackGame, at: (i32, i32)) {
    g.world.view_center = Some(at);
    g.world.cursor = Some(at);
}

fn glyph(kind: GlyphKind, ch: char, color: i32) -> Glyph {
    Glyph {
        glyph: None,
        ch: ch as i32,
        color,
        flags: 0,
        tile: 0,
        kind,
        mon: None,
        cmap: None,
        level: None,
    }
}

/// A thrown dagger, as tmp_at shows it.
fn dagger(g: &RenethackGame) -> Result<Glyph, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let tile = cat
        .object_tiles
        .iter()
        .find(|t| t.appearance == "dagger")
        .ok_or("no dagger in the catalog")?;
    Ok(Glyph {
        tile: tile.tile,
        ..glyph(GlyphKind::Obj, ')', tile.color.unwrap_or(6))
    })
}

/// A resistance's sparkle (shieldeff), an effect cmap.
fn sparkle(g: &RenethackGame) -> Result<Glyph, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let ss = cat
        .cmap
        .iter()
        .find(|c| c.sym == "S_ss1")
        .ok_or("no S_ss1 in the catalog")?;
    Ok(Glyph {
        cmap: Some(ss.idx),
        ..glyph(GlyphKind::Cmap, '0', 14)
    })
}

fn node_on(g: &RenethackGame, (x, y): (i32, i32)) -> Result<Option<InstanceId>, String> {
    Ok(map_view(g)?.entity_rim(x, y).map(|(n, _)| n.instance_id()))
}

/// The hero and the pet are still who stands on their cells: the same
/// models, each ring under its own, the hero where the map has them.
fn stayed(g: &RenethackGame, what: &str, hero: (i32, i32)) -> Result<(), String> {
    let d = drawn()?;
    let map = map_view(g)?;
    if g.world.hero() != Some(hero) {
        wrong(format!(
            "{what}: the hero is taken to be on {:?}, not on {hero:?}",
            g.world.hero()
        ));
    }
    if node_on(g, hero)? != Some(d.hero_node) {
        wrong(format!("{what}: the hero's model is not on {hero:?}"));
    }
    if map.hero_model().map(|m| m.node.instance_id()) != Some(d.hero_node) {
        wrong(format!(
            "{what}: the hero's model is not the one the doll shows"
        ));
    }
    if node_on(g, d.pet)? != Some(d.pet_node) {
        wrong(format!("{what}: the pet's model is not on {:?}", d.pet));
    }
    let rings = map.ring_gaps();
    for (whose, cell) in [("hero", hero), ("pet", d.pet)] {
        match rings.iter().find(|r| (r.0, r.1) == (whose, cell)) {
            Some((_, _, Some(gap))) if *gap <= 0.05 => {}
            Some((_, _, Some(gap))) => wrong(format!(
                "{what}: the {whose}'s ring lies {gap:.2} from its model"
            )),
            Some((_, _, None)) => wrong(format!("{what}: the {whose}'s ring has no model in it")),
            None => wrong(format!("{what}: no ring of the {whose}'s on {cell:?}")),
        }
    }
    Ok(())
}

pub(super) fn passing() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("closer, the pointer off the picture", |g| {
            // a window's mouse would frame a cell of its own choosing
            g.test_hover = Some((1, 0));
            zoom_by(g, -3.0)
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Wait("the hero's and the pet's models, standing", |g| {
            let Some(hero) = g.world.map.hero() else {
                return Ok(false);
            };
            let pet = (1..nh_world::COLNO)
                .flat_map(|x| (0..nh_world::ROWNO).map(move |y| (x, y)))
                .find(|&(x, y)| {
                    let cell = g.world.map.cell(x, y);
                    cell.and_then(|c| c.entity())
                        .is_some_and(|e| e.flags & mg::PET != 0)
                });
            let Some(pet) = pet else {
                return Err("no pet in sight".into());
            };
            let map = map_view(g)?;
            let (Some(hero_node), Some(pet_node)) = (node_on(g, hero)?, node_on(g, pet)?) else {
                return Ok(false);
            };
            if map.steps_under_way() != (false, 0) {
                return Ok(false);
            }
            let glyph_on = |(x, y): (i32, i32)| {
                let cell = g.world.map.cell(x, y);
                cell.and_then(|c| c.glyph.clone()).ok_or("an empty cell")
            };
            // a cell beside the hero that shows a room's floor and nothing
            // on it (what is under the hero the engine has not said: the
            // same floor will do for the cell they are drawn stepping off)
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let room = |c: &nh_world::Cell| {
                let sym = c.glyph.as_ref().filter(|g| g.kind == GlyphKind::Cmap);
                let sym = sym.and_then(|g| cat.cmap.get(usize::try_from(g.cmap?).ok()?));
                sym.is_some_and(|c| c.sym == "S_room")
            };
            let next = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .into_iter()
                .map(|(dx, dy)| (hero.0 + dx, hero.1 + dy))
                .find(|&(x, y)| g.world.map.cell(x, y).is_some_and(room))
                .ok_or("no bare floor beside the hero")?;
            let floor = glyph_on(next)?;
            let d = Drawn {
                hero,
                next,
                pet,
                hero_glyph: glyph_on(hero)?,
                pet_glyph: glyph_on(pet)?,
                floor,
                hero_node,
                pet_node,
                hero_steps: map.motion_stats().hero_steps,
            };
            *DRAWN.lock().map_err(|e| e.to_string())? = Some(d);
            Ok(true)
        }),
        Step::Shot("passing-0-before"),
        // a turn of the engine's: the hero steps, a monster throws a dagger
        // at them; no command is asked in between
        Step::Call("the hero drawn a step on", |g| {
            let d = drawn()?;
            draw(g, d.hero, &d.floor);
            draw(g, d.next, &d.hero_glyph);
            stands_on(g, d.next);
            Ok(())
        }),
        Step::Wait("the hero walks there", |g| {
            let d = drawn()?;
            let map = map_view(g)?;
            Ok(map.motion_stats().hero_steps == d.hero_steps + 1 && map.hero_step().is_some())
        }),
        Step::Call("a dagger drawn on their cell, as it reaches them", |g| {
            let d = drawn()?;
            let dagger = dagger(g)?;
            draw(g, d.next, &dagger);
            Ok(())
        }),
        shown("the dagger on the hero's cell"),
        Step::Shot("passing-1-missile"),
        Step::Call("the hero is there under the dagger", |g| {
            let d = drawn()?;
            stayed(g, "a dagger on the hero's cell", d.next)
        }),
        Step::Call("the inventory opened at that moment", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.open();
            Ok(())
        }),
        shown("the panel and its doll"),
        Step::Shot("passing-2-missile-doll"),
        Step::Call("the doll shows the hero", |g| {
            let p = panel(g)?;
            if !p.doll_rendered() {
                wrong("a dagger on the hero's cell: the doll does not render the hero".into());
            }
            // without a renderer there is no picture to look at
            if let Some(share) = p.doll_drawn() {
                godot_print!("selftest: passing: the doll's render is {share:.3} drawn");
                if share < 0.02 {
                    wrong(format!(
                        "a dagger on the hero's cell: the doll is blank ({share:.3} of it drawn)"
                    ));
                }
            }
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            Ok(())
        }),
        Step::Call("the flight over: the hero drawn again", |g| {
            let d = drawn()?;
            draw(g, d.next, &d.hero_glyph);
            Ok(())
        }),
        shown("the hero alone on the cell"),
        Step::Call("the hero is as before", |g| {
            let d = drawn()?;
            stayed(g, "after the dagger", d.next)
        }),
        // effects: a ray over the pet, a sparkle over the hero
        Step::Call("a ray drawn over the pet, a sparkle over the hero", |g| {
            let d = drawn()?;
            draw(g, d.pet, &glyph(GlyphKind::Zap, '-', 12));
            let sparkle = sparkle(g)?;
            draw(g, d.next, &sparkle);
            Ok(())
        }),
        shown("the ray and the sparkle"),
        Step::Shot("passing-3-effects"),
        Step::Call("both are there under them", |g| {
            let d = drawn()?;
            stayed(g, "a ray over the pet, a sparkle over the hero", d.next)
        }),
        Step::Call("the effects over: both drawn again", |g| {
            let d = drawn()?;
            draw(g, d.pet, &d.pet_glyph);
            draw(g, d.next, &d.hero_glyph);
            Ok(())
        }),
        shown("the pet and the hero alone on their cells"),
        Step::Call("both are as before", |g| {
            let d = drawn()?;
            stayed(g, "after the effects", d.next)
        }),
        // a step whose two cells come in two frames: back to where the
        // engine has the hero
        Step::Call("the cell the hero steps off drawn first", |g| {
            let d = drawn()?;
            draw(g, d.next, &d.floor);
            Ok(())
        }),
        shown("a frame between the two cells"),
        Step::Shot("passing-4-between"),
        Step::Call("the hero has not gone anywhere yet", |g| {
            let d = drawn()?;
            stayed(g, "the cell the hero leaves drawn first", d.next)
        }),
        Step::Call("the hero drawn on the next cell", |g| {
            let d = drawn()?;
            draw(g, d.hero, &d.hero_glyph);
            stands_on(g, d.hero);
            Ok(())
        }),
        shown("the hero on the cell the engine has them on"),
        Step::Call("the hero walked there", |g| {
            let d = drawn()?;
            let steps = map_view(g)?.motion_stats().hero_steps;
            if steps != d.hero_steps + 2 {
                wrong(format!(
                    "a step drawn in two frames: the hero did not walk it ({} steps, not {})",
                    steps - d.hero_steps,
                    2
                ));
            }
            stayed(g, "a step drawn in two frames", d.hero)
        }),
        Step::Wait("everyone arrived", |g| {
            Ok(map_view(g)?.steps_under_way() == (false, 0))
        }),
        Step::Shot("passing-5-after"),
        // a hero the engine does not draw (invisible), standing on a thing:
        // the engine asks for a command with the thing on their cell
        Step::Call("the hero unseen on a dagger, the inventory open", |g| {
            let d = drawn()?;
            let dagger = dagger(g)?;
            draw(g, d.hero, &dagger);
            g.world.map.settle();
            g.ui.as_mut().ok_or("no UI")?.inventory.open();
            Ok(())
        }),
        shown("the dagger where the hero stands unseen"),
        Step::Shot("passing-6-unseen-doll"),
        Step::Call("the dagger is not taken for the hero", |g| {
            let d = drawn()?;
            if g.world.map.hero().is_some() || g.world.hero() != Some(d.hero) {
                wrong(format!(
                    "an unseen hero: taken to be on {:?}, drawn on {:?}",
                    g.world.hero(),
                    g.world.map.hero()
                ));
            }
            if node_on(g, d.hero)?.is_none() {
                wrong("an unseen hero: the dagger they stand on is not shown".into());
            }
            if map_view(g)?.hero_model().is_some() {
                wrong("an unseen hero: the thing they stand on is taken for their model".into());
            }
            if panel(g)?.doll_rendered() {
                wrong("an unseen hero: the doll renders the thing they stand on".into());
            }
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            draw(g, d.hero, &d.hero_glyph);
            Ok(())
        }),
        shown("the hero seen again"),
        Step::Call("the hero's model is back", |g| {
            if map_view(g)?.hero_model().is_none() {
                wrong("a hero seen again has no model".into());
            }
            Ok(())
        }),
        Step::Call("everything stayed", |g| {
            g.test_hover = None;
            let wrong = WRONG.lock().map_err(|e| e.to_string())?;
            if wrong.is_empty() {
                Ok(())
            } else {
                Err(wrong.join("; "))
            }
        }),
    ]);
    steps.extend(quit());
    steps
}
