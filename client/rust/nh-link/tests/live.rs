//! LiveSession, saves and recover against the real nh-engine.
//! Build it first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use nh_link::*;
use nh_protocol::{PickHow, Reply, Request, WinCall};

const SEED: u64 = 42;
/// 2026-01-18 00:00 UTC, as in tests/engine.rs.
const NEW_MOON: i64 = 1_768_694_400;
const QUIT: &str = "key #\next quit\nyn y\nyn q\n";
/// Longest a test waits for the engine.
const PATIENCE: Duration = Duration::from_secs(30);

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

fn engine_path() -> PathBuf {
    let engine = engine_dir().join("nh-engine");
    assert!(
        engine.exists(),
        "{} missing: run `make -C engine` first",
        engine.display()
    );
    engine
}

fn config_with(playground: &Path, options: String) -> EngineConfig {
    create_playground(&engine_dir().join("data"), playground).expect("playground");
    EngineConfig {
        engine: engine_path(),
        playground: playground.to_path_buf(),
        options,
        seed: Some(SEED),
        fixed_time: Some(NEW_MOON),
    }
}

fn hero(name: &str) -> String {
    EngineConfig::character_options(name, "valkyrie", "human", "female", "neutral").unwrap()
}

fn config(playground: &Path) -> EngineConfig {
    config_with(playground, hero("Hero"))
}

/// Poll like a game loop (1 ms naps when idle), acknowledging displays the
/// way ScriptResponder does, until a request needs a decision or the
/// engine exits. Every event lands in `seen`.
fn next_decision(s: &mut LiveSession, seen: &mut Vec<SessionEvent>) -> Option<(u64, Request)> {
    let give_up = Instant::now() + PATIENCE;
    loop {
        assert!(Instant::now() < give_up, "engine stalled");
        let events = s.poll(Instant::now() + Duration::from_millis(20));
        if events.is_empty() {
            thread::sleep(Duration::from_millis(1));
        }
        for ev in events {
            seen.push(ev.clone());
            match ev {
                SessionEvent::Request { id, req } => match req {
                    Request::DisplayNhwindow { .. }
                    | Request::DisplayFile { .. }
                    | Request::SelectMenu {
                        how: PickHow::None, ..
                    } => s.answer(id, &Reply::Ack).unwrap(),
                    _ => return Some((id, req)),
                },
                SessionEvent::Exited(_) => return None,
                _ => {}
            }
        }
    }
}

/// Everything until `Exited`, which must be last.
fn until_exit(s: &mut LiveSession) -> (Vec<SessionEvent>, Ending) {
    let mut seen = Vec::new();
    let give_up = Instant::now() + PATIENCE;
    while !s.has_exited() {
        assert!(Instant::now() < give_up, "engine did not exit");
        let events = s.poll(Instant::now() + Duration::from_millis(20));
        if events.is_empty() {
            thread::sleep(Duration::from_millis(1));
        }
        seen.extend(events);
    }
    assert!(
        s.poll(Instant::now() + Duration::from_millis(20))
            .is_empty()
    );
    match seen.last() {
        Some(SessionEvent::Exited(ending)) => {
            let ending = ending.clone();
            (seen, ending)
        }
        other => panic!("last event is {other:?}"),
    }
}

fn reply_for(step: &Step) -> Reply {
    match step {
        Step::Key(k) => Reply::Key(*k),
        Step::Ext(c) => Reply::ExtCmd(c.clone()),
        Step::Yn(c) => Reply::Char(*c),
        Step::Text(t) => Reply::Text(t.clone()),
        Step::Menu(items) => Reply::Menu(items.clone()),
        Step::Cancel => Reply::Cancel,
        other => panic!("no reply for {other:?}"),
    }
}

/// Answer decisions from a script until it runs out; then the engine exits.
fn play_script(s: &mut LiveSession, script: &str) -> Vec<SessionEvent> {
    let mut steps: VecDeque<Step> = parse_script(script).unwrap().into();
    let mut seen = Vec::new();
    while let Some((id, req)) = next_decision(s, &mut seen) {
        let step = steps
            .pop_front()
            .unwrap_or_else(|| panic!("script exhausted at {req:?}"));
        s.answer(id, &reply_for(&step)).unwrap();
    }
    assert!(steps.is_empty(), "left over: {steps:?}");
    seen
}

fn messages(events: &[SessionEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Win(WinCall::Putstr { win: 1, text, .. }) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn exited(events: &[SessionEvent]) -> &Ending {
    match events.last() {
        Some(SessionEvent::Exited(ending)) => ending,
        other => panic!("last event is {other:?}"),
    }
}

/// Start a session that restores `name` and return its messages up to the
/// first command prompt; the session is dropped (and saves again).
fn restored_messages(playground: &Path, name: &str) -> Vec<String> {
    let cfg = config_with(playground, EngineConfig::restore_options(name).unwrap());
    let mut s = LiveSession::start(&cfg).unwrap();
    let mut seen = Vec::new();
    let (_, req) = next_decision(&mut s, &mut seen).expect("a command prompt");
    assert_eq!(req, Request::NhPoskey { getpos: false });
    messages(&seen)
}

fn welcomed_back(msgs: &[String]) -> bool {
    msgs.iter().any(|m| m.contains("welcome back"))
}

fn lock_files(playground: &Path) -> Vec<String> {
    std::fs::read_dir(playground)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|f| f.contains("lock"))
        .collect()
}

#[test]
fn live_session_matches_run_session() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let events = play_script(&mut s, QUIT);

    let pg2 = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg2.path())).unwrap();
    let mut responder = ScriptResponder::new(parse_script(QUIT).unwrap());
    let t = run_session(&mut engine, &mut responder, &SessionLimits::default()).unwrap();

    assert_eq!(s.stream_hash(), t.stream_hash());
    assert_eq!(s.replies(), t.replies.as_slice());
    let ending = exited(&events);
    assert_eq!(ending.code, Some(0));
    assert!(ending.said_bye);
    assert!(!ending.hung_up_by_client);
    assert_eq!(ending.engine_error, None);
    assert_eq!(ending.failed, None);
    assert!(
        matches!(events[0], SessionEvent::Hello(_)),
        "{:?}",
        events[0]
    );
    assert!(matches!(events[1], SessionEvent::Catalog(_)));
    assert_eq!(s.recent_lines().count(), t.lines.len());
    assert_eq!(s.recent_lines().last(), t.lines.last().map(String::as_str));
    assert!(s.pending().is_none() && s.silent_for().is_none());
}

#[test]
fn answer_without_request_is_refused() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let err = s.answer(1, &Reply::Key('s' as i32)).unwrap_err();
    assert!(matches!(err, AnswerError::NotPending(1)), "{err}");
    let mut seen = Vec::new();
    let (id, _) = next_decision(&mut s, &mut seen).unwrap();
    assert_eq!(s.pending().map(|(id, _)| id), Some(id));
    assert_eq!(s.silent_for(), None);
    let err = s.answer(id + 1, &Reply::Key('s' as i32)).unwrap_err();
    assert!(matches!(err, AnswerError::NotPending(n) if n == id + 1));
    assert_eq!(s.pending().map(|(id, _)| id), Some(id));
    s.answer(id, &Reply::Key('#' as i32)).unwrap();
    let events = play_script(&mut s, "ext quit\nyn y\nyn q\n");
    assert_eq!(exited(&events).code, Some(0));
}

#[test]
fn answer_twice_is_refused_and_the_game_goes_on() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let mut seen = Vec::new();
    let (id, req) = next_decision(&mut s, &mut seen).unwrap();
    assert_eq!(req, Request::NhPoskey { getpos: false });
    s.answer(id, &Reply::Key('s' as i32)).unwrap();
    let err = s.answer(id, &Reply::Key('s' as i32)).unwrap_err();
    assert!(
        matches!(err, AnswerError::NotPending(n) if n == id),
        "{err}"
    );
    assert!(s.silent_for().is_some());
    let events = play_script(&mut s, QUIT);
    let ending = exited(&events);
    assert_eq!(ending.code, Some(0));
    assert!(ending.said_bye);
    assert_eq!(ending.engine_error, None);
}

#[test]
fn hangup_saves_and_the_game_continues() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let mut seen = Vec::new();
    let (id, _) = next_decision(&mut s, &mut seen).unwrap();
    s.answer(id, &Reply::Key('l' as i32)).unwrap();
    let (id, _) = next_decision(&mut s, &mut seen).unwrap();
    s.hang_up();
    assert!(s.pending().is_none());
    assert!(matches!(
        s.answer(id, &Reply::Key('l' as i32)),
        Err(AnswerError::NotPending(_))
    ));
    let (events, ending) = until_exit(&mut s);
    assert!(ending.hung_up_by_client);
    assert_eq!(ending.code, Some(0));
    assert!(ending.said_bye);
    assert_eq!(
        ending.engine_error.as_deref(),
        Some("client closed the connection")
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Request { .. }))
    );
    let last = s.replies().last().unwrap();
    assert_eq!((last.id, last.r.is_null()), (id, true));

    let saves = list_saves(pg.path()).unwrap();
    assert_eq!(saves.len(), 1, "{saves:?}");
    assert_eq!(saves[0].name, "Hero");
    assert!(save_exists(pg.path(), "Hero"));
    assert!(interrupted_games(pg.path()).unwrap().is_empty());
    let msgs = restored_messages(pg.path(), "Hero");
    assert!(welcomed_back(&msgs), "{msgs:?}");
}

#[test]
fn hangup_while_engine_busy_still_saves() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let mut seen = Vec::new();
    let (id, _) = next_decision(&mut s, &mut seen).unwrap();
    s.answer(id, &Reply::Key('s' as i32)).unwrap();
    s.hang_up();
    let (events, ending) = until_exit(&mut s);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::EngineError(_)))
    );
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Bye)));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Request { .. }))
    );
    assert_eq!(ending.code, Some(0));
    assert!(ending.hung_up_by_client);
    assert!(s.pending().is_none());
    // the request that came after the hangup is recorded as one
    let last = s.replies().last().unwrap();
    assert!(last.id > id && last.r.is_null(), "{last:?}");
    assert_eq!(list_saves(pg.path()).unwrap().len(), 1);
}

#[test]
fn dropping_a_live_session_saves() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let mut seen = Vec::new();
    let (id, _) = next_decision(&mut s, &mut seen).unwrap();
    s.answer(id, &Reply::Key('s' as i32)).unwrap();
    next_decision(&mut s, &mut seen).unwrap();
    drop(s);
    assert_eq!(list_saves(pg.path()).unwrap().len(), 1);
    assert!(lock_files(pg.path()).is_empty());
}

#[test]
fn saved_names_round_trip() {
    let pg = tempfile::tempdir().unwrap();
    for name in ["Olaf the Bold", "Сигурд"] {
        remember_name(pg.path(), name).unwrap();
        let mut s = LiveSession::start(&config_with(pg.path(), hero(name))).unwrap();
        next_decision(&mut s, &mut Vec::new()).unwrap();
        s.hang_up();
        let (_, ending) = until_exit(&mut s);
        assert_eq!(ending.code, Some(0));
    }
    let mut saves = list_saves(pg.path()).unwrap();
    saves.sort_by(|a, b| a.name.cmp(&b.name));
    let shown: Vec<(&str, &str)> = saves
        .iter()
        .map(|g| (g.name.as_str(), g.display_name.as_str()))
        .collect();
    assert_eq!(
        shown,
        [("Olaf_the_Bold", "Olaf the Bold"), ("Сигурд", "Сигурд")]
    );
    let file = saves[0].file.file_name().unwrap().to_string_lossy();
    assert!(file.contains("Olaf_the_Bold"), "{file}");
    assert!(save_exists(pg.path(), "Olaf the Bold"));
    assert!(!save_exists(pg.path(), "Olaf"));
    for game in &saves {
        let msgs = restored_messages(pg.path(), &game.name);
        assert!(welcomed_back(&msgs), "{}: {msgs:?}", game.name);
    }
}

#[test]
fn recover_rebuilds_a_killed_game() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = LiveSession::start(&config(pg.path())).unwrap();
    let mut seen = Vec::new();
    for _ in 0..2 {
        let (id, _) = next_decision(&mut s, &mut seen).unwrap();
        s.answer(id, &Reply::Key('s' as i32)).unwrap();
    }
    // a running game is not interrupted: recovering it would take its
    // level files away
    assert!(interrupted_games(pg.path()).unwrap().is_empty());
    // killed while busy: a request it may have sent already is not handed out
    s.kill();
    assert!(s.pending().is_none());
    let (events, ending) = until_exit(&mut s);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SessionEvent::Request { .. }))
    );
    assert!(s.pending().is_none());
    assert_eq!(ending.code, None);
    assert!(!ending.said_bye);
    drop(s);

    assert_eq!(interrupted_games(pg.path()).unwrap(), ["alock"]);
    assert!(list_saves(pg.path()).unwrap().is_empty());
    let recovered = recover_game(&engine_dir().join("recover"), pg.path(), "alock").unwrap();
    assert_eq!(recovered, Recovered::Saved("Hero".into()));
    assert!(lock_files(pg.path()).is_empty());
    assert!(interrupted_games(pg.path()).unwrap().is_empty());
    let msgs = restored_messages(pg.path(), "Hero");
    assert!(welcomed_back(&msgs), "{msgs:?}");
    assert!(lock_files(pg.path()).is_empty());
}

#[test]
fn recover_frees_the_slot_when_it_fails() {
    let pg = tempfile::tempdir().unwrap();
    create_playground(&engine_dir().join("data"), pg.path()).unwrap();
    std::fs::write(pg.path().join("alock.0"), b"xy").unwrap();
    std::fs::write(pg.path().join("alock.1"), b"junk").unwrap();
    std::fs::write(pg.path().join("block.0"), b"other game").unwrap();
    assert_eq!(interrupted_games(pg.path()).unwrap(), ["alock", "block"]);
    let recovered = recover_game(&engine_dir().join("recover"), pg.path(), "alock").unwrap();
    assert_eq!(recovered, Recovered::Lost);
    assert_eq!(lock_files(pg.path()), ["block.0"]);
    assert!(list_saves(pg.path()).unwrap().is_empty());
}

#[test]
fn recover_that_cannot_run_is_an_error_and_keeps_the_slot() {
    let pg = tempfile::tempdir().unwrap();
    std::fs::write(pg.path().join("alock.0"), b"xy").unwrap();
    let err = recover_game(&pg.path().join("no-recover"), pg.path(), "alock").unwrap_err();
    assert!(matches!(err, LinkError::Spawn { .. }), "{err}");
    assert!(pg.path().join("alock.0").exists());
}

#[test]
fn fetch_catalog_does_not_start_a_game() {
    // a playground with a save named after the login user: the probe must
    // neither restore it nor touch the playground's sysconf
    let pg = tempfile::tempdir().unwrap();
    create_playground(&engine_dir().join("data"), pg.path()).unwrap();
    let save = pg
        .path()
        .join("save")
        .join(format!("{}alice", my_uid(pg.path())));
    std::fs::write(&save, b"not a real save").unwrap();
    let sysconf = std::fs::read(pg.path().join("sysconf")).unwrap();
    // Ok means the engine said bye and exited by itself with code 0
    let (hello, catalog) = fetch_catalog(&engine_path(), pg.path()).unwrap();
    assert_eq!(hello.engine, "5.0.0");
    assert_eq!(catalog.roles.len(), 13);
    assert!(lock_files(pg.path()).is_empty());
    assert_eq!(std::fs::read(&save).unwrap(), b"not a real save");
    assert_eq!(std::fs::read(pg.path().join("sysconf")).unwrap(), sysconf);

    let (_, again) = fetch_catalog(&engine_path(), &engine_dir().join("data")).unwrap();
    assert_eq!(again, catalog);
}

#[cfg(unix)]
fn my_uid(dir: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(dir).unwrap().uid()
}

#[cfg(not(unix))]
fn my_uid(_dir: &Path) -> u32 {
    0
}

/// NetHack takes a missing name from $USER, $LOGNAME or getlogin(); the
/// probe must ask for one anyway, whoever runs it.
#[test]
fn fetch_catalog_ignores_the_login_name() {
    if std::env::var_os("USER").is_some_and(|u| u == "alice") {
        return; // already the rerun below
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fetch_catalog_does_not_start_a_game"])
        .env("USER", "alice")
        .env("LOGNAME", "alice")
        .env("RENETHACK_ENGINE_DIR", engine_dir())
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && report.contains("1 passed"),
        "{report}{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exit_before_hello_is_reported() {
    let pg = tempfile::tempdir().unwrap();
    let mut cfg = config(pg.path());
    cfg.engine = PathBuf::from("/usr/bin/false");
    let mut s = LiveSession::start(&cfg).unwrap();
    let (events, ending) = until_exit(&mut s);
    assert!(
        events[..events.len() - 1]
            .iter()
            .all(|e| matches!(e, SessionEvent::Failed(_))),
        "{events:?}"
    );
    assert!(!ending.said_bye);
    assert_ne!(ending.code, Some(0));

    cfg.engine = pg.path().join("no-such-engine");
    assert!(matches!(
        LiveSession::start(&cfg),
        Err(LinkError::Spawn { .. })
    ));
}

/// The start every stand-in engine sends: hello and a minimal catalog.
const FAKE_START: &str = r#"echo '{"t":"hello","a":{"protocol":1,"engine":"fake","patchset":"x"}}'
echo '{"t":"catalog","a":{"glyphs":{"max":0,"mon":0,"pet":0,"invisible":0,"detect":0,"body":0,"ridden":0,"obj":0,"cmap":0,"zap":0,"swallow":0,"explode":0,"warning":0,"statue":0,"unexplored":0,"nothing":0},"tiles":{"last_monster":0,"last_object":0,"last_other":0},"monsters":[],"object_tiles":[],"cmap":[],"roles":[],"races":[],"genders":[],"aligns":[]}}'
"#;

/// A session on a shell script standing in for the engine.
#[cfg(unix)]
fn fake_session(playground: &Path, body: &str) -> LiveSession {
    use std::os::unix::fs::PermissionsExt;
    let script = playground.join("fake-engine");
    std::fs::write(&script, format!("#!/bin/sh\n{FAKE_START}{body}")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut cfg = config(playground);
    cfg.engine = script;
    LiveSession::start(&cfg).unwrap()
}

#[cfg(unix)]
#[test]
fn bad_line_keeps_earlier_events() {
    let pg = tempfile::tempdir().unwrap();
    // the bye after the garbage must never be delivered
    let mut s = fake_session(
        pg.path(),
        "echo '{\"t\":\"win\",\"fn\":\"init_nhwindows\",\"a\":{}}'\n\
         echo 'this is not JSON'\n\
         echo '{\"t\":\"bye\",\"a\":{}}'\n\
         exec sleep 5\n",
    );
    let started = Instant::now();
    let (events, ending) = until_exit(&mut s);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "not killed after the failure"
    );
    assert_eq!(events.len(), 5, "{events:?}");
    assert!(matches!(events[0], SessionEvent::Hello(_)));
    assert!(matches!(events[1], SessionEvent::Catalog(_)));
    assert_eq!(events[2], SessionEvent::Win(WinCall::InitNhwindows));
    let SessionEvent::Failed(why) = &events[3] else {
        panic!("{:?}", events[3]);
    };
    assert!(why.contains("this is not JSON"), "{why}");
    assert_eq!(ending.failed.as_deref(), Some(why.as_str()));
    assert!(!ending.said_bye);
    assert_eq!(s.recent_lines().last(), Some("this is not JSON"));
}

#[cfg(unix)]
#[test]
fn poll_stops_after_a_frame_boundary() {
    let pg = tempfile::tempdir().unwrap();
    let mut s = fake_session(
        pg.path(),
        "echo '{\"t\":\"win\",\"fn\":\"delay_output\",\"a\":{}}'\n\
         echo '{\"t\":\"win\",\"fn\":\"nhbell\",\"a\":{}}'\n\
         echo '{\"t\":\"bye\",\"a\":{}}'\n\
         touch done\n",
    );
    // let the script finish, so the lines after delay_output are written too
    let give_up = Instant::now() + PATIENCE;
    while !pg.path().join("done").exists() {
        assert!(Instant::now() < give_up, "script stalled");
        thread::sleep(Duration::from_millis(1));
    }
    let mut batches: Vec<Vec<SessionEvent>> = Vec::new();
    while !batches
        .iter()
        .flatten()
        .any(|e| *e == SessionEvent::Win(WinCall::DelayOutput))
    {
        assert!(Instant::now() < give_up, "no delay_output");
        let batch = s.poll(Instant::now() + Duration::from_secs(10));
        if batch.is_empty() {
            thread::sleep(Duration::from_millis(1));
        } else {
            batches.push(batch);
        }
    }
    let frame = batches.last().unwrap();
    assert_eq!(frame.last(), Some(&SessionEvent::Win(WinCall::DelayOutput)));
    let (rest, ending) = until_exit(&mut s);
    assert_eq!(rest[0], SessionEvent::Win(WinCall::Nhbell));
    assert_eq!(rest[1], SessionEvent::Bye);
    assert_eq!((ending.code, ending.said_bye), (Some(0), true));
}
