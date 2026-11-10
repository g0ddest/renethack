//! `motion`: a fight caught frame by frame, for sheets of how blows,
//! hits and deaths move (the art-director's motion bursts). A debug-mode
//! Valkyrie (her kitten slain first, so it fights nothing for her), a
//! hill orc made beside her, the camera 7 m off; she waits
//! while it strikes her, then strikes it again and again; after each turn
//! a burst of pictures `motion-<round>-<frame>` a few frames apart catches
//! its blows and her reeling, then her swings, its reeling and its fall.
//! Then soldier ants made beside her bite until she falls; at debug
//! mode's "Die?" she says no, and must stand again ("OK, so you don't
//! die"), on the map and on the inventory's doll (`motion-risen-doll`);
//! they bite on till she dies, and a burst `motion-death-<frame>` catches
//! her fall. Nothing else is checked but that the fight was drawn.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

use godot::classes::{DisplayServer, Engine};
use godot::obj::Singleton;
use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, camera_settled, command, ctrl_key, key, map_view, start};
use crate::game::RenethackGame;
use crate::inventory_panel::InvInput;
use crate::ui_events::UiEvent;

/// Turns she waits (it strikes), turns in all, pictures after each,
/// frames between pictures.
const WAITS: usize = 3;
const ROUNDS: usize = 9;
const BURST: usize = 9;
const APART: i64 = 3;

static FRAME: AtomicI64 = AtomicI64::new(0);
/// The pet's cell (slain first: it would fight the foe for her), and
/// whether #wizkill's cursor picked it.
static PET: Mutex<(i32, i32)> = Mutex::new((0, 0));
static PICKED: AtomicBool = AtomicBool::new(false);
/// Turns waited for the ants, and whether she was let die.
static WAITED: AtomicUsize = AtomicUsize::new(0);
static DIED: AtomicBool = AtomicBool::new(false);
/// She said no at her first "Die?" (debug mode).
static REFUSED: AtomicBool = AtomicBool::new(false);

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// The way to a hostile beside the hero (a vi-key), if one is there.
fn foe(g: &RenethackGame) -> Option<char> {
    let (hx, hy) = g.world.map.hero()?;
    [
        ((-1, 0), 'h'),
        ((1, 0), 'l'),
        ((0, -1), 'k'),
        ((0, 1), 'j'),
        ((-1, -1), 'y'),
        ((1, -1), 'u'),
        ((-1, 1), 'b'),
        ((1, 1), 'n'),
    ]
    .into_iter()
    .find_map(|((dx, dy), k)| {
        let gl = g.world.map.cell(hx + dx, hy + dy)?.glyph.as_ref()?;
        (gl.kind == GlyphKind::Mon && gl.flags & (mg::PET | mg::HERO) == 0).then_some(k)
    })
}

/// The pet's cell near the hero.
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

/// The pet slain by #wizkill, its cursor walked to it.
fn slay_pet() -> Vec<Step> {
    vec![
        Step::Wait("the pet near the hero", |g| {
            let at = pet(g);
            if let Some(at) = at {
                *PET.lock().unwrap() = at;
            }
            Ok(at.is_some())
        }),
        Step::Call("", |_| {
            PICKED.store(false, Ordering::Relaxed);
            Ok(())
        }),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        Step::KeysUntil("the pet picked and slain", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            if !g.world.getpos {
                return Ok(PICKED
                    .load(Ordering::Relaxed)
                    .then_some(())
                    .map_or(Some(KeyInput::plain(Key::Escape)), |_| None));
            }
            if PICKED.load(Ordering::Relaxed) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let target = *PET.lock().unwrap();
            let (cx, cy) = g.world.cursor.or(g.world.map.hero()).ok_or("no cursor")?;
            if (cx, cy) == target {
                PICKED.store(true, Ordering::Relaxed);
                return Ok(Some(KeyInput::plain(Key::Char('.'))));
            }
            let (dx, dy) = ((target.0 - cx).signum(), (target.1 - cy).signum());
            let way = super::audio_tests::STEPS
                .iter()
                .find(|(d, _)| *d == (dx, dy))
                .ok_or("no way to the pet")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }),
    ]
}

pub(super) fn motion() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 11", |g| {
        g.debug_mode = true;
        g.seed = Some(11);
        Ok(())
    })];
    steps.extend(start());
    steps.extend([Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    })]);
    steps.extend(slay_pet());
    steps.extend([
        Step::Key(ctrl_key('g')),
        Step::Request(
            "the kind of monster to make",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("hill orc".into())),
        Step::Request("a command after the monster", command),
        Step::Call("7 m off, as in play", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(7.0, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    for round in 0..ROUNDS {
        // (`m` lets a search pass the turn beside a hostile)
        if round < WAITS {
            steps.extend([
                Step::KeyFrom("m, to wait beside it", |_| {
                    Ok(KeyInput::plain(Key::Char('m')))
                }),
                Step::KeyFrom("s", |_| Ok(KeyInput::plain(Key::Char('s')))),
            ]);
        } else {
            steps.extend([
                Step::KeyFrom("F, to fight (or m)", |g| {
                    let k = if foe(g).is_some() { 'F' } else { 'm' };
                    Ok(KeyInput::plain(Key::Char(k)))
                }),
                Step::KeyFrom("toward the orc (or a search)", |g| {
                    Ok(KeyInput::plain(Key::Char(foe(g).unwrap_or('s'))))
                }),
            ]);
        }
        steps.extend([Step::Call("the burst begins", |_| {
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        })]);
        for k in 0..BURST {
            let name: &'static str = format!("motion-{round}-{k}").leak();
            steps.extend([
                Step::Wait("a few frames on", |_| {
                    Ok(frames() >= FRAME.load(Ordering::Relaxed) + APART)
                }),
                Step::Shot(name),
                Step::Call("the next frame of the burst", |_| {
                    FRAME.store(frames(), Ordering::Relaxed);
                    Ok(())
                }),
            ]);
        }
        steps.push(Step::Request("the next command", command));
    }
    for _ in 0..2 {
        steps.extend([
            Step::Key(ctrl_key('g')),
            Step::Request(
                "the kind of monster to make",
                |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster")),
            ),
            Step::Dialog(DialogEvent::TextSubmitted("soldier ant".into())),
            Step::Request("a command after the monster", command),
        ]);
    }
    steps.extend([
        Step::Call("", |_| {
            WAITED.store(0, Ordering::Relaxed);
            Ok(())
        }),
        // she falls; at "Die?" (debug mode) she says no, and stands again,
        // on the map and on the inventory's doll (the map's model)
        Step::KeysUntil("waiting till she falls", |g| {
            match g.pending.as_ref().map(|(_, p)| p) {
                Some(Prompt::Command) if REFUSED.load(Ordering::Relaxed) => Ok(None),
                Some(Prompt::Command) => {
                    let n = WAITED.fetch_add(1, Ordering::Relaxed);
                    if n > 400 {
                        return Err("she never fell".into());
                    }
                    Ok(Some(KeyInput::plain(Key::Char(if n.is_multiple_of(2) {
                        'm'
                    } else {
                        's'
                    }))))
                }
                Some(Prompt::Choice { query, .. }) if query.contains("Die?") => {
                    REFUSED.store(true, Ordering::Relaxed);
                    Ok(Some(KeyInput::plain(Key::Char('n'))))
                }
                _ => Ok(Some(KeyInput::plain(Key::Escape))),
            }
        }),
        Step::Wait("\"OK, so you don't die\" drawn", |g| {
            Ok(map_view(g)?.drawn_generation() == Some(g.world.map.generation()))
        }),
        Step::Call("she stands again", |g| {
            let (down, clip) = map_view(g)?.hero_lies();
            if down || clip.as_deref().is_some_and(|c| c.contains("Death")) {
                return Err(format!(
                    "she lies dead after \"OK, so you don't die\": {clip:?}"
                ));
            }
            Ok(())
        }),
        Step::Call("the inventory opened", |g| {
            g.push_ui(UiEvent::Inventory(InvInput::Toggle));
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the doll drawn", |g| {
            let Some(p) = g.ui.as_ref().map(|ui| &ui.inventory) else {
                return Err("no UI".into());
            };
            Ok(p.is_open()
                && (p.doll_rendered() || DisplayServer::singleton().get_name() == "headless"))
        }),
        Step::Wait("a few frames on", |_| {
            Ok(frames() >= FRAME.load(Ordering::Relaxed) + APART * 4)
        }),
        Step::Shot("motion-risen-doll"),
        Step::Call("the inventory closed", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            Ok(())
        }),
        Step::Request("the next command", command),
        Step::KeysUntil("waiting till she dies", |g| {
            match g.pending.as_ref().map(|(_, p)| p) {
                Some(Prompt::Command) => {
                    let n = WAITED.fetch_add(1, Ordering::Relaxed);
                    if n > 800 {
                        return Err("she never died".into());
                    }
                    Ok(Some(KeyInput::plain(Key::Char(if n.is_multiple_of(2) {
                        'm'
                    } else {
                        's'
                    }))))
                }
                Some(Prompt::Choice { query, .. }) if query.contains("Die?") => {
                    DIED.store(true, Ordering::Relaxed);
                    Ok(Some(KeyInput::plain(Key::Char('y'))))
                }
                _ if DIED.load(Ordering::Relaxed) => Ok(None),
                _ => Ok(Some(KeyInput::plain(Key::Escape))),
            }
        }),
        Step::Call("the burst begins", |_| {
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
    ]);
    for k in 0..BURST * 2 {
        let name: &'static str = format!("motion-death-{k}").leak();
        steps.extend([
            Step::Wait("a few frames on", |_| {
                Ok(frames() >= FRAME.load(Ordering::Relaxed) + APART * 2)
            }),
            Step::Shot(name),
            Step::Call("the next frame of the burst", |_| {
                FRAME.store(frames(), Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps
}
