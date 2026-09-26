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

/// One save file, and not NetHack's panic save (`<name>.e[.Z]`).
fn assert_saved_normally(pg: &Path) {
    let names: Vec<String> = std::fs::read_dir(pg.join("save"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(!names[0].contains(".e"), "panic save: {names:?}");
}

/// A bad select_menu reply must end like a lost client: error, normal save, exit 0.
fn assert_menu_reply_rejected(script: &str, why: &str) {
    let (pg, t) = run_script(SEED, NEW_MOON, script);
    assert_eq!(t.errors.len(), 1, "{:?}", t.errors);
    assert!(t.errors[0].contains(why), "{:?}", t.errors);
    assert!(t.said_bye);
    assert!(t.exit.unwrap().success(), "{:?}", t.exit);
    assert_saved_normally(pg.path());
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
fn same_seed_and_clock_replay_identically() {
    let (_a, first) = run_script(SEED, NEW_MOON, QUIT);
    let (_b, second) = run_script(SEED, NEW_MOON, QUIT);
    assert_eq!(first.lines, second.lines);
    let (_c, other_seed) = run_script(SEED + 1, NEW_MOON, QUIT);
    assert_ne!(first.stream_hash(), other_seed.stream_hash());
    let (_d, other_time) = run_script(SEED, NEW_MOON + 14 * 86_400, QUIT);
    assert_ne!(first.stream_hash(), other_time.stream_hash());
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
fn a_negative_menu_count_is_rejected_instead_of_panicking_the_core() {
    // D, Weapons, then the spear with count -5: NetHack itself panics in splitobj()
    assert_menu_reply_rejected("key D\nmenu 4\nmenu 1:-5\n", "count");
}

#[test]
fn a_menu_item_named_twice_is_rejected() {
    assert_menu_reply_rejected("key D\nmenu 4\nmenu 1 1\n", "twice");
}

#[test]
fn two_items_in_a_pick_one_menu_are_rejected() {
    // the inventory menu picks one item: the spear (1) and the dagger (2)
    assert_menu_reply_rejected("key i\nmenu 1 2\n", "more than one");
}

#[test]
fn objects_that_look_alike_share_a_tile() {
    // debug mode: wish for a sack and a bag of holding; both show as "a bag"
    let pg = tempfile::tempdir().unwrap();
    let mut cfg = config(pg.path(), SEED, NEW_MOON);
    std::fs::write(
        pg.path().join("sysconf"),
        "WIZARDS=*\nMAXPLAYERS=10\nPANICTRACE_GDB=0\nPANICTRACE_LIBC=0\n",
    )
    .unwrap();
    cfg.options.push_str(",playmode:debug");
    let script = "key 23\ntext sack\nkey 23\ntext bag of holding\nkey i\ncancel\n\
                  key #\next quit\nyn y\nyn n\nyn q\n";
    let mut engine = Engine::spawn(&cfg).unwrap();
    let mut responder = ScriptResponder::new(parse_script(script).unwrap());
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default()).unwrap();
    let bag_tiles: Vec<i32> = t
        .lines
        .iter()
        .filter_map(|l| match nh_protocol::parse_line(l) {
            Ok(EngineMsg::Win(WinCall::AddMenu(item))) if item.str.as_deref() == Some("a bag") => {
                item.glyph.map(|g| g.tile)
            }
            _ => None,
        })
        .collect();
    assert_eq!(bag_tiles.len(), 2, "{bag_tiles:?}");
    assert_eq!(
        bag_tiles[0], bag_tiles[1],
        "a bag's tile reveals which bag it is"
    );
}

#[test]
fn escape_at_a_yes_no_prompt_is_answered_like_the_terminal_does() {
    // ESC at "Really quit without saving? [yn]" means "n", as in tty; the
    // core must never receive a character that is not among the choices
    let (pg, t) = run_script(SEED, NEW_MOON, &format!("key #\next quit\nyn ESC\n{QUIT}"));
    let msgs = t.messages(1);
    assert!(
        !msgs.iter().any(|m| m.contains("Program in disorder")),
        "{msgs:?}"
    );
    assert!(!pg.path().join("paniclog").exists());
    assert!(t.said_bye && t.exit.unwrap().success());
}

#[test]
fn a_yes_no_answer_outside_the_choices_is_a_protocol_error() {
    let (pg, t) = run_script(SEED, NEW_MOON, "key #\next quit\nyn x\n");
    assert_eq!(t.errors.len(), 1, "{:?}", t.errors);
    assert!(
        t.errors[0].contains("not one of the choices"),
        "{:?}",
        t.errors
    );
    assert!(t.exit.unwrap().success());
    assert_saved_normally(pg.path());
}

#[test]
fn a_message_menu_answer_is_the_letter_nothing_or_escape() {
    // eat, ask for the list: with one food item NetHack shows it through
    // message_menu; an unrelated key there means "no selection"
    let (_pg, t) = run_script(
        SEED,
        NEW_MOON,
        &format!("key e\nyn ?\nyn x\nyn ESC\n{QUIT}"),
    );
    let msgs = t.messages(1);
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("You don't have that object")),
        "{msgs:?}"
    );
    assert!(msgs.iter().any(|m| m.contains("Never mind")), "{msgs:?}");
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
fn a_recorded_session_replays_to_the_same_stream() {
    let (_pg, t) = run_script(SEED, NEW_MOON, QUIT);
    let hello = t.hello.clone().unwrap();
    let recording = Recording {
        header: RecordingHeader {
            format: RECORDING_FORMAT,
            seed: SEED,
            fixed_time: NEW_MOON,
            options: EngineConfig::character_options(
                "Hero", "valkyrie", "human", "female", "neutral",
            ),
            engine: hello.engine,
            patchset: hello.patchset,
            stream_hash: t.stream_hash(),
        },
        replies: t.replies.clone(),
    };
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("quit.rhrec");
    recording.save(&file).unwrap();
    let loaded = Recording::load(&file).unwrap();
    assert_eq!(loaded, recording);

    let pg = tempfile::tempdir().unwrap();
    let mut cfg = config(pg.path(), loaded.header.seed, loaded.header.fixed_time);
    cfg.options = loaded.header.options.clone();
    let mut engine = Engine::spawn(&cfg).unwrap();
    let mut replay = ReplayResponder::new(&loaded);
    let again = run_session(&mut engine, &mut replay, &SessionLimits::default()).unwrap();
    assert_eq!(again.stream_hash(), loaded.header.stream_hash);
}

#[test]
fn a_session_the_client_hung_up_on_still_replays() {
    // the usual bug report: play a little, then the client dies
    let (_pg, t) = run_script(SEED, NEW_MOON, "key s\nkey s\nhangup\n");
    let recording = Recording {
        header: RecordingHeader {
            format: RECORDING_FORMAT,
            seed: SEED,
            fixed_time: NEW_MOON,
            options: String::new(),
            engine: String::new(),
            patchset: String::new(),
            stream_hash: t.stream_hash(),
        },
        replies: t.replies.clone(),
    };
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), SEED, NEW_MOON)).unwrap();
    let mut replay = ReplayResponder::new(&recording);
    let again = run_session(&mut engine, &mut replay, &SessionLimits::default()).unwrap();
    assert_eq!(again.stream_hash(), t.stream_hash());
}

#[test]
fn replay_against_a_different_game_reports_divergence() {
    let (_pg, t) = run_script(SEED, NEW_MOON, QUIT);
    let recording = Recording {
        header: RecordingHeader {
            format: RECORDING_FORMAT,
            seed: SEED,
            fixed_time: NEW_MOON,
            options: String::new(),
            engine: String::new(),
            patchset: String::new(),
            stream_hash: t.stream_hash(),
        },
        // drop the "Really quit?" answer: the engine will ask yn, we have ext/yn mismatch
        replies: t
            .replies
            .iter()
            .filter(|r| r.func != "get_ext_cmd")
            .cloned()
            .collect(),
    };
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), SEED, NEW_MOON)).unwrap();
    let mut replay = ReplayResponder::new(&recording);
    let err = run_session(&mut engine, &mut replay, &SessionLimits::default()).unwrap_err();
    assert!(matches!(err, LinkError::Divergence { .. }), "{err}");
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
