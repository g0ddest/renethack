//! The hero's equipment and item use (spec phases D and E), checked on the
//! scene tree of the hero's model: `equipment` (wield, swap, a shield off,
//! a lamp lit and snuffed) and `item-use` (quaff, read, zap, cast, eat,
//! apply, throw each start their clip and effect).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
