//! Headless self-tests: a scenario replaces only the source of input. It
//! queues the same `UiEvent`s the widgets and `input.rs` would, waits for
//! states with a timeout per step, and never dispatches into the game
//! synchronously. The verdict is one "SELFTEST PASS <name>" or
//! "SELFTEST FAIL <name>: <reason>" line and the exit code.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use godot::classes::{DisplayServer, Input, InputEventKey};
use godot::global::{Error, Key as GKey};
use godot::prelude::*;
use nh_world::{Key, KeyInput, Prompt, cell_terrain};

use nh_link::save_exists;

use crate::game::{GameState, RenethackGame};
use crate::ui_events::{CharacterChoice, DialogEvent, UiEvent};

const STEP_TIMEOUT: Duration = Duration::from_secs(30);
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
    /// A dialog event for the request now pending.
    Dialog(DialogEvent),
    Call(&'static str, fn(&mut RenethackGame) -> Result<(), String>),
    /// A real key event (keycode, character typed, Shift) through Godot's
    /// input buffer: delivered next frame to `input()` and the focused
    /// text field, never from inside the game's own call.
    Press(GKey, char, bool),
    Shot(&'static str),
    /// Answer each question with this key (text windows with OK) until the check holds.
    AnswerUntil(char, &'static str, Check),
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
        key('i'),
        Step::Request("the inventory menu", |p| matches!(p, Prompt::Menu { .. })),
        Step::Wait("five items in the menu", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.dialogs.open_req() != g.pending.as_ref().map(|(id, _)| *id) {
                return Ok(false);
            }
            let entries = ui.dialogs.menu_entries();
            let Some(entries) = entries else {
                return Ok(false);
            };
            let n = entries.iter().filter(|e| e.selectable).count();
            if n == 5 {
                Ok(true)
            } else {
                Err(format!("the inventory has {n} items, not 5"))
            }
        }),
        Step::Shot("inventory"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Request("a command after the menu", command),
        key('l'),
        Step::Request("a command after a step", command),
        key('l'),
        Step::Request("a command after two steps", command),
        Step::Shot("game"),
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
    vec![
        Step::Wait("the title screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("title"))
        }),
        Step::Push(UiEvent::StartCharacter(smoke_choice())),
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
        Step::Request("the inventory menu", |p| matches!(p, Prompt::Menu { .. })),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after Esc", command),
        press('l'),
        Step::Request("a command after a step", command),
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

impl SelfTest {
    pub fn new(name: &str, shots: Option<PathBuf>) -> SelfTest {
        let steps = match name {
            "smoke" => smoke(),
            "keys" => keys(),
            "save" => save(),
            "close" => close(),
            "crash" => crash(),
            _ => Vec::new(),
        };
        SelfTest {
            name: name.to_string(),
            shots,
            steps: steps.into(),
            step_started: Instant::now(),
            frames: 0,
            last_req: (0, 0),
            started: false,
            finished: false,
        }
    }

    fn pass(&mut self, game: &mut RenethackGame) {
        self.finished = true;
        godot_print!("SELFTEST PASS {}", self.name);
        game.quit(0);
    }

    fn fail(&mut self, game: &mut RenethackGame, why: &str) {
        self.finished = true;
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
        loop {
            let Some(step) = self.steps.front() else {
                return self.pass(game);
            };
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
                Step::Press(keycode, typed, shift) => {
                    let Some((id, _)) = pending else {
                        return;
                    };
                    let mut ev = InputEventKey::new_gd();
                    ev.set_keycode(*keycode);
                    ev.set_physical_keycode(*keycode);
                    ev.set_unicode(*typed as u32);
                    ev.set_shift_pressed(*shift);
                    ev.set_pressed(true);
                    Input::singleton().parse_input_event(&ev);
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
        Step::Dialog(ev) => format!("a request for {ev:?}"),
        Step::Call(what, _) => what.to_string(),
        Step::Press(k, c, _) => format!("a request for key {k:?} {c:?}"),
        Step::Shot(name) => format!("screenshot {name}"),
    }
}

fn save_shot(game: &RenethackGame, path: &std::path::Path) -> Result<(), String> {
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
