//! The world's looks by branch (spec part 2, phase K): `branches` starts a
//! game in debug mode (its playground allows it), teleports the hero to a
//! level of the Gnomish Mines, of Sokoban, of Gehennom and of Vlad's Tower
//! by the debug level teleport's menu (Fort Ludios is out of its reach
//! until its portal is made), and checks that each is drawn in its own
//! materials, with a picture of each. `title` checks the scene behind the
//! title menu.

use nh_world::{Branch, KeyInput, Prompt};

use super::{
    DialogEvent, Step, UiEvent, camera_settled, ctrl_key, entry_key, is_menu, map_view, quit,
    screen, start,
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
