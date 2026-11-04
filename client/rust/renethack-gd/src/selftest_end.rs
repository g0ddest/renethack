//! The end screen (`end_view`), staged three ways and pictured each time.
//! `end-death` plays a debug game set up in English: a wish for Mjollnir,
//! a level-up past two ranks and a teleport to level 10 (the chronicle
//! and the achievements fill), then, in the language asked for, a
//! cockatrice corpse eaten: the stone says how. `end-quit` gives up a
//! normal game over a top ten of five earlier games, then a new game
//! taken with the pad's A climbs out of the dungeon: the top ten as a
//! table, an escape's page. `end-ascend` teleports a debug game to the
//! Astral Plane (the Amulet given), finds the hero's own high altar
//! (each told apart only from beside it; the Amulet's wish on arriving
//! is an amulet of life saving) and offers the Amulet there:
//! the ascension's page. Each time the page fits the canvas, its words
//! are the language's (none left in English, on the way there either),
//! the focus is on the title's button, and the arrows or the pad scroll a
//! body longer than the canvas. `end-beat` presses Space, Enter and the
//! pad's A and B during the death's beat: the page stays, and starts
//! nothing until a key is let go and pressed again.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use godot::classes::{Control, Engine, Label};
use godot::obj::Singleton;
use godot::prelude::*;
use nh_world::{Key, KeyInput, Prompt, Terrain};

use super::{
    DialogEvent, PadEv, Step, command, ctrl_key, fail_on_error_screen, is_menu, key, letter_of,
    pad_event, screen, start, tap, terrain_cells,
};
use crate::end_view::{self, EndHow};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, Lang};
use crate::tr;
use crate::ui_events::UiEvent;

/// The language the test was started in: a debug game is set up in
/// English (the Russian pickers would answer its questions), the end
/// is seen in it.
static ASKED: Mutex<Option<Lang>> = Mutex::new(None);

/// English found on the way to the end, in another language.
static ENGLISH: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The frame the page came up on.
static SHOWN_AT: AtomicI64 = AtomicI64::new(0);

/// The Astral Plane: the altar tried now, and the one found the hero's.
static ALTAR: AtomicUsize = AtomicUsize::new(0);
/// The teleports beside an altar: each tries the next cell round it.
static TRIES: AtomicUsize = AtomicUsize::new(0);
static MINE: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// Five games before this one, as topten.c keeps them in `record`
/// (version, points, dungeon, level, deepest, hit points and their most,
/// deaths, dates, uid, role, race, gender, alignment, name and death).
const RECORD: &str = "\
5.0.0 4210 0 7 9 -1 54 1 20261012 20261010 501 Sam Hum Mal Law Musashi,killed by a soldier ant
5.0.0 1875 2 5 5 -1 38 1 20261020 20261019 501 Wiz Elf Fem Cha Morgana,killed by a gnome lord
5.0.0 960 0 4 4 -2 22 1 20261024 20261023 501 Arc Dwa Fem Law Indy,petrified by a chickatrice
5.0.0 312 0 2 2 -1 16 1 20261025 20261025 501 Val Hum Fem Neu Brunhild,killed by a jackal
5.0.0 88 0 1 1 9 15 1 20261026 20261026 501 Tou Hum Mal Neu Twoflower,quit
";

/// The names of those games (Latin in every language).
const NAMES: [&str; 5] = ["Musashi", "Morgana", "Indy", "Brunhild", "Twoflower"];

/// Key names a hint may show in any language.
const KEY_NAMES: [&str; 6] = ["PgUp", "PgDn", "Tab", "Enter", "Esc", "Space"];

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

fn asked() -> Lang {
    ASKED.lock().ok().and_then(|l| *l).unwrap_or(Lang::En)
}

fn asking(p: &Prompt, what: &str) -> bool {
    matches!(p, Prompt::Text { query, .. } if query.contains(what))
}

/// The game set up in English (`debug`: in debug mode, seed 7; else
/// seed 42), the language asked for kept for the end.
fn set_up(debug: bool) -> Step {
    if debug {
        Step::Call("a debug game set up in English", |g| {
            begin(g)?;
            g.debug_mode = true;
            g.seed = Some(7);
            Ok(())
        })
    } else {
        Step::Call("a game set up in English", |g| {
            begin(g)?;
            g.seed = Some(42);
            Ok(())
        })
    }
}

fn begin(g: &mut RenethackGame) -> Result<(), String> {
    *ASKED.lock().map_err(|e| e.to_string())? = Some(i18n::lang());
    ENGLISH.lock().map_err(|e| e.to_string())?.clear();
    ALTAR.store(0, Ordering::Relaxed);
    TRIES.store(0, Ordering::Relaxed);
    *MINE.lock().map_err(|e| e.to_string())? = None;
    g.push_ui(UiEvent::SetLanguage(Lang::En));
    Ok(())
}

/// A debug game earns as a normal one would: the page has medals.
fn earns() -> Step {
    Step::Call("a debug game earns", |g: &mut RenethackGame| {
        g.achievements
            .as_mut()
            .ok_or("no tracker")?
            .debug_earns(true);
        Ok(())
    })
}

fn in_asked_language() -> Step {
    Step::Call("the language asked for", |g| {
        g.push_ui(UiEvent::SetLanguage(asked()));
        Ok(())
    })
}

/// A debug wish.
fn wish(what: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", |p| asking(p, "For what do you wish")),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::AnswerUntil(' ', "a command after the wish", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]
}

/// The words of the open dialog that are English in another language.
fn english_on_the_way(g: &RenethackGame) -> Result<(), String> {
    if i18n::lang() != Lang::Ru {
        return Ok(());
    }
    let Some(ui) = g.ui.as_ref() else {
        return Ok(());
    };
    let words = ui.dialogs.shown_words();
    let mut allowed: Vec<&str> = KEY_NAMES.to_vec();
    allowed.extend(NAMES);
    allowed.extend(["Hero", "wizard"]);
    let left = end_view::english_left(&words, &allowed);
    if !left.is_empty() {
        let mut seen = ENGLISH.lock().map_err(|e| e.to_string())?;
        for l in left {
            if !seen.contains(&l) {
                godot_print!("selftest: English on the way: {l}");
                seen.push(l);
            }
        }
    }
    Ok(())
}

/// The open question is a debug death's, in its own words (`key`'s
/// title, line and answers), not NetHack's bare "Die?" or "Save bones?".
fn debug_question_says(g: &RenethackGame, key: &str) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let words = ui.dialogs.shown_words();
    for part in ["title", "about", "yes", "no"] {
        let want = i18n::tr(&format!("{key}-{part}"));
        if !words.iter().any(|w| w.contains(&want)) {
            return Err(format!("{key}: no {want:?} among {words:?}"));
        }
    }
    Ok(())
}

/// The questions of the end answered no (the windows closed) until the
/// end page is up, the dialogs on the way looked at.
fn on_the_way() -> Step {
    Step::AnswerUntil('n', "the end page", |g| {
        fail_on_error_screen(g)?;
        english_on_the_way(g)?;
        if screen(g) == Some("end") {
            SHOWN_AT.store(frames(), Ordering::Relaxed);
            return Ok(true);
        }
        Ok(false)
    })
}

/// Every label under `node` as shown, with its rectangle.
fn labels(node: &Gd<Node>, out: &mut Vec<(String, Rect2)>) {
    if let Ok(c) = node.clone().try_cast::<Control>()
        && !c.is_visible_in_tree()
    {
        return;
    }
    if let Ok(l) = node.clone().try_cast::<Label>() {
        out.push((l.get_text().to_string(), l.get_global_rect()));
    }
    for c in node.get_children().iter_shared() {
        labels(&c, out);
    }
}

/// A label under `node` with more lines than its place shows.
fn cut_label(node: &Gd<Node>) -> Option<String> {
    if let Ok(c) = node.clone().try_cast::<Control>()
        && !c.is_visible_in_tree()
    {
        return None;
    }
    if let Ok(l) = node.clone().try_cast::<Label>()
        && !l.get_text().is_empty()
        && l.get_visible_line_count() < l.get_line_count()
    {
        return Some(format!(
            "{:?} shows {} of its {} lines",
            l.get_text().to_string(),
            l.get_visible_line_count(),
            l.get_line_count()
        ));
    }
    node.get_children()
        .iter_shared()
        .find_map(|c| cut_label(&c))
}

/// The end page is up, laid out (a few frames after it came), and tells
/// the game ended `how`; it fits the canvas, no label wider than the
/// page, its buttons on screen, the focus on a new game; in another
/// language no English is left on it, nor was on the way.
fn end_page_ok(g: &RenethackGame, how: EndHow) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    if screen(g) != Some("end") || frames() - SHOWN_AT.load(Ordering::Relaxed) < 4 {
        return Ok(false);
    }
    let ui = g.ui.as_ref().ok_or("no UI")?;
    // the beat first: the page takes no input until it is over
    if ui.screens.end_beat() {
        SHOWN_AT.store(frames(), Ordering::Relaxed);
        return Ok(false);
    }
    let s = ui.screens.end_summary().ok_or("no summary")?;
    if s.how != how {
        return Err(format!("the game ended {:?}, not {how:?}", s.how));
    }
    let (panel, _, _) = ui.screens.end_rects().ok_or("no end page")?;
    let size = g.canvas_size();
    let canvas = Rect2::new(Vector2::ZERO, size).grow(1.0);
    if !canvas.encloses(panel) {
        return Err(format!(
            "the end page at {:.0},{:.0} {:.0}×{:.0} is not inside the {:.0}×{:.0} canvas",
            panel.position.x, panel.position.y, panel.size.x, panel.size.y, size.x, size.y
        ));
    }
    let root = ui.screens.end_root().ok_or("no end page")?;
    let mut shown = Vec::new();
    labels(&root.clone().upcast(), &mut shown);
    // the body's own are scrolled out of it, down or up
    let mut in_body = Vec::new();
    let scroll = ui.screens.end_body().ok_or("no end page")?;
    labels(&scroll.upcast(), &mut in_body);
    let inside = panel.grow(1.0);
    for (text, r) in &shown {
        if r.size.x < 1.0 || text.trim().is_empty() {
            continue;
        }
        let scrolled = in_body.iter().any(|(t, b)| t == text && b == r);
        let across = r.position.x >= inside.position.x && r.end().x <= inside.end().x;
        let down = scrolled || inside.encloses(*r);
        if !across || !down {
            return Err(format!(
                "{text:?} at {:.0},{:.0} {:.0}×{:.0} is out of the page at {:.0},{:.0} {:.0}×{:.0}",
                r.position.x,
                r.position.y,
                r.size.x,
                r.size.y,
                panel.position.x,
                panel.position.y,
                panel.size.x,
                panel.size.y
            ));
        }
    }
    if let Some(cut) = cut_label(&root.clone().upcast()) {
        return Err(cut);
    }
    for name in ["end-new-game", "end-to-title", "end-quit"] {
        if !ui.screens.has_button(&i18n::tr(name)) {
            return Err(format!("no {name} button"));
        }
    }
    if ui.screens.nav_focused().as_deref() != Some(tr!("end-to-title").as_str()) {
        return Err(format!(
            "the focus is on {:?}, not the title's",
            ui.screens.nav_focused()
        ));
    }
    // what the page must say
    let texts: Vec<String> = shown.iter().map(|(t, _)| t.clone()).collect();
    let has = |t: &str| texts.iter().any(|x| x.contains(t));
    let want = match how {
        EndHow::Quit => crate::layouts::upstart(&tr!("end-stone-quit", female = "yes")),
        EndHow::Ascended => tr!("end-ascended", female = "yes"),
        EndHow::Escaped => tr!("end-escaped", female = "yes"),
        EndHow::Died => crate::layouts::death(&s.stone().map(|t| t.death).unwrap_or_default()),
    };
    for t in [
        want,
        tr!("end-chronicle"),
        tr!("end-topten"),
        tr!("end-achievements"),
    ] {
        if t.is_empty() || !has(&t) {
            return Err(format!("the page does not say {t:?}: {texts:?}"));
        }
    }
    if s.chronicle.is_empty() {
        return Err("the chronicle is empty".into());
    }
    if s.numbers().turns.is_none_or(|t| t < 1) {
        return Err(format!("no turns: {:?}", s.numbers()));
    }
    if i18n::lang() == Lang::Ru {
        let mut allowed: Vec<&str> = KEY_NAMES.to_vec();
        allowed.extend(NAMES);
        allowed.push(s.name.as_str());
        let left = end_view::english_left(&texts, &allowed);
        if !left.is_empty() {
            return Err(format!("English on the page: {}", left.join("; ")));
        }
        let seen = ENGLISH.lock().map_err(|e| e.to_string())?;
        if !seen.is_empty() {
            return Err(format!("English on the way: {}", seen.join("; ")));
        }
    }
    godot_print!(
        "selftest: end page: {how:?}, {} events, {} achievements, {} scores, numbers {:?}",
        s.chronicle.len(),
        s.achievements.len(),
        s.scores().len(),
        s.numbers()
    );
    Ok(true)
}

/// The page checked and pictured as `name`; then, when its body is longer
/// than the canvas, the pad's d-pad and RB scroll it (pictured as
/// `name-scrolled`), LB and the d-pad back.
fn end_page(name: &'static str, check: super::Check) -> Vec<Step> {
    vec![
        // the beat: the words over the dark, the stone coming
        Step::Wait("the end's beat along", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.end_beat_at().is_none_or(|t| t > 0.7))
        }),
        Step::ShotIf(name_beat(name), |g| {
            Ok(g.ui
                .as_ref()
                .is_some_and(|ui| ui.screens.end_beat_at().is_some()))
        }),
        Step::Wait("the end page laid out", check),
        Step::Shot(name),
        Step::Call("the body scrolled down if it is longer", |g| {
            SHOWN_AT.store(frames(), Ordering::Relaxed);
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (_, _, range) = ui.screens.end_rects().ok_or("no end page")?;
            if range > 0 {
                pad_event(PadEv::Button(PadButton::Down, true));
                pad_event(PadEv::Button(PadButton::Down, false));
                for _ in 0..8 {
                    pad_event(PadEv::Button(PadButton::Rb, true));
                    pad_event(PadEv::Button(PadButton::Rb, false));
                }
            }
            Ok(())
        }),
        Step::Wait("scrolled, if it scrolls; the hints the pad's", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (_, _, range) = ui.screens.end_rects().ok_or("no end page")?;
            if range == 0 {
                return Ok(true);
            }
            let root = ui.screens.end_root().ok_or("no end page")?;
            let words = crate::pad_view::shown_words(&root);
            let keys = words.iter().any(|w| w.contains("PgUp"));
            let at = ui.screens.end_scrolled().unwrap_or(0);
            if !keys && at >= range {
                // at the end of the body: its last words above its bottom
                let (_, body, _) = ui.screens.end_rects().ok_or("no end page")?;
                let scroll = ui.screens.end_body().ok_or("no end page")?;
                let mut inside = Vec::new();
                labels(&scroll.upcast(), &mut inside);
                if let Some((text, r)) = inside
                    .iter()
                    .filter(|(t, r)| !t.trim().is_empty() && r.size.x >= 1.0)
                    .max_by(|a, b| a.1.end().y.total_cmp(&b.1.end().y))
                    && r.end().y > body.end().y + 1.0
                {
                    return Err(format!(
                        "scrolled to the end, {text:?} ends at {:.0}, under the body's bottom at {:.0}",
                        r.end().y,
                        body.end().y
                    ));
                }
                return Ok(true);
            }
            if frames() - SHOWN_AT.load(Ordering::Relaxed) > 60 {
                return Err(format!(
                    "scrolled {at} of {range}, the keys' hints still shown: {keys}"
                ));
            }
            Ok(false)
        }),
        Step::ShotIf(name_scrolled(name), |g| {
            Ok(g.ui
                .as_ref()
                .and_then(|ui| ui.screens.end_scrolled())
                .is_some_and(|at| at > 0))
        }),
        // the disclosures a part at a time: Y the next, X the one before
        Step::Wait("the disclosures on their first tab", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            match ui.screens.end_section() {
                Some((0, n)) if n >= 2 => Ok(true),
                other => Err(format!("the disclosures' tabs: {other:?}")),
            }
        }),
        Step::Call("Y: the next tab", |_| {
            pad_event(PadEv::Button(PadButton::Y, true));
            pad_event(PadEv::Button(PadButton::Y, false));
            Ok(())
        }),
        Step::Wait("the second tab", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.end_section().is_some_and(|(at, _)| at == 1))
        }),
        Step::Shot(name_tab(name)),
        Step::Call("X: back", |_| {
            pad_event(PadEv::Button(PadButton::X, true));
            pad_event(PadEv::Button(PadButton::X, false));
            Ok(())
        }),
        Step::Wait("the first tab again", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.end_section().is_some_and(|(at, _)| at == 0))
        }),
        Step::Call("back to the top", |_| {
            for _ in 0..8 {
                pad_event(PadEv::Button(PadButton::Lb, true));
                pad_event(PadEv::Button(PadButton::Lb, false));
            }
            pad_event(PadEv::Button(PadButton::Up, true));
            pad_event(PadEv::Button(PadButton::Up, false));
            Ok(())
        }),
        Step::Wait("back at the top, the focus still on the title's", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.end_scrolled().unwrap_or(0) == 0
                && ui.screens.nav_focused().as_deref() == Some(tr!("end-to-title").as_str()))
        }),
    ]
}

fn name_beat(name: &'static str) -> &'static str {
    match name {
        "end-death" => "end-death-beat",
        "end-quit" => "end-quit-beat",
        "end-escape" => "end-escape-beat",
        _ => "end-ascend-beat",
    }
}

fn name_tab(name: &'static str) -> &'static str {
    match name {
        "end-death" => "end-death-tab",
        "end-quit" => "end-quit-tab",
        "end-escape" => "end-escape-tab",
        _ => "end-ascend-tab",
    }
}

fn name_scrolled(name: &'static str) -> &'static str {
    match name {
        "end-death" => "end-death-scrolled",
        "end-quit" => "end-quit-scrolled",
        "end-escape" => "end-escape-scrolled",
        _ => "end-ascend-scrolled",
    }
}

pub(super) fn end_death() -> Vec<Step> {
    let mut steps = vec![set_up(true)];
    steps.extend(start());
    steps.push(earns());
    steps.extend(wish("blessed rustproof +2 Mjollnir"));
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        // a debug command: the engine knows it by its name
        Step::Dialog(DialogEvent::ExtCmd(Some("levelchange".into()))),
        Step::Request("to what experience level", |p| {
            asking(p, "experience level")
        }),
        Step::Dialog(DialogEvent::TextSubmitted("8".into())),
        Step::AnswerUntil(' ', "a command after the level-up", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Key(ctrl_key('v')),
        Step::Request("to what level", |p| asking(p, "level")),
        Step::Dialog(DialogEvent::TextSubmitted("10".into())),
        Step::AnswerUntil(' ', "a command on level 10", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command)))
                && g.world.status.number("leveldesc") == Some(10))
        }),
    ]);
    steps.extend(wish("cockatrice corpse"));
    steps.push(in_asked_language());
    steps.extend([
        key('e'),
        Step::KeysUntil("the corpse eaten", |g| {
            fail_on_error_screen(g)?;
            match g.pending.as_ref().map(|(_, p)| p) {
                Some(Prompt::Choice { query, .. }) if query.contains("Die?") => Ok(None),
                // a wished corpse lies on the floor (the wish's caution)
                Some(Prompt::Choice { query, .. }) => Ok(Some(KeyInput::plain(Key::Char(
                    if query.contains("cockatrice") {
                        'y'
                    } else {
                        'n'
                    },
                )))),
                // the command 'e' answered, or the eating given up
                Some(Prompt::Command) => Ok(Some(KeyInput::plain(Key::Escape))),
                Some(_) => {
                    let c = letter_of(g, "corpse").ok_or("no corpse")?;
                    Ok(Some(KeyInput::plain(Key::Char(c))))
                }
                None => Ok(None),
            }
        }),
        Step::Request(
            "Die?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Die?")),
        ),
        Step::Call("Die? in its own words", |g| {
            debug_question_says(g, "dlg-debug-die")
        }),
        Step::Shot("end-death-die"),
        // opened for the keys: the pad's first press shows the default
        tap(PadButton::Right),
        Step::Wait("→: on the default, to live on", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            match ui.dialogs.choice_focus() {
                None => Ok(false),
                Some('n') => Ok(true),
                Some(c) => Err(format!("the first press went to {c:?}, not the default n")),
            }
        }),
        Step::Shot("end-death-die-pad"),
        key('y'),
        Step::Wait("Save bones? (when the level keeps bones) or the end", |g| {
            fail_on_error_screen(g)?;
            match g.pending.as_ref().map(|(_, p)| p) {
                Some(Prompt::Choice { query, .. }) if query.contains("bones") => {
                    debug_question_says(g, "dlg-debug-bones")?;
                    let ui = g.ui.as_ref().ok_or("no UI")?;
                    match ui.dialogs.choice_focus() {
                        Some('n') => Ok(true),
                        other => Err(format!("Save bones? opens on {other:?}, not n")),
                    }
                }
                _ => Ok(screen(g) == Some("end")),
            }
        }),
        Step::ShotIf("end-death-bones", |g| {
            Ok(matches!(g.pending.as_ref().map(|(_, p)| p),
                Some(Prompt::Choice { query, .. }) if query.contains("bones")))
        }),
        on_the_way(),
    ]);
    steps.extend(end_page("end-death", |g| end_page_ok(g, EndHow::Died)));
    steps.push(Step::Call("the stone, the medals", |g| {
        let ui = g.ui.as_ref().ok_or("no UI")?;
        let s = ui.screens.end_summary().ok_or("no summary")?;
        let stone = s.stone().ok_or("no stone in the end window")?;
        if !stone.death.contains("cockatrice") {
            return Err(format!("the stone says {stone:?}"));
        }
        let ids: Vec<&str> = s.achievements.iter().map(|(a, _)| a.id.as_str()).collect();
        for id in ["rank_1", "rank_2", "depth_10"] {
            if !ids.contains(&id) {
                return Err(format!("no {id} among {ids:?}"));
            }
        }
        if !s.achievements.iter().any(|(_, new)| *new) {
            return Err("none marked new".into());
        }
        Ok(())
    }));
    steps
}

pub(super) fn end_quit() -> Vec<Step> {
    let mut steps = vec![
        set_up(false),
        Step::Call("five games before this one", |g| {
            let pg = g.playground();
            std::fs::create_dir_all(&pg).map_err(|e| e.to_string())?;
            std::fs::write(pg.join("record"), RECORD).map_err(|e| e.to_string())
        }),
    ];
    steps.extend(start());
    steps.push(in_asked_language());
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("quit".into())),
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("quit")),
        ),
        key('y'),
        on_the_way(),
    ]);
    steps.extend(end_page("end-quit", |g| end_page_ok(g, EndHow::Quit)));
    steps.push(Step::Call("the top ten as a table", |g| {
        let ui = g.ui.as_ref().ok_or("no UI")?;
        let s = ui.screens.end_summary().ok_or("no summary")?;
        let rows = s.scores();
        if rows.len() < 6 || !rows.iter().any(|r| r.mine) {
            return Err(format!("the top ten has {rows:?}"));
        }
        for n in NAMES {
            if !rows.iter().any(|r| r.words.starts_with(n)) {
                return Err(format!("no {n} in {rows:?}"));
            }
        }
        Ok(())
    }));
    // a new game with the pad (left of the title's, A), climbed out of
    // the dungeon
    steps.extend([
        tap(PadButton::Left),
        Step::Wait("the focus on a new game", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.nav_focused().as_deref() == Some(tr!("end-new-game").as_str()))
        }),
        tap(PadButton::A),
        Step::Wait("the creation form", |g| Ok(screen(g) == Some("creation"))),
        Step::Push(UiEvent::StartCharacter(super::smoke_choice())),
        Step::Request("the first command", command),
        key('<'),
        Step::Request(
            "no return",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("no return")),
        ),
        key('y'),
        on_the_way(),
    ]);
    steps.extend(end_page("end-escape", |g| end_page_ok(g, EndHow::Escaped)));
    steps
}

/// The hero's alignment as the status says it, in lower case.
fn alignment(g: &RenethackGame) -> String {
    g.world
        .status
        .get("align")
        .map(|a| a.trim().to_lowercase())
        .unwrap_or_default()
}

/// The high altars of the level, in map order.
fn altars(g: &RenethackGame) -> Vec<(i32, i32)> {
    terrain_cells(g)
        .into_iter()
        .filter(|(_, t)| *t == Terrain::Altar)
        .map(|(xy, _)| xy)
        .collect()
}

/// A step of the cursor (or the hero) from `from` towards `to`.
fn toward(from: (i32, i32), to: (i32, i32)) -> Option<char> {
    let d = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    super::audio_tests::STEPS
        .iter()
        .find(|(s, _)| *s == d)
        .map(|(_, c)| *c)
}

fn adjacent(a: (i32, i32), b: (i32, i32)) -> bool {
    a != b && (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
}

/// The key for the request pending on the Astral Plane: teleport (^T)
/// beside each high altar in turn, the cursor on the altar tells its
/// alignment from there (autodescribe), and the hero's own is the one
/// teleported onto. Dying is declined; a teleport the Amulet disturbs
/// is forced. None: the hero stands on their own altar.
fn to_my_altar(g: &RenethackGame) -> Result<Option<KeyInput>, String> {
    fail_on_error_screen(g)?;
    let plain = |c: char| Ok(Some(KeyInput::plain(Key::Char(c))));
    let hero = g.world.map.hero().ok_or("no hero")?;
    let mine = *MINE.lock().map_err(|e| e.to_string())?;
    match g.pending.as_ref().map(|(_, p)| p) {
        Some(Prompt::Choice { query, .. }) if query.contains("Die?") => return plain('n'),
        Some(Prompt::Choice { query, .. }) if query.contains("Override?") => return plain('y'),
        Some(Prompt::Choice { .. }) => return plain('n'),
        Some(Prompt::Command) => {}
        Some(_) => return Ok(Some(KeyInput::plain(Key::Escape))),
        None => return Ok(None),
    }
    let all = altars(g);
    if all.is_empty() {
        return Err("no altars on the map".into());
    }
    if !g.world.getpos {
        if mine == Some(hero) {
            return Ok(None);
        }
        // teleport: from here the cursor goes on
        return Ok(Some(ctrl_key('t')));
    }
    let cursor = g.world.cursor.unwrap_or(hero);
    let i = ALTAR.load(Ordering::Relaxed) % all.len();
    let altar = mine.unwrap_or(all[i]);
    if mine.is_none() && adjacent(hero, altar) && cursor == altar {
        // the cursor on it from beside it: its alignment told
        let said = g.world.transient.clone().unwrap_or_default().to_lowercase();
        if !said.contains("altar") {
            return Err(format!(
                "the cursor on the altar at {altar:?} says {said:?}"
            ));
        }
        if said.contains(&alignment(g)) {
            *MINE.lock().map_err(|e| e.to_string())? = Some(altar);
            return plain('.');
        }
        godot_print!("selftest: the altar at {altar:?} is not the hero's: {said}");
        ALTAR.fetch_add(1, Ordering::Relaxed);
        return Ok(Some(KeyInput::plain(Key::Escape)));
    }
    // where the cursor goes: onto the altar (from beside it, or the
    // hero's own), else to a cell round it (another after each try)
    let goal = if mine.is_some() || adjacent(hero, altar) {
        altar
    } else {
        const ROUND: [(i32, i32); 8] = [
            (0, 1),
            (0, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (1, 1),
            (-1, -1),
            (1, -1),
        ];
        let (dx, dy) = ROUND[TRIES.load(Ordering::Relaxed) % ROUND.len()];
        (altar.0 + dx, altar.1 + dy)
    };
    if cursor == goal {
        if goal != altar {
            TRIES.fetch_add(1, Ordering::Relaxed);
        }
        return plain('.');
    }
    match toward(cursor, goal) {
        Some(c) => plain(c),
        None => plain('.'),
    }
}

pub(super) fn end_ascend() -> Vec<Step> {
    let mut steps = vec![set_up(true)];
    steps.extend(start());
    steps.push(earns());
    steps.extend([
        Step::Key(ctrl_key('v')),
        Step::Request("to what level", |p| asking(p, "level")),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the levels to teleport to", is_menu),
        Step::KeyFrom("the Astral Plane", |g| super::entry_key(g, "astral")),
        // the Amulet bestows a wish on the hero arriving with it
        Step::AnswerUntil('n', "the Amulet's wish", |g| {
            fail_on_error_screen(g)?;
            Ok(g.pending
                .as_ref()
                .is_some_and(|(_, p)| asking(p, "For what do you wish")))
        }),
        Step::Dialog(DialogEvent::TextSubmitted(
            "blessed amulet of life saving".into(),
        )),
        Step::AnswerUntil('n', "a command on the Astral Plane", |g| {
            fail_on_error_screen(g)?;
            let astral = g
                .world
                .status
                .get("leveldesc")
                .is_some_and(|d| d.contains("Astral"));
            Ok(astral && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // the level mapped: its three high altars
        Step::Key(ctrl_key('f')),
        Step::AnswerUntil('n', "a command after the map", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && altars(g).len() == 3)
        }),
        Step::KeysUntil("the hero on their own high altar", to_my_altar),
        Step::AnswerUntil('n', "a command on the altar", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && !g.world.getpos)
        }),
        in_asked_language(),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("offer".into())),
        Step::Request("what to offer", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the Amulet", |g| {
            let c = letter_of(g, "Amulet of Yendor").ok_or("no Amulet")?;
            Ok(KeyInput::plain(Key::Char(c)))
        }),
        on_the_way(),
    ]);
    steps.extend(end_page("end-ascend", |g| end_page_ok(g, EndHow::Ascended)));
    steps.push(Step::Call("the ascension's medals", |g| {
        let ui = g.ui.as_ref().ok_or("no UI")?;
        let s = ui.screens.end_summary().ok_or("no summary")?;
        let ids: Vec<&str> = s.achievements.iter().map(|(a, _)| a.id.as_str()).collect();
        for id in ["ascension", "ascend_val", "astral"] {
            if !ids.contains(&id) {
                return Err(format!("no {id} among {ids:?}"));
            }
        }
        Ok(())
    }));
    steps
}

/// The direction key (vi-keys, the scenarios' Classic keys) from the hero
/// toward a foe beside them, if one stands there.
fn toward_foe(g: &RenethackGame) -> Option<char> {
    let (hx, hy) = g.world.map.hero()?;
    let foe = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
        .filter(|d| *d != (0, 0))
        .find(|(dx, dy)| {
            g.world
                .map
                .cell(hx + dx, hy + dy)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|gl| {
                    gl.kind == nh_protocol::GlyphKind::Mon
                        && gl.flags & (nh_protocol::mg::PET | nh_protocol::mg::HERO) == 0
                })
        })?;
    toward((hx, hy), (hx + foe.0, hy + foe.1))
}

/// The fight prefix sent: its direction comes next.
static FIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The tab the sheet's run shows next (0: the first).
static NEXT_TAB: AtomicUsize = AtomicUsize::new(0);

/// The frame a step of the sheet's run began on.
static SHEET_FRAME: AtomicI64 = AtomicI64::new(0);

/// A few frames after the last step of the sheet's run: laid out.
fn sheet_settled(_: &RenethackGame) -> Result<bool, String> {
    Ok(frames() > SHEET_FRAME.load(Ordering::Relaxed) + 4)
}

/// The next tab of the end's disclosures shown and shot: its start (the
/// tabs at the body's top), then the body at its end (`end`; none when
/// there are no more tabs).
fn tab_shot(start: &'static str, end: &'static str) -> [Step; 8] {
    [
        Step::Call("the next tab", |_| {
            if NEXT_TAB.fetch_add(1, Ordering::Relaxed) > 0 {
                pad_event(PadEv::Button(PadButton::Y, true));
                pad_event(PadEv::Button(PadButton::Y, false));
            }
            Ok(())
        }),
        Step::Wait("that tab shown", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let want = NEXT_TAB.load(Ordering::Relaxed) - 1;
            Ok(ui
                .screens
                .end_section()
                .is_some_and(|(at, n)| at == want % n.max(1)))
        }),
        Step::Call("the tabs at the body's top", |g| {
            if let Some(ui) = g.ui.as_mut() {
                ui.screens.scroll_to_sections();
            }
            SHEET_FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("laid out", sheet_settled),
        Step::ShotIf(start, |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let want = NEXT_TAB.load(Ordering::Relaxed) - 1;
            Ok(ui.screens.end_section().is_some_and(|(_, n)| want < n))
        }),
        Step::Call("the body at its end", |_| {
            for _ in 0..12 {
                pad_event(PadEv::Button(PadButton::Rb, true));
                pad_event(PadEv::Button(PadButton::Rb, false));
            }
            SHEET_FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("scrolled", sheet_settled),
        Step::ShotIf(end, |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let want = NEXT_TAB.load(Ordering::Relaxed) - 1;
            Ok(ui.screens.end_section().is_some_and(|(_, n)| want < n))
        }),
    ]
}

/// The end's disclosures whole, for a sheet: a Rogue's sack with a potion
/// in it, a jackal slain, newts genocided, a death; the page's top, its
/// end (the conduct), then each tab.
pub(super) fn end_tabs() -> Vec<Step> {
    let mut steps = vec![set_up(true)];
    steps.extend(super::start_as(crate::ui_events::CharacterChoice {
        name: "Hero".into(),
        role: "rogue".into(),
        race: "human".into(),
        gender: "female".into(),
        align: "chaotic".into(),
        profile: nh_world::KeyProfile::Classic,
    }));
    steps.push(earns());
    // the potion into the sack
    steps.extend([
        key('a'),
        Step::Request("what to apply", |p| !command(p)),
        Step::KeyFrom("the sack", |g| {
            letter_of(g, "sack")
                .map(|c| KeyInput::plain(Key::Char(c)))
                .ok_or_else(|| "no sack".into())
        }),
        Step::Request("what to do with the sack", |p| !command(p)),
        key('i'),
        // the potions' class, and every one of it
        Step::Request("what type to put in", is_menu),
        Step::KeyFrom("the potions' class", |g| {
            let Some((_, Prompt::Menu { items, .. })) = g.pending.as_ref() else {
                return Err("no menu".into());
            };
            (items.iter())
                .find(|i| i.str.as_deref().is_some_and(|t| t.contains("Potion")))
                .and_then(|i| u32::try_from(i.ch).ok().and_then(char::from_u32))
                .map(|c| KeyInput::plain(Key::Char(c)))
                .ok_or_else(|| "no potions' class".into())
        }),
        key('A'),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::AnswerUntil(' ', "a command, the potion in the sack", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]);
    // newts genocided
    steps.extend(wish("uncursed scroll of genocide"));
    steps.extend([
        key('r'),
        Step::Request("what to read", |p| !command(p)),
        Step::KeyFrom("the scroll", |g| {
            letter_of(g, "genocide")
                .or_else(|| letter_of(g, "scroll"))
                .map(|c| KeyInput::plain(Key::Char(c)))
                .ok_or_else(|| "no scroll".into())
        }),
        Step::Request("what to genocide", |p| asking(p, "genocide")),
        Step::Dialog(DialogEvent::TextSubmitted("newt".into())),
        Step::AnswerUntil(' ', "a command after the genocide", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]);
    // a jackal made and slain
    steps.extend([
        Step::Key(ctrl_key('g')),
        Step::Request("what to create", |p| asking(p, "Create what kind")),
        Step::Dialog(DialogEvent::TextSubmitted("jackal".into())),
        Step::AnswerUntil(' ', "a command, the jackal here", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::KeysUntil("the jackal slain", |g| {
            fail_on_error_screen(g)?;
            // F, then its direction at the next command prompt
            let after_f = FIGHT.swap(false, Ordering::Relaxed);
            match g.pending.as_ref().map(|(_, p)| p) {
                Some(Prompt::Command) if after_f => Ok(Some(KeyInput::plain(Key::Char(
                    toward_foe(g).unwrap_or('.'),
                )))),
                Some(Prompt::Command) => Ok(toward_foe(g).map(|_| {
                    FIGHT.store(true, Ordering::Relaxed);
                    KeyInput::plain(Key::Char('F'))
                })),
                Some(Prompt::FreeKey {
                    directions: true, ..
                }) => Ok(Some(KeyInput::plain(Key::Char(
                    toward_foe(g).unwrap_or('.'),
                )))),
                Some(_) => Ok(Some(KeyInput::plain(Key::Char(' ')))),
                None => Ok(None),
            }
        }),
        Step::Request("a command after the fight", command),
    ]);
    steps.push(in_asked_language());
    // the hero slain by their own player
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        Step::Request("the monster to slay", command),
        key('.'),
        Step::Request("the suicide's question", |p| asking(p, "suicide")),
        Step::Dialog(DialogEvent::TextSubmitted("yes".into())),
        Step::Request(
            "Die?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Die?")),
        ),
        key('y'),
        on_the_way(),
        Step::Wait("the end page laid out, its beat over", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.screens.end_beat() {
                SHOWN_AT.store(frames(), Ordering::Relaxed);
                return Ok(false);
            }
            Ok(ui.screens.end_section().is_some()
                && frames() - SHOWN_AT.load(Ordering::Relaxed) > 4)
        }),
        Step::Shot("end-tabs-top"),
    ]);
    steps.push(Step::Call("", |_| {
        NEXT_TAB.store(0, Ordering::Relaxed);
        Ok(())
    }));
    for (start, end) in [
        ("end-tabs-1", "end-tabs-1-end"),
        ("end-tabs-2", "end-tabs-2-end"),
        ("end-tabs-3", "end-tabs-3-end"),
        ("end-tabs-4", "end-tabs-4-end"),
        ("end-tabs-5", "end-tabs-5-end"),
    ] {
        steps.extend(tab_shot(start, end));
    }
    steps
}

/// A key through Godot's input as the keyboard sends it: pressed (a
/// repeat of one held: `echo`) or let go.
fn key_event(k: godot::global::Key, pressed: bool, echo: bool) {
    let mut ev = godot::classes::InputEventKey::new_gd();
    ev.set_keycode(k);
    ev.set_physical_keycode(k);
    ev.set_pressed(pressed);
    ev.set_echo(echo);
    godot::classes::Input::singleton().parse_input_event(&ev);
}

/// Still the end page, its focus on the title's: nothing was taken.
fn still_the_end(g: &RenethackGame) -> Result<(), String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if screen(g) != Some("end") {
        return Err(format!("the end page left for {:?}", screen(g)));
    }
    let focused = ui.screens.nav_focused();
    if focused.as_deref() != Some(tr!("end-to-title").as_str()) {
        return Err(format!("the focus moved to {focused:?}"));
    }
    Ok(())
}

/// The death's beat takes no input: Space, Enter and the pad's A and B
/// tapped as the page comes up, a Space and an arrow held through it.
/// The page stays, nothing starts, the held keys' repeats count for
/// nothing; let go and pressed again, Enter takes the focused button
/// (the title's).
pub(super) fn end_beat() -> Vec<Step> {
    use godot::global::Key as GKey;
    let mut steps = vec![set_up(true)];
    steps.extend(start());
    steps.push(in_asked_language());
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        Step::Request("the monster to slay", command),
        key('.'),
        Step::Request("the suicide's question", |p| asking(p, "suicide")),
        Step::Dialog(DialogEvent::TextSubmitted("yes".into())),
        Step::Request(
            "Die?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Die?")),
        ),
        key('y'),
        on_the_way(),
        Step::Call("keys and buttons pressed as the beat begins", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.screens.end_beat() {
                return Err("the end page came without its beat".into());
            }
            for k in [GKey::SPACE, GKey::ENTER, GKey::ESCAPE] {
                key_event(k, true, false);
                key_event(k, false, false);
            }
            for b in [
                PadButton::A,
                PadButton::B,
                PadButton::Start,
                PadButton::Left,
            ] {
                pad_event(PadEv::Button(b, true));
                pad_event(PadEv::Button(b, false));
            }
            // held through the beat
            key_event(GKey::SPACE, true, false);
            key_event(GKey::LEFT, true, false);
            Ok(())
        }),
        // the game darkening behind the words
        Step::Shot("end-beat-dark"),
        Step::Wait("the beat's words over the game", |g| {
            still_the_end(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.end_beat_at().is_some_and(|t| t > 0.7))
        }),
        Step::Shot("end-beat-words"),
        Step::Call("Enter and A again in the beat", |g| {
            still_the_end(g)?;
            key_event(GKey::ENTER, true, false);
            key_event(GKey::ENTER, false, false);
            pad_event(PadEv::Button(PadButton::A, true));
            pad_event(PadEv::Button(PadButton::A, false));
            Ok(())
        }),
        Step::Wait("the beat over, the page as it came", |g| {
            still_the_end(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.screens.end_beat() {
                SHOWN_AT.store(frames(), Ordering::Relaxed);
                return Ok(false);
            }
            Ok(frames() - SHOWN_AT.load(Ordering::Relaxed) > 4)
        }),
        Step::Shot("end-beat-page"),
        Step::Call("the held keys repeat", |g| {
            still_the_end(g)?;
            for _ in 0..3 {
                key_event(GKey::SPACE, true, true);
                key_event(GKey::LEFT, true, true);
            }
            SHOWN_AT.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("their repeats counted for nothing", |g| {
            still_the_end(g)?;
            let s = g.ui.as_ref().and_then(|ui| ui.screens.end_summary());
            if s.is_none_or(|s| s.name.is_empty()) {
                return Err("the end page lost its hero".into());
            }
            Ok(frames() - SHOWN_AT.load(Ordering::Relaxed) > 6)
        }),
        Step::Call("let go, Enter pressed anew", |_| {
            key_event(GKey::SPACE, false, false);
            key_event(GKey::LEFT, false, false);
            key_event(GKey::ENTER, true, false);
            key_event(GKey::ENTER, false, false);
            Ok(())
        }),
        Step::Wait("the title, no new game", |g| {
            fail_on_error_screen(g)?;
            match screen(g) {
                Some("title") => Ok(true),
                Some("end") => Ok(false),
                other => Err(format!("Enter on the end page led to {other:?}")),
            }
        }),
    ]);
    steps
}
