//! The whole loop with a gamepad alone, as a Steam Deck player meets it
//! (`pad-journey`, made for Russian at 1280×800): the title, the creation
//! form (a role card, the dice's name), the prologue, "Your quest" and
//! the pet's card, a few steps and a search, the game's menu with every
//! panel, the pack as a list, a fight to the death, the end page, a new
//! game from it, a save from the game's menu and Continue on the title.
//!
//! The player's every input is the pad's: no key, no click, no event of
//! the interface pushed. Two things no player's input stands for are
//! staged, and said so: the first game is in debug mode (its "Die?"
//! answered with the pad), and two soldier ants are made beside the hero
//! with debug mode's ^G, answered to the engine directly (`stage`).
//!
//! At each state reached the focus is somewhere (a page's control, an
//! answer, a menu's row, an item); at each picture no word on screen names
//! a key or a click, no word is cut (a wrapped label short of lines), and
//! in Russian no English is left (names, the controller's own button
//! names and the engine's scroll labels aside). The pictures end on one
//! contact sheet.

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};

use godot::classes::text_server::AutowrapMode;
use godot::classes::{Button, CanvasItem, Control, Engine, Label, RichTextLabel};
use godot::global::JoyAxis;
use godot::prelude::*;
use nh_protocol::{GlyphKind, Reply, mg};
use nh_world::Prompt;

use super::pad_panels_tests::{back, menu_on, open, panel};
use super::{PadEv, Step, fail_on_error_screen, idle_command, pad_event, screen, tap};
use crate::game::RenethackGame;
use crate::game_menu::GameMenuPick;
use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, Lang};
use crate::tr;

/// The frame the last press of a loop went on.
static PRESSED_AT: AtomicI64 = AtomicI64::new(0);
/// The request (session serial × 1000 + id) the fight last acted on.
static ACTED_ON: AtomicU64 = AtomicU64::new(0);
/// The portrait's renders before the restore.
static RENDERS: AtomicU32 = AtomicU32::new(0);
/// The turn and the log before a search beside a foe.
static TURN: AtomicI64 = AtomicI64::new(0);
static LOG_SEQ: AtomicU64 = AtomicU64::new(0);

/// Words only a keyboard or a mouse has: whole words, word stems (the
/// Russian's cases), and phrases.
const KEY_WORDS: [&str; 15] = [
    "Esc",
    "ESC",
    "Enter",
    "PgUp",
    "PgDn",
    "Tab",
    "Space",
    "Home",
    "Backspace",
    "Shift",
    "Ctrl",
    "click",
    "Click",
    "Keys",
    "Пробел",
];
const KEY_STEMS: [&str; 5] = ["щелч", "щёлк", "Щелч", "клавиш", "Клавиш"];
const KEY_PHRASES: [&str; 3] = ["any key", "(y)", "(n)"];

/// A label that is a key alone, as a keyboard's hint prints it: a control
/// key ("^X"), a meta key ("M-a"), an extended command ("#twoweapon"). A
/// letter alone is a menu's or the pack's letter as often as a key: a key
/// hint marked as the keyboard's (`pad_view::key_words`) is caught by its
/// mark.
fn is_a_key(text: &str) -> bool {
    let t = text.trim();
    let mut chars = t.chars();
    let one = |c: Option<char>| c.is_some_and(|c| c.is_ascii_graphic());
    match (chars.next(), t.chars().count()) {
        (Some('^'), 2) => one(chars.next()),
        (Some('M'), 3) => chars.next() == Some('-') && one(chars.next()),
        (Some('#'), n) if n > 2 => chars.all(|c| c.is_ascii_lowercase()),
        _ => false,
    }
}

/// The key or click `text` names, if any ("Escape spells" names none).
fn names_a_key(text: &str) -> Option<&'static str> {
    let mut words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    // a sentence's first word, a verb ("Enter explore mode"), is no key;
    // a key's name is followed by what it does ("Esc: cancel")
    let verb = words.first().is_some_and(|w| {
        let rest = text
            .trim_start()
            .strip_prefix(w)
            .and_then(|r| r.strip_prefix(' '));
        rest.is_some_and(|r| r.starts_with(char::is_lowercase))
    });
    if verb {
        words.remove(0);
    }
    KEY_WORDS
        .iter()
        .find(|k| words.contains(*k))
        .or_else(|| {
            KEY_STEMS
                .iter()
                .find(|k| words.iter().any(|w| w.starts_with(**k)))
        })
        .or_else(|| KEY_PHRASES.iter().find(|k| text.contains(**k)))
        .copied()
}

/// Seams found in other owners' files and reported to them: what a seam's
/// words hold, and whose it is. Told as known, they fail nothing; a seam
/// not here fails the run. (None now: the ^X, ^G and palette ones of
/// 2026-10-30 have their Russian.)
const KNOWN: [(&str, &str); 0] = [];

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// A button pressed now and then while a loop waits (every `every`
/// frames).
fn press_every(b: PadButton, every: i64) {
    if frames() - PRESSED_AT.load(Ordering::Relaxed) > every {
        PRESSED_AT.store(frames(), Ordering::Relaxed);
        pad_event(PadEv::Button(b, true));
        pad_event(PadEv::Button(b, false));
    }
}

/// The words on screen: each visible label's, button's and rich text's,
/// with whether its widget is the same in every language (a name, a
/// language, a controller's button) and whether it is marked as the
/// keyboard's words.
fn on_screen(g: &RenethackGame) -> Vec<(String, bool, bool, String)> {
    let mut out = Vec::new();
    let mut stack: Vec<Gd<Node>> = vec![g.to_gd().upcast()];
    while let Some(node) = stack.pop() {
        if node.is_class("Node3D") {
            continue;
        }
        if let Ok(item) = node.clone().try_cast::<CanvasItem>()
            && !item.is_visible_in_tree()
        {
            continue;
        }
        // the on-screen keyboard's keys: Space and Shift are its own
        if node.has_meta(crate::dialogs::OSK) {
            continue;
        }
        let verbatim = node.has_meta(i18n::VERBATIM);
        let keys = crate::pad_view::is_key_words(&node);
        let text = if let Ok(l) = node.clone().try_cast::<Label>() {
            Some(l.get_text().to_string())
        } else if let Ok(b) = node.clone().try_cast::<Button>() {
            Some(b.get_text().to_string())
        } else if let Ok(r) = node.clone().try_cast::<RichTextLabel>() {
            Some(r.get_parsed_text().to_string())
        } else {
            None
        };
        if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
            out.push((t, verbatim, keys, node.get_path().to_string()));
        }
        stack.extend(node.get_children().iter_shared());
    }
    out
}

/// The screen's seams: a word naming a key or a click, a wrapped label
/// short of lines, English in another language.
fn seams(g: &mut RenethackGame) -> Result<(), String> {
    let words = on_screen(g);
    let mut found = Vec::new();
    for (t, _, keys, path) in &words {
        if let Some(k) = names_a_key(t) {
            found.push(format!("{k:?} with a gamepad in {path}: {t:?}"));
        } else if *keys || is_a_key(t) {
            found.push(format!("a key with a gamepad in {path}: {t:?}"));
        }
    }
    // a wrapped label that shows fewer lines than it has
    let mut stack: Vec<Gd<Node>> = vec![g.to_gd().upcast()];
    while let Some(node) = stack.pop() {
        if node.is_class("Node3D") {
            continue;
        }
        if let Ok(c) = node.clone().try_cast::<Control>()
            && !c.is_visible_in_tree()
        {
            continue;
        }
        if let Ok(l) = node.clone().try_cast::<Label>()
            && l.get_autowrap_mode() != AutowrapMode::OFF
            && l.get_max_lines_visible() < 0
            && !l.get_text().is_empty()
            && l.get_visible_line_count() < l.get_line_count()
        {
            found.push(format!(
                "{:?} shows {} of its {} lines ({})",
                l.get_text().to_string(),
                l.get_visible_line_count(),
                l.get_line_count(),
                node.get_path()
            ));
        }
        stack.extend(node.get_children().iter_shared());
    }
    if i18n::lang() == Lang::Ru {
        use PadButton::*;
        let pad = PadKind::default();
        let buttons: Vec<String> = [A, B, X, Y, Lb, Rb, Lt, Rt, Start, Back, L3, R3]
            .iter()
            .map(|b| pad.label(*b).to_string())
            .collect();
        let name = g.name.clone().unwrap_or_default();
        let mut allowed: Vec<&str> = buttons.iter().map(String::as_str).collect();
        // names: the hero's, debug mode's, the game's
        allowed.extend([name.as_str(), "wizard", "NetHack", "renethack"]);
        // a line at a time: the log's English line named, not its first
        let texts: Vec<String> = words
            .iter()
            .filter(|(_, verbatim, ..)| !verbatim)
            .flat_map(|(t, ..)| t.lines().map(str::to_string).collect::<Vec<_>>())
            .collect();
        // NetHack's scroll labels are Latin capitals in every language
        let left: Vec<String> = crate::end_view::english_left(&texts, &allowed)
            .into_iter()
            .filter(|w| {
                let word = w.split('"').nth(1).unwrap_or("");
                !word.chars().all(|c| c.is_ascii_uppercase())
            })
            .collect();
        found.extend(left.into_iter().map(|w| format!("English: {w}")));
    }
    // what is known and reported to its owner: told, failing nothing
    found.retain(|f| match KNOWN.iter().find(|(k, _)| f.contains(k)) {
        Some((_, owner)) => {
            godot_print!("selftest: known seam ({owner}): {f}");
            false
        }
        None => true,
    });
    if found.is_empty() {
        return Ok(());
    }
    // a survey (RENETHACK_JOURNEY_SURVEY): every seam of the run told,
    // none failing it
    if std::env::var_os("RENETHACK_JOURNEY_SURVEY").is_some() {
        for f in &found {
            godot_print!("selftest: seam: {f}");
        }
        return Ok(());
    }
    found.truncate(8);
    Err(found.join("; "))
}

/// A picture, and the screen's seams checked once it is drawn; a few
/// frames first, with pictures or without (a run at no set size takes
/// none, and its next press would come in the frame the screen opened).
fn shot(name: &'static str) -> [Step; 4] {
    [
        Step::Call("", |_| {
            PRESSED_AT.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("drawn", |_| {
            Ok(frames() - PRESSED_AT.load(Ordering::Relaxed) > 3)
        }),
        Step::Shot(name),
        Step::Call("the screen's seams", seams),
    ]
}

/// The screen's seams, once what just opened is laid out (a few frames).
fn settled_seams() -> [Step; 3] {
    [
        Step::Call("", |_| {
            PRESSED_AT.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("laid out", |_| {
            Ok(frames() - PRESSED_AT.load(Ordering::Relaxed) > 6)
        }),
        Step::Call("the screen's seams", seams),
    ]
}

/// Whatever is on top has a focus for the pad.
fn focused(g: &RenethackGame) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if matches!(screen(g), Some("title" | "creation" | "end" | "error"))
        && ui.screens.nav_focused().is_none()
    {
        return Err(format!("no focus on the {:?} page", screen(g)));
    }
    if ui.dialogs.is_open() && !ui.dialogs.pad_focused() {
        return Err(format!(
            "no focus in the {:?} dialog",
            ui.dialogs.kind_name()
        ));
    }
    if ui.inventory.is_open() && !ui.inventory.pad_focused() {
        return Err("no focus in the inventory".into());
    }
    Ok(())
}

/// The gender the creation form has chosen, before a list's pick.
static GENDER: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// The creation form's name keyboard is up.
fn name_keys(g: &RenethackGame) -> bool {
    g.ui.as_ref().is_some_and(|ui| ui.screens.name_keys_open())
}

/// What the journey types on the name's keyboard, `n` letters: its first
/// key in the interface's letters (Cyrillic for Russian), then Y's.
fn typed(n: usize) -> String {
    let (first, then) = match i18n::lang() {
        Lang::En => ("q", "й"),
        _ => ("й", "q"),
    };
    [first, then][..n].concat()
}

/// The creation form's name is `name`.
fn name_is(g: &RenethackGame, name: &str) -> bool {
    g.ui.as_ref()
        .and_then(|ui| ui.screens.creation_choice())
        .is_some_and(|c| c.name == name)
}

/// The gender the creation form has chosen.
fn creation_gender(g: &RenethackGame) -> String {
    g.ui.as_ref()
        .and_then(|ui| ui.screens.creation_choice())
        .map(|c| c.gender)
        .unwrap_or_default()
}

/// A list of the page is open.
fn list_open(g: &RenethackGame) -> bool {
    g.ui.as_ref().is_some_and(|ui| ui.screens.list_open())
}

/// On `page`, the focus on the control whose words begin with `words`.
fn focus_on(g: &RenethackGame, page: &str, words: &str) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(screen(g) == Some(page)
        && ui
            .screens
            .nav_focused()
            .is_some_and(|f| f.starts_with(words)))
}

/// The game at its command prompt, nothing over it, the pad's focus right.
fn playing(g: &RenethackGame) -> Result<bool, String> {
    focused(g)?;
    Ok(back(g)? && idle_command(g)?)
}

/// The HUD's title holds the hero's whole name, however long (NetHack's
/// status line cuts "Name the Rank" at 30 bytes, 8 Cyrillic letters of a
/// name with a long rank).
fn whole_name_shown(g: &RenethackGame) -> Result<bool, String> {
    let name = g.name.clone().unwrap_or_default();
    let title = g.ui.as_ref().ok_or("no UI")?.hud.title_shown();
    if name.is_empty() || title.is_empty() {
        return Ok(false);
    }
    if !title.contains(name.as_str()) {
        return Err(format!("the title {title:?} cuts {name:?}"));
    }
    Ok(true)
}

/// Between the steps of a game: a card put away, a text window or a
/// --More-- closed with B, until the command prompt.
fn settle(what: &'static str) -> Step {
    Step::Wait(what, |g| {
        fail_on_error_screen(g)?;
        focused(g)?;
        let ui = g.ui.as_ref().ok_or("no UI")?;
        if ui.hint.current().is_some() || ui.dialogs.is_open() {
            press_every(PadButton::B, 30);
            return Ok(false);
        }
        playing(g)
    })
}

/// One pad's step `(dx, dy)` with the left stick, let go after.
fn stick(dx: i32, dy: i32) -> Step {
    let ev = |a, v: i32| PadEv::Axis(a, v as f32);
    Step::Pad(vec![
        ev(JoyAxis::LEFT_X, dx),
        ev(JoyAxis::LEFT_Y, dy),
        ev(JoyAxis::LEFT_X, 0),
        ev(JoyAxis::LEFT_Y, 0),
    ])
}

/// A hostile beside the hero: the way to it.
fn foe_beside(g: &RenethackGame) -> Option<(i32, i32)> {
    let (hx, hy) = g.world.map.hero()?;
    (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
        .filter(|d| *d != (0, 0))
        .find(|(dx, dy)| {
            g.world
                .map
                .cell(hx + dx, hy + dy)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|gl| gl.kind == GlyphKind::Mon && gl.flags & (mg::PET | mg::HERO) == 0)
        })
}

/// Debug mode's ^G for `name`, answered to the engine directly: staged,
/// no player's input.
fn stage_monster(steps: &mut Vec<Step>) {
    steps.extend([
        settle("the command prompt before the staging"),
        Step::Call("staged: debug mode's ^G", |g| {
            g.reply(Reply::Key(7));
            Ok(())
        }),
        Step::Wait("the kind of monster asked", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(&g.pending,
                Some((_, Prompt::Text { query, .. })) if query.contains("Create what kind")))
        }),
        Step::Call("staged: a soldier ant", |g| {
            g.reply(Reply::Text("soldier ant".into()));
            Ok(())
        }),
    ]);
}

/// The fight's next move, once a request: attack the foe beside the hero
/// (the stick toward it), else search for it to come (X); "Die?" (debug
/// mode) answered yes (← A), the end's questions let go (B), until the
/// end page.
fn fight_on(g: &RenethackGame, until_hit: bool) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    if screen(g) == Some("end") {
        return Ok(true);
    }
    // the fight's picture: a foe beside the hero at the command prompt
    if until_hit && foe_beside(g).is_some() && idle_command(g)? {
        return Ok(true);
    }
    let Some((id, prompt)) = g.pending.as_ref() else {
        return Ok(false);
    };
    let at = g.session_serial * 1000 + id;
    let fresh = ACTED_ON.load(Ordering::Relaxed) != at;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    // a card: put away first
    if ui.hint.current().is_some() {
        press_every(PadButton::B, 30);
        return Ok(false);
    }
    if !fresh {
        return Ok(false);
    }
    match prompt {
        Prompt::Choice { query, .. } if query.contains("Die?") => {
            if !ui.dialogs.is_open() {
                return Ok(false);
            }
            focused(g)?;
            ACTED_ON.store(at, Ordering::Relaxed);
            for b in [PadButton::Left, PadButton::A] {
                pad_event(PadEv::Button(b, true));
                pad_event(PadEv::Button(b, false));
            }
        }
        Prompt::Command if idle_command(g)? => {
            ACTED_ON.store(at, Ordering::Relaxed);
            let evs: Vec<PadEv> = match foe_beside(g) {
                Some((dx, dy)) => vec![
                    PadEv::Axis(JoyAxis::LEFT_X, dx as f32),
                    PadEv::Axis(JoyAxis::LEFT_Y, dy as f32),
                    PadEv::Axis(JoyAxis::LEFT_X, 0.0),
                    PadEv::Axis(JoyAxis::LEFT_Y, 0.0),
                ],
                None => vec![
                    PadEv::Button(PadButton::X, true),
                    PadEv::Button(PadButton::X, false),
                ],
            };
            for e in evs {
                pad_event(e);
            }
        }
        Prompt::Command => {}
        _ => {
            if ui.dialogs.is_open() {
                focused(g)?;
            }
            press_every(PadButton::B, 30);
        }
    }
    Ok(false)
}

pub(super) fn pad_journey() -> Vec<Step> {
    let mut steps = vec![
        Step::Call(
            "the first game in debug mode, the guide on, the prologue told",
            |g| {
                g.guide_in_test = true;
                g.prologue_forced = true;
                g.debug_mode = true;
                g.seed = Some(7);
                ACTED_ON.store(0, Ordering::Relaxed);
                Ok(())
            },
        ),
        Step::Wait("the title, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        // the pad takes over: a press that moves nothing on the title
        tap(PadButton::Up),
        tap(PadButton::Down),
        Step::Wait("the title's focus on a new game", |g| {
            focus_on(g, "title", &tr!("title-new-game"))
        }),
    ];
    steps.extend(shot("journey-title"));
    // creation: a role's card, the dice's name, Start
    steps.extend([
        tap(PadButton::A),
        Step::Wait("the creation form, on its cards", |g| {
            focused(g)?;
            Ok(screen(g) == Some("creation")
                && g.ui
                    .as_ref()
                    .is_some_and(|ui| ui.screens.creation_choice().is_some()))
        }),
        // the wizard's card, the last: down the left column, then right
        // (a wizard knows a spell: the spells' panel has one)
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Right),
        Step::Wait("the wizard's card chosen", |g| {
            focused(g)?;
            let role =
                g.ui.as_ref()
                    .and_then(|ui| ui.screens.creation_choice())
                    .map(|c| c.role);
            Ok(role.is_some_and(|r| r.to_lowercase().starts_with("wiz")))
        }),
        tap(PadButton::A),
        Step::Wait("A: past the cards, on the name", |g| {
            focus_on(g, "creation", &tr!("creation-name-placeholder"))
        }),
        // the name typed on the screen's keyboard: Cyrillic first, Y the
        // Latin letters, B erases, Start puts it away
        tap(PadButton::A),
        Step::Wait("A: the name's keyboard", |g| Ok(name_keys(g))),
        tap(PadButton::A),
        Step::Wait("A: a letter typed", |g| Ok(name_is(g, &typed(1)))),
        tap(PadButton::Y),
        tap(PadButton::A),
        Step::Wait("Y, A: a letter of the other layout", |g| {
            Ok(name_is(g, &typed(2)))
        }),
        tap(PadButton::B),
        Step::Wait("B: erased", |g| Ok(name_is(g, &typed(1)))),
    ]);
    steps.extend(shot("journey-name-keys"));
    steps.extend([
        tap(PadButton::Start),
        Step::Wait("Start: the keyboard away, the name kept", |g| {
            Ok(!name_keys(g) && focus_on(g, "creation", &tr!("creation-name-placeholder"))?)
        }),
        // B steps back to the cards, the card and the name kept
        tap(PadButton::B),
        Step::Wait("B: back on the cards", |g| {
            focused(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let wizard = ui
                .screens
                .creation_choice()
                .is_some_and(|c| c.role.to_lowercase().starts_with("wiz"));
            Ok(screen(g) == Some("creation")
                && ui.screens.on_cards()
                && wizard
                && name_is(g, &typed(1)))
        }),
        tap(PadButton::A),
        tap(PadButton::Right),
        Step::Wait("A, →: the dice", |g| {
            focus_on(g, "creation", &tr!("creation-name-roll-tip"))
        }),
        tap(PadButton::A),
        Step::Wait("A: a name rolled", |g| {
            focused(g)?;
            Ok(g.ui
                .as_ref()
                .and_then(|ui| ui.screens.creation_choice())
                .is_some_and(|c| !c.name.is_empty() && c.name != typed(1)))
        }),
        // a list opened with A: the d-pad moves in it, A picks, B closes
        tap(PadButton::Down),
        tap(PadButton::Down),
        Step::Call("the gender before", |g| {
            *GENDER.lock().unwrap() = creation_gender(g);
            Ok(())
        }),
        tap(PadButton::A),
        Step::Wait("A: the gender's list open", |g| Ok(list_open(g))),
        tap(PadButton::Down),
        tap(PadButton::A),
        Step::Wait("↓, A: another gender, the list closed", |g| {
            Ok(!list_open(g) && creation_gender(g) != *GENDER.lock().unwrap())
        }),
        Step::Call("the gender picked", |g| {
            *GENDER.lock().unwrap() = creation_gender(g);
            Ok(())
        }),
        tap(PadButton::A),
        Step::Wait("A: the list open again", |g| Ok(list_open(g))),
        tap(PadButton::Down),
        tap(PadButton::B),
        Step::Wait("B: the list closed, the gender kept", |g| {
            Ok(!list_open(g) && creation_gender(g) == *GENDER.lock().unwrap())
        }),
    ]);
    steps.extend(shot("journey-creation"));
    // the prologue, A beat by beat
    steps.extend([
        tap(PadButton::Start),
        Step::Wait("Start: the prologue", |g| {
            fail_on_error_screen(g)?;
            Ok(g.prologue.as_ref().is_some_and(|p| p.run.all_shown()))
        }),
    ]);
    steps.extend(shot("journey-prologue"));
    steps.extend([
        Step::Call("", |_| {
            PRESSED_AT.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("A through the prologue", |g| {
            fail_on_error_screen(g)?;
            if g.prologue.is_none() {
                return Ok(true);
            }
            press_every(PadButton::A, 45);
            Ok(false)
        }),
        // "Your quest", A step by step
        Step::Wait("Your quest", |g| {
            fail_on_error_screen(g)?;
            Ok(g.ui.as_ref().is_some_and(|ui| ui.quest.is_open()))
        }),
        // the game comes up out of the prologue's black under it
        Step::Call("", |_| {
            PRESSED_AT.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the game up out of black", |_| {
            Ok(frames() - PRESSED_AT.load(Ordering::Relaxed) > 150)
        }),
    ]);
    steps.extend(shot("journey-quest"));
    steps.extend([
        Step::Wait("A through Your quest", |g| {
            fail_on_error_screen(g)?;
            if !g.ui.as_ref().is_some_and(|ui| ui.quest.is_open()) {
                return Ok(true);
            }
            press_every(PadButton::A, 30);
            Ok(false)
        }),
        // the pet's card at the first command
        Step::Wait("the pet's card", |g| {
            fail_on_error_screen(g)?;
            Ok(g.ui.as_ref().is_some_and(|ui| ui.hint.current().is_some()))
        }),
    ]);
    steps.extend(shot("journey-card"));
    steps.extend([
        tap(PadButton::B),
        Step::Wait("B: the card put away", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.hint.current().is_none()))
        }),
        settle("the game at its command prompt"),
        // a few turns: steps with the stick, a search
        stick(1, 0),
        settle("a step east"),
        stick(-1, 0),
        settle("a step back"),
        tap(PadButton::X),
        settle("a search"),
        Step::Wait("the camera on the hero", super::camera_settled),
    ]);
    steps.extend(shot("journey-map"));
    // the game's menu and every panel in it
    steps.push(tap(PadButton::Start));
    steps.push(Step::Wait("Start: the game's menu", |g| {
        menu_on(g, GameMenuPick::Resume)
    }));
    steps.extend(shot("journey-menu"));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("B: back in the game", playing));
    let panels: [(GameMenuPick, usize, super::Check); 9] = [
        (GameMenuPick::Spells, 0, |g| panel(g, &tr!("spells-title"))),
        (GameMenuPick::Character, 1, |g| {
            panel(g, &tr!("sheet-title"))
        }),
        (GameMenuPick::Skills, 2, |g| {
            // debug mode asks first whether to advance without practice: B
            if matches!(&g.pending,
                Some((_, Prompt::Choice { query, .. })) if query.contains("without practice"))
            {
                press_every(PadButton::B, 30);
                return Ok(false);
            }
            panel(g, &tr!("skills-title"))
        }),
        (GameMenuPick::Overview, 3, |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("overview"))
        }),
        (GameMenuPick::Journal, 4, |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("journal"))
        }),
        (GameMenuPick::Codex, 5, |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("codex"))
        }),
        (GameMenuPick::Achievements, 6, |g| {
            Ok(screen(g) == Some("achievements"))
        }),
        (GameMenuPick::Help, 7, |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.help.is_open())
        }),
        (GameMenuPick::Commands, 8, |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.kind_name() == Some("extcmd"))
        }),
    ];
    for (pick, down, shown) in panels {
        open(pick, down, &mut steps);
        steps.push(Step::Wait("the panel open", shown));
        steps.push(Step::Call("the panel's focus", |g| focused(g)));
        match pick {
            GameMenuPick::Character => steps.extend(shot("journey-character")),
            GameMenuPick::Journal => steps.extend(shot("journey-journal")),
            // what each command does, in the player's words, no key
            GameMenuPick::Commands => steps.extend(shot("journey-palette")),
            _ => steps.extend(settled_seams()),
        }
        steps.push(tap(PadButton::B));
        steps.push(settle("the panel closed, the game back"));
    }
    // the pack as a list
    steps.extend([
        tap(PadButton::Y),
        Step::Wait("Y: the inventory, the focus on an item", |g| {
            focused(g)?;
            let p = &g.ui.as_ref().ok_or("no UI")?.inventory;
            Ok(p.mode_name() == Some("browse") && p.pad_focused())
        }),
        tap(PadButton::Back),
        Step::Wait("View: the pack as a list", |g| {
            focused(g)?;
            Ok(g.ui.as_ref().ok_or("no UI")?.inventory.list_view())
        }),
        tap(PadButton::Down),
    ]);
    steps.extend(shot("journey-pack-list"));
    // an item's actions: what each does, no key beside it
    steps.extend([
        tap(PadButton::X),
        Step::Wait("X: the item's actions", |g| {
            focused(g)?;
            Ok(g.ui
                .as_ref()
                .ok_or("no UI")?
                .inventory
                .context_rows()
                .is_some())
        }),
    ]);
    steps.extend(shot("journey-item-actions"));
    steps.extend([
        tap(PadButton::B),
        tap(PadButton::B),
        settle("B, B: the inventory closed"),
    ]);
    // a fight: two soldier ants staged beside the hero, met with the stick
    stage_monster(&mut steps);
    stage_monster(&mut steps);
    steps.push(Step::Wait("a foe beside the hero", |g| fight_on(g, true)));
    steps.extend(shot("journey-fight"));
    // X beside a foe searches all the same (the pad has no 'm' prefix)
    steps.extend([
        Step::Call("the turn before a search", |g| {
            TURN.store(g.world.turn().unwrap_or(0), Ordering::Relaxed);
            LOG_SEQ.store(g.world.log.last_seq(), Ordering::Relaxed);
            Ok(())
        }),
        tap(PadButton::X),
        Step::Wait("X: a search beside a foe", |g| {
            let seq = LOG_SEQ.load(Ordering::Relaxed);
            if let Some(m) = (g.world.log.since(seq)).find(|m| m.text.contains("prefix to force")) {
                return Err(format!("the search refused: {:?}", m.text));
            }
            let turn = g.world.turn().unwrap_or(0);
            Ok(screen(g) == Some("end") || turn > TURN.load(Ordering::Relaxed))
        }),
    ]);
    steps.push(Step::Wait(
        "the fight to the death, the end's questions let go",
        |g| fight_on(g, false),
    ));
    steps.push(Step::Wait("the end page, on a new game", |g| {
        focus_on(g, "end", &tr!("end-new-game"))
    }));
    steps.extend(shot("journey-end"));
    // a new game from the end page, in normal mode
    steps.extend([
        Step::Call("the next game a normal one", |g| {
            g.debug_mode = false;
            g.prologue_forced = false;
            Ok(())
        }),
        tap(PadButton::A),
        Step::Wait("A: the creation form again", |g| {
            focused(g)?;
            Ok(screen(g) == Some("creation"))
        }),
    ]);
    steps.extend(shot("journey-creation-again"));
    steps.extend([
        tap(PadButton::Start),
        settle("Start: the second game at its command prompt"),
        Step::Wait("the hero's whole name in the HUD's title", whole_name_shown),
        stick(0, 1),
        settle("a step south"),
        // saved from the game's menu
        tap(PadButton::Start),
        Step::Wait("the game's menu", |g| menu_on(g, GameMenuPick::Resume)),
        tap(PadButton::Down),
        tap(PadButton::Down),
        Step::Wait("↓ ↓ on saving", |g| menu_on(g, GameMenuPick::SaveLeave)),
        tap(PadButton::A),
        Step::Wait("Really save?", |g| {
            fail_on_error_screen(g)?;
            let asks = matches!(&g.pending,
                Some((_, Prompt::Choice { query, .. })) if query.contains("save"));
            Ok(asks && g.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open()))
        }),
        tap(PadButton::Left),
        tap(PadButton::A),
        Step::Wait("← A: saved, the title on Continue", |g| {
            fail_on_error_screen(g)?;
            let name = g.name.clone().unwrap_or_default();
            let ui = g.ui.as_ref().ok_or("no UI")?;
            // Continue's words name the hero
            Ok(screen(g) == Some("title")
                && !name.is_empty()
                && ui
                    .screens
                    .nav_focused()
                    .is_some_and(|f| f.contains(name.as_str())))
        }),
    ]);
    steps.extend(shot("journey-title-continue"));
    steps.extend([
        Step::Call("the portrait's renders so far", |g| {
            let n = g.ui.as_ref().map_or(0, |ui| ui.hud.portrait_shown().1);
            RENDERS.store(n, Ordering::Relaxed);
            Ok(())
        }),
        tap(PadButton::A),
        settle("A: the game restored at its command prompt"),
        Step::Wait(
            "its level drawn, the camera on the hero",
            super::camera_settled,
        ),
        // the portrait renders this hero: a render since the restore, not
        // the last game's hero kept
        Step::Wait("the hero's whole name, the portrait of this hero", |g| {
            let (_, n) = g.ui.as_ref().ok_or("no UI")?.hud.portrait_shown();
            Ok(whole_name_shown(g)? && n > RENDERS.load(Ordering::Relaxed))
        }),
    ]);
    steps.extend(shot("journey-restored"));
    steps.push(Step::Sheet("journey-sheet"));
    steps
}
