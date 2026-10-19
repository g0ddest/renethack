//! Play with a gamepad alone, over the map (`pad-play`). The message
//! history is the top layer: Back opens it, the d-pad scrolls it and the
//! hero stays put, B closes it (the game's menu stays shut), A too, and
//! its close names B; F9 and Esc do the same with the keys. The ring
//! held by LT closes when the window loses the focus, and the next pull
//! opens it at once. A on the hero's own cell (on the stairs up, at the
//! start) starts the engine's "What do you want to do?" on looking, never
//! on the stairs.

use godot::global::{JoyAxis, Key as GKey};
use nh_world::Prompt;

use super::{PadEv, Step, command, fail_on_error_screen, start, tap};
use crate::game::RenethackGame;
use crate::gamepad::{PadButton, PadCtx};
use crate::ui_events::UiEvent;
use std::sync::Mutex;

/// The hero's cell before the d-pad pressed under the history.
static HERO: Mutex<Option<(i32, i32)>> = Mutex::new(None);

fn history(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(g.ui.as_ref().ok_or("no UI")?.hud.full_log_open())
}

/// The history shut, the game's menu too: the command prompt.
fn plain(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.hud.full_log_open()
        && !ui.game_menu.is_open()
        && !ui.dialogs.is_open()
        && matches!(g.pending, Some((_, Prompt::Command))))
}

pub(super) fn pad_play() -> Vec<Step> {
    let mut steps = start();
    steps.extend(the_history());
    steps.extend(the_ring());
    steps.extend(here());
    steps
}

/// A on the hero: what can be done here, the focus on looking.
fn here() -> Vec<Step> {
    vec![
        tap(PadButton::A),
        Step::Request("What do you want to do?", |p| {
            matches!(p, Prompt::Menu { title: Some(t), .. } if t == "What do you want to do?")
        }),
        Step::Wait("the focus on looking, not on the stairs", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let Some(i) = ui.dialogs.menu_cursor() else {
                return Ok(false);
            };
            // the engine's own words for the rows
            let Some((_, Prompt::Menu { items, .. })) = g.pending.as_ref() else {
                return Ok(false);
            };
            let rows: Vec<&str> = items.iter().filter_map(|it| it.str.as_deref()).collect();
            if !rows.iter().any(|r| r.starts_with("Go up")) {
                return Err(format!("not on the stairs up: {rows:?}"));
            }
            let row = items.get(i).and_then(|it| it.str.as_deref()).unwrap_or("");
            if row != "Look at what is here" {
                return Err(format!("the focus is on {row:?}"));
            }
            Ok(true)
        }),
        Step::Shot("pad-play-here"),
        tap(PadButton::A),
        Step::Request("a command after looking", command),
        Step::Wait("still on the first level", |g| {
            Ok(g.world.status.get("dlvl").is_none_or(|d| d.trim() == "1") && plain(g)?)
        }),
    ]
}

/// The ring held open by LT, the window's focus lost.
fn the_ring() -> Vec<Step> {
    let lt = |v: f32| Step::Pad(vec![PadEv::Axis(JoyAxis::TRIGGER_LEFT, v)]);
    let ring = |g: &RenethackGame| Ok(g.ui.as_ref().ok_or("no UI")?.pad.radial_shown());
    vec![
        lt(1.0),
        Step::Wait("LT: the ring", ring),
        Step::Call("the window loses the focus", |g| {
            g.push_ui(UiEvent::FocusLost);
            Ok(())
        }),
        Step::Wait("the ring gone", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.pad.radial_shown())
        }),
        lt(0.0),
        lt(1.0),
        Step::Wait("the next pull opens it at once", ring),
        lt(0.0),
        Step::Wait("let go in the middle: nothing", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.pad.radial_shown() && plain(g)?)
        }),
    ]
}

/// The history over the map, with the pad and with the keys.
fn the_history() -> Vec<Step> {
    vec![
        tap(PadButton::Back),
        Step::Wait("Back: the history, read and closed by the pad", |g| {
            if !history(g)? {
                return Ok(false);
            }
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let close = ui.hud.full_log_close_words();
            if close.contains("F9") {
                return Err(format!("its close says {close:?}"));
            }
            *HERO.lock().map_err(|e| e.to_string())? = g.world.hero();
            Ok(close.contains('B') && ui.pad.shown().is_some_and(|(_, c)| c == PadCtx::Read))
        }),
        Step::Shot("pad-play-history"),
        tap(PadButton::Down),
        tap(PadButton::Left),
        Step::Wait("the d-pad under the history: the hero stays", |g| {
            if !history(g)? {
                return Err("the d-pad closed the history".into());
            }
            let was = *HERO.lock().map_err(|e| e.to_string())?;
            if g.world.hero() != was {
                return Err(format!("the hero walked: {was:?} -> {:?}", g.world.hero()));
            }
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        tap(PadButton::B),
        Step::Wait("B closes the history, no menu", plain),
        tap(PadButton::Back),
        Step::Wait("the history again", history),
        tap(PadButton::A),
        Step::Wait("A closes it", plain),
        // and with the keys
        Step::Press(GKey::F9, '\0', false),
        Step::Wait("F9: the history, its close says F9", |g| {
            if !history(g)? {
                return Ok(false);
            }
            let close = g.ui.as_ref().ok_or("no UI")?.hud.full_log_close_words();
            Ok(close.contains("F9"))
        }),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Wait("Esc closes it, no menu", plain),
    ]
}
