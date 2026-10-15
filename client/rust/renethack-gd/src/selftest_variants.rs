//! `variants`: what the game can draw of creatures and things, laid out a
//! batch at a time for a census of the renderer's pipelines
//! (`RENETHACK_VARIANTS`, `tools/perf.py --census`). Every creature as
//! either sex, sensed, as a corpse and as a statue, and every object's
//! look (`gallery::every_glyph`); each batch is drawn plain, then with the
//! hover's outline over every model, then with the x-ray. Then the hero
//! wears and wields every object's look, a slot each in turn (a weapon in
//! hand, on the back and in the quiver; armour as a shield, a helmet, on
//! the body, as gloves, a cloak, boots; a light lit in the other hand; an
//! amulet). Nothing is checked: the census is the result.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use nh_protocol::{InvItem, Inventory, Slot};

use super::{Step, fail_on_error_screen, map_view, quit, start};
use crate::game::RenethackGame;

/// Glyphs a batch: three rows.
const BATCH: usize = 48;
/// Batches enough for every glyph (some 2400 of them).
const BATCHES: usize = 60;

/// Packs the hero is dressed in: enough for the most looks of a class.
const DRESSES: usize = 100;

static NEXT: AtomicUsize = AtomicUsize::new(0);
static DRESS: AtomicUsize = AtomicUsize::new(0);
static FRAMES: AtomicU32 = AtomicU32::new(0);

/// The next pack: the `i`th look of each class in each slot it may go in
/// (armour by turns in each, so that every look is in every slot).
fn dress(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let i = DRESS.fetch_add(1, Ordering::Relaxed);
    let of = |class: &str| -> Vec<&nh_protocol::ObjectTile> {
        cat.object_tiles
            .iter()
            .filter(|t| t.class == class)
            .collect()
    };
    let (weapons, armour, tools, amulets) = (of(")"), of("["), of("("), of("\""));
    let nth =
        |v: &[&nh_protocol::ObjectTile], k: usize| (!v.is_empty()).then(|| v[k % v.len()].tile);
    let mut items = Vec::new();
    let mut letters = ('a'..='z').chain('A'..='Z');
    let mut put = |class: char, tile: Option<i32>, slot: Option<Slot>, lit: bool| {
        if let (Some(tile), Some(letter)) = (tile, letters.next()) {
            items.push(InvItem {
                letter,
                class,
                tile,
                quan: 1,
                slots: slot.into_iter().collect(),
                lit,
                text: String::new(),
            });
        }
    };
    put(')', nth(&weapons, i), Some(Slot::Weapon), false);
    put(')', nth(&weapons, i + 1), Some(Slot::Alternate), false);
    put(')', nth(&weapons, i + 2), Some(Slot::Quiver), false);
    let slots = [
        Slot::Shield,
        Slot::Helmet,
        Slot::Body,
        Slot::Gloves,
        Slot::Cloak,
        Slot::Boots,
        Slot::Shirt,
    ];
    for (k, slot) in slots.into_iter().enumerate() {
        put('[', nth(&armour, i + k), Some(slot), false);
    }
    put('(', nth(&tools, i), None, true);
    put('(', nth(&tools, i + 1), Some(Slot::Eyes), false);
    put('"', nth(&amulets, i), Some(Slot::Amulet), false);
    g.world.inventory.replace(&Inventory {
        items,
        twoweap: i % 2 == 1,
    });
    FRAMES.store(0, Ordering::Relaxed);
    Ok(())
}

/// Lay out the next batch (nothing past the last).
fn next_batch(g: &mut RenethackGame) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let i = NEXT.fetch_add(1, Ordering::Relaxed);
    let all = crate::gallery::every_glyph(&cat);
    if let Some(batch) = all.chunks(BATCH).nth(i) {
        crate::gallery::lay_out_batch(&mut g.world, &cat, batch)?;
    }
    Ok(())
}

/// The batch's models all built (at once past the last).
fn batch_drawn(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let map = map_view(g)?;
    Ok(map.drawn_generation() == Some(g.world.map.generation()) && map.models_pending() == 0)
}

/// Every model with an overlay (or none), for the census of a frame.
fn overlay(g: &mut RenethackGame, xray: Option<bool>) -> Result<(), String> {
    let ui = g.ui.as_mut().ok_or("no UI")?;
    ui.map.overlay_models(xray);
    FRAMES.store(0, Ordering::Relaxed);
    Ok(())
}

/// A frame gone by since the last overlay (the census walks the tree at a
/// frame's start).
fn a_frame(_: &RenethackGame) -> Result<bool, String> {
    Ok(FRAMES.fetch_add(1, Ordering::Relaxed) >= 1)
}

pub(super) fn variants() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        fail_on_error_screen(g)?;
        Ok(g.world.map.hero().is_some())
    }));
    for _ in 0..BATCHES {
        steps.extend([
            Step::Call("the next batch", next_batch),
            Step::Wait("the batch drawn", batch_drawn),
            Step::Call("the outline over all", |g| overlay(g, Some(false))),
            Step::Wait("a frame with it", a_frame),
            Step::Call("the x-ray over all", |g| overlay(g, Some(true))),
            Step::Wait("a frame with it", a_frame),
            Step::Call("no overlay", |g| overlay(g, None)),
        ]);
    }
    // the hero alone, dressed in turn
    steps.push(Step::Call("the hero alone", |g| {
        let cat = g.catalog.clone().ok_or("no catalog")?;
        crate::gallery::lay_out_batch(&mut g.world, &cat, &[])?;
        Ok(())
    }));
    steps.push(Step::Wait("the hero drawn", batch_drawn));
    for _ in 0..DRESSES {
        steps.extend([
            Step::Call("the next pack", dress),
            Step::Wait("a frame in it", a_frame),
            Step::Wait("the gear on", batch_drawn),
            Step::Call("the x-ray over the gear", |g| overlay(g, Some(true))),
            Step::Wait("a frame with it", a_frame),
            Step::Call("no overlay", |g| overlay(g, None)),
        ]);
    }
    steps.extend(quit());
    steps
}
