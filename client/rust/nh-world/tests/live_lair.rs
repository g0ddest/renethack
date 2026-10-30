//! A lair the overview names, on the real engine in debug mode: the hero
//! teleported to the Valley of the Dead is told only its dungeon and
//! depth; once the level is mapped and the overview names its temple of
//! Moloch, the level notice says "valley".
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

mod common;

use common::{Act, play};
use nh_protocol::Reply;
use nh_world::*;

const SEED: u64 = 42;
/// ^V: debug mode's level teleport; ^F: its map of the level.
const LEVEL_TELEPORT: i32 = 22;
const MAP: i32 = 6;

#[test]
fn the_overview_names_the_valley_once_it_is_known() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        Mapped,
        Overviewed,
    }
    let mut stage = Stage::Start;
    let mut arrived = None;
    let mut known = None;
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, _, p| {
        prompts += 1;
        assert!(prompts < 200, "no end at {stage:?}");
        match p {
            // the level teleport: its menu of special levels, the Valley
            Prompt::Text { query, .. } if query.contains("level") => {
                return Act::Reply(Reply::Text("?".into()));
            }
            Prompt::Menu {
                how, title, items, ..
            } if stage == Stage::Start => {
                let mut m = MenuState::new(*how, title.clone(), items);
                let valley = m
                    .entries
                    .iter()
                    .find(|e| e.selectable && e.text.contains("valley"))
                    .and_then(|e| e.letter)
                    .expect("the Valley in the level teleport's menu");
                return Act::Reply(match m.key(valley) {
                    MenuOutcome::Done(r) => r,
                    _ => m.confirm(),
                });
            }
            Prompt::ExtCmd => return Act::Reply(Reply::ExtCmd(Some("overview".into()))),
            Prompt::Choice { .. } => return Act::Reply(Reply::Char('n' as i32)),
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        let in_valley = w.depth().is_some_and(|d| d > 20);
        let lair = w.level.as_ref().and_then(|l| l.lair.clone());
        let (next, reply) = match stage {
            Stage::Start if !in_valley => (Stage::Start, Reply::Key(LEVEL_TELEPORT)),
            Stage::Start => {
                arrived = Some(lair);
                (Stage::Mapped, Reply::Key(MAP))
            }
            Stage::Mapped => (Stage::Overviewed, Reply::Key('#' as i32)),
            Stage::Overviewed => {
                known = Some(lair);
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    assert_eq!(
        arrived,
        Some(None),
        "a lair told before the overview names it"
    );
    assert_eq!(
        known,
        Some(Some("valley".to_string())),
        "the Valley not told once the overview names it"
    );
}
