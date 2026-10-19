//! Counts are the engine's (the critics' pass, vet 1, 7 and 10): a count
//! and its command go to NetHack as tty types them, and NetHack reads the
//! count itself. A samurai drops their ya and picks five of the stack up
//! again: `n5,` with Modern keys, `5,` with Classic ones, whose digits
//! count as in NetHack. With Modern keys `n3` and an arrow are three
//! steps (number_pad's digit would have joined the count). `>` off the
//! stairs is NetHack's own ("You can't go down here."): no walk, unless
//! the player's setting asks for one. With the orders' speed at Instant a
//! click walks three cells at once.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Passage, Prompt, passage};

use super::{Check, Step, command, idle_command, key, letter_of, quit, smoke_choice, start_as};
use godot::prelude::godot_print;

use crate::game::RenethackGame;
use crate::ui_events::{CharacterChoice, UiEvent};

fn samurai(profile: KeyProfile) -> CharacterChoice {
    CharacterChoice {
        role: "samurai".into(),
        align: "lawful".into(),
        profile,
        ..smoke_choice()
    }
}

/// The ya in the pack: how many.
fn ya(g: &RenethackGame) -> Option<i64> {
    g.world
        .inventory
        .items()
        .iter()
        .find(|i| i.text.contains(" ya"))
        .map(|i| i.quan)
}

/// The keys the engine was answered at its command prompt, as text.
fn command_keys(g: &RenethackGame) -> String {
    g.sent
        .iter()
        .filter(|(p, _)| *p == Prompt::Command)
        .filter_map(|(_, r)| match r {
            Reply::Key(k) => u8::try_from(*k).ok().map(char::from),
            _ => None,
        })
        .collect()
}

/// Drop every ya, then pick five up with `count` typed before `,`: five
/// ya in the pack, and `sent` holds for what the engine was sent.
fn pick_five(count: Vec<Step>, sent: Check) -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the ya in the pack", |g| Ok(ya(g).is_some_and(|n| n > 5))),
        key('d'),
        Step::Request(
            "What do you want to drop?",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("drop")),
        ),
        Step::KeyFrom("the ya's letter", |g| {
            let l = letter_of(g, " ya").ok_or("no ya")?;
            Ok(KeyInput::plain(Key::Char(l)))
        }),
        Step::Request("a command after the drop", command),
        Step::Wait("no ya in the pack", |g| {
            Ok(idle_command(g)? && ya(g).is_none())
        }),
        Step::Call("forget what was sent", |g| {
            g.sent.clear();
            Ok(())
        }),
    ];
    steps.extend(count);
    steps.extend([
        key(','),
        Step::Wait("five ya picked up", |g| {
            if !idle_command(g)? {
                return Ok(false);
            }
            match ya(g) {
                None => Ok(false),
                Some(5) => Ok(true),
                Some(n) => Err(format!(
                    "{n} ya picked up, the engine sent {:?}",
                    command_keys(g)
                )),
            }
        }),
        Step::Wait("the count went to the engine", sent),
    ]);
    steps
}

/// The engine was sent `want` at its command prompt since the drop.
fn keys_were(g: &RenethackGame, want: &str) -> Result<bool, String> {
    let sent = command_keys(g);
    if sent == want {
        Ok(true)
    } else {
        Err(format!("the engine was sent {sent:?}, not {want:?}"))
    }
}

/// The arrows in dirchars' order (W NW N NE E SE S SW), and their steps.
const ARROWS: [(Key, (i32, i32)); 8] = [
    (Key::Left, (-1, 0)),
    (Key::Home, (-1, -1)),
    (Key::Up, (0, -1)),
    (Key::PageUp, (1, -1)),
    (Key::Right, (1, 0)),
    (Key::PageDown, (1, 1)),
    (Key::Down, (0, 1)),
    (Key::End, (-1, 1)),
];

/// Where the counted steps should take the hero.
static STEPS_TO: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// The arrow toward three cells of open floor in a row, and the third.
fn open_way(g: &RenethackGame) -> Result<(Key, (i32, i32)), String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let (x, y) = g.world.hero().ok_or("no hero")?;
    ARROWS
        .iter()
        .find_map(|&(k, (dx, dy))| {
            let open = (1..=3).all(|i| {
                g.world
                    .map
                    .cell(x + dx * i, y + dy * i)
                    .is_some_and(|c| c.entity().is_none() && passage(c, cat) == Passage::Open)
            });
            open.then_some((k, (x + dx * 3, y + dy * 3)))
        })
        .ok_or_else(|| format!("no three open cells in a row from {:?}", (x, y)))
}

/// `n3` and an arrow: three steps, the step as number_pad's extended
/// command after the count.
fn three_steps() -> Vec<Step> {
    vec![
        Step::Wait("the player's turn", idle_command),
        Step::Call("forget what was sent", |g| {
            g.sent.clear();
            Ok(())
        }),
        key('n'),
        key('3'),
        Step::KeyFrom("an arrow toward open floor", |g| {
            let (k, to) = open_way(g)?;
            *STEPS_TO.lock().map_err(|e| e.to_string())? = Some(to);
            Ok(KeyInput::plain(k))
        }),
        Step::Wait("three steps by the engine", |g| {
            let to = *STEPS_TO.lock().map_err(|e| e.to_string())?;
            Ok(idle_command(g)? && g.world.hero() == to)
        }),
        Step::Wait("the count, then the step as #move", |g| {
            let ext = g.sent.iter().find_map(|(_, r)| match r {
                Reply::ExtCmd(Some(name)) => Some(name.clone()),
                _ => None,
            });
            match (command_keys(g).as_str(), ext.as_deref()) {
                ("n3#", Some(m)) if m.starts_with("move") => Ok(true),
                (keys, ext) => Err(format!("the engine was sent {keys:?}, then {ext:?}")),
            }
        }),
    ]
}

/// Open floor three to six steps' walk from the hero.
fn walk_target(g: &RenethackGame) -> Result<(i32, i32), String> {
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let hero = g.world.hero().ok_or("no hero")?;
    (-6..=6)
        .flat_map(|dy| (-6..=6).map(move |dx| (hero.0 + dx, hero.1 + dy)))
        .filter(|&(x, y)| {
            g.world
                .map
                .cell(x, y)
                .is_some_and(|c| c.entity().is_none() && passage(c, cat) == Passage::Open)
        })
        .find(|&to| {
            nh_world::find_path(&g.world.map, cat, hero, to, nh_world::Reach::Onto)
                .is_some_and(|way| (3..=6).contains(&way.len()))
        })
        .ok_or_else(|| format!("no floor three to six steps from {hero:?}"))
}

/// When the instant walk was clicked.
static CLICKED: Mutex<Option<Instant>> = Mutex::new(None);

/// Order speed Instant: a click three to six steps off walks there with
/// no tick between the steps.
fn instant_walk() -> Vec<Step> {
    vec![
        Step::Wait("the player's turn", idle_command),
        Step::Call("the orders' speed at Instant", |g| {
            let mut s = g.settings.clone();
            s.order_speed = nh_world::OrderSpeed::Instant;
            g.set_settings(s);
            if g.driver.tick() != Duration::ZERO {
                return Err(format!("the tick is {:?}", g.driver.tick()));
            }
            let to = walk_target(g)?;
            *STEPS_TO.lock().map_err(|e| e.to_string())? = Some(to);
            *CLICKED.lock().map_err(|e| e.to_string())? = Some(Instant::now());
            g.push_ui(UiEvent::MapClick {
                x: to.0,
                y: to.1,
                button: 1,
            });
            Ok(())
        }),
        Step::Wait("there at once", |g| {
            let to = *STEPS_TO.lock().map_err(|e| e.to_string())?;
            if !(idle_command(g)? && g.world.hero() == to) {
                return Ok(false);
            }
            let took = CLICKED
                .lock()
                .map_err(|e| e.to_string())?
                .map(|t| t.elapsed())
                .unwrap_or_default();
            godot_print!("selftest: counts: a walk at Instant in {took:?}");
            // three ticks of Normal, at the least, would be 0.9 s
            if took >= Duration::from_millis(800) {
                return Err(format!("three steps took {took:?}"));
            }
            Ok(true)
        }),
        Step::Call("the orders' speed back at Normal", |g| {
            let mut s = g.settings.clone();
            s.order_speed = nh_world::OrderSpeed::Normal;
            g.set_settings(s);
            Ok(())
        }),
    ]
}

/// `>` off the stairs: NetHack's answer, the hero where they stood.
fn not_down_here() -> Vec<Step> {
    vec![
        Step::Wait("the player's turn, off the stairs", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let (x, y) = g.world.hero().ok_or("no hero")?;
            let on = g
                .world
                .map
                .cell(x, y)
                .and_then(|c| nh_world::cell_terrain(c, cat));
            if on == Some(nh_world::Terrain::StairsDown) {
                return Err("the hero stands on the stairs down".into());
            }
            Ok(idle_command(g)? && !g.settings.walk_to_stairs)
        }),
        Step::Call("forget what was sent", |g| {
            g.sent.clear();
            *STEPS_TO.lock().map_err(|e| e.to_string())? = g.world.hero();
            Ok(())
        }),
        key('>'),
        Step::Wait("NetHack's answer", |g| {
            let said = g
                .world
                .log
                .iter()
                .rev()
                .take(3)
                .any(|m| m.text.starts_with("You can't go down here"));
            Ok(idle_command(g)? && said)
        }),
        Step::Wait("no walk, '>' sent as it is", |g| {
            let at = *STEPS_TO.lock().map_err(|e| e.to_string())?;
            if g.world.hero() != at || command_keys(g) != ">" || g.order_actions != 0 {
                return Err(format!(
                    "the hero at {:?} (from {at:?}), sent {:?}, {} order actions",
                    g.world.hero(),
                    command_keys(g),
                    g.order_actions
                ));
            }
            Ok(true)
        }),
    ]
}

pub(super) fn counts() -> Vec<Step> {
    let mut steps = start_as(samurai(KeyProfile::Modern));
    steps.extend(pick_five(vec![key('n'), key('5')], |g| keys_were(g, "n5,")));
    steps.extend(three_steps());
    steps.extend(not_down_here());
    steps.extend(instant_walk());
    steps.extend(quit());
    steps.extend([
        Step::Push(UiEvent::BackToTitle),
        Step::Wait(
            "the title screen",
            |g| Ok(super::screen(g) == Some("title")),
        ),
        Step::Push(UiEvent::StartCharacter(samurai(KeyProfile::Classic))),
        Step::Request("the first command (Classic)", command),
    ]);
    steps.extend(pick_five(vec![key('5')], |g| keys_were(g, "5,")));
    steps.extend(quit());
    steps
}
