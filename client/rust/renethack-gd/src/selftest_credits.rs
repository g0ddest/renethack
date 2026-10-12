//! The credits and licences: `credits` opens them from the title's button
//! on the game's own credit, then goes down the list with ↓, each document
//! drawn with what it must show (the art's CC-BY models and their authors,
//! the sounds', the NGPL as the engine ships it, Godot's licence and the
//! code it includes, the Rust crates linked, the fonts' licence...), and
//! none showing the repository's tools or recipes (`tools/`, `.json`); PgDn
//! scrolls a long one and Esc goes back to the title. From the settings
//! their button opens the page and Back returns to the settings, where
//! they were; the d-pad goes through the list, its hints show, and B goes
//! back. Every screen fits the canvas.

use std::sync::atomic::{AtomicUsize, Ordering};

use godot::classes::{Input, InputEventKey};
use godot::global::Key as GKey;
use godot::prelude::*;

use super::{PadEv, Step, fail_on_error_screen, screen};
use crate::credits::{AUTHOR, Doc};
use crate::credits_page::CreditsPage;
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::settings_page::Section;
use crate::ui_events::UiEvent;

/// What each document must show, in the list's order (the words as
/// written: the same in every language).
const NEEDLES: [(Doc, &[&str]); 13] = [
    (Doc::Game, &["renethack", AUTHOR]),
    (
        Doc::NetHack,
        &["NetHack DevTeam", "Stichting Mathematisch Centrum"],
    ),
    (
        Doc::Art,
        &[
            "Creative Commons Attribution 3.0",
            "Poly by Google",
            "Quaternius",
        ],
    ),
    (Doc::Sound, &["Kenney", "Interface Sounds"]),
    (Doc::Help, &["Guidebook", "Velikodniy"]),
    (Doc::Translation, &["pymorphy3", "OpenCorpora"]),
    (Doc::Notice, &["renethack 0.1", "Lua 5.4"]),
    (Doc::Ngpl, &["NETHACK GENERAL PUBLIC LICENSE"]),
    (Doc::Godot, &["Juan Linietsky"]),
    (Doc::GodotParts, &["FreeType"]),
    (Doc::Rust, &["godot-core", "MPL-2.0", "serde_json"]),
    (Doc::EngineParts, &["Lua 5.4", "Dave Gamble"]),
    (Doc::Fonts, &["SIL OPEN FONT LICENSE", "Cormorant"]),
];

/// The document checked next.
static AT: AtomicUsize = AtomicUsize::new(0);

fn page(g: &RenethackGame) -> Result<&CreditsPage, String> {
    g.credits_page
        .as_ref()
        .ok_or_else(|| "the credits page is not open".to_string())
}

/// A key pressed and let go through Godot's input.
fn tap_key(k: GKey) {
    for pressed in [true, false] {
        let mut ev = InputEventKey::new_gd();
        ev.set_keycode(k);
        ev.set_physical_keycode(k);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
}

fn pad_tap(b: PadButton) -> Step {
    Step::Pad(vec![PadEv::Button(b, true), PadEv::Button(b, false)])
}

/// The document shown is the next to check, laid out, with its words;
/// then ↓ to the one after.
fn checked(g: &RenethackGame) -> Result<bool, String> {
    let at = AT.load(Ordering::Relaxed);
    let (doc, needles) = NEEDLES[at];
    let p = page(g)?;
    if p.selected().1 != doc {
        return Ok(false);
    }
    let Some(text) = p.shown_text() else {
        return Ok(false);
    };
    if let Some(missing) = needles.iter().find(|n| !text.contains(**n)) {
        return Err(format!("{doc:?} shows no \"{missing}\""));
    }
    // the repository's workings stay in the files
    if let Some(inner) = ["tools/", ".json"].iter().find(|w| text.contains(**w)) {
        return Err(format!("{doc:?} shows \"{inner}\""));
    }
    AT.store(at + 1, Ordering::Relaxed);
    Ok(true)
}

pub(super) fn credits() -> Vec<Step> {
    let mut steps = vec![
        Step::Call("from the first document", |_| {
            AT.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the title screen, its scene drawn", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let drawn = ui.map.preloaded() && ui.map.title_drawn();
            Ok(screen(g) == Some("title") && drawn && g.warmed_up())
        }),
        Step::Push(UiEvent::OpenCredits),
        Step::Wait("the credits over the title, every document listed", |g| {
            let p = page(g)?;
            let listed: Vec<Doc> = p.docs().to_vec();
            let want: Vec<Doc> = NEEDLES.iter().map(|(d, _)| *d).collect();
            if !listed.starts_with(&want) {
                return Err(format!("the list is {listed:?}"));
            }
            Ok(screen(g) == Some("credits"))
        }),
        Step::Wait("the game's own credit", checked),
        Step::Shot("credits-game"),
    ];
    for (doc, _) in &NEEDLES[1..] {
        steps.push(Step::Call("↓", |_| {
            tap_key(GKey::DOWN);
            Ok(())
        }));
        steps.push(Step::Wait(
            "the next document, with what it must show",
            checked,
        ));
        // the art's attributions, and a long licence, for the review
        match doc {
            Doc::Art => steps.push(Step::Shot("credits-art")),
            Doc::Rust => steps.push(Step::Shot("credits-rust")),
            _ => {}
        }
    }
    steps.extend([
        // PgDn scrolls the long one shown (the fonts' licence)
        Step::Call("PgDn", |_| {
            tap_key(GKey::PAGEDOWN);
            Ok(())
        }),
        Step::Wait("the text scrolled", |g| Ok(page(g)?.scrolled() > 0.0)),
        Step::Call("Esc", |_| {
            tap_key(GKey::ESCAPE);
            Ok(())
        }),
        Step::Wait("the title again", |g| {
            Ok(screen(g) == Some("title") && g.credits_page.is_none())
        }),
        // from the settings, on their button: back to them, where they were
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings", |g| Ok(screen(g) == Some("settings"))),
        Step::Call("the settings on their graphics", |_| {
            tap_key(GKey::TAB);
            Ok(())
        }),
        Step::Wait("the graphics", |g| {
            Ok(g.settings_page
                .as_ref()
                .is_some_and(|p| p.section() == Section::Graphics))
        }),
        Step::Push(UiEvent::OpenCredits),
        Step::Wait("the credits from the settings", |g| {
            Ok(screen(g) == Some("credits") && g.settings_page.is_none())
        }),
        // a gamepad: the d-pad goes down the list, its hints show
        pad_tap(PadButton::Down),
        Step::Wait("the d-pad went down the list, a gamepad's hints", |g| {
            let p = page(g)?;
            Ok(p.selected().1 == Doc::NetHack && p.pad().is_some())
        }),
        Step::Shot("credits-pad"),
        pad_tap(PadButton::B),
        Step::Wait("B: the settings again, on their graphics", |g| {
            Ok(screen(g) == Some("settings")
                && g.credits_page.is_none()
                && g.settings_page
                    .as_ref()
                    .is_some_and(|p| p.section() == Section::Graphics))
        }),
        Step::Push(UiEvent::CloseSettings),
        Step::Wait("the title once more", |g| Ok(screen(g) == Some("title"))),
    ]);
    steps
}
