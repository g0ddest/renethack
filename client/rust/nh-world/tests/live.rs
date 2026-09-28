//! The world model on real engine output: these tests start nh-engine.
//! Build it first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{Catalog, EngineMsg, PickHow, Reply, Slot, parse_line};
use nh_world::*;

const SEED: u64 = 42;
const NEW_MOON: i64 = 1_768_694_400;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

fn config(playground: &Path) -> EngineConfig {
    let dir = engine_dir();
    let engine = dir.join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    create_playground(&dir.join("data"), playground).expect("playground");
    let options =
        EngineConfig::character_options("Hero", "valkyrie", "human", "female", "neutral").unwrap();
    EngineConfig {
        engine,
        playground: playground.to_path_buf(),
        // the interactive client asks for experience points too
        options: format!("{options},showexp"),
        seed: Some(SEED),
        fixed_time: Some(NEW_MOON),
    }
}

/// The world as it stood at one request.
struct Seen {
    prompt: Prompt,
    world: World,
}

/// Feed a transcript through `parse_line` into a World, as the client would.
fn replay(lines: &[String]) -> (Catalog, Vec<Seen>, World) {
    let mut world = World::new();
    let mut catalog = None;
    let mut seen = Vec::new();
    for line in lines {
        match parse_line(line).unwrap_or_else(|e| panic!("{e}: {line}")) {
            EngineMsg::Catalog(c) => {
                world.set_catalog(&c);
                catalog = Some(*c);
            }
            EngineMsg::Win(call) => world.apply(&call),
            EngineMsg::Req { req, .. } => {
                let prompt = world.on_request(&req);
                seen.push(Seen {
                    prompt,
                    world: world.clone(),
                });
            }
            _ => {}
        }
    }
    (catalog.expect("catalog"), seen, world)
}

fn terrain_at(world: &World, catalog: &Catalog, x: i32, y: i32) -> Option<Terrain> {
    cell_terrain(world.map.cell(x, y)?, catalog)
}

#[test]
fn a_scripted_game_builds_the_world() {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path())).unwrap();
    let script = "key i\ncancel\nkey #\next quit\nyn y\nyn n\nyn n\nyn n\nyn n\n";
    let mut responder = ScriptResponder::new(parse_script(script).unwrap());
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default()).unwrap();
    assert!(t.said_bye && t.exit.unwrap().success(), "{:?}", t.errors);
    let (catalog, seen, end) = replay(&t.lines);

    // the first command prompt: the hero in a lit room, the status line, the welcome
    let start = &seen[0];
    assert_eq!(start.prompt, Prompt::Command);
    let w = &start.world;
    let (hx, hy) = w.map.hero().expect("hero on the map");
    assert_eq!(w.cursor, Some((hx, hy)));
    for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1)] {
        assert_eq!(
            terrain_at(w, &catalog, hx + dx, hy + dy),
            Some(Terrain::Floor),
            "next to the hero at {dx},{dy}"
        );
    }
    let mut kinds = std::collections::HashSet::new();
    for y in 0..ROWNO {
        for x in 1..COLNO {
            kinds.extend(terrain_at(w, &catalog, x, y));
        }
    }
    for t in [Terrain::Floor, Terrain::Wall, Terrain::Doorway] {
        assert!(kinds.contains(&t), "{t:?} not in {kinds:?}");
    }
    // the pet stands next to the hero, and the hover names it
    let pet = (hx + 1, hy + 1);
    let text = describe_cell(w.map.cell(pet.0, pet.1).unwrap(), &catalog).unwrap();
    assert!(text.starts_with("tame little dog"), "{text}");
    assert_eq!(w.status.number("hp"), Some(16));
    assert_eq!(w.status.number("hpmax"), Some(16));
    assert_eq!(w.status.get("leveldesc"), Some("Dlvl:1"));
    assert_eq!(w.status.get("title"), Some("Hero the Stripling"));
    assert_eq!(w.status.get("exp"), Some("0"));
    assert_eq!(w.status.number("gold"), Some(0));
    let log: Vec<_> = w.log.iter().map(|m| m.text.as_str()).collect();
    assert!(
        log.iter().any(|m| m.contains("welcome to NetHack")),
        "{log:?}"
    );
    assert!(w.log.iter().all(|m| m.turn == Some(1)));
    assert_eq!(w.dirchars, "hykulnjb><");
    assert!(!w.number_pad);

    // 'i': the inventory, a pick-one menu lettered a to e
    let Prompt::Menu {
        how, title, items, ..
    } = &seen[1].prompt
    else {
        panic!("{:?}", seen[1].prompt);
    };
    assert_eq!(*how, PickHow::One);
    let menu = MenuState::new(*how, title.clone(), items);
    let letters: String = menu.entries.iter().filter_map(|e| e.letter).collect();
    assert_eq!(letters, "abcde");

    // #quit, "n" to every question: the summary window, then the top ten
    let prompts: Vec<_> = seen.iter().map(|s| &s.prompt).collect();
    assert!(prompts.contains(&&Prompt::ExtCmd));
    let summary = seen
        .iter()
        .find_map(|s| match &s.prompt {
            Prompt::Show { lines, .. } => Some(lines),
            _ => None,
        })
        .expect("a summary window");
    assert!(
        summary.iter().any(|l| l.text.contains("You quit")),
        "{summary:?}"
    );
    assert!(end.last_text.iter().any(|l| l.text.contains("You quit")));
    assert!(end.windows_exited);
    assert!(
        end.raw_lines.iter().any(|l| l.contains("Points")),
        "{:?}",
        end.raw_lines
    );
    assert!(end.windows.is_empty());
}

#[test]
fn a_new_game_has_its_inventory_at_the_first_command() {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path())).unwrap();
    let script = "key #\next quit\nyn y\nyn n\nyn n\nyn n\nyn n\n";
    let mut responder = ScriptResponder::new(parse_script(script).unwrap());
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default()).unwrap();
    assert!(t.said_bye && t.exit.unwrap().success(), "{:?}", t.errors);
    let (_, seen, _) = replay(&t.lines);

    let start = &seen[0];
    assert_eq!(start.prompt, Prompt::Command);
    let pack = &start.world.inventory;
    assert!(pack.received());
    let letters: String = pack.items().iter().map(|i| i.letter).collect();
    assert_eq!(letters, "abcde");
    let weapon = pack.wielded().expect("a wielded weapon");
    assert_eq!(weapon.class, ')');
    assert_eq!(ItemKey::of(weapon).stem, "spear");
    let name = parse_item_name(&weapon.text);
    assert_eq!(name.enchantment, Some(1));
    assert_eq!(pack.offhand().expect("a shield").letter, 'c');
    assert_eq!(pack.in_slot(&Slot::Alternate).unwrap().letter, 'b');
    assert!(!pack.twoweap());
}

/// Play by reading engine lines one at a time, answering each prompt with
/// `decide`; read-only displays are acknowledged. Returns the final world.
fn play(decide: &mut dyn FnMut(&mut World, &Prompt) -> Reply) -> World {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path())).unwrap();
    let mut world = World::new();
    let mut errors = Vec::new();
    while let Some(inc) = engine.recv(Duration::from_secs(10)).unwrap() {
        match inc.msg {
            EngineMsg::Catalog(c) => world.set_catalog(&c),
            EngineMsg::Win(call) => world.apply(&call),
            EngineMsg::Req { id, req } => {
                let prompt = world.on_request(&req);
                let reply = match prompt {
                    Prompt::AutoAck | Prompt::Show { .. } | Prompt::MapPause => Reply::Ack,
                    _ => decide(&mut world, &prompt),
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

fn key(world: &World, k: Key, ctx: KeyContext) -> Option<i32> {
    nethack_key(&KeyInput::plain(k), ctx, world.number_pad, &world.dirchars)
}

#[test]
fn keys_and_menus_answer_the_engine() {
    let mut step = 0;
    let mut hero_after_move = None;
    let world = play(&mut |w, prompt| {
        step += 1;
        match (step, prompt) {
            // the right arrow walks east
            (1, Prompt::Command) => Reply::Key(key(w, Key::Right, KeyContext::Command).unwrap()),
            (2, Prompt::Command) => {
                hero_after_move = w.map.hero();
                Reply::Key('D' as i32)
            }
            // D: select all item types; the skipinvert "Auto-select" and
            // "All types" stay off, or everything would be dropped at once
            (
                3,
                Prompt::Menu {
                    how, title, items, ..
                },
            ) => {
                let mut m = MenuState::new(*how, title.clone(), items);
                assert_eq!(m.key('.'), MenuOutcome::Pending);
                assert!(m.entries.iter().any(|e| e.skipinvert && !e.selected));
                match m.key('\n') {
                    MenuOutcome::Done(r) => r,
                    other => panic!("{other:?}"),
                }
            }
            // then the food ration by its letter
            (
                4,
                Prompt::Menu {
                    how, title, items, ..
                },
            ) => {
                let mut m = MenuState::new(*how, title.clone(), items);
                let food = m
                    .entries
                    .iter()
                    .find(|e| e.text.contains("food ration"))
                    .and_then(|e| e.letter)
                    .expect("food in the menu");
                m.key(food);
                m.confirm()
            }
            (5, Prompt::Command) => Reply::Key('w' as i32),
            // getobj: an arrow is not an item letter; '-' is bare hands
            (
                6,
                Prompt::FreeKey {
                    directions: false, ..
                },
            ) => {
                assert_eq!(key(w, Key::Down, KeyContext::Letters), None);
                Reply::Char(key(w, Key::Char('-'), KeyContext::Letters).unwrap())
            }
            (7, Prompt::Command) => Reply::Key('#' as i32),
            (8, Prompt::ExtCmd) => Reply::ExtCmd(Some("quit".into())),
            (9, Prompt::Choice { default, .. }) => {
                assert_eq!(*default, Some('n'));
                Reply::Char('y' as i32)
            }
            (_, Prompt::Choice { allowed, .. }) if allowed.contains(&'n') => {
                Reply::Char('n' as i32)
            }
            (n, p) => panic!("step {n}: unexpected {p:?}"),
        }
    });
    assert_eq!(hero_after_move, Some((19, 5)));
    let log: Vec<_> = world.log.iter().map(|m| m.text.as_str()).collect();
    assert!(
        log.iter()
            .any(|m| m.contains("You drop an uncursed food ration")),
        "{log:?}"
    );
    assert!(log.iter().any(|m| m.contains("bare handed")), "{log:?}");
    assert!(world.last_text.iter().any(|l| l.text.contains("You quit")));
}

/// Orders on a real game (seed 42), as the client runs them: a click walks
/// to the far corner of the start room one step per command request, a
/// click on the west doorway walks there, `20j` walks down the corridor
/// until it ends, and `>` walks to the stairs it found and goes down.
#[test]
fn orders_walk_the_real_map_and_take_the_stairs() {
    use std::collections::VecDeque;

    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path())).unwrap();
    let mut world = World::new();
    let mut catalog: Option<Catalog> = None;
    let mut driver = TickDriver::new();
    let mut plan: VecDeque<&str> = [
        "click 14 7",
        "click 13 3",
        "key h",
        "count 20 j",
        "key >",
        "quit",
    ]
    .into();
    // what the plan said, actions sent, how the order ended, the hero then
    type Done<'a> = (&'a str, u32, Option<Stop>, Option<(i32, i32)>);
    let mut done: Vec<Done> = Vec::new();
    let mut current: Option<(&str, u32)> = None;
    let mut quitting = false;
    while let Some(inc) = engine.recv(Duration::from_secs(10)).unwrap() {
        let (id, req) = match inc.msg {
            EngineMsg::Catalog(c) => {
                world.set_catalog(&c);
                catalog = Some(*c);
                continue;
            }
            EngineMsg::Win(call) => {
                world.apply(&call);
                continue;
            }
            EngineMsg::Req { id, req } => (id, req),
            _ => continue,
        };
        let cat = catalog.as_ref().expect("the catalog comes first");
        let prompt = world.on_request(&req);
        let reply = |r: Reply| r.to_value();
        if let Some(c) = driver.follow_up(&prompt) {
            let r = match prompt {
                Prompt::Command => Reply::Key(c as i32),
                _ => Reply::Char(c as i32),
            };
            engine.reply(id, &reply(r)).unwrap();
            continue;
        }
        let answer = match &prompt {
            Prompt::AutoAck | Prompt::Show { .. } | Prompt::MapPause => Reply::Ack,
            Prompt::ExtCmd => Reply::ExtCmd(Some("quit".into())),
            Prompt::Choice { query, .. } if quitting => {
                Reply::Char(if query.contains("quit") { 'y' } else { 'n' } as i32)
            }
            Prompt::Command if !world.getpos => {
                driver.observe(&world, cat);
                if driver.is_active()
                    && let Some(a) = driver.next(&world, cat)
                {
                    if let Some((_, n)) = current.as_mut() {
                        *n += 1;
                    }
                    engine
                        .reply(id, &reply(Reply::Key(a.keys[0] as i32)))
                        .unwrap();
                    continue;
                }
                if let Some((what, n)) = current.take() {
                    done.push((what, n, driver.take_stop(), world.hero()));
                }
                let step = plan.pop_front().expect("the plan ends with quit");
                let words: Vec<&str> = step.split(' ').collect();
                let order = match words[..] {
                    ["click", x, y] => {
                        let at = (x.parse().unwrap(), y.parse().unwrap());
                        match click_order(&world, cat, at) {
                            ClickPlan::Order(o) => Some(o),
                            other => panic!("{step}: {other:?}"),
                        }
                    }
                    ["count", n, k] => Some(Order::Repeat {
                        key: k.chars().next().unwrap(),
                        left: n.parse().unwrap(),
                    }),
                    ["key", ">"] => {
                        Some(stairs_order(&world, cat, '>').expect("the stairs are known"))
                    }
                    _ => None,
                };
                match (order, &words[..]) {
                    (Some(o), _) => {
                        let a = driver
                            .start(o, &world, cat)
                            .unwrap_or_else(|e| panic!("{step}: {e:?}"));
                        current = Some((step, 1));
                        Reply::Key(a.keys[0] as i32)
                    }
                    (None, &["key", k]) => Reply::Key(k.chars().next().unwrap() as i32),
                    (None, &["quit"]) => {
                        quitting = true;
                        Reply::Key('#' as i32)
                    }
                    _ => panic!("{step}"),
                }
            }
            other => {
                driver.on_question(None, cat);
                other.escape_reply()
            }
        };
        engine.reply(id, &reply(answer)).unwrap();
    }
    engine.wait(Duration::from_secs(10)).unwrap();
    let find = |what: &str| {
        done.iter()
            .find(|d| d.0 == what)
            .unwrap_or_else(|| panic!("{what}: {done:?}"))
    };
    // four steps across the room, one action each
    let walk = find("click 14 7");
    assert_eq!(
        (walk.2.clone(), walk.3),
        (Some(Stop::Arrived), Some((14, 7))),
        "{done:?}"
    );
    assert!(walk.1 <= 5, "{done:?}");
    assert_eq!(find("click 13 3").3, Some((13, 3)), "{done:?}");
    // the corridor ends where it turns: the step goes nowhere
    let corridor = find("count 20 j");
    assert!(
        corridor.1 >= 10 && corridor.3.is_some_and(|h| h.1 >= 15),
        "{done:?}"
    );
    let stairs = find("key >");
    assert_eq!(stairs.2, Some(Stop::Arrived), "{done:?}");
    assert!(stairs.1 >= 2, "walked, then '>': {done:?}");
}
