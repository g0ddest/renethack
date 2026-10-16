//! `prologue`: a new game told with the prologue (NetHack's legacy, which
//! the client asks the engine for): each beat in turn, its scene drawn and
//! its words all shown, word for word the beat's part of the engine's text
//! (in the language played), a picture of it, a key on to the next; after
//! the last the game itself, up out of black, its first command awaited.
//! The engine is not held: its first command is already waiting while the
//! prologue begins. `prologue-skip`: Esc in the second beat lands in the
//! game at once.

use godot::global::Key as GKey;
use godot::prelude::godot_print;

use super::{Step, command, fail_on_error_screen, quit, screen, smoke_choice};
use crate::game::{GameState, RenethackGame};
use crate::prologue::Beat;
use crate::ui_events::UiEvent;

/// A new game asked to start with the prologue, up to its first beat.
fn begin() -> Vec<Step> {
    vec![
        Step::Call("the prologue asked for", |g| {
            g.prologue_forced = true;
            Ok(())
        }),
        Step::Wait("the title screen, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        Step::Push(UiEvent::StartCharacter(smoke_choice())),
        Step::Wait("the prologue begins, the engine not held", |g| {
            fail_on_error_screen(g)?;
            Ok(g.prologue.is_some())
        }),
    ]
}

/// The beat `beat` up: its scene drawn, its words all shown, out of black.
fn shown(g: &RenethackGame, beat: Beat) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let Some(p) = g.prologue.as_ref() else {
        return Err(format!(
            "the prologue ended before its {} beat",
            beat.name()
        ));
    };
    if p.run.beat().index() > beat.index() {
        return Err(format!("the {} beat went by unseen", beat.name()));
    }
    let Some(ui) = g.ui.as_ref() else {
        return Ok(false);
    };
    Ok(p.run.beat() == beat
        && p.run.all_shown()
        && p.run.shade() <= 0.0
        && ui.map.prologue_beat() == Some(beat)
        && ui.map.title_drawn())
}

/// The words on screen are the beat's, whole and in order.
fn words(g: &mut RenethackGame) -> Result<(), String> {
    let p = g.prologue.as_ref().ok_or("no prologue")?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let lines = ui.prologue.lines();
    let joined = lines.join(" ");
    godot_print!(
        "selftest: prologue: the {} beat: {lines:?}",
        p.run.beat().name()
    );
    if joined != p.run.text() {
        return Err(format!(
            "the {} beat shows {joined:?}, not its text {:?}",
            p.run.beat().name(),
            p.run.text()
        ));
    }
    // the engine's text, in the language played
    let first = &p.run.texts()[0];
    let ru = crate::i18n::lang() == crate::i18n::Lang::Ru;
    let book = if ru {
        "Написано в Книге"
    } else {
        "It is written in the Book of"
    };
    if !first.starts_with(book) {
        return Err(format!("the Book's words are {first:?}"));
    }
    Ok(())
}

/// The game up after the prologue: its map and HUD, the black gone, its
/// first command waiting.
fn in_game(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let up = g.prologue.is_none()
        && g.state == GameState::Playing
        && g.ui
            .as_ref()
            .is_some_and(|ui| !ui.prologue.is_visible() && ui.map.drawn_cells() > 0)
        && g.pending.as_ref().is_some_and(|(_, p)| command(p));
    Ok(up)
}

pub(super) fn prologue() -> Vec<Step> {
    let mut steps = begin();
    for beat in Beat::ALL {
        let (name, shot) = match beat {
            Beat::Book => ("the Book", "prologue-book"),
            Beat::Rebellion => ("Moloch's rebellion", "prologue-rebellion"),
            Beat::Amulet => ("the Amulet into Gehennom", "prologue-amulet"),
            Beat::God => ("the god's altar", "prologue-god"),
            Beat::Hero => ("the hero", "prologue-hero"),
            Beat::Descent => ("the descent", "prologue-descent"),
        };
        let wait: fn(&RenethackGame) -> Result<bool, String> = match beat {
            Beat::Book => |g| shown(g, Beat::Book),
            Beat::Rebellion => |g| shown(g, Beat::Rebellion),
            Beat::Amulet => |g| shown(g, Beat::Amulet),
            Beat::God => |g| shown(g, Beat::God),
            Beat::Hero => |g| shown(g, Beat::Hero),
            Beat::Descent => |g| shown(g, Beat::Descent),
        };
        steps.push(Step::Wait(name, wait));
        steps.push(Step::Shot(shot));
        steps.push(Step::Call("its words, word for word", words));
        if beat != Beat::Descent {
            // a key goes on (its words are all shown)
            steps.push(Step::Press(GKey::SPACE, ' ', false));
        }
    }
    steps.extend([
        Step::Wait("the hero on the stairs down", |g| {
            fail_on_error_screen(g)?;
            let on_stairs =
                g.ui.as_ref()
                    .and_then(|ui| ui.map.hero_stands())
                    .is_some_and(|(p, _)| {
                        (p.x - crate::map_view::PROLOGUE_STAIRS.0 as f32).abs() < 0.2
                            && (p.z - crate::map_view::PROLOGUE_STAIRS.1 as f32).abs() < 0.2
                    });
            Ok(on_stairs || g.prologue.is_none())
        }),
        Step::ShotIf("prologue-down", |g| Ok(g.prologue.is_some())),
        Step::Wait("going down them", |g| {
            fail_on_error_screen(g)?;
            Ok(g.prologue
                .as_ref()
                .is_none_or(|p| p.run.descent().is_some_and(|d| d > 2.9)))
        }),
        Step::ShotIf("prologue-deeper", |g| Ok(g.prologue.is_some())),
        Step::Wait("down the stairs, the game up out of black", in_game),
        Step::Shot("prologue-game"),
        Step::Call("the profile has seen it, and keeps it to replay", |g| {
            let p = crate::game::load_profile(&g.playground());
            if !p.prologue_seen
                || !crate::prologue::is_legacy(
                    &p.legacy.iter().map(String::as_str).collect::<Vec<_>>(),
                )
            {
                return Err("the profile does not keep the prologue".into());
            }
            Ok(())
        }),
    ]);
    steps.extend(quit());
    steps
}

/// The legacy text a game told a Valkyrie of Tyr, as the profile keeps it.
fn kept_legacy() -> Vec<String> {
    [
        "It is written in the Book of Tyr:",
        "",
        "    After the Creation, the cruel god Moloch rebelled",
        "    against the authority of Marduk the Creator.",
        "    Moloch stole from Marduk the most powerful of all",
        "    the artifacts of the gods, the Amulet of Yendor,",
        "    and he hid it in the dark cavities of Gehennom, the",
        "    Under World, where he now lurks, and bides his time.",
        "",
        "Your god Tyr seeks to possess the Amulet, and with it",
        "to gain deserved ascendance over the other gods.",
        "",
        "You, a newly trained Stripling, have been heralded",
        "from birth as the instrument of Tyr.  You are destined",
        "to recover the Amulet for your deity, or die in the",
        "attempt.  Your hour of destiny has come.  For the sake",
        "of us all:  Go bravely with Tyr!",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// `prologue-replay`: a profile that kept a prologue offers it again on
/// the title; told over the title's scene, Esc ends it and the title is
/// back.
pub(super) fn prologue_replay() -> Vec<Step> {
    vec![
        Step::Call("a prologue told before, kept in the profile", |g| {
            g.update_profile(|p| {
                p.legacy = kept_legacy();
                p.last_role = Some("Val".into());
                p.last_female = Some(true);
            });
            Ok(())
        }),
        Step::Wait("the title screen, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        Step::Push(UiEvent::ReplayPrologue),
        Step::Wait("the Book, over the title", |g| shown(g, Beat::Book)),
        Step::Shot("prologue-replay"),
        Step::Call("its words, word for word", words),
        // (no request waits on the title: the key goes in as it is)
        Step::Call("Esc", |_| {
            super::settings_tests::tap_key(GKey::ESCAPE);
            Ok(())
        }),
        Step::Wait("the title again, its scene behind it", |g| {
            fail_on_error_screen(g)?;
            Ok(g.prologue.is_none()
                && screen(g) == Some("title")
                && g.ui.as_ref().is_some_and(|ui| {
                    !ui.prologue.is_visible() && ui.map.title_shown() && ui.map.title_drawn()
                }))
        }),
        Step::Shot("prologue-replay-title"),
    ]
}

pub(super) fn prologue_skip() -> Vec<Step> {
    let mut steps = begin();
    steps.extend([
        Step::Wait("Moloch's rebellion", |g| shown(g, Beat::Rebellion)),
        Step::Press(GKey::ESCAPE, '\u{1b}', false),
        Step::Wait("the game at once", in_game),
        Step::Shot("prologue-skipped"),
    ]);
    steps.extend(quit());
    steps
}
