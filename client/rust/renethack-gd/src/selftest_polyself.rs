//! `polyself`: the hero polymorphed, in a debug-mode game where #polyself
//! changes them at will. Into a xorn, a red dragon and a newt, each: the map
//! draws the hero as that monster's model, their ring and the rim light on
//! it, lit as the monsters about them are (their own key and fills are for
//! their outfit), the portrait names the form, and the doll shows it framed
//! whole, whatever its size. The xorn's ring is wide enough to show round
//! its dome; the dragon turning to stone greys, and a prayer cures it; the
//! newt behind a man is drawn through him. Back to their own form
//! (#polyself into their role: "You return to human form!") the hero is
//! drawn as the Valkyrie again, by their own lights, their ring a human's:
//! from the xorn with the spear and the shield it kept in hand, from the
//! newt with none (the dragon dropped them). Pictures of the map and the
//! doll in each form with `--screenshots`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use godot::classes::DisplayServer;
use godot::obj::Singleton;
use nh_protocol::{Glyph, GlyphKind};
use nh_world::Prompt;

use super::{DialogEvent, Step, command, entry_key, key, map_view, start};
use crate::game::RenethackGame;
use crate::inventory_panel::InvInput;
use crate::map_view::Shown;
use crate::ui_events::UiEvent;

const SEED: u64 = 17;

/// The forms, in order: the name #polyself is told, the portrait's word,
/// the pictures of the map and the doll.
const FORMS: [(&str, &str, &str, &str); 3] = [
    ("xorn", "Xorn", "polyself-xorn", "polyself-xorn-doll"),
    (
        "red dragon",
        "Red Dragon",
        "polyself-dragon",
        "polyself-dragon-doll",
    ),
    ("newt", "Newt", "polyself-newt", "polyself-newt-doll"),
];
/// The hero's ring across in their own form (`map_view`'s).
const RING: f32 = 0.95;
/// The form under test (an index into `FORMS`, from 1).
static NOW: AtomicUsize = AtomicUsize::new(0);
/// The log's last message before the hero last turned back.
static BEFORE: AtomicU64 = AtomicU64::new(0);
/// The cell south of the hero and what it showed before the man was put
/// there.
static SOUTH: Mutex<Option<((i32, i32), Glyph)>> = Mutex::new(None);

fn form() -> (&'static str, &'static str, &'static str, &'static str) {
    FORMS[NOW.load(Ordering::Relaxed).saturating_sub(1)]
}

pub(super) fn polyself() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        g.seed = Some(SEED);
        NOW.store(0, Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    for (name, _, shot, doll) in FORMS {
        steps.push(Step::Call("the next form", |_| {
            NOW.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        steps.extend(become_(name));
        steps.extend([
            Step::Wait("the hero drawn as the form", |g| {
                let (shown, wanted) = map_view(g)?.hero_model_names(&g.world, cat(g)?);
                let now = hero_form_name(g);
                Ok(now.as_deref() == Some(form().0) && shown.is_some() && shown == wanted)
            }),
            Step::Wait("their ring and rim light on it", |g| {
                let (ring, rim) = map_view(g)?.hero_marked();
                Ok(ring && rim)
            }),
            Step::Wait("lit as the monsters are", |g| {
                Ok(!map_view(g)?.hero_studio_on())
            }),
            Step::Wait("the portrait naming the form", |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                Ok(ui.hud.role() == Some(form().1))
            }),
        ]);
        steps.extend(match name {
            "xorn" => vec![Step::Wait("the ring round the dome", |g| {
                Ok(map_view(g)?.hero_ring_across() > RING + 0.2)
            })],
            "red dragon" => turning_to_stone(),
            _ => behind_a_man(),
        });
        steps.extend(close_up());
        steps.push(Step::Shot(shot));
        steps.extend(doll_shot(doll, true));
        if name == "xorn" {
            steps.extend(back(
                true,
                "polyself-back-armed",
                "polyself-back-armed-doll",
            ));
        }
    }
    steps.extend(back(false, "polyself-back", "polyself-back-doll"));
    steps
}

/// Back to their own form: drawn as the Valkyrie, by their own lights,
/// their ring a human's, and holding what they kept (`armed`) or nothing.
fn back(armed: bool, shot: &'static str, doll: &'static str) -> Vec<Step> {
    let mut steps = vec![Step::Call("", |g| {
        BEFORE.store(g.world.log.last_seq(), Ordering::Relaxed);
        Ok(())
    })];
    steps.extend(become_("valkyrie"));
    steps.extend([
        Step::Wait("back in their own form", |g| {
            let back = g
                .world
                .log
                .since(BEFORE.load(Ordering::Relaxed))
                .any(|m| m.text.contains("You return to human form"));
            Ok(back && hero_form_name(g).as_deref() == Some("valkyrie"))
        }),
        Step::Wait("drawn as the Valkyrie again", |g| {
            let (shown, wanted) = map_view(g)?.hero_model_names(&g.world, cat(g)?);
            Ok(shown.is_some() && shown == wanted)
        }),
        Step::Wait("by their own lights, their ring a human's", |g| {
            let map = map_view(g)?;
            Ok(map.hero_studio_on() && map.hero_ring_across() == RING)
        }),
        if armed {
            Step::Wait("the spear and the shield in hand again", |g| {
                let held = map_view(g)?.hero_held_slots();
                Ok(held.contains(&"hand_r") && held.contains(&"arm_l"))
            })
        } else {
            Step::Wait("nothing in hand: the dragon dropped it", |g| {
                let held = map_view(g)?.hero_held_slots();
                Ok(!held.iter().any(|s| ["hand_r", "arm_l"].contains(s)))
            })
        },
    ]);
    steps.extend(close_up());
    steps.push(Step::Shot(shot));
    steps.extend(doll_shot(doll, false));
    steps
}

fn cat(g: &RenethackGame) -> Result<&nh_protocol::Catalog, String> {
    g.catalog.as_deref().ok_or_else(|| "no catalog".into())
}

/// The catalog's name of the hero's form now.
fn hero_form_name(g: &RenethackGame) -> Option<String> {
    let c = g.catalog.as_deref()?;
    let glyph = g.world.map.hero_form()?;
    let m = c.monsters.get(usize::try_from(glyph.mon?).ok()?)?;
    Some(m.name.clone())
}

/// #polyself into `name`, every question after it answered no.
fn become_(name: &'static str) -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("polyself".into()))),
        Step::Request(
            "what to become",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Become what kind of monster")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
        Step::AnswerUntil('n', "a command in the new form", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && !g.driver.is_active())
        }),
    ]
}

/// The form turning to stone (#wizintrinsic): the stone on its model,
/// and gone with a prayer.
fn turning_to_stone() -> Vec<Step> {
    let mut steps = vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizintrinsic".into()))),
        Step::Request("the intrinsics", |p| matches!(p, Prompt::Menu { .. })),
        Step::KeyFrom("petrifying", |g| entry_key(g, "petrifying")),
        Step::Dialog(DialogEvent::MenuConfirm),
        Step::Request("a command, turning to stone", command),
        Step::Wait("the stone on the form's model", |g| {
            let (shown, tinted) = map_view(g)?.hero_shown(&g.world);
            Ok(tinted && shown.iter().any(|(s, _)| *s == Shown::Stone))
        }),
        // (the debug command redraws the screen: the map comes up again)
        Step::Wait("the map still", |g| Ok(super::map_still(g))),
        Step::Shot("polyself-dragon-stone"),
    ];
    steps.extend(super::conditions_tests::pray());
    steps.push(Step::Wait("cured: the stone gone", |g| {
        let (shown, tinted) = map_view(g)?.hero_shown(&g.world);
        Ok(shown.is_empty() && !tinted)
    }));
    steps.push(Step::Wait("the prayer over", |g| {
        Ok(map_view(g)?.hero_prayer().is_none())
    }));
    steps
}

/// A man south of the form, between it and the eye (on the client's map
/// alone, the engine not asked): the form is drawn through him, and not
/// once he is gone.
fn behind_a_man() -> Vec<Step> {
    vec![
        Step::Call("a man south of the hero", |g| {
            let hero = g.world.map.hero().ok_or("no hero")?;
            let at = (hero.0, hero.1 + 1);
            let was = g
                .world
                .map
                .cell(at.0, at.1)
                .and_then(|c| c.glyph.clone())
                .ok_or("nothing south of the hero")?;
            let man = monster(cat(g)?, "human zombie")?;
            g.world.map.print(at.0, at.1, &man, None);
            *SOUTH.lock().unwrap() = Some((at, was));
            Ok(())
        }),
        Step::Wait("the form drawn through him", |g| {
            Ok(map_view(g)?.hero_seen_through())
        }),
        Step::Shot("polyself-newt-through"),
        Step::Call("the man gone", |g| {
            let (at, was) = SOUTH.lock().unwrap().take().ok_or("no man put")?;
            g.world.map.print(at.0, at.1, &was, None);
            Ok(())
        }),
        Step::Wait("the form seen as it is", |g| {
            Ok(!map_view(g)?.hero_seen_through())
        }),
    ]
}

/// A monster's glyph by its name.
fn monster(c: &nh_protocol::Catalog, name: &str) -> Result<Glyph, String> {
    let m = c
        .monsters
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("no monster {name}"))?;
    Ok(Glyph {
        glyph: None,
        ch: m.class.chars().next().map_or(' ' as i32, |c| c as i32),
        color: m.color,
        flags: 0,
        tile: m.tile_male,
        kind: GlyphKind::Mon,
        mon: Some(m.idx),
        cmap: None,
        level: None,
    })
}

/// The camera close on the hero.
fn close_up() -> [Step; 2] {
    [
        Step::Call("closer", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(6.5, 0.2);
            Ok(())
        }),
        Step::Wait("the camera on the hero, the map still", |g| {
            Ok(super::camera_settled(g)? && super::map_still(g))
        }),
    ]
}

fn headless() -> bool {
    DisplayServer::singleton().get_name() == "headless"
}

/// The inventory opened, its doll pictured, closed again. A form's doll
/// (`form`) fills much of its frame and none of it is cut off at an edge
/// (in a window: headless, nothing is rendered to look at).
fn doll_shot(shot: &'static str, form: bool) -> Vec<Step> {
    let mut steps = vec![
        Step::Call("the inventory opened", |g| {
            g.push_ui(UiEvent::Inventory(InvInput::Toggle));
            Ok(())
        }),
        Step::Wait("the panel open", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open()))
        }),
    ];
    if form {
        steps.push(Step::Wait("the form framed whole on the doll", |g| {
            if headless() {
                return Ok(true);
            }
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let Some(b) = ui.inventory.doll_drawn_box() else {
                return Ok(false);
            };
            let (lo, hi) = (b.position, b.end());
            let inside = lo.x > 0.01 && lo.y > 0.01 && hi.x < 0.99 && hi.y < 0.99;
            Ok(inside && b.size.x.max(b.size.y) > 0.45)
        }));
    }
    steps.extend([
        Step::Shot(shot),
        Step::Call("the inventory closed", |g| {
            g.push_ui(UiEvent::Inventory(InvInput::Close));
            Ok(())
        }),
    ]);
    steps
}
