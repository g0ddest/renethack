//! End-to-end: these tests start the real nh-engine.
//! Build it first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

use std::path::{Path, PathBuf};

use nh_link::*;
use nh_protocol::{EngineMsg, GlyphKind, WinCall};

const SEED: u64 = 42;
/// 2026-01-18 00:00 UTC: a new moon, so the welcome includes a warning.
const NEW_MOON: i64 = 1_768_694_400;
const QUIT: &str = "key #\next quit\nyn y\nyn q\n";

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

fn config(playground: &Path, seed: u64, fixed_time: i64) -> EngineConfig {
    let dir = engine_dir();
    let engine = dir.join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    create_playground(&dir.join("data"), playground).expect("playground");
    EngineConfig {
        engine,
        playground: playground.to_path_buf(),
        options: EngineConfig::character_options("Hero", "valkyrie", "human", "female", "neutral"),
        seed: Some(seed),
        fixed_time: Some(fixed_time),
    }
}

fn run_script(seed: u64, fixed_time: i64, script: &str) -> (tempfile::TempDir, Transcript) {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), seed, fixed_time)).unwrap();
    let mut responder = ScriptResponder::new(parse_script(script).unwrap());
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default()).unwrap();
    (pg, t)
}

fn save_files(pg: &Path) -> usize {
    std::fs::read_dir(pg.join("save")).unwrap().count()
}

#[test]
fn quitting_a_new_game_is_a_clean_exit() {
    let (_pg, t) = run_script(SEED, NEW_MOON, QUIT);
    let hello = t.hello.as_ref().expect("hello");
    assert_eq!(hello.engine, "5.0.0");
    let cat = t.catalog.as_ref().expect("catalog");
    assert_eq!(cat.roles.len(), 13);
    assert!(t.said_bye);
    assert!(t.errors.is_empty(), "{:?}", t.errors);
    assert!(t.exit.unwrap().success());
    let msgs = t.messages(1);
    assert!(
        msgs.iter().any(|m| m.contains("welcome to NetHack")),
        "{msgs:?}"
    );
    assert!(msgs.iter().any(|m| m.contains("New moon")), "{msgs:?}");
}

#[test]
fn losing_the_client_saves_the_game() {
    let (pg, t) = run_script(SEED, NEW_MOON, "hangup\n");
    assert_eq!(t.errors, vec!["client closed the connection".to_string()]);
    assert!(t.exit.unwrap().success());
    assert_eq!(save_files(pg.path()), 1);
}

#[test]
fn a_nonsense_menu_reply_saves_the_game() {
    // 'i' opens the inventory, a pick-one menu; item 999 does not exist
    let (pg, t) = run_script(SEED, NEW_MOON, "key i\nmenu 999\n");
    assert_eq!(t.errors.len(), 1);
    assert!(t.errors[0].contains("not selectable"), "{:?}", t.errors);
    assert_eq!(save_files(pg.path()), 1);
}

#[test]
fn objects_on_the_map_hide_their_glyph_number() {
    let (_pg, t) = run_script(SEED, NEW_MOON, QUIT);
    let mut objects = 0;
    for line in &t.lines {
        if let Ok(EngineMsg::Win(WinCall::PrintGlyph { g, .. })) = nh_protocol::parse_line(line)
            && g.kind == GlyphKind::Obj
        {
            objects += 1;
            assert_eq!(g.glyph, None);
            assert!(g.tile > 0);
        }
    }
    assert!(
        objects > 0,
        "seed {SEED} shows no object; pick another seed"
    );
}

#[test]
fn a_script_that_never_quits_is_stopped() {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), SEED, NEW_MOON)).unwrap();
    // search forever: 's' is a legal command every turn
    let mut responder = ScriptResponder::new(vec![Step::Key('s' as i32); 100]);
    let limits = SessionLimits {
        max_requests: 20,
        ..SessionLimits::default()
    };
    let err = run_session(&mut engine, &mut responder, &limits).unwrap_err();
    assert!(matches!(err, LinkError::TooManyRequests(20)), "{err}");
}
