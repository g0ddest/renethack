//! `falls`: a monster the hero sees die falls where it stood, in a
//! debug-mode game where ^G makes each and #wizkill slays it. A lizard
//! (always a corpse, a body built in code) topples onto its side and its
//! corpse takes over in place; an ogre (always a corpse) plays its death
//! clip and its corpse takes over from the clip's last frame; an acid blob
//! slumps; a ghost dissolves and leaves nothing; a gas spore, slain from
//! two cells off, is gone in its blast. A monster gone from sight (the
//! hero blinded) falls not at all. Pictures mid-fall and after with
//! `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use nh_protocol::GlyphKind;
use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, command, ctrl_key, key, map_view, start};
use crate::game::RenethackGame;
use crate::map_view::FallKind;

const SEED: u64 = 11;

/// A monster slain: the name ^G makes it by, how it falls, whether its
/// corpse must take over (None: as the engine's dice say), the pictures
/// mid-fall and after.
#[derive(Clone, Copy)]
struct Victim {
    name: &'static str,
    kind: FallKind,
    corpse: Option<bool>,
    shot: &'static str,
    after: &'static str,
}

/// The monsters slain, in order.
const VICTIMS: [Victim; 5] = [
    Victim {
        name: "lizard",
        kind: FallKind::Topple,
        corpse: Some(true),
        shot: "falls-lizard",
        after: "falls-lizard-after",
    },
    Victim {
        name: "ogre",
        kind: FallKind::Clip,
        corpse: Some(true),
        shot: "falls-ogre",
        after: "falls-ogre-after",
    },
    Victim {
        name: "acid blob",
        kind: FallKind::Slump,
        corpse: None,
        shot: "falls-blob",
        after: "falls-blob-after",
    },
    Victim {
        name: "ghost",
        kind: FallKind::Dissolve,
        corpse: Some(false),
        shot: "falls-ghost",
        after: "falls-ghost-after",
    },
    Victim {
        name: "gas spore",
        kind: FallKind::Burst,
        corpse: Some(false),
        shot: "falls-spore",
        after: "falls-spore-after",
    },
];
/// The victim now (an index into `VICTIMS`, from 1), its cell, and
/// whether #wizkill's cursor picked it.
static NOW: AtomicUsize = AtomicUsize::new(0);
static AT: Mutex<(i32, i32)> = Mutex::new((0, 0));
static PICKED: AtomicBool = AtomicBool::new(false);

fn victim() -> Victim {
    VICTIMS[NOW.load(Ordering::Relaxed).saturating_sub(1)]
}

pub(super) fn falls() -> Vec<Step> {
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
    for v in VICTIMS {
        steps.extend(slay(v));
    }
    steps.extend(unseen());
    steps
}

/// The next victim made beside the hero (a gas spore stepped away from),
/// slain, seen falling, its corpse in its place or nothing left.
fn slay(v: Victim) -> Vec<Step> {
    let mut steps = vec![Step::Call("the next victim", |_| {
        NOW.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(make(v.name));
    steps.push(Step::Wait("the victim beside the hero", |g| {
        let at = find(g, victim().name);
        if let Some(at) = at {
            *AT.lock().unwrap() = at;
        }
        Ok(at.is_some())
    }));
    if v.kind == FallKind::Burst {
        // two cells off its blast
        steps.push(Step::Call("", |_| {
            STEPPED.store(0, Ordering::Relaxed);
            Ok(())
        }));
        steps.push(Step::KeysUntil("away from the spore", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
            let spore = find(g, "gas spore").ok_or("the spore is gone")?;
            *AT.lock().unwrap() = spore;
            let off = |(x, y): (i32, i32)| (x - spore.0).abs().max((y - spore.1).abs());
            if off((hx, hy)) >= 2 {
                return Ok(None);
            }
            if STEPPED.fetch_add(1, Ordering::Relaxed) >= AWAY_MOST {
                return Err("the spore keeps beside the hero".into());
            }
            let way = super::audio_tests::STEPS
                .iter()
                .filter(|((dx, dy), _)| open(g, (hx + dx, hy + dy)))
                .max_by_key(|((dx, dy), _)| off((hx + dx, hy + dy)))
                .ok_or("nowhere to step")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }));
    }
    steps.extend(closer(true));
    steps.extend(wizkill());
    steps.extend([
        Step::Wait("the fall begun, or over", |g| {
            let view = map_view(g)?;
            let at = *AT.lock().unwrap();
            let kind = victim().kind;
            let falling = view.falls().iter().any(|&(c, k, _)| c == at && k == kind);
            let fell = view.fell().is_some_and(|f| f.cell == at && f.kind == kind);
            Ok(falling || fell)
        }),
        // halfway down (a blast is over at once)
        Step::Wait("the fall halfway", |g| {
            let view = map_view(g)?;
            let at = *AT.lock().unwrap();
            let half = view.falls().iter().any(|&(c, _, k)| c == at && k >= 0.5);
            Ok(half || view.fell().is_some_and(|f| f.cell == at))
        }),
        Step::Shot(v.shot),
        Step::Wait("the fall over, its corpse in its place", |g| {
            let view = map_view(g)?;
            let at = *AT.lock().unwrap();
            let Victim { kind, corpse, .. } = victim();
            if view.falls().iter().any(|&(c, _, _)| c == at) {
                return Ok(false);
            }
            let Some(f) = view.fell().filter(|f| f.cell == at) else {
                return Ok(false);
            };
            if f.kind != kind {
                return Err(format!("fell as {:?}, not {kind:?}", f.kind));
            }
            if f.corpse && !(f.off.0 < 0.01 && f.off.1 < 1.0) {
                return Err(format!("the corpse took over {:?} away", f.off));
            }
            if corpse.is_some_and(|c| c != f.corpse) {
                return Err(format!("a corpse took over: {}", f.corpse));
            }
            Ok(true)
        }),
        Step::Shot(v.after),
    ]);
    steps.extend(closer(false));
    steps
}

/// The camera close to the victim's fall (`near`), or back.
fn closer(near: bool) -> [Step; 2] {
    [
        if near {
            Step::Call("closer", |g| {
                g.ui.as_mut().ok_or("no UI")?.map.set_distance(6.5, 0.0);
                Ok(())
            })
        } else {
            Step::Call("back", |g| {
                g.ui.as_mut().ok_or("no UI")?.map.set_distance(11.0, 0.0);
                Ok(())
            })
        },
        Step::Wait("the camera on the hero", super::camera_settled),
    ]
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

/// #wizkill: its cursor walked to the victim and picked, then no more.
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
            let commanded = matches!(g.pending, Some((_, Prompt::Command)));
            if !commanded {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            if !g.world.getpos {
                if PICKED.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                return Err("#wizkill asked for no monster".into());
            }
            if PICKED.load(Ordering::Relaxed) {
                // "Next monster": none
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let target = *AT.lock().unwrap();
            let (cx, cy) = g.world.cursor.or(g.world.map.hero()).ok_or("no cursor")?;
            if (cx, cy) == target {
                PICKED.store(true, Ordering::Relaxed);
                return Ok(Some(KeyInput::plain(Key::Char('.'))));
            }
            let (dx, dy) = ((target.0 - cx).signum(), (target.1 - cy).signum());
            let way = super::audio_tests::STEPS
                .iter()
                .find(|(d, _)| *d == (dx, dy))
                .ok_or("no way to the victim")?;
            Ok(Some(KeyInput::plain(Key::Char(way.1))))
        }),
    ]
}

/// A monster unseen (the hero blinded) slain: no fall, no body left.
fn unseen() -> Vec<Step> {
    let mut steps = vec![Step::Call("a newt to slay unseen", |_| {
        NOW.store(0, Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(make("newt"));
    steps.push(Step::Wait("the newt beside the hero", |g| {
        let at = find(g, "newt");
        if let Some(at) = at {
            *AT.lock().unwrap() = at;
        }
        Ok(at.is_some())
    }));
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        Step::KeyFrom("blinded", |g| super::entry_key(g, "blinded")),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, blind", command),
        Step::Call("", |g| {
            let falls = map_view(g)?.motion_stats().falls;
            UNSEEN_FROM.store(falls as usize, Ordering::Relaxed);
            Ok(())
        }),
    ]);
    steps.extend(wizkill());
    steps.extend([
        Step::Request("a command after the kill", command),
        Step::Wait("no fall for what the hero did not see", |g| {
            let view = map_view(g)?;
            let falls = view.motion_stats().falls as usize;
            if falls != UNSEEN_FROM.load(Ordering::Relaxed) {
                return Err("a monster fell that the hero could not see".into());
            }
            Ok(view.falls().is_empty())
        }),
    ]);
    steps
}

/// Falls counted before the unseen kill.
static UNSEEN_FROM: AtomicUsize = AtomicUsize::new(0);

/// The cell of the monster named `name` nearest the hero.
fn find(g: &RenethackGame, name: &str) -> Option<(i32, i32)> {
    let cat = g.catalog.as_deref()?;
    let (hx, hy) = g.world.map.hero()?;
    let mut best: Option<((i32, i32), i32)> = None;
    for y in (hy - 3)..=(hy + 3) {
        for x in (hx - 3)..=(hx + 3) {
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

/// Floor to step on: open, or with a thing lying on it (a corpse of
/// those slain before), nobody on it.
fn open(g: &RenethackGame, (x, y): (i32, i32)) -> bool {
    let Some(cat) = g.catalog.as_deref() else {
        return false;
    };
    g.world.map.cell(x, y).is_some_and(|c| {
        let lying = c
            .glyph
            .as_ref()
            .is_some_and(|gl| matches!(gl.kind, GlyphKind::Obj | GlyphKind::Body));
        let floor = c.entity().is_none()
            && matches!(
                nh_world::cell_terrain(c, cat),
                Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
            );
        lying || floor
    })
}

/// Steps taken away from the spore, and the most taken.
static STEPPED: AtomicUsize = AtomicUsize::new(0);
const AWAY_MOST: usize = 6;
