//! The journal (`journal`): a game's #chronicle laid out from the engine's
//! own window, each event by its turn, beside the message history as the
//! log keeps it, both in the language the test runs in; no turn passes.
//! The keys scroll the messages, Esc closes the journal, and so do its ✕
//! and a gamepad's B.

use std::sync::atomic::{AtomicI64, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, PadEv, Step, fail_on_error_screen, key, quit, start};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, EngineKind};
use crate::journal;
use crate::tr;

/// The turn the journal first opened on.
static TURN: AtomicI64 = AtomicI64::new(-1);

/// #chronicle from the palette.
fn chronicle() -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("chronicle".into())),
    ]
}

/// The journal is open on #chronicle's window, its events and the log's
/// messages on it.
fn shown(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.layout_name() != Some("journal") {
        return Ok(false);
    }
    let Some((_, Prompt::Show { lines, .. })) = g.pending.as_ref() else {
        return Ok(false);
    };
    let english: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    let j = journal::journal(&english).ok_or("the window is no journal")?;
    if j.events.is_empty() {
        return Err("no events logged".into());
    }
    let texts = ui.dialogs.panel_texts();
    let mut want: Vec<String> = Vec::new();
    for e in &j.events {
        want.push(tr!("hud-turn", turn = e.turn));
        want.push(i18n::engine(EngineKind::Message, &e.text).into_owned());
    }
    if let Some(missing) = want.iter().find(|w| !texts.contains(w)) {
        return Err(format!("the journal shows no {missing:?}: {texts:?}"));
    }
    // the messages: the newest as the log says it
    let newest = g.world.log.iter().last().ok_or("no message logged")?;
    let said = i18n::engine_message(newest).into_owned();
    let messages = ui.dialogs.journal_messages().unwrap_or_default();
    if !messages.contains(&said) {
        return Err(format!("the messages show no {said:?}: {messages:?}"));
    }
    Ok(true)
}

fn command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && matches!(g.pending, Some((_, Prompt::Command))))
}

pub(super) fn journal() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Call("the turn", |g| {
        TURN.store(g.world.turn().unwrap_or(-1), Ordering::Relaxed);
        Ok(())
    }));
    steps.extend(chronicle());
    steps.extend([
        Step::Wait("the journal, its events and the messages", shown),
        Step::Shot("journal"),
        Step::Key(KeyInput::plain(Key::PageUp)),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("closed, no turn passed", |g| {
            let was = TURN.load(Ordering::Relaxed);
            if command(g)? && g.world.turn().unwrap_or(-1) != was {
                return Err(format!("the turn was {was}, now {:?}", g.world.turn()));
            }
            command(g)
        }),
    ]);
    // the ✕, then a gamepad's B
    steps.extend(chronicle());
    steps.extend([
        Step::Wait("the journal again", shown),
        Step::Dialog(DialogEvent::Close),
        Step::Wait("closed by its ✕", command),
    ]);
    steps.extend(chronicle());
    steps.extend([
        Step::Wait("the journal for a gamepad", shown),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("closed by B", command),
    ]);
    steps.extend(quit());
    steps
}
