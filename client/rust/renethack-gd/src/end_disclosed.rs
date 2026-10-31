//! What NetHack discloses at the end (its `disclose` option, answered
//! for the player), on the end page in its order and words: the
//! possessions with their letters, ^X's attributes in its sections, the
//! creatures vanquished and genocided, the overview, a section at a time
//! under a row of tabs (the pad's X and Y, the keys' [ and ]); the conduct
//! goes with the page's own, once.

use std::cell::Cell;
use std::rc::Rc;

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, Control, HBoxContainer, Label, MarginContainer, StyleBoxFlat, TextureRect,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::i18n::{self, EngineKind};
use crate::icons;
use crate::theme::{self, Face};
use crate::tr;

/// One of the end's disclosures as NetHack wrote it: a text window's
/// lines as they came (their spaces, true: bold), or the possessions'
/// menu. A window the client lays out (the creatures vanquished, the
/// overview) comes in the words of its layout, with its heading.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Disclosure {
    pub lines: Vec<(String, bool)>,
    /// A menu's rows (the possessions), not a text window's lines.
    pub items: Vec<DisclosedItem>,
    /// The heading, and the lines already in the player's language.
    pub laid_out: Option<String>,
}

/// A row of the possessions' menu.
#[derive(Debug, Clone, PartialEq)]
pub struct DisclosedItem {
    /// Its letter ("a - an uncursed +1 spear"); none for a heading.
    pub letter: Option<char>,
    /// The appearance's tile and its class's symbol, for its icon.
    pub look: Option<(i32, char)>,
    /// As the engine wrote it.
    pub text: String,
    pub heading: bool,
}

/// Which part of the end a disclosure is.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    /// The possessions, and a container's contents after them.
    Possessions,
    /// ^X's attributes.
    Attributes,
    /// The conduct (and NetHack's achievements): the page's own section.
    Conduct,
    /// A window the client lays out, under its heading.
    Laid(String),
}

fn part(d: &Disclosure) -> Part {
    if let Some(h) = &d.laid_out {
        return Part::Laid(h.clone());
    }
    if !d.items.is_empty() {
        return Part::Possessions;
    }
    let first = (d.lines.iter())
        .map(|(l, _)| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if first.starts_with("Contents of ") || first.ends_with(" is empty.") {
        Part::Possessions
    } else if first.starts_with("Voluntary challenges") {
        Part::Conduct
    } else {
        Part::Attributes
    }
}

/// A text window's line in the language now, its indent kept apart: the
/// engine's own line, its spaces too (" You were…" is ^X's).
fn said(line: &str) -> (usize, String) {
    let text = i18n::engine(EngineKind::Window, line).into_owned();
    let indent = line.len() - line.trim_start().len();
    (indent, text.trim().to_string())
}

/// NetHack's conduct, in its words and the language now, for the page's
/// conduct section: each line, true: a heading ("Achievements:"). None:
/// not disclosed (the page shows the conduct it reckoned).
pub fn conduct_lines(disclosed: &[Disclosure]) -> Option<Vec<(String, bool)>> {
    let d = disclosed.iter().find(|d| part(d) == Part::Conduct)?;
    let lines = (d.lines.iter())
        .filter(|(l, _)| !l.trim().is_empty())
        // its own heading: the page's section says it
        .skip(1)
        .map(|(l, bold)| {
            let (indent, text) = said(l);
            (text, *bold || (indent == 0 && l.trim_end().ends_with(':')))
        })
        .collect();
    Some(lines)
}

/// The tabs under the page's story: their buttons and pages, the one
/// shown.
pub struct Sections {
    pub root: Gd<VBoxContainer>,
    buttons: Vec<Gd<Button>>,
    pages: Vec<Gd<Control>>,
    at: Rc<Cell<usize>>,
}

impl Sections {
    /// The tab `by` along, round.
    pub fn step(&self, by: i32) {
        let n = self.pages.len() as i32;
        let i = (self.at.get() as i32 + by).rem_euclid(n.max(1)) as usize;
        show(&self.buttons, &self.pages, &self.at, i);
    }

    /// The tab shown and how many there are (self-tests).
    pub fn shown(&self) -> (usize, usize) {
        (self.at.get(), self.pages.len())
    }
}

/// Tab `i` shown, the others hidden.
fn show(buttons: &[Gd<Button>], pages: &[Gd<Control>], at: &Rc<Cell<usize>>, i: usize) {
    at.set(i);
    for (k, (b, p)) in buttons.iter().zip(pages).enumerate() {
        let mut b = b.clone();
        b.set_pressed_no_signal(k == i);
        let mut p = p.clone();
        p.set_visible(k == i);
    }
}

/// The disclosures but the conduct, a tab each part, in their order; the
/// columns `ratio` apart as the page's above them are. None: nothing.
pub fn sections(disclosed: &[Disclosure], ratio: f32) -> Option<Sections> {
    let mut parts: Vec<(String, Gd<Control>)> = Vec::new();
    let mut possessions: Vec<&Disclosure> = Vec::new();
    let mut attributes: Vec<&Disclosure> = Vec::new();
    // in NetHack's order: the possessions and contents first, then the
    // attributes, the creatures, the overview
    let mut order: Vec<Part> = Vec::new();
    for d in disclosed {
        let p = part(d);
        match &p {
            Part::Possessions => possessions.push(d),
            Part::Attributes => attributes.push(d),
            _ => {}
        }
        if !order.contains(&p) && p != Part::Conduct {
            order.push(p);
        }
    }
    for p in order {
        match p {
            Part::Possessions => parts.push((
                tr!("end-possessions"),
                possessions_page(&possessions, ratio),
            )),
            Part::Attributes => {
                parts.push((tr!("end-attributes"), attributes_page(&attributes, ratio)))
            }
            Part::Laid(heading) => {
                let d = disclosed
                    .iter()
                    .find(|d| d.laid_out.as_ref() == Some(&heading));
                if let Some(d) = d {
                    parts.push((heading.clone(), laid_page(d, ratio)));
                }
            }
            Part::Conduct => {}
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut root = VBoxContainer::new_alloc();
    root.add_theme_constant_override("separation", 12);
    root.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut bar = HBoxContainer::new_alloc();
    bar.add_theme_constant_override("separation", 6);
    root.add_child(&bar);
    root.add_child(&rule());
    let at = Rc::new(Cell::new(0));
    let mut buttons = Vec::new();
    let mut pages = Vec::new();
    for (title, page) in parts {
        let b = tab_button(&title);
        bar.add_child(&b);
        root.add_child(&page);
        buttons.push(b);
        pages.push(page);
    }
    // a click on a tab shows it
    for (i, b) in buttons.iter().enumerate() {
        let (bs, ps, at) = (buttons.clone(), pages.clone(), at.clone());
        b.clone()
            .signals()
            .pressed()
            .connect(move || show(&bs, &ps, &at, i));
    }
    show(&buttons, &pages, &at, 0);
    Some(Sections {
        root,
        buttons,
        pages,
        at,
    })
}

/// A tab's button: its words, the one shown lit.
fn tab_button(title: &str) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_text(title);
    b.set_toggle_mode(true);
    b.set_focus_mode(godot::classes::control::FocusMode::NONE);
    b.add_theme_font_override("font", &theme::font(Face::Caps));
    b.add_theme_font_size_override("font_size", 17);
    let flat = |border: Color, width: i32| {
        let mut sb = StyleBoxFlat::new_gd();
        sb.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        sb.set_border_width(godot::builtin::Side::BOTTOM, width);
        sb.set_border_color(border);
        for (side, px) in [
            (godot::builtin::Side::LEFT, 10.0),
            (godot::builtin::Side::RIGHT, 10.0),
            (godot::builtin::Side::TOP, 4.0),
            (godot::builtin::Side::BOTTOM, 6.0),
        ] {
            sb.set_content_margin(side, px);
        }
        sb
    };
    let off = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), 0);
    let on = flat(theme::GOLD, 2);
    for (state, style) in [
        ("normal", &off),
        ("hover", &off),
        ("pressed", &on),
        ("hover_pressed", &on),
        ("focus", &off),
    ] {
        b.add_theme_stylebox_override(state, style);
    }
    for (state, color) in [
        ("font_color", theme::TEXT_DIM),
        ("font_hover_color", theme::TEXT),
        ("font_pressed_color", theme::GOLD_BRIGHT),
        ("font_hover_pressed_color", theme::GOLD_BRIGHT),
    ] {
        b.add_theme_color_override(state, color);
    }
    b
}

fn rule() -> Gd<Control> {
    let mut r = godot::classes::ColorRect::new_alloc();
    r.set_color(Color {
        a: 0.35,
        ..theme::GOLD_DIM
    });
    r.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.upcast()
}

/// Blocks laid in two columns as the page's above (`ratio` the right's
/// share to the left's), the left filled to about half their lines first.
fn two_columns(blocks: Vec<(Gd<Control>, usize)>, ratio: f32) -> Gd<Control> {
    let total: usize = blocks.iter().map(|(_, n)| n).sum();
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 34);
    row.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let column = |stretch: f32| {
        let mut c = VBoxContainer::new_alloc();
        c.add_theme_constant_override("separation", 14);
        c.set_h_size_flags(SizeFlags::EXPAND_FILL);
        c.set_stretch_ratio(stretch);
        c
    };
    let (mut left, mut right) = (column(1.0), column(ratio));
    let mut lines = 0;
    for (block, n) in blocks {
        if lines * 2 < total || lines == 0 {
            left.add_child(&block);
        } else {
            right.add_child(&block);
        }
        lines += n;
    }
    row.add_child(&left);
    row.add_child(&right);
    row.upcast()
}

/// A line of words at the page's reading size.
fn line(text: &str, face: Face, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, 17, color);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_h_size_flags(SizeFlags::EXPAND_FILL);
    l
}

/// A heading of a part's block: a class, ^X's "Background:".
fn heading(text: &str) -> Gd<Label> {
    let mut l = theme::styled_label(text.trim_end_matches(':'), Face::Caps, 17, theme::GOLD);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l
}

/// Lines under a heading, each indented as the engine indented it.
fn text_block(title: Option<&str>, lines: &[(usize, String, bool)]) -> (Gd<Control>, usize) {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 3);
    if let Some(t) = title {
        col.add_child(&heading(t));
    }
    for (indent, text, bold) in lines {
        let face = if *bold { Face::BodyBold } else { Face::Body };
        let l = line(text, face, theme::TEXT);
        let mut m = MarginContainer::new_alloc();
        m.add_theme_constant_override("margin_left", (*indent as i32 * 6).min(36));
        m.add_child(&l);
        col.add_child(&m);
    }
    (col.upcast(), lines.len() + usize::from(title.is_some()))
}

/// The possessions as the pack's list shows them (a letter, an icon, the
/// name, under their classes), then each container's contents.
fn possessions_page(ds: &[&Disclosure], ratio: f32) -> Gd<Control> {
    let mut blocks: Vec<(Gd<Control>, usize)> = Vec::new();
    for d in ds {
        if !d.items.is_empty() {
            let mut col: Option<(Gd<VBoxContainer>, usize)> = None;
            for it in &d.items {
                if it.heading {
                    if let Some((c, n)) = col.take() {
                        blocks.push((c.upcast(), n));
                    }
                    let mut c = VBoxContainer::new_alloc();
                    c.add_theme_constant_override("separation", 3);
                    let words = i18n::engine(EngineKind::Menu, &it.text).into_owned();
                    c.add_child(&heading(&words));
                    col = Some((c, 1));
                    continue;
                }
                let (c, n) = col.get_or_insert_with(|| {
                    let mut c = VBoxContainer::new_alloc();
                    c.add_theme_constant_override("separation", 3);
                    (c, 0)
                });
                c.add_child(&item_row(it));
                *n += 1;
            }
            if let Some((c, n)) = col {
                blocks.push((c.upcast(), n));
            }
            continue;
        }
        // a container's contents: its own heading, its lines
        let mut lines: Vec<(usize, String, bool)> = (d.lines.iter())
            .filter(|(l, _)| !l.trim().is_empty())
            .map(|(l, bold)| {
                let (indent, text) = said(l);
                (indent, text, *bold)
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        let (_, first, _) = lines.remove(0);
        blocks.push(text_block(Some(&first), &lines));
    }
    two_columns(blocks, ratio)
}

/// A possession's row: its letter, its icon, its name.
fn item_row(it: &DisclosedItem) -> Gd<Control> {
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    // not small caps: `a` and `A` are different items
    let letter = it.letter.map(|c| c.to_string()).unwrap_or_default();
    let mut l = theme::styled_label(&letter, Face::BodyBold, 17, theme::GOLD_BRIGHT);
    l.set_custom_minimum_size(Vector2::new(16.0, 0.0));
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    i18n::verbatim(&l);
    row.add_child(&l);
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(26.0, 26.0));
    icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    if let Some((tile, class)) = it.look {
        icon.set_texture(&icons::item_icon(tile, class));
    }
    row.add_child(&icon);
    let words = i18n::engine(EngineKind::Name, &it.text).into_owned();
    let mut w = line(&words, Face::Body, theme::TEXT);
    w.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    row.add_child(&w);
    row.upcast()
}

/// ^X's attributes in its own sections ("Background:", "Basics:"…), its
/// title left to the tab.
fn attributes_page(ds: &[&Disclosure], ratio: f32) -> Gd<Control> {
    let mut blocks: Vec<(Gd<Control>, usize)> = Vec::new();
    for d in ds {
        let mut title: Option<String> = None;
        let mut lines: Vec<(usize, String, bool)> = Vec::new();
        let raw: Vec<&(String, bool)> = (d.lines.iter())
            .filter(|(l, _)| !l.trim().is_empty())
            // its first line names the hero: the tab says what it is
            .skip(1)
            .collect();
        for (l, bold) in raw {
            let (indent, text) = said(l);
            // a section's title: at the left, ending with a colon
            if indent == 0 && l.trim_end().ends_with(':') {
                if title.is_some() || !lines.is_empty() {
                    blocks.push(text_block(title.as_deref(), &lines));
                }
                title = Some(text);
                lines.clear();
                continue;
            }
            lines.push((indent.saturating_sub(1), text, *bold));
        }
        if title.is_some() || !lines.is_empty() {
            blocks.push(text_block(title.as_deref(), &lines));
        }
    }
    two_columns(blocks, ratio)
}

/// A window the client lays out, in its layout's words: its headings
/// (bold) start the blocks.
fn laid_page(d: &Disclosure, ratio: f32) -> Gd<Control> {
    let mut blocks: Vec<(Gd<Control>, usize)> = Vec::new();
    let mut title: Option<String> = None;
    let mut lines: Vec<(usize, String, bool)> = Vec::new();
    for (text, bold) in &d.lines {
        if *bold {
            if title.is_some() || !lines.is_empty() {
                blocks.push(text_block(title.as_deref(), &lines));
            }
            title = Some(text.clone());
            lines.clear();
        } else {
            lines.push((0, text.clone(), false));
        }
    }
    if title.is_some() || !lines.is_empty() {
        blocks.push(text_block(title.as_deref(), &lines));
    }
    two_columns(blocks, ratio)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[&str]) -> Disclosure {
        Disclosure {
            lines: lines.iter().map(|l| (l.to_string(), false)).collect(),
            ..Disclosure::default()
        }
    }

    #[test]
    fn each_disclosure_is_its_part() {
        assert_eq!(
            part(&text(&["Contents of your sack:", "  a scroll"])),
            Part::Possessions
        );
        assert_eq!(
            part(&text(&["Voluntary challenges:", "  You were an atheist."])),
            Part::Conduct
        );
        assert_eq!(
            part(&text(&["Hero the Valkyrie's attributes:", "Background:"])),
            Part::Attributes
        );
        let laid = Disclosure {
            laid_out: Some("Dungeon overview".into()),
            ..Disclosure::default()
        };
        assert_eq!(part(&laid), Part::Laid("Dungeon overview".into()));
    }

    #[test]
    fn the_conduct_reads_as_nethack_wrote_it() {
        let d = text(&[
            "Voluntary challenges:",
            "",
            "  You were an atheist.",
            "Achievements:",
            "  You entered the Gnomish Mines.",
        ]);
        let lines = conduct_lines(&[d]).unwrap_or_default();
        assert_eq!(
            lines,
            [
                ("You were an atheist.".to_string(), false),
                ("Achievements:".to_string(), true),
                ("You entered the Gnomish Mines.".to_string(), false),
            ]
        );
        assert!(conduct_lines(&[]).is_none());
    }
}
