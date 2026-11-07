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

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

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

/// The colours each cell of the view was sampled as, frame by frame
/// (`sample_view`), for `judge_view`.
static SAMPLES: std::sync::Mutex<Vec<Vec<Option<Color>>>> = std::sync::Mutex::new(Vec::new());

/// How many frames a view is sampled on, and how far apart (seconds):
/// lava's glow seethes and its cracks drift, and on one frame a bright
/// crack under the point (pale in the tonemap) read as warm floor.
const SAMPLE_FRAMES: usize = 5;
const SAMPLE_APART: f32 = 0.35;
/// How far from the point a sample reaches (pixels, either way): a few
/// pixels; on lava wide enough to take in a crack's crust about it (a
/// bright crack alone read pale), well inside the cell (about 80 pixels
/// across at the view's distance). Wide everywhere, a deep cell beside a
/// bubble took in the bubble's floor.
const SAMPLE_BOX: i32 = 2;
const LAVA_BOX: i32 = 7;

/// When the view was last sampled (seconds; steps are plain functions).
static SAMPLED_AT: AtomicU32 = AtomicU32::new(0);

fn clock() -> f32 {
    godot::classes::Time::singleton().get_ticks_msec() as f32 / 1000.0
}

/// Each cell of the view as it is drawn now, where it is looked at: the
/// mean colour of a box about the point (None off the frame or under the
/// HUD), added to `SAMPLES`.
fn sample_view(g: &mut RenethackGame) -> Result<(), String> {
    SAMPLED_AT.store(clock().to_bits(), Ordering::Relaxed);
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
    let mut frame = Vec::new();
    for ((x, y), (dx, dz), shows) in checks {
        let Some(p) = map.on_screen(Vector3::new(x as f32 + dx, 0.0, y as f32 + dz)) else {
            frame.push(None);
            continue;
        };
        let (px, py) = (p.x as i32, p.y as i32);
        // inside the frame, clear of the HUD's corners and its bar
        let clear = px > 8 && py > 8 && px < w - 8 && py < h - 8;
        let hud = (px < w * 30 / 100 && py < h * 20 / 100)
            || (px > w * 75 / 100 && py < h * 22 / 100)
            || py > h * 82 / 100;
        if !clear || hud {
            frame.push(None);
            continue;
        }
        let mut sum = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        let b = if shows == Shows::Lava {
            LAVA_BOX
        } else {
            SAMPLE_BOX
        };
        for oy in -b..=b {
            for ox in -b..=b {
                let c = image.get_pixel(px + ox, py + oy);
                sum = Color::from_rgba(sum.r + c.r, sum.g + c.g, sum.b + c.b, 0.0);
            }
        }
        let n = ((2 * b + 1) * (2 * b + 1)) as f32;
        frame.push(Some(Color::from_rgb(sum.r / n, sum.g / n, sum.b / n)));
    }
    SAMPLES.lock().map_err(|e| e.to_string())?.push(frame);
    Ok(())
}

/// `SAMPLE_APART` since the last sample.
fn apart(_: &RenethackGame) -> Result<bool, String> {
    Ok(clock() - f32::from_bits(SAMPLED_AT.load(Ordering::Relaxed)) >= SAMPLE_APART)
}

/// The median of each channel over the frames a cell was sampled on.
fn median(frames: &[Color]) -> Color {
    let mid = |f: fn(&Color) -> f32| {
        let mut v: Vec<f32> = frames.iter().map(f).collect();
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    Color::from_rgb(mid(|c| c.r), mid(|c| c.g), mid(|c| c.b))
}

/// Every cell the view shows must show its own kind where it is looked
/// at, judged on the frame right after the picture; lava by the median
/// of its samples over `SAMPLE_FRAMES` frames (`sample_view`). (Every
/// kind over the frames, a deep cell beside the hero read pale as the
/// level's lights came up after the picture.) What is wrong is told cell
/// by cell.
fn judge_view(g: &mut RenethackGame) -> Result<(), String> {
    let frames = std::mem::take(&mut *SAMPLES.lock().map_err(|e| e.to_string())?);
    if DisplayServer::singleton().get_name() == "headless" {
        return Ok(());
    }
    let v = &LIQUID_VIEWS[VIEW.load(Ordering::Relaxed)];
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let checks =
        crate::gallery::lay_out_liquids(&mut g.world.clone(), &cat, v.rows, v.hero, v.view)?;
    let water_plane = v.plane == Some("water");
    let mut wrong = Vec::new();
    let mut judged = 0;
    for (i, ((x, y), _, shows)) in checks.into_iter().enumerate() {
        let seen_on: Vec<Color> = frames
            .iter()
            .filter_map(|f| f.get(i).copied().flatten())
            .collect();
        // (most of the frames at least: a frame may miss one)
        if seen_on.len() * 2 <= SAMPLE_FRAMES {
            continue;
        }
        // (the deep by its darkest frame: something pale passing over a
        // deep cell beside the hero, now and then, read as a bubble's
        // floor; the deep turned pale for good stays pale in every frame)
        let c = match shows {
            Shows::Lava => median(&seen_on),
            Shows::Deep => seen_on
                .iter()
                .copied()
                .min_by(|a, b| a.r.total_cmp(&b.r))
                .unwrap_or(seen_on[0]),
            _ => seen_on[0],
        };
        let spread: Vec<String> = seen_on
            .iter()
            .map(|c| format!("{:.2}/{:.2}/{:.2}", c.r, c.g, c.b))
            .collect();
        godot_print!(
            "selftest: liquids: {} {x},{y} frames {}",
            v.shot,
            spread.join(" ")
        );
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
        ]);
        for _ in 0..SAMPLE_FRAMES {
            steps.extend([
                Step::Wait("a while between samples", apart),
                Step::Call("each cell as drawn now", sample_view),
            ]);
        }
        steps.extend([
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
