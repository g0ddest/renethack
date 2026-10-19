//! Giving up (`give-up`): Esc at the command prompt opens the game's menu.
//! A gamepad's B opens it too, its d-pad goes down to "Give up" and A
//! takes it: the engine's own "Really quit without saving?" comes, a
//! warning under it, and its default (no) lets the game go on, no turn
//! gone. A letter on the menu takes nothing. Then "Save and leave" from
//! the menu: the title offers the game to continue; continued, given up
//! with the keyboard, confirmed: the disclosure's questions, the tombstone,
//! the title again, and the save is gone.

use std::sync::atomic::{AtomicI64, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{
    PadEv, Step, command, continue_label, fail_on_error_screen, key, screen, start, welcome_back,
};
use crate::game::{GameState, RenethackGame};
use crate::game_menu::GameMenuPick;
use crate::gamepad::PadButton;
use crate::tr;
use crate::ui_events::UiEvent;

/// The turn before the menu opened.
static TURN: AtomicI64 = AtomicI64::new(-1);

fn pad(b: PadButton) -> Step {
    Step::Pad(vec![PadEv::Button(b, true), PadEv::Button(b, false)])
}

/// The menu is open, its entries in the language now, the focus on `at`.
fn menu_at(g: &RenethackGame, at: usize) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if !ui.game_menu.is_open() {
        return Ok(false);
    }
    let (words, focus) = ui.game_menu.entries();
    // the game's own column first, the panels after it
    let want = [
        tr!("game-menu-resume"),
        tr!("game-menu-settings"),
        tr!("game-menu-save"),
        tr!("game-menu-give-up"),
        tr!("spells-title"),
    ];
    if words.get(..want.len()) != Some(&want[..]) {
        return Err(format!("the menu says {words:?}"));
    }
    Ok(focus == at)
}

fn menu_closed(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.game_menu.is_open()
        && !ui.dialogs.is_open()
        && matches!(g.pending, Some((_, Prompt::Command))))
}

/// The engine asks whether to give up, the warning under its question.
fn asks_to_quit(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let asked = matches!(
        &g.pending,
        Some((_, Prompt::Choice { query, .. })) if query.starts_with("Really quit")
    );
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(asked && ui.dialogs.is_open() && ui.dialogs.panel_texts().contains(&tr!("quit-warning")))
}

fn turn(g: &RenethackGame) -> i64 {
    g.world.turn().unwrap_or(-1)
}

pub(super) fn give_up() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('l'),
        Step::Request("a command after a step", command),
        Step::Call("the turn", |g| {
            TURN.store(turn(g), Ordering::Relaxed);
            Ok(())
        }),
        // a gamepad: B, the d-pad down to "Give up", A
        pad(PadButton::B),
        Step::Wait("the game's menu, on going back", |g| menu_at(g, 0)),
        Step::Shot("give-up-menu"),
        pad(PadButton::Down),
        pad(PadButton::Down),
        pad(PadButton::Down),
        Step::Wait("the focus on giving up", |g| menu_at(g, 3)),
        pad(PadButton::A),
        Step::Wait("the engine's question, the warning under it", asks_to_quit),
        Step::Shot("give-up-question"),
        // its default, no: the game goes on, no turn gone
        pad(PadButton::A),
        Step::Wait("the game again, unharmed", |g| {
            if !menu_closed(g)? {
                return Ok(false);
            }
            let was = TURN.load(Ordering::Relaxed);
            if turn(g) != was || g.state != GameState::Playing {
                return Err(format!("the turn was {was}, now {}", turn(g)));
            }
            Ok(g.world.status.number("hp").is_some_and(|hp| hp > 0))
        }),
        // a letter on the menu takes nothing; Esc goes back
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the menu by Esc", |g| menu_at(g, 0)),
        key('y'),
        key('q'),
        Step::Wait("still the menu after letters, the game where it was", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Err(format!("a letter reached the engine: {:?}", g.pending));
            }
            let was = TURN.load(Ordering::Relaxed);
            if turn(g) != was {
                return Err(format!("the turn was {was}, now {}", turn(g)));
            }
            menu_at(g, 0)
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("back to the game", menu_closed),
        // save and leave from the menu: the title offers the game
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the menu", |g| menu_at(g, 0)),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("the focus on saving", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.game_menu.is_open() && ui.game_menu.focused() == GameMenuPick::SaveLeave)
        }),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request(
            "Really save?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        key('y'),
        Step::Wait("\"Continue: Hero\" on the title", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("title") && ui.screens.has_button(&continue_label("Hero")))
        }),
        Step::Push(UiEvent::ContinueGame("Hero".into())),
        Step::Request("the first command of the restored game", command),
        Step::Wait("\"welcome back\" in the log", welcome_back),
        // given up with the keyboard, confirmed
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the menu again", |g| menu_at(g, 0)),
        Step::Key(KeyInput::plain(Key::Up)),
        Step::Wait("round to giving up", |g| menu_at(g, 3)),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Wait("the question again", asks_to_quit),
        key('y'),
        Step::AnswerUntil('n', "the tombstone", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("end"))
        }),
        Step::Shot("give-up-end"),
        Step::Push(UiEvent::BackToTitle),
        Step::Wait("the title, the game gone from it and from the saves", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if screen(g) != Some("title") {
                return Ok(false);
            }
            if ui.screens.has_button(&continue_label("Hero")) {
                return Err("the title still offers the game".into());
            }
            let pg = g.playground();
            if nh_link::save_exists(&pg, "Hero") {
                return Err("the save is still there".into());
            }
            let interrupted = nh_link::interrupted_games(&pg).map_err(|e| e.to_string())?;
            if interrupted.iter().any(|n| n.contains("Hero")) {
                return Err(format!("an interrupted game is left: {interrupted:?}"));
            }
            Ok(true)
        }),
    ]);
    steps
}
