//! The spells' panel's cards (`spells`): a card a spell, in the language
//! now: its own icon (its school's medallion when none is baked), its name,
//! school and level and what it asks once cast (a direction, a place), a
//! line on what it does in 5.0, its chance of failure coloured by it, its
//! cost in Pw (greyed when the hero is short of it) and how well it is
//! remembered. Beside the list, the full card of the spell under the
//! pointer or the focus: the line whole, what it asks, the numbers spelt
//! out, a forgotten spell's danger, and a button that puts it on the
//! action bar (a card dragged onto a slot puts it there).

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, InputEvent, InputEventMouse, InputEventMouseButton,
    InputEventMouseMotion, Label, MarginContainer, Panel, PanelContainer, StyleBoxFlat, Texture2D,
    TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;

use crate::i18n::{self, EngineKind};
use crate::icons::{self, Glyph};
use crate::pad_view;
use crate::spells::{self, Aim, Retention, School, Spell};
use crate::theme::{self, Face};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// A spell's card in the list, and the sort's row.
pub const CARD_H: f32 = 88.0;
pub const SORT_H: f32 = 40.0;
/// The list's width, the full card's, and the room between them.
pub const LIST_W: f32 = 700.0;
pub const DETAIL_W: f32 = 380.0;
pub const GAP: f32 = 18.0;
/// The icon on a card and on the full card; the failure's, the cost's and
/// the memory's columns.
const ICON: f32 = 64.0;
const BIG_ICON: f32 = 104.0;
const FAIL_W: f32 = 74.0;
const COST_W: f32 = 70.0;
const MEMORY_W: f32 = 100.0;
const BAR_H: f32 = 5.0;
/// Pixels the pointer moves with the button down before a card is dragged.
const DRAG_START: f32 = 10.0;

pub fn school_glyph(s: School) -> Glyph {
    match s {
        School::Attack => Glyph::Burst,
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

/// A school's colour in words on the panel: its plates' light.
pub fn school_color(s: School) -> Color {
    let [r, g, b] = crate::spell_bake::paint_of(s).light;
    Color::from_rgb(r, g, b)
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

/// The Fluent id of the line on what the spell `english` does.
pub fn description_id(english: &str) -> String {
    format!("spells-desc-{}", spells::slug(english).replace('_', "-"))
}

/// What the spell `english` does, in a line (None for a spell 5.0 has not).
pub fn description(english: &str) -> Option<String> {
    let id = description_id(english);
    i18n::has(&id).then(|| i18n::tr(&id))
}

/// What a spell asks once cast, in a word or two (none for one that asks
/// nothing).
pub fn aim_tag(aim: Aim) -> Option<String> {
    match aim {
        Aim::None => None,
        Aim::Direction => Some(tr!("spells-aim-direction")),
        Aim::Place => Some(tr!("spells-aim-place")),
        Aim::DirectionOrPlace => Some(tr!("spells-aim-direction")),
    }
}

/// The same in a sentence, for the full card.
pub fn aim_words(aim: Aim) -> String {
    match aim {
        Aim::None => tr!("spells-aim-none-long"),
        Aim::Direction => tr!("spells-aim-direction-long"),
        Aim::Place => tr!("spells-aim-place-long"),
        Aim::DirectionOrPlace => tr!("spells-aim-direction-or-place-long"),
    }
}

/// The prompt line while the spell `spell` (as the player reads it) asks
/// its direction `question`: the spell named, the question after it, and
/// how to aim: with a gamepad (`pad`: its A's and B's words) by its stick
/// and buttons, never a key. A cancelled direction still spends the Pw
/// (spell.c: "The magical energy is released!").
pub fn aim_prompt(spell: &str, question: &str, pad: Option<(&str, &str)>) -> String {
    let mut chars = question.trim().chars();
    let question: String = match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    };
    let how = match pad {
        Some((a, b)) => tr!("spells-aim-pad", a = a, b = b),
        None => tr!("spells-aim-keys"),
    };
    tr!(
        "spells-aim-line",
        spell = spell,
        question = question,
        how = how
    )
}

/// What a spell asks, by its English name (Aim::None for one not known).
pub fn aim_of(english: &str) -> Aim {
    spells::spell_known(english).map_or(Aim::None, |k| k.aim)
}

/// The hero's Pw `pw` pays for `s`: unknown Pw does.
pub fn can_pay(s: &Spell, pw: Option<i64>) -> bool {
    pw.is_none_or(|pw| pw >= spells::cost(s.level))
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

/// The colour of a chance of failure: green for a sure spell, amber for a
/// risk, red for a gamble.
pub fn fail_color(fail: u8) -> Color {
    match fail {
        0..=20 => theme::GOOD,
        21..=60 => theme::WARN,
        _ => theme::DANGER,
    }
}

/// The spell's own icon texture, else its school's medallion.
fn icon_texture(s: &Spell) -> Gd<Texture2D> {
    icons::spell_icon(&s.name).unwrap_or_else(|| icons::emblem(school_glyph(s.school)))
}

fn icon_rect(tex: &Gd<Texture2D>, side: f32) -> Gd<TextureRect> {
    let mut t = TextureRect::new_alloc();
    t.set_mouse_filter(MouseFilter::IGNORE);
    t.set_expand_mode(ExpandMode::IGNORE_SIZE);
    t.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    t.set_custom_minimum_size(Vector2::new(side, side));
    t.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    t.set_texture(tex);
    t
}

/// A thin bar `part` full (0..1) of `color` over a dark track.
fn bar(width: f32, part: f32, color: Color) -> Gd<Control> {
    let mut track = Control::new_alloc();
    track.set_mouse_filter(MouseFilter::IGNORE);
    track.set_custom_minimum_size(Vector2::new(width, BAR_H));
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
        [0.0, 0.0, part.clamp(0.0, 1.0), 1.0],
        [0.0, 0.0, 0.0, 0.0],
    );
    track.add_child(&fill);
    track
}

/// How well it is remembered: the most it may be, its colour, its words.
fn memory_of(r: Retention) -> (u8, Color, String, bool) {
    match r {
        Retention::Full => (100, theme::GOLD_BRIGHT, "100%".to_string(), true),
        Retention::Range(lo, hi) => (hi, theme::GOLD, format!("{lo}%–{hi}%"), true),
        Retention::Gone => (0, theme::DANGER, tr!("spells-gone"), false),
    }
}

/// How well it is remembered: a bar as full as the most it may be, the
/// range under it.
fn memory(r: Retention) -> Gd<VBoxContainer> {
    let mut col = column(Some(MEMORY_W));
    let (most, color, text, verbatim) = memory_of(r);
    col.add_child(&bar(MEMORY_W, f32::from(most) / 100.0, color));
    let mut l = label(&text, Face::Body, 15, theme::TEXT_DIM);
    l.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    if verbatim {
        i18n::verbatim(&l);
    }
    col.add_child(&l);
    let mut c = label(&tr!("spells-memory"), Face::Caps, 12, theme::TEXT_DIM);
    c.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    col.add_child(&c);
    col
}

/// The cost in Pw, greyed when the hero is short of it.
fn cost(s: &Spell, pw: Option<i64>) -> Gd<VBoxContainer> {
    let mut col = column(Some(COST_W));
    let paid = can_pay(s, pw);
    let color = if paid { theme::PW } else { theme::TEXT_OFF };
    let mut v = label(
        &spells::cost(s.level).to_string(),
        Face::BodyBold,
        21,
        color,
    );
    v.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    i18n::verbatim(&v);
    col.add_child(&v);
    let (caption, color) = if paid {
        (tr!("spells-cost"), theme::TEXT_DIM)
    } else {
        (tr!("spells-cost-short"), theme::DANGER)
    };
    let mut c = label(&caption, Face::Caps, 12, color);
    c.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    col.add_child(&c);
    col
}

/// The chance of failure: the number and a bar, coloured by it.
fn failure(s: &Spell) -> Gd<VBoxContainer> {
    let mut col = column(Some(FAIL_W));
    let mut v = label(
        &format!("{}%", s.fail),
        Face::BodyBold,
        21,
        fail_color(s.fail),
    );
    v.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    i18n::verbatim(&v);
    col.add_child(&v);
    let mut c = label(&tr!("spells-fail"), Face::Caps, 12, theme::TEXT_DIM);
    c.set_horizontal_alignment(HorizontalAlignment::RIGHT);
    col.add_child(&c);
    let part = (f32::from(s.fail) / 100.0).max(0.04);
    col.add_child(&bar(FAIL_W, part, fail_color(s.fail)));
    col
}

/// School and level, and what the spell asks once cast.
fn what_line(s: &Spell) -> String {
    let what = tr!(
        "spells-school-level",
        school = school_name(s.school),
        level = s.level
    );
    match aim_tag(aim_of(&s.name)) {
        Some(aim) => format!("{what} · {aim}"),
        None => what,
    }
}

/// A spell's card after its letter: the icon, the name over the school,
/// level and aim and the line on what it does, the failure, the cost, the
/// memory; and the name's label (the menu colours it).
pub fn cells(s: &Spell, pw: Option<i64>) -> (Vec<Gd<Control>>, Gd<Label>) {
    let mut icon = icon_rect(&icon_texture(s), ICON);
    if !can_pay(s, pw) {
        icon.set_modulate(Color::from_rgba(0.6, 0.6, 0.6, 0.55));
    }
    let mut out: Vec<Gd<Control>> = vec![icon.upcast()];
    let mut text = column(None);
    text.add_theme_constant_override("separation", 1);
    let mut n = label(&spell_name(&s.name), Face::BodyBold, 21, theme::TEXT);
    n.set_clip_text(true);
    text.add_child(&n);
    let name_label = n.clone();
    let mut what = label(&what_line(s), Face::Body, 15, school_color(s.school));
    what.set_clip_text(true);
    text.add_child(&what);
    if let Some(d) = description(&s.name) {
        let mut l = label(&d, Face::Body, 15, theme::TEXT_DIM);
        l.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        l.set_clip_text(true);
        text.add_child(&l);
    }
    out.push(text.upcast());
    out.push(failure(s).upcast());
    out.push(cost(s, pw).upcast());
    out.push(memory(s.retention).upcast());
    (out, name_label)
}

/// A card's frame behind its cells, inset so that the cards stand apart
/// and the row's highlight (the pointer's, the focus') shows through; a
/// stripe of its school's colour at its left.
pub fn card_back(button: &Gd<Button>, s: &Spell) {
    let mut back = Panel::new_alloc();
    back.set_mouse_filter(MouseFilter::IGNORE);
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.12, 0.09, 0.06, 0.42));
    sb.set_border_width_all(1);
    sb.set_border_color(Color {
        a: 0.45,
        ..theme::GOLD_DIM
    });
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    back.add_theme_stylebox_override("panel", &sb);
    theme::place(&back, [0.0, 0.0, 1.0, 1.0], [2.0, 3.0, -2.0, -3.0]);
    let mut stripe = ColorRect::new_alloc();
    stripe.set_mouse_filter(MouseFilter::IGNORE);
    stripe.set_color(Color {
        a: 0.8,
        ..school_color(s.school)
    });
    theme::place(&stripe, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 4.0, -1.0]);
    back.add_child(&stripe);
    let mut b = button.clone();
    b.add_child(&back);
    b.move_child(&back, 0);
}

/// The sort's cells after its letter, and its label.
pub fn sort_cells() -> (Vec<Gd<Control>>, Gd<Label>) {
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(ICON, 24.0));
    icon.set_texture(&icons::glyph_icon(Glyph::Grid));
    icon.set_modulate(theme::GOLD);
    let mut l = label(&tr!("spells-sort"), Face::Body, 17, theme::TEXT_DIM);
    l.set_h_size_flags(SizeFlags::EXPAND_FILL);
    (vec![icon.upcast(), l.clone().upcast()], l)
}

// ---- the full card ----

/// A fact's row on the full card: its name, its value.
fn fact(name: &str) -> (Gd<HBoxContainer>, Gd<Label>) {
    let mut row = HBoxContainer::new_alloc();
    row.set_mouse_filter(MouseFilter::IGNORE);
    row.add_theme_constant_override("separation", 10);
    let mut n = label(name, Face::Caps, 14, theme::TEXT_DIM);
    n.set_custom_minimum_size(Vector2::new(104.0, 0.0));
    n.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    row.add_child(&n);
    let mut v = label("", Face::Body, 17, theme::TEXT);
    v.set_h_size_flags(SizeFlags::EXPAND_FILL);
    v.set_autowrap_mode(AutowrapMode::WORD_SMART);
    row.add_child(&v);
    (row, v)
}

/// The full card of the spell under the pointer or the focus.
pub struct Detail {
    pub root: Gd<PanelContainer>,
    icon: Gd<TextureRect>,
    name: Gd<Label>,
    what: Gd<Label>,
    desc: Gd<Label>,
    aim: Gd<Label>,
    fail: Gd<Label>,
    cost: Gd<Label>,
    memory: Gd<Label>,
    warn: Gd<Label>,
    button: Gd<Button>,
    /// The spell shown (its English name), for the button.
    shown: Rc<RefCell<Option<String>>>,
}

impl Detail {
    /// The card, `width` wide; its button puts the spell shown on the bar
    /// (its first free slot). `pad`: the button is no pad player's.
    pub fn new(width: f32, height: f32, queue: &UiQueue, pad: bool) -> Detail {
        let mut root = PanelContainer::new_alloc();
        root.set_mouse_filter(MouseFilter::IGNORE);
        root.set_custom_minimum_size(Vector2::new(width, height));
        let mut sb = theme::socket_style();
        sb.set_content_margin_all(16.0);
        root.add_theme_stylebox_override("panel", &sb);
        let mut col = VBoxContainer::new_alloc();
        col.set_mouse_filter(MouseFilter::IGNORE);
        col.add_theme_constant_override("separation", 10);
        root.add_child(&col);
        // the icon beside the name and the school
        let mut head = HBoxContainer::new_alloc();
        head.set_mouse_filter(MouseFilter::IGNORE);
        head.add_theme_constant_override("separation", 14);
        let icon = icon_rect(&icons::emblem(Glyph::Book), BIG_ICON);
        head.add_child(&icon);
        let mut titles = column(None);
        titles.add_theme_constant_override("separation", 4);
        let mut name = label("", Face::Title, 27, theme::GOLD_BRIGHT);
        name.set_autowrap_mode(AutowrapMode::WORD_SMART);
        titles.add_child(&name);
        let mut what = label("", Face::Body, 16, theme::TEXT_DIM);
        what.set_autowrap_mode(AutowrapMode::WORD_SMART);
        titles.add_child(&what);
        head.add_child(&titles);
        col.add_child(&head);
        let mut desc = label("", Face::Body, 18, theme::TEXT);
        desc.set_autowrap_mode(AutowrapMode::WORD_SMART);
        desc.set_vertical_alignment(VerticalAlignment::TOP);
        col.add_child(&desc);
        let (row, aim) = fact(&tr!("spells-detail-aim"));
        col.add_child(&row);
        let (row, fail) = fact(&tr!("spells-detail-fail"));
        col.add_child(&row);
        let (row, cost) = fact(&tr!("spells-detail-cost"));
        col.add_child(&row);
        let (row, memory) = fact(&tr!("spells-detail-memory"));
        col.add_child(&row);
        let mut warn = label("", Face::Body, 16, theme::DANGER);
        warn.set_autowrap_mode(AutowrapMode::WORD_SMART);
        warn.set_visible(false);
        col.add_child(&warn);
        let mut fill = Control::new_alloc();
        fill.set_mouse_filter(MouseFilter::IGNORE);
        fill.set_v_size_flags(SizeFlags::EXPAND_FILL);
        col.add_child(&fill);
        let shown: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let mut button = Button::new_alloc();
        button.set_text(&tr!("spells-to-bar"));
        button.set_custom_minimum_size(Vector2::new(0.0, 36.0));
        crate::tips::set(&button, &tr!("spells-to-bar-tip"));
        let (q, s) = (queue.clone(), shown.clone());
        button.signals().pressed().connect(move || {
            if let Some(name) = s.borrow().clone() {
                push(&q, UiEvent::BindSpell { name, slot: None });
            }
        });
        // a mouse's button: a pad player puts a spell on the bar otherwise
        pad_view::key_words(&button, pad);
        let mut foot = MarginContainer::new_alloc();
        foot.set_mouse_filter(MouseFilter::IGNORE);
        foot.add_child(&button);
        col.add_child(&foot);
        Detail {
            root,
            icon,
            name,
            what,
            desc,
            aim,
            fail,
            cost,
            memory,
            warn,
            button,
            shown,
        }
    }

    /// The spell shown (self-tests).
    pub fn shown(&self) -> Option<String> {
        self.shown.borrow().clone()
    }

    /// Press its button, as a click does (self-tests).
    pub fn press_to_bar(&mut self) {
        self.button.emit_signal("pressed", &[]);
    }

    /// Show `s`'s card, the hero's Pw `pw`.
    pub fn show(&mut self, s: &Spell, pw: Option<i64>) {
        if self.shown.borrow().as_deref() == Some(s.name.as_str()) {
            return;
        }
        *self.shown.borrow_mut() = Some(s.name.clone());
        self.icon.set_texture(&icon_texture(s));
        self.name.set_text(&spell_name(&s.name));
        let what = tr!(
            "spells-school-level",
            school = school_name(s.school),
            level = s.level
        );
        self.what.set_text(&what);
        self.what
            .add_theme_color_override("font_color", school_color(s.school));
        self.desc
            .set_text(&description(&s.name).unwrap_or_default());
        self.aim.set_text(&aim_words(aim_of(&s.name)));
        self.fail.set_text(&format!("{}%", s.fail));
        self.fail
            .add_theme_color_override("font_color", fail_color(s.fail));
        let price = spells::cost(s.level);
        let (cost, color) = match pw {
            Some(have) if have < price => (
                tr!("spells-detail-cost-short", cost = price, have = have),
                theme::DANGER,
            ),
            Some(have) => (
                tr!("spells-detail-cost-have", cost = price, have = have),
                theme::PW,
            ),
            None => (price.to_string(), theme::PW),
        };
        self.cost.set_text(&cost);
        self.cost.add_theme_color_override("font_color", color);
        let (_, color, text, _) = memory_of(s.retention);
        self.memory.set_text(&match s.turns {
            Some(t) => tr!("spells-detail-memory-turns", memory = text, turns = t),
            None => text,
        });
        self.memory.add_theme_color_override("font_color", color);
        let gone = s.retention == Retention::Gone;
        self.warn.set_text(&tr!("spells-gone-warn"));
        self.warn.set_visible(gone);
    }
}

// ---- a card dragged onto the bar ----

/// A press on a card that may become a drag: where it began, whether it
/// is a drag now, its icon following the pointer.
#[derive(Default)]
struct Drag {
    from: Option<Vector2>,
    preview: Option<Gd<TextureRect>>,
}

/// Let the card `button` of the spell `s` be dragged onto the action bar
/// (its slots' rects on screen in `slots`): dropped on a slot, the spell
/// is bound to it. The icon follows the pointer, drawn over everything
/// (and gone with the card if the panel closes mid-drag); the card does
/// not cast when a drag ends on it.
pub fn draggable(button: &Gd<Button>, s: &Spell, slots: Rc<RefCell<Vec<Rect2>>>, queue: UiQueue) {
    let drag = Rc::new(RefCell::new(Drag::default()));
    let tex = icon_texture(s);
    let name = s.name.clone();
    let mut card = button.clone();
    button
        .signals()
        .gui_input()
        .connect(move |ev: Gd<InputEvent>| {
            // where on the canvas (the event's own: a self-test's too)
            let Ok(mouse) = ev.clone().try_cast::<InputEventMouse>() else {
                return;
            };
            let at = mouse.get_global_position();
            if let Ok(b) = ev.clone().try_cast::<InputEventMouseButton>() {
                if b.get_button_index() != MouseButton::LEFT {
                    return;
                }
                let mut d = drag.borrow_mut();
                if b.is_pressed() {
                    d.from = Some(at);
                    return;
                }
                d.from = None;
                if let Some(mut p) = d.preview.take() {
                    p.queue_free();
                    card.set_disabled(false);
                    let slot = slots.borrow().iter().position(|r| r.contains_point(at));
                    if let Some(slot) = slot {
                        push(
                            &queue,
                            UiEvent::BindSpell {
                                name: name.clone(),
                                slot: Some(slot),
                            },
                        );
                    }
                }
                return;
            }
            if ev.clone().try_cast::<InputEventMouseMotion>().is_err() {
                return;
            }
            let mut d = drag.borrow_mut();
            let Some(from) = d.from else {
                return;
            };
            if d.preview.is_none() && (at - from).length() > DRAG_START {
                // the press is no click now: the card does not cast
                card.set_disabled(true);
                let mut p = icon_rect(&tex, 56.0);
                p.set_size(Vector2::new(56.0, 56.0));
                p.set_as_top_level(true);
                p.set_z_index(100);
                card.add_child(&p);
                d.preview = Some(p);
            }
            if let Some(p) = d.preview.as_mut() {
                p.set_global_position(at - Vector2::new(28.0, 28.0));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    #[test]
    fn every_spell_says_what_it_does_and_what_it_asks_in_both_languages() {
        for k in &spells::KNOWN {
            let id = description_id(k.name);
            let en = i18n::tr_in(Lang::En, &id).unwrap_or_else(|| panic!("no {id} in English"));
            let ru = i18n::tr_in(Lang::Ru, &id).unwrap_or_else(|| panic!("no {id} in Russian"));
            assert!(en.len() > 10 && ru.len() > 10, "{id}");
            assert_ne!(en, ru, "{id} is not in Russian");
            assert!(ru.chars().any(|c| ('а'..='я').contains(&c)), "{id}: {ru}");
            // a spell that asks a direction or a place says so on its card
            assert_eq!(aim_tag(k.aim).is_some(), k.aim != Aim::None, "{}", k.name);
        }
        // the deferred spheres say they are not in the game
        let sphere = i18n::tr_in(Lang::En, &description_id("flame sphere")).unwrap();
        assert!(sphere.contains("5.0"));
    }

    #[test]
    fn the_aim_names_the_spell_and_a_pad_s_buttons_not_keys() {
        i18n::set_lang(Lang::Ru);
        let keys = aim_prompt("Силовой удар", "В каком направлении?", None);
        assert!(
            keys.starts_with("Силовой удар: в каком направлении?"),
            "{keys}"
        );
        assert!(keys.contains("Esc"), "{keys}");
        let pad = aim_prompt("Силовой удар", "В каком направлении?", Some(("A", "B")));
        assert!(
            pad.contains("A") && !pad.contains("Esc") && !pad.contains("стрелк"),
            "{pad}"
        );
        i18n::set_lang(Lang::En);
        let en = aim_prompt("Force bolt", "In what direction?", Some(("A", "B")));
        assert!(en.starts_with("Force bolt: in what direction?"), "{en}");
    }

    #[test]
    fn a_card_greys_what_the_hero_cannot_pay() {
        let spell = |level| Spell {
            idx: 1,
            letter: 'a',
            name: "force bolt".into(),
            level,
            school: School::Attack,
            fail: 0,
            retention: Retention::Full,
            turns: None,
        };
        assert!(can_pay(&spell(1), Some(5)));
        assert!(!can_pay(&spell(1), Some(4)));
        assert!(!can_pay(&spell(7), Some(34)));
        assert!(can_pay(&spell(7), None), "Pw not known yet: nothing greyed");
        assert_eq!(fail_color(0), theme::GOOD);
        assert_eq!(fail_color(40), theme::WARN);
        assert_eq!(fail_color(90), theme::DANGER);
    }
}
