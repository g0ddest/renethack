//! `overhang`: a grown red dragon towers over the hero and overhangs its
//! cell (`nh_art::footprint_cap`), and still keeps to it where a player
//! looks: a jackal two cells north of it, which its body hides from the
//! camera, is drawn through it in the x-ray as a creature behind the hero
//! is; the pointer on the jackal, and on the cell beside the dragon under
//! its wing, picks those cells and not the dragon's; and with an arrow trap and a dagger
//! beside the jackal, which it hides too, the dragon is drawn
//! see-through. With `--screenshots`, pictures of both.

use std::sync::atomic::{AtomicI32, Ordering};

use godot::prelude::*;

use super::{Step, camera_settled, map_view, quit, start};
use crate::gallery::Entry;

/// Where the jackal stands (the dragon two cells south).
static JACKAL: (AtomicI32, AtomicI32) = (AtomicI32::new(0), AtomicI32::new(0));

fn jackal() -> (i32, i32) {
    (
        JACKAL.0.load(Ordering::Relaxed),
        JACKAL.1.load(Ordering::Relaxed),
    )
}

pub(super) fn overhang() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("a jackal behind a red dragon", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let rows: [(i32, &[&str]); 2] = [(2, &["jackal"]), (2, &["red dragon"])];
            let (x, y) = crate::gallery::lay_out_rows(&mut g.world, &cat, &rows)?;
            // the rows two apart, the first a row north of the middle
            JACKAL.0.store(x, Ordering::Relaxed);
            JACKAL.1.store(y - 1, Ordering::Relaxed);
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_showcase(true);
            Ok(())
        }),
        Step::Wait("the rows drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on them", camera_settled),
        Step::Wait("the jackal drawn through the dragon", |g| {
            let (x, y) = jackal();
            Ok(map_view(g)?.seen_through_hero(x, y).is_some())
        }),
        Step::Call("the pointer picks by cell, not by the dragon", |g| {
            let map = map_view(g)?;
            let (x, y) = jackal();
            // the jackal behind it, and the cell beside it under its wing
            for (cell, h) in [((x, y), 0.25), ((x - 1, y + 2), 0.05)] {
                let at = Vector3::new(cell.0 as f32, h, cell.1 as f32);
                let p = map.on_screen(at).ok_or("off the screen")?;
                let picked = map.cell_at(p);
                if picked != Some(cell) {
                    return Err(format!("the pointer at {cell:?} picks {picked:?}"));
                }
            }
            Ok(())
        }),
        Step::Shot("overhang"),
        Step::Call("an arrow trap and a dagger beside the jackal", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let (x, y) = jackal();
            crate::gallery::put_entry(
                &mut g.world,
                &cat,
                (x - 1, y),
                Entry::Feature("S_arrow_trap"),
            )?;
            crate::gallery::put_entry(&mut g.world, &cat, (x + 1, y), Entry::Obj(")", "dagger"))?;
            Ok(())
        }),
        Step::Wait("the dragon see-through over them", |g| {
            Ok(map_view(g)?.big_faded() == 1)
        }),
        Step::Shot("overhang-faded"),
    ]);
    steps.extend(quit());
    steps
}
