//! `threats`: every hostile marked. Six soldier ants round the hero (drawn
//! as the engine would show them) are six arrows in a fight: the larger
//! over the nearest's head, a smaller one over each other's, none on the
//! hero or on the HUD's blocks. One the engine tells peaceful has none.
//! Out of the frame, the arrow at the edge points to the nearest with how
//! many there are beside it. Pictures with `--screenshots`; at both sizes
//! (`--size=1280x800`: the Steam Deck's).

use std::sync::Mutex;

use godot::prelude::*;
use nh_world::Told;

use super::{Step, camera_settled, map_view, quit, start};
use crate::game::RenethackGame;

/// The ants' cells, the laid-out one first.
static ANTS: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());

/// Cells round the hero (laid out at 42,10) the other ants stand on.
const AROUND: [(i32, i32); 5] = [(41, 9), (43, 9), (41, 11), (43, 11), (42, 12)];

pub(super) fn threats() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("six soldier ants round the hero, a fight", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let first = crate::gallery::lay_out_one(&mut g.world, &cat, "soldier ant")?;
            let ant = g
                .world
                .map
                .cell(first.0, first.1)
                .and_then(|c| c.glyph.clone());
            let ant = ant.ok_or("no soldier ant laid out")?;
            for (x, y) in AROUND {
                g.world.map.print(x, y, &ant, None);
            }
            *ANTS.lock().unwrap() = std::iter::once(first).chain(AROUND).collect();
            let hero = g.world.map.hero().ok_or("no hero laid out")?;
            g.world.view_center = Some(hero);
            fight(g)
        }),
        Step::Wait("the view drawn", drawn),
        Step::Wait("the camera there", camera_settled),
        Step::Wait("the ants' models", ants_drawn),
        Step::Wait("an arrow for each ant", |g| marked(g, 6)),
        Step::Shot("threats-six"),
        Step::Call("each over its own ant, none on the hero or the HUD", |g| {
            over_each(g, 6)
        }),
        // one the engine tells peaceful: no arrow over it
        Step::Call("one of them peaceful", |g| {
            let at = AROUND[4];
            let ant = g.world.map.cell(at.0, at.1).and_then(|c| c.glyph.clone());
            let told = Told {
                peaceful: true,
                ..Told::default()
            };
            g.world
                .map
                .print_with(at.0, at.1, &ant.ok_or("no ant there")?, None, told);
            fight(g)
        }),
        Step::Wait("an arrow for each hostile ant", |g| marked(g, 5)),
        Step::Call("none over the peaceful one", |g| over_each(g, 5)),
        // out of the frame: the arrow at the edge, how many beside it
        Step::Call("the hero far west of them", |g| {
            // the camera keeps the hero in view: they go to a room there
            let floor = g.world.map.cell(35, 10).and_then(|c| c.glyph.clone());
            let floor = floor.ok_or("no floor laid out")?;
            let hero = g.world.map.hero().ok_or("no hero")?;
            let me = g
                .world
                .map
                .cell(hero.0, hero.1)
                .and_then(|c| c.glyph.clone());
            for y in 6..=14 {
                for x in 2..=14 {
                    g.world.map.print(x, y, &floor, None);
                }
            }
            g.world.map.print(hero.0, hero.1, &floor, None);
            g.world.map.print(8, 10, &me.ok_or("no hero glyph")?, None);
            g.world.view_center = Some((8, 10));
            fight(g)
        }),
        Step::Wait("the view drawn", drawn),
        Step::Wait("the camera there", camera_settled),
        Step::Wait("the arrow at the edge, with their count", |g| {
            let hud = &g.ui.as_ref().ok_or("no UI")?.hud;
            Ok(hud.threat_arrow().is_some() && hud.threat_count().is_some())
        }),
        Step::Shot("threats-off-frame"),
        Step::Call("five out of the frame", |g| {
            let hud = &g.ui.as_ref().ok_or("no UI")?.hud;
            let count = hud.threat_count().ok_or("no count")?;
            if count != "×5" || !hud.threat_marks().is_empty() {
                return Err(format!(
                    "the count says {count:?}, {} marks in the frame",
                    hud.threat_marks().len()
                ));
            }
            Ok(())
        }),
    ]);
    steps.extend(quit());
    steps
}

/// The tick's moment, as at a command: hostiles in view are a fight.
fn fight(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    g.driver.observe(&g.world, &cat);
    if g.driver.mode() != nh_world::Mode::Combat {
        return Err("six soldier ants round the hero are no fight".into());
    }
    Ok(())
}

fn drawn(g: &RenethackGame) -> Result<bool, String> {
    let drawn = map_view(g)?.drawn_generation();
    Ok(drawn == Some(g.world.map.generation()))
}

fn ants_drawn(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    let ants = ANTS.lock().unwrap().clone();
    Ok(ants.iter().all(|&(x, y)| map.entity_stands(x, y).is_some()))
}

/// The larger arrow and the smaller ones are `n` in all.
fn marked(g: &RenethackGame, n: usize) -> Result<bool, String> {
    let hud = &g.ui.as_ref().ok_or("no UI")?.hud;
    Ok(hud.threat_arrow().is_some() && hud.threat_marks().len() + 1 == n)
}

/// The `n` arrows each over a hostile ant's head (an arrow beside the hero
/// for an ant behind them), never on the hero's body nor on a block of the
/// HUD, one ant each.
fn over_each(g: &RenethackGame, n: usize) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let hud = &ui.hud;
    let arrows: Vec<Vector2> = hud
        .threat_arrow()
        .into_iter()
        .chain(hud.threat_marks())
        .collect();
    if arrows.len() != n {
        return Err(format!("{} arrows for {n} hostiles", arrows.len()));
    }
    let heads: Vec<Vector2> = ANTS
        .lock()
        .unwrap()
        .iter()
        .filter(|&&(x, y)| !g.world.map.cell(x, y).is_some_and(|c| c.peaceful))
        .filter_map(|&(x, y)| {
            let (at, tall) = ui.map.entity_stands(x, y)?;
            ui.map.on_screen(at + Vector3::new(0.0, tall, 0.0))
        })
        .collect();
    let (hero, _) = hud.threat_beside();
    let blocks = hud.blocks();
    let mut taken = vec![false; heads.len()];
    for p in &arrows {
        if let Some((name, _)) = blocks.iter().find(|(_, r)| r.grow(8.0).contains_point(*p)) {
            return Err(format!("an arrow at {:.0},{:.0} on the {name}", p.x, p.y));
        }
        if hero.is_some_and(|h| h.grow(8.0).contains_point(*p)) {
            return Err(format!("an arrow at {:.0},{:.0} on the hero", p.x, p.y));
        }
        // its ant: the nearest head not taken, above it or beside the hero
        let near = heads
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken[*i])
            .min_by(|(_, a), (_, b)| a.distance_to(*p).total_cmp(&b.distance_to(*p)));
        match near {
            Some((i, h)) if h.distance_to(*p) < 140.0 => taken[i] = true,
            _ => {
                return Err(format!(
                    "the arrow at {:.0},{:.0} is over no ant of {heads:?}",
                    p.x, p.y
                ));
            }
        }
    }
    godot_print!("selftest: threats: {n} arrows over {n} ants");
    Ok(())
}
