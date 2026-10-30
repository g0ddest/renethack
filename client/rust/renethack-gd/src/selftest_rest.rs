//! F5's rest goes to the engine as counted searches (`rest`): a Wizard
//! casts force bolt, and F5 rests until his Pw is full again. The engine
//! runs twenty searches at a time and ends them itself at full Pw ("You
//! feel full of energy."): dozens of turns pass in a few seconds, not one
//! a tick.

use std::sync::Mutex;
use std::time::Instant;

use nh_world::{Key, KeyInput, Prompt};

use super::{CharacterChoice, Step, command, idle_command, key, quit, smoke_choice, start_as};
use crate::game::RenethackGame;

/// When the rest began, and the turn then.
static BEGAN: Mutex<Option<(Instant, i64)>> = Mutex::new(None);

/// The most a rest of a Wizard's few Pw may take, at the order speed.
const REST_SECS: f64 = 12.0;

fn pw(g: &RenethackGame) -> Option<(i64, i64)> {
    Some((
        g.world.status.number("energy")?,
        g.world.status.number("energymax")?,
    ))
}

pub(super) fn rest() -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: "wizard".into(),
        gender: "male".into(),
        ..smoke_choice()
    });
    steps.extend([
        // force bolt: some Pw spent
        key('Z'),
        Step::Request("which spell", |p| matches!(p, Prompt::Menu { .. })),
        key('a'),
        Step::Request("the spell's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        key('h'),
        Step::AnswerUntil('n', "a command after casting", idle_command),
        Step::Wait("Pw spent", |g| Ok(pw(g).is_some_and(|(v, max)| v < max))),
        Step::Call("F5", |g| {
            let turn = g.world.turn().ok_or("no turn")?;
            *BEGAN.lock().map_err(|e| e.to_string())? = Some((Instant::now(), turn));
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("Pw full again, the rest over, quickly", |g| {
            let (since, turn) = BEGAN
                .lock()
                .map_err(|e| e.to_string())?
                .ok_or("no rest began")?;
            let secs = since.elapsed().as_secs_f64();
            let full = pw(g).is_some_and(|(v, max)| v >= max);
            if !(full
                && !g.driver.is_active()
                && g.pending.as_ref().is_some_and(|(_, p)| command(p)))
            {
                if secs > REST_SECS {
                    return Err(format!("still resting after {secs:.1} s, Pw {:?}", pw(g)));
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
    ]);
    steps.extend(quit());
    steps
}
