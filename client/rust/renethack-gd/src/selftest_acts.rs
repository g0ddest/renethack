//! `acts`: what monsters do, shown as they do it, in a debug-mode game
//! where ^G makes them beside the hero. A gnomish wizard's spell shows on
//! the caster where the map shows it (its cast clip, turned to the hero),
//! the glow in its hand and, aimed at the hero, the spell striking them.
//! Blind, the hero sees none of it, and nothing is shown. Pictures with
//! `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, command, ctrl_key, entry_key, key, map_view, start};
use crate::game::RenethackGame;
use crate::map_view::{ActKind, Acted, Aim};

const SEED: u64 = 13;

/// The caster's cell, the acts shown before a mark, and the log's length
/// then.
static AT: Mutex<(i32, i32)> = Mutex::new((0, 0));
static SHOWN: AtomicUsize = AtomicUsize::new(0);
static LOGGED: AtomicU64 = AtomicU64::new(0);

pub(super) fn acts() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        g.seed = Some(SEED);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend(no_pet());
    steps.extend(casting());
    steps
}

/// The pet slain first: it would kill what the scenes need alive.
fn no_pet() -> Vec<Step> {
    let mut steps = vec![Step::Wait("the pet found", |g| {
        let pet = pet(g);
        if let Some(at) = pet {
            *AT.lock().unwrap() = at;
        }
        Ok(pet.is_some())
    })];
    steps.extend(wizkill());
    steps.push(Step::Wait("no pet", |g| Ok(pet(g).is_none())));
    steps
}

/// The pet's cell, if the map shows it.
fn pet(g: &RenethackGame) -> Option<(i32, i32)> {
    let (hx, hy) = g.world.map.hero()?;
    (hy - 3..=hy + 3)
        .flat_map(|y| (hx - 3..=hx + 3).map(move |x| (x, y)))
        .find(|&(x, y)| {
            g.world
                .map
                .cell(x, y)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|gl| gl.kind == GlyphKind::Mon && gl.flags & mg::PET != 0)
        })
}

/// Whether #wizkill's cursor picked its victim.
static PICKED: AtomicBool = AtomicBool::new(false);

/// #wizkill: its cursor walked to `AT` and picked, then no more.
fn wizkill() -> Vec<Step> {
    vec![
        Step::Call("", |_| {
            PICKED.store(false, Ordering::Relaxed);
            Ok(())
        }),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        Step::KeysUntil("the victim picked and slain", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            if !g.world.getpos {
                if PICKED.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                return Err("#wizkill asked for no monster".into());
            }
            if PICKED.load(Ordering::Relaxed) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let target = *AT.lock().unwrap();
            let (cx, cy) = g.world.cursor.or(g.world.map.hero()).ok_or("no cursor")?;
            if (cx, cy) == target {
                PICKED.store(true, Ordering::Relaxed);
                return Ok(Some(KeyInput::plain(Key::Char('.'))));
            }
            let d = ((target.0 - cx).signum(), (target.1 - cy).signum());
            let way = super::audio_tests::STEPS
                .iter()
                .find(|(s, _)| *s == d)
                .ok_or("no way to the victim")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }),
    ]
}

/// A gnomish wizard beside the hero. A low one fumbles or curses (the
/// engine's own lines, waited for), and that shows on it; a spell told
/// cast at the hero (the line put in the log as the engine would, the
/// wizard seen) shows its clip, its glow and its strike. Blind, the same
/// line shows nothing: the hero does not see the wizard.
fn casting() -> Vec<Step> {
    let mut steps = Vec::from(make("gnomish wizard"));
    steps.push(Step::Wait("the wizard beside the hero", |g| {
        let at = find(g, "gnomish wizard");
        if let Some(at) = at {
            *AT.lock().unwrap() = at;
        }
        Ok(at.is_some())
    }));
    steps.push(Step::Call("closer", |g| {
        g.ui.as_mut().ok_or("no UI")?.map.set_distance(6.5, 0.0);
        Ok(())
    }));
    steps.push(mark());
    steps.extend([
        Step::KeysUntil("turns till it fumbles or curses", |g| {
            let fumbled = |t: &str| {
                t == "The air crackles around the gnomish wizard."
                    || t.starts_with("The gnomish wizard points ")
            };
            if logged_since(g, fumbled) {
                return Ok(None);
            }
            wait_a_turn(g)
        }),
        Step::Wait("that shown on the wizard", |g| {
            Ok(shown_on(g, |a| match a.kind {
                ActKind::Curse(_) => a.clip,
                ActKind::Crackle => true,
                ActKind::Cast(_) => false,
            }))
        }),
        mark(),
        Step::Call("a spell told cast at the hero", |g| {
            *AT.lock().unwrap() = find(g, "gnomish wizard").ok_or("the wizard is gone")?;
            tell(g, "The gnomish wizard casts a spell at you!");
            Ok(())
        }),
        Step::Wait("its cast shown, on the wizard", |g| {
            Ok(shown_on(g, |a| {
                a.kind == ActKind::Cast(Aim::Hero) && a.clip
            }))
        }),
        // the glow in its hand, then the spell where it went
        Step::Wait("its glow drawn", |g| Ok(map_view(g)?.acts_pending() <= 1)),
        Step::Shot("acts-cast"),
        Step::Wait("its spell drawn", |g| Ok(map_view(g)?.acts_pending() == 0)),
        Step::Shot("acts-cast-strikes"),
        Step::Call("back", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(11.0, 0.0);
            Ok(())
        }),
    ]);
    // blind: the wizard is not seen, and its spell shows nothing
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        Step::KeyFrom("blinded", |g| entry_key(g, "blinded")),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, blind", command),
        mark(),
        Step::Call("the same line told, blind", |g| {
            tell(g, "The gnomish wizard casts a spell at you!");
            Ok(())
        }),
        Step::KeysUntil("a turn after it", |g| {
            if WAITED.load(Ordering::Relaxed) >= 2 {
                return Ok(None);
            }
            wait_a_turn(g)
        }),
        Step::Wait("nothing shown of it", |g| {
            let view = map_view(g)?;
            if view.acts_shown().len() != SHOWN.load(Ordering::Relaxed) {
                return Err(format!("shown unseen: {:?}", view.acts_shown().last()));
            }
            Ok(true)
        }),
    ]);
    steps
}

/// A line put in the log as the engine tells it.
fn tell(g: &mut RenethackGame, text: &str) {
    let turn = g.world.turn();
    g.world.log.push(text.into(), 0, turn, false);
}

/// An act since the mark on the monster at `AT` that `is`.
fn shown_on(g: &RenethackGame, is: fn(&Acted) -> bool) -> bool {
    let Ok(view) = map_view(g) else {
        return false;
    };
    let at = *AT.lock().unwrap();
    let all = view.acts_shown();
    let new = &all[SHOWN.load(Ordering::Relaxed).min(all.len())..];
    match new.iter().find(|a| a.cell == at && is(a)) {
        Some(a) => {
            godot::prelude::godot_print!("selftest: shown {a:?}");
            true
        }
        None => false,
    }
}

/// From now on: the acts and the lines a check looks at must come after.
fn mark() -> Step {
    Step::Call("", |g| {
        SHOWN.store(map_view(g)?.acts_shown().len(), Ordering::Relaxed);
        LOGGED.store(g.world.log.last_seq(), Ordering::Relaxed);
        WAITED.store(0, Ordering::Relaxed);
        Ok(())
    })
}

/// A line since the mark that `says`.
fn logged_since(g: &RenethackGame, says: fn(&str) -> bool) -> bool {
    let from = LOGGED.load(Ordering::Relaxed);
    g.world.log.since(from).any(|m| says(&m.text))
}

/// Turns waited, at most `WAIT_MOST`; whether the `m` before the next
/// search was sent.
static WAITED: AtomicU32 = AtomicU32::new(0);
static FORCING: AtomicBool = AtomicBool::new(false);
const WAIT_MOST: u32 = 80;

/// A turn searched (`m`, then `s`: a search beside a hostile is refused
/// unless forced); any other question answered no.
fn wait_a_turn(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if !matches!(g.pending, Some((_, Prompt::Command))) {
        return Ok(Some(KeyInput::plain(Key::Escape)));
    }
    if !FORCING.swap(false, Ordering::Relaxed) {
        FORCING.store(true, Ordering::Relaxed);
        return Ok(Some(KeyInput::plain(Key::Char('m'))));
    }
    if WAITED.fetch_add(1, Ordering::Relaxed) >= WAIT_MOST {
        return Err("it never did".into());
    }
    Ok(Some(KeyInput::plain(Key::Char('s'))))
}

/// ^G and a monster's name.
fn make(name: &'static str) -> [Step; 4] {
    [
        Step::Key(ctrl_key('g')),
        Step::Request(
            "the kind of monster to make",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
        Step::Request("a command after the monster", command),
    ]
}

/// The cell of the monster named `name` nearest the hero.
fn find(g: &RenethackGame, name: &str) -> Option<(i32, i32)> {
    let cat = g.catalog.as_deref()?;
    let (hx, hy) = g.world.map.hero()?;
    let mut best: Option<((i32, i32), i32)> = None;
    for y in (hy - 4)..=(hy + 4) {
        for x in (hx - 4)..=(hx + 4) {
            let Some(gl) = g.world.map.cell(x, y).and_then(|c| c.glyph.as_ref()) else {
                continue;
            };
            let named = gl.kind == GlyphKind::Mon
                && gl
                    .mon
                    .and_then(|m| usize::try_from(m).ok())
                    .and_then(|m| cat.monsters.get(m))
                    .is_some_and(|m| m.name == name);
            let d = (x - hx).abs().max((y - hy).abs());
            if named && best.is_none_or(|(_, b)| d < b) {
                best = Some(((x, y), d));
            }
        }
    }
    best.map(|(c, _)| c)
}
