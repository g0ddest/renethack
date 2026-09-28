//! Key profiles and item macros on real engine output: these tests start
//! nh-engine. Build it first with `make -C engine` (or set
//! RENETHACK_ENGINE_DIR).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{EngineMsg, InvItem, Reply, Slot};
use nh_world::*;

const SEED: u64 = 42;
const NEW_MOON: i64 = 1_768_694_400;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

/// A new character with the client's options and a key profile.
fn config(playground: &Path, role: &str, profile: KeyProfile) -> EngineConfig {
    let dir = engine_dir();
    let engine = dir.join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    create_playground(&dir.join("data"), playground).expect("playground");
    let options =
        EngineConfig::character_options("Hero", role, "human", "female", "lawful").unwrap();
    EngineConfig {
        engine,
        playground: playground.to_path_buf(),
        options: format!(
            "{options},{CLIENT_EXTRA_OPTIONS},{}",
            profile.engine_option()
        ),
        seed: Some(SEED),
        fixed_time: Some(NEW_MOON),
    }
}

/// What a test does at a prompt: answer it, or quit the game.
enum Act {
    Reply(Reply),
    Quit,
}

/// Play by engine lines; read-only displays are acknowledged. `decide`
/// sees every other prompt until it says Quit; then the game is quit
/// (yes to "Really quit?", no to the rest). Returns the final world.
fn play(role: &str, profile: KeyProfile, decide: &mut dyn FnMut(&World, &Prompt) -> Act) -> World {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), role, profile)).unwrap();
    let mut world = World::new();
    let mut errors = Vec::new();
    let mut quit: Option<VecDeque<Reply>> = None;
    while let Some(inc) = engine.recv(Duration::from_secs(10)).unwrap() {
        match inc.msg {
            EngineMsg::Catalog(c) => world.set_catalog(&c),
            EngineMsg::Win(call) => world.apply(&call),
            EngineMsg::Req { id, req } => {
                let prompt = world.on_request(&req);
                let reply = match (&prompt, quit.as_mut()) {
                    (Prompt::AutoAck | Prompt::Show { .. } | Prompt::MapPause, _) => Reply::Ack,
                    (Prompt::Choice { .. }, Some(q)) => {
                        q.pop_front().unwrap_or(Reply::Char('n' as i32))
                    }
                    (p, Some(_)) => p.escape_reply(),
                    (p, None) => match decide(&world, p) {
                        Act::Reply(r) => r,
                        Act::Quit => {
                            assert_eq!(*p, Prompt::Command, "quit at a command prompt");
                            quit = Some(VecDeque::new());
                            Reply::Key('#' as i32)
                        }
                    },
                };
                let reply = match (&prompt, &quit) {
                    (Prompt::ExtCmd, Some(_)) => {
                        quit = Some([Reply::Char('y' as i32)].into());
                        Reply::ExtCmd(Some("quit".into()))
                    }
                    _ => reply,
                };
                engine.reply(id, &reply.to_value()).unwrap();
            }
            EngineMsg::Error { msg } => errors.push(msg),
            _ => {}
        }
    }
    let status = engine.wait(Duration::from_secs(10)).unwrap();
    assert!(status.success() && errors.is_empty(), "{errors:?}");
    world
}

/// Keys typed at the command prompt, through the profile's key map and
/// count, to the keys the engine gets.
fn typed(keys: &[KeyInput], profile: KeyProfile, dirchars: &str) -> VecDeque<i32> {
    let mut count = CountEntry::default();
    let mut out = VecDeque::new();
    for k in keys {
        match count.feed_key(k, profile, dirchars) {
            CommandInput::Typing => {}
            CommandInput::Command { key, count } => {
                out.extend(
                    count
                        .map(|n| count_keys(n, profile.number_pad()))
                        .unwrap_or_default(),
                );
                out.push_back(key);
            }
            other => panic!("{k:?}: {other:?}"),
        }
    }
    out
}

/// A count of 20 before `s` searches 20 turns, typed as each profile types
/// it; the engine reads the count itself.
fn twenty_searches(profile: KeyProfile, keys: &[KeyInput]) {
    let mut start = None;
    let mut sent: Option<VecDeque<i32>> = None;
    let mut turns = None;
    play("valkyrie", profile, &mut |w, p| {
        assert_eq!(*p, Prompt::Command, "{p:?}");
        assert_eq!(w.number_pad, profile.number_pad());
        let queue = sent.get_or_insert_with(|| {
            start = w.status.number("time");
            typed(keys, profile, &w.dirchars)
        });
        match queue.pop_front() {
            Some(k) => Act::Reply(Reply::Key(k)),
            None => {
                turns = w
                    .status
                    .number("time")
                    .zip(start)
                    .map(|(now, then)| now - then);
                Act::Quit
            }
        }
    });
    assert_eq!(turns, Some(20), "{profile:?}");
}

#[test]
fn modern_counts_after_n() {
    let keys: Vec<KeyInput> = "n20s"
        .chars()
        .map(|c| KeyInput::plain(Key::Char(c)))
        .collect();
    twenty_searches(KeyProfile::Modern, &keys);
}

#[test]
fn classic_counts_with_alt_digits() {
    let alt = |c| KeyInput {
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
        ..KeyInput::plain(Key::Char(c))
    };
    let keys = [alt('2'), alt('0'), KeyInput::plain(Key::Char('s'))];
    twenty_searches(KeyProfile::Classic, &keys);
}

/// Run macros one after another, each started at a command prompt; `check`
/// sees the world at the command prompt after each one ends.
fn run_macros(
    role: &str,
    make: &mut dyn FnMut(usize, &World) -> Option<Macro>,
    check: &mut dyn FnMut(usize, &World, &MacroRunner),
) -> Vec<(String, MenuKind)> {
    let mut runner = MacroRunner::new();
    let mut n = 0;
    let mut menus = Vec::new();
    play(role, KeyProfile::Modern, &mut |w, p| {
        if let Prompt::Menu { items, title, .. } = p {
            menus.push((
                title.clone().unwrap_or_default(),
                menu_kind(items, &w.inventory),
            ));
        }
        if runner.is_active() {
            match runner.on_prompt(p, w) {
                MacroStep::Reply(r) => return Act::Reply(r),
                MacroStep::Done => {
                    check(n, w, &runner);
                    n += 1;
                }
                other => panic!("macro {n}: {other:?} at {p:?}"),
            }
        }
        assert_eq!(*p, Prompt::Command, "{p:?}");
        match make(n, w) {
            Some(m) => Act::Reply(runner.start(m).expect("a command key first")),
            None => Act::Quit,
        }
    });
    menus
}

fn letter_of(w: &World, class: char) -> InvItem {
    w.inventory
        .items()
        .iter()
        .find(|i| i.class == class)
        .cloned()
        .unwrap_or_else(|| panic!("no {class} in {:?}", w.inventory.items()))
}

#[test]
fn macros_wield_drop_and_find_nothing_to_drink() {
    let mut log_at_quaff = Vec::new();
    run_macros(
        "valkyrie",
        &mut |n, w| {
            let pack = &w.inventory;
            match n {
                // the dagger to the hand; pushweapon makes the spear the alternate
                0 => Macro::item(ItemActionKind::Wield, pack.by_letter('b')?, None),
                1 => Macro::item(ItemActionKind::Drop, &letter_of(w, '%'), None),
                // no potion in the pack: the engine says so
                2 => {
                    let potion = InvItem {
                        letter: 'f',
                        class: '!',
                        tile: 0,
                        quan: 1,
                        slots: vec![],
                        lit: false,
                        text: "a bubbly potion".into(),
                    };
                    Macro::item(ItemActionKind::Quaff, &potion, None)
                }
                _ => None,
            }
        },
        &mut |n, w, runner| {
            let pack = &w.inventory;
            match n {
                0 => {
                    assert!(runner.completed());
                    assert_eq!(pack.wielded().unwrap().letter, 'b');
                    let alt = pack.in_slot(&Slot::Alternate).unwrap();
                    assert_eq!(alt.letter, 'a', "pushweapon is on");
                    assert!(pack.by_letter('b').unwrap().text.contains("(weapon in"));
                }
                1 => {
                    assert!(runner.completed());
                    assert!(pack.items().iter().all(|i| i.class != '%'), "dropped");
                }
                2 => {
                    assert!(!runner.completed());
                    log_at_quaff = w.log.iter().map(|m| m.text.clone()).collect();
                }
                _ => unreachable!(),
            }
        },
    );
    assert!(
        log_at_quaff
            .iter()
            .any(|m| m.contains("You don't have anything to drink")),
        "{log_at_quaff:?}"
    );
}

#[test]
fn macros_take_part_of_a_stack_and_drop_several() {
    let quiver = std::cell::RefCell::new(None);
    let menus = run_macros(
        "samurai",
        &mut |n, w| {
            let pack = &w.inventory;
            match n {
                0 => {
                    let ya = pack
                        .in_slot(&Slot::Quiver)
                        .expect("ya in the quiver")
                        .clone();
                    assert!(ya.quan > 5, "{ya:?}");
                    *quiver.borrow_mut() = Some(ya.clone());
                    Macro::item(ItemActionKind::Drop, &ya, Some(5))
                }
                1 => {
                    let mut all: Vec<(char, Option<u32>)> = pack
                        .items()
                        .iter()
                        .filter(|i| i.slots.is_empty())
                        .map(|i| (i.letter, None))
                        .collect();
                    all.truncate(2);
                    assert!(!all.is_empty());
                    Some(Macro::drop_many(&all))
                }
                _ => None,
            }
        },
        &mut |n, w, runner| {
            assert!(runner.completed(), "macro {n}");
            let pack = &w.inventory;
            if n == 0 {
                let before = quiver.borrow().clone().unwrap();
                let now = pack.by_letter(before.letter).unwrap();
                assert_eq!(now.quan, before.quan - 5);
            }
        },
    );
    // D's second menu lists pack items: the inventory panel shows it
    let kinds: Vec<(&str, MenuKind)> = menus.iter().map(|(t, k)| (t.as_str(), *k)).collect();
    assert_eq!(
        kinds,
        [
            ("Drop what type of items?", MenuKind::Generic),
            ("What would you like to drop?", MenuKind::Inventory)
        ]
    );
}
