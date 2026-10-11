//! `first-open`: the inventory panel and the command palette opened for the
//! first time in a session, and again, checked as the frame each opens in
//! is drawn (when the next frame begins: `at_next_frame`). The panel shows
//! the hero's own pack, not the sample of the warm-up it was drawn with
//! last; the palette's list lies in its frame at the height it is made to
//! show. A picture taken in the middle of that frame showed both wrong:
//! controls are redrawn, and containers laid out, at the frame's end.

use std::sync::atomic::{AtomicU32, Ordering};

use godot::classes::{Engine, SceneTree};
use godot::prelude::*;
use nh_world::Prompt;

use super::{Step, at_next_frame, camera_settled, command, key, next_frame_failed, quit, start};
use crate::game::RenethackGame;
use crate::inventory_panel::InvInput;
use crate::ui_events::{DialogEvent, UiEvent};

static FRAMES: AtomicU32 = AtomicU32::new(0);
/// The opening checked now: the first or the second.
static ROUND: AtomicU32 = AtomicU32::new(0);

fn frames(what: &'static str) -> Step {
    Step::Wait(what, |_| {
        if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 < 12 {
            return Ok(false);
        }
        FRAMES.store(0, Ordering::Relaxed);
        Ok(true)
    })
}

fn which() -> &'static str {
    if ROUND.load(Ordering::Relaxed) == 0 {
        "the first"
    } else {
        "a second"
    }
}

/// The game, between two frames.
fn game_now() -> Option<Gd<RenethackGame>> {
    Engine::singleton()
        .get_main_loop()?
        .try_cast::<SceneTree>()
        .ok()?
        .get_current_scene()?
        .try_cast::<RenethackGame>()
        .ok()
}

/// As the opening frame is drawn: the hero's pack in the grid, nothing
/// picked (the warm-up's sample had its long sword picked).
fn pack_drawn(round: &'static str) {
    let Some(game) = game_now() else {
        return next_frame_failed("no game".into());
    };
    let g = game.bind();
    let Some(ui) = g.ui.as_ref() else {
        return next_frame_failed("no UI".into());
    };
    let mut drawn = ui.inventory.cell_letters();
    let mut pack: Vec<String> = g
        .world
        .inventory
        .items()
        .iter()
        .map(|i| i.letter.to_string())
        .collect();
    drawn.sort();
    pack.sort();
    godot_print!("selftest: first-open: {round} opening drawn with {drawn:?}");
    if drawn != pack {
        next_frame_failed(format!(
            "{round} opening of the inventory is drawn with {drawn:?}, the pack is {pack:?}"
        ));
    } else if let Some(l) = ui.inventory.selected() {
        next_frame_failed(format!(
            "{round} opening of the inventory is drawn with {l} picked"
        ));
    }
}

/// As the opening frame is drawn: the palette's list in its frame, as tall
/// as it is made to be.
fn palette_drawn(round: &'static str) {
    let Some(game) = game_now() else {
        return next_frame_failed("no game".into());
    };
    let g = game.bind();
    let Some(ui) = g.ui.as_ref() else {
        return next_frame_failed("no UI".into());
    };
    let (Some(panel), Some((list, view_h))) = (ui.dialogs.panel_rect(), ui.dialogs.palette_view())
    else {
        return next_frame_failed(format!("{round} palette is not open as it is drawn"));
    };
    godot_print!("selftest: first-open: {round} palette drawn: list {list:?} in {panel:?}");
    let inside = panel.grow(0.5).encloses(list);
    if !inside || (list.size.y - view_h).abs() > 0.5 {
        next_frame_failed(format!(
            "{round} palette is drawn with its list at {list:?} ({view_h} tall) in a frame at {panel:?}"
        ));
    }
}

pub(super) fn first_open() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the camera on the hero", camera_settled),
        frames("a still screen"),
    ]);
    for _ in 0..2 {
        steps.extend([
            Step::Call("the inventory toggled", |g| {
                g.push_ui(UiEvent::Inventory(InvInput::Toggle));
                Ok(())
            }),
            Step::Wait("the panel open, checked as it is drawn", |g| {
                let open = g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open());
                if open {
                    let round = which();
                    at_next_frame(move || pack_drawn(round))?;
                }
                Ok(open)
            }),
            frames("the panel shown"),
            Step::Shot("first-open-inventory"),
            Step::Call("the inventory closed", |g| {
                g.push_ui(UiEvent::Inventory(InvInput::Close));
                Ok(())
            }),
            frames("the map again"),
            key('#'),
            Step::Wait("the palette open, checked as it is drawn", |g| {
                let open = matches!(g.pending, Some((_, Prompt::ExtCmd)));
                if open {
                    let round = which();
                    at_next_frame(move || palette_drawn(round))?;
                }
                Ok(open)
            }),
            Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
            frames("the palette shown"),
            Step::Shot("first-open-palette"),
            Step::Dialog(DialogEvent::ExtCmd(None)),
            Step::Request("a command after the palette", command),
            frames("the map again"),
            Step::Call("the next opening", |_| {
                ROUND.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(quit());
    steps
}
