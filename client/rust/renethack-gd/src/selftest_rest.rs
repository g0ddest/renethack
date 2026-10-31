//! F5's rest goes to the engine as counted searches (`rest`): a Valkyrie
//! drinks a potion of sickness (debug mode, seed 1: no hostile about), and
//! F5 rests until her HP is full again. The engine runs twenty searches at
//! a time and ends them itself at full HP ("You are in full health."):
//! a hundred turns pass in a few seconds, not one a tick. F5 at full says
//! so and sends nothing. A brown mold in sight across the room lets her
//! rest (NetHack's count stops for a foe beside her only); a jackal beside
//! her makes F5 refuse in the client's words, never NetHack's "use the 'm'
//! prefix".

use std::sync::Mutex;
use std::time::Instant;

use nh_protocol::Reply;
use nh_world::{Key, KeyInput, Mode, Prompt};

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
    steps.extend(wish("3 potions of sickness"));
    steps.extend(drink());
    steps.extend([
        Step::Wait("no hostile in sight", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            if nh_world::threats(&g.world, cat).is_empty() {
                Ok(true)
            } else {
                Err("a hostile in sight: the seed's level changed".into())
            }
        }),
        Step::Call("F5", began),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("HP full again, the rest over, quickly", |g| rested(g, 20)),
        // full already: F5 says so, and sends nothing
        Step::Call("what was sent so far", |g| {
            *SENT.lock().map_err(|e| e.to_string())? = g.sent.back().cloned();
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("F5 at full: \"Rested: HP and Pw are full\"", |g| {
            said(g, &tr!("stop-healed"))
        }),
    ]);
    // a foe in sight across the room: the rest is NetHack's to stop (it
    // refuses a count for a foe beside the hero only); HP lost first, with
    // no foe about (HP lost with one in sight makes a fight)
    steps.extend(drink());
    steps.extend(drink());
    steps.extend([
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("hostile brown mold".into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
        Step::KeysUntil("steps away from the mold", away_from_foe),
        Step::KeysUntil("searches until the fight is over", |g| {
            Ok((g.driver.mode() == Mode::Combat).then(|| KeyInput::plain(Key::Char('s'))))
        }),
        Step::Wait("the mold in sight, not beside her, HP still short", |g| {
            if !hp(g).is_some_and(|(v, max)| v < max) {
                return Err("HP full again before F5".into());
            }
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            if nh_world::threats(&g.world, cat).is_empty() || nh_world::engaged(&g.world, cat) {
                return Err("no foe in sight, or one beside her".into());
            }
            Ok(true)
        }),
        Step::Call("F5", began),
        Step::Key(KeyInput::plain(Key::F(5))),
        Step::Wait("HP full again across the room from the mold", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.hud.mode_view().1.as_deref() == Some(tr!("rest-foe").as_str()) {
                return Err("F5 refused with the foe across the room".into());
            }
            rested(g, 2)
        }),
        // a foe beside her: no rest, said in the client's words, not
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

/// `q` and the pack's potion of sickness: HP lost, nothing to fight.
fn drink() -> [Step; 5] {
    [
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the potion (the pack's only kind)", |g| letter(g, "potion")),
        Step::AnswerUntil('n', "a command after the drink", idle_command),
        Step::Wait("HP lost", |g| Ok(hp(g).is_some_and(|(v, max)| v < max))),
    ]
}

/// F5 is pressed now: the time and the turn.
fn began(g: &mut RenethackGame) -> Result<(), String> {
    let turn = g.world.turn().ok_or("no turn")?;
    *BEGAN.lock().map_err(|e| e.to_string())? = Some((Instant::now(), turn));
    Ok(())
}

/// The rest since `began` is over, HP full, after `least` turns at least
/// (a count's, not a single search's), quickly.
fn rested(g: &RenethackGame, least: i64) -> Result<bool, String> {
    let (since, turn) = BEGAN
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or("no rest began")?;
    let secs = since.elapsed().as_secs_f64();
    let full = hp(g).is_some_and(|(v, max)| v >= max);
    if !(full && !g.driver.is_active() && g.pending.as_ref().is_some_and(|(_, p)| command(p))) {
        if secs > REST_SECS {
            return Err(format!("still resting after {secs:.1} s, HP {:?}", hp(g)));
        }
        return Ok(false);
    }
    let turns = g.world.turn().ok_or("no turn")? - turn;
    godot::prelude::godot_print!("selftest: rest: {turns} turns in {secs:.1} s");
    if turns < least {
        return Err(format!("only {turns} turns of rest"));
    }
    Ok(true)
}

/// A step that takes the hero farther from the foe in sight, until she is
/// three cells from it.
fn away_from_foe(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let hero = g.world.hero().ok_or("no hero")?;
    let foe = nh_world::threats(&g.world, cat)
        .into_iter()
        .min_by_key(|&t| nh_world::chebyshev(hero, t))
        .ok_or("no foe in sight")?;
    let far = nh_world::chebyshev(hero, foe);
    if far >= 3 {
        return Ok(None);
    }
    let open = |c: (i32, i32)| {
        g.world
            .map
            .cell(c.0, c.1)
            .is_some_and(|cell| nh_world::passage(cell, cat) == nh_world::Passage::Open)
    };
    let (dx, dy) = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
        .filter(|&(dx, dy)| (dx, dy) != (0, 0) && open((hero.0 + dx, hero.1 + dy)))
        .max_by_key(|&(dx, dy)| nh_world::chebyshev((hero.0 + dx, hero.1 + dy), foe))
        .filter(|&(dx, dy)| nh_world::chebyshev((hero.0 + dx, hero.1 + dy), foe) > far)
        .ok_or("no step away from the foe")?;
    let k = nh_world::dir_key(&g.world.dirchars, dx, dy).ok_or("no key for the step")?;
    Ok(Some(KeyInput::plain(Key::Char(k))))
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
