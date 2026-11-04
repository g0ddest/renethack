//! The hero's states by the HP orb (`states`): a pill a state, as long as
//! the status line shows it. Each pill is the state's icon (its symbol on
//! its tone's plate) and its word, framed in its tone. The deadly ones
//! pulse. Over a pill, the tooltip says what the state means and what
//! usually helps. The column stands on the orb and grows upward: hunger and
//! encumbrance first, then the conditions in the status line's order.

use godot::builtin::Side;
use godot::classes::control::{GrowDirection, MouseFilter, SizeFlags};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Control, HBoxContainer, PanelContainer, StyleBoxFlat, TextureRect, VBoxContainer,
};
use godot::global::VerticalAlignment;
use godot::prelude::*;

use crate::hud::Tone;
use crate::i18n::{self, EngineKind};
use crate::states;
use crate::theme::{self, Face};
use crate::tr;

/// A pill's height and its icon's side.
pub const PILL_H: f32 = 40.0;
const ICON: f32 = 36.0;

/// The state's word as the player reads it (the engine's translator's).
pub fn word_shown(word: &str) -> String {
    i18n::engine(EngineKind::Status, word).into_owned()
}

/// What a pill's tooltip says: the word, what it means, what helps.
pub fn tip(word: &str) -> String {
    let id = states::help_id(word);
    let shown = word_shown(word);
    if !i18n::has(&id) {
        return shown;
    }
    tr!("state-tip", state = shown, help = i18n::tr(&id))
}

/// What usually helps a state, alone (the deadly banner's second line).
pub fn help(word: &str) -> Option<String> {
    let id = states::help_id(word);
    i18n::has(&id).then(|| i18n::tr(&id))
}

struct Pill {
    word: String,
    tone: Tone,
    node: Gd<PanelContainer>,
    style: Gd<StyleBoxFlat>,
    label: Gd<godot::classes::Label>,
}

/// The size of a pill's word, and its room besides: the icon, the gap, the
/// frame's margins.
const WORD_SIZE: i32 = 17;
const PILL_CHROME: f32 = ICON + 6.0 + 2.0 + 12.0 + 4.0;

/// How wide `word`'s pill is with its word.
fn pill_width(word: &str) -> f32 {
    let w = theme::font(Face::BodyBold)
        .get_string_size_ex(&word_shown(word))
        .font_size(WORD_SIZE)
        .done()
        .x;
    w + PILL_CHROME
}

fn pill(word: &str, tone: Tone) -> Pill {
    let (bg, border, fg) = crate::hud::chip_colors(tone);
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(bg);
    style.set_border_width_all(1);
    style.set_border_color(border);
    style.set_corner_radius_all(PILL_H as i32 / 2);
    style.set_content_margin_all(2.0);
    style.set_content_margin(Side::RIGHT, 12.0);
    let mut node = PanelContainer::new_alloc();
    // the tooltip wants the pointer; a click goes on to the world
    node.set_mouse_filter(MouseFilter::PASS);
    node.add_theme_stylebox_override("panel", &style);
    node.set_custom_minimum_size(Vector2::new(0.0, PILL_H));
    node.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    let mut row = HBoxContainer::new_alloc();
    row.set_mouse_filter(MouseFilter::IGNORE);
    row.add_theme_constant_override("separation", 6);
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(ICON, ICON));
    icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    if let Some(t) = crate::icons::state_icon(word) {
        icon.set_texture(&t);
    }
    row.add_child(&icon);
    let mut l = theme::styled_label(&word_shown(word), Face::BodyBold, WORD_SIZE, fg);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    theme::outline(&l, 4);
    row.add_child(&l);
    node.add_child(&row);
    crate::tips::set(&node, &tip(word));
    Pill {
        word: word.to_string(),
        tone,
        node,
        style,
        label: l,
    }
}

/// The column of pills by the HP orb.
pub struct StatesRail {
    root: Gd<VBoxContainer>,
    pills: Vec<Pill>,
    /// Pills hug the column's left (above the orb) or its right (left of
    /// the orb).
    right: bool,
    /// The room the column has across: a pill wider shows its icon alone
    /// (its word stays in its tooltip).
    room: f32,
}

/// The column's widest, and its room from the orb.
const RAIL_W: f32 = 300.0;
const RAIL_GAP: f32 = 8.0;

impl StatesRail {
    pub fn new() -> StatesRail {
        let mut root = VBoxContainer::new_alloc();
        root.set_name("States");
        // as wide as its widest pill: the world takes no click there
        root.set_mouse_filter(MouseFilter::PASS);
        root.add_theme_constant_override("separation", 4);
        root.set_alignment(godot::classes::box_container::AlignmentMode::END);
        root.set_v_grow_direction(GrowDirection::BEGIN);
        root.set_visible(false);
        StatesRail {
            root,
            pills: Vec::new(),
            right: false,
            room: RAIL_W,
        }
    }

    pub fn node(&self) -> Gd<Control> {
        self.root.clone().upcast()
    }

    /// Stand over the HP orb (`orb::SIZE` across at the cluster's left,
    /// its top at `orb_top` in the cluster) and grow upward, as wide as
    /// its widest pill: left-aligned at the orb's left above the cluster's
    /// top (over the toast's lane, which runs along that top and takes the
    /// pointer while it shows); in the `compact` layout, where the log
    /// docks right of the orb, right-aligned at the orb's right edge just
    /// above the orb, growing left into the map, `room` across at most.
    pub fn lay(&mut self, compact: bool, orb_top: f32, room: f32) {
        self.room = if compact { room.min(RAIL_W) } else { RAIL_W };
        let side = crate::orb::SIZE;
        let (x, y) = if compact {
            (side, orb_top - RAIL_GAP)
        } else {
            (0.0, -RAIL_GAP)
        };
        theme::place(&self.root, [0.0, 0.0, 0.0, 0.0], [x, y, x, y]);
        self.root.set_v_grow_direction(GrowDirection::BEGIN);
        self.root.set_h_grow_direction(if compact {
            GrowDirection::BEGIN
        } else {
            GrowDirection::END
        });
        self.right = compact;
        let (flag, room) = (self.flag(), self.room);
        for p in &mut self.pills {
            p.node.set_h_size_flags(flag);
            p.label.set_visible(pill_width(&p.word) <= room);
        }
    }

    fn flag(&self) -> SizeFlags {
        if self.right {
            SizeFlags::SHRINK_END
        } else {
            SizeFlags::SHRINK_BEGIN
        }
    }

    /// Show `shown` (the status line's words and their tones, in order):
    /// None when it shows that already, else the words new since.
    pub fn show(&mut self, shown: &[(String, Tone)]) -> Option<Vec<String>> {
        let same = self.pills.len() == shown.len()
            && self
                .pills
                .iter()
                .zip(shown)
                .all(|(p, (w, t))| p.word == *w && p.tone == *t);
        if same {
            return None;
        }
        let new: Vec<String> = shown
            .iter()
            .filter(|(w, _)| !self.pills.iter().any(|p| p.word == *w))
            .map(|(w, _)| w.clone())
            .collect();
        self.rebuild(shown);
        Some(new)
    }

    fn rebuild(&mut self, shown: &[(String, Tone)]) {
        for mut p in self.pills.drain(..) {
            p.node.queue_free();
        }
        let flag = self.flag();
        for (w, t) in shown {
            let mut p = pill(w, *t);
            p.node.set_h_size_flags(flag);
            p.label.set_visible(pill_width(w) <= self.room);
            self.root.add_child(&p.node);
            self.pills.push(p);
        }
        self.root.set_visible(!self.pills.is_empty());
    }

    /// The language changed: the same pills in its words.
    pub fn relang(&mut self) {
        let shown: Vec<(String, Tone)> = self
            .pills
            .iter()
            .map(|p| (p.word.clone(), p.tone))
            .collect();
        self.rebuild(&shown);
    }

    /// The deadly pills glow with the pulse `k` (0..1).
    pub fn pulse(&mut self, k: f32) {
        let (bg, ..) = crate::hud::chip_colors(Tone::Deadly);
        let glow = bg.lerp(Color::from_rgba(0.9, 0.16, 0.12, 0.95), k as f64);
        for p in self.pills.iter_mut().filter(|p| p.tone == Tone::Deadly) {
            p.style.set_bg_color(glow);
        }
    }

    /// Any pill is deadly (the pulse runs).
    pub fn deadly(&self) -> bool {
        self.pills.iter().any(|p| p.tone == Tone::Deadly)
    }

    /// The words shown, their tooltips and their rects on the canvas
    /// (self-tests).
    pub fn shown(&self) -> Vec<(String, String, Rect2)> {
        self.pills
            .iter()
            .map(|p| (p.word.clone(), tip(&p.word), p.node.get_global_rect()))
            .collect()
    }
}
