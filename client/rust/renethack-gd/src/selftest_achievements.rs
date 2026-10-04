//! The achievements (phases S3 and S6): `achievements` opens the page over
//! the title (nothing earned, the secrets hidden), then plays a game in
//! debug mode with a mock Steam: depth 10 earns nothing until the
//! self-tests' override lets a debug game earn; then the Mines, Sokoban
//! and a level-up past two titles are each earned once, kept in the local
//! store and told to the mock by their API names; the toast tells of them
//! under the prompt banner, one after another; the page in the game lists
//! them, and the arrows and the d-pad choose on it, B goes back. Every
//! screen fits the canvas. (Minetown is earned walking into the town, not
//! arriving on its level: a teleport does not earn it.)

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use godot::prelude::*;
use nh_world::achievements::{Achievements, Mock, Tracker};
use nh_world::{Key, KeyInput, Prompt};

use super::{
    DialogEvent, PadEv, Step, UiEvent, camera_settled, ctrl_key, entry_key, fail_on_error_screen,
    is_menu, key, quit, screen, start,
};
use crate::game::{GameState, RenethackGame};
use crate::gamepad::PadButton;

thread_local! {
    /// The mock Steam of the scenario: what it was told.
    static MOCK: RefCell<Option<Mock>> = const { RefCell::new(None) };
}

/// The medallion chosen before a move.
static CHOSEN: AtomicUsize = AtomicUsize::new(0);

/// What the scenario earns, in this order, and the API names Steam gets
/// (Minetown too when the teleport lands the hero in the town itself).
const EARNED: [(&str, &str); 5] = [
    ("mines", "ACH_MINES"),
    ("sokoban", "ACH_SOKOBAN"),
    ("rank_1", "ACH_RANK_1"),
    ("rank_2", "ACH_RANK_2"),
    ("depth_10", "ACH_DEPTH_10"),
];

fn told() -> Vec<String> {
    MOCK.with(|m| {
        m.borrow()
            .as_ref()
            .map(|m| m.unlocked.borrow().clone())
            .unwrap_or_default()
    })
}

fn tracker(g: &RenethackGame) -> Result<&Tracker, String> {
    g.achievements
        .as_ref()
        .ok_or_else(|| "no tracker".to_string())
}

fn earned(g: &RenethackGame, id: &str) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(tracker(g)?.store().has(id))
}

fn page(g: &RenethackGame) -> Result<&crate::achievement_view::Page, String> {
    g.achievement_page
        .as_ref()
        .ok_or_else(|| "the achievements page is not open".to_string())
}

/// The page is inside the canvas.
fn page_fits(g: &RenethackGame) -> Result<(), String> {
    let r = page(g)?.rect();
    let size = g.canvas_size();
    if !Rect2::new(Vector2::ZERO, size).grow(1.0).encloses(r) {
        return Err(format!(
            "the achievements page ({:.0}×{:.0} at {:.0},{:.0}) is not inside the {:.0}×{:.0} canvas",
            r.size.x, r.size.y, r.position.x, r.position.y, size.x, size.y
        ));
    }
    Ok(())
}

/// The toast is inside the canvas and clear of the HUD's blocks (the
/// prompt banner among them).
fn toast_fits(g: &RenethackGame) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no ui")?;
    let r = ui.toast.rect().ok_or("the toast is not shown")?;
    let size = g.canvas_size();
    if !Rect2::new(Vector2::ZERO, size).grow(1.0).encloses(r) {
        return Err(format!("the toast at {r:?} is not inside the canvas"));
    }
    if let Some((name, b)) = ui
        .hud
        .blocks()
        .into_iter()
        .find(|(_, b)| b.intersects_exclude_borders(r))
    {
        return Err(format!(
            "the toast at {:.0},{:.0} {:.0}×{:.0} covers the {name} at {:.0},{:.0} {:.0}×{:.0} \
             (canvas {:.0}×{:.0})",
            r.position.x,
            r.position.y,
            r.size.x,
            r.size.y,
            b.position.x,
            b.position.y,
            b.size.x,
            b.size.y,
            size.x,
            size.y
        ));
    }
    Ok(())
}

/// A debug level teleport to the special level whose menu entry names
/// `level`, until `arrived` holds.
fn teleport(
    pick: fn(&RenethackGame) -> Result<KeyInput, String>,
    arrived: &'static str,
) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom(arrived, pick),
        Step::Request("a command on the new level", |p| *p == Prompt::Command),
    ]
}

fn pad_press(b: PadButton) -> Step {
    Step::Pad(vec![PadEv::Button(b, true), PadEv::Button(b, false)])
}

pub(super) fn achievements() -> Vec<Step> {
    let mut steps = vec![
        Step::Call(
            "a fresh store, a mock Steam, the next game in debug mode",
            |g: &mut RenethackGame| {
                let path = g.paths.as_ref().ok_or("no paths")?.achievements();
                let _ = std::fs::remove_file(&path);
                let mock = Mock::default();
                MOCK.with(|m| *m.borrow_mut() = Some(mock.clone()));
                g.achievements = Some(Tracker::start(
                    Achievements::built_in(),
                    Some(path),
                    Box::new(mock),
                ));
                g.debug_mode = true;
                Ok(())
            },
        ),
        Step::Wait("the title screen, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        // the page from the title: nothing earned yet, the secrets hidden
        Step::Push(UiEvent::OpenAchievements),
        Step::Wait("the achievements page over the title", |g| {
            Ok(g.achievements_open())
        }),
        Step::Shot("achievements-title"),
        Step::Call("nothing earned, four secrets; the page fits", |g| {
            let p = page(g)?;
            if !p.earned_ids().is_empty() || p.secret_count() != 4 {
                return Err(format!(
                    "earned {:?}, {} secrets",
                    p.earned_ids(),
                    p.secret_count()
                ));
            }
            page_fits(g)
        }),
        Step::Push(UiEvent::Key(KeyInput::plain(Key::Escape))),
        Step::Wait("the title again", |g| Ok(screen(g) == Some("title"))),
    ];
    steps.extend(start());
    steps.push(Step::Wait("the first level drawn", |g| {
        Ok(g.world.map.hero().is_some() && camera_settled(g)?)
    }));
    // a debug game earns nothing: depth 10 is not earned
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request(
            "to what level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("10".into())),
        // what lies where the hero lands may be shown first
        Step::AnswerUntil(' ', "a command on level 10", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command)))
                && g.world.status.number("leveldesc") == Some(10))
        }),
    ]);
    steps.push(Step::Call("a debug game earned nothing", |g| {
        let t = tracker(g)?;
        if !t.store().unlocked.is_empty() || !told().is_empty() {
            return Err(format!(
                "a debug game earned {:?}; Steam was told {:?}",
                t.store().unlocked.keys().collect::<Vec<_>>(),
                told()
            ));
        }
        Ok(())
    }));
    // the override: the same notice earns it, and the toast tells of it
    steps.push(Step::Call(
        "the self-tests' override",
        |g: &mut RenethackGame| {
            g.achievements
                .as_mut()
                .ok_or("no tracker")?
                .debug_earns(true);
            Ok(())
        },
    ));
    steps.push(Step::Wait("depth 10 earned", |g| earned(g, "depth_10")));
    steps.push(Step::Wait("the toast tells of depth 10", |g| {
        let ui = g.ui.as_ref().ok_or("no ui")?;
        Ok(ui.toast.shown() == Some("depth_10") && ui.toast.settled())
    }));
    steps.push(Step::Shot("achievement-toast"));
    steps.push(Step::Call("the toast fits under the prompt banner", |g| {
        toast_fits(g)
    }));
    // the Mines, Sokoban
    steps.extend(teleport(|g| entry_key(g, "minetn"), "Minetown"));
    steps.push(Step::Wait("the Mines earned", |g| earned(g, "mines")));
    steps.extend(teleport(|g| entry_key(g, "soko"), "Sokoban"));
    steps.push(Step::Wait("Sokoban earned", |g| earned(g, "sokoban")));
    // a level-up past two titles at once: their toasts one after another
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        // a debug command: the palette lists only the player's, the
        // engine knows it by its name
        Step::Dialog(DialogEvent::ExtCmd(Some("levelchange".into()))),
        Step::Request(
            "to what experience level",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("experience level")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("6".into())),
        Step::Wait("the first two ranks earned", |g| {
            Ok(earned(g, "rank_1")? && earned(g, "rank_2")?)
        }),
        Step::Request("a command after the level-up", |p| *p == Prompt::Command),
        Step::Wait("the first rank's toast", |g| {
            Ok(g.ui.as_ref().and_then(|ui| ui.toast.shown()) == Some("rank_1"))
        }),
        Step::Wait("the second rank's toast after the first's", |g| {
            Ok(g.ui.as_ref().and_then(|ui| ui.toast.shown()) == Some("rank_2"))
        }),
    ]);
    steps.push(Step::Call(
        "each earned once, and Steam told each by its API name",
        |g| {
            let mut got = told();
            got.sort();
            got.retain(|s| s != "ACH_MINETOWN");
            let mut want: Vec<String> = EARNED.iter().map(|(_, s)| s.to_string()).collect();
            want.sort();
            if got != want {
                return Err(format!("Steam was told {got:?}, not {want:?}"));
            }
            let t = tracker(g)?;
            let kept: Vec<&String> = t.store().unlocked.keys().collect();
            if kept.len() != told().len() {
                return Err(format!(
                    "the store keeps {kept:?}, Steam was told {:?}",
                    told()
                ));
            }
            godot_print!("selftest: achievements: earned {kept:?}");
            Ok(())
        },
    ));
    // the page in the game: the earned ones; the arrows and the d-pad
    // choose, B goes back
    steps.extend([
        Step::Push(UiEvent::OpenAchievements),
        Step::Wait("the achievements page over the game", |g| {
            Ok(g.achievements_open())
        }),
        Step::Shot("achievements"),
        Step::Call("the page lists the earned ones; it fits", |g| {
            let p = page(g)?;
            let mut shown: Vec<&str> = p.earned_ids();
            shown.retain(|id| *id != "minetown");
            shown.sort_unstable();
            let mut want: Vec<&str> = EARNED.iter().map(|(id, _)| *id).collect();
            want.sort_unstable();
            if shown != want {
                return Err(format!("the page shows {shown:?} earned"));
            }
            CHOSEN.store(p.selected(), Ordering::Relaxed);
            page_fits(g)
        }),
        Step::Push(UiEvent::Key(KeyInput::plain(Key::Right))),
        Step::Call("→ chooses the next medallion", |g| {
            let want = CHOSEN.load(Ordering::Relaxed) + 1;
            let got = page(g)?.selected();
            if got != want {
                return Err(format!("{got} chosen, not {want}"));
            }
            Ok(())
        }),
        pad_press(PadButton::Down),
        Step::Wait("the d-pad's ↓ chooses a row down", |g| {
            Ok(page(g)?.selected() == CHOSEN.load(Ordering::Relaxed) + 1 + 10)
        }),
        Step::Shot("achievements-chosen"),
        pad_press(PadButton::B),
        Step::Wait("B goes back to the game", |g| {
            Ok(!g.achievements_open() && g.state == GameState::Playing && screen(g).is_none())
        }),
        Step::Wait("the command still waits after the page", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]);
    steps.extend(quit());
    steps
}
