//! The HP and Pw orbs (ui-design §1.3): liquid in a glass sphere set in a
//! bronze ring. A loss flashes white and leaves the lost part as a pale
//! ghost for a moment before it drains (Diablo 3 style).

use godot::classes::control::MouseFilter;
use godot::classes::{ColorRect, Control, Label, ShaderMaterial};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::theme::{self, Face, place};

/// The orb's side in design pixels.
pub const SIZE: f32 = 148.0;
/// Seconds the level takes to settle (ease-out).
const SETTLE: f64 = 0.25;
/// Seconds the ghost of a loss holds before it drains.
const GHOST_HOLD: f64 = 0.6;
/// Seconds the white flash of a loss takes to fade.
const FLASH: f64 = 0.3;
/// Throbs a second while low.
const PULSE_HZ: f64 = 1.1;

/// The share of the orb filled for `value` (0 when unknown).
pub fn level(value: Option<(i64, i64)>) -> f32 {
    match value {
        Some((v, m)) if m > 0 => (v.clamp(0, m) as f64 / m as f64) as f32,
        _ => 0.0,
    }
}

/// One step of an ease-out from `from` to `to` over `dt` seconds.
pub fn approach(from: f32, to: f32, dt: f64) -> f32 {
    // about 95 % of the way in SETTLE seconds
    let k = 1.0 - (-3.0 * dt / SETTLE).exp();
    let v = from + (to - from) * k as f32;
    if (v - to).abs() < 0.001 { to } else { v }
}

pub struct Orb {
    root: Gd<Control>,
    material: Option<Gd<ShaderMaterial>>,
    text: Gd<Label>,
    name: &'static str,
    value: Option<(i64, i64)>,
    shown: f32,
    target: f32,
    ghost: f32,
    ghost_until: f64,
    flash_at: f64,
    low: bool,
    last_tick: Option<f64>,
}

impl Orb {
    /// `name` for the tooltip ("Hit points"); the liquid from `deep` at
    /// the bottom to `bright` at the surface.
    pub fn new(name: &'static str, deep: Color, bright: Color) -> Orb {
        let mut root = Control::new_alloc();
        root.set_custom_minimum_size(Vector2::new(SIZE, SIZE));
        root.set_size(Vector2::new(SIZE, SIZE));
        root.set_mouse_filter(MouseFilter::STOP);
        let mut disc = ColorRect::new_alloc();
        disc.set_mouse_filter(MouseFilter::IGNORE);
        place(&disc, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        let material = theme::ui_material("ui_orb");
        match &material {
            Some(m) => {
                let mut m = m.clone();
                m.set_shader_parameter("deep", &deep.to_variant());
                m.set_shader_parameter("bright", &bright.to_variant());
                disc.set_material(&m);
            }
            None => disc.set_color(deep),
        }
        root.add_child(&disc);
        let mut text = theme::styled_label("", Face::Black, 22, theme::TEXT);
        theme::outline(&text, 6);
        place(&text, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        text.set_horizontal_alignment(HorizontalAlignment::CENTER);
        text.set_vertical_alignment(VerticalAlignment::CENTER);
        root.add_child(&text);
        Orb {
            root,
            material,
            text,
            name,
            value: None,
            shown: 0.0,
            target: 0.0,
            ghost: 0.0,
            ghost_until: 0.0,
            flash_at: f64::NEG_INFINITY,
            low: false,
            last_tick: None,
        }
    }

    pub fn node(&self) -> Gd<Control> {
        self.root.clone()
    }

    /// A new value from the status; `now` in seconds.
    pub fn set(&mut self, value: Option<(i64, i64)>, now: f64) {
        if self.value == value {
            return;
        }
        let was = self.value;
        self.value = value;
        let target = level(value);
        // a loss (the value fell, not the maximum rose) flashes and leaves a ghost
        let lost = matches!((was, value), (Some((a, _)), Some((b, _))) if b < a);
        if lost {
            self.ghost = self.ghost.max(self.shown);
            self.ghost_until = now + GHOST_HOLD;
            self.flash_at = now;
        }
        if was.is_none() {
            self.shown = target;
        }
        self.target = target;
        let text = value.map_or(String::new(), |(v, m)| format!("{v} / {m}"));
        self.text.set_text(&text);
        let tip = value.map_or(String::new(), |(v, m)| format!("{} {v} of {m}", self.name));
        self.root.set_tooltip_text(&tip);
    }

    /// Throb (HP critically low by the engine's own rule).
    pub fn set_low(&mut self, on: bool) {
        self.low = on;
    }

    /// Animate; called every frame.
    pub fn tick(&mut self, now: f64) {
        let dt = self.last_tick.map_or(0.0, |t| (now - t).clamp(0.0, 0.25));
        self.last_tick = Some(now);
        self.shown = approach(self.shown, self.target, dt);
        if now >= self.ghost_until {
            self.ghost = approach(self.ghost, self.shown, dt * 0.6);
        }
        if self.ghost < self.shown {
            self.ghost = self.shown;
        }
        let flash = (1.0 - (now - self.flash_at) / FLASH).clamp(0.0, 1.0) as f32;
        let pulse = if self.low {
            (0.5 + 0.5 * (now * std::f64::consts::TAU * PULSE_HZ).sin()) as f32
        } else {
            0.0
        };
        if let Some(m) = &mut self.material {
            m.set_shader_parameter("fill", &self.shown.to_variant());
            m.set_shader_parameter("ghost", &self.ghost.to_variant());
            m.set_shader_parameter("flash", &flash.to_variant());
            m.set_shader_parameter("pulse", &pulse.to_variant());
        }
    }

    /// (value shown, level drawn) (self-tests).
    pub fn view(&self) -> (Option<(i64, i64)>, f32) {
        (self.value, self.shown)
    }

    pub fn reset(&mut self) {
        self.value = None;
        self.shown = 0.0;
        self.target = 0.0;
        self.ghost = 0.0;
        self.low = false;
        self.text.set_text("");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_is_the_share_left() {
        assert_eq!(level(Some((16, 16))), 1.0);
        assert_eq!(level(Some((4, 16))), 0.25);
        assert_eq!(level(Some((-3, 16))), 0.0);
        assert_eq!(level(Some((20, 16))), 1.0);
        assert_eq!(level(Some((0, 0))), 0.0);
        assert_eq!(level(None), 0.0);
    }

    #[test]
    fn the_level_eases_out_and_arrives() {
        let a = approach(1.0, 0.0, 0.05);
        assert!(a < 1.0 && a > 0.0, "{a}");
        let mut v = 1.0;
        for _ in 0..40 {
            v = approach(v, 0.5, 1.0 / 60.0);
        }
        assert_eq!(v, 0.5, "settled within two thirds of a second");
        assert_eq!(approach(0.3, 0.3, 0.0), 0.3);
    }
}
