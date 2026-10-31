//! `features`: a throne, a sink, a fountain and an altar in a row, with
//! a patch of ice and a pool beside them and the hero for scale, close
//! and at the game's distance; then a cell of ice, of water and of lava
//! alone in the floor the same way: sheets of the map's features and
//! ground before and after a change of their look. Then every liquid and
//! feature a player meets on water, lava and ice (`gallery::LIQUIDS`:
//! each kind alone, a pair, lines, a field; fountains, a sink,
//! drawbridges over a moat, a boulder over a pool, a trap on ice, the hero
//! wading, a swamp's pattern, pools seen in part; the Planes of Fire and
//! Water): each cell must show its own kind where it is looked at, its
//! middle (off it where something stands there), judged by the colour
//! drawn there. A blur of the ground once hid a lone pool, lava and ice,
//! and dried every fountain. The pixels need a window (headless, nothing
//! is judged). With `--screenshots`, the pictures `features-close`,
//! `features`, `features-lone-close`, `features-lone` and `liquids-*`.

use std::sync::atomic::{AtomicUsize, Ordering};

use godot::classes::DisplayServer;
use godot::obj::Singleton;
use godot::prelude::*;
use nh_protocol::LevelNotice;

use super::{Step, camera_settled, drawn_image, quit, start};
use crate::gallery::{BUBBLES, LIQUIDS, LIQUIDS_AT, Shows};
use crate::game::RenethackGame;

/// A view of a liquids scene: its rows, the hero's cell where it has no
/// `@`, the view's middle (by column and row), the plane it lies on, its
/// picture.
struct LiquidView {
    rows: &'static [&'static str],
    hero: (i32, i32),
    view: (i32, i32),
    plane: Option<&'static str>,
    shot: &'static str,
}

const LIQUID_VIEWS: [LiquidView; 6] = [
    view(&LIQUIDS, (10, 2), None, "liquids-pools"),
    view(&LIQUIDS, (36, 2), None, "liquids-lava"),
    view(&LIQUIDS, (62, 2), None, "liquids-ice"),
    view(&LIQUIDS, (14, 8), None, "liquids-features"),
    view(&LIQUIDS, (36, 2), Some("fire"), "liquids-fire"),
    LiquidView {
        rows: &BUBBLES,
        hero: (9, 2),
        view: (7, 2),
        plane: Some("water"),
        shot: "liquids-water",
    },
];

/// A view of a scene with the hero on its `@`.
const fn view(
    rows: &'static [&'static str],
    view: (i32, i32),
    plane: Option<&'static str>,
    shot: &'static str,
) -> LiquidView {
    LiquidView {
        rows,
        hero: (0, 0),
        view,
        plane,
        shot,
    }
}

/// The view of `LIQUID_VIEWS` now laid out (steps are plain functions).
static VIEW: AtomicUsize = AtomicUsize::new(0);

/// Lay out the view's scene, on its plane.
fn lay_out_view(g: &mut RenethackGame) -> Result<(), String> {
    let v = &LIQUID_VIEWS[VIEW.load(Ordering::Relaxed)];
    let cat = g.catalog.clone().ok_or("no catalog")?;
    crate::gallery::lay_out_liquids(&mut g.world, &cat, v.rows, v.hero, v.view)?;
    g.world.level = v.plane.map(|p| LevelNotice {
        dungeon: "The Elemental Planes".into(),
        depth: -1,
        plane: Some(p.into()),
        lair: None,
    });
    let ui = g.ui.as_mut().ok_or("no UI")?;
    ui.map.set_showcase(false);
    ui.map.set_distance(10.0, 0.0);
    Ok(())
}

/// What a colour drawn tells of the ground under it, as far as colour
/// can tell at any light: lava's glow, the cold of water and ice, the warm
/// of the floor; on the Plane of Water a bubble's pale floor or the deep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    Warm,
    Cold,
    Glow,
    Pale,
    Dark,
}

/// What a kind of ground should read as.
fn reads_as(shows: Shows) -> Reads {
    match shows {
        Shows::Floor => Reads::Warm,
        Shows::Water | Shows::Ice => Reads::Cold,
        Shows::Lava => Reads::Glow,
        Shows::Bubble => Reads::Pale,
        Shows::Deep => Reads::Dark,
    }
}

/// What a colour drawn (sRGB, 0 to 1) reads as, or None where it tells
/// nothing for sure.
fn reads(c: Color, plane_water: bool) -> Option<Reads> {
    if plane_water {
        // (the deep has next to no red in it, a bubble's floor some)
        return Some(if c.r > 0.15 { Reads::Pale } else { Reads::Dark });
    }
    if c.r > 0.25 && c.r > 1.8 * c.b && c.r > 1.3 * c.g {
        Some(Reads::Glow)
    } else if c.b > 1.02 * c.r {
        Some(Reads::Cold)
    } else if c.r > 1.05 * c.b {
        Some(Reads::Warm)
    } else {
        None
    }
}

/// Every cell the view shows must show its own kind where it is looked
/// at; what is wrong is told cell by cell.
fn judge_view(g: &mut RenethackGame) -> Result<(), String> {
    if DisplayServer::singleton().get_name() == "headless" {
        return Ok(());
    }
    let v = &LIQUID_VIEWS[VIEW.load(Ordering::Relaxed)];
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let checks =
        crate::gallery::lay_out_liquids(&mut g.world.clone(), &cat, v.rows, v.hero, v.view)?;
    let image = drawn_image().ok_or("no picture of the frame")?;
    let (w, h) = (image.get_width(), image.get_height());
    let map = &g.ui.as_ref().ok_or("no UI")?.map;
    let water_plane = v.plane == Some("water");
    let mut wrong = Vec::new();
    let mut judged = 0;
    for ((x, y), (dx, dz), shows) in checks {
        let Some(p) = map.on_screen(Vector3::new(x as f32 + dx, 0.0, y as f32 + dz)) else {
            continue;
        };
        let (px, py) = (p.x as i32, p.y as i32);
        // inside the frame, clear of the HUD's corners and its bar
        let clear = px > 8 && py > 8 && px < w - 8 && py < h - 8;
        let hud = (px < w * 30 / 100 && py < h * 20 / 100)
            || (px > w * 75 / 100 && py < h * 22 / 100)
            || py > h * 82 / 100;
        if !clear || hud {
            continue;
        }
        let mut sum = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        for oy in -2..=2 {
            for ox in -2..=2 {
                let c = image.get_pixel(px + ox, py + oy);
                sum = Color::from_rgba(sum.r + c.r, sum.g + c.g, sum.b + c.b, 0.0);
            }
        }
        let c = Color::from_rgb(sum.r / 25.0, sum.g / 25.0, sum.b / 25.0);
        let seen = reads(c, water_plane);
        let want = reads_as(shows);
        godot_print!(
            "selftest: liquids: {} {x},{y} {shows:?} wants {want:?}, reads {seen:?} ({:.3} {:.3} {:.3})",
            v.shot,
            c.r,
            c.g,
            c.b
        );
        judged += 1;
        let (col, row) = (x - LIQUIDS_AT.0, y - LIQUIDS_AT.1);
        if seen != Some(want) {
            wrong.push(format!("{col},{row} ({shows:?}) reads {seen:?}"));
        }
    }
    if judged == 0 {
        return Err(format!("{}: nothing judged", v.shot));
    }
    if !wrong.is_empty() {
        return Err(format!("{}: {}", v.shot, wrong.join("; ")));
    }
    Ok(())
}

pub(super) fn features() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("the features in a row", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let syms = ["S_throne", "S_sink", "S_fountain", "S_altar"];
            crate::gallery::lay_out_features(&mut g.world, &cat, &syms)?;
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_showcase(true);
            ui.map.set_distance(7.0, 0.4);
            Ok(())
        }),
        Step::Wait("the row drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on them", camera_settled),
        Step::Shot("features-close"),
        Step::Call("at the game's distance", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(10.0, 0.0);
            Ok(())
        }),
        Step::Wait("the camera there", camera_settled),
        Step::Shot("features"),
        // a cell of ice, of water and of lava alone in the floor, each
        // its own (a lone cell's ground lost to the floor about it: the
        // ice a grey puff)
        Step::Call("lone cells of ground in a row", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let syms = ["S_ice", "S_pool", "S_lava"];
            crate::gallery::lay_out_features(&mut g.world, &cat, &syms)?;
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(7.0, 0.4);
            Ok(())
        }),
        Step::Wait("the row drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on them", camera_settled),
        Step::Shot("features-lone-close"),
        Step::Call("at the game's distance", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(10.0, 0.0);
            Ok(())
        }),
        Step::Wait("the camera there", camera_settled),
        Step::Shot("features-lone"),
    ]);
    for v in &LIQUID_VIEWS {
        steps.extend([
            Step::Call("a liquids scene", lay_out_view),
            Step::Wait("the scene drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on it", camera_settled),
            Step::Shot(v.shot),
            Step::Call("each cell its own kind", judge_view),
            Step::Call("the next view", |_| {
                VIEW.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(quit());
    steps
}
