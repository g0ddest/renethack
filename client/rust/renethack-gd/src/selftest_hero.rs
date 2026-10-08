//! The hero's equipment and item use (spec phases D and E), checked on the
//! scene tree of the hero's model: `equipment` (wield, swap, a shield off,
//! a lamp lit and snuffed) and `item-use` (quaff, read, zap, cast, eat,
//! apply, throw each start their clip and effect); `kits` (every role's
//! starting kit) and `two-hands` (the left hand on what is held in both).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use godot::classes::{BoneAttachment3D, Node, Node3D, SkeletonModifier3D};
use godot::prelude::*;
use nh_world::{Key, KeyInput, Prompt};

use super::{
    CharacterChoice, DialogEvent, Step, UiEvent, camera_settled, command, ctrl_key,
    fail_on_error_screen, idle_command, key, map_view, quit, smoke_choice, start_as,
};
use crate::art::{GLOVES_NODE, HELD_NODE, LAMP_LIGHT, NECK_NODE, OFF_HAND, USE_NODE};
use crate::game::RenethackGame;
use crate::off_hand::OffHand;

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

/// How far the shaft of the held `name` on `bone` leans from the upright,
/// in degrees (a pole lies along its model's z).
fn lean(g: &RenethackGame, bone: &str, name: &str) -> Result<f32, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let held = node
        .find_child_ex(&format!("Gear_{bone}"))
        .owned(false)
        .done()
        .and_then(|att| {
            att.find_child_ex(&format!("{HELD_NODE}_{name}"))
                .owned(false)
                .done()
        })
        .ok_or_else(|| format!("no {name} on {bone}"))?;
    let holder = held
        .get_child(0)
        .ok_or("an empty held node")?
        .cast::<Node3D>();
    let along = (holder.get_global_transform().basis * Vector3::BACK).normalized();
    Ok(along.y.abs().clamp(0.0, 1.0).acos().to_degrees())
}

/// A pole at rest stands upright beside the boot: within this of it.
const UPRIGHT_DEG: f32 = 12.0;

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

/// Seed 42's Valkyrie: the spear upright in the right hand (its butt on
/// the floor by her boot) and the shield on the left forearm; `x` takes
/// the dagger; taking the shield off leaves the arm bare. Seed 2's Archeologist: the fedora on the head, the bullwhip in
/// hand; applying the oil lamp lights it in the left hand (with a light),
/// applying it again snuffs it.
pub(super) fn equipment() -> Vec<Step> {
    let mut steps = start_as(smoke_choice());
    steps.extend([Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    })]);
    steps.extend(close_up());
    steps.extend(shown(
        "the spear in hand upright, the shield on the arm",
        |g| {
            Ok(holds(g, "hand_r", &["spear"])?
                && holds(g, "lowerarm_l", &["round_shield"])?
                && lean(g, "hand_r", "spear")? < UPRIGHT_DEG)
        },
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
        |g| Ok(holds(g, "hand_r", &["whip"])? && holds(g, "Head", &["helm"])?),
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

/// The item in one of the hero's hands while it is used: (the hand's
/// slot, the held model: "potion").
fn in_use_at(g: &RenethackGame) -> Result<Option<(&'static str, String)>, String> {
    let map = map_view(g)?;
    let Some(m) = map.hero_model() else {
        return Ok(None);
    };
    let node: Gd<Node> = m.node.clone().upcast();
    let held = ["hand_r", "hand_l"].into_iter().find_map(|hand| {
        let att = node
            .find_child_ex(&format!("Gear_{hand}"))
            .owned(false)
            .done()?;
        att.get_children()
            .iter_shared()
            .filter(|c| !c.is_queued_for_deletion())
            .find_map(|c| {
                let name = c.get_name().to_string();
                name.strip_prefix(&format!("{USE_NODE}_"))
                    .map(|held| (hand, held.to_string()))
            })
    });
    Ok(held)
}

/// The item in the hero's hand while it is used ("potion").
fn in_use(g: &RenethackGame) -> Result<Option<String>, String> {
    Ok(in_use_at(g)?.map(|(_, held)| held))
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

/// Seed 5's Wizard, at rest with his quarterstaff upright at his side,
/// drinks a potion, reads a scroll, zaps a wand (a beam to the wall) and
/// casts force bolt; seed 2's Archeologist eats, applies the lamp and
/// throws a stone. Each use starts its clip and its effect (a picture of
/// each, mid-motion).
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
    steps.extend(shown(
        "the quarterstaff in hand, upright",
        |g| Ok(holds(g, "hand_r", &["staff"])? && lean(g, "hand_r", "staff")? < UPRIGHT_DEG),
        "use-wizard",
    ));
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
        // west: across the room (east is a door at arm's length)
        key('h'),
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
                && map.hero_fx().started().contains(&"beam")
                && in_use(g)?.is_none())
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

/// A hostile next to the hero: (where it is, the vi-key towards it).
fn hostile_near(g: &RenethackGame) -> Option<((i32, i32), char)> {
    use nh_protocol::{GlyphKind, mg};
    let (hx, hy) = g.world.map.hero()?;
    let keys = [
        ((-1, 0), 'h'),
        ((1, 0), 'l'),
        ((0, -1), 'k'),
        ((0, 1), 'j'),
        ((-1, -1), 'y'),
        ((1, -1), 'u'),
        ((-1, 1), 'b'),
        ((1, 1), 'n'),
    ];
    keys.into_iter().find_map(|((dx, dy), k)| {
        let g = g.world.map.cell(hx + dx, hy + dy)?.glyph.as_ref()?;
        (g.kind == GlyphKind::Mon && g.flags & (mg::PET | mg::HERO) == 0)
            .then_some(((hx + dx, hy + dy), k))
    })
}

static BLOW_MARK: AtomicU64 = AtomicU64::new(0);
static SHOT_TAKEN: AtomicBool = AtomicBool::new(false);

/// The hero's last blow landed (the log says so since it was struck).
fn blow_landed(g: &RenethackGame) -> bool {
    let mark = BLOW_MARK.load(Ordering::Relaxed);
    g.world
        .log
        .since(mark)
        .any(|m| m.text.starts_with("You hit") || m.text.starts_with("You kill"))
}

/// The blow meets its target: the attack clip just past contact, while
/// its sparks fly (they start at 0.16 s and last 0.22 s).
fn contact(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    let Some(p) = map.hero_model().and_then(|m| m.player()) else {
        return Ok(false);
    };
    let t = p.get_current_animation_position();
    Ok(p.is_playing() && (0.17..0.32).contains(&t))
}

/// The hero is mid-blow: the attack clip a little way in.
fn mid_blow(g: &RenethackGame) -> Result<bool, String> {
    let map = map_view(g)?;
    let Some(p) = map.hero_model().and_then(|m| m.player()) else {
        return Ok(false);
    };
    let clip = p.get_current_animation().to_string();
    Ok(p.is_playing()
        && (clip == "Sword_Attack" || clip == "Punch_Jab")
        && p.get_current_animation_position() > 0.28)
}

/// Seed 173's Tourist fights the kobold zombie south of her at the start
/// with her fists (a sturdy foe: a Valkyrie's spear or a whip kills a
/// kobold or a goblin at the first blow, and a killed foe has no model
/// left to reel): each blow turns her to it with her attack clip, a hit
/// throws sparks and makes the zombie reel. A picture mid-blow, at the
/// medium distance that shows both, and one after the fight.
pub(super) fn combat() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 173", |g| {
        g.seed = Some(173);
        Ok(())
    })];
    steps.extend(start_as(choice("tourist", "female")));
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("a hostile next to the hero", |g| {
            fail_on_error_screen(g)?;
            Ok(hostile_near(g).is_some())
        }),
        Step::Call("the fight framed", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(4.2, 0.4);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    // blow after blow; the first that lands is caught as its sparks fly
    steps.push(Step::Call("no picture yet", |_| {
        SHOT_TAKEN.store(false, Ordering::Relaxed);
        Ok(())
    }));
    for _ in 0..10 {
        steps.extend([
            Step::Call("mark the log", |g| {
                BLOW_MARK.store(g.world.log.last_seq(), Ordering::Relaxed);
                Ok(())
            }),
            Step::KeyFrom("a blow at the hostile", |g| {
                let k = hostile_near(g).map_or('s', |(_, k)| k);
                Ok(nh_world::KeyInput::plain(nh_world::Key::Char(k)))
            }),
            Step::Request("a command after the blow", command),
            Step::Wait("the blow's contact (or a miss)", |g| {
                if !blow_landed(g) || SHOT_TAKEN.load(Ordering::Relaxed) {
                    return Ok(true);
                }
                // at contact, or past it (the next hit gets the picture)
                let map = map_view(g)?;
                let p = map.hero_model().and_then(|m| m.player());
                Ok(p.is_none_or(|p| !p.is_playing() || p.get_current_animation_position() >= 0.17))
            }),
            Step::ShotIf("combat-blow", |g| {
                let now = blow_landed(g) && !SHOT_TAKEN.load(Ordering::Relaxed) && contact(g)?;
                if now {
                    SHOT_TAKEN.store(true, Ordering::Relaxed);
                }
                Ok(now)
            }),
        ]);
    }
    steps.extend([
        Step::Wait("the hero settled", |g| Ok(!mid_blow(g)?)),
        Step::Call("a landed blow was pictured", |_| {
            if SHOT_TAKEN.load(Ordering::Relaxed) {
                Ok(())
            } else {
                Err("no blow landed at a moment for the picture".into())
            }
        }),
        Step::Shot("combat-after"),
        Step::Call("blows struck, a target reeled", |g| {
            let s = map_view(g)?.motion_stats();
            godot_print!("selftest: combat: {s:?}");
            if s.strikes == 0 || s.flinches == 0 {
                return Err(format!("no blow landed on a target that reels: {s:?}"));
            }
            Ok(())
        }),
    ]);
    steps.extend(quit());
    steps
}

/// The alignment `kits` plays each role with (one the role allows).
fn kit_align(role: &str) -> &'static str {
    match role {
        "knight" | "samurai" => "lawful",
        "rogue" => "chaotic",
        _ => "neutral",
    }
}
const KIT_GENDERS: [&str; 2] = ["male", "female"];

/// The `kits` hero `i`: (role, gender).
fn kit_hero(i: usize) -> (&'static str, &'static str) {
    (crate::kits::ROLES[i / 2], KIT_GENDERS[i % 2])
}

/// The hero of `kits` being looked at.
static KIT_AT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The names of the held models on a bone ("-" for none).
fn kit_slot(g: &RenethackGame, bone: &str) -> Result<String, String> {
    let (names, lit) = held_on(g, bone)?;
    let mut s = if names.is_empty() {
        "-".to_string()
    } else {
        names.join("+")
    };
    if lit {
        s.push_str(" (lit)");
    }
    Ok(s)
}

/// What a hero shows of a kit: the held models on the right hand, the
/// left forearm, the back (the quiver after the alternate weapon) and the
/// head, as `kit_slot` names them, then what the outfit has on of the
/// things worn ("neck", "gloves"); the idle and attack clips.
type KitLook = ([String; 5], Option<String>, Option<String>);

/// "neck" and "gloves", those of them that are there ("-" for none).
fn kit_worn(neck: bool, gloves: bool) -> String {
    let on = [(neck, "neck"), (gloves, "gloves")];
    let names: Vec<&str> = on.into_iter().filter(|o| o.0).map(|o| o.1).collect();
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join("+")
    }
}

/// The art's manifest, read once.
fn kit_art() -> Result<&'static nh_art::ArtManifest, String> {
    static ART: std::sync::OnceLock<Result<nh_art::ArtManifest, String>> =
        std::sync::OnceLock::new();
    ART.get_or_init(|| {
        let text = godot::classes::FileAccess::get_file_as_string("res://art/manifest.json");
        nh_art::ArtManifest::parse(&text.to_string()).map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(String::clone)
}

/// What a starting kit shows on the hero, by the art.
fn kit_look(g: &RenethackGame, kit: &[crate::kits::KitItem]) -> Result<KitLook, String> {
    let art = kit_art()?;
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let mut pack = nh_world::Pack::new();
    pack.replace(&crate::kits::inventory(&cat, kit));
    let gear = art.gear(&pack, &cat);
    let name = |h: Option<nh_art::HeldArt>| h.map(|h| art.held_at(h.held).0.to_string());
    let join = |names: Vec<String>| {
        if names.is_empty() {
            "-".to_string()
        } else {
            names.join("+")
        }
    };
    let one = |h| join(name(h).into_iter().collect());
    let back = join(
        name(gear.back)
            .into_iter()
            .chain(name(gear.quiver))
            .collect(),
    );
    let worn = kit_worn(gear.neck.is_some(), gear.gloves.is_some());
    Ok((
        [
            one(gear.hand_r),
            one(gear.arm_l),
            back,
            one(gear.head),
            worn,
        ],
        gear.idle,
        gear.attack,
    ))
}

/// What the hero on the map (the game's or the title's) shows.
fn hero_look(g: &RenethackGame) -> Result<KitLook, String> {
    let map = map_view(g)?;
    let (idle, attack) = map.hero_fight_clips();
    // the meshes the things worn put on the skeleton
    let node: Gd<Node> = map
        .hero_model()
        .ok_or("no hero model")?
        .node
        .clone()
        .upcast();
    let on = |prefix: &str| {
        node.find_children_ex(&format!("{prefix}*"))
            .type_("MeshInstance3D")
            .owned(false)
            .done()
            .iter_shared()
            .any(|n| !n.is_queued_for_deletion())
    };
    Ok((
        [
            kit_slot(g, "hand_r")?,
            kit_slot(g, "lowerarm_l")?,
            kit_slot(g, "spine_03")?,
            kit_slot(g, "Head")?,
            kit_worn(on(NECK_NODE), on(GLOVES_NODE)),
        ],
        idle,
        attack,
    ))
}

/// The hero turned to `yaw` degrees on the screen (0: facing the camera),
/// whichever way the game has them face.
fn kit_turn(g: &RenethackGame, yaw: f32) -> Result<(), String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let mut inner = m.node.get_child(0).ok_or("no model node")?.cast::<Node3D>();
    let mut r = inner.get_rotation_degrees();
    r.y = yaw - m.node.get_rotation_degrees().y;
    inner.set_rotation_degrees(r);
    Ok(())
}

/// When a clip's blow lands, in seconds (the pictures' moment);
/// RENETHACK_BLOW_AT=0.15: that moment of any clip instead (to look a
/// blow through).
fn blow_at(clip: &str) -> f64 {
    let asked = std::env::var("RENETHACK_BLOW_AT").ok();
    if let Some(at) = asked.and_then(|v| v.parse().ok()) {
        return at;
    }
    match clip {
        "proc/pole" => 0.3,
        "proc/dig" => 0.36,
        "proc/chop" => 0.26,
        _ => 0.24,
    }
}

/// The hero mid-blow, turned three-quarters on: the attack clip of their
/// gear, as it lands, held there.
fn kit_blow(g: &mut RenethackGame) -> Result<(), String> {
    let map = map_view(g)?;
    let (_, attack) = map.hero_fight_clips();
    let attack = attack.ok_or("no attack clip")?;
    let m = map.hero_model().ok_or("no hero model")?;
    let mut p = m.player().cloned().ok_or("no animation player")?;
    p.play_ex().name(attack.as_str()).custom_blend(0.0).done();
    p.seek_ex(blow_at(&attack)).update(true).done();
    p.pause();
    kit_turn(g, 70.0)
}

/// RENETHACK_KITS_LOOK=1: `kits` and `two-hands` only look (the pictures
/// of an older art).
fn only_look() -> bool {
    std::env::var_os("RENETHACK_KITS_LOOK").is_some()
}

/// The off hand's solver on the hero's model, if one was ever called
/// for, and how much of it holds (0: the arm as its clip has it; 1: the
/// hand on the haft).
fn off_hand(g: &RenethackGame) -> Result<(Option<Gd<OffHand>>, f32), String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let ik = node
        .find_child_ex(OFF_HAND)
        .owned(false)
        .done()
        .and_then(|n| n.try_cast::<OffHand>().ok());
    let Some(ik) = ik else {
        return Ok((None, 0.0));
    };
    let modifier = ik.clone().upcast::<SkeletonModifier3D>();
    let hold = modifier.get_influence();
    if modifier.is_active() != (hold > 0.0) {
        return Err(format!(
            "the off hand's solver holds {hold}, awake: {}",
            modifier.is_active()
        ));
    }
    Ok((Some(ik), hold))
}

/// A held slot's grip, from its bone to the held frame.
fn slot_grip(slot: &str) -> Result<Transform3D, String> {
    let s = kit_art()?
        .held_slot(slot)
        .ok_or_else(|| format!("no slot {slot}"))?;
    let rot = Vector3::new(s.rot[0], s.rot[1], s.rot[2]) * (std::f32::consts::PI / 180.0);
    Ok(Transform3D::new(
        Basis::from_euler(EulerOrder::YXZ, rot),
        Vector3::new(s.pos[0], s.pos[1], s.pos[2]),
    ))
}

/// Where the off hand grips what the right hand holds, in its held frame:
/// a thing applied in both hands, else a weapon wielded in both while the
/// left hand is free; none: the left arm is its clip's.
fn two_grip(g: &RenethackGame) -> Result<Option<Vector3>, String> {
    let art = kit_art()?;
    let spec = |name: &str| art.held_index(name).map(|i| art.held_at(i).1);
    let v = |t: [f32; 3]| Vector3::new(t[0], t[1], t[2]);
    let busy = !held_on(g, "lowerarm_l")?.0.is_empty() || !held_on(g, "hand_l")?.0.is_empty();
    if let Some((hand, name)) = in_use_at(g)? {
        let two = spec(&name)
            .and_then(|s| s.apply.as_ref()?.two)
            .filter(|_| hand == "hand_r" && !busy);
        return Ok(two.map(v));
    }
    let (names, _) = held_on(g, "hand_r")?;
    let two = names.first().and_then(|n| spec(n)).and_then(|s| s.two);
    Ok(two.filter(|_| !busy).map(v))
}

const PALM_PROBE: &str = "PalmProbe";

/// A bone attachment on the hero's left hand, which follows the pose as
/// the off hand's solver leaves it; none in the frame it is made.
fn palm_probe(g: &RenethackGame) -> Result<Option<Gd<Node3D>>, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    if let Some(p) = node.find_child_ex(PALM_PROBE).owned(false).done() {
        return Ok(Some(p.cast::<Node3D>()));
    }
    let mut skeleton = node
        .find_children_ex("*")
        .type_("Skeleton3D")
        .owned(false)
        .done()
        .iter_shared()
        .next()
        .ok_or("no skeleton in the hero's model")?;
    let mut probe = BoneAttachment3D::new_alloc();
    probe.set_name(PALM_PROBE);
    probe.set_bone_name("hand_l");
    skeleton.add_child(&probe);
    Ok(None)
}

/// How far the left palm is from where the off hand grips the thing in
/// the right hand (`two`, in its held frame), in metres on the map: both
/// measured on the scene's bone attachments, after the solver. None in
/// the frame the palm's probe is made.
fn off_grip(g: &RenethackGame, two: Vector3) -> Result<Option<f32>, String> {
    let Some(palm) = palm_probe(g)? else {
        return Ok(None);
    };
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let right = node
        .find_child_ex("Gear_hand_r")
        .owned(false)
        .done()
        .ok_or("nothing on the right hand")?
        .cast::<Node3D>();
    let grip = right.get_global_transform() * slot_grip("hand_r")? * two;
    let at = palm.get_global_transform() * slot_grip("hand_l")?.origin;
    Ok(Some((grip - at).length()))
}

/// The hand counts as on the haft within this of its grip, in metres.
const OFF_GRIP: f32 = 0.01;
/// The most the off hand's solver may cost a frame, in microseconds (of
/// the 16 667 a frame has; a debug build's cost is a few tens).
const OFF_HAND_USEC: f64 = 400.0;

/// The frame the off hand's hold was last seen to change in, and the
/// hold since.
static HOLD: Mutex<(u64, f32)> = Mutex::new((0, -1.0));

/// The off hand's hold, once the skeleton has been solved with it (the
/// bone attachments show the pose of the frame before: a hold set this
/// frame is not in them yet); none while it is fresh.
fn steady_hold(g: &RenethackGame) -> Result<Option<f32>, String> {
    let (_, hold) = off_hand(g)?;
    let frame = godot::classes::Engine::singleton().get_process_frames();
    let mut last = HOLD.lock().map_err(|e| e.to_string())?;
    if last.1 != hold {
        *last = (frame, hold);
    }
    Ok((frame > last.0).then_some(hold))
}

/// The off hand's hold has faded all the way in or out, as what the right
/// hand holds wants it (and the palm's probe is up).
fn hands_settled(g: &RenethackGame) -> Result<bool, String> {
    let want = if two_grip(g)?.is_some() { 1.0 } else { 0.0 };
    Ok(palm_probe(g)?.is_some() && steady_hold(g)? == Some(want))
}

/// The left hand is on the haft of a thing held in both hands, or left to
/// its clip when nothing is.
fn hands_as_held(g: &RenethackGame) -> Result<(), String> {
    let (ik, hold) = off_hand(g)?;
    let Some(two) = two_grip(g)? else {
        return match hold {
            0.0 => Ok(()),
            _ => Err(format!(
                "nothing is held in both hands, yet the off hand holds {hold}"
            )),
        };
    };
    let d = off_grip(g, two)?.ok_or("no probe on the left palm yet")?;
    let short = ik.map_or(0.0, |ik| ik.bind().miss());
    if hold == 1.0 && d < OFF_GRIP {
        Ok(())
    } else {
        Err(format!(
            "the left palm is {d:.3} m from its grip (the hold {hold}, {short:.3} m out of reach)"
        ))
    }
}

/// Every role in both genders with the starting kit NetHack gives it
/// (`kits.rs`, after u_init.c): what the hero holds, on which arm, on the
/// back and on the head, what hangs round the neck and covers the hands,
/// and the clips that calls for, are that kit's (logged too), and the left
/// hand is on a weapon of both hands and on no other; at rest facing the
/// camera and mid-blow; then the title shows the same hero in the same
/// kit. Pictures of each with `--screenshots`. RENETHACK_ROLES=knight,
/// priest: only those; RENETHACK_KITS_LOOK=1: pictures only.
pub(super) fn kits() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 1, the first hero", |g| {
        g.seed = Some(1);
        KIT_AT.store(0, Ordering::Relaxed);
        Ok(())
    })];
    let only: Option<Vec<String>> = std::env::var("RENETHACK_ROLES")
        .ok()
        .map(|v| v.split(',').map(str::to_string).collect());
    for i in 0..crate::kits::ROLES.len() * KIT_GENDERS.len() {
        let (role, gender) = kit_hero(i);
        // the pictures' names, made once for the test's life
        let rest: &'static str = format!("kit-{role}-{}", &gender[..1]).leak();
        let blow: &'static str = format!("{rest}-blow").leak();
        let title: &'static str = format!("{rest}-title").leak();
        if i > 0 {
            steps.push(Step::Call("the next hero", |_| {
                KIT_AT.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }));
        }
        if only.as_ref().is_some_and(|o| !o.iter().any(|x| x == role)) {
            continue;
        }
        steps.extend(start_as(CharacterChoice {
            align: kit_align(role).into(),
            ..choice(role, gender)
        }));
        steps.extend([
            Step::Wait("the hero on the map", |g| {
                Ok(map_view(g)?.hero_model().is_some())
            }),
            Step::Call("close up, the HUD out of the picture", |g| {
                let ui = g.ui.as_mut().ok_or("no UI")?;
                ui.map.set_distance(2.8, 0.5);
                ui.hud.set_visible(false);
                Ok(())
            }),
            Step::Wait("the hero's pose", pose_settled),
            Step::Wait("the camera on the hero", camera_settled),
            Step::Wait("the off hand settled", |g| {
                Ok(only_look() || hands_settled(g)?)
            }),
            Step::Call("the kit as NetHack gives it", |g| {
                let (role, gender) = kit_hero(KIT_AT.load(Ordering::Relaxed));
                let shown = hero_look(g)?;
                godot_print!("selftest: kit: {role} {gender}: {shown:?}");
                let kits = crate::kits::kits(role);
                let looks: Vec<KitLook> = kits
                    .iter()
                    .map(|k| kit_look(g, k))
                    .collect::<Result<_, _>>()?;
                if !only_look() && !looks.contains(&shown) {
                    return Err(format!(
                        "the {role} shows {shown:?}, not a kit of {looks:?}"
                    ));
                }
                // a weapon of both hands (the Barbarian's, the Wizard's
                // staff) has the left hand on it; a spear or a sword not
                if !only_look() {
                    hands_as_held(g).map_err(|e| format!("the {role}: {e}"))?;
                }
                kit_turn(g, 0.0)
            }),
            Step::Wait("a frame", |_| Ok(true)),
            Step::Shot(rest),
            Step::Call("mid-blow", kit_blow),
            Step::Wait("a frame", |_| Ok(true)),
            Step::Shot(blow),
            Step::Call("the HUD back", |g| {
                g.ui.as_mut().ok_or("no UI")?.hud.set_visible(true);
                Ok(())
            }),
        ]);
        steps.extend(quit());
        steps.extend([
            Step::Push(UiEvent::BackToTitle),
            Step::Wait("the title over the hero just played", |g| {
                let (role, _) = kit_hero(KIT_AT.load(Ordering::Relaxed));
                let map = map_view(g)?;
                // the title names the role by the catalog's code ("Arc")
                let cat = g.catalog.clone().ok_or("no catalog")?;
                let title = map.title_hero().0;
                let shown = cat.roles.iter().find(|r| {
                    r.code.eq_ignore_ascii_case(title) || r.name.eq_ignore_ascii_case(title)
                });
                Ok(map.title_drawn()
                    && shown.is_some_and(|r| r.name.eq_ignore_ascii_case(role))
                    && map.hero_model().is_some_and(|m| !m.is_pending()))
            }),
            Step::Wait("the title hero's pose", pose_settled),
            Step::Wait("the title hero's off hand settled", |g| {
                Ok(only_look() || hands_settled(g)?)
            }),
            Step::Call("the title hero in the kit the game starts with", |g| {
                let (role, _) = kit_hero(KIT_AT.load(Ordering::Relaxed));
                let shown = hero_look(g)?;
                let want = crate::kits::kits(role)
                    .first()
                    .map(|k| kit_look(g, k))
                    .ok_or("no kit")??;
                godot_print!("selftest: kit: {role} on the title: {shown:?}");
                if only_look() {
                    return Ok(());
                }
                if shown != want {
                    return Err(format!("the title's {role} shows {shown:?}, not {want:?}"));
                }
                hands_as_held(g).map_err(|e| format!("the title's {role}: {e}"))
            }),
            Step::Shot(title),
        ]);
    }
    steps
}

/// What `two-hands` wishes for and wields in turn, by its hero: the wish,
/// the held model it shows as, whether both hands hold it.
const TWO_HANDED: [(&str, &str, &str, bool); 7] = [
    ("samurai", "naginata", "polearm", true),
    ("samurai", "spear", "spear", false),
    ("samurai", "lance", "lance", false),
    ("barbarian", "two-handed sword", "great_blade", true),
    ("barbarian", "battle-axe", "great_axe", true),
    ("barbarian", "quarterstaff", "staff", true),
    ("barbarian", "dwarvish mattock", "mattock", true),
];

/// The thing of `TWO_HANDED` being looked at.
static TWO_AT: AtomicUsize = AtomicUsize::new(0);

fn two_now() -> (&'static str, &'static str, &'static str, bool) {
    TWO_HANDED[TWO_AT.load(Ordering::Relaxed)]
}

/// The pack's letters before a wish (the new one is the thing wished for).
static PACK: Mutex<String> = Mutex::new(String::new());

fn wished_letter(g: &RenethackGame) -> Result<char, String> {
    let had = PACK.lock().map_err(|e| e.to_string())?.clone();
    g.world
        .inventory
        .items()
        .iter()
        .map(|i| i.letter)
        .find(|l| !had.contains(*l))
        .ok_or_else(|| "the wish put nothing new in the pack".to_string())
}

/// A sweep through the hero's clips, a pose a frame: the clip and the
/// sample it is at, the worst the left palm was from its grip in the clip,
/// and the clips done (their worst).
#[derive(Default)]
struct Sweep {
    clip: usize,
    sample: usize,
    worst: f32,
    done: Vec<(String, f32)>,
}

static SWEEP: Mutex<Sweep> = Mutex::new(Sweep {
    clip: 0,
    sample: 0,
    worst: 0.0,
    done: Vec::new(),
});

/// The poses a clip is looked at in, from its start to its end.
const SWEEP_SAMPLES: usize = 24;

/// The clips the hero rests, walks, runs and strikes with now.
fn sweep_clips(g: &RenethackGame) -> Result<Vec<String>, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let anims = &kit_art()?.model_at(m.model_index()).1.anims;
    let (idle, attack) = map.hero_fight_clips();
    Ok([idle, anims.walk.clone(), anims.run.clone(), attack]
        .into_iter()
        .flatten()
        .collect())
}

/// One frame of the sweep: the pose set last frame is measured (the
/// solver has run on it since), the next one set. True when every clip is
/// through.
fn sweep(g: &RenethackGame) -> Result<bool, String> {
    let clips = sweep_clips(g)?;
    let two = two_grip(g)?.ok_or("nothing is held in both hands")?;
    let mut s = SWEEP.lock().map_err(|e| e.to_string())?;
    if s.sample > 0 {
        let d = off_grip(g, two)?.ok_or("no probe on the left palm yet")?;
        s.worst = s.worst.max(d);
    }
    if s.sample > SWEEP_SAMPLES {
        let through = (clips[s.clip].clone(), s.worst);
        s.done.push(through);
        (s.clip, s.sample, s.worst) = (s.clip + 1, 0, 0.0);
    }
    let Some(clip) = clips.get(s.clip) else {
        return Ok(true);
    };
    let map = map_view(g)?;
    let mut p = map
        .hero_model()
        .and_then(|m| m.player())
        .cloned()
        .ok_or("no animation player")?;
    let secs = p
        .get_animation(clip.as_str())
        .ok_or_else(|| format!("no clip {clip}"))?
        .get_length();
    let at = secs * s.sample as f32 / SWEEP_SAMPLES as f32;
    p.play_ex().name(clip.as_str()).custom_blend(0.0).done();
    p.seek_ex(f64::from(at)).update(true).done();
    p.pause();
    s.sample += 1;
    Ok(false)
}

/// The stroke of a thing applied being watched: its held model, whether
/// it is applied downwards, the clip it plays, the worst the left palm was
/// from its grip while both hands held it and the frames that was
/// measured in.
#[derive(Clone)]
struct Stroke {
    held: &'static str,
    down: bool,
    clip: String,
    worst: f32,
    frames: u32,
}

static STROKE: Mutex<Stroke> = Mutex::new(Stroke {
    held: "",
    down: false,
    clip: String::new(),
    worst: 0.0,
    frames: 0,
});

fn watch(held: &'static str, down: bool) -> Result<(), String> {
    *STROKE.lock().map_err(|e| e.to_string())? = Stroke {
        held,
        down,
        clip: String::new(),
        worst: 0.0,
        frames: 0,
    };
    Ok(())
}

/// A frame of the stroke: the left palm's distance from its grip, once
/// the off hand's hold is whole.
fn watch_stroke(g: &RenethackGame) -> Result<(), String> {
    let whole = steady_hold(g)? == Some(1.0);
    let Some(two) = two_grip(g)?.filter(|_| whole) else {
        return Ok(());
    };
    if let Some(d) = off_grip(g, two)? {
        let mut s = STROKE.lock().map_err(|e| e.to_string())?;
        (s.worst, s.frames) = (s.worst.max(d), s.frames + 1);
    }
    Ok(())
}

/// The hero applies the thing `watch` was told of: its clip plays with it
/// in the right hand and the left hand on it; a picture as the stroke
/// lands.
fn stroke(what: &'static str, landed: &'static str, shot: &'static str) -> Vec<Step> {
    vec![
        Step::Wait(what, |g| {
            // from the frame the use shows: the thing in a hand
            let Some(hand) = in_use_at(g)? else {
                return Ok(false);
            };
            let clip = map_view(g)?.hero_clip().ok_or("the apply plays no clip")?;
            let mut s = STROKE.lock().map_err(|e| e.to_string())?;
            let (held, art) = (s.held, kit_art()?);
            let want = art
                .held_index(held)
                .and_then(|i| art.held_at(i).1.apply.as_ref())
                .map(|a| a.clip(s.down).to_string());
            if !only_look() {
                if Some(&clip) != want.as_ref() {
                    return Err(format!("the {held} applied plays {clip}, not {want:?}"));
                }
                if hand != ("hand_r", held.to_string()) {
                    return Err(format!("the {held} applied shows {hand:?} in use"));
                }
            }
            s.clip = clip;
            Ok(true)
        }),
        Step::Wait(landed, |g| {
            watch_stroke(g)?;
            let clip = STROKE.lock().map_err(|e| e.to_string())?.clip.clone();
            let map = map_view(g)?;
            let mut p = map
                .hero_model()
                .and_then(|m| m.player())
                .cloned()
                .ok_or("no animation player")?;
            if p.get_current_animation().to_string() != clip {
                return Err(format!("the {clip} ended before its stroke landed"));
            }
            let at = if only_look() { 0.5 } else { blow_at(&clip) };
            if p.get_current_animation_position() < at {
                return Ok(false);
            }
            p.pause();
            Ok(true)
        }),
        Step::Shot(shot),
        Step::Call("the stroke goes on", |g| {
            let map = map_view(g)?;
            let mut p = map
                .hero_model()
                .and_then(|m| m.player())
                .cloned()
                .ok_or("no animation player")?;
            p.play();
            Ok(())
        }),
        Step::Wait("the stroke ends", |g| {
            watch_stroke(g)?;
            let clip = STROKE.lock().map_err(|e| e.to_string())?.clip.clone();
            Ok(map_view(g)?.hero_clip().as_deref() != Some(clip.as_str()))
        }),
        Step::Call("both hands on it through the stroke", |_| {
            let Stroke {
                held,
                clip,
                worst,
                frames,
                ..
            } = STROKE.lock().map_err(|e| e.to_string())?.clone();
            godot_print!(
                "selftest: two-hands: {held} applied, {clip}: the palm within {worst:.4} m \
                 of its grip over {frames} frames"
            );
            if only_look() {
                return Ok(());
            }
            if frames < 5 {
                return Err(format!(
                    "both hands held the {held} for {frames} frames only"
                ));
            }
            if worst >= OFF_GRIP {
                return Err(format!(
                    "the left palm came {worst:.3} m off the {held} in {clip}"
                ));
            }
            Ok(())
        }),
    ]
}

/// The way the lance of `two-hands` strikes: (the step, its vi-key).
static LANCE: Mutex<Option<((i32, i32), char)>> = Mutex::new(None);

/// The vi-key towards two open cells in a row from the hero, the second
/// with nothing on it: where a lance applied strikes. Chosen once, as the
/// cursor first steps.
fn lance_key(g: &RenethackGame) -> Result<KeyInput, String> {
    let mut way = LANCE.lock().map_err(|e| e.to_string())?;
    if way.is_none() {
        let cat = g.catalog.clone().ok_or("no catalog")?;
        let (x, y) = g.world.map.hero().ok_or("no hero")?;
        let cell = |dx: i32, dy: i32, n: i32| g.world.map.cell(x + dx * n, y + dy * n);
        *way = [((1, 0), 'l'), ((-1, 0), 'h'), ((0, 1), 'j'), ((0, -1), 'k')]
            .into_iter()
            .find(|&((dx, dy), _)| {
                crate::map_view::is_open(cell(dx, dy, 1), &cat)
                    && crate::map_view::is_open(cell(dx, dy, 2), &cat)
                    && cell(dx, dy, 2).is_some_and(|c| c.entity().is_none())
            });
    }
    let (_, k) = way.ok_or("no two open cells in a row by the hero")?;
    Ok(KeyInput::plain(Key::Char(k)))
}

/// The hero digs down with the pick `watch` was told of (`pick` gives its
/// letter): the dig plays in both hands; a picture as its first stroke
/// lands.
fn dig_down(
    pick: fn(&RenethackGame) -> Result<KeyInput, String>,
    what: &'static str,
    shot: &'static str,
) -> Vec<Step> {
    let mut steps = vec![
        key('a'),
        Step::Request("what to apply", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: false,
                    ..
                }
            )
        }),
        Step::KeyFrom("the pick", pick),
        // one not in hand is wielded first, a turn before
        Step::AnswerUntil('n', "the dig's direction", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(
                g.pending,
                Some((
                    _,
                    Prompt::FreeKey {
                        directions: true,
                        ..
                    }
                ))
            ))
        }),
        key('>'),
        Step::AnswerUntil('n', "a command after digging", idle_command),
    ];
    steps.extend(stroke(what, "the pick's stroke lands", shot));
    steps
}

/// A game of `two-hands` begun: the hero close up, the HUD out of the
/// picture.
fn two_game(choice: CharacterChoice) -> Vec<Step> {
    let mut steps = start_as(choice);
    steps.extend([
        Step::Wait("the hero on the map", |g| {
            Ok(map_view(g)?.hero_model().is_some())
        }),
        Step::Call("close up, the HUD out of the picture", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(3.6, 1.3);
            ui.hud.set_visible(false);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
    ]);
    steps
}

/// The game of `two-hands` ended.
fn two_over() -> Vec<Step> {
    let mut steps = vec![Step::Call("the HUD back", |g| {
        g.ui.as_mut().ok_or("no UI")?.hud.set_visible(true);
        Ok(())
    })];
    steps.extend(quit());
    steps
}

/// The left hand on what is held in both hands. A Samurai wishes for a
/// naginata, a spear and a lance, a Barbarian for a two-handed sword, a
/// battle-axe, a dwarvish mattock and a quarterstaff, and each wields them
/// in turn: the naginata, the sword, the axes and the staff are held in
/// both hands, the left palm within a centimetre of its grip in every pose
/// of the clips the hero rests, walks, runs and strikes with (measured on
/// the scene's bones, after the solver); the spear and the lance in one,
/// the left arm left to its clip. The lance applied at a spot two cells
/// off turns the hero to it and strikes in both hands; the mattock
/// applied downwards digs in both. Last an Archeologist digs down with
/// her own pick-axe, which no hand held: it plays the dig in both hands
/// too. The solver's cost is logged. Pictures of each at rest and as the
/// blow lands with `--screenshots`; RENETHACK_KITS_LOOK=1: pictures only.
pub(super) fn two_hands() -> Vec<Step> {
    let getobj = |p: &Prompt| {
        matches!(
            p,
            Prompt::FreeKey {
                directions: false,
                ..
            }
        )
    };
    let mut steps = vec![Step::Call("the first thing", |_| {
        TWO_AT.store(0, Ordering::Relaxed);
        Ok(())
    })];
    for (i, (role, wish, held, both)) in TWO_HANDED.into_iter().enumerate() {
        let rest: &'static str = format!("two-{held}").leak();
        let blow: &'static str = format!("{rest}-blow").leak();
        if i > 0 {
            steps.push(Step::Call("the next thing", |_| {
                TWO_AT.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }));
        }
        if i == 0 || TWO_HANDED[i - 1].0 != role {
            // debug mode for the wishes
            steps.push(Step::Call("debug mode, seed 7", |g| {
                g.debug_mode = true;
                g.seed = Some(7);
                Ok(())
            }));
            steps.extend(two_game(CharacterChoice {
                align: kit_align(role).into(),
                ..choice(role, "male")
            }));
        }
        steps.extend([
            Step::Call("the pack before the wish", |g| {
                let letters = g.world.inventory.items().iter().map(|i| i.letter);
                *PACK.lock().map_err(|e| e.to_string())? = letters.collect();
                Ok(())
            }),
            Step::Key(ctrl_key('w')),
            Step::Request(
                "the wish",
                |p| matches!(p, Prompt::Text { query, .. } if query.contains("For what do you wish")),
            ),
            Step::Dialog(DialogEvent::TextSubmitted(format!("uncursed +0 {wish}"))),
            Step::Request("a command after the wish", command),
            key('w'),
            Step::Request("what to wield", getobj),
            Step::KeyFrom("the thing wished for", |g| {
                Ok(KeyInput::plain(Key::Char(wished_letter(g)?)))
            }),
            Step::Request("a command after wielding", command),
            Step::Wait("the thing wished for in hand", |g| {
                let (_, _, held, _) = two_now();
                let wielded = g.world.inventory.wielded().map(|i| i.letter);
                Ok(wielded == wished_letter(g).ok()
                    && (only_look() || holds(g, "hand_r", &[held])?))
            }),
            Step::Wait("the hero's pose", pose_settled),
            Step::Wait("the off hand settled", |g| {
                Ok(only_look() || hands_settled(g)?)
            }),
            Step::Call("held in both hands, or in one", |g| {
                let (_, wish, held, both) = two_now();
                if !only_look() {
                    if two_grip(g)?.is_some() != both {
                        return Err(format!("the {wish} ({held}) in both hands: not {both}"));
                    }
                    hands_as_held(g).map_err(|e| format!("the {wish}: {e}"))?;
                }
                kit_turn(g, 0.0)
            }),
            Step::Shot(rest),
            Step::Call("mid-blow", kit_blow),
            Step::Shot(blow),
        ]);
        if both {
            steps.extend([
                Step::Call("the sweep begins", |_| {
                    *SWEEP.lock().map_err(|e| e.to_string())? = Sweep::default();
                    Ok(())
                }),
                Step::Wait("the left hand on the haft in every pose", |g| {
                    Ok(only_look() || sweep(g)?)
                }),
                Step::Call("within a centimetre of its grip all along", |_| {
                    let (_, wish, _, _) = two_now();
                    let s = SWEEP.lock().map_err(|e| e.to_string())?;
                    godot_print!(
                        "selftest: two-hands: {wish}: the palm from its grip, m: {:?}",
                        s.done
                    );
                    match s.done.iter().find(|(_, worst)| *worst >= OFF_GRIP) {
                        Some((clip, worst)) if !only_look() => Err(format!(
                            "the left palm comes {worst:.3} m off the {wish} in {clip}"
                        )),
                        _ => Ok(()),
                    }
                }),
            ]);
        }
        steps.push(Step::Call("at rest again", |g| {
            let map = map_view(g)?;
            let (idle, _) = map.hero_fight_clips();
            let m = map.hero_model().ok_or("no hero model")?;
            let mut p = m.player().cloned().ok_or("no animation player")?;
            p.play_ex()
                .name(idle.ok_or("no idle clip")?.as_str())
                .custom_blend(0.0)
                .done();
            kit_turn(g, 0.0)
        }));
        if held == "lance" {
            // applied, it strikes a spot two cells off in both hands
            steps.extend([
                Step::Call("the lance applied", |_| {
                    *LANCE.lock().map_err(|e| e.to_string())? = None;
                    watch("lance", false)
                }),
                key('a'),
                Step::Request("what to apply", getobj),
                Step::KeyFrom("the lance", |g| {
                    Ok(KeyInput::plain(Key::Char(wished_letter(g)?)))
                }),
                // the game's first getpos shows its tip first
                Step::AnswerUntil('n', "where to hit", |g| {
                    fail_on_error_screen(g)?;
                    Ok(g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
                }),
                Step::KeyFrom("a step of the cursor", lance_key),
                Step::Request("the cursor a cell off", command),
                Step::KeyFrom("a step of the cursor", lance_key),
                Step::Request("the cursor two cells off", command),
                key('.'),
                Step::AnswerUntil('n', "a command after the lance", idle_command),
            ]);
            steps.extend(stroke(
                "the lance struck out in both hands",
                "the lance's stroke lands",
                "two-lance-applied",
            ));
            steps.push(Step::Call("the hero turned to the spot", |g| {
                let way = *LANCE.lock().map_err(|e| e.to_string())?;
                let (step, _) = way.ok_or("the lance struck nowhere")?;
                let (yaw, want) = (
                    map_view(g)?.hero_yaw(),
                    crate::animator::yaw_toward((0, 0), step),
                );
                if !only_look() && (yaw - want).abs() > 0.5 {
                    return Err(format!("the hero faces {yaw}, the spot is at {want}"));
                }
                Ok(())
            }));
        }
        if held == "mattock" {
            // applied downwards, it digs in both hands as a pick-axe does,
            // from its guard and back to it
            steps.push(Step::Call("the mattock applied", |_| {
                watch("mattock", true)
            }));
            steps.extend(dig_down(
                |g| Ok(KeyInput::plain(Key::Char(wished_letter(g)?))),
                "digging, the mattock in both hands",
                "two-mattock-dig",
            ));
        }
        let last = TWO_HANDED.get(i + 1).is_none_or(|next| next.0 != role);
        if last {
            steps.push(Step::Call("the solver's cost", |g| {
                let Some(ik) = off_hand(g)?.0 else {
                    return match only_look() {
                        true => Ok(()),
                        false => Err("no off hand solver".to_string()),
                    };
                };
                let (usec, frames) = ik.bind().spent();
                let each = usec as f64 / frames.max(1) as f64;
                godot_print!(
                    "selftest: two-hands: the off hand solved {frames} frames in {usec} µs, \
                     {each:.1} µs a frame"
                );
                // a frame is 16 667 µs: the arm must not show in it
                if frames > 0 && each > OFF_HAND_USEC {
                    return Err(format!("the off hand costs {each:.0} µs a frame"));
                }
                Ok(())
            }));
            steps.extend(two_over());
            steps.push(Step::Push(UiEvent::BackToTitle));
        }
    }
    // the Archeologist's own pick-axe: applied downwards, it digs in both
    // hands (it is wielded for that first, a turn before)
    steps.push(Step::Call("no debug mode, seed 2", |g| {
        g.debug_mode = false;
        g.seed = Some(2);
        Ok(())
    }));
    steps.extend(two_game(choice("archeologist", "female")));
    steps.extend([
        Step::Wait("the hero's pose", pose_settled),
        Step::Call("the pick-axe applied", |_| watch("pick", true)),
    ]);
    steps.extend(dig_down(
        |g| {
            let c = super::letter_of(g, "pick-axe").ok_or("no pick-axe in the pack")?;
            Ok(KeyInput::plain(Key::Char(c)))
        },
        "digging, the pick-axe in both hands",
        "two-pick-dig",
    ));
    steps.extend(two_over());
    steps
}

/// The hero models the `roles` test saw, in order.
static ROLE_MODELS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// The 13 roles: role, gender, alignment, picture.
const ROLES: [(&str, &str, &str, &str); 13] = [
    ("archeologist", "male", "neutral", "role-archeologist"),
    ("barbarian", "male", "neutral", "role-barbarian"),
    ("caveman", "female", "neutral", "role-caveman"),
    ("healer", "female", "neutral", "role-healer"),
    ("knight", "male", "lawful", "role-knight"),
    ("monk", "male", "neutral", "role-monk"),
    ("priest", "female", "neutral", "role-priest"),
    ("rogue", "male", "chaotic", "role-rogue"),
    ("ranger", "female", "neutral", "role-ranger"),
    ("samurai", "male", "lawful", "role-samurai"),
    ("tourist", "female", "neutral", "role-tourist"),
    ("valkyrie", "female", "neutral", "role-valkyrie"),
    ("wizard", "male", "neutral", "role-wizard"),
];

/// Each of the 13 roles, created and seen close up: every role's hero is
/// its own model, and none is part 1's generic outfit.
pub(super) fn roles() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 1, nothing seen yet", |g| {
        g.seed = Some(1);
        ROLE_MODELS.lock().map_err(|e| e.to_string())?.clear();
        Ok(())
    })];
    // RENETHACK_ROLES=healer,wizard: only those (to look at a few)
    let only: Option<Vec<String>> = std::env::var("RENETHACK_ROLES")
        .ok()
        .map(|v| v.split(',').map(str::to_string).collect());
    let picked: Vec<_> = ROLES
        .into_iter()
        .filter(|(r, ..)| only.as_ref().is_none_or(|o| o.iter().any(|x| x == r)))
        .collect();
    // RENETHACK_ROLES_OTHER=1: each of the other gender
    let other = std::env::var_os("RENETHACK_ROLES_OTHER").is_some();
    for (i, (role, gender, align, shot)) in picked.into_iter().enumerate() {
        let gender = match (other, gender) {
            (false, g) => g,
            (true, "male") => "female",
            (true, _) => "male",
        };
        if i > 0 {
            steps.push(Step::Push(UiEvent::BackToTitle));
        }
        steps.extend(start_as(CharacterChoice {
            align: align.into(),
            ..choice(role, gender)
        }));
        steps.extend([
            Step::Wait("the hero on the map", |g| {
                Ok(map_view(g)?.hero_model().is_some())
            }),
            Step::Call("close to the face", |g| {
                g.ui.as_mut().ok_or("no UI")?.map.set_distance(2.3, 0.7);
                Ok(())
            }),
            Step::Wait("the hero's pose", pose_settled),
            Step::Wait("the camera on the hero", camera_settled),
            Step::Shot(shot),
            Step::Call("the hero's model", |g| {
                let m = map_view(g)?
                    .hero_model()
                    .ok_or("no hero model")?
                    .model_index();
                ROLE_MODELS.lock().map_err(|e| e.to_string())?.push(m);
                Ok(())
            }),
        ]);
        steps.extend(quit());
    }
    steps.push(Step::Call("every role its own model", |_| {
        let seen = ROLE_MODELS.lock().map_err(|e| e.to_string())?.clone();
        let text = godot::classes::FileAccess::get_file_as_string("res://art/manifest.json");
        let art = nh_art::ArtManifest::parse(&text.to_string()).map_err(|e| e.to_string())?;
        let generic: Vec<usize> = ["human_male", "human_female", "ranger_male", "ranger_female"]
            .iter()
            .filter_map(|n| art.model_index(n))
            .collect();
        let names: Vec<&str> = seen.iter().map(|&m| art.model_at(m).0).collect();
        godot_print!("selftest: roles: {names:?}");
        let mut unique = seen.clone();
        unique.sort_unstable();
        unique.dedup();
        let wanted = std::env::var("RENETHACK_ROLES").map_or(ROLES.len(), |v| v.split(',').count());
        if seen.len() != wanted || unique.len() != seen.len() {
            return Err(format!("the roles share models: {names:?}"));
        }
        if seen.iter().any(|m| generic.contains(m)) {
            return Err(format!("a role wears the generic outfit: {names:?}"));
        }
        Ok(())
    }));
    steps
}
