//! `effects`: the corridors' stretch of level (`gallery::lay_out_corridors`)
//! with the effects that read wrong in play: a bolt of lightning across the
//! lit room, blood of three colours fallen on its floor, something unseen
//! remembered in the room and in the corridor's dark, a fountain and an
//! up stair in the light; then the level unlit, an up stair a few steps
//! into the dark, remembered. Pictures only (`--screenshots`): not part of
//! `make test-client`.

use super::{Step, camera_settled, map_view, quit, start};

use crate::gallery::{corridor_cell, lay_out_corridors, put_feature};
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
