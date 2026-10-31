//! A click on a peaceful creature never strikes it unasked (`peaceful`): a
//! peaceful watchman made beside the hero in debug mode. The mouse's
//! click, then the gamepad's A with its cursor on him, step into him as
//! NetHack's move does, with no `F` (whose blow 5.0 deals without the
//! question): with the pet option that swaps places off, the engine asks
//! "Really attack the watchman?", No, and he stays peaceful and unhurt.
//! `peaceful-swap`: with it on (the default), the hero in a room's corner
//! with the watchman east or south of them: a click swaps places with him
//! (5.0 draws him on the hero's old cell before the hero is on the new),
//! he stays told peaceful, and the next click swaps back, no blow.

use std::sync::Mutex;

use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, command, ctrl_key, idle_command, key, logged, quit, start, tap};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::ui_events::UiEvent;

/// The watchman's cell.
static AT: Mutex<Option<(i32, i32)>> = Mutex::new(None);

fn at() -> Result<(i32, i32), String> {
    AT.lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no watchman found".to_string())
}

/// A peaceful creature beside the hero, if the map shows one.
fn peaceful_beside(g: &RenethackGame) -> Option<(i32, i32)> {
    let (hx, hy) = g.world.map.hero()?;
    (hy - 1..=hy + 1)
        .flat_map(|y| (hx - 1..=hx + 1).map(move |x| (x, y)))
        .find(|&(x, y)| g.world.map.cell(x, y).is_some_and(|c| c.peaceful))
}

/// No blow was struck, and a peaceful creature is still beside the hero.
fn unhurt(g: &RenethackGame) -> Result<bool, String> {
    for blow in ["You hit", "You miss", "gets angry"] {
        if logged(g, blow) {
            return Err(format!("\"{blow}\" in the log: the watchman was struck"));
        }
    }
    let near = peaceful_beside(g);
    if let Some(p) = near {
        *AT.lock().map_err(|e| e.to_string())? = Some(p);
    }
    Ok(near.is_some())
}

fn really_attack(p: &Prompt) -> bool {
    matches!(p, Prompt::Choice { query, .. } if query.contains("Really attack"))
}

/// A peaceful watchman beside the hero in a debug-mode game; `swap`: the
/// pet option that swaps places with peacefuls on.
fn with_watchman(swap: bool) -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode; the swap option as asked",
        if swap {
            |g: &mut RenethackGame| {
                g.debug_mode = true;
                g.settings.gameplay.safe_pet = true;
                Ok(())
            }
        } else {
            |g: &mut RenethackGame| {
                g.debug_mode = true;
                g.settings.gameplay.safe_pet = false;
                Ok(())
            }
        },
    )];
    steps.extend(start());
    steps.extend([
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("peaceful watchman".into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
        Step::Wait("the watchman beside the hero, peaceful", |g| {
            let p = peaceful_beside(g);
            *AT.lock().map_err(|e| e.to_string())? = p;
            Ok(p.is_some())
        }),
    ]);
    steps
}

fn click() -> Step {
    Step::Call("a click on him", |g| {
        let (x, y) = at()?;
        g.push_ui(UiEvent::MapClick { x, y, button: 1 });
        Ok(())
    })
}

pub(super) fn peaceful() -> Vec<Step> {
    let mut steps = with_watchman(false);
    steps.extend([
        // the mouse
        click(),
        Step::Request("Really attack the watchman?", really_attack),
        Step::Shot("peaceful-asks"),
        key('n'),
        Step::Request("a command after not attacking", command),
        Step::Wait("still peaceful, unhurt", unhurt),
        // the gamepad's A, its cursor on him
        Step::Call("the gamepad's cursor on him", |g| {
            g.pad_cursor = Some(at()?);
            Ok(())
        }),
        tap(PadButton::A),
        Step::Request("Really attack the watchman? (A)", really_attack),
        key('n'),
        Step::Request("a command after not attacking", command),
        Step::Wait("still peaceful, unhurt (A)", unhurt),
    ]);
    steps.extend(quit());
    steps
}

/// The hero's cell before a swap.
static HERO_WAS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// The room's corner the hero waits in.
static CORNER: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// The north-west corner of the hero's room: floor with wall west and
/// north of it.
fn room_corner(g: &RenethackGame) -> Option<(i32, i32)> {
    let cat = g.catalog.as_deref()?;
    let (hx, hy) = g.world.map.hero()?;
    let terrain = |x: i32, y: i32| {
        g.world
            .map
            .cell(x, y)
            .and_then(|c| nh_world::cell_terrain(c, cat))
    };
    let floor = |x, y| {
        matches!(
            terrain(x, y),
            Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
        )
    };
    let wall = |x, y| terrain(x, y) == Some(nh_world::Terrain::Wall);
    (hy - 8..=hy)
        .flat_map(|y| (hx - 20..=hx).map(move |x| (x, y)))
        .filter(|&(x, y)| floor(x, y) && wall(x - 1, y) && wall(x, y - 1))
        .min_by_key(|&(x, y)| (hx - x).abs() + (hy - y).abs())
}

/// Steps to the corner, a key at a time.
fn to_corner() -> Step {
    Step::KeysUntil("the hero stepped to the room's corner", |g| {
        if !matches!(g.pending, Some((_, Prompt::Command))) {
            return Ok(Some(KeyInput::plain(Key::Escape)));
        }
        let target = CORNER
            .lock()
            .map_err(|e| e.to_string())?
            .ok_or("no corner")?;
        let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
        if (hx, hy) == target {
            return Ok(None);
        }
        let d = ((target.0 - hx).signum(), (target.1 - hy).signum());
        let way = super::audio_tests::STEPS
            .iter()
            .find(|(s, _)| *s == d)
            .ok_or("no way to the corner")?;
        Ok(Some(KeyInput::plain(Key::Char(way.1))))
    })
}

/// A click on the watchman: a swap (or its refusal), and he is still
/// peaceful, unhurt; the way the hero went said.
fn swap_steps() -> Vec<Step> {
    vec![
        Step::Call("the hero's cell before", |g| {
            *HERO_WAS.lock().map_err(|e| e.to_string())? = g.world.map.hero();
            Ok(())
        }),
        click(),
        Step::Request("a command after the step into him", |p| {
            !really_attack(p) && command(p)
        }),
        Step::Wait("swapped with or refused, still peaceful, unhurt", |g| {
            let said = logged(g, "swap places") || logged(g, "You stop");
            Ok(said && unhurt(g)?)
        }),
    ]
}

pub(super) fn peaceful_swap() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode; the swap option on", |g| {
        g.debug_mode = true;
        g.settings.gameplay.safe_pet = true;
        Ok(())
    })];
    steps.extend(start());
    // in the room's north-west corner the watchman can only come east,
    // south-east or south of the hero: the ways 5.0 draws him on the
    // hero's old cell before the hero has left it
    steps.push(Step::Wait("the room's corner", |g| {
        let c = room_corner(g);
        *CORNER.lock().map_err(|e| e.to_string())? = c;
        Ok(c.is_some())
    }));
    steps.push(to_corner());
    steps.extend([
        Step::Wait("the hero in the corner", |g| {
            let c = *CORNER.lock().map_err(|e| e.to_string())?;
            if g.world.map.hero() == c {
                Ok(true)
            } else {
                Err(format!(
                    "the hero at {:?}, not the corner {c:?}",
                    g.world.map.hero()
                ))
            }
        }),
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("peaceful watchman".into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
        Step::Wait("the watchman beside the hero, peaceful", |g| {
            let p = peaceful_beside(g);
            *AT.lock().map_err(|e| e.to_string())? = p;
            Ok(p.is_some())
        }),
    ]);
    steps.extend(swap_steps());
    steps.push(Step::Call("the swap went east or south", |g| {
        let was = HERO_WAS
            .lock()
            .map_err(|e| e.to_string())?
            .ok_or("no cell before")?;
        let now = g.world.map.hero().ok_or("no hero")?;
        let way = (now.0 - was.0, now.1 - was.1);
        godot::prelude::godot_print!("selftest: peaceful-swap: the hero went {way:?}");
        if way == (0, 0) {
            return Err("the watchman refused the swap".into());
        }
        if !(way.1 > 0 || (way.1 == 0 && way.0 > 0)) {
            return Err(format!("the hero went {way:?}, not east or south"));
        }
        Ok(())
    }));
    // and back: the next click swaps again, no blow
    steps.extend(swap_steps());
    steps.extend(quit());
    steps
}
