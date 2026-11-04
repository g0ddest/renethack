//! The title, the creation form and the end with a gamepad alone
//! (`pad-screens`): the title opens with the focus on a new game; the
//! d-pad moves it and A takes it (the achievements, then B back); the
//! form opens on the role chosen (Random: the hooded figure framed in the
//! title's place), → picks the next and shows them, A goes on to the
//! empty name, → to the die, and A rolls a name that agrees with the
//! gender (chosen with it, the figure unhooded); Start begins the game
//! under that name. Given up from the game's menu (B, the d-pad,
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
/// The name the dice rolled.
static NAME: Mutex<String> = Mutex::new(String::new());
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

fn choice(g: &RenethackGame) -> Option<crate::ui_events::CharacterChoice> {
    g.ui.as_ref()?.screens.creation_choice()
}

fn role(g: &RenethackGame) -> Option<String> {
    choice(g).map(|c| c.role)
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
        Step::Wait("the form, on the role chosen (Random), no name", |g| {
            let empty = choice(g).is_some_and(|c| c.name.is_empty() && c.role == "random");
            Ok(empty && focus_on(g, "creation", &tr!("creation-random"))?)
        }),
        Step::Wait("no keys' row under a pad", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.creation_keys_shown() == Some(false))
        }),
        Step::Wait("the hooded figure framed in the title's place", |g| {
            let map = &g.ui.as_ref().ok_or("no UI")?.map;
            Ok(map.creation_hero().is_some_and(|p| p.veiled()) && map.creation_framed())
        }),
        tap(PadButton::Right),
        Step::Wait("→ another role, shown", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let role = role(g).unwrap_or_default();
            let shown = ui.map.creation_hero().and_then(|p| p.role.clone());
            if role != "random" && shown.as_deref() == Some(role.as_str()) {
                *ROLE.lock().map_err(|e| e.to_string())? = role;
                return Ok(true);
            }
            Ok(false)
        }),
        tap(PadButton::A),
        Step::Wait("A: on past the cards, to the empty name", |g| {
            focus_on(g, "creation", &tr!("creation-name-placeholder"))
        }),
        tap(PadButton::Right),
        Step::Wait("→ the dice", |g| {
            focus_on(g, "creation", &tr!("creation-name-roll-tip"))
        }),
        tap(PadButton::A),
        Step::Wait("A: a name of the pool, the gender chosen with it", |g| {
            fail_on_error_screen(g)?;
            let Some(c) = choice(g).filter(|c| !c.name.is_empty()) else {
                return Ok(false);
            };
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            if c.gender == "random" {
                return Err(format!("the dice left the gender to chance: {c:?}"));
            }
            let female = crate::names::is_female(cat, &c.gender);
            if !crate::names::pool(female).contains(&c.name) {
                return Err(format!("{:?} is no {} name", c.name, c.gender));
            }
            if c.role != *ROLE.lock().map_err(|e| e.to_string())? {
                return Err(format!("the role changed: {c:?}"));
            }
            *NAME.lock().map_err(|e| e.to_string())? = c.name;
            Ok(true)
        }),
        Step::Wait("the hero shown as rolled, unhooded", |g| {
            let map = &g.ui.as_ref().ok_or("no UI")?.map;
            Ok(map.creation_hero().is_some_and(|p| !p.veiled()))
        }),
        Step::Shot("pad-creation"),
        tap(PadButton::Start),
        Step::Request("Start: the game's first command", |p| *p == Prompt::Command),
        Step::Call("the hero named by the dice", |g| {
            let rolled = NAME.lock().map_err(|e| e.to_string())?.clone();
            match g.name.as_deref() {
                Some(name) if name == rolled => Ok(()),
                other => Err(format!("the hero is {other:?}, the dice rolled {rolled:?}")),
            }
        }),
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
        Step::Wait("the end, its beat over, on the title's", |g| {
            let beat = g.ui.as_ref().is_some_and(|ui| ui.screens.end_beat());
            Ok(!beat && focus_on(g, "end", &tr!("end-to-title"))?)
        }),
        Step::Shot("pad-end"),
        tap(PadButton::B),
        Step::Wait("B: the title", |g| {
            focus_on(g, "title", &tr!("title-new-game"))
        }),
    ]
}
