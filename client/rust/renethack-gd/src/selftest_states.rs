//! `states`: what the hero's body shows of their state, in debug mode
//! (qa C3): an archeologist digs a pit and stands sunk in it, then climbs
//! out; sets a bear trap wished for, steps into it and crouches held by its
//! jaws until free; wishes for a web, steps into it and struggles wrapped
//! in it until free. Sinking into lava and being under water are told to
//! the client as the engine would tell them (the hero notice, the status
//! condition Submrg): the engine's own way there kills a hero without
//! fire resistance or water breathing. Then the hero prays (qa C4): too
//! soon after the start the god is angered, and darkness closes in round
//! the kneeling hero; debug mode forces the god pleased the second time,
//! and a shaft of light falls on them. Last the hero drinks three potions
//! wished for (qa C16), each a spell of helplessness seen only once it
//! passed, and shown after it: sleeping (lying, letters rising), passed out
//! drunk (lying), paralysed (slumped, still), then up again. With
//! --screenshots a picture of each.

use std::sync::Mutex;

use nh_world::{Helpless, Key, KeyInput, PrayerOutcome, Prompt, Terrain, TrapKind, cell_terrain};

use nh_protocol::{HeroNotice, WinCall};

use super::{
    CharacterChoice, Check, DialogEvent, Step, camera_settled, command, ctrl_key,
    fail_on_error_screen, key, map_view, quit, set_status, smoke_choice, start_as,
};
use crate::game::RenethackGame;
use crate::map_view::{Held, HelplessPhase};

type Chooser = fn(&RenethackGame) -> Result<Option<KeyInput>, String>;

/// Where the trap of the step under way lies, and the way off it.
static TRAP: Mutex<Option<((i32, i32), char)>> = Mutex::new(None);

/// The vi-keys of the eight steps.
const STEPS: [((i32, i32), char); 8] = [
    ((-1, 0), 'h'),
    ((-1, -1), 'y'),
    ((0, -1), 'k'),
    ((1, -1), 'u'),
    ((1, 0), 'l'),
    ((1, 1), 'n'),
    ((0, 1), 'j'),
    ((-1, 1), 'b'),
];

fn plain(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

fn choice(role: &str) -> CharacterChoice {
    CharacterChoice {
        role: role.into(),
        ..smoke_choice()
    }
}

/// A step off the hero's cell onto open floor nobody stands on.
fn way_off(g: &RenethackGame) -> Result<char, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let (hx, hy) = g.world.map.hero().ok_or("the hero is not on the map")?;
    STEPS
        .iter()
        .find_map(|&((dx, dy), key)| {
            let cell = g.world.map.cell(hx + dx, hy + dy)?;
            let open = matches!(
                cell_terrain(cell, cat),
                Some(Terrain::Floor | Terrain::DarkFloor | Terrain::Corridor)
            ) && cell.entity().is_none();
            open.then_some(key)
        })
        .ok_or_else(|| format!("no open floor next to the hero at {:?}", (hx, hy)))
}

fn back_of(key: char) -> char {
    let (d, _) = STEPS
        .iter()
        .find(|(_, k)| *k == key)
        .copied()
        .unwrap_or(((0, 0), '.'));
    STEPS
        .iter()
        .find(|((x, y), _)| (*x, *y) == (-d.0, -d.1))
        .map_or('.', |(_, k)| *k)
}

fn pending(g: &RenethackGame) -> Option<&Prompt> {
    g.pending.as_ref().map(|(_, p)| p)
}

/// What holds the hero as the map draws it, and how deep they stand.
fn held(g: &RenethackGame) -> Result<(Option<Held>, f32, Option<&'static str>), String> {
    Ok(map_view(g)?.hero_held())
}

/// The letter of the first item whose name has `name`.
fn letter_of(g: &RenethackGame, name: &str) -> Result<char, String> {
    let items = g.world.inventory.items();
    items
        .iter()
        .find(|i| i.text.contains(name))
        .map(|i| i.letter)
        .ok_or_else(|| format!("no {name} in the pack"))
}

/// Dig down with the pick-axe until the hero stands in the pit (digging
/// may be interrupted: applying again goes on).
fn dig_pit(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if g.world.hero_trap.is_some_and(TrapKind::is_pit) {
        return Ok(None);
    }
    Ok(match pending(g) {
        Some(Prompt::Command) => Some(plain('a')),
        Some(Prompt::FreeKey {
            directions: true, ..
        }) => Some(plain('>')),
        Some(Prompt::FreeKey { .. }) => Some(plain(letter_of(g, "pick-axe")?)),
        Some(Prompt::Choice { .. }) => Some(plain('y')),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

/// Set the beartrap wished for (setting it may be interrupted: applying
/// it again goes on).
fn arm_bear_trap(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    let Ok(letter) = letter_of(g, "beartrap") else {
        if !g.world.log.iter().any(|m| m.text.contains("finish arming")) {
            return Err("the beartrap went, unset".into());
        }
        return Ok(None);
    };
    Ok(match pending(g) {
        Some(Prompt::Command) => Some(plain('a')),
        Some(Prompt::FreeKey { .. }) => Some(plain(letter)),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

/// Off the trap and back onto it until it holds the hero (a trap seen is
/// escaped one time in five).
fn step_in(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if g.world.hero_trap.is_some() {
        return Ok(None);
    }
    let (at, off) = TRAP
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or("no trap remembered")?;
    Ok(match pending(g) {
        // "Really step onto that bear trap?"
        Some(Prompt::Choice { .. }) => Some(plain('y')),
        Some(Prompt::Command) if g.world.map.hero() == Some(at) => Some(plain(off)),
        Some(Prompt::Command) => Some(plain(back_of(off))),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

/// Walk off until free (every try is a turn).
fn struggle(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    if g.world.hero_trap.is_none() {
        return Ok(None);
    }
    let (_, off) = TRAP
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or("no trap remembered")?;
    Ok(match pending(g) {
        Some(Prompt::Command) => Some(plain(off)),
        Some(Prompt::Choice { .. }) => Some(plain('n')),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

/// Remember the hero's cell as the trap's, and a way off it.
fn mark_trap(g: &mut RenethackGame) -> Result<(), String> {
    let at = g.world.map.hero().ok_or("the hero is not on the map")?;
    let off = way_off(g)?;
    *TRAP.lock().map_err(|e| e.to_string())? = Some((at, off));
    Ok(())
}

fn pose_settled(g: &RenethackGame) -> Result<bool, String> {
    let p = map_view(g)?.hero_model().and_then(|m| m.player()).cloned();
    Ok(p.is_none_or(|p| !p.is_playing() || p.get_current_animation_position() > 0.6))
}

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

fn wish(what: &'static str) -> [Step; 4] {
    [
        Step::Key(ctrl_key('w')),
        Step::Request(
            "the wish",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("wish")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::Request("a command after the wish", command),
    ]
}

/// `#pray`, yes to "Are you sure you want to pray?", and `force` to debug
/// mode's "Force the gods to be pleased?".
fn pray(force: char) -> [Step; 8] {
    [
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("pray".into()))),
        Step::Request(
            "Are you sure you want to pray?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("pray")),
        ),
        key('y'),
        Step::Request(
            "Force the gods to be pleased?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Force")),
        ),
        key(force),
        Step::Request("a command after the prayer", command),
    ]
}

/// The prayer shown with `outcome`, its answer at full strength (the
/// darkness's fog takes a while to gather), the hero knelt and the camera
/// still.
fn prayer_shown(g: &RenethackGame, outcome: PrayerOutcome) -> Result<bool, String> {
    let now = map_view(g)?.hero_prayer();
    Ok(
        now.is_some_and(|(o, t)| o == outcome && (1.5..2.2).contains(&t))
            && pose_settled(g)?
            && camera_settled(g)?,
    )
}

fn prayer_over(g: &RenethackGame) -> Result<bool, String> {
    Ok(map_view(g)?.hero_prayer().is_none())
}

/// Drink the one potion in the pack.
fn quaff(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    let items = g.world.inventory.items();
    let potion = items.iter().find(|i| i.class == '!').map(|i| i.letter);
    Ok(match (pending(g), potion) {
        (Some(Prompt::Command), None) => None,
        (Some(Prompt::Command), Some(_)) => Some(plain('q')),
        (Some(Prompt::FreeKey { .. }), Some(letter)) => Some(plain(letter)),
        // "Drink from the fountain?"
        (Some(Prompt::Choice { .. }), _) => Some(plain('n')),
        _ => Some(KeyInput::plain(Key::Escape)),
    })
}

/// The spell of helplessness shown down as `kind`, the camera still.
fn down(g: &RenethackGame, kind: Helpless) -> Result<bool, String> {
    let now = map_view(g)?.hero_helpless();
    Ok(now == Some((kind, HelplessPhase::Held)) && camera_settled(g)?)
}

/// Wish for `potion` and drink it: the spell of helplessness it brings
/// shown after it, down (`shown`), up again.
fn drink(potion: &'static str, shown: Check, shot: &'static str) -> Vec<Step> {
    let mut steps = Vec::from(wish(potion));
    steps.extend([
        until("the potion drunk", quaff),
        Step::Wait("the hero down", shown),
        Step::Shot(shot),
        Step::Wait("up again", |g| Ok(map_view(g)?.hero_helpless().is_none())),
    ]);
    steps
}

fn until(what: &'static str, f: Chooser) -> Step {
    Step::KeysUntil(what, f)
}

/// Onto open floor with no trap: one trap to a cell.
fn fresh_floor() -> [Step; 2] {
    [
        Step::KeyFrom("a step onto fresh floor", |g| way_off(g).map(plain)),
        Step::Request("a command on fresh floor", command),
    ]
}

pub(super) fn states() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode for this game", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start_as(choice("archeologist")));
    steps.extend([
        Step::Wait("the hero on the map", |g| {
            fail_on_error_screen(g)?;
            Ok(g.world.map.hero().is_some())
        }),
        Step::Call("closer", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(3.2, 0.6);
            Ok(())
        }),
        // off the stairs: no trap nor digging there
        Step::KeyFrom("a step off the stairs", |g| way_off(g).map(plain)),
        Step::Request("a command after the step", command),
        // a pit dug under the hero
        until("a pit dug down", dig_pit),
        Step::Request("a command in the pit", command),
    ]);
    steps.extend(shown(
        "the hero sunk into the pit drawn",
        |g| {
            let (h, sink, _) = held(g)?;
            Ok(h.is_some_and(|h| matches!(h, Held::Trap(t) if t.is_pit())) && sink > 0.5)
        },
        "state-pit",
    ));
    steps.extend([
        Step::Call("the pit's cell", mark_trap),
        until("out of the pit", struggle),
        Step::Wait("climbed out, at floor height", |g| {
            let (h, sink, _) = held(g)?;
            Ok(h.is_none() && sink == 0.0)
        }),
    ]);
    // a bear trap: the beartrap wished for, set, stepped into
    steps.extend(fresh_floor());
    steps.extend(wish("bear trap"));
    steps.extend([
        until("the bear trap set", arm_bear_trap),
        Step::Call("the bear trap's cell", mark_trap),
        until("held by the bear trap", step_in),
    ]);
    steps.extend(shown(
        "the hero crouched in the jaws",
        |g| {
            let (h, _, clip) = held(g)?;
            Ok(h == Some(Held::Trap(TrapKind::BearTrap)) && clip.is_some())
        },
        "state-bear-trap",
    ));
    steps.extend([
        until("out of the bear trap", struggle),
        Step::Wait("free of the bear trap", |g| Ok(held(g)?.0.is_none())),
    ]);
    // a web wished for, stepped into
    steps.extend(fresh_floor());
    steps.extend(wish("web"));
    steps.extend([
        Step::Call("the web's cell", mark_trap),
        until("stuck in the web", step_in),
    ]);
    steps.extend(shown(
        "the hero wrapped in the web, struggling",
        |g| {
            let (h, _, clip) = held(g)?;
            Ok(h == Some(Held::Trap(TrapKind::Web)) && clip.is_some())
        },
        "state-web",
    ));
    steps.extend([
        until("out of the web", struggle),
        Step::Wait("free of the web", |g| Ok(held(g)?.0.is_none())),
    ]);
    // lava and water, as the engine tells them
    steps.extend([
        Step::Call("sinking into lava, as the hero notice says", |g| {
            told_trap(g, Some("lava"));
            Ok(())
        }),
        Step::Wait("the hero sunk in the lava", |g| {
            let (h, sink, _) = held(g)?;
            Ok(h == Some(Held::Trap(TrapKind::Lava)) && sink > 0.4)
        }),
        Step::Shot("state-lava"),
        Step::Call("out of the lava", |g| {
            told_trap(g, None);
            Ok(())
        }),
        Step::Wait("out of the lava, at floor height", |g| {
            let (h, sink, _) = held(g)?;
            Ok(h.is_none() && sink == 0.0)
        }),
        Step::Call("under water, as the status line says", |g| {
            let mask = condition_mask(g, "Submrg")?;
            let conds = g.world.status.conditions() | mask;
            set_status(g, "condition", "", Some(conds));
            Ok(())
        }),
        Step::Wait("the hero under the water", |g| {
            let (h, sink, _) = held(g)?;
            Ok(h == Some(Held::Water) && sink > 0.9)
        }),
        Step::Shot("state-water"),
        Step::Call("out of the water", |g| {
            let mask = condition_mask(g, "Submrg")?;
            let conds = g.world.status.conditions() & !mask;
            set_status(g, "condition", "", Some(conds));
            Ok(())
        }),
        Step::Wait("out of the water", |g| Ok(held(g)?.0.is_none())),
    ]);
    // a prayer too soon after the start: the god angered
    steps.extend(pray('n'));
    steps.extend([
        Step::Wait("kneeling, darkness round the hero", |g| {
            prayer_shown(g, PrayerOutcome::Angered)
        }),
        Step::Shot("state-pray-angered"),
        Step::Wait("the prayer over, the hero up", prayer_over),
    ]);
    // forced pleased
    steps.extend(pray('y'));
    steps.extend([
        Step::Wait("kneeling in a shaft of light", |g| {
            prayer_shown(g, PrayerOutcome::Pleased)
        }),
        Step::Shot("state-pray-pleased"),
        Step::Wait("the prayer over, the hero up", prayer_over),
    ]);
    // helpless: asleep, passed out drunk (the messages tell that one: the
    // status line shows no Out for it), paralysed
    steps.extend(drink(
        "blessed potion of sleeping",
        |g| down(g, Helpless::Asleep),
        "state-asleep",
    ));
    steps.extend(drink(
        "cursed potion of booze",
        |g| down(g, Helpless::Unconscious),
        "state-out",
    ));
    steps.extend(drink(
        "blessed potion of paralysis",
        |g| down(g, Helpless::Paralyzed),
        "state-paralysed",
    ));
    steps.extend(quit());
    steps
}

/// The client told of the trap the hero is caught in, as the host tells it.
fn told_trap(g: &mut RenethackGame, trap: Option<&str>) {
    g.world.apply(&WinCall::Hero(HeroNotice {
        trap: trap.map(str::to_string),
    }));
}

fn condition_mask(g: &RenethackGame, name: &str) -> Result<u64, String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    cat.conditions
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.mask)
        .ok_or_else(|| format!("no condition {name} in the catalog"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_has_a_way_back() {
        for (_, k) in STEPS {
            assert_eq!(back_of(back_of(k)), k);
            assert_ne!(back_of(k), k);
        }
    }
}
