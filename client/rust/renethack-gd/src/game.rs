//! `RenethackGame`, the scene's root: runs the engine session, feeds the
//! world model, routes input to the map, the dialogs and the screens, and
//! owns the lifecycle (title, creation, play, save on close, end, errors).

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use godot::classes::notify::NodeNotification;
use godot::classes::{
    CanvasLayer, INode, InputEvent, InputEventKey, InputEventMouseButton, InputEventMouseMotion,
    Node3D, Os, ProjectSettings,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_link::{
    AnswerError, CLIENT_EXTRA_OPTIONS, Ending, EngineConfig, LiveSession, PlaygroundLock,
    Recovered, SessionEvent, create_playground, fetch_catalog, interrupted_games, list_saves,
    lock_playground, recover_game, remember_name, save_exists,
};
use nh_protocol::{Catalog, Reply, WinCall};
use nh_world::{
    Action, Cell, ClickPlan, CountEntry, Counted, Key, KeyContext, KeyInput, Mode, Order, Prompt,
    Stop, TickDriver, Typeahead, World, click_order, describe_cell, find_path, in_field,
    nethack_key, repeatable, stairs_order,
};

use crate::dialogs::Dialogs;
use crate::hud::Hud;
use crate::input::{client_key, key_input, key_release};
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
/// Project setting with the order tick in milliseconds (150–500).
const TICK_SETTING: &str = "renethack/tick_ms";
/// The soak self-test's tick, unless it is given one.
const SOAK_TICK_MS: u64 = 20;
/// How long the HUD tells why an order stopped.
const STOP_NOTE: Duration = Duration::from_millis(2500);
/// Self-tests are reproducible: this seed and clock unless the environment says otherwise.
pub(crate) const SELFTEST_SEED: u64 = 42;
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
    /// The engine's seed (overrides RENETHACK_SEED).
    pub seed: Option<u64>,
    /// The soak self-test's budget of answered requests.
    pub soak: Option<u32>,
    /// The order tick in milliseconds, any value (self-tests).
    pub tick: Option<u64>,
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
        } else if let Some(v) = a.strip_prefix("--seed=")
            && let Ok(n) = v.trim().parse()
        {
            out.seed = Some(n);
        } else if let Some(v) = a.strip_prefix("--soak=")
            && let Ok(n) = v.trim().parse()
        {
            out.soak = Some(n);
        } else if let Some(v) = a.strip_prefix("--tick=")
            && let Ok(n) = v.trim().parse()
        {
            out.tick = Some(n);
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

/// A held key as its release names it (a letter's case may differ).
fn held_key(k: Key) -> Key {
    match k {
        Key::Char(c) => Key::Char(c.to_ascii_lowercase()),
        other => other,
    }
}

/// What the HUD says an order does.
fn order_text(order: &Order) -> String {
    use nh_world::Arrival;
    let what = match order {
        Order::Walk { arrival, .. } => match arrival {
            Arrival::None => "Walking".to_string(),
            Arrival::PickUp => "Walking to pick up".to_string(),
            Arrival::Stairs('<') => "Going to the stairs up".to_string(),
            Arrival::Stairs(_) => "Going to the stairs down".to_string(),
            Arrival::Open => "Going to open the door".to_string(),
            Arrival::Attack => "Going to attack".to_string(),
        },
        Order::Repeat { key: 's', left } => format!("Searching, {left} more"),
        Order::Repeat { key: '.', left } => format!("Waiting, {left} more"),
        Order::Repeat { left, .. } => format!("Walking, {left} more steps"),
        Order::Hold { .. } => "Holding the key".to_string(),
        Order::Rest => "Resting until HP and Pw are full".to_string(),
    };
    format!("{what}  -  any key stops")
}

pub(crate) fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
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
    /// The next engine's seed; else RENETHACK_SEED (else the self-test's).
    pub(crate) seed: Option<u64>,
    /// Things that went wrong without stopping the client (engine errors,
    /// protocol failures, dead links, double answers): self-tests fail on them.
    pub(crate) faults: Vec<String>,
    /// What the error screen says.
    pub(crate) failure: Option<String>,
    /// Where the mouse last moved inside the window; `None` before it
    /// first moves there and after it leaves.
    mouse_pos: Option<Vector2>,
    hover: Hover,
    /// This client's hold on the playground; `None` while another client
    /// uses it (then nothing here recovers, lists or starts games).
    playground_lock: Option<PlaygroundLock>,
    /// Orders, the tick and the mode (exploring or fighting).
    pub(crate) driver: TickDriver,
    /// When the last action of an order went out.
    last_action: Option<Instant>,
    /// A count typed before a command.
    count: CountEntry,
    /// The key held down for a `Hold` order.
    held: Option<Key>,
    /// Why the last order stopped, shown until then.
    stop_note: Option<(String, Instant)>,
    /// What the way preview was drawn for.
    preview_key: Option<PreviewKey>,
    /// The last actions orders sent (self-tests: when and what), and how
    /// many were sent in all.
    pub(crate) order_log: std::collections::VecDeque<(Instant, char)>,
    pub(crate) order_actions: u64,
    /// Why the last order ended (self-tests).
    pub(crate) last_stop: Option<Stop>,
    /// A cell the self-test hovers instead of the mouse.
    pub(crate) test_hover: Option<(i32, i32)>,
}

/// The way preview is drawn again only when one of these changes.
#[derive(Debug, Clone, PartialEq)]
struct PreviewKey {
    hover: Option<(i32, i32)>,
    request: Option<u64>,
    order: Option<Order>,
    hero: Option<(i32, i32)>,
    generation: u64,
    mode: Mode,
}

/// What the hover marker and the tooltip show now: a still mouse over an
/// unchanged cell costs no Godot call and no new text.
#[derive(Default)]
struct Hover {
    cell: Option<(i32, i32)>,
    /// The cell as the tooltip describes it; a new look redoes the text.
    look: Option<Cell>,
    text: Option<String>,
    pos: Vector2,
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
            mouse_pos: None,
            hover: Hover::default(),
            seed: None,
            faults: Vec::new(),
            failure: None,
            playground_lock: None,
            driver: TickDriver::new(),
            last_action: None,
            count: CountEntry::default(),
            held: None,
            stop_note: None,
            preview_key: None,
            order_log: std::collections::VecDeque::new(),
            order_actions: 0,
            last_stop: None,
            test_hover: None,
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
        self.seed = args.seed;
        // the soak plays many orders: quickly
        let soak = args.selftest.as_deref() == Some("soak");
        self.set_tick(args.tick.or(soak.then_some(SOAK_TICK_MS)));
        if args.selftest.is_some() {
            self.selftest = SelfTest::new(&args);
        }
        if let Err(e) = paths.check() {
            self.show_failure(
                "The game engine is not built.",
                &format!("{e}\nRun `make` in the repository, or set RENETHACK_ENGINE_DIR."),
                None,
            );
            return;
        }
        self.start_up();
    }

    fn process(&mut self, delta: f64) {
        if self.ui.is_none() || self.quitting {
            return;
        }
        // input first: Godot delivered this frame's keys before process(), so
        // they belong to the prompt the player saw, or to the typeahead while
        // the engine works; a request read afterwards never gets them (the
        // typeahead policy drops them when it opens a modal question)
        self.drain_ui();
        self.pump();
        self.drive();
        self.sync_views(delta);
        if let Some(ui) = self.ui.as_mut() {
            ui.map.preload_step();
        }
        self.watch_engine();
        if !self.quitting
            && let Some(mut test) = self.selftest.take()
        {
            // a panic in the scenario is caught here so the test survives to
            // report it (its panic hook recorded it) instead of hanging
            let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test.tick(self)));
            if ticked.is_err() {
                godot_error!("renethack: the self-test panicked");
            }
            self.selftest = Some(test);
        }
    }

    /// Keys the open dialog or the map own; a text field gets everything
    /// else (only its `text_submitted` answers).
    /// Mouse motion is only recorded here, never handled: the hover needs
    /// the position over the HUD panels too, which `unhandled_input` never
    /// sees.
    fn input(&mut self, event: Gd<InputEvent>) {
        let event = match event.try_cast::<InputEventMouseMotion>() {
            Ok(motion) => {
                self.record_mouse(motion.get_position());
                return;
            }
            Err(event) => event,
        };
        if self.state != GameState::Playing {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        let text = self.ui.as_ref().is_some_and(|ui| ui.dialogs.wants_text());
        if !key.is_pressed() {
            // letting go ends holding it down (text fields keep theirs)
            if !text && let Some(k) = key_release(&key) {
                push(&self.queue, UiEvent::KeyUp(k));
            }
            return;
        }
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
        push(&self.queue, client_key(&k).unwrap_or(UiEvent::Key(k)));
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
        // hovering follows the recorded mouse position (update_hover)
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
        match what {
            NodeNotification::WM_CLOSE_REQUEST => push(&self.queue, UiEvent::CloseRequested),
            // held keys are let go without a release event
            NodeNotification::WM_WINDOW_FOCUS_OUT | NodeNotification::APPLICATION_FOCUS_OUT => {
                push(&self.queue, UiEvent::FocusLost)
            }
            // entering is followed by motion, which gives the position
            NodeNotification::WM_MOUSE_EXIT => self.mouse_pos = None,
            _ => {}
        }
    }
}

impl RenethackGame {
    /// A motion outside the window (a drag holds the pointer) hovers nothing.
    fn record_mouse(&mut self, pos: Vector2) {
        let inside = self
            .base()
            .get_viewport()
            .is_some_and(|vp| vp.get_visible_rect().contains_point(pos));
        self.mouse_pos = inside.then_some(pos);
    }

    /// Keep the first few faults for a self-test to fail on.
    fn fault(&mut self, what: String) {
        const KEEP: usize = 16;
        if self.faults.len() < KEEP {
            self.faults.push(what);
        }
    }

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
        self.clear_hover();
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
            SessionEvent::EngineError(e) => {
                godot_warn!("renethack: engine error: {e}");
                self.fault(format!("engine error: {e}"));
            }
            SessionEvent::Bye => {}
            SessionEvent::Failed(e) => {
                godot_error!("renethack: session failed: {e}");
                self.fault(format!("session failed: {e}"));
            }
            SessionEvent::Exited(ending) => self.on_exit(ending),
        }
    }

    fn open_prompt(&mut self, id: u64, prompt: Prompt) {
        // the next key of an order's action (the direction after F or o):
        // part of the decision already made, answered at once
        if self.driver.expects_follow_up()
            && let Some(c) = self.driver.follow_up(&prompt)
        {
            let reply = match prompt {
                Prompt::Command => Reply::Key(c as i32),
                _ => Reply::Char(c as i32),
            };
            self.pending = Some((id, prompt));
            self.log_action(c);
            self.reply(reply);
            return;
        }
        if let Some(cat) = self.catalog.clone() {
            let query = match &prompt {
                Prompt::Choice { query, .. } | Prompt::FreeKey { query, .. } => {
                    Some(query.as_str())
                }
                _ => None,
            };
            if prompt == Prompt::Command && !self.world.getpos {
                // the tick's moment: the mode, and whether the order goes on
                if let Some(mode) = self.driver.observe(&self.world, &cat) {
                    self.ui_mut().hud.set_mode(mode == Mode::Combat, true);
                }
            } else if self.driver.is_active() {
                // a question the order did not expect: the player answers
                self.driver.on_question(query, &cat);
            } else if let Some(q) = query {
                self.driver.note_query(q, &cat);
            }
        }
        let keeps = prompt.keeps_typeahead();
        if !keeps {
            self.typeahead.clear();
        }
        let catalog = self.catalog.clone();
        let line = match prompt {
            Prompt::Command => self.world.getpos_line(),
            _ => prompt_line(&prompt),
        };
        let ui = self.ui_mut();
        ui.dialogs.open(id, &prompt, catalog.as_deref());
        let dialog = ui.dialogs.is_open();
        ui.hud.set_prompt_line(line.as_deref());
        if dialog {
            // the mouse belongs to the dialog now
            self.clear_hover();
        }
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
                godot_warn!("renethack: request {id} no longer waits; {reply:?} dropped");
                self.fault(format!("request {id} answered twice: {reply:?}"));
            }
            Err(AnswerError::Link(e)) => {
                godot_error!("renethack: cannot answer the engine: {e}");
                self.link_error = Some(e.to_string());
                session.kill();
                self.fault(format!("link error: {e}"));
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
        self.reset_orders();
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

    /// Take the playground, recover interrupted games, read the catalog and
    /// show the title. Again from the error screen's "Title" when another
    /// client held the playground.
    fn start_up(&mut self) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        match lock_playground(&paths.playground) {
            Ok(Some(lock)) => self.playground_lock = Some(lock),
            Ok(None) => {
                self.show_failure(
                    "renethack is already running with this game directory.",
                    &format!(
                        "{}\nClose the other window first, then press Title.",
                        paths.playground.display()
                    ),
                    None,
                );
                return;
            }
            Err(e) => {
                self.show_failure(
                    "Cannot prepare the game directory.",
                    &format!("{}: {e}", paths.playground.display()),
                    None,
                );
                return;
            }
        }
        let notice = self.recover_interrupted();
        if self.catalog.is_none() {
            match fetch_catalog(&paths.engine(), &paths.data()) {
                Ok((_, catalog)) => self.catalog = Some(Rc::new(catalog)),
                Err(e) => {
                    self.show_failure("The game engine does not start.", &e.to_string(), None);
                    return;
                }
            }
        }
        self.show_title(notice);
    }

    /// Run `recover` on every interrupted game; what happened, if anything.
    fn recover_interrupted(&mut self) -> Option<String> {
        // another client's playground is not ours to repair
        self.playground_lock.as_ref()?;
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
        // typed ahead or pushed by a self-test as a plain key
        if let Some(ev) = client_key(&k) {
            self.on_ui_event(ev);
            return;
        }
        if !k.echo && self.held == Some(held_key(k.key)) {
            // pressed anew: it was let go (its release went unseen)
            self.held = None;
        }
        if self.driver.is_active() {
            // any key stops an order before its next step; a key's repeat
            // neither stops one nor steps (a held key's order does)
            if k.echo {
                return;
            }
            self.driver.interrupt(Stop::Key);
            if k.key == Key::Escape {
                return;
            }
        }
        let Some((_, prompt)) = &self.pending else {
            self.typeahead.push(k);
            return;
        };
        if *prompt == Prompt::Command && !self.world.getpos {
            self.command_key(k);
            return;
        }
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

    /// A key at a command prompt (not getpos): a count, an order, or the
    /// engine's command.
    fn command_key(&mut self, k: KeyInput) {
        let (np, dirs) = (self.world.number_pad, self.world.dirchars.clone());
        let Some(code) = nethack_key(&k, KeyContext::Command, np, &dirs) else {
            return;
        };
        let c = u32::try_from(code).ok().and_then(char::from_u32);
        if k.echo {
            // held down: a Hold order repeats it on the tick, the key's own
            // repeat never steps; once stopped it waits for a new press
            if self.held.is_none()
                && self.count.shown().is_none()
                && let Some(c) = c.filter(|c| repeatable(*c, &dirs))
            {
                self.held = Some(held_key(k.key));
                self.start_order(Order::Hold { key: c });
            }
            return;
        }
        match self.count.feed(code, np) {
            Counted::Typing => {
                let line = self.count.shown();
                self.ui_mut().hud.set_prompt_line(line.as_deref());
            }
            Counted::Command(n) => {
                if n.is_some() {
                    self.ui_mut().hud.set_prompt_line(None);
                }
                if let (Some(n), Some(c)) = (n, c)
                    && repeatable(c, &dirs)
                {
                    self.start_order(Order::Repeat { key: c, left: n });
                    return;
                }
                // '<' or '>' away from the stairs: walk there and use them
                if let Some(c @ ('<' | '>')) = c
                    && let Some(cat) = self.catalog.clone()
                    && let Some(order) = stairs_order(&self.world, &cat, c)
                    && self.start_order(order)
                {
                    return;
                }
                self.reply(Reply::Key(code));
            }
        }
    }

    /// A left click walks (and acts at the end of the way); a right click
    /// asks the engine for the cell's actions (#therecmdmenu). In getpos a
    /// click picks the cell.
    fn on_click(&mut self, x: i32, y: i32, button: i32) {
        // a click stops the order; a left click gives the next one
        self.driver.interrupt(Stop::Click);
        match self.pending.as_ref().map(|(_, p)| p) {
            Some(Prompt::Command) if self.world.getpos => self.reply(Reply::Click {
                x,
                y,
                modifier: button,
            }),
            Some(Prompt::Command) => {
                if self.count.shown().is_some() {
                    self.count.clear();
                    self.ui_mut().hud.set_prompt_line(None);
                }
                let plan = match (button, self.catalog.clone()) {
                    (1, Some(cat)) => click_order(&self.world, &cat, (x, y)),
                    _ => ClickPlan::Engine,
                };
                match plan {
                    ClickPlan::Order(order) => {
                        self.start_order(order);
                    }
                    // CLICK_1 is the engine's #therecmdmenu
                    ClickPlan::Engine => self.reply(Reply::Click { x, y, modifier: 1 }),
                    ClickPlan::Nothing(why) => {
                        self.stop_note =
                            Some((format!("Nothing to do there ({why})"), Instant::now()));
                    }
                }
            }
            Some(Prompt::MapPause) => self.reply(Reply::Ack),
            _ => {}
        }
    }

    /// Start an order at the command prompt: its first action goes now.
    fn start_order(&mut self, order: Order) -> bool {
        let Some(cat) = self.catalog.clone() else {
            return false;
        };
        match self.driver.start(order, &self.world, &cat) {
            Ok(action) => {
                self.send_action(action);
                true
            }
            Err(stop) => {
                self.note_stop(&stop);
                self.last_stop = Some(stop);
                false
            }
        }
    }

    /// Answer the command prompt with an order's action.
    fn send_action(&mut self, action: Action) {
        let Some(&c) = action.keys.first() else {
            return;
        };
        self.last_action = Some(Instant::now());
        self.log_action(c);
        // the step this action brings lasts the tick, the first one too
        let tick = self.driver.tick().as_secs_f32();
        self.ui_mut().map.set_order_pace(Some(tick));
        self.reply(Reply::Key(c as i32));
    }

    /// Keep the last actions for self-tests.
    fn log_action(&mut self, c: char) {
        const KEEP: usize = 256;
        if self.order_log.len() == KEEP {
            self.order_log.pop_front();
        }
        self.order_log.push_back((Instant::now(), c));
        self.order_actions += 1;
    }

    /// Why an order ended, for the HUD (not when it simply finished).
    fn note_stop(&mut self, stop: &Stop) {
        if !stop.is_completion() {
            self.stop_note = Some((stop.says(), Instant::now()));
        }
    }

    /// The order's next action, when a command prompt waits, the tick has
    /// passed and the hero's last step has been played.
    fn drive(&mut self) {
        if !self.driver.is_active() || self.state != GameState::Playing {
            return;
        }
        if !matches!(self.pending, Some((_, Prompt::Command))) || self.world.getpos {
            return;
        }
        let since = self.last_action.map_or(Duration::MAX, |t| t.elapsed());
        let walking = self.ui.as_ref().is_some_and(|ui| ui.map.hero_walking());
        if !self.driver.ready(since, walking) {
            return;
        }
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        if let Some(action) = self.driver.next(&self.world, &cat) {
            self.send_action(action);
        }
    }

    /// A key let go: the order holding it ends.
    fn on_key_up(&mut self, k: KeyInput) {
        if self.held == Some(held_key(k.key)) {
            self.held = None;
            if matches!(self.driver.order(), Some(Order::Hold { .. })) {
                self.driver.interrupt(Stop::Released);
            }
        }
    }

    /// Rest until HP and Pw are full (F5), at a command prompt.
    fn rest(&mut self) {
        self.driver.interrupt(Stop::Key);
        if matches!(self.pending, Some((_, Prompt::Command))) && !self.world.getpos {
            self.start_order(Order::Rest);
        }
    }

    /// The tick: the project setting (150–500 ms), RENETHACK_TICK_MS, or a
    /// self-test's `--tick=` (any value).
    fn set_tick(&mut self, test: Option<u64>) {
        if let Some(ms) = test {
            self.driver.set_test_tick(Duration::from_millis(ms));
            return;
        }
        let settings = ProjectSettings::singleton();
        let setting = settings
            .has_setting(TICK_SETTING)
            .then(|| settings.get_setting(TICK_SETTING).try_to::<i64>().ok())
            .flatten()
            .and_then(|v| u64::try_from(v).ok());
        let ms = env_number::<u64>("RENETHACK_TICK_MS")
            .or(setting)
            .unwrap_or(nh_world::DEFAULT_TICK_MS);
        self.driver.set_tick_ms(ms);
    }

    /// Forget orders, counts and notes (a new game, the end of one).
    fn reset_orders(&mut self) {
        self.driver.reset();
        self.held = None;
        self.count.clear();
        self.stop_note = None;
        self.preview_key = None;
        self.last_action = None;
    }

    /// The HUD's mode badge and order line, the hero's pace.
    fn sync_orders(&mut self) {
        if let Some(stop) = self.driver.take_stop() {
            self.note_stop(&stop);
            self.last_stop = Some(stop);
        }
        if self.driver.is_active() {
            self.stop_note = None;
        }
        let note = self
            .stop_note
            .as_ref()
            .filter(|(_, at)| at.elapsed() < STOP_NOTE)
            .map(|(t, _)| t.clone());
        let line = match self.driver.order() {
            Some(o) => Some(order_text(o)),
            None => note,
        };
        let combat = self.driver.mode() == Mode::Combat;
        let pace = self
            .driver
            .is_active()
            .then(|| self.driver.tick().as_secs_f32());
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.hud.set_mode(combat, false);
        ui.hud.set_order_line(line.as_deref());
        ui.map.set_order_pace(pace);
    }

    /// The way a click would walk (under the mouse), or the way the order
    /// walks; drawn again only when something it depends on changed.
    fn update_preview(&mut self) {
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        let command = matches!(self.pending, Some((_, Prompt::Command))) && !self.world.getpos;
        let dialog = self.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open());
        let key = PreviewKey {
            hover: self.hover.cell.filter(|_| command && !dialog),
            request: self.pending.as_ref().map(|(id, _)| *id),
            order: self.driver.order().cloned(),
            hero: self.world.hero(),
            generation: self.world.map.generation(),
            mode: self.driver.mode(),
        };
        if self.preview_key.as_ref() == Some(&key) {
            return;
        }
        let walk = match (&key.order, key.hover) {
            (Some(Order::Walk { goal, arrival }), _) => Some((*goal, *arrival)),
            (Some(_), _) => None,
            (None, Some(at)) => match click_order(&self.world, &cat, at) {
                ClickPlan::Order(Order::Walk { goal, arrival }) => Some((goal, arrival)),
                _ => None,
            },
            (None, None) => None,
        };
        let path = walk
            .zip(key.hero)
            .and_then(|((goal, arrival), hero)| {
                let mut way = find_path(&self.world.map, &cat, hero, goal, arrival.reach())?;
                // the ring on what the walk is for: the door, the monster
                if arrival.reach() != nh_world::Reach::Onto {
                    way.push(goal);
                }
                Some(way)
            })
            .unwrap_or_default();
        let combat = key.mode == Mode::Combat;
        self.preview_key = Some(key);
        if let Some(ui) = self.ui.as_mut() {
            ui.map.set_path(&path, combat);
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
            UiEvent::BackToTitle if idle && self.playground_lock.is_none() => self.start_up(),
            UiEvent::BackToTitle if idle => self.show_title(None),
            UiEvent::QuitApp if idle => self.finish_close(),
            UiEvent::CloseRequested => self.on_close_requested(),
            UiEvent::ToggleFullLog => {
                self.driver.interrupt(Stop::Panel);
                self.ui_mut().hud.toggle_full_log();
            }
            UiEvent::KeyUp(k) => self.on_key_up(k),
            UiEvent::FocusLost => {
                self.held = None;
                self.driver.interrupt(Stop::Focus);
            }
            UiEvent::Rest => self.rest(),
            UiEvent::Zoom(steps) => self.ui_mut().map.zoom(steps),
            UiEvent::ToggleOverview => self.ui_mut().map.toggle_overview(),
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
        self.failure = Some(format!("{what}\n{details}").trim().to_string());
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
            seed: self
                .seed
                .or_else(|| env_number("RENETHACK_SEED"))
                .or(test.then_some(SELFTEST_SEED)),
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
                self.reset_orders();
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
                self.driver.interrupt(Stop::Reset);
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
        // ^P: tty shows the previous messages; here the whole log
        if std::mem::take(&mut self.world.wants_history) {
            ui.hud.open_full_log();
        }
        ui.hud.sync(&mut self.world, catalog.as_deref());
        self.update_hover();
        self.sync_orders();
        self.update_preview();
    }

    /// Hover marker and tooltip for the cell under the mouse, checked every
    /// frame: they follow the camera and a cell that changes under a still
    /// mouse, and go away when the mouse leaves the map or the window, or is
    /// over a HUD panel or a dialog. Godot is told only about changes.
    fn update_hover(&mut self) {
        let playing = self.state == GameState::Playing;
        let catalog = self.catalog.clone();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        let pos = self
            .mouse_pos
            .filter(|&p| playing && !ui.dialogs.is_open() && !ui.hud.covers(p));
        let cell = match self.test_hover {
            Some(c) if playing && !ui.dialogs.is_open() => Some(c),
            _ => pos.and_then(|p| ui.map.cell_at(p)),
        };
        if cell != self.hover.cell {
            self.hover.cell = cell;
            ui.map.set_hover(cell);
        }
        // a self-test's hovered cell has no mouse to put a tooltip at
        let look = cell
            .filter(|_| self.test_hover.is_none())
            .and_then(|(x, y)| self.world.map.cell(x, y));
        let pos = pos.unwrap_or(Vector2::ZERO);
        if look != self.hover.look.as_ref() {
            self.hover.look = look.cloned();
            self.hover.text = look
                .zip(catalog.as_deref())
                .and_then(|(c, cat)| describe_cell(c, cat));
        } else if self.hover.text.is_none() || pos == self.hover.pos {
            return;
        }
        self.hover.pos = pos;
        ui.hud.set_tooltip(self.hover.text.as_deref(), pos);
    }

    /// No hover marker and no tooltip, and nothing remembered about them.
    fn clear_hover(&mut self) {
        self.hover = Hover::default();
        if let Some(ui) = self.ui.as_mut() {
            ui.hud.set_tooltip(None, Vector2::ZERO);
            ui.map.set_hover(None);
        }
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
        let a = parse_args(&["--selftest=soak", "--soak=300", "--seed=7"]);
        assert_eq!((a.soak, a.seed), (Some(300), Some(7)));
    }

    #[test]
    fn orders_say_what_they_do() {
        let walk = Order::Walk {
            goal: (5, 5),
            arrival: nh_world::Arrival::Stairs('>'),
        };
        assert!(order_text(&walk).starts_with("Going to the stairs down"));
        let search = Order::Repeat { key: 's', left: 4 };
        assert!(order_text(&search).starts_with("Searching, 4 more"));
        assert!(order_text(&Order::Rest).contains("any key stops"));
        assert_eq!(held_key(Key::Char('L')), Key::Char('l'));
        assert_eq!(held_key(Key::Left), Key::Left);
        let a = parse_args(&["--tick=20"]);
        assert_eq!(a.tick, Some(20));
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
