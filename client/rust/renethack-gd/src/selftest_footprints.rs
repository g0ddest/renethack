//! No creature outgrows its cell (`footprints`): every monster of the
//! catalog is built and the sides of its model on the floor measured as it
//! stands idle. One drawn much longer than a cell stands in its
//! neighbours' cells, hides the hero it fights and goes through walls; a
//! model that long says how it lies in the manifest (`footprint`) and the
//! size resolution fits it (`nh_art::ModelSpec::fitted`).

use godot::prelude::*;

use super::{Step, quit, start};

/// A little over the longest a fitted body is: the idle sways.
const SLACK: f32 = 1.05;

pub(super) fn footprints() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("every monster within its cell", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let ui = g.ui.as_mut().ok_or("no UI")?;
            let mut all = ui.map.footprints(&cat);
            let long = |sides: &[f32; 2]| sides[0].max(sides[1]);
            all.sort_by(|a, b| long(&b.2).total_cmp(&long(&a.2)));
            let most = nh_art::MAX_FOOTPRINT * (1.0 + nh_art::FOOTPRINT_GIVE);
            // the long ones for the log, with their models' sides in the
            // models' own units (what a manifest `footprint` says)
            let mut over = Vec::new();
            for (name, model, sides, scale) in &all {
                if long(sides) <= 1.0 {
                    break;
                }
                let line = format!(
                    "{name} ({model}) {:.2} x {:.2} m, the model [{:.2}, {:.2}]",
                    sides[0],
                    sides[1],
                    sides[0] / scale,
                    sides[1] / scale
                );
                godot_print!("selftest: footprints: {line}");
                if long(sides) > most * SLACK {
                    over.push(line);
                }
            }
            if over.is_empty() {
                return Ok(());
            }
            Err(format!(
                "longer than {most:.2} m (say the models' `footprint` in the manifest): {}",
                over.join("; ")
            ))
        }),
    ]);
    steps.extend(quit());
    steps
}
