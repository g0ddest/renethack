//! NetHack colours, the monospace font and the dark UI theme.

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter};
use godot::classes::{Button, Control, Label, StyleBoxFlat, SystemFont, Theme};
use godot::prelude::*;

use crate::ui_events::{UiEvent, UiQueue, push};

/// NetHack's CLR_* colours 0..15; NO_COLOR (8) shows as gray. Black is
/// lifted so it stays visible on the dark background.
const PALETTE: [(u8, u8, u8); 16] = [
    (85, 85, 110),   // black
    (205, 40, 40),   // red
    (40, 180, 40),   // green
    (170, 110, 30),  // brown
    (60, 90, 230),   // blue
    (190, 60, 190),  // magenta
    (30, 175, 185),  // cyan
    (190, 190, 190), // gray
    (190, 190, 190), // no colour
    (255, 140, 30),  // orange
    (90, 255, 90),   // bright green
    (255, 240, 60),  // yellow
    (110, 130, 255), // bright blue
    (255, 110, 255), // bright magenta
    (100, 255, 255), // bright cyan
    (255, 255, 255), // white
];

/// A NetHack colour (attribute bits above the low byte are ignored).
pub fn nh_color(c: i32) -> Color {
    let (r, g, b) = usize::try_from(c & 0xff)
        .ok()
        .and_then(|i| PALETTE.get(i).copied())
        .unwrap_or(PALETTE[8]);
    Color::from_rgba8(r, g, b, 255)
}

pub const BG: Color = Color::from_rgb(0.05, 0.055, 0.07);
pub const PANEL_BG: Color = Color::from_rgba(0.08, 0.09, 0.12, 0.92);
pub const PANEL_BORDER: Color = Color::from_rgb(0.28, 0.3, 0.38);
pub const TEXT: Color = Color::from_rgb(0.86, 0.86, 0.84);
pub const TEXT_DIM: Color = Color::from_rgb(0.5, 0.52, 0.55);
pub const ACCENT: Color = Color::from_rgb(0.95, 0.78, 0.35);
pub const WARN: Color = Color::from_rgb(1.0, 0.45, 0.3);

pub const FONT_SIZE: i32 = 16;

const MONO_FONTS: [&str; 5] = [
    "DejaVu Sans Mono",
    "Menlo",
    "Consolas",
    "Liberation Mono",
    "monospace",
];

/// A monospace system font (first one installed from the list).
pub fn mono_font() -> Gd<SystemFont> {
    let mut font = SystemFont::new_gd();
    let names: Vec<GString> = MONO_FONTS.iter().map(|n| GString::from(*n)).collect();
    font.set_font_names(&PackedStringArray::from(names.as_slice()));
    font
}

/// The same font in bold.
pub fn mono_bold() -> Gd<SystemFont> {
    let mut font = mono_font();
    font.set_font_weight(700);
    font
}

/// A panel background: dark, with a thin border.
pub fn panel_style(bg: Color) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(1);
    sb.set_border_color(PANEL_BORDER);
    sb.set_corner_radius_all(4);
    sb.set_content_margin_all(10.0);
    sb
}

fn button_style(bg: Color, border: Color) -> Gd<StyleBoxFlat> {
    let mut sb = panel_style(bg);
    sb.set_border_color(border);
    sb.set_content_margin_all(6.0);
    sb.set_content_margin(Side::LEFT, 14.0);
    sb.set_content_margin(Side::RIGHT, 14.0);
    sb
}

/// The dark theme every layer's root Control uses.
pub fn dark_theme() -> Gd<Theme> {
    let mut theme = Theme::new_gd();
    theme.set_default_font(&mono_font());
    theme.set_default_font_size(FONT_SIZE);
    let panel = panel_style(PANEL_BG);
    for class in ["PanelContainer", "Panel"] {
        theme.set_stylebox("panel", class, &panel);
    }
    theme.set_stylebox(
        "normal",
        "Button",
        &button_style(Color::from_rgb(0.14, 0.15, 0.2), PANEL_BORDER),
    );
    theme.set_stylebox(
        "hover",
        "Button",
        &button_style(Color::from_rgb(0.2, 0.22, 0.3), ACCENT),
    );
    theme.set_stylebox(
        "pressed",
        "Button",
        &button_style(Color::from_rgb(0.25, 0.22, 0.12), ACCENT),
    );
    theme.set_stylebox(
        "disabled",
        "Button",
        &button_style(Color::from_rgb(0.1, 0.1, 0.12), PANEL_BORDER),
    );
    for class in ["Label", "Button", "LineEdit", "ItemList", "OptionButton"] {
        theme.set_color("font_color", class, TEXT);
    }
    theme.set_color("font_hover_color", "Button", ACCENT);
    theme.set_color("font_disabled_color", "Button", TEXT_DIM);
    theme.set_color("default_color", "RichTextLabel", TEXT);
    theme.set_stylebox(
        "normal",
        "LineEdit",
        &button_style(Color::from_rgb(0.03, 0.03, 0.05), PANEL_BORDER),
    );
    theme.set_stylebox(
        "focus",
        "LineEdit",
        &button_style(Color::from_rgba(0.0, 0.0, 0.0, 0.0), ACCENT),
    );
    theme.set_stylebox(
        "panel",
        "ItemList",
        &button_style(Color::from_rgb(0.03, 0.03, 0.05), PANEL_BORDER),
    );
    theme.set_font("bold_font", "RichTextLabel", &mono_bold());
    theme
}

/// Anchors (left, top, right, bottom) and offsets in pixels.
pub fn place<T: Inherits<Control>>(node: &Gd<T>, anchors: [f32; 4], offsets: [f32; 4]) {
    let mut c = node.clone().upcast::<Control>();
    let sides = [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM];
    for (i, side) in sides.into_iter().enumerate() {
        c.set_anchor(side, anchors[i]);
        c.set_offset(side, offsets[i]);
    }
}

/// Fill the parent and let the mouse through (layout-only containers).
pub fn full_rect_ignore<T: Inherits<Control>>(node: &Gd<T>) {
    place(node, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
    node.clone()
        .upcast::<Control>()
        .set_mouse_filter(MouseFilter::IGNORE);
}

pub fn label(text: &str) -> Gd<Label> {
    let mut l = Label::new_alloc();
    l.set_text(text);
    l.set_mouse_filter(MouseFilter::IGNORE);
    l
}

/// A button that queues `ev` when pressed; it never takes keyboard focus,
/// so Space and Enter stay with the game.
pub fn button(text: &str, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_text(text);
    b.set_focus_mode(FocusMode::NONE);
    let q = queue.clone();
    b.signals().pressed().connect(move || push(&q, ev.clone()));
    b
}

/// Text for a BBCode label, shown literally.
pub fn bbcode_escape(s: &str) -> String {
    s.replace('[', "[lb]")
}

/// "#rrggbb" for BBCode colours.
pub fn hex(c: Color) -> String {
    format!("#{}", c.to_html_without_alpha())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbcode_brackets_are_escaped() {
        assert_eq!(bbcode_escape("a [b] c"), "a [lb]b] c");
    }

    #[test]
    fn palette_maps_no_color_and_out_of_range_to_gray() {
        assert_eq!(nh_color(8), nh_color(7));
        assert_eq!(nh_color(99), nh_color(8));
        assert_eq!(nh_color(-1), nh_color(8));
        // attribute bits above the colour byte do not change it
        assert_eq!(nh_color(1 | (1 << 8)), nh_color(1));
        assert_ne!(nh_color(1), nh_color(9));
    }
}
