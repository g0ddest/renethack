//! F5's rest goes to the engine as counted searches (`rest`): a Valkyrie
//! drinks a potion of sickness (debug mode, seed 1: no hostile about), and
//! F5 rests until her HP is full again. The engine runs twenty searches at
//! a time and ends them itself at full HP ("You are in full health."):
//! a hundred turns pass in a few seconds, not one a tick. F5 at full says so and sends nothing; with a foe in sight it
//! refuses in the client's words, never NetHack's "use the 'm' prefix".

use std::sync::Mutex;
use std::time::Instant;

use nh_protocol::Reply;
use nh_world::{Key, KeyInput, Prompt};

use super::audio_tests::{letter, wish};
use super::{DialogEvent, Step, command, ctrl_key, idle_command, key, logged, quit, start};
use crate::game::RenethackGame;
use crate::tr;

/// When the rest began, and the turn then.
static BEGAN: Mutex<Option<(Instant, i64)>> = Mutex::new(None);

/// The most a rest of a few HP at XL 1 may take, at the order speed.
const REST_SECS: f64 = 12.0;

fn hp(g: &RenethackGame) -> Option<(i64, i64)> {
    Some((
        g.world.status.number("hp")?,
        g.world.status.number("hpmax")?,
    ))
}

pub(super) fn rest() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 1 (no hostile about)", |g| {
        g.debug_mode = true;
        g.seed = Some(1);
        Ok(())
    })];
    steps.extend(start());
    steps.extend(wish("potion of sickness"));
    steps.extend([
        // a bad potion: HP lost, nothing to fight
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the potion (the pack's only one)", |g| letter(g, "potion")),
        Step::AnswerUntil('n', "a command after the drink", idle_command),
        Step::Wait("HP lost", |g| Ok(hp(g).is_some_and(|(v, max)| v < max))),
        Step::Wait("no hostile in sight (F5 would refuse)", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            if nh_world::threats(&g.world, cat).is_empty() {
                Ok(true)
            } else {
                Err("a hostile in sight: the seed's level changed".into())
            }
        }),
        Step::Call("F5", |g| {
            let turn = g.world.turn().ok_or("no turn")?;
            *BEGAN.lock().map_err(|e| e.to_string())? = Some((Instant::now(), turn));
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("HP full again, the rest over, quickly", |g| {
            let (since, turn) = BEGAN
                .lock()
                .map_err(|e| e.to_string())?
                .ok_or("no rest began")?;
            let secs = since.elapsed().as_secs_f64();
            let full = hp(g).is_some_and(|(v, max)| v >= max);
            if !(full
                && !g.driver.is_active()
                && g.pending.as_ref().is_some_and(|(_, p)| command(p)))
            {
                if secs > REST_SECS {
                    return Err(format!("still resting after {secs:.1} s, HP {:?}", hp(g)));
                }
                return Ok(false);
            }
            let turns = g.world.turn().ok_or("no turn")? - turn;
            godot::prelude::godot_print!("selftest: rest: {turns} turns in {secs:.1} s");
            if turns < 20 {
                return Err(format!("only {turns} turns of rest"));
            }
            Ok(true)
        }),
        // full already: F5 says so, and sends nothing
        Step::Call("what was sent so far", |g| {
            *SENT.lock().map_err(|e| e.to_string())? = g.sent.back().cloned();
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("F5 at full: \"Rested: HP and Pw are full\"", |g| {
            said(g, &tr!("stop-healed"))
        }),
        // a foe in sight: no rest, said in the client's words, not
        // NetHack's refusal of a counted search beside a monster
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("jackal".into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
        Step::Call("what was sent so far", |g| {
            *SENT.lock().map_err(|e| e.to_string())? = g.sent.back().cloned();
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("F5 beside the jackal: no rest, the client's words", |g| {
            if logged(g, "'m' prefix") {
                return Err("NetHack's refusal of a counted search".into());
            }
            said(g, &tr!("rest-foe"))
        }),
    ]);
    steps.extend(quit());
    steps
}

/// The last answer sent before the last F5.
static SENT: Mutex<Option<(Prompt, Reply)>> = Mutex::new(None);

/// The HUD's order line says `words`, and nothing went to the engine for
/// the last F5.
fn said(g: &RenethackGame, words: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let (_, line, _) = ui.hud.mode_view();
    if line.as_deref() != Some(words) {
        return Ok(false);
    }
    let before = SENT.lock().map_err(|e| e.to_string())?.clone();
    if g.sent.back().cloned() != before {
        return Err(format!("F5 sent {:?}", g.sent.back()));
    }
    Ok(true)
}
