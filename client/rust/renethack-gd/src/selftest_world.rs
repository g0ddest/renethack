//! The world's looks by branch (spec part 2, phase K): `branches` starts a
//! game in debug mode (its playground allows it), teleports the hero to a
//! level of the Gnomish Mines, of Sokoban and of Gehennom by the debug
//! level teleport's menu, and checks that each is drawn in its own
//! materials, with a picture of each.

use nh_world::{Branch, KeyInput, Prompt};

use super::{
    DialogEvent, Step, camera_settled, ctrl_key, entry_key, is_menu, map_view, quit, start,
};
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
    steps.extend(quit());
    steps
}
