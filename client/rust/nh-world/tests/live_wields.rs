//! What a monster holds, on the real engine in debug mode: a soldier made
//! beside the hero takes up her weapon, and while the hero sees her the
//! engine tells what she holds (an object's glyph: its look, never its
//! type) on her cell; the hero's own weapon is never told. Blindfolded,
//! the hero sees nothing held.
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::{Glyph, GlyphKind, Reply};
use nh_world::*;

const SEED: u64 = 42;
/// ^G: debug mode's monster making; ^W: its wish.
const GENESIS: i32 = 7;
const WISH: i32 = 23;
/// Turns waited at most for the soldier to take up her weapon.
const TURNS: u32 = 20;

/// The soldier's cell and what the engine tells it holds.
fn soldier(world: &World, catalog: &nh_protocol::Catalog) -> Option<((i32, i32), Option<Glyph>)> {
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            let cell = world.map.cell(x, y)?;
            let g = cell.glyph.as_ref()?;
            let named = g.kind == GlyphKind::Mon
                && g.mon
                    .and_then(|m| catalog.monsters.iter().find(|i| i.idx == m))
                    .is_some_and(|m| m.name == "soldier");
            named.then(|| ((x, y), cell.wields.clone()))
        })
}

#[test]
fn a_monster_seen_is_told_holding_its_weapon() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        Made,
        /// Turns waited, and whether the `m` forcing the next search went.
        Waiting(u32, bool),
        WishBlindfold,
        PutBlindfold,
        Blind,
    }
    let mut stage = Stage::Start;
    let (mut held, mut hero_held, mut unseen) = (None, false, None);
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, cat, p| {
        prompts += 1;
        assert!(prompts < 300, "no end at {stage:?}");
        match p {
            Prompt::Text { query, .. } if query.contains("Create what kind") => {
                return Act::Reply(Reply::Text("soldier".into()));
            }
            Prompt::Text { query, .. } if query.contains("wish") => {
                return Act::Reply(Reply::Text("uncursed blindfold".into()));
            }
            Prompt::FreeKey { query, .. } if query.contains("put on") => {
                let items = w.inventory.items();
                let letter = items
                    .iter()
                    .find(|i| i.text.contains("blindfold"))
                    .map(|i| i.letter)
                    .expect("no blindfold in the pack");
                return Act::Reply(Reply::Char(letter as i32));
            }
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        // the hero's own cell never tells what they hold
        if let Some((hx, hy)) = w.map.hero() {
            hero_held |= w.map.cell(hx, hy).is_some_and(|c| c.wields.is_some());
        }
        let (next, reply) = match stage {
            Stage::Start => (Stage::Made, Reply::Key(GENESIS)),
            Stage::Made => (Stage::Waiting(0, true), Reply::Key('m' as i32)),
            // a turn searched, forced by `m` (a search beside a hostile is
            // refused otherwise)
            Stage::Waiting(n, true) => (Stage::Waiting(n + 1, false), Reply::Key('s' as i32)),
            Stage::Waiting(n, false) => {
                if let Some((_, Some(g))) = soldier(w, cat) {
                    held = Some(g);
                    (Stage::WishBlindfold, Reply::Key(WISH))
                } else {
                    if n >= TURNS {
                        for m in w.log.iter() {
                            eprintln!("LOG {}", m.text);
                        }
                        eprintln!("SOLDIER {:?}", soldier(w, cat));
                    }
                    assert!(n < TURNS, "the soldier never took up a weapon");
                    (Stage::Waiting(n, true), Reply::Key('m' as i32))
                }
            }
            Stage::WishBlindfold => (Stage::PutBlindfold, Reply::Key('P' as i32)),
            Stage::PutBlindfold => (Stage::Blind, Reply::Key('s' as i32)),
            Stage::Blind => {
                unseen = Some(
                    (0..ROWNO)
                        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
                        .any(|(x, y)| w.map.cell(x, y).is_some_and(|c| c.wields.is_some())),
                );
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    let held = held.expect("the soldier seen holding nothing");
    assert_eq!(held.kind, GlyphKind::Obj, "an object's glyph: {held:?}");
    assert!(!hero_held, "the hero's own weapon told");
    assert_eq!(
        unseen,
        Some(false),
        "a weapon told held by a monster the hero does not see"
    );
}
