//! `shop`: a weapon shop's floor laid out in place of the level, a ware
//! on every cell: every appearance of the weapons' class, the polearms
//! and the lance among the daggers. On a shop's floor each square's ware
//! must be told from its neighbours', so every one must lie on its own
//! cell, however long it is. With `--screenshots`, pictures of the floor
//! (half of it lit, half only remembered: steel must read as steel in
//! both). RENETHACK_SHOP_LOOK=1: pictures only (those of another build).

use super::{Step, camera_settled, map_view, quit, start};
use godot::prelude::*;

use crate::gallery::{lay_out_shop, shop_cells};

/// A ware may reach this far over its cell's edge (metres).
const SLACK: f32 = 0.02;

pub(super) fn shop() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("lay out the shop's floor", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            lay_out_shop(&mut g.world, &cat)?;
            g.ui.as_mut().ok_or("no UI")?.map.set_showcase(false);
            Ok(())
        }),
        Step::Wait("the floor drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on the room", camera_settled),
        Step::Call("every ware on its own cell", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let map = map_view(g)?;
            let wares = cat.object_tiles.iter().filter(|t| t.class == ")");
            let mut over = Vec::new();
            let mut most: f32 = 0.0;
            for (t, cell) in wares.zip(shop_cells(&cat)) {
                let by = map.overhang(cell);
                most = most.max(by);
                if by > SLACK {
                    over.push(format!("{} by {by:.2} m", t.appearance));
                }
            }
            godot_print!(
                "selftest: shop: {} wares, the longest reaches {most:.2} m over its cell",
                shop_cells(&cat).len()
            );
            if over.is_empty() || std::env::var_os("RENETHACK_SHOP_LOOK").is_some() {
                return Ok(());
            }
            Err(format!(
                "wares lie across their neighbours' cells: {}",
                over.join(", ")
            ))
        }),
        Step::Shot("shop"),
        Step::Call("closer", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.zoom(-10.0);
            Ok(())
        }),
        Step::Wait("the camera closer", camera_settled),
        Step::Shot("shop-close"),
    ]);
    steps.extend(quit());
    steps
}
