//! What the things worn do to the hero's outfit (`worn`), checked on the
//! scene tree of the hero's model: a Healer's stethoscope hangs round the
//! neck, the one fitted to a man on him and to a woman on her, and comes
//! off when it is put down; gloves cover the hands until they are taken
//! off, and the title's hero, dressed again in the role's kit, wears them
//! too; a Knight's and a Barbarian's ring mail is chain mail over the
//! tunic, a Samurai's splint mail is plate, and the tunic is the outfit's
//! own again with the armour off.

use godot::classes::base_material_3d::TextureParam;
use godot::classes::{BaseMaterial3D, MeshInstance3D, Node};
use godot::prelude::*;
use nh_world::{Key, KeyInput, Prompt};

use super::{
    CharacterChoice, Step, camera_settled, command, map_view, quit, smoke_choice, start_as,
};
use crate::art::{GLOVES_NODE, NECK_NODE};
use crate::game::RenethackGame;

/// What the hero's outfit shows of the things worn.
#[derive(Debug, Default, PartialEq)]
struct Looks {
    /// The meshes round the neck, by name.
    neck: Vec<String>,
    /// The texture over the tunic (an armour's, the outfit's own, or none
    /// on a tunic dyed outright), by its file.
    tunic: String,
    /// The gloves' meshes.
    gloves: usize,
}

fn looks(g: &RenethackGame) -> Result<Looks, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let mut l = Looks::default();
    for n in node
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if n.is_queued_for_deletion() {
            continue;
        }
        let Ok(mi) = n.try_cast::<MeshInstance3D>() else {
            continue;
        };
        let name = mi.get_name().to_string();
        if name.starts_with(NECK_NODE) {
            let mesh = mi.get_mesh().map(|m| m.get_name().to_string());
            l.neck.push(mesh.unwrap_or_default());
        } else if name.starts_with(GLOVES_NODE) {
            l.gloves += 1;
        } else if name.ends_with("_Body") {
            l.tunic = mi
                .get_active_material(0)
                .and_then(|m| m.try_cast::<BaseMaterial3D>().ok())
                .and_then(|m| m.get_texture(TextureParam::ALBEDO))
                .map(|t| t.get_path().to_string())
                .and_then(|p| p.rsplit('/').next().map(str::to_string))
                .unwrap_or_default();
        }
    }
    Ok(l)
}

/// The hero shows these meshes round the neck, this armour's texture over
/// the tunic (None: no armour's) and gloves or bare hands.
fn shows(
    g: &RenethackGame,
    neck: &[&str],
    armour: Option<&str>,
    gloves: bool,
) -> Result<bool, String> {
    let l = looks(g)?;
    // RENETHACK_WORN_LOOK=1: only look (the pictures of another build)
    if std::env::var_os("RENETHACK_WORN_LOOK").is_some() {
        return Ok(true);
    }
    let tunic = match armour {
        Some(a) => l.tunic.starts_with(a),
        None => !["Chainmail", "Metal"]
            .iter()
            .any(|a| l.tunic.starts_with(a)),
    };
    // a mesh is named after its scene and itself
    let hung = l.neck.len() == neck.len() && l.neck.iter().zip(neck).all(|(a, b)| a.starts_with(b));
    Ok(hung && tunic && (l.gloves > 0) == gloves)
}

fn letter_key(g: &RenethackGame, what: &str) -> Result<KeyInput, String> {
    let items = g.world.inventory.items();
    let item = items.iter().find(|i| i.text.contains(what));
    let item = item.ok_or_else(|| format!("no {what} in the pack"))?;
    Ok(KeyInput::plain(Key::Char(item.letter)))
}

/// A new game as `role`, the camera close on the hero.
fn hero(role: &str, gender: &str, align: &str) -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: role.into(),
        gender: gender.into(),
        align: align.into(),
        ..smoke_choice()
    });
    steps.extend([
        Step::Wait("the hero on the map", |g| {
            Ok(map_view(g)?.hero_model().is_some())
        }),
        Step::Call("close up, the HUD out of the picture", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(2.4, 0.6);
            ui.hud.set_visible(false);
            Ok(())
        }),
    ]);
    steps
}

/// Wait for the look, log it, take its picture.
fn shown(what: &'static str, check: super::Check, shot: &'static str) -> Vec<Step> {
    vec![
        Step::Wait(what, check),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Call("what the outfit shows", |g| {
            // what is worn, by its appearance (the looks follow that alone)
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let worn: Vec<&str> = g
                .world
                .inventory
                .items()
                .iter()
                .filter(|i| i.text.contains("worn") || i.text.contains("stethoscope"))
                .filter_map(|i| cat.object_tiles.iter().find(|t| t.tile == i.tile))
                .map(|t| t.appearance.as_str())
                .collect();
            godot_print!("selftest: worn: {:?} of {worn:?}", looks(g)?);
            Ok(())
        }),
        Step::Wait("a frame", |_| Ok(true)),
        Step::Shot(shot),
    ]
}

fn next_hero() -> Vec<Step> {
    let mut steps = vec![Step::Call("the HUD back", |g| {
        g.ui.as_mut().ok_or("no UI")?.hud.set_visible(true);
        Ok(())
    })];
    steps.extend(quit());
    steps.push(Step::Push(super::UiEvent::BackToTitle));
    steps
}

pub(super) fn worn() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 1", |g| {
        g.seed = Some(1);
        Ok(())
    })];
    // a Healer: the stethoscope fitted to a man, and his gloves
    steps.extend(hero("healer", "male", "neutral"));
    steps.extend(shown(
        "the man's stethoscope round the neck, the hands gloved",
        |g| shows(g, &["StethoscopeMan"], None, true),
        "worn-healer-m",
    ));
    // put down, it is off the neck; picked up, it is back
    steps.extend([
        super::key('d'),
        Step::Request("what to drop", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the stethoscope's letter", |g| letter_key(g, "stethoscope")),
        Step::Request("a command after dropping it", command),
    ]);
    steps.extend(shown(
        "nothing round the neck",
        |g| shows(g, &[], None, true),
        "worn-healer-m-dropped",
    ));
    steps.extend([
        super::key(','),
        Step::Request("a command after picking it up", command),
        Step::Wait("the stethoscope round the neck again", |g| {
            shows(g, &["StethoscopeMan"], None, true)
        }),
    ]);
    steps.extend(next_hero());
    // the title shows the hero just played, kept and dressed again in the
    // role's kit: the gloves made in the frame the old ones go are under
    // their own name (Godot renames a node whose name a sibling has)
    steps.push(Step::Wait(
        "the title's Healer in gloves under their name",
        |g| {
            let map = map_view(g)?;
            let shown = map.title_drawn() && map.hero_model().is_some_and(|m| !m.is_pending());
            Ok(shown && looks(g)?.gloves > 0)
        },
    ));
    // on a woman, the one fitted to her
    steps.extend(hero("healer", "female", "neutral"));
    steps.extend(shown(
        "the woman's stethoscope round the neck, the hands gloved",
        |g| shows(g, &["StethoscopeWoman"], None, true),
        "worn-healer-f",
    ));
    steps.extend(next_hero());
    // a Knight: ring mail over the tunic, gloves; the gloves off
    steps.extend(hero("knight", "female", "lawful"));
    steps.extend(shown(
        "chain mail over the tunic, the hands gloved",
        |g| shows(g, &[], Some("Chainmail004"), true),
        "worn-knight-f",
    ));
    steps.extend([
        super::key('T'),
        Step::Request("what to take off", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the gloves' letter", |g| letter_key(g, "gloves")),
        Step::Request("a command after taking them off", command),
    ]);
    steps.extend(shown(
        "the hands bare",
        |g| shows(g, &[], Some("Chainmail004"), false),
        "worn-knight-f-bare-hands",
    ));
    steps.extend(next_hero());
    // a Barbarian: ring mail on a body with bare arms; off, the tunic is
    // the outfit's own again
    steps.extend(hero("barbarian", "male", "neutral"));
    steps.extend(shown(
        "chain mail over the tunic",
        |g| shows(g, &[], Some("Chainmail004"), false),
        "worn-barbarian-m",
    ));
    steps.extend([
        super::key('T'),
        Step::Request("a command after taking the mail off", command),
    ]);
    steps.extend(shown(
        "the outfit's own tunic",
        |g| shows(g, &[], None, false),
        "worn-barbarian-m-unarmoured",
    ));
    steps.extend(next_hero());
    // a Samurai: splint mail is plate
    steps.extend(hero("samurai", "male", "lawful"));
    steps.extend(shown(
        "plate over the tunic",
        |g| shows(g, &[], Some("Metal038"), false),
        "worn-samurai-m",
    ));
    steps.extend(next_hero());
    // a Monk: gloves, and no armour
    steps.extend(hero("monk", "female", "neutral"));
    steps.extend(shown(
        "the hands gloved, the outfit's own tunic",
        |g| shows(g, &[], None, true),
        "worn-monk-f",
    ));
    steps.extend(quit());
    steps
}
