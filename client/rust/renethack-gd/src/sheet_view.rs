//! The character sheet's panel (`sheet`): the hero's portrait beside who
//! they are, their hit points, energy, armour class, experience and gold,
//! and the six characteristics; under them the window's other lines by
//! section, in the engine's words as the language says them.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    AtlasTexture, ColorRect, Control, HBoxContainer, HFlowContainer, Label, PanelContainer,
    StyleBoxFlat, Texture2D, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::i18n;
use crate::icons;
use crate::sheet::{Attr, Part, Sheet, Stat};
use crate::theme::{self, Face, Frame};
use crate::tr;

/// The portrait's frame, and the part of the hero's render it shows (the
/// doll's 480×880 render: a tall hero from the helmet's crest down to the
/// hips, a short one whole).
const PORTRAIT: Vector2 = Vector2::new(204.0, 268.0);
const PORTRAIT_REGION: Rect2 = Rect2::new(Vector2::new(90.0, 120.0), Vector2::new(300.0, 394.0));
/// Between the portrait and the column beside it, and between columns.
const GAP: f32 = 24.0;
/// A characteristic's card.
const CARD_H: f32 = 84.0;

/// The panel's content, and the engraved figure under the portrait that
/// the hero's render covers once it shows.
pub struct SheetView {
    pub root: Gd<Control>,
    figure: Gd<TextureRect>,
    hero: Option<Gd<TextureRect>>,
}

impl SheetView {
    /// Whether the portrait shows the hero's render (self-tests).
    pub fn rendered(&self) -> bool {
        self.hero.as_ref().is_some_and(|h| h.is_visible())
    }

    /// The hero's render shows (the inventory's doll draws them now): the
    /// engraved figure goes.
    pub fn set_rendered(&mut self, rendered: bool) {
        let Some(hero) = self.hero.as_mut() else {
            return;
        };
        if hero.is_visible() != rendered {
            hero.set_visible(rendered);
            self.figure.set_visible(!rendered);
        }
    }
}

fn label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    l
}

/// A label that wraps at its column's width.
fn wrapped(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = label(text, face, size, color);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_h_size_flags(SizeFlags::EXPAND_FILL);
    l
}

fn rule() -> Gd<ColorRect> {
    let mut r = ColorRect::new_alloc();
    r.set_color(Color {
        a: 0.55,
        ..theme::GOLD_DIM
    });
    r.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    r.set_mouse_filter(MouseFilter::IGNORE);
    r
}

fn vbox(gap: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", gap);
    b
}

fn hbox(gap: i32) -> Gd<HBoxContainer> {
    let mut b = HBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", gap);
    b
}

/// A dark well with a thin iron line.
fn well() -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.05, 0.04, 0.03, 0.92));
    sb.set_border_width_all(1);
    sb.set_border_color(theme::IRON);
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    sb.set_content_margin_all(8.0);
    sb
}

/// The portrait: the hero rendered in their gear over an engraved figure
/// (`render`: the inventory's doll), the level on a plaque under it.
fn portrait(
    render: Option<&Gd<Texture2D>>,
    level: Option<i32>,
) -> (Gd<Control>, Gd<TextureRect>, Option<Gd<TextureRect>>) {
    let mut col = vbox(0);
    let mut frame = theme::framed(Frame::Socket);
    frame.set_mouse_filter(MouseFilter::IGNORE);
    frame.set_custom_minimum_size(PORTRAIT);
    let mut stack = Control::new_alloc();
    stack.set_mouse_filter(MouseFilter::IGNORE);
    stack.set_clip_contents(true);
    let mut figure = TextureRect::new_alloc();
    figure.set_mouse_filter(MouseFilter::IGNORE);
    figure.set_expand_mode(ExpandMode::IGNORE_SIZE);
    figure.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    figure.set_texture(&icons::figure());
    figure.set_modulate(Color::from_rgba(0.72, 0.54, 0.23, 0.5));
    theme::full_rect_ignore(&figure);
    stack.add_child(&figure);
    let hero = render.map(|tex| {
        let mut part = AtlasTexture::new_gd();
        part.set_atlas(tex);
        part.set_region(PORTRAIT_REGION);
        let mut r = TextureRect::new_alloc();
        r.set_mouse_filter(MouseFilter::IGNORE);
        r.set_expand_mode(ExpandMode::IGNORE_SIZE);
        r.set_stretch_mode(StretchMode::KEEP_ASPECT_COVERED);
        r.set_texture(&part);
        r.set_visible(false);
        theme::full_rect_ignore(&r);
        stack.add_child(&r);
        r
    });
    frame.add_child(&stack);
    col.add_child(&frame);
    if let Some(level) = level {
        let mut plaque = theme::framed(Frame::Banner);
        plaque.set_mouse_filter(MouseFilter::IGNORE);
        plaque.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let mut l = label(
            &tr!("hud-level", level = level),
            Face::Caps,
            16,
            theme::GOLD_BRIGHT,
        );
        l.set_horizontal_alignment(HorizontalAlignment::CENTER);
        plaque.add_child(&l);
        // over the frame's lower edge
        col.add_theme_constant_override("separation", -14);
        col.add_child(&plaque);
    }
    (col.upcast(), figure, hero)
}

/// A value under its caption ("Hit points", "12 / 16"), and a note under it.
fn chip(caption: &str, value: &str, color: Color, note: Option<&str>) -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.add_theme_stylebox_override("panel", &well());
    p.set_custom_minimum_size(Vector2::new(112.0, 0.0));
    let mut col = vbox(0);
    col.add_child(&label(caption, Face::Caps, 14, theme::TEXT_DIM));
    let mut v = label(value, Face::BodyBold, 22, color);
    i18n::verbatim(&v);
    v.set_custom_minimum_size(Vector2::new(0.0, 28.0));
    col.add_child(&v);
    if let Some(n) = note {
        col.add_child(&label(n, Face::Body, 14, theme::TEXT_DIM));
    }
    p.add_child(&col);
    p
}

fn attr_name(a: Attr) -> String {
    match a {
        Attr::Str => tr!("sheet-strength"),
        Attr::Dex => tr!("sheet-dexterity"),
        Attr::Con => tr!("sheet-constitution"),
        Attr::Int => tr!("sheet-intelligence"),
        Attr::Wis => tr!("sheet-wisdom"),
        Attr::Cha => tr!("sheet-charisma"),
    }
}

/// What a characteristic's value is besides ("base 11 · limit 18").
fn stat_notes(s: &Stat) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(b) = &s.base {
        notes.push(tr!("sheet-base", value = b.as_str()));
    }
    if let Some(p) = &s.peak {
        notes.push(tr!("sheet-peak", value = p.as_str()));
    }
    if let Some(l) = &s.limit {
        notes.push(if s.innate {
            tr!("sheet-innate-limit", value = l.as_str())
        } else {
            tr!("sheet-limit", value = l.as_str())
        });
    }
    notes
}

/// A characteristic's card: its name, its value, what else it is.
fn card(s: &Stat, width: f32) -> Gd<PanelContainer> {
    let mut p = theme::framed(Frame::Socket);
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.set_custom_minimum_size(Vector2::new(width, CARD_H));
    p.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut col = vbox(0);
    let mut name = label(&attr_name(s.attr), Face::Caps, 14, theme::TEXT_DIM);
    name.set_horizontal_alignment(HorizontalAlignment::CENTER);
    name.set_clip_text(true);
    col.add_child(&name);
    let mut v = label(&s.value, Face::Title, 32, theme::GOLD_BRIGHT);
    v.set_horizontal_alignment(HorizontalAlignment::CENTER);
    i18n::verbatim(&v);
    col.add_child(&v);
    let notes = stat_notes(s);
    if !notes.is_empty() {
        let mut n = label(&notes.join("\n"), Face::Body, 13, theme::TEXT_DIM);
        n.set_horizontal_alignment(HorizontalAlignment::CENTER);
        col.add_child(&n);
    }
    p.add_child(&col);
    p
}

/// A section's name over its lines.
fn heading(part: Part, english: &str, say: &dyn Fn(&str) -> String) -> String {
    match part {
        Part::Background => tr!("sheet-background"),
        Part::Basics => tr!("sheet-basics"),
        Part::Characteristics => tr!("sheet-characteristics"),
        Part::Status => tr!("sheet-status"),
        Part::Attributes => tr!("sheet-attributes"),
        Part::Miscellaneous => tr!("sheet-miscellaneous"),
        Part::Other => {
            let h = say(english);
            h.trim().trim_end_matches(':').to_string()
        }
    }
}

/// A section: its name, then each line after a mark.
fn section(name: &str, lines: &[String]) -> Gd<VBoxContainer> {
    let mut col = vbox(4);
    let mut h = label(name, Face::Caps, 16, theme::GOLD);
    h.set_custom_minimum_size(Vector2::new(0.0, 24.0));
    col.add_child(&h);
    col.add_child(&rule());
    for line in lines {
        let mut row = hbox(8);
        let mut mark = label("◆", Face::Body, 9, theme::GOLD_DIM);
        mark.set_vertical_alignment(VerticalAlignment::TOP);
        mark.set_custom_minimum_size(Vector2::new(10.0, 22.0));
        i18n::verbatim(&mark);
        row.add_child(&mark);
        row.add_child(&wrapped(line, Face::Body, 16, theme::TEXT));
        col.add_child(&row);
    }
    col
}

/// The sheet at `width`; `say` gives a line of the window as the player
/// reads it; `render` is the hero's render for the portrait.
pub fn view(
    s: &Sheet,
    say: &dyn Fn(&str) -> String,
    width: f32,
    render: Option<&Gd<Texture2D>>,
) -> SheetView {
    let mut root = vbox(18);
    root.set_custom_minimum_size(Vector2::new(width, 0.0));

    // ---- the portrait, and who the hero is beside it ----
    let mut top = hbox(GAP as i32);
    let (face, figure, hero) = portrait(render, s.level);
    top.add_child(&face);
    let side_w = width - PORTRAIT.x - GAP;
    let mut side = vbox(8);
    side.set_custom_minimum_size(Vector2::new(side_w, 0.0));
    side.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut name = label(&s.name, Face::Title, 34, theme::GOLD_BRIGHT);
    name.set_clip_text(true);
    i18n::verbatim(&name);
    side.add_child(&name);
    for (i, line) in s.identity.iter().enumerate() {
        let face = if i == 0 { Face::BodyBold } else { Face::Body };
        side.add_child(&wrapped(&say(line), face, 17, theme::TEXT));
    }
    side.add_child(&rule());
    // the numbers
    let mut chips = HFlowContainer::new_alloc();
    chips.set_mouse_filter(MouseFilter::IGNORE);
    chips.add_theme_constant_override("h_separation", 8);
    chips.add_theme_constant_override("v_separation", 8);
    let pair = |p: crate::sheet::Points| format!("{} / {}", p.now, p.most);
    if let Some(hp) = s.hit_points {
        chips.add_child(&chip(
            &tr!("sheet-hit-points"),
            &pair(hp),
            theme::HP.lightened(0.2),
            None,
        ));
    }
    if let Some(pw) = s.energy {
        chips.add_child(&chip(
            &tr!("sheet-energy"),
            &pair(pw),
            theme::PW.lightened(0.25),
            None,
        ));
    }
    if let Some(ac) = s.armor_class {
        chips.add_child(&chip(
            &tr!("sheet-armor-class"),
            &ac.to_string(),
            theme::GOLD_BRIGHT,
            None,
        ));
    }
    if let Some(xp) = s.experience {
        let note = xp
            .needed
            .map(|(n, level)| tr!("sheet-to-level", points = n, level = level));
        chips.add_child(&chip(
            &tr!("sheet-experience"),
            &xp.points.to_string(),
            theme::XP,
            note.as_deref(),
        ));
    }
    if let Some(gold) = s.gold {
        chips.add_child(&chip(
            &tr!("sheet-gold"),
            &gold.to_string(),
            theme::GOLD_BRIGHT,
            None,
        ));
    }
    side.add_child(&chips);
    // the six
    if !s.stats.is_empty() {
        let mut cards = hbox(8);
        let w = ((side_w - 8.0 * 5.0) / 6.0).floor();
        for st in &s.stats {
            cards.add_child(&card(st, w));
        }
        side.add_child(&cards);
    }
    top.add_child(&side);
    root.add_child(&top);

    // ---- the other lines, by section: two columns ----
    let mut cols = hbox(GAP as i32);
    let col_w = ((width - GAP) / 2.0).floor();
    let mut left = vbox(16);
    let mut right = vbox(16);
    for c in [&mut left, &mut right] {
        c.set_custom_minimum_size(Vector2::new(col_w, 0.0));
        c.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    for sec in s.sections.iter().filter(|sec| !sec.lines.is_empty()) {
        let lines: Vec<String> = sec.lines.iter().map(|l| say(l)).collect();
        let name = heading(sec.part, &sec.heading, say);
        let col = match sec.part {
            Part::Status | Part::Attributes => &mut right,
            _ => &mut left,
        };
        col.add_child(&section(&name, &lines));
    }
    cols.add_child(&left);
    cols.add_child(&right);
    root.add_child(&cols);
    SheetView {
        root: root.upcast(),
        figure,
        hero,
    }
}
