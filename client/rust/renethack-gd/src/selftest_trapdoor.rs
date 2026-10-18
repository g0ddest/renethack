//! `trapdoor`: falling through the floor, as it is seen, in a debug-mode
//! game where wishes make the traps under the hero. A dwarf seen to fall
//! through a trap door steps onto it and drops in, the lid's leaves
//! swinging down, and the lid lies shut again after it. The hero steps
//! onto the trap door and drops through: the level they leave is held
//! while they do, its floor open under them, then black and the level
//! below. There they fall into a pit, the floor open round them, climb
//! out, and step into a hole (one they escape they go down with '>'); on
//! the next level they zap a wand of digging down and fall through the
//! hole dug. A trap door creaks, a thud comes from below. Pictures of the
//! steps and mid-fall with `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use nh_protocol::{Glyph, GlyphKind};
use nh_world::{Key, KeyInput, Prompt};

use super::audio_tests::{STEPS, letter, open_way, wish};
use super::polyself_tests::monster;
use super::{Step, command, key, map_view, start};
use crate::game::RenethackGame;
use crate::map_view::Opening;

const SEED: u64 = 17;

/// A trap made: its cell, the step off it and the step back.
type Made = ((i32, i32), char, char);

/// The trap made last.
static TRAP: Mutex<Option<Made>> = Mutex::new(None);
/// The cell the dwarf was drawn on beside the trap door, and what it
/// showed before.
static DWARF: Mutex<Option<((i32, i32), Glyph)>> = Mutex::new(None);
/// The level the hero falls from.
static FROM: AtomicI64 = AtomicI64::new(0);
/// Where the hero set out into the room from.
static SET_OUT: Mutex<Option<(i32, i32)>> = Mutex::new(None);
/// The log's length when a wish was made: what it checks came after.
static MARK: AtomicU64 = AtomicU64::new(0);
/// The sounds asked for before a fall: what it checks came after.
static HEARD: AtomicU64 = AtomicU64::new(0);
/// Seconds into a drop its picture waits for (taken a few frames on):
/// the hero half down, before the black; a monster's, smaller, sooner.
const HERO_MID: f32 = 0.14;
const MONSTER_MID: f32 = 0.1;
/// A picture of the step onto the opening waits for none if less than
/// this is left of it.
const STEP_SHOT: f32 = 0.1;

pub(super) fn trapdoor() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        g.seed = Some(SEED);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend(into_room());
    steps.extend(made_and_left("trap door"));
    steps.extend([
        Step::Call("closer", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(6.5, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero", super::camera_settled),
    ]);
    steps.extend(dwarf_drops());
    steps.extend(falls_through(
        Opening::TrapDoor,
        false,
        ("trapdoor-door-step", "trapdoor-door"),
    ));
    // where a fall lands them is open floor (never stairs): the traps are
    // made there, before what lives on the level comes for them
    steps.extend(made_and_left("pit"));
    steps.extend(into_pit());
    steps.extend(made_and_left("hole"));
    steps.extend(falls_through(
        Opening::Hole,
        true,
        ("trapdoor-hole-step", "trapdoor-hole"),
    ));
    steps.extend(digs_down());
    steps
}

/// Off the stairs the game began on (no trap is made on stairs), out
/// into the room till open floor is all about the hero: nothing stands
/// between the eye and what opens under them.
fn into_room() -> [Step; 2] {
    [
        Step::Call("", |g| {
            *SET_OUT.lock().unwrap() = g.world.map.hero();
            Ok(())
        }),
        Step::KeysUntil("out into the room", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let moved = g.world.map.hero() != *SET_OUT.lock().unwrap();
            if moved && open_all_round(g)? {
                return Ok(None);
            }
            Ok(Some(KeyInput::plain(Key::Char(roomward(g)?))))
        }),
    ]
}

/// The trap `what` wished up under the hero, and a step off it.
fn made_and_left(what: &'static str) -> Vec<Step> {
    let mut steps = vec![Step::Call("", mark)];
    steps.extend(wish(what));
    steps.extend([
        Step::Wait("the trap made", |g| {
            let from = MARK.load(Ordering::Relaxed);
            match g
                .world
                .log
                .since(from)
                .find(|m| m.text.contains("Creation of"))
            {
                Some(m) => Err(m.text.clone()),
                None => Ok(true),
            }
        }),
        Step::KeyFrom("a step off it", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            let way = open_way(g)?;
            let ((dx, dy), _) = STEPS.iter().find(|(_, k)| *k == way).ok_or("no way")?;
            let back = STEPS
                .iter()
                .find(|((x, y), _)| (*x, *y) == (-dx, -dy))
                .ok_or("no way back")?
                .1;
            *TRAP.lock().unwrap() = Some((hero, way, back));
            Ok(KeyInput::plain(Key::Char(way)))
        }),
        Step::Request("a command off it", command),
    ]);
    steps
}

/// A dwarf beside the trap door (drawn there as the map would show one)
/// seen to fall through it, as the engine shows that: gone from its cell,
/// the trap door shown, the line told. It steps onto it and drops in, the
/// lid's leaves swinging down; once it is gone the lid lies shut again.
fn dwarf_drops() -> [Step; 9] {
    [
        Step::Call("a dwarf beside the trap door", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let ((tx, ty), way, _) = trap()?;
            let hero = g.world.map.hero().ok_or("no hero")?;
            // across the trap from the hero, else any open floor beside it
            let ((dx, dy), _) = STEPS.iter().find(|(_, k)| *k == way).ok_or("no way")?;
            let open = |c: (i32, i32)| {
                c != hero
                    && g.world.map.cell(c.0, c.1).is_some_and(|cell| {
                        matches!(
                            nh_world::cell_terrain(cell, &cat),
                            Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
                        ) && cell.entity().is_none()
                    })
            };
            let at = std::iter::once((tx - dx, ty - dy))
                .chain(STEPS.iter().map(|((x, y), _)| (tx + x, ty + y)))
                .find(|&c| open(c))
                .ok_or("no floor beside the trap door")?;
            let was = g.world.map.cell(at.0, at.1).and_then(|c| c.glyph.clone());
            *DWARF.lock().unwrap() = Some((at, was.ok_or("nothing drawn there")?));
            g.world
                .map
                .print(at.0, at.1, &monster(&cat, "dwarf")?, None);
            Ok(())
        }),
        Step::Wait("the dwarf drawn", |g| {
            let ((x, y), _) = dwarf()?;
            Ok(map_view(g)?
                .entity_model(x, y)
                .is_some_and(|m| !m.is_pending()))
        }),
        Step::Call("", mark_heard),
        Step::Call("the dwarf falls through", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let ((x, y), ..) = trap()?;
            let (at, was) = dwarf()?;
            let turn = g.world.turn();
            let line = "Suddenly, the dwarf falls through a trap door.";
            g.world.log.push(line.into(), 0, turn, false);
            g.world.map.print(at.0, at.1, &was, None);
            g.world
                .map
                .print(x, y, &feature(&cat, "S_trap_door")?, None);
            Ok(())
        }),
        Step::Wait("the dwarf stepping onto the trap door", |g| {
            let (cell, ..) = trap()?;
            Ok(map_view(g)?.drops_shown().iter().any(|(c, _)| *c == cell))
        }),
        Step::ShotIf("trapdoor-dwarf-step", |g| {
            let (cell, ..) = trap()?;
            let view = map_view(g)?;
            Ok(view
                .drops_shown()
                .iter()
                .any(|&(c, t)| c == cell && t < -STEP_SHOT))
        }),
        Step::Wait("the dwarf dropping, the lid open under it", |g| {
            let view = map_view(g)?;
            let (cell, ..) = trap()?;
            let Some(&(_, t)) = view.drops_shown().iter().find(|(c, _)| *c == cell) else {
                return Err("the dwarf's drop is over before its picture".into());
            };
            if t < 0.0 {
                return Ok(false);
            }
            match view.opening_drawn(cell) {
                Some((Opening::TrapDoor, true, false)) => Ok(t >= MONSTER_MID),
                other => Err(format!("the cell shows {other:?}")),
            }
        }),
        Step::Shot("trapdoor-dwarf"),
        Step::Wait("the trap door's creak and the thud below heard", |g| {
            Ok(heard(g, "door.open")? && heard(g, "trap.fall")?)
        }),
    ]
}

/// The hero steps back onto the trap (`confirm`: one they know of, asked
/// first) and falls through `o`: the level held as they step onto it and
/// drop, the floor open under them, then the level below. Pictures as
/// they step onto it (`shots.0`) and drop (`shots.1`).
fn falls_through(o: Opening, confirm: bool, shots: (&'static str, &'static str)) -> Vec<Step> {
    let mut steps = vec![Step::Wait("the trap's cell drawn", |g| {
        // a monster seen to drop through it is gone, the lid shut again
        let view = map_view(g)?;
        Ok(view.drops_shown().is_empty() && view.is_settled())
    })];
    steps.extend([
        Step::Call("", |g| {
            let level = g.world.status.number("leveldesc").ok_or("no level")?;
            FROM.store(level, Ordering::Relaxed);
            mark_heard(g)
        }),
        Step::KeyFrom("back onto it", |_| {
            let (_, _, back) = trap()?;
            Ok(KeyInput::plain(Key::Char(back)))
        }),
    ]);
    if confirm {
        // one they know of they may escape (one time in five): then '>'
        // takes them down it, with nothing said
        steps.extend([
            Step::Request("really step onto it?", |p| {
                matches!(p, Prompt::Choice { .. })
            }),
            Step::Call("", mark),
            key('y'),
            Step::Wait("the hero on it", |g| {
                Ok(map_view(g)?.hero_dropping().is_some() || escaped(g)?)
            }),
            Step::KeyIf("down it if they escaped it", |g| {
                if map_view(g)?.hero_dropping().is_none() && escaped(g)? {
                    godot::prelude::godot_print!("selftest: escaped it, '>' down it");
                    return Ok(Some(KeyInput::plain(Key::Char('>'))));
                }
                Ok(None)
            }),
        ]);
    }
    steps.extend([
        Step::Wait("the hero on their way down", |g| {
            Ok(on_way_down(g)?.is_some())
        }),
        Step::ShotIf(shots.0, |g| {
            Ok(on_way_down(g)?.is_some_and(|t| t < -STEP_SHOT))
        }),
    ]);
    steps.push(match o {
        Opening::TrapDoor => Step::Wait("the hero dropping through the trap door", |g| {
            dropping(g, Opening::TrapDoor)
        }),
        _ => Step::Wait("the hero dropping through the hole", |g| {
            dropping(g, Opening::Hole)
        }),
    });
    steps.push(Step::Shot(shots.1));
    steps.extend(below());
    if o == Opening::TrapDoor {
        steps.push(Step::Wait("the trap door's creak heard", |g| {
            heard(g, "door.open")
        }));
    }
    steps
}

/// The hero's drop: the seconds since the floor opened under them, less
/// than 0 while they step onto it (None: not begun).
fn on_way_down(g: &RenethackGame) -> Result<Option<f32>, String> {
    let view = map_view(g)?;
    let t = view.hero_dropping();
    let below = g.world.status.number("leveldesc") > Some(FROM.load(Ordering::Relaxed));
    if t.is_none() && below && view.drawn_generation() == Some(g.world.map.generation()) {
        return Err("the level below came with no drop".into());
    }
    Ok(t)
}

/// The hero escaped the trap they stepped onto, standing on it with a
/// command asked.
fn escaped(g: &RenethackGame) -> Result<bool, String> {
    let from = MARK.load(Ordering::Relaxed);
    let (cell, ..) = trap()?;
    let said = g
        .world
        .log
        .since(from)
        .any(|m| m.text.starts_with("You escape"));
    let asked = matches!(g.pending, Some((_, Prompt::Command)));
    Ok(said && asked && g.world.map.hero() == Some(cell))
}

/// The hero dropping through `o` on the trap's cell, its floor open
/// under them: true once they are half down.
fn dropping(g: &RenethackGame, o: Opening) -> Result<bool, String> {
    let Some(t) = on_way_down(g)?.filter(|&t| t >= 0.0) else {
        return Ok(false);
    };
    let (cell, ..) = trap()?;
    match map_view(g)?.opening_drawn(cell) {
        Some((drawn, true, false)) if drawn == o => Ok(t >= HERO_MID),
        other => Err(format!("the cell under them shows {other:?}")),
    }
}

/// A command on the level below, the drop over and the level drawn.
fn below() -> [Step; 4] {
    [
        Step::AnswerUntil('n', "a command on the level below", |g| {
            let below = g.world.status.number("leveldesc") > Some(FROM.load(Ordering::Relaxed));
            Ok(below && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Wait("the drop over, the level below drawn", |g| {
            let view = map_view(g)?;
            let drawn = view.drawn_generation() == Some(g.world.map.generation());
            Ok(view.hero_dropping().is_none() && drawn && !g.driver.is_active())
        }),
        Step::Wait("the thud below heard", |g| heard(g, "trap.fall")),
        Step::Wait("the hero shown on the level below", |g| {
            Ok(map_view(g)?
                .hero_model()
                .is_some_and(|m| m.node.is_visible()))
        }),
    ]
}

/// From now on: the sounds a check looks for come after this.
fn mark_heard(g: &mut RenethackGame) -> Result<(), String> {
    let audio = &g.ui.as_ref().ok_or("no UI")?.audio;
    HEARD.store(audio.asked(), Ordering::Relaxed);
    Ok(())
}

/// The cue `cue` played since the mark.
fn heard(g: &RenethackGame, cue: &str) -> Result<bool, String> {
    let mark = HEARD.load(Ordering::Relaxed);
    let audio = &g.ui.as_ref().ok_or("no UI")?.audio;
    Ok(audio
        .heard()
        .rev()
        .take_while(|h| h.seq > mark)
        .any(|h| h.cue.as_deref() == Some(cue)))
}

/// The hero steps back into the pit and falls in: their floor open round
/// them, its walls about them; they climb out the way they came.
fn into_pit() -> [Step; 6] {
    [
        Step::KeyFrom("back into it", |_| {
            let (_, _, back) = trap()?;
            Ok(KeyInput::plain(Key::Char(back)))
        }),
        Step::Wait("the hero down in the pit, its floor open round them", |g| {
            if !g.world.hero_trap.is_some_and(|t| t.is_pit()) {
                return Ok(false);
            }
            let (cell, ..) = trap()?;
            if g.world.map.hero() != Some(cell) {
                return Err(format!("in a pit off its cell {cell:?}"));
            }
            let view = map_view(g)?;
            Ok(matches!(
                view.opening_drawn(cell),
                Some((Opening::Pit { .. }, true, false))
            ) && view.is_settled())
        }),
        Step::Wait("the camera on the hero", super::camera_settled),
        Step::Shot("trapdoor-pit"),
        Step::KeysUntil("out of the pit", |g| {
            if !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(Some(KeyInput::plain(Key::Escape)));
            }
            let (cell, way, _) = trap()?;
            if g.world.map.hero() != Some(cell) {
                return Ok(None);
            }
            Ok(Some(KeyInput::plain(Key::Char(way))))
        }),
        Step::Wait("the pit open, no one in it", |g| {
            let (cell, ..) = trap()?;
            Ok(matches!(
                map_view(g)?.opening_drawn(cell),
                Some((Opening::Pit { .. }, true, false))
            ))
        }),
    ]
}

/// A wand of digging zapped down: the hero falls through the hole dug.
fn digs_down() -> Vec<Step> {
    let mut steps = Vec::from(wish("wand of digging"));
    steps.extend([
        Step::Call("", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            *TRAP.lock().unwrap() = Some((hero, '.', '.'));
            let level = g.world.status.number("leveldesc").ok_or("no level")?;
            FROM.store(level, Ordering::Relaxed);
            Ok(())
        }),
        key('z'),
        Step::Request("what to zap", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the wand", |g| letter(g, "wand")),
        Step::Request("the zap's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        key('>'),
        Step::Wait("the hero dropping through the hole dug", |g| {
            dropping(g, Opening::Hole)
        }),
        Step::Shot("trapdoor-dug"),
    ]);
    steps.extend(below());
    steps
}

/// The trap made last: its cell, the step off it and back.
fn trap() -> Result<Made, String> {
    TRAP.lock().unwrap().ok_or_else(|| "no trap made".into())
}

/// The dwarf's cell beside the trap door, and what it showed before.
fn dwarf() -> Result<((i32, i32), Glyph), String> {
    DWARF
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "no dwarf drawn".into())
}

/// From now on: the lines a check looks at come after this.
fn mark(g: &mut RenethackGame) -> Result<(), String> {
    MARK.store(g.world.log.last_seq(), Ordering::Relaxed);
    Ok(())
}

/// A map feature's glyph by its symbol, as the map shows one seen.
fn feature(cat: &nh_protocol::Catalog, sym: &str) -> Result<Glyph, String> {
    let c = cat
        .cmap
        .iter()
        .find(|c| c.sym == sym)
        .ok_or_else(|| format!("no {sym} in the catalog"))?;
    Ok(Glyph {
        glyph: Some(c.idx),
        ch: c.ch,
        color: 0,
        flags: 0,
        tile: 0,
        kind: GlyphKind::Cmap,
        mon: None,
        cmap: Some(c.idx),
        level: None,
    })
}

/// Open floor on all eight cells about the hero.
fn open_all_round(g: &RenethackGame) -> Result<bool, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    Ok(STEPS.iter().all(|((dx, dy), _)| {
        g.world.map.cell(hx + dx, hy + dy).is_some_and(|c| {
            matches!(
                nh_world::cell_terrain(c, cat),
                Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
            )
        })
    }))
}

/// A step onto open floor, south or east first (away from the stairs the
/// game began on, towards the camera).
fn roomward(g: &RenethackGame) -> Result<char, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    ['n', 'j', 'l', 'b', 'u', 'h', 'k', 'y']
        .into_iter()
        .find(|&k| {
            let Some(((dx, dy), _)) = STEPS.iter().find(|(_, c)| *c == k) else {
                return false;
            };
            g.world.map.cell(hx + dx, hy + dy).is_some_and(|c| {
                matches!(
                    nh_world::cell_terrain(c, cat),
                    Some(nh_world::Terrain::Floor | nh_world::Terrain::DarkFloor)
                ) && c.entity().is_none()
            })
        })
        .ok_or_else(|| "no open floor about the hero".to_string())
}
