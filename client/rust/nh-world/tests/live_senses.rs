//! What the hero only senses, on the real engine in debug mode: telepathy
//! draws a monster as it would be seen, and the engine tells it sensed. A
//! Valkyrie sees her kitten; with an amulet of ESP and a blindfold on she
//! senses it, and the map's cell says so.
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::{GlyphKind, Reply, mg};
use nh_world::*;

const SEED: u64 = 42;
/// ^W: debug mode's wish.
const WISH: i32 = 23;

/// The pet's cell, and whether the engine told it only sensed.
fn pet(world: &World) -> Option<((i32, i32), bool)> {
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            let cell = world.map.cell(x, y)?;
            let g = cell.glyph.as_ref()?;
            (g.kind == GlyphKind::Mon && g.flags & mg::PET != 0).then_some(((x, y), cell.sensed))
        })
}

#[test]
fn a_monster_only_sensed_is_told_so() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        WishAmulet,
        PutAmulet,
        WishBlindfold,
        PutBlindfold,
        Blind,
    }
    let mut stage = Stage::Start;
    let (mut seen, mut sensed) = (None, None);
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, _, p| {
        prompts += 1;
        assert!(prompts < 200, "no end at {stage:?}");
        let thing = match stage {
            Stage::WishAmulet | Stage::PutAmulet => "amulet",
            _ => "blindfold",
        };
        match p {
            Prompt::Text { query, .. } if query.contains("wish") => {
                let wish = match thing {
                    "amulet" => "blessed amulet of ESP",
                    _ => "uncursed blindfold",
                };
                return Act::Reply(Reply::Text(wish.into()));
            }
            Prompt::FreeKey { query, .. } if query.contains("put on") => {
                let items = w.inventory.items();
                let letter = items
                    .iter()
                    .find(|i| i.text.contains(thing))
                    .map(|i| i.letter)
                    .unwrap_or_else(|| panic!("no {thing} in the pack"));
                return Act::Reply(Reply::Char(letter as i32));
            }
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        let (next, reply) = match stage {
            Stage::Start => {
                seen = pet(w);
                (Stage::WishAmulet, Reply::Key(WISH))
            }
            Stage::WishAmulet => (Stage::PutAmulet, Reply::Key('P' as i32)),
            Stage::PutAmulet => (Stage::WishBlindfold, Reply::Key(WISH)),
            Stage::WishBlindfold => (Stage::PutBlindfold, Reply::Key('P' as i32)),
            // a turn blind: the pet drawn as telepathy shows it
            Stage::PutBlindfold => (Stage::Blind, Reply::Key('s' as i32)),
            Stage::Blind => {
                sensed = pet(w);
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    assert!(
        matches!(seen, Some((_, false))),
        "the kitten seen at the start: {seen:?}"
    );
    assert!(
        matches!(sensed, Some((_, true))),
        "the kitten sensed by a blind hero with ESP: {sensed:?}"
    );
}
