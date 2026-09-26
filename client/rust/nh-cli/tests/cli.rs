//! The nh-cli binary against the real engine (build it: `make -C engine`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

fn nh_cli(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nh-cli"))
        .args(args)
        .env("RENETHACK_ENGINE_DIR", engine_dir())
        .output()
        .expect("nh-cli runs");
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), text)
}

#[test]
fn run_prints_the_game_and_replay_confirms_it() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("quit.script");
    std::fs::write(&script, "key #\next quit\nyn y\nyn q\n").unwrap();
    let rec = dir.path().join("quit.rhrec");

    let (ok, out) = nh_cli(&[
        "run",
        script.to_str().unwrap(),
        "--seed",
        "42",
        "--fixed-time",
        "1768694400",
        "--record",
        rec.to_str().unwrap(),
    ]);
    assert!(ok, "{out}");
    assert!(
        out.contains("| Velkommen Hero, welcome to NetHack!"),
        "{out}"
    );
    assert!(out.contains("? Really quit"), "{out}");

    let (ok, out) = nh_cli(&["replay", rec.to_str().unwrap()]);
    assert!(ok, "{out}");
    assert!(out.contains("replay matches"), "{out}");
}

#[test]
fn catalog_summarizes_the_static_data() {
    let (ok, out) = nh_cli(&["catalog"]);
    assert!(ok, "{out}");
    assert!(out.contains("engine 5.0.0"), "{out}");
    assert!(out.contains("roles 13, races 5"), "{out}");
}

#[test]
fn a_bad_script_is_reported_not_hung() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("bad.script");
    std::fs::write(&script, "ext quit\n").unwrap(); // the first request wants a key
    let (ok, out) = nh_cli(&[
        "run",
        script.to_str().unwrap(),
        "--seed",
        "1",
        "--fixed-time",
        "1",
    ]);
    assert!(!ok);
    assert!(out.contains("cannot answer"), "{out}");
}
