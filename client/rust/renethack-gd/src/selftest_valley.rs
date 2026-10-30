//! The Valley of the Dead is known as the hero arrives (`valley`): NetHack
//! names it then ("You arrive at the Valley of the Dead..."), so the level
//! notice says so at once, before any `#overview`, and the Valley's own
//! grey air comes with the level (a debug level teleport there).

use nh_world::Prompt;

use super::{DialogEvent, Step, ctrl_key, entry_key, idle_command, is_menu, logged, quit, start};
use crate::game::RenethackGame;

fn lair(g: &RenethackGame) -> Option<String> {
    g.world.level.as_ref().and_then(|l| l.lair.clone())
}

pub(super) fn valley() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Wait("no lair on the first level", |g| Ok(lair(g).is_none())),
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the Valley", |g| entry_key(g, "valley")),
        Step::AnswerUntil('n', "a command in the Valley", idle_command),
        Step::Wait("the Valley named on arrival, no overview asked", |g| {
            if !logged(g, "Valley of the Dead") {
                return Ok(false);
            }
            match lair(g).as_deref() {
                Some("valley") => Ok(true),
                other => Err(format!("the level notice's lair is {other:?}")),
            }
        }),
        Step::Shot("valley-arrival"),
    ]);
    steps.extend(quit());
    steps
}
