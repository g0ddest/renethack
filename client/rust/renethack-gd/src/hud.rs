//! The play screen's HUD (ui-design §1): the portrait block, the HP and Pw
//! orbs, the XP bar, the action bar and its micro-buttons, the message
//! log, the minimap with the mode badge and the order line, the prompt
//! banner, the hover tooltip and the full log.
//!
//! Only the HUD's blocks take the mouse (`MOUSE_FILTER_STOP`); everything
//! else lets it through to the map.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, GrowDirection, MouseFilter, SizeFlags};
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, HBoxContainer, HFlowContainer, InputEvent,
    InputEventMouseButton, Label, PanelContainer, RichTextLabel, ShaderMaterial, StyleBoxFlat,
    Time, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::{Key, KeyInput, Message, Mods, Status, World};

use crate::action_bar::{self, ActionBar};
use crate::minimap::{self, Minimap};
use crate::orb::{self, Orb};
use crate::theme::{self, Face, Frame, bbcode_escape, hex, hex_alpha, place};
use crate::ui_events::{UiEvent, UiQueue, push};

/// Messages kept in the log panel; the wheel scrolls through them (the
/// full log has the rest).
const LOG_LINES: usize = 200;
/// The screen edge the blocks keep, in design pixels.
const EDGE: f32 = 24.0;
const PORTRAIT_W: f32 = 520.0;
const PORTRAIT_H: f32 = 176.0;
const LOG_W: f32 = 404.0;
const LOG_H: f32 = 260.0;
/// The docked log of the compact layout, above the cluster's left half.
const LOG_COMPACT_W: f32 = 480.0;
const LOG_COMPACT_H: f32 = 132.0;
/// The orbs beside the bar's frame, never over its slots: 148 + 4 + 736
/// + 4 + 148.
const CLUSTER_W: f32 = 1040.0;
const CLUSTER_H: f32 = 176.0;
const CLUSTER_BOTTOM: f32 = 12.0;
/// The action bar's frame: the slots plus 12 of padding each side.
const BAR_W: f32 = action_bar::WIDTH + 24.0;
const BAR_H: f32 = action_bar::SLOT + 24.0;
const MINIMAP_W: f32 = 424.0;
const MINIMAP_H: f32 = 150.0;
const MODE_W: f32 = 240.0;
/// Seconds before a log line fades to `LOG_FADED` (the compact log sooner).
const LOG_FADE_SECS: f64 = 10.0;
const LOG_FADE_COMPACT_SECS: f64 = 6.0;
const LOG_FADED: f32 = 0.45;
/// Tooltip offset from the mouse.
const TOOLTIP_GAP: f32 = 18.0;
/// How long a toast stays (ui-design §4.2: "Slot 4 cleared · Undo").
const TOAST_SECS: f64 = 4.0;
/// Deadly chips pulse this many times a second (a 1.2 s cycle).
const PULSE_HZ: f64 = 1.0 / 1.2;
/// Seconds the banner of a new mode stays, fading out in the last third.
const FLASH_SECS: f64 = 1.6;
/// Seconds a changed attribute stays green or red.
const ATTR_FLASH_SECS: f64 = 1.5;

const COMBAT_TEXT: Color = Color::from_rgb(1.0, 0.42, 0.32);
const EXPLORE_TEXT: Color = theme::GOLD;
/// The mode badge's frame is tinted red in combat.
const COMBAT_TINT: Color = Color::from_rgb(1.0, 0.62, 0.55);

// ---- what the HUD shows (plain data, tested without Godot) ----

/// How alarming a chip is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Kills unless cured: red and pulsing.
    Deadly,
    /// Dangerous now.
    Bad,
    /// Impairs the hero.
    Warn,
    /// Worth knowing.
    Info,
}

/// Conditions that kill unless cured (botl.c names; "Strangled" in case a
/// catalog spells it out).
const DEADLY: [&str; 6] = [
    "Stone",
    "Slime",
    "Strngl",
    "Strangled",
    "FoodPois",
    "TermIll",
];
const BAD: [&str; 5] = ["InLava", "Grab", "Parlyz", "Out", "Zzz"];
const IMPAIRED: [&str; 12] = [
    "Blind", "Conf", "Stun", "Hallu", "Deaf", "Held", "Trap", "WLegs", "Iron", "Submrg", "Teth",
    "Slip",
];

pub fn condition_tone(name: &str) -> Tone {
    if DEADLY.contains(&name) {
        Tone::Deadly
    } else if BAD.contains(&name) {
        Tone::Bad
    } else if IMPAIRED.contains(&name) {
        Tone::Warn
    } else {
        Tone::Info
    }
}

/// The words of the deadly banner for a deadly condition.
pub fn deadly_words(name: &str) -> Option<&'static str> {
    Some(match name {
        "Stone" => "Turning to stone!",
        "Slime" => "Turning into slime!",
        "Strngl" | "Strangled" => "Strangled!",
        "FoodPois" => "Food poisoning!",
        "TermIll" => "Terminally ill!",
        _ => return None,
    })
}

/// Fainting leads straight to starving: as deadly as stoning.
fn hunger_tone(hunger: &str) -> Tone {
    match hunger {
        "Satiated" | "Hungry" => Tone::Warn,
        "Fainting" | "Fainted" | "Starved" => Tone::Deadly,
        _ => Tone::Bad,
    }
}

fn encumbrance_tone(cap: &str) -> Tone {
    match cap {
        "Burdened" => Tone::Warn,
        _ => Tone::Bad,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub text: String,
    pub tone: Tone,
}

/// What the status shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusView {
    /// Name and rank title ("Hero the Stripling").
    pub title: String,
    pub align: Option<String>,
    /// (tag, value): Dlvl, $, AC, XL and Exp (or HD when polymorphed), T, S.
    /// An empty tag shows the value alone ("Home 2").
    pub fields: Vec<(String, String)>,
    pub hp: Option<(i64, i64)>,
    pub pw: Option<(i64, i64)>,
    /// St Dx Co In Wi Ch.
    pub attrs: Vec<(&'static str, String)>,
    /// Hunger and encumbrance, when not the usual.
    pub needs: Vec<Chip>,
    /// The conditions in catalog order.
    pub conditions: Vec<Chip>,
}

impl StatusView {
    /// `conditions`: names of the conditions in effect (`Status::condition_names`).
    pub fn new(status: &Status, conditions: &[String]) -> StatusView {
        let text = |f: &str| status.get(f).filter(|v| !v.is_empty()).map(str::to_string);
        let pair = |a: &str, b: &str| Some((status.number(a)?, status.number(b)?));
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut field = |tag: &str, value: String| fields.push((tag.to_string(), value));
        if let Some(d) = text("leveldesc") {
            match d.split_once(':') {
                Some((tag, v)) => field(tag.trim(), v.trim().to_string()),
                None => field("", d),
            }
        }
        if let Some(g) = text("gold") {
            let n = status.number("gold");
            field(
                "$",
                n.map_or_else(|| g.trim_matches(':').to_string(), |n| n.to_string()),
            );
        }
        if let Some(ac) = text("ac") {
            field("AC", ac);
        }
        // a polymorphed hero has hit dice instead of an experience level
        match (text("hitdice"), text("xlevel")) {
            (Some(hd), None) => field("HD", hd),
            (_, Some(xl)) => {
                field("XL", xl);
                if let Some(e) = text("exp") {
                    field("Exp", e);
                }
            }
            (None, None) => {}
        }
        if let Some(t) = text("time") {
            field("T", t);
        }
        if let Some(s) = text("score") {
            field("S", s);
        }
        let attrs = [
            ("St", "str"),
            ("Dx", "dex"),
            ("Co", "con"),
            ("In", "int"),
            ("Wi", "wis"),
            ("Ch", "cha"),
        ]
        .into_iter()
        .filter_map(|(tag, f)| text(f).map(|v| (tag, v)))
        .collect();
        let mut needs = Vec::new();
        if let Some(h) = text("hunger") {
            let tone = hunger_tone(&h);
            needs.push(Chip { text: h, tone });
        }
        if let Some(c) = text("cap") {
            let tone = encumbrance_tone(&c);
            needs.push(Chip { text: c, tone });
        }
        let conditions = conditions
            .iter()
            .map(|c| Chip {
                text: c.clone(),
                tone: condition_tone(c),
            })
            .collect();
        StatusView {
            title: text("title").unwrap_or_default(),
            align: text("align"),
            fields,
            hp: pair("hp", "hpmax"),
            pw: pair("energy", "energymax"),
            attrs,
            needs,
            conditions,
        }
    }

    /// The value of the field tagged `tag`.
    pub fn field(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, v)| v.as_str())
    }

    /// Where the hero is ("Dlvl 1", "Home 2"): the first field.
    pub fn place(&self) -> Option<String> {
        let (tag, v) = self.fields.first()?;
        match tag.as_str() {
            "$" | "AC" | "XL" | "Exp" | "HD" | "T" | "S" => None,
            "" => Some(v.clone()),
            _ => Some(format!("{tag} {v}")),
        }
    }

    /// Hunger, encumbrance, then the conditions.
    pub fn chips(&self) -> impl Iterator<Item = &Chip> {
        self.needs.iter().chain(&self.conditions)
    }

    /// The status as the classic bottom lines ("Dlvl:1  $0  AC:6",
    /// "HP:16(16)  Pw:2(2)"...): the block's tooltip, and self-tests.
    pub fn lines(&self) -> Vec<String> {
        let mut head = self.title.clone();
        if let Some(a) = &self.align {
            head.push_str(&format!("  {a}"));
        }
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|(tag, v)| match tag.as_str() {
                "" => v.clone(),
                "$" => format!("${v}"),
                _ => format!("{tag}:{v}"),
            })
            .collect();
        let bars: Vec<String> = [("HP", self.hp), ("Pw", self.pw)]
            .into_iter()
            .filter_map(|(tag, p)| p.map(|(v, m)| format!("{tag}:{v}({m})")))
            .collect();
        let attrs: Vec<String> = self
            .attrs
            .iter()
            .map(|(tag, v)| format!("{tag}:{v}"))
            .collect();
        let mut lines = vec![head, fields.join("  "), bars.join("  "), attrs.join(" ")];
        let chips: Vec<&str> = self.chips().map(|c| c.text.as_str()).collect();
        if !chips.is_empty() {
            lines.push(chips.join(" "));
        }
        lines
    }
}

/// Experience points needed for level `lev + 1` (exper.c `newuexp`: rule
/// knowledge, the same for every character).
pub fn newuexp(lev: i64) -> i64 {
    if lev < 1 {
        0
    } else if lev < 10 {
        10 * (1 << lev)
    } else if lev < 20 {
        10_000 * (1 << (lev - 10))
    } else {
        10_000_000 * (lev - 19)
    }
}

/// How far the hero is from level `lev` to the next: (share, next level's
/// threshold).
pub fn xp_progress(lev: i64, exp: i64) -> (f32, i64) {
    let (from, to) = (newuexp(lev - 1), newuexp(lev));
    let share = if to > from {
        ((exp - from) as f64 / (to - from) as f64).clamp(0.0, 1.0) as f32
    } else {
        1.0
    };
    (share, to)
}

/// pray.c `critically_low_hp(FALSE)`: the engine's own "low HP" (what
/// prayer fixes), from the shown HP and the experience level.
pub fn critically_low_hp(hp: i64, max: i64, lev: i64) -> bool {
    let max = max.min(15 * lev);
    let rank = if lev <= 2 {
        0
    } else if lev <= 30 {
        (lev + 2) / 4
    } else {
        8
    };
    let divisor = match rank {
        0 | 1 => 5,
        2 | 3 => 6,
        4 | 5 => 7,
        6 | 7 => 8,
        _ => 9,
    };
    hp <= 5 || hp * divisor <= max
}

/// An attribute's value for comparing ("18/50" above "18", "18/**" above
/// all of them).
pub fn attr_rank(v: &str) -> Option<f32> {
    match v.split_once('/') {
        Some((base, "**")) => Some(base.trim().parse::<f32>().ok()? + 1.0),
        Some((base, pct)) => {
            Some(base.trim().parse::<f32>().ok()? + pct.parse::<f32>().ok()? / 100.0)
        }
        None => v.trim().parse().ok(),
    }
}

/// The compact layout (ui-design §1.6): the log docks over the cluster
/// when there is no room for it beside the cluster.
pub fn compact(virtual_width: f32) -> bool {
    (virtual_width - CLUSTER_W) / 2.0 - 40.0 < 380.0
}

/// "valkyrie" → "Valkyrie", "wood nymph" → "Wood Nymph".
fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a message looks in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Arrived after the player's last input (and not restored from a save).
    pub new: bool,
    pub urgent: bool,
}

pub fn message_look(m: &Message, input_seq: u64) -> Look {
    Look {
        new: m.seq > input_seq && !m.from_history,
        urgent: m.urgent,
    }
}

/// A message as BBCode: new ones bright, old ones dimmed, urgent ones in
/// the warning colour (and bold while new); `faded` ones at 45 %.
fn message_bbcode(m: &Message, input_seq: u64, faded: bool) -> String {
    let look = message_look(m, input_seq);
    let text = bbcode_escape(&m.text);
    let color = match (look.urgent, look.new) {
        (true, true) => theme::DANGER,
        (true, false) => theme::DANGER.lerp(theme::TEXT_DIM, 0.45),
        (false, true) => theme::TEXT,
        (false, false) => theme::TEXT_DIM,
    };
    let color = if faded {
        Color {
            a: LOG_FADED,
            ..color
        }
    } else {
        color
    };
    if look.new {
        format!("[color={}][b]{text}[/b][/color]", hex_alpha(color))
    } else {
        format!("[color={}]{text}[/color]", hex_alpha(color))
    }
}

// ---- Godot nodes ----

/// (background, border, text) of a chip.
fn chip_colors(tone: Tone) -> (Color, Color, Color) {
    match tone {
        Tone::Deadly => (
            Color::from_rgba(0.42, 0.05, 0.04, 0.95),
            Color::from_rgb(1.0, 0.36, 0.28),
            Color::from_rgb(1.0, 0.92, 0.88),
        ),
        Tone::Bad => (
            Color::from_rgba(0.3, 0.12, 0.03, 0.92),
            Color::from_rgb(0.95, 0.5, 0.2),
            Color::from_rgb(1.0, 0.72, 0.45),
        ),
        Tone::Warn => (
            Color::from_rgba(0.22, 0.16, 0.05, 0.92),
            theme::WARN,
            Color::from_rgb(0.98, 0.82, 0.52),
        ),
        Tone::Info => (
            Color::from_rgba(0.1, 0.08, 0.06, 0.92),
            theme::GOLD_DIM,
            theme::TEXT,
        ),
    }
}

/// The deadly chips' pulse now: 0..1.
fn pulse_now(now: f64) -> f32 {
    (0.5 + 0.5 * (now * TAU * PULSE_HZ).sin()) as f32
}

fn chip_style(bg: Color, border: Color) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(1);
    sb.set_border_color(border);
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    sb.set_content_margin(Side::LEFT, 8.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    sb
}

/// An engraved glyph of `ui_icon.gdshader`, `size` pixels square.
fn icon(which: i32, size: f32, color: Color) -> Gd<ColorRect> {
    let mut r = ColorRect::new_alloc();
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.set_custom_minimum_size(Vector2::new(size, size));
    r.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    match theme::ui_material("ui_icon") {
        Some(mut m) => {
            m.set_shader_parameter("icon", &which.to_variant());
            m.set_shader_parameter("color", &color.to_variant());
            r.set_material(&m);
        }
        None => r.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0)),
    }
    r
}

/// A medallion of `ui_ring.gdshader`.
fn ring(size: f32, inner: f32, rim: Color) -> (Gd<ColorRect>, Option<Gd<ShaderMaterial>>) {
    let mut r = ColorRect::new_alloc();
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.set_custom_minimum_size(Vector2::new(size, size));
    r.set_size(Vector2::new(size, size));
    r.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    let m = theme::ui_material("ui_ring");
    match &m {
        Some(m) => {
            let mut m = m.clone();
            m.set_shader_parameter("inner", &inner.to_variant());
            m.set_shader_parameter("rim", &rim.to_variant());
            r.set_material(&m);
        }
        None => r.set_color(rim),
    }
    (r, m)
}

/// A value chip with a glyph: AC, gold.
struct ValueChip {
    panel: Gd<PanelContainer>,
    text: Gd<Label>,
}

impl ValueChip {
    fn new(glyph: i32) -> ValueChip {
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::PASS);
        panel.add_theme_stylebox_override(
            "panel",
            &chip_style(Color::from_rgba(0.06, 0.045, 0.035, 0.9), theme::IRON),
        );
        let mut row = hbox(4);
        row.add_child(&icon(glyph, 18.0, theme::GOLD));
        let text = theme::styled_label("", Face::BodyBold, 16, theme::TEXT);
        row.add_child(&text);
        panel.add_child(&row);
        panel.set_custom_minimum_size(Vector2::new(0.0, 26.0));
        ValueChip { panel, text }
    }

    fn set(&mut self, value: Option<&str>, tip: &str) {
        self.text.set_text(value.unwrap_or(""));
        self.panel.set_visible(value.is_some());
        self.panel.set_tooltip_text(tip);
    }
}

/// A chip on screen (hunger, encumbrance, a condition) and what pulses in it.
struct ShownChip {
    chip: Chip,
    node: Gd<PanelContainer>,
    style: Gd<StyleBoxFlat>,
    gem: Option<Gd<ShaderMaterial>>,
}

/// A need chip (hunger, encumbrance): text in a toned frame.
fn need_chip(c: &Chip) -> ShownChip {
    let (bg, border, fg) = chip_colors(c.tone);
    let style = chip_style(bg, border);
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.add_theme_stylebox_override("panel", &style);
    p.set_custom_minimum_size(Vector2::new(0.0, 26.0));
    p.add_child(&theme::styled_label(&c.text, Face::BodyBold, 16, fg));
    ShownChip {
        chip: c.clone(),
        node: p,
        style,
        gem: None,
    }
}

/// A condition: a gem set in a ring, toned by the condition's danger, and
/// its name (the glyphs of phase I replace the name with an icon).
fn condition_chip(c: &Chip) -> ShownChip {
    let (bg, border, fg) = chip_colors(c.tone);
    let rim = match c.tone {
        Tone::Info => theme::GOLD,
        _ => border,
    };
    let mut style = chip_style(Color { a: 0.85, ..bg }, Color { a: 0.8, ..border });
    style.set_content_margin(Side::LEFT, 3.0);
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::PASS);
    p.add_theme_stylebox_override("panel", &style);
    p.set_custom_minimum_size(Vector2::new(0.0, 26.0));
    p.set_tooltip_text(&c.text);
    let mut row = hbox(5);
    let (gem, material) = ring(20.0, 0.62, rim);
    if let Some(m) = &material {
        let mut m = m.clone();
        m.set_shader_parameter("face_center", &rim.lerp(Color::WHITE, 0.15).to_variant());
        m.set_shader_parameter("face_edge", &rim.darkened(0.7).to_variant());
    }
    row.add_child(&gem);
    row.add_child(&theme::styled_label(&c.text, Face::BodyBold, 15, fg));
    p.add_child(&row);
    ShownChip {
        chip: c.clone(),
        node: p,
        style,
        gem: material,
    }
}

/// Seconds since the engine started (for fades).
fn now_secs() -> f64 {
    Time::singleton().get_ticks_msec() as f64 / 1000.0
}

/// The banner's opacity `t` seconds after the mode changed.
pub fn flash_alpha(t: f64) -> f32 {
    let fade = FLASH_SECS / 3.0;
    if !(0.0..FLASH_SECS).contains(&t) {
        0.0
    } else if t < FLASH_SECS - fade {
        1.0
    } else {
        ((FLASH_SECS - t) / fade) as f32
    }
}

/// Replace a scrolling label's text; show the end (`follow`) or stay where
/// the wheel left it.
fn set_scrolled_text(label: &mut Gd<RichTextLabel>, text: &str, follow: bool) {
    let bar = label.get_v_scroll_bar();
    let keep = bar.as_ref().filter(|_| !follow).map(|b| b.get_value());
    label.set_text(text);
    // counting lines lays the text out, so the scroll range is current
    let last = (label.get_line_count() - 1).max(0);
    match (keep, bar) {
        (Some(v), Some(mut b)) => b.set_value(v),
        _ => label.scroll_to_line(last),
    }
}

fn rich(mouse: MouseFilter) -> Gd<RichTextLabel> {
    let mut r = RichTextLabel::new_alloc();
    r.set_use_bbcode(true);
    r.set_focus_mode(FocusMode::NONE);
    r.set_selection_enabled(false);
    r.set_mouse_filter(mouse);
    r
}

fn vbox(separation: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", separation);
    b
}

fn hbox(separation: i32) -> Gd<HBoxContainer> {
    let mut b = HBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", separation);
    b
}

fn ctrl(c: char) -> KeyInput {
    KeyInput {
        key: Key::Char(c),
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        echo: false,
    }
}

/// An icon-only button (36×28) with a tooltip naming it and its key.
fn micro_button(glyph: i32, tip: &str, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = theme::button("", queue, ev);
    b.set_custom_minimum_size(Vector2::new(36.0, 28.0));
    b.set_tooltip_text(tip);
    b.set_mouse_filter(MouseFilter::STOP);
    for state in ["normal", "hover", "pressed", "hover_pressed", "disabled"] {
        if let Some(mut sb) = b.get_theme_stylebox(state) {
            // the default content margins are for words
            sb = sb.duplicate_resource();
            sb.set_content_margin_all(0.0);
            b.add_theme_stylebox_override(state, &sb);
        }
    }
    let mut g = icon(glyph, 20.0, theme::GOLD);
    place(&g, [0.5, 0.5, 0.5, 0.5], [-10.0, -10.0, 10.0, 10.0]);
    g.set_v_size_flags(SizeFlags::FILL);
    b.add_child(&g);
    b
}

/// Queue a key from a signal: the same as pressing it.
fn key_event(k: KeyInput) -> UiEvent {
    UiEvent::Key(k)
}

/// (HP, Pw) on the orbs, and each slot's (filled, enabled).
pub type ClusterView = (Option<(i64, i64)>, Option<(i64, i64)>, Vec<(bool, bool)>);

pub struct Hud {
    root: Gd<Control>,
    /// The blocks that take the mouse (then the map is not hovered).
    solid: Vec<Gd<Control>>,

    // portrait block, top left
    portrait_panel: Gd<PanelContainer>,
    initial: Gd<Label>,
    plaque: Gd<Label>,
    title: Gd<Label>,
    subtitle: Gd<Label>,
    ac: ValueChip,
    gold: ValueChip,
    need_box: Gd<HBoxContainer>,
    needs: Vec<ShownChip>,
    cond_box: Gd<HFlowContainer>,
    conditions: Vec<ShownChip>,
    attrs: Gd<RichTextLabel>,
    /// The attributes last shown, and the ones flashing (index, up, since).
    attr_values: Vec<(&'static str, String)>,
    attr_flash: Vec<(usize, bool, f64)>,
    role: Option<String>,
    align_text: Option<String>,

    // bottom cluster
    hp: Orb,
    pw: Orb,
    xp_fill: Gd<ColorRect>,
    xp_label: Gd<Label>,
    bar: ActionBar,

    // the log, bottom left (docked over the cluster when compact)
    log_panel: Gd<PanelContainer>,
    log: Gd<RichTextLabel>,
    /// The log's title and history button (not in the compact layout).
    log_head: Gd<HBoxContainer>,
    log_compact: Option<bool>,
    /// When each message was first shown (seq, seconds).
    arrivals: VecDeque<(u64, f64)>,
    transient_panel: Gd<PanelContainer>,
    transient: Gd<Label>,

    // minimap, mode badge and order line, top right
    minimap: Minimap,
    minimap_caption: Gd<Label>,
    mode_panel: Gd<PanelContainer>,
    mode_label: Gd<Label>,
    order_label: Gd<Label>,

    // centre
    prompt_row: Gd<HBoxContainer>,
    prompt_panel: Gd<PanelContainer>,
    prompt: Gd<Label>,
    flash: Gd<Label>,
    flash_since: Option<f64>,
    deadly: Gd<Label>,
    combat: Option<bool>,

    tooltip_panel: Gd<PanelContainer>,
    tooltip: Gd<Label>,
    tooltip_text: Option<String>,
    toast_panel: Gd<PanelContainer>,
    toast: Gd<Label>,
    toast_undo: Gd<Button>,
    toast_until: f64,
    full_log_shade: Gd<ColorRect>,
    full_log_panel: Gd<PanelContainer>,
    full_log: Gd<RichTextLabel>,
    full_log_count: Gd<Label>,

    status_text: String,
    /// (newest message, input seq, full log open, faded lines) last shown.
    log_key: Option<(u64, u64, bool, usize)>,
    full_log_dirty: bool,
    /// The full log jumps to the newest message on its next rebuild.
    full_log_follow: bool,
    /// The canvas size the layout was made for.
    laid_out: Option<Vector2>,
}

impl Hud {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Hud {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);
        let mut solid: Vec<Gd<Control>> = Vec::new();

        // ---- portrait block, top left ----
        let mut portrait_panel = theme::framed(Frame::Hud);
        portrait_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &portrait_panel,
            [0.0, 0.0, 0.0, 0.0],
            [EDGE, EDGE, EDGE + PORTRAIT_W, EDGE + PORTRAIT_H],
        );
        let mut block = Control::new_alloc();
        block.set_mouse_filter(MouseFilter::IGNORE);
        let (mut medallion, _) = ring(124.0, 0.9, theme::GOLD);
        medallion.set_position(Vector2::new(0.0, 0.0));
        block.add_child(&medallion);
        let mut initial = theme::styled_label("", Face::TitleBold, 64, theme::GOLD_BRIGHT);
        theme::outline(&initial, 6);
        place(&initial, [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 124.0, 118.0]);
        initial.set_horizontal_alignment(HorizontalAlignment::CENTER);
        initial.set_vertical_alignment(VerticalAlignment::CENTER);
        block.add_child(&initial);
        let mut plaque_panel = theme::framed(Frame::Banner);
        plaque_panel.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &plaque_panel,
            [0.0, 0.0, 0.0, 0.0],
            [30.0, 112.0, 94.0, 138.0],
        );
        let mut plaque_style = theme::frame_style(Frame::Banner);
        plaque_style.set_content_margin_all(0.0);
        if plaque_panel.get_material().is_none() {
            plaque_panel.add_theme_stylebox_override("panel", &plaque_style);
        } else if let Some(mut sb) = plaque_panel
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin_all(0.0);
        }
        let mut plaque = theme::styled_label("", Face::Caps, 15, theme::GOLD_BRIGHT);
        plaque.set_horizontal_alignment(HorizontalAlignment::CENTER);
        plaque.set_vertical_alignment(VerticalAlignment::CENTER);
        plaque_panel.add_child(&plaque);
        block.add_child(&plaque_panel);

        let mut col = vbox(4);
        place(&col, [0.0, 0.0, 1.0, 1.0], [140.0, -4.0, 0.0, 0.0]);
        let mut title = theme::styled_label("", Face::Title, 27, theme::GOLD_BRIGHT);
        title.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        title.set_clip_text(true);
        col.add_child(&title);
        let mut subtitle = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        subtitle.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        subtitle.set_clip_text(true);
        col.add_child(&subtitle);
        let mut chip_row = hbox(6);
        chip_row.set_custom_minimum_size(Vector2::new(0.0, 26.0));
        let ac = ValueChip::new(5);
        chip_row.add_child(&ac.panel);
        let gold = ValueChip::new(6);
        chip_row.add_child(&gold.panel);
        let need_box = hbox(6);
        chip_row.add_child(&need_box);
        col.add_child(&chip_row);
        let mut cond_box = HFlowContainer::new_alloc();
        cond_box.set_mouse_filter(MouseFilter::IGNORE);
        cond_box.add_theme_constant_override("h_separation", 6);
        cond_box.add_theme_constant_override("v_separation", 4);
        col.add_child(&cond_box);
        block.add_child(&col);
        let mut attrs = rich(MouseFilter::STOP);
        attrs.set_fit_content(true);
        attrs.set_scroll_active(false);
        attrs.set_autowrap_mode(AutowrapMode::OFF);
        attrs.add_theme_font_override("normal_font", &theme::font(Face::Body));
        attrs.add_theme_font_size_override("normal_font_size", 16);
        attrs.set_tooltip_text("Attributes. Click: your character (^X).");
        place(&attrs, [0.0, 1.0, 1.0, 1.0], [140.0, -24.0, 0.0, 0.0]);
        let q = queue.clone();
        attrs
            .signals()
            .gui_input()
            .connect(move |ev: Gd<InputEvent>| {
                if let Ok(b) = ev.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                    && b.get_button_index() == MouseButton::LEFT
                {
                    push(&q, key_event(ctrl('x')));
                }
            });
        block.add_child(&attrs);
        portrait_panel.add_child(&block);
        root.add_child(&portrait_panel);
        solid.push(portrait_panel.clone().upcast());

        // ---- bottom cluster: orbs, XP bar, micro-buttons, action bar ----
        let mut cluster = Control::new_alloc();
        cluster.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &cluster,
            [0.5, 1.0, 0.5, 1.0],
            [
                -CLUSTER_W / 2.0,
                -CLUSTER_BOTTOM - CLUSTER_H,
                CLUSTER_W / 2.0,
                -CLUSTER_BOTTOM,
            ],
        );
        let bar_x = (CLUSTER_W - BAR_W) / 2.0;
        let mut bar_frame = theme::framed(Frame::Hud);
        bar_frame.set_mouse_filter(MouseFilter::STOP);
        place(
            &bar_frame,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, CLUSTER_H - BAR_H, bar_x + BAR_W, CLUSTER_H],
        );
        let bar = ActionBar::new(&queue);
        let mut bar_node = bar.node();
        bar_node.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        bar_node.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        bar_frame.add_child(&bar_node);
        cluster.add_child(&bar_frame);
        solid.push(bar_frame.upcast());

        let xp_y = CLUSTER_H - BAR_H - 6.0 - 22.0;
        let mut xp_frame = theme::framed(Frame::Socket);
        xp_frame.set_mouse_filter(MouseFilter::STOP);
        place(
            &xp_frame,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, xp_y, bar_x + BAR_W, xp_y + 22.0],
        );
        let mut xp_box = Control::new_alloc();
        xp_box.set_mouse_filter(MouseFilter::IGNORE);
        let mut xp_track = ColorRect::new_alloc();
        xp_track.set_mouse_filter(MouseFilter::IGNORE);
        xp_track.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.5));
        place(&xp_track, [0.0, 0.5, 1.0, 0.5], [8.0, -6.0, -8.0, 6.0]);
        xp_box.add_child(&xp_track);
        let mut xp_fill = ColorRect::new_alloc();
        xp_fill.set_mouse_filter(MouseFilter::IGNORE);
        xp_fill.set_color(theme::XP);
        place(&xp_fill, [0.0, 0.5, 0.0, 0.5], [8.0, -6.0, 8.0, 6.0]);
        let mut xp_shine = ColorRect::new_alloc();
        xp_shine.set_mouse_filter(MouseFilter::IGNORE);
        xp_shine.set_color(Color::from_rgba(1.0, 0.92, 0.7, 0.45));
        place(&xp_shine, [0.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 3.0]);
        xp_fill.add_child(&xp_shine);
        xp_box.add_child(&xp_fill);
        let mut xp_label = theme::styled_label("", Face::BodyBold, 14, theme::TEXT);
        theme::outline(&xp_label, 5);
        place(&xp_label, [0.0, 0.0, 1.0, 1.0], [0.0, -4.0, 0.0, 4.0]);
        xp_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        xp_label.set_vertical_alignment(VerticalAlignment::CENTER);
        xp_box.add_child(&xp_label);
        xp_frame.add_child(&xp_box);
        if let Some(mut sb) = xp_frame
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin_all(0.0);
        }
        xp_frame.set_tooltip_text("Experience");
        cluster.add_child(&xp_frame);
        solid.push(xp_frame.clone().upcast());

        let mut micro = hbox(4);
        micro.set_alignment(AlignmentMode::END);
        let micro_y = xp_y - 6.0 - 28.0;
        place(
            &micro,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, micro_y, bar_x + BAR_W, micro_y + 28.0],
        );
        let plain = |c| key_event(KeyInput::plain(Key::Char(c)));
        for (glyph, tip, ev) in [
            (0, "Inventory (i)", plain('i')),
            (1, "Spells (+)", plain('+')),
            (2, "Character (^X)", key_event(ctrl('x'))),
            (3, "Dungeon overview (^O)", key_event(ctrl('o'))),
            (4, "Message history (F9)", UiEvent::ToggleFullLog),
        ] {
            let b = micro_button(glyph, tip, &queue, ev);
            micro.add_child(&b);
            solid.push(b.upcast());
        }
        cluster.add_child(&micro);

        let mut hp = Orb::new("Hit points", theme::HP_DEEP, theme::HP);
        let mut pw = Orb::new("Power", theme::PW_DEEP, theme::PW);
        let orb_y = CLUSTER_H - orb::SIZE;
        hp.node().set_position(Vector2::new(0.0, orb_y));
        pw.node()
            .set_position(Vector2::new(CLUSTER_W - orb::SIZE, orb_y));
        cluster.add_child(&hp.node());
        cluster.add_child(&pw.node());
        solid.push(hp.node());
        solid.push(pw.node());
        hp.reset();
        pw.reset();
        root.add_child(&cluster);

        // ---- the log, bottom left; the wheel scrolls it ----
        let mut log_panel = theme::framed(Frame::Hud);
        log_panel.set_mouse_filter(MouseFilter::STOP);
        log_panel.set_self_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        let mut log_col = vbox(2);
        let mut log_head = hbox(4);
        log_head.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        let mut log_title = theme::styled_label("Messages", Face::Caps, 14, theme::TEXT_DIM);
        theme::outline(&log_title, 4);
        log_title.set_h_size_flags(SizeFlags::EXPAND_FILL);
        log_title.set_vertical_alignment(VerticalAlignment::CENTER);
        log_head.add_child(&log_title);
        let history = micro_button(4, "Message history (F9)", &queue, UiEvent::ToggleFullLog);
        log_head.add_child(&history);
        log_col.add_child(&log_head.clone());
        let mut log_area = vbox(0);
        log_area.set_alignment(AlignmentMode::END);
        log_area.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut log = rich(MouseFilter::STOP);
        log.set_scroll_follow(true);
        log.add_theme_font_override("normal_font", &theme::font(Face::Body));
        log.add_theme_font_override("bold_font", &theme::font(Face::BodyBold));
        log.add_theme_font_size_override("normal_font_size", 16);
        log.add_theme_font_size_override("bold_font_size", 16);
        log.add_theme_constant_override("line_separation", 2);
        theme::outline(&log, 5);
        log.add_theme_constant_override("outline_size", 5);
        log.set_v_size_flags(SizeFlags::SHRINK_END);
        log_area.add_child(&log);
        log_col.add_child(&log_area);
        log_panel.add_child(&log_col);
        root.add_child(&log_panel);
        solid.push(log_panel.clone().upcast());

        // transient message (getpos autodescribe...), its own line above the log
        let mut transient_panel = theme::framed(Frame::Banner);
        transient_panel.set_mouse_filter(MouseFilter::IGNORE);
        transient_panel.set_v_grow_direction(GrowDirection::BEGIN);
        let mut transient = theme::styled_label("", Face::Body, 16, theme::GOLD_BRIGHT);
        transient.set_clip_text(true);
        transient_panel.add_child(&transient);
        transient_panel.set_visible(false);
        root.add_child(&transient_panel);

        // ---- minimap, top right; the mode and the order under it ----
        let mut minimap_panel = theme::framed(Frame::Hud);
        minimap_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &minimap_panel,
            [1.0, 0.0, 1.0, 0.0],
            [-EDGE - MINIMAP_W, EDGE, -EDGE, EDGE + MINIMAP_H],
        );
        let mut mm_col = vbox(4);
        let minimap = Minimap::new(&queue);
        let mut well = ColorRect::new_alloc();
        well.set_mouse_filter(MouseFilter::IGNORE);
        well.set_color(Color::from_rgba(0.07, 0.055, 0.04, 0.85));
        well.set_custom_minimum_size(Vector2::new(minimap::WIDTH as f32, minimap::HEIGHT as f32));
        let mut map_node = minimap.node();
        place(&map_node, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        map_node.set_custom_minimum_size(Vector2::ZERO);
        well.add_child(&map_node);
        mm_col.add_child(&well);
        let mut minimap_caption = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        minimap_caption.set_horizontal_alignment(HorizontalAlignment::CENTER);
        mm_col.add_child(&minimap_caption);
        minimap_panel.add_child(&mm_col);
        root.add_child(&minimap_panel);
        solid.push(minimap_panel.clone().upcast());

        let mut mode_panel = theme::framed(Frame::Banner);
        mode_panel.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &mode_panel,
            [1.0, 0.0, 1.0, 0.0],
            [
                -EDGE - MODE_W,
                EDGE + MINIMAP_H + 8.0,
                -EDGE,
                EDGE + MINIMAP_H + 40.0,
            ],
        );
        if let Some(mut sb) = mode_panel
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin(Side::TOP, 2.0);
            sb.set_content_margin(Side::BOTTOM, 2.0);
        }
        let mut mode_label = theme::styled_label("", Face::Title, 18, EXPLORE_TEXT);
        mode_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        mode_label.set_vertical_alignment(VerticalAlignment::CENTER);
        mode_panel.add_child(&mode_label);
        mode_panel.set_visible(false);
        root.add_child(&mode_panel);
        let mut order_label = theme::styled_label("", Face::Body, 16, theme::GOLD_BRIGHT);
        theme::outline(&order_label, 5);
        place(
            &order_label,
            [1.0, 0.0, 1.0, 0.0],
            [
                -EDGE - 400.0,
                EDGE + MINIMAP_H + 46.0,
                -EDGE,
                EDGE + MINIMAP_H + 70.0,
            ],
        );
        order_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        order_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        order_label.set_visible(false);
        root.add_child(&order_label);

        // ---- the prompt banner, top centre ----
        let mut prompt_row = hbox(0);
        prompt_row.set_alignment(AlignmentMode::CENTER);
        place(&prompt_row, [0.0, 0.0, 1.0, 0.0], [0.0, EDGE, 0.0, EDGE]);
        let mut prompt_panel = theme::framed(Frame::Banner);
        prompt_panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut prompt = theme::styled_label("", Face::BodyBold, 19, theme::GOLD_BRIGHT);
        prompt.set_horizontal_alignment(HorizontalAlignment::CENTER);
        prompt.set_vertical_alignment(VerticalAlignment::CENTER);
        prompt.set_autowrap_mode(AutowrapMode::WORD_SMART);
        prompt.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        prompt_panel.add_child(&prompt);
        prompt_panel.set_visible(false);
        prompt_row.add_child(&prompt_panel);
        root.add_child(&prompt_row);

        // ---- the banner of a new mode, big, under the prompt banner ----
        let mut flash = theme::styled_label("", Face::TitleBold, 60, EXPLORE_TEXT);
        place(&flash, [0.0, 0.0, 1.0, 0.0], [0.0, 110.0, 0.0, 190.0]);
        flash.set_horizontal_alignment(HorizontalAlignment::CENTER);
        theme::outline(&flash, 12);
        flash.set_visible(false);
        root.add_child(&flash);

        // ---- the deadly banner, centre ----
        let mut deadly = theme::styled_label("", Face::TitleBold, 42, theme::DANGER);
        place(&deadly, [0.5, 0.0, 0.5, 0.0], [-360.0, 236.0, 360.0, 300.0]);
        deadly.set_horizontal_alignment(HorizontalAlignment::CENTER);
        deadly.set_vertical_alignment(VerticalAlignment::CENTER);
        theme::outline(&deadly, 12);
        deadly.set_visible(false);
        root.add_child(&deadly);

        // ---- full log, F9; scrolls with the wheel ----
        // a modal the size of the inventory panel, over the dimmed world
        let mut full_log_shade = ColorRect::new_alloc();
        full_log_shade.set_color(theme::SHADE);
        theme::full_rect_ignore(&full_log_shade);
        full_log_shade.set_visible(false);
        root.add_child(&full_log_shade);
        let mut full_log_panel = theme::framed(Frame::Panel);
        full_log_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &full_log_panel,
            [0.5, 0.5, 0.5, 0.5],
            [-708.0, -380.0, 708.0, 380.0],
        );
        let mut full_col = vbox(8);
        let mut full_head = hbox(12);
        let full_title =
            theme::styled_label("Message history", Face::Title, 28, theme::GOLD_BRIGHT);
        full_head.add_child(&full_title);
        let mut full_log_count = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        full_log_count.set_h_size_flags(SizeFlags::EXPAND_FILL);
        full_log_count.set_vertical_alignment(VerticalAlignment::CENTER);
        full_head.add_child(&full_log_count);
        full_head.add_child(&theme::button("Close  F9", &queue, UiEvent::ToggleFullLog));
        full_col.add_child(&full_head);
        let mut rule = ColorRect::new_alloc();
        rule.set_color(Color {
            a: 0.6,
            ..theme::GOLD_DIM
        });
        rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
        rule.set_mouse_filter(MouseFilter::IGNORE);
        full_col.add_child(&rule);
        let mut full_log = rich(MouseFilter::STOP);
        full_log.set_scroll_follow(true);
        full_log.set_v_size_flags(SizeFlags::EXPAND_FILL);
        full_log.add_theme_font_override("normal_font", &theme::font(Face::Body));
        full_log.add_theme_font_override("bold_font", &theme::font(Face::BodyBold));
        full_log.add_theme_font_size_override("normal_font_size", 17);
        full_log.add_theme_font_size_override("bold_font_size", 17);
        full_log.add_theme_constant_override("line_separation", 3);
        full_col.add_child(&full_log);
        full_log_panel.add_child(&full_col);
        full_log_panel.set_visible(false);
        root.add_child(&full_log_panel);
        solid.push(full_log_panel.clone().upcast());

        // ---- the toast lane, top centre under the prompt banner ----
        let mut toast_row = hbox(0);
        toast_row.set_alignment(AlignmentMode::CENTER);
        place(&toast_row, [0.0, 0.0, 1.0, 0.0], [0.0, 80.0, 0.0, 80.0]);
        let mut toast_panel = theme::framed(Frame::Banner);
        toast_panel.set_mouse_filter(MouseFilter::STOP);
        let mut toast_box = hbox(14);
        let mut toast = theme::styled_label("", Face::BodyBold, 17, theme::TEXT);
        toast.set_vertical_alignment(VerticalAlignment::CENTER);
        toast_box.add_child(&toast);
        let toast_undo = theme::button("Undo", &queue, UiEvent::SlotUndo);
        toast_box.add_child(&toast_undo);
        toast_panel.add_child(&toast_box);
        toast_panel.set_visible(false);
        toast_row.add_child(&toast_panel);
        root.add_child(&toast_row);
        solid.push(toast_panel.clone().upcast());

        // ---- tooltip, follows the mouse; on top of everything ----
        let mut tooltip_panel = theme::framed(Frame::Tooltip);
        tooltip_panel.set_mouse_filter(MouseFilter::IGNORE);
        let tooltip = theme::styled_label("", Face::Body, 16, theme::TEXT);
        tooltip_panel.add_child(&tooltip);
        tooltip_panel.set_visible(false);
        root.add_child(&tooltip_panel);

        let mut hud = Hud {
            root,
            solid,
            portrait_panel,
            initial,
            plaque,
            title,
            subtitle,
            ac,
            gold,
            need_box,
            needs: Vec::new(),
            cond_box,
            conditions: Vec::new(),
            attrs,
            attr_values: Vec::new(),
            attr_flash: Vec::new(),
            role: None,
            align_text: None,
            hp,
            pw,
            xp_fill,
            xp_label,
            bar,
            log_panel,
            log,
            log_head,
            log_compact: None,
            arrivals: VecDeque::new(),
            transient_panel,
            transient,
            minimap,
            minimap_caption,
            mode_panel,
            mode_label,
            order_label,
            prompt_row,
            prompt_panel,
            prompt,
            flash,
            flash_since: None,
            deadly,
            combat: None,
            tooltip_panel,
            tooltip,
            tooltip_text: None,
            toast_panel,
            toast,
            toast_undo,
            toast_until: 0.0,
            full_log_shade,
            full_log_panel,
            full_log,
            full_log_count,
            status_text: String::new(),
            log_key: None,
            full_log_dirty: true,
            full_log_follow: true,
            laid_out: None,
        };
        hud.reset();
        hud
    }

    /// A short notice at the top centre for a few seconds; `undo` offers
    /// the Undo button.
    pub fn toast(&mut self, text: &str, undo: bool) {
        self.toast.set_text(text);
        self.toast_undo.set_visible(undo);
        self.toast_panel.reset_size();
        self.toast_panel.set_visible(true);
        self.toast_until = now_secs() + TOAST_SECS;
    }

    /// The hero's role (or form) as the map shows it: "Valkyrie".
    pub fn role(&self) -> Option<&str> {
        self.role.as_deref()
    }

    /// The action bar's slots (their bindings drive it).
    pub fn action_bar(&mut self) -> &mut ActionBar {
        &mut self.bar
    }

    /// Exploring or fighting; `announce` shows the big banner too.
    pub fn set_mode(&mut self, combat: bool, announce: bool) {
        if self.combat == Some(combat) && !announce {
            return;
        }
        self.combat = Some(combat);
        let (text, badge, word) = if combat {
            (COMBAT_TEXT, "COMBAT - TURN BY TURN", "COMBAT")
        } else {
            (EXPLORE_TEXT, "EXPLORING", "EXPLORATION")
        };
        self.mode_label.set_text(badge);
        self.mode_label.add_theme_color_override("font_color", text);
        self.mode_panel
            .set_self_modulate(if combat { COMBAT_TINT } else { Color::WHITE });
        self.mode_panel.set_visible(true);
        if announce {
            self.flash.set_text(word);
            self.flash.add_theme_color_override("font_color", text);
            self.flash_since = Some(now_secs());
            self.flash.set_modulate(Color::WHITE);
            self.flash.set_visible(true);
        }
    }

    /// What the order does or why it ended; None hides the line.
    pub fn set_order_line(&mut self, text: Option<&str>) {
        let text = text.unwrap_or("");
        if self.order_label.get_text() != text {
            self.order_label.set_text(text);
            self.order_label.set_visible(!text.is_empty());
        }
    }

    /// (the mode badge, the order line, the banner on screen) (self-tests).
    pub fn mode_view(&self) -> (Option<String>, Option<String>, bool) {
        let badge = self
            .mode_panel
            .is_visible()
            .then(|| self.mode_label.get_text().to_string());
        let order = self
            .order_label
            .is_visible()
            .then(|| self.order_label.get_text().to_string());
        (badge, order, self.flash.is_visible())
    }

    /// (HP, Pw) the orbs show, and the ten slots' (filled, enabled) (self-tests).
    pub fn cluster_view(&self) -> ClusterView {
        (self.hp.view().0, self.pw.view().0, self.bar.slot_states())
    }

    /// The banner fades out.
    fn fade_flash(&mut self, now: f64) {
        let Some(since) = self.flash_since else {
            return;
        };
        let a = flash_alpha(now - since);
        if a <= 0.0 {
            self.flash_since = None;
            self.flash.set_visible(false);
        } else {
            self.flash.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
        }
    }

    /// Called every frame while a game is on screen.
    pub fn sync(&mut self, world: &mut World, catalog: Option<&Catalog>) {
        let now = now_secs();
        self.layout();
        if let Some(cat) = catalog {
            self.sync_role(world, cat);
        }
        if world.status.take_changed() || self.status_text.is_empty() {
            self.show_status(&world.status, catalog, now);
        }
        self.hp.tick(now);
        self.pw.tick(now);
        self.pulse(now);
        self.fade_attrs(now);
        self.fade_flash(now);
        if self.toast_panel.is_visible() && now > self.toast_until {
            self.toast_panel.set_visible(false);
        }
        self.hover_log();
        if let Some(cat) = catalog {
            self.minimap.sync(&world.map, cat, now);
        }
        let full_open = self.full_log_panel.is_visible();
        let last_seq = world.log.last_seq();
        let faded = self.note_arrivals(world, now);
        let key = (last_seq, world.input_seq(), full_open, faded);
        if self.log_key != Some(key) {
            let news = self.log_key.is_none_or(|(seen, ..)| seen != last_seq);
            self.log_key = Some(key);
            self.show_log(world, news, now);
            self.full_log_dirty |= news;
            self.full_log_follow |= news;
        }
        if full_open && self.full_log_dirty {
            self.full_log_dirty = false;
            let follow = std::mem::take(&mut self.full_log_follow);
            self.show_full_log(world, follow);
        }
        let transient = world.transient.as_deref().unwrap_or("").trim();
        if self.transient.get_text() != transient {
            self.transient.set_text(transient);
            self.transient_panel.set_visible(!transient.is_empty());
        }
    }

    /// Place the blocks that depend on the canvas size (the compact
    /// layout, the prompt banner's width).
    fn layout(&mut self) {
        let size = self.root.get_viewport_rect().size;
        if self.laid_out == Some(size) {
            return;
        }
        self.laid_out = Some(size);
        let compact = compact(size.x);
        if self.log_compact != Some(compact) {
            self.log_compact = Some(compact);
            if compact {
                // docked above the cluster's left half
                let top = -CLUSTER_BOTTOM - CLUSTER_H - 8.0 - LOG_COMPACT_H;
                place(
                    &self.log_panel,
                    [0.5, 1.0, 0.5, 1.0],
                    [
                        -CLUSTER_W / 2.0 + orb::SIZE - 20.0,
                        top,
                        -CLUSTER_W / 2.0 + orb::SIZE - 20.0 + LOG_COMPACT_W,
                        top + LOG_COMPACT_H,
                    ],
                );
            } else {
                place(
                    &self.log_panel,
                    [0.0, 1.0, 0.0, 1.0],
                    [EDGE, -EDGE - LOG_H, EDGE + LOG_W, -EDGE],
                );
            }
            // the attribute strip goes to the tooltip and the Character panel
            self.attrs.set_visible(!compact);
            let h = if compact {
                PORTRAIT_H - 14.0
            } else {
                PORTRAIT_H
            };
            self.log_head.set_visible(!compact);
            self.portrait_panel.set_offset(Side::BOTTOM, EDGE + h);
            let log_top = self.log_panel.get_offset(Side::TOP);
            let (l, r) = (
                self.log_panel.get_offset(Side::LEFT),
                self.log_panel.get_offset(Side::RIGHT),
            );
            let anchor = self.log_panel.get_anchor(Side::LEFT);
            place(
                &self.transient_panel,
                [anchor, 1.0, anchor, 1.0],
                [l, log_top - 6.0, r, log_top - 6.0],
            );
            self.log_key = None;
        }
        // the banner fits between the portrait block and the minimap
        let room = size.x - 2.0 * (EDGE + PORTRAIT_W.max(MINIMAP_W) + 16.0);
        let max = room.clamp(320.0, 960.0);
        self.prompt.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        self.prompt_row.set_meta("max_width", &max.to_variant());
        self.fit_prompt();
    }

    /// The prompt banner as wide as its text, 560–960 where there is room.
    fn fit_prompt(&mut self) {
        let max = self
            .prompt_row
            .get_meta("max_width")
            .try_to::<f32>()
            .unwrap_or(960.0);
        let text = self.prompt.get_text().to_string();
        let font = theme::font(Face::BodyBold);
        let w = font.get_string_size_ex(&text).font_size(19).done().x;
        let width = (w + 8.0).clamp(560.0f32.min(max), max);
        self.prompt
            .set_custom_minimum_size(Vector2::new(width, 24.0));
        self.prompt_panel.reset_size();
    }

    /// The hero's form, from the glyph under the hero: the portrait's
    /// initial and the subtitle.
    fn sync_role(&mut self, world: &World, catalog: &Catalog) {
        let name = world
            .map
            .hero()
            .and_then(|(x, y)| world.map.cell(x, y))
            .and_then(|c| c.glyph.as_ref())
            .filter(|g| g.kind == GlyphKind::Mon && g.flags & mg::HERO != 0)
            .and_then(|g| {
                let m = catalog.monsters.get(usize::try_from(g.mon?).ok()?)?;
                let gendered = if g.flags & mg::FEMALE != 0 {
                    m.female.as_ref()
                } else if g.flags & mg::MALE != 0 {
                    m.male.as_ref()
                } else {
                    None
                };
                Some(title_case(gendered.unwrap_or(&m.name)))
            });
        if name.is_some() && name != self.role {
            self.role = name;
            self.show_subtitle();
        }
    }

    fn show_subtitle(&mut self) {
        let parts: Vec<&str> = [self.role.as_deref(), self.align_text.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        self.subtitle.set_text(&parts.join("  ·  "));
        let initial = self
            .role
            .as_deref()
            .and_then(|r| r.chars().next())
            .map_or(String::new(), |c| c.to_string());
        self.initial.set_text(&initial);
    }

    fn show_status(&mut self, status: &Status, catalog: Option<&Catalog>, now: f64) {
        let conditions = catalog.map_or_else(Vec::new, |c| status.condition_names(c));
        let view = StatusView::new(status, &conditions);
        self.status_text = view.lines().join("\n");
        self.portrait_panel.set_tooltip_text(&self.status_text);
        self.title.set_text(&view.title);
        if self.align_text != view.align {
            self.align_text = view.align.clone();
            self.show_subtitle();
        }
        self.ac.set(
            view.field("AC"),
            &format!("Armour class {}", view.field("AC").unwrap_or("")),
        );
        self.gold.set(
            view.field("$"),
            &format!("Gold {}", view.field("$").unwrap_or("")),
        );
        let level = view.field("XL").and_then(|v| v.parse::<i64>().ok());
        let plaque = match (level, view.field("HD")) {
            (Some(l), _) => format!("Lv {l}"),
            (None, Some(hd)) => format!("HD {hd}"),
            (None, None) => String::new(),
        };
        self.plaque.set_text(&plaque);

        self.hp.set(view.hp, now);
        self.pw.set(view.pw, now);
        let low = match (view.hp, level) {
            (Some((h, m)), Some(l)) => critically_low_hp(h, m, l),
            _ => false,
        };
        self.hp.set_low(low);

        // the XP bar
        let exp = view.field("Exp").and_then(|v| v.parse::<i64>().ok());
        let (share, label) = match (level, exp) {
            (Some(l), Some(e)) => {
                let (share, next) = xp_progress(l, e);
                (share, format!("Lv {l}  ·  {e} / {next}"))
            }
            (Some(l), None) => (0.0, format!("Lv {l}")),
            (None, _) => (
                0.0,
                view.field("HD")
                    .map_or(String::new(), |hd| format!("HD {hd}")),
            ),
        };
        let w = (BAR_W - 16.0) * share;
        self.xp_fill.set_offset(Side::RIGHT, 8.0 + w);
        self.xp_fill.set_visible(share > 0.0);
        self.xp_label.set_text(&label);

        // the minimap's caption: where and when
        let caption: Vec<String> = [view.place(), view.field("T").map(|t| format!("T {t}"))]
            .into_iter()
            .flatten()
            .collect();
        self.minimap_caption.set_text(&caption.join("  ·  "));

        self.show_attrs(&view.attrs, now);
        self.show_chips(&view);
    }

    fn show_attrs(&mut self, attrs: &[(&'static str, String)], now: f64) {
        if self.attr_values.len() == attrs.len() {
            for (i, ((_, old), (_, new))) in self.attr_values.iter().zip(attrs).enumerate() {
                if old == new {
                    continue;
                }
                if let (Some(a), Some(b)) = (attr_rank(old), attr_rank(new)) {
                    self.attr_flash.retain(|(j, ..)| *j != i);
                    self.attr_flash.push((i, b > a, now));
                }
            }
        }
        self.attr_values = attrs.to_vec();
        self.draw_attrs();
    }

    fn draw_attrs(&mut self) {
        let dim = hex(theme::TEXT_DIM);
        let text: Vec<String> = self
            .attr_values
            .iter()
            .enumerate()
            .map(|(i, (tag, v))| {
                let color = match self.attr_flash.iter().find(|(j, ..)| *j == i) {
                    Some((_, true, _)) => theme::GOOD,
                    Some((_, false, _)) => theme::DANGER,
                    None => theme::TEXT,
                };
                format!(
                    "[color={dim}]{tag}[/color] [color={}]{}[/color]",
                    hex(color),
                    bbcode_escape(v)
                )
            })
            .collect();
        self.attrs.set_text(&text.join("    "));
    }

    /// Changed attributes go back to parchment after a while.
    fn fade_attrs(&mut self, now: f64) {
        let before = self.attr_flash.len();
        self.attr_flash
            .retain(|(_, _, since)| now - since < ATTR_FLASH_SECS);
        if self.attr_flash.len() != before {
            self.draw_attrs();
        }
    }

    fn show_chips(&mut self, view: &StatusView) {
        let same = |shown: &[ShownChip], chips: &[Chip]| {
            shown.len() == chips.len() && shown.iter().zip(chips).all(|(a, b)| a.chip == *b)
        };
        if !same(&self.needs, &view.needs) {
            for mut c in self.needs.drain(..) {
                c.node.queue_free();
            }
            for c in &view.needs {
                let shown = need_chip(c);
                self.need_box.add_child(&shown.node);
                self.needs.push(shown);
            }
        }
        if !same(&self.conditions, &view.conditions) {
            for mut c in self.conditions.drain(..) {
                c.node.queue_free();
            }
            for c in &view.conditions {
                let shown = condition_chip(c);
                self.cond_box.add_child(&shown.node);
                self.conditions.push(shown);
            }
            self.cond_box.set_visible(!self.conditions.is_empty());
            let words: Vec<&str> = view
                .conditions
                .iter()
                .filter_map(|c| deadly_words(&c.text))
                .collect();
            self.deadly.set_text(&words.join("   "));
            self.deadly.set_visible(!words.is_empty());
        }
    }

    /// Deadly chips, their gems and the deadly banner glow and fade.
    fn pulse(&mut self, now: f64) {
        let any = |v: &[ShownChip]| v.iter().any(|c| c.chip.tone == Tone::Deadly);
        if !any(&self.needs) && !any(&self.conditions) && !self.deadly.is_visible() {
            return;
        }
        let k = pulse_now(now);
        let (bg, ..) = chip_colors(Tone::Deadly);
        let glow = bg.lerp(Color::from_rgb(0.95, 0.2, 0.15), k as f64);
        for c in self.needs.iter_mut().chain(self.conditions.iter_mut()) {
            if c.chip.tone == Tone::Deadly {
                c.style.set_bg_color(glow);
                if let Some(m) = &mut c.gem {
                    m.set_shader_parameter("glow", &k.to_variant());
                }
            }
        }
        if self.deadly.is_visible() {
            self.deadly
                .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.55 + 0.45 * k));
        }
    }

    /// The log's frame shows while the mouse is over it.
    fn hover_log(&mut self) {
        let mouse = self.root.get_global_mouse_position();
        let over = self.log_panel.is_visible_in_tree()
            && self.log_panel.get_global_rect().contains_point(mouse);
        let target = if over { 1.0 } else { 0.0 };
        let mut m = self.log_panel.get_self_modulate();
        if (m.a - target).abs() > 0.01 {
            m.a += (target - m.a) * 0.25;
            if (m.a - target).abs() <= 0.01 {
                m.a = target;
            }
            self.log_panel.set_self_modulate(m);
            // the title and the history button come with the frame
            self.log_head
                .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, m.a));
        }
    }

    /// Remember when each message came; how many of the log's lines have
    /// faded by `now`.
    fn note_arrivals(&mut self, world: &World, now: f64) -> usize {
        let last = self.arrivals.back().map_or(0, |(s, _)| *s);
        for m in world.log.since(last) {
            // restored history is old from the start
            let at = if m.from_history {
                f64::NEG_INFINITY
            } else {
                now
            };
            self.arrivals.push_back((m.seq, at));
        }
        while self.arrivals.len() > LOG_LINES {
            self.arrivals.pop_front();
        }
        let secs = self.fade_secs();
        self.arrivals
            .iter()
            .filter(|(_, at)| now - at > secs)
            .count()
    }

    fn fade_secs(&self) -> f64 {
        if self.log_compact == Some(true) {
            LOG_FADE_COMPACT_SECS
        } else {
            LOG_FADE_SECS
        }
    }

    /// `follow`: new messages came, show the newest; else keep the place
    /// the player scrolled to.
    fn show_log(&mut self, world: &World, follow: bool, now: f64) {
        let input_seq = world.input_seq();
        let secs = self.fade_secs();
        let recent: Vec<&Message> = world.log.iter().rev().take(LOG_LINES).collect();
        let lines: Vec<String> = recent
            .iter()
            .rev()
            .map(|m| {
                let at = self
                    .arrivals
                    .iter()
                    .rev()
                    .find(|(s, _)| *s == m.seq)
                    .map_or(f64::NEG_INFINITY, |(_, at)| *at);
                message_bbcode(m, input_seq, now - at > secs)
            })
            .collect();
        set_scrolled_text(&mut self.log, &lines.join("\n"), follow);
        // the log sits on the panel's bottom edge and grows up to fill it
        let h = self.log.get_content_height() as f32;
        let room = self.log.get_parent_control().map_or(h, |p| p.get_size().y);
        self.log
            .set_custom_minimum_size(Vector2::new(0.0, h.min(room.max(1.0))));
    }

    fn show_full_log(&mut self, world: &World, follow: bool) {
        let input_seq = world.input_seq();
        let dim = hex(theme::TEXT_OFF);
        let lines: Vec<String> = world
            .log
            .iter()
            .map(|m| {
                let turn = m.turn.map_or(String::new(), |t| format!("T {t}"));
                format!(
                    "[color={dim}]{turn:<8}[/color]  {}",
                    message_bbcode(m, input_seq, false)
                )
            })
            .collect();
        self.full_log_count
            .set_text(&format!("{} messages", world.log.len()));
        set_scrolled_text(&mut self.full_log, &lines.join("\n"), follow);
    }

    /// The status as plain text.
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    /// Is a block that takes the mouse under `pos` (then the map is not hovered)?
    pub fn covers(&self, pos: Vector2) -> bool {
        self.root.is_visible()
            && self
                .solid
                .iter()
                .any(|p| p.is_visible_in_tree() && p.get_global_rect().contains_point(pos))
    }

    /// What the prompt line shows, if it is up (self-tests).
    pub fn prompt_line(&self) -> Option<String> {
        self.prompt_panel
            .is_visible()
            .then(|| self.prompt.get_text().to_string())
    }

    /// The full message log is open (self-tests).
    pub fn full_log_open(&self) -> bool {
        self.full_log_panel.is_visible()
    }

    pub fn set_prompt_line(&mut self, text: Option<&str>) {
        match text.filter(|t| !t.is_empty()) {
            Some(t) => {
                if self.prompt.get_text() != t {
                    self.prompt.set_text(t);
                    self.fit_prompt();
                }
                self.prompt_panel.set_visible(true);
            }
            None => self.prompt_panel.set_visible(false),
        }
    }

    /// Show `text` next to the mouse at `screen_pos`, kept inside the
    /// window; `None` hides it. Cheap when nothing changed (called every frame).
    pub fn set_tooltip(&mut self, text: Option<&str>, screen_pos: Vector2) {
        let Some(t) = text.filter(|t| !t.is_empty()) else {
            if self.tooltip_text.take().is_some() {
                self.tooltip_panel.set_visible(false);
            }
            return;
        };
        if self.tooltip_text.as_deref() != Some(t) {
            self.tooltip_text = Some(t.to_string());
            self.tooltip.set_text(t);
            self.tooltip_panel.reset_size();
            self.tooltip_panel.set_visible(true);
        }
        let size = self.tooltip_panel.get_size();
        let view = self.root.get_viewport_rect().size;
        let mut pos = screen_pos + Vector2::new(TOOLTIP_GAP, TOOLTIP_GAP);
        if pos.x + size.x > view.x {
            pos.x = screen_pos.x - TOOLTIP_GAP - size.x;
        }
        if pos.y + size.y > view.y {
            pos.y = screen_pos.y - TOOLTIP_GAP - size.y;
        }
        let pos = Vector2::new(pos.x.max(0.0), pos.y.max(0.0));
        if self.tooltip_panel.get_position() != pos {
            self.tooltip_panel.set_position(pos);
        }
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
        if !on {
            self.set_tooltip(None, Vector2::ZERO);
            self.set_full_log(false);
        }
    }

    pub fn toggle_full_log(&mut self) {
        let on = !self.full_log_panel.is_visible();
        self.set_full_log(on);
    }

    /// Open the full log (^P), scrolled to the newest message.
    pub fn open_full_log(&mut self) {
        self.set_full_log(true);
    }

    fn set_full_log(&mut self, on: bool) {
        self.full_log_panel.set_visible(on);
        self.full_log_shade.set_visible(on);
        self.full_log_dirty = true;
        self.full_log_follow = true;
    }

    /// Forget what was shown (a new game).
    pub fn reset(&mut self) {
        self.status_text.clear();
        self.log_key = None;
        self.full_log_dirty = true;
        self.full_log_follow = true;
        self.title.set_text("");
        self.subtitle.set_text("");
        self.initial.set_text("");
        self.plaque.set_text("");
        self.role = None;
        self.align_text = None;
        self.ac.set(None, "");
        self.gold.set(None, "");
        self.attrs.set_text("");
        self.attr_values.clear();
        self.attr_flash.clear();
        for mut c in self.needs.drain(..).chain(self.conditions.drain(..)) {
            c.node.queue_free();
        }
        self.cond_box.set_visible(false);
        self.deadly.set_visible(false);
        self.hp.reset();
        self.pw.reset();
        self.xp_fill.set_visible(false);
        self.xp_label.set_text("");
        self.minimap.reset();
        self.minimap_caption.set_text("");
        self.arrivals.clear();
        self.log.set_text("");
        self.full_log.set_text("");
        self.transient.set_text("");
        self.transient_panel.set_visible(false);
        self.set_prompt_line(None);
        self.set_tooltip(None, Vector2::ZERO);
        self.set_full_log(false);
        self.combat = None;
        self.mode_panel.set_visible(false);
        self.set_order_line(None);
        self.flash_since = None;
        self.flash.set_visible(false);
        self.toast_panel.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::StatusUpdate;
    use nh_world::{MessageLog, NO_COLOR};

    fn status(fields: &[(&str, &str)]) -> Status {
        let mut s = Status::new();
        for (f, v) in fields {
            s.apply(&StatusUpdate {
                field: f.to_string(),
                value: Some(v.to_string()),
                conds: None,
                chg: 0,
                percent: 0,
                color: NO_COLOR,
            });
        }
        s
    }

    const START: [(&str, &str); 18] = [
        ("title", "Hero the Stripling            "),
        ("str", "16"),
        ("dex", "15"),
        ("con", "17"),
        ("int", "9"),
        ("wis", "11"),
        ("cha", "7"),
        ("align", "Neutral"),
        ("cap", ""),
        ("gold", "\\G0C9F0F2E:0"),
        ("energy", "2"),
        ("energymax", "2"),
        ("ac", "6"),
        ("time", "1"),
        ("hunger", ""),
        ("hp", "16"),
        ("hpmax", "16"),
        ("leveldesc", "Dlvl:1  "),
    ];

    #[test]
    fn a_new_hero_reads_like_the_bottom_lines() {
        let mut s = status(&START);
        s.apply(&StatusUpdate {
            field: "xlevel".into(),
            value: Some("1".into()),
            conds: None,
            chg: 0,
            percent: 0,
            color: NO_COLOR,
        });
        let v = StatusView::new(&s, &[]);
        assert_eq!(v.title, "Hero the Stripling");
        assert_eq!(v.align.as_deref(), Some("Neutral"));
        assert_eq!(v.hp, Some((16, 16)));
        assert_eq!(v.pw, Some((2, 2)));
        assert_eq!(v.chips().count(), 0, "no hunger, no burden");
        assert_eq!(v.field("AC"), Some("6"));
        assert_eq!(v.field("$"), Some("0"));
        assert_eq!(v.place().as_deref(), Some("Dlvl 1"));
        let text = v.lines().join("\n");
        for part in [
            "HP:16(16)",
            "Pw:2(2)",
            "Dlvl:1",
            "$0",
            "AC:6",
            "XL:1",
            "T:1",
            "St:16 Dx:15 Co:17 In:9 Wi:11 Ch:7",
        ] {
            assert!(text.contains(part), "{part:?} missing from {text:?}");
        }
    }

    #[test]
    fn experience_points_come_with_the_level_and_hit_dice_replace_both() {
        let v = StatusView::new(&status(&[("xlevel", "3"), ("exp", "27")]), &[]);
        let tags: Vec<&str> = v.fields.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tags, ["XL", "Exp"]);
        assert_eq!(v.place(), None);
        let v = StatusView::new(&status(&[("hitdice", "4")]), &[]);
        assert_eq!(v.fields, [("HD".to_string(), "4".to_string())]);
        assert!(v.lines()[1].contains("HD:4"));
    }

    #[test]
    fn other_places_show_their_whole_name() {
        let v = StatusView::new(&status(&[("leveldesc", "Home 2 ")]), &[]);
        assert_eq!(v.fields, [(String::new(), "Home 2".to_string())]);
        assert_eq!(v.lines()[1], "Home 2");
        assert_eq!(v.place().as_deref(), Some("Home 2"));
    }

    #[test]
    fn hunger_burden_and_conditions_become_chips() {
        let s = status(&[("hunger", "Weak"), ("cap", "Burdened")]);
        let conds = ["Blind".to_string(), "Stone".to_string(), "Fly".to_string()];
        let v = StatusView::new(&s, &conds);
        let chips: Vec<(&str, Tone)> = v.chips().map(|c| (c.text.as_str(), c.tone)).collect();
        assert_eq!(
            chips,
            [
                ("Weak", Tone::Bad),
                ("Burdened", Tone::Warn),
                ("Blind", Tone::Warn),
                ("Stone", Tone::Deadly),
                ("Fly", Tone::Info),
            ]
        );
        assert_eq!(v.needs.len(), 2);
        assert_eq!(v.conditions.len(), 3);
        assert_eq!(v.lines().last().unwrap(), "Weak Burdened Blind Stone Fly");
    }

    #[test]
    fn fainting_is_deadly_weak_is_bad() {
        assert_eq!(hunger_tone("Hungry"), Tone::Warn);
        assert_eq!(hunger_tone("Weak"), Tone::Bad);
        assert_eq!(hunger_tone("Fainting"), Tone::Deadly);
        assert_eq!(hunger_tone("Fainted"), Tone::Deadly);
    }

    #[test]
    fn the_five_deadly_conditions_are_deadly() {
        for name in [
            "Stone",
            "Slime",
            "Strngl",
            "Strangled",
            "FoodPois",
            "TermIll",
        ] {
            assert_eq!(condition_tone(name), Tone::Deadly, "{name}");
            assert!(deadly_words(name).is_some(), "{name} has a banner");
        }
        for name in ["Blind", "Conf", "Lev", "Ride", "Unknown"] {
            assert_ne!(condition_tone(name), Tone::Deadly, "{name}");
            assert_eq!(deadly_words(name), None);
        }
    }

    #[test]
    fn experience_thresholds_follow_newuexp() {
        assert_eq!(newuexp(0), 0);
        assert_eq!(newuexp(1), 20);
        assert_eq!(newuexp(2), 40);
        assert_eq!(newuexp(9), 5120);
        assert_eq!(newuexp(10), 10_000);
        assert_eq!(newuexp(19), 5_120_000);
        assert_eq!(newuexp(20), 10_000_000);
        assert_eq!(xp_progress(1, 0), (0.0, 20));
        assert_eq!(xp_progress(1, 10), (0.5, 20));
        assert_eq!(xp_progress(3, 57), ((57.0 - 40.0) / 40.0, 80));
        assert_eq!(xp_progress(2, 500).0, 1.0);
    }

    #[test]
    fn low_hp_is_prayers_rule() {
        // five or fewer is always low
        assert!(critically_low_hp(5, 100, 10));
        // level 1-5: a fifth of the maximum
        assert!(critically_low_hp(6, 30, 3));
        assert!(!critically_low_hp(7, 30, 3));
        // level 6-13: a sixth
        assert!(critically_low_hp(10, 60, 8));
        assert!(!critically_low_hp(11, 60, 8));
        // a huge maximum counts as 15 per level
        assert!(critically_low_hp(6, 1000, 2));
        assert!(!critically_low_hp(16, 16, 1));
    }

    #[test]
    fn attributes_compare_with_their_percentile() {
        assert_eq!(attr_rank("16"), Some(16.0));
        assert!(attr_rank("18/50") > attr_rank("18/10"));
        assert!(attr_rank("18/**") > attr_rank("18/99"));
        assert!(attr_rank("19") > attr_rank("18/**").map(|v| v - 0.5));
        assert_eq!(attr_rank("x"), None);
    }

    #[test]
    fn the_log_docks_when_the_canvas_is_narrow() {
        assert!(!compact(1920.0));
        assert!(compact(1600.0));
        assert!(!compact(2560.0));
    }

    #[test]
    fn forms_read_as_titles() {
        assert_eq!(title_case("valkyrie"), "Valkyrie");
        assert_eq!(title_case("wood nymph"), "Wood Nymph");
    }

    #[test]
    fn the_mode_banner_holds_then_fades() {
        assert_eq!(flash_alpha(-0.1), 0.0);
        assert_eq!(flash_alpha(0.0), 1.0);
        assert_eq!(flash_alpha(FLASH_SECS * 0.5), 1.0);
        let late = flash_alpha(FLASH_SECS * 0.9);
        assert!(late > 0.0 && late < 1.0, "{late}");
        assert_eq!(flash_alpha(FLASH_SECS), 0.0);
    }

    #[test]
    fn messages_after_the_last_input_are_new() {
        let mut log = MessageLog::new();
        log.push("restored".into(), 0, None, true);
        log.push("old".into(), 0, Some(1), false);
        log.push("You die...".into(), nh_world::ATR_URGENT, Some(2), false);
        log.push("fresh".into(), 0, Some(2), false);
        let looks: Vec<Look> = log.iter().map(|m| message_look(m, 2)).collect();
        let at = |new, urgent| Look { new, urgent };
        assert_eq!(
            looks,
            [
                at(false, false),
                at(false, false),
                at(true, true),
                at(true, false)
            ]
        );
        // restored history is never new
        assert!(!message_look(log.iter().next().unwrap(), 0).new);
    }
}
