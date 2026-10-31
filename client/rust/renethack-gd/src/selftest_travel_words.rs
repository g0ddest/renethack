//! `travel-words`: travel's cursor tells this client's words, not the
//! tty's keys ("Enter or a click: go · ?: keys · Esc: cancel", not ". , ;
//! : pick, ? help"); `?` lists the keys that jump the cursor, as the line
//! says; and Enter does pick the cell: `_`, the cursor a cell west, Enter,
//! and the hero walks there.

use std::sync::atomic::{AtomicI32, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{Step, command, key, quit, start};
use crate::tr;

/// Where the hero stood before the travel.
static FROM: AtomicI32 = AtomicI32::new(0);

pub(super) fn travel_words() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("", |g| {
            let (x, _) = g.world.map.hero().ok_or("no hero")?;
            FROM.store(x, Ordering::Relaxed);
            Ok(())
        }),
        key('_'),
        Step::Request("where to travel", command),
        Step::Wait("the line in this client's words", |g| {
            let Some(line) = g.ui.as_ref().ok_or("no UI")?.hud.prompt_line() else {
                return Ok(false);
            };
            if line.contains(". , ;") || line.contains('?') && line.contains("Esc cancel") {
                return Err(format!("the tty's keys: {line:?}"));
            }
            Ok(line.contains("Enter")
                && line.contains("?:")
                && line.contains(&tr!("prompt-getpos-go")))
        }),
        Step::Shot("travel-words"),
        key('?'),
        Step::Request("the keys that jump the cursor", |p| {
            matches!(p, Prompt::Show { lines, .. } if lines.iter().any(|l| l.text.contains('@')))
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Request("the cursor again", command),
        key('h'),
        Step::Request("the cursor a cell west", command),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Wait("Enter: the hero walks there", |g| {
            let at = g.world.map.hero().map(|(x, _)| x);
            Ok(!g.world.getpos && at == Some(FROM.load(Ordering::Relaxed) - 1))
        }),
    ]);
    steps.extend(quit());
    steps
}
