//! What the hero's body shows of their state, on the real engine in debug
//! mode: the hero notice tells the trap the hero is caught in (the hero's
//! glyph hides it), and the status line's opt-in Trap condition is on.
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::{Catalog, Reply};
use nh_world::*;

const SEED: u64 = 42;

/// The vi-keys of the eight steps, in the order of `dirchars`.
const STEPS: [((i32, i32), char); 8] = [
    ((-1, 0), 'h'),
    ((-1, -1), 'y'),
    ((0, -1), 'k'),
    ((1, -1), 'u'),
    ((1, 0), 'l'),
    ((1, 1), 'n'),
    ((0, 1), 'j'),
    ((-1, 1), 'b'),
];

/// A step off the hero's cell onto open floor, and the step back.
fn way_off(world: &World, catalog: &Catalog) -> Option<(char, char)> {
    let (hx, hy) = world.map.hero()?;
    STEPS.iter().find_map(|&((dx, dy), key)| {
        let cell = world.map.cell(hx + dx, hy + dy)?;
        let open = matches!(
            cell_terrain(cell, catalog),
            Some(Terrain::Floor | Terrain::DarkFloor | Terrain::Corridor)
        ) && cell.entity().is_none();
        let back = STEPS
            .iter()
            .find(|((bx, by), _)| (*bx, *by) == (-dx, -dy))?
            .1;
        open.then_some((key, back))
    })
}

/// The letter of a beartrap (the tool) in the pack.
fn beartrap(world: &World) -> Option<char> {
    let items = world.inventory.items();
    items
        .iter()
        .find(|i| i.text.contains("beartrap"))
        .map(|i| i.letter)
}

/// Step off the stairs the game starts on, wish for `trap` under the hero
/// (a wish for a bear trap makes the tool, which the hero sets), then step
/// off and back onto it until it holds
/// the hero (a trap seen is escaped one time in five), then walk until
/// free. What the World said meanwhile: (the traps it named, the
/// conditions while caught, freed).
fn caught_in(role: &str, trap: &'static str) -> (Vec<TrapKind>, Vec<String>, bool) {
    #[derive(Debug, PartialEq)]
    enum Stage {
        Start,
        Wish,
        Arm,
        Off,
        Back,
        Caught,
    }
    let mut stage = Stage::Start;
    let mut way = None;
    let mut seen = Vec::new();
    let mut conditions = Vec::new();
    let mut freed = false;
    let mut prompts = 0;
    play(SEED, role, &mut |w, cat, p| {
        prompts += 1;
        assert!(prompts < 300, "{trap}: no end at {stage:?}");
        if let Some(t) = w.hero_trap
            && seen.last() != Some(&t)
        {
            seen.push(t);
        }
        match p {
            Prompt::Text { query, .. } if query.contains("wish") => {
                return Act::Reply(Reply::Text(trap.into()));
            }
            // the tool wished for: set it here
            Prompt::FreeKey { query, .. } if query.contains("apply") => {
                let letter = beartrap(w).expect("the beartrap in the pack");
                return Act::Reply(Reply::Char(letter as i32));
            }
            // "Really step onto that bear trap?"
            Prompt::Choice { query, .. } if query.contains("Really") => {
                return Act::Reply(Reply::Char('y' as i32));
            }
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        match stage {
            // off the stairs: no trap goes there
            Stage::Start => {
                let (off, _) = way_off(w, cat).expect("no open floor next to the stairs");
                stage = Stage::Wish;
                Act::Reply(Reply::Key(off as i32))
            }
            Stage::Wish => {
                way = way_off(w, cat);
                assert!(way.is_some(), "no open floor next to the hero");
                stage = if trap == "bear trap" {
                    Stage::Arm
                } else {
                    Stage::Off
                };
                Act::Reply(Reply::Key(23)) // ^W
            }
            // setting it takes a few turns; a pet's doings may interrupt
            // it, and applying it again goes on with it
            Stage::Arm if beartrap(w).is_some() => Act::Reply(Reply::Key('a' as i32)),
            Stage::Arm => {
                assert!(
                    w.log.iter().any(|m| m.text.contains("finish arming")),
                    "the bear trap was not set"
                );
                stage = Stage::Back;
                Act::Reply(Reply::Key(way.unwrap().0 as i32))
            }
            Stage::Off => {
                stage = Stage::Back;
                Act::Reply(Reply::Key(way.unwrap().0 as i32))
            }
            Stage::Back if w.hero_trap.is_some() => {
                stage = Stage::Caught;
                conditions = w.status.condition_names(cat);
                Act::Reply(Reply::Key(way.unwrap().0 as i32))
            }
            // escaped it, or stepped back: off and on again
            Stage::Back => {
                let (off, back) = way.unwrap();
                let on_it = w
                    .log
                    .iter()
                    .last()
                    .is_some_and(|m| m.text.contains("escape"));
                Act::Reply(Reply::Key(if on_it { off } else { back } as i32))
            }
            Stage::Caught if w.hero_trap.is_none() => {
                freed = true;
                Act::Quit
            }
            // struggling: every try is a turn
            Stage::Caught => Act::Reply(Reply::Key(way.unwrap().0 as i32)),
        }
    });
    (seen, conditions, freed)
}

#[test]
fn a_bear_trap_holds_the_hero_until_they_pull_free() {
    let (seen, conditions, freed) = caught_in("archeologist", "bear trap");
    assert_eq!(seen, [TrapKind::BearTrap]);
    assert!(conditions.contains(&"Trap".to_string()), "{conditions:?}");
    assert!(freed);
}

#[test]
fn a_web_holds_a_hero_too_weak_to_tear_it() {
    // an archeologist's strength does not tear through it (a valkyrie's may)
    let (seen, conditions, freed) = caught_in("archeologist", "web");
    assert_eq!(seen, [TrapKind::Web]);
    assert!(conditions.contains(&"Trap".to_string()), "{conditions:?}");
    assert!(freed);
}
