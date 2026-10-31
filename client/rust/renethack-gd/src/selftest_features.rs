//! `features`: a throne, a sink, a fountain and an altar in a row, with
//! a patch of ice and a pool beside them and the hero for scale, close
//! and at the game's distance; then a cell of ice, of water and of lava
//! alone in the floor the same way: sheets of the map's features and
//! ground before and after a change of their look. Nothing is checked
//! but that they are drawn; with `--screenshots`, the pictures
//! `features-close`, `features`, `features-lone-close` and
//! `features-lone`.

use super::{Step, camera_settled, quit, start};

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
    steps.extend(quit());
    steps
}
