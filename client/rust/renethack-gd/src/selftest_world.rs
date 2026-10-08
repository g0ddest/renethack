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
