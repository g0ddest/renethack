//! The action bar's ten slots on `1`–`0` (ui-design §4.2): the look only.
//! What a slot is bound to, and what pressing it sends, is nh-world's
//! `ActionBar`; the game drives this view through the setters and hears
//! clicks as `UiEvent::ActionSlot`.

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, InputEvent, InputEventMouseButton, Label,
    StyleBoxFlat, Texture2D, TextureRect,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;

use crate::theme::{self, Face, place};
use crate::ui_events::{UiEvent, UiQueue, push};

/// Slots on the bar.
pub const SLOTS: usize = 10;
/// A slot's side, and the gap between slots, in design pixels.
pub const SLOT: f32 = 64.0;
pub const GAP: f32 = 8.0;
/// The bar's width: ten slots and nine gaps.
pub const WIDTH: f32 = SLOTS as f32 * SLOT + (SLOTS - 1) as f32 * GAP;

/// The key of slot `i` (0-based): "1".."9", then "0".
pub fn key_label(i: usize) -> &'static str {
    ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
        .get(i)
        .copied()
        .unwrap_or("")
}

struct Slot {
    button: Gd<Button>,
    icon: Gd<TextureRect>,
    count: Gd<Label>,
    key: Gd<Label>,
    /// "+" shown over an empty slot under the mouse.
    ghost: Gd<Label>,
    /// The thin diagonal slash of an item that is gone.
    slash: Gd<ColorRect>,
    filled: bool,
    enabled: bool,
}

/// The ten slots: frames, key labels, icons, counts.
pub struct ActionBar {
    root: Gd<HBoxContainer>,
    slots: Vec<Slot>,
    active: bool,
}

fn slot_style(border: Color, width: i32, glow: bool) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(theme::SOCKET);
    sb.set_border_width_all(width);
    sb.set_border_color(border);
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    sb.set_content_margin_all(0.0);
    if glow {
        sb.set_shadow_size(8);
        sb.set_shadow_color(Color::from_rgba(0.906, 0.761, 0.478, 0.33));
    }
    sb
}

fn corner_label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let l = theme::styled_label(text, face, size, color);
    theme::outline(&l, 4);
    l
}

impl ActionBar {
    pub fn new(queue: &UiQueue) -> ActionBar {
        let mut root = HBoxContainer::new_alloc();
        root.set_mouse_filter(MouseFilter::IGNORE);
        root.add_theme_constant_override("separation", GAP as i32);
        let normal = slot_style(theme::IRON, 1, false);
        let hover = slot_style(theme::GOLD, 1, true);
        let mut pressed = slot_style(theme::GOLD_BRIGHT, 2, true);
        pressed.set_content_margin(Side::TOP, 2.0);
        let mut slots = Vec::with_capacity(SLOTS);
        for i in 0..SLOTS {
            let mut button = Button::new_alloc();
            button.set_focus_mode(FocusMode::NONE);
            button.set_custom_minimum_size(Vector2::new(SLOT, SLOT));
            button.set_mouse_filter(MouseFilter::STOP);
            for (name, sb) in [
                ("normal", &normal),
                ("hover", &hover),
                ("pressed", &pressed),
                ("hover_pressed", &pressed),
                ("disabled", &normal),
                ("focus", &normal),
            ] {
                button.add_theme_stylebox_override(name, sb);
            }
            button.set_tooltip_text(&crate::tr!("bar-slot-empty", key = key_label(i)));

            // the inner shadow of the socket
            let mut well = godot::classes::ColorRect::new_alloc();
            well.set_mouse_filter(MouseFilter::IGNORE);
            place(&well, [0.0, 0.0, 1.0, 1.0], [1.0, 1.0, -1.0, -1.0]);
            match theme::ui_material("ui_frame") {
                Some(mut m) => {
                    for (n, v) in [
                        ("size", Vector2::new(SLOT - 2.0, SLOT - 2.0).to_variant()),
                        (
                            "top_color",
                            Color::from_rgba(0.03, 0.022, 0.018, 1.0).to_variant(),
                        ),
                        (
                            "bottom_color",
                            Color::from_rgba(0.075, 0.058, 0.045, 1.0).to_variant(),
                        ),
                        (
                            "outer_color",
                            Color::from_rgba(0.0, 0.0, 0.0, 0.0).to_variant(),
                        ),
                        (
                            "border_color",
                            Color::from_rgba(0.0, 0.0, 0.0, 0.0).to_variant(),
                        ),
                        ("border_px", 0.0f32.to_variant()),
                        ("inner_shadow_px", 12.0f32.to_variant()),
                        ("corner_cut_px", 2.0f32.to_variant()),
                        ("trim_px", 0.0f32.to_variant()),
                        ("shadow_px", 0.0f32.to_variant()),
                        ("shadow_alpha", 0.0f32.to_variant()),
                    ] {
                        m.set_shader_parameter(n, &v);
                    }
                    well.set_material(&m);
                }
                None => well.set_color(theme::SOCKET),
            }
            button.add_child(&well);

            let mut icon = TextureRect::new_alloc();
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            place(&icon, [0.0, 0.0, 1.0, 1.0], [4.0, 4.0, -4.0, -4.0]);
            button.add_child(&icon);

            let mut ghost = corner_label("+", Face::Title, 34, theme::GOLD_DIM);
            place(&ghost, [0.0, 0.0, 1.0, 1.0], [0.0, -2.0, 0.0, 0.0]);
            ghost.set_horizontal_alignment(HorizontalAlignment::CENTER);
            ghost.set_vertical_alignment(VerticalAlignment::CENTER);
            ghost.set_visible(false);
            button.add_child(&ghost);

            let mut slash = ColorRect::new_alloc();
            slash.set_mouse_filter(MouseFilter::IGNORE);
            slash.set_color(Color::from_rgba(0.75, 0.22, 0.17, 0.85));
            slash.set_size(Vector2::new(SLOT * 1.2, 2.0));
            slash.set_position(Vector2::new(SLOT * 0.5 - SLOT * 0.6, SLOT * 0.5 - 1.0));
            slash.set_pivot_offset(Vector2::new(SLOT * 0.6, 1.0));
            slash.set_rotation(-std::f32::consts::FRAC_PI_4);
            slash.set_visible(false);
            button.add_child(&slash);

            let key = corner_label(key_label(i), Face::Caps, 15, theme::TEXT_DIM);
            place(&key, [0.0, 0.0, 0.0, 0.0], [7.0, 3.0, 27.0, 23.0]);
            button.add_child(&key);

            let mut count = corner_label("", Face::BodyBold, 15, theme::TEXT);
            place(&count, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, -5.0, -2.0]);
            count.set_horizontal_alignment(HorizontalAlignment::RIGHT);
            count.set_vertical_alignment(VerticalAlignment::BOTTOM);
            button.add_child(&count);

            let q = queue.clone();
            button
                .signals()
                .gui_input()
                .connect(move |ev: Gd<InputEvent>| {
                    let Ok(b) = ev.try_cast::<InputEventMouseButton>() else {
                        return;
                    };
                    if !b.is_pressed() {
                        return;
                    }
                    let button = match b.get_button_index() {
                        MouseButton::LEFT => 1,
                        MouseButton::RIGHT => 2,
                        _ => return,
                    };
                    push(&q, UiEvent::ActionSlot { slot: i, button });
                });
            let mut g = ghost.clone();
            let b = button.clone();
            button.signals().mouse_entered().connect(move || {
                // an empty slot offers itself for binding
                g.set_visible(b.get_meta("empty").try_to::<bool>().unwrap_or(true));
            });
            let mut g = ghost.clone();
            button
                .signals()
                .mouse_exited()
                .connect(move || g.set_visible(false));
            button.set_meta("empty", &true.to_variant());
            root.add_child(&button);
            slots.push(Slot {
                button,
                icon,
                count,
                key,
                ghost,
                slash,
                filled: false,
                enabled: true,
            });
        }
        ActionBar {
            root,
            slots,
            active: true,
        }
    }

    /// The bar's node, 712×64, to be placed by the HUD.
    pub fn node(&self) -> Gd<Control> {
        self.root.clone().upcast()
    }

    /// Slot `slot`'s icon; None empties it.
    pub fn set_icon(&mut self, slot: usize, icon: Option<&Gd<Texture2D>>) {
        let Some(s) = self.slots.get_mut(slot) else {
            return;
        };
        s.icon.set_texture(icon);
        s.filled = icon.is_some();
        s.button.set_meta("empty", &(!s.filled).to_variant());
        if s.filled {
            s.ghost.set_visible(false);
        }
        let color = if s.filled {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        s.key.add_theme_color_override("font_color", color);
    }

    /// The number at the bottom right (a stack's count, charges, a spell's
    /// cost); None hides it.
    pub fn set_count(&mut self, slot: usize, count: Option<&str>) {
        if let Some(s) = self.slots.get_mut(slot) {
            s.count.set_text(count.unwrap_or(""));
        }
    }

    /// A slot whose item is gone or whose spell is unaffordable: greyed,
    /// still clickable (the logic decides what a click does).
    pub fn set_enabled(&mut self, slot: usize, on: bool) {
        if let Some(s) = self.slots.get_mut(slot) {
            s.enabled = on;
            let m = if on {
                Color::WHITE
            } else {
                Color::from_rgba(0.55, 0.55, 0.55, 0.35)
            };
            s.icon.set_modulate(m);
            s.count.set_visible(on);
            s.slash.set_visible(!on && s.filled);
        }
    }

    /// What hovering slot `slot` tells.
    pub fn set_tooltip(&mut self, slot: usize, text: &str) {
        if let Some(s) = self.slots.get_mut(slot) {
            s.button.set_tooltip_text(text);
        }
    }

    /// The whole bar at 60 % while the engine is not at a command prompt.
    pub fn set_active(&mut self, on: bool) {
        if self.active != on {
            self.active = on;
            let a = if on { 1.0 } else { 0.6 };
            self.root.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
        }
    }

    /// The slots' key labels: a gamepad's chords ("LB A"), or None for the
    /// keys 1–0.
    pub fn set_key_labels(&mut self, labels: Option<&[String]>) {
        use godot::builtin::Side;
        // a chord: a small dark pill in the corner, over the icon, as on
        // consoles; a digit: as before
        let mut pill = StyleBoxFlat::new_gd();
        pill.set_bg_color(Color::from_rgba(0.035, 0.027, 0.02, 0.88));
        pill.set_border_width_all(1);
        pill.set_border_color(theme::GOLD_DIM);
        pill.set_corner_radius_all(3);
        pill.set_content_margin(Side::LEFT, 3.0);
        pill.set_content_margin(Side::RIGHT, 3.0);
        for (i, s) in self.slots.iter_mut().enumerate() {
            let chord = labels.and_then(|l| l.get(i)).filter(|t| !t.is_empty());
            match chord {
                Some(text) => {
                    s.key.set_text(text);
                    s.key.add_theme_font_size_override("font_size", 14);
                    s.key.add_theme_stylebox_override("normal", &pill);
                    s.key.add_theme_constant_override("outline_size", 0);
                    let w = 10.0 + 8.0 * text.chars().count() as f32;
                    s.key.set_offset(Side::LEFT, 2.0);
                    s.key.set_offset(Side::TOP, 2.0);
                    s.key.set_offset(Side::RIGHT, 2.0 + w);
                    s.key.set_offset(Side::BOTTOM, 21.0);
                    s.key.set_visible(true);
                }
                None => {
                    s.key.set_text(key_label(i));
                    s.key.add_theme_font_size_override("font_size", 15);
                    s.key.remove_theme_stylebox_override("normal");
                    s.key.add_theme_constant_override("outline_size", 4);
                    s.key.set_offset(Side::LEFT, 7.0);
                    s.key.set_offset(Side::TOP, 3.0);
                    s.key.set_offset(Side::RIGHT, 27.0);
                    s.key.set_offset(Side::BOTTOM, 23.0);
                    // a slot off the pad's page shows no chord
                    s.key.set_visible(labels.is_none());
                }
            }
        }
    }

    /// Where the slots are on the canvas (drop targets of the inventory).
    pub fn slot_rects(&self) -> Vec<Rect2> {
        self.slots
            .iter()
            .map(|s| s.button.get_global_rect())
            .collect()
    }

    /// (filled, enabled) of every slot (self-tests).
    pub fn slot_states(&self) -> Vec<(bool, bool)> {
        self.slots.iter().map(|s| (s.filled, s.enabled)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_keyed_one_to_nine_then_zero() {
        let keys: Vec<&str> = (0..SLOTS).map(key_label).collect();
        assert_eq!(keys, ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]);
        assert_eq!(key_label(SLOTS), "");
        assert_eq!(WIDTH, 712.0);
    }
}
