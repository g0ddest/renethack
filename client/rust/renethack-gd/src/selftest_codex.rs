//! The codex (`codex`): a Wizard's `\` laid out from the engine's own
//! window, a card an object under its class with its appearance's icon,
//! its name and its looks as the window's translation says them, in the
//! language the test runs in; no turn passes. Tab asks the engine for
//! #conduct, its tab's lines as the engine says them; Tab again, the
//! discoveries. `\` closes its own tab; a gamepad's bumper turns the tab
//! and B closes the codex. The hero is a man: in Russian the HUD's rank
//! and role are a man's.

use std::sync::atomic::{AtomicI64, Ordering};

use nh_world::{Key, KeyInput, KeyProfile, Prompt};

use super::{CharacterChoice, PadEv, Step, fail_on_error_screen, key, quit, start_as};
use crate::codex;
use crate::dialogs::CodexTab;
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, EngineKind};

/// The turn the codex first opened on.
static TURN: AtomicI64 = AtomicI64::new(-1);

/// A man: the HUD's rank and role are a man's (the spells' scenario has a
/// woman's).
fn wizard() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "wizard".into(),
        race: "human".into(),
        gender: "male".into(),
        align: "neutral".into(),
        profile: KeyProfile::Classic,
    }
}

/// The text window the engine waits on: its lines and whether each heads
/// a section.
fn window(g: &RenethackGame) -> Option<Vec<(String, bool)>> {
    match g.pending.as_ref()? {
        (_, Prompt::Show { lines, .. }) => Some(
            lines
                .iter()
                .map(|l| (l.text.clone(), l.attr & 0x0f != 0))
                .collect(),
        ),
        _ => None,
    }
}

/// The codex shows the discoveries: a card an object, its name and looks
/// as said, and its icon.
fn discoveries(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.codex_tab() != Some(CodexTab::Discoveries) {
        return Ok(false);
    }
    let lines = window(g).ok_or("no window under the codex")?;
    let d = codex::discoveries(&lines).ok_or("the window is no discoveries")?;
    let texts = ui.dialogs.panel_texts();
    let english: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    let said = i18n::engine_window(&english).filter(|t| t.len() == english.len());
    let say = |line: &str| -> String {
        let at = english.iter().position(|e| *e == line);
        match (at, said.as_ref()) {
            (Some(i), Some(t)) => t[i].clone(),
            _ => i18n::engine(EngineKind::Window, line).into_owned(),
        }
    };
    let mut cards = 0;
    for s in &d.sections {
        for e in &s.entries {
            cards += usize::from(codex::class_of(&s.heading).is_some());
            let mark = if e.unseen { "* " } else { "  " };
            let line = english
                .iter()
                .find(|l| l.strip_prefix(mark).is_some_and(|r| r.starts_with(&e.text)))
                .copied()
                .ok_or("no line for a card")?;
            let (name, looks) = codex::card_words(&say(line), e);
            if !texts.contains(&name) {
                return Err(format!("no card {name:?}: {texts:?}"));
            }
            if let Some(l) = looks
                && !texts.contains(&l)
            {
                return Err(format!("{name} looks not {l:?}: {texts:?}"));
            }
        }
    }
    let (with, all) = ui.dialogs.codex_icons();
    // every object of a class has its appearance's icon
    if all != cards || with != all {
        return Err(format!(
            "{with} of {all} cards have an icon ({cards} of a class)"
        ));
    }
    Ok(cards > 0)
}

/// The codex shows the conduct: each line as the engine says it.
fn conduct(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.codex_tab() != Some(CodexTab::Conduct) {
        return Ok(false);
    }
    let lines = window(g).ok_or("no window under the codex")?;
    let english: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    let c = codex::conduct(&english).ok_or("the window is no conduct")?;
    let texts = ui.dialogs.panel_texts();
    for line in &c {
        let said = i18n::engine(EngineKind::Window, &format!(" {line}"))
            .trim()
            .to_string();
        if !texts.contains(&said) {
            return Err(format!("the conduct shows no {said:?}: {texts:?}"));
        }
    }
    Ok(!c.is_empty())
}

fn command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && matches!(g.pending, Some((_, Prompt::Command))))
}

fn tab() -> Step {
    Step::Key(KeyInput::plain(Key::Tab))
}

pub(super) fn codex() -> Vec<Step> {
    let mut steps = start_as(wizard());
    steps.extend([
        Step::Call("the turn", |g| {
            TURN.store(g.world.turn().unwrap_or(-1), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the HUD's title and role in the hero's gender", |g| {
            // in Russian a male Wizard is Призыватель, Волшебник
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (title, role) = (ui.hud.title_shown(), ui.hud.subtitle_shown());
            if i18n::lang() == i18n::Lang::Ru
                && (!title.starts_with("Призыватель ") || !role.starts_with("Волшебник "))
            {
                return Err(format!("the HUD says {title:?}, {role:?}"));
            }
            Ok(true)
        }),
        key('\\'),
        Step::Wait("the codex's discoveries, as \\'s window says", discoveries),
        Step::Shot("codex"),
        // Tab: the engine's #conduct, then the discoveries again
        tab(),
        Step::Wait("the codex's conduct", conduct),
        Step::Shot("codex-conduct"),
        tab(),
        Step::Wait("the discoveries again", discoveries),
        // \ closes its own tab; no turn passed
        key('\\'),
        Step::Wait("closed on \\, no turn passed", |g| {
            let was = TURN.load(Ordering::Relaxed);
            if command(g)? && g.world.turn().unwrap_or(-1) != was {
                return Err(format!("the turn was {was}, now {:?}", g.world.turn()));
            }
            command(g)
        }),
        // a gamepad: a bumper turns the tab, B closes
        key('\\'),
        Step::Wait("the discoveries for a gamepad", discoveries),
        Step::Pad(vec![
            PadEv::Button(PadButton::Rb, true),
            PadEv::Button(PadButton::Rb, false),
        ]),
        Step::Wait("the conduct by RB", conduct),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("closed by B", command),
    ]);
    steps.extend(quit());
    steps
}
