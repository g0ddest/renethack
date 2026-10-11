//! Debug-mode games on the real engine for the live tests: a new character
//! whose playground allows debug mode, played by a test's answers.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{Catalog, EngineMsg, Reply};
use nh_world::*;

const NEW_MOON: i64 = 1_768_694_400;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

/// A new character in debug mode (the playground's sysconf allows it).
fn config(playground: &Path, seed: u64, role: &str) -> EngineConfig {
    let dir = engine_dir();
    let engine = dir.join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    create_playground(&dir.join("data"), playground).expect("playground");
    std::fs::write(
        playground.join("sysconf"),
        "WIZARDS=*\nMAXPLAYERS=10\nPANICTRACE_GDB=0\nPANICTRACE_LIBC=0\n",
    )
    .expect("sysconf");
    let options =
        EngineConfig::character_options("Hero", role, "human", "female", "neutral").unwrap();
    EngineConfig {
        engine,
        playground: playground.to_path_buf(),
        options: format!("{options},{CLIENT_EXTRA_OPTIONS},playmode:debug"),
        seed: Some(seed),
        fixed_time: Some(NEW_MOON),
    }
}

/// What a test does at a prompt: answer it, or quit the game.
pub enum Act {
    Reply(Reply),
    Quit,
}

/// Play by engine lines; read-only displays are acknowledged. `decide`
/// sees every other prompt until it says Quit; then the game is quit
/// (yes to "Really quit?", no to the rest). Returns the final world.
pub fn play(
    seed: u64,
    role: &str,
    decide: &mut dyn FnMut(&World, &Catalog, &Prompt) -> Act,
) -> World {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), seed, role)).unwrap();
    let mut world = World::new();
    let mut catalog: Option<Catalog> = None;
    let mut errors = Vec::new();
    let mut quitting = false;
    let mut said_yes = false;
    while let Some(inc) = engine.recv(Duration::from_secs(10)).unwrap() {
        match inc.msg {
            EngineMsg::Catalog(c) => {
                world.set_catalog(&c);
                catalog = Some(*c);
            }
            EngineMsg::Win(call) => world.apply(&call),
            EngineMsg::Req { id, req } => {
                let prompt = world.on_request(&req);
                let reply = match &prompt {
                    Prompt::AutoAck | Prompt::Show { .. } | Prompt::MapPause => Reply::Ack,
                    Prompt::ExtCmd if quitting => Reply::ExtCmd(Some("quit".into())),
                    Prompt::Choice { .. } if quitting => {
                        let c = if said_yes { 'n' } else { 'y' };
                        said_yes = true;
                        Reply::Char(c as i32)
                    }
                    p if quitting => p.escape_reply(),
                    p => match decide(&world, catalog.as_ref().expect("catalog"), p) {
                        Act::Reply(r) => r,
                        Act::Quit => {
                            assert_eq!(*p, Prompt::Command, "quit at a command prompt");
                            quitting = true;
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
