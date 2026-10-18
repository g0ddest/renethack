//! The title, the creation form and the end with a gamepad alone
//! (`pad-screens`): the title opens with the focus on a new game; the
//! d-pad moves it and A takes it (the achievements, then B back); the
//! form opens on the name, the d-pad goes down to the role and → changes
//! it, Start begins the game. Given up from the game's menu (B, the d-pad,
//! A, ← to yes, A), its questions let go with B; the end opens on a new
//! game and B goes back to the title.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use godot::classes::Engine;
use godot::obj::Singleton;
use nh_world::Prompt;

use super::{PadEv, Step, fail_on_error_screen, pad_event, screen, tap};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::tr;

/// The role the form had before → changed it.
static ROLE: Mutex<String> = Mutex::new(String::new());
static FRAME: AtomicI64 = AtomicI64::new(0);

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// On `page`, the focus on the control that says `words`.
fn focus_on(g: &RenethackGame, page: &str, words: &str) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(screen(g) == Some(page) && ui.screens.nav_focused().as_deref() == Some(words))
}

fn role(g: &RenethackGame) -> Option<String> {
    g.ui.as_ref()?.screens.creation_choice().map(|c| c.role)
}

pub(super) fn pad_screens() -> Vec<Step> {
    vec![
        Step::Wait("the title, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        Step::Wait("the focus on a new game", |g| {
            focus_on(g, "title", &tr!("title-new-game"))
        }),
        Step::Shot("pad-title"),
        tap(PadButton::Down),
        Step::Wait("down to the achievements", |g| {
            focus_on(g, "title", &tr!("title-achievements"))
        }),
        tap(PadButton::A),
        Step::Wait(
            "the achievements",
            |g| Ok(screen(g) == Some("achievements")),
        ),
        tap(PadButton::B),
        Step::Wait("the title again, on a new game", |g| {
            focus_on(g, "title", &tr!("title-new-game"))
        }),
        tap(PadButton::A),
        Step::Wait("the form, on the name", |g| {
            focus_on(g, "creation", crate::screens::DEFAULT_NAME)
        }),
        tap(PadButton::Down),
        Step::Wait("down to the role", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let on_role = ui
                .screens
                .nav_focused()
                .is_some_and(|w| w != crate::screens::DEFAULT_NAME);
            if on_role {
                *ROLE.lock().map_err(|e| e.to_string())? = role(g).unwrap_or_default();
            }
            Ok(on_role)
        }),
        tap(PadButton::Right),
        Step::Wait("→ another role", |g| {
            let was = ROLE.lock().map_err(|e| e.to_string())?.clone();
            Ok(role(g).is_some_and(|r| r != was))
        }),
        Step::Shot("pad-creation"),
        tap(PadButton::Start),
        Step::Request("Start: the game's first command", |p| *p == Prompt::Command),
        // given up from the game's menu
        tap(PadButton::B),
        Step::Wait("the game's menu", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.game_menu.is_open()))
        }),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::A),
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.starts_with("Really quit")),
        ),
        tap(PadButton::Left),
        tap(PadButton::A),
        Step::Call("", |_| {
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the questions let go with B, the end", |g| {
            fail_on_error_screen(g)?;
            if screen(g) == Some("end") {
                return Ok(true);
            }
            // B now and then, while a question waits
            if frames() - FRAME.load(Ordering::Relaxed) > 30 {
                FRAME.store(frames(), Ordering::Relaxed);
                if g.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open()) {
                    pad_event(PadEv::Button(PadButton::B, true));
                    pad_event(PadEv::Button(PadButton::B, false));
                }
            }
            Ok(false)
        }),
        Step::Wait("the end, on a new game", |g| {
            focus_on(g, "end", &tr!("end-new-game"))
        }),
        Step::Shot("pad-end"),
        tap(PadButton::B),
        Step::Wait("B: the title", |g| {
            focus_on(g, "title", &tr!("title-new-game"))
        }),
    ]
}
