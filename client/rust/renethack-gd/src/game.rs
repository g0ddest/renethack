//! `RenethackGame`, the scene's root: runs the engine session, feeds the
//! world model, routes input to the map, the dialogs and the screens, and
//! owns the lifecycle (title, creation, play, save on close, end, errors).

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use godot::classes::notify::NodeNotification;
use godot::classes::{
    CanvasLayer, INode, InputEvent, InputEventKey, InputEventMouseButton, InputEventMouseMotion,
    Node3D, Os,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_link::{
    AnswerError, CLIENT_EXTRA_OPTIONS, Ending, EngineConfig, LiveSession, Recovered, SessionEvent,
    create_playground, fetch_catalog, interrupted_games, list_saves, recover_game, remember_name,
    save_exists,
};
use nh_protocol::{Catalog, Reply, WinCall};
use nh_world::{
    Key, KeyContext, KeyInput, Prompt, Typeahead, World, describe_cell, in_field, nethack_key,
};

use crate::dialogs::Dialogs;
use crate::hud::Hud;
use crate::input::key_input;
use crate::map_view::MapView;
use crate::paths::Paths;
use crate::screens::{DEFAULT_NAME, EndSummary, Screens};
use crate::selftest::SelfTest;
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, new_queue, push};

/// Time per frame spent reading the engine.
const POLL_BUDGET: Duration = Duration::from_millis(8);
/// The engine owes output and has said nothing this long: it hangs.
const HANG_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a closing window waits for the engine to save.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
/// Messages the end screen repeats.
const END_MESSAGES: usize = 12;
/// Self-tests are reproducible: this seed and clock unless the environment says otherwise.
const SELFTEST_SEED: u64 = 42;
const SELFTEST_TIME: i64 = 1_768_694_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Title,
    Creating,
    Playing,
    /// The window is closing: the engine saves, then the client quits.
    Closing,
    Ended,
    Failed,
}

/// Arguments after `--` on the Godot command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    pub selftest: Option<String>,
    pub screenshots: Option<PathBuf>,
    pub playground: Option<String>,
}

pub fn parse_args<S: AsRef<str>>(args: &[S]) -> Args {
    let mut out = Args::default();
    for a in args {
        let a = a.as_ref();
        if let Some(v) = a.strip_prefix("--selftest=") {
            out.selftest = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--screenshots=") {
            out.screenshots = Some(PathBuf::from(v));
        } else if let Some(v) = a.strip_prefix("--playground=") {
            out.playground = Some(v.to_string());
        } else {
            godot_warn!("unknown argument {a:?}");
        }
    }
    out
}

/// The widgets; plain structs holding their Godot nodes.
pub struct Ui {
    pub map: MapView,
    pub hud: Hud,
    pub dialogs: Dialogs,
    pub screens: Screens,
}

/// What the prompt line says while `prompt` waits.
fn prompt_line(prompt: &Prompt) -> Option<String> {
    match prompt {
        Prompt::Key => Some("Press a key".to_string()),
        Prompt::FreeKey { query, .. } => Some(query.clone()),
        Prompt::MapPause => Some("--More--  (any key)".to_string()),
        _ => None,
    }
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

#[derive(GodotClass)]
#[class(base=Node)]
pub struct RenethackGame {
    base: Base<Node>,
    pub(crate) state: GameState,
    session: Option<LiveSession>,
    pub(crate) world: World,
    pub(crate) catalog: Option<Rc<Catalog>>,
    /// The request waiting for the player and how the UI shows it.
    pub(crate) pending: Option<(u64, Prompt)>,
    typeahead: Typeahead,
    queue: UiQueue,
    pub(crate) ui: Option<Ui>,
    pub(crate) paths: Option<Paths>,
    /// Bumped by every engine start: request ids restart at 1.
    pub(crate) session_serial: u64,
    /// The hero's name, as typed or as saved.
    pub(crate) name: Option<String>,
    /// A reply could not be written: the engine is gone.
    link_error: Option<String>,
    close_deadline: Option<Instant>,
    quitting: bool,
    selftest: Option<SelfTest>,
}

#[godot_api]
impl INode for RenethackGame {
    fn init(base: Base<Node>) -> Self {
        RenethackGame {
            base,
            state: GameState::Title,
            session: None,
            world: World::new(),
            catalog: None,
            pending: None,
            typeahead: Typeahead::new(),
            queue: new_queue(),
            ui: None,
            paths: None,
            session_serial: 0,
            name: None,
            link_error: None,
            close_deadline: None,
            quitting: false,
            selftest: None,
        }
    }

    fn ready(&mut self) {
        let raw: Vec<String> = Os::singleton()
            .get_cmdline_user_args()
            .as_slice()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let args = parse_args(&raw);
        self.base().get_tree().set_auto_accept_quit(false);

        // layers are the game's own children, added through base_mut():
        // widgets get them and never the game node itself
        let mut map_root = Node3D::new_alloc();
        map_root.set_name("MapRoot");
        let mut layers = Vec::new();
        for (name, z) in [("HudLayer", 1), ("DialogLayer", 2), ("ScreenLayer", 3)] {
            let mut layer = CanvasLayer::new_alloc();
            layer.set_name(name);
            layer.set_layer(z);
            layers.push(layer);
        }
        self.base_mut().add_child(&map_root);
        for layer in &layers {
            self.base_mut().add_child(layer);
        }
        let queue = self.queue.clone();
        let mut layers = layers.into_iter();
        let mut next = || layers.next().expect("three layers");
        self.ui = Some(Ui {
            map: MapView::new(map_root),
            hud: Hud::new(next(), queue.clone()),
            dialogs: Dialogs::new(next(), queue.clone()),
            screens: Screens::new(next(), queue),
        });
        self.show_game(false);

        let paths = Paths::resolve(args.playground.as_deref());
        godot_print!(
            "renethack: engine {}, playground {}",
            paths.engine_dir.display(),
            paths.playground.display()
        );
        self.paths = Some(paths.clone());
        if let Some(name) = &args.selftest {
            self.selftest = Some(SelfTest::new(name, args.screenshots.clone()));
        }
        if let Err(e) = paths.check() {
            self.show_failure(
                "The game engine is not built.",
                &format!("{e}\nRun `make` in the repository, or set RENETHACK_ENGINE_DIR."),
                None,
            );
            return;
        }
        let notice = self.recover_interrupted();
        match fetch_catalog(&paths.engine(), &paths.data()) {
            Ok((_, catalog)) => self.catalog = Some(Rc::new(catalog)),
            Err(e) => {
                self.show_failure("The game engine does not start.", &e.to_string(), None);
                return;
            }
        }
        self.show_title(notice);
    }

    fn process(&mut self, delta: f64) {
        if self.ui.is_none() || self.quitting {
            return;
        }
        self.pump();
        self.sync_views(delta);
        self.drain_ui();
        self.watch_engine();
        if !self.quitting
            && let Some(mut test) = self.selftest.take()
        {
            test.tick(self);
            self.selftest = Some(test);
        }
    }

    /// Keys the open dialog or the map own; a text field gets everything
    /// else (only its `text_submitted` answers).
    fn input(&mut self, event: Gd<InputEvent>) {
        if self.state != GameState::Playing {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        let text = self.ui.as_ref().is_some_and(|ui| ui.dialogs.wants_text());
        let Some(k) = key_input(&key, text) else {
            return;
        };
        let owned = matches!(
            k.key,
            Key::Escape | Key::Tab | Key::Up | Key::Down | Key::PageUp | Key::PageDown
        );
        if text && !owned {
            return;
        }
        let ev = if k.key == Key::F(9) {
            UiEvent::ToggleFullLog
        } else {
            UiEvent::Key(k)
        };
        push(&self.queue, ev);
        self.handled();
    }

    /// The mouse over the map: whatever no panel or dialog took.
    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.state != GameState::Playing {
            return;
        }
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if ui.dialogs.is_open() {
            return;
        }
        if let Ok(m) = event.clone().try_cast::<InputEventMouseMotion>() {
            let pos = m.get_position();
            let cell = ui.map.cell_at(pos);
            ui.map.set_hover(cell);
            let text = match (cell, self.catalog.as_deref()) {
                (Some((x, y)), Some(cat)) => self
                    .world
                    .map
                    .cell(x, y)
                    .and_then(|c| describe_cell(c, cat)),
                _ => None,
            };
            ui.hud.set_tooltip(text.as_deref(), pos);
            return;
        }
        let Ok(b) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !b.is_pressed() {
            return;
        }
        let ev = match b.get_button_index() {
            MouseButton::WHEEL_UP => UiEvent::Zoom(-1.0),
            MouseButton::WHEEL_DOWN => UiEvent::Zoom(1.0),
            button @ (MouseButton::LEFT | MouseButton::RIGHT) => {
                let Some((x, y)) = ui.map.cell_at(b.get_position()) else {
                    return;
                };
                let button = if button == MouseButton::LEFT { 1 } else { 2 };
                UiEvent::MapClick { x, y, button }
            }
            _ => return,
        };
        push(&self.queue, ev);
        self.handled();
    }

    /// Only records: `process()` acts on it.
    fn on_notification(&mut self, what: NodeNotification) {
        if what == NodeNotification::WM_CLOSE_REQUEST {
            push(&self.queue, UiEvent::CloseRequested);
        }
    }
}

impl RenethackGame {
    fn handled(&mut self) {
        if let Some(mut vp) = self.base().get_viewport() {
            vp.set_input_as_handled();
        }
    }

    fn playground(&self) -> PathBuf {
        self.paths
            .as_ref()
            .map(|p| p.playground.clone())
            .unwrap_or_default()
    }

    fn ui_mut(&mut self) -> &mut Ui {
        self.ui.as_mut().expect("ready() built the UI")
    }

    /// Map and HUD on (playing) or off (screens).
    fn show_game(&mut self, on: bool) {
        let ui = self.ui_mut();
        ui.map.set_visible(on);
        ui.hud.set_visible(on);
        if !on {
            ui.dialogs.close();
        }
    }

    // ---- engine session ----

    fn pump(&mut self) {
        let deadline = Instant::now() + POLL_BUDGET;
        while self.pending.is_none() {
            let Some(session) = self.session.as_mut() else {
                break;
            };
            let events = session.poll(deadline);
            if events.is_empty() {
                break;
            }
            let mut frame = false;
            for ev in events {
                frame |= matches!(ev, SessionEvent::Win(WinCall::DelayOutput));
                self.on_session_event(ev);
            }
            // a delay_output is a frame boundary: let the effect be drawn
            if frame || Instant::now() >= deadline {
                break;
            }
        }
    }

    fn on_session_event(&mut self, ev: SessionEvent) {
        match ev {
            SessionEvent::Hello(h) => {
                godot_print!("renethack: engine {} (protocol {})", h.engine, h.protocol)
            }
            SessionEvent::Catalog(c) => {
                self.world.set_catalog(&c);
                self.catalog = Some(Rc::new(*c));
            }
            SessionEvent::Win(w) => self.world.apply(&w),
            SessionEvent::Request { id, req } => {
                let prompt = self.world.on_request(&req);
                if prompt == Prompt::AutoAck {
                    self.send(id, &prompt, Reply::Ack);
                } else {
                    self.open_prompt(id, prompt);
                }
            }
            SessionEvent::EngineError(e) => godot_warn!("renethack: engine error: {e}"),
            SessionEvent::Bye => {}
            SessionEvent::Failed(e) => godot_error!("renethack: session failed: {e}"),
            SessionEvent::Exited(ending) => self.on_exit(ending),
        }
    }

    fn open_prompt(&mut self, id: u64, prompt: Prompt) {
        let keeps = prompt.keeps_typeahead();
        if !keeps {
            self.typeahead.clear();
        }
        let catalog = self.catalog.clone();
        let ui = self.ui_mut();
        ui.dialogs.open(id, &prompt, catalog.as_deref());
        if ui.dialogs.is_open() {
            // the mouse belongs to the dialog now
            ui.hud.set_tooltip(None, Vector2::ZERO);
            ui.map.set_hover(None);
        }
        ui.hud.set_prompt_line(prompt_line(&prompt).as_deref());
        self.pending = Some((id, prompt));
        // keys typed while the engine was busy: the first one that answers
        while keeps && self.pending.is_some() {
            let Some(k) = self.typeahead.pop() else {
                break;
            };
            self.on_key(k);
        }
    }

    /// Answer the pending request.
    fn reply(&mut self, reply: Reply) {
        let Some((id, prompt)) = self.pending.take() else {
            godot_warn!("renethack: no request waits for {reply:?}");
            return;
        };
        let ui = self.ui_mut();
        ui.dialogs.close();
        ui.hud.set_prompt_line(None);
        self.send(id, &prompt, reply);
    }

    fn send(&mut self, id: u64, prompt: &Prompt, reply: Reply) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.answer(id, &reply) {
            Ok(()) => {
                if matches!(
                    prompt,
                    Prompt::Command | Prompt::Key | Prompt::FreeKey { .. } | Prompt::Choice { .. }
                ) {
                    self.world.note_player_input();
                }
            }
            // a stale event or a double click: harmless
            Err(AnswerError::NotPending(id)) => {
                godot_warn!("renethack: request {id} no longer waits; {reply:?} dropped")
            }
            Err(AnswerError::Link(e)) => {
                godot_error!("renethack: cannot answer the engine: {e}");
                self.link_error = Some(e.to_string());
                session.kill();
            }
        }
    }

    /// Hang or close timeout.
    fn watch_engine(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if self.state == GameState::Closing {
            if self.close_deadline.is_some_and(|d| Instant::now() >= d) {
                godot_warn!("renethack: the engine did not save in time; stopping it");
                session.kill();
                self.finish_close();
            }
            return;
        }
        if let Some(silent) = session.silent_for()
            && silent > HANG_TIMEOUT
        {
            godot_error!(
                "renethack: the engine hangs ({} s silent)",
                silent.as_secs()
            );
            self.link_error = Some(format!(
                "The engine stopped responding ({} s without output).",
                silent.as_secs()
            ));
            session.kill();
        }
    }

    fn on_exit(&mut self, ending: Ending) {
        godot_print!("renethack: engine exited: {ending:?}");
        self.session = None;
        self.pending = None;
        self.typeahead.clear();
        let ui = self.ui_mut();
        ui.dialogs.close();
        ui.hud.set_prompt_line(None);
        if self.state == GameState::Closing {
            self.finish_close();
            return;
        }
        let pg = self.playground();
        let name = self.name.clone();
        let saved = |name: &Option<String>| name.clone().filter(|n| save_exists(&pg, n));
        let link_error = self.link_error.take();
        let engine_error = ending
            .engine_error
            .clone()
            .filter(|_| !ending.hung_up_by_client);
        if let Some(what) = ending.failed.clone().or(link_error).or(engine_error) {
            let (notes, save) = match saved(&name) {
                Some(n) => (None, Some(n)),
                None => (self.recover_interrupted(), saved(&name)),
            };
            let details = [notes.unwrap_or_default(), ending.stderr_tail.clone()].join("\n\n");
            let mut text = format!("The game engine failed: {what}");
            if let Some(n) = &save {
                text.push_str(&format!("\nThe game is saved; you can continue it as {n}."));
            }
            self.show_failure(&text, details.trim(), save.as_deref());
        } else if let Some(n) = saved(&name) {
            self.show_title(Some(format!("Game saved: {n}.")));
        } else if ending.said_bye && ending.code == Some(0) {
            self.show_end();
        } else {
            let notes = self.recover_interrupted();
            let save = saved(&name);
            let code = ending.code.map_or("killed by a signal".to_string(), |c| {
                format!("exit code {c}")
            });
            let details = [notes.unwrap_or_default(), ending.stderr_tail.clone()].join("\n\n");
            self.show_failure(
                &format!("The game engine stopped unexpectedly ({code})."),
                details.trim(),
                save.as_deref(),
            );
        }
    }

    /// Run `recover` on every interrupted game; what happened, if anything.
    fn recover_interrupted(&mut self) -> Option<String> {
        let paths = self.paths.clone()?;
        let bases = match interrupted_games(&paths.playground) {
            Ok(b) => b,
            Err(e) => return Some(format!("Cannot look for interrupted games: {e}")),
        };
        let notes: Vec<String> = bases
            .iter()
            .map(
                |base| match recover_game(&paths.recover(), &paths.playground, base) {
                    Ok(Recovered::Saved(n)) => format!("Recovered an interrupted game: {n}."),
                    Ok(Recovered::Lost) => {
                        format!("An interrupted game ({base}) could not be recovered.")
                    }
                    Err(e) => format!("Recovering {base} failed: {e}"),
                },
            )
            .collect();
        for n in &notes {
            godot_print!("renethack: {n}");
        }
        (!notes.is_empty()).then(|| notes.join("\n"))
    }

    // ---- input ----

    fn on_key(&mut self, k: KeyInput) {
        if self.state != GameState::Playing {
            return;
        }
        if k.key == Key::F(9) {
            self.ui_mut().hud.toggle_full_log();
            return;
        }
        let Some((_, prompt)) = &self.pending else {
            self.typeahead.push(k);
            return;
        };
        let (np, dirs) = (self.world.number_pad, self.world.dirchars.clone());
        let code = |ctx| nethack_key(&k, ctx, np, &dirs);
        let reply = match prompt {
            Prompt::Command => code(KeyContext::Command).map(Reply::Key),
            Prompt::Key => code(KeyContext::Directions).map(Reply::Key),
            Prompt::FreeKey { directions, .. } => {
                let ctx = if *directions {
                    KeyContext::Directions
                } else {
                    KeyContext::Letters
                };
                code(ctx).map(Reply::Char)
            }
            Prompt::MapPause => (!k.echo).then_some(Reply::Ack),
            Prompt::AutoAck => Some(Reply::Ack),
            _ => self.ui_mut().dialogs.key(&k),
        };
        if let Some(r) = reply {
            self.reply(r);
        }
    }

    fn on_click(&mut self, x: i32, y: i32, button: i32) {
        match self.pending.as_ref().map(|(_, p)| p) {
            Some(Prompt::Command) => self.reply(Reply::Click {
                x,
                y,
                modifier: button,
            }),
            Some(Prompt::MapPause) => self.reply(Reply::Ack),
            _ => {}
        }
    }

    fn drain_ui(&mut self) {
        // signals are synchronous: never hold the borrow while handling
        let events: Vec<UiEvent> = self.queue.borrow_mut().drain(..).collect();
        for ev in events {
            if self.quitting {
                return;
            }
            self.on_ui_event(ev);
        }
    }

    fn on_ui_event(&mut self, ev: UiEvent) {
        let idle = self.session.is_none();
        match ev {
            UiEvent::Key(k) => self.on_key(k),
            UiEvent::MapClick { x, y, button } => {
                if self.state == GameState::Playing && in_field(x, y) {
                    self.on_click(x, y, button);
                }
            }
            UiEvent::Dialog { req, ev } => {
                if self.pending.as_ref().map(|(id, _)| *id) != Some(req) {
                    godot_warn!("renethack: dialog event for request {req} dropped: {ev:?}");
                    return;
                }
                if let Some(r) = self.ui_mut().dialogs.dialog_event(req, &ev) {
                    self.reply(r);
                }
            }
            UiEvent::NewGame if idle => self.show_creation(),
            UiEvent::ContinueGame(name) if idle => self.continue_game(&name),
            UiEvent::StartCharacter(choice) if idle => self.start_new(&choice),
            UiEvent::NameEdited(name) if idle => self.check_name(&name),
            UiEvent::BackToTitle if idle => self.show_title(None),
            UiEvent::QuitApp if idle => self.finish_close(),
            UiEvent::CloseRequested => self.on_close_requested(),
            UiEvent::ToggleFullLog => self.ui_mut().hud.toggle_full_log(),
            UiEvent::Zoom(steps) => self.ui_mut().map.zoom(steps),
            other => godot_warn!("renethack: {other:?} ignored while a game runs"),
        }
    }

    // ---- screens and lifecycle ----

    fn show_title(&mut self, notice: Option<String>) {
        self.state = GameState::Title;
        self.show_game(false);
        let saves = list_saves(&self.playground()).unwrap_or_else(|e| {
            godot_warn!("renethack: cannot list saves: {e}");
            Vec::new()
        });
        self.ui_mut().screens.show_title(&saves, notice.as_deref());
    }

    fn show_creation(&mut self) {
        let Some(catalog) = self.catalog.clone() else {
            self.show_failure("The game catalog is not available.", "", None);
            return;
        };
        self.state = GameState::Creating;
        self.show_game(false);
        self.ui_mut().screens.show_creation(catalog);
    }

    fn check_name(&mut self, name: &str) {
        let name = name.trim();
        let name = if name.is_empty() { DEFAULT_NAME } else { name };
        let taken = save_exists(&self.playground(), name);
        let error = EngineConfig::character_options(name, "random", "random", "random", "random")
            .err()
            .map(|e| e.to_string());
        let screens = &mut self.ui_mut().screens;
        screens.set_name_taken(taken);
        if !taken {
            screens.set_name_error(error.as_deref());
        }
    }

    fn show_end(&mut self) {
        let summary = EndSummary {
            last_messages: {
                let mut m: Vec<String> = self
                    .world
                    .log
                    .iter()
                    .rev()
                    .take(END_MESSAGES)
                    .map(|m| m.text.clone())
                    .collect();
                m.reverse();
                m
            },
            text: self
                .world
                .last_text
                .iter()
                .map(|l| l.text.clone())
                .collect(),
            scores: self.world.raw_lines.clone(),
        };
        self.state = GameState::Ended;
        self.show_game(false);
        self.ui_mut().screens.show_end(&summary);
    }

    fn show_failure(&mut self, what: &str, details: &str, can_continue: Option<&str>) {
        godot_error!("renethack: {what}");
        self.state = GameState::Failed;
        self.show_game(false);
        self.ui_mut()
            .screens
            .show_error(what, details, can_continue);
    }

    fn start_new(&mut self, choice: &CharacterChoice) {
        let pg = self.playground();
        if save_exists(&pg, &choice.name) {
            self.ui_mut().screens.set_name_taken(true);
            return;
        }
        let options = match EngineConfig::character_options(
            &choice.name,
            &choice.role,
            &choice.race,
            &choice.gender,
            &choice.align,
        ) {
            Ok(o) => o,
            Err(e) => {
                self.ui_mut().screens.set_name_error(Some(&e.to_string()));
                return;
            }
        };
        if !self.prepare_playground() {
            return;
        }
        if let Err(e) = remember_name(&pg, &choice.name) {
            godot_warn!("renethack: cannot remember the name: {e}");
        }
        self.start_session(options, &choice.name);
    }

    fn continue_game(&mut self, name: &str) {
        match EngineConfig::restore_options(name) {
            Ok(options) => {
                if self.prepare_playground() {
                    self.start_session(options, name);
                }
            }
            Err(e) => self.show_failure("This save cannot be restored.", &e.to_string(), None),
        }
    }

    fn prepare_playground(&mut self) -> bool {
        let Some(paths) = self.paths.clone() else {
            return false;
        };
        match create_playground(&paths.data(), &paths.playground) {
            Ok(()) => true,
            Err(e) => {
                self.show_failure(
                    "Cannot prepare the game directory.",
                    &format!("{}: {e}", paths.playground.display()),
                    None,
                );
                false
            }
        }
    }

    fn start_session(&mut self, options: String, name: &str) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        let test = self.selftest.is_some();
        let cfg = EngineConfig {
            engine: paths.engine(),
            playground: paths.playground.clone(),
            options: format!("{options},{CLIENT_EXTRA_OPTIONS}"),
            seed: env_number("RENETHACK_SEED").or(test.then_some(SELFTEST_SEED)),
            fixed_time: env_number("RENETHACK_FIXED_TIME").or(test.then_some(SELFTEST_TIME)),
        };
        match LiveSession::start(&cfg) {
            Ok(session) => {
                self.session = Some(session);
                self.session_serial += 1;
                self.world = World::new();
                if let Some(cat) = &self.catalog {
                    self.world.set_catalog(cat);
                }
                self.pending = None;
                self.typeahead.clear();
                self.link_error = None;
                self.name = Some(name.to_string());
                self.state = GameState::Playing;
                let ui = self.ui_mut();
                ui.screens.hide();
                ui.map.clear();
                ui.hud.reset();
                self.show_game(true);
            }
            Err(e) => self.show_failure("Cannot start the game engine.", &e.to_string(), None),
        }
    }

    fn on_close_requested(&mut self) {
        match (self.state, self.session.as_mut()) {
            (GameState::Playing, Some(session)) => {
                // the engine saves when its input closes
                session.hang_up();
                self.pending = None;
                self.typeahead.clear();
                self.state = GameState::Closing;
                self.close_deadline = Some(Instant::now() + CLOSE_TIMEOUT);
                let ui = self.ui_mut();
                ui.dialogs.close();
                ui.hud.set_prompt_line(Some("Saving the game..."));
            }
            (GameState::Closing, _) => {}
            _ => self.finish_close(),
        }
    }

    /// Leave the application (after the engine exited, or outside a game).
    pub(crate) fn finish_close(&mut self) {
        if let Some(mut s) = self.session.take() {
            s.kill();
        }
        let code = match self.selftest.take() {
            Some(mut test) => test.on_close(self),
            None => 0,
        };
        self.quit(code);
    }

    pub(crate) fn quit(&mut self, code: i32) {
        self.quitting = true;
        self.base().get_tree().quit_ex().exit_code(code).done();
    }

    pub(crate) fn push_ui(&self, ev: UiEvent) {
        push(&self.queue, ev);
    }

    pub(crate) fn viewport_image(&self) -> Option<Gd<godot::classes::Image>> {
        self.base().get_viewport()?.get_texture()?.get_image()
    }

    /// Fill the creation form (self-tests).
    pub(crate) fn preset_creation(&mut self, choice: &CharacterChoice) {
        self.ui_mut().screens.preset_creation(choice);
    }

    pub(crate) fn sync_views(&mut self, delta: f64) {
        if !matches!(self.state, GameState::Playing | GameState::Closing) {
            return;
        }
        let catalog = self.catalog.clone();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if let Some(cat) = catalog.as_deref() {
            ui.map.sync(&mut self.world, cat, delta);
        }
        ui.hud.sync(&mut self.world, catalog.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_after_the_double_dash() {
        let a = parse_args(&[
            "--selftest=smoke",
            "--playground=/tmp/pg",
            "--screenshots=/tmp/shots",
        ]);
        assert_eq!(a.selftest.as_deref(), Some("smoke"));
        assert_eq!(a.playground.as_deref(), Some("/tmp/pg"));
        assert_eq!(a.screenshots, Some(PathBuf::from("/tmp/shots")));
        assert_eq!(parse_args::<&str>(&[]), Args::default());
    }

    #[test]
    fn the_prompt_line_says_what_waits() {
        assert_eq!(prompt_line(&Prompt::Command), None);
        assert_eq!(prompt_line(&Prompt::Key).as_deref(), Some("Press a key"));
        let free = Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        };
        assert_eq!(prompt_line(&free).as_deref(), Some("In what direction?"));
        assert!(prompt_line(&Prompt::MapPause).unwrap().contains("More"));
    }
}
