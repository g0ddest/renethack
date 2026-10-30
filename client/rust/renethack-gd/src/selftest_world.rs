//! The world's looks by branch (spec part 2, phase K): `branches` starts a
//! game in debug mode (its playground allows it), teleports the hero to a
//! level of the Gnomish Mines, of Sokoban, of Gehennom and of Vlad's Tower
//! by the debug level teleport's menu (Fort Ludios is out of its reach
//! until its portal is made), and checks that each is drawn in its own
//! materials, with a picture of each. `title` checks the scene behind the
//! title menu; `bestiary` shoots the creatures a game meets most, one by
//! one, then those told apart by colour side by side, then a game's start.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use nh_world::{Branch, KeyInput, Prompt};

use super::{
    CharacterChoice, DialogEvent, MIDWAY, Step, UiEvent, camera_settled, ctrl_key, entry_key,
    hold_at, is_menu, map_view, quit, screen, smoke_choice, start, start_as,
};
use godot::classes::{Node, Node3D};
use godot::prelude::*;

use crate::game::RenethackGame;

type KeyCheck = fn(&RenethackGame) -> Result<KeyInput, String>;
type Check = fn(&RenethackGame) -> Result<bool, String>;
type Act = fn(&mut RenethackGame) -> Result<(), String>;

/// The level is drawn in `branch`, the camera settled on it.
fn drawn_in(g: &RenethackGame, branch: Branch) -> Result<bool, String> {
    let map = map_view(g)?;
    Ok(g.world.branch() == branch && map.branch() == branch && camera_settled(g)?)
}

/// The walls drawn are of the branch's own material, not the main
/// dungeon's.
fn own_walls(g: &RenethackGame, branch: Branch) -> Result<(), String> {
    let wall = map_view(g)?.wall_material();
    let expected = crate::branch_look::look_of(branch).material("masonry");
    if wall != expected || wall == "masonry" {
        return Err(format!("{branch:?}: walls of {wall:?}, not {expected:?}"));
    }
    godot_print!("selftest: branches: {branch:?} drawn in {wall}");
    Ok(())
}

/// A level teleport by the debug menu's entry `pick` (a special level's
/// name: only this test reads them), to a level drawn as `drawn` checks.
fn teleport(pick: KeyCheck, drawn: Check, check: Act, shot: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the level", pick),
        Step::Wait("the level drawn in its branch", drawn),
        Step::Shot(shot),
        Step::Call("the branch's own walls", check),
    ]
}

pub(super) fn branches() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode for this game",
        |g: &mut RenethackGame| {
            g.debug_mode = true;
            Ok(())
        },
    )];
    steps.extend(start());
    // the first level drawn and come up out of black
    steps.push(Step::Wait("the first level drawn", |g| {
        drawn_in(g, Branch::Main)
    }));
    steps.push(Step::Call("the main dungeon's walls", |g| {
        let wall = map_view(g)?.wall_material();
        if wall != "masonry" {
            return Err(format!("the main dungeon's walls are {wall:?}"));
        }
        Ok(())
    }));
    steps.push(Step::Shot("branch-main"));
    steps.extend(teleport(
        |g| entry_key(g, "minetn"),
        |g| drawn_in(g, Branch::Mines),
        |g| own_walls(g, Branch::Mines),
        "branch-mines",
    ));
    steps.extend(teleport(
        |g| entry_key(g, "soko"),
        |g| drawn_in(g, Branch::Sokoban),
        |g| own_walls(g, Branch::Sokoban),
        "branch-sokoban",
    ));
    steps.extend(teleport(
        |g| entry_key(g, "valley"),
        |g| drawn_in(g, Branch::Gehennom),
        |g| own_walls(g, Branch::Gehennom),
        "branch-gehennom",
    ));
    steps.extend(teleport(
        |g| entry_key(g, "tower1"),
        |g| drawn_in(g, Branch::Vlad),
        |g| own_walls(g, Branch::Vlad),
        "branch-vlad",
    ));
    steps.extend(quit());
    steps
}

/// The engine waits for a command (every question before it answered).
fn at_command(g: &RenethackGame) -> Result<bool, String> {
    Ok(matches!(g.pending, Some((_, Prompt::Command))))
}

/// The level drawn, up out of black and the camera settled on it,
/// whatever its branch.
fn drawn_here(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    Ok(map.branch() == g.world.branch()
        && map.drawn_generation() == Some(g.world.map.generation())
        && map.is_settled()
        && camera_settled(g)?)
}

/// A level teleport by the debug prompt's answer `to` (a depth in the
/// branch the hero is in), the level mapped and drawn.
fn teleport_to(to: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(to.into())),
        Step::AnswerUntil('n', "a command on the new level", at_command),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", |p| matches!(p, Prompt::Command)),
        Step::Wait("the level drawn", drawn_here),
    ]
}

/// A level teleport by the debug menu's entry `pick` (a special level),
/// the level mapped and drawn, a picture of it near and far (`shot`,
/// `shot`-far).
fn visit(steps: &mut Vec<Step>, pick: KeyCheck, shot: &'static str) {
    // (a name for the far view too, for the run's length)
    let far_shot: &'static str = Box::leak(format!("{shot}-far").into_boxed_str());
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the level", pick),
        Step::AnswerUntil('n', "a command on the new level", at_command),
        // the whole level mapped (the debug mode's map), so its terrain
        // shows beyond the hero's own light
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", |p| matches!(p, Prompt::Command)),
        Step::Wait("the level drawn", drawn_here),
        Step::Shot(shot),
        // and further off
        Step::Call("the camera far off", |g| far(g, 19.0)),
        Step::Wait("the camera far off", drawn_here),
        Step::Shot(far_shot),
        Step::Call("the camera back", |g| far(g, 11.0)),
        Step::Wait("the camera back", drawn_here),
    ]);
}

/// The view on the cell of terrain `t` nearest the hero, as far as the
/// map shows it.
fn view_nearest(g: &mut RenethackGame, t: nh_world::Terrain) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    let at = (0..nh_world::ROWNO)
        .flat_map(|y| (0..nh_world::COLNO).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            g.world
                .map
                .cell(x, y)
                .and_then(|c| nh_world::cell_terrain(c, &cat))
                == Some(t)
        })
        .min_by_key(|&(x, y)| (x - hx).pow(2) + (y - hy).pow(2))
        .ok_or_else(|| format!("no {t:?} on the level"))?;
    g.world.view_center = Some(at);
    Ok(())
}

/// The view close on the north wall of a room nearest the hero: a wall
/// with walls either side of it and the room's floor south of it; a long
/// one first (a run of five, the floor south of the middle three), then
/// a lit one. In a maze (its walls a cell thick) any wall over a floor;
/// with none, the view stays on the hero.
fn view_north_wall(g: &mut RenethackGame) -> Result<(), String> {
    use nh_world::Terrain;
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    let map = &g.world.map;
    let t = |x: i32, y: i32| map.cell(x, y).and_then(|c| nh_world::cell_terrain(c, &cat));
    let floor = |x: i32, y: i32| matches!(t(x, y), Some(Terrain::Floor | Terrain::DarkFloor));
    let at = (0..nh_world::ROWNO)
        .flat_map(|y| (0..nh_world::COLNO).map(move |x| (x, y)))
        .filter(|&(x, y)| t(x, y) == Some(Terrain::Wall) && floor(x, y + 1))
        .min_by_key(|&(x, y)| {
            let room = (-1..=1).all(|dx| t(x + dx, y) == Some(Terrain::Wall)) && !floor(x, y - 1);
            let long = [-2, 2].iter().all(|dx| t(x + dx, y) == Some(Terrain::Wall))
                && (-1..=1).all(|dx| floor(x + dx, y + 1));
            let dark = t(x, y + 1) != Some(Terrain::Floor);
            (!room, !long, dark, (x - hx).pow(2) + (y - hy).pow(2))
        });
    if let Some(at) = at {
        g.world.view_center = Some(at);
    }
    far(g, 7.0)
}

/// A picture close on the north wall of the room nearest the hero
/// (`shot`-wall), then the camera back on the hero.
fn close_on_wall(steps: &mut Vec<Step>, shot: &'static str) {
    let wall_shot: &'static str = Box::leak(format!("{shot}-wall").into_boxed_str());
    steps.extend([
        Step::Call("the view on a room's north wall", view_north_wall),
        Step::Wait("the camera on the wall", drawn_here),
        Step::Shot(wall_shot),
        Step::Call("the camera back on the hero", |g| {
            g.world.view_center = None;
            far(g, 11.0)
        }),
        Step::Wait("the camera back", drawn_here),
    ]);
}

/// A room's dressing where the critics wanted it, for pictures before and
/// after (not part of `make test-client`): the first rooms and a few
/// levels down, the Big Room, Medusa's island, the Castle, the Gnomish
/// Mines' rock, Sokoban (left bare), the Valley, Orcus' town, the mazes
/// round the Wizard's tower and Vlad's Tower; each with a picture close
/// on a room's north wall.
pub(super) fn dressing() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode for this game",
        |g: &mut RenethackGame| {
            g.debug_mode = true;
            Ok(())
        },
    )];
    steps.extend(start());
    steps.push(Step::Wait("the first level drawn", |g| {
        drawn_in(g, Branch::Main)
    }));
    steps.push(Step::Shot("dressing-dlvl1"));
    close_on_wall(&mut steps, "dressing-dlvl1");
    for (depth, shot) in [
        ("2", "dressing-dlvl2"),
        ("3", "dressing-dlvl3"),
        ("4", "dressing-dlvl4"),
        ("6", "dressing-dlvl6"),
    ] {
        steps.extend(teleport_to(depth));
        steps.push(Step::Shot(shot));
        close_on_wall(&mut steps, shot);
    }
    let visits: [(KeyCheck, &'static str); 4] = [
        (|g| entry_key(g, "bigrm"), "dressing-bigroom"),
        (|g| entry_key(g, "medusa"), "dressing-medusa"),
        (|g| entry_key(g, "castle"), "dressing-castle"),
        (|g| entry_key(g, "minetn"), "dressing-minetown"),
    ];
    for (pick, shot) in visits {
        visit(&mut steps, pick, shot);
        close_on_wall(&mut steps, shot);
    }
    steps.extend(teleport_to("9"));
    steps.push(Step::Shot("dressing-mines-deep"));
    close_on_wall(&mut steps, "dressing-mines-deep");
    let visits: [(KeyCheck, &'static str); 5] = [
        (|g| entry_key(g, "soko"), "dressing-sokoban"),
        (|g| entry_key(g, "valley"), "dressing-valley"),
        (|g| entry_key(g, "orcus"), "dressing-orcus"),
        (|g| entry_key(g, "wizard1"), "dressing-wizard"),
        (|g| entry_key(g, "tower1"), "dressing-vlad"),
    ];
    for (pick, shot) in visits {
        visit(&mut steps, pick, shot);
        close_on_wall(&mut steps, shot);
    }
    steps.extend(quit());
    steps
}

/// Gehennom's looks (the critics' art #8), for pictures before and after
/// (not part of `make test-client`): the Valley as the hero arrives and
/// once the overview names it (the level mapped, its temple of Moloch
/// with it), Juiblex's swamp, Orcus' town, the Wizard's mazes and a few
/// levels deeper (each its own fire), each near, far and close on a
/// room's north wall.
pub(super) fn gehennom() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode for this game",
        |g: &mut RenethackGame| {
            g.debug_mode = true;
            Ok(())
        },
    )];
    steps.extend(start());
    steps.push(Step::Wait("the first level drawn", |g| {
        drawn_in(g, Branch::Main)
    }));
    visit(&mut steps, |g| entry_key(g, "valley"), "gehennom-valley");
    close_on_wall(&mut steps, "gehennom-valley");
    // the overview names its temple, its altar mapped: its own air
    steps.extend([
        Step::Key(KeyInput::plain(nh_world::Key::Char('#'))),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("overview".into()))),
        Step::AnswerUntil('n', "a command after the overview", at_command),
        Step::Wait("the level drawn after the overview", drawn_here),
        Step::Call("what the Valley is told", |g| {
            let lair = g.world.level.as_ref().and_then(|l| l.lair.clone());
            godot_print!("selftest: gehennom: the Valley told as {lair:?}");
            Ok(())
        }),
        Step::Shot("gehennom-valley-known"),
    ]);
    let lairs: [(KeyCheck, &'static str); 3] = [
        (|g| entry_key(g, "juiblex"), "gehennom-juiblex"),
        (|g| entry_key(g, "orcus"), "gehennom-orcus"),
        (|g| entry_key(g, "wizard1"), "gehennom-wizard"),
    ];
    for (pick, shot) in lairs {
        visit(&mut steps, pick, shot);
        close_on_wall(&mut steps, shot);
    }
    for (depth, shot) in [
        // (each its fire: blood, brimstone, embers)
        ("33", "gehennom-33"),
        ("34", "gehennom-34"),
        ("36", "gehennom-36"),
    ] {
        steps.extend(teleport_to(depth));
        steps.push(Step::Shot(shot));
        close_on_wall(&mut steps, shot);
    }
    steps.extend(quit());
    steps
}

/// The terrain's looks where the critics met them, for pictures (not part
/// of `make test-client`): the shores of Medusa's island, the ice and
/// lava of the Valkyrie's home, the Castle's moat, Juiblex's swamp, and
/// the Gnomish Mines' rock above and below Minetown.
pub(super) fn terrain() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode for this game",
        |g: &mut RenethackGame| {
            g.debug_mode = true;
            Ok(())
        },
    )];
    steps.extend(start());
    steps.push(Step::Wait("the first level drawn", |g| {
        drawn_in(g, Branch::Main)
    }));
    steps.push(Step::Shot("terrain-main"));
    let visits: [(KeyCheck, &'static str); 3] = [
        (|g| entry_key(g, "medusa"), "terrain-medusa"),
        (|g| entry_key(g, "castle"), "terrain-castle"),
        (|g| entry_key(g, "juiblex"), "terrain-juiblex"),
    ];
    for (pick, shot) in visits {
        visit(&mut steps, pick, shot);
    }
    visit(&mut steps, |g| entry_key(g, "minetn"), "terrain-minetown");
    // a filler level of the Mines below Minetown, unlit, and its end
    steps.extend(teleport_to("9"));
    steps.push(Step::Shot("terrain-mines-deep"));
    visit(&mut steps, |g| entry_key(g, "minend"), "terrain-mines-end");
    // last: the quest's home (the level teleport works no further there),
    // its greeting read first
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the quest's home", |g| entry_key(g, "-strt")),
        Step::AnswerUntil('n', "a command in the quest's home", at_command),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", |p| matches!(p, Prompt::Command)),
        Step::Wait("the level drawn", drawn_here),
        Step::Shot("terrain-home"),
        // and its lava, as far as the map shows it, nearest the hero
        Step::Call("the view on the lava", |g| {
            view_nearest(g, nh_world::Terrain::Lava)
        }),
        Step::Wait("the camera on the lava", drawn_here),
        Step::Shot("terrain-home-lava"),
    ]);
    steps.extend(quit());
    steps
}

/// What hangs shown on the hero's bone attachment `Gear_<bone>` (the held
/// models' names).
fn held_on(g: &RenethackGame, bone: &str) -> Result<Vec<String>, String> {
    let map = map_view(g)?;
    let Some(m) = map.hero_model() else {
        return Ok(Vec::new());
    };
    let node: Gd<Node> = m.node.clone().upcast();
    let Some(att) = node
        .find_child_ex(&format!("Gear_{bone}"))
        .owned(false)
        .done()
    else {
        return Ok(Vec::new());
    };
    let prefix = format!("{}_", crate::art::HELD_NODE);
    Ok(att
        .get_children()
        .iter_shared()
        .filter(|c| !c.is_queued_for_deletion())
        .filter(|c| c.clone().try_cast::<Node3D>().is_ok_and(|n| n.is_visible()))
        .filter_map(|c| {
            c.get_name()
                .to_string()
                .strip_prefix(&prefix)
                .map(str::to_string)
        })
        .collect())
}

/// The scene behind the title: drawn before the start-up veil lifts, a
/// Valkyrie (no game played yet) with a sword in hand and a shield on her
/// arm, pictures at both ends of the camera's sway; a game started puts
/// it away, and the title after the game has it again.
pub(super) fn title() -> Vec<Step> {
    use crate::title_scene::SWAY_SECS;
    let mut steps = vec![
        // the start-up veil waits for it (but for its cap: a busy machine)
        Step::Wait("the title up over its scene", |g| {
            Ok(screen(g) == Some("title") && map_view(g)?.title_drawn())
        }),
        Step::Wait("the hero in her gear", |g| {
            let hand = held_on(g, "hand_r")?;
            let arm = held_on(g, "lowerarm_l")?;
            godot_print!("selftest: title: the hero holds {hand:?}, on her arm {arm:?}");
            Ok(!hand.is_empty() && !arm.is_empty())
        }),
        Step::Shot("title"),
        Step::Call("every pipeline variant made", |g| {
            // the rehearsal makes one of each variant a game draws: a
            // line of the list it cannot make is a pipeline a game builds
            // as it first draws it
            let (made, unmade) = map_view(g)?.variants_made();
            if made == 0 || unmade > 0 {
                return Err(format!(
                    "{made} pipeline variants made, {unmade} could not be"
                ));
            }
            // and each standard material for the shader the census saw
            let wrong = crate::variants::mismatches();
            if !wrong.is_empty() {
                return Err(format!("variants made for another shader: {wrong:?}"));
            }
            godot_print!("selftest: title: {made} pipeline variants made");
            Ok(())
        }),
        Step::Call("the camera at one end of its sway", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_title_clock(f64::from(SWAY_SECS) * 0.25);
            Ok(())
        }),
        Step::Shot("title-sway-left"),
        Step::Call("the camera at the other end", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_title_clock(f64::from(SWAY_SECS) * 0.75);
            Ok(())
        }),
        Step::Shot("title-sway-right"),
    ];
    steps.extend(start());
    steps.push(Step::Call("the title scene put away", |g| {
        if map_view(g)?.title_shown() {
            return Err("the title scene is still drawn in the game".into());
        }
        Ok(())
    }));
    steps.extend(quit());
    steps.extend([
        Step::Push(UiEvent::BackToTitle),
        Step::Wait("the title over its scene again", |g| {
            Ok(screen(g) == Some("title") && map_view(g)?.title_drawn())
        }),
    ]);
    steps
}

/// The creatures drawn in code that a game meets most (a soak of 24 seeds
/// counted them), and the rest of their kinds, each alone and close up:
/// `bestiary-<name>.png`, for the contact sheets. Not part of
/// `make test-client`.
const BESTIARY: &[&str] = &[
    "kitten",
    "housecat",
    "newt",
    "gecko",
    "crocodile",
    "grid bug",
    "giant ant",
    "soldier ant",
    "killer bee",
    "giant beetle",
    "lichen",
    "brown mold",
    "yellow mold",
    "red mold",
    "acid blob",
    "blue jelly",
    "gas spore",
    "floating eye",
    "bat",
    "giant bat",
    "cave spider",
    "giant spider",
    "centipede",
    "garter snake",
    "cobra",
    "baby red dragon",
    "red dragon",
    "long worm",
    "purple worm",
    "piranha",
    "shark",
    "giant eel",
    "electric eel",
    "jellyfish",
    "kraken",
    "raven",
    "rock piercer",
    "iron piercer",
    "glass piercer",
    "trapper",
    "lurker above",
    "yellow light",
    "black light",
];

/// RENETHACK_BESTIARY=gnome,gnome (F);dwarf,dwarf leader: these creatures
/// instead, each alone, then midway through a step (`step-<name>`), each
/// in a corridor with the hero in the next cell (`corridor-<name>`), and
/// then together in rows (`;` between rows; the picture `rows`), for
/// sheets of a kind before and after a change. A name ending in " (F)" is
/// the female.
fn asked() -> Option<&'static [Vec<&'static str>]> {
    static ASKED: OnceLock<Option<Vec<Vec<&'static str>>>> = OnceLock::new();
    ASKED
        .get_or_init(|| {
            let rows = std::env::var("RENETHACK_BESTIARY").ok()?;
            let rows: &'static str = rows.leak();
            Some(
                rows.split(';')
                    .map(|r| r.split(',').map(str::trim).collect())
                    .collect(),
            )
        })
        .as_deref()
}

/// The creatures shot one by one.
fn creatures() -> Vec<&'static str> {
    match asked() {
        Some(rows) => rows.iter().flatten().copied().collect(),
        None => BESTIARY.to_vec(),
    }
}

/// The creatures a player tells apart by their colour, in rows (the
/// spacing, the names): the fungi, the small and the many-legged, the
/// worms and a blob.
const COLOUR_ROWS: &[(i32, &[&str])] = &[
    (
        1,
        &[
            "lichen",
            "brown mold",
            "yellow mold",
            "green mold",
            "red mold",
            "shrieker",
            "violet fungus",
        ],
    ),
    (
        1,
        &[
            "newt",
            "gecko",
            "giant ant",
            "soldier ant",
            "fire ant",
            "grid bug",
            "floating eye",
        ],
    ),
    (3, &["long worm", "purple worm", "acid blob"]),
];

/// The creatures of the water and of the deep, shot in their own rows (on
/// the floor, where a swimmer stands out of the water: its whole body's
/// colour shows).
const DEEP_ROWS: &[(i32, &[&str])] = &[
    (
        2,
        &[
            "piranha",
            "shark",
            "giant eel",
            "electric eel",
            "jellyfish",
            "kraken",
        ],
    ),
    (
        2,
        &[
            "rock piercer",
            "iron piercer",
            "glass piercer",
            "trapper",
            "lurker above",
        ],
    ),
    (2, &["raven", "yellow light", "black light"]),
];

pub(super) fn bestiary() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    for name in creatures() {
        let file = name.replace(" (F)", "-f").replace(' ', "-");
        let shot: &'static str = Box::leak(format!("bestiary-{file}").into_boxed_str());
        steps.extend([
            Step::Call("lay out the next creature", |g| {
                let i = NEXT_CREATURE.fetch_add(1, Ordering::Relaxed);
                one(g, i)
            }),
            Step::Wait("the creature drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on it", camera_settled),
            Step::Shot(shot),
        ]);
        if asked().is_some() {
            // and a step west, stopped midway: its legs as it walks
            let shot: &'static str = Box::leak(format!("step-{file}").into_boxed_str());
            steps.extend([
                Step::Call("stop motions midway", |g| hold_at(g, Some(MIDWAY))),
                Step::Call("the creature steps west", |g| {
                    let cat = g.catalog.clone().ok_or("no catalog")?;
                    let i = NEXT_CREATURE.load(Ordering::Relaxed) - 1;
                    let name = creatures().get(i).copied().ok_or("no such creature")?;
                    crate::gallery::step_one(&mut g.world, &cat, name)
                }),
                Step::Wait("the step midway", |g| {
                    let map = map_view(g)?;
                    Ok(map.steps_under_way().1 == 1 && map.motions_held())
                }),
                Step::Shot(shot),
                Step::Call("let it arrive", |g| hold_at(g, None)),
                Step::Wait("the creature arrived", |g| {
                    Ok(map_view(g)?.steps_under_way() == (false, 0))
                }),
            ]);
        }
    }
    if asked().is_some() {
        for name in creatures() {
            let file = name.replace(" (F)", "-f").replace(' ', "-");
            let shot: &'static str = Box::leak(format!("corridor-{file}").into_boxed_str());
            steps.extend([
                Step::Call("the next creature in a corridor", |g| {
                    let cat = g.catalog.clone().ok_or("no catalog")?;
                    let i = NEXT_CORRIDOR.fetch_add(1, Ordering::Relaxed);
                    let name = creatures().get(i).copied().ok_or("no more creatures")?;
                    crate::gallery::lay_out_corridor(&mut g.world, &cat, name)?;
                    let ui = g.ui.as_mut().ok_or("no UI")?;
                    ui.map.set_showcase(true);
                    ui.map.set_distance(5.0, 0.3);
                    Ok(())
                }),
                Step::Wait("the corridor drawn", |g| {
                    let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                    Ok(drawn == Some(g.world.map.generation()))
                }),
                Step::Wait("the camera on it", camera_settled),
                Step::Shot(shot),
            ]);
        }
        steps.extend([
            Step::Call("lay out the rows asked for", |g| {
                let cat = g.catalog.clone().ok_or("no catalog")?;
                let rows: Vec<(i32, &[&str])> = asked()
                    .unwrap_or_default()
                    .iter()
                    .map(|r| (2, r.as_slice()))
                    .collect();
                crate::gallery::lay_out_rows(&mut g.world, &cat, &rows)?;
                let ui = g.ui.as_mut().ok_or("no UI")?;
                ui.map.set_showcase(true);
                ui.map.set_distance(8.0, 0.2);
                Ok(())
            }),
            Step::Wait("the rows drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on them", camera_settled),
            Step::Shot("rows"),
        ]);
        steps.extend(quit());
        return steps;
    }
    steps.extend([
        Step::Call("lay out the colour rows", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            crate::gallery::lay_out_rows(&mut g.world, &cat, COLOUR_ROWS)?;
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_showcase(true);
            ui.map.set_distance(9.5, 0.2);
            Ok(())
        }),
        Step::Wait("the rows drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on them", camera_settled),
        Step::Shot("colours"),
        Step::Call("lay out the deep's rows", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            crate::gallery::lay_out_rows(&mut g.world, &cat, DEEP_ROWS)?;
            Ok(())
        }),
        Step::Wait("the deep's rows drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on the deep's rows", camera_settled),
        Step::Shot("colours-deep"),
    ]);
    steps.extend(quit());
    // and in a game: a wizard's kitten at the start, as far as a game
    // begins and as close as a player zooms
    steps.extend([
        Step::Call("seed 1", |g| {
            g.seed = Some(1);
            Ok(())
        }),
        Step::Push(UiEvent::BackToTitle),
    ]);
    steps.extend(start_as(CharacterChoice {
        role: "wizard".into(),
        gender: "male".into(),
        ..smoke_choice()
    }));
    let shots: [(&'static str, Act); 2] = [
        ("ingame-wizard", |g| far(g, 11.0)),
        ("ingame-wizard-close", |g| far(g, 7.0)),
    ];
    for (shot, distance) in shots {
        steps.extend([
            Step::Wait("the hero on the map", |g| {
                Ok(map_view(g)?.hero_model().is_some())
            }),
            Step::Call("the camera's distance", distance),
            Step::Wait("the camera on the hero", camera_settled),
            Step::Shot(shot),
        ]);
    }
    steps.extend(quit());
    steps
}

/// The camera `distance` from the hero, aimed as in a game.
fn far(g: &mut RenethackGame, distance: f32) -> Result<(), String> {
    g.ui.as_mut()
        .ok_or("no UI")?
        .map
        .set_distance(distance, 0.0);
    Ok(())
}

/// The next creature of `BESTIARY` to lay out (steps are plain functions).
static NEXT_CREATURE: AtomicUsize = AtomicUsize::new(0);
/// The next creature asked for to put in a corridor.
static NEXT_CORRIDOR: AtomicUsize = AtomicUsize::new(0);

/// Lay out creature `i` of `BESTIARY` alone, the camera close.
fn one(g: &mut RenethackGame, i: usize) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let name = creatures().get(i).copied().ok_or("no more creatures")?;
    crate::gallery::lay_out_one(&mut g.world, &cat, name)?;
    // as close as the creature is small
    let kind = crate::gallery::named(name).0;
    let size = cat
        .monsters
        .iter()
        .find(|m| m.name == kind)
        .map(|m| m.size.as_str());
    let (distance, lift) = match size {
        Some("tiny") => (1.3, 0.1),
        Some("small") => (2.0, 0.15),
        Some("medium") => (3.2, 0.25),
        Some("large") => (4.4, 0.3),
        _ => (6.0, 0.4),
    };
    // those asked for may wear a tall hat
    let lift = lift + if asked().is_some() { 0.12 } else { 0.0 };
    let ui = g.ui.as_mut().ok_or("no UI")?;
    ui.map.set_showcase(true);
    ui.map.set_distance(distance, lift);
    Ok(())
}
