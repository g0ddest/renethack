//! The creation form with the keys alone (`creation`): Enter on the title
//! opens it on the empty name; ↓ goes past the dice to the role, ← twice
//! (round past Random and the Wizard) picks the Valkyrie; ↑ back to the
//! name, Enter gives it the keys and Enter again starts the game with the
//! field still empty: the hero is named from the pool of heroines' names,
//! in the language of the game.

use std::sync::Mutex;

use godot::global::Key as GKey;
use nh_world::Prompt;

use super::{Step, fail_on_error_screen, screen};
use crate::game::RenethackGame;
use crate::tr;

/// The name the form had when Enter started the game.
static NAME: Mutex<Option<String>> = Mutex::new(None);

fn press(key: GKey) -> Step {
    Step::PagePress(key)
}

/// On the form, the focus on the control that says `words`.
fn on(g: &RenethackGame, words: &str) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(screen(g) == Some("creation") && ui.screens.nav_focused().as_deref() == Some(words))
}

fn choice(g: &RenethackGame) -> Result<crate::ui_events::CharacterChoice, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    ui.screens.creation_choice().ok_or_else(|| "no form".into())
}

pub(super) fn creation() -> Vec<Step> {
    vec![
        Step::Wait("the title, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        press(GKey::ENTER),
        Step::Wait("Enter: the form, on the empty name", |g| {
            Ok(on(g, &tr!("creation-name-placeholder"))? && choice(g)?.name.is_empty())
        }),
        press(GKey::DOWN),
        Step::Wait("↓ the dice", |g| on(g, &tr!("creation-name-roll-tip"))),
        press(GKey::DOWN),
        Step::Wait("↓ the role", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("creation")
                && g.ui
                    .as_ref()
                    .and_then(|ui| ui.screens.nav_focused())
                    .is_some_and(|w| w == tr!("creation-random")))
        }),
        press(GKey::LEFT),
        press(GKey::LEFT),
        Step::Wait("← ← the Valkyrie", |g| Ok(choice(g)?.role == "Val")),
        Step::Shot("creation-keys"),
        press(GKey::UP),
        press(GKey::UP),
        Step::Wait("↑ ↑ the name", |g| {
            on(g, &tr!("creation-name-placeholder"))
        }),
        press(GKey::ENTER),
        Step::Wait("Enter: the field has the keys", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.screens.typing()))
        }),
        Step::Call("the name left empty", |g| {
            *NAME.lock().map_err(|e| e.to_string())? = Some(choice(g)?.name);
            Ok(())
        }),
        press(GKey::ENTER),
        Step::Request("Enter: the game's first command", |p| *p == Prompt::Command),
        Step::Call("a heroine's name of the pool", |g| {
            if NAME.lock().map_err(|e| e.to_string())?.as_deref() != Some("") {
                return Err("the field was not empty".into());
            }
            let name = g.name.clone().unwrap_or_default();
            if !crate::names::pool(true).contains(&name) {
                return Err(format!("the Valkyrie is named {name:?}"));
            }
            Ok(())
        }),
    ]
}
