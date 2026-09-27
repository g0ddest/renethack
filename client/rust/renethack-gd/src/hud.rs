//! Status panel, message log, prompt line, hover tooltip and the full log.
//!
//! Only the status panel, the log panel and the full log take the mouse
//! (`MOUSE_FILTER_STOP`); everything else lets it through to the map.

use std::f64::consts::TAU;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, GrowDirection, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    CanvasLayer, Control, HBoxContainer, HFlowContainer, Label, PanelContainer, ProgressBar,
    RichTextLabel, StyleBoxFlat, Time, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::{Message, Status, World};

use crate::theme::{self, bbcode_escape, hex, place};
use crate::ui_events::{UiEvent, UiQueue};

/// Messages kept in the log panel; the wheel scrolls through them (the
/// full log has the rest).
const LOG_LINES: usize = 60;
/// Messages the log panel shows at once.
const LOG_VISIBLE: f32 = 8.0;
/// Height of one line of the monospace font at `theme::FONT_SIZE`.
const LINE_HEIGHT: f32 = 19.0;
const MARGIN: f32 = 12.0;
const STATUS_WIDTH: f32 = 430.0;
const LOG_WIDTH: f32 = 780.0;
/// Header row, the panel's content margins and the box separation.
const LOG_CHROME: f32 = 26.0 + 20.0 + 4.0;
const LOG_HEIGHT: f32 = LOG_VISIBLE * LINE_HEIGHT + LOG_CHROME;
/// Tooltip offset from the mouse.
const TOOLTIP_GAP: f32 = 18.0;
/// Deadly chips pulse this many times a second.
const PULSE_HZ: f64 = 1.25;

const HP_GREEN: Color = Color::from_rgb(0.25, 0.75, 0.3);
const HP_YELLOW: Color = Color::from_rgb(0.9, 0.8, 0.2);
const HP_RED: Color = Color::from_rgb(0.85, 0.2, 0.2);
const PW_BLUE: Color = Color::from_rgb(0.3, 0.45, 0.95);
const GOLD: Color = Color::from_rgb(1.0, 0.84, 0.3);
const BRIGHT: Color = Color::from_rgb(1.0, 1.0, 1.0);
/// The bright end of a deadly chip's pulse.
const DEADLY_GLOW: Color = Color::from_rgb(0.95, 0.2, 0.15);

// ---- what the panel shows (plain data, tested without Godot) ----

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

/// The status panel's content.
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
    /// Hunger, encumbrance, then the conditions in catalog order.
    pub chips: Vec<Chip>,
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
        let mut chips = Vec::new();
        if let Some(h) = text("hunger") {
            let tone = hunger_tone(&h);
            chips.push(Chip { text: h, tone });
        }
        if let Some(c) = text("cap") {
            let tone = encumbrance_tone(&c);
            chips.push(Chip { text: c, tone });
        }
        chips.extend(conditions.iter().map(|c| Chip {
            text: c.clone(),
            tone: condition_tone(c),
        }));
        StatusView {
            title: text("title").unwrap_or_default(),
            align: text("align"),
            fields,
            hp: pair("hp", "hpmax"),
            pw: pair("energy", "energymax"),
            attrs,
            chips,
        }
    }

    /// The panel as plain text ("Dlvl:1  $0  AC:6", "HP:16(16)  Pw:2(2)"...).
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
        if !self.chips.is_empty() {
            let chips: Vec<&str> = self.chips.iter().map(|c| c.text.as_str()).collect();
            lines.push(chips.join(" "));
        }
        lines
    }
}

/// HP bar colour by share: >50 % green, >25 % yellow, else red.
fn hp_color(hp: Option<(i64, i64)>) -> Color {
    match hp {
        Some((h, m)) if m > 0 && h * 2 > m => HP_GREEN,
        Some((h, m)) if m > 0 && h * 4 > m => HP_YELLOW,
        _ => HP_RED,
    }
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
/// the warning colour (and bold while new).
fn message_bbcode(m: &Message, input_seq: u64) -> String {
    let look = message_look(m, input_seq);
    let text = bbcode_escape(&m.text);
    let color = match (look.urgent, look.new) {
        (true, true) => theme::WARN,
        (true, false) => theme::WARN.lerp(theme::TEXT_DIM, 0.45),
        (false, true) => BRIGHT,
        (false, false) => theme::TEXT_DIM,
    };
    if look.new {
        format!("[color={}][b]{text}[/b][/color]", hex(color))
    } else {
        format!("[color={}]{text}[/color]", hex(color))
    }
}

/// "tag value" pairs with dim tags, as BBCode, `gap` spaces apart.
fn tagged<S: AsRef<str>>(pairs: &[(S, String)], gap: usize) -> String {
    let dim = hex(theme::TEXT_DIM);
    pairs
        .iter()
        .map(|(tag, v)| {
            let (tag, v) = (tag.as_ref(), bbcode_escape(v));
            match tag {
                "" => v,
                "$" => format!("[color={}]$[/color]{v}", hex(GOLD)),
                _ => format!("[color={dim}]{tag}[/color] {v}"),
            }
        })
        .collect::<Vec<_>>()
        .join(&" ".repeat(gap))
}

// ---- Godot nodes ----

fn chip_colors(tone: Tone) -> (Color, Color) {
    // (background, border and text)
    match tone {
        Tone::Deadly => (
            Color::from_rgb(0.62, 0.08, 0.06),
            Color::from_rgb(1.0, 0.55, 0.5),
        ),
        Tone::Bad => (
            Color::from_rgb(0.36, 0.16, 0.04),
            Color::from_rgb(1.0, 0.6, 0.25),
        ),
        Tone::Warn => (
            Color::from_rgb(0.26, 0.22, 0.06),
            Color::from_rgb(0.95, 0.85, 0.35),
        ),
        Tone::Info => (
            Color::from_rgb(0.1, 0.16, 0.26),
            Color::from_rgb(0.55, 0.75, 1.0),
        ),
    }
}

/// A chip and its background (deadly chips pulse through it).
fn chip(c: &Chip) -> (Gd<PanelContainer>, Gd<StyleBoxFlat>) {
    let (bg, fg) = chip_colors(c.tone);
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(1);
    sb.set_border_color(fg);
    sb.set_corner_radius_all(9);
    sb.set_content_margin(Side::LEFT, 8.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.add_theme_stylebox_override("panel", &sb);
    let mut l = theme::label(&c.text);
    let text = if c.tone == Tone::Deadly { BRIGHT } else { fg };
    l.add_theme_color_override("font_color", text);
    l.add_theme_font_size_override("font_size", 14);
    if c.tone == Tone::Deadly {
        l.add_theme_font_override("font", &theme::mono_bold());
    }
    p.add_child(&l);
    (p, sb)
}

struct Bar {
    bar: Gd<ProgressBar>,
    fill: Gd<StyleBoxFlat>,
    text: Gd<Label>,
}

impl Bar {
    fn new(parent: &mut Gd<VBoxContainer>, name: &str) -> Bar {
        let mut row = HBoxContainer::new_alloc();
        row.set_mouse_filter(MouseFilter::IGNORE);
        let mut tag = theme::label(name);
        tag.add_theme_color_override("font_color", theme::TEXT_DIM);
        tag.set_custom_minimum_size(Vector2::new(28.0, 0.0));
        row.add_child(&tag);
        let mut bar = ProgressBar::new_alloc();
        bar.set_show_percentage(false);
        bar.set_custom_minimum_size(Vector2::new(0.0, 14.0));
        bar.set_h_size_flags(SizeFlags::EXPAND_FILL);
        bar.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        bar.set_mouse_filter(MouseFilter::IGNORE);
        let mut bg = StyleBoxFlat::new_gd();
        bg.set_bg_color(Color::from_rgb(0.12, 0.12, 0.15));
        bg.set_border_width_all(1);
        bg.set_border_color(Color::from_rgb(0.2, 0.21, 0.26));
        bg.set_corner_radius_all(3);
        let mut fill = StyleBoxFlat::new_gd();
        fill.set_corner_radius_all(3);
        bar.add_theme_stylebox_override("background", &bg);
        bar.add_theme_stylebox_override("fill", &fill);
        row.add_child(&bar);
        let mut text = theme::label("");
        text.set_custom_minimum_size(Vector2::new(96.0, 0.0));
        text.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        row.add_child(&text);
        parent.add_child(&row);
        Bar { bar, fill, text }
    }

    fn set(&mut self, value: Option<(i64, i64)>, color: Color) {
        let (v, m) = value.unwrap_or((0, 0));
        let m = m.max(1);
        self.bar.set_max(m as f64);
        self.bar.set_value(v.clamp(0, m) as f64);
        self.fill.set_bg_color(color);
        self.text
            .add_theme_color_override("font_color", color.lerp(BRIGHT, 0.35));
        self.text
            .set_text(&value.map_or(String::new(), |(v, m)| format!("{v}/{m}")));
    }
}

pub struct Hud {
    root: Gd<Control>,
    status_panel: Gd<PanelContainer>,
    title: Gd<Label>,
    align: Gd<Label>,
    fields: Gd<RichTextLabel>,
    hp: Bar,
    pw: Bar,
    attrs: Gd<RichTextLabel>,
    chip_box: Gd<HFlowContainer>,
    chips: Vec<(Chip, Gd<PanelContainer>, Gd<StyleBoxFlat>)>,
    log_panel: Gd<PanelContainer>,
    log: Gd<RichTextLabel>,
    transient_panel: Gd<PanelContainer>,
    transient: Gd<Label>,
    prompt_panel: Gd<PanelContainer>,
    prompt: Gd<Label>,
    tooltip_panel: Gd<PanelContainer>,
    tooltip: Gd<Label>,
    tooltip_text: Option<String>,
    full_log_panel: Gd<PanelContainer>,
    full_log: Gd<RichTextLabel>,
    full_log_count: Gd<Label>,
    status_text: String,
    /// (newest message, input seq, full log open) last shown.
    log_key: Option<(u64, u64, bool)>,
    full_log_dirty: bool,
    /// The full log jumps to the newest message on its next rebuild.
    full_log_follow: bool,
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

/// A panel that takes the mouse (status, log, full log).
fn panel() -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::STOP);
    p
}

/// A panel the mouse goes through, with its own background.
fn overlay(bg: Color, border: Color) -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    let mut sb = theme::panel_style(bg);
    sb.set_border_color(border);
    sb.set_content_margin(Side::TOP, 4.0);
    sb.set_content_margin(Side::BOTTOM, 4.0);
    p.add_theme_stylebox_override("panel", &sb);
    p
}

fn rich(mouse: MouseFilter) -> Gd<RichTextLabel> {
    let mut r = RichTextLabel::new_alloc();
    r.set_use_bbcode(true);
    r.set_focus_mode(FocusMode::NONE);
    r.set_selection_enabled(false);
    r.set_mouse_filter(mouse);
    r
}

/// A one-line BBCode label as tall as its text.
fn rich_line() -> Gd<RichTextLabel> {
    let mut r = rich(MouseFilter::IGNORE);
    r.set_fit_content(true);
    r.set_scroll_active(false);
    r.set_autowrap_mode(AutowrapMode::OFF);
    r
}

fn vbox(separation: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", separation);
    b
}

fn hbox() -> Gd<HBoxContainer> {
    let mut b = HBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b
}

fn small_button(text: &str, queue: &UiQueue, ev: UiEvent) -> Gd<godot::classes::Button> {
    let mut b = theme::button(text, queue, ev);
    b.add_theme_font_size_override("font_size", 12);
    b
}

impl Hud {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Hud {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);

        // status, top left
        let mut status_panel = panel();
        place(
            &status_panel,
            [0.0, 0.0, 0.0, 0.0],
            [MARGIN, MARGIN, MARGIN + STATUS_WIDTH, MARGIN],
        );
        let mut col = vbox(4);
        let mut head = hbox();
        let mut title = theme::label("");
        title.add_theme_color_override("font_color", theme::ACCENT);
        title.add_theme_font_override("font", &theme::mono_bold());
        title.set_h_size_flags(SizeFlags::EXPAND_FILL);
        title.set_clip_text(true);
        head.add_child(&title);
        let mut align = theme::label("");
        align.add_theme_color_override("font_color", theme::TEXT_DIM);
        head.add_child(&align);
        col.add_child(&head);
        let fields = rich_line();
        col.add_child(&fields);
        let hp = Bar::new(&mut col, "HP");
        let pw = Bar::new(&mut col, "Pw");
        let attrs = rich_line();
        col.add_child(&attrs);
        let mut chip_box = HFlowContainer::new_alloc();
        chip_box.set_mouse_filter(MouseFilter::IGNORE);
        chip_box.add_theme_constant_override("h_separation", 6);
        chip_box.add_theme_constant_override("v_separation", 4);
        chip_box.set_visible(false);
        col.add_child(&chip_box);
        status_panel.add_child(&col);
        root.add_child(&status_panel);

        // log, bottom left; the wheel scrolls it
        let mut log_panel = panel();
        place(
            &log_panel,
            [0.0, 1.0, 0.0, 1.0],
            [MARGIN, -MARGIN - LOG_HEIGHT, MARGIN + LOG_WIDTH, -MARGIN],
        );
        let mut log_col = vbox(4);
        let mut log_head = hbox();
        let mut log_title = theme::label("Messages");
        log_title.add_theme_color_override("font_color", theme::TEXT_DIM);
        log_title.add_theme_font_size_override("font_size", 13);
        log_title.set_h_size_flags(SizeFlags::EXPAND_FILL);
        log_title.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        log_head.add_child(&log_title);
        log_head.add_child(&small_button("History  F9", &queue, UiEvent::ToggleFullLog));
        log_col.add_child(&log_head);
        let mut log = rich(MouseFilter::STOP);
        log.set_scroll_follow(true);
        log.set_v_size_flags(SizeFlags::EXPAND_FILL);
        log_col.add_child(&log);
        log_panel.add_child(&log_col);
        root.add_child(&log_panel);

        // transient message (getpos autodescribe...), its own line above the log
        let mut transient_panel = overlay(
            Color::from_rgba(0.1, 0.09, 0.05, 0.9),
            Color::from_rgb(0.45, 0.38, 0.2),
        );
        place(
            &transient_panel,
            [0.0, 1.0, 0.0, 1.0],
            [
                MARGIN,
                -MARGIN - LOG_HEIGHT - 6.0,
                MARGIN + LOG_WIDTH,
                -MARGIN - LOG_HEIGHT - 6.0,
            ],
        );
        transient_panel.set_v_grow_direction(GrowDirection::BEGIN);
        let mut transient = theme::label("");
        transient.add_theme_color_override("font_color", theme::ACCENT);
        transient.set_clip_text(true);
        transient_panel.add_child(&transient);
        transient_panel.set_visible(false);
        root.add_child(&transient_panel);

        // prompt line, top: centred in a row right of the status panel
        let mut prompt_row = hbox();
        prompt_row.set_alignment(AlignmentMode::CENTER);
        place(
            &prompt_row,
            [0.0, 0.0, 1.0, 0.0],
            [STATUS_WIDTH + 2.0 * MARGIN, MARGIN, -MARGIN, MARGIN],
        );
        let mut prompt_panel = overlay(Color::from_rgba(0.08, 0.07, 0.04, 0.94), theme::ACCENT);
        let mut prompt = theme::label("");
        prompt.set_horizontal_alignment(HorizontalAlignment::CENTER);
        prompt.add_theme_color_override("font_color", theme::ACCENT);
        prompt.add_theme_font_size_override("font_size", 18);
        prompt_panel.add_child(&prompt);
        prompt_panel.set_visible(false);
        prompt_row.add_child(&prompt_panel);
        root.add_child(&prompt_row);

        // full log, F9; scrolls with the wheel
        let mut full_log_panel = panel();
        full_log_panel.add_theme_stylebox_override(
            "panel",
            &theme::panel_style(Color::from_rgba(0.06, 0.065, 0.085, 0.985)),
        );
        place(
            &full_log_panel,
            [0.0, 0.0, 1.0, 1.0],
            [80.0, 60.0, -80.0, -60.0],
        );
        let mut full_col = vbox(8);
        let mut full_head = hbox();
        let mut full_title = theme::label("Message history");
        full_title.add_theme_color_override("font_color", theme::ACCENT);
        full_title.add_theme_font_override("font", &theme::mono_bold());
        full_head.add_child(&full_title);
        let mut full_log_count = theme::label("");
        full_log_count.add_theme_color_override("font_color", theme::TEXT_DIM);
        full_log_count.set_h_size_flags(SizeFlags::EXPAND_FILL);
        full_head.add_child(&full_log_count);
        full_head.add_child(&small_button("Close  F9", &queue, UiEvent::ToggleFullLog));
        full_col.add_child(&full_head);
        let mut full_log = rich(MouseFilter::STOP);
        full_log.set_scroll_follow(true);
        full_log.set_v_size_flags(SizeFlags::EXPAND_FILL);
        full_col.add_child(&full_log);
        full_log_panel.add_child(&full_col);
        full_log_panel.set_visible(false);
        root.add_child(&full_log_panel);

        // tooltip, follows the mouse; on top of everything
        let mut tooltip_panel = overlay(
            Color::from_rgba(0.04, 0.045, 0.06, 0.95),
            theme::PANEL_BORDER,
        );
        let mut tooltip = theme::label("");
        tooltip.add_theme_font_size_override("font_size", 15);
        tooltip_panel.add_child(&tooltip);
        tooltip_panel.set_visible(false);
        root.add_child(&tooltip_panel);

        Hud {
            root,
            status_panel,
            title,
            align,
            fields,
            hp,
            pw,
            attrs,
            chip_box,
            chips: Vec::new(),
            log_panel,
            log,
            transient_panel,
            transient,
            prompt_panel,
            prompt,
            tooltip_panel,
            tooltip,
            tooltip_text: None,
            full_log_panel,
            full_log,
            full_log_count,
            status_text: String::new(),
            log_key: None,
            full_log_dirty: true,
            full_log_follow: true,
        }
    }

    /// Called every frame while a game is on screen.
    pub fn sync(&mut self, world: &mut World, catalog: Option<&Catalog>) {
        if world.status.take_changed() || self.status_text.is_empty() {
            self.show_status(&world.status, catalog);
        }
        self.pulse();
        let full_open = self.full_log_panel.is_visible();
        let last_seq = world.log.last_seq();
        let key = (last_seq, world.input_seq(), full_open);
        if self.log_key != Some(key) {
            let news = self.log_key.is_none_or(|(seen, ..)| seen != last_seq);
            self.log_key = Some(key);
            self.show_log(world, news);
            self.full_log_dirty = true;
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

    fn show_status(&mut self, status: &Status, catalog: Option<&Catalog>) {
        let conditions = catalog.map_or_else(Vec::new, |c| status.condition_names(c));
        let view = StatusView::new(status, &conditions);
        self.status_text = view.lines().join("\n");
        self.title.set_text(&view.title);
        self.align.set_text(view.align.as_deref().unwrap_or(""));
        self.fields.set_text(&tagged(&view.fields, 3));
        self.attrs.set_text(&tagged(&view.attrs, 2));
        self.hp.set(view.hp, hp_color(view.hp));
        self.pw.set(view.pw, PW_BLUE);
        let same = self.chips.len() == view.chips.len()
            && self
                .chips
                .iter()
                .zip(&view.chips)
                .all(|((a, ..), b)| a == b);
        if !same {
            for (_, mut node, _) in self.chips.drain(..) {
                node.queue_free();
            }
            for c in view.chips {
                let (node, style) = chip(&c);
                self.chip_box.add_child(&node);
                self.chips.push((c, node, style));
            }
            self.chip_box.set_visible(!self.chips.is_empty());
        }
    }

    /// Deadly chips glow and fade.
    fn pulse(&mut self) {
        if !self.chips.iter().any(|(c, ..)| c.tone == Tone::Deadly) {
            return;
        }
        let t = Time::singleton().get_ticks_msec() as f64 / 1000.0;
        let k = 0.5 + 0.5 * (t * TAU * PULSE_HZ).sin();
        let (bg, _) = chip_colors(Tone::Deadly);
        let glow = bg.lerp(DEADLY_GLOW, k);
        for (c, _, style) in &mut self.chips {
            if c.tone == Tone::Deadly {
                style.set_bg_color(glow);
            }
        }
    }

    /// `follow`: new messages came, show the newest; else keep the place
    /// the player scrolled to.
    fn show_log(&mut self, world: &World, follow: bool) {
        let input_seq = world.input_seq();
        let recent: Vec<&Message> = world.log.iter().rev().take(LOG_LINES).collect();
        let lines: Vec<String> = recent
            .iter()
            .rev()
            .map(|m| message_bbcode(m, input_seq))
            .collect();
        set_scrolled_text(&mut self.log, &lines.join("\n"), follow);
    }

    fn show_full_log(&mut self, world: &World, follow: bool) {
        let input_seq = world.input_seq();
        let dim = hex(theme::TEXT_DIM);
        let lines: Vec<String> = world
            .log
            .iter()
            .map(|m| {
                let turn = m.turn.map_or(String::new(), |t| format!("T:{t}"));
                format!(
                    "[color={dim}]{turn:<8}[/color]{}",
                    message_bbcode(m, input_seq)
                )
            })
            .collect();
        self.full_log_count
            .set_text(&format!("  {} messages", world.log.len()));
        set_scrolled_text(&mut self.full_log, &lines.join("\n"), follow);
    }

    /// The status as plain text.
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    /// Is a panel that takes the mouse under `pos` (then the map is not hovered)?
    pub fn covers(&self, pos: Vector2) -> bool {
        self.root.is_visible()
            && [&self.status_panel, &self.log_panel, &self.full_log_panel]
                .into_iter()
                .any(|p| p.is_visible() && p.get_global_rect().contains_point(pos))
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
                self.prompt.set_text(t);
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
            self.full_log_panel.set_visible(false);
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
        self.align.set_text("");
        self.fields.set_text("");
        self.attrs.set_text("");
        for (_, mut node, _) in self.chips.drain(..) {
            node.queue_free();
        }
        self.chip_box.set_visible(false);
        self.log.set_text("");
        self.full_log.set_text("");
        self.transient.set_text("");
        self.transient_panel.set_visible(false);
        self.set_prompt_line(None);
        self.set_tooltip(None, Vector2::ZERO);
        self.full_log_panel.set_visible(false);
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
        assert!(v.chips.is_empty(), "no hunger, no burden: {:?}", v.chips);
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
        let v = StatusView::new(&status(&[("hitdice", "4")]), &[]);
        assert_eq!(v.fields, [("HD".to_string(), "4".to_string())]);
        assert!(v.lines()[1].contains("HD:4"));
    }

    #[test]
    fn other_places_show_their_whole_name() {
        let v = StatusView::new(&status(&[("leveldesc", "Home 2 ")]), &[]);
        assert_eq!(v.fields, [(String::new(), "Home 2".to_string())]);
        assert_eq!(v.lines()[1], "Home 2");
    }

    #[test]
    fn hunger_burden_and_conditions_become_chips() {
        let s = status(&[("hunger", "Weak"), ("cap", "Burdened")]);
        let conds = ["Blind".to_string(), "Stone".to_string(), "Fly".to_string()];
        let v = StatusView::new(&s, &conds);
        let chips: Vec<(&str, Tone)> = v.chips.iter().map(|c| (c.text.as_str(), c.tone)).collect();
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
        }
        for name in ["Blind", "Conf", "Lev", "Ride", "Unknown"] {
            assert_ne!(condition_tone(name), Tone::Deadly, "{name}");
        }
    }

    #[test]
    fn hp_colour_follows_the_share_left() {
        assert_eq!(hp_color(Some((16, 16))), HP_GREEN);
        assert_eq!(hp_color(Some((9, 16))), HP_GREEN);
        assert_eq!(hp_color(Some((8, 16))), HP_YELLOW);
        assert_eq!(hp_color(Some((5, 16))), HP_YELLOW);
        assert_eq!(hp_color(Some((4, 16))), HP_RED);
        assert_eq!(hp_color(Some((0, 0))), HP_RED);
        assert_eq!(hp_color(None), HP_RED);
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
