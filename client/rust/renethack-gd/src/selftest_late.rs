//! `late-monsters`: the big monsters of the late game under the gallery's
//! light, dragons in one picture and the titan, the giants, the mumak, the
//! mastodon and the purple worm in another, hides and coats in a third
//! (the owlbear, trolls, dark dragons), each at the default distance and
//! close up, for sheets before and after a change of their look. Nothing
//! is checked but that each is drawn; with `--screenshots`, the pictures
//! `late-dragons`, `late-giants`, `late-hides` and their `-close`.

use super::{Step, camera_settled, quit, start, zoom_by};
use crate::game::RenethackGame;

/// The rows of each picture: the gap between those of a row, and who.
type Rows = &'static [(i32, &'static [&'static str])];

/// Lays a picture's rows out.
type Lay = fn(&mut RenethackGame) -> Result<(), String>;

const DRAGONS: Rows = &[
    (3, &["red dragon", "black dragon", "silver dragon"]),
    (3, &["gold dragon", "blue dragon", "white dragon"]),
];

const GIANTS: Rows = &[
    (3, &["titan", "storm giant", "giant"]),
    (3, &["mumak", "mastodon", "purple worm"]),
];

const HIDES: Rows = &[
    (3, &["owlbear", "troll", "ice troll"]),
    (3, &["black dragon", "green dragon", "Olog-hai"]),
];

pub(super) fn late_monsters() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    let pictures: [(Lay, &str, &str); 3] = [
        (
            |g| lay_out(g, DRAGONS),
            "late-dragons",
            "late-dragons-close",
        ),
        (|g| lay_out(g, GIANTS), "late-giants", "late-giants-close"),
        (|g| lay_out(g, HIDES), "late-hides", "late-hides-close"),
    ];
    for (rows, far, close) in pictures {
        steps.extend([
            Step::Call("lay out the rows", rows),
            Step::Wait("the rows drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on them", camera_settled),
            Step::Shot(far),
            Step::Call("closer", |g| zoom_by(g, -3.0)),
            Step::Wait("the camera closer", camera_settled),
            Step::Shot(close),
            Step::Call("back", |g| zoom_by(g, 3.0)),
        ]);
    }
    steps.extend(quit());
    steps
}

fn lay_out(g: &mut RenethackGame, rows: Rows) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    crate::gallery::lay_out_rows(&mut g.world, &cat, rows)?;
    g.ui.as_mut().ok_or("no UI")?.map.set_showcase(true);
    Ok(())
}
