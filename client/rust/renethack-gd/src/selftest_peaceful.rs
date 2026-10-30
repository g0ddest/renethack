//! A click on a peaceful creature never strikes it unasked (`peaceful`): a
//! peaceful watchman made beside the hero in debug mode. The mouse's
//! click, then the gamepad's A with its cursor on him, step into him as
//! NetHack's move does, with no `F` (whose blow 5.0 deals without the
//! question): with the pet option that swaps places off, the engine asks
//! "Really attack the watchman?", No, and he stays peaceful and unhurt.
//! `peaceful-swap`: with it on (the default), a click swaps places with
//! him or is refused, and he stays peaceful.

use std::sync::Mutex;

use nh_world::Prompt;

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

pub(super) fn peaceful_swap() -> Vec<Step> {
    let mut steps = with_watchman(true);
    steps.extend([
        click(),
        Step::Request("a command after the step into him", |p| {
            !really_attack(p) && command(p)
        }),
        Step::Wait("swapped with or refused, still peaceful", |g| {
            let said = logged(g, "swap places") || logged(g, "You stop");
            Ok(said && unhurt(g)?)
        }),
    ]);
    steps.extend(quit());
    steps
}
