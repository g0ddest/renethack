//! `crowd`: a crowded room, for timing frames (`tools/perf.py`). In debug
//! mode about thirty peaceful creatures of many kinds are made round the
//! hero: animals, humanoids in their kit, procedural bodies, a vortex, a
//! cloud and an elemental. The hero then waits forty turns among them as
//! they move.

use std::sync::atomic::{AtomicU32, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{
    CharacterChoice, DialogEvent, Step, camera_settled, ctrl_key, fail_on_error_screen, quit,
    smoke_choice, start_as,
};
use crate::game::RenethackGame;

/// The creatures made, by `^G`'s names (peaceful: the hero lives through
/// it).
const CROWD: [&str; 17] = [
    "3 peaceful jackal",
    "2 peaceful giant rat",
    "2 peaceful newt",
    "2 peaceful dwarf",
    "2 peaceful hobbit",
    "2 peaceful gnome lord",
    "2 peaceful hill orc",
    "2 peaceful homunculus",
    "2 peaceful giant beetle",
    "1 peaceful floating eye",
    "2 peaceful acid blob",
    "1 peaceful dust vortex",
    "1 peaceful fog cloud",
    "1 peaceful air elemental",
    "2 peaceful soldier ant",
    "2 peaceful wolf",
    "1 peaceful pony",
];

/// Turns the hero waits among them.
const TURNS: u32 = 40;

static WAITED: AtomicU32 = AtomicU32::new(0);

fn plain(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

fn at_command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(matches!(g.pending, Some((_, Prompt::Command))))
}

/// `^G` with `what`: the creatures made next to the hero.
fn genesis(what: &'static str) -> [Step; 4] {
    [
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::AnswerUntil('n', "a command after the creation", at_command),
    ]
}

/// Search a turn at a time, `TURNS` times.
fn wait(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    Ok(match g.pending.as_ref().map(|(_, p)| p) {
        Some(Prompt::Command) => {
            if WAITED.fetch_add(1, Ordering::Relaxed) >= TURNS {
                return Ok(None);
            }
            Some(plain('s'))
        }
        Some(Prompt::Choice { .. }) => Some(plain('n')),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

pub(super) fn crowd() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start_as(CharacterChoice {
        role: "archeologist".into(),
        ..smoke_choice()
    }));
    steps.push(Step::Wait("the hero on the map", |g| {
        fail_on_error_screen(g)?;
        Ok(g.world.map.hero().is_some())
    }));
    for what in CROWD {
        steps.extend(genesis(what));
    }
    steps.extend([
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("crowd"),
        Step::KeysUntil("forty turns among them", wait),
    ]);
    steps.extend(quit());
    steps
}
