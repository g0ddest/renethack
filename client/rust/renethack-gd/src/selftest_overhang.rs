//! `overhang`: a grown red dragon towers over the hero and overhangs its
//! cell (`nh_art::footprint_cap`), and a jackal two cells north of it,
//! which its body hides from the camera, is drawn through it in the
//! x-ray, as a creature behind the hero is. With `--screenshots`, a
//! picture.

use std::sync::atomic::{AtomicI32, Ordering};

use super::{Step, camera_settled, map_view, quit, start};

/// Where the jackal stands.
static JACKAL: (AtomicI32, AtomicI32) = (AtomicI32::new(0), AtomicI32::new(0));

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
            let (x, y) = (
                JACKAL.0.load(Ordering::Relaxed),
                JACKAL.1.load(Ordering::Relaxed),
            );
            Ok(map_view(g)?.seen_through_hero(x, y).is_some())
        }),
        Step::Shot("overhang"),
    ]);
    steps.extend(quit());
    steps
}
