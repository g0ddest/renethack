//! Modal dialogs for menus, questions, text input, the extended-command
//! palette and text windows. At most one is open, for one request; every
//! event it queues carries that request's id.
//!
//! Lists (menus, the palette) are rows of a fixed height in a scroll
//! container, so a dialog knows where each row is and pages and reveals rows
//! itself. No dialog grows past the window: long content scrolls.

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, GrowDirection, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, DisplayServer, Font, HBoxContainer, Label, LineEdit,
    OptionButton, PanelContainer, RichTextLabel, ScrollContainer, StyleBox, StyleBoxEmpty,
    StyleBoxFlat, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::{Catalog, ESC, PickHow, Reply};
use nh_world::{Key, KeyInput, MenuEntry, MenuOutcome, MenuState, Prompt, TextLine, choice_answer};

use nh_i18n::lexicon::Lexicon;

use crate::i18n::{self, EngineKind, Lang};
use crate::pickers::{self, Ask, ObjClass, Pick, Wish};
use crate::theme::{self, Face, Frame, bbcode_escape, hex, nh_color, place};
use crate::tr;
use crate::ui_events::{DialogEvent, UiEvent, UiQueue, push};

/// NetHack's BUFSZ less the NUL.
/// Where a dialog's panel sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Menus, text windows, the palette: centred over a dimmed map.
    Centre,
    /// Questions: top centre, the map left as it is.
    Top,
}

/// The top of a question's panel: under the prompt banner.
const TOP_Y: f32 = 84.0;

const MAX_TEXT_BYTES: usize = 255;
const ESC_CHAR: char = '\u{1b}';
/// A menu item without a colour of its own (CLR_* NO_COLOR).
const NO_COLOR: i32 = 8;
/// One list row, and a blank separator row, in pixels.
pub(crate) const ROW_H: f32 = 30.0;
const SPACER_H: f32 = 10.0;
/// Height a list dialog keeps for everything but the list: the title, the
/// footer, the buttons, the panel's margins and the screen's.
const LIST_CHROME: f32 = 320.0;
/// The same for a text window (no footer).
const TEXT_CHROME: f32 = 220.0;
/// The canvas the HUD is designed for (theme::apply_scaling).
const REFERENCE_SCREEN: Vector2 = theme::DESIGN;
/// Space kept free left and right of a dialog.
const SIDE_MARGIN: f32 = 80.0;
/// Rows the palette shows at once.
const PALETTE_ROWS: f32 = 12.0;
/// Rows a picker of names shows at once, and the most it lists (a longer
/// list says how many more a search would narrow).
const PICKER_ROWS: f32 = 10.0;
const PICKER_SLOTS: usize = 80;
/// The most rows an engine menu shows at once.
const MENU_ROWS: f32 = 18.0;
/// Unselectable menu lines: grey, still easy to read.
const INFO_TEXT: Color = Color::from_rgb(0.74, 0.69, 0.61);

/// One extended command as the palette lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteCmd {
    pub name: String,
    pub desc: String,
    pub key: i32,
}

/// Does the palette list this command? Not "#" itself: choosing it would
/// only ask for an extended command again.
fn palette_lists(name: &str) -> bool {
    !name.is_empty() && name != "#"
}

/// The catalog's extended commands the palette offers.
pub fn palette_cmds(catalog: Option<&Catalog>) -> Vec<PaletteCmd> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    catalog
        .extcmds
        .iter()
        .filter(|e| palette_lists(&e.name))
        .map(|e| PaletteCmd {
            name: e.name.clone(),
            // the engine's words: its translator gives them
            desc: i18n::engine(EngineKind::Menu, &e.desc).into_owned(),
            key: e.key,
        })
        .collect()
}

/// Commands matching `text`: the exact name first, then prefix matches,
/// then other substring matches, each shortest first (then by name). An
/// empty `text` lists everything in catalog order.
pub fn palette_filter(cmds: &[PaletteCmd], text: &str) -> Vec<usize> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return (0..cmds.len()).collect();
    }
    let mut hits: Vec<(u8, usize)> = cmds
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = c.name.to_lowercase();
            let tier = if name == text {
                0
            } else if name.starts_with(&text) {
                1
            } else if name.contains(&text) {
                2
            } else {
                return None;
            };
            Some((tier, i))
        })
        .collect();
    hits.sort_by(|&(ta, a), &(tb, b)| {
        let (a, b) = (&cmds[a].name, &cmds[b].name);
        ta.cmp(&tb).then(a.len().cmp(&b.len())).then(a.cmp(b))
    });
    hits.into_iter().map(|(_, i)| i).collect()
}

/// The longest common prefix of the names that start with `text`; `text`
/// itself when none do.
pub fn palette_complete(cmds: &[PaletteCmd], text: &str) -> String {
    let text = text.trim().to_lowercase();
    let mut names = cmds
        .iter()
        .map(|c| c.name.as_str())
        .filter(|n| n.starts_with(&text));
    let Some(first) = names.next() else {
        return text;
    };
    let mut common = first.to_string();
    for n in names {
        let len = common
            .chars()
            .zip(n.chars())
            .take_while(|(a, b)| a == b)
            .count();
        common = common.chars().take(len).collect();
    }
    common
}

/// Cut `s` to at most `max` bytes on a character boundary.
fn truncate_bytes(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// How a key code reads: "^X", "M-x", "x".
fn key_name(key: i32) -> String {
    match key {
        0 => String::new(),
        1..=31 => format!("^{}", char::from(b'@' + key as u8)),
        128..=255 => format!("M-{}", char::from((key & 0x7f) as u8)),
        _ => char::from_u32(key as u32).map_or(String::new(), |c| c.to_string()),
    }
}

/// A Choice button's label: the usual answers spelled out, others as typed.
fn choice_label(c: char) -> String {
    match c {
        'y' => tr!("dlg-yes"),
        'n' => tr!("dlg-no"),
        'q' => tr!("dlg-cancel-q"),
        'a' => tr!("dlg-all-a"),
        c => c.to_string(),
    }
}

/// What the check medallion of a pick-any row shows: nothing, a check,
/// or the count.
fn mark_text(e: &MenuEntry) -> String {
    match (e.selected, e.count) {
        (true, Some(n)) => n.to_string(),
        (true, None) => "✔".to_string(),
        (false, _) => String::new(),
    }
}

/// An option's value in the options menu: "[X]", "[ ]", "[default]".
#[derive(Debug, Clone, PartialEq)]
enum OptionValue {
    On,
    Off,
    Set(String),
}

fn option_value(field: &str) -> Option<OptionValue> {
    let inner = field.strip_prefix('[')?.strip_suffix(']')?;
    if inner.contains('[') || inner.contains(']') {
        return None;
    }
    Some(match inner.trim() {
        "X" | "x" => OptionValue::On,
        "" => OptionValue::Off,
        v => OptionValue::Set(v.to_string()),
    })
}

/// The columns the engine laid a row out in with runs of spaces or tabs
/// (the options, the skills, the spells, `#?`): each is drawn in its own
/// cell, so the body face keeps them aligned.
fn fields(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut spaces = 0;
    for c in text.trim().chars() {
        if c == ' ' {
            spaces += 1;
            continue;
        }
        if c == '\t' {
            spaces = 2;
            continue;
        }
        if spaces >= 2 && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        } else if spaces == 1 {
            cur.push(' ');
        }
        spaces = 0;
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The item's map symbol, when it has a printable one.
fn glyph_char(e: &MenuEntry) -> Option<char> {
    e.glyph
        .as_ref()
        .and_then(|g| u32::try_from(g.ch).ok())
        .and_then(char::from_u32)
        .filter(|c| !c.is_control() && *c != ' ')
}

/// How a menu row looks.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RowKind {
    /// Blank: a gap between groups.
    Spacer,
    /// Unselectable with an attribute: a group header.
    Header,
    /// Unselectable plain text.
    Info,
    Item,
}

fn row_kind(e: &MenuEntry) -> RowKind {
    if e.selectable {
        RowKind::Item
    } else if e.text.trim().is_empty() {
        RowKind::Spacer
    } else if e.attr & 0x0f != 0 {
        RowKind::Header
    } else {
        RowKind::Info
    }
}

/// The literal character a key typed, for dialogs: never a repeat, never
/// with Ctrl/Alt, never a direction key.
fn typed(input: &KeyInput) -> Option<char> {
    if input.echo || input.mods.ctrl || input.mods.alt {
        return None;
    }
    match input.key {
        Key::Char(c) => Some(c),
        Key::Enter | Key::KeypadEnter => Some('\n'),
        Key::Escape => Some(ESC_CHAR),
        Key::Keypad(d) => char::from_digit(u32::from(d), 10),
        _ => None,
    }
}

/// Keys that move around a list or a text: arrows and paging, held or not,
/// without Ctrl/Alt.
fn nav(input: &KeyInput) -> Option<Key> {
    if input.mods.ctrl || input.mods.alt {
        return None;
    }
    match input.key {
        k @ (Key::Up
        | Key::Down
        | Key::Left
        | Key::Right
        | Key::PageUp
        | Key::PageDown
        | Key::Home
        | Key::End) => Some(k),
        _ => None,
    }
}

/// Fonts and row looks, made once.
#[derive(Clone)]
struct Look {
    bold: Gd<Font>,
    italic: Gd<Font>,
    /// Monospace advance and line height.
    char_w: f32,
    line_h: f32,
    /// (normal, hover) for: plain, selected, current, current and selected.
    rows: [(Gd<StyleBoxFlat>, Gd<StyleBoxFlat>); 4],
    no_focus: Gd<StyleBoxEmpty>,
    default_button: Gd<StyleBox>,
    /// The button a gamepad is on.
    focus_button: Gd<StyleBoxFlat>,
    /// The proportional faces of rows that are not engine columns.
    body: Gd<Font>,
    body_bold: Gd<Font>,
    title: Gd<Font>,
    /// The check medallion, off and on; the letter badge.
    mark_off: Gd<StyleBoxFlat>,
    mark_on: Gd<StyleBoxFlat>,
    badge: Gd<StyleBoxFlat>,
}

/// Row text in the body face.
const BODY_SIZE: i32 = 17;

impl Look {
    fn new() -> Look {
        let flat = |bg: Color, border: Option<Color>| {
            let mut sb = StyleBoxFlat::new_gd();
            sb.set_bg_color(bg);
            sb.set_corner_radius_all(3);
            sb.set_content_margin_all(0.0);
            if let Some(b) = border {
                sb.set_border_width_all(1);
                sb.set_border_color(b);
            }
            sb
        };
        let accent = |a: f32| Color { a, ..theme::ACCENT };
        let gold = |a: f32| Color { a, ..theme::GOLD };
        // a selected row: a gold bar on its left edge
        let picked = |bg: Color, border: Option<Color>| {
            let mut sb = flat(bg, border);
            sb.set_border_width(godot::builtin::Side::LEFT, 3);
            if border.is_none() {
                sb.set_border_color(theme::GOLD_BRIGHT);
            }
            sb
        };
        let rows = [
            (flat(gold(0.0), None), flat(gold(0.12), None)),
            (picked(accent(0.13), None), picked(accent(0.2), None)),
            (
                flat(gold(0.1), Some(accent(0.9))),
                flat(gold(0.16), Some(accent(0.9))),
            ),
            (
                picked(accent(0.18), Some(theme::ACCENT)),
                picked(accent(0.26), Some(theme::ACCENT)),
            ),
        ];
        let medallion = |bg: Color, border: Color| {
            let mut sb = StyleBoxFlat::new_gd();
            sb.set_bg_color(bg);
            sb.set_border_width_all(1);
            sb.set_border_color(border);
            sb.set_corner_radius_all(4);
            sb.set_content_margin_all(0.0);
            sb
        };
        let mut badge = medallion(Color::from_rgba(0.043, 0.035, 0.031, 0.9), theme::GOLD_DIM);
        badge.set_corner_radius_all(3);
        let (char_w, line_h) = theme::mono_metrics();
        Look {
            bold: theme::font(Face::MonoBold),
            italic: theme::font(Face::MonoItalic),
            char_w,
            line_h,
            rows,
            no_focus: StyleBoxEmpty::new_gd(),
            default_button: theme::default_button_style(),
            focus_button: {
                let mut sb = StyleBoxFlat::new_gd();
                sb.set_bg_color(Color::from_rgba(0.3, 0.22, 0.09, 1.0));
                sb.set_border_width_all(2);
                sb.set_border_color(theme::GOLD_BRIGHT);
                sb.set_corner_radius_all(3);
                sb.set_shadow_size(6);
                sb.set_shadow_color(Color::from_rgba(0.906, 0.761, 0.478, 0.35));
                sb.set_content_margin_all(6.0);
                sb
            },
            body: theme::font(Face::Body),
            body_bold: theme::font(Face::BodyBold),
            title: theme::font(Face::Title),
            mark_off: medallion(theme::SOCKET, theme::GOLD_DIM),
            mark_on: medallion(Color::from_rgba(0.3, 0.22, 0.09, 1.0), theme::GOLD_BRIGHT),
            badge,
        }
    }

    /// Pixels `text` takes in `font` at `size`.
    fn text_w(&self, text: &str, font: &Gd<Font>, size: i32) -> f32 {
        font.get_string_size_ex(text).font_size(size).done().x
    }

    /// Style a list row for its state.
    fn style_row(&self, button: &mut Gd<Button>, selected: bool, current: bool) {
        let (normal, hover) = &self.rows[usize::from(selected) + 2 * usize::from(current)];
        for (name, sb) in [
            ("normal", normal),
            ("hover", hover),
            ("pressed", hover),
            ("hover_pressed", hover),
            ("disabled", normal),
        ] {
            button.add_theme_stylebox_override(name, sb);
        }
    }

    /// Pixels for `n` characters of the monospace font.
    fn chars(&self, n: usize) -> f32 {
        n as f32 * self.char_w
    }
}

/// One column of a list row: a label in `color`; `width` in pixels, or None
/// to take the rest of the row (cut with an ellipsis).
fn cell(text: &str, color: Color, font: Option<&Gd<Font>>, width: Option<f32>) -> Gd<Label> {
    let mut l = theme::label(text);
    l.add_theme_color_override("font_color", color);
    if let Some(f) = font {
        l.add_theme_font_override("font", f);
    }
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    match width {
        Some(w) => l.set_custom_minimum_size(Vector2::new(w, 0.0)),
        None => {
            l.set_h_size_flags(SizeFlags::EXPAND_FILL);
            l.set_clip_text(true);
            l.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        }
    }
    l
}

/// A scroll container the dialog pages and scrolls itself.
struct Scroller {
    scroll: Gd<ScrollContainer>,
    /// Visible height.
    view_h: f32,
}

impl Scroller {
    fn new(width: f32, view_h: f32, sideways: bool) -> Scroller {
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_focus_mode(FocusMode::NONE);
        scroll.set_horizontal_scroll_mode(if sideways {
            ScrollMode::AUTO
        } else {
            ScrollMode::DISABLED
        });
        scroll.set_custom_minimum_size(Vector2::new(width, view_h));
        Scroller { scroll, view_h }
    }

    fn top(&self) -> f32 {
        self.scroll.get_v_scroll() as f32
    }

    /// Scroll so `v` is at the top. Also once more at the end of the frame:
    /// right after rows change the container has not measured them yet and
    /// would clamp `v` to the old height.
    fn scroll_to(&mut self, v: f32) {
        let v = v.max(0.0).round() as i32;
        self.scroll.set_v_scroll(v);
        self.scroll.call_deferred("set_v_scroll", &[v.to_variant()]);
    }

    fn scroll_by(&mut self, dy: f32) {
        let v = self.top() + dy;
        self.scroll_to(v);
    }

    fn page(&mut self, pages: f32) {
        let page = (self.view_h - ROW_H).max(ROW_H);
        self.scroll_by(pages * page);
    }

    fn sideways(&mut self, dx: f32) {
        let h = (self.scroll.get_h_scroll() as f32 + dx).max(0.0).round() as i32;
        self.scroll.set_h_scroll(h);
    }

    /// Bring the band `top..top + h` into view.
    fn reveal(&mut self, top: f32, h: f32) {
        let v = self.top();
        if top < v {
            self.scroll_to(top);
        } else if top + h > v + self.view_h {
            self.scroll_to(top + h - self.view_h);
        }
    }

    /// Arrows, paging, Home/End; `line` is one arrow step. False: not a
    /// scrolling key.
    fn key(&mut self, key: Key, line: f32) -> bool {
        match key {
            Key::Up => self.scroll_by(-line),
            Key::Down => self.scroll_by(line),
            Key::Left => self.sideways(-4.0 * line),
            Key::Right => self.sideways(4.0 * line),
            Key::PageUp => self.page(-1.0),
            Key::PageDown => self.page(1.0),
            Key::Home => self.scroll_to(0.0),
            Key::End => self.scroll_to(1.0e9),
            _ => return false,
        }
        true
    }
}

enum MenuRow {
    Item {
        button: Gd<Button>,
        mark: Option<Gd<Label>>,
        text: Gd<Label>,
    },
    Fixed,
}

/// An open menu: NetHack's rules live in `MenuState`; this adds the rows,
/// the keyboard cursor and scrolling.
struct MenuView {
    state: MenuState,
    rows: Vec<MenuRow>,
    /// Top of each row in the list.
    tops: Vec<f32>,
    /// The furthest the list scrolls: a row top, thanks to a filler below
    /// the last row, so every page shows whole rows.
    max_top: f32,
    /// Each item's own colour (menucolors), CLR_*.
    colors: Vec<i32>,
    list: Scroller,
    /// The row the keyboard is on: moved by the arrows, and set to the row
    /// a letter or a click toggled last.
    cursor: Option<usize>,
    /// The arrows moved the cursor: Space toggles it (and Enter picks it
    /// in pick-one) instead of confirming the menu.
    armed: bool,
    status: Gd<Label>,
    hint: Gd<Label>,
}

impl MenuView {
    fn refresh(&mut self, look: &Look) {
        let any = self.state.how == PickHow::Any;
        for (i, row) in self.rows.iter_mut().enumerate() {
            let MenuRow::Item { button, mark, text } = row else {
                continue;
            };
            let e = &self.state.entries[i];
            look.style_row(button, e.selected, self.cursor == Some(i));
            if let Some(m) = mark {
                m.set_text(&mark_text(e));
                let sb = if e.selected {
                    &look.mark_on
                } else {
                    &look.mark_off
                };
                m.add_theme_stylebox_override("normal", sb);
            }
            let color = match self.colors.get(i).copied() {
                _ if e.selected => Color::from_rgb(1.0, 1.0, 1.0),
                Some(c) if (0..16).contains(&c) && c != NO_COLOR => nh_color(c),
                _ => theme::TEXT,
            };
            text.add_theme_color_override("font_color", color);
        }
        let mut status = Vec::new();
        if let Some(n) = self.state.typed_count() {
            status.push(tr!("dlg-count", n = n));
        }
        if any {
            let n = self.state.entries.iter().filter(|e| e.selected).count();
            status.push(tr!("dlg-selected", n = n));
        }
        self.status.set_text(&status.join("   ·   "));
        self.status.set_visible(!status.is_empty());
        let pickable = self.state.entries.iter().any(|e| e.selectable);
        let hint = if !pickable {
            tr!("dlg-hint-read")
        } else if any {
            // Space confirms, as in NetHack, until the arrows mark a row
            let space = if self.armed {
                tr!("dlg-space-toggle")
            } else {
                tr!("dlg-space-then")
            };
            tr!("dlg-hint-any", space = space)
        } else {
            tr!("dlg-hint-one")
        };
        self.hint.set_text(&i18n::whole_parts(&hint));
    }

    /// Scroll by pages, to the top of a row so none is cut at the top.
    fn page(&mut self, pages: f32) {
        let page = (self.list.view_h - ROW_H).max(ROW_H);
        let target = (self.list.top() + pages * page).clamp(0.0, self.max_top);
        let snapped = self
            .tops
            .iter()
            .copied()
            .rfind(|&t| t <= target + 0.5)
            .unwrap_or(0.0);
        self.list.scroll_to(snapped);
    }

    /// Is row `i` wholly in view?
    fn in_view(&self, i: usize) -> bool {
        let (v, h) = (self.list.top(), self.list.view_h);
        self.tops
            .get(i)
            .is_some_and(|&t| t >= v - 0.5 && t + ROW_H <= v + h + 0.5)
    }

    /// Scrolling away from the keyboard row lets it go: the next arrow
    /// starts from the rows in view, and Space does not toggle a row the
    /// player cannot see.
    fn drop_hidden_cursor(&mut self) {
        if self.cursor.is_some_and(|c| !self.in_view(c)) {
            self.cursor = None;
            self.armed = false;
        }
    }

    /// Scroll the least that shows row `i` whole, to a row top.
    fn reveal(&mut self, i: usize) {
        let Some(&top) = self.tops.get(i) else {
            return;
        };
        let (v, h) = (self.list.top(), self.list.view_h);
        if top < v {
            self.list.scroll_to(top);
        } else if top + ROW_H > v + h {
            let least = top + ROW_H - h;
            let t = self.tops.iter().copied().find(|&t| t >= least - 0.5);
            self.list.scroll_to(t.unwrap_or(top).min(self.max_top));
        }
    }

    /// Move the cursor to the next selectable row up (`-1`) or down (`1`);
    /// from nothing (or from a row scrolled out of view), start at the
    /// first row in view.
    fn step_cursor(&mut self, dir: i32) {
        let items: Vec<usize> = (0..self.state.entries.len())
            .filter(|&i| self.state.entries[i].selectable)
            .collect();
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return;
        };
        let cursor = self.cursor.filter(|&c| self.in_view(c));
        let next = match cursor {
            Some(c) if dir > 0 => items.iter().copied().find(|&i| i > c).unwrap_or(last),
            Some(c) => items
                .iter()
                .rev()
                .copied()
                .find(|&i| i < c)
                .unwrap_or(first),
            None => {
                let in_view = |&i: &usize| self.in_view(i);
                if dir > 0 {
                    items.iter().copied().find(in_view).unwrap_or(first)
                } else {
                    items.iter().rev().copied().find(in_view).unwrap_or(last)
                }
            }
        };
        self.cursor = Some(next);
        self.armed = true;
        self.reveal(next);
    }

    /// Toggle (pick-any) or pick (pick-one) row `i`, as a click does.
    fn click(&mut self, i: usize, look: &Look) -> Option<Reply> {
        match self.state.click(i) {
            MenuOutcome::Done(reply) => Some(reply),
            _ => {
                if self.state.entries.get(i).is_some_and(|e| e.selectable) {
                    self.cursor = Some(i);
                }
                self.refresh(look);
                None
            }
        }
    }

    fn key(&mut self, input: &KeyInput, look: &Look) -> Option<Reply> {
        match nav(input) {
            Some(k @ (Key::Up | Key::Down)) => {
                self.step_cursor(if k == Key::Up { -1 } else { 1 });
                self.refresh(look);
                return None;
            }
            Some(k @ (Key::PageUp | Key::PageDown)) => {
                self.page(if k == Key::PageUp { -1.0 } else { 1.0 });
                self.drop_hidden_cursor();
                self.refresh(look);
                return None;
            }
            Some(k) => {
                self.list.key(k, ROW_H);
                self.drop_hidden_cursor();
                self.refresh(look);
                return None;
            }
            None => {}
        }
        let c = typed(input)?;
        if self.armed
            && let Some(i) = self.cursor
            && (c == ' ' || (c == '\n' && self.state.how == PickHow::One))
        {
            // the wheel may have scrolled it away: show what was toggled
            self.reveal(i);
            return self.click(i, look);
        }
        let before: Vec<(bool, Option<i64>)> = self
            .state
            .entries
            .iter()
            .map(|e| (e.selected, e.count))
            .collect();
        match self.state.key(c) {
            MenuOutcome::Done(reply) => return Some(reply),
            MenuOutcome::PageUp => {
                self.page(-1.0);
                self.drop_hidden_cursor();
            }
            MenuOutcome::PageDown => {
                self.page(1.0);
                self.drop_hidden_cursor();
            }
            MenuOutcome::Pending => {
                let changed: Vec<usize> = self
                    .state
                    .entries
                    .iter()
                    .zip(&before)
                    .enumerate()
                    .filter(|(_, (e, b))| (e.selected, e.count) != **b)
                    .map(|(i, _)| i)
                    .collect();
                // one row toggled by its letter: the cursor follows it
                // (bulk commands leave the view alone)
                if let [i] = changed[..] {
                    self.cursor = Some(i);
                    self.armed = false;
                    self.reveal(i);
                }
            }
        }
        self.refresh(look);
        None
    }
}

/// One palette row; rows are slots showing the filtered commands in order.
#[derive(Clone)]
struct PaletteSlot {
    button: Gd<Button>,
    name: Gd<Label>,
    key: Gd<Label>,
    desc: Gd<Label>,
}

struct Palette {
    cmds: Vec<PaletteCmd>,
    /// Indexes into `cmds`, in the order shown.
    shown: Vec<usize>,
    /// Position in `shown` that Enter runs.
    selected: Option<usize>,
    slots: Vec<PaletteSlot>,
    /// The slot styled as the highlighted one: restyling every row is a
    /// theme change each, too slow to do on every keystroke.
    styled: Option<usize>,
    list: Scroller,
    none: Gd<Label>,
    look: Look,
}

impl Palette {
    fn refilter(&mut self, text: &str) {
        self.shown = palette_filter(&self.cmds, text);
        self.selected = (!text.trim().is_empty() && !self.shown.is_empty()).then_some(0);
    }
}

/// Show the palette's rows. The borrow is released before Godot is called.
fn fill_palette(palette: &Rc<RefCell<Palette>>) {
    let (rows, selected, styled, slots, look, mut none) = {
        let p = palette.borrow();
        let rows: Vec<(String, String, String)> = p
            .shown
            .iter()
            .map(|&i| {
                let c = &p.cmds[i];
                (c.name.clone(), key_name(c.key), c.desc.clone())
            })
            .collect();
        (
            rows,
            p.selected,
            p.styled,
            p.slots.clone(),
            p.look.clone(),
            p.none.clone(),
        )
    };
    for (i, mut slot) in slots.into_iter().enumerate() {
        let Some((name, key, desc)) = rows.get(i) else {
            slot.button.set_visible(false);
            continue;
        };
        slot.name.set_text(name);
        slot.key.set_text(key);
        slot.desc.set_text(desc);
        if (selected == Some(i)) != (styled == Some(i)) {
            look.style_row(&mut slot.button, false, selected == Some(i));
        }
        slot.button.set_visible(true);
    }
    none.set_visible(rows.is_empty());
    let mut p = palette.borrow_mut();
    p.styled = selected;
    match selected {
        Some(i) => p.list.reveal(i as f32 * ROW_H, ROW_H),
        None => p.list.scroll_to(0.0),
    }
}

/// One row of a picker of names: a class's symbol, the name as the
/// player reads it, the English the engine knows.
#[derive(Clone)]
struct PickSlot {
    button: Gd<Button>,
    symbol: Gd<Label>,
    name: Gd<Label>,
    english: Gd<Label>,
}

/// The wish's builder: the count, the blessing, the enchantment.
#[derive(Clone)]
struct WishControls {
    count: Gd<Label>,
    buc: Gd<Button>,
    ench: Gd<Label>,
    /// The wish as the player reads it, and the English the engine is sent.
    shown: Gd<Label>,
    english: Gd<Label>,
}

/// A picker of names (Russian input): what the engine asks the name of,
/// found by any form of its name in the player's language or in English;
/// the engine is sent the English.
struct Picker {
    ask: Ask,
    picks: Vec<Pick>,
    /// Indexes into `picks`, the best match first.
    shown: Vec<usize>,
    /// Position in `shown` that Enter picks.
    selected: Option<usize>,
    /// The search less what the wish's builder took from it.
    query: String,
    /// The class the list keeps to (None: every class), and the classes
    /// the class picker offers after "every class" (empty: no picker).
    class: Option<ObjClass>,
    classes: Vec<ObjClass>,
    class_option: Option<Gd<OptionButton>>,
    wish: Wish,
    wish_controls: Option<WishControls>,
    slots: Vec<PickSlot>,
    list: Scroller,
    more: Gd<Label>,
    look: Look,
}

impl Picker {
    /// The search as typed: a wish's count, blessing and enchantment go to
    /// the builder, the rest names the thing.
    fn retype(&mut self, text: &str) {
        if self.ask == Ask::Wish {
            let w = pickers::parse_wish(Lexicon::ru(), text);
            if let Some(c) = w.count {
                self.wish.count = c;
            }
            if let Some(b) = w.buc {
                self.wish.buc = b;
            }
            if let Some(e) = w.ench {
                self.wish.ench = Some(e);
            }
            self.query = w.rest;
        } else {
            self.query = text.to_string();
        }
        self.refilter();
    }

    fn refilter(&mut self) {
        self.shown = pickers::search(&self.picks, &self.query, self.class);
        self.selected = (!self.query.trim().is_empty() && !self.shown.is_empty()).then_some(0);
    }

    /// The pick Enter takes: the highlighted one.
    fn chosen(&self) -> Option<usize> {
        self.selected.and_then(|s| self.shown.get(s)).copied()
    }

    /// What the engine is sent for `picks[i]`.
    fn reply(&self, i: usize) -> String {
        let pick = &self.picks[i];
        match self.ask {
            Ask::Wish => self.wish.english(&pick.english),
            _ => pick.reply.clone(),
        }
    }

    /// The highlight a row up or down (a page: ten).
    fn step(&mut self, by: i64) {
        let n = self.shown.len().min(self.slots.len()) as i64;
        if n == 0 {
            return;
        }
        let cur = self.selected.map_or(-1, |s| s as i64);
        let next = if cur < 0 && by < 0 {
            n - 1
        } else {
            (cur + by).clamp(0, n - 1)
        };
        self.selected = Some(next as usize);
    }

    /// The next class (`by` 1) or the previous one, round through "every
    /// class".
    fn turn_class(&mut self, by: i32) {
        if self.classes.is_empty() {
            return;
        }
        let n = self.classes.len() as i32 + 1;
        let now = self
            .class
            .and_then(|c| self.classes.iter().position(|x| *x == c))
            .map_or(0, |i| i as i32 + 1);
        let next = (now + by).rem_euclid(n);
        self.class = (next > 0).then(|| self.classes[next as usize - 1]);
        if let Some(mut o) = self.class_option.clone() {
            o.select(next);
        }
        self.refilter();
    }

    /// The wish's count (by 1), blessing (round) or enchantment (by 1).
    fn adjust(&mut self, count: i32, buc: bool, ench: i32) {
        let c = self.wish.count as i32 + count;
        self.wish.count = c.clamp(1, pickers::MAX_COUNT as i32) as u32;
        if buc {
            self.wish.buc = self.wish.buc.next();
        }
        if ench != 0 {
            let e = self.wish.ench.unwrap_or(0) + ench;
            self.wish.ench = Some(e.clamp(-pickers::MAX_ENCH, pickers::MAX_ENCH));
        }
    }
}

/// Show the picker's rows, the highlight, the wish. The borrow is released
/// before Godot is called.
fn fill_picker(picker: &Rc<RefCell<Picker>>) {
    let (rows, selected, more, slots, wish, look) = {
        let p = picker.borrow();
        let rows: Vec<(String, String, String)> = p
            .shown
            .iter()
            .take(p.slots.len())
            .map(|&i| {
                let pick = &p.picks[i];
                let symbol = pick.symbol.map_or(String::new(), |c| c.to_string());
                (symbol, pick.shown.clone(), pick.english.clone())
            })
            .collect();
        let more = if p.shown.is_empty() {
            Some(tr!("picker-none"))
        } else if p.shown.len() > p.slots.len() {
            Some(tr!("picker-more", n = p.shown.len() - p.slots.len()))
        } else {
            None
        };
        // the wish: what the highlighted (else the first) thing makes
        let wish = p.wish_controls.clone().map(|c| {
            let target = p.chosen().or_else(|| p.shown.first().copied());
            let english = target.map(|i| p.reply(i));
            (c, p.wish, english)
        });
        (
            rows,
            p.selected,
            more,
            p.slots.clone(),
            wish,
            p.look.clone(),
        )
    };
    for (i, mut slot) in slots.into_iter().enumerate() {
        let Some((symbol, name, english)) = rows.get(i) else {
            slot.button.set_visible(false);
            continue;
        };
        slot.symbol.set_text(symbol);
        slot.name.set_text(name);
        slot.english.set_text(english);
        look.style_row(&mut slot.button, false, selected == Some(i));
        slot.button.set_visible(true);
    }
    {
        let mut p = picker.borrow_mut();
        match &more {
            Some(t) => p.more.set_text(t),
            None => p.more.set_text(""),
        }
        p.more.set_visible(more.is_some());
        match selected {
            Some(i) => p.list.reveal(i as f32 * ROW_H, ROW_H),
            None => p.list.scroll_to(0.0),
        }
    }
    if let Some((mut c, wish, english)) = wish {
        c.count.set_text(&wish.count.to_string());
        c.buc.set_text(&i18n::tr(wish.buc.key()));
        c.ench
            .set_text(&wish.ench.map_or("0".to_string(), |e| format!("{e:+}")));
        match english {
            Some(e) => {
                let shown = pickers::wish_shown(Lexicon::ru(), &e);
                c.shown.set_text(&tr!("wish-shown", wish = shown));
                c.english.set_text(&e);
            }
            None => {
                c.shown.set_text(&tr!("wish-pick"));
                c.english.set_text("");
            }
        }
    }
}

enum Kind {
    Menu(Box<MenuView>),
    Choice {
        allowed: Vec<char>,
        default: Option<char>,
        /// The answers' buttons, and the one the d-pad is on.
        buttons: Vec<(char, Gd<Button>)>,
        focus: Option<usize>,
    },
    Text {
        edit: Gd<LineEdit>,
        /// The on-screen keyboard (a gamepad).
        osk: Option<Osk>,
        /// An engraving: it goes to the engine in Latin letters.
        engrave: bool,
    },
    /// A name the engine asks for, picked in the player's language.
    Picker {
        edit: Gd<LineEdit>,
        picker: Rc<RefCell<Picker>>,
    },
    ExtCmd {
        edit: Gd<LineEdit>,
        palette: Rc<RefCell<Palette>>,
        /// The request its rows and buttons answer (the palette is reused).
        req: Rc<std::cell::Cell<u64>>,
    },
    Show {
        text: Scroller,
    },
    MessageMenu {
        letter: char,
        pick: bool,
    },
}

struct Open {
    req: u64,
    prompt: Prompt,
    kind: Kind,
    shade: Gd<ColorRect>,
    panel: Gd<PanelContainer>,
    /// Room at the bottom of the panel for a gamepad's hints.
    pad_room: Option<Gd<Control>>,
}

pub struct Dialogs {
    root: Gd<Control>,
    queue: UiQueue,
    look: Look,
    open: Option<Open>,
    /// A gamepad gave the last input: dialogs open with a focus, a text
    /// field with the on-screen keyboard.
    pad: bool,
    /// Dialogs of every kind built behind the title screen one a frame,
    /// and the frames each has been drawn (the warm-up); the next to build.
    warm: Vec<(Open, u32)>,
    warm_next: usize,
    /// A dialog of the warm-up is being built: its text field takes no
    /// keyboard focus (the system's text input would start up for it).
    warming: bool,
    /// The command palette, built once (about a hundred rows: tens of
    /// milliseconds) and hidden between uses.
    palette_pool: Option<PalettePool>,
}

struct PalettePool {
    shade: Gd<ColorRect>,
    panel: Gd<PanelContainer>,
    edit: Gd<LineEdit>,
    palette: Rc<RefCell<Palette>>,
    req: Rc<std::cell::Cell<u64>>,
}

impl Dialogs {
    /// The open dialog's panel on screen (self-tests).
    pub fn panel_rect(&self) -> Option<Rect2> {
        self.open.as_ref().map(|o| o.panel.get_global_rect())
    }

    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Dialogs {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dialog_theme());
        layer.add_child(&root);
        Dialogs {
            root,
            queue,
            look: Look::new(),
            open: None,
            pad: false,
            warm: Vec::new(),
            warm_next: 0,
            warming: false,
            palette_pool: None,
        }
    }

    /// The visible area in pixels. Headless Godot reports a square viewport
    /// as tall as it is wide; dialogs there are laid out for the window the
    /// project opens, so self-tests see the same scrolling as a player.
    fn screen(&self) -> Vector2 {
        let s = self.root.get_viewport_rect().size;
        let headless = DisplayServer::singleton().get_name() == "headless";
        if headless || s.x < 320.0 || s.y < 240.0 {
            REFERENCE_SCREEN
        } else {
            s
        }
    }

    /// A width for `chars` characters of content plus `extra` pixels, kept
    /// between `min` and the screen less its margins.
    fn fit_width(&self, chars: usize, extra: f32, min: f32) -> f32 {
        let max = (self.screen().x - 2.0 * SIDE_MARGIN).max(min);
        (self.look.chars(chars) + extra).clamp(min, max)
    }

    /// A centred panel with a column; `width` is the content's width.
    fn frame(
        &mut self,
        width: f32,
        title: Option<&str>,
    ) -> (Gd<ColorRect>, Gd<PanelContainer>, Gd<VBoxContainer>) {
        self.frame_at(width, title, Place::Centre)
    }

    /// A panel with a column where `place` says; a short question sits at
    /// the top, under the prompt line, and leaves the map around the hero
    /// (the monster it is about) in sight and undimmed.
    fn frame_at(
        &mut self,
        width: f32,
        title: Option<&str>,
        place_at: Place,
    ) -> (Gd<ColorRect>, Gd<PanelContainer>, Gd<VBoxContainer>) {
        let mut shade = ColorRect::new_alloc();
        let dim = match place_at {
            Place::Centre => 0.45,
            Place::Top => 0.0,
        };
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, dim));
        theme::full_rect_ignore(&shade);
        self.root.add_child(&shade);
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::STOP);
        theme::apply_frame(&panel, Frame::Panel);
        match place_at {
            Place::Centre => {
                place(
                    &panel,
                    [0.5, 0.5, 0.5, 0.5],
                    [-width / 2.0, -40.0, width / 2.0, 40.0],
                );
                panel.set_v_grow_direction(GrowDirection::BOTH);
            }
            Place::Top => {
                // centred under the prompt banner
                let x = 0.0;
                place(
                    &panel,
                    [0.5, 0.0, 0.5, 0.0],
                    [x - width / 2.0, TOP_Y, x + width / 2.0, TOP_Y],
                );
                panel.set_v_grow_direction(GrowDirection::END);
            }
        }
        panel.set_h_grow_direction(GrowDirection::BOTH);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
            // the engine's own words: its prompts and menu titles
            let mut l = theme::styled_label(t, Face::BodyBold, 20, theme::GOLD_BRIGHT);
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            l.set_custom_minimum_size(Vector2::new(width, 0.0));
            col.add_child(&l);
        }
        panel.add_child(&col);
        self.root.add_child(&panel);
        (shade, panel, col)
    }

    fn buttons(&self, col: &mut Gd<VBoxContainer>, items: &[(String, UiEvent)]) -> Vec<Gd<Button>> {
        let mut row = HBoxContainer::new_alloc();
        row.set_alignment(AlignmentMode::CENTER);
        row.add_theme_constant_override("separation", 12);
        let mut out = Vec::new();
        for (text, ev) in items {
            let mut b = theme::button(text, &self.queue, ev.clone());
            b.set_custom_minimum_size(Vector2::new(112.0, 36.0));
            row.add_child(&b);
            out.push(b);
        }
        col.add_child(&row);
        out
    }

    fn hint(&self, col: &mut Gd<VBoxContainer>, text: &str, width: f32) -> Gd<Label> {
        let text = i18n::whole_parts(text);
        let mut hint = theme::styled_label(&text, Face::Body, 15, theme::TEXT_DIM);
        hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        hint.set_custom_minimum_size(Vector2::new(width, 0.0));
        col.add_child(&hint);
        hint
    }

    /// A list row: a flat button over `cells` that queues `MenuClick(index)`.
    fn row_button<T: Inherits<Control>>(
        &self,
        req: u64,
        index: usize,
        cells: &[Gd<T>],
    ) -> Gd<Button> {
        self.row_button_at(Rc::new(std::cell::Cell::new(req)), index, cells)
    }

    /// A list row whose request is read when it is pressed (the palette,
    /// reused across requests).
    fn row_button_at<T: Inherits<Control>>(
        &self,
        req: Rc<std::cell::Cell<u64>>,
        index: usize,
        cells: &[Gd<T>],
    ) -> Gd<Button> {
        let mut b = Button::new_alloc();
        b.set_focus_mode(FocusMode::NONE);
        b.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
        b.add_theme_stylebox_override("focus", &self.look.no_focus);
        self.look.style_row(&mut b, false, false);
        let mut hbox = HBoxContainer::new_alloc();
        hbox.set_mouse_filter(MouseFilter::IGNORE);
        hbox.add_theme_constant_override("separation", 10);
        for c in cells {
            hbox.add_child(&c.clone().upcast::<Control>());
        }
        place(&hbox, [0.0, 0.0, 1.0, 1.0], [8.0, 0.0, -8.0, 0.0]);
        b.add_child(&hbox);
        let q = self.queue.clone();
        b.signals().pressed().connect(move || {
            let ev = DialogEvent::MenuClick(index);
            push(&q, UiEvent::Dialog { req: req.get(), ev });
        });
        b
    }

    fn line_edit(&self, req: u64) -> Gd<LineEdit> {
        let mut edit = LineEdit::new_alloc();
        edit.set_max_length(MAX_TEXT_BYTES as i32);
        edit.set_keep_editing_on_text_submit(true);
        let q = self.queue.clone();
        edit.signals()
            .text_submitted()
            .connect(move |text: GString| {
                let ev = DialogEvent::TextSubmitted(text.to_string());
                push(&q, UiEvent::Dialog { req, ev });
            });
        edit
    }

    fn open_menu(
        &mut self,
        req: u64,
        how: PickHow,
        title: Option<&str>,
        items: &[nh_protocol::MenuItem],
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let state = MenuState::new(how, title.map(str::to_string), items);
        let any = how == PickHow::Any;
        let has_glyphs = state.entries.iter().any(|e| glyph_char(e).is_some());
        let look = self.look.clone();
        // the engine's columns: the widest of each, over every row with more
        // than one
        let split: Vec<Vec<String>> = state.entries.iter().map(|e| fields(&e.text)).collect();
        let mut col_w: Vec<f32> = Vec::new();
        for (e, f) in state.entries.iter().zip(&split) {
            if f.len() < 2 || row_kind(e) == RowKind::Spacer {
                continue;
            }
            for (j, t) in f.iter().enumerate() {
                let w = look.text_w(t, &look.body, BODY_SIZE) + 18.0;
                if col_w.len() <= j {
                    col_w.push(w);
                } else {
                    col_w[j] = col_w[j].max(w);
                }
            }
        }
        let (mark_w, letter_w, glyph_w) = (
            if any { 22.0 } else { 0.0 },
            24.0,
            if has_glyphs { 24.0 } else { 0.0 },
        );
        let text_w = state
            .entries
            .iter()
            .zip(&split)
            .map(|(e, f)| {
                if f.len() >= 2 {
                    return col_w.iter().take(f.len()).sum::<f32>();
                }
                let font = if row_kind(e) == RowKind::Header {
                    &look.title
                } else {
                    &look.body
                };
                look.text_w(&e.text, font, BODY_SIZE + 2)
            })
            .fold(0.0f32, f32::max);
        // the cells of a row laid out in columns, else one cell
        let columns = |f: &[String], color: Color, font: &Gd<Font>, size: i32| -> Vec<Gd<Label>> {
            f.iter()
                .enumerate()
                .map(|(j, t)| {
                    let last = j + 1 == f.len();
                    let w = (!last).then(|| col_w.get(j).copied().unwrap_or(0.0));
                    // an option's value: on, off, or what it is set to
                    let (t, color) = match option_value(t) {
                        Some(OptionValue::On) => ("✔".to_string(), theme::GOOD),
                        Some(OptionValue::Off) => ("·".to_string(), theme::TEXT_OFF),
                        Some(OptionValue::Set(v)) => (v, theme::GOLD_BRIGHT),
                        None => (t.clone(), color),
                    };
                    let mut l = cell(&t, color, Some(font), w);
                    l.add_theme_font_size_override("font_size", size);
                    l
                })
                .collect()
        };
        // non-item rows start where the items' text does
        let lead = mark_w
            + letter_w
            + glyph_w
            + 10.0 * (1.0 + f32::from(u8::from(any)) + f32::from(u8::from(has_glyphs)));
        let title_w = title.map_or(0.0, |t| look.text_w(t, &look.body_bold, 20).min(760.0));
        // row padding, column gaps, the scroll bar
        let chrome = mark_w + letter_w + glyph_w + 16.0 + 32.0 + 20.0;
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(420.0);
        let width = (text_w + chrome).max(title_w).clamp(420.0, max_w);
        let mut tops = Vec::with_capacity(state.entries.len());
        let mut y = 0.0;
        for e in &state.entries {
            tops.push(y);
            y += if row_kind(e) == RowKind::Spacer {
                SPACER_H
            } else {
                ROW_H
            };
        }
        // a long list scrolls in a panel of 18 rows rather than filling the screen
        let max_h = (self.screen().y - LIST_CHROME).clamp(ROW_H * 4.0, MENU_ROWS * ROW_H);
        let view_h = y.min(max_h).max(ROW_H);
        let list = Scroller::new(width, view_h, false);
        // Scrolled to the end, the first row in view is whole: the list
        // ends in a filler so the furthest scroll is a row top.
        let max_top = if y > view_h {
            tops.iter()
                .copied()
                .find(|&t| t >= y - view_h)
                .unwrap_or(y - view_h)
        } else {
            0.0
        };
        let filler_h = (max_top + view_h - y).max(0.0);

        let (shade, panel, mut col) = self.frame(width, title);
        let mut rows_box = VBoxContainer::new_alloc();
        rows_box.add_theme_constant_override("separation", 0);
        rows_box.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = Vec::with_capacity(state.entries.len());
        for (i, e) in state.entries.iter().enumerate() {
            let kind = row_kind(e);
            if kind != RowKind::Item {
                let mut row: Gd<Control> = match kind {
                    RowKind::Spacer => Control::new_alloc(),
                    RowKind::Header => {
                        let color = match items.get(i).map(|it| it.clr) {
                            Some(c) if (0..16).contains(&c) && c != NO_COLOR => nh_color(c),
                            _ => theme::ACCENT,
                        };
                        let mut l = cell(e.text.trim(), color, Some(&look.title), None);
                        l.add_theme_font_size_override("font_size", BODY_SIZE + 3);
                        l.upcast()
                    }
                    // column headings line up with the columns below
                    _ if split[i].len() >= 2 => {
                        let mut hbox = HBoxContainer::new_alloc();
                        hbox.add_theme_constant_override("separation", 0);
                        let mut pad = Control::new_alloc();
                        pad.set_custom_minimum_size(Vector2::new(lead + 8.0, 0.0));
                        hbox.add_child(&pad);
                        for l in columns(&split[i], INFO_TEXT, &look.body_bold, BODY_SIZE - 1) {
                            hbox.add_child(&l);
                        }
                        hbox.upcast()
                    }
                    _ => {
                        let mut l = cell(&e.text, INFO_TEXT, Some(&look.body), None);
                        l.add_theme_font_size_override("font_size", BODY_SIZE - 1);
                        l.upcast()
                    }
                };
                let h = if kind == RowKind::Spacer {
                    SPACER_H
                } else {
                    ROW_H
                };
                row.set_custom_minimum_size(Vector2::new(0.0, h));
                row.set_mouse_filter(MouseFilter::IGNORE);
                rows_box.add_child(&row);
                rows.push(MenuRow::Fixed);
                continue;
            }
            let mut cells: Vec<Gd<Control>> = Vec::new();
            // a check medallion, the letter on a badge, the item's icon
            let mark = any.then(|| {
                let mut m = cell("", theme::GOLD_BRIGHT, Some(&look.body_bold), Some(mark_w));
                m.add_theme_font_size_override("font_size", 13);
                m.set_horizontal_alignment(HorizontalAlignment::CENTER);
                m.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                m.set_custom_minimum_size(Vector2::new(mark_w, mark_w));
                m.add_theme_stylebox_override("normal", &look.mark_off);
                m
            });
            if let Some(m) = &mark {
                cells.push(m.clone().upcast());
            }
            let mut letter = cell(
                &e.letter.map_or(String::new(), |l| l.to_string()),
                theme::GOLD_BRIGHT,
                Some(&look.body_bold),
                Some(letter_w),
            );
            letter.add_theme_font_size_override("font_size", 15);
            letter.set_horizontal_alignment(HorizontalAlignment::CENTER);
            letter.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            letter.set_custom_minimum_size(Vector2::new(letter_w, 22.0));
            if e.letter.is_some() {
                letter.add_theme_stylebox_override("normal", &look.badge);
            }
            cells.push(letter.upcast());
            if has_glyphs {
                match &e.glyph {
                    // an object: its appearance's icon
                    Some(g) if g.kind == nh_protocol::GlyphKind::Obj => {
                        let class = glyph_char(e).unwrap_or('?');
                        let mut t = TextureRect::new_alloc();
                        t.set_mouse_filter(MouseFilter::IGNORE);
                        t.set_expand_mode(ExpandMode::IGNORE_SIZE);
                        t.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
                        t.set_custom_minimum_size(Vector2::new(glyph_w, glyph_w));
                        t.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                        t.set_texture(&crate::icons::item_icon(g.tile, class));
                        cells.push(t.upcast());
                    }
                    g => {
                        let (sym, color) = match (glyph_char(e), g) {
                            (Some(c), Some(g)) => (c.to_string(), nh_color(g.color)),
                            _ => (String::new(), theme::TEXT),
                        };
                        let mut l = cell(&sym, color, Some(&look.bold), Some(glyph_w));
                        l.set_horizontal_alignment(HorizontalAlignment::CENTER);
                        cells.push(l.upcast());
                    }
                }
            }
            let font = if e.attr & 0x0f == 1 {
                &look.body_bold
            } else {
                &look.body
            };
            let mut texts = if split[i].len() >= 2 {
                columns(&split[i], theme::TEXT, font, BODY_SIZE)
            } else {
                let mut l = cell(&e.text, theme::TEXT, Some(font), None);
                l.add_theme_font_size_override("font_size", BODY_SIZE);
                vec![l]
            };
            if e.skipinvert {
                for t in texts.iter_mut() {
                    t.add_theme_color_override("font_color", INFO_TEXT);
                }
            }
            let text = texts[0].clone();
            cells.extend(texts.into_iter().map(|t| t.upcast::<Control>()));
            let button = self.row_button(req, i, &cells);
            rows_box.add_child(&button);
            rows.push(MenuRow::Item { button, mark, text });
        }
        if filler_h > 0.0 {
            let mut filler = Control::new_alloc();
            filler.set_custom_minimum_size(Vector2::new(0.0, filler_h));
            filler.set_mouse_filter(MouseFilter::IGNORE);
            rows_box.add_child(&filler);
        }
        let mut scroll = list.scroll.clone();
        scroll.add_child(&rows_box);
        col.add_child(&scroll);

        let mut status = theme::label("");
        status.add_theme_color_override("font_color", theme::ACCENT);
        status.add_theme_font_override("font", &look.body_bold);
        col.add_child(&status);
        let hint = self.hint(&mut col, "", width);
        self.buttons(
            &mut col,
            &[
                (tr!("dlg-ok"), dialog_ui(req, DialogEvent::MenuConfirm)),
                (tr!("dlg-cancel"), dialog_ui(req, DialogEvent::MenuCancel)),
            ],
        );
        let colors = items.iter().map(|i| i.clr).collect();
        let mut view = MenuView {
            state,
            rows,
            tops,
            max_top,
            colors,
            list,
            cursor: None,
            armed: false,
            status,
            hint,
        };
        // a preselected item: show it
        if let Some(i) = view.state.entries.iter().position(|e| e.selected) {
            view.cursor = Some(i);
            view.reveal(i);
        }
        // a gamepad starts on the first entry, ready for A
        if self.pad
            && view.cursor.is_none()
            && let Some(i) = view.state.entries.iter().position(|e| e.selectable)
        {
            view.cursor = Some(i);
            view.armed = true;
            view.reveal(i);
        }
        view.refresh(&look);
        (Kind::Menu(Box::new(view)), shade, panel)
    }

    fn open_palette(
        &mut self,
        req: u64,
        catalog: Option<&Catalog>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let cmds = palette_cmds(catalog);
        if let Some(pool) = self
            .palette_pool
            .take()
            .filter(|p| p.palette.borrow().cmds.len() == cmds.len())
        {
            let PalettePool {
                mut shade,
                mut panel,
                mut edit,
                palette,
                req: pooled,
            } = pool;
            pooled.set(req);
            edit.set_text("");
            palette.borrow_mut().refilter("");
            fill_palette(&palette);
            shade.set_visible(true);
            panel.set_visible(true);
            if !self.warming {
                edit.call_deferred("grab_focus", &[]);
            }
            return (
                Kind::ExtCmd {
                    edit,
                    palette,
                    req: pooled,
                },
                shade,
                panel,
            );
        }
        let req_cell = Rc::new(std::cell::Cell::new(req));
        let look = self.look.clone();
        let name_w = cmds
            .iter()
            .map(|c| look.text_w(&c.name, &look.body_bold, BODY_SIZE))
            .fold(120.0f32, f32::max)
            + 8.0;
        let key_w = look.chars(4);
        let longest = cmds
            .iter()
            .map(|c| c.desc.chars().count())
            .max()
            .unwrap_or(0);
        // the descriptions in the body face, about 0.52 em a character
        let desc_w = longest as f32 * BODY_SIZE as f32 * 0.52;
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(480.0);
        let width = (desc_w + name_w + key_w + 16.0 + 30.0 + 20.0).clamp(480.0, max_w.min(820.0));
        let max_h = (self.screen().y - LIST_CHROME).max(ROW_H * 4.0);
        // a fixed height, so the dialog does not jump while filtering
        let view_h = (cmds.len() as f32 * ROW_H).clamp(ROW_H, max_h.min(PALETTE_ROWS * ROW_H));
        let list = Scroller::new(width, view_h, false);

        let (shade, panel, mut col) = self.frame(width, Some(&tr!("palette-title")));
        let mut edit = LineEdit::new_alloc();
        edit.set_max_length(MAX_TEXT_BYTES as i32);
        edit.set_keep_editing_on_text_submit(true);
        let (q, c) = (self.queue.clone(), req_cell.clone());
        edit.signals()
            .text_submitted()
            .connect(move |text: GString| {
                let ev = DialogEvent::TextSubmitted(text.to_string());
                push(&q, UiEvent::Dialog { req: c.get(), ev });
            });
        edit.set_placeholder(&tr!("palette-placeholder"));
        col.add_child(&edit);
        let mut rows_box = VBoxContainer::new_alloc();
        rows_box.add_theme_constant_override("separation", 0);
        rows_box.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut slots = Vec::with_capacity(cmds.len());
        for i in 0..cmds.len() {
            let mut name = cell("", theme::TEXT, Some(&look.body_bold), Some(name_w));
            name.add_theme_font_size_override("font_size", BODY_SIZE);
            let mut key = cell("", theme::ACCENT, None, Some(key_w));
            key.add_theme_font_size_override("font_size", 14);
            // the command and its key as the player types them
            i18n::verbatim(&name);
            i18n::verbatim(&key);
            let mut desc = cell("", theme::TEXT_DIM, Some(&look.body), None);
            desc.add_theme_font_size_override("font_size", BODY_SIZE - 1);
            // the palette is reused: its rows answer the request it is open
            // for now
            let button = self.row_button_at(
                req_cell.clone(),
                i,
                &[name.clone(), key.clone(), desc.clone()],
            );
            rows_box.add_child(&button);
            slots.push(PaletteSlot {
                button,
                name,
                key,
                desc,
            });
        }
        let mut scroll = list.scroll.clone();
        scroll.add_child(&rows_box);
        col.add_child(&scroll);
        let mut none = theme::label(&tr!("palette-none"));
        none.add_theme_color_override("font_color", theme::WARN);
        none.set_visible(false);
        col.add_child(&none);
        self.hint(&mut col, &tr!("palette-hint"), width);
        let mut row = HBoxContainer::new_alloc();
        row.set_alignment(AlignmentMode::CENTER);
        let mut cancel = Button::new_alloc();
        cancel.set_text(&tr!("dlg-cancel"));
        cancel.set_focus_mode(FocusMode::NONE);
        cancel.set_custom_minimum_size(Vector2::new(112.0, 36.0));
        let (q, c) = (self.queue.clone(), req_cell.clone());
        cancel.signals().pressed().connect(move || {
            let ev = DialogEvent::ExtCmd(None);
            push(&q, UiEvent::Dialog { req: c.get(), ev });
        });
        row.add_child(&cancel);
        col.add_child(&row);
        let palette = Rc::new(RefCell::new(Palette {
            shown: palette_filter(&cmds, ""),
            cmds,
            selected: None,
            slots,
            styled: None,
            list,
            none,
            look,
        }));
        fill_palette(&palette);
        // refiltering is local to the palette: it never touches the game
        let p = palette.clone();
        edit.signals().text_changed().connect(move |text: GString| {
            p.borrow_mut().refilter(&text.to_string());
            fill_palette(&p);
        });
        if !self.warming {
            edit.call_deferred("grab_focus", &[]);
        }
        (
            Kind::ExtCmd {
                edit,
                palette,
                req: req_cell,
            },
            shade,
            panel,
        )
    }

    fn open_text(
        &mut self,
        req: u64,
        query: &str,
        name: bool,
        engrave: bool,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let width = self.fit_width(query.chars().count().min(70), 0.0, 560.0);
        let (shade, panel, mut col) = self.frame_at(width, Some(query), Place::Top);
        let mut edit = self.line_edit(req);
        if name {
            edit.set_placeholder(&tr!("text-name-placeholder"));
        }
        col.add_child(&edit);
        // an engraving goes in Latin letters: what they will be, under it
        if engrave {
            let mut latin = theme::styled_label("", Face::Body, 15, theme::ACCENT);
            latin.set_autowrap_mode(AutowrapMode::WORD_SMART);
            latin.set_custom_minimum_size(Vector2::new(width, 0.0));
            latin.set_visible(false);
            col.add_child(&latin);
            let mut l = latin.clone();
            edit.signals().text_changed().connect(move |text: GString| {
                let text = text.to_string();
                let shown = !text.is_ascii();
                if shown {
                    l.set_text(&tr!("text-latin", text = pickers::latin(&text)));
                }
                l.set_visible(shown);
            });
        }
        let mut bytes = theme::styled_label(
            &tr!("text-bytes", len = 0, max = MAX_TEXT_BYTES),
            Face::Body,
            14,
            theme::TEXT_DIM,
        );
        bytes.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        col.add_child(&bytes);
        // NetHack takes 255 bytes of UTF-8; LineEdit counts characters
        let (mut e, mut b) = (edit.clone(), bytes.clone());
        edit.signals().text_changed().connect(move |text: GString| {
            let text = text.to_string();
            let mut len = text.len();
            if len > MAX_TEXT_BYTES {
                let cut = truncate_bytes(&text, MAX_TEXT_BYTES);
                let caret = e.get_caret_column().min(cut.chars().count() as i32);
                len = cut.len();
                e.set_text(&cut);
                e.set_caret_column(caret);
            }
            b.set_text(&tr!("text-bytes", len = len, max = MAX_TEXT_BYTES));
        });
        // with a gamepad the keyboard below has its own OK (and B cancels)
        if !self.pad {
            self.hint(&mut col, &tr!("text-hint"), width);
            let mut row = HBoxContainer::new_alloc();
            row.set_alignment(AlignmentMode::CENTER);
            row.add_theme_constant_override("separation", 12);
            // OK submits what the field holds, as Enter does
            let mut ok = Button::new_alloc();
            ok.set_text(&tr!("dlg-ok"));
            ok.set_focus_mode(FocusMode::NONE);
            let (q, e) = (self.queue.clone(), edit.clone());
            ok.signals().pressed().connect(move || {
                let ev = DialogEvent::TextSubmitted(e.get_text().to_string());
                push(&q, UiEvent::Dialog { req, ev });
            });
            row.add_child(&ok);
            let cancel = dialog_ui(req, DialogEvent::TextCancelled);
            row.add_child(&theme::button(&tr!("dlg-cancel"), &self.queue, cancel));
            col.add_child(&row);
        }
        // a gamepad types on a keyboard of ours (and asks Steam for its
        // own, which types into the field, where there is one)
        let osk = self.pad.then(|| {
            let mut osk = Osk::new(&mut col, req, &self.queue);
            osk.show(&self.look);
            let steam = std::env::var("SteamDeck").is_ok_and(|v| v == "1")
                || std::env::var("SteamTenfoot").is_ok();
            if steam {
                godot::classes::Os::singleton().shell_open("steam://open/keyboard");
            }
            osk
        });
        if osk.is_none() && !self.warming {
            edit.call_deferred("grab_focus", &[]);
        }
        (Kind::Text { edit, osk, engrave }, shade, panel)
    }

    /// A picker of names for what `ask` asks (None: nothing to pick from,
    /// such as monster classes before the engine's catalog came).
    fn open_picker(
        &mut self,
        req: u64,
        title: &str,
        ask: Ask,
        catalog: Option<&Catalog>,
    ) -> Option<(Kind, Gd<ColorRect>, Gd<PanelContainer>)> {
        let lex = Lexicon::ru();
        let (picks, class) = match ask {
            Ask::Wish => (pickers::objects(lex), None),
            Ask::Write(c) => (pickers::objects(lex), Some(c)),
            Ask::Monster => (pickers::monsters(lex, catalog), None),
            Ask::MonsterClass => (pickers::monster_classes(lex, catalog), None),
        };
        if picks.is_empty() {
            return None;
        }
        let look = self.look.clone();
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(480.0);
        let width = 660.0f32.min(max_w);
        let (shade, panel, mut col) = self.frame(width, Some(title));
        let small = |text: &str| {
            let mut b = Button::new_alloc();
            b.set_text(text);
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(34.0, 32.0));
            b
        };
        let value = |w: f32| {
            let mut l = theme::styled_label("", Face::BodyBold, 18, theme::GOLD_BRIGHT);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            l.set_custom_minimum_size(Vector2::new(w, 0.0));
            l
        };
        // the wish's builder: count, blessing, enchantment
        let mut builder_buttons: Vec<(Gd<Button>, i32, bool, i32)> = Vec::new();
        let wish_controls = (ask == Ask::Wish).then(|| {
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 6);
            row.add_child(&theme::label(&tr!("wish-count")));
            let less = small("−");
            let count = value(40.0);
            let more = small("+");
            row.add_child(&less);
            row.add_child(&count);
            row.add_child(&more);
            builder_buttons.push((less, -1, false, 0));
            builder_buttons.push((more, 1, false, 0));
            let mut gap = Control::new_alloc();
            gap.set_custom_minimum_size(Vector2::new(18.0, 0.0));
            row.add_child(&gap);
            let mut buc = Button::new_alloc();
            buc.set_focus_mode(FocusMode::NONE);
            buc.set_custom_minimum_size(Vector2::new(190.0, 32.0));
            row.add_child(&buc);
            builder_buttons.push((buc.clone(), 0, true, 0));
            let mut gap = Control::new_alloc();
            gap.set_custom_minimum_size(Vector2::new(18.0, 0.0));
            row.add_child(&gap);
            row.add_child(&theme::label(&tr!("wish-ench")));
            let down = small("−");
            let ench = value(44.0);
            let up = small("+");
            row.add_child(&down);
            row.add_child(&ench);
            row.add_child(&up);
            builder_buttons.push((down, 0, false, -1));
            builder_buttons.push((up, 0, false, 1));
            col.add_child(&row);
            let shown = theme::styled_label("", Face::BodyBold, 17, theme::TEXT);
            let mut english = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
            // what the engine is sent, as it is sent
            i18n::verbatim(&english);
            english.set_clip_text(true);
            (count, buc, ench, shown, english)
        });
        // the class a wish keeps to
        let classes: Vec<ObjClass> = match ask {
            Ask::Wish => ObjClass::ALL.to_vec(),
            _ => Vec::new(),
        };
        let class_option = (!classes.is_empty()).then(|| {
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 10);
            row.add_child(&theme::label(&tr!("picker-class")));
            let mut ob = OptionButton::new_alloc();
            ob.set_focus_mode(FocusMode::NONE);
            ob.add_item(&tr!("picker-class-all"));
            for c in &classes {
                ob.add_item(&i18n::tr(c.key()));
            }
            ob.select(0);
            ob.set_custom_minimum_size(Vector2::new(260.0, 0.0));
            row.add_child(&ob);
            col.add_child(&row);
            ob
        });
        let mut edit = self.line_edit(req);
        edit.set_placeholder(&i18n::tr(match ask {
            Ask::Wish => "picker-placeholder-wish",
            Ask::Monster => "picker-placeholder-monster",
            Ask::MonsterClass => "picker-placeholder-class",
            Ask::Write(_) => "picker-placeholder-write",
        }));
        col.add_child(&edit);
        // the list: a slot a row
        let n = picks.len().min(PICKER_SLOTS);
        let max_h = (self.screen().y - LIST_CHROME - 120.0).max(ROW_H * 4.0);
        let view_h = (n as f32 * ROW_H).clamp(ROW_H, max_h.min(PICKER_ROWS * ROW_H));
        let list = Scroller::new(width, view_h, false);
        let req_cell = Rc::new(std::cell::Cell::new(req));
        let mut rows_box = VBoxContainer::new_alloc();
        rows_box.add_theme_constant_override("separation", 0);
        rows_box.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let symbols = picks.iter().any(|p| p.symbol.is_some());
        let english_w = 230.0;
        let mut slots = Vec::with_capacity(n);
        for i in 0..n {
            let mut symbol = cell(
                "",
                theme::ACCENT,
                Some(&look.bold),
                Some(if symbols { 22.0 } else { 0.0 }),
            );
            symbol.set_visible(symbols);
            // a monster's class symbol, as the map draws it
            i18n::verbatim(&symbol);
            let mut name = cell("", theme::TEXT, Some(&look.body_bold), None);
            name.add_theme_font_size_override("font_size", BODY_SIZE);
            let mut english = cell("", theme::TEXT_DIM, Some(&look.body), Some(english_w));
            english.add_theme_font_size_override("font_size", BODY_SIZE - 2);
            english.set_clip_text(true);
            english.set_horizontal_alignment(HorizontalAlignment::RIGHT);
            // the engine's English for it, as the manual entry takes it
            i18n::verbatim(&english);
            let button = self.row_button_at(
                req_cell.clone(),
                i,
                &[symbol.clone(), name.clone(), english.clone()],
            );
            rows_box.add_child(&button);
            slots.push(PickSlot {
                button,
                symbol,
                name,
                english,
            });
        }
        let mut scroll = list.scroll.clone();
        scroll.add_child(&rows_box);
        col.add_child(&scroll);
        let mut more = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        more.set_visible(false);
        col.add_child(&more);
        let wish_controls = wish_controls.map(|(count, buc, ench, shown, english)| {
            col.add_child(&shown);
            col.add_child(&english);
            WishControls {
                count,
                buc,
                ench,
                shown,
                english,
            }
        });
        let hint = if ask == Ask::Wish {
            tr!("picker-hint-wish")
        } else {
            tr!("picker-hint")
        };
        self.hint(&mut col, &hint, width);
        let confirm = if ask == Ask::Wish {
            tr!("picker-wish")
        } else {
            tr!("picker-choose")
        };
        self.buttons(
            &mut col,
            &[
                (confirm, dialog_ui(req, DialogEvent::PickConfirm)),
                (
                    tr!("picker-manual"),
                    dialog_ui(req, DialogEvent::PickManual),
                ),
                (
                    tr!("dlg-cancel"),
                    dialog_ui(req, DialogEvent::TextCancelled),
                ),
            ],
        );
        let mut picker = Picker {
            ask,
            picks,
            shown: Vec::new(),
            selected: None,
            query: String::new(),
            class,
            classes,
            class_option: class_option.clone(),
            wish: Wish::default(),
            wish_controls,
            slots,
            list,
            more,
            look,
        };
        picker.refilter();
        let picker = Rc::new(RefCell::new(picker));
        fill_picker(&picker);
        // searching, the class and the builder are the picker's own: they
        // never touch the game
        let p = picker.clone();
        edit.signals().text_changed().connect(move |text: GString| {
            p.borrow_mut().retype(&text.to_string());
            fill_picker(&p);
        });
        if let Some(ob) = class_option {
            let p = picker.clone();
            ob.signals().item_selected().connect(move |i: i64| {
                {
                    let mut pk = p.borrow_mut();
                    pk.class = (i > 0)
                        .then(|| pk.classes.get(i as usize - 1).copied())
                        .flatten();
                    pk.refilter();
                }
                fill_picker(&p);
            });
        }
        for (b, count, buc, ench) in builder_buttons {
            let p = picker.clone();
            b.signals().pressed().connect(move || {
                p.borrow_mut().adjust(count, buc, ench);
                fill_picker(&p);
            });
        }
        // a gamepad picks from the list; a keyboard types into the search
        if !self.warming && !self.pad {
            edit.call_deferred("grab_focus", &[]);
        }
        Some((Kind::Picker { edit, picker }, shade, panel))
    }

    fn open_show(
        &mut self,
        title: Option<&str>,
        lines: &[TextLine],
        req: u64,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        // the engine pads a window with blank lines: no empty band under
        // the text
        let end = lines
            .iter()
            .rposition(|l| !l.text.trim().is_empty())
            .map_or(0, |i| i + 1);
        let lines = &lines[..end];
        // prose (`^X`, the intro, messages) reads in the body face; tables
        // and pictures keep the monospace columns
        let prose = !lines.iter().any(|l| {
            let t = l.text.trim();
            t.contains("  ") || t.contains('|') || t.contains("--") || t.contains('\t')
        });
        let look = self.look.clone();
        let longest_w = lines
            .iter()
            .map(|l| {
                if prose {
                    look.text_w(&l.text, &look.body_bold, BODY_SIZE)
                } else {
                    look.chars(l.text.chars().count())
                }
            })
            .fold(0.0f32, f32::max)
            .max(title.map_or(0.0, |t| look.text_w(t, &look.body_bold, 20).min(760.0)));
        // the scroll bar and a little air
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(420.0);
        let width = (longest_w + 30.0).clamp(420.0, max_w);
        let line_h = if prose {
            look.body.get_height_ex().font_size(BODY_SIZE).done()
        } else {
            self.look.line_h
        };
        let content_h = lines.len().max(1) as f32 * line_h + 8.0;
        let max_h = (self.screen().y - TEXT_CHROME).max(ROW_H * 4.0);
        let text = Scroller::new(width, content_h.min(max_h), true);
        let (shade, panel, mut col) = self.frame(width, title);
        let mut label = RichTextLabel::new_alloc();
        label.set_use_bbcode(true);
        label.set_focus_mode(FocusMode::NONE);
        label.set_mouse_filter(MouseFilter::PASS);
        label.set_selection_enabled(false);
        // tables and ASCII art keep their columns: no wrapping, the label
        // as wide and tall as its text, the container scrolls
        label.set_autowrap_mode(AutowrapMode::OFF);
        label.set_scroll_active(false);
        label.set_fit_content(true);
        if prose {
            label.add_theme_font_override("normal_font", &look.body);
            label.add_theme_font_override("bold_font", &look.body_bold);
            label.add_theme_font_size_override("normal_font_size", BODY_SIZE);
            label.add_theme_font_size_override("bold_font_size", BODY_SIZE);
            label.set_text(&prose_text(lines));
        } else {
            label.add_theme_font_override("italics_font", &self.look.italic);
            label.set_text(&show_text(lines));
        }
        let mut scroll = text.scroll.clone();
        scroll.add_child(&label);
        col.add_child(&scroll);
        // the estimate above errs tall: once the text is laid out (its
        // first draw), the scroller and the panel shrink to it
        if content_h < max_h {
            let (mut sc, l, mut p) = (scroll.clone(), label.clone(), panel.clone());
            label.signals().draw().connect(move || {
                let content = l.get_content_height() as f32 + 8.0;
                let now = sc.get_custom_minimum_size();
                if content > 8.0 && content + 1.0 < now.y {
                    sc.set_custom_minimum_size(Vector2::new(now.x, content.max(ROW_H)));
                    // the centred panel shrinks and grows back both ways
                    p.set_offset(godot::builtin::Side::TOP, -40.0);
                    p.set_offset(godot::builtin::Side::BOTTOM, 40.0);
                }
            });
        }
        if content_h > max_h {
            self.hint(&mut col, &tr!("show-hint"), width);
        }
        self.buttons(
            &mut col,
            &[(tr!("dlg-ok"), dialog_ui(req, DialogEvent::Close))],
        );
        (Kind::Show { text }, shade, panel)
    }

    /// Open the UI for Menu, Choice, Text, ExtCmd, Show, MessageMenu (other
    /// prompts: no-op).
    pub fn open(&mut self, req: u64, original: &Prompt, catalog: Option<&Catalog>) {
        self.close();
        // the engine's words as the player reads them; the original is kept
        // (a switch of the language opens it again)
        let shown = shown_prompt(original);
        let prompt = &shown;
        let (kind, shade, panel) = match prompt {
            Prompt::Menu {
                how, title, items, ..
            } => self.open_menu(req, *how, title.as_deref(), items),
            Prompt::Choice {
                query,
                visible,
                allowed,
                default,
            } => {
                let width = self.fit_width(query.chars().count().min(80), 0.0, 420.0);
                let (shade, panel, mut col) = self.frame_at(width, Some(query), Place::Top);
                let items: Vec<(String, UiEvent)> = visible
                    .iter()
                    .map(|&c| (choice_label(c), dialog_ui(req, DialogEvent::Choice(c))))
                    .collect();
                let buttons = self.buttons(&mut col, &items);
                let mut hint = tr!("choice-hint");
                if let Some(d) = *default {
                    hint = tr!("choice-hint-default", hint = hint, answer = choice_label(d));
                    if let Some(i) = visible.iter().position(|&c| c == d) {
                        let mut b = buttons[i].clone();
                        b.add_theme_stylebox_override("normal", &self.look.default_button);
                    }
                }
                self.hint(&mut col, &hint, width);
                let buttons: Vec<(char, Gd<Button>)> =
                    visible.iter().copied().zip(buttons).collect();
                // a gamepad starts on the default answer
                let focus = self
                    .pad
                    .then(|| {
                        default
                            .and_then(|d| buttons.iter().position(|(c, _)| *c == d))
                            .unwrap_or(0)
                    })
                    .filter(|_| !buttons.is_empty());
                let mut kind = Kind::Choice {
                    allowed: allowed.clone(),
                    default: *default,
                    buttons,
                    focus,
                };
                if let Kind::Choice { buttons, focus, .. } = &mut kind {
                    show_focus(buttons, *focus, &self.look);
                }
                (kind, shade, panel)
            }
            Prompt::Text { query, name } => {
                // the engine's own question decides (the shown one may be
                // translated)
                let english = match original {
                    Prompt::Text { query, .. } => query.as_str(),
                    _ => query.as_str(),
                };
                let ask = (i18n::lang() != Lang::En)
                    .then(|| pickers::ask_of(english))
                    .flatten();
                match ask.and_then(|a| self.open_picker(req, query, a, catalog)) {
                    Some(opened) => opened,
                    None => self.open_text(req, query, *name, pickers::is_engraving(english)),
                }
            }
            Prompt::ExtCmd => self.open_palette(req, catalog),
            Prompt::Show { title, lines } => self.open_show(title.as_deref(), lines, req),
            Prompt::MessageMenu { letter, mesg, pick } => {
                let width = self.fit_width(mesg.chars().count().min(80), 0.0, 420.0);
                let (shade, panel, mut col) = self.frame_at(width, Some(mesg), Place::Top);
                let (buttons, hint) = if *pick {
                    (
                        vec![
                            (
                                format!("{letter}"),
                                dialog_ui(req, DialogEvent::Choice(*letter)),
                            ),
                            (
                                tr!("dlg-cancel"),
                                dialog_ui(req, DialogEvent::Choice(ESC_CHAR)),
                            ),
                        ],
                        tr!("msgmenu-hint-pick", letter = *letter),
                    )
                } else {
                    (
                        vec![(tr!("dlg-ok"), dialog_ui(req, DialogEvent::Close))],
                        tr!("msgmenu-hint-close"),
                    )
                };
                self.buttons(&mut col, &buttons);
                self.hint(&mut col, &hint, width);
                let kind = Kind::MessageMenu {
                    letter: *letter,
                    pick: *pick,
                };
                (kind, shade, panel)
            }
            Prompt::Command
            | Prompt::Key
            | Prompt::FreeKey { .. }
            | Prompt::MapPause
            | Prompt::AutoAck => {
                return;
            }
        };
        let pad_room = add_pad_room(&panel);
        self.open = Some(Open {
            req,
            prompt: original.clone(),
            kind,
            shade,
            panel,
            pad_room,
        });
    }

    /// Room at the bottom of the open dialog for a gamepad's hints, as
    /// high as they are (0: no gamepad).
    pub fn set_pad_room(&mut self, h: f32) {
        if let Some(mut room) = self.open.as_ref().and_then(|o| o.pad_room.clone())
            && room.get_custom_minimum_size().y != h
        {
            room.set_custom_minimum_size(Vector2::new(0.0, h));
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// True when a LineEdit owns the keyboard (Text, ExtCmd).
    pub fn wants_text(&self) -> bool {
        matches!(
            self.open.as_ref().map(|o| &o.kind),
            Some(Kind::Text { .. } | Kind::ExtCmd { .. } | Kind::Picker { .. })
        )
    }

    /// The request the open dialog serves.
    pub fn open_req(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.req)
    }

    /// "menu", "choice", "text", "extcmd", "show", "message".
    pub fn kind_name(&self) -> Option<&'static str> {
        self.open.as_ref().map(|o| match o.kind {
            Kind::Menu(_) => "menu",
            Kind::Choice { .. } => "choice",
            Kind::Text { .. } => "text",
            Kind::Picker { .. } => "picker",
            Kind::ExtCmd { .. } => "extcmd",
            Kind::Show { .. } => "show",
            Kind::MessageMenu { .. } => "message",
        })
    }

    /// One frame of the warm-up behind the title screen: the dialogs drawn
    /// long enough go, and either the last one built is drawn for the
    /// first time or the next kind is built, hidden (what was done: this
    /// frame's work; a making and a first draw never share a frame).
    pub fn warm_step(&mut self, catalog: Option<&Catalog>) -> Option<&'static str> {
        if self.warm_tick() {
            return Some("dialog drawn");
        }
        let samples = warm_samples();
        let sample = samples.get(self.warm_next)?;
        // a dialog of the game's own stays
        if self.open.is_some() {
            return None;
        }
        self.warm_next += 1;
        self.warming = true;
        self.open(0, sample, catalog);
        self.warming = false;
        let mut open = self.open.take()?;
        // built hidden, drawn from the next frame: its making and its
        // first draw (layout, text, pipelines) fall on two frames
        open.panel.set_visible(false);
        open.shade.set_visible(false);
        self.warm.push((open, 0));
        Some(prompt_kind(sample))
    }

    /// A frame passed: the warm-up's dialog built last frame is drawn, the
    /// ones drawn for a few frames go (the palette to its pool, for the
    /// first `#`). True when one is drawn for the first time.
    pub fn warm_tick(&mut self) -> bool {
        let mut shown = false;
        for (open, frames) in self.warm.iter_mut() {
            *frames += 1;
            if *frames == 1 {
                open.panel.set_visible(true);
                open.shade.set_visible(true);
                shown = true;
            }
        }
        let (old, keep): (Vec<_>, Vec<_>) = self
            .warm
            .drain(..)
            .partition(|(_, frames)| *frames >= WARM_FRAMES);
        self.warm = keep;
        for (open, _) in old {
            self.retire(open);
        }
        shown
    }

    /// The open dialog again in the language now (its state starts over:
    /// a switch happens between questions, from the settings); the
    /// palette is made again on its next use.
    pub fn relang(&mut self, catalog: Option<&Catalog>) {
        if let Some(mut pool) = self.palette_pool.take() {
            pool.panel.queue_free();
            pool.shade.queue_free();
        }
        if let Some((req, prompt)) = self.open.as_ref().map(|o| (o.req, o.prompt.clone())) {
            self.open(req, &prompt, catalog);
        }
    }

    /// The command palette made ahead, hidden, with its rows' text shaped
    /// (at start-up, under the boot splash: about a hundred rows are tens
    /// of milliseconds); its first draw, in the warm-up, has only drawing
    /// left.
    pub fn prebuild_palette(&mut self, catalog: Option<&Catalog>) {
        if self.palette_pool.is_some() || self.open.is_some() {
            return;
        }
        self.warming = true;
        self.open(0, &Prompt::ExtCmd, catalog);
        self.warming = false;
        let Some(open) = self.open.take() else {
            return;
        };
        if let Kind::ExtCmd { palette, .. } = &open.kind {
            for slot in &palette.borrow().slots {
                for label in [&slot.name, &slot.key, &slot.desc] {
                    label.get_minimum_size();
                }
            }
        }
        self.retire(open);
    }

    /// Every kind of dialog has been built, drawn and put away.
    pub fn warm_done(&self) -> bool {
        self.warm.is_empty() && self.warm_next >= warm_samples().len()
    }

    /// Whether a gamepad gives the input now.
    pub fn set_pad(&mut self, on: bool) {
        self.pad = on;
    }

    /// The open menu picks several entries.
    pub fn menu_any(&self) -> bool {
        matches!(self.open.as_ref().map(|o| &o.kind), Some(Kind::Menu(v)) if v.state.how == PickHow::Any)
    }

    /// The letter a message to pick takes.
    pub fn message_letter(&self) -> Option<char> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::MessageMenu { letter, pick: true }) => Some(*letter),
            _ => None,
        }
    }

    /// The on-screen keyboard: a gamepad's move, key, backspace, layout or
    /// OK; the reply when it answers.
    pub fn osk(&mut self, op: crate::gamepad::OskOp) -> Option<Reply> {
        use crate::gamepad::OskOp;
        let open = self.open.as_mut()?;
        let Kind::Text {
            edit,
            osk: Some(osk),
            engrave,
        } = &mut open.kind
        else {
            return None;
        };
        let text = edit.get_text().to_string();
        let sent = if *engrave {
            pickers::latin(&text)
        } else {
            text.clone()
        };
        match op {
            OskOp::Move(dx, dy) => osk.step(dx, dy, &self.look),
            OskOp::Layout => osk.switch(),
            OskOp::Submit => return Some(Reply::Text(truncate_bytes(&sent, MAX_TEXT_BYTES))),
            OskOp::Back if text.is_empty() => return Some(open.prompt.escape_reply()),
            OskOp::Back => {
                let mut t = text;
                t.pop();
                edit.set_text(&t);
            }
            OskOp::Press => match osk.current() {
                OskKey::Char(c) => {
                    let t = format!("{text}{c}");
                    if t.len() <= MAX_TEXT_BYTES {
                        edit.set_text(&t);
                    }
                }
                OskKey::Space => edit.set_text(&format!("{text} ")),
                OskKey::Back => {
                    let mut t = text;
                    t.pop();
                    edit.set_text(&t);
                }
                OskKey::Shift => osk.shift(),
                OskKey::Layout => osk.switch(),
                OskKey::Ok => return Some(Reply::Text(truncate_bytes(&sent, MAX_TEXT_BYTES))),
            },
        }
        None
    }

    /// The on-screen keyboard is up (self-tests).
    pub fn osk_open(&self) -> bool {
        matches!(
            self.open.as_ref().map(|o| &o.kind),
            Some(Kind::Text { osk: Some(_), .. })
        )
    }

    pub fn menu_entries(&self) -> Option<&[MenuEntry]> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => Some(&view.state.entries),
            _ => None,
        }
    }

    /// The open menu's keyboard row (an index into `menu_entries`).
    pub fn menu_cursor(&self) -> Option<usize> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => view.cursor,
            _ => None,
        }
    }

    /// The open menu's list: how far it is scrolled and how tall its view
    /// is, in pixels.
    pub fn menu_scroll(&self) -> Option<(f32, f32)> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => Some((view.list.top(), view.list.view_h)),
            _ => None,
        }
    }

    /// How far the open menu or text window is scrolled, in pixels.
    pub fn list_top(&self) -> Option<f32> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => Some(view.list.top()),
            Some(Kind::Show { text }) => Some(text.top()),
            _ => None,
        }
    }

    /// The top of each of the open menu's rows (one per `menu_entries`).
    pub fn menu_row_tops(&self) -> Option<&[f32]> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => Some(&view.tops),
            _ => None,
        }
    }

    /// The commands the palette lists now, in order.
    pub fn palette_names(&self) -> Option<Vec<String>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::ExtCmd { palette, .. }) => {
                let p = palette.borrow();
                Some(p.shown.iter().map(|&i| p.cmds[i].name.clone()).collect())
            }
            _ => None,
        }
    }

    /// The text field's contents.
    pub fn text(&self) -> Option<String> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(
                Kind::Text { edit, .. } | Kind::ExtCmd { edit, .. } | Kind::Picker { edit, .. },
            ) => Some(edit.get_text().to_string()),
            _ => None,
        }
    }

    /// Does the text field have the keyboard?
    pub fn text_has_focus(&self) -> bool {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(
                Kind::Text { edit, .. } | Kind::ExtCmd { edit, .. } | Kind::Picker { edit, .. },
            ) => edit.has_focus(),
            _ => false,
        }
    }

    /// The open picker is a wish's (its builder takes a gamepad's X and Y).
    pub fn pick_is_wish(&self) -> bool {
        matches!(self.open.as_ref().map(|o| &o.kind), Some(Kind::Picker { picker, .. }) if picker.borrow().ask == Ask::Wish)
    }

    /// The open picker's list, as the English the engine knows (self-tests).
    pub fn pick_list(&self) -> Option<Vec<String>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Picker { picker, .. }) => {
                let p = picker.borrow();
                Some(
                    p.shown
                        .iter()
                        .map(|&i| p.picks[i].english.clone())
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// What the open picker would send now (self-tests).
    pub fn pick_reply(&self) -> Option<String> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Picker { picker, .. }) => {
                let p = picker.borrow();
                p.chosen().map(|i| p.reply(i))
            }
            _ => None,
        }
    }

    /// Type `text` into the open picker's search, as a player would.
    pub fn type_pick(&mut self, text: &str) -> bool {
        let Some(Kind::Picker { edit, picker }) = self.open.as_mut().map(|o| &mut o.kind) else {
            return false;
        };
        edit.set_text(text);
        picker.borrow_mut().retype(text);
        fill_picker(picker);
        true
    }

    /// A gamepad's button in a picker: the wish's blessing or enchantment,
    /// or the answer typed in English instead.
    pub fn pick_op(&mut self, op: crate::gamepad::PickOp) -> Option<Reply> {
        use crate::gamepad::PickOp;
        let Some(Kind::Picker { picker, .. }) = self.open.as_ref().map(|o| &o.kind) else {
            return None;
        };
        let picker = picker.clone();
        match op {
            PickOp::Buc => picker.borrow_mut().adjust(0, true, 0),
            PickOp::Ench => {
                let mut p = picker.borrow_mut();
                // up to the most, then round to the least
                if p.wish.ench.unwrap_or(0) >= pickers::MAX_ENCH {
                    p.wish.ench = Some(-pickers::MAX_ENCH);
                } else {
                    p.adjust(0, false, 1);
                }
            }
            PickOp::Manual => {
                let req = self.open.as_ref()?.req;
                return self.dialog_event(req, &DialogEvent::PickManual);
            }
        }
        fill_picker(&picker);
        None
    }

    pub fn key(&mut self, input: &KeyInput) -> Option<Reply> {
        let open = self.open.as_mut()?;
        let escape = open.prompt.escape_reply();
        match &mut open.kind {
            Kind::Menu(view) => view.key(input, &self.look),
            Kind::Choice {
                allowed,
                default,
                buttons,
                focus,
            } => {
                // the arrows (a gamepad) move between the answers
                let step = match nav(input) {
                    Some(Key::Left | Key::Up) => -1,
                    Some(Key::Right | Key::Down) => 1,
                    _ => 0,
                };
                if step != 0 && !buttons.is_empty() {
                    let n = buttons.len() as i32;
                    let f = focus.map_or(if step > 0 { -1 } else { 0 }, |f| f as i32);
                    *focus = Some(((f + step).rem_euclid(n)) as usize);
                    show_focus(buttons, *focus, &self.look);
                    return None;
                }
                match typed(input)? {
                    ESC_CHAR => Some(escape),
                    '\n' | ' ' => match focus.and_then(|f| buttons.get(f)) {
                        Some((c, _)) => Some(Reply::Char(*c as i32)),
                        None => default.map(|d| Reply::Char(d as i32)),
                    },
                    c => choice_answer(allowed, c).map(|c| Reply::Char(c as i32)),
                }
            }
            // a held key never answers: Esc cancels only when pressed
            Kind::Text { .. } => (input.key == Key::Escape && !input.echo).then_some(escape),
            Kind::Picker { edit, picker } => {
                let typing = edit.has_focus();
                {
                    let mut p = picker.borrow_mut();
                    match input.key {
                        Key::Escape if input.echo => return None,
                        Key::Escape => return Some(escape),
                        // a gamepad's A (a keyboard's Enter goes to the field)
                        Key::Enter | Key::KeypadEnter if !input.echo => {
                            return p.chosen().map(|i| Reply::Text(p.reply(i)));
                        }
                        Key::Up => p.step(-1),
                        Key::Down => p.step(1),
                        Key::PageUp => p.step(-10),
                        Key::PageDown => p.step(10),
                        Key::Tab => p.turn_class(if input.mods.shift { -1 } else { 1 }),
                        // the d-pad's ← → count a wish (the field's caret
                        // takes them while typing)
                        Key::Left if !typing && p.ask == Ask::Wish => p.adjust(-1, false, 0),
                        Key::Right if !typing && p.ask == Ask::Wish => p.adjust(1, false, 0),
                        _ => return None,
                    }
                }
                fill_picker(picker);
                None
            }
            Kind::ExtCmd { edit, palette, .. } => {
                let step = match input.key {
                    Key::Escape | Key::Tab if input.echo => return None,
                    Key::Escape => return Some(escape),
                    // a gamepad's A: the highlighted command
                    Key::Enter | Key::KeypadEnter if !input.echo => {
                        let p = palette.borrow();
                        return p
                            .selected
                            .and_then(|s| p.shown.get(s))
                            .map(|&i| Reply::ExtCmd(Some(p.cmds[i].name.clone())));
                    }
                    Key::Up => -1,
                    Key::Down => 1,
                    Key::PageUp => -10,
                    Key::PageDown => 10,
                    Key::Tab => {
                        let text = edit.get_text().to_string();
                        let done = palette_complete(&palette.borrow().cmds, &text);
                        edit.set_text(&done);
                        edit.set_caret_column(done.chars().count() as i32);
                        palette.borrow_mut().refilter(&done);
                        fill_palette(palette);
                        return None;
                    }
                    _ => return None,
                };
                {
                    let mut p = palette.borrow_mut();
                    let n = p.shown.len() as i64;
                    if n > 0 {
                        let cur = p.selected.map_or(-1, |s| s as i64);
                        let next = if cur < 0 && step < 0 {
                            n - 1
                        } else {
                            (cur + step).clamp(0, n - 1)
                        };
                        p.selected = Some(next as usize);
                    }
                }
                fill_palette(palette);
                None
            }
            Kind::Show { text } => {
                if let Some(k) = nav(input) {
                    text.key(k, self.look.line_h);
                    return None;
                }
                match typed(input)? {
                    '<' => text.page(-1.0),
                    '>' => text.page(1.0),
                    '\n' | ' ' | ESC_CHAR => return Some(Reply::Ack),
                    _ => {}
                }
                None
            }
            // With `pick`, only the letter or Esc answers: Enter and Space
            // do nothing, so a stray Enter neither picks nor cancels.
            Kind::MessageMenu { letter, pick } => match typed(input)? {
                ESC_CHAR => Some(escape),
                c if *pick && c == *letter => Some(Reply::Char(c as i32)),
                '\n' | ' ' if !*pick => Some(Reply::Char(0)),
                _ => None,
            },
        }
    }

    pub fn dialog_event(&mut self, req: u64, ev: &DialogEvent) -> Option<Reply> {
        // a picker given up for the English the engine reads: the plain
        // question in its place
        if matches!(ev, DialogEvent::PickManual) {
            let open = self
                .open
                .take_if(|o| o.req == req && matches!(o.kind, Kind::Picker { .. }))?;
            let prompt = open.prompt.clone();
            self.retire(open);
            let Prompt::Text { query, name } = shown_prompt(&prompt) else {
                return None;
            };
            let (kind, shade, panel) = self.open_text(req, &query, name, false);
            let pad_room = add_pad_room(&panel);
            self.open = Some(Open {
                req,
                prompt,
                kind,
                shade,
                panel,
                pad_room,
            });
            return None;
        }
        // a key of the on-screen keyboard clicked: as if A typed it
        if let DialogEvent::OskKey(r, c) = ev {
            if let Some(Open {
                req: open_req,
                kind: Kind::Text { osk: Some(o), .. },
                ..
            }) = self.open.as_mut()
                && *open_req == req
            {
                o.at(*r, *c);
            } else {
                return None;
            }
            return self.osk(crate::gamepad::OskOp::Press);
        }
        let open = self.open.as_mut().filter(|o| o.req == req)?;
        let escape = open.prompt.escape_reply();
        match (&mut open.kind, ev) {
            (Kind::Menu(view), DialogEvent::MenuClick(i)) => {
                view.armed = false;
                view.click(*i, &self.look)
            }
            (Kind::Menu(view), DialogEvent::MenuConfirm) => Some(view.state.confirm()),
            (Kind::Menu(_), DialogEvent::MenuCancel) => Some(Reply::Cancel),
            (Kind::Choice { allowed, .. }, DialogEvent::Choice(c)) => {
                allowed.contains(c).then_some(Reply::Char(*c as i32))
            }
            (Kind::Text { engrave, .. }, DialogEvent::TextSubmitted(s)) => {
                let s = if *engrave {
                    pickers::latin(s)
                } else {
                    s.clone()
                };
                Some(Reply::Text(truncate_bytes(&s, MAX_TEXT_BYTES)))
            }
            (
                Kind::Picker { picker, .. },
                DialogEvent::TextSubmitted(_) | DialogEvent::PickConfirm,
            ) => {
                let p = picker.borrow();
                p.chosen()
                    .or_else(|| {
                        p.shown
                            .first()
                            .copied()
                            .filter(|_| !p.query.trim().is_empty())
                    })
                    .map(|i| Reply::Text(p.reply(i)))
            }
            (Kind::Picker { picker, .. }, DialogEvent::MenuClick(i)) => {
                let p = picker.borrow();
                p.shown.get(*i).map(|&i| Reply::Text(p.reply(i)))
            }
            (Kind::Picker { .. }, DialogEvent::TextCancelled) => Some(escape),
            (Kind::Text { .. }, DialogEvent::TextCancelled) => Some(escape),
            (Kind::ExtCmd { palette, edit, .. }, DialogEvent::TextSubmitted(text)) => {
                let p = palette.borrow();
                let chosen = p
                    .selected
                    .and_then(|s| p.shown.get(s))
                    .map(|&i| p.cmds[i].name.clone())
                    .or_else(|| {
                        let t = text.trim().to_lowercase();
                        p.cmds.iter().find(|c| c.name == t).map(|c| c.name.clone())
                    });
                match chosen {
                    Some(name) => Some(Reply::ExtCmd(Some(name))),
                    None if text.trim().is_empty() => Some(Reply::ExtCmd(None)),
                    None => {
                        drop(p);
                        edit.call_deferred("grab_focus", &[]);
                        None
                    }
                }
            }
            (Kind::ExtCmd { palette, .. }, DialogEvent::MenuClick(i)) => {
                let p = palette.borrow();
                let name = p.shown.get(*i).map(|&c| p.cmds[c].name.clone())?;
                Some(Reply::ExtCmd(Some(name)))
            }
            (Kind::ExtCmd { .. }, DialogEvent::ExtCmd(cmd)) => Some(Reply::ExtCmd(cmd.clone())),
            (Kind::MessageMenu { letter, pick: true }, DialogEvent::Choice(c)) => {
                if *c == *letter {
                    Some(Reply::Char(*c as i32))
                } else {
                    Some(Reply::Char(ESC))
                }
            }
            (_, DialogEvent::Close) => Some(match &open.kind {
                Kind::MessageMenu { pick: false, .. } => Reply::Char(0),
                _ => escape,
            }),
            _ => None,
        }
    }

    pub fn close(&mut self) {
        if let Some(open) = self.open.take() {
            self.retire(open);
        }
    }

    /// A dialog closed: freed, the palette hidden for the next time.
    fn retire(&mut self, mut open: Open) {
        // the palette is hidden for the next time
        if let Kind::ExtCmd { edit, palette, req } = open.kind {
            open.panel.set_visible(false);
            open.shade.set_visible(false);
            let mut e = edit.clone();
            e.release_focus();
            self.palette_pool = Some(PalettePool {
                shade: open.shade,
                panel: open.panel,
                edit,
                palette,
                req,
            });
            return;
        }
        open.panel.queue_free();
        open.shade.queue_free();
    }
}

/// Frames a dialog of the warm-up is kept (built hidden, then drawn).
const WARM_FRAMES: u32 = 4;

/// One dialog of each kind, built behind the title screen, a kind a frame,
/// and drawn for a few frames: the first real one then costs no font
/// loading, text shaping, frame shader or style set-up (tens of
/// milliseconds in the frame the getpos tip opened).
fn warm_samples() -> Vec<Prompt> {
    let item =
        |idx: i32, ch: char, text: &str, attr: i32, selectable: bool| nh_protocol::MenuItem {
            win: 0,
            idx,
            glyph: None,
            selectable,
            ch: if selectable { ch as i32 } else { 0 },
            gch: 0,
            attr,
            clr: 8,
            str: Some(text.to_string()),
            preselected: false,
            skipinvert: false,
        };
    let line = |attr: i32, text: &str| TextLine {
        attr,
        text: text.to_string(),
    };
    vec![
        Prompt::Show {
            title: Some("Tip".into()),
            lines: vec![
                line(0, "Use '@' to move the cursor on yourself."),
                line(1, "Background:"),
                line(0, " a | b  table  1234567890"),
            ],
        },
        Prompt::Menu {
            win: 0,
            how: PickHow::Any,
            title: Some("Pick up what?".into()),
            items: vec![
                item(0, ' ', "Weapons", 1, false),
                item(1, 'a', "a long sword", 0, true),
                item(2, 'b', "option   [X]  (for autopickup)", 0, true),
            ],
        },
        Prompt::Choice {
            query: "Really attack?".into(),
            visible: vec!['y', 'n'],
            allowed: vec!['y', 'n'],
            default: Some('n'),
        },
        Prompt::Text {
            query: "What do you want to name it?".into(),
            name: false,
        },
        Prompt::ExtCmd,
        Prompt::MessageMenu {
            letter: 'a',
            mesg: "a - a long sword.".into(),
            pick: true,
        },
    ]
}

/// A prompt's kind, by name (RENETHACK_FRAME_STATS).
fn prompt_kind(p: &Prompt) -> &'static str {
    match p {
        Prompt::Show { .. } => "text",
        Prompt::Menu { .. } => "menu",
        Prompt::Choice { .. } => "choice",
        Prompt::Text { .. } => "getlin",
        Prompt::ExtCmd => "palette",
        Prompt::MessageMenu { .. } => "message menu",
        _ => "dialog",
    }
}

/// A prompt with the engine's words in it (the question, a menu's title and
/// entries, a text window's lines, a message) as the player reads them.
fn shown_prompt(p: &Prompt) -> Prompt {
    if i18n::lang() == Lang::En {
        return p.clone();
    }
    let e = |kind: EngineKind, t: &str| i18n::engine(kind, t).into_owned();
    match p {
        Prompt::Menu {
            win,
            how,
            title,
            items,
        } => Prompt::Menu {
            win: *win,
            how: *how,
            title: title.as_deref().map(|t| e(EngineKind::Menu, t)),
            items: items
                .iter()
                .map(|i| nh_protocol::MenuItem {
                    str: i.str.as_deref().map(|t| e(EngineKind::Menu, t)),
                    ..i.clone()
                })
                .collect(),
        },
        Prompt::Choice {
            query,
            visible,
            allowed,
            default,
        } => Prompt::Choice {
            query: e(EngineKind::Prompt, query),
            visible: visible.clone(),
            allowed: allowed.clone(),
            default: *default,
        },
        Prompt::Text { query, name } => Prompt::Text {
            query: e(EngineKind::Prompt, query),
            name: *name,
        },
        Prompt::Show { title, lines } => Prompt::Show {
            title: title.as_deref().map(|t| e(EngineKind::Window, t)),
            lines: lines
                .iter()
                .map(|l| TextLine {
                    attr: l.attr,
                    text: e(EngineKind::Window, &l.text),
                })
                .collect(),
        },
        Prompt::MessageMenu { letter, mesg, pick } => Prompt::MessageMenu {
            letter: *letter,
            mesg: e(EngineKind::Message, mesg),
            pick: *pick,
        },
        other => other.clone(),
    }
}

/// A text window as BBCode, one line per line. NetHack's attributes:
/// 1 bold, 2 dim, 3 italic, 4 underline, 5 blink, 7 inverse.
fn show_text(lines: &[TextLine]) -> String {
    lines
        .iter()
        .map(|l| {
            let t = bbcode_escape(&l.text);
            match l.attr & 0x0f {
                0 => t,
                1 => format!("[b]{t}[/b]"),
                2 => format!("[color={}]{t}[/color]", hex(theme::TEXT_DIM)),
                3 => format!("[i]{t}[/i]"),
                4 => format!("[u]{t}[/u]"),
                // blink, inverse (headings): bold in the accent colour
                _ => format!("[b][color={}]{t}[/color][/b]", hex(theme::ACCENT)),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A text window of prose: as `show_text`, and the lines that head a
/// section ("Background:") in gold.
fn prose_text(lines: &[TextLine]) -> String {
    lines
        .iter()
        .map(|l| {
            let heading = l.attr & 0x0f == 0
                && l.text.ends_with(':')
                && !l.text.starts_with(' ')
                && l.text.len() < 60;
            if heading {
                let t = bbcode_escape(&l.text);
                format!("[b][color={}]{t}[/color][/b]", hex(theme::GOLD_BRIGHT))
            } else {
                show_text(std::slice::from_ref(l))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mark the answer the d-pad is on.
fn show_focus(buttons: &mut [(char, Gd<Button>)], focus: Option<usize>, look: &Look) {
    for (i, (_, b)) in buttons.iter_mut().enumerate() {
        if Some(i) == focus {
            b.add_theme_stylebox_override("normal", &look.focus_button);
            b.add_theme_color_override("font_color", theme::GOLD_BRIGHT);
        } else {
            b.remove_theme_color_override("font_color");
        }
    }
}

/// A key of the on-screen keyboard.
#[derive(Debug, Clone, PartialEq)]
enum OskKey {
    Char(String),
    Space,
    Back,
    Shift,
    Layout,
    Ok,
}

/// The keyboard's rows: digits and letters by layout, then the controls.
const OSK_LATIN: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"];
const OSK_CYRILLIC: [&str; 4] = ["1234567890", "йцукенгшщзх", "фывапролджэ", "ячсмитьбю.ё"];

/// The on-screen keyboard of a text question, driven by a gamepad (or the
/// mouse): the d-pad moves, A types, Y switches Latin and Cyrillic.
struct Osk {
    rows: Vec<Vec<(OskKey, Gd<Button>)>>,
    row: usize,
    col: usize,
    cyrillic: bool,
    upper: bool,
}

impl Osk {
    fn new(col: &mut Gd<VBoxContainer>, req: u64, queue: &UiQueue) -> Osk {
        let mut grid = VBoxContainer::new_alloc();
        grid.add_theme_constant_override("separation", 4);
        let mut rows = Vec::new();
        let specials = [
            (OskKey::Shift, tr!("osk-shift")),
            (OskKey::Layout, tr!("osk-layout")),
            (OskKey::Space, tr!("osk-space")),
            (OskKey::Back, "⌫".to_string()),
            (OskKey::Ok, tr!("dlg-ok")),
        ];
        for r in 0..5 {
            let mut line = HBoxContainer::new_alloc();
            line.set_alignment(AlignmentMode::CENTER);
            line.add_theme_constant_override("separation", 4);
            let mut keys = Vec::new();
            let row: Vec<(OskKey, String)> = if r < 4 {
                (0..11)
                    .map(|_| (OskKey::Char(String::new()), String::new()))
                    .collect()
            } else {
                specials.to_vec()
            };
            for (c, (k, text)) in row.into_iter().enumerate() {
                let mut b = Button::new_alloc();
                b.set_focus_mode(FocusMode::NONE);
                b.set_custom_minimum_size(Vector2::new(if r < 4 { 40.0 } else { 104.0 }, 36.0));
                if r == 4 {
                    b.set_text(&text);
                }
                // a click types the key as A does (the dialog knows which)
                let q = queue.clone();
                let (rr, cc) = (r, c);
                b.signals().pressed().connect(move || {
                    let ev = DialogEvent::OskKey(rr, cc);
                    push(&q, UiEvent::Dialog { req, ev });
                });
                line.add_child(&b);
                keys.push((k, b));
            }
            grid.add_child(&line);
            rows.push(keys);
        }
        col.add_child(&grid);
        let mut hint = theme::styled_label(
            &i18n::whole_parts(&tr!("osk-hint")),
            Face::Body,
            14,
            theme::TEXT_DIM,
        );
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        col.add_child(&hint);
        let mut osk = Osk {
            rows,
            row: 1,
            col: 0,
            cyrillic: false,
            upper: false,
        };
        osk.relabel();
        osk
    }

    /// The letters of the layout and case on the keys.
    fn relabel(&mut self) {
        let layout = if self.cyrillic {
            &OSK_CYRILLIC
        } else {
            &OSK_LATIN
        };
        for (r, line) in layout.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for (c, (k, b)) in self.rows[r].iter_mut().enumerate() {
                let ch = chars.get(c).map(|ch| {
                    if self.upper {
                        ch.to_uppercase().collect::<String>()
                    } else {
                        ch.to_string()
                    }
                });
                *k = OskKey::Char(ch.clone().unwrap_or_default());
                b.set_text(ch.as_deref().unwrap_or(""));
                b.set_visible(ch.is_some());
            }
        }
    }

    fn width(&self, row: usize) -> usize {
        self.rows[row]
            .iter()
            .filter(|(k, _)| !matches!(k, OskKey::Char(c) if c.is_empty()))
            .count()
            .max(1)
    }

    fn step(&mut self, dx: i32, dy: i32, look: &Look) {
        let rows = self.rows.len() as i32;
        self.row = (self.row as i32 + dy).rem_euclid(rows) as usize;
        let w = self.width(self.row) as i32;
        self.col = (self.col as i32 + dx).clamp(0, w - 1) as usize;
        self.show(look);
    }

    fn show(&mut self, look: &Look) {
        for (r, line) in self.rows.iter_mut().enumerate() {
            for (c, (_, b)) in line.iter_mut().enumerate() {
                if (r, c) == (self.row, self.col) {
                    b.add_theme_stylebox_override("normal", &look.focus_button);
                } else {
                    b.remove_theme_stylebox_override("normal");
                }
            }
        }
    }

    fn current(&self) -> OskKey {
        self.rows[self.row][self.col].0.clone()
    }

    fn at(&mut self, row: usize, col: usize) {
        if row < self.rows.len() && col < self.width(row) {
            self.row = row;
            self.col = col;
        }
    }

    fn switch(&mut self) {
        self.cyrillic = !self.cyrillic;
        self.relabel();
        self.col = self.col.min(self.width(self.row) - 1);
    }

    fn shift(&mut self) {
        self.upper = !self.upper;
        self.relabel();
    }
}

/// An empty row at the end of a dialog's column: room a gamepad's hints
/// take inside the panel (`Dialogs::set_pad_room`).
fn add_pad_room(panel: &Gd<PanelContainer>) -> Option<Gd<Control>> {
    let mut col = panel.get_child(0)?.try_cast::<VBoxContainer>().ok()?;
    let mut room = Control::new_alloc();
    room.set_mouse_filter(MouseFilter::IGNORE);
    col.add_child(&room);
    Some(room)
}

fn dialog_ui(req: u64, ev: DialogEvent) -> UiEvent {
    UiEvent::Dialog { req, ev }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(names: &[&str]) -> Vec<PaletteCmd> {
        names
            .iter()
            .map(|n| PaletteCmd {
                name: n.to_string(),
                desc: String::new(),
                key: 0,
            })
            .collect()
    }

    fn names(c: &[PaletteCmd], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| c[i].name.clone()).collect()
    }

    #[test]
    fn palette_filter_puts_prefixes_first_and_short_names_first() {
        let c = cmds(&["pray", "quit", "loot", "quiver", "exploremode", "qq"]);
        assert_eq!(names(&c, &palette_filter(&c, "qu")), ["quit", "quiver"]);
        assert_eq!(names(&c, &palette_filter(&c, "o")), ["loot", "exploremode"]);
        // "r" starts none of these but is inside several
        assert_eq!(
            names(&c, &palette_filter(&c, "r")),
            ["pray", "quiver", "exploremode"]
        );
        // empty: everything, catalog order
        assert_eq!(palette_filter(&c, "").len(), c.len());
        assert_eq!(names(&c, &palette_filter(&c, " QUIT ")), ["quit"]);
    }

    #[test]
    fn palette_filter_puts_the_exact_name_first() {
        // "sit" is exact; "situation" is a longer prefix match; "exsit" an
        // inner match
        let c = cmds(&["exsit", "situation", "sit"]);
        assert_eq!(
            names(&c, &palette_filter(&c, "sit")),
            ["sit", "situation", "exsit"]
        );
        // an exact name beats a shorter inner match
        let c = cmds(&["ab", "xabx", "abc"]);
        assert_eq!(names(&c, &palette_filter(&c, "abc")), ["abc"]);
        let c = cmds(&["lo", "look", "loot"]);
        assert_eq!(names(&c, &palette_filter(&c, "look")), ["look"]);
    }

    #[test]
    fn palette_leaves_out_the_hash_command() {
        assert!(!palette_lists("#"));
        assert!(!palette_lists(""));
        assert!(palette_lists("?"));
        assert!(palette_lists("pray"));
        assert!(palette_cmds(None).is_empty());
    }

    #[test]
    fn palette_tab_completes_the_common_prefix() {
        let c = cmds(&["quit", "quiver", "pray", "prevmsg"]);
        assert_eq!(palette_complete(&c, "q"), "qui");
        assert_eq!(palette_complete(&c, "pra"), "pray");
        assert_eq!(palette_complete(&c, "p"), "pr");
        assert_eq!(palette_complete(&c, "zz"), "zz");
    }

    #[test]
    fn text_is_cut_on_a_character_boundary() {
        assert_eq!(truncate_bytes("abc", 2), "ab");
        // 'ж' is two bytes
        assert_eq!(truncate_bytes("жж", 3), "ж");
        assert_eq!(truncate_bytes("жж", 4), "жж");
        // 200 two-byte letters: 127 of them fit in 255 bytes
        let long = "ж".repeat(200);
        assert_eq!(truncate_bytes(&long, MAX_TEXT_BYTES).chars().count(), 127);
    }

    #[test]
    fn key_names_read_like_nethack() {
        assert_eq!(key_name(0), "");
        assert_eq!(key_name(4), "^D");
        assert_eq!(key_name(0x80 | 'p' as i32), "M-p");
        assert_eq!(key_name('#' as i32), "#");
    }

    #[test]
    fn choice_labels_spell_out_the_usual_answers() {
        assert_eq!(choice_label('y'), "Yes (y)");
        assert_eq!(choice_label('n'), "No (n)");
        assert_eq!(choice_label('q'), "Cancel (q)");
        assert_eq!(choice_label('a'), "All (a)");
        assert_eq!(choice_label('m'), "m");
        assert_eq!(choice_label('?'), "?");
    }

    fn entry(selectable: bool, text: &str, attr: i32) -> MenuEntry {
        MenuEntry {
            idx: 0,
            text: text.to_string(),
            attr,
            selectable,
            letter: selectable.then_some('a'),
            group: None,
            glyph: None,
            skipinvert: false,
            selected: false,
            count: None,
        }
    }

    #[test]
    fn menu_rows_are_headers_info_spacers_or_items() {
        assert_eq!(row_kind(&entry(true, "a dagger", 0)), RowKind::Item);
        assert_eq!(row_kind(&entry(false, "Weapons", 7)), RowKind::Header);
        assert_eq!(row_kind(&entry(false, "Weapons", 1)), RowKind::Header);
        assert_eq!(row_kind(&entry(false, "(not carried)", 0)), RowKind::Info);
        assert_eq!(row_kind(&entry(false, "  ", 7)), RowKind::Spacer);
        // ATR_NOHISTORY and ATR_URGENT are no display attributes
        assert_eq!(row_kind(&entry(false, "note", 32)), RowKind::Info);
    }

    #[test]
    fn menu_marks_show_selection_and_counts() {
        let mut e = entry(true, "arrows", 0);
        assert_eq!(mark_text(&e), "");
        e.selected = true;
        assert_eq!(mark_text(&e), "✔");
        e.count = Some(12);
        assert_eq!(mark_text(&e), "12");
    }

    #[test]
    fn engine_columns_split_on_runs_of_spaces() {
        assert_eq!(
            fields("pickup_stolen      [X]  (for autopickup)"),
            ["pickup_stolen", "[X]", "(for autopickup)"]
        );
        assert_eq!(fields("autoquiver   [ ]"), ["autoquiver", "[ ]"]);
        assert_eq!(fields("All types"), ["All types"]);
        assert_eq!(fields("enhance\t[A] advance"), ["enhance", "[A] advance"]);
        assert_eq!(option_value("[X]"), Some(OptionValue::On));
        assert_eq!(option_value("[ ]"), Some(OptionValue::Off));
        assert_eq!(option_value("[all]"), Some(OptionValue::Set("all".into())));
        assert_eq!(option_value("[A] advance"), None);
    }

    #[test]
    fn show_text_escapes_and_styles_lines() {
        let line = |attr, text: &str| TextLine {
            attr,
            text: text.to_string(),
        };
        let s = show_text(&[line(0, " [a] |  x"), line(1, "Bold"), line(3, "it")]);
        assert_eq!(s, " [lb]a] |  x\n[b]Bold[/b]\n[i]it[/i]");
    }
}
