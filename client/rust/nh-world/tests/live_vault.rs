//! A vault guard's escort to its end, on the real engine: the hero waits in
//! a vault until the guard comes, follows the guard out along its
//! temporary corridor, and the guard disappears. NetHack 5.0.0 then asked
//! for a screen update of the off-map spot it parks the guard on and
//! printed "newsym: attempting screen update for <0,0>" and "Program in
//! disorder!" (engine/patches/0004 backports upstream's fix). Debug mode
//! maps the level (^F) and teleports the hero into the vault (^T).
//! Build the engine first with `make -C engine` (or set RENETHACK_ENGINE_DIR).

use std::path::{Path, PathBuf};
use std::time::Duration;

use nh_link::*;
use nh_protocol::{Catalog, EngineMsg, GlyphKind, Reply};
use nh_world::*;

/// A seed whose first level has a vault, with the options below.
const SEED: u64 = 42;
const NEW_MOON: i64 = 1_768_694_400;

fn engine_dir() -> PathBuf {
    std::env::var_os("RENETHACK_ENGINE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
}

/// A new valkyrie in debug mode (the playground's sysconf allows it).
fn config(playground: &Path, seed: u64) -> EngineConfig {
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
        EngineConfig::character_options("Hero", "valkyrie", "human", "female", "neutral").unwrap();
    EngineConfig {
        engine,
        playground: playground.to_path_buf(),
        options: format!("{options},{CLIENT_EXTRA_OPTIONS},playmode:debug"),
        seed: Some(seed),
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
fn play(seed: u64, decide: &mut dyn FnMut(&World, &Catalog, &Prompt) -> Act) -> World {
    let pg = tempfile::tempdir().unwrap();
    let mut engine = Engine::spawn(&config(pg.path(), seed)).unwrap();
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

fn floorish(t: Option<Terrain>) -> bool {
    matches!(t, Some(Terrain::Floor | Terrain::DarkFloor))
}

/// A vault: a room of 2×2 floor closed by walls all round (no door).
fn find_vault(world: &World, catalog: &Catalog) -> Option<(i32, i32)> {
    let terrain = |x, y| world.map.cell(x, y).and_then(|c| cell_terrain(c, catalog));
    for y in 1..ROWNO - 2 {
        for x in 2..COLNO - 2 {
            let inside = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
            if !inside.iter().all(|&(cx, cy)| floorish(terrain(cx, cy))) {
                continue;
            }
            let walled = (x - 1..=x + 2)
                .flat_map(|cx| (y - 1..=y + 2).map(move |cy| (cx, cy)))
                .filter(|c| !inside.contains(c))
                .all(|(cx, cy)| terrain(cx, cy) == Some(Terrain::Wall));
            if walled {
                return Some((x, y));
            }
        }
    }
    None
}

fn guard_at(world: &World, catalog: &Catalog) -> Option<(i32, i32)> {
    for y in 0..ROWNO {
        for x in 1..COLNO {
            let Some(g) = world.map.cell(x, y).and_then(|c| c.glyph.as_ref()) else {
                continue;
            };
            let name = g
                .mon
                .and_then(|m| usize::try_from(m).ok())
                .and_then(|m| catalog.monsters.get(m))
                .map(|m| m.name.as_str());
            if g.kind == GlyphKind::Mon && name == Some("guard") {
                return Some((x, y));
            }
        }
    }
    None
}

fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The vi-key of a step from `from` to the neighbouring `to`.
fn step_key(from: (i32, i32), to: (i32, i32)) -> char {
    match ((to.0 - from.0).signum(), (to.1 - from.1).signum()) {
        (-1, 0) => 'h',
        (1, 0) => 'l',
        (0, -1) => 'k',
        (0, 1) => 'j',
        (-1, -1) => 'y',
        (1, -1) => 'u',
        (-1, 1) => 'b',
        (1, 1) => 'n',
        _ => 's',
    }
}

#[test]
fn a_vault_guard_leads_the_hero_out_and_disappears_cleanly() {
    #[derive(Debug, PartialEq)]
    enum Stage {
        Map,
        Teleport,
        Wait,
        Follow,
    }
    let mut stage = Stage::Map;
    let mut vault = None;
    let mut trail: Vec<(i32, i32)> = Vec::new();
    let mut prompts = 0;
    let mut after_gone = 0;
    let world = play(SEED, &mut |w, cat, p| {
        prompts += 1;
        assert!(prompts < 600, "no end of the escort at {stage:?}");
        let gone = w
            .log
            .iter()
            .any(|m| m.text.contains("the guard disappears"));
        match p {
            // the guard asks the hero's name: any but Croesus
            Prompt::Text { query, .. } if query.contains("who are you?") => {
                stage = Stage::Follow;
                return Act::Reply(Reply::Text("Hero".into()));
            }
            // wizard ^T: "debug mode; ignore restrictions"
            Prompt::Menu { items, .. } if stage == Stage::Teleport => {
                let w_entry = items
                    .iter()
                    .find(|i| i.ch == 'w' as i32)
                    .expect("the 'w' way");
                return Act::Reply(Reply::Menu(vec![(w_entry.idx as u32, -1)]));
            }
            Prompt::Command if w.getpos && stage == Stage::Teleport => {
                let (x, y) = vault.expect("a vault");
                stage = Stage::Wait;
                return Act::Reply(Reply::Click { x, y, modifier: 1 });
            }
            Prompt::Command => {}
            other => panic!("unexpected at {stage:?}: {other:?}"),
        }
        match stage {
            Stage::Map => {
                stage = Stage::Teleport;
                Act::Reply(Reply::Key(6)) // ^F: map the level
            }
            Stage::Teleport => {
                vault = find_vault(w, cat);
                assert!(vault.is_some(), "seed {SEED}: no vault on the first level");
                Act::Reply(Reply::Key(20)) // ^T
            }
            // in the vault, searching until the guard comes
            Stage::Wait => Act::Reply(Reply::Key('s' as i32)),
            Stage::Follow => {
                if gone {
                    after_gone += 1;
                    // a few turns more: the guard's corridor is filled in
                    // behind the hero
                    return if after_gone > 3 {
                        Act::Quit
                    } else {
                        Act::Reply(Reply::Key('s' as i32))
                    };
                }
                let hero = w.map.hero().expect("the hero on the map");
                // the guard is seen next to the hero; a step further on,
                // in its dark corridor, it is not
                let guard = guard_at(w, cat);
                if let Some(g) = guard
                    && trail.last() != Some(&g)
                {
                    trail.push(g);
                }
                if guard.is_some_and(|g| chebyshev(hero, g) <= 1) {
                    // next to the guard: it leads on
                    return Act::Reply(Reply::Key('s' as i32));
                }
                // behind: onto the guard's way, as near the guard as can be
                let next = trail
                    .iter()
                    .rev()
                    .find(|&&c| Some(c) != guard && chebyshev(hero, c) == 1)
                    .copied();
                let key = next.map_or('s', |c| step_key(hero, c));
                Act::Reply(Reply::Key(key as i32))
            }
        }
    });
    let log: Vec<&str> = world.log.iter().map(|m| m.text.as_str()).collect();
    assert!(
        log.iter().any(|t| t.contains("Vault's guards enters")),
        "the guard never came: {log:?}"
    );
    assert!(
        log.iter().any(|t| t.contains("the guard disappears")),
        "the escort did not end: {log:?}"
    );
    assert_eq!(world.log.impossible_since(0), None, "{log:?}");
}
