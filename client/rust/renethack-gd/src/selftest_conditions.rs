//! `conditions`: the hero's conditions shown on their body, in a
//! debug-mode game where #wizintrinsic sets them. Turning to stone the
//! hero greys, a step further each turn, and at the end is of the
//! statues' own stone; a prayer cures it and the tint goes. Turning to slime they go green the same way, and a prayer cures
//! that too. Levitating for three turns they rise off the floor and bob,
//! glide a step, and come down when it ends. Then, stepped where nothing
//! stands between the eye and them: stunned they sway and the camera
//! drifts, confused more gently; hallucinating a hue tints the view;
//! blind the view dims and its edges darken; each at its full strength
//! for its picture, and gone when its three turns end. With fewer screen
//! effects (the settings) a stun shows no sway and no drift, while
//! blindness still dims. Pictures of each with `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use nh_world::{Key, KeyInput, Prompt};

use godot::prelude::Vector3;

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
    steps.extend(levitating());
    steps.extend([
        Step::Call("", |_| {
            WALKED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("a step into the eye's sight", into_sight),
    ]);
    for (_, _, shot) in EFFECTS {
        steps.extend(effect(shot));
    }
    steps.extend(reduced());
    steps
}

/// Steps taken into the eye's sight, and the most taken.
static WALKED: AtomicU32 = AtomicU32::new(0);
const INTO_SIGHT: u32 = 4;

/// A step to open floor with open floor south of it, towards the eye,
/// till the hero stands so: then nothing tall (an up stair's arch) stands
/// between the eye and them in the pictures.
fn into_sight(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if !matches!(g.pending, Some((_, Prompt::Command))) {
        return Ok(Some(KeyInput::plain(Key::Escape)));
    }
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    let open = |x: i32, y: i32| {
        g.world.map.cell(x, y).is_some_and(|c| {
            c.entity().is_none()
                && matches!(
                    nh_world::cell_terrain(c, cat),
                    Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
                )
        })
    };
    if open(hx, hy + 1) || WALKED.fetch_add(1, Ordering::Relaxed) >= INTO_SIGHT {
        return Ok(None);
    }
    let ways = super::audio_tests::STEPS;
    let way = ways
        .iter()
        .find(|((dx, dy), _)| open(hx + dx, hy + dy) && open(hx + dx, hy + dy + 1))
        .or_else(|| ways.iter().find(|((dx, dy), _)| open(hx + dx, hy + dy)));
    Ok(way.map(|&(_, k)| KeyInput::plain(Key::Char(k))))
}

/// The screen's effects, in the order the scenario sets them: the
/// #wizintrinsic entry, the status line's condition, the picture.
const EFFECTS: [(&str, &str, &str); 4] = [
    ("stunned", "Stun", "conditions-stunned"),
    ("confused", "Conf", "conditions-confused"),
    ("hallucinating", "Hallu", "conditions-hallucinating"),
    ("blinded", "Blind", "conditions-blind"),
];
/// The effect under test (an index into `EFFECTS`, from 1), and the
/// view's brightness before it.
static EFFECT: AtomicUsize = AtomicUsize::new(0);
static BRIGHT: Mutex<f32> = Mutex::new(0.0);

fn effect_now() -> (&'static str, &'static str) {
    let (entry, cond, _) = EFFECTS[EFFECT.load(Ordering::Relaxed).saturating_sub(1)];
    (entry, cond)
}

/// The steps that set #wizintrinsic's `pick` for three turns.
fn three_turns_of(pick: fn(&RenethackGame) -> Result<KeyInput, String>) -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        key('3'),
        Step::KeyFrom("the intrinsic", pick),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, the condition on", command),
    ]
}

/// The effect shows at its full strength (`on`), or not at all.
fn effect_shows(g: &RenethackGame, on: bool) -> Result<bool, String> {
    let e = map_view(g)?.hero_effects();
    if !on {
        let none = e.sway == 0.0 && e.drift == Vector3::ZERO && e.hue == 0.0 && e.dim == 0.0;
        return Ok(none && !e.screen && e.bright == *BRIGHT.lock().unwrap());
    }
    Ok(e.settled
        && match effect_now().1 {
            "Stun" => e.sway == 1.0 && e.drift.length() > 0.0 && !e.screen,
            "Conf" => e.sway > 0.4 && e.sway < 0.7 && e.drift.length() > 0.0 && !e.screen,
            "Hallu" => e.hue == 1.0 && e.sway == 0.0 && e.screen,
            _ => e.dim == 1.0 && e.bright < 0.8 && e.screen,
        })
}

/// The next screen effect: on for three turns, pictured, then gone.
fn effect(shot: &'static str) -> Vec<Step> {
    let mut steps = vec![Step::Call("the next effect", |g| {
        EFFECT.fetch_add(1, Ordering::Relaxed);
        *BRIGHT.lock().unwrap() = map_view(g)?.hero_effects().bright;
        Ok(())
    })];
    steps.extend(three_turns_of(|g| entry_key(g, effect_now().0)));
    steps.extend([
        Step::Wait("the effect shown", |g| effect_shows(g, true)),
        Step::Shot(shot),
        Step::Call("", |_| {
            SEARCHED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("searching until it ends", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let names = g.world.status.condition_names(cat);
            if !names.iter().any(|n| n == effect_now().1) && !FORCING.load(Ordering::Relaxed) {
                return Ok(None);
            }
            forced_search(g)
        }),
        Step::Wait("the effect gone", |g| effect_shows(g, false)),
    ]);
    steps
}

/// Fewer screen effects: a stun without sway or drift, blindness still
/// dimming the view.
fn reduced() -> Vec<Step> {
    let mut steps = vec![Step::Call("fewer screen effects", |g| {
        let mut s = g.settings.clone();
        s.reduce_effects = true;
        g.set_settings(s);
        Ok(())
    })];
    steps.extend(three_turns_of(|g| entry_key(g, "stunned")));
    steps.extend(three_turns_of(|g| entry_key(g, "blinded")));
    steps.extend([
        Step::Wait("blind and dimmed, stunned and still", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let names = g.world.status.condition_names(cat);
            let e = map_view(g)?.hero_effects();
            let both = names.iter().any(|n| n == "Stun") && names.iter().any(|n| n == "Blind");
            let still = e.sway == 0.0 && e.drift == Vector3::ZERO && e.hue == 0.0;
            Ok(both && e.settled && e.dim == 1.0 && e.bright < 0.8 && still)
        }),
        Step::Call("all the screen's effects again", |g| {
            let mut s = g.settings.clone();
            s.reduce_effects = false;
            g.set_settings(s);
            Ok(())
        }),
    ]);
    steps
}

/// A float the hero rises into and bobs in, three turns long.
fn levitating() -> Vec<Step> {
    let mut steps = vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        // three turns of it, not the thirty a pick alone gives
        key('3'),
        Step::KeyFrom("levitating", |g| entry_key(g, "levitating")),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, afloat", command),
        Step::Wait("the hero risen off the floor", |g| {
            Ok(map_view(g)?.hero_lift() > 0.25)
        }),
        Step::Call("", |g| {
            *AT.lock().unwrap() = map_view(g)?.hero_lift();
            Ok(())
        }),
        Step::Wait("bobbing", |g| {
            let before = *AT.lock().unwrap();
            Ok((map_view(g)?.hero_lift() - before).abs() > 0.02)
        }),
        Step::Shot("conditions-levitating"),
        // a step afloat: a glide, the legs still (the idle, not a walk)
        Step::KeyFrom("a step", |g| {
            super::audio_tests::open_way(g).map(|c| KeyInput::plain(Key::Char(c)))
        }),
        Step::Wait("gliding", |g| {
            let (gait, _) = map_view(g)?.hero_clips();
            Ok(gait.is_some_and(|c| {
                let c = c.to_lowercase();
                !c.contains("walk") && !c.contains("run") && !c.contains("jog")
            }))
        }),
        Step::Request("a command after the glide", command),
        Step::Call("", |_| {
            SEARCHED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeysUntil("searching until the float ends", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let names = g.world.status.condition_names(cat);
            if !names.iter().any(|n| n == "Lev") && !FORCING.load(Ordering::Relaxed) {
                return Ok(None);
            }
            forced_search(g)
        }),
    ];
    steps.push(Step::Wait("down on the floor again", |g| {
        Ok(map_view(g)?.hero_lift() == 0.0)
    }));
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
        // stoned through, the hero is of the statues' own stone
        Step::Wait("the end's look", |g| {
            Ok(current().0 != Shown::Stone || map_view(g)?.hero_in_stone())
        }),
        Step::Shot(end),
    ]);
    steps.extend(pray());
    steps.push(Step::Wait("cured: the tint gone", |g| {
        let (shown, tinted) = map_view(g)?.hero_shown(&g.world);
        Ok(shown.is_empty() && !tinted)
    }));
    // the god's light gone before the next picture
    steps.push(Step::Wait("the prayer over", |g| {
        Ok(map_view(g)?.hero_prayer().is_none())
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
