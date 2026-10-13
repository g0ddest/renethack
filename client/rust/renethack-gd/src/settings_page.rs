//! The settings page, from the title's button and the HUD's gear: the
//! window and how often it is drawn, what the 3D view draws, the sound's
//! volumes, the interface, and what the keys and the gamepad do (read
//! only), a section at a time.
//! A row is a setting, its value between arrows: ↑↓ (the d-pad) choose a
//! row, ←→ change it, Enter (A) steps it on, Tab or PgUp/PgDn (the
//! bumpers) turn to another section, Esc (B) goes back; the mouse clicks
//! the arrows and the tabs. A change applies at once and is kept in the
//! profile: the game does both (`RenethackGame::settings_step`), then
//! shows the page what is now (`SettingsPage::show`).

use std::cell::RefCell;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::image::Format;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, HSlider, Image, ImageTexture, InputEvent,
    InputEventMouseButton, Label, MarginContainer, PanelContainer, ScrollContainer, StyleBoxFlat,
    Texture2D, VBoxContainer,
};
use godot::global::MouseButton;
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::{
    FPS_CAPS, Graphics, Key, KeyInput, KeyProfile, Keymap, Level, Msaa, Quality, RENDER_SCALES,
    Settings, Shadows, UI_SCALES, Upscale, WindowMode, key_name, step_through, step_window_size,
};

use crate::audio::Bus;
use crate::bindings::{Binding, Part, Shown};
use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, Lang};
use crate::pad_view;
use crate::rebind::{Command, Group, Taken, taken};
use crate::theme::{self, Face, Frame};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// The page's sections, in the order of their tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Display,
    Graphics,
    Sound,
    Interface,
    Controls,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Display,
        Section::Graphics,
        Section::Sound,
        Section::Interface,
        Section::Controls,
    ];

    fn name(self) -> String {
        match self {
            Section::Display => tr!("settings-display"),
            Section::Graphics => tr!("settings-graphics"),
            Section::Sound => tr!("settings-sound"),
            Section::Interface => tr!("settings-interface"),
            Section::Controls => tr!("settings-controls"),
        }
    }

    /// Its rows, top to bottom.
    pub fn fields(self) -> &'static [Field] {
        match self {
            Section::Display => &[
                Field::WindowMode,
                Field::WindowSize,
                Field::Vsync,
                Field::FpsCap,
            ],
            Section::Graphics => &[
                Field::Quality,
                Field::Ssao,
                Field::Ssil,
                Field::Fog,
                Field::Shadows,
                Field::Msaa,
                Field::Scale,
                Field::Upscale,
            ],
            Section::Sound => &[
                Field::Volume(Bus::Master),
                Field::Volume(Bus::Music),
                Field::Volume(Bus::Ambience),
                Field::Volume(Bus::Sfx),
                Field::Volume(Bus::Ui),
            ],
            Section::Interface => &[
                Field::UiScale,
                Field::Keys,
                Field::Language,
                Field::ReduceEffects,
            ],
            // and under it, the list it chooses
            Section::Controls => &[Field::Controls],
        }
    }
}

/// A row's setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WindowMode,
    WindowSize,
    Vsync,
    FpsCap,
    /// The graphics' preset, Custom once a control below differs.
    Quality,
    Ssao,
    Ssil,
    Fog,
    Shadows,
    Msaa,
    Scale,
    Upscale,
    /// A bus's volume (`audio`): a slider.
    Volume(Bus),
    UiScale,
    /// A new character's keys.
    Keys,
    Language,
    /// Fewer screen effects (the hero's sway, the camera's drift, a
    /// hallucination's hue).
    ReduceEffects,
    /// Which controls the list shows: a key profile's, or the gamepad's.
    Controls,
}

impl Field {
    fn name(self) -> String {
        match self {
            Field::WindowMode => tr!("settings-window-mode"),
            Field::WindowSize => tr!("settings-window-size"),
            Field::Vsync => tr!("settings-vsync"),
            Field::FpsCap => tr!("settings-fps-cap"),
            Field::Quality => tr!("settings-quality"),
            Field::Ssao => tr!("settings-ssao"),
            Field::Ssil => tr!("settings-ssil"),
            Field::Fog => tr!("settings-fog"),
            Field::Shadows => tr!("settings-shadows"),
            Field::Msaa => tr!("settings-msaa"),
            Field::Scale => tr!("settings-scale"),
            Field::Upscale => tr!("settings-upscale"),
            Field::Volume(Bus::Master) => tr!("settings-volume-master"),
            Field::Volume(Bus::Music) => tr!("settings-volume-music"),
            Field::Volume(Bus::Ambience) => tr!("settings-volume-ambience"),
            Field::Volume(Bus::Sfx) => tr!("settings-volume-sfx"),
            Field::Volume(Bus::Ui) => tr!("settings-volume-ui"),
            Field::UiScale => tr!("settings-ui-scale"),
            Field::Keys => tr!("settings-keys"),
            Field::Language => tr!("settings-language"),
            Field::ReduceEffects => tr!("settings-reduce-effects"),
            Field::Controls => tr!("controls-show"),
        }
    }

    /// What it does, under the rows while it has the focus.
    fn about(self, v: &View) -> String {
        match self {
            Field::WindowMode => tr!("settings-window-mode-about"),
            Field::WindowSize if !resizable(v) => tr!("settings-window-size-fullscreen"),
            Field::WindowSize => tr!("settings-window-size-about"),
            Field::Vsync => tr!("settings-vsync-about"),
            Field::FpsCap => tr!("settings-fps-cap-about"),
            Field::Quality => tr!("settings-quality-about"),
            Field::Ssao => tr!("settings-ssao-about"),
            Field::Ssil => tr!("settings-ssil-about"),
            Field::Fog => tr!("settings-fog-about"),
            Field::Shadows => tr!("settings-shadows-about"),
            Field::Msaa => tr!("settings-msaa-about"),
            Field::Scale => tr!("settings-scale-about"),
            Field::Upscale => tr!("settings-upscale-about"),
            Field::Volume(Bus::Master) => tr!("settings-volume-master-about"),
            Field::Volume(Bus::Music) => tr!("settings-volume-music-about"),
            Field::Volume(Bus::Ambience) => tr!("settings-volume-ambience-about"),
            Field::Volume(Bus::Sfx) => tr!("settings-volume-sfx-about"),
            Field::Volume(Bus::Ui) => tr!("settings-volume-ui-about"),
            Field::UiScale if v.ui_forced.is_some() => tr!("settings-ui-scale-forced"),
            Field::UiScale => tr!("settings-ui-scale-about"),
            Field::Keys => {
                let keys = match v.settings.keys.unwrap_or_default() {
                    KeyProfile::Modern => tr!("creation-keys-modern"),
                    KeyProfile::Classic => tr!("creation-keys-classic"),
                };
                let about = tr!("settings-keys-about", keys = keys);
                match v.playing_keys {
                    Some(KeyProfile::Modern) => {
                        format!("{about} {}", tr!("settings-keys-playing-modern"))
                    }
                    Some(KeyProfile::Classic) => {
                        format!("{about} {}", tr!("settings-keys-playing-classic"))
                    }
                    None => about,
                }
            }
            Field::Language => tr!("settings-language-about"),
            Field::ReduceEffects => tr!("settings-reduce-effects-about"),
            Field::Controls => tr!("controls-show-about"),
        }
    }
}

/// What the page shows: the settings, and what they depend on.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub settings: Settings,
    pub lang: Lang,
    /// The window's size now: the size row shows it while the window
    /// keeps the size it opened with.
    pub window: (u32, u32),
    /// The screen's: the window sizes offered fit it.
    pub screen: (u32, u32),
    /// The device's own preset: the graphics until the player chooses.
    pub device: Quality,
    /// The UI scale "Auto" means here, a percent.
    pub ui_auto: u32,
    /// The UI scale `RENETHACK_UI_SCALE` forces, if it does.
    pub ui_forced: Option<u32>,
    /// The keys of the character playing (over a game).
    pub playing_keys: Option<KeyProfile>,
    /// The buses' volumes, linear, in `Bus::ALL`'s order.
    pub volumes: [f32; 5],
}

impl View {
    /// The volume of `bus` now, linear.
    pub fn volume(&self, bus: Bus) -> f32 {
        let i = Bus::ALL.iter().position(|&b| b == bus).unwrap_or(0);
        self.volumes[i]
    }

    /// The graphics drawn now: the player's, else the device's preset.
    pub fn graphics(&self) -> Graphics {
        self.settings
            .graphics
            .unwrap_or_else(|| Graphics::preset(self.device))
    }
}

/// The window size is the player's to choose (in a window only).
fn resizable(v: &View) -> bool {
    v.settings.display.mode == WindowMode::Windowed
}

/// The row can change now: the window's size in a window only, the UI
/// scale unless `RENETHACK_UI_SCALE` sets it.
fn live(field: Field, v: &View) -> bool {
    match field {
        Field::WindowSize => resizable(v),
        Field::UiScale => v.ui_forced.is_none(),
        _ => true,
    }
}

/// The UI scales offered: Auto (None), then the percents.
fn ui_scales() -> Vec<Option<u32>> {
    std::iter::once(None)
        .chain(UI_SCALES.iter().copied().map(Some))
        .collect()
}

/// A level of an effect: off, or its quality.
fn level_text(l: Level) -> String {
    match l {
        Level::Off => tr!("settings-off"),
        Level::Low => tr!("settings-quality-low"),
        Level::Medium => tr!("settings-quality-medium"),
        Level::High => tr!("settings-quality-high"),
    }
}

/// A row's value as the page shows it.
pub fn value_text(field: Field, v: &View) -> String {
    let d = &v.settings.display;
    match field {
        Field::WindowMode => match d.mode {
            WindowMode::Windowed => tr!("settings-window-windowed"),
            WindowMode::Borderless => tr!("settings-window-borderless"),
            WindowMode::Exclusive => tr!("settings-window-exclusive"),
        },
        Field::WindowSize => {
            let (w, h) = if resizable(v) {
                d.size.unwrap_or(v.window)
            } else {
                v.window
            };
            tr!("settings-size", width = w, height = h)
        }
        Field::Vsync if d.vsync => tr!("settings-on"),
        Field::Vsync => tr!("settings-off"),
        Field::FpsCap if d.fps_cap == 0 => tr!("settings-fps-none"),
        Field::FpsCap => tr!("settings-fps", fps = d.fps_cap),
        Field::Quality => match v.graphics().quality() {
            Quality::Low => tr!("settings-quality-low"),
            Quality::Medium => tr!("settings-quality-medium"),
            Quality::High => tr!("settings-quality-high"),
            Quality::Ultra => tr!("settings-quality-ultra"),
            Quality::Custom => tr!("settings-quality-custom"),
        },
        Field::Ssao => level_text(v.graphics().ssao),
        Field::Ssil => level_text(v.graphics().ssil),
        Field::Fog => level_text(v.graphics().fog),
        Field::Shadows => match v.graphics().shadows {
            Shadows::Low => tr!("settings-quality-low"),
            Shadows::Medium => tr!("settings-quality-medium"),
            Shadows::High => tr!("settings-quality-high"),
            Shadows::Ultra => tr!("settings-quality-ultra"),
        },
        Field::Msaa => match v.graphics().msaa {
            Msaa::Off => tr!("settings-off"),
            Msaa::X2 => tr!("settings-samples", samples = 2),
            Msaa::X4 => tr!("settings-samples", samples = 4),
            Msaa::X8 => tr!("settings-samples", samples = 8),
        },
        Field::Scale => tr!("settings-percent", percent = v.graphics().scale),
        Field::Upscale => match v.graphics().upscale {
            Upscale::Bilinear => tr!("settings-upscale-bilinear"),
            Upscale::Fsr1 => tr!("settings-upscale-fsr1"),
            Upscale::Fsr2 => tr!("settings-upscale-fsr2"),
        },
        Field::Volume(bus) => tr!(
            "settings-percent",
            percent = volume_steps(v.volume(bus)) * 5
        ),
        Field::UiScale => match (v.ui_forced, v.settings.ui_scale) {
            (Some(p), _) | (None, Some(p)) => tr!("settings-percent", percent = p),
            (None, None) => tr!("settings-ui-scale-auto", percent = v.ui_auto),
        },
        Field::Keys => match v.settings.keys.unwrap_or_default() {
            KeyProfile::Modern => tr!("settings-keys-modern"),
            KeyProfile::Classic => tr!("settings-keys-classic"),
        },
        Field::Language => v.lang.name().to_string(),
        Field::ReduceEffects if v.settings.reduce_effects => tr!("settings-on"),
        Field::ReduceEffects => tr!("settings-off"),
        // the page's own choice (`SettingsPage::show`)
        Field::Controls => String::new(),
    }
}

/// What the controls' list shows, as its row names it.
fn shown_text(shown: Shown) -> String {
    match shown {
        Shown::Keys(KeyProfile::Modern) => tr!("controls-shown-modern"),
        Shown::Keys(KeyProfile::Classic) => tr!("controls-shown-classic"),
        Shown::Pad => tr!("controls-shown-pad"),
    }
}

/// A volume in the sliders' steps of 5 % (0–20).
fn volume_steps(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 20.0).round() as u32
}

/// What a step of a row changes.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Settings(Settings),
    Lang(Lang),
    /// A bus's volume, linear (`UiEvent::SetVolume`).
    Volume(Bus, f32),
}

/// `field` a step on (`dir` 1) or back (-1); `wrap`: past the last value
/// to the first (Enter, A). None: nothing changes (a row that cannot
/// change now, or at its end).
pub fn step(field: Field, v: &View, dir: i32, wrap: bool) -> Option<Change> {
    let mut s = v.settings.clone();
    let d = &mut s.display;
    match field {
        Field::WindowMode => d.mode = step_through(&WindowMode::ALL, d.mode, dir, wrap),
        Field::WindowSize => {
            if !resizable(v) {
                return None;
            }
            let now = d.size.unwrap_or(v.window);
            d.size = Some(step_window_size(now, v.screen, dir, wrap));
        }
        Field::Vsync => d.vsync = step_through(&[false, true], d.vsync, dir, wrap),
        Field::FpsCap => d.fps_cap = step_through(&FPS_CAPS, d.fps_cap, dir, wrap),
        Field::Quality
        | Field::Ssao
        | Field::Ssil
        | Field::Fog
        | Field::Shadows
        | Field::Msaa
        | Field::Scale
        | Field::Upscale => {
            let now = v.graphics();
            let mut g = now;
            match field {
                Field::Quality => g = Graphics::preset(now.step_quality(dir, wrap)),
                Field::Ssao => g.ssao = step_through(&Level::ALL, g.ssao, dir, wrap),
                Field::Ssil => g.ssil = step_through(&Level::ALL, g.ssil, dir, wrap),
                Field::Fog => g.fog = step_through(&Level::ALL, g.fog, dir, wrap),
                Field::Shadows => g.shadows = step_through(&Shadows::ALL, g.shadows, dir, wrap),
                Field::Msaa => g.msaa = step_through(&Msaa::ALL, g.msaa, dir, wrap),
                Field::Scale => g.scale = step_through(&RENDER_SCALES, g.scale, dir, wrap),
                _ => g.upscale = step_through(&Upscale::ALL, g.upscale, dir, wrap),
            }
            // the device's preset stays the device's until one differs
            if g == now {
                return None;
            }
            s.graphics = Some(g);
        }
        Field::Volume(bus) => {
            let now = volume_steps(v.volume(bus));
            let steps: Vec<u32> = (0..=20).collect();
            let next = step_through(&steps, now, dir, wrap);
            return (next != now).then_some(Change::Volume(bus, next as f32 / 20.0));
        }
        Field::UiScale => {
            if !live(field, v) {
                return None;
            }
            s.ui_scale = step_through(&ui_scales(), s.ui_scale, dir, wrap);
        }
        Field::Keys => {
            let now = s.keys.unwrap_or_default();
            let keys = step_through(&[KeyProfile::Modern, KeyProfile::Classic], now, dir, wrap);
            if keys == now {
                return None;
            }
            s.keys = Some(keys);
        }
        Field::Language => {
            let lang = step_through(&Lang::ALL, v.lang, dir, wrap);
            return (lang != v.lang).then_some(Change::Lang(lang));
        }
        Field::ReduceEffects => {
            s.reduce_effects = step_through(&[false, true], s.reduce_effects, dir, wrap);
        }
        // the page's own (`SettingsPage::controls_step`)
        Field::Controls => return None,
    }
    (s != v.settings).then_some(Change::Settings(s))
}

/// What the page's widgets tell the game.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsInput {
    /// A section's tab clicked.
    Tab(Section),
    /// A row hovered or clicked: it has the focus.
    Focus(Field),
    /// A row's arrow clicked: its value a step back (-1) or on (1).
    Step(Field, i32),
    /// An entry of the controls' list clicked: a command's new key asked.
    Entry(usize),
}

/// What a key on the page asks of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    /// A step of the row with the focus (see `step`).
    Step(Field, i32, bool),
    /// The player presses `key` for `command` in `profile`; `swap`: the
    /// command that had `key` takes `command`'s.
    Rebind {
        profile: KeyProfile,
        command: KeyInput,
        key: KeyInput,
        swap: bool,
    },
    /// `command` back on its own key.
    Reset {
        profile: KeyProfile,
        command: KeyInput,
    },
    /// Every command of `profile` back on its own key.
    ResetAll(KeyProfile),
    Back,
}

/// What an entry of the controls' list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Every key back on its command.
    ResetAll,
    /// The `n`-th command of the profile's list (`rebind::commands`).
    Command(usize),
    /// What stays as it is.
    Fixed,
}

/// A command's key being changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    None,
    /// The entry waits for its new key.
    Press(usize),
    /// The key pressed is another command's: Enter swaps them.
    Swap {
        entry: usize,
        key: KeyInput,
    },
    /// Enter puts every key back.
    ResetAll,
}

/// A row: its button (the focus's frame, the mouse's target), its name,
/// its value between arrows.
struct Row {
    field: Field,
    button: Gd<Button>,
    name: Gd<Label>,
    value: Gd<Label>,
    /// A volume's slider, beside its value.
    slider: Option<Gd<HSlider>>,
    left: Gd<Button>,
    right: Gd<Button>,
}

/// Row height, and the gap between rows, in design pixels.
const ROW_H: f32 = 44.0;
const ROW_GAP: i32 = 4;
/// The value's column between its arrows, and its text's size.
const VALUE_W: f32 = 340.0;
/// A volume's percent beside its slider.
const VOLUME_W: f32 = 64.0;
const VALUE_SIZE: i32 = 19;
/// The page's width inside its frame.
const PAGE_W: f32 = 1000.0;

pub struct SettingsPage {
    root: Gd<PanelContainer>,
    tabs: Vec<(Section, Gd<Button>)>,
    bodies: Vec<(Section, Gd<VBoxContainer>)>,
    rows: Vec<Row>,
    about: Gd<Label>,
    section: Section,
    /// The row with the focus, among its section's.
    focus: usize,
    view: View,
    row_normal: Gd<StyleBoxFlat>,
    row_current: Gd<StyleBoxFlat>,
    tab_normal: Gd<StyleBoxFlat>,
    tab_current: Gd<StyleBoxFlat>,
    /// The gamepad whose buttons the hints show (None: the keyboard's).
    pad: Option<PadKind>,
    /// The gamepad whose buttons the controls show (the last one used).
    pad_kind: PadKind,
    /// The controls' list: what it shows, its scroll, its entries (and
    /// what each is), the commands of each key profile.
    controls: Shown,
    scroll: Gd<ScrollContainer>,
    list: Gd<VBoxContainer>,
    entries: Vec<Gd<PanelContainer>>,
    kinds: Vec<EntryKind>,
    commands: [Vec<Command>; 2],
    edit: Edit,
    queue: UiQueue,
    /// The entry to scroll into view once the list is laid out, and for
    /// how many frames more.
    reveal: Option<(usize, u8)>,
}

/// The profile a keyboard's list shows, and its commands' index.
fn profile_index(p: KeyProfile) -> usize {
    match p {
        KeyProfile::Modern => 0,
        KeyProfile::Classic => 1,
    }
}

fn flat(bg: Color, border: Option<(Side, i32, Color)>) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_corner_radius_all(3);
    sb.set_content_margin_all(0.0);
    if let Some((side, width, color)) = border {
        sb.set_border_width(side, width);
        sb.set_border_color(color);
    }
    sb
}

/// Every state of a button drawn with `sb`.
fn style_all(b: &mut Gd<Button>, sb: &Gd<StyleBoxFlat>) {
    for state in [
        "normal",
        "hover",
        "pressed",
        "hover_pressed",
        "disabled",
        "focus",
    ] {
        b.add_theme_stylebox_override(state, sb);
    }
}

thread_local! {
    /// The arrows' triangles, pointing left and right.
    static ARROWS: RefCell<Option<[Gd<Texture2D>; 2]>> = const { RefCell::new(None) };
}

/// The arrows' triangle on screen, design pixels across (painted at three
/// times that: crisp at any UI scale).
const ARROW_W: i32 = 12;

/// A solid triangle pointing left or right, white (its button tints it),
/// its edges smoothed.
fn triangle(right: bool) -> Option<Gd<Texture2D>> {
    const W: usize = 36;
    const H: usize = 48;
    const SS: usize = 4;
    let mut px = vec![0u8; W * H * 4];
    let (w, h) = (W as f32, H as f32);
    for y in 0..H {
        for x in 0..W {
            let mut inside = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // from the base (u 0) to the point (u 1)
                    let u = if right { fx / w } else { 1.0 - fx / w };
                    if (fy - h / 2.0).abs() <= (1.0 - u) * h / 2.0 {
                        inside += 1;
                    }
                }
            }
            let i = (y * W + x) * 4;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, (inside * 255 / (SS * SS)) as u8]);
        }
    }
    let image = Image::create_from_data(
        W as i32,
        H as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(px.as_slice()),
    )?;
    Some(ImageTexture::create_from_image(&image)?.upcast())
}

/// A row's arrow, ◀ or ▶ (`right`), in gold.
fn arrow(right: bool, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_flat(true);
    b.set_focus_mode(FocusMode::NONE);
    b.set_custom_minimum_size(Vector2::new(40.0, 36.0));
    let icons = ARROWS.with(|a| {
        a.borrow_mut()
            .get_or_insert_with(|| {
                let blank = || -> Gd<Texture2D> { ImageTexture::new_gd().upcast() };
                [
                    triangle(false).unwrap_or_else(blank),
                    triangle(true).unwrap_or_else(blank),
                ]
            })
            .clone()
    });
    b.set_button_icon(&icons[usize::from(right)]);
    b.set_icon_alignment(HorizontalAlignment::CENTER);
    b.add_theme_constant_override("icon_max_width", ARROW_W);
    for (state, color) in [
        ("icon_normal_color", theme::GOLD),
        ("icon_focus_color", theme::GOLD),
        ("icon_hover_color", theme::GOLD_BRIGHT),
        ("icon_pressed_color", theme::GOLD_BRIGHT),
        ("icon_hover_pressed_color", theme::GOLD_BRIGHT),
        ("icon_disabled_color", theme::TEXT_OFF),
    ] {
        b.add_theme_color_override(state, color);
    }
    let q = queue.clone();
    b.signals().pressed().connect(move || push(&q, ev.clone()));
    b
}

/// Let the arrows and the knobs go while Godot still runs
/// (`icons::clear`).
pub fn clear() {
    ARROWS.with(|a| a.borrow_mut().take());
    KNOBS.with(|k| k.borrow_mut().take());
}

thread_local! {
    /// The volume sliders' knob, and the knob under the mouse.
    static KNOBS: RefCell<Option<[Gd<Texture2D>; 2]>> = const { RefCell::new(None) };
}

/// A gold knob with a dark rim, its edges smoothed.
fn knob(fill: Color) -> Option<Gd<Texture2D>> {
    const D: usize = 20;
    const SS: usize = 6;
    let r = D as f32 / 2.0;
    let rim = theme::IRON_DARK;
    let mut px = vec![0u8; D * D * 4];
    for y in 0..D {
        for x in 0..D {
            let (mut inside, mut core) = (0, 0);
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = x as f32 + (sx as f32 + 0.5) / SS as f32 - r;
                    let fy = y as f32 + (sy as f32 + 0.5) / SS as f32 - r;
                    let d = (fx * fx + fy * fy).sqrt();
                    if d <= r - 0.25 {
                        inside += 1;
                        if d <= r - 2.25 {
                            core += 1;
                        }
                    }
                }
            }
            let t = if inside == 0 {
                0.0
            } else {
                core as f32 / inside as f32
            };
            let mix = |a: f32, b: f32| ((a + (b - a) * t) * 255.0).round() as u8;
            let i = (y * D + x) * 4;
            px[i..i + 4].copy_from_slice(&[
                mix(rim.r, fill.r),
                mix(rim.g, fill.g),
                mix(rim.b, fill.b),
                (inside * 255 / (SS * SS)) as u8,
            ]);
        }
    }
    let image = Image::create_from_data(
        D as i32,
        D as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(px.as_slice()),
    )?;
    Some(ImageTexture::create_from_image(&image)?.upcast())
}

/// A volume's slider: 0 to 1 in steps of 5 %, a gold groove in a dark
/// one, a gold knob; a drag sets the volume at once.
fn volume_slider(bus: Bus, queue: &UiQueue) -> Gd<HSlider> {
    let mut s = HSlider::new_alloc();
    s.set_min(0.0);
    s.set_max(1.0);
    s.set_step(0.05);
    s.set_focus_mode(FocusMode::NONE);
    s.set_h_size_flags(SizeFlags::EXPAND_FILL);
    s.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    s.set_custom_minimum_size(Vector2::new(0.0, 24.0));
    let groove = |bg: Color, border: Color| {
        let mut sb = StyleBoxFlat::new_gd();
        sb.set_bg_color(bg);
        sb.set_border_width_all(1);
        sb.set_border_color(border);
        sb.set_corner_radius_all(3);
        sb.set_content_margin(Side::TOP, 3.0);
        sb.set_content_margin(Side::BOTTOM, 3.0);
        sb
    };
    s.add_theme_stylebox_override("slider", &groove(theme::SOCKET, theme::IRON));
    s.add_theme_stylebox_override("grabber_area", &groove(theme::GOLD_DIM, theme::GOLD_DIM));
    s.add_theme_stylebox_override("grabber_area_highlight", &groove(theme::GOLD, theme::GOLD));
    let knobs = KNOBS.with(|k| {
        k.borrow_mut()
            .get_or_insert_with(|| {
                let blank = || -> Gd<Texture2D> { ImageTexture::new_gd().upcast() };
                [
                    knob(theme::GOLD).unwrap_or_else(blank),
                    knob(theme::GOLD_BRIGHT).unwrap_or_else(blank),
                ]
            })
            .clone()
    });
    s.add_theme_icon_override("grabber", &knobs[0]);
    s.add_theme_icon_override("grabber_highlight", &knobs[1]);
    let q = queue.clone();
    s.signals()
        .value_changed()
        .connect(move |v: f64| push(&q, UiEvent::SetVolume(bus, v as f32)));
    s
}

/// A key's cap: its name as keyboards print it, in a dark frame.
fn keycap(name: &str) -> Gd<PanelContainer> {
    let mut cap = PanelContainer::new_alloc();
    cap.set_mouse_filter(MouseFilter::IGNORE);
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.10, 0.08, 0.06, 1.0));
    sb.set_border_width_all(1);
    sb.set_border_color(Color {
        a: 0.7,
        ..theme::TEXT_DIM
    });
    sb.set_corner_radius_all(4);
    sb.set_content_margin(Side::LEFT, 7.0);
    sb.set_content_margin(Side::RIGHT, 7.0);
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    cap.add_theme_stylebox_override("panel", &sb);
    cap.set_custom_minimum_size(Vector2::new(28.0, 28.0));
    let mut l = theme::styled_label(name, Face::BodyBold, 15, theme::TEXT);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    // the name printed on the key
    i18n::verbatim(&l);
    cap.add_child(&l);
    cap
}

/// Frames a list's entry is kept in view after the list changed.
const REVEAL_FRAMES: u8 = 4;

/// The controls' input cell's width, and an entry's height at least.
const INPUT_W: f32 = 440.0;
const ENTRY_H: f32 = 38.0;

/// An entry of the controls' list: its input drawn on the left (key caps,
/// the gamepad's buttons, words), what it does on the right.
fn entry(binding: &Binding, kind: PadKind, normal: &Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    let mut e = PanelContainer::new_alloc();
    // the wheel goes through to the list's scroll
    e.set_mouse_filter(MouseFilter::PASS);
    // as tall with words alone as with key caps
    e.set_custom_minimum_size(Vector2::new(0.0, ENTRY_H));
    e.add_theme_stylebox_override("panel", normal);
    let mut line = HBoxContainer::new_alloc();
    line.set_mouse_filter(MouseFilter::IGNORE);
    line.add_theme_constant_override("separation", 16);
    let mut pad = MarginContainer::new_alloc();
    pad.set_mouse_filter(MouseFilter::IGNORE);
    for (side, px) in [("left", 18), ("right", 10), ("top", 4), ("bottom", 4)] {
        pad.add_theme_constant_override(&format!("margin_{side}"), px);
    }
    let mut input = HBoxContainer::new_alloc();
    input.set_mouse_filter(MouseFilter::IGNORE);
    input.add_theme_constant_override("separation", 4);
    input.set_custom_minimum_size(Vector2::new(INPUT_W, 0.0));
    for &part in binding.input {
        match part {
            Part::Key(name) => input.add_child(&keycap(name)),
            Part::Pad(b) => input.add_child(&pad_view::medallion(kind, b)),
            Part::Words(id) => {
                let mut l = theme::styled_label(&i18n::tr(id), Face::Body, 16, theme::TEXT_DIM);
                l.set_vertical_alignment(VerticalAlignment::CENTER);
                input.add_child(&l);
            }
            Part::Plus | Part::Or => {
                let mark = if part == Part::Plus { "+" } else { "/" };
                let mut l = theme::styled_label(mark, Face::Body, 16, theme::TEXT_DIM);
                l.set_vertical_alignment(VerticalAlignment::CENTER);
                // a mark, the same in every language
                i18n::verbatim(&l);
                input.add_child(&l);
            }
        }
    }
    line.add_child(&input);
    let mut action = theme::styled_label(&i18n::tr(binding.action), Face::Body, 17, theme::TEXT);
    action.set_vertical_alignment(VerticalAlignment::CENTER);
    action.set_autowrap_mode(AutowrapMode::WORD_SMART);
    action.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line.add_child(&action);
    pad.add_child(&line);
    e.add_child(&pad);
    e
}

/// A group's name over its commands in the controls' list.
fn group_label(name: &str) -> Gd<Label> {
    let mut l = theme::styled_label(name, Face::Title, 20, theme::GOLD);
    l.set_custom_minimum_size(Vector2::new(0.0, 34.0));
    l.set_vertical_alignment(VerticalAlignment::BOTTOM);
    l
}

/// An entry's frame and its line inside it.
fn entry_frame(normal: &Gd<StyleBoxFlat>) -> (Gd<PanelContainer>, Gd<HBoxContainer>) {
    let mut e = PanelContainer::new_alloc();
    e.set_mouse_filter(MouseFilter::PASS);
    e.set_custom_minimum_size(Vector2::new(0.0, ENTRY_H));
    e.add_theme_stylebox_override("panel", normal);
    let mut pad = MarginContainer::new_alloc();
    pad.set_mouse_filter(MouseFilter::IGNORE);
    for (side, px) in [("left", 18), ("right", 10), ("top", 4), ("bottom", 4)] {
        pad.add_theme_constant_override(&format!("margin_{side}"), px);
    }
    let mut line = HBoxContainer::new_alloc();
    line.set_mouse_filter(MouseFilter::IGNORE);
    line.add_theme_constant_override("separation", 16);
    pad.add_child(&line);
    e.add_child(&pad);
    (e, line)
}

/// The entry that puts every key back (dim while none moved).
fn reset_entry(nothing_moved: bool, normal: &Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    let (e, mut line) = entry_frame(normal);
    let color = if nothing_moved {
        theme::TEXT_OFF
    } else {
        theme::GOLD_BRIGHT
    };
    let mut l = theme::styled_label(&tr!("controls-reset-all"), Face::BodyBold, 17, color);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    line.add_child(&l);
    e
}

/// A command's entry: its key (gold when moved, "…" while one is awaited),
/// its name, its own key when it is not on it.
fn command_entry(
    c: &Command,
    map: &Keymap,
    normal: &Gd<StyleBoxFlat>,
    edit: Option<Edit>,
) -> Gd<PanelContainer> {
    let (e, mut line) = entry_frame(normal);
    let mut input = HBoxContainer::new_alloc();
    input.set_mouse_filter(MouseFilter::IGNORE);
    input.add_theme_constant_override("separation", 4);
    input.set_custom_minimum_size(Vector2::new(INPUT_W, 0.0));
    let now = map.key_for(c.key);
    match (edit, now) {
        (Some(Edit::Press(_)), _) => input.add_child(&waiting_cap()),
        (Some(Edit::Swap { key, .. }), _) => input.add_child(&gold_cap(&key_name(&key))),
        (_, Some(k)) if k != c.key => input.add_child(&gold_cap(&key_name(&k))),
        (_, Some(k)) => input.add_child(&keycap(&key_name(&k))),
        (_, None) => {
            let mut l = theme::styled_label("—", Face::Body, 16, theme::TEXT_DIM);
            i18n::verbatim(&l);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            input.add_child(&l);
        }
    }
    line.add_child(&input);
    let mut name = theme::styled_label(&c.name, Face::Body, 17, theme::TEXT);
    name.set_vertical_alignment(VerticalAlignment::CENTER);
    name.set_autowrap_mode(AutowrapMode::WORD_SMART);
    name.set_h_size_flags(SizeFlags::EXPAND_FILL);
    // the engine's words (translated by it) and the client's
    i18n::verbatim(&name);
    line.add_child(&name);
    if now != Some(c.key) {
        let mut own = theme::styled_label(
            &tr!("controls-own-key", key = key_name(&c.key)),
            Face::Body,
            14,
            theme::TEXT_DIM,
        );
        own.set_vertical_alignment(VerticalAlignment::CENTER);
        line.add_child(&own);
    }
    e
}

/// A key cap in gold: a key the player moved.
fn gold_cap(name: &str) -> Gd<PanelContainer> {
    let mut cap = keycap(name);
    if let Some(mut l) = cap.get_child(0).and_then(|c| c.try_cast::<Label>().ok()) {
        l.add_theme_color_override("font_color", theme::GOLD_BRIGHT);
    }
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.16, 0.12, 0.06, 1.0));
    sb.set_border_width_all(1);
    sb.set_border_color(theme::GOLD);
    sb.set_corner_radius_all(4);
    sb.set_content_margin(Side::LEFT, 7.0);
    sb.set_content_margin(Side::RIGHT, 7.0);
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    cap.add_theme_stylebox_override("panel", &sb);
    cap
}

/// A key cap waiting for its key.
fn waiting_cap() -> Gd<PanelContainer> {
    let mut cap = gold_cap("…");
    cap.set_custom_minimum_size(Vector2::new(56.0, 28.0));
    cap
}

impl SettingsPage {
    /// The page showing `view`, open on `section` with the focus on its
    /// `focus`-th row; the hints for a gamepad of `pad`'s kind, else for
    /// the keyboard.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        view: View,
        queue: &UiQueue,
        section: Section,
        focus: usize,
        pad: Option<PadKind>,
        pad_kind: PadKind,
        controls: Shown,
        commands: [Vec<Command>; 2],
    ) -> SettingsPage {
        let mut root = theme::framed(Frame::Panel);
        root.set_mouse_filter(MouseFilter::STOP);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 12);
        col.set_custom_minimum_size(Vector2::new(PAGE_W, 0.0));
        let title = theme::styled_label(&tr!("settings-title"), Face::Title, 32, theme::ACCENT);
        col.add_child(&title);

        // the sections' tabs, the current one underlined in gold
        let mut tab_normal = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), None);
        let mut tab_current = flat(
            Color {
                a: 0.12,
                ..theme::ACCENT
            },
            Some((Side::BOTTOM, 2, theme::GOLD_BRIGHT)),
        );
        // a long name (Russian's) keeps clear of its neighbours
        for sb in [&mut tab_normal, &mut tab_current] {
            sb.set_content_margin(Side::LEFT, 14.0);
            sb.set_content_margin(Side::RIGHT, 14.0);
        }
        let mut tab_row = HBoxContainer::new_alloc();
        tab_row.add_theme_constant_override("separation", 8);
        let mut tabs = Vec::new();
        for s in Section::ALL {
            let mut b = Button::new_alloc();
            b.set_text(&s.name());
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(150.0, 40.0));
            b.add_theme_font_override("font", &theme::font(Face::Title));
            b.add_theme_font_size_override("font_size", 24);
            b.add_theme_color_override("font_hover_color", theme::TEXT);
            let q = queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::Settings(SettingsInput::Tab(s))));
            tab_row.add_child(&b);
            tabs.push((s, b));
        }
        col.add_child(&tab_row);
        let mut rule = ColorRect::new_alloc();
        rule.set_color(Color {
            a: 0.6,
            ..theme::GOLD_DIM
        });
        rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
        rule.set_mouse_filter(MouseFilter::IGNORE);
        col.add_child(&rule);

        // the rows, each section's in a body of its own: one shows
        let row_normal = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), None);
        let row_current = flat(
            Color {
                a: 0.16,
                ..theme::ACCENT
            },
            Some((Side::LEFT, 3, theme::GOLD_BRIGHT)),
        );
        let most = Section::ALL
            .iter()
            .map(|s| s.fields().len())
            .max()
            .unwrap_or(1) as f32;
        let mut holder = VBoxContainer::new_alloc();
        holder.set_custom_minimum_size(Vector2::new(0.0, most * (ROW_H + ROW_GAP as f32)));
        let mut bodies = Vec::new();
        let mut rows = Vec::new();
        let mut scroll = ScrollContainer::new_alloc();
        let list = VBoxContainer::new_alloc();
        for s in Section::ALL {
            let mut body = VBoxContainer::new_alloc();
            body.add_theme_constant_override("separation", ROW_GAP);
            body.set_v_size_flags(SizeFlags::EXPAND_FILL);
            for &field in s.fields() {
                let row = Self::row(field, queue, &row_normal);
                body.add_child(&row.button);
                rows.push(row);
            }
            if s == Section::Controls {
                // the list under its row, scrolled in the room the
                // sections share
                scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
                scroll.set_horizontal_scroll_mode(
                    godot::classes::scroll_container::ScrollMode::DISABLED,
                );
                let mut list = list.clone();
                list.add_theme_constant_override("separation", 2);
                list.set_h_size_flags(SizeFlags::EXPAND_FILL);
                scroll.add_child(&list);
                body.add_child(&scroll);
            }
            holder.add_child(&body);
            bodies.push((s, body));
        }
        col.add_child(&holder);

        let mut about = theme::styled_label("", Face::Body, 17, theme::TEXT_DIM);
        about.set_autowrap_mode(AutowrapMode::WORD_SMART);
        about.set_custom_minimum_size(Vector2::new(PAGE_W, 52.0));
        about.set_vertical_alignment(VerticalAlignment::TOP);
        col.add_child(&about);

        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 18);
        let mut hint = Self::hint(pad);
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        foot.add_child(&hint);
        foot.add_child(&theme::button(
            &tr!("settings-credits"),
            queue,
            UiEvent::OpenCredits,
        ));
        foot.add_child(&theme::button(
            &tr!("settings-back"),
            queue,
            UiEvent::CloseSettings,
        ));
        col.add_child(&foot);
        root.add_child(&col);

        let mut page = SettingsPage {
            root,
            tabs,
            bodies,
            rows,
            about,
            section,
            focus,
            view: view.clone(),
            row_normal,
            row_current,
            tab_normal,
            tab_current,
            pad,
            pad_kind,
            controls,
            scroll,
            list,
            entries: Vec::new(),
            kinds: Vec::new(),
            commands,
            edit: Edit::None,
            queue: queue.clone(),
            reveal: None,
        };
        page.fill_controls();
        page.show(&view);
        page.turn_to(section, focus);
        page
    }

    /// The controls' list for what it shows now: a keyboard's commands,
    /// each on its key (a click or Enter asks another), by group, then what
    /// stays as it is; the gamepad's, read only.
    fn fill_controls(&mut self) {
        // out of the list at once (freed at the frame's end): the list's
        // size and its scroll are the new entries' only
        for mut c in self.list.get_children().iter_shared() {
            self.list.remove_child(&c);
            c.queue_free();
        }
        self.entries.clear();
        self.kinds.clear();
        if let Shown::Keys(profile) = self.controls {
            let map = self.view.settings.keymaps.get(profile).clone();
            self.add_entry(
                reset_entry(map.is_empty(), &self.row_normal),
                EntryKind::ResetAll,
            );
            let list = self.commands[profile_index(profile)].clone();
            for group in Group::ALL {
                let mut first = true;
                for (i, c) in list.iter().enumerate().filter(|(_, c)| c.group == group) {
                    if first {
                        self.list.add_child(&group_label(&group.name()));
                        first = false;
                    }
                    let edit = self.edit_at(self.entries.len());
                    let e = command_entry(c, &map, &self.row_normal, edit);
                    self.add_entry(e, EntryKind::Command(i));
                }
            }
            self.list
                .add_child(&group_label(&tr!("controls-group-fixed")));
        }
        for binding in self.controls.bindings() {
            let e = entry(&binding, self.pad_kind, &self.row_normal);
            self.add_entry(e, EntryKind::Fixed);
        }
        let text = shown_text(self.controls);
        if let Some(r) = self.rows.iter_mut().find(|r| r.field == Field::Controls) {
            r.value.set_text(&text);
        }
    }

    /// An entry under the list's, a click on it asking for a new key.
    fn add_entry(&mut self, e: Gd<PanelContainer>, kind: EntryKind) {
        let i = self.entries.len();
        if kind != EntryKind::Fixed {
            let q = self.queue.clone();
            e.signals().gui_input().connect(move |ev: Gd<InputEvent>| {
                if let Ok(b) = ev.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                    && b.get_button_index() == MouseButton::LEFT
                {
                    push(&q, UiEvent::Settings(SettingsInput::Entry(i)));
                }
            });
        }
        self.list.add_child(&e);
        self.entries.push(e);
        self.kinds.push(kind);
    }

    /// The edit the `entry`-th entry shows, if it is its.
    fn edit_at(&self, entry: usize) -> Option<Edit> {
        match self.edit {
            Edit::Press(e) | Edit::Swap { entry: e, .. } if e == entry => Some(self.edit),
            _ => None,
        }
    }

    /// The controls' row a step on or back: another list.
    pub fn controls_step(&mut self, dir: i32, wrap: bool) {
        let next = step_through(&Shown::ALL, self.controls, dir, wrap);
        if next != self.controls {
            self.controls = next;
            self.edit = Edit::None;
            self.fill_controls();
            self.scroll.set_v_scroll(0);
            self.style_rows();
        }
    }

    /// The entry with the focus, and what it is.
    fn focused_entry(&self) -> Option<(usize, EntryKind)> {
        if self.section != Section::Controls {
            return None;
        }
        let i = self.focus.checked_sub(self.section.fields().len())?;
        self.kinds.get(i).map(|k| (i, *k))
    }

    /// The profile the list shows (a keyboard's).
    fn shown_profile(&self) -> Option<KeyProfile> {
        match self.controls {
            Shown::Keys(p) => Some(p),
            Shown::Pad => None,
        }
    }

    /// A click on the `i`-th entry: its focus, and what Enter would do.
    pub fn click_entry(&mut self, i: usize) {
        let first = self.section.fields().len();
        self.focus = first + i;
        self.edit = Edit::None;
        if let Some(k) = self.start_edit() {
            self.edit = k;
        }
        self.refresh_edit();
    }

    /// Enter (or a click) on the entry with the focus: what it starts
    /// (nothing to put back while no key moved).
    fn start_edit(&self) -> Option<Edit> {
        match self.focused_entry()? {
            (i, EntryKind::Command(_)) => Some(Edit::Press(i)),
            (_, EntryKind::ResetAll) => {
                let p = self.shown_profile()?;
                let moved = !self.view.settings.keymaps.get(p).is_empty();
                moved.then_some(Edit::ResetAll)
            }
            (_, EntryKind::Fixed) => None,
        }
    }

    /// The list and the words under it after an edit began or ended.
    fn refresh_edit(&mut self) {
        let focus = self.focus;
        let scroll = self.scroll.get_v_scroll();
        self.fill_controls();
        self.focus = focus;
        self.style_rows();
        self.scroll.set_v_scroll(scroll);
        self.show_about();
    }

    /// The command of the `i`-th entry: its profile and its own key.
    fn entry_command(&self, i: usize) -> Option<(KeyProfile, KeyInput)> {
        let profile = self.shown_profile()?;
        match self.kinds.get(i)? {
            EntryKind::Command(c) => self.commands[profile_index(profile)]
                .get(*c)
                .map(|cmd| (profile, cmd.key)),
            _ => None,
        }
    }

    /// A key pressed while an entry waits for one: taken, the command's;
    /// taken by another, Enter swaps them; a key that stays, another.
    fn key_for_entry(&mut self, entry: usize, key: KeyInput) -> Option<PageKey> {
        let (profile, command) = self.entry_command(entry)?;
        let list = &self.commands[profile_index(profile)];
        let map = self.view.settings.keymaps.get(profile);
        match taken(list, map, profile, key, command) {
            Taken::Free => {
                self.edit = Edit::None;
                self.refresh_edit();
                Some(PageKey::Rebind {
                    profile,
                    command,
                    key,
                    swap: false,
                })
            }
            Taken::Fixed(what) => {
                self.about
                    .set_text(&tr!("controls-fixed", key = key_name(&key), what = what));
                None
            }
            Taken::Command(_) => {
                self.edit = Edit::Swap { entry, key };
                self.refresh_edit();
                None
            }
        }
    }

    /// The edit under way (self-tests).
    pub fn edit(&self) -> Edit {
        self.edit
    }

    /// The entry of the shown list's command whose own key is `command`
    /// (self-tests).
    pub fn entry_of(&self, command: KeyInput) -> Option<usize> {
        let profile = self.shown_profile()?;
        let list = &self.commands[profile_index(profile)];
        let c = list.iter().position(|cmd| cmd.key == command)?;
        self.kinds.iter().position(|k| *k == EntryKind::Command(c))
    }

    /// Give the `i`-th entry the focus (self-tests).
    pub fn focus_entry(&mut self, i: usize) {
        self.focus = self.section.fields().len() + i;
        self.style_rows();
        self.show_about();
    }

    /// The entry with the focus scrolled into view, the list laid out
    /// since it changed (the game calls it every frame: a new list takes
    /// a few frames to lay its entries out, the words that wrap last).
    pub fn reveal_focus(&mut self) {
        let Some((i, frames)) = self.reveal else {
            return;
        };
        self.reveal = frames.checked_sub(1).map(|f| (i, f));
        let Some(e) = self.entries.get(i) else {
            self.reveal = None;
            return;
        };
        let (top, h) = (e.get_position().y, e.get_size().y);
        let view = self.scroll.get_size().y;
        let at = self.scroll.get_v_scroll() as f32;
        if top < at {
            self.scroll.set_v_scroll(top as i32);
        } else if top + h > at + view {
            self.scroll.set_v_scroll((top + h - view).ceil() as i32);
        }
    }

    /// What the controls' list shows (self-tests).
    pub fn controls(&self) -> Shown {
        self.controls
    }

    /// The controls' list as the player reads it: each entry's input and
    /// action (self-tests).
    pub fn control_texts(&self) -> Vec<String> {
        fn texts(node: &Gd<godot::classes::Node>, out: &mut Vec<String>) {
            if let Ok(l) = node.clone().try_cast::<Label>() {
                out.push(l.get_text().to_string());
            }
            for c in node.get_children().iter_shared() {
                texts(&c, out);
            }
        }
        self.entries
            .iter()
            .map(|e| {
                let mut out = Vec::new();
                texts(&e.clone().upcast(), &mut out);
                out.join(" ")
            })
            .collect()
    }

    /// The rows the focus moves through: the section's, and the controls'
    /// entries under their row.
    fn row_count(&self) -> usize {
        let n = self.section.fields().len();
        if self.section == Section::Controls {
            n + self.entries.len()
        } else {
            n
        }
    }

    /// A row of `field`: its name on the left, its value between arrows
    /// on the right.
    fn row(field: Field, queue: &UiQueue, normal: &Gd<StyleBoxFlat>) -> Row {
        let mut button = Button::new_alloc();
        button.set_focus_mode(FocusMode::NONE);
        button.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
        style_all(&mut button, normal);
        let q = queue.clone();
        button
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::Settings(SettingsInput::Focus(field))));
        let q = queue.clone();
        button
            .signals()
            .mouse_entered()
            .connect(move || push(&q, UiEvent::Settings(SettingsInput::Focus(field))));
        let mut line = HBoxContainer::new_alloc();
        line.set_mouse_filter(MouseFilter::IGNORE);
        line.add_theme_constant_override("separation", 6);
        theme::place(&line, [0.0, 0.0, 1.0, 1.0], [18.0, 0.0, -10.0, 0.0]);
        let mut name = theme::styled_label(&field.name(), Face::Body, 19, theme::TEXT);
        name.set_vertical_alignment(VerticalAlignment::CENTER);
        name.set_h_size_flags(SizeFlags::EXPAND_FILL);
        line.add_child(&name);
        let left = arrow(
            false,
            queue,
            UiEvent::Settings(SettingsInput::Step(field, -1)),
        );
        line.add_child(&left);
        let mut value = theme::styled_label("", Face::BodyBold, VALUE_SIZE, theme::GOLD_BRIGHT);
        value.set_horizontal_alignment(HorizontalAlignment::CENTER);
        value.set_vertical_alignment(VerticalAlignment::CENTER);
        value.set_custom_minimum_size(Vector2::new(VALUE_W, 0.0));
        value.set_clip_text(true);
        if field == Field::Language {
            // each language in its own name, whatever the language now
            i18n::verbatim(&value);
        }
        // a volume: a slider the mouse drags, its percent beside it
        let slider = match field {
            Field::Volume(bus) => {
                let mut cell = HBoxContainer::new_alloc();
                cell.set_mouse_filter(MouseFilter::IGNORE);
                cell.add_theme_constant_override("separation", 12);
                cell.set_custom_minimum_size(Vector2::new(VALUE_W, 0.0));
                let s = volume_slider(bus, queue);
                cell.add_child(&s);
                value.set_custom_minimum_size(Vector2::new(VOLUME_W, 0.0));
                cell.add_child(&value);
                line.add_child(&cell);
                Some(s)
            }
            _ => {
                line.add_child(&value);
                None
            }
        };
        let right = arrow(
            true,
            queue,
            UiEvent::Settings(SettingsInput::Step(field, 1)),
        );
        line.add_child(&right);
        button.add_child(&line);
        Row {
            field,
            button,
            name,
            value,
            slider,
            left,
            right,
        }
    }

    /// The keys under the page: the keyboard's, or a gamepad's buttons.
    fn hint(pad: Option<PadKind>) -> Gd<Control> {
        let Some(kind) = pad else {
            let mut l = theme::styled_label(
                &i18n::whole_parts(&tr!("settings-hint")),
                Face::Body,
                16,
                theme::TEXT_DIM,
            );
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            return l.upcast();
        };
        use PadButton::*;
        let mut strip = HBoxContainer::new_alloc();
        strip.set_mouse_filter(MouseFilter::IGNORE);
        strip.add_theme_constant_override("separation", 16);
        let parts: [(&[PadButton], String); 4] = [
            (&[Up, Down], tr!("hint-choose")),
            (&[Left, Right], tr!("hint-change")),
            (&[Lb, Rb], tr!("hint-section")),
            (&[B], tr!("hint-back")),
        ];
        for (buttons, what) in parts {
            let mut item = HBoxContainer::new_alloc();
            item.set_mouse_filter(MouseFilter::IGNORE);
            item.add_theme_constant_override("separation", 4);
            for &b in buttons {
                item.add_child(&pad_view::medallion(kind, b));
            }
            let mut l = theme::styled_label(&what, Face::Body, 16, theme::TEXT_DIM);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            item.add_child(&l);
            strip.add_child(&item);
        }
        strip.upcast()
    }

    /// What the screens show.
    pub fn root(&self) -> Gd<Control> {
        self.root.clone().upcast()
    }

    /// The values of `view` in the rows; a row that cannot change now
    /// dimmed, its arrows gone.
    pub fn show(&mut self, view: &View) {
        let keys_moved = self.view.settings.keymaps != view.settings.keymaps;
        self.view = view.clone();
        if keys_moved {
            self.refresh_edit();
        }
        let controls = shown_text(self.controls);
        for r in &mut self.rows {
            r.value.set_text(&if r.field == Field::Controls {
                controls.clone()
            } else {
                value_text(r.field, view)
            });
            if let (Field::Volume(bus), Some(s)) = (r.field, r.slider.as_mut()) {
                // no signal: the slider only tells of the player's drags
                s.set_value_no_signal(f64::from(view.volume(bus)));
            }
            let live = live(r.field, view);
            let (name, value) = if live {
                (theme::TEXT, theme::GOLD_BRIGHT)
            } else {
                (theme::TEXT_OFF, theme::TEXT_OFF)
            };
            r.name.add_theme_color_override("font_color", name);
            r.value.add_theme_color_override("font_color", value);
            for a in [&mut r.left, &mut r.right] {
                a.set_disabled(!live);
                a.set_modulate(if live {
                    Color::WHITE
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.0)
                });
            }
        }
        self.show_about();
    }

    /// What the page shows now.
    pub fn view(&self) -> &View {
        &self.view
    }

    /// The gamepad whose buttons the hints show.
    pub fn pad(&self) -> Option<PadKind> {
        self.pad
    }

    fn show_about(&mut self) {
        // an entry of the controls' list: what it does, and an edit's state
        if let Some((i, kind)) = self.focused_entry() {
            let name = |c: usize| {
                self.shown_profile()
                    .and_then(|p| self.commands[profile_index(p)].get(c))
                    .map(|cmd| cmd.name.clone())
                    .unwrap_or_default()
            };
            let text = match (kind, self.edit) {
                (EntryKind::Command(c), Edit::Press(e)) if e == i => {
                    tr!("controls-press", name = name(c))
                }
                (EntryKind::Command(_), Edit::Swap { entry, key }) if entry == i => {
                    let holder = self
                        .shown_profile()
                        .and_then(|p| self.view.settings.keymaps.get(p).translate(key))
                        .and_then(|h| {
                            let p = self.shown_profile()?;
                            self.commands[profile_index(p)].iter().find(|c| c.key == h)
                        })
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    tr!("controls-taken", key = key_name(&key), name = holder)
                }
                (EntryKind::Command(_), _) => tr!("controls-edit-about"),
                (EntryKind::ResetAll, Edit::ResetAll) => tr!("controls-reset-all-confirm"),
                (EntryKind::ResetAll, _) => tr!("controls-reset-all-about"),
                (EntryKind::Fixed, _) => tr!("controls-show-about"),
            };
            self.about.set_text(&text);
            return;
        }
        // in the controls' list, what its row says
        let field = self
            .field()
            .or((self.section == Section::Controls).then_some(Field::Controls));
        let text = field.map(|f| f.about(&self.view)).unwrap_or_default();
        self.about.set_text(&text);
    }

    /// Turn to `section`, the focus on its `focus`-th row.
    fn turn_to(&mut self, section: Section, focus: usize) {
        self.section = section;
        for (s, body) in &mut self.bodies {
            body.set_visible(*s == section);
        }
        for (s, tab) in &mut self.tabs {
            let current = *s == section;
            style_all(
                tab,
                if current {
                    &self.tab_current
                } else {
                    &self.tab_normal
                },
            );
            let color = if current {
                theme::GOLD_BRIGHT
            } else {
                theme::TEXT_DIM
            };
            tab.add_theme_color_override("font_color", color);
            tab.add_theme_color_override("font_pressed_color", color);
        }
        self.focus = focus.min(self.row_count().saturating_sub(1));
        self.style_rows();
        self.show_about();
    }

    fn style_rows(&mut self) {
        let field = self.field();
        for r in &mut self.rows {
            let sb = if Some(r.field) == field {
                &self.row_current
            } else {
                &self.row_normal
            };
            style_all(&mut r.button, sb);
        }
        // an entry of the controls' list with the focus: framed, in view
        let first = self.section.fields().len();
        let at =
            (self.section == Section::Controls && self.focus >= first).then(|| self.focus - first);
        for (i, e) in self.entries.iter_mut().enumerate() {
            let sb = if Some(i) == at {
                &self.row_current
            } else {
                &self.row_normal
            };
            e.add_theme_stylebox_override("panel", sb);
        }
        // into view once the list is laid out (the next frame's sync)
        if let Some(i) = at {
            self.reveal = Some((i, REVEAL_FRAMES));
        } else if self.section == Section::Controls {
            self.scroll.set_v_scroll(0);
        }
    }

    /// The section shown.
    pub fn section(&self) -> Section {
        self.section
    }

    /// The row with the focus.
    pub fn field(&self) -> Option<Field> {
        self.section.fields().get(self.focus).copied()
    }

    /// Where the page is: its section and the focus's row (to build it
    /// again in another language).
    pub fn place(&self) -> (Section, usize) {
        (self.section, self.focus)
    }

    /// Give `field` the focus (its section shown).
    pub fn focus_field(&mut self, field: Field) {
        for s in Section::ALL {
            if let Some(i) = s.fields().iter().position(|&f| f == field) {
                if s != self.section || i != self.focus {
                    self.turn_to(s, i);
                }
                return;
            }
        }
    }

    /// A tab clicked.
    pub fn tab(&mut self, section: Section) {
        if section != self.section {
            self.turn_to(section, 0);
        }
    }

    /// The next section (`dir` 1) or the one before, round.
    fn turn(&mut self, dir: i32) {
        let next = step_through(&Section::ALL, self.section, dir, true);
        self.turn_to(next, 0);
    }

    /// A key: ↑↓ choose a row, Tab and PgUp/PgDn a section (here); ←→
    /// and Enter change the row's value and Esc goes back (the game's).
    pub fn key(&mut self, k: &KeyInput, from_pad: bool) -> Option<PageKey> {
        // an edit has the keys first: the key a command waits for, the
        // swap's Enter, Esc to give up (a gamepad's buttons are no keys)
        match self.edit {
            Edit::None => {}
            _ if k.echo => return None,
            _ if k.key == Key::Escape => {
                self.edit = Edit::None;
                self.refresh_edit();
                return None;
            }
            Edit::ResetAll if matches!(k.key, Key::Enter | Key::KeypadEnter) => {
                self.edit = Edit::None;
                let profile = self.shown_profile()?;
                self.refresh_edit();
                return Some(PageKey::ResetAll(profile));
            }
            Edit::Swap { entry, key } if matches!(k.key, Key::Enter | Key::KeypadEnter) => {
                self.edit = Edit::None;
                let (profile, command) = self.entry_command(entry)?;
                self.refresh_edit();
                return Some(PageKey::Rebind {
                    profile,
                    command,
                    key,
                    swap: true,
                });
            }
            Edit::Press(entry) | Edit::Swap { entry, .. } if !from_pad => {
                return self.key_for_entry(entry, *k);
            }
            _ => return None,
        }
        // an entry: Enter asks its new key, Delete puts its own back
        if let Some((i, kind)) = self.focused_entry() {
            match k.key {
                Key::Enter | Key::KeypadEnter | Key::Char(' ') if !k.echo => {
                    if let Some(edit) = self.start_edit() {
                        self.edit = edit;
                        self.refresh_edit();
                    }
                    return None;
                }
                Key::Delete | Key::Backspace
                    if !k.echo && matches!(kind, EntryKind::Command(_)) =>
                {
                    let (profile, command) = self.entry_command(i)?;
                    return Some(PageKey::Reset { profile, command });
                }
                _ => {}
            }
        }
        let n = self.row_count();
        // the controls' row changes the list here
        if self.field() == Some(Field::Controls) {
            let step = match k.key {
                Key::Left => Some((-1, false)),
                Key::Right => Some((1, false)),
                Key::Enter | Key::KeypadEnter | Key::Char(' ') if !k.echo => Some((1, true)),
                _ => None,
            };
            if let Some((dir, wrap)) = step {
                self.controls_step(dir, wrap);
                return None;
            }
        }
        match k.key {
            Key::Up if n > 0 => {
                self.focus = (self.focus + n - 1) % n;
                self.style_rows();
                self.show_about();
            }
            Key::Down if n > 0 => {
                self.focus = (self.focus + 1) % n;
                self.style_rows();
                self.show_about();
            }
            Key::Tab if !k.echo => self.turn(if k.mods.shift { -1 } else { 1 }),
            Key::PageUp if !k.echo => self.turn(-1),
            Key::PageDown if !k.echo => self.turn(1),
            Key::Left => return self.field().map(|f| PageKey::Step(f, -1, false)),
            Key::Right => return self.field().map(|f| PageKey::Step(f, 1, false)),
            Key::Enter | Key::KeypadEnter | Key::Char(' ') if !k.echo => {
                return self.field().map(|f| PageKey::Step(f, 1, true));
            }
            Key::Escape if !k.echo => return Some(PageKey::Back),
            _ => {}
        }
        None
    }

    /// The page's frame on screen (self-tests: it fits the canvas).
    pub fn rect(&self) -> Rect2 {
        self.root.get_global_rect()
    }

    /// The values wider than their place between the arrows (self-tests:
    /// every value fits, in every language).
    pub fn overflows(&self) -> Vec<String> {
        let font = theme::font(Face::BodyBold);
        self.rows
            .iter()
            .filter_map(|r| {
                let text = r.value.get_text();
                let w = font
                    .get_string_size_ex(&text)
                    .font_size(VALUE_SIZE)
                    .done()
                    .x;
                let room = r.value.get_size().x.max(VALUE_W);
                (w > room + 0.5).then(|| format!("{:?} \"{text}\" ({w:.0} > {room:.0})", r.field))
            })
            .collect()
    }

    /// The slider of `bus`'s volume (self-tests: a drag of the mouse).
    pub fn slider(&self, bus: Bus) -> Option<Gd<HSlider>> {
        self.rows
            .iter()
            .find(|r| r.field == Field::Volume(bus))
            .and_then(|r| r.slider.clone())
    }

    /// The rows of the section shown: each one's name and value as the
    /// player reads them (self-tests).
    pub fn shown_rows(&self) -> Vec<(String, String)> {
        let fields = self.section.fields();
        self.rows
            .iter()
            .filter(|r| fields.contains(&r.field))
            .map(|r| {
                (
                    r.name.get_text().to_string(),
                    r.value.get_text().to_string(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_world::Display;

    fn view() -> View {
        View {
            settings: Settings::default(),
            lang: Lang::En,
            window: (1600, 900),
            screen: (1920, 1080),
            device: Quality::High,
            ui_auto: 100,
            ui_forced: None,
            playing_keys: None,
            volumes: Bus::ALL.map(Bus::default_volume),
        }
    }

    fn changed(c: Option<Change>) -> Settings {
        match c {
            Some(Change::Settings(s)) => s,
            other => panic!("not a change of the settings: {other:?}"),
        }
    }

    #[test]
    fn a_step_changes_its_row_s_setting() {
        let v = view();
        let s = changed(step(Field::WindowMode, &v, 1, false));
        assert_eq!(s.display.mode, WindowMode::Borderless);
        let s = changed(step(Field::FpsCap, &v, 1, false));
        assert_eq!(s.display.fps_cap, 30);
        let s = changed(step(Field::Vsync, &v, -1, false));
        assert!(!s.display.vsync);
        // at the end: nothing changes, unless it wraps (Enter)
        assert_eq!(step(Field::Vsync, &v, 1, false), None);
        assert!(!changed(step(Field::Vsync, &v, 1, true)).display.vsync);
        assert_eq!(
            step(Field::Language, &v, 1, false),
            Some(Change::Lang(Lang::Ru))
        );
        // fewer screen effects: off, then on (→, or Enter's wrap)
        assert_eq!(step(Field::ReduceEffects, &v, -1, false), None);
        assert!(changed(step(Field::ReduceEffects, &v, 1, false)).reduce_effects);
        assert!(changed(step(Field::ReduceEffects, &v, 1, true)).reduce_effects);
    }

    #[test]
    fn the_window_size_steps_from_the_window_s_own_in_a_window_only() {
        let v = view();
        let s = changed(step(Field::WindowSize, &v, 1, false));
        assert_eq!(s.display.size, Some((1920, 1080)));
        let s = changed(step(Field::WindowSize, &v, -1, false));
        assert_eq!(s.display.size, Some((1366, 768)));
        let full = View {
            settings: Settings {
                display: Display {
                    mode: WindowMode::Borderless,
                    ..Display::default()
                },
                ..Settings::default()
            },
            ..view()
        };
        assert_eq!(step(Field::WindowSize, &full, 1, false), None);
    }

    #[test]
    fn the_graphics_follow_the_device_until_one_control_differs() {
        let v = view();
        assert_eq!(v.graphics(), Graphics::preset(Quality::High));
        // the preset a step down: chosen, kept
        let s = changed(step(Field::Quality, &v, -1, false));
        assert_eq!(s.graphics, Some(Graphics::preset(Quality::Medium)));
        // a control by hand: Custom
        let s = changed(step(Field::Ssil, &v, -1, false));
        let g = s.graphics.expect("chosen");
        assert_eq!((g.ssil, g.quality()), (Level::Low, Quality::Custom));
        // at an end nothing changes, and the device's preset stays its own
        let top = View {
            device: Quality::Ultra,
            ..view()
        };
        assert_eq!(step(Field::Msaa, &top, 1, false), None);
        assert_eq!(step(Field::Quality, &top, 1, false), None);
        // the upscaler and the scale step through their own
        let s = changed(step(Field::Scale, &v, -1, false));
        assert_eq!(s.graphics.map(|g| g.scale), Some(85));
        let s = changed(step(Field::Upscale, &v, 1, false));
        assert_eq!(s.graphics.map(|g| g.upscale), Some(Upscale::Fsr1));
    }

    #[test]
    fn the_ui_scale_steps_from_auto_unless_forced_and_the_keys_between_two() {
        let v = view();
        let s = changed(step(Field::UiScale, &v, 1, false));
        assert_eq!(s.ui_scale, Some(80));
        assert_eq!(
            step(Field::UiScale, &v, -1, false),
            None,
            "Auto is the first"
        );
        let top = View {
            settings: Settings {
                ui_scale: Some(140),
                ..Settings::default()
            },
            ..view()
        };
        assert_eq!(changed(step(Field::UiScale, &top, 1, true)).ui_scale, None);
        let forced = View {
            ui_forced: Some(120),
            ..view()
        };
        assert_eq!(step(Field::UiScale, &forced, 1, false), None);
        let s = changed(step(Field::Keys, &v, 1, false));
        assert_eq!(s.keys, Some(KeyProfile::Classic));
        assert_eq!(
            step(Field::Keys, &v, -1, false),
            None,
            "Modern is the first"
        );
    }

    #[test]
    fn a_volume_steps_by_five_percent_between_silence_and_full() {
        let v = view();
        assert_eq!(
            step(Field::Volume(Bus::Music), &v, -1, false),
            Some(Change::Volume(Bus::Music, 0.65))
        );
        assert_eq!(step(Field::Volume(Bus::Master), &v, 1, false), None);
        assert_eq!(
            step(Field::Volume(Bus::Master), &v, 1, true),
            Some(Change::Volume(Bus::Master, 0.0))
        );
    }

    #[test]
    fn every_section_has_rows_and_every_row_one_section() {
        for s in Section::ALL {
            assert!(!s.fields().is_empty(), "{s:?}");
        }
        let all: Vec<Field> = Section::ALL
            .iter()
            .flat_map(|s| s.fields().iter().copied())
            .collect();
        for (i, f) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(f), "{f:?} twice");
        }
    }
}
