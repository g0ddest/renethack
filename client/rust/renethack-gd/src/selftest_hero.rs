//! The hero's equipment and item use (spec phases D and E), checked on the
//! scene tree of the hero's model: `equipment` (wield, swap, a shield off,
//! a lamp lit and snuffed) and `item-use` (quaff, read, zap, cast, eat,
//! apply, throw each start their clip and effect).

use godot::classes::{Node, Node3D};
use godot::prelude::*;
use nh_world::Prompt;

use super::{
    CharacterChoice, Step, UiEvent, camera_settled, command, fail_on_error_screen, idle_command,
    key, map_view, quit, smoke_choice, start_as,
};
use crate::art::{HELD_NODE, LAMP_LIGHT, USE_NODE};
use crate::game::RenethackGame;

fn choice(role: &str, gender: &str) -> CharacterChoice {
    CharacterChoice {
        role: role.into(),
        gender: gender.into(),
        ..smoke_choice()
    }
}

/// What hangs on the hero's bone attachment `Gear_<bone>`: the held
/// models' names ("spear"), and whether a lamp light burns there.
fn held_on(g: &RenethackGame, bone: &str) -> Result<(Vec<String>, bool), String> {
    let map = map_view(g)?;
    let Some(m) = map.hero_model() else {
        return Ok((Vec::new(), false));
    };
    let node: Gd<Node> = m.node.clone().upcast();
    let Some(att) = node
        .find_child_ex(&format!("Gear_{bone}"))
        .owned(false)
        .done()
    else {
        return Ok((Vec::new(), false));
    };
    let mut names = Vec::new();
    let mut lit = false;
    for c in att.get_children().iter_shared() {
        if c.is_queued_for_deletion() {
            continue;
        }
        let name = c.get_name().to_string();
        if let Some(held) = name.strip_prefix(&format!("{HELD_NODE}_")) {
            let shown = c.clone().try_cast::<Node3D>().is_ok_and(|n| n.is_visible());
            if shown {
                names.push(held.to_string());
            }
            lit |= c.find_child_ex(LAMP_LIGHT).owned(false).done().is_some();
        }
    }
    Ok((names, lit))
}

fn holds(g: &RenethackGame, bone: &str, want: &[&str]) -> Result<bool, String> {
    let (names, _) = held_on(g, bone)?;
    Ok(names == want)
}

/// Close to the hero for the pictures.
fn close_up() -> Vec<Step> {
    vec![
        Step::Call("closer", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(2.6, 0.55);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]
}

/// The hero's clip has blended in and played a moment.
fn pose_settled(g: &RenethackGame) -> Result<bool, String> {
    let p = map_view(g)?.hero_model().and_then(|m| m.player()).cloned();
    Ok(p.is_none_or(|p| !p.is_playing() || p.get_current_animation_position() > 0.6))
}

/// Wait until the hero shows `what`, then take a picture.
fn shown(
    what: &'static str,
    check: fn(&RenethackGame) -> Result<bool, String>,
    shot: &'static str,
) -> [Step; 4] {
    [
        Step::Wait(what, check),
        Step::Wait("the hero's pose", pose_settled),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot(shot),
    ]
}

/// Seed 42's Valkyrie: the spear in the right hand and the shield on the
/// left forearm; `x` takes the dagger; taking the shield off leaves the
/// arm bare. Seed 2's Archeologist: the fedora on the head, the bullwhip in
/// hand; applying the oil lamp lights it in the left hand (with a light),
/// applying it again snuffs it.
pub(super) fn equipment() -> Vec<Step> {
    let mut steps = start_as(smoke_choice());
    steps.extend([Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    })]);
    steps.extend(close_up());
    steps.extend(shown(
        "the spear in hand, the shield on the arm",
        |g| Ok(holds(g, "hand_r", &["spear"])? && holds(g, "lowerarm_l", &["round_shield"])?),
        "equip-valkyrie",
    ));
    steps.extend([key('x'), Step::Request("a command after x", command)]);
    steps.extend(shown(
        "the dagger in hand, the spear on the back",
        |g| Ok(holds(g, "hand_r", &["dagger"])? && holds(g, "spine_03", &["spear"])?),
        "equip-swapped",
    ));
    steps.extend([key('T'), Step::Request("the shield taken off", command)]);
    steps.extend(shown(
        "the arm bare",
        |g| holds(g, "lowerarm_l", &[]),
        "equip-no-shield",
    ));
    steps.extend(quit());
    steps.extend([
        Step::Call("seed 2", |g| {
            g.seed = Some(2);
            Ok(())
        }),
        Step::Push(UiEvent::BackToTitle),
    ]);
    steps.extend(start_as(choice("archeologist", "female")));
    steps.extend([Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    })]);
    steps.extend(close_up());
    steps.extend(shown(
        "the bullwhip in hand, the fedora on",
        |g| Ok(holds(g, "hand_r", &["missile"])? && holds(g, "Head", &["helm"])?),
        "equip-archeologist",
    ));
    steps.extend([
        key('a'),
        Step::Request("what to apply", |p| matches!(p, Prompt::FreeKey { .. })),
        key('i'),
        Step::Request("a command after lighting the lamp", command),
    ]);
    steps.extend(shown(
        "the lamp lit in the left hand",
        |g| {
            let (names, lit) = held_on(g, "hand_l")?;
            let clip = g.ui.as_ref().and_then(|ui| ui.map.hero_clip());
            // the apply clip first, then the lamp held up
            Ok(names == ["oil_lamp"] && lit && clip.as_deref() == Some("Idle_Torch"))
        },
        "equip-lamp-lit",
    ));
    steps.extend([
        key('a'),
        Step::Request("what to apply", |p| matches!(p, Prompt::FreeKey { .. })),
        key('i'),
        Step::Request("a command after snuffing the lamp", command),
    ]);
    steps.extend(shown(
        "the lamp snuffed and put away",
        |g| {
            fail_on_error_screen(g)?;
            let (names, lit) = held_on(g, "hand_l")?;
            Ok(names.is_empty() && !lit)
        },
        "equip-lamp-snuffed",
    ));
    steps.extend(quit());
    steps
}

/// The item in the hero's right hand while it is used ("potion").
fn in_use(g: &RenethackGame) -> Result<Option<String>, String> {
    let map = map_view(g)?;
    let Some(m) = map.hero_model() else {
        return Ok(None);
    };
    let node: Gd<Node> = m.node.clone().upcast();
    let held = ["Gear_hand_r", "Gear_hand_l"].into_iter().find_map(|bone| {
        let att = node.find_child_ex(bone).owned(false).done()?;
        att.get_children()
            .iter_shared()
            .filter(|c| !c.is_queued_for_deletion())
            .find_map(|c| {
                let name = c.get_name().to_string();
                name.strip_prefix(&format!("{USE_NODE}_"))
                    .map(str::to_string)
            })
    });
    Ok(held)
}

/// The hero plays `clip` and the effect `fx` has started (with the item
/// `held` in hand, if given).
fn using(g: &RenethackGame, clip: &str, fx: &str, held: Option<&str>) -> Result<bool, String> {
    let map = map_view(g)?;
    let playing = map.hero_clip();
    let started = map.hero_fx().started().contains(&fx);
    let hand = in_use(g)?;
    if started && playing.as_deref() != Some(clip) {
        return Err(format!(
            "{fx} started with the hero playing {playing:?}, not {clip}"
        ));
    }
    if started && held.is_some() && hand.as_deref() != held {
        return Err(format!("{fx} started with {hand:?} in hand, not {held:?}"));
    }
    Ok(started)
}

/// Seed 5's Wizard drinks a potion, reads a scroll, zaps a wand (a beam
/// to the wall) and casts
/// force bolt; seed 2's Archeologist eats, applies the lamp and throws a
/// stone. Each use starts its clip and its effect (a picture of each,
/// mid-motion).
pub(super) fn item_use() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 5", |g| {
        g.seed = Some(5);
        Ok(())
    })];
    steps.extend(start_as(choice("wizard", "male")));
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend(close_up());
    let getobj = |p: &Prompt| {
        matches!(
            p,
            Prompt::FreeKey {
                directions: false,
                ..
            }
        )
    };
    steps.extend([
        key('q'),
        Step::Request("what to drink", getobj),
        // not f: object detection asks where to look
        key('h'),
        Step::Request("a command after drinking", command),
        Step::Wait("drinking, the potion in hand, a sparkle", |g| {
            using(g, "ual2/Consume", "quaff", Some("potion"))
        }),
        Step::Shot("use-quaff"),
        Step::Wait("the hero idle again", |g| {
            Ok(!map_view(g)?.hero_fx().busy())
        }),
        key('r'),
        Step::Request("what to read", getobj),
        key('i'),
        Step::AnswerUntil('n', "a command after reading", idle_command),
        Step::Wait("reading, the scroll in hand, runes rising", |g| {
            using(g, "proc/read", "read", Some("scroll"))
        }),
        Step::Shot("use-read"),
        Step::Wait("the hero idle again", |g| {
            Ok(!map_view(g)?.hero_fx().busy())
        }),
        key('z'),
        Step::Request("what to zap", getobj),
        key('c'),
        Step::Request("the zap's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        key('l'),
        Step::AnswerUntil('n', "a command after zapping", idle_command),
        Step::Wait("zapping, the wand in hand, a beam", |g| {
            Ok(using(g, "Pistol_Shoot", "zap", Some("wand"))?
                && map_view(g)?.hero_fx().started().contains(&"beam"))
        }),
        Step::Shot("use-zap"),
        Step::Wait("the hero idle again", |g| {
            Ok(!map_view(g)?.hero_fx().busy())
        }),
        key('Z'),
        Step::Request("which spell", |p| matches!(p, Prompt::Menu { .. })),
        key('a'),
        Step::Request("the spell's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        key('l'),
        Step::AnswerUntil('n', "a command after casting", idle_command),
        Step::Wait("casting, a glow and a bolt", |g| {
            let map = map_view(g)?;
            Ok(using(g, "Spell_Simple_Shoot", "cast", None)?
                && map.hero_fx().started().contains(&"beam"))
        }),
        Step::Shot("use-cast"),
    ]);
    steps.extend(quit());
    steps.extend([
        Step::Call("seed 2", |g| {
            g.seed = Some(2);
            Ok(())
        }),
        Step::Push(UiEvent::BackToTitle),
    ]);
    steps.extend(start_as(choice("archeologist", "female")));
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    steps.extend(close_up());
    steps.extend([
        key('a'),
        Step::Request("what to apply", getobj),
        key('i'),
        Step::AnswerUntil('n', "a command after applying", idle_command),
        Step::Wait("applying, the lamp in hand", |g| {
            using(g, "Interact", "apply", Some("oil_lamp"))
        }),
        Step::Shot("use-apply"),
        Step::Wait("the hero idle again", |g| {
            Ok(!map_view(g)?.hero_fx().busy())
        }),
        key('t'),
        Step::Request("what to throw", getobj),
        key('g'),
        Step::Request("the throw's direction", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
        key('j'),
        Step::AnswerUntil('n', "a command after throwing", idle_command),
        Step::Wait("throwing, the stone in flight", |g| {
            let map = map_view(g)?;
            let clip = map.hero_clip();
            let flying = map.hero_fx().started().contains(&"throw");
            if flying && clip.as_deref() != Some("proc/throw") {
                return Err(format!("the throw plays {clip:?}"));
            }
            Ok(flying)
        }),
        Step::Shot("use-throw"),
        Step::Wait("the hero idle again", |g| {
            Ok(!map_view(g)?.hero_fx().busy())
        }),
        key('e'),
        Step::Request("what to eat", getobj),
        key('d'),
        Step::AnswerUntil('n', "a command after eating", idle_command),
        Step::Wait("eating, the ration in hand, crumbs", |g| {
            using(g, "ual2/Consume", "eat", Some("food"))
        }),
        Step::Shot("use-eat"),
    ]);
    steps.extend(quit());
    steps
}
