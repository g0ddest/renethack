//! `doll`: the inventory's doll shows the hero as they are, in its own
//! light, and not as the map marks them.
//!
//! The doll's camera looks at the hero's own model in the map's world, and
//! a camera takes only the lights on a layer it sees: the doll's key, fill
//! and rim are on a layer of their own that the map's camera leaves out.
//! Put out, they leave the doll in its dim ambient light and the hero on
//! the map as lit as before. (The doll's first lights were on the map's
//! layer: they lit the hero on the map, and the doll not at all.)
//!
//! While a wall hides the hero from the map's camera, their
//! model wears the x-ray overlay: a tint drawn wherever something nearer
//! covers it. The doll's camera looks at the same model from close by, and
//! there what is nearer is the hero's own shield, weapon and limbs.
//!
//! The doll is there from the first frame an opening draws, lit: the first
//! time in a session (the doll's camera was last at the title's hero) and
//! after the hero walked on. A camera and its lamps reach the renderer a
//! frame after they move: aimed only as the panel opened, the doll's
//! first render looked at where the hero had been, blank or unlit.
//!
//! The scenario needs a window: a headless run renders no doll. The
//! overlay is put on as while a wall hides the hero, in a colour no hero
//! wears and with nothing kept back for the model's own depth, so that
//! every part of the hero behind another would show it: the map's camera
//! must draw it still, and the doll's render must have none of it. With
//! `--screenshots`, a picture of each.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use godot::classes::{DisplayServer, RenderingServer};
use godot::prelude::*;

use super::{
    Step, at_next_frame, camera_settled, game_now, map_view, next_frame_failed, panel, quit, start,
};
use crate::game::RenethackGame;
use crate::inventory_panel::InvInput;
use crate::ui_events::UiEvent;

static FRAMES: AtomicU32 = AtomicU32::new(0);
static SINCE: Mutex<Option<Instant>> = Mutex::new(None);
/// Frames, and the time, for the doll to render the hero anew a few times
/// (it renders thirty times a second at most, however fast the frames
/// come).
const FRAMES_RENDERED: u32 = 20;
const TIME_RENDERED: Duration = Duration::from_millis(300);

/// How bright the doll and the hero on the map were, the doll's lights out.
static UNLIT: Mutex<Option<(f32, f32)>> = Mutex::new(None);

/// How bright the map's picture is where the hero stands (the window's
/// last frame, drawn whatever the system thinks of the window).
fn map_hero_luma(g: &RenethackGame) -> Result<f32, String> {
    RenderingServer::singleton().force_draw();
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let hero = ui
        .hud
        .threat_beside()
        .0
        .ok_or("the hero is not on screen")?;
    let image = g.viewport_image().ok_or("no picture of the map")?;
    // the HUD's canvas may be scaled against the window's pixels
    let scale = image.get_width() as f32 / g.canvas_size().x.max(1.0);
    let (x0, x1) = (hero.position.x * scale, hero.end().x * scale);
    let (y0, y1) = (hero.position.y * scale, hero.end().y * scale);
    let cells = ((y0 as i32).max(0)..(y1 as i32).min(image.get_height()))
        .step_by(2)
        .flat_map(|y| {
            ((x0 as i32).max(0)..(x1 as i32).min(image.get_width()))
                .step_by(2)
                .map(move |x| (x, y))
        });
    let lumas: Vec<f32> = cells
        .map(|(x, y)| image.get_pixel(x, y).luminance() as f32)
        .collect();
    if lumas.is_empty() {
        return Err("the hero's place on screen is empty".into());
    }
    Ok(lumas.iter().sum::<f32>() / lumas.len() as f32)
}

fn doll_lights(g: &mut RenethackGame, on: bool) -> Result<(), String> {
    g.ui.as_mut().ok_or("no UI")?.inventory.doll_lights(on);
    Ok(())
}

/// The probe's colour as the doll's render shows it: a strong magenta.
fn probe(c: Color) -> bool {
    c.a > 0.5 && c.r > 0.45 && c.b > 0.45 && c.g < 0.6 * c.r.min(c.b)
}

fn drawn(c: Color) -> bool {
    c.a > 0.5
}

fn probed(g: &mut RenethackGame, on: bool) -> Result<(), String> {
    if g.ui.as_mut().ok_or("no UI")?.map.probe_xray(on) {
        Ok(())
    } else {
        Err("the hero's model takes no x-ray overlay".into())
    }
}

/// Frames drawn whatever the window: macOS does not draw one it thinks
/// nobody sees, and the doll renders only when a frame is drawn. A check
/// made on a doll not rendered since would look at the old picture.
/// A cell of the map.
type At = (i32, i32);

/// Where the hero stood before the scenario moved them, and where to.
static MOVED: Mutex<Option<(At, At)>> = Mutex::new(None);

/// The doll as the first frame of an opening draws it: there, and lit.
fn doll_as_drawn(when: &'static str) {
    // a window the system did not draw: its doll renders now, as it would
    // have in that frame
    RenderingServer::singleton().force_draw();
    let Some(game) = game_now() else {
        return next_frame_failed("no game".into());
    };
    let g = game.bind();
    let Some(p) = g.ui.as_ref().map(|ui| &ui.inventory) else {
        return next_frame_failed("no UI".into());
    };
    let (Some(share), luma) = (p.doll_share(drawn), p.doll_luma()) else {
        return next_frame_failed("no doll render".into());
    };
    let luma = luma.unwrap_or(0.0);
    godot_print!(
        "selftest: doll: {when}: the first frame's doll is {share:.3} drawn, {luma:.3} bright"
    );
    if share < 0.05 {
        next_frame_failed(format!(
            "{when}: the first frame drawn has a blank doll ({share:.3} of it drawn)"
        ));
    } else if luma < 0.15 {
        next_frame_failed(format!(
            "{when}: the first frame drawn has an unlit doll ({luma:.3} bright)"
        ));
    }
}

/// The inventory toggled as a player would, its first frame checked.
fn opened() -> [Step; 2] {
    [
        Step::Call("the inventory toggled", |g| {
            g.push_ui(UiEvent::Inventory(InvInput::Toggle));
            Ok(())
        }),
        Step::Wait("the panel open, its doll checked as it is drawn", |g| {
            let open = g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open());
            if open {
                let when = if MOVED.lock().map_err(|e| e.to_string())?.is_some() {
                    "after the hero walked on"
                } else {
                    "the first opening"
                };
                at_next_frame(move || doll_as_drawn(when))?;
            }
            Ok(open)
        }),
    ]
}

/// The hero drawn some cells away on a floor of the room (the engine is
/// not asked); `back`: where they were.
fn move_hero(g: &mut RenethackGame, back: bool) -> Result<(), String> {
    let mut moved = MOVED.lock().map_err(|e| e.to_string())?;
    let (from, to) = match (*moved, back) {
        (Some((from, to)), true) => (to, from),
        (None, false) => {
            let hero = g.world.map.hero().ok_or("no hero")?;
            let cat = g.catalog.clone().ok_or("no catalog")?;
            // a room's floor with nothing on it
            let bare = |c: &nh_world::Cell| {
                let g = c
                    .glyph
                    .as_ref()
                    .filter(|g| g.kind == nh_protocol::GlyphKind::Cmap);
                let sym = g.and_then(|g| cat.cmap.get(usize::try_from(g.cmap?).ok()?));
                sym.is_some_and(|s| s.sym == "S_room")
            };
            let far = (1..nh_world::COLNO)
                .flat_map(|x| (0..nh_world::ROWNO).map(move |y| (x, y)))
                .filter(|&(x, y)| {
                    let d = (x - hero.0).abs().max((y - hero.1).abs());
                    (4..=8).contains(&d) && g.world.map.cell(x, y).is_some_and(bare)
                })
                .max_by_key(|&(x, y)| (x - hero.0).abs().max((y - hero.1).abs()))
                .ok_or("no bare floor four cells or more from the hero")?;
            (hero, far)
        }
        _ => return Err("the hero is not where the scenario left them".into()),
    };
    let cell = |at: (i32, i32)| g.world.map.cell(at.0, at.1).cloned();
    let (from_cell, to_cell) = (cell(from).ok_or("no cell")?, cell(to).ok_or("no cell")?);
    let (Some(hero_glyph), Some(floor)) = (from_cell.glyph, to_cell.glyph) else {
        return Err("a cell with nothing drawn".into());
    };
    g.world
        .map
        .print(to.0, to.1, &hero_glyph, to_cell.bk.as_ref());
    g.world
        .map
        .print(from.0, from.1, &floor, from_cell.bk.as_ref());
    *moved = (!back).then_some((from, to));
    Ok(())
}

/// The hero's model stands on the cell the scenario drew them on.
fn hero_there(g: &RenethackGame) -> Result<bool, String> {
    let to = match *MOVED.lock().map_err(|e| e.to_string())? {
        Some((_, to)) => to,
        None => match g.world.map.hero() {
            Some(at) => at,
            None => return Ok(false),
        },
    };
    let Some((at, _)) = map_view(g)?.hero_stands() else {
        return Ok(false);
    };
    Ok((at.x - to.0 as f32).abs() < 0.05 && (at.z - to.1 as f32).abs() < 0.05)
}

fn rendered(what: &'static str) -> Step {
    Step::Wait(what, |_| {
        RenderingServer::singleton().force_draw();
        let mut since = SINCE.lock().map_err(|e| e.to_string())?;
        let begun = *since.get_or_insert_with(Instant::now);
        if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 < FRAMES_RENDERED
            || begun.elapsed() < TIME_RENDERED
        {
            return Ok(false);
        }
        FRAMES.store(0, Ordering::Relaxed);
        *since = None;
        Ok(true)
    })
}

pub(super) fn doll() -> Vec<Step> {
    let mut steps = vec![Step::Call("a window to draw in", |_| {
        if DisplayServer::singleton().get_name() == "headless" {
            return Err("the renderer must run: start it without --headless".into());
        }
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Wait("the hero's model", |g| {
            Ok(map_view(g)?.hero_model().is_some())
        }),
    ]);
    steps.extend(opened());
    steps.extend([
        Step::Wait("the doll renders the hero", |g| {
            RenderingServer::singleton().force_draw();
            let p = panel(g)?;
            Ok(p.doll_rendered() && p.doll_share(drawn).is_some_and(|s| s > 0.05))
        }),
        rendered("the doll as it is"),
        Step::Call("nothing the hero wears has the probe's colour", |g| {
            let share = panel(g)?.doll_share(probe).ok_or("no doll render")?;
            if share > 0.0 {
                return Err(format!(
                    "the hero wears the probe's colour ({share:.4} of the doll's render)"
                ));
            }
            Ok(())
        }),
        // the doll's own lights: put out, the doll is dimmer and the hero
        // on the map is not
        Step::Shot("doll-lit"),
        Step::Call("the doll's lights out", |g| doll_lights(g, false)),
        rendered("the doll in its ambient light alone"),
        Step::Shot("doll-unlit"),
        Step::Call("how bright the doll is without them", |g| {
            let doll = panel(g)?.doll_luma().ok_or("no doll render")?;
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            *UNLIT.lock().map_err(|e| e.to_string())? = Some((doll, 0.0));
            Ok(())
        }),
        rendered("the map without the panel, the doll's lights out"),
        Step::Call("how bright the hero on the map is without them", |g| {
            let map = map_hero_luma(g)?;
            if let Some(unlit) = UNLIT.lock().map_err(|e| e.to_string())?.as_mut() {
                unlit.1 = map;
            }
            doll_lights(g, true)
        }),
        rendered("the map, the doll's lights lit again"),
        Step::Call("the doll's lights do not light the map's hero", |g| {
            let (_, unlit) = UNLIT.lock().map_err(|e| e.to_string())?.ok_or("not measured")?;
            let lit = map_hero_luma(g)?;
            godot_print!("selftest: doll: the map's hero is {unlit:.4} bright, {lit:.4} with the doll's lights");
            if (lit - unlit).abs() > 0.05 * unlit.max(0.02) {
                return Err(format!(
                    "the doll's lights light the hero on the map: {unlit:.4} without them, {lit:.4} with"
                ));
            }
            g.ui.as_mut().ok_or("no UI")?.inventory.open();
            Ok(())
        }),
        rendered("the doll in its lights again"),
        Step::Call("the doll's lights light the doll", |g| {
            let (unlit, _) = UNLIT.lock().map_err(|e| e.to_string())?.ok_or("not measured")?;
            let lit = panel(g)?.doll_luma().ok_or("no doll render")?;
            godot_print!("selftest: doll: the doll is {unlit:.4} bright, {lit:.4} in its lights");
            if lit < unlit * 1.3 {
                return Err(format!(
                    "the doll's lights do not light the doll: {unlit:.4} without them, {lit:.4} with"
                ));
            }
            Ok(())
        }),
        Step::Call("the panel away, the x-ray overlay on the hero", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            probed(g, true)
        }),
        rendered("the map with the overlay on the hero's model"),
        Step::Shot("doll-xray-map"),
        Step::Call("the map's own camera draws the overlay", |g| {
            // the window's last frame, drawn whatever the system thinks of it
            RenderingServer::singleton().force_draw();
            let image = g.viewport_image().ok_or("no picture of the map")?;
            let (w, h) = (image.get_width(), image.get_height());
            // the hero stands in the middle of the view
            let cells = (h / 4..h * 3 / 4)
                .step_by(2)
                .flat_map(|y| (w / 3..w * 2 / 3).step_by(2).map(move |x| (x, y)));
            let seen = cells.filter(|&(x, y)| probe(image.get_pixel(x, y))).count();
            godot_print!("selftest: doll: {seen} points of the map's hero are the overlay's");
            if seen < 20 {
                return Err(format!(
                    "the map shows no x-ray overlay on the hero ({seen} points)"
                ));
            }
            Ok(())
        }),
        Step::Call("the inventory open again", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.open();
            Ok(())
        }),
        rendered("the doll with the overlay on the hero's model"),
        Step::Shot("doll-xray"),
        Step::Call("the doll shows none of the overlay", |g| {
            let p = panel(g)?;
            let share = p.doll_share(probe).ok_or("no doll render")?;
            godot_print!("selftest: doll: {share:.4} of the doll's render is the overlay's");
            if !p.doll_share(drawn).is_some_and(|s| s > 0.05) {
                return Err("the doll does not show the hero any more".into());
            }
            if share > 0.0 {
                return Err(format!(
                    "the x-ray overlay shows on the doll ({share:.4} of its render)"
                ));
            }
            Ok(())
        }),
        Step::Call("the overlay off", |g| {
            probed(g, false)?;
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            Ok(())
        }),
        // the hero walks on, the panel closed; it opens again
        Step::Call("the hero drawn some cells away", |g| move_hero(g, false)),
        Step::Wait("the hero's model there", hero_there),
        rendered("the map, the hero there"),
    ]);
    steps.extend(opened());
    steps.extend([
        rendered("the doll"),
        Step::Shot("doll-walked-on"),
        Step::Call("the panel away, the hero where they were", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            move_hero(g, true)
        }),
        Step::Wait("the hero's model back", hero_there),
    ]);
    steps.extend(quit());
    steps
}
