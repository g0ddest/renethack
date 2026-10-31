//! `RenethackGame`, the scene's root: runs the engine session, feeds the
//! world model, routes input to the map, the dialogs and the screens, and
//! owns the lifecycle (title, creation, play, save on close, end, errors).

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use godot::classes::notify::NodeNotification;
use godot::classes::{
    Camera3D, CanvasLayer, INode, Input, InputEvent, InputEventJoypadButton,
    InputEventJoypadMotion, InputEventKey, InputEventMouseButton, InputEventMouseMotion, InputMap,
    Node3D, Os, ProjectSettings, Time,
};
use godot::global::{JoyAxis, JoyButton, Key as GKey, MouseButton};
use godot::prelude::*;
use nh_link::{
    AnswerError, CLIENT_EXTRA_OPTIONS, Ending, EngineConfig, LiveSession, PlaygroundLock,
    Recovered, SessionEvent, create_playground, fetch_catalog, interrupted_games, list_saves,
    lock_playground, migrate_profile, read_profile, read_ui_state, recover_game, remember_name,
    remove_ui_state, save_exists, write_profile, write_ui_state,
};
use nh_protocol::{Catalog, PickHow, Reply, WinCall};
use nh_world::achievements::{Achievements, Tracker};
use nh_world::{
    Action, ActionBar, BAR_SLOTS, Cell, ClickPlan, CommandInput, CountEntry, GameOption, Gameplay,
    Key, KeyContext, KeyInput, KeyProfile, MacroRunner, MacroStep, MenuKind, Mode, OptionsRun,
    Order, Profile, Prompt, RunStep, Settings, SlotBinding, SlotState, SlotUse, Stop, TickDriver,
    Typeahead, UiState, World, click_order, describe_cell, find_path, in_field, item_question,
    menu_kind, nethack_key, repeatable, stairs_order,
};

use crate::census::Census;
use crate::dialogs::Dialogs;
use crate::end_view::{EndHow, EndSummary};
use crate::frame_log::{self, FrameLog};
use crate::gamepad::{OskOp, Pad, PadButton, PadCtx, PadKind, PadOut, RADIAL, RadialEntry};
use crate::hud::Hud;
use crate::i18n::{self, EngineKind, Lang};
use crate::icons;
use crate::input::{client_key, key_input, key_release};
use crate::inventory_panel::{self, Intent, InvInput, InventoryPanel, KeyUse, WarmStep};
use crate::map_view::MapView;
use crate::names;
use crate::paths::Paths;
use crate::screens::Screens;
use crate::selftest::SelfTest;
use crate::settings_page::{self, Change, PageKey, SettingsInput, SettingsPage};
use crate::tr;
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, new_queue, push};

/// Time per frame spent reading the engine.
const POLL_BUDGET: Duration = Duration::from_millis(8);
/// The engine owes output and has said nothing this long: it hangs.
const HANG_TIMEOUT: Duration = Duration::from_secs(20);
/// The guide's next card waits this long after the last went.
const HINT_GAP: Duration = Duration::from_millis(800);
/// How long a closing window waits for the engine to save.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);
/// Messages the end screen repeats.
const END_MESSAGES: usize = 12;
/// NetHack's achievement for entering Sokoban (ACH_SOKO): its rules are a
/// conduct from then on.
const SOKOBAN_ENTERED: i32 = 21;
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

/// The prologue playing: its run, the character it is told for as the
/// creation form chose it (the engine tells the rest as the game starts;
/// replayed from the title, the one the profile kept with its text), and
/// whether it was replayed.
pub(crate) struct PrologueShown {
    pub(crate) run: crate::prologue::Run,
    /// The engine's text, said again in the language played at each beat
    /// (the hero's gender known by then).
    english: Vec<String>,
    pub(crate) role: String,
    pub(crate) female: bool,
    /// The alignment (1 lawful, 0 neutral, -1 chaotic): a replay's, kept
    /// with its text (a new game's is the status's).
    pub(crate) align: i32,
    replay: bool,
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
    /// The interface's language, `--lang=ru` (self-tests; else the
    /// profile's, the system's or English).
    pub lang: Option<Lang>,
    /// The engine's output read a few events a frame, `--engine-pieces=1`
    /// (self-tests: as a loaded machine may read it).
    pub engine_pieces: Option<usize>,
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
        } else if let Some(v) = a.strip_prefix("--lang=")
            && let Some(lang) = Lang::from_code(v)
        {
            out.lang = Some(lang);
        } else if let Some(v) = a.strip_prefix("--engine-pieces=")
            && let Ok(n) = v.trim().parse::<usize>()
            && n > 0
        {
            out.engine_pieces = Some(n);
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
    pub help: crate::help_panel::HelpPanel,
    pub pad: crate::pad_view::PadView,
    pub screens: Screens,
    /// An achievement just earned, under the prompt banner.
    pub toast: crate::achievement_view::Toast,
    /// The first-steps guide's hint card, beside the HUD, and "Your
    /// quest" over the game at a new player's first game.
    pub hint: crate::guide_view::HintCard,
    pub quest: crate::guide_view::QuestPage,
    /// The game's menu (Esc at the command prompt).
    pub game_menu: crate::game_menu::GameMenu,
    /// The one tip on screen, over everything but the veil.
    pub tips: crate::tips::TipHost,
    pub audio: crate::audio::Audio,
    /// The prologue's words and black, over everything.
    pub prologue: crate::prologue_view::PrologueView,
}

/// The legacy text (`english`, the engine's lines) in the language played,
/// in the prologue's beats.
fn legacy_beats(english: &[String]) -> Option<[String; 6]> {
    let refs: Vec<&str> = english.iter().map(String::as_str).collect();
    let shown = i18n::engine_window(&refs).unwrap_or_else(|| english.to_vec());
    crate::prologue::beats(&shown).or_else(|| crate::prologue::beats(english))
}

/// The engine's legacy text, when `prompt` shows it: its lines.
fn legacy_of(prompt: &Prompt) -> Option<Vec<String>> {
    let Prompt::Show { lines, .. } = prompt else {
        return None;
    };
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    crate::prologue::is_legacy(&texts).then(|| texts.iter().map(|t| t.to_string()).collect())
}

/// What the prompt line says while `prompt` waits; with a gamepad (`pad`)
/// its button, not a key.
fn prompt_line(prompt: &Prompt, pad: Option<PadKind>) -> Option<String> {
    let a = pad.map(|k| k.label(PadButton::A));
    match (prompt, a) {
        (Prompt::Key, None) => Some(tr!("prompt-press-key")),
        (Prompt::Key, Some(a)) => Some(tr!("prompt-press-key-pad", a = a)),
        (Prompt::FreeKey { query, .. }, _) => {
            Some(i18n::engine(EngineKind::Prompt, query).into_owned())
        }
        (Prompt::MapPause, None) => Some(tr!("prompt-more")),
        (Prompt::MapPause, Some(a)) => Some(tr!("prompt-more-pad", a = a)),
        _ => None,
    }
}

/// What the prompt line says while getpos waits: what for (the engine's
/// words), and the keys, or a gamepad's buttons.
fn getpos_line(world: &World, pad: Option<PadKind>) -> Option<String> {
    world.getpos.then(|| {
        let goal = match world.getpos_goal.as_deref() {
            Some(g) => i18n::engine(EngineKind::Prompt, g).into_owned(),
            None => tr!("prompt-pick-spot"),
        };
        // this client's words, not the tty's keys: travel goes there,
        // anything else (a look, a spell's target) picks the cell
        let travel = world
            .getpos_goal
            .as_deref()
            .is_some_and(|g| g.contains("travel"));
        let verb = if travel {
            tr!("prompt-getpos-go")
        } else {
            tr!("prompt-getpos-pick")
        };
        match pad {
            Some(k) => tr!(
                "prompt-getpos-pad",
                goal = goal,
                verb = verb,
                a = k.label(PadButton::A),
                b = k.label(PadButton::B)
            ),
            None => tr!("prompt-getpos", goal = goal, verb = verb),
        }
    })
}

/// A held key as its release names it (a letter's case may differ).
fn held_key(k: Key) -> Key {
    match k {
        Key::Char(c) => Key::Char(c.to_ascii_lowercase()),
        other => other,
    }
}

/// What the HUD says an order does, and that a key (or a gamepad's
/// button) stops it.
fn order_text(order: &Order, pad: bool) -> String {
    use nh_world::Arrival;
    let what = match order {
        Order::Walk { arrival, .. } => match arrival {
            // to a peaceful one: a walk, no attack
            Arrival::None | Arrival::Meet => tr!("order-walk"),
            Arrival::PickUp => tr!("order-walk-pick-up"),
            Arrival::Stairs('<') => tr!("order-stairs-up"),
            Arrival::Stairs(_) => tr!("order-stairs-down"),
            Arrival::Open => tr!("order-open-door"),
            Arrival::Attack => tr!("order-attack"),
        },
        Order::Hold { .. } => tr!("order-hold"),
        Order::Rest => tr!("order-rest"),
    };
    if pad {
        tr!("order-line-pad", what = what)
    } else {
        tr!("order-line", what = what)
    }
}

/// What the HUD says about why an order ended.
fn stop_text(stop: &Stop) -> String {
    match stop {
        Stop::Arrived => tr!("stop-arrived"),
        Stop::Done => tr!("stop-done"),
        Stop::Healed => tr!("stop-healed"),
        Stop::OneAction => tr!("stop-one-action"),
        Stop::Released => tr!("stop-released"),
        Stop::Key | Stop::Click | Stop::Reset => tr!("stop-stopped"),
        Stop::Panel => tr!("stop-panel"),
        Stop::Focus => tr!("stop-focus"),
        Stop::Question => tr!("stop-question"),
        Stop::Hostile => tr!("stop-hostile"),
        Stop::HpLost => tr!("stop-hurt"),
        Stop::Hunger => tr!("stop-hunger"),
        Stop::Condition => tr!("stop-condition"),
        Stop::Message(m) => tr!(
            "stop-message",
            message = i18n::engine(EngineKind::Message, m).into_owned()
        ),
        Stop::Level => tr!("stop-level"),
        Stop::Blocked => tr!("stop-blocked"),
        Stop::NoPath => tr!("stop-no-path"),
    }
}

/// The interface's language at start: `--lang`, `RENETHACK_LANG`, else
/// (no self-test: those play in English unless asked) the profile's, else
/// the system's, else English.
pub(crate) fn startup_lang(args: &Args) -> Lang {
    if let Some(lang) = args.lang {
        return lang;
    }
    if let Some(lang) = std::env::var("RENETHACK_LANG")
        .ok()
        .and_then(|v| Lang::from_code(&v))
    {
        return lang;
    }
    if args.selftest.is_some() {
        return Lang::En;
    }
    let paths = Paths::resolve(args.playground.as_deref());
    load_profile(&paths.playground)
        .lang
        .as_deref()
        .and_then(Lang::from_code)
        .or_else(|| Lang::from_code(&Os::singleton().get_locale_language().to_string()))
        .unwrap_or_default()
}

/// The profile: the player's file and this machine's (`Profile::split`).
/// An older version's single file is split into them first, once.
pub(crate) fn load_profile(playground: &std::path::Path) -> Profile {
    if let Err(e) = migrate_profile(playground, Profile::split_old) {
        godot_warn!("renethack: cannot split the old profile: {e}");
    }
    let (player, device) = read_profile(playground);
    Profile::from_parts(player.as_deref(), device.as_deref())
}

/// The settings page's settings at start: the profile's; a self-test
/// plays with the defaults (as it plays in English).
pub(crate) fn startup_settings(args: &Args) -> Settings {
    if args.selftest.is_some() {
        return Settings::default();
    }
    let paths = Paths::resolve(args.playground.as_deref());
    load_profile(&paths.playground).settings
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
    /// The achievements: earned as the progress notices tell, kept in the
    /// local store, sent to Steam when it runs.
    pub(crate) achievements: Option<Tracker>,
    /// The achievements page while it is open.
    pub(crate) achievement_page: Option<crate::achievement_view::Page>,
    /// The achievements first earned in a game: its engine start
    /// (`session_serial`) and their ids (the end screen marks them).
    earned_in_game: (u64, Vec<String>),
    /// The end's disclosures, kept for the end page as they come (NetHack
    /// asks none: `disclose` says yes for the player), until the farewell.
    disclosed: Vec<crate::end_view::Disclosure>,
    farewell_seen: bool,
    /// The client's settings, as the profile keeps them (read at start,
    /// changed on the settings page).
    pub(crate) settings: Settings,
    /// A self-test's tick (`--tick=`, the soak's), over the order speed.
    test_tick: Option<u64>,
    /// The settings page while it is open.
    pub(crate) settings_page: Option<SettingsPage>,
    /// What the player set over this game, until the game has it.
    gameplay_goal: Option<Gameplay>,
    /// The game's options menu driven for the page (`drive_options`).
    options_run: Option<OptionsRun>,
    /// The key being handled is the game's own (a gamepad's, a macro's):
    /// the player's moved keys do not apply to it.
    raw_keys: bool,
    /// The engine's last answers and the requests they answered
    /// (self-tests).
    pub(crate) sent: std::collections::VecDeque<(Prompt, Reply)>,
    /// The credits and licences page while it is open, and the settings
    /// page's place it was opened from (Back returns there).
    pub(crate) credits_page: Option<crate::credits_page::CreditsPage>,
    credits_return: Option<(settings_page::Section, usize)>,
    /// The device's own graphics preset (until the player chooses).
    device_quality: nh_world::Quality,
    /// The hero's gender as the map draws the hero, told to the engine's
    /// translator (the words that agree with the hero).
    pub(crate) hero_female: Option<bool>,
    /// The prologue playing (over a new game, or replayed over the title),
    /// whether this game asked the engine for it, and after it the black
    /// lifting off the game (seconds into that).
    pub(crate) prologue: Option<PrologueShown>,
    prologue_due: bool,
    prologue_fade: Option<f64>,
    /// The creation form's role and gender for the prologue (the engine
    /// tells them, random ones too, as the game starts).
    prologue_choice: Option<(String, bool)>,
    /// A self-test's game plays the prologue (self-tests have none else).
    pub(crate) prologue_forced: bool,
    /// A new game not yet handed on to what follows the prologue
    /// (`prologue_done`): its start, prologue or none.
    new_game: bool,
    /// The session whose hero went to the profile and the title scene.
    last_hero_of: Option<u64>,
    /// The session whose prologue's hero is still to go to the profile
    /// beside its text, once the engine has told who they are.
    legacy_hero_of: Option<u64>,
    /// A reply could not be written: the engine is gone.
    link_error: Option<String>,
    close_deadline: Option<Instant>,
    quitting: bool,
    selftest: Option<SelfTest>,
    /// RENETHACK_FRAME_LOG: a line a frame (`frame_log.rs`).
    frame_log: Option<FrameLog>,
    /// RENETHACK_VARIANTS: what is drawn for the first time (`census.rs`),
    /// and the frames walked so far.
    census: Option<Census>,
    census_frames: u64,
    /// The next engine's seed; else RENETHACK_SEED (else the self-test's).
    pub(crate) seed: Option<u64>,
    /// The engine's output read this many events a frame at most
    /// (`--engine-pieces`, self-tests).
    engine_pieces: Option<usize>,
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
    /// Two arrows pressed together: a diagonal step (`nh_world::Chords`).
    chords: nh_world::Chords,
    /// The left stick on the title's pages: the way it leans (a step each
    /// time it leans anew).
    screen_stick: (i32, i32),
    /// The first-steps guide: its cards' occasions, those waiting their
    /// turn, when the last one went (the next waits a moment), the pages
    /// the player has met (the profile's), and whether a self-test asked
    /// for it (the others play without it).
    hint_watch: nh_world::HintWatch,
    hints_waiting: Vec<nh_world::Guide>,
    hint_gone_at: Option<Instant>,
    pub(crate) guide_seen: std::collections::BTreeSet<nh_world::Guide>,
    pub(crate) guide_in_test: bool,
    /// The cards shown, in turn (self-tests: each once).
    pub(crate) hint_log: Vec<nh_world::Guide>,
    /// The one card a self-test stages: other occasions are passed by.
    pub(crate) hint_only: Option<nh_world::Guide>,
    /// Why the last order stopped, shown until then.
    stop_note: Option<(String, Instant)>,
    /// The engine's answer about the cell the player looked at.
    look_tip: Option<LookTip>,
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
    /// What the engine reads after a key that began it, one a request: a
    /// count and its command (`n20s`), the rest of a getobj count and the
    /// letter. Keys answer command prompts, an extended command its
    /// prompt.
    typing: std::collections::VecDeque<Reply>,
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
    /// The gamepad the words naming keys were last made for (None: the
    /// keyboard's): the prompt line, the guide's card and page.
    words_pad: Option<PadKind>,
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
    /// Requests the engine made, every game of the session (the frame log).
    requests: u64,
    /// The map's last sync, milliseconds (the frame log).
    map_ms: f64,
    pub(crate) headless: bool,
    /// The start-up veil (the boot splash's colour), the cue pulsing on
    /// it, and when it began to lift (None: still down).
    veil: Option<Gd<godot::classes::ColorRect>>,
    veil_cue: Option<Gd<godot::classes::TextureRect>>,
    veil_lift: Option<Instant>,
    /// Over the title, whether the scene was drawn (and the engine's words
    /// in) as the veil began to lift (self-tests: not at its cap).
    pub(crate) veil_over_scene: Option<bool>,
    /// Godot's shader cache holds nothing of this build (a first launch):
    /// the veil waits for the whole warm-up, its note and bar saying so.
    first_launch: bool,
    veil_note: Option<VeilNote>,
    /// The cue fading out before the veil lifts: since when, from what
    /// strength.
    cue_out: Option<(Instant, f32)>,
    /// The short veil over graphics that change what Godot builds
    /// pipelines for (`start_rewarm`).
    rewarm: Option<Rewarm>,
    /// The map's warm-up (its rehearsal, under the veil) has begun.
    map_warm: bool,
    /// When the last title frame began, and the pipelines compiled and
    /// video memory used then (RENETHACK_FRAME_STATS).
    title_clock: Option<Instant>,
    title_render: ([i64; 5], f64),
    /// The bar's key labels as drawn: a gamepad's chords, the keys the
    /// player moved onto the slots, or None for the digits.
    bar_labels: Option<Option<Vec<String>>>,
    /// The window's size the self-test asked for (its screenshots must be
    /// that size).
    pub(crate) window_size: Option<Vector2i>,
    /// A volume was set since the profile last kept them.
    volumes_set: bool,
    /// What the interface showed last frame (its sounds are the changes).
    ui_watch: crate::audio::UiWatch,
    /// The music by the scene and the fights that last.
    pub(crate) score: crate::hearing::Score,
    /// What the features' loops were last chosen for (None: no game); and
    /// when (a feature that appears where the hero stands is found within
    /// a second).
    emit_key: Option<EmitKey>,
    emit_at: f64,
}

/// What the features' loops are chosen by: the hero's cell, the level
/// drawn, the hero deaf.
type EmitKey = (Option<(i32, i32)>, u64, bool);

/// The warm-up behind the start-up veil, a step a frame, beside the map's
/// rehearsal: what a game draws for the first time is drawn there, unseen.
/// A step left when the veil has lifted shares no frame with the map's
/// loading if it draws something for the first time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleWarm {
    /// A kind of dialog built a frame.
    Dialogs,
    /// A glyph's textures painted a frame.
    Glyphs,
    /// The inventory panel, its doll on the rehearsal's hero.
    Panel,
    Done,
}

/// The start-up veil: down for at least these frames (the first set up
/// the 3D view's buffers and passes) and until the map's rehearsal has
/// drawn everything for the first time and the dialogs and glyphs are
/// warm, at most these seconds after start; then it fades. A cue pulses
/// on it after a second.
const VEIL_FRAMES: u64 = 8;
/// A later launch's veil waits for the title scene drawn and the engine's
/// words in, at most this long (at 6 s the bare menu stood on grey for
/// 2–9 s more; a Deck is slower than a laptop).
const VEIL_CAP_SECS: f64 = 30.0;
const VEIL_FADE: f32 = 0.25;
const VEIL_CUE_SECS: f64 = 1.0;
/// Seconds the cue takes to fade out, before the veil lifts.
const CUE_OUT: f32 = 0.2;

/// On a first launch the veil waits for the warm-up to end, at most this
/// long (should a load never finish, the title still comes).
const FIRST_LAUNCH_CAP_SECS: f64 = 120.0;

/// A file in Godot's shader cache saying a warm-up of this build has run
/// to its end: the shaders and the variants' pipelines the start compiles
/// are cached from then on. Emptied with the cache (a perf run's first
/// launch), and changed with the game's version (each package's own),
/// the engine or the variants list.
const WARM_MARKER: &str = "user://shader_cache/renethack-warm";

/// What the marker says: this version, this Godot and this variants list.
fn warm_fingerprint() -> String {
    let godot = godot::classes::Engine::singleton()
        .get_version_info()
        .get("string")
        .map(|v| v.to_string())
        .unwrap_or_default();
    format!(
        "{} {godot} {:016x}",
        crate::credits::version(),
        crate::variants::list_hash()
    )
}

/// Godot's shader cache has this build's warm-up in it.
fn shader_cache_warm() -> bool {
    use godot::classes::FileAccess;
    FileAccess::file_exists(WARM_MARKER)
        && FileAccess::get_file_as_string(WARM_MARKER).to_string() == warm_fingerprint()
}

/// The warm-up ran to its end: the next start may lift the veil at its
/// cap.
fn mark_shader_cache_warm() {
    use godot::classes::file_access::ModeFlags;
    use godot::classes::{DirAccess, FileAccess};
    let _ = DirAccess::make_dir_recursive_absolute("user://shader_cache");
    if let Some(mut f) = FileAccess::open(WARM_MARKER, ModeFlags::WRITE) {
        f.store_string(&warm_fingerprint());
    }
}

/// The note on the veil of a first launch: a line and a thin bar under the
/// ankh, the bar filling as the warm-up goes.
struct VeilNote {
    note: Gd<godot::classes::Control>,
    fill: Gd<godot::classes::ColorRect>,
    /// The most the bar has shown (it never goes back).
    shown: f32,
}

/// The bar's width, and its thickness.
const NOTE_BAR: (f32, f32) = (240.0, 3.0);

impl VeilNote {
    fn new(veil: &mut Gd<godot::classes::ColorRect>) -> VeilNote {
        use godot::classes::{ColorRect, Control, control::MouseFilter};
        let mut note = Control::new_alloc();
        note.set_mouse_filter(MouseFilter::IGNORE);
        crate::theme::place(&note, [0.5, 0.5, 0.5, 0.5], [-200.0, 52.0, 200.0, 100.0]);
        let mut line = crate::theme::styled_label(
            &crate::tr!("veil-first-launch"),
            crate::theme::Face::Body,
            18,
            crate::theme::GOLD,
        );
        line.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        crate::theme::place(&line, [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 28.0]);
        note.add_child(&line);
        let (w, h) = NOTE_BAR;
        let mut track = ColorRect::new_alloc();
        track.set_color(Color::from_rgba(1.0, 1.0, 1.0, 0.12));
        crate::theme::place(
            &track,
            [0.5, 0.0, 0.5, 0.0],
            [-w / 2.0, 38.0, w / 2.0, 38.0 + h],
        );
        let mut fill = ColorRect::new_alloc();
        fill.set_color(crate::theme::GOLD);
        crate::theme::place(&fill, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]);
        track.add_child(&fill);
        note.add_child(&track);
        note.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        veil.add_child(&note);
        VeilNote {
            note,
            fill,
            shown: 0.0,
        }
    }

    /// A frame of it: in after the cue's second, the bar at `progress`.
    fn show(&mut self, secs: f64, progress: f32) {
        let a = ((secs - VEIL_CUE_SECS) / 0.6).clamp(0.0, 1.0) as f32;
        self.note.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
        self.shown = self.shown.max(progress.clamp(0.0, 1.0));
        self.fill
            .set_size(Vector2::new(NOTE_BAR.0 * self.shown, NOTE_BAR.1));
    }

    /// Fading out with the cue (`k` 1 to 0).
    fn fade(&mut self, k: f32) {
        self.note
            .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, k.clamp(0.0, 1.0)));
    }
}

/// The veil's cap: RENETHACK_VEIL_CAP's seconds instead (a census of what
/// the rehearsal draws slows it past the cap).
fn veil_cap() -> f64 {
    static CAP: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("RENETHACK_VEIL_CAP")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(VEIL_CAP_SECS)
    })
}

/// The short veil over a change of the graphics that changes what Godot
/// builds pipelines for: what is on screen waits for its own a frame or
/// two, and the map makes its variants again behind it (`variants.rs`). It
/// stays this many frames at least and at most this long, then fades.
struct Rewarm {
    layer: Gd<CanvasLayer>,
    veil: Gd<godot::classes::ColorRect>,
    since: Instant,
    frames: u32,
    lift: Option<Instant>,
}
const REWARM_FRAMES: u32 = 4;
const REWARM_CAP_SECS: f64 = 2.0;
/// A frame slower than this still waits for pipelines.
const REWARM_SETTLED: f64 = 0.05;

/// What of the graphics Godot builds pipelines for: the MSAA (their
/// framebuffers), the passes ambient occlusion and indirect light add (the
/// depth pass's normals), the upscaler that wants motion vectors.
fn pipeline_settings(g: &nh_world::Graphics) -> (nh_world::Msaa, bool, bool, bool) {
    use nh_world::{Level, Upscale};
    (
        g.msaa,
        g.ssao != Level::Off,
        g.ssil != Level::Off,
        g.upscale == Upscale::Fsr2,
    )
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
    /// The tooltip gives way to the engine's answer about the same cell.
    quiet: bool,
}

/// The engine's answer about the cell the player looked at (`;`), shown
/// over the cell until the player's next answer to the engine, or a while.
struct LookTip {
    at: (i32, i32),
    text: String,
    since: Instant,
}

/// How long the answer about a cell stays over it, unanswered.
const LOOK_TIP_SECS: f64 = 20.0;

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
            achievements: None,
            hero_female: None,
            prologue: None,
            prologue_due: false,
            prologue_fade: None,
            prologue_choice: None,
            prologue_forced: false,
            new_game: false,
            last_hero_of: None,
            legacy_hero_of: None,
            achievement_page: None,
            earned_in_game: (0, Vec::new()),
            disclosed: Vec::new(),
            farewell_seen: false,
            settings: Settings::default(),
            test_tick: None,
            settings_page: None,
            gameplay_goal: None,
            options_run: None,
            raw_keys: false,
            sent: std::collections::VecDeque::new(),
            credits_page: None,
            credits_return: None,
            device_quality: nh_world::Quality::High,
            link_error: None,
            close_deadline: None,
            quitting: false,
            selftest: None,
            frame_log: FrameLog::from_env(),
            census: Census::from_env(),
            census_frames: 0,
            mouse_pos: None,
            hover: Hover::default(),
            seed: None,
            engine_pieces: None,
            debug_mode: false,
            faults: Vec::new(),
            failure: None,
            playground_lock: None,
            driver: TickDriver::new(),
            last_action: None,
            count: CountEntry::default(),
            held: None,
            chords: nh_world::Chords::new(),
            screen_stick: (0, 0),
            words_pad: None,
            hint_watch: nh_world::HintWatch::new(),
            hints_waiting: Vec::new(),
            hint_gone_at: None,
            guide_seen: std::collections::BTreeSet::new(),
            guide_in_test: false,
            hint_log: Vec::new(),
            hint_only: None,
            stop_note: None,
            look_tip: None,
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
            bar_labels: None,
            prof: None,
            title_warm: TitleWarm::Dialogs,
            warm_did: "-",
            warm_heavy: false,
            title_frames: 0,
            frames: 0,
            requests: 0,
            map_ms: 0.0,
            headless: false,
            veil: None,
            veil_cue: None,
            first_launch: false,
            veil_note: None,
            veil_lift: None,
            veil_over_scene: None,
            cue_out: None,
            rewarm: None,
            map_warm: false,
            title_clock: None,
            title_render: ([0; 5], 0.0),
            window_size: None,
            volumes_set: false,
            ui_watch: crate::audio::UiWatch::default(),
            score: crate::hearing::Score::default(),
            emit_key: None,
            emit_at: f64::NEG_INFINITY,
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
        pad_ui_actions();
        // the interface's language before anything shows words
        i18n::set_lang(startup_lang(&args));
        // the engine's texts in the player's language: nh-i18n's translator
        // (loaded on a thread; the English shows until it is ready)
        i18n::set_engine_text(Some(Box::new(crate::engine_text::EngineTranslator::new())));
        self.settings = startup_settings(&args);
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
            crate::video::apply_display(&mut window, &self.settings.display, false);
            crate::theme::apply_scaling(window, self.settings.ui_scale);
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
            // over the dialogs (the same layer, later), under the hints
            ("HelpLayer", 3),
            ("PadLayer", 4),
            ("ScreenLayer", 5),
            // the prologue over all of it
            ("PrologueLayer", 6),
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
        // while the map's rehearsal draws behind it: an ankh pulses
        let mut cue = godot::classes::TextureRect::new_alloc();
        cue.set_texture(&icons::glyph_icon(icons::Glyph::Ankh));
        cue.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        cue.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        cue.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        crate::theme::place(&cue, [0.5, 0.5, 0.5, 0.5], [-36.0, -36.0, 36.0, 36.0]);
        cue.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        cue.set_self_modulate(crate::theme::GOLD);
        veil.add_child(&cue);
        self.headless = godot::classes::DisplayServer::singleton().get_name() == "headless";
        // a first launch: Godot compiles the shaders behind the veil for
        // a while, and a line and a bar under the ankh say so
        self.first_launch = !self.headless && !shader_cache_warm();
        if self.first_launch {
            self.veil_note = Some(VeilNote::new(&mut veil));
        }
        veil_layer.add_child(&veil);
        self.base_mut().add_child(&veil_layer);
        self.veil = Some(veil);
        self.veil_cue = Some(cue);
        // the toast over the HUD, under the panels and dialogs
        let mut toast_layer = CanvasLayer::new_alloc();
        toast_layer.set_name("ToastLayer");
        toast_layer.set_layer(1);
        self.base_mut().add_child(&toast_layer);
        // the guide's hint card, beside the HUD; "Your quest" over the
        // game, under the pages
        let mut hint_layer = CanvasLayer::new_alloc();
        hint_layer.set_name("HintLayer");
        hint_layer.set_layer(1);
        self.base_mut().add_child(&hint_layer);
        let mut quest_layer = CanvasLayer::new_alloc();
        quest_layer.set_name("QuestLayer");
        quest_layer.set_layer(4);
        self.base_mut().add_child(&quest_layer);
        // the game's menu, over the game like the guide's page
        let mut game_menu_layer = CanvasLayer::new_alloc();
        game_menu_layer.set_name("GameMenuLayer");
        game_menu_layer.set_layer(4);
        self.base_mut().add_child(&game_menu_layer);
        // the tips, over the pages and the prologue, under the veil
        let mut tip_layer = CanvasLayer::new_alloc();
        tip_layer.set_name("TipLayer");
        tip_layer.set_layer(7);
        self.base_mut().add_child(&tip_layer);
        // the voices and the ears, in the map's 3D world
        crate::audio::ensure_buses();
        let mut audio_root = Node3D::new_alloc();
        audio_root.set_name("Audio");
        self.base_mut().add_child(&audio_root);
        let queue = self.queue.clone();
        let mut layers = layers.into_iter();
        let mut next = || layers.next().expect("seven layers");
        self.ui = Some(Ui {
            map: MapView::new(map_root),
            hud: Hud::new(next(), queue.clone()),
            inventory: InventoryPanel::new(next(), queue.clone()),
            dialogs: Dialogs::new(next(), queue.clone()),
            help: crate::help_panel::HelpPanel::new(next(), queue.clone()),
            pad: crate::pad_view::PadView::new(next()),
            screens: Screens::new(next(), queue),
            toast: crate::achievement_view::Toast::new(toast_layer),
            hint: crate::guide_view::HintCard::new(hint_layer, self.queue.clone()),
            quest: crate::guide_view::QuestPage::new(quest_layer, self.queue.clone()),
            game_menu: crate::game_menu::GameMenu::new(game_menu_layer, self.queue.clone()),
            tips: crate::tips::TipHost::new(tip_layer),
            audio: crate::audio::Audio::new(audio_root),
            prologue: crate::prologue_view::PrologueView::new(next()),
        });
        // the character sheet's portrait: the hero the inventory's doll
        // renders
        if let Some(ui) = self.ui.as_mut() {
            let render = ui.inventory.doll_texture();
            ui.dialogs.set_portrait(render);
        }
        self.show_game(false);
        self.device_quality = crate::video::device_quality();
        self.apply_graphics();

        let paths = Paths::resolve(args.playground.as_deref());
        godot_print!(
            "renethack: engine {}, playground {}",
            paths.engine_dir.display(),
            paths.playground.display()
        );
        self.paths = Some(paths.clone());
        let profile = load_profile(&paths.playground);
        self.guide_seen = profile.guide_seen.clone();
        // the pack as the player last chose to see it
        if let Some(ui) = self.ui.as_mut() {
            ui.inventory
                .set_list(profile.pack_list_for(self.ui_state.profile));
        }
        // the sound at the volumes the player left it at
        crate::audio::apply_volumes(&profile.volumes);
        // the title scene shows the hero of the last game played
        if let Some(role) = profile.last_role.as_deref()
            && let Some(ui) = self.ui.as_mut()
        {
            ui.map
                .set_title_hero(role, profile.last_female.unwrap_or(false));
        }
        self.seed = args.seed;
        self.engine_pieces = args.engine_pieces;
        // the soak plays many orders: quickly
        let soak = args.selftest.as_deref() == Some("soak");
        self.set_tick(args.tick.or(soak.then_some(SOAK_TICK_MS)));
        if args.selftest.is_some() {
            self.selftest = SelfTest::new(&args);
        }
        if let Err(e) = paths.check() {
            self.show_failure(
                &tr!("err-engine-not-built"),
                &tr!("err-engine-not-built-details", error = e.to_string()),
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
        if self.frame_log.is_some() {
            let viewport = self.base().get_viewport().map(|v| v.get_viewport_rid());
            if let Some(log) = self.frame_log.as_mut() {
                if let Some(rid) = viewport {
                    log.set_viewport(rid);
                }
                log.begin();
            }
        }
        if self.census.is_some() {
            // what the frame before added: numbered as the frame log's line
            let veil = self.veil_state();
            let root = self.base().get_tree().get_root();
            let frame = self.census_frames;
            self.census_frames += 1;
            if let (Some(c), Some(root)) = (self.census.as_mut(), root) {
                c.scan(root.upcast(), frame, veil);
            }
        }
        let mut parts = frame_log::Parts::default();
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        // input first: Godot delivered this frame's keys before process(), so
        // they belong to the prompt the player saw, or to the typeahead while
        // the engine works; a request read afterwards never gets them (the
        // typeahead policy drops them when it opens a modal question)
        self.frames += 1;
        let stats = std::env::var_os("RENETHACK_FRAME_STATS").is_some();
        let t = Instant::now();
        self.time_title_frame(stats);
        self.lift_veil(stats);
        self.tick_rewarm(delta);
        // the map keeps no frame's budget while nothing of it shows
        let veiled = self.veil.is_some() && self.veil_lift.is_none();
        if let Some(ui) = self.ui.as_mut() {
            ui.map.set_veiled(veiled);
        }
        self.warm_up();
        parts.warm = ms(t);
        self.prof = stats.then(Vec::new);
        let t = Instant::now();
        self.pad_tick();
        // an arrow that waited its moment for another steps alone
        let outs = self.chords.tick(Time::singleton().get_ticks_msec());
        self.chord_out(outs);
        self.drain_ui();
        self.sync_settings_page();
        self.drive_options();
        self.sync_credits_page();
        self.sync_achievements_page();
        self.sync_pad_words();
        parts.input = ms(t);
        self.lap("input", t);
        let t = Instant::now();
        self.pump();
        parts.pump = ms(t);
        self.lap("pump", t);
        let t = Instant::now();
        self.drive();
        parts.rest = ms(t);
        let built = self.ui.as_ref().map_or(0, |ui| ui.map.models_built());
        let t = Instant::now();
        self.map_ms = 0.0;
        // (the prologue's scenes and words are the views' work too; the
        // frame it ends in clears its scene, the game's come up in the next)
        if !self.play_prologue(delta) {
            self.sync_views(delta);
        }
        parts.views = ms(t);
        parts.map = self.map_ms;
        self.lap("views", t);
        let built = self.ui.as_ref().map_or(0, |ui| ui.map.models_built()) - built;
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
        // under the veil the map loads every frame beside the warm-up;
        // after it, dialogs and glyphs still to warm go first and a frame of
        // a first draw is theirs alone (such frames only keep the map's
        // title log a frame each)
        let veiled = self.veil.is_some();
        let map_turn = veiled
            || !matches!(self.title_warm, TitleWarm::Dialogs | TitleWarm::Glyphs)
            || self.state != GameState::Title;
        let t = Instant::now();
        self.sync_creation_preview(delta);
        if let Some(ui) = self.ui.as_mut() {
            if map_turn && (veiled || !self.warm_heavy) {
                ui.map.preload_step();
            } else {
                ui.map.title_tick();
            }
        }
        parts.preload = ms(t);
        let t = Instant::now();
        self.watch_engine();
        self.achieve(delta);
        self.tick_hints(delta);
        self.tick_quest();
        // a block's tip, the mouse resting on it (the title's, the game's)
        let hovered = self
            .base()
            .get_viewport()
            .and_then(|v| v.gui_get_hovered_control());
        if let Some(ui) = self.ui.as_mut() {
            ui.tips.tick(hovered, now_secs());
        }
        parts.rest += ms(t);
        let t = Instant::now();
        self.update_audio(delta);
        parts.audio = ms(t);
        let t = Instant::now();
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
        parts.test = ms(t);
        if self.frame_log.is_some() {
            let firsts = self
                .ui
                .as_mut()
                .map(|ui| ui.map.take_firsts())
                .unwrap_or_default();
            let context = frame_log::Context {
                state: format!("{:?}", self.state),
                veil: self.veil_state(),
                level: self.world.map.generation(),
                depth: self.world.depth().map_or(0, i64::from),
                branch: format!("{:?}", self.world.branch()),
                built,
                requests: self.requests,
                firsts: firsts.join(" "),
                step: self
                    .selftest
                    .as_ref()
                    .map(SelfTest::step)
                    .unwrap_or_default(),
            };
            if let Some(log) = self.frame_log.as_mut() {
                log.end(parts, context);
            }
        }
    }

    /// Keys the open dialog or the map own; a text field gets everything
    /// else (only its `text_submitted` answers).
    /// Mouse motion is only recorded here, never handled: the hover needs
    /// the position over the HUD panels too, which `unhandled_input` never
    /// sees.
    fn input(&mut self, event: Gd<InputEvent>) {
        self.hear_click(&event);
        let event = match event.try_cast::<InputEventMouseMotion>() {
            Ok(motion) => {
                self.record_mouse(motion.get_position());
                return;
            }
            Err(event) => event,
        };
        if self.prologue.is_some() {
            self.prologue_input(&event);
            self.handled();
            return;
        }
        // the achievements and settings pages take keys and the gamepad on
        // the title too
        let page = self.achievements_open() || self.settings_open() || self.credits_open();
        if self.state != GameState::Playing && !page {
            // the title, the creation form, the end, an error: a focus the
            // keys and the gamepad move
            if self.screen_input(&event) {
                self.handled();
            }
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
        let text = self.ui.as_ref().is_some_and(|ui| {
            ui.dialogs.wants_text() || ui.inventory.wants_text() || ui.help.wants_text()
        });
        if !key.is_pressed() {
            // letting go ends holding it down (text fields keep theirs)
            if !text && let Some(k) = key_release(&key) {
                let outs = self.chords.release(k);
                self.chord_out(outs);
            }
            return;
        }
        let Some(k) = key_input(&key, text) else {
            return;
        };
        // the help's search lets the F-keys by: its own key closes it
        let help_search = self.ui.as_ref().is_some_and(|ui| ui.help.wants_text());
        let owned = matches!(
            k.key,
            Key::Escape | Key::Tab | Key::Up | Key::Down | Key::PageUp | Key::PageDown
        ) || (help_search && matches!(k.key, Key::F(_)));
        if text && !owned {
            return;
        }
        // on_key knows where the player's own keys apply (and a page over
        // everything has every key: F1 opens no help under it); two arrows
        // together where a direction is awaited are a diagonal step
        let steps = self.arrows_step();
        let outs = self
            .chords
            .press(k, Time::singleton().get_ticks_msec(), steps);
        self.chord_out(outs);
        self.handled();
    }

    /// The mouse over the map: whatever no panel or dialog took.
    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.state != GameState::Playing || self.prologue.is_some() {
            return;
        }
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if ui.dialogs.is_open() || ui.help.is_open() {
            return;
        }
        // the message history is over the map: a click beside it puts it
        // away, and does nothing on the map
        if ui.hud.full_log_open() {
            if let Ok(b) = event.try_cast::<InputEventMouseButton>()
                && b.is_pressed()
                && b.get_button_index() == MouseButton::LEFT
            {
                push(&self.queue, UiEvent::ToggleFullLog);
                self.handled();
            }
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

/// Godot's own lists (an OptionButton's popup, which takes the input
/// before the game sees it) take a gamepad's A and B as Enter and Esc:
/// Godot's `ui_accept` and `ui_cancel` have no pad buttons. The game
/// handles every pad button itself, so nothing else sees them.
fn pad_ui_actions() {
    let mut map = InputMap::singleton();
    for (action, button) in [("ui_accept", JoyButton::A), ("ui_cancel", JoyButton::B)] {
        let mut ev = InputEventJoypadButton::new_gd();
        ev.set_device(-1);
        ev.set_button_index(button);
        if !map.action_has_event(action, &ev) {
            map.action_add_event(action, &ev);
        }
    }
}

impl RenethackGame {
    /// A window or a menu of the end's disclosures, as the end page shows
    /// it: what the engine shows after the game is over, before the
    /// farewell ("Goodbye Hero the Valkyrie...").
    fn disclosure(&mut self, prompt: &Prompt) -> Option<crate::end_view::Disclosure> {
        if self.farewell_seen || !self.world.progress().is_some_and(|p| p.gameover) {
            return None;
        }
        let bold = |attr: i32| attr & nh_world::ATR_BOLD != 0;
        match prompt {
            Prompt::Show { lines, .. }
                if lines.iter().any(|l| crate::end_view::farewell(&l.text)) =>
            {
                self.farewell_seen = true;
                None
            }
            Prompt::Show { lines, .. } => {
                // a window the client lays out: in its layout's words
                let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
                let laid = crate::dialogs::layout_words(&texts, self.catalog.as_deref());
                Some(match laid {
                    Some((heading, words)) => crate::end_view::Disclosure {
                        lines: words,
                        items: false,
                        laid_out: Some(heading),
                    },
                    None => crate::end_view::Disclosure {
                        lines: (lines.iter())
                            .map(|l| (l.text.clone(), bold(l.attr)))
                            .collect(),
                        items: false,
                        laid_out: None,
                    },
                })
            }
            Prompt::Menu { items, .. } => Some(crate::end_view::Disclosure {
                lines: (items.iter())
                    .filter_map(|i| i.str.clone().map(|t| (t, !i.selectable)))
                    .collect(),
                items: true,
                laid_out: None,
            }),
            _ => None,
        }
    }

    /// A button pressed, by the mouse or the accept key: its click.
    fn hear_click(&mut self, event: &Gd<InputEvent>) {
        if !event.is_pressed() || event.is_echo() {
            return;
        }
        let mouse = event
            .clone()
            .try_cast::<InputEventMouseButton>()
            .is_ok_and(|b| b.get_button_index() == MouseButton::LEFT);
        if !mouse && !event.is_action_pressed("ui_accept") {
            return;
        }
        let Some(vp) = self.base().get_viewport() else {
            return;
        };
        let control = if mouse {
            vp.gui_get_hovered_control()
        } else {
            vp.gui_get_focus_owner()
        };
        let button = control.and_then(|c| c.try_cast::<godot::classes::BaseButton>().ok());
        if button.is_some_and(|b| !b.is_disabled())
            && let Some(ui) = self.ui.as_mut()
        {
            ui.audio.ui("ui.click");
        }
    }

    /// The sound's frame: the interface's changes, the ears at the hero
    /// (else where the camera is), the sounds waiting for their moment.
    fn update_audio(&mut self, delta: f64) {
        let Some(vp) = self.base().get_viewport() else {
            return;
        };
        let camera = vp.get_camera_3d().map(|c| c.get_global_transform());
        let hovered = vp
            .gui_get_hovered_control()
            .and_then(|c| c.try_cast::<godot::classes::BaseButton>().ok())
            .filter(|b| !b.is_disabled())
            .map(|b| b.instance_id());
        let playing = self.state == GameState::Playing;
        // the warm-up opens panels and dialogs unseen: not heard either
        let unseen = self.veiled() || (self.state == GameState::Title && !self.warmed_up());
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        let shown = crate::audio::Shown {
            inventory: ui.inventory.is_open(),
            help: ui.help.is_open(),
            dialog: ui.dialogs.is_open(),
            screen: ui.screens.current(),
            hovered,
        };
        if unseen {
            self.ui_watch = crate::audio::UiWatch::default();
        } else {
            for cue in self.ui_watch.cues(shown) {
                ui.audio.ui(cue);
            }
        }
        // the map's own sounds (the rehearsal behind the title is unheard)
        // and the messages', the world muffled for a deaf hero
        for sound in ui.map.take_sounds() {
            if playing {
                ui.audio.play(sound);
            }
        }
        if playing && let Some(cat) = self.catalog.as_deref() {
            use crate::cues::Place;
            let at = |(x, y): (i32, i32)| Vector3::new(x as f32, 0.8, y as f32);
            let world = &self.world;
            let ray = ui.map.last_ray_cell();
            let place = |p: Place, text: &str| match p {
                Place::Hero => world.hero().map(at),
                Place::Door => crate::hearing::door_beside(world, cat)
                    .or(world.hero())
                    .map(at),
                Place::Ray => ray.map(at),
                Place::Named => crate::hearing::named(world, cat, text).map(at),
                Place::None => None,
            };
            let female = world
                .map
                .hero_form()
                .is_some_and(|g| g.flags & nh_protocol::mg::FEMALE != 0);
            ui.audio.hear_log(&world.log, &place, female);
            let senses = crate::hearing::Senses::of(&self.world, cat);
            ui.audio.set_muffled(senses.deaf);
            // the features near the hero, chosen again when they move (or
            // the level or their hearing changes), else once a second
            let key = Some((world.hero(), world.map.generation(), senses.deaf));
            let now = Time::singleton().get_ticks_msec() as f64 / 1000.0;
            if self.emit_key != key || now - self.emit_at >= 1.0 {
                self.emit_key = key;
                self.emit_at = now;
                let found = if senses.deaf {
                    Vec::new()
                } else {
                    crate::hearing::emitters(world, cat)
                };
                let wanted: Vec<_> = found.iter().map(|&(cue, c)| (cue, c, at(c))).collect();
                ui.audio.set_emitters(&wanted);
            }
        } else {
            ui.audio.set_muffled(false);
            if self.emit_key.take().is_some() {
                ui.audio.set_emitters(&[]);
            }
        }
        // the loops: the title's or the branch's ambience
        use crate::hearing::Scene;
        let scene = match self.state {
            _ if self.prologue.is_some() => Scene::Title,
            GameState::Title | GameState::Creating => Scene::Title,
            GameState::Playing | GameState::Closing => Scene::Playing,
            GameState::Ended | GameState::Failed => Scene::Over,
        };
        let branch = self.world.branch();
        ui.audio
            .set_ambience(&crate::hearing::ambience(scene, branch));
        let combat = self.driver.mode() == Mode::Combat;
        let now = Time::singleton().get_ticks_msec() as f64 / 1000.0;
        let music = self.score.music(now, scene, branch, combat);
        ui.audio.set_music(&music);
        let hero = ui.map.hero_stands().filter(|_| playing);
        let ears = camera.map(|t| match hero {
            Some((at, _)) => Transform3D::new(
                t.basis,
                at + Vector3::new(0.0, crate::audio::EAR_HEIGHT, 0.0),
            ),
            None => t,
        });
        ui.audio.update(delta, ears);
    }

    /// A motion outside the window (a drag holds the pointer) hovers nothing.
    fn record_mouse(&mut self, pos: Vector2) {
        let inside = self
            .base()
            .get_viewport()
            .is_some_and(|vp| vp.get_visible_rect().contains_point(pos));
        self.mouse_pos = inside.then_some(pos);
    }

    /// Behind the start-up veil, beside the map's rehearsal (started on the
    /// first frames), what the first frames of a game would otherwise pay
    /// for, a step a frame (see `TitleWarm`): one dialog of each kind drawn
    /// for a few frames, a glyph's textures painted, the inventory panel
    /// and its doll on the rehearsal's hero.
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
        // the map's rehearsal from the first frames on, under the veil:
        // its first draws are made there, unseen
        if !self.map_warm
            && shown >= 1
            && let Some(cat) = catalog.as_deref()
        {
            self.map_warm = true;
            ui.map.warm_up(cat, None);
            self.warm_did = "map warm-up";
        }
        match self.title_warm {
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
                    self.title_warm = TitleWarm::Panel;
                }
            }
            TitleWarm::Panel => {
                ui.dialogs.warm_tick();
                // the doll renders the rehearsal's hero (without one once
                // the rehearsal is over, or none came for long)
                self.title_frames += 1;
                let hero = ui.map.hero_model().map(|m| m.node.clone());
                if hero.is_none() && !ui.map.preloaded() && self.title_frames <= 900 {
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

    /// The start-up veil, as the frame log writes it: 2 down, 1 lifting,
    /// 0 gone.
    fn veil_state(&self) -> u8 {
        match (&self.veil, self.veil_lift) {
            (None, _) => 0,
            (Some(_), Some(_)) => 1,
            (Some(_), None) => 2,
        }
    }

    /// The start-up veil lifts once the map's rehearsal has drawn
    /// everything for the first time behind it and the dialogs and glyphs
    /// are warm, after its first frames and at the latest at its cap; at
    /// once on another first screen than the title (an error). Its cue
    /// pulses after a second.
    fn lift_veil(&mut self, stats: bool) {
        let Some(mut veil) = self.veil.clone() else {
            return;
        };
        let secs = Time::singleton().get_ticks_msec() as f64 / 1000.0;
        match (self.veil_lift, self.cue_out) {
            (None, None) => {
                let mut shown = 0.0;
                if let Some(cue) = self.veil_cue.as_mut() {
                    let t = secs - VEIL_CUE_SECS;
                    let fade_in = (t / 0.6).clamp(0.0, 1.0);
                    let pulse = 0.45 + 0.25 * (t * std::f64::consts::TAU / 2.4).sin();
                    shown = (fade_in * pulse) as f32;
                    cue.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, shown));
                }
                let title = self.state == GameState::Title;
                let map = self.ui.as_ref().map(|ui| &ui.map);
                // a first launch shows the title only once nothing is left
                // to load or draw for the first time behind it
                // (and the title scene itself drawn, the engine's words in
                // the player's language: the creation form names its roles)
                let scene = map.is_some_and(|m| m.title_drawn()) && i18n::engine_ready();
                let drawn = scene
                    && if self.first_launch {
                        map.is_some_and(|m| m.rehearsal_first_draws_done() && m.preloaded())
                            && self.title_warm == TitleWarm::Done
                    } else {
                        map.is_some_and(|m| m.rehearsal_first_draws_done())
                            && matches!(self.title_warm, TitleWarm::Panel | TitleWarm::Done)
                    };
                let progress = map.map_or(0.0, |m| m.warm_progress());
                if let Some(note) = self.veil_note.as_mut() {
                    note.show(secs, progress);
                }
                let cap = if self.first_launch {
                    FIRST_LAUNCH_CAP_SECS
                } else {
                    veil_cap()
                };
                let capped = secs >= cap;
                if self.frames_shown() < VEIL_FRAMES || (title && !drawn && !capped) {
                    return;
                }
                if title && drawn && self.first_launch {
                    mark_shader_cache_warm();
                }
                if title {
                    self.veil_over_scene = Some(drawn);
                }
                if title && !drawn {
                    godot_warn!(
                        "renethack: the start-up veil lifts at {secs:.1} s, before the warm-up behind it was done ({:?})",
                        self.title_warm
                    );
                }
                // the cue goes first: half faded over the title it would
                // read as part of it
                if shown > 0.01 {
                    self.cue_out = Some((Instant::now(), shown));
                } else {
                    self.start_lift(stats, secs);
                }
            }
            (None, Some((t, from))) => {
                let a = from * (1.0 - t.elapsed().as_secs_f32() / CUE_OUT);
                if let Some(cue) = self.veil_cue.as_mut() {
                    cue.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a.max(0.0)));
                }
                if let Some(note) = self.veil_note.as_mut() {
                    note.fade(a.max(0.0) / from.max(0.01));
                }
                if a <= 0.0 {
                    self.start_lift(stats, secs);
                }
            }
            (Some(t), _) => {
                let a = 1.0 - t.elapsed().as_secs_f32() / VEIL_FADE;
                if a <= 0.0 {
                    if let Some(mut layer) = veil.get_parent() {
                        layer.queue_free();
                    }
                    self.veil = None;
                    self.veil_cue = None;
                } else {
                    veil.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
                }
            }
        }
    }

    /// The short veil down (again) over graphics that change what Godot
    /// builds pipelines for: the start-up veil is enough while it is up.
    fn start_rewarm(&mut self) {
        if self.veil.is_some() || self.headless {
            return;
        }
        if let Some(r) = self.rewarm.as_mut() {
            r.since = Instant::now();
            r.frames = 0;
            r.lift = None;
            r.veil.set_modulate(Color::WHITE);
            return;
        }
        let mut layer = CanvasLayer::new_alloc();
        layer.set_layer(100);
        let mut veil = godot::classes::ColorRect::new_alloc();
        veil.set_color(crate::theme::BG);
        crate::theme::place(&veil, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        layer.add_child(&veil);
        self.base_mut().add_child(&layer);
        self.rewarm = Some(Rewarm {
            layer,
            veil,
            since: Instant::now(),
            frames: 0,
            lift: None,
        });
    }

    /// The short veil lifts once the map has made its variants again and a
    /// frame is drawn in time (at its cap at the latest), then fades.
    fn tick_rewarm(&mut self, delta: f64) {
        let busy = self.ui.as_ref().is_some_and(|ui| ui.map.variants_busy());
        let Some(r) = self.rewarm.as_mut() else {
            return;
        };
        r.frames += 1;
        match r.lift {
            None => {
                let settled = r.frames >= REWARM_FRAMES && !busy && delta < REWARM_SETTLED;
                if settled || r.since.elapsed().as_secs_f64() >= REWARM_CAP_SECS {
                    r.lift = Some(Instant::now());
                }
            }
            Some(t) => {
                let a = 1.0 - t.elapsed().as_secs_f32() / VEIL_FADE;
                if a > 0.0 {
                    r.veil.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
                } else if let Some(mut r) = self.rewarm.take() {
                    r.layer.queue_free();
                }
            }
        }
    }

    /// The short veil over new graphics is down (self-tests).
    pub(crate) fn rewarming(&self) -> bool {
        self.rewarm.is_some()
    }

    /// The start-up veil begins to fade.
    fn start_lift(&mut self, stats: bool, secs: f64) {
        if stats {
            godot_print!("game: the start-up veil lifts {secs:.1} s after start");
        }
        self.veil_lift = Some(Instant::now());
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

    pub(crate) fn playground(&self) -> PathBuf {
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
            if frame || self.engine_pieces.is_some() || Instant::now() >= deadline {
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
            // the options menu's note of a toggle stays out of the log:
            // the settings page shows the change
            SessionEvent::Win(WinCall::Putstr { text, .. })
                if self.options_run.is_some() && nh_world::is_toggle_note(&text) => {}
            SessionEvent::Win(w) => self.world.apply(&w),
            SessionEvent::Request { id, req } => {
                self.requests += 1;
                let prompt = self.world.on_request(&req);
                // the turn resolved: the hero's use is shown
                if let Some(u) = self.uses.on_prompt(&prompt, &self.world)
                    && let (Some(ui), Some(cat)) = (self.ui.as_mut(), self.catalog.as_deref())
                {
                    ui.map.show_use(&u, cat, &self.world);
                }
                if prompt == Prompt::AutoAck {
                    self.send(id, &prompt, Reply::Ack);
                } else if let Some(legacy) = legacy_of(&prompt) {
                    // NetHack's legacy, asked for the prologue: the engine
                    // goes on at once, the prologue tells it meanwhile
                    self.send(id, &prompt, Reply::Ack);
                    if std::mem::take(&mut self.prologue_due) {
                        self.start_prologue(legacy, false);
                    }
                } else {
                    let first_command = prompt == Prompt::Command;
                    self.open_prompt(id, prompt);
                    // a new game up with no prologue (none due, or the
                    // engine told none): handed on as it appears
                    if first_command && self.new_game && self.prologue.is_none() {
                        self.prologue_due = false;
                        self.new_game = false;
                        self.prologue_done();
                    }
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
        // the game over: its disclosures kept for the end page, each one
        // let go at once, until the farewell's window
        if let Some(d) = self.disclosure(&prompt) {
            self.disclosed.push(d);
            self.pending = Some((id, prompt.clone()));
            self.reply(prompt.escape_reply());
            return;
        }
        // the options menu driven for the settings page answers all it
        // leads to; its end, a command prompt, is the player's again
        if let Some(run) = self.options_run.as_mut() {
            match run.on_prompt(&prompt) {
                RunStep::Reply(r) => {
                    self.pending = Some((id, prompt));
                    self.reply(r);
                    return;
                }
                RunStep::Done => {
                    let gave = run.goal().clone();
                    self.options_run = None;
                    self.options_given(&gave);
                }
            }
        }
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
        // a count and its command, the rest of a getobj count and the
        // letter: read one key at a time by get_count
        if let Some(next) = self.typing.front() {
            let fits = match next {
                Reply::ExtCmd(_) => prompt == Prompt::ExtCmd,
                _ => matches!(prompt, Prompt::Command | Prompt::Key),
            };
            if fits && let Some(r) = self.typing.pop_front() {
                self.pending = Some((id, prompt));
                self.reply(r);
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
            if prompt == Prompt::Command && !self.world.getpos {
                // the tick's moment: the mode, and whether the order goes on
                if let Some(mode) = self.driver.observe(&self.world, &cat) {
                    self.ui_mut().hud.set_mode(mode == Mode::Combat, true);
                }
                self.check_hints();
                // the player's own NetHack options as the game has them
                // (what `O` set): kept for the next games
                if let Some(rc) = self.world.options_rc.take() {
                    self.keep_nethack_options(&rc);
                }
            } else if self.driver.is_active() {
                // a question the order did not expect: the player answers
                self.driver.on_question();
            }
        }
        let keeps = prompt.keeps_typeahead();
        if !keeps {
            self.typeahead.clear();
        }
        let catalog = self.catalog.clone();
        let pad = self.pad.active.then_some(self.pad.kind);
        let line = match prompt {
            Prompt::Command => getpos_line(&self.world, pad),
            _ => prompt_line(&prompt, pad),
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
                // built for the input as it is now (a gamepad's focus, its
                // words)
                ui.dialogs.set_pad(pad.is_some());
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

    /// Answer the pending request (a self-test stages with it what no
    /// player's input stands for: debug mode's commands).
    pub(crate) fn reply(&mut self, reply: Reply) {
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
                // what the engine was answered, the last few (self-tests)
                if self.sent.len() >= 32 {
                    self.sent.pop_front();
                }
                self.sent.push_back((prompt.clone(), reply.clone()));
                self.uses.on_reply(prompt, &reply, &self.world);
                if *prompt == Prompt::Command {
                    self.world.note_getpos_answer(&reply);
                }
                // the answer about a cell stays until the player goes on
                if *prompt != Prompt::AutoAck {
                    self.look_tip = None;
                }
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
            self.link_error = Some(tr!("err-engine-hangs", secs = silent.as_secs()));
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
            let mut text = tr!("err-engine-failed", what = what.clone());
            if let Some(n) = &save {
                text.push('\n');
                text.push_str(&tr!("err-game-saved-as", name = n));
            }
            self.show_failure(&text, details.trim(), save.as_deref());
        } else if let Some(n) = saved(&name) {
            self.show_title(Some(tr!("title-game-saved", name = n)));
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
            let code = ending.code.map_or(tr!("err-killed-by-signal"), |c| {
                tr!("err-exit-code", code = c)
            });
            let details = [notes.unwrap_or_default(), ending.stderr_tail.clone()].join("\n\n");
            self.show_failure(
                &tr!("err-engine-stopped", code = code),
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
                    &tr!("err-already-running"),
                    &tr!(
                        "err-already-running-details",
                        dir = paths.playground.display().to_string()
                    ),
                    None,
                );
                return;
            }
            Err(e) => {
                self.show_failure(
                    &tr!("err-playground"),
                    &format!("{}: {e}", paths.playground.display()),
                    None,
                );
                return;
            }
        }
        let notice = self.recover_interrupted();
        if self.achievements.is_none() {
            let (backend, why) = crate::steam::backend();
            if let Some(why) = why {
                godot_print!("renethack: achievements without {why}");
            }
            self.achievements = Some(Tracker::start(
                Achievements::built_in(),
                Some(paths.achievements()),
                backend,
            ));
        }
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
                    self.show_failure(&tr!("err-engine-does-not-start"), &e.to_string(), None);
                    return;
                }
            }
        }
        self.show_title(notice);
    }

    /// What the last progress notice newly earns, kept, sent on and told
    /// (the toast, `dt` seconds on); the backend's work.
    fn achieve(&mut self, dt: f64) {
        let Some(t) = self.achievements.as_mut() else {
            return;
        };
        let character = self.name.as_deref().unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        for a in t.update(&self.world, self.session_serial, character, now) {
            godot_print!("renethack: achievement {} ({})", a.name_en, a.steam);
            if self.earned_in_game.0 != self.session_serial {
                self.earned_in_game = (self.session_serial, Vec::new());
            }
            self.earned_in_game.1.push(a.id);
        }
        if let Some(e) = t.take_trouble() {
            godot_warn!("renethack: achievements: {e}");
        }
        t.tick();
        let fresh: Vec<_> = t
            .take_fresh()
            .iter()
            .filter_map(|id| t.achievements().get(id).cloned())
            .collect();
        let playing = self.state == GameState::Playing;
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        for a in fresh {
            ui.toast.push(a);
        }
        // anything over the map holds the toast back
        let blocked = !playing
            || ui.dialogs.is_open()
            || ui.inventory.is_open()
            || ui.help.is_open()
            || ui.hud.full_log_open()
            || ui.screens.current().is_some();
        ui.toast.tick(dt, blocked);
    }

    /// Run `recover` on every interrupted game; what happened, if anything.
    fn recover_interrupted(&mut self) -> Option<String> {
        // another client's playground is not ours to repair
        self.playground_lock.as_ref()?;
        let paths = self.paths.clone()?;
        let bases = match interrupted_games(&paths.playground) {
            Ok(b) => b,
            Err(e) => return Some(tr!("recover-cannot-look", error = e.to_string())),
        };
        let notes: Vec<String> = bases
            .iter()
            .map(
                |base| match recover_game(&paths.recover(), &paths.playground, base) {
                    Ok(Recovered::Saved(n)) => tr!("recover-saved", name = n),
                    Ok(Recovered::Lost) => tr!("recover-lost", base = base),
                    Err(e) => tr!("recover-failed", base = base, error = e.to_string()),
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
        // a page has the keys, over the title or the game
        if self.achievements_open() {
            self.achievements_key(k);
            return;
        }
        if self.settings_open() {
            self.settings_key(k);
            return;
        }
        if self.credits_open() {
            self.credits_key(k);
            return;
        }
        if self.state != GameState::Playing {
            return;
        }
        // "Your quest" has the keys while it is up: the game waits
        if self.ui.as_ref().is_some_and(|ui| ui.quest.is_open()) {
            self.quest_key(k);
            return;
        }
        // so has the game's menu
        if self.ui.as_ref().is_some_and(|ui| ui.game_menu.is_open()) {
            self.game_menu_key(k);
            return;
        }
        // the client's own keys as the player moved them, where commands
        // are awaited (the engine at work or at its prompt, nothing open
        // that takes letters), and a key that is never data (F-keys, Ctrl)
        // anywhere; elsewhere a key is what it is
        let dialog = self.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open());
        let world = !dialog
            && !self.world.getpos
            && matches!(self.pending, None | Some((_, Prompt::Command)));
        let never_data = matches!(k.key, Key::F(_)) || k.mods.ctrl;
        let client = if world || never_data {
            self.player_key(k).and_then(|m| client_key(&m))
        } else {
            client_key(&k)
        };
        // the help has them while it is open (F1 closes it too)
        if self.ui.as_ref().is_some_and(|ui| ui.help.is_open()) {
            if client == Some(UiEvent::ToggleHelp) {
                self.on_ui_event(UiEvent::ToggleHelp);
            } else {
                self.ui_mut().help.key(&k);
            }
            return;
        }
        // so has the message history over the map: the arrows and the
        // pages scroll it, its own key, Esc (B) and Enter (A) close it;
        // no key acts on the map under it
        if !dialog && self.ui.as_ref().is_some_and(|ui| ui.hud.full_log_open()) {
            let close = client == Some(UiEvent::ToggleFullLog)
                || matches!(
                    k.key,
                    Key::Escape | Key::Enter | Key::KeypadEnter | Key::Char(' ')
                );
            if close && !k.echo {
                self.on_ui_event(UiEvent::ToggleFullLog);
            } else {
                self.ui_mut().hud.full_log_key(k.key);
            }
            return;
        }
        // typed ahead or pushed by a self-test as a plain key
        if let Some(ev) = client {
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
        let Some((id, prompt)) = &self.pending else {
            // a gamepad's key waits as the key the player presses for it:
            // it gives the game its command again when it is read
            let queued = if self.raw_keys {
                self.keymap()
                    .key_for(k)
                    .map(|p| KeyInput { echo: k.echo, ..p })
            } else {
                Some(k)
            };
            if let Some(q) = queued {
                self.typeahead.push(q);
            }
            return;
        };
        let id = *id;
        let command = *prompt == Prompt::Command && !self.world.getpos;
        // the open panel first (not under a dialog of its own), at the
        // command prompt and for a request it answers itself; a request it
        // does not answer (the engine's pause on the map, a direction,
        // getpos) takes the key: Space ends a --More-- with the panel open.
        // A count being typed keeps its Esc.
        let panel_asks = self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.inventory.request() == Some(id));
        let typing_count = command && self.count.shown().is_some();
        let dialog = self.ui.as_ref().is_some_and(|ui| ui.dialogs.is_open());
        if !dialog && (command || panel_asks) && !(typing_count && k.key == Key::Escape) {
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
        // Esc (a gamepad's B) puts the guide's card away first, and so do
        // Space and Enter: the prologue and "Your quest" taught "any key:
        // next", and the engine took them for commands it does not know
        if command
            && matches!(
                k.key,
                Key::Escape | Key::Enter | Key::KeypadEnter | Key::Char(' ')
            )
            && k.mods == nh_world::Mods::default()
            && !k.echo
            && self.count.shown().is_none()
            && self.ui.as_ref().is_some_and(|ui| ui.hint.shown().is_some())
        {
            self.ui_mut().hint.dismiss();
            return;
        }
        // Esc (B) at the command prompt, nothing else up: the game's menu
        if command && k.key == Key::Escape && !k.echo && self.count.shown().is_none() {
            self.open_game_menu();
            return;
        }
        if command {
            self.command_key(k);
            return;
        }
        // where a direction is asked (getdir, getpos's cursor), a step's
        // key the player moved gives that step
        let direction = matches!(
            prompt,
            Prompt::Command
                | Prompt::FreeKey {
                    directions: true,
                    ..
                }
        );
        let k = if direction {
            match self.direction_key(k) {
                Some(k) => k,
                None => return,
            }
        } else {
            k
        };
        let Some((_, prompt)) = &self.pending else {
            return;
        };
        let (np, dirs) = (self.world.number_pad, self.world.dirchars.clone());
        let code = |ctx| nethack_key(&k, ctx, np, &dirs);
        // getpos's cursor: Enter picks the cell, as its line says (NetHack
        // picks with . , ; : alone)
        let pick = matches!(k.key, Key::Enter | Key::KeypadEnter) && !k.echo;
        let reply = match prompt {
            Prompt::Command if self.world.getpos && pick => Some(Reply::Key('.' as i32)),
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
            self.dialog_reply(r);
        }
    }

    /// A direction is awaited, or keys wait for the prompt that awaits one
    /// (the engine at work): the command prompt with nothing over the map
    /// that takes arrows (getpos's cursor too), or a direction's question.
    /// Two arrows pressed together there are a diagonal step; elsewhere
    /// (menus, texts, panels, pages) arrows are arrows.
    fn arrows_step(&self) -> bool {
        if self.state != GameState::Playing
            || self.achievements_open()
            || self.settings_open()
            || self.credits_open()
        {
            return false;
        }
        let Some(ui) = self.ui.as_ref() else {
            return false;
        };
        if ui.help.is_open()
            || ui.inventory.is_open()
            || ui.quest.is_open()
            || ui.game_menu.is_open()
        {
            return false;
        }
        match &self.pending {
            Some((
                _,
                Prompt::FreeKey {
                    directions: true, ..
                },
            )) => true,
            None | Some((_, Prompt::Command)) => !ui.dialogs.is_open(),
            _ => false,
        }
    }

    /// The arrows' keys as they come to the game: pressed, let go, or two
    /// arrows' diagonal (the step's own key, as the game's: no key the
    /// player moved stands for it).
    fn chord_out(&self, outs: Vec<nh_world::ChordOut>) {
        use nh_world::ChordOut;
        for o in outs {
            let ev = match o {
                ChordOut::Press(k) => UiEvent::Key(k),
                ChordOut::Release(k) => UiEvent::KeyUp(k),
                ChordOut::Diagonal(k) => UiEvent::GameKey(k),
            };
            push(&self.queue, ev);
        }
    }

    /// A new game has begun (after the prologue, or at its first command
    /// when none plays): the guide's "Your quest", once per player, while
    /// the guide is on. Never for a restored game.
    pub(crate) fn prologue_done(&mut self) {
        if !self.hints_on() || self.guide_seen.contains(&nh_world::Guide::Quest) {
            return;
        }
        self.driver.interrupt(Stop::Panel);
        self.ui_mut().quest.open();
        self.show_quest();
        self.guide_seen.insert(nh_world::Guide::Quest);
        self.update_profile(|p| {
            p.guide_seen.insert(nh_world::Guide::Quest);
        });
    }

    /// "Your quest"'s step shown again, its keys as bound now.
    fn show_quest(&mut self) {
        let Some(step) = self.ui.as_ref().and_then(|ui| ui.quest.step()) else {
            return;
        };
        let words = self.quest_words(step);
        let ui = self.ui_mut();
        let mut blocks = ui.hud.blocks();
        // the XP bar's words: a leader across the bar keeps off them
        blocks.extend(ui.hud.xp_words().map(|r| ("XP words", r)));
        ui.quest.show(&words, &blocks);
    }

    /// "Your quest": a step on or back, or the rest skipped.
    fn quest_input(&mut self, input: crate::guide_view::QuestInput) {
        if self.ui_mut().quest.go(input) {
            self.show_quest();
        }
    }

    /// A key on "Your quest": Esc skips the rest, ← or Backspace goes
    /// back, any other key on (a gamepad's B, the d-pad, A).
    fn quest_key(&mut self, k: KeyInput) {
        use crate::guide_view::QuestInput;
        if k.echo {
            return;
        }
        let input = match k.key {
            Key::Escape => QuestInput::Skip,
            Key::Left | Key::Backspace => QuestInput::Back,
            _ => QuestInput::Next,
        };
        self.quest_input(input);
    }

    /// The game's menu, over the game at its command prompt (orders stop).
    fn open_game_menu(&mut self) {
        self.driver.interrupt(Stop::Panel);
        self.ui_mut().game_menu.open();
    }

    /// A key on the game's menu: ↑↓ ←→ (Tab) choose, Enter or Space takes
    /// the entry, Esc goes back to the game; no other key does anything.
    fn game_menu_key(&mut self, k: KeyInput) {
        use crate::game_menu::GameMenuPick;
        if k.echo && !matches!(k.key, Key::Up | Key::Down) {
            return;
        }
        match k.key {
            Key::Up => self.ui_mut().game_menu.step(-1),
            Key::Down => self.ui_mut().game_menu.step(1),
            Key::Left => self.ui_mut().game_menu.side(-1),
            Key::Right => self.ui_mut().game_menu.side(1),
            Key::Tab => self.ui_mut().game_menu.next(),
            Key::Enter | Key::KeypadEnter | Key::Char(' ') => {
                let pick = self.ui_mut().game_menu.focused();
                self.game_menu_pick(pick);
            }
            Key::Escape => self.game_menu_pick(GameMenuPick::Resume),
            _ => {}
        }
    }

    /// An entry of the game's menu taken: the menu goes, and its deed is
    /// done (saving and giving up are the engine's commands, at the
    /// command prompt, under the engine's own questions; a panel opens as
    /// its command does, whatever keys the player moved).
    fn game_menu_pick(&mut self, pick: crate::game_menu::GameMenuPick) {
        use crate::game_menu::GameMenuPick;
        if !self.ui.as_ref().is_some_and(|ui| ui.game_menu.is_open()) {
            return;
        }
        self.ui_mut().game_menu.close();
        match pick {
            GameMenuPick::Resume => {}
            GameMenuPick::Settings => self.open_settings(),
            GameMenuPick::SaveLeave => {
                self.use_slot(SlotUse::Macro(nh_world::Macro::ext("save")));
            }
            GameMenuPick::GiveUp => {
                self.use_slot(SlotUse::Macro(nh_world::Macro::ext("quit")));
            }
            GameMenuPick::Spells => self.use_ext("showspells"),
            GameMenuPick::Character => self.use_ext("attributes"),
            GameMenuPick::Skills => self.use_ext("enhance"),
            GameMenuPick::Overview => self.use_ext("overview"),
            GameMenuPick::Journal => self.use_ext("chronicle"),
            GameMenuPick::Codex => self.use_ext("known"),
            GameMenuPick::Achievements => self.open_achievements(),
            GameMenuPick::Help => self.push_ui(UiEvent::ToggleHelp),
            GameMenuPick::Commands => {
                self.push_ui(UiEvent::GameKey(KeyInput::plain(Key::Char('#'))));
            }
        }
    }

    /// An extended command, at the command prompt.
    fn use_ext(&mut self, name: &'static str) {
        self.use_slot(SlotUse::Macro(nh_world::Macro::ext(name)));
    }

    /// "Your quest"'s words for `step`, the keys as the player presses them,
    /// or while a gamepad plays its buttons.
    pub(crate) fn quest_words(
        &self,
        step: crate::guide_view::QuestStep,
    ) -> crate::guide_view::QuestWords {
        use crate::guide_view::QuestStep;
        let mut words = self.quest_words_keys(step);
        if !self.pad.active {
            return words;
        }
        let pad = self.pad_args();
        let padded = |key: &str| i18n::tr_with(key, Some(&pad));
        match step {
            QuestStep::Screen => {
                for (block, w) in words.tags.iter_mut() {
                    match *block {
                        "action bar" => *w = padded("quest-tour-bar-pad"),
                        "micro-buttons" => *w = padded("quest-tour-buttons-pad"),
                        _ => {}
                    }
                }
                // the whole level by the stick; no mouse to rest
                words.lines.truncate(2);
                words.lines.push(padded("quest-screen-overview-pad"));
            }
            QuestStep::Moves => {
                words.lines = [
                    "quest-moves-pad-walk",
                    "quest-moves-pad-fight",
                    "quest-moves-pad-cursor",
                    "quest-moves-pad-more",
                ]
                .map(padded)
                .to_vec();
            }
            QuestStep::Goal => {}
        }
        words
    }

    /// "Your quest"'s words for `step`, the keys as the player presses them.
    fn quest_words_keys(
        &self,
        step: crate::guide_view::QuestStep,
    ) -> crate::guide_view::QuestWords {
        use crate::guide_view::{QuestStep, QuestWords};
        let map = self.keymap();
        let bound = |hint: &str| crate::rebind::bound_hint(map, hint);
        let pad = self.pad_args();
        match step {
            QuestStep::Goal => QuestWords {
                title: tr!("guide-quest-title"),
                lines: vec![tr!("guide-quest"), tr!("quest-goal-more")],
                tags: Vec::new(),
            },
            QuestStep::Screen => QuestWords {
                title: tr!("quest-screen-title"),
                lines: vec![
                    tr!("quest-screen"),
                    tr!(
                        "quest-screen-combat",
                        banner = tr!("mode-combat"),
                        badge = tr!("mode-combat-badge")
                    ),
                    tr!("quest-screen-overview", overview = bound("F8")),
                    tr!("quest-screen-tips"),
                ],
                tags: vec![
                    ("portrait", tr!("quest-tour-portrait")),
                    ("mode badge", tr!("quest-tour-mode")),
                    ("HP orb", tr!("quest-tour-hp")),
                    ("Pw orb", tr!("quest-tour-pw")),
                    ("minimap", tr!("quest-tour-minimap")),
                    ("micro-buttons", tr!("quest-tour-buttons")),
                    (
                        "action bar",
                        tr!("quest-tour-bar", first = bound("1"), last = bound("0")),
                    ),
                    ("log", tr!("quest-tour-log")),
                ],
            },
            QuestStep::Moves => QuestWords {
                title: tr!("quest-moves-title"),
                lines: vec![
                    tr!("quest-moves-keys", steps = self.steps_words()),
                    tr!("quest-moves-fight", fight = bound("F")),
                    tr!("quest-moves-click"),
                    i18n::tr_with("quest-moves-pad", Some(&pad)),
                    tr!(
                        "quest-moves-more",
                        inventory = bound("i"),
                        palette = bound("#"),
                        help = bound("F1")
                    ),
                ],
                tags: Vec::new(),
            },
        }
    }

    /// The eight steps' keys as the player presses them: the profile's own
    /// words while none has moved, else the keys, west first round by north.
    fn steps_words(&self) -> String {
        let profile = self.ui_state.profile;
        let steps = crate::rebind::steps(profile);
        let map = self.keymap();
        if steps.iter().all(|&s| map.key_for(s) == Some(s)) {
            return match profile {
                KeyProfile::Modern => tr!("quest-steps-modern"),
                KeyProfile::Classic => tr!("quest-steps-classic"),
            };
        }
        steps
            .iter()
            .map(|&s| {
                map.key_for(s)
                    .map_or_else(|| "—".to_string(), |k| crate::rebind::short_name(&k))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The first-steps guide runs: the player's setting, and in a self-test
    /// only when the test asks for it.
    pub(crate) fn hints_on(&self) -> bool {
        !self.settings.hints_off && (self.selftest.is_none() || self.guide_in_test)
    }

    /// At a command prompt (and when a self-test has staged one): the
    /// guide's cards whose occasion has come wait their turn, each card
    /// once (what was said while the guide was off is passed by).
    pub(crate) fn check_hints(&mut self) {
        if !self.hints_on() {
            self.hint_watch.skip_log(&self.world);
            return;
        }
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        for g in self.hint_watch.occasions(&self.world, &cat) {
            if self.hint_only.is_some_and(|only| only != g) {
                continue;
            }
            if !self.guide_seen.contains(&g) && !self.hints_waiting.contains(&g) {
                self.hints_waiting.push(g);
            }
        }
        self.hints_waiting
            .sort_by_key(|g| nh_world::Guide::CARDS.iter().position(|c| c == g));
    }

    /// The guide's card on screen while the map is clear (the engine at a
    /// command or at work, nothing over the map, the prologue over); the
    /// next one waiting a moment after the last went.
    fn tick_hints(&mut self, dt: f64) {
        let on = self.hints_on();
        // the prologue plays while the engine's first command waits
        let clear = self.state == GameState::Playing
            && self.prologue.is_none()
            && matches!(self.pending, None | Some((_, Prompt::Command)))
            && !self.world.getpos
            && self.ui.as_ref().is_some_and(|ui| {
                !ui.dialogs.is_open()
                    && !ui.inventory.is_open()
                    && !ui.help.is_open()
                    && !ui.hud.full_log_open()
                    && !ui.quest.is_open()
                    && !ui.game_menu.is_open()
                    && ui.screens.current().is_none()
            });
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if !on {
            ui.hint.clear();
            self.hints_waiting.clear();
            return;
        }
        let next = !self.hints_waiting.is_empty();
        if ui.hint.tick(dt, !clear, self.world.turn(), next) {
            self.hint_gone_at = Some(Instant::now());
        }
        let rested = self.hint_gone_at.is_none_or(|t| t.elapsed() >= HINT_GAP);
        if clear && ui.hint.idle() && rested && !self.hints_waiting.is_empty() {
            let g = self.hints_waiting.remove(0);
            self.show_hint(g);
        }
    }

    /// "Your quest" follows the screen: a window resized, its names go
    /// beside their blocks again.
    fn tick_quest(&mut self) {
        if self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.quest.moved(&ui.hud.blocks()))
        {
            self.show_quest();
        }
    }

    /// A card shown, under the portrait block; met from now on.
    fn show_hint(&mut self, g: nh_world::Guide) {
        let (title, body) = self.guide_words(g);
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        let at = ui
            .hud
            .blocks()
            .into_iter()
            .find(|(name, _)| *name == "portrait")
            .map_or(Vector2::new(24.0, 220.0), |(_, r)| {
                Vector2::new(r.position.x, r.end().y + 16.0)
            });
        ui.hint.show(g, &title, &body, at);
        self.hint_log.push(g);
        self.guide_seen.insert(g);
        self.update_profile(|p| {
            p.guide_seen.insert(g);
        });
    }

    /// A gamepad's buttons by name, for words that tell them.
    fn pad_args(&self) -> i18n::FluentArgs<'static> {
        let kind = self.pad.kind;
        let mut args = i18n::FluentArgs::new();
        for (name, b) in [
            ("a", PadButton::A),
            ("b", PadButton::B),
            ("x", PadButton::X),
            ("y", PadButton::Y),
            ("lb", PadButton::Lb),
            ("rb", PadButton::Rb),
            ("lt", PadButton::Lt),
            ("rt", PadButton::Rt),
            ("start", PadButton::Start),
            ("back", PadButton::Back),
            ("l3", PadButton::L3),
        ] {
            args.set(name, i18n::Arg::fluent(kind.label(b).to_string()));
        }
        args
    }

    /// The guide's page `g` in the language now: its title, and its words
    /// with the keys as the player presses them, or while a gamepad plays
    /// its buttons (the pages that name keys have words of their own).
    pub(crate) fn guide_words(&self, g: nh_world::Guide) -> (String, String) {
        let key = g.key();
        let pad_key = format!("{key}-pad");
        if self.pad.active && i18n::has(&pad_key) {
            return (
                i18n::tr(&format!("{key}-title")),
                i18n::tr_with(&pad_key, Some(&self.pad_args())),
            );
        }
        let map = self.keymap();
        let bound = |hint: &str| crate::rebind::bound_hint(map, hint);
        let kick = match self.ui_state.profile {
            KeyProfile::Modern => "k",
            KeyProfile::Classic => "^D",
        };
        let mut args = i18n::FluentArgs::new();
        for (name, hint) in [
            ("pickup", ","),
            ("fight", "F"),
            ("rest", "F5"),
            ("search", "s"),
            ("eat", "e"),
            ("pay", "p"),
            ("down", ">"),
            ("kick", kick),
            ("quaff", "q"),
        ] {
            args.set(name, i18n::Arg::fluent(bound(hint)));
        }
        (
            i18n::tr(&format!("{key}-title")),
            i18n::tr_with(key, Some(&args)),
        )
    }

    /// The guide's page `g` in the help: its title and paragraphs (BBCode);
    /// "Your quest" whole, each of its steps under its title.
    /// The guide, first in the help's list: "Your quest" always, the cards
    /// met so far.
    fn help_guide(&self) -> Vec<(String, Vec<String>)> {
        std::iter::once(nh_world::Guide::Quest)
            .chain(
                self.guide_seen
                    .iter()
                    .copied()
                    .filter(|&g| g != nh_world::Guide::Quest),
            )
            .map(|g| self.guide_page(g))
            .collect()
    }

    fn guide_page(&self, g: nh_world::Guide) -> (String, Vec<String>) {
        use crate::guide_view::QuestStep;
        let esc = crate::theme::bbcode_escape;
        if g != nh_world::Guide::Quest {
            let (title, words) = self.guide_words(g);
            return (title, vec![esc(&words)]);
        }
        let mut title = String::new();
        let mut paragraphs = Vec::new();
        for step in QuestStep::ALL {
            let words = self.quest_words(step);
            if step == QuestStep::Goal {
                title = words.title;
            } else {
                paragraphs.push(format!("[b]{}[/b]", esc(&words.title)));
            }
            let mut lines = words.lines.iter().map(|l| esc(l));
            paragraphs.extend(lines.next());
            // the screen's names, a list, after the line that says so
            if !words.tags.is_empty() {
                let list: Vec<String> = words
                    .tags
                    .iter()
                    .map(|(_, t)| format!("• {}", esc(t)))
                    .collect();
                paragraphs.push(list.join("\n"));
            }
            paragraphs.extend(lines);
        }
        (title, paragraphs)
    }

    /// A key where a direction is asked outside the command prompt (getdir,
    /// getpos's cursor): a step's key the player moved gives the step, the
    /// step's own key moved away nothing; any other key is what it is.
    fn direction_key(&self, k: KeyInput) -> Option<KeyInput> {
        if self.raw_keys {
            return Some(k);
        }
        let steps = crate::rebind::steps(self.ui_state.profile);
        let is_step = |k: KeyInput| steps.contains(&nh_world::norm(k));
        match self.keymap().translate(k) {
            Some(t) if is_step(t) => Some(KeyInput { echo: k.echo, ..t }),
            Some(_) => Some(k),
            None if is_step(k) => None,
            None => Some(k),
        }
    }

    /// A dialog's answer to the engine; what a panel asked to follow it (a
    /// spell to cast, the codex's other tab) at the command prompt it goes
    /// back to.
    fn dialog_reply(&mut self, r: Reply) {
        use crate::dialogs::Follow;
        use nh_world::Macro;
        let follow = match self.ui_mut().dialogs.take_follow() {
            Some(Follow::Cast(name)) => Some(Macro::cast(&name)),
            Some(Follow::Key(k)) => Some(Macro::key(k, None, self.world.number_pad)),
            Some(Follow::Ext(name)) => Some(Macro::ext(name)),
            None => None,
        };
        if let Some(m) = follow {
            self.deferred = Some(SlotUse::Macro(m));
        }
        self.reply(r);
    }

    /// A key at a command prompt (not getpos): a count, an order, or the
    /// engine's command.
    fn command_key(&mut self, pressed: KeyInput) {
        let (np, dirs) = (self.world.number_pad, self.world.dirchars.clone());
        let profile = self.ui_state.profile;
        // the command the key the player pressed stands for (the keys
        // they moved); the key itself is what is held down and let go
        let Some(k) = self.player_key(pressed) else {
            return;
        };
        // the top-row digits: Modern's bar, Classic's count (its bar is
        // Alt and the digits)
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
                self.held = Some(held_key(pressed.key));
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
                // a count goes to the engine with its command, as tty
                // types them: NetHack repeats it, and stops it by its rules
                if let Some(n) = n {
                    let mut keys: std::collections::VecDeque<Reply> =
                        nh_world::counted(code, n, np, &dirs).into();
                    if let Some(first) = keys.pop_front() {
                        self.typing = keys;
                        self.reply(first);
                    }
                    return;
                }
                // '<' or '>' away from the stairs: walk there and use them,
                // if the player asked for it; else NetHack answers ("You
                // can't go down here.")
                if let Some(c @ ('<' | '>')) = c
                    && self.settings.walk_to_stairs
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
        // a page over everything: the d-pad's arrows, A and B, the bumpers
        if self.achievements_open()
            || self.settings_open()
            || self.credits_open()
            || ui.quest.is_open()
            || ui.game_menu.is_open()
        {
            return PadCtx::Other;
        }
        if ui.help.is_open() {
            return PadCtx::Help;
        }
        // the codex: its tabs on the bumpers, as the inventory's filters
        if ui.dialogs.layout_name() == Some("codex") {
            return PadCtx::Codex;
        }
        if ui.dialogs.is_open() {
            return match ui.dialogs.kind_name() {
                Some("menu") if !ui.dialogs.menu_picks() => PadCtx::Read,
                Some("menu") => PadCtx::Menu {
                    any: ui.dialogs.menu_any(),
                },
                Some("show") => PadCtx::Read,
                Some("extcmd") => PadCtx::Palette,
                Some("choice") => PadCtx::Choice,
                Some("text") => PadCtx::Text,
                Some("picker") => PadCtx::Picker {
                    wish: ui.dialogs.pick_is_wish(),
                },
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
        // the message history over the map: read it, B closes it
        if ui.hud.full_log_open() {
            return PadCtx::Read;
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
        let page = self.achievements_open() || self.settings_open() || self.credits_open();
        if self.state != GameState::Playing && !page {
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
                PadOut::Key(k) => self.push_ui(UiEvent::GameKey(k)),
                PadOut::KeyUp(k) => self.push_ui(UiEvent::KeyUp(k)),
                // A puts the guide's card away first, as B does
                PadOut::Activate
                    if self.at_command()
                        && self.ui.as_ref().is_some_and(|ui| ui.hint.shown().is_some()) =>
                {
                    self.ui_mut().hint.dismiss();
                }
                PadOut::Activate => {
                    if let Some((x, y)) = self.pad_cell() {
                        self.push_ui(UiEvent::MapClick { x, y, button: 1 });
                    }
                    self.pad_cursor = None;
                }
                PadOut::Search => self.use_slot(SlotUse::Macro(nh_world::Macro::search())),
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
                // Start: the game's menu, at the command prompt
                PadOut::Menu => {
                    let open = self.ui.as_ref().is_some_and(|ui| ui.game_menu.is_open());
                    if self.at_command() && !open {
                        self.open_game_menu();
                    }
                }
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
                PadOut::Pick(op) => self.pick_op(op),
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

    /// The hero's gender from the hero's glyph (a polymorph changes it),
    /// told to the engine's translator when it changes. On a steed the
    /// map's glyph is the steed's, a mare's or a stallion's: the hero's
    /// own is what the engine tells of the rider.
    fn sync_hero_gender(&mut self) {
        use nh_protocol::mg;
        let female = self
            .world
            .map
            .hero_form()
            .map(|g| g.flags & mg::FEMALE != 0);
        if female.is_some() && female != self.hero_female {
            self.hero_female = female;
            i18n::set_hero_female(female == Some(true));
            if let Some(ui) = self.ui.as_mut() {
                ui.hud.hero_changed();
            }
        }
    }

    /// A picker's button (a gamepad's).
    fn pick_op(&mut self, op: crate::gamepad::PickOp) {
        let pending = self.pending.as_ref().map(|(id, _)| *id);
        let ui = self.ui_mut();
        if ui.dialogs.open_req() != pending {
            return;
        }
        if let Some(r) = ui.dialogs.pick_op(op) {
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
                        self.typing = keys.into_iter().map(Reply::Key).collect();
                    }
                    None => self.reply(Reply::Char(letter as i32)),
                }
            }
            Intent::View(list) => self.update_profile(|p| p.pack_list = Some(list)),
            Intent::Bind { slot, binding } => {
                self.ui_state.bar.set(slot, Some(binding));
                self.save_ui_state();
                let key = self.slot_key(slot);
                self.ui_mut().hud.toast(&tr!("bar-bound", key = key), false);
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
        let key = self.slot_key(slot);
        self.ui_mut()
            .hud
            .toast(&tr!("bar-cleared", key = key), true);
    }

    fn undo_clear(&mut self) {
        if let Some((slot, b)) = self.cleared.take() {
            self.ui_state.bar.set(slot, Some(b));
            self.save_ui_state();
            let key = self.slot_key(slot);
            self.ui_mut()
                .hud
                .toast(&tr!("bar-restored", key = key), false);
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
        // the keys the player moved, wherever a key is named
        let map = self.settings.keymaps.get(self.ui_state.profile);
        ui.hud.set_keymap(map);
        ui.dialogs.set_keymap(map);
        ui.inventory.set_keymap(map);
        ui.help.set_keymap(map);
        // the RPG panels keep above the HUD's orbs and bar
        let room = ui.hud.cluster_room();
        ui.dialogs.set_bottom_room(room);
        // the journal's messages: the log's, beside #chronicle's events
        if ui.dialogs.journal_wants_messages() {
            ui.dialogs.fill_journal(self.world.log.iter());
        }
        // a gamepad: its hints, the bar's chords, dialogs with a focus
        let pad = self.pad.active.then_some(self.pad.kind);
        ui.dialogs.set_pad(pad.is_some());
        let owes = (self.world.inventory.items().iter()).any(|i| i.text.contains("(unpaid"));
        ui.dialogs.set_owes(owes);
        ui.help.set_pad(pad.is_some());
        ui.inventory.set_pad(pad);
        ui.game_menu.set_pad(pad);
        ui.quest.set_pad(pad);
        ui.hint.set_pad(pad);
        ui.hud.set_pad(pad);
        // the pages, the game's menu and "Your quest" show their own
        let page = matches!(
            ui.screens.current(),
            Some("settings" | "credits" | "achievements")
        ) || ui.game_menu.is_open()
            || ui.quest.is_open();
        ui.pad.show_hints(pad.filter(|_| !page).map(|k| (k, ctx)));
        // the hints go into the panel on top: a dialog's, else the
        // inventory's, which keep room for them
        let panel = ui
            .help
            .frame_rect()
            .or_else(|| ui.dialogs.panel_rect())
            .or_else(|| ui.inventory.frame_rect());
        ui.pad.dock_hints(panel, ui.hud.log_rect());
        let room = if pad.is_some() {
            ui.pad.strip_height() + 6.0
        } else {
            0.0
        };
        ui.dialogs.set_pad_room(room);
        // the slots' keys: a gamepad's chords, every slot's, else the keys
        // the player presses for them when moved (the digits as drawn, else)
        let texts: Option<Vec<String>> = match pad {
            Some(kind) => Some(
                (0..BAR_SLOTS)
                    .map(|i| {
                        let (lb, rb, face) = Pad::chord_of(i);
                        let mut words = Vec::new();
                        if lb {
                            words.push(kind.label(PadButton::Lb));
                        }
                        if rb {
                            words.push(kind.label(PadButton::Rb));
                        }
                        words.push(kind.label(face));
                        // held together: "LB+A", "LB+RB+A"
                        words.join("+")
                    })
                    .collect(),
            ),
            None => {
                let profile = self.ui_state.profile;
                let map = self.settings.keymaps.get(profile);
                let keys: Vec<String> = (0..BAR_SLOTS)
                    .map(|i| crate::rebind::bound_hint(map, &crate::rebind::slot_hint(profile, i)))
                    .collect();
                let moved = keys
                    .iter()
                    .enumerate()
                    .any(|(i, k)| k != crate::action_bar::key_label(i));
                moved.then_some(keys)
            }
        };
        if self.bar_labels.as_ref() != Some(&texts) {
            ui.hud.action_bar().set_key_labels(texts.as_deref());
            self.bar_labels = Some(texts);
        }
        // a model still loading has nothing to show, nor to render ahead
        let loading = ui.map.hero_model().is_some_and(|m| m.is_pending());
        let hero = ui.map.hero_view().filter(|_| !loading);
        ui.inventory.set_hero(hero.clone());
        ui.inventory.set_form(self.world.status.polymorphed());
        ui.hud.sync_portrait(
            hero,
            ui.map.hero_gear_shown(),
            self.world.status.polymorphed(),
        );
        ui.inventory.set_dark(ui.map.dark());
        let rects = ui.hud.action_bar().slot_rects();
        ui.inventory.set_bar_rects(rects);
        // the character sheet's portrait is the doll's render
        let sheet = ui.dialogs.layout_name() == Some("sheet");
        let rendered = ui.inventory.lend_doll(sheet);
        ui.dialogs.set_portrait_rendered(rendered);
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
            // the keys as the player presses them now
            let profile = self.ui_state.profile;
            let map = self.settings.keymaps.get(profile);
            let key = crate::rebind::bound_hint(map, &crate::rebind::slot_hint(profile, i));
            let what = v
                .label_key
                .as_deref()
                .map(inventory_panel::label)
                .unwrap_or_default();
            let text = v
                .text
                .as_deref()
                .map(|t| i18n::engine(EngineKind::Name, t).into_owned());
            let tip = match (v.state, text) {
                (SlotState::Empty, _) => tr!("bar-slot-empty-tip", key = &key),
                (SlotState::Gone, Some(t)) => tr!("bar-slot-gone-tip", what = what, item = t),
                (_, Some(t)) => tr!("bar-slot-tip", what = what, item = t),
                (_, None) => what,
            };
            let hint = crate::rebind::bound_hint(map, v.hint.as_deref().unwrap_or(""));
            // an empty slot has no command to name, nothing to clear
            let full = if v.state == SlotState::Empty {
                tip
            } else {
                tr!("bar-slot-keys-tip", tip = tip, key = key, hint = hint)
            };
            bar.set_tooltip(i, &full);
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
                        let why = match why {
                            "unexplored" => tr!("click-unexplored"),
                            _ => tr!("click-off-map"),
                        };
                        self.stop_note = Some((tr!("click-nothing", why = why), Instant::now()));
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
            self.stop_note = Some((stop_text(stop), Instant::now()));
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

    /// The tick: a self-test's `--tick=` (any value), RENETHACK_TICK_MS,
    /// else the player's order speed, Normal being the project setting
    /// (150–500 ms).
    fn set_tick(&mut self, test: Option<u64>) {
        self.test_tick = test.or(self.test_tick);
        if let Some(ms) = self.test_tick {
            self.driver.set_test_tick(Duration::from_millis(ms));
            return;
        }
        let settings = ProjectSettings::singleton();
        let setting = settings
            .has_setting(TICK_SETTING)
            .then(|| settings.get_setting(TICK_SETTING).try_to::<i64>().ok())
            .flatten()
            .and_then(|v| u64::try_from(v).ok());
        let normal = setting.unwrap_or(nh_world::DEFAULT_TICK_MS);
        let ms = env_number::<u64>("RENETHACK_TICK_MS")
            .unwrap_or_else(|| self.settings.order_speed.tick_ms(normal));
        self.driver.set_tick_ms(ms);
    }

    /// The player's own NetHack options as a notice tells them, kept in
    /// the profile for every next game and restore.
    fn keep_nethack_options(&mut self, rc: &str) {
        let own = nh_world::player_options(rc);
        if own != self.settings.nethack_options {
            let mut s = self.settings.clone();
            s.nethack_options = own;
            self.set_settings(s);
        }
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
            Some(o) => Some(order_text(o, self.pad.active)),
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
            // a gamepad's and a button's keys are the game's own: no moved
            // key applies
            UiEvent::GameKey(k) => {
                self.raw_keys = true;
                self.on_key(k);
                self.raw_keys = false;
            }
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
                    self.dialog_reply(r);
                }
            }
            UiEvent::NewGame if idle => self.show_creation(),
            UiEvent::ContinueGame(name) if idle => self.continue_game(&name),
            UiEvent::StartCharacter(choice) if idle => self.start_new(&choice),
            UiEvent::NameEdited(name) if idle => self.check_name(&name),
            UiEvent::NameKey(row, col) => self.ui_mut().screens.name_key(row, col),
            UiEvent::BackToTitle if idle && self.playground_lock.is_none() => self.start_up(),
            UiEvent::BackToTitle if idle => self.show_title(None),
            UiEvent::QuitApp if idle => self.finish_close(),
            UiEvent::CloseRequested => self.on_close_requested(),
            UiEvent::ToggleFullLog => {
                self.driver.interrupt(Stop::Panel);
                self.ui_mut().hud.toggle_full_log();
            }
            UiEvent::ToggleHelp if self.state == GameState::Playing => {
                self.driver.interrupt(Stop::Panel);
                let pages = self.help_guide();
                let help = &mut self.ui_mut().help;
                if help.is_open() {
                    help.close();
                } else {
                    help.set_guide(pages);
                    help.open();
                }
            }
            UiEvent::HintClose => self.ui_mut().hint.dismiss(),
            UiEvent::Quest(input) => self.quest_input(input),
            UiEvent::GameMenu(pick) => self.game_menu_pick(pick),
            UiEvent::Help(input) => self.ui_mut().help.input(input),
            UiEvent::KeyUp(k) => self.on_key_up(k),
            UiEvent::FocusLost => {
                // the radial held open by LT closes, nothing chosen
                let outs = self.pad.release_all();
                self.pad_out(outs);
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
            UiEvent::OpenSettings => self.open_settings(),
            UiEvent::CloseSettings => self.close_settings(),
            UiEvent::Settings(input) => self.on_settings(input),
            UiEvent::OpenCredits => self.open_credits(),
            UiEvent::CloseCredits => self.close_credits(),
            UiEvent::Credits(crate::credits_page::CreditsInput::Pick(i)) => {
                if let Some(p) = self.credits_page.as_mut() {
                    p.select(i);
                }
            }
            UiEvent::OpenAchievements => self.open_achievements(),
            UiEvent::ReplayPrologue
                if self.state == GameState::Title && self.prologue.is_none() =>
            {
                let legacy = load_profile(&self.playground()).legacy;
                if !legacy.is_empty() {
                    self.start_prologue(legacy, true);
                }
            }
            UiEvent::ReplayPrologue => {}
            UiEvent::CloseAchievements => self.close_achievements(),
            UiEvent::AchievementPick(i) => {
                if let Some(p) = self.achievement_page.as_mut() {
                    p.select(i);
                }
            }
            UiEvent::SetLanguage(lang) => self.set_language(lang),
            UiEvent::SetVolume(bus, linear) => {
                crate::audio::set_volume(bus, linear);
                self.volumes_set = true;
            }
            other => godot_warn!("renethack: {other:?} ignored while a game runs"),
        }
    }

    // ---- screens and lifecycle ----

    /// The settings page, over the title or the game (its orders stop).
    fn open_settings(&mut self) {
        let in_game = match self.state {
            GameState::Title => false,
            GameState::Playing => true,
            _ => return,
        };
        if in_game {
            self.driver.interrupt(Stop::Panel);
        }
        self.build_settings(in_game, (settings_page::Section::Display, 0));
    }

    /// The settings page made anew (on opening, in another language, for
    /// a gamepad's hints or the keyboard's), where it was.
    fn build_settings(&mut self, in_game: bool, at: (settings_page::Section, usize)) {
        let view = self.settings_view();
        let pad = self.pad.active.then_some(self.pad.kind);
        // the controls: the gamepad's while it is in hand, else the keys
        // of the character playing, else a new character's
        let controls = if pad.is_some() {
            crate::bindings::Shown::Pad
        } else {
            let keys = view
                .playing_keys
                .unwrap_or(self.settings.keys.unwrap_or_default());
            crate::bindings::Shown::Keys(keys)
        };
        // the keyboard's commands of each profile, by the catalog's names
        let catalog = self.catalog.clone();
        let commands = [KeyProfile::Modern, KeyProfile::Classic]
            .map(|p| crate::rebind::commands(p, catalog.as_deref()));
        let page = SettingsPage::new(
            view,
            &self.queue,
            at.0,
            at.1,
            pad,
            self.pad.kind,
            controls,
            commands,
        );
        self.ui_mut().screens.show_settings(&page.root(), in_game);
        self.settings_page = Some(page);
    }

    /// What the settings page shows now.
    fn settings_view(&self) -> settings_page::View {
        let window = self
            .base()
            .get_tree()
            .get_root()
            .map(|w| w.get_size())
            .unwrap_or_default();
        let window = (window.x.max(0) as u32, window.y.max(0) as u32);
        // without a display, the window's own size stands for the screen
        let screen = match crate::video::screen_size() {
            (0, 0) => window,
            s => s,
        };
        let root = self.base().get_tree().get_root();
        let ui_auto = root.as_ref().map_or(100, crate::theme::auto_ui_scale);
        settings_page::View {
            settings: self.settings.clone(),
            lang: i18n::lang(),
            window,
            screen,
            device: self.device_quality,
            ui_auto,
            ui_forced: crate::theme::forced_ui_scale(),
            deck: crate::video::is_deck(),
            playing_keys: (self.state == GameState::Playing).then_some(self.ui_state.profile),
            volumes: crate::audio::Bus::ALL.map(crate::audio::volume),
            // over a game, its own as its notice tells them: what the player
            // set until it has it
            game_options: (self.state == GameState::Playing)
                .then(|| self.gameplay_goal.clone().or(self.world.options.clone()))
                .flatten(),
        }
    }

    /// The graphics drawn: the player's, else the device's preset.
    pub(crate) fn graphics(&self) -> nh_world::Graphics {
        self.settings
            .graphics
            .unwrap_or_else(|| nh_world::Graphics::preset(self.device_quality))
    }

    /// The graphics to the renderer and to the map.
    fn apply_graphics(&mut self) {
        let g = self.graphics();
        if let Some(mut window) = self.base().get_tree().get_root() {
            crate::video::apply_graphics(&mut window, &g);
        }
        if let Some(ui) = self.ui.as_mut() {
            ui.map.set_graphics(&g);
        }
    }

    /// The settings page's widgets: a tab, a row's focus, an arrow.
    fn on_settings(&mut self, input: SettingsInput) {
        let Some(page) = self.settings_page.as_mut() else {
            return;
        };
        match input {
            SettingsInput::Tab(s) => page.tab(s),
            SettingsInput::Focus(f) => page.focus_field(f),
            // the controls' list is the page's own
            SettingsInput::Step(settings_page::Field::Controls, dir) => {
                page.focus_field(settings_page::Field::Controls);
                page.controls_step(dir, false);
            }
            SettingsInput::Step(f, dir) => {
                page.focus_field(f);
                self.settings_step(f, dir, false);
            }
            SettingsInput::Entry(i) => page.click_entry(i),
        }
    }

    /// A key on the settings page: the page moves its focus itself.
    fn settings_key(&mut self, k: KeyInput) {
        let Some(page) = self.settings_page.as_mut() else {
            return;
        };
        match page.key(&k, self.raw_keys) {
            Some(PageKey::Back) => self.close_settings(),
            Some(PageKey::Step(f, dir, wrap)) => self.settings_step(f, dir, wrap),
            Some(PageKey::Rebind {
                profile,
                command,
                key,
                swap,
            }) => {
                let mut s = self.settings.clone();
                s.keymaps.get_mut(profile).rebind(command, key, swap);
                self.set_settings(s);
            }
            Some(PageKey::Reset { profile, command }) => {
                let mut s = self.settings.clone();
                s.keymaps.get_mut(profile).reset(command);
                self.set_settings(s);
            }
            Some(PageKey::ResetAll(profile)) => {
                let mut s = self.settings.clone();
                *s.keymaps.get_mut(profile) = nh_world::Keymap::default();
                self.set_settings(s);
            }
            None => {}
        }
    }

    /// A slot's key as the player presses it now.
    fn slot_key(&self, slot: usize) -> String {
        let hint = crate::rebind::slot_hint(self.ui_state.profile, slot);
        crate::rebind::bound_hint(self.keymap(), &hint)
    }

    /// The keys the player moved, for the character playing's profile.
    pub(crate) fn keymap(&self) -> &nh_world::Keymap {
        self.settings.keymaps.get(self.ui_state.profile)
    }

    /// The command `k` gives the game where commands are awaited (the
    /// player's own keys, the settings' controls); None: a key whose
    /// command moved away. A gamepad's keys are the game's own.
    fn player_key(&self, k: KeyInput) -> Option<KeyInput> {
        if self.raw_keys {
            return Some(k);
        }
        self.keymap()
            .translate(k)
            .map(|t| KeyInput { echo: k.echo, ..t })
    }

    /// A row of the settings page a step on or back.
    fn settings_step(&mut self, field: settings_page::Field, dir: i32, wrap: bool) {
        let view = self.settings_view();
        match settings_page::step(field, &view, dir, wrap) {
            Some(Change::Lang(lang)) => self.set_language(lang),
            Some(Change::Settings(s)) => self.set_settings(s),
            Some(Change::Gameplay(o, g)) => self.set_gameplay(o, g),
            // as a slider's drag sets it
            Some(Change::Volume(bus, v)) => {
                crate::audio::set_volume(bus, v);
                self.volumes_set = true;
            }
            None => {}
        }
    }

    /// The client's settings changed: what changed applied at once, all
    /// kept in the profile, the page shown them.
    pub(crate) fn set_settings(&mut self, s: Settings) {
        let drawn = self.graphics();
        let old = std::mem::replace(&mut self.settings, s.clone());
        if s.display != old.display
            && let Some(mut window) = self.base().get_tree().get_root()
        {
            // after this call: a new size or mode notifies this node at
            // once, and it is bound here
            let display = s.display;
            Callable::from_fn("apply the display", move |_| {
                crate::video::apply_display(&mut window, &display, true);
                Variant::nil()
            })
            .call_deferred(&[]);
        }
        if self.graphics() != drawn {
            let rewarm = pipeline_settings(&self.graphics()) != pipeline_settings(&drawn);
            self.apply_graphics();
            if rewarm {
                self.start_rewarm();
            }
        }
        if s.ui_scale != old.ui_scale
            && let Some(window) = self.base().get_tree().get_root()
        {
            // after this call too: the window's new scale notifies it
            let scale = s.ui_scale;
            Callable::from_fn("apply the scale", move |_| {
                crate::theme::apply_scaling(window.clone(), scale);
                Variant::nil()
            })
            .call_deferred(&[]);
        }
        // the slots' keys and tips drawn again
        if s.keymaps != old.keymaps {
            self.bar_key = None;
            self.bar_labels = None;
        }
        if s.order_speed != old.order_speed {
            self.set_tick(None);
        }
        self.update_profile(|p| p.settings = s);
        let view = self.settings_view();
        if let Some(page) = self.settings_page.as_mut() {
            page.show(&view);
        }
    }

    /// A Gameplay row changed: kept in the profile for new games; over a
    /// game, given to it through its own options menu (`drive_options`).
    fn set_gameplay(&mut self, o: GameOption, g: Gameplay) {
        let mut s = self.settings.clone();
        s.gameplay.take(o, &g);
        if self.state == GameState::Playing {
            self.gameplay_goal = Some(g);
        }
        self.set_settings(s);
    }

    /// What the player set on the page is still being given to the game
    /// (self-tests).
    pub(crate) fn options_busy(&self) -> bool {
        self.options_run.is_some() || self.gameplay_goal.is_some()
    }

    /// What the player set over the game, given to it through its own
    /// full options menu as a player would use it, at a command prompt
    /// with nothing else under way. Reading needs no menu: the host tells
    /// the game's options before each request (`World::options`), so a
    /// look at the page leaves the command ^A repeats as it was.
    fn drive_options(&mut self) {
        let Some(goal) = self.gameplay_goal.clone() else {
            return;
        };
        if self.options_run.is_some() || self.state != GameState::Playing {
            return;
        }
        // the game has it already
        if self
            .world
            .options
            .as_ref()
            .is_some_and(|o| o.same_as(&goal))
        {
            self.gameplay_goal = None;
            return;
        }
        let idle = matches!(self.pending, Some((_, Prompt::Command)))
            && !self.world.getpos
            && !self.macros.is_active()
            && !self.driver.is_active()
            && self.typing.is_empty()
            && self.count.shown().is_none();
        if !idle {
            return;
        }
        let (run, reply) = OptionsRun::change(goal);
        self.options_run = Some(run);
        self.reply(reply);
    }

    /// A run that gave the game `gave` ended: the goal is done (the game
    /// has it, or would not take it: the page shows what its notice tells)
    /// unless the player set another since, which goes next.
    fn options_given(&mut self, gave: &Gameplay) {
        if self.gameplay_goal.as_ref().is_some_and(|g| g.same_as(gave)) {
            self.gameplay_goal = None;
        }
    }

    /// The settings page follows the window (a size dragged, a mode the
    /// screen took its time to take) and the input: a gamepad's hints.
    fn sync_settings_page(&mut self) {
        if let Some(page) = self.settings_page.as_mut() {
            page.reveal_focus();
        }
        let Some(page) = self.settings_page.as_ref() else {
            return;
        };
        let pad = self.pad.active.then_some(self.pad.kind);
        if page.pad() != pad {
            let at = page.place();
            let in_game = self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.settings_in_game());
            self.build_settings(in_game, at);
            return;
        }
        let view = self.settings_view();
        if let Some(page) = self.settings_page.as_mut()
            && *page.view() != view
        {
            page.show(&view);
        }
    }

    fn close_settings(&mut self) {
        self.keep_volumes();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if ui.screens.current() != Some("settings") {
            return;
        }
        self.settings_page = None;
        if ui.screens.settings_in_game() && self.state == GameState::Playing {
            ui.screens.hide();
        } else {
            self.show_title(None);
        }
    }

    /// The achievements page, over the title or the game (its orders
    /// stop): every achievement, earned or not, as the local store has
    /// them.
    fn open_achievements(&mut self) {
        let in_game = match self.state {
            GameState::Title => false,
            GameState::Playing => true,
            _ => return,
        };
        if in_game {
            self.driver.interrupt(Stop::Panel);
        }
        self.build_achievements(in_game, None);
    }

    /// The page made anew (on opening, and in another language), its
    /// choice kept.
    fn build_achievements(&mut self, in_game: bool, selected: Option<usize>) {
        let canvas = self.canvas_size();
        let (all, store) = match self.achievements.as_ref() {
            Some(t) => (t.achievements().clone(), t.store().clone()),
            None => (Achievements::built_in(), Default::default()),
        };
        let queue = self.queue.clone();
        let pad = self.pad.active.then_some(self.pad.kind);
        let mut page = crate::achievement_view::Page::new(&all, &store, &queue, canvas, pad);
        if let Some(i) = selected {
            page.select(i);
        }
        self.ui_mut()
            .screens
            .show_achievements(&page.root(), in_game);
        self.achievement_page = Some(page);
    }

    fn close_achievements(&mut self) {
        if !self.achievements_open() {
            return;
        }
        self.achievement_page = None;
        let in_game = self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.screens.achievements_in_game());
        if in_game && self.state == GameState::Playing {
            self.ui_mut().screens.hide();
        } else {
            self.show_title(None);
        }
    }

    /// A key or a gamepad's input on the title, the creation form, the end
    /// or an error page: ↑↓ (the d-pad, the left stick) move the focus,
    /// ←→ change a list's choice, Enter, Space or A take the control,
    /// Esc or B go back, Start does the page's deed. A name being typed
    /// keeps its keys; an open list has its own. True: taken.
    fn screen_input(&mut self, event: &Gd<InputEvent>) -> bool {
        use crate::screens::NavInput;
        let Some(ui) = self.ui.as_ref() else {
            return false;
        };
        if !matches!(
            ui.screens.current(),
            Some("title" | "creation" | "end" | "error")
        ) || ui.screens.list_open()
        {
            return false;
        }
        let typing = ui.screens.typing();
        let input = if let Ok(b) = event.clone().try_cast::<InputEventJoypadButton>() {
            self.pad.active = true;
            if !b.is_pressed() {
                return true;
            }
            match PadButton::from_joy(b.get_button_index()) {
                Some(PadButton::Up) => NavInput::Up,
                Some(PadButton::Down) => NavInput::Down,
                Some(PadButton::Left) => NavInput::Left,
                Some(PadButton::Right) => NavInput::Right,
                Some(PadButton::A) => NavInput::Accept,
                Some(PadButton::B) => NavInput::Back,
                Some(PadButton::Start) => NavInput::Start,
                Some(PadButton::Lb) => NavInput::PageUp,
                Some(PadButton::Rb) => NavInput::PageDown,
                Some(PadButton::Y) => NavInput::Layout,
                _ => return true,
            }
        } else if let Ok(m) = event.clone().try_cast::<InputEventJoypadMotion>() {
            self.pad.active = true;
            // the left stick: a step each time it leans anew
            let lean = |v: f32| match v {
                v if v > 0.6 => 1,
                v if v < -0.6 => -1,
                v if v.abs() < 0.3 => 0,
                _ => 2,
            };
            let (x, y) = self.screen_stick;
            match m.get_axis() {
                JoyAxis::LEFT_Y => match lean(m.get_axis_value()) {
                    2 => return true,
                    l if l == y => return true,
                    l => {
                        self.screen_stick.1 = l;
                        match l {
                            -1 => NavInput::Up,
                            1 => NavInput::Down,
                            _ => return true,
                        }
                    }
                },
                JoyAxis::LEFT_X => match lean(m.get_axis_value()) {
                    2 => return true,
                    l if l == x => return true,
                    l => {
                        self.screen_stick.0 = l;
                        match l {
                            -1 => NavInput::Left,
                            1 => NavInput::Right,
                            _ => return true,
                        }
                    }
                },
                _ => return true,
            }
        } else if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
            if !key.is_pressed() {
                return false;
            }
            self.pad.active = false;
            // keys instead of the pad: the name's keyboard goes
            self.ui_mut().screens.close_name_keys();
            let shift = key.is_shift_pressed();
            match key.get_keycode() {
                GKey::UP => NavInput::Up,
                GKey::DOWN => NavInput::Down,
                GKey::TAB if shift => NavInput::Prev,
                GKey::TAB => NavInput::Next,
                GKey::PAGEUP => NavInput::PageUp,
                GKey::PAGEDOWN => NavInput::PageDown,
                // a name being typed keeps the rest
                _ if typing && key.get_keycode() != GKey::ESCAPE => return false,
                GKey::LEFT => NavInput::Left,
                GKey::RIGHT => NavInput::Right,
                GKey::ENTER | GKey::KP_ENTER | GKey::SPACE if !key.is_echo() => NavInput::Accept,
                GKey::ESCAPE if !key.is_echo() => {
                    if typing {
                        // the field gives up the keys first
                        self.ui_mut().screens.stop_typing();
                        return true;
                    }
                    NavInput::Back
                }
                _ => return false,
            }
        } else {
            return false;
        };
        self.ui_mut().screens.nav_input(input)
    }

    /// The achievements page is up (keys and the gamepad are its own).
    pub(crate) fn achievements_open(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|ui| ui.screens.current() == Some("achievements"))
    }

    /// A key on the achievements page: the arrows choose, Esc goes back.
    fn achievements_key(&mut self, k: KeyInput) {
        let step = match k.key {
            Key::Left => (-1, 0),
            Key::Right => (1, 0),
            Key::Up => (0, -1),
            Key::Down => (0, 1),
            Key::Escape if !k.echo => {
                self.close_achievements();
                return;
            }
            _ => return,
        };
        if let Some(p) = self.achievement_page.as_mut() {
            p.step(step.0, step.1);
        }
    }

    /// The settings page is up (keys are its own).
    fn settings_open(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|ui| ui.screens.current() == Some("settings"))
    }

    /// The credits and licences page, over the title or the game (its
    /// orders stop); from the settings page, Back goes back to it.
    fn open_credits(&mut self) {
        let in_game = match self.state {
            GameState::Title => false,
            GameState::Playing => true,
            _ => return,
        };
        if in_game {
            self.driver.interrupt(Stop::Panel);
        }
        self.credits_return = self.settings_page.as_ref().map(|p| p.place());
        if self.credits_return.is_some() {
            // the settings close under it
            self.keep_volumes();
            self.settings_page = None;
        }
        self.build_credits(in_game, 0);
    }

    /// The credits page made anew (on opening, in another language, for
    /// a gamepad's hints), on its `selected`-th document. The NGPL and
    /// Lua's notice are read as the engine ships them.
    fn build_credits(&mut self, in_game: bool, selected: usize) {
        let canvas = self.canvas_size();
        let data = self.paths.as_ref().map(Paths::data);
        let read = |name: &str| {
            data.as_ref()
                .and_then(|d| std::fs::read_to_string(d.join(name)).ok())
        };
        let shipped = crate::credits::Shipped {
            ngpl: read("license"),
            lua: read("lua-license"),
        };
        let pad = self.pad.active.then_some(self.pad.kind);
        let page =
            crate::credits_page::CreditsPage::new(&self.queue, canvas, shipped, selected, pad);
        self.ui_mut().screens.show_credits(&page.root(), in_game);
        self.credits_page = Some(page);
    }

    fn close_credits(&mut self) {
        if !self.credits_open() {
            return;
        }
        self.credits_page = None;
        let in_game = self.state == GameState::Playing
            && self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.credits_in_game());
        if let Some(at) = self.credits_return.take() {
            self.build_settings(in_game, at);
        } else if in_game {
            self.ui_mut().screens.hide();
        } else {
            self.show_title(None);
        }
    }

    /// The credits page is up (keys are its own).
    pub(crate) fn credits_open(&self) -> bool {
        self.ui
            .as_ref()
            .is_some_and(|ui| ui.screens.current() == Some("credits"))
    }

    /// A key on the credits page: the page moves itself; Esc goes back.
    fn credits_key(&mut self, k: KeyInput) {
        let Some(page) = self.credits_page.as_mut() else {
            return;
        };
        if page.key(&k) {
            self.close_credits();
        }
    }

    /// The achievements page's hints follow the input: a gamepad's or the
    /// keyboard's.
    fn sync_achievements_page(&mut self) {
        let Some(page) = self.achievement_page.as_ref() else {
            return;
        };
        let pad = self.pad.active.then_some(self.pad.kind);
        if page.pad() != pad {
            let selected = page.selected();
            let in_game = self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.achievements_in_game());
            self.build_achievements(in_game, Some(selected));
        }
    }

    /// The words that name keys follow the input: a gamepad's buttons
    /// while it plays, the keys again once the keyboard or the mouse does
    /// (the prompt line, the guide's card and "Your quest", and the feet
    /// of the game's menu, the page and the card).
    fn sync_pad_words(&mut self) {
        let pad = self.pad.active.then_some(self.pad.kind);
        if pad == self.words_pad {
            return;
        }
        self.words_pad = pad;
        let line = self.pending.as_ref().and_then(|(_, p)| match p {
            Prompt::Command => getpos_line(&self.world, pad),
            p => prompt_line(p, pad),
        });
        let card = self.ui.as_ref().and_then(|ui| ui.hint.current());
        let words = card.map(|g| self.guide_words(g));
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.game_menu.set_pad(pad);
        ui.screens.set_pad(pad);
        ui.quest.set_pad(pad);
        ui.hint.set_pad(pad);
        ui.prologue.set_pad(pad);
        if line.is_some() {
            ui.hud.set_prompt_line(line.as_deref());
        }
        if let Some((title, body)) = words {
            ui.hint.rewrite(&title, &body);
        }
        self.show_quest();
    }

    /// The credits page's hints follow the input: a gamepad's or the
    /// keyboard's.
    fn sync_credits_page(&mut self) {
        let Some(page) = self.credits_page.as_ref() else {
            return;
        };
        let pad = self.pad.active.then_some(self.pad.kind);
        if page.pad() != pad {
            let selected = page.selected().0;
            let in_game = self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.credits_in_game());
            self.build_credits(in_game, selected);
        }
    }

    /// Switch the interface's language: kept in the profile and with the
    /// character playing; what is on screen is drawn again in it (the log
    /// from the engine's own English).
    fn set_language(&mut self, lang: Lang) {
        if i18n::lang() == lang {
            return;
        }
        i18n::set_lang(lang);
        self.update_profile(|p| p.lang = Some(lang.code().to_string()));
        if self.session.is_some() {
            self.ui_state.lang = Some(lang.code().to_string());
            self.save_ui_state();
        }
        self.relang();
    }

    /// The volumes set since the profile last kept them, kept.
    fn keep_volumes(&mut self) {
        if std::mem::take(&mut self.volumes_set) {
            self.update_profile(|p| p.volumes = crate::audio::volumes());
        }
    }

    /// The profile read, changed by `change`, written back (what else it
    /// keeps stays).
    /// Whether a new game starts with the prologue: as the settings say
    /// (the first game: once a profile), never in a self-test unless it
    /// asks.
    fn prologue_plays(&self) -> bool {
        if self.selftest.is_some() {
            return self.prologue_forced;
        }
        let seen = load_profile(&self.playground()).prologue_seen;
        self.settings.prologue.plays(seen)
    }

    /// Tell the legacy text (`english`: the engine's lines) over the
    /// prologue's scenes; `replay`: from the title. False when it is not
    /// laid out as NetHack's.
    pub(crate) fn start_prologue(&mut self, english: Vec<String>, replay: bool) -> bool {
        let (role, female, align) = if replay {
            // the hero the text was told for (its rank, its god), not a
            // later game's: none kept, none told
            let Some(h) = load_profile(&self.playground()).legacy_hero else {
                return false;
            };
            (h.role, h.female, h.align)
        } else {
            let (role, female) = self
                .prologue_choice
                .clone()
                .unwrap_or_else(|| ("valkyrie".to_string(), true));
            (role, female, 0)
        };
        // the words that agree with the hero agree with this one
        i18n::set_hero_female(if replay {
            female
        } else {
            self.hero_female.unwrap_or(female)
        });
        let Some(texts) = legacy_beats(&english) else {
            godot_warn!("renethack: the legacy text is not laid out as NetHack's");
            return false;
        };
        if !replay {
            // the text kept to replay, its hero beside it once told
            let kept = english.clone();
            self.update_profile(|p| {
                p.prologue_seen = true;
                p.legacy = kept;
                p.legacy_hero = None;
            });
            self.legacy_hero_of = Some(self.session_serial);
        }
        self.prologue = Some(PrologueShown {
            run: crate::prologue::Run::new(texts),
            english,
            role,
            female,
            align,
            replay,
        });
        self.prologue_fade = None;
        if replay {
            self.ui_mut().screens.hide();
        } else {
            self.show_game(false);
        }
        let ui = self.ui_mut();
        ui.prologue.set_visible(true);
        true
    }

    /// Whom the prologue is told for now: the engine's word on the hero
    /// once it has told it (a random role, the alignment, the pack), else
    /// the creation form's.
    pub(crate) fn prologue_hero(&self) -> crate::map_view::PrologueHero {
        let shown = self.prologue.as_ref();
        let replay = shown.is_some_and(|p| p.replay);
        let told = (!replay)
            .then(|| self.world.progress().map(|p| p.role.clone()))
            .flatten()
            .filter(|r| !r.is_empty());
        let role = told
            .or_else(|| shown.map(|p| p.role.clone()))
            .filter(|r| r != "random")
            .unwrap_or_else(|| "valkyrie".to_string());
        let female = if replay {
            shown.is_some_and(|p| p.female)
        } else {
            self.hero_female.unwrap_or(shown.is_some_and(|p| p.female))
        };
        let align = if replay {
            shown.map_or(0, |p| p.align)
        } else {
            self.hero_align().unwrap_or(0)
        };
        crate::map_view::PrologueHero {
            role,
            female,
            align,
            inventory: if replay {
                nh_world::Pack::default()
            } else {
                self.world.inventory.clone()
            },
        }
    }

    /// The prologue's frame: its run on, a beat's scene laid as it begins
    /// (the next one's built ahead while it is read), its words; over, the
    /// game (or the title) up out of its black. True in the frame it ends:
    /// the game's views come up in the next one.
    fn play_prologue(&mut self, delta: f64) -> bool {
        let catalog = self.catalog.clone();
        if self.prologue.is_none() {
            // after it, the black lifts off what it hid
            if let Some(t) = self.prologue_fade.as_mut() {
                *t += delta;
                let left = 1.0 - (*t / crate::prologue::GAME_FADE) as f32;
                let done = left <= 0.0;
                if let Some(ui) = self.ui.as_mut() {
                    if done {
                        ui.prologue.set_visible(false);
                    } else {
                        ui.prologue.set_black(left);
                    }
                }
                if done {
                    self.prologue_fade = None;
                }
            }
            return false;
        }
        let event = self.prologue.as_mut().and_then(|p| p.run.tick(delta));
        match event {
            Some(crate::prologue::Event::Beat(beat)) => {
                let hero = self.prologue_hero();
                // said again, agreeing with the hero as now known
                if let Some(p) = self.prologue.as_mut()
                    && !p.replay
                    && let Some(female) = self.hero_female
                {
                    i18n::set_hero_female(female);
                    if let Some(texts) = legacy_beats(&p.english) {
                        p.run.set_texts(texts);
                    }
                }
                if let (Some(ui), Some(cat)) = (self.ui.as_mut(), catalog.as_deref()) {
                    ui.map.show_prologue(Some(beat), &hero, cat);
                }
            }
            Some(crate::prologue::Event::Done) => {
                self.end_prologue();
                return true;
            }
            None => {}
        }
        let Some(p) = self.prologue.as_ref() else {
            return false;
        };
        let (secs, descent) = (p.run.secs(), p.run.descent());
        let run = p.run.clone();
        if let Some(ui) = self.ui.as_mut() {
            ui.map.prologue_frame(secs, descent);
            ui.prologue.update(&run);
            if let (Some(next), Some(cat)) = (run.beat().next(), catalog.as_deref())
                && secs > crate::prologue::BUILD_NEXT_AT
            {
                ui.map.prepare_prologue(next, cat);
            }
        }
        false
    }

    /// The prologue over (played out or skipped): the game it hid (or the
    /// title it was replayed over) comes up out of its black.
    fn end_prologue(&mut self) {
        let Some(p) = self.prologue.take() else {
            return;
        };
        let hero = crate::map_view::PrologueHero::default();
        let catalog = self.catalog.clone();
        if let (Some(ui), Some(cat)) = (self.ui.as_mut(), catalog.as_deref()) {
            ui.map.show_prologue(None, &hero, cat);
            ui.prologue.set_black(1.0);
        }
        self.prologue_fade = Some(0.0);
        if p.replay {
            self.show_title(None);
        } else {
            self.show_game(true);
            if std::mem::take(&mut self.new_game) {
                self.prologue_done();
            }
        }
    }

    /// A key, a button or a click while the prologue plays: Esc or B
    /// skips the rest, anything else goes on.
    fn prologue_input(&mut self, event: &Gd<InputEvent>) {
        if !event.is_pressed() || event.is_echo() {
            return;
        }
        let skip = if let Ok(k) = event.clone().try_cast::<InputEventKey>() {
            k.get_keycode() == godot::global::Key::ESCAPE
        } else if let Ok(b) = event.clone().try_cast::<InputEventJoypadButton>() {
            b.get_button_index() == godot::global::JoyButton::B
        } else {
            false
        };
        if let Some(p) = self.prologue.as_mut() {
            if skip {
                p.run.skip();
            } else {
                p.run.advance();
            }
        }
    }

    pub(crate) fn update_profile(&self, change: impl FnOnce(&mut Profile)) {
        let playground = self.playground();
        let mut profile = load_profile(&playground);
        change(&mut profile);
        let (player, device) = profile.split();
        if let Err(e) = write_profile(&playground, &player, &device) {
            godot_warn!("renethack: cannot keep the profile: {e}");
        }
    }

    /// The hero of this game, once the engine has told the role and the
    /// map has drawn the hero: the title scene's hero from now on, kept in
    /// the profile for the next start (once a game, so a polymorph later
    /// does not change it).
    fn sync_last_hero(&mut self) {
        if self.last_hero_of == Some(self.session_serial) {
            return;
        }
        let (Some(role), Some(female)) = (
            self.world.progress().map(|p| p.role.clone()),
            self.hero_female,
        ) else {
            return;
        };
        if role.is_empty() {
            return;
        }
        self.last_hero_of = Some(self.session_serial);
        self.update_profile(|p| {
            p.last_role = Some(role.clone());
            p.last_female = Some(female);
        });
        if let Some(ui) = self.ui.as_mut() {
            ui.map.set_title_hero(&role, female);
        }
    }

    /// The hero's alignment as the status tells it (1 lawful, 0 neutral,
    /// -1 chaotic); none till it has.
    fn hero_align(&self) -> Option<i32> {
        let a = self.world.status.get("align").filter(|a| !a.is_empty())?;
        Some(if a.eq_ignore_ascii_case("lawful") {
            1
        } else if a.eq_ignore_ascii_case("chaotic") {
            -1
        } else {
            0
        })
    }

    /// The hero this game's prologue was told for, to the profile beside
    /// its text once the engine has told the role, the gender and the
    /// alignment (the title replays the two together).
    fn sync_legacy_hero(&mut self) {
        if self.legacy_hero_of != Some(self.session_serial) {
            return;
        }
        let role = self
            .world
            .progress()
            .map(|p| p.role.clone())
            .filter(|r| !r.is_empty());
        let (Some(role), Some(female), Some(align)) = (role, self.hero_female, self.hero_align())
        else {
            return;
        };
        self.legacy_hero_of = None;
        self.update_profile(|p| {
            p.legacy_hero = Some(nh_world::LegacyHero {
                role,
                female,
                align,
            });
        });
    }

    /// Everything on screen in the language now.
    fn relang(&mut self) {
        self.bar_key = None;
        self.bar_labels = None;
        let pad = self.pad.active.then_some(self.pad.kind);
        let line = self.pending.as_ref().and_then(|(_, p)| match p {
            Prompt::Command => getpos_line(&self.world, pad),
            p => prompt_line(p, pad),
        });
        let catalog = self.catalog.clone();
        let guide = self.help_guide();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.hud.relang();
        ui.inventory.relang();
        // the guide's pages in the language too
        ui.help.set_guide(guide);
        ui.help.relang();
        ui.prologue.relang();
        ui.dialogs.relang(catalog.as_deref());
        ui.pad.relang();
        ui.screens.relang();
        ui.toast.relang();
        // the guide's card and page in the language
        if let Some(card) = ui.hint.shown() {
            let (title, body) = self.guide_words(card);
            if let Some(ui) = self.ui.as_mut() {
                ui.hint.rewrite(&title, &body);
            }
        }
        self.show_quest();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if line.is_some() {
            ui.hud.set_prompt_line(line.as_deref());
        }
        // the achievements page is made anew in the language, its choice kept
        if let Some(selected) = self.achievement_page.as_ref().map(|p| p.selected()) {
            let in_game = ui.screens.achievements_in_game();
            self.build_achievements(in_game, Some(selected));
            return;
        }
        // and the settings page, where it was
        if let Some(at) = self.settings_page.as_ref().map(|p| p.place()) {
            let in_game = self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.settings_in_game());
            self.build_settings(in_game, at);
        }
        // and the credits, on the same document
        if let Some(selected) = self.credits_page.as_ref().map(|p| p.selected().0) {
            let in_game = self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.screens.credits_in_game());
            self.build_credits(in_game, selected);
        }
    }

    fn show_title(&mut self, notice: Option<String>) {
        self.state = GameState::Title;
        self.show_game(false);
        self.ui_mut().game_menu.close();
        let saves = list_saves(&self.playground()).unwrap_or_else(|e| {
            godot_warn!("renethack: cannot list saves: {e}");
            Vec::new()
        });
        // the prologue again, once one was told (and its hero known)
        let profile = load_profile(&self.playground());
        let replay = !profile.legacy.is_empty() && profile.legacy_hero.is_some();
        let screens = &mut self.ui_mut().screens;
        screens.set_prologue(replay);
        screens.show_title(&saves, notice.as_deref());
    }

    fn show_creation(&mut self) {
        let Some(catalog) = self.catalog.clone() else {
            self.show_failure(&tr!("err-no-catalog"), "", None);
            return;
        };
        self.state = GameState::Creating;
        self.show_game(false);
        let keys = self.settings.keys.unwrap_or_default();
        let pg = self.playground();
        let taken = Rc::new(move |name: &str| save_exists(&pg, name));
        self.ui_mut().screens.show_creation(catalog, keys, taken);
    }

    /// The title scene shows the hero the creation form is making, and
    /// its own again once the form is gone; the pages over it hide it
    /// until it is drawn.
    fn sync_creation_preview(&mut self, delta: f64) {
        let creating = self.state == GameState::Creating;
        if let Some(ui) = self.ui.as_mut() {
            let preview = creating.then(|| ui.screens.creation_preview()).flatten();
            ui.map.set_creation_hero(preview);
            let drawn = ui.map.title_drawn();
            ui.screens.sync_scene(drawn, delta);
        }
    }

    fn check_name(&mut self, name: &str) {
        let name = name.trim();
        // an empty field: the dice choose a name no saved game has
        let taken = !name.is_empty() && save_exists(&self.playground(), name);
        let error = (!name.is_empty())
            .then(|| EngineConfig::character_options(name, "random", "random", "random", "random"))
            .and_then(Result::err)
            .map(|_| tr!("creation-name-bad"));
        let screens = &mut self.ui_mut().screens;
        screens.set_name_taken(taken);
        if !taken {
            screens.set_name_error(error.as_deref());
        }
    }

    fn show_end(&mut self) {
        // the last notice's achievements counted before the page lists them
        self.achieve(0.0);
        let summary = self.end_summary();
        self.state = GameState::Ended;
        self.show_game(false);
        self.ui_mut().screens.show_end(&summary);
    }

    /// What the end screen tells of the game just over, as the engine
    /// left it: its last progress notice, status, end window and top ten.
    fn end_summary(&self) -> EndSummary {
        let progress = self.world.progress();
        let text: Vec<String> = self
            .world
            .last_text
            .iter()
            .map(|l| l.text.clone())
            .collect();
        let told = |field: &str| {
            self.world
                .status
                .get(field)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let how = match progress.and_then(|p| p.how.as_deref()) {
            Some(how) => EndHow::of(Some(how)),
            // an older host: the summary's words
            None if text.iter().any(|l| l.contains("You quit")) => EndHow::Quit,
            None => EndHow::Died,
        };
        let female = self.hero_female.unwrap_or(false);
        let role = progress
            .zip(self.catalog.as_deref())
            .and_then(|(p, cat)| cat.roles.iter().find(|r| r.code == p.role))
            .map(|r| match (&r.name_female, female) {
                (Some(hers), true) => hers.clone(),
                _ => r.name.clone(),
            })
            .unwrap_or_default();
        // the engine's name (debug mode names every hero "wizard"), whole:
        // the stone and the status line cut a long one by its bytes
        let typed = self.name.clone().unwrap_or_default();
        let name = crate::layouts::tombstone(&text)
            .map(|(t, _)| t.name)
            .or_else(|| told("title").and_then(|t| t.split(" the ").next().map(str::to_string)))
            .or_else(|| self.name.clone())
            .unwrap_or_default();
        let name = if !name.is_empty()
            && name.len() < typed.len()
            && typed.to_lowercase().starts_with(&name.to_lowercase())
        {
            typed.clone()
        } else {
            name
        };
        let held: Vec<(nh_world::achievements::Achievement, bool)> = self
            .achievements
            .as_ref()
            .map(|t| {
                let fresh = &self.earned_in_game;
                t.held(&self.world)
                    .into_iter()
                    .map(|a| {
                        let new = fresh.0 == self.session_serial && fresh.1.contains(&a.id);
                        (a.clone(), new)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let year = crate::screens::format_time(std::time::SystemTime::now())
            .get(..4)
            .and_then(|y| y.parse().ok())
            .unwrap_or(2026);
        let roles = (self.catalog.as_deref())
            .map(|c| {
                (c.roles.iter())
                    .map(|r| (r.code.clone(), r.name.clone(), r.name_female.clone()))
                    .collect()
            })
            .unwrap_or_default();
        EndSummary {
            disclosed: self.disclosed.clone(),
            roles,
            how,
            name,
            female,
            title: told("title")
                .map(|t| crate::hud::whole_name(&t, &typed))
                .unwrap_or_default(),
            role,
            align: told("align").unwrap_or_default(),
            text,
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
            printed: self
                .world
                .raw_lines
                .iter()
                .map(|l| (l.text.clone(), l.attr & nh_world::ATR_BOLD != 0))
                .collect(),
            turns: progress
                .map(|p| p.turn)
                .filter(|t| *t > 0)
                .or_else(|| self.world.turn()),
            deepest: progress.map(|p| i64::from(p.deepest)).filter(|d| *d > 0),
            score: progress.and_then(|p| p.score),
            gold: self.world.status.number("gold"),
            level: self.world.status.number("xlevel"),
            year,
            conduct: progress.map_or_else(Vec::new, |p| {
                crate::end_view::conduct_kept(&p.conduct, p.achieved(SOKOBAN_ENTERED))
            }),
            chronicle: progress.map_or_else(Vec::new, |p| {
                p.chronicle
                    .iter()
                    .map(|e| (e.turn, e.text.clone()))
                    .collect()
            }),
            achievements: held,
        }
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
        // no name (the form names an empty field itself): the dice's
        let choice = &match self.catalog.as_deref() {
            Some(cat) if choice.name.trim().is_empty() => {
                names::named(cat, choice.clone(), names::roll(), false, |n| {
                    save_exists(&pg, n)
                })
            }
            _ => choice.clone(),
        };
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
                godot_warn!("renethack: {e}");
                let error = tr!("creation-name-bad");
                self.ui_mut().screens.set_name_error(Some(&error));
                return;
            }
        };
        if !self.prepare_playground() {
            return;
        }
        let options = if self.allow_debug() {
            format!("{options},playmode:debug")
        } else {
            options
        };
        // the Gameplay section's NetHack options lead: the engine reads the
        // list right to left, the first value wins (a saved game keeps its
        // own, from the save)
        let options = format!("{},{options}", self.settings.gameplay.engine_options());
        // the prologue tells NetHack's legacy: asked for when it plays (the
        // engine reads the list right to left: this wins over BASE_OPTIONS)
        let prologue = self.prologue_plays();
        let options = if prologue {
            format!("legacy,{options}")
        } else {
            options
        };
        if let Err(e) = remember_name(&pg, &choice.name) {
            godot_warn!("renethack: cannot remember the name: {e}");
        }
        // a new character: its profile, and the default loadout once the
        // first inventory (and the role, if random) is known
        let mut state = UiState::new(choice.profile);
        state.lang = Some(i18n::lang().code().to_string());
        if self.start_session(options, &choice.name, state) {
            self.loadout_for = Some(choice.role.clone());
            self.save_ui_state();
            self.prologue_due = prologue;
            let female = self
                .catalog
                .as_deref()
                .is_some_and(|cat| names::is_female(cat, &choice.gender));
            self.prologue_choice = Some((choice.role.clone(), female));
            self.new_game = true;
        }
    }

    fn continue_game(&mut self, name: &str) {
        // a restored game: no prologue, nothing handed on
        self.new_game = false;
        self.prologue_due = false;
        match EngineConfig::restore_options(name) {
            Ok(options) => {
                if self.prepare_playground() {
                    // a game saved in debug mode is restored in it
                    self.allow_debug();
                    // the profile and the bar kept with the save; none (a
                    // game from before them): Modern and the default loadout
                    let kept = read_ui_state(&self.playground(), name)
                        .and_then(|t| UiState::from_json(&t).ok());
                    let fresh = kept.is_none();
                    let state = kept.unwrap_or_else(|| UiState::new(KeyProfile::Modern));
                    // the character plays in its own language
                    if let Some(lang) = state.lang.as_deref().and_then(Lang::from_code) {
                        self.set_language(lang);
                    }
                    if self.start_session(options, name, state) && fresh {
                        self.loadout_for = Some("random".into());
                    }
                }
            }
            Err(e) => self.show_failure(&tr!("err-cannot-restore"), &e.to_string(), None),
        }
    }

    /// A self-test's debug mode: the playground's sysconf lets anyone be
    /// the wizard (after `prepare_playground`, which copies the data's).
    /// True when it does.
    fn allow_debug(&self) -> bool {
        if !self.debug_mode || self.selftest.is_none() {
            return false;
        }
        if let Err(e) = std::fs::write(
            self.playground().join("sysconf"),
            "WIZARDS=*\nMAXPLAYERS=10\nPANICTRACE_GDB=0\nPANICTRACE_LIBC=0\n",
        ) {
            godot_warn!("renethack: cannot allow debug mode: {e}");
        }
        true
    }

    fn prepare_playground(&mut self) -> bool {
        let Some(paths) = self.paths.clone() else {
            return false;
        };
        match create_playground(&paths.data(), &paths.playground) {
            Ok(()) => true,
            Err(e) => {
                self.show_failure(
                    &tr!("err-playground"),
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
            // NetHack does not save number_pad: every start passes it; the
            // player's own options (`O`'s, kept) last, and the client's
            // own the player set otherwise left out (the engine reads the
            // list right to left: the first of a name wins)
            options: {
                let own = &self.settings.nethack_options;
                let client = format!(
                    "{options},{CLIENT_EXTRA_OPTIONS},{}",
                    state.profile.engine_option()
                );
                let client = nh_world::without_options(&client, own);
                if own.is_empty() {
                    client
                } else {
                    format!("{client},{}", own.join(","))
                }
            },
            seed: self
                .seed
                .or_else(|| env_number("RENETHACK_SEED"))
                .or(test.then_some(SELFTEST_SEED)),
            fixed_time: env_number("RENETHACK_FIXED_TIME").or(test.then_some(SELFTEST_TIME)),
        };
        match LiveSession::start(&cfg) {
            Ok(mut session) => {
                session.read_in_pieces(self.engine_pieces);
                self.session = Some(session);
                self.session_serial += 1;
                self.world = World::new();
                if let Some(cat) = &self.catalog {
                    self.world.set_catalog(cat);
                }
                self.pending = None;
                self.typeahead.clear();
                self.reset_orders();
                self.options_run = None;
                self.gameplay_goal = None;
                self.hint_watch = nh_world::HintWatch::new();
                self.hints_waiting.clear();
                self.hint_gone_at = None;
                if let Some(ui) = self.ui.as_mut() {
                    ui.hint.clear();
                    ui.game_menu.close();
                }
                self.link_error = None;
                self.name = Some(name.to_string());
                self.state = GameState::Playing;
                let ui = self.ui_mut();
                ui.screens.hide();
                ui.map.clear();
                ui.hud.reset();
                self.show_game(true);
                self.ui_state = state;
                self.disclosed.clear();
                self.farewell_seen = false;
                // the pack as the player chose to see it, else as the
                // character's keys have it (Classic: the list)
                if let Some(paths) = self.paths.as_ref() {
                    let list = load_profile(&paths.playground).pack_list_for(self.ui_state.profile);
                    self.ui_mut().inventory.set_list(list);
                }
                self.loadout_for = None;
                self.cleared = None;
                self.bar_key = None;
                true
            }
            Err(e) => {
                self.show_failure(&tr!("err-cannot-start"), &e.to_string(), None);
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
                ui.hud.set_prompt_line(Some(&tr!("prompt-saving")));
            }
            (GameState::Closing, _) => {}
            _ => self.finish_close(),
        }
    }

    /// Leave the application (after the engine exited, or outside a game).
    pub(crate) fn finish_close(&mut self) {
        self.keep_volumes();
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
        // the hero's gender before the HUD reads a line that agrees with it
        // (the welcome comes with the hero's first drawing)
        self.sync_hero_gender();
        // the map draws the prologue's scenes meanwhile, the game behind;
        // its HUD and pack, hidden, keep up with it from the second beat
        // (the game comes up out of the black with them already drawn)
        if let Some(p) = self.prologue.as_ref() {
            if p.run.beat() != crate::prologue::Beat::Book {
                let catalog = self.catalog.clone();
                if let Some(ui) = self.ui.as_mut() {
                    ui.hud.sync(&mut self.world, catalog.as_deref());
                }
                self.sync_inventory();
            }
            return;
        }
        let catalog = self.catalog.clone();
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        if let Some(cat) = catalog.as_deref() {
            ui.map.set_reduce_effects(self.settings.reduce_effects);
            let t = Instant::now();
            ui.map.sync(&mut self.world, cat, delta);
            self.map_ms = t.elapsed().as_secs_f64() * 1000.0;
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
        // every hostile the hero sees, nearest first: the top of its own
        // model, where its arrow hangs (over a fixed height it sat on
        // whoever stood north of a grid bug)
        let hero = self.world.hero();
        let mut hostiles = match (catalog.as_deref(), hero) {
            (Some(cat), Some(_)) => nh_world::threats(&self.world, cat),
            _ => Vec::new(),
        };
        if let Some(h) = hero {
            hostiles.sort_by_key(|&(x, y)| ((x - h.0).abs().max((y - h.1).abs()), y, x));
        }
        let heads: Vec<Vector2> = hostiles
            .into_iter()
            .filter_map(|(x, y)| {
                let head = self
                    .ui
                    .as_ref()
                    .and_then(|ui| ui.map.entity_stands(x, y))
                    .map_or(Vector3::new(x as f32, 0.8, y as f32), |(at, tall)| {
                        at + Vector3::new(0.0, tall, 0.0)
                    });
                let cam = camera.as_ref()?;
                (!cam.is_position_behind(head)).then(|| cam.unproject_position(head))
            })
            .collect();
        // the hero's body on screen: feet to head, a shoulder either side
        let stands = self.ui.as_ref().and_then(|ui| ui.map.hero_stands());
        let hero_box = stands.zip(camera.as_ref()).and_then(|((p, tall), cam)| {
            if cam.is_position_behind(p) {
                return None;
            }
            let feet = cam.unproject_position(p);
            let head = cam.unproject_position(p + Vector3::new(0.0, tall, 0.0));
            let half = (cam.unproject_position(p + Vector3::new(0.4, 0.0, 0.0)).x - feet.x).abs();
            Some(Rect2::new(
                Vector2::new(feet.x - half, head.y),
                Vector2::new(2.0 * half, feet.y - head.y),
            ))
        });
        let look = self.look_tip_at(camera.as_ref());
        let Some(ui) = self.ui.as_mut() else {
            return;
        };
        ui.hud.set_cursor_at(cursor);
        ui.hud.set_threats(
            &heads,
            hero_box,
            Time::singleton().get_ticks_msec() as f64 / 1000.0,
        );
        ui.hud.set_look_tip(
            look.as_ref()
                .zip(self.look_tip.as_ref())
                .map(|(&at, tip)| (tip.text.as_str(), at)),
        );
        let t = Instant::now();
        ui.hud.set_hero_name(self.name.as_deref());
        ui.hud.sync(&mut self.world, catalog.as_deref());
        self.lap("hud", t);
        self.sync_last_hero();
        self.sync_legacy_hero();
        let t = Instant::now();
        self.sync_inventory();
        self.lap("inventory", t);
        self.update_hover();
        self.sync_orders();
        self.update_preview();
    }

    /// The engine's answer about the cell the player looked at, newly told
    /// or still up, and where on screen the thing on that cell stands
    /// (feet to head; the floor's middle for an item or the ground). None
    /// once it timed out, or while a dialog or a panel covers the map or
    /// the cell is behind the camera.
    fn look_tip_at(&mut self, camera: Option<&Gd<Camera3D>>) -> Option<Rect2> {
        if let Some(looked) = self.world.looked.take() {
            // the log's line, as the player reads it there (without the
            // tty's map symbol)
            self.look_tip = self
                .world
                .log
                .iter()
                .rev()
                .find(|m| m.seq == looked.seq)
                .map(|m| LookTip {
                    at: looked.at,
                    text: i18n::engine_message(m).into_owned(),
                    since: Instant::now(),
                });
        }
        if self
            .look_tip
            .as_ref()
            .is_some_and(|t| t.since.elapsed().as_secs_f64() > LOOK_TIP_SECS)
        {
            self.look_tip = None;
        }
        let (x, y) = self.look_tip.as_ref()?.at;
        let ui = self.ui.as_ref()?;
        if self.state != GameState::Playing || ui.dialogs.is_open() || ui.inventory.is_open() {
            return None;
        }
        let cam = camera?;
        let (feet, tall) = ui
            .map
            .entity_stands(x, y)
            .unwrap_or((Vector3::new(x as f32, 0.0, y as f32), 0.5));
        if cam.is_position_behind(feet) {
            return None;
        }
        let low = cam.unproject_position(feet);
        let high = cam.unproject_position(feet + Vector3::new(0.0, tall, 0.0));
        let half = (cam.unproject_position(feet + Vector3::new(0.4, 0.0, 0.0)).x - low.x).abs();
        Some(Rect2::new(
            Vector2::new(low.x - half, high.y),
            Vector2::new(2.0 * half, (low.y - high.y).max(1.0)),
        ))
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
        // the engine's answer over the same cell says it better
        let quiet = cell.is_some() && self.look_tip.as_ref().map(|t| t.at) == cell;
        if look != self.hover.look.as_ref() {
            self.hover.look = look.cloned();
            // the catalog's names, a line each, as the player reads them
            self.hover.text = look
                .zip(catalog.as_deref())
                .and_then(|(c, cat)| describe_cell(c, cat))
                .map(|d| {
                    d.lines()
                        .map(|l| i18n::engine(EngineKind::Name, l).into_owned())
                        .collect::<Vec<_>>()
                        .join("\n")
                });
        } else if quiet == self.hover.quiet && (self.hover.text.is_none() || pos == self.hover.pos)
        {
            return;
        }
        self.hover.pos = pos;
        self.hover.quiet = quiet;
        let text = self.hover.text.as_deref().filter(|_| !quiet);
        ui.hud.set_tooltip(text, pos);
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
        assert!(order_text(&walk, false).starts_with("Going to the stairs down"));
        assert!(order_text(&Order::Hold { key: 's' }, false).starts_with("Holding the key"));
        assert!(order_text(&Order::Rest, false).contains("any key stops"));
        assert!(order_text(&Order::Rest, true).contains("any button stops"));
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
        assert_eq!(prompt_line(&Prompt::Command, None), None);
        assert_eq!(
            prompt_line(&Prompt::Key, None).as_deref(),
            Some("Press a key")
        );
        let free = Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        };
        assert_eq!(
            prompt_line(&free, None).as_deref(),
            Some("In what direction?")
        );
        assert!(
            prompt_line(&Prompt::MapPause, None)
                .unwrap()
                .contains("More")
        );
        // a gamepad's button, never a key
        let pad = Some(PadKind::PlayStation);
        assert_eq!(prompt_line(&Prompt::Key, pad).as_deref(), Some("Press ✕"));
        assert_eq!(
            prompt_line(&Prompt::MapPause, pad).as_deref(),
            Some("--More--  (✕)")
        );
    }
}
