//! The Gameplay section's options on the real engine: a new game starts
//! with the ones it is given, the full options menu reads and changes them
//! as a player would (the engine's own `O` shows the change), and a saved
//! game keeps its own over the options it is restored with. These tests
//! start nh-engine: build it first with `make -C engine` (or set
//! RENETHACK_ENGINE_DIR).

use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{EngineMsg, Reply};
use nh_world::*;

const NEW_MOON: i64 = 1_768_694_400;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

/// What a session does at a prompt: answer it, or end the game.
enum Act {
    Reply(Reply),
    Save,
    Quit,
}

/// One engine process in the playground `pg` started with `options`,
/// played by `decide` until it saves or quits. Read-only displays are
/// acknowledged.
fn session(pg: &Path, options: &str, decide: &mut dyn FnMut(&World, &Prompt) -> Act) -> World {
    let dir = engine_dir();
    let engine = dir.join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    let cfg = EngineConfig {
        engine,
        playground: pg.to_path_buf(),
        options: options.to_string(),
        seed: Some(42),
        fixed_time: Some(NEW_MOON),
    };
    let mut engine = Engine::spawn(&cfg).unwrap();
    let mut world = World::new();
    let mut ending: Option<Act> = None;
    let mut said_yes = false;
    let mut errors = Vec::new();
    while let Some(inc) = engine.recv(Duration::from_secs(10)).unwrap() {
        match inc.msg {
            EngineMsg::Catalog(c) => world.set_catalog(&c),
            EngineMsg::Win(call) => world.apply(&call),
            EngineMsg::Req { id, req } => {
                let prompt = world.on_request(&req);
                let reply = match (&prompt, &ending) {
                    (Prompt::AutoAck | Prompt::Show { .. } | Prompt::MapPause, _) => Reply::Ack,
                    (Prompt::ExtCmd, Some(Act::Quit)) => Reply::ExtCmd(Some("quit".into())),
                    // "Really quit?", "Really save?"; then no to the rest
                    (Prompt::Choice { .. }, Some(_)) => {
                        let c = if said_yes { 'n' } else { 'y' };
                        said_yes = true;
                        Reply::Char(c as i32)
                    }
                    // paranoid_confirmation:quit asks for the word
                    (Prompt::Text { query, .. }, Some(_)) if query.starts_with("Really") => {
                        said_yes = true;
                        Reply::Text("yes".into())
                    }
                    (p, Some(_)) => p.escape_reply(),
                    (p, None) => match decide(&world, p) {
                        Act::Reply(r) => r,
                        Act::Save => {
                            ending = Some(Act::Save);
                            Reply::Key('S' as i32)
                        }
                        Act::Quit => {
                            ending = Some(Act::Quit);
                            Reply::Key('#' as i32)
                        }
                    },
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

/// A run's answer, or None when it is over (`read` has what it read).
fn run_on(
    run: &mut Option<OptionsRun>,
    read: &mut Vec<Option<Gameplay>>,
    p: &Prompt,
) -> Option<Act> {
    let r = run.as_mut()?;
    match r.on_prompt(p) {
        RunStep::Reply(reply) => Some(Act::Reply(reply)),
        RunStep::Done(g) => {
            read.push(g);
            *run = None;
            None
        }
    }
}

#[test]
fn the_options_start_a_game_change_through_its_menu_and_stay_in_its_save() {
    let pg = tempfile::tempdir().unwrap();
    create_playground(&engine_dir().join("data"), pg.path()).expect("playground");
    let start = Gameplay {
        pickup_types: "$?".into(),
        pickup_burden: Burden::Burdened,
        autoquiver: true,
        paranoid_remove: true,
        paranoid_pray: false,
        ..Gameplay::default()
    };
    let goal = Gameplay {
        autopickup: true,
        pickup_types: String::new(),
        pickup_burden: Burden::Overtaxed,
        verbose: false,
        paranoid_pray: true,
        paranoid_quit: true,
        ..start.clone()
    };
    let hero =
        EngineConfig::character_options("Hero", "valkyrie", "human", "female", "neutral").unwrap();
    // the section's options lead: the engine reads the list right to left
    let options = format!("{},{hero},{CLIENT_EXTRA_OPTIONS}", start.engine_options());
    let mut run: Option<OptionsRun> = None;
    let mut read = Vec::new();
    let mut simple: Option<String> = None;
    let mut asked_o = false;
    let world = session(pg.path(), &options, &mut |_, p| {
        if let Some(act) = run_on(&mut run, &mut read, p) {
            return act;
        }
        // the engine's own O: its line for autopickup
        if let Prompt::Menu { title, items, .. } = p
            && title.as_deref() == Some("Options")
        {
            simple = items
                .iter()
                .filter_map(|i| i.str.clone())
                .find(|t| t.starts_with("autopickup "));
            return Act::Reply(Reply::Cancel);
        }
        assert_eq!(*p, Prompt::Command, "an unexpected request");
        let (next, reply) = match read.len() {
            0 => OptionsRun::read(),
            1 => OptionsRun::change(goal.clone()),
            _ if !asked_o => {
                asked_o = true;
                return Act::Reply(Reply::Key('O' as i32));
            }
            _ => return Act::Save,
        };
        run = Some(next);
        Act::Reply(reply)
    });
    assert_eq!(read, vec![Some(start), Some(goal.clone())]);
    let simple = simple.expect("the engine's O shows autopickup");
    assert!(simple.ends_with("[X]"), "{simple:?}");
    // the engine said what it toggled (the client keeps it from its log)
    assert!(
        world.log.iter().any(|m| is_toggle_note(&m.text)),
        "{:?}",
        world.log.iter().map(|m| &m.text).collect::<Vec<_>>()
    );

    // restored with the defaults given: the save's own values
    let options = format!(
        "{},{}",
        Gameplay::default().engine_options(),
        EngineConfig::restore_options("Hero").unwrap()
    );
    let mut run: Option<OptionsRun> = None;
    let mut read = Vec::new();
    session(pg.path(), &options, &mut |_, p| {
        if let Some(act) = run_on(&mut run, &mut read, p) {
            return act;
        }
        assert_eq!(*p, Prompt::Command, "an unexpected request");
        if read.is_empty() {
            let (next, reply) = OptionsRun::read();
            run = Some(next);
            return Act::Reply(reply);
        }
        // paranoid_confirmation:quit: the word yes
        Act::Quit
    });
    assert_eq!(read, vec![Some(goal)]);
}
