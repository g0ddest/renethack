//! `feet`: a hero's boots show only while boots are worn. An Archeologist
//! starts without any: her own shoes and hose show and the outfit's boots
//! don't; with high boots on, the boots and none of her footwear; with
//! them off, her shoes again. With `--screenshots`, the heroine shod
//! (`feet-shoes`) and booted (`feet-boots`).

use godot::classes::MeshInstance3D;
use nh_protocol::{InvItem, Inventory, Slot};

use super::{Step, camera_settled, fail_on_error_screen, map_view, quit, smoke_choice, start_as};
use crate::game::RenethackGame;
use crate::ui_events::CharacterChoice;

/// The boots put on: their letter in the pack.
const BOOTS: char = 'z';

pub(super) fn feet() -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: "archeologist".into(),
        gender: "female".into(),
        ..smoke_choice()
    });
    steps.extend([
        Step::Wait("the heroine drawn in her own shoes", |g| {
            fail_on_error_screen(g)?;
            Ok(booted(g)? == Some(false))
        }),
        Step::Wait("the camera on her", camera_settled),
        Step::Shot("feet-shoes"),
        Step::Call("high boots on", |g| wear_boots(g, true)),
        Step::Wait("the boots, and none of her shoes or hose", |g| {
            Ok(booted(g)? == Some(true))
        }),
        Step::Shot("feet-boots"),
        Step::Call("the boots off", |g| wear_boots(g, false)),
        Step::Wait("her shoes again", |g| Ok(booted(g)? == Some(false))),
    ]);
    steps.extend(quit());
    steps
}

/// The pack as it is, with high boots worn or without them.
fn wear_boots(g: &mut RenethackGame, on: bool) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let mut items: Vec<InvItem> = g
        .world
        .inventory
        .items()
        .iter()
        .filter(|i| i.letter != BOOTS)
        .cloned()
        .collect();
    if on {
        let tile = cat
            .object_tiles
            .iter()
            .find(|t| t.class == "[" && t.appearance == "jackboots")
            .ok_or("no jackboots")?
            .tile;
        items.push(InvItem {
            letter: BOOTS,
            class: '[',
            tile,
            quan: 1,
            slots: vec![Slot::Boots],
            lit: false,
            text: String::new(),
        });
    }
    let twoweap = g.world.inventory.twoweap();
    g.world.inventory.replace(&Inventory { items, twoweap });
    Ok(())
}

/// Whether the hero shows the outfit's boots (true) or the role's own
/// footwear (false); None: not drawn yet, or a mix of the two.
fn booted(g: &RenethackGame) -> Result<Option<bool>, String> {
    let Some(model) = map_view(g)?.hero_model().filter(|m| !m.is_pending()) else {
        return Ok(None);
    };
    let meshes: Vec<(String, bool)> = model
        .node
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<MeshInstance3D>().ok())
        .map(|mi| (mi.get_name().to_string(), mi.is_visible()))
        .collect();
    let own = ["BaseShins", "BaseBareShins", "BaseFeet", "BaseShoe"];
    let footwear: Vec<bool> = meshes
        .iter()
        .filter(|(n, _)| own.iter().any(|p| n.starts_with(p)))
        .map(|&(_, v)| v)
        .collect();
    let boots: Vec<bool> = meshes
        .iter()
        .filter(|(n, _)| {
            !n.starts_with("Base") && (n.ends_with("_Feet") || n.ends_with("_Feet_Boots"))
        })
        .map(|&(_, v)| v)
        .collect();
    if footwear.is_empty() || boots.is_empty() {
        return Ok(None);
    }
    let all = |v: &[bool], on: bool| v.iter().all(|&s| s == on);
    Ok(if all(&boots, true) && all(&footwear, false) {
        Some(true)
    } else if all(&boots, false) && all(&footwear, true) {
        Some(false)
    } else {
        None
    })
}
