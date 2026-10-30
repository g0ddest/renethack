//! The lair the hero knows their level to be, on the real engine in debug
//! mode: teleported to the Valley of the Dead, which NetHack names on
//! arrival ("You arrive at the Valley of the Dead..."), the level notice
//! says "valley" at once; teleported to Moloch's Sanctum, it says only the
//! dungeon and depth until the level is mapped and the overview names its
//! temple, then "sanctum".
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
fn the_valley_is_known_on_arrival_the_sanctum_once_the_overview_names_it() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stage {
        Start,
        Valley,
        Sanctum,
        Mapped,
    }
    let mut stage = Stage::Start;
    let mut valley = None;
    let mut sanctum = None;
    let mut known = None;
    let mut prompts = 0;
    play(SEED, "valkyrie", &mut |w, _, p| {
        prompts += 1;
        assert!(prompts < 300, "no end at {stage:?}");
        match p {
            // the level teleport: its menu of special levels
            Prompt::Text { query, .. } if query.contains("level") => {
                return Act::Reply(Reply::Text("?".into()));
            }
            Prompt::Menu {
                how, title, items, ..
            } if matches!(stage, Stage::Start | Stage::Valley) => {
                let to = if stage == Stage::Start {
                    "valley"
                } else {
                    "sanctum"
                };
                let mut m = MenuState::new(*how, title.clone(), items);
                let letter = m
                    .entries
                    .iter()
                    .find(|e| e.selectable && e.text.contains(to))
                    .and_then(|e| e.letter)
                    .unwrap_or_else(|| panic!("{to} in the level teleport's menu"));
                return Act::Reply(match m.key(letter) {
                    MenuOutcome::Done(r) => r,
                    _ => m.confirm(),
                });
            }
            Prompt::ExtCmd => return Act::Reply(Reply::ExtCmd(Some("overview".into()))),
            Prompt::Choice { .. } => return Act::Reply(Reply::Char('n' as i32)),
            Prompt::Command => {}
            other => return Act::Reply(other.escape_reply()),
        }
        let depth = w.depth().unwrap_or(0);
        let lair = w.level.as_ref().and_then(|l| l.lair.clone());
        let (next, reply) = match stage {
            Stage::Start if depth <= 20 => (Stage::Start, Reply::Key(LEVEL_TELEPORT)),
            Stage::Start => {
                valley = Some(lair);
                (Stage::Valley, Reply::Key(LEVEL_TELEPORT))
            }
            // still in the Valley: the teleport did not happen yet
            Stage::Valley if valley.is_some() && lair.as_deref() == Some("valley") => {
                (Stage::Valley, Reply::Key(LEVEL_TELEPORT))
            }
            Stage::Valley => {
                sanctum = Some(lair);
                (Stage::Sanctum, Reply::Key(MAP))
            }
            Stage::Sanctum => (Stage::Mapped, Reply::Key('#' as i32)),
            Stage::Mapped => {
                known = Some(lair);
                return Act::Quit;
            }
        };
        stage = next;
        Act::Reply(reply)
    });
    assert_eq!(
        valley,
        Some(Some("valley".to_string())),
        "the Valley not told on arrival, which NetHack names"
    );
    assert_eq!(
        sanctum,
        Some(None),
        "the Sanctum told before the overview names it"
    );
    assert_eq!(
        known,
        Some(Some("sanctum".to_string())),
        "the Sanctum not told once the overview names it"
    );
}
