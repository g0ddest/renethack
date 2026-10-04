//! What a gamepad shows: the radial menu (hold LT) and a strip of the
//! buttons' meanings by what is on screen. Buttons are drawn as themed
//! medallions with the controller's letters or shapes (no logos).

use godot::builtin::Side;
use godot::classes::control::GrowDirection;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    CanvasLayer, ColorRect, Control, HBoxContainer, HFlowContainer, Label, PanelContainer,
    ShaderMaterial, StyleBoxFlat, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::gamepad::{PadButton, PadCtx, PadKind, RADIAL, RadialEntry};
use crate::i18n;
use crate::icons::{self, Glyph};
use crate::theme::{self, Face, place};
use crate::tr;

/// The radial ring's size, its radii (share of the half size, as the
/// shader has them) and a sector's icon and caption box, in design pixels.
const RING: f32 = 460.0;
const INNER: f32 = 0.44;
const OUTER: f32 = 0.97;
const CELL: Vector2 = Vector2::new(110.0, 70.0);

/// The icon of a radial entry.
fn entry_glyph(e: RadialEntry) -> Glyph {
    match e {
        RadialEntry::Here => Glyph::Hand,
        RadialEntry::PickUp => Glyph::Sack,
        RadialEntry::Fight => Glyph::Sword,
        RadialEntry::Kick => Glyph::Boots,
        RadialEntry::Rest => Glyph::Moon,
        RadialEntry::Pray => Glyph::Ankh,
        RadialEntry::Travel => Glyph::Stairs,
        RadialEntry::Save => Glyph::Scroll,
    }
}

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
    // the name printed on the controller
    i18n::verbatim(&l);
    p.add_child(&l);
    p
}

/// The buttons that mean something now, and what.
pub fn hints(ctx: PadCtx) -> Vec<(Vec<PadButton>, &'static str)> {
    use PadButton::*;
    match ctx {
        PadCtx::World => vec![
            (vec![A], "hint-act"),
            (vec![X], "hint-search"),
            (vec![Y], "hint-inventory"),
            (vec![Lt], "hint-actions"),
            (vec![Rt], "hint-fire"),
            (vec![Lb, Rb], "hint-bar"),
            (vec![Start], "hint-commands"),
        ],
        PadCtx::Getpos => vec![(vec![A], "hint-pick"), (vec![B], "hint-cancel")],
        PadCtx::Direction => vec![(vec![A], "hint-here"), (vec![B], "hint-cancel")],
        PadCtx::Menu { any: true } => vec![
            (vec![A], "hint-toggle"),
            (vec![Start], "hint-confirm"),
            (vec![B], "hint-cancel"),
            (vec![Lb, Rb], "hint-page"),
        ],
        PadCtx::PanelBrowse => vec![
            (vec![A], "hint-use"),
            (vec![X], "hint-actions"),
            (vec![Y], "hint-carry"),
            (vec![Lb, Rb], "hint-filter"),
            (vec![B], "hint-close"),
        ],
        PadCtx::PanelSelect => vec![
            (vec![A], "hint-choose"),
            (vec![Lb, Rb], "hint-filter"),
            (vec![B], "hint-cancel"),
        ],
        PadCtx::PanelMenu => vec![
            (vec![A], "hint-toggle"),
            (vec![Start], "hint-confirm"),
            (vec![Lb, Rb], "hint-filter"),
            (vec![B], "hint-cancel"),
        ],
        PadCtx::Text => vec![
            (vec![A], "hint-type"),
            (vec![B], "hint-erase"),
            (vec![Y], "hint-layout"),
            (vec![Start], "hint-ok"),
        ],
        PadCtx::Picker { wish: true } => vec![
            (vec![A], "hint-wish"),
            (vec![Left, Right], "hint-count"),
            (vec![X], "hint-blessing"),
            (vec![Y], "hint-enchantment"),
            (vec![Lb, Rb], "hint-class"),
            (vec![Start], "hint-english"),
            (vec![B], "hint-cancel"),
        ],
        PadCtx::Picker { wish: false } => vec![
            (vec![A], "hint-choose"),
            (vec![Start], "hint-english"),
            (vec![B], "hint-cancel"),
        ],
        _ => vec![(vec![A], "hint-ok"), (vec![B], "hint-back")],
    }
}

pub struct PadView {
    root: Gd<Control>,
    radial: Gd<Control>,
    entries: Vec<Gd<VBoxContainer>>,
    material: Option<Gd<ShaderMaterial>>,
    title: Gd<Label>,
    hint: Gd<Label>,
    strip: Gd<HFlowContainer>,
    /// Where the strip is docked: the panel it sits in the bottom of, or
    /// over the world right of the log (its rect).
    strip_in: Option<(Option<Rect2>, Rect2)>,
    shown: Option<(PadKind, PadCtx)>,
    selected: Option<usize>,
}

impl PadView {
    pub fn new(mut layer: Gd<CanvasLayer>) -> PadView {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);

        // the radial: a ring of 8 sectors centred on the screen (above the
        // bottom cluster at every size), an icon and a caption in each, the
        // chosen action's name in the middle
        let mut radial = Control::new_alloc();
        radial.set_mouse_filter(MouseFilter::IGNORE);
        place(&radial, [0.5, 0.5, 0.5, 0.5], [0.0, -60.0, 0.0, -60.0]);
        let mut ring = ColorRect::new_alloc();
        ring.set_mouse_filter(MouseFilter::IGNORE);
        ring.set_size(Vector2::new(RING, RING));
        ring.set_position(Vector2::new(-RING / 2.0, -RING / 2.0));
        let material = theme::ui_material("ui_radial");
        match &material {
            Some(m) => ring.set_material(m),
            None => ring.set_color(Color::from_rgba(0.07, 0.055, 0.04, 0.8)),
        }
        radial.add_child(&ring);
        let mut entries = Vec::new();
        let mid = RING / 2.0 * (INNER + OUTER) / 2.0;
        for (i, e) in RADIAL.iter().enumerate() {
            let a = (i as f32) * std::f32::consts::TAU / RADIAL.len() as f32;
            let c = Vector2::new(a.sin(), -a.cos()) * mid;
            let mut col = VBoxContainer::new_alloc();
            col.set_mouse_filter(MouseFilter::IGNORE);
            col.add_theme_constant_override("separation", 2);
            col.set_size(CELL);
            col.set_position(c - CELL * 0.5);
            let mut icon = TextureRect::new_alloc();
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_custom_minimum_size(Vector2::new(44.0, 44.0));
            icon.set_h_size_flags(SizeFlags::SHRINK_CENTER);
            icon.set_texture(&icons::emblem(entry_glyph(*e)));
            col.add_child(&icon);
            let mut l = theme::styled_label("", Face::BodyBold, 14, theme::TEXT);
            i18n::text(&l, e.label_key());
            theme::outline(&l, 4);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            col.add_child(&l);
            radial.add_child(&col);
            entries.push(col);
        }
        let mut centre = VBoxContainer::new_alloc();
        centre.set_mouse_filter(MouseFilter::IGNORE);
        centre.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        let inner_w = RING * INNER * 0.85;
        centre.set_size(Vector2::new(inner_w, inner_w));
        centre.set_position(Vector2::new(-inner_w / 2.0, -inner_w / 2.0));
        let mut title = theme::styled_label("", Face::Title, 22, theme::GOLD_BRIGHT);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        centre.add_child(&title);
        let mut hint = theme::styled_label("", Face::Body, 13, theme::TEXT_DIM);
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        hint.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        centre.add_child(&hint);
        radial.add_child(&centre);
        radial.set_visible(false);
        root.add_child(&radial);

        // the strip of hints, bottom right above the Pw orb's corner; a
        // second row above when the words are too long for one
        let mut strip = HFlowContainer::new_alloc();
        strip.set_mouse_filter(MouseFilter::IGNORE);
        strip.add_theme_constant_override("h_separation", 14);
        strip.add_theme_constant_override("v_separation", 6);
        strip.set_alignment(godot::classes::flow_container::AlignmentMode::END);
        strip.set_v_grow_direction(GrowDirection::BEGIN);
        place(&strip, [0.0, 1.0, 1.0, 1.0], [24.0, -232.0, -24.0, -200.0]);
        strip.set_visible(false);
        root.add_child(&strip);
        PadView {
            root,
            radial,
            entries,
            material,
            title,
            hint,
            strip,
            strip_in: None,
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
        if let Some(m) = self.material.as_mut() {
            let i = sel.map_or(-1, |i| i as i32);
            m.set_shader_parameter("selected", &i.to_variant());
        }
        for (i, e) in self.entries.iter_mut().enumerate() {
            let on = Some(i) == sel;
            e.set_modulate(if on {
                Color::from_rgb(1.0, 0.95, 0.8)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.72)
            });
            e.set_scale(if on {
                Vector2::new(1.1, 1.1)
            } else {
                Vector2::ONE
            });
            e.set_pivot_offset(CELL * 0.5);
        }
        match sel.and_then(|i| RADIAL.get(i)) {
            Some(e) => {
                self.title.set_text(&i18n::tr(e.label_key()));
                self.hint.set_text(&tr!("radial-let-go"));
            }
            None => {
                self.title.set_text(&tr!("radial-title"));
                self.hint.set_text(&tr!("radial-hint"));
            }
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
            let mut l: Gd<Label> =
                theme::styled_label(&i18n::tr(what), Face::Body, 16, theme::TEXT);
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

    /// The hints again in the language now (the radial's captions are
    /// bound).
    pub fn relang(&mut self) {
        let shown = self.shown.take();
        self.show_hints(shown);
    }

    /// The strip goes in the bottom right of a panel open on screen (the
    /// inventory), inside its frame, rather than across its edge; None:
    /// back over the world, above the Pw orb, right of the log (`log`).
    pub fn dock_hints(&mut self, panel: Option<Rect2>, log: Rect2) {
        if self.strip_in == Some((panel, log)) {
            return;
        }
        self.strip_in = Some((panel, log));
        match panel {
            Some(r) => {
                let (end, bottom) = (r.end().x - 28.0, r.end().y - 18.0);
                place(
                    &self.strip,
                    [0.0, 0.0, 0.0, 0.0],
                    [r.position.x + 28.0, bottom - 32.0, end, bottom],
                );
            }
            None => place(
                &self.strip,
                [0.0, 1.0, 1.0, 1.0],
                [log.end().x.max(0.0) + 24.0, -232.0, -24.0, -200.0],
            ),
        }
    }

    /// How high the strip is (0: hidden): the room a dialog keeps for it.
    pub fn strip_height(&self) -> f32 {
        if self.strip.is_visible() {
            self.strip.get_size().y
        } else {
            0.0
        }
    }

    /// Where the strip's hints are on screen (None: no strip), and the
    /// panel it is docked in (self-tests: it fits the panel, or the
    /// screen beside the HUD's blocks).
    pub fn strip_rect(&self) -> Option<(Rect2, Option<Rect2>)> {
        if !self.strip.is_visible_in_tree() {
            return None;
        }
        let rect = self
            .strip
            .get_children()
            .iter_shared()
            .filter_map(|k| k.try_cast::<Control>().ok())
            .filter(|k| k.is_visible())
            .map(|k| k.get_global_rect())
            .reduce(|a, b| a.merge(b))?;
        Some((rect, self.strip_in.and_then(|(panel, _)| panel)))
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
        assert!(
            hints(PadCtx::World)
                .iter()
                .any(|(_, w)| *w == "hint-inventory")
        );
    }
}
