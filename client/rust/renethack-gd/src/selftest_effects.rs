//! `effects`: the corridors' stretch of level (`gallery::lay_out_corridors`)
//! with the effects that read wrong in play: a bolt of lightning across the
//! lit room, blood of three colours fallen on its floor, something unseen
//! remembered in the room and in the corridor's dark, a fountain and an
//! up stair in the light; then the level unlit, an up stair a few steps
//! into the dark, remembered. `rays`: a ray of every kind the engine draws
//! across the lit room, cell by cell. Pictures only (`--screenshots`): not
//! part of `make test-client`.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Step, camera_settled, map_view, quit, start};

use crate::gallery::{
    CorridorScene, corridor_cell, lay_out_corridors, lay_out_scene, put_feature, put_ray,
};
use crate::game::RenethackGame;
use crate::vfx::VfxKind;

/// The corridors' scene with something unseen in it, and the one in the
/// east room's doorway (`CORRIDOR_SCENES`).
const SCENE: usize = 11;
const DOORWAY: usize = 5;

fn ui_map(g: &mut RenethackGame) -> Result<&mut crate::map_view::MapView, String> {
    Ok(&mut g.ui.as_mut().ok_or("no UI")?.map)
}

pub(super) fn effects() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("lay out the corridors", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            lay_out_corridors(&mut g.world, &cat, SCENE, false)?;
            put_feature(&mut g.world, &cat, "S_fountain", (4, 3))?;
            put_feature(&mut g.world, &cat, "S_upstair", (3, 5))?;
            ui_map(g)?.set_showcase(false);
            Ok(())
        }),
        Step::Wait("the stretch drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("effects-unseen"),
        Step::Call("blood on the floor", |g| {
            let map = ui_map(g)?;
            for (at, color) in [
                ((5, 4), godot::prelude::Color::from_rgb(0.8, 0.08, 0.06)),
                ((6, 5), godot::prelude::Color::from_rgb(0.75, 0.1, 0.08)),
                ((8, 5), godot::prelude::Color::from_rgb(0.3, 0.7, 0.15)),
            ] {
                let (x, y) = corridor_cell(at);
                map.vfx().burst(
                    VfxKind::Blood(color),
                    godot::prelude::Vector3::new(x as f32, 0.6, y as f32),
                );
            }
            Ok(())
        }),
        Step::Wait("the drops fallen", |g| Ok(map_view(g)?.splats_shown() >= 3)),
        Step::Shot("effects-blood"),
        Step::Call("a bolt of lightning across the room", |g| {
            let (from, to) = (corridor_cell((8, 3)), corridor_cell((3, 3)));
            ui_map(g)?.beam_between(VfxKind::Lightning, from, to);
            Ok(())
        }),
        Step::Shot("effects-lightning"),
        Step::Call("another strike", |g| {
            let (from, to) = (corridor_cell((8, 3)), corridor_cell((4, 2)));
            ui_map(g)?.beam_between(VfxKind::Lightning, from, to);
            Ok(())
        }),
        Step::Shot("effects-lightning-2"),
        Step::Call("the level unlit, an up stair in the dark", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            lay_out_corridors(&mut g.world, &cat, DOORWAY, true)?;
            put_feature(&mut g.world, &cat, "S_upstair", (20, 10))?;
            Ok(())
        }),
        Step::Wait("the dark level drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on the hero again", camera_settled),
        Step::Shot("effects-upstair-remembered"),
    ]);
    steps.extend(quit());
    steps
}

/// The west room of the corridors, the hero in its south-east corner and
/// a jackal by the west wall, out of the rays' way.
const RAYS_SCENE: CorridorScene = CorridorScene {
    name: "rays",
    hero: (9, 5),
    others: &[("jackal", (3, 3))],
};

/// A ray drawn in `rays`: its picture's name, NetHack's beam type (`ZT_*`;
/// None: a wand of digging's beam) and the cells it crosses in turn.
struct Drawn {
    shot: &'static str,
    /// Its pictures as it fades, and once it is gone: what it shed in
    /// the air.
    after: &'static str,
    later: &'static str,
    beam: Option<i32>,
    path: &'static [(i32, i32)],
}

/// West along the room's south row, from the hero to the wall.
const WEST: &[(i32, i32)] = &[(8, 5), (7, 5), (6, 5), (5, 5), (4, 5), (3, 5)];
/// North-west from the hero, aslant.
const ASLANT: &[(i32, i32)] = &[(8, 4), (7, 3), (6, 2)];

const RAYS: [Drawn; 10] = [
    Drawn {
        shot: "rays-magic-missile",
        after: "rays-magic-missile-after",
        later: "rays-magic-missile-later",
        beam: Some(0),
        path: WEST,
    },
    Drawn {
        shot: "rays-fire",
        after: "rays-fire-after",
        later: "rays-fire-later",
        beam: Some(1),
        path: WEST,
    },
    Drawn {
        shot: "rays-cold",
        after: "rays-cold-after",
        later: "rays-cold-later",
        beam: Some(2),
        path: WEST,
    },
    Drawn {
        shot: "rays-sleep",
        after: "rays-sleep-after",
        later: "rays-sleep-later",
        beam: Some(3),
        path: WEST,
    },
    Drawn {
        shot: "rays-death",
        after: "rays-death-after",
        later: "rays-death-later",
        beam: Some(4),
        path: WEST,
    },
    Drawn {
        shot: "rays-lightning",
        after: "rays-lightning-after",
        later: "rays-lightning-later",
        beam: Some(5),
        path: WEST,
    },
    Drawn {
        shot: "rays-poison",
        after: "rays-poison-after",
        later: "rays-poison-later",
        beam: Some(6),
        path: WEST,
    },
    Drawn {
        shot: "rays-acid",
        after: "rays-acid-after",
        later: "rays-acid-later",
        beam: Some(7),
        path: WEST,
    },
    Drawn {
        shot: "rays-fire-aslant",
        after: "rays-fire-aslant-after",
        later: "rays-fire-aslant-later",
        beam: Some(1),
        path: ASLANT,
    },
    Drawn {
        shot: "rays-digging",
        after: "rays-digging-after",
        later: "rays-digging-later",
        beam: None,
        path: WEST,
    },
];

/// The ray of `RAYS` under way, and its next cell (steps are plain
/// functions).
static RAY: AtomicUsize = AtomicUsize::new(0);
static PIECE: AtomicUsize = AtomicUsize::new(0);

fn drawn_now(g: &RenethackGame) -> bool {
    let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
    drawn == Some(g.world.map.generation())
}

/// The last piece put on the map is drawn.
fn piece_drawn(g: &RenethackGame) -> Result<bool, String> {
    let ray = &RAYS[RAY.load(Ordering::Relaxed)];
    let put = PIECE.load(Ordering::Relaxed);
    let at = *ray.path.get(put.wrapping_sub(1)).ok_or("no piece put")?;
    Ok(map_view(g)?.ray_drawn_at(corridor_cell(at)))
}

/// The last ray is gone from the map (its effects may linger).
fn ray_cleared(g: &RenethackGame) -> Result<bool, String> {
    let last = RAY
        .load(Ordering::Relaxed)
        .checked_sub(1)
        .ok_or("no ray yet")?;
    let map = map_view(g)?;
    Ok(RAYS[last]
        .path
        .iter()
        .all(|&at| !map.ray_drawn_at(corridor_cell(at))))
}

/// The next cell of the ray under way: its piece put on the map.
fn next_piece(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let ray = &RAYS[RAY.load(Ordering::Relaxed)];
    let i = PIECE.fetch_add(1, Ordering::Relaxed);
    let at = *ray.path.get(i).ok_or("past the ray's end")?;
    let (first, second) = (ray.path[0], ray.path[1]);
    let step = (second.0 - first.0, second.1 - first.1);
    put_ray(&mut g.world, &cat, ray.beam, step, at)
}

/// `rays`: the west room of the corridors, lit, and a ray of each kind
/// the engine draws crossing it from the hero cell by cell, a frame a
/// cell as in a game (the map's own glyphs, so each is what its glyph
/// says): magic missile, fire, cold, sleep, death, lightning, poison gas,
/// acid, a ray of fire aslant and a wand of digging's beam. Each must be
/// one ray, however many cells it crosses; a picture of each as its last
/// cell is drawn. Then the rings of force the hero's own zap sends where
/// the engine draws no ray. Pictures only (`--screenshots`): not part of
/// `make test-client`.
pub(super) fn rays() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("lay out the room", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            lay_out_scene(&mut g.world, &cat, &RAYS_SCENE, false)?;
            ui_map(g)?.set_showcase(false);
            RAY.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the room drawn", |g| Ok(drawn_now(g))),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    for ray in &RAYS {
        steps.push(Step::Call("a new ray", |_| {
            PIECE.store(0, Ordering::Relaxed);
            Ok(())
        }));
        for _ in ray.path {
            steps.extend([
                Step::Call("the ray's next cell", next_piece),
                Step::Wait("the cell drawn", piece_drawn),
            ]);
        }
        steps.extend([
            Step::Call("one ray", |g| match map_view(g)?.rays_shown().0 {
                1 => Ok(()),
                n => Err(format!("{n} rays shown for one")),
            }),
            Step::Shot(ray.shot),
            Step::Wait("the ray fading", |g| {
                Ok(map_view(g)?.rays_shown().1.is_none_or(|age| age > 0.3))
            }),
            Step::Shot(ray.after),
            Step::Wait("the ray gone, what it shed in the air", |g| {
                Ok(map_view(g)?.rays_shown().0 == 0)
            }),
            Step::Shot(ray.later),
            Step::Call("the ray gone from the map", |g| {
                let cat = g.catalog.clone().ok_or("no catalog")?;
                let ray = &RAYS[RAY.load(Ordering::Relaxed)];
                for &at in ray.path {
                    put_feature(&mut g.world, &cat, "S_room", at)?;
                }
                RAY.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
            Step::Wait("the room drawn again", ray_cleared),
            Step::Wait("all it shed gone", |g| Ok(!map_view(g)?.effects_busy())),
        ]);
    }
    steps.extend([
        Step::Call("the hero's rings of force", |g| {
            let (from, to) = (corridor_cell((9, 5)), corridor_cell((3, 5)));
            let color = godot::prelude::Color::from_rgb(0.5, 0.7, 1.0);
            ui_map(g)?.beam_between(VfxKind::Shock(color), from, to);
            Ok(())
        }),
        Step::Wait("the rings under way", |g| {
            Ok(map_view(g)?.rays_shown().1.is_some_and(|age| age > 0.12))
        }),
        Step::Shot("rays-force"),
    ]);
    steps.extend(quit());
    steps
}
