//! `doll`: the inventory's doll shows the hero as they are, not as the map
//! marks them. While a wall hides the hero from the map's camera, their
//! model wears the x-ray overlay: a tint drawn wherever something nearer
//! covers it. The doll's camera looks at the same model from close by, and
//! there what is nearer is the hero's own shield, weapon and limbs.
//!
//! The scenario needs a window: a headless run renders no doll. The
//! overlay is put on as while a wall hides the hero, in a colour no hero
//! wears and with nothing kept back for the model's own depth, so that
//! every part of the hero behind another would show it: the map's camera
//! must draw it still, and the doll's render must have none of it. With
//! `--screenshots`, a picture of each.

use std::sync::atomic::{AtomicU32, Ordering};

use godot::classes::{DisplayServer, RenderingServer};
use godot::prelude::*;

use super::{Step, camera_settled, map_view, panel, quit, start};
use crate::game::RenethackGame;

static FRAMES: AtomicU32 = AtomicU32::new(0);
/// Frames for the doll to render the hero anew a few times (it renders
/// every other frame at most).
const FRAMES_RENDERED: u32 = 20;

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

fn rendered(what: &'static str) -> Step {
    Step::Wait(what, |_| {
        if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 < FRAMES_RENDERED {
            return Ok(false);
        }
        FRAMES.store(0, Ordering::Relaxed);
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
        Step::Call("the inventory open", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.open();
            Ok(())
        }),
        Step::Wait("the doll renders the hero", |g| {
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
    ]);
    steps.extend(quit());
    steps
}
