//! `RenethackGame`, the scene's root: runs the engine session, feeds the
//! world model, routes input to the map, the dialogs and the screens, and
//! owns the lifecycle (title, creation, play, save on close, end, errors).

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use godot::classes::notify::NodeNotification;
use godot::classes::{
    CanvasLayer, INode, Input, InputEvent, InputEventJoypadButton, InputEventJoypadMotion,
    InputEventKey, InputEventMouseButton, InputEventMouseMotion, Node3D, Os, ProjectSettings, Time,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_link::{
    AnswerError, CLIENT_EXTRA_OPTIONS, Ending, EngineConfig, LiveSession, PlaygroundLock,
    Recovered, SessionEvent, create_playground, fetch_catalog, interrupted_games, list_saves,
    lock_playground, read_ui_state, recover_game, remember_name, remove_ui_state, save_exists,
    write_ui_state,
};
use nh_protocol::{Catalog, PickHow, Reply, WinCall};
use nh_world::{
    Action, ActionBar, BAR_SLOTS, Cell, ClickPlan, CommandInput, CountEntry, Key, KeyContext,
    KeyInput, KeyProfile, MacroRunner, MacroStep, MenuKind, Mode, Order, Prompt, SlotBinding,
    SlotState, SlotUse, Stop, TickDriver, Typeahead, UiState, World, click_order, describe_cell,
    find_path, in_field, item_question, menu_kind, nethack_key, repeatable, stairs_order,
};

use crate::dialogs::Dialogs;
use crate::gamepad::{OskOp, Pad, PadButton, PadCtx, PadKind, PadOut, RADIAL, RadialEntry};
use crate::hud::Hud;
use crate::icons;
use crate::input::{client_key, key_input, key_release};
use crate::inventory_panel::{self, Intent, InvInput, InventoryPanel, KeyUse, WarmStep};
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
    /// The window's size in pixels, `--size=1280x800` (self-tests: the
    /// Steam Deck's screen). Godot's own `--resolution` never reaches the
    /// game: Godot takes it out of the arguments.
    pub size: Option<(i32, i32)>,
}

/// The window's size the project opens with (its override, else its
/// viewport's).
fn project_window_size() -> Vector2i {
    let ps = ProjectSettings::singleton();
    let get = |key: &str| {
        ps.get_setting(&format!("display/window/size/{key}"))
            .try_to::<i32>()
            .unwrap_or(0)
    };
    let pick = |over: i32, base: i32| if over > 0 { over } else { base };
    Vector2i::new(
        pick(get("window_width_override"), get("viewport_width")),
        pick(get("window_height_override"), get("viewport_height")),
    )
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
        } else if let Some(v) = a.strip_prefix("--size=")
            && let Some((w, h)) = v.split_once('x')
            && let (Ok(w), Ok(h)) = (w.trim().parse(), h.trim().parse())
            && w > 0
            && h > 0
        {
            out.size = Some((w, h));
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
    pub inventory: InventoryPanel,
    pub dialogs: Dialogs,
    pub pad: crate::pad_view::PadView,
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

/// Seconds since the engine started (the pad's clock).
fn now_secs() -> f64 {
    godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0
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
    /// Self-tests only: the next game in debug (wizard) mode, its
    /// playground allowing it (level teleport to a branch).
    pub(crate) debug_mode: bool,
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
    /// What the hero uses, from the keys and letters sent (decision 8).
    uses: nh_world::UseTracker,
    /// This character's key profile and action bar (`<save>.rhui.json`).
    pub(crate) ui_state: UiState,
    /// A new character's role (or "random": the one the map shows): its
    /// default loadout goes on the bar once the first inventory comes.
    loadout_for: Option<String>,
    /// The item actions behind clicks, drags and bar slots.
    pub(crate) macros: MacroRunner,
    /// Keys the engine reads after a getobj answer (get_count: the rest
    /// of a count, then the letter), one per command prompt.
    typing: std::collections::VecDeque<i32>,
    /// A slot or panel action waiting for the command prompt (the engine
    /// was busy with an order's step).
    deferred: Option<SlotUse>,
    /// The slot a right click cleared last, for Undo.
    cleared: Option<(usize, SlotBinding)>,
    /// The bar is drawn again when this changes.
    bar_key: Option<Vec<nh_world::SlotView>>,
    /// A gamepad's state, and its world cursor (None: on the hero).
    pub(crate) pad: Pad,
    pub(crate) pad_cursor: Option<(i32, i32)>,
    /// With RENETHACK_FRAME_STATS: what this frame's work spent, by part
    /// (printed when it is long).
    prof: Option<Vec<(&'static str, f64)>>,
    /// Where the warm-up behind the title is, what it did this frame
    /// (RENETHACK_FRAME_STATS), and whether that was a first draw (the
    /// map's loading waits a frame then: no two first draws share one).
    title_warm: TitleWarm,
    warm_did: &'static str,
    warm_heavy: bool,
    /// Frames waited for the rehearsal's hero to show on the doll.
    title_frames: u32,
    /// Frames processed, and whether none is drawn (headless): frames are
    /// counted by those processed then.
    frames: u64,
    headless: bool,
    /// The start-up veil (the boot splash's colour) and when it began to
    /// lift (None: still down).
    veil: Option<Gd<godot::classes::ColorRect>>,
    veil_lift: Option<Instant>,
    /// When the last title frame began, and the pipelines compiled and
    /// video memory used then (RENETHACK_FRAME_STATS).
    title_clock: Option<Instant>,
    title_render: ([i64; 5], f64),
    /// The bar's labels were drawn for this (pad kind and page, if a pad).
    pad_labels: Option<Option<(PadKind, usize)>>,
    /// The window's size the self-test asked for (its screenshots must be
    /// that size).
    pub(crate) window_size: Option<Vector2i>,
}

/// The warm-up behind the title screen, in order: nothing drawn for the
/// first time shares a frame with another first draw (each is tens of
/// milliseconds of pipelines and buffers), and none falls on the frames
/// that present the title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleWarm {
    /// The title presented for a few frames first, once the start-up veil
    /// is up (the frames shown when it began).
    Settle(Option<u64>),
    /// A kind of dialog built a frame.
    Dialogs,
    /// A glyph's textures painted a frame.
    Glyphs,
    /// The map's warm-up (the rehearsal behind the title, the first
    /// levels' art), then the inventory panel on the rehearsal's hero.
    Map,
    Panel,
    Done,
}

/// Frames the title is drawn before the warm-up begins.
const TITLE_SETTLE: u64 = 4;
/// Frames drawn under the start-up veil: the first ones set up the 3D
/// view's buffers and passes (a frame or two of 50-220 ms); then seconds
/// the veil takes to fade.
const VEIL_FRAMES: u64 = 8;
const VEIL_FADE: f32 = 0.25;

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
            debug_mode: false,
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
            uses: nh_world::UseTracker::new(),
            ui_state: UiState::new(KeyProfile::Modern),
            loadout_for: None,
            macros: MacroRunner::new(),
            typing: std::collections::VecDeque::new(),
            deferred: None,
            cleared: None,
            bar_key: None,
            pad: Pad::new(),
            pad_cursor: None,
            pad_labels: None,
            prof: None,
            title_warm: TitleWarm::Settle(None),
            warm_did: "-",
            warm_heavy: false,
            title_frames: 0,
            frames: 0,
            headless: false,
            veil: None,
            veil_lift: None,
            title_clock: None,
            title_render: ([0; 5], 0.0),
            window_size: None,
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
        if let Some(mut window) = self.base().get_tree().get_root() {
            // the window the self-tests asked for; screenshots are
            // 1920×1080 (the review set, success criterion 1) unless a size
            // is given, or Godot's --resolution made the window another
            // size than the project's. Set before the UI scale is chosen:
            // 1280×800 is a Steam Deck's.
            let size = args.size.map(|(w, h)| Vector2i::new(w, h)).or_else(|| {
                let start = window.get_size();
                let asked = start != project_window_size();
                args.screenshots.is_some().then_some(if asked {
                    start
                } else {
                    Vector2i::new(1920, 1080)
                })
            });
            if let Some(size) = size {
                window.set_size(size);
                self.window_size = Some(size);
            }
            crate::theme::apply_scaling(window);
        }

        // layers are the game's own children, added through base_mut():
        // widgets get them and never the game node itself
        let mut map_root = Node3D::new_alloc();
        map_root.set_name("MapRoot");
        let mut layers = Vec::new();
        for (name, z) in [
            ("HudLayer", 1),
            ("PanelLayer", 2),
            ("DialogLayer", 3),
            ("PadLayer", 4),
            ("ScreenLayer", 5),
        ] {
            let mut layer = CanvasLayer::new_alloc();
            layer.set_name(name);
            layer.set_layer(z);
            layers.push(layer);
        }
        self.base_mut().add_child(&map_root);
        for layer in &layers {
            self.base_mut().add_child(layer);
        }
        // the boot splash's colour over everything for the first frames:
        // the first 3D frame's buffers and passes are made behind it
        let mut veil_layer = CanvasLayer::new_alloc();
        veil_layer.set_name("VeilLayer");
        veil_layer.set_layer(100);
        let mut veil = godot::classes::ColorRect::new_alloc();
        veil.set_color(crate::theme::BG);
        crate::theme::place(&veil, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        veil_layer.add_child(&veil);
        self.base_mut().add_child(&veil_layer);
        self.veil = Some(veil);
        self.headless = godot::classes::DisplayServer::singleton().get_name() == "headless";
        let queue = self.queue.clone();
        let mut layers = layers.into_iter();
        let mut next = || layers.next().expect("five layers");
        self.ui = Some(Ui {
            map: MapView::new(map_root),
            hud: Hud::new(next(), queue.clone()),
            inventory: InventoryPanel::new(next(), queue.clone()),
            dialogs: Dialogs::new(next(), queue.clone()),
            pad: crate::pad_view::PadView::new(next()),
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
        self.frames += 1;
        let stats = std::env::var_os("RENETHACK_FRAME_STATS").is_some();
        self.time_title_frame(stats);
        self.lift_veil();
        self.warm_up();
        self.prof = stats.then(Vec::new);
        let t = Instant::now();
        self.pad_tick();
        self.drain_ui();
        self.lap("input", t);
        let t = Instant::now();
        self.pump();
        self.lap("pump", t);
        self.drive();
        let t = Instant::now();
        self.sync_views(delta);
        self.lap("views", t);
        if let Some(p) = self.prof.take() {
            let total: f64 = p
                .iter()
                .filter(|(k, _)| matches!(*k, "input" | "pump" | "views"))
                .map(|(_, v)| v)
                .sum();
            if total > 12.0 {
                let parts: Vec<String> = p.iter().map(|(k, v)| format!("{k} {v:.1}")).collect();
                godot_print!(
                    "game: a frame's work of {total:.1} ms: {}",
                    parts.join(", ")
                );
            }
        }
        // the map's loading begins after the title's own warm-up and waits
        // out a frame of a first draw of it (such frames only keep its
        // title log a frame each)
        let map_turn = !matches!(
            self.title_warm,
            TitleWarm::Settle(_) | TitleWarm::Dialogs | TitleWarm::Glyphs
        ) || self.state != GameState::Title;
        if let Some(ui) = self.ui.as_mut() {
            if map_turn && !self.warm_heavy {
                ui.map.preload_step();
            } else {
                ui.map.title_tick();
            }
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
        // a gamepad: what its buttons and sticks mean on this screen
        let event = match event.try_cast::<InputEventJoypadButton>() {
            Ok(b) => {
                self.pad.kind = PadKind::from_name(
                    &Input::singleton().get_joy_name(b.get_device()).to_string(),
                );
                if let Some(pb) = PadButton::from_joy(b.get_button_index()) {
                    let ctx = self.pad_ctx();
                    let outs = self.pad.button(pb, b.is_pressed(), ctx, now_secs());
                    self.pad_out(outs);
                }
                self.handled();
                return;
            }
            Err(event) => event,
        };
        let event = match event.try_cast::<InputEventJoypadMotion>() {
            Ok(m) => {
                let ctx = self.pad_ctx();
                let outs = self
                    .pad
                    .axis(m.get_axis(), m.get_axis_value(), ctx, now_secs());
                self.pad_out(outs);
                self.handled();
                return;
            }
            Err(event) => event,
        };
        if event.clone().try_cast::<InputEventMouseButton>().is_ok() {
            self.pad.active = false;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        self.pad.active = false;
        let text = self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.dialogs.wants_text() || ui.inventory.wants_text());
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

    /// Behind the title screen, what the first frames of a game would
    /// otherwise pay for, a step a frame (see `TitleWarm`): one dialog of
    /// each kind drawn for a few frames, a glyph's textures painted, the
    /// map's rehearsal and art, the inventory panel and its doll.
    fn warm_up(&mut self) {
        self.warm_did = "-";
        self.warm_heavy = false;
        let shown = self.frames_shown();
        let catalog = self.catalog.clone();
        let title = self.state == GameState::Title;
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if !title {
            // a game started before the warm-up was over: nothing of it
            // stays on screen
            ui.dialogs.warm_tick();
            ui.inventory.warm_end();
            return;
        }
        match self.title_warm {
            TitleWarm::Settle(_) if self.veil.is_some() => {}
            TitleWarm::Settle(start) => {
                ui.dialogs.warm_tick();
                let start = start.unwrap_or(shown);
                self.title_warm = if shown >= start + TITLE_SETTLE {
                    TitleWarm::Dialogs
                } else {
                    TitleWarm::Settle(Some(start))
                };
            }
            TitleWarm::Dialogs => match ui.dialogs.warm_step(catalog.as_deref()) {
                Some(kind) => {
                    self.warm_did = kind;
                    self.warm_heavy = true;
                }
                None if ui.dialogs.warm_done() => self.title_warm = TitleWarm::Glyphs,
                None => {}
            },
            TitleWarm::Glyphs => {
                ui.dialogs.warm_tick();
                self.warm_did = "glyph";
                if !icons::warm_step() {
                    self.title_warm = TitleWarm::Map;
                }
            }
            TitleWarm::Map => {
                ui.dialogs.warm_tick();
                // the rehearsal begins and the map is drawn behind the
                // title (once there is a catalog): its loading waits a frame
                let Some(cat) = catalog.as_deref() else {
                    return;
                };
                ui.map.warm_up(cat, None);
                self.warm_did = "map warm-up";
                self.warm_heavy = true;
                self.title_warm = TitleWarm::Panel;
            }
            TitleWarm::Panel => {
                ui.dialogs.warm_tick();
                // the doll renders the rehearsal's hero (if none comes for
                // long, the panel warms without it)
                self.title_frames += 1;
                let hero = ui.map.hero_model().map(|m| m.node.clone());
                if hero.is_none() && self.title_frames <= 900 {
                    return;
                }
                let step = ui.inventory.warm_step(hero);
                (self.warm_did, self.warm_heavy) = match step {
                    WarmStep::Panel => ("panel made", true),
                    WarmStep::Drawn => ("panel drawn", true),
                    WarmStep::Doll => ("doll", true),
                    WarmStep::Shown => ("panel shown", false),
                    WarmStep::Done => ("panel done", false),
                };
                if step == WarmStep::Done {
                    self.title_warm = TitleWarm::Done;
                }
            }
            TitleWarm::Done => {
                ui.dialogs.warm_tick();
            }
        }
    }

    /// The start-up veil lifts after its frames, whatever the first screen
    /// is (the title, or an error).
    fn lift_veil(&mut self) {
        let Some(mut veil) = self.veil.clone() else {
            return;
        };
        match self.veil_lift {
            None => {
                if self.frames_shown() >= VEIL_FRAMES {
                    self.veil_lift = Some(Instant::now());
                }
            }
            Some(t) => {
                let a = 1.0 - t.elapsed().as_secs_f32() / VEIL_FADE;
                if a <= 0.0 {
                    if let Some(mut layer) = veil.get_parent() {
                        layer.queue_free();
                    }
                    self.veil = None;
                } else {
                    veil.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
                }
            }
        }
    }

    /// Frames shown so far: those drawn, or (headless, where none is)
    /// those processed.
    fn frames_shown(&self) -> u64 {
        if self.headless {
            self.frames
        } else {
            godot::classes::Engine::singleton().get_frames_drawn() as u64
        }
    }

    /// The first screen is still under the start-up veil (self-tests: not
    /// up yet).
    pub(crate) fn veiled(&self) -> bool {
        self.veil.is_some()
    }

    /// The warm-up behind the title screen is done (self-tests start a
    /// game after it, as a player would).
    pub(crate) fn warmed_up(&self) -> bool {
        self.title_warm == TitleWarm::Done
    }

    /// A title frame over 33 ms, with what the warm-up did in the one
    /// before (RENETHACK_FRAME_STATS; the map logs its own part).
    fn time_title_frame(&mut self, stats: bool) {
        let now = Instant::now();
        let last = self.title_clock.replace(now);
        if !stats || self.state != GameState::Title {
            return;
        }
        use godot::classes::performance::Monitor as M;
        let perf = godot::classes::Performance::singleton();
        let pipelines = [
            M::PIPELINE_COMPILATIONS_CANVAS,
            M::PIPELINE_COMPILATIONS_MESH,
            M::PIPELINE_COMPILATIONS_SURFACE,
            M::PIPELINE_COMPILATIONS_DRAW,
            M::PIPELINE_COMPILATIONS_SPECIALIZATION,
        ]
        .map(|m| perf.get_monitor(m) as i64);
        let vmem = perf.get_monitor(M::RENDER_VIDEO_MEM_USED) / 1e6;
        let (before, vmem_before) = std::mem::replace(&mut self.title_render, (pipelines, vmem));
        if let Some(t) = last {
            let ms = now.duration_since(t).as_secs_f64() * 1000.0;
            if ms > 33.0 {
                let new: Vec<i64> = pipelines.iter().zip(before).map(|(a, b)| a - b).collect();
                // a frame drawn under the start-up veil (fully down) is not
                // seen: the boot splash goes on
                let seen = if self.veil.is_some() && self.veil_lift.is_none() {
                    ", under the start-up veil"
                } else {
                    ""
                };
                godot_print!(
                    "game: a title frame of {ms:.1} ms (frame {}{seen}); the warm-up before it: {} ({:?}); pipelines compiled (canvas, mesh, surface, draw, specialization) {new:?}, video memory {:+.1} MB",
                    godot::classes::Engine::singleton().get_frames_drawn(),
                    self.warm_did,
                    self.title_warm,
                    vmem - vmem_before
                );
            }
        }
    }

    /// Time since `t` under `what` (RENETHACK_FRAME_STATS).
    fn lap(&mut self, what: &'static str, t: Instant) {
        if let Some(p) = self.prof.as_mut() {
            p.push((what, t.elapsed().as_secs_f64() * 1000.0));
        }
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
        ui.pad.set_visible(on);
        if !on {
            ui.dialogs.close();
            ui.inventory.reset();
            ui.pad.radial_open(false);
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
                icons::set_catalog(&c);
                self.catalog = Some(Rc::new(*c));
            }
            SessionEvent::Win(w) => self.world.apply(&w),
            SessionEvent::Request { id, req } => {
                let prompt = self.world.on_request(&req);
                // the turn resolved: the hero's use is shown
                if let Some(u) = self.uses.on_prompt(&prompt, &self.world)
                    && let (Some(ui), Some(cat)) = (self.ui.as_mut(), self.catalog.as_deref())
                {
                    ui.map.show_use(&u, cat, &self.world);
                }
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
        // the rest of a count and the letter, read by get_count
        if !self.typing.is_empty() {
            if matches!(prompt, Prompt::Command | Prompt::Key)
                && let Some(k) = self.typing.pop_front()
            {
                self.pending = Some((id, prompt));
                self.reply(Reply::Key(k));
                return;
            }
            self.typing.clear();
        }
        // a macro answers the prompts it expects; the first one it does
        // not goes to the player as usual
        if self.macros.is_active()
            && let MacroStep::Reply(r) = self.macros.on_prompt(&prompt, &self.world)
        {
            self.pending = Some((id, prompt));
            self.reply(r);
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
        let inventory_menu = match &prompt {
            Prompt::Menu { how, items, .. } => {
                *how != PickHow::None
                    && menu_kind(items, &self.world.inventory) == MenuKind::Inventory
            }
            _ => false,
        };
        let question = item_question(&prompt);
        let t = Instant::now();
        let ui = self.ui_mut();
        match (&prompt, question) {
            // getobj: the panel in selection mode
            (Prompt::FreeKey { query, .. }, Some(q)) => {
                ui.dialogs.close();
                ui.inventory.open_select(id, q, query);
            }
            (
                Prompt::Menu {
                    how, title, items, ..
                },
                _,
            ) if inventory_menu => {
                ui.dialogs.close();
                ui.inventory.open_menu(id, *how, title.as_deref(), items);
            }
            _ => {
                // a direction or a place follows: the panel makes way
                if matches!(
                    prompt,
                    Prompt::FreeKey {
                        directions: true,
                        ..
                    }
                ) {
                    ui.inventory.close();
                }
                ui.dialogs.open(id, &prompt, catalog.as_deref());
            }
        }
        let dialog = ui.dialogs.is_open();
        let kind = match &prompt {
            Prompt::Menu { .. } => "dialog menu",
            Prompt::Show { .. } => "dialog text",
            Prompt::ExtCmd => "dialog palette",
            Prompt::Choice { .. } => "dialog choice",
            Prompt::Text { .. } => "dialog getlin",
            Prompt::FreeKey { .. } => "dialog getobj",
            _ => "dialog",
        };
        self.lap(kind, t);
        let ui = self.ui_mut();
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
        // a slot pressed while an order's step was out
        if self.at_command()
            && !self.driver.is_active()
            && let Some(u) = self.deferred.take()
        {
            self.use_slot(u);
        }
    }

    /// The engine waits at its command prompt (not getpos).
    fn at_command(&self) -> bool {
        matches!(self.pending, Some((_, Prompt::Command))) && !self.world.getpos
    }

    /// Answer the pending request.
    fn reply(&mut self, reply: Reply) {
        let Some((id, prompt)) = self.pending.take() else {
            godot_warn!("renethack: no request waits for {reply:?}");
            return;
        };
        let ui = self.ui_mut();
        ui.dialogs.close();
        if ui.inventory.request() == Some(id) {
            ui.inventory.end_request();
        }
        ui.hud.set_prompt_line(None);
        self.send(id, &prompt, reply);
    }

    fn send(&mut self, id: u64, prompt: &Prompt, reply: Reply) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match session.answer(id, &reply) {
            Ok(()) => {
                self.uses.on_reply(prompt, &reply, &self.world);
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
            // the character is gone with its save: its bar too
            if let Some(n) = &name
                && let Err(e) = remove_ui_state(&pg, n)
            {
                godot_warn!("renethack: cannot remove the UI state: {e}");
            }
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
                Ok((_, catalog)) => {
                    // the first level's models are built behind the title,
                    // once it is up (warm_up); the palette now, under the
                    // boot splash
                    icons::set_catalog(&catalog);
                    if let Some(ui) = self.ui.as_mut() {
                        ui.dialogs.prebuild_palette(Some(&catalog));
                    }
                    self.catalog = Some(Rc::new(catalog));
                }
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
        // a key of the player's own ends a macro that waits for the engine
        if !k.echo && self.macros.is_active() {
            self.macros.cancel();
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
        let command = *prompt == Prompt::Command && !self.world.getpos;
        // the open panel first (not under a dialog of its own); a count
        // being typed keeps its Esc
        let typing_count = command && self.count.shown().is_some();
        let dialog = self.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open());
        if !dialog && !(typing_count && k.key == Key::Escape) {
            match self.ui_mut().inventory.key(&k, command) {
                KeyUse::Used => return,
                KeyUse::Then(intent) => {
                    self.inventory_intent(intent);
                    return;
                }
                KeyUse::Pass => {}
            }
        }
        let Some((_, prompt)) = &self.pending else {
            return;
        };
        if command {
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
        let profile = self.ui_state.profile;
        // the top-row digits are the bar's (Alt+digits Classic's count)
        let digit = matches!(k.key, Key::Char('0'..='9')) && !k.mods.ctrl;
        let Some(code) = nethack_key(&k, KeyContext::Command, np, &dirs) else {
            return;
        };
        let c = u32::try_from(code).ok().and_then(char::from_u32);
        if k.echo && digit {
            return;
        }
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
        match self.count.feed_key(&k, profile, &dirs) {
            CommandInput::Typing => {
                let line = self.count.shown();
                self.ui_mut().hud.set_prompt_line(line.as_deref());
            }
            CommandInput::Ignored => {}
            CommandInput::Bar { slot, count } => {
                if count.is_some() {
                    self.ui_mut().hud.set_prompt_line(None);
                }
                self.activate_slot(slot, count);
            }
            CommandInput::Command {
                key: code,
                count: n,
            } => {
                let c = u32::try_from(code).ok().and_then(char::from_u32);
                if n.is_some() {
                    self.ui_mut().hud.set_prompt_line(None);
                }
                // `i`: the client's inventory panel; the engine's `i` is
                // never sent (the host pushes the inventory)
                if c == Some('i') {
                    self.driver.interrupt(Stop::Panel);
                    self.on_inventory(InvInput::Toggle);
                    return;
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

    /// What a gamepad's buttons mean now (the topmost thing on screen).
    pub(crate) fn pad_ctx(&self) -> PadCtx {
        let Some(ui) = self.ui.as_ref() else {
            return PadCtx::Other;
        };
        if ui.dialogs.is_open() {
            return match ui.dialogs.kind_name() {
                Some("menu") => PadCtx::Menu {
                    any: ui.dialogs.menu_any(),
                },
                Some("choice") => PadCtx::Choice,
                Some("text") => PadCtx::Text,
                Some("message") => match ui.dialogs.message_letter() {
                    Some(letter) => PadCtx::Message { letter },
                    None => PadCtx::Other,
                },
                _ => PadCtx::Other,
            };
        }
        if ui.inventory.is_open() {
            return match ui.inventory.mode_name() {
                Some("select") => PadCtx::PanelSelect,
                Some("menu") => PadCtx::PanelMenu,
                _ => PadCtx::PanelBrowse,
            };
        }
        match self.pending.as_ref().map(|(_, p)| p) {
            Some(Prompt::Command) if self.world.getpos => PadCtx::Getpos,
            Some(Prompt::Command) | None => PadCtx::World,
            Some(Prompt::FreeKey {
                directions: true, ..
            }) => PadCtx::Direction,
            _ => PadCtx::Other,
        }
    }

    /// Held sticks repeat.
    fn pad_tick(&mut self) {
        if self.state != GameState::Playing {
            return;
        }
        let ctx = self.pad_ctx();
        let outs = self.pad.tick(ctx, now_secs());
        self.pad_out(outs);
    }

    /// The world cursor's cell: where it was moved, else the hero.
    fn pad_cell(&self) -> Option<(i32, i32)> {
        self.pad_cursor.or_else(|| self.world.hero())
    }

    /// Do what the pad asks.
    fn pad_out(&mut self, outs: Vec<PadOut>) {
        for o in outs {
            match o {
                PadOut::Key(k) => self.push_ui(UiEvent::Key(k)),
                PadOut::KeyUp(k) => self.push_ui(UiEvent::KeyUp(k)),
                PadOut::Activate => {
                    if let Some((x, y)) = self.pad_cell() {
                        self.push_ui(UiEvent::MapClick { x, y, button: 1 });
                    }
                    self.pad_cursor = None;
                }
                PadOut::Cursor(dx, dy) => {
                    if let Some((x, y)) = self.pad_cell() {
                        let (nx, ny) = (x + dx, y + dy);
                        if in_field(nx, ny) {
                            self.pad_cursor = Some((nx, ny));
                        }
                    }
                }
                PadOut::CursorHome => self.pad_cursor = None,
                PadOut::Slot(slot) => self.push_ui(UiEvent::ActionSlot { slot, button: 1 }),
                PadOut::Page(_) => {}
                PadOut::Carry => self.push_ui(UiEvent::Inventory(InvInput::Carry)),
                PadOut::History => self.push_ui(UiEvent::ToggleFullLog),
                PadOut::RadialOpen => self.ui_mut().pad.radial_open(true),
                PadOut::RadialHover(sel) => self.ui_mut().pad.select(sel),
                PadOut::RadialClose(sel) => {
                    self.ui_mut().pad.radial_open(false);
                    if let Some(e) = sel.and_then(|i| RADIAL.get(i)) {
                        self.radial_run(*e);
                    }
                }
                PadOut::Osk(op) => self.osk(op),
            }
        }
    }

    /// The on-screen keyboard's key; its answer, if any.
    fn osk(&mut self, op: OskOp) {
        let pending = self.pending.as_ref().map(|(id, _)| *id);
        let ui = self.ui_mut();
        if ui.dialogs.open_req() != pending {
            return;
        }
        if let Some(r) = ui.dialogs.osk(op) {
            self.reply(r);
        }
    }

    /// A radial menu entry.
    fn radial_run(&mut self, e: RadialEntry) {
        let key = |c: char| UiEvent::Key(KeyInput::plain(Key::Char(c)));
        match e {
            RadialEntry::Here => {
                if let Some((x, y)) = self.pad_cell() {
                    self.push_ui(UiEvent::MapClick { x, y, button: 2 });
                }
            }
            RadialEntry::PickUp => self.push_ui(key(',')),
            RadialEntry::Fight => self.push_ui(key('F')),
            RadialEntry::Kick => self.push_ui(UiEvent::Key(KeyInput {
                key: Key::Char('d'),
                mods: nh_world::Mods {
                    ctrl: true,
                    ..Default::default()
                },
                echo: false,
            })),
            RadialEntry::Rest => self.push_ui(UiEvent::Rest),
            RadialEntry::Pray => self.use_slot(SlotUse::Macro(nh_world::Macro::ext("pray"))),
            RadialEntry::Travel => self.push_ui(key('_')),
            RadialEntry::Save => self.push_ui(key('S')),
        }
    }

    /// Slot `slot` of the bar, with the count typed before it.
    fn activate_slot(&mut self, slot: usize, count: Option<u32>) {
        let u =
            self.ui_state
                .bar
                .activate(slot, &self.world.inventory, count, self.ui_state.profile);
        self.use_slot(u);
    }

    /// Run a macro or start an order, at the command prompt (after
    /// stopping any order); while the engine plays an order's step, at the
    /// next command prompt; never as the answer to another question.
    fn use_slot(&mut self, u: SlotUse) {
        if matches!(u, SlotUse::Nothing) {
            return;
        }
        self.driver.interrupt(Stop::Key);
        if !self.at_command() {
            if self.pending.is_none() {
                self.deferred = Some(u);
            }
            return;
        }
        if self.count.shown().is_some() {
            self.count.clear();
            self.ui_mut().hud.set_prompt_line(None);
        }
        match u {
            SlotUse::Macro(m) => {
                if let Some(r) = self.macros.start(m) {
                    self.reply(r);
                }
            }
            SlotUse::Order(o) => {
                self.start_order(o);
            }
            SlotUse::Nothing => {}
        }
    }

    /// Something the panel's widgets did.
    fn on_inventory(&mut self, ev: InvInput) {
        if ev == InvInput::Toggle {
            self.driver.interrupt(Stop::Panel);
        }
        if let Some(i) = self.ui_mut().inventory.input(ev) {
            self.inventory_intent(i);
        }
    }

    /// What the panel asks for.
    fn inventory_intent(&mut self, i: Intent) {
        let panel_req = self.ui.as_ref().and_then(|ui| ui.inventory.request());
        let pending = self.pending.as_ref().map(|(id, _)| *id);
        match i {
            Intent::Macro(m) => self.use_slot(SlotUse::Macro(m)),
            Intent::Reply(r) => {
                if panel_req.is_some() && panel_req == pending {
                    self.reply(r);
                }
            }
            Intent::Pick { letter, count } => {
                if panel_req.is_none() || panel_req != pending {
                    return;
                }
                match count {
                    // getobj takes the first digit, get_count the rest
                    Some(n) => {
                        let mut keys: std::collections::VecDeque<i32> =
                            n.to_string().bytes().map(i32::from).collect();
                        let first = keys.pop_front().unwrap_or('1' as i32);
                        keys.push_back(letter as i32);
                        self.reply(Reply::Char(first));
                        self.typing = keys;
                    }
                    None => self.reply(Reply::Char(letter as i32)),
                }
            }
            Intent::Bind { slot, binding } => {
                self.ui_state.bar.set(slot, Some(binding));
                self.save_ui_state();
                let key = crate::action_bar::key_label(slot);
                self.ui_mut()
                    .hud
                    .toast(&format!("Bound to slot {key}"), false);
            }
        }
    }

    /// Right click on a slot: it is cleared, with an Undo.
    fn clear_slot(&mut self, slot: usize) {
        let Some(b) = self.ui_state.bar.get(slot).cloned() else {
            return;
        };
        self.ui_state.bar.set(slot, None);
        self.cleared = Some((slot, b));
        self.save_ui_state();
        let key = crate::action_bar::key_label(slot);
        self.ui_mut()
            .hud
            .toast(&format!("Slot {key} cleared"), true);
    }

    fn undo_clear(&mut self) {
        if let Some((slot, b)) = self.cleared.take() {
            self.ui_state.bar.set(slot, Some(b));
            self.save_ui_state();
            let key = crate::action_bar::key_label(slot);
            self.ui_mut()
                .hud
                .toast(&format!("Slot {key} restored"), false);
        }
    }

    /// Write `<save>.rhui.json` (atomically).
    fn save_ui_state(&mut self) {
        self.bar_key = None;
        let Some(name) = self.name.clone() else {
            return;
        };
        if let Err(e) = write_ui_state(&self.playground(), &name, &self.ui_state.to_json()) {
            godot_warn!("renethack: cannot write the UI state: {e}");
        }
    }

    /// The pack for the panel and the bar: the bar follows its items, a
    /// new character gets its loadout, the slots are drawn again.
    fn sync_inventory(&mut self) {
        if self.world.inventory.take_changed() {
            let pack = self.world.inventory.clone();
            self.ui_mut().inventory.set_pack(&pack);
            if self.ui_state.bar.rebind(&pack) {
                self.save_ui_state();
            }
            self.bar_key = None;
        }
        if let Some(role) = self.loadout_for.clone()
            && self.world.inventory.received()
        {
            let role = match role.as_str() {
                "random" => self
                    .ui
                    .as_ref()
                    .and_then(|ui| ui.hud.role().map(str::to_string)),
                _ => Some(role),
            };
            if let Some(role) = role {
                self.loadout_for = None;
                self.ui_state.bar = ActionBar::default_for(&role, &self.world.inventory);
                self.save_ui_state();
            }
        }
        let status = &self.world.status;
        let get = |f: &str| {
            status
                .get(f)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        // gold comes as "$:12" (the glyph and the amount)
        let gold = get("gold").map(|g| g.rsplit(':').next().unwrap_or(&g).to_string());
        let (ac, cap) = (get("ac"), get("cap"));
        let active = self.at_command();
        let views: Vec<nh_world::SlotView> = (0..BAR_SLOTS)
            .map(|i| self.ui_state.bar.view(i, &self.world.inventory))
            .collect();
        let redraw = self.bar_key.as_ref() != Some(&views);
        let ctx = self.pad_ctx();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.inventory.set_status(ac, gold, cap);
        let open = ui.inventory.is_open();
        let asks = ui.inventory.request().is_some();
        ui.hud.set_panel_open(open, asks);
        // a gamepad: its hints, the bar's chords, dialogs with a focus
        let pad = self.pad.active.then_some(self.pad.kind);
        ui.dialogs.set_pad(pad.is_some());
        ui.inventory.set_pad(pad);
        ui.pad.show_hints(pad.map(|k| (k, ctx)));
        ui.pad.dock_hints(ui.inventory.frame_rect());
        let labels = pad.map(|k| (k, self.pad.page()));
        if self.pad_labels != Some(labels) {
            self.pad_labels = Some(labels);
            let texts: Option<Vec<String>> = labels.map(|(kind, page)| {
                (0..BAR_SLOTS)
                    .map(|i| {
                        let (p, rb, face) = Pad::chord_of(i);
                        if p != page {
                            return String::new();
                        }
                        let bumper = if rb { PadButton::Rb } else { PadButton::Lb };
                        format!("{} {}", kind.label(bumper), kind.label(face))
                    })
                    .collect()
            });
            ui.hud.action_bar().set_key_labels(texts.as_deref());
        }
        let hero = ui.map.hero_model().map(|m| m.node.clone());
        ui.inventory.set_hero(hero);
        let rects = ui.hud.action_bar().slot_rects();
        ui.inventory.set_bar_rects(rects);
        ui.inventory.sync();
        let bar = ui.hud.action_bar();
        bar.set_active(active);
        if !redraw {
            return;
        }
        for (i, v) in views.iter().enumerate() {
            let binding = self.ui_state.bar.get(i);
            let icon = match binding {
                None => None,
                Some(SlotBinding::Item { key, .. }) => {
                    let class = self
                        .world
                        .inventory
                        .items()
                        .iter()
                        .find(|it| it.tile == key.tile)
                        .map_or_else(|| icons::tile_class(key.tile), |it| it.class);
                    Some(icons::item_icon(key.tile, class))
                }
                Some(SlotBinding::Spell { .. }) => Some(icons::emblem(icons::Glyph::Book)),
                Some(SlotBinding::Command { cmd }) => {
                    Some(icons::emblem(icons::command_glyph(*cmd)))
                }
            };
            bar.set_icon(i, icon.as_ref());
            let count = v
                .charges
                .map(|c| c.to_string())
                .or_else(|| v.count.map(|c| c.to_string()));
            bar.set_count(i, count.as_deref());
            bar.set_enabled(i, v.state != SlotState::Gone);
            let key = crate::action_bar::key_label(i);
            let what = v
                .label_key
                .as_deref()
                .map(inventory_panel::label)
                .unwrap_or_default();
            let tip = match (v.state, &v.text) {
                (SlotState::Empty, _) => {
                    format!("Slot {key} (empty): drag an item here from the inventory")
                }
                (SlotState::Gone, Some(t)) => format!("{what}: {t} (not in your pack)"),
                (_, Some(t)) => format!("{what}: {t}"),
                (_, None) => what,
            };
            let hint = v.hint.as_deref().unwrap_or("");
            bar.set_tooltip(
                i,
                &format!("{tip}\nKey {key} · NetHack: {hint} · right-click: clear"),
            );
        }
        self.bar_key = Some(views);
    }

    /// A left click walks (and acts at the end of the way); a right click
    /// asks the engine for the cell's actions (#therecmdmenu). In getpos a
    /// click picks the cell.
    fn on_click(&mut self, x: i32, y: i32, button: i32) {
        // a click stops the order; a left click gives the next one
        self.driver.interrupt(Stop::Click);
        // a click on the world beside the open panel closes it (and does
        // what it does); while the panel asks, the question waits
        if let Some(ui) = self.ui.as_mut()
            && ui.inventory.is_open()
        {
            if ui.inventory.request().is_some() {
                return;
            }
            ui.inventory.close();
        }
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
        self.macros.cancel();
        self.typing.clear();
        self.deferred = None;
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
                // an inventory menu is the panel's
                let ui = self.ui_mut();
                let r = if ui.inventory.request() == Some(req) {
                    ui.inventory.dialog_event(req, &ev)
                } else {
                    ui.dialogs.dialog_event(req, &ev)
                };
                if let Some(r) = r {
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
                self.pad.release_all();
                self.held = None;
                self.driver.interrupt(Stop::Focus);
            }
            UiEvent::Rest => self.rest(),
            UiEvent::Zoom(steps) => self.ui_mut().map.zoom(steps),
            UiEvent::ToggleOverview => self.ui_mut().map.toggle_overview(),
            UiEvent::ActionSlot { slot, button: 1 } if self.state == GameState::Playing => {
                // a count typed before (`n20`, then a click)
                let count = self.count.take();
                if count.is_some() {
                    self.ui_mut().hud.set_prompt_line(None);
                }
                self.activate_slot(slot, count);
            }
            UiEvent::ActionSlot { slot, .. } => self.clear_slot(slot),
            UiEvent::SlotUndo => self.undo_clear(),
            UiEvent::Inventory(ev) if self.state == GameState::Playing => self.on_inventory(ev),
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
        if let (Some(ui), Some(cat)) = (self.ui.as_mut(), self.catalog.as_deref()) {
            ui.map.warm_up(cat, Some(&choice.role));
        }
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
        let options = if self.debug_mode && self.selftest.is_some() {
            if let Err(e) = std::fs::write(
                pg.join("sysconf"),
                "WIZARDS=*\nMAXPLAYERS=10\nPANICTRACE_GDB=0\nPANICTRACE_LIBC=0\n",
            ) {
                godot_warn!("renethack: cannot allow debug mode: {e}");
            }
            format!("{options},playmode:debug")
        } else {
            options
        };
        if let Err(e) = remember_name(&pg, &choice.name) {
            godot_warn!("renethack: cannot remember the name: {e}");
        }
        // a new character: its profile, and the default loadout once the
        // first inventory (and the role, if random) is known
        let state = UiState::new(choice.profile);
        if self.start_session(options, &choice.name, state) {
            self.loadout_for = Some(choice.role.clone());
            self.save_ui_state();
        }
    }

    fn continue_game(&mut self, name: &str) {
        match EngineConfig::restore_options(name) {
            Ok(options) => {
                if self.prepare_playground() {
                    // the profile and the bar kept with the save; none (a
                    // game from before them): Modern and the default loadout
                    let kept = read_ui_state(&self.playground(), name)
                        .and_then(|t| UiState::from_json(&t).ok());
                    let fresh = kept.is_none();
                    let state = kept.unwrap_or_else(|| UiState::new(KeyProfile::Modern));
                    if self.start_session(options, name, state) && fresh {
                        self.loadout_for = Some("random".into());
                    }
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

    /// Start the engine for `name` with this UI state; false when it
    /// cannot start.
    fn start_session(&mut self, options: String, name: &str, state: UiState) -> bool {
        let Some(paths) = self.paths.clone() else {
            return false;
        };
        let test = self.selftest.is_some();
        let cfg = EngineConfig {
            engine: paths.engine(),
            playground: paths.playground.clone(),
            // NetHack does not save number_pad: every start passes it
            options: format!(
                "{options},{CLIENT_EXTRA_OPTIONS},{}",
                state.profile.engine_option()
            ),
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
                self.ui_state = state;
                self.loadout_for = None;
                self.cleared = None;
                self.bar_key = None;
                true
            }
            Err(e) => {
                self.show_failure("Cannot start the game engine.", &e.to_string(), None);
                false
            }
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
        crate::icons::clear();
        self.base().get_tree().quit_ex().exit_code(code).done();
    }

    /// Answer the pending request with a reply no key gives (self-tests:
    /// the engine's own `?` at getobj, which the panel keeps).
    pub(crate) fn answer(&mut self, r: Reply) {
        self.reply(r);
    }

    pub(crate) fn push_ui(&self, ev: UiEvent) {
        push(&self.queue, ev);
    }

    /// The canvas the UI is laid out on, in design pixels (self-tests).
    pub(crate) fn canvas_size(&self) -> Vector2 {
        self.base()
            .get_viewport()
            .map_or(Vector2::ZERO, |v| v.get_visible_rect().size)
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
        // getpos's cursor on screen, and the nearest hostile in view
        let camera = self.base().get_viewport().and_then(|v| v.get_camera_3d());
        let on_screen = |(x, y): (i32, i32), lift: f32| {
            let cam = camera.as_ref()?;
            let p = Vector3::new(x as f32, lift, y as f32);
            if cam.is_position_behind(p) {
                return None;
            }
            Some(cam.unproject_position(p))
        };
        let cursor = self
            .world
            .cursor
            .filter(|_| self.world.getpos)
            .and_then(|c| on_screen(c, 0.3));
        let hero = self.world.hero();
        let threat = match (catalog.as_deref(), hero) {
            (Some(cat), Some(h)) => nh_world::threats(&self.world, cat, self.driver.peaceful())
                .into_iter()
                .min_by_key(|&(x, y)| (x - h.0).abs().max((y - h.1).abs())),
            _ => None,
        };
        let threat = threat.and_then(|t| on_screen(t, 0.8));
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.hud.set_cursor_at(cursor);
        ui.hud
            .set_threat(threat, Time::singleton().get_ticks_msec() as f64 / 1000.0);
        let t = Instant::now();
        ui.hud.sync(&mut self.world, catalog.as_deref());
        self.lap("hud", t);
        let t = Instant::now();
        self.sync_inventory();
        self.lap("inventory", t);
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
        let pos = self.mouse_pos.filter(|&p| {
            playing && !ui.dialogs.is_open() && !ui.hud.covers(p) && !ui.inventory.covers(p)
        });
        // a gamepad's cursor is the pointer while it plays
        let pad = self.pad_cursor.filter(|_| {
            self.pad.active && playing && !ui.dialogs.is_open() && !ui.inventory.is_open()
        });
        let pad_pos = pad.and_then(|(x, y)| {
            let cam = self.base().get_viewport()?.get_camera_3d()?;
            let p = Vector3::new(x as f32, 0.3, y as f32);
            (!cam.is_position_behind(p)).then(|| cam.unproject_position(p))
        });
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        let pos = pad_pos.or(pos);
        let cell = match (self.test_hover, pad) {
            (Some(c), _) if playing && !ui.dialogs.is_open() => Some(c),
            (_, Some(c)) => Some(c),
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
        assert_eq!(parse_args(&["--size=1280x800"]).size, Some((1280, 800)));
        assert_eq!(parse_args(&["--size=1280"]).size, None);
        assert_eq!(parse_args(&["--size=0x800"]).size, None);
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
