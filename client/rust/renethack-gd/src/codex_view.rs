//! The codex's panel (`codex`): the discoveries by class, each object a
//! card with its icon (its appearance's, as the inventory shows it), its
//! name and what it looks like; or the conduct, a line each. Two tabs: the
//! one shown is the engine's window now, the other a click or Tab away.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    ColorRect, Control, HBoxContainer, HFlowContainer, Label, PanelContainer, StyleBoxFlat,
    Texture2D, TextureRect, VBoxContainer,
};
use godot::global::VerticalAlignment;
use godot::prelude::*;

use crate::codex::{Discoveries, Discovery};
use crate::i18n;
use crate::theme::{self, Face};

/// A card's width at least, its icon's side.
const CARD_W: f32 = 300.0;
const ICON: f32 = 40.0;

fn label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    l
}

fn vbox(gap: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", gap);
    b
}

/// A section's name over a line.
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

/// A card's well.
fn well(unseen: bool) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(
        0.05,
        0.04,
        0.03,
        if unseen { 0.5 } else { 0.9 },
    ));
    sb.set_border_width_all(1);
    sb.set_border_color(if unseen { theme::IRON } else { theme::GOLD_DIM });
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    sb.set_content_margin_all(6.0);
    sb
}

/// An object's card: its icon, its name, what it looks like, the prices a
/// shop showed; one known from the start and never come across, dimmer.
fn card(
    d: &Discovery,
    name: &str,
    looks: Option<&str>,
    icon: Option<Gd<Texture2D>>,
) -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.add_theme_stylebox_override("panel", &well(d.unseen));
    p.set_custom_minimum_size(Vector2::new(CARD_W, 0.0));
    let mut row = HBoxContainer::new_alloc();
    row.set_mouse_filter(MouseFilter::IGNORE);
    row.add_theme_constant_override("separation", 10);
    let mut t = TextureRect::new_alloc();
    t.set_mouse_filter(MouseFilter::IGNORE);
    t.set_expand_mode(ExpandMode::IGNORE_SIZE);
    t.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    t.set_custom_minimum_size(Vector2::new(ICON, ICON));
    t.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    if let Some(tex) = icon {
        t.set_texture(&tex);
    }
    if d.unseen {
        t.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.55));
    }
    row.add_child(&t);
    let mut col = vbox(0);
    col.set_h_size_flags(SizeFlags::EXPAND_FILL);
    col.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    let color = if d.unseen {
        theme::TEXT_DIM
    } else {
        theme::TEXT
    };
    let mut n = label(name, Face::Body, 16, color);
    n.set_autowrap_mode(AutowrapMode::WORD_SMART);
    col.add_child(&n);
    if let Some(l) = looks {
        col.add_child(&label(l, Face::Body, 13, theme::TEXT_DIM));
    }
    if let Some(prices) = &d.prices {
        let mut pl = label(prices, Face::Body, 12, theme::GOLD_DIM);
        i18n::verbatim(&pl);
        pl.set_autowrap_mode(AutowrapMode::WORD_SMART);
        col.add_child(&pl);
    }
    row.add_child(&col);
    p.add_child(&row);
    p
}

/// The discoveries, `width` wide: a section a class, a card an object.
/// `heading` says a class's heading, `said` an object's name and looks,
/// `icon` its appearance's icon.
pub fn discoveries(
    d: &Discoveries,
    width: f32,
    heading_of: &dyn Fn(&str) -> String,
    said: &dyn Fn(&Discovery) -> (String, Option<String>),
    icon: &dyn Fn(&str, &Discovery) -> Option<Gd<Texture2D>>,
) -> Gd<Control> {
    let mut root = vbox(14);
    root.set_custom_minimum_size(Vector2::new(width, 0.0));
    // the window's header: the order they are in
    let mut order = label(&heading_of(&d.header), Face::Body, 14, theme::TEXT_DIM);
    order.set_autowrap_mode(AutowrapMode::WORD_SMART);
    root.add_child(&order);
    for s in &d.sections {
        if !s.heading.is_empty() {
            root.add_child(&heading(&heading_of(&s.heading)));
        }
        let mut grid = HFlowContainer::new_alloc();
        grid.set_mouse_filter(MouseFilter::IGNORE);
        grid.add_theme_constant_override("h_separation", 8);
        grid.add_theme_constant_override("v_separation", 8);
        grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
        for e in &s.entries {
            let (name, looks) = said(e);
            grid.add_child(&card(e, &name, looks.as_deref(), icon(&s.heading, e)));
        }
        root.add_child(&grid);
    }
    root.upcast()
}

/// The conduct's lines, `width` wide, each said as the engine says it.
pub fn conduct(lines: &[String], width: f32) -> Gd<Control> {
    let mut root = vbox(8);
    root.set_custom_minimum_size(Vector2::new(width, 0.0));
    for line in lines {
        let mut row = HBoxContainer::new_alloc();
        row.set_mouse_filter(MouseFilter::IGNORE);
        row.add_theme_constant_override("separation", 10);
        let mut mark = label("◆", Face::Body, 10, theme::GOLD_DIM);
        mark.set_vertical_alignment(VerticalAlignment::TOP);
        mark.set_custom_minimum_size(Vector2::new(12.0, 22.0));
        i18n::verbatim(&mark);
        row.add_child(&mark);
        let english = format!(" {line}");
        let said = i18n::engine(crate::i18n::EngineKind::Window, &english);
        let mut l = label(said.trim(), Face::Body, 17, theme::TEXT);
        l.set_autowrap_mode(AutowrapMode::WORD_SMART);
        l.set_h_size_flags(SizeFlags::EXPAND_FILL);
        row.add_child(&l);
        root.add_child(&row);
    }
    root.upcast()
}
