//! The character sheet (`sheet`): a game's `^X` laid out as its panel,
//! from the engine's own window, in the language the test runs in. The
//! sheet shows the hero's name, the hit points, energy, armour class,
//! experience and gold, the six characteristics and every other line of
//! the window, said as its translation says it; no turn passes. `^X`
//! again closes it; the HUD's button opens it and its ✕ closes it; a
//! gamepad's B closes it. The sheet's command moved onto F7 opens on F7,
//! names F7 in its hint and closes on F7.

use std::sync::atomic::{AtomicI64, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, PadEv, Step, ctrl_key, fail_on_error_screen, quit, start};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, EngineKind};
use crate::sheet;
use crate::ui_events::UiEvent;

/// The turn the sheet first opened on.
static TURN: AtomicI64 = AtomicI64::new(-1);

fn f7() -> KeyInput {
    KeyInput::plain(Key::F(7))
}

/// The lines of the text window the engine waits on, as it wrote them.
fn window(g: &RenethackGame) -> Option<Vec<String>> {
    match g.pending.as_ref() {
        Some((_, Prompt::Show { lines, .. })) => {
            Some(lines.iter().map(|l| l.text.clone()).collect())
        }
        _ => None,
    }
}

/// The sheet is open on the engine's `^X`, and shows what its window says:
/// the name, the numbers, each line as the player reads it.
fn shown(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.layout_name() != Some("sheet") {
        return Ok(false);
    }
    let lines = window(g).ok_or("no text window under the sheet")?;
    let s = sheet::sheet(&lines).ok_or("the window is no sheet")?;
    let texts = ui.dialogs.panel_texts();
    let has = |t: &str| texts.iter().any(|x| x == t);
    // the window's translation, line by line where its lines stay
    let english: Vec<&str> = lines.iter().map(String::as_str).collect();
    let said = i18n::engine_window(&english).filter(|t| t.len() == english.len());
    let say = |line: &str| -> String {
        let at = english.iter().position(|e| e.trim() == line.trim());
        match (at, said.as_ref()) {
            (Some(i), Some(t)) => t[i].trim().to_string(),
            _ => i18n::engine(EngineKind::Window, line).trim().to_string(),
        }
    };
    let mut want = vec![s.name.clone()];
    for p in [s.hit_points, s.energy].into_iter().flatten() {
        want.push(format!("{} / {}", p.now, p.most));
    }
    want.extend(s.armor_class.map(|ac| ac.to_string()));
    want.extend(s.gold.map(|g| g.to_string()));
    want.extend(s.experience.map(|x| x.points.to_string()));
    if s.stats.len() != 6 {
        return Err(format!("the six read as {:?}", s.stats));
    }
    want.extend(s.stats.iter().map(|st| st.value.clone()));
    want.extend(s.identity.iter().map(|l| say(l)));
    for sec in &s.sections {
        want.extend(sec.lines.iter().map(|l| say(l)));
    }
    match want.iter().find(|w| !has(w)) {
        Some(missing) => Err(format!("the sheet shows no {missing:?}: {texts:?}")),
        None => Ok(true),
    }
}

/// No dialog over the game, the engine at its command prompt.
fn closed(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && matches!(g.pending, Some((_, Prompt::Command))))
}

/// The panel's hint names `key`.
fn hint_names(g: &RenethackGame, key: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let texts = ui.dialogs.panel_texts();
    if texts.iter().any(|t| t.contains(key)) {
        Ok(true)
    } else {
        Err(format!("the sheet names no {key}: {texts:?}"))
    }
}

pub(super) fn sheet() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Call("the turn", |g| {
            TURN.store(g.world.turn().unwrap_or(-1), Ordering::Relaxed);
            Ok(())
        }),
        Step::Key(ctrl_key('x')),
        Step::Wait("the sheet, as ^X's window says", shown),
        Step::Wait("the hero's portrait drawn", |g| {
            // a renderer draws the doll; headless draws nothing
            Ok(g.headless || g.ui.as_ref().is_some_and(|ui| ui.dialogs.portrait_shown()))
        }),
        Step::Shot("sheet"),
        // ^X again closes it; no turn passed
        Step::Key(ctrl_key('x')),
        Step::Wait("closed on ^X, no turn passed", |g| {
            let was = TURN.load(Ordering::Relaxed);
            if closed(g)? && g.world.turn().unwrap_or(-1) != was {
                return Err(format!("the turn was {was}, now {:?}", g.world.turn()));
            }
            closed(g)
        }),
        // the HUD's button, the ✕
        Step::Push(UiEvent::GameKey(ctrl_key('x'))),
        Step::Wait("the sheet from the HUD's button", shown),
        Step::Dialog(DialogEvent::Close),
        Step::Wait("closed by its ✕", closed),
        // a gamepad's B
        Step::Push(UiEvent::GameKey(ctrl_key('x'))),
        Step::Wait("the sheet again", shown),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("closed by B", closed),
        // the sheet's command on F7: F7 opens it, its hint names F7
        Step::Call("^X moved onto F7", |g| {
            let mut s = g.settings.clone();
            s.keymaps
                .get_mut(g.ui_state.profile)
                .rebind(crate::rebind::ctrl('x'), f7(), false);
            g.set_settings(s);
            Ok(())
        }),
        Step::Key(f7()),
        Step::Wait("the sheet on F7", shown),
        Step::Wait("its hint naming F7", |g| hint_names(g, "F7")),
        Step::Key(f7()),
        Step::Wait("closed on F7", closed),
        Step::Call("^X back on its key", |g| {
            let mut s = g.settings.clone();
            s.keymaps
                .get_mut(g.ui_state.profile)
                .reset(crate::rebind::ctrl('x'));
            g.set_settings(s);
            Ok(())
        }),
    ]);
    steps.extend(quit());
    steps
}
