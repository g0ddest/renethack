//! COMBAT only with something to fight (`calm`), in debug mode with seed
//! 1 (no hostile about at the start): a potion
//! of sickness costs HP with no creature in sight and starts no fight; a
//! jackal made beside the hero does; a level teleport mid-fight leaves it
//! behind on that level.

use nh_world::{Mode, Prompt};

use super::audio_tests::{letter, wish};
use super::{DialogEvent, Step, command, ctrl_key, idle_command, key, quit, start};
use crate::game::RenethackGame;

fn hp(g: &RenethackGame) -> Option<(i64, i64)> {
    Some((
        g.world.status.number("hp")?,
        g.world.status.number("hpmax")?,
    ))
}

/// The mode stays exploring for a few command prompts.
fn exploring(g: &RenethackGame) -> Result<bool, String> {
    match g.driver.mode() {
        Mode::Combat => {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let foes = nh_world::threats(&g.world, cat);
            let what: Vec<String> = foes
                .iter()
                .filter_map(|&(x, y)| g.world.map.cell(x, y))
                .map(|c| format!("{:?}", c.entity()))
                .collect();
            Err(format!(
                "COMBAT with nothing to fight (threats {foes:?} {what:?}, hero {:?})",
                g.world.hero()
            ))
        }
        Mode::Exploration => Ok(g.pending.as_ref().is_some_and(|(_, p)| command(p))),
    }
}

pub(super) fn calm() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 1 (no hostile about)", |g| {
        g.debug_mode = true;
        g.seed = Some(1);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("no hostile in sight", |g| {
        let cat = g.catalog.as_deref().ok_or("no catalog")?;
        if !nh_world::threats(&g.world, cat).is_empty() {
            return Err("a hostile in sight: the seed's level changed".into());
        }
        Ok(g.world.hero().is_some())
    }));
    // a bad potion: HP lost, nothing to fight
    steps.extend(wish("potion of sickness"));
    steps.extend([
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the potion (the pack's only one)", |g| letter(g, "potion")),
        Step::AnswerUntil('n', "a command after the drink", idle_command),
        Step::Wait("HP lost", |g| Ok(hp(g).is_some_and(|(v, max)| v < max))),
        Step::Wait("no fight for it", exploring),
        // the next turn too
        key('s'),
        Step::Request("a command after a search", command),
        Step::Wait("still no fight", exploring),
        // a jackal beside the hero: a fight
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("jackal".into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
        Step::Wait("COMBAT with the jackal", |g| {
            Ok(g.driver.mode() == Mode::Combat)
        }),
        // away to another level mid-fight
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("4".into())),
        Step::AnswerUntil('n', "a command on the new level", |g| {
            Ok(idle_command(g)? && g.world.depth() == Some(4))
        }),
        Step::Wait("the fight left on its level", exploring),
    ]);
    steps.extend(quit());
    steps
}
