//! `portrait`: the HUD's medallion shows the hero rendered, head and
//! shoulders, in place of the role's initial: a Wizard, a Samurai and a
//! Valkyrie, each a game of their own (none showing the last game's
//! face before their own is rendered). It renders when the look changes
//! and not every frame: once for a hero, not again while they only stand
//! there, again when the Valkyrie swaps the spear for the dagger, and
//! again when she polymorphs (#polyself, a debug-mode game) into a red
//! dragon, framed as it is.
//!
//! Its pixels need a window (a headless run renders nothing): there the
//! render is checked to draw the figure in the middle, its head near the
//! top and its shoulders down to the bottom. With `--screenshots`, a
//! picture of the screen for each.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use godot::classes::{DisplayServer, Engine};
use godot::obj::Singleton;
use nh_world::{Key, KeyInput, KeyProfile, Prompt};

use super::{Step, command, key, quit, start_as};
use crate::game::RenethackGame;
use crate::ui_events::{CharacterChoice, UiEvent};

const SEED: u64 = 17;

/// Renders made when the last one was checked, and the frame then.
static RENDERS: AtomicU32 = AtomicU32::new(0);
static FRAME: AtomicU64 = AtomicU64::new(0);
/// Frames the hero stands still with no render made.
const STILL_FRAMES: u64 = 90;

fn hero(role: &str, gender: &str, align: &str) -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: role.into(),
        race: "human".into(),
        gender: gender.into(),
        align: align.into(),
        profile: KeyProfile::Classic,
    }
}

fn frames() -> u64 {
    Engine::singleton().get_process_frames()
}

fn shown(g: &RenethackGame) -> Result<(bool, u32), String> {
    Ok(g.ui.as_ref().ok_or("no UI")?.hud.portrait_shown())
}

/// The render shows a figure: in the middle, the head near the top, the
/// shoulders down to the bottom (`whole`: a form framed whole, anywhere
/// inside). Nothing to check without a renderer.
fn framed(g: &RenethackGame, whole: bool) -> Result<bool, String> {
    if DisplayServer::singleton().get_name() == "headless" {
        return Ok(true);
    }
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some(b) = ui.hud.portrait_drawn_box() else {
        return Ok(false);
    };
    godot::global::godot_print!("selftest: portrait: drawn {b:?}");
    let middle = b.position.x + b.size.x / 2.0;
    let ok = if whole {
        b.size.x > 0.4 || b.size.y > 0.4
    } else {
        (0.3..=0.7).contains(&middle) && b.position.y < 0.3 && b.end().y > 0.9
    };
    if !ok {
        return Err(format!("the portrait draws {b:?}"));
    }
    Ok(true)
}

/// A new portrait rendered since the last one checked, shown, and framed:
/// head and shoulders, or (`whole`) a form framed whole.
fn rendered_anew(whole: bool) -> Step {
    if whole {
        Step::Wait("a portrait rendered anew, the form whole", |g| {
            anew(g, true)
        })
    } else {
        Step::Wait("a portrait rendered anew, head and shoulders", |g| {
            anew(g, false)
        })
    }
}

fn anew(g: &RenethackGame, whole: bool) -> Result<bool, String> {
    let (on, renders) = shown(g)?;
    let was = RENDERS.load(Ordering::Relaxed);
    if !on || renders <= was {
        return Ok(false);
    }
    // the frame after the render: its pixels are there
    if FRAME.load(Ordering::Relaxed) == 0 {
        FRAME.store(frames(), Ordering::Relaxed);
        return Ok(false);
    }
    if frames() < FRAME.load(Ordering::Relaxed) + 2 || !framed(g, whole)? {
        return Ok(false);
    }
    RENDERS.store(renders, Ordering::Relaxed);
    FRAME.store(0, Ordering::Relaxed);
    Ok(true)
}

/// No render while the hero only stands there.
fn not_again() -> Vec<Step> {
    vec![
        Step::Call("", |_| {
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("no render while nothing changes", |g| {
            let (_, renders) = shown(g)?;
            if renders != RENDERS.load(Ordering::Relaxed) {
                return Err(format!("{renders} renders, the look unchanged"));
            }
            let done = frames() >= FRAME.load(Ordering::Relaxed) + STILL_FRAMES;
            if done {
                FRAME.store(0, Ordering::Relaxed);
            }
            Ok(done)
        }),
    ]
}

fn game(choice: CharacterChoice, shot: &'static str) -> Vec<Step> {
    let mut steps = start_as(choice);
    steps.extend([
        // the last game's hero is not this one's face
        Step::Call("no other game's face", |g| {
            let (on, renders) = shown(g)?;
            if on && renders <= RENDERS.load(Ordering::Relaxed) {
                return Err("the portrait shows the last game's hero".into());
            }
            Ok(())
        }),
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        rendered_anew(false),
        Step::Shot(shot),
    ]);
    steps.extend(not_again());
    steps
}

pub(super) fn portrait() -> Vec<Step> {
    let mut steps = vec![Step::Call("", |_| {
        RENDERS.store(0, Ordering::Relaxed);
        FRAME.store(0, Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(game(hero("wizard", "female", "neutral"), "portrait-wizard"));
    steps.extend(quit());
    steps.push(Step::Push(UiEvent::BackToTitle));
    steps.extend(game(hero("samurai", "male", "lawful"), "portrait-samurai"));
    steps.extend(quit());
    steps.extend([
        Step::Call("debug mode for the Valkyrie's game", |g| {
            g.debug_mode = true;
            g.seed = Some(SEED);
            Ok(())
        }),
        Step::Push(UiEvent::BackToTitle),
    ]);
    steps.extend(game(
        // debug mode's name, which its save takes
        CharacterChoice {
            name: "wizard".into(),
            ..hero("valkyrie", "female", "neutral")
        },
        "portrait-valkyrie",
    ));
    steps.extend([
        key('x'),
        Step::Request("a command, the dagger in hand", command),
        rendered_anew(false),
        Step::Shot("portrait-valkyrie-dagger"),
    ]);
    // invisible: the map draws no hero, the portrait keeps its last render
    steps.extend(super::audio_tests::wish("uncursed ring of invisibility"));
    steps.extend([
        key('P'),
        Step::Request("what to put on", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the ring", |g| {
            (g.world.inventory.items().iter())
                // (not known by its name yet: "an opal ring")
                .find(|i| i.class == '=')
                .map(|i| KeyInput::plain(Key::Char(i.letter)))
                .ok_or_else(|| "no ring".to_string())
        }),
        Step::AnswerUntil('r', "a command, the ring on", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Call("", |_| {
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("invisible: the map draws no hero", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.map.hero_view().is_none())
        }),
        Step::Wait("invisible: still the hero's portrait", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.hud.portrait_shown().0 {
                return Err("the portrait lost the hero's render while invisible".into());
            }
            Ok(frames() > FRAME.load(Ordering::Relaxed) + STILL_FRAMES)
        }),
        Step::Shot("portrait-invisible"),
        // saved and restored invisible: the picture kept with the save
        key('S'),
        Step::Request(
            "Really save?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        key('y'),
        Step::AnswerUntil(' ', "the title after the save", |g| {
            super::fail_on_error_screen(g)?;
            Ok(super::screen(g) == Some("title"))
        }),
        Step::Push(UiEvent::ContinueGame("wizard".into())),
        // (debug mode asks whether to keep the save file)
        Step::AnswerUntil('n', "a command in the restored game", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Wait("restored invisible: the hero's saved picture", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if DisplayServer::singleton().get_name() == "headless" {
                return Ok(true);
            }
            Ok(ui.map.hero_view().is_none() && ui.hud.portrait_shown().0)
        }),
        Step::Shot("portrait-restored-invisible"),
    ]);
    steps.extend(super::polyself_tests::become_("red dragon"));
    steps.extend([
        Step::Wait("the hero drawn as the dragon", |g| {
            Ok(g.world.status.polymorphed())
        }),
        rendered_anew(true),
        Step::Shot("portrait-dragon"),
    ]);
    steps.extend(not_again());
    steps
}
