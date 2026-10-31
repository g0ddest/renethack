//! `helm-hair`: a Samurai's buns go under a helmet. With one worn none of
//! his hair shows; with it off, all of it again. With `--screenshots`, the
//! hero helmed.

use godot::classes::MeshInstance3D;
use nh_protocol::{InvItem, Inventory, Slot};

use super::{Step, fail_on_error_screen, map_view, quit, smoke_choice, start_as};
use crate::game::RenethackGame;
use crate::ui_events::CharacterChoice;

/// The helmet put on: its letter in the pack.
const HELMET: char = 'z';

pub(super) fn helm_hair() -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: "samurai".into(),
        gender: "male".into(),
        align: "lawful".into(),
        ..smoke_choice()
    });
    steps.extend([
        Step::Wait("the hero drawn, his hair showing", |g| {
            fail_on_error_screen(g)?;
            Ok(hair_shown(g)? == Some(true))
        }),
        Step::Call("a helmet on", |g| wear_helmet(g, true)),
        Step::Wait("none of his hair under it", |g| {
            Ok(hair_shown(g)? == Some(false))
        }),
        Step::Shot("helm-hair"),
        Step::Call("the helmet off", |g| wear_helmet(g, false)),
        Step::Wait("his buns again", |g| Ok(hair_shown(g)? == Some(true))),
    ]);
    steps.extend(quit());
    steps
}

/// The pack as it is, with a visored helmet worn or without it.
fn wear_helmet(g: &mut RenethackGame, on: bool) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let mut items: Vec<InvItem> = g
        .world
        .inventory
        .items()
        .iter()
        .filter(|i| i.letter != HELMET)
        .cloned()
        .collect();
    if on {
        let tile = cat
            .object_tiles
            .iter()
            .find(|t| t.class == "[" && t.appearance == "visored helmet")
            .ok_or("no visored helmet")?
            .tile;
        items.push(InvItem {
            letter: HELMET,
            class: '[',
            tile,
            quan: 1,
            slots: vec![Slot::Helmet],
            lit: false,
            text: String::new(),
        });
    }
    let twoweap = g.world.inventory.twoweap();
    g.world.inventory.replace(&Inventory { items, twoweap });
    Ok(())
}

/// Whether the hero's hair shows (None: no hair drawn yet).
fn hair_shown(g: &RenethackGame) -> Result<Option<bool>, String> {
    let Some(model) = map_view(g)?.hero_model().filter(|m| !m.is_pending()) else {
        return Ok(None);
    };
    let hair: Vec<bool> = model
        .node
        .find_children_ex("Base*Hair")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<MeshInstance3D>().ok())
        .map(|mi| mi.is_visible())
        .collect();
    Ok(match hair.as_slice() {
        [] => None,
        _ if hair.iter().all(|&v| v) => Some(true),
        _ if hair.iter().all(|&v| !v) => Some(false),
        _ => None,
    })
}
