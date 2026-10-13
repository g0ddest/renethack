//! The journal's panel (`journal`): the events #chronicle logged, each by
//! its turn, beside the message history as the log keeps it, each message
//! in the language now.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    ColorRect, Control, HBoxContainer, Label, RichTextLabel, ScrollContainer, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::Message;

use crate::i18n::{self, EngineKind};
use crate::journal::Journal;
use crate::theme::{self, Face};
use crate::tr;

/// Between the columns; the events' share of the width.
const GAP: f32 = 24.0;
const EVENTS_SHARE: f32 = 0.42;
/// A turn's column.
const TURN_W: f32 = 72.0;

/// The panel's content, and the messages' text the game fills (one text
/// for the log's thousand lines: a label a line would cost a frame).
pub struct JournalView {
    pub root: Gd<Control>,
    messages: Gd<RichTextLabel>,
    filled: bool,
}

impl JournalView {
    /// The messages are still to be put in.
    pub fn wants_messages(&self) -> bool {
        !self.filled
    }

    /// The message history, oldest first, a message a row by its turn.
    pub fn fill<'a>(&mut self, log: impl Iterator<Item = &'a Message>) {
        let dim = theme::hex(theme::TEXT_DIM);
        let mut rows = String::new();
        for m in log {
            let turn = m.turn.map_or(String::new(), |t| tr!("hud-turn", turn = t));
            let text = theme::bbcode_escape(&i18n::engine_message(m));
            rows.push_str(&format!(
                "[cell padding=0,2,10,2][right][color={dim}]{}[/color][/right][/cell][cell expand=1 padding=0,2,0,2]{text}[/cell]",
                theme::bbcode_escape(&turn)
            ));
        }
        let text = if rows.is_empty() {
            format!("[color={dim}]{}[/color]", tr!("journal-no-messages"))
        } else {
            format!("[table=2]{rows}[/table]")
        };
        self.messages.set_text(&text);
        self.filled = true;
    }

    /// The messages as they show (self-tests).
    pub fn messages_text(&self) -> String {
        self.messages.get_parsed_text().to_string()
    }
}

fn label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_vertical_alignment(VerticalAlignment::TOP);
    l
}

fn vbox(gap: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", gap);
    b
}

/// A row: the turn in its column, the words wrapping beside it.
fn row(turn: &str, text: &str, size: i32, turn_color: Color) -> Gd<HBoxContainer> {
    let mut r = HBoxContainer::new_alloc();
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.add_theme_constant_override("separation", 10);
    let mut t = label(turn, Face::Body, size - 2, turn_color);
    t.set_custom_minimum_size(Vector2::new(TURN_W, 0.0));
    t.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    r.add_child(&t);
    let mut w = label(text, Face::Body, size, theme::TEXT);
    w.set_autowrap_mode(AutowrapMode::WORD_SMART);
    w.set_h_size_flags(SizeFlags::EXPAND_FILL);
    r.add_child(&w);
    r
}

/// A column's name over a line.
fn heading(name: &str) -> Gd<VBoxContainer> {
    let mut col = vbox(4);
    col.add_child(&label(name, Face::Caps, 16, theme::GOLD));
    let mut rule = ColorRect::new_alloc();
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule.set_color(Color {
        a: 0.55,
        ..theme::GOLD_DIM
    });
    rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    col.add_child(&rule);
    col
}

/// The widths of the events' column and the messages'.
pub fn widths(width: f32) -> (f32, f32) {
    let events = ((width - GAP) * EVENTS_SHARE).floor();
    (events, width - GAP - events)
}

/// The journal `width` wide and `height` tall: the events in a column of
/// their own, the messages in `messages` (the panel's scroller, which the
/// keys move), empty until `JournalView::fill`.
pub fn view(
    j: &Journal,
    width: f32,
    height: f32,
    mut messages: Gd<ScrollContainer>,
) -> JournalView {
    let (events_w, messages_w) = widths(width);
    let mut root = HBoxContainer::new_alloc();
    root.set_mouse_filter(MouseFilter::IGNORE);
    root.add_theme_constant_override("separation", GAP as i32);
    root.set_custom_minimum_size(Vector2::new(width, height));

    // the events, by turn
    let mut left = vbox(8);
    left.set_custom_minimum_size(Vector2::new(events_w, 0.0));
    let title = if j.major {
        tr!("journal-major-events")
    } else {
        tr!("journal-events")
    };
    left.add_child(&heading(&title));
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(events_w, height - 40.0));
    let mut list = vbox(6);
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    for e in &j.events {
        let text = i18n::engine(EngineKind::Message, &e.text).into_owned();
        let turn = tr!("hud-turn", turn = e.turn);
        list.add_child(&row(&turn, &text, 16, theme::GOLD_BRIGHT));
    }
    if j.events.is_empty() {
        list.add_child(&label(
            &tr!("journal-no-events"),
            Face::Body,
            16,
            theme::TEXT_DIM,
        ));
    }
    scroll.add_child(&list);
    left.add_child(&scroll);
    root.add_child(&left);

    // the messages, as the log keeps them
    let mut right = vbox(8);
    right.set_custom_minimum_size(Vector2::new(messages_w, 0.0));
    right.set_h_size_flags(SizeFlags::EXPAND_FILL);
    right.add_child(&heading(&tr!("journal-messages")));
    messages.set_custom_minimum_size(Vector2::new(messages_w, height - 40.0));
    let mut text = RichTextLabel::new_alloc();
    text.set_use_bbcode(true);
    text.set_fit_content(true);
    text.set_scroll_active(false);
    text.set_selection_enabled(false);
    text.set_mouse_filter(MouseFilter::PASS);
    text.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text.add_theme_font_override("normal_font", &theme::font(Face::Body));
    text.add_theme_font_size_override("normal_font_size", 15);
    text.add_theme_color_override("default_color", theme::TEXT);
    messages.add_child(&text);
    right.add_child(&messages);
    root.add_child(&right);
    JournalView {
        root: root.upcast(),
        messages: text,
        filled: false,
    }
}
