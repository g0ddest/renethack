//! The spells' panel's rows (`spells`): a spell's school on a medallion,
//! its name, school and level, its chance of failure and how well it is
//! remembered, each in the language now.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{ColorRect, Control, Label, TextureRect, VBoxContainer};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::i18n::{self, EngineKind};
use crate::icons::{self, Glyph};
use crate::spells::{Retention, School, Spell};
use crate::theme::{self, Face};
use crate::tr;

/// A spell's row, and the sort's.
pub const ROW_H: f32 = 60.0;
pub const SORT_H: f32 = 40.0;
/// The medallion, the failure's column and the memory's.
const MEDALLION: f32 = 44.0;
const FAIL_W: f32 = 96.0;
const MEMORY_W: f32 = 132.0;
const BAR_H: f32 = 6.0;

pub fn school_glyph(s: School) -> Glyph {
    match s {
        School::Attack => Glyph::Flames,
        School::Healing => Glyph::Heart,
        School::Divination => Glyph::Eye,
        School::Enchantment => Glyph::Star,
        School::Clerical => Glyph::Ankh,
        School::Escape => Glyph::Boots,
        School::Matter => Glyph::Boulder,
    }
}

pub fn school_name(s: School) -> String {
    match s {
        School::Attack => tr!("spells-attack"),
        School::Healing => tr!("spells-healing"),
        School::Divination => tr!("spells-divination"),
        School::Enchantment => tr!("spells-enchantment"),
        School::Clerical => tr!("spells-clerical"),
        School::Escape => tr!("spells-escape"),
        School::Matter => tr!("spells-matter"),
    }
}

/// A spell's name as the player reads it, with a capital.
pub fn spell_name(english: &str) -> String {
    let said = i18n::engine(EngineKind::Name, english);
    let mut chars = said.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    l
}

fn column(width: Option<f32>) -> Gd<VBoxContainer> {
    let mut c = VBoxContainer::new_alloc();
    c.set_mouse_filter(MouseFilter::IGNORE);
    c.add_theme_constant_override("separation", 0);
    c.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    match width {
        Some(w) => c.set_custom_minimum_size(Vector2::new(w, 0.0)),
        None => c.set_h_size_flags(SizeFlags::EXPAND_FILL),
    }
    c
}

/// The colour of a chance of failure: green for a sure spell, red for a
/// gamble.
fn fail_color(fail: u8) -> Color {
    match fail {
        0..=20 => theme::GOOD,
        21..=60 => theme::WARN,
        _ => theme::DANGER,
    }
}

/// A medallion of `glyph`.
pub fn medallion(glyph: Glyph, side: f32) -> Gd<TextureRect> {
    let mut t = TextureRect::new_alloc();
    t.set_mouse_filter(MouseFilter::IGNORE);
    t.set_expand_mode(ExpandMode::IGNORE_SIZE);
    t.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    t.set_custom_minimum_size(Vector2::new(side, side));
    t.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    t.set_texture(&icons::emblem(glyph));
    t
}

/// How well it is remembered: a bar as full as the most it may be, the
/// range under it.
fn memory(r: Retention) -> Gd<VBoxContainer> {
    let mut col = column(Some(MEMORY_W));
    let (most, text, color) = match r {
        Retention::Full => (100, "100%".to_string(), theme::GOLD_BRIGHT),
        Retention::Range(lo, hi) => (hi, format!("{lo}%–{hi}%"), theme::GOLD),
        Retention::Gone => (0, tr!("spells-gone"), theme::DANGER),
    };
    let mut track = Control::new_alloc();
    track.set_mouse_filter(MouseFilter::IGNORE);
    track.set_custom_minimum_size(Vector2::new(MEMORY_W, BAR_H));
    let mut bg = ColorRect::new_alloc();
    bg.set_mouse_filter(MouseFilter::IGNORE);
    bg.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.55));
    theme::full_rect_ignore(&bg);
    track.add_child(&bg);
    let mut fill = ColorRect::new_alloc();
    fill.set_mouse_filter(MouseFilter::IGNORE);
    fill.set_color(color);
    theme::place(
        &fill,
        [0.0, 0.0, f32::from(most) / 100.0, 1.0],
        [0.0, 0.0, 0.0, 0.0],
    );
    track.add_child(&fill);
    col.add_child(&track);
    let mut l = label(&text, Face::Body, 14, theme::TEXT_DIM);
    l.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    if r != Retention::Gone {
        i18n::verbatim(&l);
    }
    col.add_child(&l);
    col
}

/// A spell's cells after its letter: the medallion, the name over the
/// school and level, the failure, the memory; and the name's label.
pub fn cells(s: &Spell) -> (Vec<Gd<Control>>, Gd<Label>) {
    let mut out: Vec<Gd<Control>> = vec![medallion(school_glyph(s.school), MEDALLION).upcast()];
    let mut name = column(None);
    let mut n = label(&spell_name(&s.name), Face::BodyBold, 19, theme::TEXT);
    n.set_clip_text(true);
    name.add_child(&n);
    let name_label = n.clone();
    let what = tr!(
        "spells-school-level",
        school = school_name(s.school),
        level = s.level
    );
    name.add_child(&label(&what, Face::Body, 14, theme::TEXT_DIM));
    out.push(name.upcast());
    let mut fail = column(Some(FAIL_W));
    let mut v = label(
        &format!("{}%", s.fail),
        Face::BodyBold,
        19,
        fail_color(s.fail),
    );
    v.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    i18n::verbatim(&v);
    fail.add_child(&v);
    let mut c = label(&tr!("spells-fail"), Face::Caps, 12, theme::TEXT_DIM);
    c.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    fail.add_child(&c);
    out.push(fail.upcast());
    out.push(memory(s.retention).upcast());
    (out, name_label)
}

/// The sort's cells after its letter, and its label.
pub fn sort_cells() -> (Vec<Gd<Control>>, Gd<Label>) {
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(MEDALLION, 24.0));
    icon.set_texture(&icons::glyph_icon(Glyph::Grid));
    icon.set_modulate(theme::GOLD);
    let mut l = label(&tr!("spells-sort"), Face::Body, 17, theme::TEXT_DIM);
    l.set_h_size_flags(SizeFlags::EXPAND_FILL);
    (vec![icon.upcast(), l.clone().upcast()], l)
}
