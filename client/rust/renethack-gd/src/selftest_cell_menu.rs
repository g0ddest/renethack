//! What can be done at a cell, at the cell (`cell-menu`): a right click on
//! the hero opens the engine's menu as a popup by the hero, what is known
//! there on top, and a click on "Look at what is here" looks. A right
//! click on the pet (or a cell beside the hero) opens its own by it; Esc
//! cancels; a click beside the popup cancels too. Under a pad: A on the
//! hero, the focus on looking, A looks; the radial's «Здесь», B cancels.

use godot::classes::{Input, InputEventMouseButton};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_world::Prompt;

use super::{Step, command, idle_command, pet_dir, quit, radial, start, tap};
use crate::cell_menu::{LOOK_HERE, TITLE};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;

fn ui(g: &RenethackGame) -> Result<&crate::game::Ui, String> {
    g.ui.as_ref().ok_or_else(|| "no UI".to_string())
}

/// Cell (`x`, `y`)'s floor on the canvas.
fn cell_at(g: &RenethackGame, x: i32, y: i32) -> Result<Vector2, String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let cam = vp.get_camera_3d().ok_or("no camera")?;
    Ok(cam.unproject_position(Vector3::new(x as f32, 0.3, y as f32)))
}

/// A click of `b` at `at` on the canvas.
fn click(g: &RenethackGame, b: MouseButton, at: Vector2) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    for pressed in [true, false] {
        let mut ev = InputEventMouseButton::new_gd();
        ev.set_button_index(b);
        ev.set_position(at);
        ev.set_global_position(at);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
    Ok(())
}

/// The cell the second right click is on: the pet beside the hero, else
/// the one east of the hero.
fn other_cell(g: &RenethackGame) -> Result<(i32, i32), String> {
    let (hx, hy) = g.world.hero().ok_or("no hero")?;
    let (dx, dy) = pet_dir(g).unwrap_or((1, 0));
    Ok((hx + dx, hy + dy))
}

/// The engine's cell menu asked, open as the popup by `cell` (the popup
/// near the cell, not in the screen's middle), its rows the engine's.
fn popup_by(g: &RenethackGame, cell: (i32, i32)) -> Result<bool, String> {
    let Some((_, Prompt::Menu { title, items, .. })) = g.pending.as_ref() else {
        return Ok(false);
    };
    if title.as_deref() != Some(TITLE) {
        return Ok(false);
    }
    let d = &ui(g)?.dialogs;
    if d.kind_name() != Some("cellmenu") {
        return Ok(false);
    }
    let Some(panel) = d.panel_rect().filter(|r| r.size.y > 40.0) else {
        return Ok(false);
    };
    let at = cell_at(g, cell.0, cell.1)?;
    let near = panel.grow(60.0).contains_point(at);
    if !near {
        return Err(format!("the popup at {panel:?}, the cell at {at:?}"));
    }
    let rows = d.cell_menu_rows().unwrap_or_default();
    let selectable = items.iter().filter(|i| i.selectable).count();
    if rows.len() != selectable {
        return Err(format!("{} rows for {selectable} entries", rows.len()));
    }
    // laid out (a box sorts its rows at a frame's end): one under another
    let laid = rows
        .windows(2)
        .all(|w| w[1].1.position.y >= w[0].1.end().y - 0.5);
    Ok(laid)
}

/// The popup's row of the engine's entry `words`, on screen.
fn row_of(g: &RenethackGame, words: &str) -> Result<Rect2, String> {
    let Some((_, Prompt::Menu { items, .. })) = g.pending.as_ref() else {
        return Err("no menu".into());
    };
    let i = (items.iter())
        .position(|it| it.str.as_deref() == Some(words))
        .ok_or_else(|| format!("no {words:?}"))?;
    let rows = ui(g)?.dialogs.cell_menu_rows().unwrap_or_default();
    rows.into_iter()
        .find(|(item, _)| *item == i)
        .map(|(_, r)| r)
        .ok_or_else(|| format!("no row for {words:?}"))
}

pub(super) fn cell_menu() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the player's turn", idle_command),
        // a right click on the hero: the popup by the hero
        Step::Call("a right click on the hero", |g| {
            let (x, y) = g.world.hero().ok_or("no hero")?;
            click(g, MouseButton::RIGHT, cell_at(g, x, y)?)
        }),
        Step::Request(
            "what can be done here",
            |p| matches!(p, Prompt::Menu { title: Some(t), .. } if t == TITLE),
        ),
        Step::Wait("the popup by the hero, the engine's rows", |g| {
            let cell = g.world.hero().ok_or("no hero")?;
            popup_by(g, cell)
        }),
        Step::Shot("cell-menu-hero"),
        Step::Call("a click on looking", |g| {
            click(g, MouseButton::LEFT, row_of(g, LOOK_HERE)?.center())
        }),
        Step::Request("a command after looking", command),
        // the pet's (or the next cell's) by it; Esc cancels
        Step::Wait("the player's turn", idle_command),
        Step::Call("a right click on the pet", |g| {
            let (x, y) = other_cell(g)?;
            click(g, MouseButton::RIGHT, cell_at(g, x, y)?)
        }),
        Step::Request(
            "what can be done there",
            |p| matches!(p, Prompt::Menu { title: Some(t), .. } if t == TITLE),
        ),
        Step::Wait("the popup by that cell", |g| {
            let cell = other_cell(g)?;
            popup_by(g, cell)
        }),
        Step::Shot("cell-menu-pet"),
        Step::Press(godot::global::Key::ESCAPE, '\0', false),
        Step::Request("Esc: a command again", command),
        // a click beside the popup cancels
        Step::Wait("the player's turn", idle_command),
        Step::Call("a right click on the hero", |g| {
            let (x, y) = g.world.hero().ok_or("no hero")?;
            click(g, MouseButton::RIGHT, cell_at(g, x, y)?)
        }),
        Step::Request(
            "what can be done here",
            |p| matches!(p, Prompt::Menu { title: Some(t), .. } if t == TITLE),
        ),
        Step::Wait("the popup", |g| {
            let cell = g.world.hero().ok_or("no hero")?;
            popup_by(g, cell)
        }),
        Step::Call("a click far beside it", |g| {
            let panel = ui(g)?.dialogs.panel_rect().ok_or("no popup")?;
            let vp = g.base().get_viewport().ok_or("no viewport")?;
            let size = vp.get_visible_rect().size;
            // the other half of the screen, above the HUD
            let x = if panel.center().x > size.x / 2.0 {
                size.x * 0.15
            } else {
                size.x * 0.85
            };
            click(g, MouseButton::LEFT, Vector2::new(x, size.y * 0.3))
        }),
        Step::Request("a click beside it: a command again", command),
        // a pad: A on the hero, the focus on looking, A looks
        Step::Wait("the player's turn", idle_command),
        tap(PadButton::A),
        Step::Request(
            "A on the hero: what can be done here",
            |p| matches!(p, Prompt::Menu { title: Some(t), .. } if t == TITLE),
        ),
        Step::Wait("the popup, the pad's focus on looking", |g| {
            let cell = g.world.hero().ok_or("no hero")?;
            if !popup_by(g, cell)? {
                return Ok(false);
            }
            let Some((_, Prompt::Menu { items, .. })) = g.pending.as_ref() else {
                return Ok(false);
            };
            let at = ui(g)?.dialogs.menu_cursor();
            let row = at
                .and_then(|i| items.get(i))
                .and_then(|it| it.str.as_deref());
            Ok(row == Some(LOOK_HERE))
        }),
        Step::Shot("cell-menu-pad"),
        tap(PadButton::A),
        Step::Request("A: a command after looking", command),
    ]);
    // the radial's «Здесь»: the same popup; B cancels
    steps.extend(radial(0));
    steps.extend([
        Step::Request(
            "«Здесь»: what can be done here",
            |p| matches!(p, Prompt::Menu { title: Some(t), .. } if t == TITLE),
        ),
        Step::Wait("the popup", |g| {
            Ok(ui(g)?.dialogs.kind_name() == Some("cellmenu"))
        }),
        tap(PadButton::B),
        Step::Request("B: a command again", command),
    ]);
    steps.extend(quit());
    steps
}
