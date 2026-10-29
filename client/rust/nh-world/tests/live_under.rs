//! What the hero remembers under them, on the real engine in debug mode:
//! the up stairs a game starts on are known under the hero before they
//! ever step off them; a step onto bare floor shows the stairs on their
//! own cell and floor under the hero; a fountain wished up under the hero
//! is known there at once.
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::{Catalog, Reply};
use nh_world::*;

const SEED: u64 = 42;
/// ^W: debug mode's wish.
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

/// What the map knows at (x, y).
fn terrain(w: &World, cat: &Catalog, (x, y): (i32, i32)) -> Option<Terrain> {
    w.map.cell(x, y).and_then(|c| cell_terrain(c, cat))
}

#[test]
fn the_hero_is_told_what_they_stand_on() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        Stepped,
        Wished,
    }
    let mut stage = Stage::Start;
    let mut start = None;
    let mut stepped = None;
    let mut wished = None;
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, cat, p| {
        prompts += 1;
        assert!(prompts < 100, "no end at {stage:?}");
        match p {
            Prompt::Text { query, .. } if query.contains("wish") => {
                return Act::Reply(Reply::Text("fountain".into()));
            }
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        let hero = w.map.hero().expect("no hero");
        let (next, reply) = match stage {
            Stage::Start => {
                start = Some((hero, terrain(w, cat, hero)));
                // a step onto bare floor beside the stairs
                let bare = |(dx, dy): (i32, i32)| {
                    let at = (hero.0 + dx, hero.1 + dy);
                    matches!(
                        terrain(w, cat, at),
                        Some(Terrain::Floor | Terrain::DarkFloor)
                    ) && w.map.cell(at.0, at.1).is_some_and(|c| c.entity().is_none())
                };
                let key = STEPS
                    .iter()
                    .find(|&&(d, _)| bare(d))
                    .map(|&(_, k)| k)
                    .expect("no bare floor beside the stairs");
                (Stage::Stepped, Reply::Key(key as i32))
            }
            Stage::Stepped => {
                let (stairs, _) = start.expect("no start");
                stepped = Some((
                    hero != stairs,
                    terrain(w, cat, stairs),
                    terrain(w, cat, hero),
                ));
                (Stage::Wished, Reply::Key(WISH))
            }
            Stage::Wished => {
                wished = Some(terrain(w, cat, hero));
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    let (_, under) = start.expect("the game never started");
    assert_eq!(
        under,
        Some(Terrain::StairsUp),
        "the stairs under the hero at the start"
    );
    let (moved, stairs, under) = stepped.expect("no step");
    assert!(moved, "the hero did not step off the stairs");
    assert_eq!(stairs, Some(Terrain::StairsUp), "the stairs stepped off");
    assert!(
        matches!(under, Some(Terrain::Floor | Terrain::DarkFloor)),
        "the floor under the hero: {under:?}"
    );
    assert_eq!(
        wished.expect("no wish"),
        Some(Terrain::Fountain),
        "the fountain wished up under the hero"
    );
}
