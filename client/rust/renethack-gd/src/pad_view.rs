//! What a gamepad shows: the radial menu (hold LT) and a strip of the
//! buttons' meanings by what is on screen. Buttons are drawn as themed
//! medallions with the controller's letters or shapes (no logos).

use godot::builtin::Side;
use godot::classes::control::MouseFilter;
use godot::classes::{CanvasLayer, Control, HBoxContainer, Label, PanelContainer, StyleBoxFlat};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::gamepad::{PadButton, PadCtx, PadKind, RADIAL};
use crate::theme::{self, Face, Frame, place};

/// The radial's radius and an entry's size, in design pixels.
const RADIUS: f32 = 170.0;
const ENTRY: Vector2 = Vector2::new(150.0, 44.0);

/// A button as a medallion: a round plate with its letter or shape.
pub fn medallion(kind: PadKind, b: PadButton) -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.06, 0.045, 0.035, 0.95));
    sb.set_border_width_all(2);
    let [r, g, bl] = kind.tint(b);
    let tint = Color::from_rgb(r, g, bl);
    sb.set_border_color(tint);
    let round = matches!(b, PadButton::A | PadButton::B | PadButton::X | PadButton::Y);
    sb.set_corner_radius_all(if round { 14 } else { 6 });
    sb.set_content_margin(Side::LEFT, if round { 7.0 } else { 6.0 });
    sb.set_content_margin(Side::RIGHT, if round { 7.0 } else { 6.0 });
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    p.add_theme_stylebox_override("panel", &sb);
    p.set_custom_minimum_size(Vector2::new(28.0, 28.0));
    let mut l = theme::styled_label(kind.label(b), Face::BodyBold, 15, tint);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    p.add_child(&l);
    p
}

/// The buttons that mean something now, and what.
pub fn hints(ctx: PadCtx) -> Vec<(Vec<PadButton>, &'static str)> {
    use PadButton::*;
    match ctx {
        PadCtx::World => vec![
            (vec![A], "Act"),
            (vec![X], "Search"),
            (vec![Y], "Inventory"),
            (vec![Lt], "Actions"),
            (vec![Rt], "Fire"),
            (vec![Lb, Rb], "Bar"),
            (vec![Start], "Commands"),
        ],
        PadCtx::Getpos => vec![(vec![A], "Pick"), (vec![B], "Cancel")],
        PadCtx::Direction => vec![(vec![A], "Here"), (vec![B], "Cancel")],
        PadCtx::Menu { any: true } => vec![
            (vec![A], "Toggle"),
            (vec![Start], "Confirm"),
            (vec![B], "Cancel"),
            (vec![Lb, Rb], "Page"),
        ],
        PadCtx::PanelBrowse => vec![
            (vec![A], "Use"),
            (vec![X], "Actions"),
            (vec![Y], "Pick up / put down"),
            (vec![Lb, Rb], "Filter"),
            (vec![B], "Close"),
        ],
        PadCtx::PanelSelect => vec![
            (vec![A], "Choose"),
            (vec![Lb, Rb], "Filter"),
            (vec![B], "Cancel"),
        ],
        PadCtx::PanelMenu => vec![
            (vec![A], "Toggle"),
            (vec![Start], "Confirm"),
            (vec![Lb, Rb], "Filter"),
            (vec![B], "Cancel"),
        ],
        PadCtx::Text => vec![
            (vec![A], "Type"),
            (vec![B], "Erase"),
            (vec![Y], "АБВ / ABC"),
            (vec![Start], "OK"),
        ],
        _ => vec![(vec![A], "OK"), (vec![B], "Back")],
    }
}

pub struct PadView {
    root: Gd<Control>,
    radial: Gd<Control>,
    entries: Vec<Gd<PanelContainer>>,
    strip: Gd<HBoxContainer>,
    shown: Option<(PadKind, PadCtx)>,
    selected: Option<usize>,
}

impl PadView {
    pub fn new(mut layer: Gd<CanvasLayer>) -> PadView {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);

        // the radial: entries on a circle around the screen's centre
        let mut radial = Control::new_alloc();
        radial.set_mouse_filter(MouseFilter::IGNORE);
        place(&radial, [0.5, 0.5, 0.5, 0.5], [0.0, -40.0, 0.0, -40.0]);
        let mut hub = theme::framed(Frame::Banner);
        hub.set_mouse_filter(MouseFilter::IGNORE);
        let mut hub_label = theme::styled_label("Actions", Face::Title, 20, theme::GOLD_BRIGHT);
        hub_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        hub.add_child(&hub_label);
        hub.set_position(Vector2::new(-60.0, -20.0));
        hub.set_custom_minimum_size(Vector2::new(120.0, 40.0));
        radial.add_child(&hub);
        let mut entries = Vec::new();
        for (i, e) in RADIAL.iter().enumerate() {
            let a = (i as f32) * std::f32::consts::TAU / RADIAL.len() as f32;
            let c = Vector2::new(a.sin(), -a.cos()) * RADIUS;
            let mut p = theme::framed(Frame::Tooltip);
            p.set_mouse_filter(MouseFilter::IGNORE);
            p.set_custom_minimum_size(ENTRY);
            p.set_position(c - ENTRY * 0.5);
            let mut l = theme::styled_label(e.label(), Face::BodyBold, 17, theme::TEXT);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            p.add_child(&l);
            radial.add_child(&p);
            entries.push(p);
        }
        radial.set_visible(false);
        root.add_child(&radial);

        // the strip of hints, bottom right above the Pw orb's corner
        let mut strip = HBoxContainer::new_alloc();
        strip.set_mouse_filter(MouseFilter::IGNORE);
        strip.add_theme_constant_override("separation", 14);
        strip.set_alignment(godot::classes::box_container::AlignmentMode::END);
        place(&strip, [0.0, 1.0, 1.0, 1.0], [24.0, -232.0, -24.0, -200.0]);
        strip.set_visible(false);
        root.add_child(&strip);
        PadView {
            root,
            radial,
            entries,
            strip,
            shown: None,
            selected: None,
        }
    }

    pub fn radial_open(&mut self, on: bool) {
        self.radial.set_visible(on);
        if on {
            self.select(None);
        }
    }

    /// The highlighted entry.
    pub fn select(&mut self, sel: Option<usize>) {
        self.selected = sel;
        for (i, e) in self.entries.iter_mut().enumerate() {
            let on = Some(i) == sel;
            e.set_modulate(if on {
                Color::from_rgb(1.0, 0.92, 0.7)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.85)
            });
            e.set_scale(if on {
                Vector2::new(1.12, 1.12)
            } else {
                Vector2::ONE
            });
            e.set_pivot_offset(ENTRY * 0.5);
        }
    }

    /// The radial is up (self-tests).
    pub fn radial_shown(&self) -> bool {
        self.radial.is_visible()
    }

    /// The strip of hints for this controller and screen; hidden without a
    /// gamepad.
    pub fn show_hints(&mut self, pad: Option<(PadKind, PadCtx)>) {
        if self.shown == pad {
            return;
        }
        self.shown = pad;
        for mut c in self.strip.get_children().iter_shared() {
            c.queue_free();
        }
        let Some((kind, ctx)) = pad else {
            self.strip.set_visible(false);
            return;
        };
        for (buttons, what) in hints(ctx) {
            let mut item = HBoxContainer::new_alloc();
            item.set_mouse_filter(MouseFilter::IGNORE);
            item.add_theme_constant_override("separation", 4);
            for b in buttons {
                item.add_child(&medallion(kind, b));
            }
            let mut l: Gd<Label> = theme::styled_label(what, Face::Body, 16, theme::TEXT);
            theme::outline(&l, 5);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            item.add_child(&l);
            self.strip.add_child(&item);
        }
        self.strip.set_visible(true);
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_has_a_way_back() {
        for ctx in [
            PadCtx::Getpos,
            PadCtx::Direction,
            PadCtx::Menu { any: true },
            PadCtx::PanelBrowse,
            PadCtx::PanelSelect,
            PadCtx::Other,
        ] {
            assert!(
                hints(ctx).iter().any(|(b, _)| b.contains(&PadButton::B)),
                "{ctx:?}"
            );
        }
        assert!(hints(PadCtx::World).iter().any(|(_, w)| *w == "Inventory"));
    }
}
