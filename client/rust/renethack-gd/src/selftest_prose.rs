//! A text window of prose reads in the body face in either language
//! (`quest-words`): the quest's first words on arriving at its home level
//! (a debug level teleport), whose English has NetHack's two spaces after
//! each sentence and whose Russian has one; a table keeps the monospace
//! face (`layouts`).

use nh_world::Prompt;

use super::{DialogEvent, Step, command, ctrl_key, entry_key, is_menu, quit, start};
use crate::game::RenethackGame;

/// The quest's words are up, in the body face; the engine's English had
/// two spaces after a sentence.
fn quest_words_prose(g: &RenethackGame) -> Result<bool, String> {
    let Some((_, Prompt::Show { lines, .. })) = g.pending.as_ref() else {
        return Ok(false);
    };
    if !lines
        .iter()
        .any(|l| l.text.contains(".  ") || l.text.contains("!  "))
    {
        return Err(format!("no two spaces after a sentence in {lines:?}"));
    }
    let ui = g.ui.as_ref().ok_or("no UI")?;
    match ui.dialogs.show_prose() {
        Some(true) => Ok(true),
        Some(false) => Err("the quest's words are in the monospace face".into()),
        None => Ok(false),
    }
}

pub(super) fn quest_words() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start());
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the quest's home", |g| entry_key(g, "Val-strt")),
        Step::Request("the quest's first words", |p| {
            matches!(p, Prompt::Show { .. })
        }),
        Step::Wait("in the body face", quest_words_prose),
        Step::Shot("quest-words"),
        Step::Dialog(DialogEvent::Close),
        Step::Request("a command on the quest's home", command),
    ]);
    steps.extend(quit());
    steps
}
