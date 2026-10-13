//! `conditions`: the hero's conditions shown on their body, in a
//! debug-mode game where #wizintrinsic sets them. Turning to stone the
//! hero greys, a step further each turn; a prayer cures it and the tint
//! goes. Turning to slime they go green the same way, and a prayer cures
//! that too. Pictures of each with `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, command, entry_key, key, map_view, start};
use crate::game::RenethackGame;
use crate::map_view::Shown;

const SEED: u64 = 5;

/// The countdowns, in the order the scenario sets them: what the body
/// shows, the #wizintrinsic entry that sets it, the picture's name.
const COUNTDOWNS: [(Shown, &str, &str, &str); 2] = [
    (
        Shown::Stone,
        "petrifying",
        "conditions-stone",
        "conditions-stone-end",
    ),
    (
        Shown::Slime,
        "becoming slime",
        "conditions-slime",
        "conditions-slime-end",
    ),
];
/// Turns searched at most for a countdown to reach its end.
const TO_THE_END: u32 = 12;

/// The countdown under test (an index into `COUNTDOWNS`, from 1; 0: none
/// yet), and its share of the way when last looked at.
static NOW: AtomicUsize = AtomicUsize::new(0);
static AT: Mutex<f32> = Mutex::new(0.0);

pub(super) fn conditions() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        g.seed = Some(SEED);
        NOW.store(0, Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    for (_, _, shot, end) in COUNTDOWNS {
        steps.extend(countdown(shot, end));
    }
    steps
}

fn current() -> (Shown, &'static str) {
    let (shown, entry, _, _) = COUNTDOWNS[NOW.load(Ordering::Relaxed).saturating_sub(1)];
    (shown, entry)
}

/// The next countdown set by #wizintrinsic, shown on the body and a step
/// further a turn later, then cured by a prayer and its tint gone.
fn countdown(shot: &'static str, end: &'static str) -> Vec<Step> {
    let mut steps = vec![
        Step::Call("the next countdown", |_| {
            NOW.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        Step::KeyFrom("the countdown's entry", |g| entry_key(g, current().1)),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, the countdown begun", command),
        Step::Wait("the hero's body shows it", |g| {
            Ok(share(g, current().0)?.is_some())
        }),
        Step::Call("", |g| {
            *AT.lock().unwrap() = share(g, current().0)?.ok_or("not shown")?;
            Ok(())
        }),
    ];
    steps.extend(search());
    steps.extend([
        Step::Wait("a step further a turn later", |g| {
            let before = *AT.lock().unwrap();
            Ok(share(g, current().0)?.is_some_and(|p| p > before))
        }),
        Step::Shot(shot),
        Step::Call("", |_| {
            SEARCHED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("searching to the countdown's end", |g| {
            if share(g, current().0)? == Some(1.0) && !FORCING.load(Ordering::Relaxed) {
                return Ok(None);
            }
            forced_search(g)
        }),
        Step::Shot(end),
    ]);
    steps.extend(pray());
    steps.push(Step::Wait("cured: the tint gone", |g| {
        let (shown, tinted) = map_view(g)?.hero_shown(&g.world);
        Ok(shown.is_empty() && !tinted)
    }));
    steps
}

/// The condition's share of the way, if the hero's model wears its tint.
fn share(g: &RenethackGame, which: Shown) -> Result<Option<f32>, String> {
    let (shown, tinted) = map_view(g)?.hero_shown(&g.world);
    Ok(shown
        .iter()
        .find(|(s, _)| *s == which)
        .map(|&(_, p)| p)
        .filter(|_| tinted))
}

/// Turns searched for the countdown's end, and whether the `m` before
/// the next search was sent.
static SEARCHED: AtomicU32 = AtomicU32::new(0);
static FORCING: AtomicBool = AtomicBool::new(false);

/// A forced search a turn (`m`, then `s`), else Esc; at most `TO_THE_END`.
fn forced_search(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if !matches!(g.pending, Some((_, Prompt::Command))) {
        return Ok(Some(KeyInput::plain(Key::Escape)));
    }
    if !FORCING.swap(false, Ordering::Relaxed) {
        FORCING.store(true, Ordering::Relaxed);
        return Ok(Some(KeyInput::plain(Key::Char('m'))));
    }
    if SEARCHED.fetch_add(1, Ordering::Relaxed) >= TO_THE_END {
        return Err("the countdown never reached its end".into());
    }
    Ok(Some(KeyInput::plain(Key::Char('s'))))
}

/// A turn searched (`m`: the engine refuses a search beside a hostile).
fn search() -> [Step; 4] {
    [
        key('m'),
        Step::Request("the search to force", command),
        key('s'),
        Step::Request("a command a turn later", command),
    ]
}

/// `#pray`, yes to "Are you sure you want to pray?", and yes to debug
/// mode's "Force the gods to be pleased?": the trouble fixed.
fn pray() -> [Step; 8] {
    [
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("pray".into()))),
        Step::Request(
            "Are you sure you want to pray?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("pray")),
        ),
        key('y'),
        Step::Request(
            "Force the gods to be pleased?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Force")),
        ),
        key('y'),
        Step::AnswerUntil('n', "a command after the prayer", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && !g.driver.is_active())
        }),
    ]
}
