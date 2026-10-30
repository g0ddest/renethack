//! Scenes for timing frames (`tools/perf.py`), in debug mode. `crowd`: a
//! crowded room. About thirty peaceful creatures of many kinds are made
//! round the hero: animals, humanoids in their kit, procedural bodies, a
//! vortex, a cloud and an elemental. The hero then waits forty turns among
//! them as they move. `deep`: the deep dungeon. The hero is teleported to
//! Medusa's island, the castle, the levels of Gehennom, Vlad's Tower and
//! the Planes of Air and Water, and at each the creatures of the classes
//! found there are made and the hero waits among them a while: what the
//! first levels never show (demons, liches, dragons, the Valley's dead, a
//! ghost, the sky and the deep) is drawn there for the first time.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{
    CharacterChoice, DialogEvent, Step, camera_settled, ctrl_key, entry_key, fail_on_error_screen,
    is_menu, map_view, quit, smoke_choice, start_as,
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

/// The levels `deep` visits (the debug level teleport's names) and the
/// creatures made at each (`^G`'s names: a class's letter makes one of the
/// class at random).
const DEEP: [(&str, &[&str]); 11] = [
    (
        "medusa",
        &[
            "2 peaceful ;",
            "2 peaceful S",
            "1 peaceful N",
            "1 peaceful c",
        ],
    ),
    (
        "castle",
        &[
            "2 peaceful D",
            "2 peaceful T",
            "2 peaceful '",
            "2 peaceful @",
        ],
    ),
    (
        "valley",
        &[
            "2 peaceful Z",
            "2 peaceful M",
            "2 peaceful W",
            "1 peaceful L",
            "1 peaceful ghost",
        ],
    ),
    (
        "juiblex",
        &[
            "2 peaceful P",
            "2 peaceful b",
            "2 peaceful j",
            "2 peaceful F",
            "1 peaceful m",
        ],
    ),
    (
        "orcus",
        &[
            "2 peaceful &",
            "2 peaceful i",
            "2 peaceful X",
            "2 peaceful z",
        ],
    ),
    (
        "asmodeus",
        &[
            "2 peaceful H",
            "2 peaceful U",
            "2 peaceful O",
            "2 peaceful K",
        ],
    ),
    (
        "baalz",
        &[
            "2 peaceful R",
            "2 peaceful s",
            "2 peaceful x",
            "2 peaceful a",
            "2 peaceful J",
        ],
    ),
    (
        "wizard1",
        &[
            "2 peaceful L",
            "2 peaceful e",
            "2 peaceful v",
            "2 peaceful E",
            "2 peaceful y",
        ],
    ),
    (
        "tower1",
        &[
            "2 peaceful V",
            "2 peaceful B",
            "2 peaceful Y",
            "2 peaceful q",
            "2 peaceful u",
        ],
    ),
    // the Planes of Air and Water: the sea of clouds, the clouds, the
    // deep and a bubble's skin
    ("air:", &["2 peaceful E", "2 peaceful v"]),
    ("water:", &["2 peaceful ;", "1 peaceful E"]),
];

/// Turns the hero waits on each deep level.
const DEEP_TURNS: u32 = 25;

/// The deep level now (one past it: `next_level` counts first).
static LEVEL: AtomicUsize = AtomicUsize::new(0);
static DEEP_WAITED: AtomicU32 = AtomicU32::new(0);

/// The debug level teleport's menu entry of the level now.
fn level_entry(g: &RenethackGame) -> Result<KeyInput, String> {
    let i = LEVEL.load(Ordering::Relaxed);
    let (name, _) = DEEP.get(i.wrapping_sub(1)).ok_or("no level now")?;
    entry_key(g, name)
}

/// Search a turn at a time, `DEEP_TURNS` times.
fn wait_deep(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    Ok(match g.pending.as_ref().map(|(_, p)| p) {
        Some(Prompt::Command) => {
            if DEEP_WAITED.fetch_add(1, Ordering::Relaxed) >= DEEP_TURNS {
                return Ok(None);
            }
            Some(plain('s'))
        }
        Some(Prompt::Choice { .. }) => Some(plain('n')),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

pub(super) fn deep() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start_as(CharacterChoice {
        role: "priest".into(),
        ..smoke_choice()
    }));
    steps.push(Step::Wait("the hero on the map", |g| {
        fail_on_error_screen(g)?;
        Ok(g.world.map.hero().is_some())
    }));
    for (name, creatures) in DEEP {
        steps.extend([
            Step::Call("the next level", |_| {
                LEVEL.fetch_add(1, Ordering::Relaxed);
                DEEP_WAITED.store(0, Ordering::Relaxed);
                Ok(())
            }),
            Step::Key(ctrl_key('v')),
            Step::Request(
                "to what level",
                |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
            ),
            Step::Dialog(DialogEvent::TextSubmitted("?".into())),
            Step::Request("the levels to teleport to", is_menu),
            Step::KeyFrom("the level", level_entry),
        ]);
        // the endgame reached by the debug teleport: the Amulet comes with
        // a wish, refused
        if name == "air:" {
            steps.extend([
                Step::Request(
                    "the Amulet's wish",
                    |p| matches!(p, Prompt::Text { query, .. } if query.contains("wish")),
                ),
                Step::Dialog(DialogEvent::TextSubmitted("nothing".into())),
            ]);
        }
        steps.extend([
            Step::AnswerUntil('n', "a command on the level", at_command),
            Step::Wait("the level drawn", |g| {
                let drawn = map_view(g)?.drawn_generation();
                Ok(drawn == Some(g.world.map.generation()) && camera_settled(g)?)
            }),
        ]);
        // with no ground under them the hero's ring still marks them
        if matches!(name, "air:" | "water:") {
            steps.push(Step::Wait("the hero's ring on no ground", |g| {
                Ok(map_view(g)?.hero_ring_drawn())
            }));
        }
        for what in creatures.iter() {
            steps.extend(genesis(what));
        }
        steps.push(Step::KeysUntil("turns among them", wait_deep));
    }
    steps.extend(quit());
    steps
}
