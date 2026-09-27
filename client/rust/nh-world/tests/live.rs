//! The world model on real engine output: these tests start nh-engine.
//! Build it first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{Catalog, EngineMsg, PickHow, Reply, parse_line};
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
