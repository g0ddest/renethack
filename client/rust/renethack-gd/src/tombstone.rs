//! The end screen's tombstone: genl_outrip's stone drawn (the
//! `ui_tombstone` shader; a plain grey slab without it), its words carved
//! on its face in the language now.

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::MouseFilter;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{ColorRect, Control, Label, VBoxContainer};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use crate::layouts::{self, Tombstone};
use crate::theme::{self, Face};
use crate::tr;

/// The stone's control, the shader's `size`.
pub const SIZE: Vector2 = Vector2::new(300.0, 460.0);
/// The face's margins in the control (left, top, right, bottom): inside
/// the shader's polished face, the words from under the arch's top down
/// to above the moss.
const FACE: [f32; 4] = [52.0, 60.0, 52.0, 120.0];
/// Letters cut in the stone: dark, their lower edge catching the light.
const CARVED: Color = Color::from_rgb(0.14, 0.13, 0.12);
const CARVED_LIGHT: Color = Color::from_rgba(0.88, 0.85, 0.78, 0.45);
/// A name longer than this is cut smaller.
const LONG_NAME: usize = 10;

pub fn view(t: &Tombstone) -> Gd<Control> {
    let mut root = Control::new_alloc();
    root.set_custom_minimum_size(SIZE);
    root.set_mouse_filter(MouseFilter::IGNORE);
    let mut stone = ColorRect::new_alloc();
    theme::full_rect_ignore(&stone);
    match theme::ui_material("ui_tombstone") {
        Some(mut m) => {
            m.set_shader_parameter("size", &SIZE.to_variant());
            stone.set_color(Color::WHITE);
            stone.set_material(&m);
        }
        None => stone.set_color(Color::from_rgb(0.42, 0.41, 0.39)),
    }
    root.add_child(&stone);
    let mut face = VBoxContainer::new_alloc();
    face.set_alignment(AlignmentMode::BEGIN);
    face.add_theme_constant_override("separation", 6);
    face.set_mouse_filter(MouseFilter::IGNORE);
    theme::place(
        &face,
        [0.0, 0.0, 1.0, 1.0],
        [FACE[0], FACE[1], -FACE[2], -FACE[3]],
    );
    let width = SIZE.x - FACE[0] - FACE[2];
    face.add_child(&carved(&tr!("rip-rest-in-peace"), Face::Title, 22, width));
    let name_size = if t.name.chars().count() > LONG_NAME {
        22
    } else {
        28
    };
    face.add_child(&carved(
        &layouts::typed(&t.name),
        Face::TitleBold,
        name_size,
        width,
    ));
    face.add_child(&carved(
        &tr!("rip-gold", gold = t.gold),
        Face::Body,
        18,
        width,
    ));
    if !t.death.is_empty() {
        face.add_child(&carved(&layouts::death(&t.death), Face::Body, 18, width));
    }
    face.add_child(&carved(&t.year.to_string(), Face::Title, 22, width));
    root.add_child(&face);
    root
}

fn carved(text: &str, face: Face, size: i32, width: f32) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, CARVED);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_custom_minimum_size(Vector2::new(width, 0.0));
    l.add_theme_color_override("font_shadow_color", CARVED_LIGHT);
    l.add_theme_constant_override("shadow_offset_x", 1);
    l.add_theme_constant_override("shadow_offset_y", 1);
    l.set_mouse_filter(MouseFilter::IGNORE);
    l
}
