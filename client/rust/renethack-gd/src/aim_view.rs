//! The aim of a spell being cast, in the world: while the engine asks the
//! direction of a directional spell (`getdir`), eight gold arrows round
//! the hero point the ways it can go, and a mark under the hero says it
//! may be cast at them (`.`). They follow the camera, under the HUD and
//! the panels; nothing is drawn otherwise.

use godot::classes::control::MouseFilter;
use godot::classes::{CanvasLayer, Control, Polygon2D};
use godot::prelude::*;

use crate::theme;

/// The arrows' distance from the hero, their length and width, in parts
/// of a cell's step on screen.
const OUT: f32 = 0.62;
const LONG: f32 = 0.30;
const WIDE: f32 = 0.24;

pub struct AimView {
    root: Gd<Control>,
    arrows: Vec<Gd<Polygon2D>>,
    me: Gd<Polygon2D>,
    shown: bool,
}

/// An arrow pointing along +x, its tip at the origin's `LONG` ahead.
fn arrow_shape() -> PackedVector2Array {
    PackedVector2Array::from(&[
        Vector2::new(1.0, 0.0),
        Vector2::new(0.0, -0.5),
        Vector2::new(0.2, 0.0),
        Vector2::new(0.0, 0.5),
    ])
}

/// A diamond round the origin, a unit across.
fn diamond_shape() -> PackedVector2Array {
    PackedVector2Array::from(&[
        Vector2::new(0.0, -0.5),
        Vector2::new(0.5, 0.0),
        Vector2::new(0.0, 0.5),
        Vector2::new(-0.5, 0.0),
    ])
}

impl AimView {
    pub fn new(mut layer: Gd<CanvasLayer>) -> AimView {
        let mut root = Control::new_alloc();
        root.set_name("Aim");
        theme::full_rect_ignore(&root);
        root.set_mouse_filter(MouseFilter::IGNORE);
        root.set_visible(false);
        layer.add_child(&root);
        let polygon = |shape: PackedVector2Array| {
            let mut p = Polygon2D::new_alloc();
            p.set_polygon(&shape);
            p.set_color(theme::GOLD_BRIGHT);
            p
        };
        let arrows: Vec<Gd<Polygon2D>> = (0..8)
            .map(|_| {
                let p = polygon(arrow_shape());
                root.add_child(&p);
                p
            })
            .collect();
        let me = polygon(diamond_shape());
        root.add_child(&me);
        AimView {
            root,
            arrows,
            me,
            shown: false,
        }
    }

    /// Show the aim round the hero on screen at `hero`, the neighbours'
    /// cells on screen at `around` (the eight ways, NetHack's order), at
    /// `time` seconds (the arrows breathe); None hides it.
    pub fn sync(&mut self, aim: Option<(Vector2, [Vector2; 8])>, time: f64) {
        let Some((hero, around)) = aim else {
            if self.shown {
                self.root.set_visible(false);
                self.shown = false;
            }
            return;
        };
        if !self.shown {
            self.root.set_visible(true);
            self.shown = true;
        }
        let glow = 0.7 + 0.3 * (time * std::f64::consts::TAU * 0.8).sin() as f32;
        for (arrow, to) in self.arrows.iter_mut().zip(around) {
            let step = to - hero;
            let reach = step.length().max(1.0);
            arrow.set_position(hero + step * OUT);
            arrow.set_rotation(step.y.atan2(step.x));
            arrow.set_scale(Vector2::new(reach * LONG, reach * WIDE));
            arrow.set_color(Color {
                a: glow,
                ..theme::GOLD_BRIGHT
            });
        }
        let cell = around
            .iter()
            .map(|p| (*p - hero).length())
            .fold(0.0, f32::max);
        self.me.set_position(hero);
        self.me.set_scale(Vector2::new(cell * 0.22, cell * 0.12));
        self.me.set_color(Color {
            a: glow * 0.8,
            ..theme::PW
        });
    }

    /// The aim is drawn (self-tests).
    pub fn shown(&self) -> bool {
        self.shown
    }
}
