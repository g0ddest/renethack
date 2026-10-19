//! Whether a monster is peaceful, on the real engine in debug mode: a
//! peaceful gnome and a hostile one made beside the hero are told apart on
//! their cells (as farlook tells it, per monster, not per species); the
//! tame kitten and the hero's own cell are never peaceful. A peaceful ogre
//! the hero attacks is angered, its glyph the same: the engine draws its
//! cell again, no longer peaceful. Hallucinating, the hero is told nothing
//! peaceful.
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::{GlyphKind, Reply, mg};
use nh_world::*;

const SEED: u64 = 42;
/// ^G: debug mode's monster making; ^W: its wish.
const GENESIS: i32 = 7;
const WISH: i32 = 23;
/// The steps of the keys hjklyubn.
const STEPS: [((i32, i32), char); 8] = [
    ((-1, 0), 'h'),
    ((1, 0), 'l'),
    ((0, -1), 'k'),
    ((0, 1), 'j'),
    ((-1, -1), 'y'),
    ((1, -1), 'u'),
    ((-1, 1), 'b'),
    ((1, 1), 'n'),
];

/// The cells of the monsters named `name`, and whether each is told
/// peaceful.
fn named(world: &World, catalog: &nh_protocol::Catalog, name: &str) -> Vec<((i32, i32), bool)> {
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let cell = world.map.cell(x, y)?;
            let g = cell.glyph.as_ref()?;
            let is = g.kind == GlyphKind::Mon
                && g.flags & mg::HERO == 0
                && g.mon
                    .and_then(|m| catalog.monsters.iter().find(|i| i.idx == m))
                    .is_some_and(|m| m.name == name);
            is.then_some(((x, y), cell.peaceful))
        })
        .collect()
}

/// Whether any cell is told peaceful.
fn any_peaceful(world: &World) -> bool {
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .any(|(x, y)| world.map.cell(x, y).is_some_and(|c| c.peaceful))
}

#[test]
fn a_monster_is_told_peaceful_per_monster() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        MakeHostile,
        MakeOgre,
        Look,
        Fight,
        Attacked,
        WishPotion,
        Quaff,
        Hallucinating,
    }
    let mut stage = Stage::Start;
    let mut making = "";
    let mut gnomes = Vec::new();
    let mut ogre_after = None;
    let mut hero_or_pet_peaceful = false;
    let mut hallu_peaceful = None;
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, cat, p| {
        prompts += 1;
        assert!(prompts < 200, "no end at {stage:?}");
        match p {
            Prompt::Text { query, .. } if query.contains("Create what kind") => {
                return Act::Reply(Reply::Text(making.into()));
            }
            Prompt::Text { query, .. } if query.contains("wish") => {
                return Act::Reply(Reply::Text("potion of hallucination".into()));
            }
            Prompt::FreeKey { query, .. } if query.contains("drink") => {
                let letter = w
                    .inventory
                    .items()
                    .iter()
                    .find(|i| i.text.contains("potion"))
                    .map(|i| i.letter)
                    .expect("no potion in the pack");
                return Act::Reply(Reply::Char(letter as i32));
            }
            // "Really attack the ogre?"
            Prompt::Choice { query, .. } if query.starts_with("Really attack") => {
                return Act::Reply(Reply::Char('y' as i32));
            }
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        // the hero's own cell, and the tame kitten's, are never peaceful
        if let Some((hx, hy)) = w.map.hero() {
            hero_or_pet_peaceful |= w.map.cell(hx, hy).is_some_and(|c| c.peaceful);
        }
        hero_or_pet_peaceful |= named(w, cat, "kitten").iter().any(|&(_, p)| p);
        let (next, reply) = match stage {
            Stage::Start => {
                making = "peaceful gnome";
                (Stage::MakeHostile, Reply::Key(GENESIS))
            }
            Stage::MakeHostile => {
                making = "hostile gnome";
                (Stage::MakeOgre, Reply::Key(GENESIS))
            }
            Stage::MakeOgre => {
                making = "peaceful ogre";
                (Stage::Look, Reply::Key(GENESIS))
            }
            Stage::Look => {
                gnomes = named(w, cat, "gnome");
                assert_eq!(named(w, cat, "ogre").len(), 1, "one ogre beside the hero");
                assert!(named(w, cat, "ogre")[0].1, "the peaceful ogre told hostile");
                // fight it: F is a prefix, the next command its direction
                (Stage::Fight, Reply::Key('F' as i32))
            }
            Stage::Fight => {
                let hero = w.map.hero().expect("no hero");
                let (ogre, _) = named(w, cat, "ogre")[0];
                let way = (ogre.0 - hero.0, ogre.1 - hero.1);
                let key = STEPS
                    .iter()
                    .find(|(d, _)| *d == way)
                    .map(|(_, k)| *k)
                    .expect("the ogre is not beside the hero");
                (Stage::Attacked, Reply::Key(key as i32))
            }
            Stage::Attacked => {
                ogre_after = named(w, cat, "ogre").first().map(|&(_, p)| p);
                (Stage::WishPotion, Reply::Key(WISH))
            }
            Stage::WishPotion => (Stage::Quaff, Reply::Key('q' as i32)),
            Stage::Quaff => {
                hallu_peaceful = Some(any_peaceful(w));
                (Stage::Hallucinating, Reply::Key('s' as i32))
            }
            Stage::Hallucinating => {
                hallu_peaceful = Some(hallu_peaceful.unwrap_or(false) || any_peaceful(w));
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    // one gnome of each, told apart on their own cells
    let mut told: Vec<bool> = gnomes.iter().map(|&(_, p)| p).collect();
    told.sort();
    assert_eq!(told, vec![false, true], "the gnomes as told: {gnomes:?}");
    assert!(
        !hero_or_pet_peaceful,
        "the hero or the tame kitten told peaceful"
    );
    assert_eq!(
        ogre_after,
        Some(false),
        "the ogre angered still told peaceful (or gone)"
    );
    assert_eq!(
        hallu_peaceful,
        Some(false),
        "a monster told peaceful to a hallucinating hero"
    );
}
