//! Status panel, message log, prompt line, hover tooltip and the full log.

use godot::classes::control::{GrowDirection, MouseFilter};
use godot::classes::{
    CanvasLayer, Control, HBoxContainer, Label, PanelContainer, ProgressBar, RichTextLabel,
    StyleBoxFlat, VBoxContainer,
};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::{Message, Status, World};

use crate::theme::{self, bbcode_escape, hex, place};
use crate::ui_events::{UiEvent, UiQueue};

/// Messages kept in the log panel (older ones are in the full log).
const LOG_LINES: usize = 60;

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
        tag.set_custom_minimum_size(Vector2::new(28.0, 0.0));
        row.add_child(&tag);
        let mut bar = ProgressBar::new_alloc();
        bar.set_show_percentage(false);
        bar.set_custom_minimum_size(Vector2::new(220.0, 14.0));
        bar.set_mouse_filter(MouseFilter::IGNORE);
        let mut bg = StyleBoxFlat::new_gd();
        bg.set_bg_color(Color::from_rgb(0.12, 0.12, 0.14));
        bg.set_corner_radius_all(3);
        let mut fill = StyleBoxFlat::new_gd();
        fill.set_corner_radius_all(3);
        bar.add_theme_stylebox_override("background", &bg);
        bar.add_theme_stylebox_override("fill", &fill);
        let mut holder = VBoxContainer::new_alloc();
        holder.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        holder.set_mouse_filter(MouseFilter::IGNORE);
        holder.add_child(&bar);
        row.add_child(&holder);
        let text = theme::label("");
        row.add_child(&text);
        parent.add_child(&row);
        Bar { bar, fill, text }
    }

    fn set(&mut self, value: Option<i64>, max: Option<i64>, color: Color) {
        let (v, m) = (value.unwrap_or(0), max.unwrap_or(0).max(1));
        self.bar.set_max(m as f64);
        self.bar.set_value(v.clamp(0, m) as f64);
        self.fill.set_bg_color(color);
        self.text.set_text(&match (value, max) {
            (Some(v), Some(m)) => format!(" {v}/{m}"),
            _ => String::new(),
        });
    }
}

pub struct Hud {
    root: Gd<Control>,
    status: Gd<RichTextLabel>,
    hp: Bar,
    pw: Bar,
    log: Gd<RichTextLabel>,
    transient: Gd<Label>,
    prompt_panel: Gd<PanelContainer>,
    prompt: Gd<Label>,
    tooltip_panel: Gd<PanelContainer>,
    tooltip: Gd<Label>,
    full_log_panel: Gd<PanelContainer>,
    full_log: Gd<RichTextLabel>,
    status_text: String,
    log_key: Option<(u64, u64, bool)>,
    full_log_dirty: bool,
}

fn panel() -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::STOP);
    p
}

fn rich() -> Gd<RichTextLabel> {
    let mut r = RichTextLabel::new_alloc();
    r.set_use_bbcode(true);
    r.set_focus_mode(godot::classes::control::FocusMode::NONE);
    r.set_selection_enabled(false);
    r
}

/// "HP" colour: >50 % green, >25 % yellow, else red.
fn hp_color(hp: Option<i64>, max: Option<i64>) -> Color {
    match (hp, max) {
        (Some(h), Some(m)) if m > 0 && h * 2 > m => Color::from_rgb(0.25, 0.75, 0.3),
        (Some(h), Some(m)) if m > 0 && h * 4 > m => Color::from_rgb(0.9, 0.8, 0.2),
        _ => Color::from_rgb(0.85, 0.2, 0.2),
    }
}

/// Deadly conditions are shown in red.
const DEADLY: [&str; 5] = ["Stone", "Slime", "Strngl", "FoodPois", "TermIll"];

/// The status lines as plain text (the self-test reads them).
pub fn status_lines(status: &Status, catalog: Option<&Catalog>) -> Vec<String> {
    let get = |f: &str| status.get(f).unwrap_or("").to_string();
    let pair = |a: &str, b: &str| match (status.get(a), status.get(b)) {
        (Some(v), Some(m)) => Some(format!("{v}({m})")),
        _ => None,
    };
    let mut line1 = get("title");
    if let Some(a) = status.get("align").filter(|a| !a.is_empty()) {
        line1.push_str(&format!("  {a}"));
    }
    let mut line2: Vec<String> = Vec::new();
    if let Some(d) = status.get("leveldesc") {
        line2.push(d.to_string());
    }
    if let Some(g) = status.get("gold") {
        line2.push(format!("${}", g.trim_start_matches('$')));
    }
    if let Some(hp) = pair("hp", "hpmax") {
        line2.push(format!("HP:{hp}"));
    }
    if let Some(pw) = pair("energy", "energymax") {
        line2.push(format!("Pw:{pw}"));
    }
    if let Some(ac) = status.get("ac") {
        line2.push(format!("AC:{ac}"));
    }
    if let Some(hd) = status.get("hitdice") {
        line2.push(format!("HD:{hd}"));
    } else if let Some(xl) = status.get("xlevel") {
        line2.push(match status.get("exp") {
            Some(e) => format!("XL:{xl}/{e}"),
            None => format!("XL:{xl}"),
        });
    }
    if let Some(t) = status.get("time") {
        line2.push(format!("T:{t}"));
    }
    if let Some(s) = status.get("score") {
        line2.push(format!("S:{s}"));
    }
    let mut line3: Vec<String> = [
        ("St", "str"),
        ("Dx", "dex"),
        ("Co", "con"),
        ("In", "int"),
        ("Wi", "wis"),
        ("Ch", "cha"),
    ]
    .iter()
    .filter_map(|(tag, f)| status.get(f).map(|v| format!("{tag}:{v}")))
    .collect();
    for f in ["hunger", "cap"] {
        if let Some(v) = status.get(f).filter(|v| !v.is_empty()) {
            line3.push(v.to_string());
        }
    }
    if let Some(c) = catalog {
        line3.extend(status.condition_names(c));
    }
    vec![line1, line2.join("  "), line3.join(" ")]
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
            [12.0, 12.0, 560.0, 12.0],
        );
        let mut col = VBoxContainer::new_alloc();
        col.set_mouse_filter(MouseFilter::IGNORE);
        let mut status = rich();
        status.set_fit_content(true);
        status.set_scroll_active(false);
        status.set_custom_minimum_size(Vector2::new(540.0, 0.0));
        status.set_mouse_filter(MouseFilter::IGNORE);
        col.add_child(&status);
        let hp = Bar::new(&mut col, "HP");
        let pw = Bar::new(&mut col, "Pw");
        status_panel.add_child(&col);
        root.add_child(&status_panel);

        // log, bottom left
        let mut log_panel = panel();
        place(
            &log_panel,
            [0.0, 1.0, 0.0, 1.0],
            [12.0, -230.0, 820.0, -12.0],
        );
        let mut log_col = VBoxContainer::new_alloc();
        log_col.set_mouse_filter(MouseFilter::IGNORE);
        let mut head = HBoxContainer::new_alloc();
        head.set_mouse_filter(MouseFilter::IGNORE);
        let mut transient = theme::label("");
        transient.add_theme_color_override("font_color", theme::ACCENT);
        transient.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        transient.set_clip_text(true);
        head.add_child(&transient);
        let mut log_button = theme::button("Log F9", &queue, UiEvent::ToggleFullLog);
        log_button.add_theme_font_size_override("font_size", 12);
        head.add_child(&log_button);
        log_col.add_child(&head);
        let mut log = rich();
        log.set_scroll_follow(true);
        log.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        log_col.add_child(&log);
        log_panel.add_child(&log_col);
        root.add_child(&log_panel);

        // prompt line, top centre
        let mut prompt_panel = PanelContainer::new_alloc();
        prompt_panel.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &prompt_panel,
            [0.5, 0.0, 0.5, 0.0],
            [-300.0, 12.0, 300.0, 12.0],
        );
        prompt_panel.set_h_grow_direction(GrowDirection::BOTH);
        let mut prompt = theme::label("");
        prompt.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        prompt.add_theme_color_override("font_color", theme::ACCENT);
        prompt.add_theme_font_size_override("font_size", 18);
        prompt_panel.add_child(&prompt);
        prompt_panel.set_visible(false);
        root.add_child(&prompt_panel);

        // tooltip, follows the mouse
        let mut tooltip_panel = PanelContainer::new_alloc();
        tooltip_panel.set_mouse_filter(MouseFilter::IGNORE);
        let tooltip = theme::label("");
        tooltip_panel.add_child(&tooltip);
        tooltip_panel.set_visible(false);
        root.add_child(&tooltip_panel);

        // full log, F9
        let mut full_log_panel = panel();
        place(
            &full_log_panel,
            [0.0, 0.0, 1.0, 1.0],
            [80.0, 60.0, -80.0, -60.0],
        );
        let mut full_log = rich();
        full_log.set_scroll_follow(true);
        full_log_panel.add_child(&full_log);
        full_log_panel.set_visible(false);
        root.add_child(&full_log_panel);

        Hud {
            root,
            status,
            hp,
            pw,
            log,
            transient,
            prompt_panel,
            prompt,
            tooltip_panel,
            tooltip,
            full_log_panel,
            full_log,
            status_text: String::new(),
            log_key: None,
            full_log_dirty: true,
        }
    }

    pub fn sync(&mut self, world: &mut World, catalog: Option<&Catalog>) {
        if world.status.take_changed() || self.status_text.is_empty() {
            self.show_status(&world.status, catalog);
        }
        let full_open = self.full_log_panel.is_visible();
        let key = (world.log.last_seq(), world.input_seq(), full_open);
        if self.log_key != Some(key) {
            self.log_key = Some(key);
            self.show_log(world);
            self.full_log_dirty = true;
        }
        if full_open && self.full_log_dirty {
            self.full_log_dirty = false;
            let text: Vec<String> = world
                .log
                .iter()
                .map(|m| {
                    let turn = m.turn.map_or(String::new(), |t| format!("T:{t:<6} "));
                    format!(
                        "[color={}]{}[/color]{}",
                        hex(theme::TEXT_DIM),
                        turn,
                        bbcode_escape(&m.text)
                    )
                })
                .collect();
            self.full_log.set_text(&format!(
                "[b]Messages[/b]  (F9 closes)\n{}",
                text.join("\n")
            ));
        }
        let transient = world.transient.clone().unwrap_or_default();
        if self.transient.get_text().to_string() != transient {
            self.transient.set_text(&transient);
        }
    }

    fn show_status(&mut self, status: &Status, catalog: Option<&Catalog>) {
        let lines = status_lines(status, catalog);
        self.status_text = lines.join("\n");
        let (hp, hpmax) = (status.number("hp"), status.number("hpmax"));
        let color = hp_color(hp, hpmax);
        let mut line2 = bbcode_escape(&lines[1]);
        if let Some(pos) = line2.find("HP:") {
            let end = line2[pos..].find("  ").map_or(line2.len(), |e| pos + e);
            line2 = format!(
                "{}[color={}]{}[/color]{}",
                &line2[..pos],
                hex(color),
                &line2[pos..end],
                &line2[end..]
            );
        }
        let mut line3 = bbcode_escape(&lines[2]);
        for d in DEADLY {
            line3 = line3.replace(d, &format!("[color={}]{d}[/color]", hex(theme::WARN)));
        }
        self.status.set_text(&format!(
            "[b][color={}]{}[/color][/b]\n{}\n{}",
            hex(theme::ACCENT),
            bbcode_escape(&lines[0]),
            line2,
            line3
        ));
        self.hp.set(hp, hpmax, color);
        self.pw.set(
            status.number("energy"),
            status.number("energymax"),
            Color::from_rgb(0.3, 0.45, 0.95),
        );
    }

    fn show_log(&mut self, world: &World) {
        let recent: Vec<&Message> = world.log.iter().rev().take(LOG_LINES).collect();
        let lines: Vec<String> = recent
            .iter()
            .rev()
            .map(|m| {
                let new = m.seq > world.input_seq() && !m.from_history;
                let color = match (m.urgent, new) {
                    (true, _) => theme::WARN,
                    (false, true) => Color::from_rgb(1.0, 1.0, 1.0),
                    (false, false) => theme::TEXT_DIM,
                };
                let text = bbcode_escape(&m.text);
                if new || m.urgent {
                    format!("[color={}][b]{text}[/b][/color]", hex(color))
                } else {
                    format!("[color={}]{text}[/color]", hex(color))
                }
            })
            .collect();
        self.log.set_text(&lines.join("\n"));
    }

    /// The status as plain text.
    pub fn status_text(&self) -> &str {
        &self.status_text
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

    pub fn set_tooltip(&mut self, text: Option<&str>, screen_pos: Vector2) {
        match text {
            Some(t) => {
                self.tooltip.set_text(t);
                self.tooltip_panel.reset_size();
                self.tooltip_panel
                    .set_position(screen_pos + Vector2::new(18.0, 18.0));
                self.tooltip_panel.set_visible(true);
            }
            None => self.tooltip_panel.set_visible(false),
        }
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
        if !on {
            self.tooltip_panel.set_visible(false);
            self.full_log_panel.set_visible(false);
        }
    }

    pub fn toggle_full_log(&mut self) {
        let on = !self.full_log_panel.is_visible();
        self.full_log_panel.set_visible(on);
        self.full_log_dirty = true;
    }

    /// Forget what was shown (a new game).
    pub fn reset(&mut self) {
        self.status_text.clear();
        self.log_key = None;
        self.full_log_dirty = true;
        self.status.set_text("");
        self.log.set_text("");
        self.set_prompt_line(None);
        self.tooltip_panel.set_visible(false);
        self.full_log_panel.set_visible(false);
    }
}
