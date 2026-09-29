//! Headless self-tests: a scenario replaces only the source of input. It
//! queues the same `UiEvent`s the widgets and `input.rs` would, waits for
//! states with a timeout per step, and never dispatches into the game
//! synchronously. The verdict is one "SELFTEST PASS <name>" or
//! "SELFTEST FAIL <name>: <reason>" line and the exit code.
//!
//! Scenarios: smoke, keys, save, close, crash, menus, text, moves, orders, and soak
//! (random play: `--soak=N` answered requests, `--seed=S` or
//! RENETHACK_SEED; RENETHACK_SOAK_TRACE=1 prints every decision).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use godot::classes::{DisplayServer, Input, InputEventKey};
use godot::global::{Error, Key as GKey};
use godot::prelude::*;
use nh_protocol::PickHow;
use nh_world::{
    Key, KeyInput, KeyProfile, MenuEntry, MenuState, Mods, Prompt, Stop, Terrain, cell_terrain,
};

use nh_link::save_exists;

#[path = "selftest_hero.rs"]
mod hero;

use crate::dialogs::ROW_H;
use crate::game::{Args, GameState, RenethackGame, SELFTEST_SEED, env_number};
use crate::screens::EndSummary;
use crate::ui_events::{CharacterChoice, DialogEvent, UiEvent};

const STEP_TIMEOUT: Duration = Duration::from_secs(30);

/// Panics anywhere in the client since the self-test started: gdext catches
/// a panic in a callback and the game goes on, a self-test must not.
static PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Record every panic, then let gdext's hook print it as before.
fn watch_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(mut panics) = PANICS.lock() {
                panics.push(info.to_string());
            }
            previous(info);
        }));
    });
}

fn first_panic() -> Option<String> {
    PANICS.lock().ok().and_then(|p| p.first().cloned())
}
/// Frames to let a state be drawn before a screenshot.
const SHOT_FRAMES: u32 = 3;

/// Ok(true): the state is reached; Ok(false): not yet; Err: failed.
type Check = fn(&RenethackGame) -> Result<bool, String>;
type PromptCheck = fn(&Prompt) -> bool;

enum Step {
    /// Wait until the check holds.
    Wait(&'static str, Check),
    /// Wait for a request newer than the last key or dialog event sent.
    Request(&'static str, PromptCheck),
    Push(UiEvent),
    /// A key for the request now pending.
    Key(KeyInput),
    /// A key for the request now pending, chosen by what the game shows
    /// (a menu entry's letter).
    KeyFrom(&'static str, fn(&RenethackGame) -> Result<KeyInput, String>),
    /// A dialog event for the request now pending.
    Dialog(DialogEvent),
    Call(&'static str, fn(&mut RenethackGame) -> Result<(), String>),
    /// An inventory panel input made from what the game shows (letters).
    Inv(
        &'static str,
        fn(&RenethackGame) -> Option<crate::inventory_panel::InvInput>,
    ),
    /// A real key event (keycode, character typed, Shift) through Godot's
    /// input buffer: delivered next frame to `input()` and the focused
    /// text field, never from inside the game's own call.
    Press(GKey, char, bool),
    Shot(&'static str),
    /// Answer each question with this key (text windows with OK) until the check holds.
    AnswerUntil(char, &'static str, Check),
    /// Random play until the budget is spent; its own watchdog, no step timeout.
    Soak(Box<Soak>),
}

pub struct SelfTest {
    name: String,
    shots: Option<PathBuf>,
    steps: VecDeque<Step>,
    step_started: Instant,
    frames: u32,
    /// The request (session serial, id) the last key or dialog event went to.
    last_req: (u64, u64),
    started: bool,
    finished: bool,
}

fn key(c: char) -> Step {
    Step::Key(KeyInput::plain(Key::Char(c)))
}

fn command(p: &Prompt) -> bool {
    *p == Prompt::Command
}

fn smoke_choice() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "valkyrie".into(),
        race: "human".into(),
        gender: "female".into(),
        align: "neutral".into(),
        // the scenarios were written for NetHack's vi-keys
        profile: KeyProfile::Classic,
    }
}

fn screen(game: &RenethackGame) -> Option<&'static str> {
    game.ui.as_ref().and_then(|ui| ui.screens.current())
}

fn fail_on_error_screen(game: &RenethackGame) -> Result<(), String> {
    if game.state == GameState::Failed {
        return Err("the error screen came up".into());
    }
    Ok(())
}

fn smoke() -> Vec<Step> {
    vec![
        Step::Wait("the title screen", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("title") && ui.screens.has_button("New game"))
        }),
        Step::Shot("title"),
        Step::Push(UiEvent::NewGame),
        Step::Wait("the creation screen", |g| Ok(screen(g) == Some("creation"))),
        Step::Call("fill the form", |g| {
            g.preset_creation(&smoke_choice());
            Ok(())
        }),
        Step::Shot("creation"),
        Step::Push(UiEvent::StartCharacter(smoke_choice())),
        Step::Request("the first command", command),
        Step::Wait("the hero and 20 known cells on the map", |g| {
            fail_on_error_screen(g)?;
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let known = (0..nh_world::ROWNO)
                .flat_map(|y| (1..nh_world::COLNO).map(move |x| (x, y)))
                .filter_map(|(x, y)| g.world.map.cell(x, y))
                .filter(|c| cell_terrain(c, cat).is_some())
                .count();
            let drawn = g.ui.as_ref().map_or(0, |ui| ui.map.drawn_cells());
            Ok(g.world.map.hero().is_some() && known >= 20 && drawn >= 20)
        }),
        Step::Wait("HP:16(16) and Dlvl:1 on the HUD", |g| {
            let text = g.ui.as_ref().map_or("", |ui| ui.hud.status_text());
            Ok(text.contains("HP:16(16)") && text.contains("Dlvl:1"))
        }),
        Step::Wait(
            "the HP and Pw orbs full, the default loadout on the bar",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let (hp, pw, slots) = ui.hud.cluster_view();
                let filled = slots.iter().filter(|&&(filled, _)| filled).count();
                Ok(slots.len() == 10
                    && filled >= 9
                    && hp == Some((16, 16))
                    && pw.is_some_and(|(v, m)| v == m && m > 0))
            },
        ),
        key('i'),
        Step::Wait("five items in the inventory panel", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.inventory.mode_name() != Some("browse") {
                return Ok(false);
            }
            match ui.inventory.shown().len() {
                5 => Ok(true),
                0 => Ok(false),
                n => Err(format!("the inventory has {n} items, not 5")),
            }
        }),
        Step::Shot("inventory"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed", panel_closed),
        key('l'),
        Step::Request("a command after a step", command),
        key('l'),
        Step::Request("a command after two steps", command),
        Step::Shot("game"),
        // typeahead: a key queued before a question exists never answers it
        Step::Call("'S' and 'y' typed in one frame", |g| {
            for c in ['S', 'y'] {
                g.push_ui(UiEvent::Key(KeyInput::plain(Key::Char(c))));
            }
            Ok(())
        }),
        Step::Request(
            "\"Really save?\" left open by the 'y' typed ahead",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        key('n'),
        Step::Request("a command after not saving", command),
        // a text window the engine does not block on is still shown
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("version".into())),
        Step::Request("the #version text", |p| {
            matches!(p, Prompt::Show { lines, .. }
                if lines.iter().any(|l| l.text.contains("NetHack Version")))
        }),
        Step::Dialog(DialogEvent::Close),
        Step::Request("a command after #version", command),
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Wait("the palette with the keyboard", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("extcmd") && ui.dialogs.text_has_focus())
        }),
        Step::Shot("palette"),
        Step::Dialog(DialogEvent::TextSubmitted("quit".into())),
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("quit")),
        ),
        Step::Shot("question"),
        key('y'),
        Step::AnswerUntil('n', "the end screen", |g| {
            if g.state == GameState::Failed {
                return Err("the error screen came up".into());
            }
            Ok(screen(g) == Some("end"))
        }),
        Step::Wait("\"You quit\" on the end screen", |g| {
            let summary = g.ui.as_ref().and_then(|ui| ui.screens.end_summary());
            let Some(s) = summary else {
                return Ok(false);
            };
            if s.text.iter().any(|l| l.contains("You quit")) {
                Ok(true)
            } else {
                Err(format!("the summary says {:?}", s.text))
            }
        }),
        Step::Shot("end"),
    ]
}

/// A new game as in `smoke`, up to the first command.
fn start() -> Vec<Step> {
    start_as(smoke_choice())
}

/// A new game of this character, up to the first command.
fn start_as(choice: CharacterChoice) -> Vec<Step> {
    vec![
        Step::Wait("the title screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("title"))
        }),
        Step::Push(UiEvent::StartCharacter(choice)),
        Step::Request("the first command", command),
    ]
}

/// `#quit`, `y`, `n` to every question, the end screen.
fn quit() -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("quit".into())),
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("quit")),
        ),
        key('y'),
        Step::AnswerUntil('n', "the end screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("end"))
        }),
    ]
}

fn welcome_back(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(g.world.log.iter().any(|m| m.text.contains("welcome back")))
}

/// Save with `S`, see it on the title screen, continue it, quit.
fn save() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('l'),
        Step::Request("a command after a step", command),
        key('S'),
        Step::Request(
            "Really save?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        key('y'),
        Step::Wait("\"Continue: Hero\" on the title screen", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("title") && ui.screens.has_button("Continue: Hero"))
        }),
        Step::Shot("saved"),
        Step::Push(UiEvent::ContinueGame("Hero".into())),
        Step::Request("the first command of the restored game", command),
        Step::Wait("\"welcome back\" in the log", welcome_back),
    ]);
    steps.extend(quit());
    steps
}

/// Close the window mid-game: the engine saves, then the client quits
/// (the verdict comes from `on_close`).
fn close() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('l'),
        Step::Request("a command after a step", command),
        Step::Push(UiEvent::CloseRequested),
        Step::Wait("the closing state", |g| Ok(g.state == GameState::Closing)),
        Step::Wait("the engine to save and the client to quit", |_| Ok(false)),
    ]);
    steps
}

/// Kill the engine while it waits for a key, then answer: the write fails
/// (EPIPE, not SIGPIPE), the error screen offers the recovered game.
fn crash() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('l'),
        Step::Request("a command after a step", command),
        Step::Call("kill the engine", |_| kill_engine()),
        key('l'),
        Step::Wait("the error screen with \"Continue Hero\"", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("error") && ui.screens.has_button("Continue Hero"))
        }),
        Step::Shot("error"),
        Step::Push(UiEvent::ContinueGame("Hero".into())),
        Step::Request("the first command of the recovered game", command),
        Step::Wait("\"welcome back\" in the log", welcome_back),
    ]);
    steps.extend(quit());
    steps
}

/// SIGKILL the engine, a child of this process.
fn kill_engine() -> Result<(), String> {
    let pid = std::process::id().to_string();
    let status = std::process::Command::new("pkill")
        .args(["-KILL", "-P", &pid, "nh-engine"])
        .status()
        .map_err(|e| format!("pkill: {e}"))?;
    if !status.success() {
        return Err("no engine process found".into());
    }
    // let the process die and its pipes close
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}

/// A key event into Godot's input buffer: `input()` gets it next frame,
/// before `process()`, like a key the player pressed.
fn real_key(keycode: GKey, typed: char, shift: bool) {
    let mut ev = InputEventKey::new_gd();
    ev.set_keycode(keycode);
    ev.set_physical_keycode(keycode);
    ev.set_unicode(typed as u32);
    ev.set_shift_pressed(shift);
    ev.set_pressed(true);
    Input::singleton().parse_input_event(&ev);
}

/// The real key event for a key the soak chose, when there is one: plain
/// printable ASCII, Enter, Esc.
fn real_key_for(ev: &UiEvent) -> Option<(GKey, char, bool)> {
    let UiEvent::Key(k) = ev else {
        return None;
    };
    if k.mods != Mods::default() || k.echo {
        return None;
    }
    match k.key {
        Key::Enter => Some((GKey::ENTER, '\0', false)),
        Key::Escape => Some((GKey::ESCAPE, '\0', false)),
        Key::Char(c) if c.is_ascii_graphic() => {
            let code = GKey::try_from_ord(c.to_ascii_uppercase() as i32)?;
            Some((code, c, c.is_ascii_uppercase()))
        }
        _ => None,
    }
}

fn ctrl_key(c: char) -> KeyInput {
    KeyInput {
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        ..KeyInput::plain(Key::Char(c))
    }
}

fn press(c: char) -> Step {
    let upper = c.to_ascii_uppercase();
    Step::Press(GKey::from_ord(upper as i32), c, c.is_ascii_uppercase())
}

/// Real key events through `input()`: commands, Esc in a menu, typing in
/// the palette with Tab completion, Enter in a text field, a y/n answer.
fn keys() -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the title screen", |g| Ok(screen(g) == Some("title"))),
        Step::Push(UiEvent::StartCharacter(smoke_choice())),
        Step::Request("the first command", command),
        press('i'),
        Step::Wait("the inventory panel", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open()))
        }),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Wait("Esc closed it", panel_closed),
        press('l'),
        Step::Request("a command after a step", command),
        // ^P goes to the engine, which asks for the message history
        Step::Key(ctrl_key('p')),
        Step::Request("a command after ^P", command),
        Step::Wait("^P opened the full log", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.hud.full_log_open()))
        }),
        Step::Call("F9 closes it", |g| {
            g.push_ui(UiEvent::Key(KeyInput::plain(Key::F(9))));
            Ok(())
        }),
        Step::Wait("the full log closed", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| !ui.hud.full_log_open()))
        }),
        // the client's own keys: zoom and the whole level
        Step::Call("Ctrl+- zooms out", |g| {
            g.push_ui(UiEvent::Key(ctrl_key('-')));
            Ok(())
        }),
        Step::Wait("the camera a step further out", |g| {
            let d = g.ui.as_ref().ok_or("no UI")?.map.camera_distance();
            Ok(d > 12.0)
        }),
        Step::Call("F8 frames the level", |g| {
            g.push_ui(UiEvent::Key(KeyInput::plain(Key::F(8))));
            Ok(())
        }),
        Step::Wait("the overview, no closer than the hero's view", |g| {
            let map = &g.ui.as_ref().ok_or("no UI")?.map;
            Ok(map.in_overview() && map.camera_distance() > 12.0)
        }),
        Step::Wait("the camera on the level", camera_settled),
        Step::Shot("overview"),
        Step::Call("Ctrl+= goes back to the hero", |g| {
            g.push_ui(UiEvent::Key(ctrl_key('=')));
            Ok(())
        }),
        Step::Wait("the hero's view, a step closer", |g| {
            let map = &g.ui.as_ref().ok_or("no UI")?.map;
            Ok(!map.in_overview() && map.camera_distance() < 13.0)
        }),
        // travel's getpos: its goal on the prompt line, not in the log
        press('_'),
        Step::Request("the getpos tip", |p| matches!(p, Prompt::Show { .. })),
        Step::Dialog(DialogEvent::Close),
        Step::Request("where to travel", command),
        Step::Wait("the travel question on the prompt line", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.prompt_line());
            let logged = g.world.log.iter().any(|m| m.text.contains("travel to"));
            match line {
                Some(l) if g.world.getpos && l.contains("travel to") && !logged => Ok(true),
                other => Err(format!("prompt line {other:?}, in the log: {logged}")),
            }
        }),
        Step::Shot("getpos"),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after leaving getpos", command),
        Step::Wait("no prompt line after getpos", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.prompt_line());
            Ok(!g.world.getpos && line.is_none())
        }),
        // Shift+3 types '#'
        Step::Press(GKey::KEY_3, '#', true),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Wait("the palette with the keyboard", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("extcmd") && ui.dialogs.text_has_focus())
        }),
    ];
    steps.extend("quiv".chars().map(press));
    steps.push(Step::Press(GKey::TAB, '\0', false));
    steps.push(Step::Wait("Tab completes \"quiver\"", |g| {
        let text = g.ui.as_ref().and_then(|ui| ui.dialogs.text());
        Ok(text.as_deref() == Some("quiver"))
    }));
    // a held Esc never cancels a text field
    steps.push(Step::Key(KeyInput {
        echo: true,
        ..KeyInput::plain(Key::Escape)
    }));
    steps.push(Step::Wait("the palette still open after a held Esc", |g| {
        let ui = g.ui.as_ref().ok_or("no UI")?;
        Ok(g.pending
            .as_ref()
            .is_some_and(|(_, p)| *p == Prompt::ExtCmd)
            && ui.dialogs.kind_name() == Some("extcmd"))
    }));
    steps.push(Step::Press(GKey::ESCAPE, '\0', false));
    steps.push(Step::Request("a command after Esc in the palette", command));
    steps.push(Step::Press(GKey::KEY_3, '#', true));
    steps.push(Step::Request("the palette again", |p| *p == Prompt::ExtCmd));
    // "qui": quit and quiver, shortest first; Down, Up, Enter picks quit
    steps.extend("qui".chars().map(press));
    for k in [GKey::DOWN, GKey::UP, GKey::ENTER] {
        steps.push(Step::Press(k, '\0', false));
    }
    steps.extend([
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("quit")),
        ),
        // held keys never answer a question: an echo is dropped
        Step::Key(KeyInput {
            echo: true,
            ..KeyInput::plain(Key::Char('y'))
        }),
        Step::Wait("the question still open after an echo", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Choice { .. }))))
        }),
        press('y'),
        Step::AnswerUntil('n', "the end screen", |g| Ok(screen(g) == Some("end"))),
    ]);
    steps
}

fn is_menu(p: &Prompt) -> bool {
    matches!(p, Prompt::Menu { .. })
}

/// The open menu's list as the dialog lays it out.
struct MenuList<'a> {
    entries: &'a [MenuEntry],
    tops: &'a [f32],
    /// How far the list is scrolled, and the height of its view.
    top: f32,
    view_h: f32,
    cursor: Option<usize>,
}

impl MenuList<'_> {
    fn is_row_top(&self, v: f32) -> bool {
        self.tops.iter().any(|&t| (t - v).abs() < 0.5)
    }

    /// Is row `i` wholly in view?
    fn whole(&self, i: usize) -> bool {
        let t = self.tops[i];
        t >= self.top - 0.5 && t + ROW_H <= self.top + self.view_h + 0.5
    }
}

/// The menu open for the pending request, once the dialog shows it.
fn menu_list(g: &RenethackGame) -> Option<MenuList<'_>> {
    let ui = g.ui.as_ref()?;
    if ui.dialogs.open_req() != g.pending.as_ref().map(|(id, _)| *id) {
        return None;
    }
    let (top, view_h) = ui.dialogs.menu_scroll()?;
    Some(MenuList {
        entries: ui.dialogs.menu_entries()?,
        tops: ui.dialogs.menu_row_tops()?,
        top,
        view_h,
        cursor: ui.dialogs.menu_cursor(),
    })
}

/// Every kind of dialog once, for screenshots, with checks on what the
/// dialogs add to NetHack's rules: the keyboard row follows a toggled item
/// and the arrows, the palette never lists "#" and ranks prefix matches.
fn dialogs() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('i'),
        Step::Wait("the inventory panel", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open()))
        }),
        Step::Shot("dialog-inventory"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed", panel_closed),
        // 'D': the item types, then all items with two marked and a count typed
        key('D'),
        Step::Request("the item types to drop", is_menu),
        Step::Shot("dialog-drop-types"),
        key('a'),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request("the items to drop", is_menu),
        key('a'),
        key('c'),
        key('5'),
        Step::Wait("the items to drop in the panel, c selected", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let Some(entries) = ui.inventory.menu_entries() else {
                return Ok(false);
            };
            Ok(entries.iter().any(|e| e.letter == Some('c') && e.selected))
        }),
        Step::Shot("dialog-drop-items"),
        // the arrows move the keyboard cell, Space toggles it (with the count)
        Step::Key(KeyInput::plain(Key::Right)),
        key(' '),
        Step::Wait("Right and Space pick the next item with the count", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let Some(entries) = ui.inventory.menu_entries() else {
                return Ok(false);
            };
            let d = entries.iter().find(|e| e.letter == Some('d'));
            Ok(d.is_some_and(|e| e.selected && e.count == Some(5)))
        }),
        Step::Dialog(DialogEvent::MenuCancel),
        Step::Request("a command after not dropping", command),
        // a long menu: the options, paged away from the keyboard row
        key('O'),
        Step::Request("the options menu", is_menu),
        Step::Wait("the options menu longer than its view", |g| {
            let Some(m) = menu_list(g) else {
                return Ok(false);
            };
            let end = m.tops.last().map_or(0.0, |t| t + ROW_H);
            if end <= m.view_h {
                return Err(format!("{end} px of options fit in {} px", m.view_h));
            }
            Ok(m.top == 0.0)
        }),
        // the arrows put the keyboard row on the first item
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("the keyboard row on the first item", |g| {
            let Some(m) = menu_list(g) else {
                return Ok(false);
            };
            let first = m.entries.iter().position(|e| e.selectable);
            Ok(m.cursor.is_some() && m.cursor == first)
        }),
        Step::Key(KeyInput::plain(Key::PageDown)),
        Step::Wait(
            "PgDn: a page to a row top, letting the hidden row go",
            |g| {
                let Some(m) = menu_list(g) else {
                    return Ok(false);
                };
                if m.top <= 0.0 {
                    return Ok(false);
                }
                if !m.is_row_top(m.top) {
                    return Err(format!("the list stopped at {}, not at a row top", m.top));
                }
                match m.cursor {
                    Some(c) if !m.whole(c) => Err(format!("the keyboard row {c} is out of view")),
                    _ => Ok(true),
                }
            },
        ),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("Down: the first whole item in view", |g| {
            let Some(m) = menu_list(g) else {
                return Ok(false);
            };
            let Some(c) = m.cursor else {
                return Ok(false);
            };
            let first = (0..m.entries.len()).find(|&i| m.entries[i].selectable && m.whole(i));
            let top_item = m.entries.iter().position(|e| e.selectable);
            if Some(c) == first && Some(c) != top_item {
                Ok(true)
            } else {
                Err(format!(
                    "Down went to row {c}, not {first:?} (top {})",
                    m.top
                ))
            }
        }),
        Step::Shot("dialog-options"),
        Step::Key(KeyInput::plain(Key::End)),
        Step::Wait("End: the last row whole and a row top at the top", |g| {
            let Some(m) = menu_list(g) else {
                return Ok(false);
            };
            if !m.whole(m.tops.len() - 1) {
                return Ok(false);
            }
            if m.is_row_top(m.top) {
                Ok(true)
            } else {
                Err(format!("the list ends at {}, not at a row top", m.top))
            }
        }),
        Step::Dialog(DialogEvent::MenuCancel),
        Step::AnswerUntil('n', "a command after the options", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // ^X: a text window with a table
        Step::Key(KeyInput {
            mods: nh_world::Mods {
                ctrl: true,
                ..Default::default()
            },
            ..KeyInput::plain(Key::Char('x'))
        }),
        Step::Request("the attributes", |p| {
            matches!(p, Prompt::Show { .. } | Prompt::Menu { .. })
        }),
        Step::Shot("dialog-attributes"),
        Step::AnswerUntil('n', "a command after the attributes", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        key('S'),
        Step::Request(
            "Really save?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        Step::Shot("dialog-question"),
        key('n'),
        Step::Request("a command after not saving", command),
        // the palette: typed "lo", real keys into the focused field
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Wait("the palette with the keyboard and without \"#\"", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let names = ui.dialogs.palette_names().unwrap_or_default();
            if names.iter().any(|n| n == "#") {
                return Err("the palette lists \"#\"".into());
            }
            Ok(!names.is_empty() && ui.dialogs.text_has_focus())
        }),
        press('l'),
        press('o'),
        Step::Wait("\"look\" and \"loot\" first for \"lo\"", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let names = ui.dialogs.palette_names().unwrap_or_default();
            if ui.dialogs.text().as_deref() != Some("lo") {
                return Ok(false);
            }
            match names.get(..2) {
                Some([a, b]) if a == "look" && b == "loot" => Ok(true),
                _ => Err(format!("the palette shows {names:?}")),
            }
        }),
        Step::Shot("dialog-palette"),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after the palette", command),
        // a long text window: every extended command
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("?".into())),
        Step::Request("the list of extended commands", |p| {
            matches!(p, Prompt::Show { .. } | Prompt::Menu { .. })
        }),
        Step::Key(KeyInput::plain(Key::PageDown)),
        Step::Wait("PgDn scrolls the list", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.list_top().is_some_and(|v| v > 0.0))
        }),
        Step::Shot("dialog-extcmd-list"),
        Step::Dialog(DialogEvent::Close),
        // "#?" asks for a command again after the list
        Step::Request("the palette after the list", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(None)),
        Step::Request("a command after the list", command),
        // getlin: engrave with a finger
        key('E'),
        Step::Request("what to write with", |p| {
            matches!(p, Prompt::FreeKey { .. })
        }),
        key('-'),
        Step::AnswerUntil('n', "the text to engrave", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Text { .. }))))
        }),
        Step::Wait("the text field with the keyboard", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("text") && ui.dialogs.text_has_focus())
        }),
    ]);
    steps.extend("Elbereth".chars().map(press));
    steps.extend([
        Step::Wait("\"Elbereth\" typed", |g| {
            let text = g.ui.as_ref().and_then(|ui| ui.dialogs.text());
            Ok(text.as_deref() == Some("Elbereth"))
        }),
        Step::Shot("dialog-getlin"),
        Step::Press(GKey::ENTER, '\0', false),
        Step::AnswerUntil('n', "a command after engraving", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // the engine got the text: ':' reads it back
        key(':'),
        Step::AnswerUntil('n', "a command after looking here", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::Wait("\"Elbereth\" read back from the floor", |g| {
            let read = g.world.log.iter().rev().find_map(|m| {
                let rest = m.text.split_once("You read: \"")?.1;
                Some(rest.split_once('"')?.0.to_string())
            });
            match read {
                Some(text) if smudged("Elbereth", &text) => Ok(true),
                other => Err(format!("the floor reads {other:?}")),
            }
        }),
        // one piece of armor worn: a message instead of a menu
        key('['),
        Step::Request("the one worn armor as a message", |p| {
            matches!(p, Prompt::MessageMenu { pick: false, .. })
        }),
        Step::Wait("the message dialog", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("message"))
        }),
        Step::Shot("dialog-message"),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request("a command after the message", command),
        // a message that picks: Enter does nothing, the letter answers
        key('e'),
        Step::Request("what to eat", |p| matches!(p, Prompt::FreeKey { .. })),
        // '?' typed is the panel's filter: the engine's own list by hand
        Step::Call("the engine's '?'", |g| {
            g.answer(nh_protocol::Reply::Char('?' as i32));
            Ok(())
        }),
        Step::Request("the one thing to eat as a message", |p| {
            matches!(
                p,
                Prompt::MessageMenu {
                    pick: true,
                    letter: 'd',
                    ..
                }
            )
        }),
        Step::Shot("dialog-message-pick"),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Key(KeyInput::plain(Key::Char(' '))),
        Step::Wait("the message still open after Enter and Space", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let open = matches!(g.pending, Some((_, Prompt::MessageMenu { .. })));
            if open && ui.dialogs.kind_name() == Some("message") {
                Ok(true)
            } else {
                Err(format!("the message was answered: {:?}", g.pending))
            }
        }),
        key('d'),
        Step::AnswerUntil('n', "a command after eating", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // 'd' reached the engine: getobj ate the ration, no "Never mind."
        Step::Wait("the food ration eaten", |g| {
            let log: Vec<&str> = g.world.log.iter().map(|m| m.text.as_str()).collect();
            let shown = log.iter().rposition(|t| t.starts_with("d - "));
            let after = shown.map_or(&[][..], |i| &log[i + 1..]);
            if !after.is_empty() && !after.iter().any(|t| t.contains("Never mind")) {
                Ok(true)
            } else {
                Err(format!("the log after the message: {after:?}"))
            }
        }),
    ]);
    steps.extend(quit());
    steps
}

fn camera_settled(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(g.ui.as_ref().is_some_and(|ui| ui.map.is_settled()))
}

/// The client's copy of a status field changed here (the engine is not asked).
fn set_status(g: &mut RenethackGame, field: &str, value: &str, conds: Option<u64>) {
    g.world.status.apply(&nh_protocol::StatusUpdate {
        field: field.into(),
        value: Some(value.into()),
        conds,
        chg: 0,
        percent: 0,
        color: nh_world::NO_COLOR,
    });
}

/// The HUD's states for screenshots: hunger and burden chips, conditions
/// (a deadly one pulsing, with its banner), a hit that leaves the HP orb
/// low and throbbing, the message history. The status and the log are the
/// client's copies, changed here. Not part of `make test-client`.
fn hud() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("hud-start"),
        Step::Call("a hungry, burdened, blind, levitating, stoning hero", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let mask = |name: &str| {
                cat.conditions
                    .iter()
                    .find(|c| c.name == name)
                    .map(|c| c.mask)
                    .ok_or(format!("no condition {name}"))
            };
            let conds = mask("Blind")? | mask("Stone")? | mask("Lev")?;
            set_status(g, "hunger", "Hungry", None);
            set_status(g, "cap", "Burdened", None);
            set_status(g, "condition", "", Some(conds));
            set_status(g, "hp", "3", None);
            set_status(g, "str", "17", None);
            let turn = Some(2);
            g.world
                .log
                .push("You are slowing down.".into(), 0, turn, false);
            g.world.log.push(
                "Your limbs are stiffening.".into(),
                nh_world::ATR_URGENT,
                turn,
                false,
            );
            Ok(())
        }),
        Step::Wait("the chips, the deadly banner and a low HP orb", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let text = ui.hud.status_text();
            let (hp, ..) = ui.hud.cluster_view();
            Ok(text.contains("Hungry Burdened") && text.contains("Stone") && hp == Some((3, 16)))
        }),
        Step::Shot("hud-conditions"),
        Step::Push(UiEvent::ToggleFullLog),
        Step::Wait("the message history", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.hud.full_log_open()))
        }),
        Step::Shot("hud-history"),
    ]);
    steps
}

/// A walk through the first rooms of seed 42 for map screenshots: the start
/// room, the up stairs, a doorway, the corridor, the room with the down
/// stairs. Not part of `make test-client`.
fn tour() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("map-start"),
        // the travel command's getpos cursor a cell east, the mouse
        // hovering south-west of the hero
        key('_'),
        // the first getpos shows a tip window
        Step::Request("the getpos tip", |p| matches!(p, Prompt::Show { .. })),
        Step::Dialog(DialogEvent::Close),
        Step::Request("where to travel", command),
        key('l'),
        Step::Request("the cursor moved", command),
        Step::Call("hover south-west of the hero", |g| {
            let (x, y) = g.world.map.hero().ok_or("no hero")?;
            if let Some(ui) = g.ui.as_mut() {
                ui.map.set_hover(Some((x - 1, y + 1)));
            }
            Ok(())
        }),
        Step::Shot("map-getpos"),
        Step::Call("hover nothing", |g| {
            if let Some(ui) = g.ui.as_mut() {
                ui.map.set_hover(None);
            }
            Ok(())
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Request("a command after the travel is cancelled", command),
    ]);
    let walk = [
        ('h', None),
        ('h', Some("map-stairs")),
        ('y', None),
        ('y', None),
        ('h', Some("map-doorway")),
        ('h', None),
        ('j', None),
        ('j', None),
        ('j', None),
        ('j', Some("map-corridor")),
        ('J', None),
        ('H', None),
        ('l', None),
        ('l', Some("map-downstairs")),
    ];
    for (c, shot) in walk {
        steps.push(key(c));
        steps.push(Step::Request("a command after a move", command));
        if let Some(name) = shot {
            steps.push(Step::Wait("the camera on the hero", camera_settled));
            steps.push(Step::Shot(name));
        }
    }
    steps.extend(quit());
    steps
}

fn map_view(g: &RenethackGame) -> Result<&crate::map_view::MapView, String> {
    fail_on_error_screen(g)?;
    Ok(&g.ui.as_ref().ok_or("no UI")?.map)
}

fn hold_at(g: &mut RenethackGame, at: Option<f32>) -> Result<(), String> {
    g.ui.as_mut().ok_or("no UI")?.map.hold_motions(at);
    Ok(())
}

/// Where motions stop for a picture: a little before halfway.
const MIDWAY: f32 = 0.45;

/// A step stopped midway: the hero's model between the two cells, turned
/// the way it goes.
fn check_midstep(g: &mut RenethackGame) -> Result<(), String> {
    let map = map_view(g)?;
    let Some((from, to, yaw, now)) = map.hero_step() else {
        // the hero did not move (a wall, a fight): nothing to check
        return Ok(());
    };
    let (a, b) = (now.distance_to(from), now.distance_to(to));
    if a < 0.2 || b < 0.2 {
        return Err(format!(
            "the hero at {now:?}, not between {from:?} and {to:?}"
        ));
    }
    let way = (to.x - from.x).atan2(to.z - from.z).to_degrees();
    if (yaw - way).abs() > 0.5 {
        return Err(format!("the hero turns to {yaw} walking along {way}"));
    }
    // a character walks (its gait clip plays), the others hop
    let (gait, now) = map.hero_clips();
    godot_print!("selftest: moves: the hero walks with {gait:?}, playing {now:?}");
    if gait.is_none() || gait != now {
        return Err(format!("the hero steps with gait {gait:?} playing {now:?}"));
    }
    Ok(())
}

/// Seed 42's first steps (as in `tour`), each stopped midway: the hero and
/// the kitten between two cells, walking and facing the way they go; with
/// `--screenshots`, a picture of each. The scene must catch up with every
/// step, and the hero and the pet must have walked.
fn moves() -> Vec<Step> {
    const SHOTS: [(&str, &str); 8] = [
        ("move-1", "move-1-close"),
        ("move-2", "move-2-close"),
        ("move-3", "move-3-close"),
        ("move-4", "move-4-close"),
        ("move-5", "move-5-close"),
        ("move-6", "move-6-close"),
        ("move-7", "move-7-close"),
        ("move-8", "move-8-close"),
    ];
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Call("closer", |g| zoom_by(g, -3.0)),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Call("stop motions midway", |g| hold_at(g, Some(MIDWAY))),
    ]);
    for (c, shot) in ['h', 'h', 'y', 'y', 'h', 'h', 'j', 'j']
        .into_iter()
        .zip(SHOTS)
    {
        steps.extend([
            key(c),
            Step::Request("a command after a move", command),
            Step::Wait("the motions midway", |g| Ok(map_view(g)?.motions_held())),
            Step::Call("the hero between cells, facing the way", check_midstep),
            Step::Wait("the camera on the walking hero", camera_settled),
            Step::Call("frame the hero's cell", |g| {
                let (hero, others) = map_view(g)?.steps_under_way();
                godot_print!("selftest: moves: hero stepping {hero}, {others} others stepping");
                let cell = g.world.map.hero();
                g.ui.as_mut().ok_or("no UI")?.map.set_hover(cell);
                Ok(())
            }),
            Step::Shot(shot.0),
            Step::Call("close up", |g| zoom_by(g, -2.0)),
            Step::Shot(shot.1),
            Step::Call("back", |g| zoom_by(g, 2.0)),
            Step::Call("let them arrive", |g| {
                g.ui.as_mut().ok_or("no UI")?.map.set_hover(None);
                hold_at(g, None)
            }),
            Step::Wait("everyone arrived", |g| {
                Ok(map_view(g)?.steps_under_way() == (false, 0))
            }),
            Step::Call("stop motions midway", |g| hold_at(g, Some(MIDWAY))),
        ]);
    }
    steps.extend([
        Step::Call("let motions go", |g| hold_at(g, None)),
        Step::Wait("the hero and the pet walked", |g| {
            let s = map_view(g)?.motion_stats();
            godot_print!("selftest: moves: {s:?}");
            if s.hero_steps >= 6 && s.other_steps >= 1 {
                Ok(true)
            } else {
                Err(format!("too few steps animated: {s:?}"))
            }
        }),
    ]);
    steps.extend(quit());
    steps
}

/// What a step of the `orders` scenario remembers for a later one.
#[derive(Debug, Clone, Copy, Default)]
struct Mark {
    actions: u64,
    turn: i64,
    hero: Option<(i32, i32)>,
    at: Option<Instant>,
    hero_steps: u32,
    /// The request the descent answered last.
    pushed: Option<(u64, u64)>,
    pushes: u32,
}

static MARK: Mutex<Mark> = Mutex::new(Mark {
    actions: 0,
    turn: 0,
    hero: None,
    at: None,
    hero_steps: 0,
    pushed: None,
    pushes: 0,
});

fn marked() -> Mark {
    MARK.lock().map(|m| *m).unwrap_or_default()
}

/// Remember the order actions, the turn, the hero and the time now.
fn mark(g: &mut RenethackGame) -> Result<(), String> {
    let hero_steps =
        g.ui.as_ref()
            .map_or(0, |ui| ui.map.motion_stats().hero_steps);
    let mut m = MARK.lock().map_err(|e| e.to_string())?;
    m.actions = g.order_actions;
    m.turn = g.world.status.number("time").unwrap_or(0);
    m.hero = g.world.hero();
    m.at = Some(Instant::now());
    m.hero_steps = hero_steps;
    g.last_stop = None;
    Ok(())
}

/// Order actions sent since the mark.
fn actions_since(g: &RenethackGame) -> u64 {
    g.order_actions - marked().actions
}

/// A command waits and no order runs: the player's turn.
fn idle_command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(!g.driver.is_active() && !g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
}

/// The seed-42 start room's south-west corner, four steps from the start.
const FAR_FLOOR: (i32, i32) = (14, 7);
/// Its south-east corner.
const CORNER: (i32, i32) = (19, 7);
/// Its west doorway, to the corridor down to the stairs.
const WEST_DOORWAY: (i32, i32) = (13, 3);
/// Seed 11: the start room's east door (closed) and top doorway.
const EAST_DOOR: (i32, i32) = (28, 15);
const TOP_DOORWAY: (i32, i32) = (22, 12);
const SOUTH_EAST: (i32, i32) = (27, 17);

fn left_click((x, y): (i32, i32)) -> Step {
    Step::Push(UiEvent::MapClick { x, y, button: 1 })
}

fn hover(g: &mut RenethackGame, cell: Option<(i32, i32)>) -> Result<(), String> {
    g.test_hover = cell;
    Ok(())
}

/// The way shown ends at `goal`, of at least `n` cells.
fn preview_to(g: &RenethackGame, goal: (i32, i32), n: usize) -> Result<bool, String> {
    let way = map_view(g)?.path_shown();
    Ok(way.last() == Some(&goal) && way.len() >= n)
}

fn badge(g: &RenethackGame) -> Option<String> {
    g.ui.as_ref().and_then(|ui| ui.hud.mode_view().0)
}

/// The walk since the mark: each action a tick apart at least, each step
/// of the hero animated, arrived.
fn check_walk(g: &mut RenethackGame) -> Result<(), String> {
    let m = marked();
    let n = actions_since(g);
    if n < 3 {
        return Err(format!("only {n} actions"));
    }
    let tick = g.driver.tick();
    let times: Vec<Instant> = g
        .order_log
        .iter()
        .rev()
        .take(n as usize)
        .map(|(t, _)| *t)
        .collect();
    for w in times.windows(2) {
        let gap = w[0].duration_since(w[1]);
        // a frame's lateness either way
        if gap + Duration::from_millis(20) < tick {
            return Err(format!("two steps {gap:?} apart, the tick is {tick:?}"));
        }
    }
    let steps = map_view(g)?.motion_stats().hero_steps - m.hero_steps;
    godot_print!("selftest: orders: walked {n} actions, {steps} animated steps");
    if u64::from(steps) < n {
        return Err(format!("{n} steps, {steps} animated"));
    }
    if g.last_stop != Some(Stop::Arrived) {
        return Err(format!("the walk ended with {:?}", g.last_stop));
    }
    Ok(())
}

/// Seed 42: down the corridor with counts, then `>` walks to the stairs
/// and goes down. Each command is answered once; another question is
/// cancelled.
fn descend(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    if g.world.status.number("leveldesc").is_some_and(|d| d >= 2) {
        return Ok(true);
    }
    if g.driver.is_active() {
        return Ok(false);
    }
    let Some((id, prompt)) = g.pending.clone() else {
        return Ok(false);
    };
    let req = (g.session_serial, id);
    let mut m = MARK.lock().map_err(|e| e.to_string())?;
    if m.pushed == Some(req) {
        return Ok(false);
    }
    m.pushed = Some(req);
    m.pushes += 1;
    if m.pushes > 40 {
        return Err("no way down in 40 commands".into());
    }
    let key = |c| UiEvent::Key(KeyInput::plain(Key::Char(c)));
    if prompt != Prompt::Command {
        g.push_ui(UiEvent::Key(KeyInput::plain(Key::Escape)));
        return Ok(false);
    }
    let cat = g.catalog.as_deref().ok_or("no catalog")?;
    let hero = g.world.hero().ok_or("no hero")?;
    if nh_world::stairs_order(&g.world, cat, '>').is_some() {
        godot_print!("selftest: orders: the stairs down are known, '>' from {hero:?}");
        g.push_ui(key('>'));
    } else if hero == WEST_DOORWAY {
        g.push_ui(key('h'));
    } else if hero.0 == WEST_DOORWAY.0 - 1 {
        g.push_ui(UiEvent::Key(alt('2')));
        g.push_ui(UiEvent::Key(alt('0')));
        g.push_ui(key('j'));
    } else {
        let (x, y) = WEST_DOORWAY;
        g.push_ui(UiEvent::MapClick { x, y, button: 1 });
    }
    Ok(false)
}

/// Orders (spec 4): a click walks one step per tick and arrives, a held
/// key steps until let go, `5s` searches five turns, Esc stops a walk
/// before its next step, `>` walks to the stairs and goes down; in a
/// second game (seed 11) a click on a closed door walks up and opens it,
/// a hostile coming into view stops a walk and starts a fight, and in the
/// fight a click takes one step. With `--screenshots`: the way previewed
/// while exploring and in the fight, and the combat banner.
fn orders() -> Vec<Step> {
    let plain_key = |c| KeyInput::plain(Key::Char(c));
    let mut steps = start();
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Wait("exploring", |g| {
            Ok(badge(g).is_some_and(|b| b.starts_with("EXPLORING")))
        }),
        // the way a click would walk, under the mouse
        Step::Call("hover a far floor cell", |g| hover(g, Some(FAR_FLOOR))),
        Step::Wait("the way to it previewed", |g| preview_to(g, FAR_FLOOR, 3)),
        Step::Shot("orders-preview"),
        Step::Call("the mouse away", |g| hover(g, None)),
        Step::Wait("the preview gone", |g| {
            Ok(map_view(g)?.path_shown().is_empty())
        }),
        // a click: one step per tick, each animated, and there
        Step::Wait("the player's turn", idle_command),
        Step::Call("mark", mark),
        left_click(FAR_FLOOR),
        Step::Wait("walking", |g| Ok(g.driver.is_active())),
        Step::Wait("the order line says so", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.mode_view().1);
            Ok(line.is_some_and(|l| l.starts_with("Walking")))
        }),
        Step::Call("the stride goes on between steps", |g| {
            // between two steps of the walk (the second has not gone out):
            // the hero is still in the walk clip, not back to idle
            if actions_since(g) >= 2 || !g.driver.is_active() {
                return Err("the walk went on too fast to look".into());
            }
            Ok(())
        }),
        Step::Wait("the first step played", |g| {
            let walking = map_view(g)?.hero_walking();
            let moved = g.world.hero() != marked().hero;
            Ok(actions_since(g) == 1 && moved && !walking && g.driver.is_active())
        }),
        Step::Call("still striding", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            let (now, idle) = ui.map.hero_animation();
            godot_print!("selftest: orders: between steps the hero plays {now:?} (idle {idle:?})");
            if now.is_some() && now == idle {
                return Err(format!("the hero stands idle between steps: {now:?}"));
            }
            Ok(())
        }),
        Step::Wait("arrived", |g| {
            Ok(idle_command(g)? && g.world.hero() == Some(FAR_FLOOR))
        }),
        Step::Call("one step a tick, each animated", check_walk),
        // a held key: the press steps, the key's repeat starts the order,
        // letting go ends it
        Step::Call("mark", mark),
        Step::Key(plain_key('k')),
        Step::Request("a command after the press", command),
        Step::Push(UiEvent::Key(KeyInput {
            echo: true,
            ..plain_key('k')
        })),
        Step::Wait("two held steps", |g| Ok(actions_since(g) >= 2)),
        Step::Push(UiEvent::KeyUp(plain_key('k'))),
        Step::Wait("let go", |g| Ok(!g.driver.is_active())),
        Step::Call("mark", mark),
        Step::Wait("no step after letting go", |g| {
            let m = marked();
            if m.at.is_some_and(|t| t.elapsed() < g.driver.tick() * 3) {
                return Ok(false);
            }
            match actions_since(g) {
                0 => Ok(true),
                n => Err(format!("{n} steps after the key was let go")),
            }
        }),
        Step::Wait("north of where it started", |g| {
            let hero = g.world.hero().ok_or("no hero")?;
            if hero.1 <= FAR_FLOOR.1 - 3 {
                Ok(true)
            } else {
                Err(format!("the hero at {hero:?} after holding north"))
            }
        }),
        // a count: 5s searches five turns, one action a tick
        Step::Wait("the player's turn", idle_command),
        Step::Call("mark", mark),
        Step::Key(alt('5')),
        Step::Wait("the count on the prompt line", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.prompt_line());
            Ok(line.as_deref() == Some("Count: 5"))
        }),
        Step::Key(plain_key('s')),
        Step::Wait("five searches", |g| {
            Ok(idle_command(g)? && actions_since(g) >= 5)
        }),
        Step::Call("five turns", |g| {
            let turns = g.world.status.number("time").unwrap_or(0) - marked().turn;
            let n = actions_since(g);
            let searched = g.order_log.iter().rev().take(5).all(|(_, c)| *c == 's');
            if n == 5 && turns == 5 && searched && g.last_stop == Some(Stop::Done) {
                Ok(())
            } else {
                Err(format!("{n} actions, {turns} turns, {:?}", g.last_stop))
            }
        }),
        // a key stops a walk before its next step
        Step::Wait("the player's turn", idle_command),
        Step::Call("mark", mark),
        left_click(CORNER),
        Step::Wait("the first step", |g| {
            Ok(actions_since(g) >= 1 && g.driver.is_active())
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("stopped by the key", |g| {
            Ok(!g.driver.is_active() && g.last_stop == Some(Stop::Key))
        }),
        Step::Call("mark", mark),
        Step::Wait("no step after the key, the corner not reached", |g| {
            let m = marked();
            if m.at.is_some_and(|t| t.elapsed() < g.driver.tick() * 3) {
                return Ok(false);
            }
            if actions_since(g) != 0 || g.world.hero() == Some(CORNER) {
                return Err(format!(
                    "{} steps after Esc, the hero at {:?}",
                    actions_since(g),
                    g.world.hero()
                ));
            }
            Ok(true)
        }),
        // the corridor by counts, then '>' walks to the stairs, goes down
        Step::Call("start counting", |g| {
            mark(g)?;
            let mut m = MARK.lock().map_err(|e| e.to_string())?;
            m.pushed = None;
            m.pushes = 0;
            Ok(())
        }),
        Step::Wait("down the corridor and the stairs to Dlvl 2", descend),
        Step::Wait("the walk to the stairs went down them", |g| {
            if g.driver.is_active() {
                return Ok(false);
            }
            let last = g.order_log.back().map(|(_, c)| *c);
            if last == Some('>') && g.last_stop == Some(Stop::Arrived) {
                Ok(true)
            } else {
                Err(format!(
                    "the last action {last:?}, ended with {:?}",
                    g.last_stop
                ))
            }
        }),
    ]);
    steps.extend(quit());
    // the second game: seed 11
    steps.extend([
        Step::Call("seed 11", |g| {
            g.seed = Some(11);
            Ok(())
        }),
        Step::Push(UiEvent::BackToTitle),
    ]);
    steps.extend(start());
    steps.extend([
        Step::Wait("the hero on the map", |g| Ok(g.world.map.hero().is_some())),
        Step::Wait("the player's turn", idle_command),
        Step::Call("mark", mark),
        left_click(EAST_DOOR),
        Step::Wait("the door opened", |g| {
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let (x, y) = EAST_DOOR;
            let door = g.world.map.cell(x, y).and_then(|c| cell_terrain(c, cat));
            Ok(idle_command(g)? && door == Some(Terrain::OpenDoor))
        }),
        Step::Call("walked up and opened it", |g| {
            let keys: String = g.order_log.iter().rev().take(2).map(|(_, c)| *c).collect();
            if keys == "lo" && g.last_stop == Some(Stop::Arrived) {
                Ok(())
            } else {
                Err(format!("the last keys {keys:?}, {:?}", g.last_stop))
            }
        }),
        Step::Call("hover the top doorway", |g| hover(g, Some(TOP_DOORWAY))),
        Step::Wait("the way to it previewed", |g| preview_to(g, TOP_DOORWAY, 4)),
        Step::Shot("orders-preview-door"),
        Step::Call("the mouse away", |g| hover(g, None)),
        Step::Call("mark", mark),
        left_click(TOP_DOORWAY),
        Step::Wait("walking", |g| {
            Ok(g.driver.is_active() || g.last_stop.is_some())
        }),
        Step::Wait("a hostile stops the walk", |g| {
            fail_on_error_screen(g)?;
            if g.driver.is_active() {
                return Ok(false);
            }
            match &g.last_stop {
                Some(Stop::Hostile) => Ok(true),
                Some(Stop::Arrived) => Err("the walk arrived: no hostile came".into()),
                _ => Ok(false),
            }
        }),
        Step::Call("in a fight, short of the doorway", |g| {
            let (b, _, flash) =
                g.ui.as_ref()
                    .map(|ui| ui.hud.mode_view())
                    .unwrap_or_default();
            godot_print!(
                "selftest: orders: a hostile at {:?}, the hero at {:?}",
                nh_world::threats(
                    &g.world,
                    g.catalog.as_deref().ok_or("no catalog")?,
                    g.driver.peaceful()
                ),
                g.world.hero()
            );
            if g.driver.mode() != nh_world::Mode::Combat || g.world.hero() == Some(TOP_DOORWAY) {
                return Err(format!(
                    "mode {:?}, hero {:?}",
                    g.driver.mode(),
                    g.world.hero()
                ));
            }
            if !b.is_some_and(|b| b.starts_with("COMBAT")) || !flash {
                return Err("no combat badge and banner".into());
            }
            Ok(())
        }),
        Step::Shot("orders-combat"),
        Step::Call("hover a far cell", |g| hover(g, Some(SOUTH_EAST))),
        Step::Wait("the way previewed in the fight", |g| {
            preview_to(g, SOUTH_EAST, 2)
        }),
        Step::Shot("orders-combat-preview"),
        Step::Call("the mouse away", |g| hover(g, None)),
        // in a fight a click takes one step
        Step::Wait("the player's turn", idle_command),
        Step::Call("mark", mark),
        left_click(SOUTH_EAST),
        Step::Wait("the order given", |g| {
            Ok(g.driver.is_active() || g.last_stop.is_some())
        }),
        Step::Wait("one action", |g| {
            if !idle_command(g)? {
                return Ok(false);
            }
            match (actions_since(g), &g.last_stop) {
                (1, Some(Stop::OneAction)) => Ok(true),
                (n, why) => Err(format!("{n} actions, ended with {why:?}")),
            }
        }),
    ]);
    steps.extend(quit());
    steps
}

fn show_page(g: &mut RenethackGame, page: usize) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    crate::gallery::lay_out(&mut g.world, &cat, page)?;
    Ok(())
}

fn zoom_by(g: &mut RenethackGame, steps: f32) -> Result<(), String> {
    g.ui.as_mut().ok_or("no UI")?.map.zoom(steps);
    Ok(())
}

/// The art gallery: pages of monsters, objects and features on a lit hall
/// in place of the level, each shot at the default distance and close up.
/// Not part of `make test-client`.
fn gallery() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    let pages: [(Check2, &'static str, &'static str); 5] = [
        (
            |g| show_page(g, 0),
            "gallery-people",
            "gallery-people-close",
        ),
        (
            |g| show_page(g, 1),
            "gallery-beasts",
            "gallery-beasts-close",
        ),
        (
            |g| show_page(g, 2),
            "gallery-bodies",
            "gallery-bodies-close",
        ),
        (
            |g| show_page(g, 3),
            "gallery-objects",
            "gallery-objects-close",
        ),
        (
            |g| show_page(g, 4),
            "gallery-features",
            "gallery-features-close",
        ),
    ];
    for (page, far, close) in pages {
        steps.extend([
            Step::Call("lay out a gallery page", page),
            Step::Wait("the page drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on the page", camera_settled),
            Step::Shot(far),
            Step::Call("closer", |g| zoom_by(g, -3.0)),
            Step::Shot(close),
            Step::Call("back", |g| zoom_by(g, 3.0)),
        ]);
    }
    steps.extend([
        Step::Call("the whole map", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            crate::gallery::lay_out_full(&mut g.world, &cat)?;
            Ok(())
        }),
        Step::Wait("the whole map drawn", |g| {
            let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
            Ok(drawn == Some(g.world.map.generation()))
        }),
        Step::Wait("the camera on the map", camera_settled),
        Step::Shot("gallery-full"),
    ]);
    steps.extend(quit());
    steps
}

type Check2 = fn(&mut RenethackGame) -> Result<(), String>;

/// A Valkyrie with the default (Modern) keys.
fn modern_choice() -> CharacterChoice {
    CharacterChoice {
        profile: KeyProfile::Modern,
        ..smoke_choice()
    }
}

fn panel(g: &RenethackGame) -> Result<&crate::inventory_panel::InventoryPanel, String> {
    Ok(&g.ui.as_ref().ok_or("no UI")?.inventory)
}

/// The letter of the first item whose name has `what`.
fn letter_of(g: &RenethackGame, what: &str) -> Option<char> {
    g.world
        .inventory
        .items()
        .iter()
        .find(|i| i.text.contains(what))
        .map(|i| i.letter)
}

fn wielding(g: &RenethackGame, what: &str) -> bool {
    g.world
        .inventory
        .wielded()
        .is_some_and(|i| i.text.contains(what))
}

fn inv(ev: crate::inventory_panel::InvInput) -> Step {
    Step::Push(UiEvent::Inventory(ev))
}

/// Push an inventory input built from the game's state (letters).
fn inv_from(
    what: &'static str,
    f: fn(&RenethackGame) -> Option<crate::inventory_panel::InvInput>,
) -> Step {
    Step::Inv(what, f)
}

/// The inventory panel (ui-design §2, §3): `i` opens it on every item,
/// a filter shows only weapons, the dagger is wielded by a drag to the
/// main hand and the long sword again by its context menu (the
/// inventory notice confirms both), `w` opens selection mode with the
/// weapons pulsing and the bare hands offered, Esc cancels it, `w` and a
/// letter still wield as in tty, and `D` drops the food ration through
/// the panel's multi-select mode. Screenshots of each state.
fn inventory() -> Vec<Step> {
    use crate::inventory_panel::{DollSlot, InvInput, InvTarget};
    use nh_world::{InvFilter, ItemActionKind};
    let mut steps = start_as(modern_choice());
    steps.extend([
        Step::Wait("the first inventory", |g| Ok(g.world.inventory.received())),
        Step::Wait("the camera on the hero", camera_settled),
        key('i'),
        Step::Wait("the panel in browse mode with every item", |g| {
            let p = panel(g)?;
            Ok(p.mode_name() == Some("browse")
                && p.shown().len() == g.world.inventory.items().len())
        }),
        Step::Call("no engine inventory menu", |g| match &g.pending {
            Some((_, Prompt::Command)) => Ok(()),
            other => Err(format!("the engine got `i`: {other:?}")),
        }),
        inv_from("select the spear", |g| {
            Some(InvInput::Click {
                target: InvTarget::Cell(letter_of(g, "spear")?),
                button: 1,
                shift: false,
                double: false,
            })
        }),
        Step::Wait("its detail", |g| {
            Ok(panel(g)?.selected() == letter_of(g, "spear"))
        }),
        Step::Shot("inventory-browse"),
        inv(InvInput::Filter(InvFilter::Weapons)),
        Step::Wait("only the weapons", |g| {
            let shown = panel(g)?.shown();
            let weapons = g
                .world
                .inventory
                .items()
                .iter()
                .filter(|i| i.class == ')')
                .count();
            Ok(shown.len() == weapons
                && shown.iter().all(|(c, _)| {
                    g.world
                        .inventory
                        .by_letter(*c)
                        .is_some_and(|i| i.class == ')')
                }))
        }),
        Step::Shot("inventory-weapons"),
        inv(InvInput::Filter(InvFilter::All)),
        // a drag from the grid to the main hand wields
        inv_from("drag the dagger to the main hand", |g| {
            Some(InvInput::Drop {
                from: InvTarget::Cell(letter_of(g, "dagger")?),
                to: InvTarget::Doll(DollSlot::Main),
                shift: false,
            })
        }),
        Step::Wait("the dagger wielded", |g| {
            Ok(wielding(g, "dagger") && idle_command(g)?)
        }),
        Step::Wait("the doll shows it", |g| {
            let d = letter_of(g, "dagger");
            Ok(panel(g)?
                .doll_letters()
                .iter()
                .any(|(s, l)| *s == DollSlot::Main && l.first().copied() == d))
        }),
        // the context menu wields the spear again
        inv_from("right-click the spear", |g| {
            Some(InvInput::Click {
                target: InvTarget::Cell(letter_of(g, "spear")?),
                button: 2,
                shift: false,
                double: false,
            })
        }),
        Step::Wait("its context menu offers Wield", |g| {
            Ok(panel(g)?
                .context_rows()
                .is_some_and(|r| r.first() == Some(&ItemActionKind::Wield)))
        }),
        Step::Shot("inventory-context"),
        inv_from("Wield", |g| {
            Some(InvInput::Action {
                letter: letter_of(g, "spear")?,
                kind: ItemActionKind::Wield,
            })
        }),
        Step::Wait("the spear wielded", |g| {
            Ok(wielding(g, "spear") && idle_command(g)?)
        }),
        // a getobj question: selection mode
        key('w'),
        Step::Request(
            "What do you want to wield?",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("wield")),
        ),
        Step::Wait(
            "selection mode: the weapons suggested, the hands offered",
            |g| {
                let p = panel(g)?;
                if p.mode_name() != Some("select") {
                    return Ok(false);
                }
                let shown = p.shown();
                let on = |c: Option<char>| shown.iter().any(|&(l, s)| Some(l) == c && s);
                let off = |c: Option<char>| shown.iter().any(|&(l, s)| Some(l) == c && !s);
                Ok(p.filter() == InvFilter::Suggested
                    && on(Some('-'))
                    && on(letter_of(g, "dagger")))
                .map(|ok| ok && !off(letter_of(g, "dagger")))
            },
        ),
        Step::Shot("inventory-getobj"),
        key('*'),
        Step::Wait("'*' shows everything, not suggested dimmed", |g| {
            let p = panel(g)?;
            Ok(p.filter() == InvFilter::All && p.shown().iter().any(|&(_, on)| !on))
        }),
        Step::Shot("inventory-getobj-all"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Request("a command after Esc", command),
        Step::Wait("back in browse mode", |g| {
            Ok(panel(g)?.mode_name() == Some("browse"))
        }),
        // typing the letter still answers
        key('w'),
        Step::Request("wield what, again", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the dagger's letter", |g| {
            let c = letter_of(g, "dagger").ok_or("no dagger")?;
            Ok(KeyInput::plain(Key::Char(c)))
        }),
        Step::Wait("the dagger wielded by its letter", |g| {
            Ok(wielding(g, "dagger") && idle_command(g)?)
        }),
        // D: the second menu is the panel's multi-select mode
        key('D'),
        Step::Request("the item types to drop", is_menu),
        Step::KeyFrom("\"All types\"", |g| entry_key(g, "All types")),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request("the items to drop", is_menu),
        Step::Wait("multi-select mode", |g| {
            Ok(panel(g)?.mode_name() == Some("menu"))
        }),
        inv_from("click the food ration", |g| {
            Some(InvInput::Click {
                target: InvTarget::Cell(letter_of(g, "food ration")?),
                button: 1,
                shift: false,
                double: false,
            })
        }),
        Step::Wait("the food ration checked", |g| {
            let f = letter_of(g, "food ration");
            Ok(panel(g)?
                .menu_entries()
                .is_some_and(|e| e.iter().any(|e| e.letter == f && e.selected)))
        }),
        Step::Shot("inventory-multidrop"),
        inv(InvInput::Confirm),
        Step::Wait("the food ration dropped", |g| {
            Ok(idle_command(g)? && letter_of(g, "food ration").is_none())
        }),
        Step::Wait("browse mode again", |g| {
            Ok(panel(g)?.mode_name() == Some("browse"))
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed", panel_closed),
    ]);
    steps.extend(quit());
    steps
}

/// How many actions the bar test's searches start from.
static SEARCH_FROM: AtomicU32 = AtomicU32::new(0);

fn mark_searches(g: &mut RenethackGame) -> Result<(), String> {
    SEARCH_FROM.store(g.order_actions as u32, Ordering::Relaxed);
    Ok(())
}

/// Twenty searches as one order since the mark.
fn searched_twenty(g: &RenethackGame) -> Result<bool, String> {
    let n = g.order_actions - u64::from(SEARCH_FROM.load(Ordering::Relaxed));
    if !idle_command(g)? || n < 20 {
        return Ok(false);
    }
    let all_s = g.order_log.iter().rev().take(20).all(|(_, c)| *c == 's');
    if n == 20 && all_s {
        Ok(true)
    } else {
        Err(format!(
            "{n} actions, the last ones {:?}",
            g.order_log.iter().rev().take(3).collect::<Vec<_>>()
        ))
    }
}

/// The action bar (ui-design §4): a new Valkyrie's default loadout, the
/// food ration bound to slot 2 by a drag from the panel, `#adjust` gives
/// it another letter and the slot follows it, `2` eats it (then the slot
/// shows it gone), `n20s` searches 20 turns (Modern); a Classic
/// character's `Alt+2 Alt+0 s` does too.
fn bar() -> Vec<Step> {
    use crate::inventory_panel::{InvInput, InvTarget};
    use nh_world::{BarCommand, ItemActionKind, SlotBinding};
    let mut steps = start_as(modern_choice());
    steps.extend([
        Step::Wait("the default loadout", |g| {
            let bar = &g.ui_state.bar;
            Ok(matches!(bar.get(2), Some(SlotBinding::Command { cmd: BarCommand::Search }))
                && matches!(bar.get(0), Some(SlotBinding::Command { cmd: BarCommand::Swap })))
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("bar-loadout"),
        key('i'),
        Step::Wait("the panel", |g| Ok(panel(g)?.is_open())),
        inv_from("drag the food ration to slot 2", |g| {
            Some(InvInput::Drop {
                from: InvTarget::Cell(letter_of(g, "food ration")?),
                to: InvTarget::Bar(1),
                shift: false,
            })
        }),
        Step::Wait("slot 2 eats the food ration", |g| {
            Ok(matches!(
                g.ui_state.bar.get(1),
                Some(SlotBinding::Item { action: ItemActionKind::Eat, text, .. }) if text.contains("food ration")
            ))
        }),
        Step::Shot("bar-bound"),
        // #adjust: the food goes to letter z, the slot follows
        inv_from("adjust the food ration", |g| {
            Some(InvInput::Action {
                letter: letter_of(g, "food ration")?,
                kind: ItemActionKind::Adjust,
            })
        }),
        key('z'),
        Step::Wait("the food ration at z, and slot 2 with it", |g| {
            let at_z = letter_of(g, "food ration") == Some('z');
            let bound = matches!(g.ui_state.bar.get(1), Some(SlotBinding::Item { letter: 'z', .. }));
            Ok(idle_command(g)? && at_z && bound)
        }),
        Step::Call("the binding survives in the state file", |g| {
            let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
            let text = nh_link::read_ui_state(&pg, "Hero").ok_or("no state file")?;
            let st = nh_world::UiState::from_json(&text).map_err(|e| e.to_string())?;
            match st.bar.get(1) {
                Some(SlotBinding::Item { letter: 'z', .. }) => Ok(()),
                other => Err(format!("slot 2 in the file: {other:?}")),
            }
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed", panel_closed),
        // `2` uses slot 2
        key('2'),
        Step::AnswerUntil('n', "the food ration eaten, slot 2 greyed", |g| {
            let gone = g.ui_state.bar.view(1, &g.world.inventory).state == nh_world::SlotState::Gone;
            Ok(gone && idle_command(g)?)
        }),
        Step::Wait("the bar shows it gone", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (.., slots) = ui.hud.cluster_view();
            Ok(slots.get(1) == Some(&(true, false)))
        }),
        Step::Shot("bar-gone"),
        // n20s: twenty searches (Modern)
        Step::Call("mark", mark_searches),
        key('n'),
        key('2'),
        key('0'),
        Step::Wait("the count on the prompt line", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.prompt_line());
            Ok(line.as_deref() == Some("Count: 20"))
        }),
        key('s'),
        Step::Wait("twenty searches", searched_twenty),
    ]);
    steps.extend(quit());
    steps.extend([
        Step::Push(UiEvent::BackToTitle),
        Step::Wait("the title screen", |g| Ok(screen(g) == Some("title"))),
        Step::Push(UiEvent::StartCharacter(CharacterChoice {
            name: "Classic".into(),
            ..smoke_choice()
        })),
        Step::Request("the first command (Classic)", command),
        Step::Wait("vi-keys: number_pad off", |g| {
            Ok(!g.world.number_pad && g.ui_state.profile == KeyProfile::Classic)
        }),
        Step::Call("mark", mark_searches),
        Step::Key(alt('2')),
        Step::Key(alt('0')),
        Step::Wait("the count on the prompt line", |g| {
            let line = g.ui.as_ref().and_then(|ui| ui.hud.prompt_line());
            Ok(line.as_deref() == Some("Count: 20"))
        }),
        key('s'),
        Step::Wait("twenty searches", searched_twenty),
    ]);
    steps.extend(quit());
    steps
}

/// The menu dialog open for the pending request, if any.
fn open_menu(g: &RenethackGame) -> Option<&[MenuEntry]> {
    let ui = g.ui.as_ref()?;
    let pending = Some(g.pending.as_ref()?.0);
    if ui.inventory.request() == pending {
        return ui.inventory.menu_entries();
    }
    if ui.dialogs.open_req() != pending {
        return None;
    }
    ui.dialogs.menu_entries()
}

/// The inventory panel is closed.
fn panel_closed(g: &RenethackGame) -> Result<bool, String> {
    Ok(g.ui.as_ref().is_some_and(|ui| !ui.inventory.is_open()))
}

/// A top-row digit with Alt: Classic's count.
fn alt(c: char) -> KeyInput {
    KeyInput {
        key: Key::Char(c),
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
        echo: false,
    }
}

/// The key of the selectable entry whose text contains `what`.
fn entry_key(g: &RenethackGame, what: &str) -> Result<KeyInput, String> {
    let entries = open_menu(g).ok_or("no menu is open")?;
    let entry = entries
        .iter()
        .find(|e| e.selectable && e.text.contains(what))
        .ok_or_else(|| {
            let texts: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
            format!("no entry {what:?} in {texts:?}")
        })?;
    let c = entry
        .letter
        .ok_or_else(|| format!("{:?} has no letter", entry.text))?;
    Ok(KeyInput::plain(Key::Char(c)))
}

fn selected(entries: &[MenuEntry]) -> Vec<&str> {
    entries
        .iter()
        .filter(|e| e.selected)
        .map(|e| e.text.as_str())
        .collect()
}

fn logged(g: &RenethackGame, text: &str) -> bool {
    g.world.log.iter().any(|m| m.text.contains(text))
}

fn menu_with(p: &Prompt, how: PickHow, text: &str) -> bool {
    matches!(p, Prompt::Menu { how: h, items, .. }
        if *h == how && items.iter().any(|i| i.str.as_deref().is_some_and(|s| s.contains(text))))
}

/// How many ya the menu of weapons showed before the drop (steps are plain
/// functions: they keep nothing themselves).
static YA_BEFORE: AtomicU32 = AtomicU32::new(0);

/// The number the stack of ya in the open menu starts with ("38 +0 ya").
fn ya_in_menu(g: &RenethackGame) -> Result<Option<u32>, String> {
    let Some(entries) = open_menu(g) else {
        return Ok(None);
    };
    let ya = entries
        .iter()
        .find(|e| e.selectable && e.text.contains(" ya"))
        .ok_or("no ya in the menu")?;
    let n = ya
        .text
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok());
    n.map(Some)
        .ok_or_else(|| format!("no number in {:?}", ya.text))
}

/// `D`: in the pick-any menu of item types '.' selects every type but
/// no skipinvert entry, a letter selects; the next menu takes a typed
/// count: a samurai drops 2 of the ya, and the turn passes and 2 fewer are
/// in the inventory. Then `w`: the panel in selection mode, where `?`
/// only changes its filter, offers the bare hands, and `-` picks them.
fn menus() -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: "samurai".into(),
        align: "lawful".into(),
        ..smoke_choice()
    });
    steps.extend([
        Step::Wait("T:1 on the status", |g| {
            match g.world.status.number("time") {
                None => Ok(false),
                Some(1) => Ok(true),
                Some(t) => Err(format!("the game starts on turn {t}")),
            }
        }),
        key('D'),
        Step::Request("the pick-any menu of item types", |p| {
            menu_with(p, PickHow::Any, "Weapons")
        }),
        Step::Wait("skipinvert entries in the menu", |g| {
            Ok(open_menu(g).is_some_and(|e| e.iter().any(|e| e.skipinvert)))
        }),
        Step::Shot("menu-types"),
        key('.'),
        Step::Wait("'.' selects every type and no skipinvert entry", |g| {
            let Some(entries) = open_menu(g) else {
                return Ok(false);
            };
            if selected(entries).is_empty() {
                return Ok(false);
            }
            let wrong: Vec<&str> = entries
                .iter()
                .filter(|e| e.selectable && e.selected == e.skipinvert)
                .map(|e| e.text.as_str())
                .collect();
            if wrong.is_empty() {
                Ok(true)
            } else {
                Err(format!("wrongly (un)selected: {wrong:?}"))
            }
        }),
        key('-'),
        Step::Wait("'-' clears the selection", |g| {
            Ok(open_menu(g).is_some_and(|e| selected(e).is_empty()))
        }),
        Step::KeyFrom("the letter of \"Weapons\"", |g| entry_key(g, "Weapons")),
        Step::Wait("only \"Weapons\" selected", |g| {
            let Some(entries) = open_menu(g) else {
                return Ok(false);
            };
            match selected(entries).as_slice() {
                [] => Ok(false),
                ["Weapons"] => Ok(true),
                other => Err(format!("selected {other:?}")),
            }
        }),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request("the menu of weapons to drop", |p| {
            menu_with(p, PickHow::Any, " ya")
        }),
        Step::Wait("a stack of ya in the menu", |g| {
            let Some(n) = ya_in_menu(g)? else {
                return Ok(false);
            };
            if n < 3 {
                return Err(format!("only {n} ya"));
            }
            YA_BEFORE.store(n, Ordering::Relaxed);
            Ok(true)
        }),
        key('2'),
        Step::KeyFrom("the ya's letter", |g| entry_key(g, " ya")),
        Step::Wait("the ya selected with a count of 2", |g| {
            let Some(entries) = open_menu(g) else {
                return Ok(false);
            };
            let ya = entries.iter().find(|e| e.text.contains(" ya"));
            match ya {
                Some(e) if !e.selected => Ok(false),
                Some(e) if e.count == Some(2) => Ok(true),
                other => Err(format!("the ya entry is {other:?}")),
            }
        }),
        Step::Shot("menu-count"),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Request("a command after the drop", command),
        Step::Wait("\"You drop 2\" in the log", |g| {
            Ok(g.world
                .log
                .iter()
                .any(|m| m.text.starts_with("You drop 2 ") && m.text.contains(" ya")))
        }),
        // the drop was the first action, on turn 1
        Step::Wait("the turn passes (T:2)", |g| {
            Ok(g.world.status.number("time").is_some_and(|t| t >= 2))
        }),
        Step::Wait("2 ya fewer in the inventory", |g| {
            let ya = g
                .world
                .inventory
                .items()
                .iter()
                .find(|i| i.text.contains(" ya"));
            let Some(ya) = ya else {
                return Err("no ya in the pack".into());
            };
            let before = i64::from(YA_BEFORE.load(Ordering::Relaxed));
            Ok(ya.quan + 2 == before)
        }),
        key('w'),
        Step::Request(
            "What do you want to wield?",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("wield")),
        ),
        // '?' is the panel's filter, never the engine's
        key('?'),
        Step::Wait("the panel in selection mode, the bare hands offered", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.inventory.mode_name() == Some("select")
                && ui.inventory.shown().iter().any(|&(c, on)| c == '-' && on))
        }),
        key('-'),
        Step::Request("a command after wielding nothing", command),
        Step::Wait("\"bare handed\" in the log", |g| {
            Ok(logged(g, "bare handed"))
        }),
    ]);
    steps.extend(quit());
    steps
}

/// Is `read` what engraving `written` in the dust can leave? Each letter
/// of it may turn into another printable character as it is written (1 in
/// 25), so at least 6 of 8 are still in place and nothing else changed.
fn smudged(written: &str, read: &str) -> bool {
    let (w, r): (Vec<char>, Vec<char>) = (written.chars().collect(), read.chars().collect());
    let kept = w.iter().zip(&r).filter(|(a, b)| a == b).count();
    w.len() == r.len()
        && kept * 4 >= w.len() * 3
        && w.iter()
            .zip(&r)
            .all(|(a, b)| a == b || (*a != ' ' && b.is_ascii_graphic()))
}

/// `E` `-`: write "Elbereth" in the dust, typed as real key events into
/// the focused text field and submitted with Enter; `:` reads it back.
fn text() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        key('E'),
        Step::Request(
            "What do you want to write with?",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("write with")),
        ),
        key('-'),
        Step::Request(
            "What do you want to write in the dust here?",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("write in the dust")),
        ),
        Step::Wait("the text field with the keyboard", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("text") && ui.dialogs.text_has_focus())
        }),
    ]);
    steps.extend("Elbereth".chars().map(press));
    steps.extend([
        Step::Wait("\"Elbereth\" in the text field", |g| {
            let text = g.ui.as_ref().and_then(|ui| ui.dialogs.text());
            Ok(text.as_deref() == Some("Elbereth"))
        }),
        Step::Shot("text"),
        Step::Press(GKey::ENTER, '\0', false),
        Step::Request("a command after engraving", command),
        key(':'),
        Step::AnswerUntil('n', "the engraving read back", |g| {
            fail_on_error_screen(g)?;
            let read = g.world.log.iter().rev().find_map(|m| {
                let text = m.text.strip_prefix("You read: \"")?;
                Some(text.trim_end_matches(['.', '"']).to_string())
            });
            let waits = g.pending.as_ref().is_some_and(|(_, p)| command(p));
            match read {
                Some(r) if waits && smudged("Elbereth", &r) => Ok(true),
                Some(r) if waits => Err(format!("read back {r:?}")),
                _ => Ok(false),
            }
        }),
    ]);
    steps.extend(quit());
    steps
}

/// Requests the soak answers unless `--soak=N` says otherwise.
const SOAK_BUDGET: u32 = 2000;
/// A soak with at least this budget fails when the level never changed.
const SOAK_MUST_DESCEND: u32 = 1000;
/// No new request (or no answer) this long: something is stuck.
const SOAK_STALL: Duration = Duration::from_secs(30);
/// With `--screenshots`, the soak saves the screen every this many answers.
const SOAK_SHOT_EVERY: u32 = 60;
/// The scene may grow this much over the first level change's node count
/// (bigger levels, more kinds of models in the pools), and no more.
const NODE_GROWTH: f64 = 2.0;
const NODE_SLACK: f64 = 4000.0;
/// Turns on a new level before the first try to leave it.
const DESCEND_EVERY: i64 = 40;
/// Turns between trips to the stairs once one is due (they were not reached).
const DESCEND_AGAIN: i64 = 5;
/// What digs down when applied (a dwarvish mattock unidentified is a
/// "broad pick").
const DIG_TOOLS: [&str; 3] = ["pick-axe", "mattock", "broad pick"];
/// Commands between exploring trips while the stairs are not known.
const EXPLORE_EVERY: u32 = 3;
/// Decisions kept to explain a failure.
const TRACE_LEN: usize = 40;
/// Extended commands the soak may run: nothing that saves, quits, changes
/// options or modes; `pray` only on purpose, rarely.
const SAFE_EXTCMDS: [&str; 26] = [
    "adjust",
    "annotate",
    "attributes",
    "chat",
    "conduct",
    "dip",
    "discoveries",
    "enhance",
    "force",
    "glance",
    "invoke",
    "jump",
    "known",
    "look",
    "loot",
    "monster",
    "name",
    "offer",
    "overview",
    "ride",
    "rub",
    "search",
    "sit",
    "terrain",
    "untrap",
    "wipe",
];
/// Commands that ask for an object (getobj).
const OBJECT_COMMANDS: [char; 15] = [
    'e', 'q', 'r', 'a', 'W', 'w', 'P', 'R', 'T', 'd', 't', 'z', 'x', 'Q', 'f',
];

fn soak(args: &Args) -> Vec<Step> {
    let seed = args
        .seed
        .or_else(|| env_number("RENETHACK_SEED"))
        .unwrap_or(SELFTEST_SEED);
    let budget = args.soak.unwrap_or(SOAK_BUDGET);
    let mut soak = Soak::new(budget, seed);
    soak.shots = args.screenshots.clone();
    vec![Step::Soak(Box::new(soak))]
}

/// splitmix64: the soak's decisions depend only on its seed and on what
/// the engine asks.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// 0..n (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
}

fn plain(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

fn ctrl(c: char) -> KeyInput {
    KeyInput {
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        ..plain(c)
    }
}

/// The letters a getobj question lists: "[- ab or ?*]", "[$a-d or ?*]".
fn listed_letters(query: &str) -> Vec<char> {
    let Some(start) = query.rfind('[') else {
        return Vec::new();
    };
    let inner = query[start + 1..].split(']').next().unwrap_or("");
    let inner = inner.split(" or ").next().unwrap_or(inner);
    let cs: Vec<char> = inner.chars().filter(|c| *c != ' ').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let range = c == '-'
            && i > 0
            && i + 1 < cs.len()
            && cs[i - 1].is_ascii_alphabetic()
            && cs[i + 1].is_ascii_alphabetic();
        if range {
            out.extend((cs[i - 1]..=cs[i + 1]).skip(1));
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Every map cell with its known terrain.
fn terrain_cells(g: &RenethackGame) -> Vec<((i32, i32), Terrain)> {
    let Some(cat) = g.catalog.as_deref() else {
        return Vec::new();
    };
    (0..nh_world::ROWNO)
        .flat_map(|y| (1..nh_world::COLNO).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let cell = g.world.map.cell(x, y)?;
            Some(((x, y), cell_terrain(cell, cat)?))
        })
        .collect()
}

/// Are stairs or a ladder down on the map?
fn stairs_down_known(g: &RenethackGame) -> bool {
    terrain_cells(g)
        .iter()
        .any(|(_, t)| matches!(t, Terrain::StairsDown | Terrain::LadderDown))
}

/// The direction key (from `dirs`, W NW N NE E SE S SW) of a closed door
/// orthogonally next to the hero.
fn closed_door_next_to_hero(g: &RenethackGame, dirs: &[char]) -> Option<char> {
    let (hx, hy) = g.world.map.hero()?;
    let cat = g.catalog.as_deref()?;
    [(0, -1, 0), (2, 0, -1), (4, 1, 0), (6, 0, 1)]
        .into_iter()
        .find(|&(_, dx, dy)| {
            g.world
                .map
                .cell(hx + dx, hy + dy)
                .is_some_and(|c| cell_terrain(c, cat) == Some(Terrain::ClosedDoor))
        })
        .and_then(|(i, _, _)| dirs.get(i).copied())
}

/// Cells whose terrain is known (in map order: deterministic).
fn known_cells(g: &RenethackGame) -> Vec<(i32, i32)> {
    terrain_cells(g).into_iter().map(|(xy, _)| xy).collect()
}

/// Does the end screen tell of a death (not a quit, a save or an escape)?
fn tells_death(s: &EndSummary) -> bool {
    const DEATH: [&str; 16] = [
        "you die",
        "you died",
        "killed by",
        "died",
        "drown",
        "starv",
        "statue",
        "turned to stone",
        "slime",
        "choke",
        "strangl",
        "burn",
        "dissolv",
        "crushed",
        "disintegrat",
        "poison",
    ];
    let lines = || {
        s.last_messages
            .iter()
            .chain(&s.text)
            .chain(&s.scores)
            .map(|l| l.to_lowercase())
    };
    let quit = lines().any(|l| l.contains("you quit") || l.contains("escaped"));
    !quit && lines().any(|l| DEATH.iter().any(|w| l.contains(w)))
}

/// The dialog `game.rs` opened for request `id` is the one its prompt
/// needs, and none is open for the prompts without one.
fn check_dialog(g: &RenethackGame, id: u64, prompt: &Prompt) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let want = match prompt {
        Prompt::Menu { .. } => Some("menu"),
        Prompt::Choice { .. } => Some("choice"),
        Prompt::Text { .. } => Some("text"),
        Prompt::ExtCmd => Some("extcmd"),
        Prompt::Show { .. } => Some("show"),
        Prompt::MessageMenu { .. } => Some("message"),
        Prompt::Command | Prompt::Key | Prompt::FreeKey { .. } | Prompt::MapPause => None,
        Prompt::AutoAck => return Err("an AutoAck request waits for the player".into()),
    };
    let open = match ui.inventory.request() {
        Some(r) if ui.inventory.mode_name() == Some("menu") => (Some("menu"), Some(r)),
        _ => (ui.dialogs.kind_name(), ui.dialogs.open_req()),
    };
    let ok = match want {
        Some(kind) => open == (Some(kind), Some(id)),
        None => open == (None, None),
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "request {id} ({}) shows dialog {open:?}, not {want:?}",
            brief(prompt)
        ))
    }
}

/// A short account of a request for the trace.
fn brief(p: &Prompt) -> String {
    match p {
        Prompt::Choice { query, .. } => format!("Choice {query:?}"),
        Prompt::FreeKey { query, .. } => format!("FreeKey {query:?}"),
        Prompt::Menu {
            how, title, items, ..
        } => format!("Menu {how:?} {title:?} ({} items)", items.len()),
        Prompt::Text { query, .. } => format!("Text {query:?}"),
        Prompt::Show { title, lines } => {
            let first = lines.iter().map(|l| l.text.trim()).find(|t| !t.is_empty());
            format!("Show {title:?} ({} lines: {first:?}...)", lines.len())
        }
        Prompt::MessageMenu { mesg, pick, .. } => format!("MessageMenu {mesg:?} pick {pick}"),
        other => format!("{other:?}"),
    }
}

/// Random but valid play through the real UI paths: every answer is a
/// `UiEvent` the widgets or `input.rs` would queue, handled by `game.rs`.
/// Deaths lead through the end screen to a new game until the budget of
/// answered requests is spent.
pub struct Soak {
    budget: u32,
    seed: u64,
    rng: Rng,
    answered: u32,
    deaths: u32,
    games: u32,
    deepest: i64,
    /// Changes of Dlvl within a game (stairs, a hole, a trap door...).
    level_changes: u32,
    /// Dlvl last seen in this game.
    level: Option<i64>,
    /// Commands answered in this game: the clock when the status has no T:.
    commands: u32,
    /// The turn of the next try to go down.
    next_descent: i64,
    /// The command count of the next exploring trip while no stairs are known.
    next_explore: u32,
    /// Keys for the next command prompts (a held direction, a trip down).
    plan: VecDeque<KeyInput>,
    /// The next palette runs #pray.
    pray: bool,
    /// The answer to the next direction question ('o', `^D`, 'F', digging).
    dir_answer: Option<char>,
    /// The hero may carry a digging tool (it was seen, or the role starts
    /// with one).
    can_dig: bool,
    /// The digging tool's inventory letter, once an inventory showed it.
    dig_letter: Option<char>,
    /// The command now answered is `a` to dig down.
    digging: bool,
    /// The command now answered is `i`: its menu has inventory letters.
    inventory: bool,
    /// The request (session serial, id) last answered.
    last_req: (u64, u64),
    /// The last answer or screen change.
    progress: Instant,
    /// The screen the soak already acted on (None: in a game).
    handled: Option<&'static str>,
    trace: VecDeque<String>,
    /// Every decision folded together: equal for equal runs.
    digest: u64,
    verbose: bool,
    /// The last log message the trace printed.
    seen: u64,
    /// Screenshots of the play now and then (`--screenshots`).
    shots: Option<PathBuf>,
    /// Nodes in the scene tree at the first level change, and the most
    /// seen at any later one (model instances are pooled, cells reused).
    nodes: Option<(f64, f64)>,
    /// The request a click or F5 went to: when it started no order, the
    /// request is still open and is decided again.
    maybe_refused: Option<(u64, u64)>,
}

impl Soak {
    fn new(budget: u32, seed: u64) -> Soak {
        Soak {
            budget,
            seed,
            rng: Rng(seed),
            answered: 0,
            deaths: 0,
            games: 0,
            deepest: 0,
            level_changes: 0,
            level: None,
            commands: 0,
            next_descent: DESCEND_EVERY,
            next_explore: EXPLORE_EVERY,
            plan: VecDeque::new(),
            pray: false,
            dir_answer: None,
            can_dig: false,
            dig_letter: None,
            digging: false,
            inventory: false,
            last_req: (0, 0),
            progress: Instant::now(),
            handled: None,
            trace: VecDeque::new(),
            digest: 0xcbf2_9ce4_8422_2325,
            verbose: std::env::var_os("RENETHACK_SOAK_TRACE").is_some(),
            seen: 0,
            shots: None,
            nodes: None,
            maybe_refused: None,
        }
    }

    fn note(&mut self, line: String) {
        for b in line.bytes() {
            self.digest = (self.digest ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
        if self.verbose {
            godot_print!("soak: {line}");
        }
        if self.trace.len() == TRACE_LEN {
            self.trace.pop_front();
        }
        self.trace.push_back(line);
    }

    fn summary(&self) -> String {
        let (first, most) = self.nodes.unwrap_or_default();
        format!(
            "{} requests, {} deaths, deepest Dlvl {}, {} level changes, {} games, seed {}, \
             digest {:016x}, nodes {first} at the first level change, at most {most}",
            self.answered,
            self.deaths,
            self.deepest,
            self.level_changes,
            self.games,
            self.seed,
            self.digest
        )
    }

    /// Print what led to a failure.
    fn report(&self) {
        godot_print!("selftest: soak: {}", self.summary());
        for line in &self.trace {
            godot_print!("selftest: soak trace: {line}");
        }
    }

    /// One frame. Ok(true): the budget is spent and the run went deep
    /// enough: a long soak that never changed the level never tried the
    /// map's clear and full redraw.
    fn tick(&mut self, g: &mut RenethackGame) -> Result<bool, String> {
        let done = self.step(g)?;
        if done && self.budget >= SOAK_MUST_DESCEND && self.level_changes == 0 {
            return Err(format!(
                "the level never changed in {} requests",
                self.answered
            ));
        }
        // nothing piles up over level changes: every level reuses the
        // cells' nodes and the pooled models of the ones before
        if let Some((first, most)) = self.nodes
            && most > first * NODE_GROWTH + NODE_SLACK
        {
            return Err(format!(
                "{most} nodes after {} level changes, {first} at the first: nodes leak",
                self.level_changes
            ));
        }
        Ok(done)
    }

    fn step(&mut self, g: &mut RenethackGame) -> Result<bool, String> {
        if let Some(p) = first_panic() {
            return Err(format!("a panic: {p}"));
        }
        if let Some(f) = g.faults.first() {
            return Err(f.clone());
        }
        if g.state == GameState::Failed {
            let what = g.failure.clone().unwrap_or_default();
            return Err(format!("the error screen came up: {what}"));
        }
        if self.progress.elapsed() > SOAK_STALL {
            let what = match &g.pending {
                Some((id, p)) => format!("request {id} ({}) is not answered", brief(p)),
                None => "the engine sends no request".to_string(),
            };
            return Err(format!(
                "nothing happened for {} s: {what} (state {:?}, screen {:?})",
                SOAK_STALL.as_secs(),
                g.state,
                screen(g)
            ));
        }
        let now = screen(g);
        if now.is_some() && now == self.handled {
            return Ok(false);
        }
        match now {
            Some("title") if self.games > 0 => {
                Err("the title screen came back: the game was saved, not ended".into())
            }
            Some("title") => {
                self.enter("title");
                g.push_ui(UiEvent::NewGame);
                Ok(false)
            }
            Some("creation") => {
                self.enter("creation");
                self.start_game(g);
                Ok(false)
            }
            Some("end") => {
                self.enter("end");
                let summary = g.ui.as_ref().and_then(|ui| ui.screens.end_summary());
                let summary = summary.ok_or("the end screen has no summary")?;
                if !tells_death(summary) {
                    return Err(format!("the game ended without a death: {summary:?}"));
                }
                self.deaths += 1;
                let last = summary.last_messages.last().cloned().unwrap_or_default();
                godot_print!(
                    "selftest: soak: death {} after {} requests: {last}",
                    self.deaths,
                    self.answered
                );
                if self.answered >= self.budget {
                    return Ok(true);
                }
                g.push_ui(UiEvent::NewGame);
                Ok(false)
            }
            Some(other) => Err(format!("unexpected screen {other:?}")),
            None => self.play(g),
        }
    }

    fn enter(&mut self, screen: &'static str) {
        self.handled = Some(screen);
        self.progress = Instant::now();
    }

    fn start_game(&mut self, g: &mut RenethackGame) {
        let seed = self.seed.wrapping_add(u64::from(self.games));
        self.games += 1;
        self.commands = 0;
        self.level = None;
        self.next_descent = DESCEND_EVERY;
        self.next_explore = EXPLORE_EVERY;
        self.plan.clear();
        self.pray = false;
        self.dir_answer = None;
        self.digging = false;
        self.inventory = false;
        self.dig_letter = None;
        // every other game an archeologist: the pick-axe digs down where
        // the stairs are hidden; the first command's inventory shows its letter
        let digger = self.games % 2 == 1;
        self.can_dig = digger;
        let role = if digger { "archeologist" } else { "random" };
        self.seen = 0;
        godot_print!(
            "selftest: soak: game {} (engine seed {seed}, role {role})",
            self.games
        );
        self.note(format!("game {} seed {seed} {role}", self.games));
        g.seed = Some(seed);
        g.push_ui(UiEvent::StartCharacter(CharacterChoice {
            name: "Hero".into(),
            role: role.into(),
            race: "random".into(),
            gender: "random".into(),
            align: "random".into(),
            profile: KeyProfile::Classic,
        }));
    }

    fn play(&mut self, g: &mut RenethackGame) -> Result<bool, String> {
        self.handled = None;
        if g.state != GameState::Playing {
            return Ok(false);
        }
        if let Some(d) = g.world.status.number("leveldesc") {
            self.deepest = self.deepest.max(d);
            if self.level.is_some_and(|l| l != d) {
                self.level_changes += 1;
                let turn = g.world.status.number("time");
                self.next_descent = turn.unwrap_or(i64::from(self.commands)) + DESCEND_EVERY;
                let nodes = godot::classes::Performance::singleton()
                    .get_monitor(godot::classes::performance::Monitor::OBJECT_NODE_COUNT);
                let (live, built) = g.ui.as_ref().map_or((0, 0), |ui| ui.map.model_counts());
                self.nodes = Some(match self.nodes {
                    None => (nodes, nodes),
                    Some((first, most)) => (first, most.max(nodes)),
                });
                godot_print!(
                    "selftest: soak: Dlvl {d} after {} requests (game {}), {nodes} nodes, \
                     models {live} shown {built} built",
                    self.answered,
                    self.games
                );
                self.note(format!("Dlvl {d}"));
            }
            self.level = Some(d);
        }
        if self.answered >= self.budget {
            return Ok(true);
        }
        // an order answers on its own until it ends
        if g.driver.is_active() {
            self.progress = Instant::now();
            return Ok(false);
        }
        let Some((id, prompt)) = g.pending.clone() else {
            return Ok(false);
        };
        let req = (g.session_serial, id);
        if req <= self.last_req {
            // a click or F5 that started no order leaves the request open
            if self.maybe_refused != Some(req) {
                return Ok(false);
            }
            self.maybe_refused = None;
        }
        if self.verbose {
            for m in g.world.log.since(self.seen) {
                godot_print!("soak:   | {}", m.text);
            }
            if let Some(t) = &g.world.transient {
                godot_print!("soak:   ~ {t}");
            }
            godot_print!(
                "soak:   @ {:?} Dlvl {:?}",
                g.world.map.hero(),
                g.world.status.number("leveldesc")
            );
        }
        self.seen = g.world.log.last_seq();
        check_dialog(g, id, &prompt)?;
        let events = self.decide(g, id, &prompt)?;
        // `i` opens the panel, `?` and `*` change its filter: no answer
        let panel_key = |e: &UiEvent| match (e, &prompt) {
            (UiEvent::Key(k), Prompt::Command) => k.key == Key::Char('i'),
            (
                UiEvent::Key(k),
                Prompt::FreeKey {
                    directions: false, ..
                },
            ) => {
                matches!(k.key, Key::Char('?' | '*'))
            }
            _ => false,
        };
        let may_start_nothing = events.iter().any(|e| {
            panel_key(e)
                || matches!(
                    e,
                    UiEvent::MapClick { button: 1, .. }
                        | UiEvent::Key(KeyInput { key: Key::F(5), .. })
                )
        });
        if may_start_nothing {
            self.maybe_refused = Some(req);
        }
        self.note(format!(
            "#{} {} -> {events:?}",
            self.answered,
            brief(&prompt)
        ));
        // keys go through Godot's input now and then (not into a text
        // field): all of an answer or none, so the order holds
        let text = g.ui.as_ref().is_some_and(|ui| ui.dialogs.wants_text());
        let real: Option<Vec<_>> = events.iter().map(real_key_for).collect();
        match real {
            Some(keys) if !text && self.rng.chance(30) => {
                for (code, c, shift) in keys {
                    real_key(code, c, shift);
                }
            }
            _ => {
                for ev in events {
                    g.push_ui(ev);
                }
            }
        }
        self.last_req = req;
        self.answered += 1;
        self.progress = Instant::now();
        if let Some(dir) = &self.shots
            && self.answered.is_multiple_of(SOAK_SHOT_EVERY)
        {
            let path = dir.join(format!("soak-{}-{:05}.png", self.seed, self.answered));
            save_shot(g, &path)?;
        }
        if self.answered.is_multiple_of(100) {
            godot_print!(
                "selftest: soak: {}/{} requests, game {}, Dlvl {}, {} cells known, T:{}",
                self.answered,
                self.budget,
                self.games,
                g.world.status.number("leveldesc").unwrap_or(0),
                known_cells(g).len(),
                g.world.status.number("time").unwrap_or(0)
            );
        }
        Ok(false)
    }

    /// The events that answer `prompt`, all for request `id`.
    fn decide(
        &mut self,
        g: &RenethackGame,
        id: u64,
        prompt: &Prompt,
    ) -> Result<Vec<UiEvent>, String> {
        let dialog = |ev| UiEvent::Dialog { req: id, ev };
        let key = |k: KeyInput| UiEvent::Key(k);
        let named = |k: Key| UiEvent::Key(KeyInput::plain(k));
        let dirs: Vec<char> = {
            let d: Vec<char> = g.world.dirchars.chars().take(8).collect();
            if d.len() == 8 {
                d
            } else {
                "hykulnjb".chars().collect()
            }
        };
        Ok(match prompt {
            Prompt::Command => {
                // a question the last command meant to answer never came
                self.dir_answer = None;
                self.digging = false;
                self.inventory = false;
                // now and then a click on a known cell: an order to walk
                // there and act (left), the engine's menu of actions there
                // (right)
                if self.plan.is_empty() && self.rng.chance(8) {
                    let cells = known_cells(g);
                    if !cells.is_empty() {
                        let (x, y) = self.rng.pick(&cells);
                        let button = if self.rng.chance(70) { 1 } else { 2 };
                        return Ok(vec![UiEvent::MapClick { x, y, button }]);
                    }
                }
                let turn = g.world.status.number("time");
                let door = closed_door_next_to_hero(g, &dirs);
                let k = self.command_key(&dirs, stairs_down_known(g), door, turn);
                self.inventory = k == plain('i');
                match k.key {
                    // a count and its command in one answer: the client
                    // keeps the count and repeats the command as an order
                    Key::Char(c) if c.is_ascii_digit() && k.mods.alt => {
                        let then = self.plan.pop_front().unwrap_or(plain('s'));
                        vec![key(k), key(then)]
                    }
                    _ => vec![key(k)],
                }
            }
            Prompt::Key => {
                if self.rng.chance(20) {
                    vec![named(Key::Escape)]
                } else {
                    vec![key(plain(self.random_letter()))]
                }
            }
            Prompt::FreeKey {
                directions: true, ..
            } => {
                if let Some(d) = self.dir_answer.take() {
                    vec![key(plain(d))]
                } else if self.rng.chance(10) {
                    vec![named(Key::Escape)]
                } else if self.rng.chance(15) {
                    vec![key(plain(self.rng.pick(&['.', '<', '>'])))]
                } else {
                    vec![key(plain(self.rng.pick(&dirs)))]
                }
            }
            Prompt::FreeKey { query, .. } if self.digging => {
                // the digging tool by its letter, else the menu of the
                // things to apply ('?') shows it
                let listed = listed_letters(query);
                if self.dig_letter.is_none() {
                    // the pack says where the tool is (`i` is the panel's)
                    self.dig_letter = g
                        .world
                        .inventory
                        .items()
                        .iter()
                        .find(|i| DIG_TOOLS.iter().any(|t| i.text.contains(t)))
                        .map(|i| i.letter);
                }
                match self.dig_letter {
                    Some(c) if listed.contains(&c) => vec![key(plain(c))],
                    Some(_) => {
                        // lost (dropped, stolen)
                        self.dig_letter = None;
                        self.can_dig = false;
                        self.digging = false;
                        vec![named(Key::Escape)]
                    }
                    None => vec![key(plain('?'))],
                }
            }
            Prompt::FreeKey { query, .. } => {
                // never drop, throw or wear out the digging tool
                let keep = self.dig_letter;
                let listed: Vec<char> = listed_letters(query)
                    .into_iter()
                    .filter(|c| Some(*c) != keep)
                    .collect();
                let roll = self.rng.below(100);
                let c = match roll {
                    0..=9 => None,
                    10..=14 => Some('?'),
                    15..=17 => Some('*'),
                    18..=27 => Some(self.random_letter()).filter(|c| Some(*c) != keep),
                    _ if listed.is_empty() => None,
                    _ => Some(self.rng.pick(&listed)),
                };
                match c {
                    Some(c) => vec![key(plain(c))],
                    None => vec![named(Key::Escape)],
                }
            }
            // leaving the dungeon ends the game without a death
            Prompt::Choice { query, .. } if query.contains("Still climb?") => {
                vec![key(plain('n'))]
            }
            Prompt::Choice {
                visible,
                allowed,
                default,
                ..
            } => {
                // '#' asks tty for a count, which a reply cannot carry
                let hidden: Vec<char> = allowed.iter().copied().filter(|c| *c != '#').collect();
                let shown = if visible.is_empty() { &hidden } else { visible };
                let roll = self.rng.below(100);
                match (roll, default) {
                    (0..=19, Some(_)) => vec![named(Key::Enter)],
                    (20..=39, Some(d)) => vec![key(plain(*d))],
                    (90.., _) => vec![named(Key::Escape)],
                    (80.., _) if !hidden.is_empty() => vec![key(plain(self.rng.pick(&hidden)))],
                    _ if !shown.is_empty() => vec![key(plain(self.rng.pick(shown)))],
                    _ => vec![named(Key::Escape)],
                }
            }
            Prompt::Menu {
                how, title, items, ..
            } => {
                let state = MenuState::new(*how, title.clone(), items);
                self.menu(&state, id)
            }
            Prompt::Text { .. } => {
                if self.rng.chance(70) {
                    vec![dialog(DialogEvent::TextSubmitted("Elbereth".into()))]
                } else {
                    vec![named(Key::Escape)]
                }
            }
            Prompt::ExtCmd => {
                let catalog = g.catalog.as_deref().ok_or("no catalog")?;
                let safe: Vec<&str> = catalog
                    .extcmds
                    .iter()
                    .map(|e| e.name.as_str())
                    .filter(|n| SAFE_EXTCMDS.contains(n))
                    .collect();
                if std::mem::take(&mut self.pray) {
                    vec![dialog(DialogEvent::TextSubmitted("pray".into()))]
                } else if self.rng.chance(15) || safe.is_empty() {
                    if self.rng.chance(50) {
                        vec![named(Key::Escape)]
                    } else {
                        vec![dialog(DialogEvent::ExtCmd(None))]
                    }
                } else {
                    let name = self.rng.pick(&safe);
                    vec![dialog(DialogEvent::TextSubmitted(name.to_string()))]
                }
            }
            Prompt::Show { .. } => match self.rng.below(4) {
                0 => vec![named(Key::Enter)],
                1 => vec![named(Key::Escape)],
                2 => vec![key(plain(' '))],
                _ => vec![dialog(DialogEvent::Close)],
            },
            Prompt::MessageMenu {
                letter, pick: true, ..
            } => {
                if self.rng.chance(50) {
                    vec![key(plain(*letter))]
                } else {
                    vec![named(Key::Escape)]
                }
            }
            Prompt::MessageMenu { .. } => {
                if self.rng.chance(50) {
                    vec![named(Key::Enter)]
                } else {
                    vec![dialog(DialogEvent::Close)]
                }
            }
            Prompt::MapPause => vec![key(plain(' '))],
            Prompt::AutoAck => return Err("an AutoAck request waits for the player".into()),
        })
    }

    fn random_letter(&mut self) -> char {
        let letters: Vec<char> = ('a'..='z').chain('A'..='Z').collect();
        self.rng.pick(&letters)
    }

    /// `stairs`: the way down is on the map; `door`: the direction of a
    /// closed door next to the hero; `turn`: T: on the status.
    fn command_key(
        &mut self,
        dirs: &[char],
        stairs: bool,
        door: Option<char>,
        turn: Option<i64>,
    ) -> KeyInput {
        if let Some(k) = self.plan.pop_front() {
            return k;
        }
        self.commands += 1;
        if let Some(d) = door
            && self.rng.chance(50)
        {
            // open it, or kick it when it is locked
            self.dir_answer = Some(d);
            return if self.rng.chance(50) {
                plain('o')
            } else {
                ctrl('d')
            };
        }
        let now = turn.unwrap_or(i64::from(self.commands));
        let due = now >= self.next_descent;
        if due && self.can_dig && (!stairs || self.rng.chance(50)) {
            // apply the digging tool downwards: a pit, then (applied
            // again) a hole to the level below; dig on as long as digging
            // takes turns, else try elsewhere
            self.next_descent = now + 1;
            self.digging = true;
            self.dir_answer = Some('>');
            return plain('a');
        }
        if due && stairs {
            // travel to the down stairs, select, and go down
            self.next_descent = now + DESCEND_AGAIN;
            let select = if self.rng.chance(50) { '.' } else { ',' };
            self.plan.extend([plain('>'), plain(select), plain('>')]);
            return plain('_');
        }
        if !stairs && self.commands >= self.next_explore {
            // explore: travel to an unexplored place or a door (getpos 'x'
            // and 'd' cycle through them, 'X' from the farthest), then look
            // for the stairs again
            self.next_explore = self.commands + EXPLORE_EVERY;
            let target = self.rng.pick(&['x', 'X', 'X', 'd']);
            let hops = 1 + self.rng.below(2);
            self.plan.extend((0..hops).map(|_| plain(target)));
            self.plan.push_back(plain('.'));
            return plain('_');
        }
        let dir = self.rng.pick(dirs);
        match self.rng.below(200) {
            // hold a direction for a few steps
            0..=109 => {
                let steps = 1 + self.rng.below(6);
                self.plan.extend((1..steps).map(|_| plain(dir)));
                plain(dir)
            }
            110..=115 => plain(dir.to_ascii_uppercase()),
            116..=125 => {
                if self.rng.chance(40) {
                    // a count: "7s"
                    self.plan.push_back(plain('s'));
                    let d = self.rng.pick(&['3', '5', '7', '9']);
                    KeyInput {
                        mods: Mods {
                            alt: true,
                            ..Mods::default()
                        },
                        ..plain(d)
                    }
                } else {
                    plain('s')
                }
            }
            126..=133 => plain(','),
            134..=139 => plain(':'),
            140..=145 => plain('i'),
            146..=173 => plain(self.rng.pick(&OBJECT_COMMANDS)),
            174..=177 => ctrl('d'),
            178..=185 => plain(self.rng.pick(&['o', 'c'])),
            186..=193 => plain('#'),
            194 => {
                self.pray = true;
                plain('#')
            }
            // rest until HP and Pw are full (nothing when they are)
            195 => KeyInput::plain(Key::F(5)),
            _ => {
                if self.rng.chance(50) {
                    self.dir_answer = Some(dir);
                    plain('F')
                } else {
                    plain(self.rng.pick(&['D', 'A', '^']))
                }
            }
        }
    }

    /// A random valid way through a menu: pick (with a count sometimes),
    /// several picks and OK, everything, or cancel. The digging tool is
    /// picked only to dig.
    fn menu(&mut self, state: &MenuState, id: u64) -> Vec<UiEvent> {
        let dialog = |ev| UiEvent::Dialog { req: id, ev };
        let named = |k: Key| UiEvent::Key(KeyInput::plain(k));
        let is_tool = |e: &MenuEntry| DIG_TOOLS.iter().any(|t| e.text.contains(t));
        let tool = state
            .entries
            .iter()
            .position(|e| e.selectable && is_tool(e));
        // the inventory and the things to apply show the tool's letter
        if (self.inventory || self.digging)
            && let Some(i) = tool
        {
            self.can_dig = true;
            self.dig_letter = state.entries[i].letter.or(self.dig_letter);
        }
        if std::mem::take(&mut self.digging) {
            let Some(i) = tool else {
                self.can_dig = false;
                return vec![named(Key::Escape)];
            };
            let mut out = vec![match state.entries[i].letter {
                Some(c) if self.rng.chance(80) => UiEvent::Key(plain(c)),
                _ => dialog(DialogEvent::MenuClick(i)),
            }];
            if state.how == PickHow::Any {
                out.push(dialog(DialogEvent::MenuConfirm));
            }
            return out;
        }
        // "Auto-select every item" would drop or put away the tool too
        let keep = |e: &MenuEntry| self.can_dig && (is_tool(e) || e.text.contains("Auto-select"));
        let all: Vec<usize> = (0..state.entries.len())
            .filter(|&i| state.entries[i].selectable)
            .collect();
        let pickable: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&i| !keep(&state.entries[i]))
            .collect();
        // '.' never selects a skipinvert entry
        let bulk = all
            .iter()
            .all(|&i| !keep(&state.entries[i]) || state.entries[i].skipinvert);
        if pickable.is_empty() || self.rng.chance(15) {
            return vec![if self.rng.chance(50) {
                named(Key::Escape)
            } else {
                dialog(DialogEvent::MenuCancel)
            }];
        }
        let pick = |this: &mut Soak, out: &mut Vec<UiEvent>| {
            let i = this.rng.pick(&pickable);
            if this.rng.chance(10) {
                let n = this.rng.pick(&['1', '2', '3']);
                out.push(UiEvent::Key(plain(n)));
            }
            match state.entries[i].letter {
                Some(c) if this.rng.chance(80) => out.push(UiEvent::Key(plain(c))),
                _ => out.push(dialog(DialogEvent::MenuClick(i))),
            }
        };
        let mut out = Vec::new();
        match state.how {
            PickHow::None => out.push(named(Key::Enter)),
            PickHow::One => pick(self, &mut out),
            PickHow::Any => {
                if bulk && self.rng.chance(10) {
                    out.push(UiEvent::Key(plain('.')));
                } else {
                    for _ in 0..1 + self.rng.below(3) {
                        pick(self, &mut out);
                    }
                }
                out.push(if self.rng.chance(50) {
                    named(Key::Enter)
                } else {
                    dialog(DialogEvent::MenuConfirm)
                });
            }
        }
        out
    }
}

impl SelfTest {
    /// The scenario `--selftest=` names (None without one).
    pub fn new(args: &Args) -> Option<SelfTest> {
        let name = args.selftest.as_deref()?;
        watch_panics();
        let steps = match name {
            "tour" => tour(),
            "hud" => hud(),
            "moves" => moves(),
            "gallery" => gallery(),
            "smoke" => smoke(),
            "keys" => keys(),
            "save" => save(),
            "close" => close(),
            "crash" => crash(),
            "dialogs" => dialogs(),
            "menus" => menus(),
            "text" => text(),
            "soak" => soak(args),
            "orders" => orders(),
            "equipment" => hero::equipment(),
            "item-use" => hero::item_use(),
            "inventory" => inventory(),
            "bar" => bar(),
            _ => Vec::new(),
        };
        Some(SelfTest {
            name: name.to_string(),
            shots: args.screenshots.clone(),
            steps: steps.into(),
            step_started: Instant::now(),
            frames: 0,
            last_req: (0, 0),
            started: false,
            finished: false,
        })
    }

    fn pass(&mut self, game: &mut RenethackGame) {
        self.finished = true;
        godot_print!("SELFTEST PASS {}", self.name);
        game.quit(0);
    }

    fn fail(&mut self, game: &mut RenethackGame, why: &str) {
        self.finished = true;
        let recent: Vec<&str> = game
            .world
            .log
            .iter()
            .rev()
            .take(8)
            .map(|m| m.text.as_str())
            .collect();
        for m in recent.iter().rev() {
            godot_print!("selftest: log: {m}");
        }
        if let Some((id, p)) = &game.pending {
            godot_print!("selftest: pending request {id}: {}", brief(p));
        }
        for i in game.world.inventory.items() {
            godot_print!("selftest: pack: {} {} {}", i.letter, i.class, i.text);
        }
        godot_print!("SELFTEST FAIL {}: {why}", self.name);
        game.quit(1);
    }

    fn next(&mut self) {
        self.steps.pop_front();
        self.step_started = Instant::now();
        self.frames = 0;
    }

    /// One frame of the scenario (called at the end of `process()`).
    pub fn tick(&mut self, game: &mut RenethackGame) {
        if self.finished {
            return;
        }
        if !self.started {
            self.started = true;
            self.step_started = Instant::now();
            if self.steps.is_empty() {
                let why = format!("scenario {:?} is not implemented", self.name);
                return self.fail(game, &why);
            }
            let headless = DisplayServer::singleton().get_name() == "headless";
            if self.shots.is_some() && headless {
                return self.fail(
                    game,
                    "--screenshots needs a display: run without --headless (e.g. under xvfb-run)",
                );
            }
            if let Some(dir) = &self.shots
                && let Err(e) = std::fs::create_dir_all(dir)
            {
                let why = format!("cannot create {}: {e}", dir.display());
                return self.fail(game, &why);
            }
        }
        if let Some(p) = first_panic() {
            let why = format!("a panic: {p}");
            return self.fail(game, &why);
        }
        loop {
            let Some(step) = self.steps.front() else {
                return self.pass(game);
            };
            if let Step::Soak(_) = step {
                let Some(Step::Soak(mut soak)) = self.steps.pop_front() else {
                    unreachable!("the front step is the soak");
                };
                match soak.tick(game) {
                    Ok(false) => self.steps.push_front(Step::Soak(soak)),
                    Ok(true) => {
                        godot_print!("selftest: soak: {}", soak.summary());
                        godot_print!("selftest: soak: {} actions of orders", game.order_actions);
                        if let Some(ui) = game.ui.as_ref() {
                            let m = ui.map.motion_stats();
                            godot_print!("selftest: soak: animated {m:?}");
                        }
                        self.step_started = Instant::now();
                        self.frames = 0;
                        continue;
                    }
                    Err(why) => {
                        soak.report();
                        return self.fail(game, &why);
                    }
                }
                return;
            }
            if self.step_started.elapsed() > STEP_TIMEOUT {
                let why = format!("timed out waiting: {}", describe(step));
                return self.fail(game, &why);
            }
            let serial = game.session_serial;
            let pending = game.pending.as_ref().map(|(id, p)| (*id, p.clone()));
            let newer = |id: u64, last: (u64, u64)| (serial, id) > last;
            match step {
                Step::Wait(_, check) => match check(game) {
                    Ok(true) => self.next(),
                    Ok(false) => return,
                    Err(why) => {
                        let why = format!("{}: {why}", describe(step));
                        return self.fail(game, &why);
                    }
                },
                Step::Request(_, check) => match pending {
                    Some((id, p)) if newer(id, self.last_req) && check(&p) => self.next(),
                    _ => return,
                },
                Step::Push(ev) => {
                    game.push_ui(ev.clone());
                    self.next();
                    return;
                }
                Step::Key(k) => {
                    let Some((id, _)) = pending else {
                        return;
                    };
                    game.push_ui(UiEvent::Key(*k));
                    self.last_req = (serial, id);
                    self.next();
                    return;
                }
                Step::KeyFrom(what, f) => {
                    let Some((id, _)) = pending else {
                        return;
                    };
                    match f(game) {
                        Ok(k) => game.push_ui(UiEvent::Key(k)),
                        Err(why) => {
                            let why = format!("{what}: {why}");
                            return self.fail(game, &why);
                        }
                    }
                    self.last_req = (serial, id);
                    self.next();
                    return;
                }
                Step::Soak(_) => unreachable!("the soak runs before the match"),
                Step::Dialog(ev) => {
                    let Some((id, _)) = pending else {
                        return;
                    };
                    game.push_ui(UiEvent::Dialog {
                        req: id,
                        ev: ev.clone(),
                    });
                    self.last_req = (serial, id);
                    self.next();
                    return;
                }
                Step::Call(what, f) => {
                    if let Err(why) = f(game) {
                        let why = format!("{what}: {why}");
                        return self.fail(game, &why);
                    }
                    self.next();
                }
                Step::Inv(what, f) => {
                    let Some(ev) = f(game) else {
                        let why = format!("{what}: the item is not in the pack");
                        return self.fail(game, &why);
                    };
                    game.push_ui(UiEvent::Inventory(ev));
                    self.next();
                    return;
                }
                Step::Press(keycode, typed, shift) => {
                    let Some((id, _)) = pending else {
                        return;
                    };
                    real_key(*keycode, *typed, *shift);
                    self.last_req = (serial, id);
                    self.next();
                    return;
                }
                Step::Shot(name) => {
                    let Some(dir) = self.shots.clone() else {
                        self.next();
                        continue;
                    };
                    if self.frames < SHOT_FRAMES {
                        self.frames += 1;
                        return;
                    }
                    let path = dir.join(format!("{name}.png"));
                    if let Err(why) = save_shot(game, &path) {
                        return self.fail(game, &why);
                    }
                    godot_print!("selftest: screenshot {}", path.display());
                    self.next();
                }
                Step::AnswerUntil(c, _, check) => {
                    match check(game) {
                        Ok(true) => {
                            self.next();
                            continue;
                        }
                        Ok(false) => {}
                        Err(why) => {
                            let why = format!("{}: {why}", describe(step));
                            return self.fail(game, &why);
                        }
                    }
                    let Some((id, p)) = pending.filter(|(id, _)| newer(*id, self.last_req)) else {
                        return;
                    };
                    let ev = match p {
                        Prompt::Choice { .. } => UiEvent::Key(KeyInput::plain(Key::Char(*c))),
                        Prompt::Show { .. } | Prompt::MessageMenu { .. } => UiEvent::Dialog {
                            req: id,
                            ev: DialogEvent::Close,
                        },
                        Prompt::Menu { .. } => UiEvent::Dialog {
                            req: id,
                            ev: DialogEvent::MenuCancel,
                        },
                        other => {
                            let why = format!("unexpected request {other:?}");
                            return self.fail(game, &why);
                        }
                    };
                    game.push_ui(ev);
                    self.last_req = (serial, id);
                    return;
                }
            }
        }
    }

    /// The window was closed (or Quit pressed) during the scenario.
    pub fn on_close(&mut self, game: &mut RenethackGame) -> i32 {
        if self.finished {
            return 1;
        }
        self.finished = true;
        let saved = game
            .paths
            .as_ref()
            .is_some_and(|p| save_exists(&p.playground, "Hero"));
        let verdict = match (self.name.as_str(), saved) {
            ("close", true) => Ok(()),
            ("close", false) => Err("the window closed without a save"),
            _ => Err("the client closed early"),
        };
        match verdict {
            Ok(()) => {
                godot_print!("SELFTEST PASS {}", self.name);
                0
            }
            Err(why) => {
                godot_print!("SELFTEST FAIL {}: {why}", self.name);
                1
            }
        }
    }
}

fn describe(step: &Step) -> String {
    match step {
        Step::Wait(what, _) | Step::Request(what, _) | Step::AnswerUntil(_, what, _) => {
            what.to_string()
        }
        Step::Push(ev) => format!("{ev:?}"),
        Step::Key(k) => format!("a request for key {k:?}"),
        Step::KeyFrom(what, _) => format!("a request for {what}"),
        Step::Soak(_) => "the soak".to_string(),
        Step::Dialog(ev) => format!("a request for {ev:?}"),
        Step::Call(what, _) | Step::Inv(what, _) => what.to_string(),
        Step::Press(k, c, _) => format!("a request for key {k:?} {c:?}"),
        Step::Shot(name) => format!("screenshot {name}"),
    }
}

fn save_shot(game: &RenethackGame, path: &std::path::Path) -> Result<(), String> {
    // Forward+ may skip drawing a window it thinks nobody sees (macOS):
    // draw the state now, a few times for the temporal effects
    for _ in 0..8 {
        godot::classes::RenderingServer::singleton().force_draw();
    }
    let image = game
        .viewport_image()
        .ok_or("no viewport image (is there a renderer?)")?;
    let err = image.save_png(&path.to_string_lossy().to_string());
    if err == Error::OK {
        Ok(())
    } else {
        Err(format!("cannot save {}: {err:?}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soak_reads_the_letters_a_getobj_question_lists() {
        let l = |q: &str| listed_letters(q).into_iter().collect::<String>();
        assert_eq!(l("What do you want to wield? [- ab or ?*]"), "-ab");
        assert_eq!(l("What do you want to eat? [a-d or ?*]"), "abcd");
        assert_eq!(l("What do you want to throw? [$a-cf or ?*]"), "$abcf");
        assert_eq!(l("What do you want to read? [*]"), "*");
        assert_eq!(l("In what direction?"), "");
    }

    #[test]
    fn dust_smudges_only_a_few_letters() {
        assert!(smudged("Elbereth", "Elbereth"));
        assert!(smudged("Elbereth", "Elwereth"));
        assert!(smudged("Elbereth", "E!bere~h"));
        assert!(!smudged("Elbereth", "Elb?r?t?"));
        assert!(!smudged("Elbereth", "Elberet"));
        assert!(!smudged("Elbereth", "Elbe eth"));
        assert!(!smudged("Elbereth", "xxxxxxxx"));
    }

    #[test]
    fn soak_tells_a_death_from_a_quit() {
        let summary = |msgs: &[&str]| EndSummary {
            last_messages: msgs.iter().map(|m| m.to_string()).collect(),
            text: Vec::new(),
            scores: Vec::new(),
        };
        assert!(tells_death(&summary(&["You die..."])));
        assert!(tells_death(&summary(&["You drown."])));
        assert!(!tells_death(&summary(&["You quit.", "killed by quitting"])));
        assert!(!tells_death(&summary(&["Be seeing you..."])));
    }

    #[test]
    fn soak_randomness_depends_on_the_seed_only() {
        let run = |seed| {
            let mut r = Rng(seed);
            (0..8).map(|_| r.below(1000)).collect::<Vec<_>>()
        };
        assert_eq!(run(1), run(1));
        assert_ne!(run(1), run(2));
    }
}
