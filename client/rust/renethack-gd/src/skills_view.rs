//! The skills' panel's rows (`skills`): a group's name, a skill's name, its
//! level as pips and words, and what the window marks it with (it can
//! advance now, later, or no further), each in the language now.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Control, Label, StyleBoxFlat, VBoxContainer};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use nh_i18n::Case;
use nh_i18n::lexicon::Lexicon;

use crate::i18n::{self, EngineKind};
use crate::skills::{Group, Level, Mark, Skill};
use crate::theme::{self, Face};
use crate::tr;

/// A skill's row, a group's heading, a line of the legend.
pub const ROW_H: f32 = 34.0;
pub const HEADING_H: f32 = 40.0;
pub const NOTE_H: f32 = 26.0;
/// The pips' column, the level's, the mark's.
const PIPS_W: f32 = 84.0;
const LEVEL_W: f32 = 132.0;
const MARK_W: f32 = 132.0;

pub fn level_name(l: Level) -> String {
    match l {
        Level::Restricted => tr!("skills-restricted"),
        Level::Unskilled => tr!("skills-unskilled"),
        Level::Basic => tr!("skills-basic"),
        Level::Skilled => tr!("skills-skilled"),
        Level::Expert => tr!("skills-expert"),
        Level::Master => tr!("skills-master"),
        Level::GrandMaster => tr!("skills-grand-master"),
    }
}

/// A group's name: the client's words for the three it knows, else the
/// engine's.
pub fn group_name(g: Group, english: &str) -> String {
    match g {
        Group::Fighting => tr!("skills-fighting"),
        Group::Weapon => tr!("skills-weapons"),
        Group::Spellcasting => tr!("skills-spellcasting"),
        Group::Other => i18n::engine(EngineKind::Window, english).into_owned(),
    }
}

/// A skill's name as the player reads it, with a capital: the lexicon's
/// skill first (polearms is one weapon's kind there, древковое оружие, not
/// the plural of a polearm), else the engine's name.
pub fn skill_name(english: &str) -> String {
    let skill = (i18n::lang() == i18n::Lang::Ru)
        .then(|| Lexicon::ru().noun("skill", english))
        .flatten()
        .map(|n| n.singular(Case::Nom).to_string());
    let said = skill.unwrap_or_else(|| i18n::engine(EngineKind::Name, english).into_owned());
    let mut chars = said.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What the mark says, if anything.
pub fn mark_text(m: Mark) -> Option<String> {
    match m {
        Mark::Advance => Some(tr!("skills-can-advance")),
        Mark::Later => Some(tr!("skills-later")),
        Mark::Peaked => Some(tr!("skills-peaked")),
        Mark::None => None,
    }
}

fn label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    l
}

/// A group's heading row.
pub fn heading(name: &str) -> Gd<Control> {
    let mut col = VBoxContainer::new_alloc();
    col.set_mouse_filter(MouseFilter::IGNORE);
    col.add_theme_constant_override("separation", 2);
    col.set_alignment(godot::classes::box_container::AlignmentMode::END);
    col.set_custom_minimum_size(Vector2::new(0.0, HEADING_H));
    col.add_child(&label(name, Face::Caps, 16, theme::GOLD));
    let mut rule = godot::classes::ColorRect::new_alloc();
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule.set_color(Color {
        a: 0.55,
        ..theme::GOLD_DIM
    });
    rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    col.add_child(&rule);
    col.upcast()
}

/// A line of the legend ("Skills flagged by "*"..."), as the engine says
/// it.
pub fn note(english: &str) -> Gd<Control> {
    let mut l = label(
        &i18n::engine(EngineKind::Window, english),
        Face::Body,
        14,
        theme::TEXT_DIM,
    );
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_custom_minimum_size(Vector2::new(0.0, NOTE_H));
    l.upcast()
}

/// A mark as a pill: gold for a skill that can advance now.
fn pill(m: Mark, text: &str) -> Gd<Label> {
    let (color, border, fill) = match m {
        Mark::Advance => (
            theme::GOLD_BRIGHT,
            theme::GOLD,
            Color::from_rgba(0.72, 0.54, 0.23, 0.22),
        ),
        Mark::Peaked => (
            theme::GOLD,
            theme::GOLD_DIM,
            Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        ),
        _ => (
            theme::TEXT_DIM,
            theme::IRON,
            Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        ),
    };
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(fill);
    sb.set_border_width_all(1);
    sb.set_border_color(border);
    sb.set_corner_radius_all(9);
    sb.set_content_margin(godot::builtin::Side::LEFT, 8.0);
    sb.set_content_margin(godot::builtin::Side::RIGHT, 8.0);
    let mut l = label(text, Face::BodyBold, 13, color);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.add_theme_stylebox_override("normal", &sb);
    l.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    l
}

/// A skill's cells (after a pick's letter): its name, its level as pips
/// and in words, its mark; and the name's label.
pub fn cells(s: &Skill) -> (Vec<Gd<Control>>, Gd<Label>) {
    let mut name = label(&skill_name(&s.name), Face::Body, 17, theme::TEXT);
    name.set_h_size_flags(SizeFlags::EXPAND_FILL);
    name.set_clip_text(true);
    let mut out: Vec<Gd<Control>> = vec![name.clone().upcast()];
    // a pip a step above Unskilled, right by the level's word (no empty
    // ones: the window tells no maximum)
    let pips = "◆".repeat(s.level.steps());
    let mut p = label(&pips, Face::Body, 15, theme::GOLD_BRIGHT);
    p.set_custom_minimum_size(Vector2::new(PIPS_W, 0.0));
    p.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    i18n::verbatim(&p);
    out.push(p.upcast());
    let mut level = level_name(s.level);
    if let Some((done, need)) = s.practice {
        level = format!("{level}  {done}/{need}");
    }
    let mut l = label(&level, Face::BodyBold, 15, theme::TEXT);
    l.set_custom_minimum_size(Vector2::new(LEVEL_W, 0.0));
    out.push(l.upcast());
    let mut mark = Control::new_alloc();
    mark.set_mouse_filter(MouseFilter::IGNORE);
    mark.set_custom_minimum_size(Vector2::new(MARK_W, 0.0));
    if let Some(t) = mark_text(s.mark) {
        let mut p = pill(s.mark, &t);
        theme::place(&p, [0.0, 0.5, 1.0, 0.5], [0.0, -11.0, 0.0, 11.0]);
        p.set_v_size_flags(SizeFlags::FILL);
        mark.add_child(&p);
    }
    out.push(mark);
    (out, name)
}
