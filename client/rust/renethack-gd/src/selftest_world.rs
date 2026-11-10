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
    if let Some(at) = north_wall_near_hero(g)? {
        g.world.view_center = Some(at);
    }
    far(g, 7.0)
}

/// The north wall of a room nearest the hero, as `view_north_wall` takes
/// it.
fn north_wall_near_hero(g: &RenethackGame) -> Result<Option<(i32, i32)>, String> {
    use nh_world::Terrain;
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    let map = &g.world.map;
    let t = |x: i32, y: i32| map.cell(x, y).and_then(|c| nh_world::cell_terrain(c, &cat));
    let floor = |x: i32, y: i32| matches!(t(x, y), Some(Terrain::Floor | Terrain::DarkFloor));
    Ok((0..nh_world::ROWNO)
        .flat_map(|y| (0..nh_world::COLNO).map(move |x| (x, y)))
        .filter(|&(x, y)| t(x, y) == Some(Terrain::Wall) && floor(x, y + 1))
        .min_by_key(|&(x, y)| {
            let room = (-1..=1).all(|dx| t(x + dx, y) == Some(Terrain::Wall)) && !floor(x, y - 1);
            let long = [-2, 2].iter().all(|dx| t(x + dx, y) == Some(Terrain::Wall))
                && (-1..=1).all(|dx| floor(x + dx, y + 1));
            let dark = t(x, y + 1) != Some(Terrain::Floor);
            (!room, !long, dark, (x - hx).pow(2) + (y - hy).pow(2))
        }))
}

/// Where impl-world's luma script measures the next picture, printed as
/// points on the screen: the hero's feet, the face of the north wall
/// nearest them turned to the camera (as `north_wall_near_hero`), and a
/// bare floor nearest the hero.
fn luma_marks(g: &mut RenethackGame) -> Result<(), String> {
    use nh_world::Terrain;
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    let wall = north_wall_near_hero(g)?;
    let map = &g.world.map;
    let bare_at = |x: i32, y: i32| {
        map.cell(x, y).is_some_and(|c| {
            c.entity().is_none()
                && matches!(
                    nh_world::cell_terrain(c, &cat),
                    Some(Terrain::Floor | Terrain::DarkFloor)
                )
        })
    };
    // (the nearest: in a maze the floor out of sight is none drawn)
    let mut near: Vec<(i32, i32)> = (-3..=3)
        .flat_map(|dy| (-3..=3).map(move |dx| (hx + dx, hy + dy)))
        .filter(|&at| at != (hx, hy))
        .collect();
    near.sort_by_key(|&(x, y)| (x - hx).abs().max((y - hy).abs()));
    let bare = near.into_iter().find(|&(x, y)| bare_at(x, y));
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let mark = |what: &str, at: Option<Vector3>| {
        if let Some(p) = at.and_then(|at| ui.map.on_screen(at)) {
            godot_print!("selftest: luma: {what} {:.0} {:.0}", p.x, p.y);
        }
    };
    mark("hero", ui.map.hero_stands().map(|(feet, _)| feet));
    mark(
        "wall",
        wall.map(|(x, y)| Vector3::new(x as f32, 1.0, y as f32 + 0.52)),
    );
    mark(
        "floor",
        bare.map(|(x, y)| Vector3::new(x as f32, 0.0, y as f32)),
    );
    // the near field's fall-off (the art director's Gehennom targets):
    // each bare floor one or two cells from the hero (their pool), and
    // three off
    for (what, from, to) in [("near", 1i32, 2i32), ("off3", 3, 3)] {
        for dy in -to..=to {
            for dx in -to..=to {
                let d = dx.abs().max(dy.abs());
                // (not hidden by a wall on the camera's side of it, nor
                // behind the hero)
                let (x, y) = (hx + dx, hy + dy);
                let hidden = (dx, dy) == (0, -1)
                    || map.cell(x, y + 1).is_none_or(|c| {
                        matches!(
                            nh_world::cell_terrain(c, &cat),
                            None | Some(Terrain::Wall | Terrain::Stone)
                        )
                    });
                if d >= from && !hidden && bare_at(x, y) {
                    let at = Vector3::new((hx + dx) as f32, 0.0, (hy + dy) as f32);
                    mark(what, Some(at));
                }
            }
        }
    }
    Ok(())
}

/// The HUD's portrait has rendered the hero as they now are, and draws
/// them (the render came out empty on a big level: the map put the hero's
/// model on the layer the portrait sees only once the level was drawn).
/// Its pixels need a window.
fn portrait_drawn(g: &RenethackGame) -> Result<bool, String> {
    let hud = &g.ui.as_ref().ok_or("no UI")?.hud;
    if !hud.portrait_settled() {
        return Ok(false);
    }
    if godot::classes::DisplayServer::singleton().get_name() == "headless" {
        return Ok(true);
    }
    match hud.portrait_drawn_box() {
        Some(_) => Ok(true),
        None => Err("the portrait renders nothing of the hero".into()),
    }
}

/// The portrait drawing the hero, a picture close on the north wall of
/// the room nearest the hero (`shot`-wall), then the camera back on the
/// hero.
fn close_on_wall(steps: &mut Vec<Step>, shot: &'static str) {
    let wall_shot: &'static str = Box::leak(format!("{shot}-wall").into_boxed_str());
    steps.extend([
        Step::Wait("the portrait drawing the hero", portrait_drawn),
        Step::Call("the view on a room's north wall", view_north_wall),
        Step::Wait("the camera on the wall", drawn_here),
        Step::Call("where to measure", luma_marks),
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

/// A level teleport for the pictures of a level's light (`gehennom`): to
/// the special level the debug menu's entry `pick` names, or without one
/// to `depth`. A picture as the hero arrives, before the level is mapped
/// (`shot`-arrival: only what they see of it), then mapped at the game's
/// camera (`shot`), close on a wall (`shot`-wall) and in the overview
/// (`shot`-overview). With `wish`, the Amulet's wish on arriving in the
/// endgame is wished away ("nothing").
fn light_visit(
    steps: &mut Vec<Step>,
    pick: Option<KeyCheck>,
    depth: &'static str,
    shot: &'static str,
    wish: bool,
) {
    light_visit_at(steps, pick, depth, shot, wish, None);
}

/// Where the hero was before a teleport beside a feature (`by`).
static BY_FROM: std::sync::Mutex<Option<(i32, i32)>> = std::sync::Mutex::new(None);
/// The features the hero is teleported beside, in turn (queued as a
/// tour's steps are laid out, taken as each teleport is made).
static BY: std::sync::Mutex<std::collections::VecDeque<nh_world::Terrain>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

/// The bare floor beside the level's first feature of the kind next in
/// `BY` (west of it first): the hero's place for a picture in a temple
/// (an altar) or a town's street (a fountain).
fn beside_feature(g: &RenethackGame) -> Result<(i32, i32), String> {
    use nh_world::Terrain;
    let feature = BY
        .lock()
        .map_err(|e| e.to_string())?
        .pop_front()
        .ok_or("no feature asked")?;
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let map = &g.world.map;
    let t = |x: i32, y: i32| map.cell(x, y).and_then(|c| nh_world::cell_terrain(c, &cat));
    let (ax, ay) = (0..nh_world::ROWNO)
        .flat_map(|y| (0..nh_world::COLNO).map(move |x| (x, y)))
        .find(|&(x, y)| t(x, y) == Some(feature))
        .ok_or_else(|| format!("no {feature:?} known"))?;
    // (a gate's drawbridge from the bank across its water)
    let water = |x: i32, y: i32| matches!(t(x, y), Some(Terrain::Pool | Terrain::Water));
    let cells = if feature == Terrain::DrawbridgeUp {
        let (dx, dy) = [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .into_iter()
            .find(|&(dx, dy)| water(ax + dx, ay + dy))
            .unwrap_or((-1, 0));
        [
            (ax + 2 * dx, ay + 2 * dy),
            (ax + 2 * dx + dy, ay + 2 * dy + dx),
            (ax + 2 * dx - dy, ay + 2 * dy - dx),
            (ax + 3 * dx, ay + 3 * dy),
            (ax - 2 * dx, ay - 2 * dy),
        ]
    } else {
        [
            (ax - 1, ay),
            (ax + 1, ay),
            (ax - 1, ay + 1),
            (ax + 1, ay + 1),
            (ax, ay + 1),
        ]
    };
    cells
        .into_iter()
        .find(|&(x, y)| {
            matches!(t(x, y), Some(Terrain::Floor | Terrain::DarkFloor))
                && map.cell(x, y).is_some_and(|c| c.entity().is_none())
        })
        .ok_or_else(|| format!("no bare floor beside the {feature:?}"))
}

/// As `light_visit`; with `by` a feature, once mapped the hero is
/// teleported beside the level's first one too (Orcus' altar, where the
/// art director measures its near field: from its maze the engine puts
/// the hero elsewhere in the town, the same for a seed; Minetown's
/// fountain, its street) for another picture at 7 m (`shot`-town), and
/// the overview's picture is taken from there.
fn light_visit_at(
    steps: &mut Vec<Step>,
    pick: Option<KeyCheck>,
    depth: &'static str,
    shot: &'static str,
    wish: bool,
    by: Option<nh_world::Terrain>,
) {
    let named =
        |suffix: &str| -> &'static str { Box::leak(format!("{shot}-{suffix}").into_boxed_str()) };
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(
            if pick.is_some() { "?" } else { depth }.into(),
        )),
    ]);
    if let Some(pick) = pick {
        steps.extend([
            Step::Request("the levels to teleport to", is_menu),
            Step::KeyFrom("the level", pick),
        ]);
    }
    if wish {
        steps.extend([
            Step::AnswerUntil('n', "the Amulet's wish", |g| {
                Ok(matches!(&g.pending,
                    Some((_, Prompt::Text { query, .. })) if query.contains("wish")))
            }),
            Step::Dialog(DialogEvent::TextSubmitted("nothing".into())),
        ]);
    }
    steps.extend([
        Step::AnswerUntil('n', "a command on the new level", at_command),
        Step::Wait("the level drawn as the hero arrives", drawn_here),
        Step::Call("where to measure", luma_marks),
        Step::Shot(named("arrival")),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", |p| matches!(p, Prompt::Command)),
        Step::Wait("the level drawn", drawn_here),
        Step::Call("what the level is told", |g| {
            let lair = g.world.level.as_ref().and_then(|l| l.lair.clone());
            let shafts = map_view(g)?.shafts_shown();
            godot_print!("selftest: light: the level told as {lair:?}, {shafts} shafts");
            Ok(())
        }),
        Step::Call("where to measure", luma_marks),
        Step::Shot(shot),
        // the near field at 7 m on the hero, where the art director
        // measures the fall-off from the hero's pool
        Step::Call("the camera close", |g| far(g, 7.0)),
        Step::Wait("the camera close", drawn_here),
        Step::Call("where to measure", luma_marks),
        Step::Shot(named("near")),
        Step::Call("the camera back", |g| far(g, 11.0)),
        Step::Wait("the camera back", drawn_here),
    ]);
    // a gate's drawbridge from the bank across its water, the camera there
    // (the Castle keeps a hero teleported from outside its walls out)
    if by == Some(nh_world::Terrain::DrawbridgeUp) {
        BY.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(nh_world::Terrain::DrawbridgeUp);
        steps.extend([
            Step::Call("the view on the gate", |g| {
                let at = beside_feature(g)?;
                godot_print!("selftest: light: the view on {at:?} across the gate's water");
                g.world.view_center = Some(at);
                far(g, 7.0)
            }),
            Step::Wait("the camera on the gate", drawn_here),
            Step::Shot(named("town")),
            Step::Call("the camera back on the hero", |g| {
                g.world.view_center = None;
                far(g, 11.0)
            }),
            Step::Wait("the camera back", drawn_here),
        ]);
    } else if let Some(feature) = by {
        BY.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(feature);
        steps.extend([
            Step::Key(ctrl_key('t')),
            Step::Request("getpos's cursor", |p| matches!(p, Prompt::Command)),
            Step::Wait("where to teleport", |g| {
                Ok(matches!(g.pending, Some((_, Prompt::Command))) && g.world.getpos)
            }),
            Step::Call("a click beside the feature", |g| {
                let (x, y) = beside_feature(g)?;
                *BY_FROM.lock().map_err(|e| e.to_string())? = g.world.map.hero();
                godot_print!("selftest: light: to ({x},{y}) beside the feature");
                g.push_ui(UiEvent::MapClick { x, y, button: 1 });
                Ok(())
            }),
            // (where something unseen stands there, the engine puts the
            // hero somewhere else, the same for a seed)
            Step::Wait("the hero teleported", |g| {
                let from = *BY_FROM.lock().map_err(|e| e.to_string())?;
                let moved = at_command(g)? && !g.world.getpos && g.world.map.hero() != from;
                if moved {
                    godot_print!("selftest: light: the hero at {:?}", g.world.map.hero());
                }
                Ok(moved)
            }),
            Step::Call("the camera close", |g| far(g, 7.0)),
            Step::Wait("the camera close in the town", drawn_here),
            Step::Call("where to measure", luma_marks),
            Step::Shot(named("town")),
            Step::Call("the camera back", |g| far(g, 11.0)),
            Step::Wait("the camera back", drawn_here),
        ]);
    }
    close_on_wall(steps, shot);
    steps.extend([
        Step::Call("the overview", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.toggle_overview();
            Ok(())
        }),
        Step::Wait("the overview framed", |g| Ok(map_view(g)?.overview_shown())),
        Step::Call("where to measure", luma_marks),
        Step::Shot(named("overview")),
        Step::Call("the camera back on the hero", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.toggle_overview();
            Ok(())
        }),
        Step::Wait("the camera back", drawn_here),
    ]);
}

/// The view from far out (the critics' third pass, new 9: at 22 m a lit
/// room had lost a third of its light to the haze thinning), for pictures
/// before and after (not part of `make test-client`): the first level's
/// room and Minetown, each at the game's distance and at 22 m, measured
/// where `luma_marks` prints.
pub(super) fn far_view() -> Vec<Step> {
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
    far_pair(&mut steps, "far-dlvl1");
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the level", |g| entry_key(g, "minetn")),
        Step::AnswerUntil('n', "a command on the new level", at_command),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", |p| matches!(p, Prompt::Command)),
        Step::Wait("the level drawn", drawn_here),
    ]);
    far_pair(&mut steps, "far-minetown");
    steps.extend(quit());
    steps
}

/// Pictures at the game's distance (`shot`-11) and at 22 m (`shot`-22),
/// then the camera back.
fn far_pair(steps: &mut Vec<Step>, shot: &'static str) {
    let named =
        |suffix: &str| -> &'static str { Box::leak(format!("{shot}-{suffix}").into_boxed_str()) };
    steps.extend([
        Step::Call("the game's distance", |g| far(g, 11.0)),
        Step::Wait("the camera there", drawn_here),
        Step::Call("where to measure", luma_marks),
        Step::Shot(named("11")),
        Step::Call("the camera far out", |g| far(g, 22.0)),
        Step::Wait("the camera far out", drawn_here),
        Step::Call("where to measure", luma_marks),
        Step::Shot(named("22")),
        Step::Call("the camera back", |g| far(g, 11.0)),
        Step::Wait("the camera back", drawn_here),
    ]);
}

/// The overview where the critics' third pass found it grey (new 2), for
/// pictures before and after (not part of `make test-client`): a plain
/// level, the Oracle, Medusa's sea, the Castle's moat, and in Gehennom the
/// Valley and a maze, each as the hero arrives, mapped, close on a wall
/// and in the overview.
pub(super) fn overview() -> Vec<Step> {
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
    light_visit(&mut steps, None, "3", "overview-dlvl3", false);
    let visits: [(KeyCheck, &'static str); 4] = [
        (|g| entry_key(g, "oracle"), "overview-oracle"),
        (|g| entry_key(g, "medusa"), "overview-medusa"),
        (|g| entry_key(g, "castle"), "overview-castle"),
        (|g| entry_key(g, "valley"), "overview-valley"),
    ];
    for (pick, shot) in visits {
        light_visit(&mut steps, Some(pick), "", shot, false);
    }
    light_visit(&mut steps, None, "34", "overview-34", false);
    steps.extend(quit());
    steps
}

/// The special levels that should each look their own (the art loop's
/// first critique, #7), for pictures before and after (not part of `make
/// test-client`): the Oracle (close beside a fountain of its centre), the
/// Big Room, the Castle (close beside its drawbridge) and the Plane of
/// Earth, each as the hero arrives, mapped, close on a wall and in the
/// overview. `RENETHACK_PLACES` (a comma list: "oracle,castle") picks some.
pub(super) fn places() -> Vec<Step> {
    use nh_world::Terrain;
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
    let wanted = |name: &str| {
        std::env::var("RENETHACK_PLACES")
            .map(|only| only.split(',').any(|w| w.trim() == name))
            .unwrap_or(true)
    };
    let visits: [(KeyCheck, &'static str, Option<Terrain>, bool); 4] = [
        (
            |g| entry_key(g, "oracle"),
            "places-oracle",
            Some(Terrain::Fountain),
            false,
        ),
        (|g| entry_key(g, "bigrm"), "places-bigroom", None, false),
        (
            |g| entry_key(g, "castle"),
            "places-castle",
            Some(Terrain::DrawbridgeUp),
            false,
        ),
        (|g| entry_key(g, "earth"), "places-earth", None, true),
    ];
    for (pick, shot, by, wish) in visits {
        if wanted(shot.trim_start_matches("places-")) {
            light_visit_at(&mut steps, Some(pick), "", shot, wish, by);
        }
    }
    steps.extend(quit());
    steps
}

/// Dark creatures on a dark floor in the game's own light (the art loop's
/// first critique, #6: a black dragon, a black pudding and a shade lost
/// on dark stone), as a hero with infravision sees them there, and
/// people beside them (skin's own rim with the creatures'), for pictures
/// before and after (not part of `make test-client`): at the game's
/// distance (`dark-creatures`) and close (`-close`).
pub(super) fn dark_creatures() -> Vec<Step> {
    const ROWS: &[(i32, &[&str])] = &[
        (3, &["black dragon", "black pudding", "shade"]),
        (3, &["black naga", "black light", "raven"]),
        (3, &["watchman", "valkyrie", "black unicorn"]),
    ];
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend([
        Step::Call("lay out the rows on a dark floor", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            crate::gallery::lay_out_rows_on(&mut g.world, &cat, ROWS, "S_darkroom")?;
            Ok(())
        }),
        Step::Wait("the rows drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Call("the game's distance", |g| far(g, 11.0)),
        Step::Wait("the camera on them", drawn_here),
        Step::Shot("dark-creatures"),
        Step::Call("closer", |g| far(g, 7.0)),
        Step::Wait("the camera closer", drawn_here),
        Step::Shot("dark-creatures-close"),
        Step::Call("back", |g| far(g, 11.0)),
    ]);
    steps.extend(quit());
    steps
}

/// Lava's shores and the Plane of Fire (the critics' second pass, #9), for
/// pictures before and after (not part of `make test-client`): Orcus'
/// town and the Wizard's tower (their lava), and the Plane of Fire, each
/// as the hero arrives, mapped, close on a wall and in the overview.
pub(super) fn lava() -> Vec<Step> {
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
    light_visit(
        &mut steps,
        Some(|g| entry_key(g, "orcus")),
        "",
        "lava-orcus",
        false,
    );
    light_visit(
        &mut steps,
        Some(|g| entry_key(g, "wizard1")),
        "",
        "lava-wizard",
        false,
    );
    light_visit(
        &mut steps,
        Some(|g| entry_key(g, "fire")),
        "",
        "lava-fire",
        true,
    );
    steps.extend(quit());
    steps
}

/// The Gnomish Mines' rock and Minetown's buildings (the critics' second
/// pass, #5), for pictures before and after (not part of `make
/// test-client`): Minetown, a level below it and Mines' End, each as the
/// hero arrives, mapped, close on a wall and in the overview.
pub(super) fn mines() -> Vec<Step> {
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
    // (its street, by a fountain: the overview's cells in view)
    light_visit_at(
        &mut steps,
        Some(|g| entry_key(g, "minetn")),
        "",
        "mines-minetown",
        false,
        Some(nh_world::Terrain::Fountain),
    );
    light_visit(&mut steps, None, "9", "mines-9", false);
    light_visit(
        &mut steps,
        Some(|g| entry_key(g, "minend")),
        "",
        "mines-end",
        false,
    );
    steps.extend(quit());
    steps
}

/// Gehennom's looks and light (the critics' art #7 and #8), for pictures
/// and their luma before and after (not part of `make test-client`): the
/// Valley, then once the overview names it (its temple of Moloch, the
/// level mapped), Orcus' town, Baalzebub's lair, Moloch's Sanctum, the
/// Wizard's tower, Juiblex's swamp and three levels a few deeper (each its
/// own fire), each as the hero arrives, mapped, close on a wall and in
/// the overview.
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
    light_visit(
        &mut steps,
        Some(|g| entry_key(g, "valley")),
        "",
        "gehennom-valley",
        false,
    );
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
    let lairs: [(KeyCheck, &'static str); 5] = [
        (|g| entry_key(g, "orcus"), "gehennom-orcus"),
        (|g| entry_key(g, "baalz"), "gehennom-baalz"),
        (|g| entry_key(g, "sanctum"), "gehennom-sanctum"),
        (|g| entry_key(g, "wizard1"), "gehennom-wizard"),
        (|g| entry_key(g, "juiblex"), "gehennom-juiblex"),
    ];
    for (pick, shot) in lairs.into_iter().filter(|(_, s)| gehennom_wanted(s)) {
        // (Orcus' temple: the art director's near field)
        let by = (shot == "gehennom-orcus").then_some(nh_world::Terrain::Altar);
        light_visit_at(&mut steps, Some(pick), "", shot, false, by);
    }
    // (each its fire: blood, brimstone, embers)
    for (depth, shot) in [
        ("33", "gehennom-33"),
        ("34", "gehennom-34"),
        ("36", "gehennom-36"),
    ]
    .into_iter()
    .filter(|(_, s)| gehennom_wanted(s))
    {
        light_visit(&mut steps, None, depth, shot, false);
    }
    steps.extend(quit());
    steps
}

/// The `gehennom` tour's levels after the Valley: all of them, or those
/// `RENETHACK_GEHENNOM` names (a comma list: "orcus,34"), for a quicker
/// run while tuning the light.
fn gehennom_wanted(shot: &str) -> bool {
    let Ok(only) = std::env::var("RENETHACK_GEHENNOM") else {
        return true;
    };
    let name = shot.trim_start_matches("gehennom-");
    only.split(',').any(|w| w.trim() == name)
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
        // and the veil held for it: no bare menu on grey after it lifted
        Step::Call("the veil lifted over the drawn scene", |g| {
            match g.veil_over_scene {
                Some(true) => Ok(()),
                other => Err(format!("the veil lifted over the scene: {other:?}")),
            }
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
