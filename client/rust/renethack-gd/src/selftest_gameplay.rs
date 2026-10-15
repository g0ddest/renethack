//! The settings' Gameplay section: at the title it shows a new game's
//! NetHack options, the profile's, a row dimmed while the switch it needs is
//! off; the keys turn autopickup on and pick up coins, the mouse lowers the
//! load lifted without asking, the gamepad turns the prayer's question off,
//! each kept. A new game starts with them: the engine's own `O` shows
//! autopickup on and coins, and the page over the game shows the game's
//! own, as the host's notice tells them, the engine's request answered by
//! nothing (^A repeats what it did). Over the game the keys turn autopickup
//! off and the gamepad on again, each time through the engine's options
//! menu as a player would use it, its note kept out of the log, the
//! engine's `O` showing the change. Every screen fits the canvas.

use std::sync::atomic::{AtomicU64, Ordering};

use godot::global::Key as GKey;
use nh_world::{Burden, GameOption, Paranoid, Prompt, is_toggle_note};

use super::settings_tests::{pad_tap, page, profile, shown, tap_key};
use super::{Step, command, fail_on_error_screen, key, quit, screen, start};
use crate::game::{GameState, RenethackGame};
use crate::gamepad::PadButton;
use crate::settings_page::{Field, Section, SettingsInput};
use crate::tr;
use crate::ui_events::{DialogEvent, UiEvent};

const AUTO: Field = Field::Game(GameOption::Autopickup);
const TYPES: Field = Field::Game(GameOption::PickupTypes);
const BURDEN: Field = Field::Game(GameOption::PickupBurden);
const PRAY: Field = Field::Game(GameOption::Paranoid(Paranoid::Pray));

macro_rules! keys {
    ($what:literal, $($k:ident),+) => {
        Step::Call($what, |_| {
            $( tap_key(GKey::$k); )+
            Ok(())
        })
    };
}

/// The engine's own options menu (`O`) is the request waiting.
fn engine_options(p: &Prompt) -> bool {
    matches!(p, Prompt::Menu { title: Some(t), .. } if t == "Options")
}

/// The line of option `name` in the engine's own `O` menu waiting.
fn engine_line(g: &RenethackGame, name: &str) -> Result<String, String> {
    let Some((_, Prompt::Menu { items, .. })) = &g.pending else {
        return Err("the engine's O is not waiting".into());
    };
    items
        .iter()
        .filter_map(|i| i.str.clone())
        .find(|t| t.starts_with(&format!("{name} ")))
        .ok_or_else(|| format!("the engine's O has no {name}"))
}

/// The page over the game shows the Gameplay section, the game's own.
fn read_over_game(g: &RenethackGame) -> Result<bool, String> {
    let p = page(g)?;
    Ok(g.state == GameState::Playing
        && p.section() == Section::Gameplay
        && !g.options_busy()
        && g.world.options.is_some()
        && p.about().contains(&tr!("settings-gameplay-game")))
}

/// The request the game waited on when the page opened over it.
static LOOKED_FROM: AtomicU64 = AtomicU64::new(0);

/// A look at the page answers nothing: the game still waits on the request
/// it waited on when the page opened.
fn only_looked(g: &RenethackGame) -> Result<bool, String> {
    match &g.pending {
        Some((id, Prompt::Command)) if *id == LOOKED_FROM.load(Ordering::Relaxed) => Ok(true),
        other => Err(format!(
            "the page answered the game: {:?} waits now",
            other.as_ref().map(|(id, _)| id)
        )),
    }
}

/// The settings page over the game, on its Gameplay section.
fn open_over_game() -> Vec<Step> {
    vec![
        Step::Call("the request the game waits on", |g| {
            let (id, _) = g.pending.as_ref().ok_or("no request waits")?;
            LOOKED_FROM.store(*id, Ordering::Relaxed);
            Ok(())
        }),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page over the game", |g| {
            Ok(screen(g) == Some("settings") && g.state == GameState::Playing)
        }),
        // the mouse: the section's tab
        Step::Push(UiEvent::Settings(SettingsInput::Tab(Section::Gameplay))),
    ]
}

/// The engine's own `O` shows autopickup `on`, then closes.
fn engine_shows(on: bool) -> Vec<Step> {
    let check: super::Check = if on {
        |g| Ok(engine_line(g, "autopickup")?.contains("[X]"))
    } else {
        |g| Ok(engine_line(g, "autopickup")?.contains("[ ]"))
    };
    vec![
        key('O'),
        Step::Request("the engine's own options", engine_options),
        Step::Wait("autopickup in the engine's O", check),
        Step::Dialog(DialogEvent::MenuCancel),
        Step::Request("a command again", command),
    ]
}

pub(super) fn gameplay() -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the title screen, its scene drawn", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let drawn = ui.map.preloaded() && ui.map.title_drawn();
            Ok(screen(g) == Some("title") && drawn && g.warmed_up())
        }),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page over the title", |g| {
            Ok(screen(g) == Some("settings") && g.state == GameState::Title)
        }),
        Step::Push(UiEvent::Settings(SettingsInput::Tab(Section::Gameplay))),
        Step::Wait(
            "a new game's options: the profile's, the engine's defaults",
            |g| {
                let p = page(g)?;
                Ok(p.section() == Section::Gameplay
                && p.field() == Some(AUTO)
                && shown(g, AUTO)? == tr!("settings-off")
                && shown(g, TYPES)? == tr!("settings-pickup-all")
                && shown(g, BURDEN)? == tr!("settings-burden-stressed")
                && shown(g, PRAY)? == tr!("settings-on")
                && shown(g, Field::Game(GameOption::Verbose))? == tr!("settings-on")
                // the kinds picked up wait for autopickup
                && !p.row_live(TYPES)
                && p.about().contains(&tr!("settings-gameplay-new")))
            },
        ),
        Step::Shot("gameplay-title"),
        // the keys: → autopickup on, its kinds live; ↓ → coins
        keys!("→", RIGHT),
        Step::Wait("autopickup on, kept; the kinds it takes live", |g| {
            Ok(profile(g)?.settings.gameplay.autopickup
                && shown(g, AUTO)? == tr!("settings-on")
                && page(g)?.row_live(TYPES))
        }),
        keys!("↓ →", DOWN, RIGHT),
        Step::Wait("coins picked up, kept", |g| {
            Ok(profile(g)?.settings.gameplay.pickup_types == "$"
                && shown(g, TYPES)? == tr!("settings-pickup-coins"))
        }),
        // the mouse: the load's left arrow
        Step::Push(UiEvent::Settings(SettingsInput::Step(BURDEN, -1))),
        Step::Wait("a pickup asks past burdened, kept", |g| {
            Ok(page(g)?.field() == Some(BURDEN)
                && profile(g)?.settings.gameplay.pickup_burden == Burden::Burdened
                && shown(g, BURDEN)? == tr!("settings-burden-burdened"))
        }),
        // the gamepad: down to the prayer's question, scrolled into view;
        // A turns it off
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        Step::Wait(
            "the prayer's question has the focus, a gamepad's hints",
            |g| {
                let p = page(g)?;
                Ok(p.field() == Some(PRAY) && p.pad().is_some())
            },
        ),
        Step::Shot("gameplay-title-pad"),
        pad_tap(PadButton::A),
        Step::Wait("no question before a prayer, kept", |g| {
            Ok(!profile(g)?.settings.gameplay.paranoid_pray
                && shown(g, PRAY)? == tr!("settings-off"))
        }),
        pad_tap(PadButton::B),
        Step::Wait("the title again", |g| {
            Ok(screen(g) == Some("title") && g.settings_page.is_none())
        }),
    ];
    // a new game starts with them: the engine's own O says so
    steps.extend(start());
    steps.extend(engine_shows(true));
    steps.extend(open_over_game());
    steps.extend([
        Step::Wait("the game's own options, as it tells them", |g| {
            let wanted = profile(g)?.settings.gameplay;
            Ok(read_over_game(g)?
                && only_looked(g)?
                && g.world.options.as_ref().is_some_and(|o| o.same_as(&wanted))
                && shown(g, AUTO)? == tr!("settings-on")
                && shown(g, TYPES)? == tr!("settings-pickup-coins")
                && shown(g, BURDEN)? == tr!("settings-burden-burdened")
                && shown(g, PRAY)? == tr!("settings-off"))
        }),
        Step::Shot("gameplay-game"),
        // the keys: ← the game's autopickup off, through its own menu
        keys!("←", LEFT),
        Step::Wait(
            "the game's autopickup off, as it reports; kept for new games",
            |g| {
                Ok(read_over_game(g)?
                    && g.world.options.as_ref().is_some_and(|o| !o.autopickup)
                    && shown(g, AUTO)? == tr!("settings-off")
                    && !page(g)?.row_live(TYPES)
                    && !profile(g)?.settings.gameplay.autopickup)
            },
        ),
        Step::Call("the menu's note of it kept out of the log", |g| {
            match g.world.log.iter().find(|m| is_toggle_note(&m.text)) {
                Some(m) => Err(format!("the log says {:?}", m.text)),
                None => Ok(()),
            }
        }),
        keys!("Esc", ESCAPE),
        Step::Wait("the game again", |g| {
            Ok(screen(g).is_none() && g.settings_page.is_none())
        }),
    ]);
    steps.extend(engine_shows(false));
    // the gamepad: A turns it on again
    steps.extend(open_over_game());
    steps.extend([
        Step::Wait("the game's own again: autopickup off", |g| {
            Ok(read_over_game(g)?
                && only_looked(g)?
                && g.world.options.as_ref().is_some_and(|o| !o.autopickup))
        }),
        pad_tap(PadButton::A),
        Step::Wait("the game's autopickup on again, as it reports", |g| {
            Ok(read_over_game(g)?
                && g.world.options.as_ref().is_some_and(|o| o.autopickup)
                && shown(g, AUTO)? == tr!("settings-on")
                && profile(g)?.settings.gameplay.autopickup)
        }),
        Step::Shot("gameplay-game-pad"),
        pad_tap(PadButton::B),
        Step::Wait("the game again", |g| {
            Ok(screen(g).is_none() && g.settings_page.is_none())
        }),
    ]);
    steps.extend(engine_shows(true));
    steps.extend(quit());
    steps
}
