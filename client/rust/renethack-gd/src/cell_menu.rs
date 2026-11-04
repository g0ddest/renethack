//! What can be done at a cell, at the cell: NetHack's #therecmdmenu (a
//! right click on the map, a left click on the hero, a pad's A on the
//! hero or its radial's «Здесь») as a popup by the cell rather than a
//! dialog in the screen's middle. On top, what the hero knows is there
//! (the catalog's names of what the map shows, as the hover tooltip
//! says); under it the engine's own entries, each with an emblem, in the
//! player's language. Only the engine's entries: nothing NetHack would
//! not offer.
//!
//! A click picks a row, a click beside the popup cancels; the arrows and
//! Enter, a row's letter, Esc; a pad's d-pad, A and B.

use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{Button, HBoxContainer, Label, TextureRect, VBoxContainer};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::MenuItem;

use crate::i18n::{self, EngineKind};
use crate::icons::{self, Glyph};
use crate::pad_view;
use crate::theme::{self, Face, place};

/// The menu's title as the engine writes it (cmd.c's there_cmd_menu).
pub const TITLE: &str = "What do you want to do?";
/// The entry that does nothing: a pad's focus starts on it.
pub const LOOK_HERE: &str = "Look at what is here";

/// A row's height, its emblem, and the popup's widths.
const ROW_H: f32 = 32.0;
const ICON: f32 = 22.0;
const LEAST_W: f32 = 240.0;
const MOST_W: f32 = 440.0;

/// An entry's emblem, from the engine's English words.
pub fn entry_glyph(english: &str) -> Glyph {
    let e = english.to_lowercase();
    let has = |w: &str| e.contains(w);
    if e.starts_with("look") || e.starts_with("examine") {
        Glyph::Eye
    } else if e.starts_with("attack") {
        Glyph::Sword
    } else if e.starts_with("kick") {
        Glyph::Boots
    } else if e.starts_with("go up") || e.starts_with("go down") || e.starts_with("travel") {
        Glyph::Stairs
    } else if e.starts_with("pick up") || e.starts_with("talk") || has("disarm") || has("unlock") {
        Glyph::Hand
    } else if e.starts_with("open")
        || e.starts_with("close")
        || e.starts_with("loot")
        || e.starts_with("tip")
        || e == "inventory"
    {
        Glyph::Sack
    } else if e.starts_with("drop") || e.starts_with("throw") {
        Glyph::Drop
    } else if e.starts_with("eat") {
        Glyph::Drumstick
    } else if e.starts_with("drink") || e.starts_with("dip") {
        Glyph::Flask
    } else if e.starts_with("sacrifice") {
        Glyph::Ankh
    } else if e.starts_with("rest") || e.starts_with("sit") {
        Glyph::Moon
    } else if e.starts_with("search") {
        Glyph::Lens
    } else if e.starts_with("cast") {
        Glyph::Book
    } else if e.starts_with("ride") || e.starts_with("dismount") || has("saddle") {
        Glyph::Leash
    } else if e.starts_with("use") {
        Glyph::Burst
    } else if e.starts_with("name") || e.starts_with("rename") {
        Glyph::Scroll
    } else if e.starts_with("move") || e.starts_with("push") || e.starts_with("swap") {
        Glyph::Boots
    } else {
        Glyph::Grid
    }
}

/// A row: its button, its letter, the engine's item it picks.
struct Row {
    button: Gd<Button>,
    letter: char,
    idx: u32,
    /// Its place among the menu's items (as `menu_cursor` counts).
    item: usize,
}

/// The popup's rows and the one the keys (a pad) are on.
pub struct CellMenu {
    rows: Vec<Row>,
    focus: Option<usize>,
    /// Row looks: (plain, current).
    styles: [Gd<godot::classes::StyleBoxFlat>; 2],
}

/// A flat row style, lit when current.
fn row_style(current: bool) -> Gd<godot::classes::StyleBoxFlat> {
    let mut sb = godot::classes::StyleBoxFlat::new_gd();
    let gold = |a: f32| Color { a, ..theme::GOLD };
    sb.set_bg_color(if current { gold(0.2) } else { gold(0.0) });
    sb.set_corner_radius_all(3);
    sb.set_content_margin_all(0.0);
    if current {
        sb.set_border_width(godot::builtin::Side::LEFT, 3);
        sb.set_border_color(theme::GOLD_BRIGHT);
    }
    sb
}

impl CellMenu {
    /// The popup's column in `col`: `look` (what is known at the cell, a
    /// line each) over the engine's `items` (`shown`: their words as the
    /// player reads them), a button each pushing `click(i)` for row `i`;
    /// `pad`: the letters hidden. Its width.
    pub fn build(
        col: &mut Gd<VBoxContainer>,
        look: &[String],
        items: &[MenuItem],
        pad: bool,
        click: impl Fn(usize) -> Box<dyn Fn()>,
    ) -> (CellMenu, f32) {
        let font = theme::font(Face::Body);
        let words: Vec<String> = items
            .iter()
            .map(|i| i18n::engine(EngineKind::Menu, i.str.as_deref().unwrap_or("")).into_owned())
            .collect();
        let text_w = words
            .iter()
            .map(|w| font.get_string_size_ex(w).font_size(16).done().x)
            .fold(0.0f32, f32::max);
        let width = (text_w + ICON + 70.0).clamp(LEAST_W, MOST_W);
        for line in look.iter().filter(|l| !l.trim().is_empty()) {
            let mut l = theme::styled_label(line, Face::BodyBold, 16, theme::GOLD_BRIGHT);
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            l.set_custom_minimum_size(Vector2::new(width, 0.0));
            col.add_child(&l);
        }
        if !look.is_empty() {
            let mut rule = godot::classes::ColorRect::new_alloc();
            rule.set_color(Color {
                a: 0.5,
                ..theme::GOLD_DIM
            });
            rule.set_custom_minimum_size(Vector2::new(width, 1.0));
            rule.set_mouse_filter(MouseFilter::IGNORE);
            col.add_child(&rule);
        }
        let styles = [row_style(false), row_style(true)];
        let mut rows = Vec::new();
        let mut letters = ('a'..='z').chain('A'..='Z');
        for (item, (it, word)) in items.iter().zip(&words).enumerate() {
            if !it.selectable {
                let mut l = theme::styled_label(word, Face::Body, 15, theme::TEXT_DIM);
                l.set_custom_minimum_size(Vector2::new(width, 0.0));
                col.add_child(&l);
                continue;
            }
            let letter = u32::try_from(it.ch)
                .ok()
                .and_then(char::from_u32)
                .filter(|c| c.is_ascii_alphabetic())
                .or_else(|| letters.next())
                .unwrap_or(' ');
            let mut b = Button::new_alloc();
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(width, ROW_H));
            for s in ["normal", "disabled", "focus"] {
                b.add_theme_stylebox_override(s, &styles[0]);
            }
            for s in ["hover", "pressed", "hover_pressed"] {
                b.add_theme_stylebox_override(s, &styles[1]);
            }
            let mut hbox = HBoxContainer::new_alloc();
            hbox.set_mouse_filter(MouseFilter::IGNORE);
            hbox.add_theme_constant_override("separation", 8);
            let mut icon = TextureRect::new_alloc();
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_custom_minimum_size(Vector2::new(ICON, ICON));
            icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            let english = it.str.as_deref().unwrap_or("");
            icon.set_texture(&icons::emblem(entry_glyph(english)));
            hbox.add_child(&icon);
            let mut l: Gd<Label> = theme::styled_label(word, Face::Body, 16, theme::TEXT);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            l.set_h_size_flags(SizeFlags::EXPAND_FILL);
            l.set_clip_text(true);
            hbox.add_child(&l);
            let mut key = theme::styled_label(&letter.to_string(), Face::Mono, 13, theme::TEXT_DIM);
            key.set_vertical_alignment(VerticalAlignment::CENTER);
            key.set_horizontal_alignment(HorizontalAlignment::RIGHT);
            i18n::verbatim(&key);
            // a key, the keyboard's: no pad player's
            pad_view::key_words(&key, pad);
            hbox.add_child(&key);
            place(&hbox, [0.0, 0.0, 1.0, 1.0], [8.0, 0.0, -8.0, 0.0]);
            b.add_child(&hbox);
            let act = click(rows.len());
            b.signals().pressed().connect(act);
            col.add_child(&b);
            rows.push(Row {
                button: b,
                letter,
                idx: it.idx.max(0) as u32,
                item,
            });
        }
        (
            CellMenu {
                rows,
                focus: None,
                styles,
            },
            width,
        )
    }

    /// The keys' (a pad's) row: `item` (a place among the menu's items,
    /// as the engine sent them), else the first.
    pub fn focus_item(&mut self, item: Option<usize>) {
        let at = item
            .and_then(|i| self.rows.iter().position(|r| r.item == i))
            .or((!self.rows.is_empty()).then_some(0));
        self.set_focus(at);
    }

    fn set_focus(&mut self, at: Option<usize>) {
        if let Some(old) = self.focus.and_then(|f| self.rows.get_mut(f)) {
            for s in ["normal", "disabled", "focus"] {
                old.button.add_theme_stylebox_override(s, &self.styles[0]);
            }
        }
        self.focus = at.filter(|&i| i < self.rows.len());
        if let Some(new) = self.focus.and_then(|f| self.rows.get_mut(f)) {
            for s in ["normal", "disabled", "focus"] {
                new.button.add_theme_stylebox_override(s, &self.styles[1]);
            }
        }
    }

    /// Row `i` picked: the engine's answer.
    pub fn pick(&self, i: usize) -> Option<nh_protocol::Reply> {
        let r = self.rows.get(i)?;
        Some(nh_protocol::Reply::Menu(vec![(r.idx, -1)]))
    }

    /// A key: ↑↓ move, Enter picks, a row's letter picks it, Esc
    /// cancels. None: nothing to answer yet.
    pub fn key(&mut self, k: &nh_world::KeyInput) -> Option<nh_protocol::Reply> {
        use nh_world::Key;
        let n = self.rows.len();
        match k.key {
            Key::Escape if !k.echo => Some(nh_protocol::Reply::Cancel),
            Key::Up | Key::Down if n > 0 => {
                let d: i64 = if k.key == Key::Up { -1 } else { 1 };
                let next = match self.focus {
                    Some(f) => (f as i64 + d).rem_euclid(n as i64) as usize,
                    None if d > 0 => 0,
                    None => n - 1,
                };
                self.set_focus(Some(next));
                None
            }
            Key::Enter | Key::KeypadEnter if !k.echo => self.focus.and_then(|f| self.pick(f)),
            Key::Char(c) if !k.echo && !k.mods.ctrl && !k.mods.alt => {
                let i = self.rows.iter().position(|r| r.letter == c)?;
                self.pick(i)
            }
            _ => None,
        }
    }

    /// The keys' (a pad's) row, as its place among the menu's items.
    pub fn focus(&self) -> Option<usize> {
        self.focus.and_then(|f| self.rows.get(f)).map(|r| r.item)
    }

    /// Each row's place among the menu's items and its rect on screen
    /// (self-tests).
    pub fn rows(&self) -> Vec<(usize, Rect2)> {
        self.rows
            .iter()
            .map(|r| (r.item, r.button.get_global_rect()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entries_get_their_emblems() {
        assert_eq!(entry_glyph("Look at what is here"), Glyph::Eye);
        assert_eq!(entry_glyph("Kick the door"), Glyph::Boots);
        assert_eq!(entry_glyph("Go up the stairs"), Glyph::Stairs);
        assert_eq!(entry_glyph("Attack the jackal"), Glyph::Sword);
        assert_eq!(entry_glyph("Talk to the kitten"), Glyph::Hand);
        assert_eq!(entry_glyph("Lock or unlock the door"), Glyph::Hand);
        assert_eq!(entry_glyph("Drink from the fountain"), Glyph::Flask);
        assert_eq!(entry_glyph("Inventory"), Glyph::Sack);
        assert_eq!(entry_glyph("Something new"), Glyph::Grid);
    }
}
