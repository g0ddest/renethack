//! `;` answers by the cell (`look-tip`): the cursor walked to the pet and
//! picked with `.`, no help window before it; the engine's answer shows in
//! a tip over the pet, without the tty's map symbol it starts with, and
//! stays in the log, without it too. The next command takes the tip away. A click
//! picks a cell as well (the hero's own): the tip goes over the hero.

use std::sync::Mutex;

use godot::prelude::*;
use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt, is_look_line, look_words};

use super::{Step, command, key, quit, start};
use crate::game::RenethackGame;
use crate::i18n;
use crate::ui_events::UiEvent;

/// The cell looked at.
static AT: Mutex<Option<(i32, i32)>> = Mutex::new(None);

fn at() -> Result<(i32, i32), String> {
    AT.lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no cell to look at".to_string())
}

/// The pet's cell, if the map shows it.
fn pet(g: &RenethackGame) -> Option<(i32, i32)> {
    let (hx, hy) = g.world.map.hero()?;
    (hy - 3..=hy + 3)
        .flat_map(|y| (hx - 3..=hx + 3).map(move |x| (x, y)))
        .find(|&(x, y)| {
            g.world
                .map
                .cell(x, y)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|gl| gl.kind == GlyphKind::Mon && gl.flags & mg::PET != 0)
        })
}

/// getpos's cursor waits.
fn getpos(g: &RenethackGame) -> Result<bool, String> {
    Ok(matches!(g.pending, Some((_, Prompt::Command))) && g.world.getpos)
}

/// The tip over the cell looked at says what the log's newest look line
/// says, without its symbol, near where the cell is on screen.
fn tip_over_the_cell(g: &RenethackGame) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((text, rect)) = ui.hud.look_tip() else {
        return Ok(false);
    };
    let line = g
        .world
        .log
        .iter()
        .rev()
        .find(|m| is_look_line(&m.text))
        .ok_or("the answer is not in the log")?;
    let logged = i18n::engine_message(line);
    let want = look_words(&logged);
    if text != want {
        return Err(format!("the tip says {text:?}, the log {logged:?}"));
    }
    let sym = line.text.chars().next().unwrap_or(' ');
    if text.strip_prefix(sym).is_some_and(|t| t.starts_with(' ')) {
        return Err(format!("the tip starts with the map symbol: {text:?}"));
    }
    // the log's line says the same, no symbol either
    let shown = ui.hud.log_shown();
    let last = shown
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    if last.trim() != text {
        return Ok(false);
    }
    let (x, y) = at()?;
    let cell = ui
        .map
        .on_screen(Vector3::new(x as f32, 0.0, y as f32))
        .ok_or("the cell is behind the camera")?;
    // over it (or under it), its middle the cell's unless pulled in
    // from the screen's edge
    let view = g
        .base()
        .get_viewport()
        .ok_or("no viewport")?
        .get_visible_rect()
        .size;
    let pulled = rect.position.x <= 9.0 || rect.end().x >= view.x - 9.0;
    if !pulled && !(rect.position.x..=rect.end().x).contains(&cell.x) {
        return Err(format!("the tip {rect:?} is not over the cell at {cell:?}"));
    }
    if rect.end().y < cell.y - 400.0 || rect.position.y > cell.y + 200.0 {
        return Err(format!("the tip {rect:?} is far from the cell at {cell:?}"));
    }
    Ok(true)
}

pub(super) fn look_tip() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the pet beside the hero", |g| {
            let p = pet(g);
            *AT.lock().map_err(|e| e.to_string())? = p;
            Ok(p.is_some())
        }),
        key(';'),
        // !tips: no window on how to move the cursor first
        Step::Request("getpos's cursor, no help window first", command),
        Step::Wait("getpos", getpos),
        Step::Shot("look-cursor"),
        Step::KeysUntil("the cursor walked to the pet and picked", |g| {
            if !getpos(g)? {
                return Ok(None);
            }
            let target = at()?;
            let (cx, cy) = g.world.cursor.ok_or("no cursor")?;
            if (cx, cy) == target {
                return Ok(Some(KeyInput::plain(Key::Char('.'))));
            }
            let d = ((target.0 - cx).signum(), (target.1 - cy).signum());
            let way = super::audio_tests::STEPS
                .iter()
                .find(|(s, _)| *s == d)
                .ok_or("no way to the pet")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }),
        Step::Wait("the answer over the pet", tip_over_the_cell),
        Step::Shot("look-tip"),
        // the next command takes it away
        key(':'),
        Step::Request("a command after looking here", command),
        Step::Wait("no tip after the next command", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.hud.look_tip().is_none())
        }),
        // a click picks the hero's own cell
        Step::Wait("the hero", |g| {
            let h = g.world.map.hero();
            *AT.lock().map_err(|e| e.to_string())? = h;
            Ok(h.is_some())
        }),
        key(';'),
        Step::Request("getpos's cursor", command),
        Step::Wait("getpos", getpos),
        Step::Call("a click on the hero", |g| {
            let (x, y) = at()?;
            g.push_ui(UiEvent::MapClick { x, y, button: 1 });
            Ok(())
        }),
        Step::Request("a command after the click", command),
        Step::Wait("the answer over the hero", tip_over_the_cell),
        Step::Shot("look-tip-hero"),
    ]);
    steps.extend(quit());
    steps
}
