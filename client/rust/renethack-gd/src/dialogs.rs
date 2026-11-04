//! Modal dialogs for menus, questions, text input, the extended-command
//! palette and text windows. At most one is open, for one request; every
//! event it queues carries that request's id.
//!
//! Lists (menus, the palette) are rows of a fixed height in a scroll
//! container, so a dialog knows where each row is and pages and reveals rows
//! itself. No dialog grows past the window: long content scrolls.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, GrowDirection, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, DisplayServer, Font, HBoxContainer, InputEvent,
    InputEventMouseButton, Label, LineEdit, MarginContainer, OptionButton, PanelContainer,
    RichTextLabel, ScrollContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat, Texture2D, TextureRect,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::{Catalog, ESC, PickHow, Reply};
use nh_world::{Key, KeyInput, MenuEntry, MenuOutcome, MenuState, Prompt, TextLine, choice_answer};

use nh_i18n::lexicon::Lexicon;

use crate::cell_menu::{self, CellMenu};
use crate::codex;
use crate::codex_view;
use crate::command_menu::{self, Section};
use crate::command_wheel::{self, CommandWheel, WheelAct};
use crate::i18n::{self, EngineKind, Lang};
use crate::journal;
use crate::journal_view::{self, JournalView};
use crate::layouts::{self, Genocided, GoneRow, KillRow, OverviewRow, Vanquished};
use crate::loot_view::LootPanel;
use crate::pad_view;
use crate::pickers::{self, Ask, ObjClass, Pick, Wish};
use crate::sheet;
use crate::sheet_view::{self, SheetView};
use crate::skills;
use crate::skills_view;
use crate::spells::{self, Purpose, Spellbook};
use crate::spells_view;
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
/// An RPG panel's own chrome: its header, its rule, its hint, its frame's
/// margins (the room above and below it is `panel_room`'s); and the air
/// between it and the screen's top or the HUD's foot.
const PANEL_OWN_CHROME: f32 = 150.0;
const PANEL_EDGE: f32 = 12.0;
/// An RPG panel's widest content, and the room its scroll bar takes; the
/// spells' panel's.
const PANEL_W: f32 = 1120.0;
const SPELLS_W: f32 = 760.0;
/// The journal's widest and tallest.
const JOURNAL_W: f32 = 1040.0;
const JOURNAL_H: f32 = 640.0;
const SCROLL_ROOM: f32 = 18.0;
/// The same for a text window (no footer).
const TEXT_CHROME: f32 = 220.0;
/// The canvas the HUD is designed for (theme::apply_scaling).
const REFERENCE_SCREEN: Vector2 = theme::DESIGN;
/// Space kept free left and right of a dialog.
const SIDE_MARGIN: f32 = 80.0;
/// Rows the palette shows at once: the common commands and their heading
/// whole, the next heading under them.
const PALETTE_ROWS: f32 = 18.0;
/// A command's name beside its words, and the palette's gap above the
/// HUD's Commands button it hangs from.
const NAME_SIZE: i32 = 14;
const PALETTE_GAP: f32 = 8.0;
/// The room the command wheel keeps above the HUD's foot besides the
/// foot itself: the upper row of buttons beside a docked log, a pad's
/// hints.
const WHEEL_FOOT: f32 = 100.0;
/// The fewest rows the palette hung from the button shows (else it opens
/// centred).
const PALETTE_LEAST_ROWS: f32 = 6.0;
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
    /// Its key as the player presses it now ("" for none).
    pub key: String,
    /// Other words that find it ("give up" finds #quit, "молитва"
    /// #pray), lower case.
    pub also: Vec<String>,
    /// A prefix for the key to come ("prefix: rush until…"), no command
    /// of its own.
    pub prefix: bool,
}

/// Does the palette list this command? Not "#" itself: choosing it would
/// only ask for an extended command again.
fn palette_lists(name: &str) -> bool {
    !name.is_empty() && name != "#"
}

/// The catalog's extended commands the palette offers, each with its key
/// as the player presses it now (the keys moved in `keymap`).
pub fn palette_cmds(catalog: Option<&Catalog>, keymap: &nh_world::Keymap) -> Vec<PaletteCmd> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    catalog
        .extcmds
        .iter()
        .filter(|e| palette_lists(&e.name))
        .map(|e| {
            // #quit is giving up: said so
            let quit = e.name == "quit";
            PaletteCmd {
                name: e.name.clone(),
                // the engine's words: its translator gives them
                desc: if quit {
                    tr!("palette-give-up")
                } else {
                    i18n::engine(EngineKind::Menu, &e.desc).into_owned()
                },
                key: crate::rebind::bound_code(keymap, e.key),
                prefix: e.desc.starts_with("prefix:"),
                // the names it goes by in the player's language
                also: tr!("palette-also", cmd = e.name.as_str())
                    .split(',')
                    .map(|w| w.trim().to_lowercase())
                    .filter(|w| !w.is_empty())
                    .collect(),
            }
        })
        .collect()
}

/// `s` with its first letter upper-cased (a command's description read
/// as the palette's row).
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Lower case, ё as е: a search's text and what it is compared with.
fn folded(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

/// `s` or one of its words starts with `text`.
fn word_starts(s: &str, text: &str) -> bool {
    s.starts_with(text)
        || s.split(|c: char| !c.is_alphanumeric())
            .any(|w| w.starts_with(text))
}

/// Commands matching `text`: the exact name first, then the names it
/// starts, then a word of the other names (`also`, in the player's
/// language: "моли" finds #pray) or of the description it starts, then
/// the names it is inside; each shortest first (then by name). An empty
/// `text` lists everything in catalog order.
pub fn palette_filter(cmds: &[PaletteCmd], text: &str) -> Vec<usize> {
    let text = folded(text.trim());
    if text.is_empty() {
        return (0..cmds.len()).collect();
    }
    let mut hits: Vec<(u8, usize)> = cmds
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let name = folded(&c.name);
            let also: Vec<String> = c.also.iter().map(|a| folded(a)).collect();
            let tier = if name == text {
                0
            } else if name.starts_with(&text) {
                1
            } else if also.iter().any(|a| word_starts(a, &text))
                || word_starts(&folded(&c.desc), &text)
            {
                2
            } else if name.contains(&text) || also.iter().any(|a| a.contains(&text)) {
                3
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

/// #therecmdmenu's question (a click or A on a cell), and its entry
/// that only looks (`:`) when the cell is the hero's.
const THERE_MENU: &str = "What do you want to do?";
const LOOK_HERE: &str = "Look at what is here";

/// The word the question names answer `c` by ("Which ring-finger, Right
/// or Left? [rl]": Right for r), as the question shown says it: the shown
/// question's word that begins as the language's stem of it does («На
/// какую руку надеть кольцо: правую или левую?»: Правую).
fn answer_word(c: char, english: &str, shown: &str) -> Option<String> {
    let words = |q: &str| -> Vec<String> {
        q.split(|ch: char| !ch.is_alphabetic())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let up = c.to_ascii_uppercase();
    let named = words(english).into_iter().skip(1).find(|w| {
        w.chars().count() > 1 && w.starts_with(up) && w.chars().skip(1).all(char::is_lowercase)
    })?;
    if i18n::lang() == Lang::En {
        return Some(named);
    }
    // (a word the language has no stem for finds none)
    let stem = tr!("choice-stem", word = named.as_str()).to_lowercase();
    let said = words(shown)
        .into_iter()
        .find(|w| w.to_lowercase().starts_with(&stem))?;
    let mut chars = said.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
}

/// An answer's button: the usual answers spelled out, another as the
/// question names it (`named`), else its letter; with its key unless a
/// gamepad plays.
fn named_choice(c: char, pad: bool, named: Option<&str>) -> String {
    if let Some(w) = named {
        return if pad {
            w.to_string()
        } else {
            tr!("dlg-named-answer", word = w, key = c.to_string())
        };
    }
    match (c, pad) {
        ('y', false) => tr!("dlg-yes"),
        ('n', false) => tr!("dlg-no"),
        ('q', false) => tr!("dlg-cancel-q"),
        ('a', false) => tr!("dlg-all-a"),
        ('y', true) => tr!("dlg-yes-pad"),
        ('n', true) => tr!("dlg-no-pad"),
        ('q', true) => tr!("dlg-cancel-q-pad"),
        ('a', true) => tr!("dlg-all-a-pad"),
        (c, _) => c.to_string(),
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

/// `text` ends a sentence (or a colon), maybe in its closing quote:
/// NetHack puts two spaces after one.
fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', '»', '”', ')'])
        .ends_with(['.', '!', '?', ':', '…'])
}

/// A row's cells as a table has them: parted by a run of three spaces or
/// more, a tab, or two spaces not after a sentence's end (prose has those).
fn table_cells(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut spaces = 0;
    for c in text.trim().chars() {
        match c {
            ' ' => spaces += 1,
            '\t' => spaces = 3,
            _ => {
                let sentence = ends_sentence(&cur);
                if !cur.is_empty() && (spaces >= 3 || (spaces == 2 && !sentence)) {
                    out.push(std::mem::take(&mut cur));
                } else if spaces > 0 && !cur.is_empty() {
                    cur.push_str(&" ".repeat(spaces));
                }
                spaces = 0;
                cur.push(c);
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A table's row in the language now, only while its translation keeps
/// its cells apart: one that runs them together keeps the engine's
/// English, in its columns.
fn row_kept(english: &str, shown: String) -> String {
    let cells = table_cells(english).len();
    if cells >= 2 && table_cells(&shown).len() < cells {
        english.to_string()
    } else {
        shown
    }
}

/// A text window's lines in the language now, its tables keeping their
/// columns in the monospace face: a block of rows of as many cells is
/// laid out again when the translation changed it, each cell padded to
/// its column; a row whose translation ran its cells together makes the
/// block keep the engine's English.
fn tables_kept(english: &[&str], shown: Vec<String>) -> Vec<String> {
    if english.len() != shown.len() {
        return shown;
    }
    let cells: Vec<Vec<String>> = english.iter().map(|t| table_cells(t)).collect();
    let mut out = shown;
    let mut i = 0;
    while i < english.len() {
        let n = cells[i].len();
        let mut j = i + 1;
        while j < english.len() && cells[j].len() == n {
            j += 1;
        }
        if n >= 2 && (i..j).any(|k| out[k] != english[k]) {
            let rows: Vec<Vec<String>> = out[i..j].iter().map(|t| table_cells(t)).collect();
            if rows.iter().all(|r| r.len() == n) {
                let width: Vec<usize> = (0..n)
                    .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
                    .collect();
                for (k, row) in (i..j).zip(&rows) {
                    let mut line: String = english[k].chars().take_while(|c| *c == ' ').collect();
                    for (c, cell) in row.iter().enumerate() {
                        line.push_str(cell);
                        if c + 1 < n {
                            let pad = width[c] - cell.chars().count() + 2;
                            line.push_str(&" ".repeat(pad));
                        }
                    }
                    out[k] = line;
                }
            } else {
                for k in i..j {
                    out[k] = english[k].to_string();
                }
            }
        }
        i = j;
    }
    out
}

/// The engine's question before a game is given up (#quit), plain or
/// paranoid ("Really quit without saving? [yes|n] (n)", and its "Yes" or
/// "No" again).
fn quits(english: &str) -> bool {
    english
        .trim_start_matches("\"Yes\" or \"No\": ")
        .starts_with("Really quit without saving?")
}

/// A debug death's question in the client's words: done()'s "Die?" and
/// really_done()'s "Save bones?" came one after the other in the same
/// frame and buttons, as one dialog twice. Each has its own title, a line
/// of what it means and its answers' words; NetHack's answers stay.
struct DebugEnd {
    title: String,
    about: String,
    yes: String,
    no: String,
}

fn debug_end(english: &str) -> Option<DebugEnd> {
    let [title, about, yes, no] = match english.trim() {
        "Die?" => [
            "dlg-debug-die-title",
            "dlg-debug-die-about",
            "dlg-debug-die-yes",
            "dlg-debug-die-no",
        ],
        "Save bones?" => [
            "dlg-debug-bones-title",
            "dlg-debug-bones-about",
            "dlg-debug-bones-yes",
            "dlg-debug-bones-no",
        ],
        _ => return None,
    };
    Some(DebugEnd {
        title: i18n::tr(title),
        about: i18n::tr(about),
        yes: i18n::tr(yes),
        no: i18n::tr(no),
    })
}

/// A line under a question's title, saying what it means.
fn about_line(text: &str, width: f32) -> Gd<Label> {
    let mut l = theme::styled_label(text, Face::Body, 16, theme::TEXT_DIM);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_custom_minimum_size(Vector2::new(width, 0.0));
    l
}

/// What giving up means, under the engine's question: in the danger's
/// colour, as wide as the question.
fn quit_warning(width: f32) -> Gd<Label> {
    let mut l = theme::styled_label(&tr!("quit-warning"), Face::Body, 16, theme::DANGER);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_custom_minimum_size(Vector2::new(width, 0.0));
    l
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
            focus_button: theme::focus_style(),
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

/// The creature lists' badge, and the overview's.
const TOKEN_W: f32 = 26.0;
const BADGE_W: f32 = 34.0;
/// A line under a level of the overview.
const DETAIL_H: f32 = 26.0;

/// A row of a list of creatures as shown.
enum CreatureRow {
    Class(String),
    Kind {
        symbol: Option<(char, Color)>,
        name: String,
        unique: bool,
        /// How many, or what became of them.
        right: String,
    },
}

/// A creature's map symbol and colour, from the catalog.
fn creature_symbol(catalog: Option<&Catalog>, english: &str) -> Option<(char, Color)> {
    let m = layouts::monster(catalog, english)?;
    Some((m.class.chars().next()?, nh_color(m.color)))
}

/// A row of the overview as shown.
enum OverviewLine {
    Dungeon {
        name: String,
        levels: Option<String>,
    },
    Level {
        badge: String,
        title: String,
        note: Option<String>,
        here: Option<String>,
        /// The hero is on it now.
        current: bool,
    },
    Detail {
        mark: &'static str,
        text: String,
        /// One of the dead under "Final resting place for".
        deep: bool,
    },
}

impl OverviewLine {
    fn of(row: &OverviewRow) -> OverviewLine {
        let detail = |mark: &'static str, text: String| OverviewLine::Detail {
            mark,
            text,
            deep: false,
        };
        match row {
            OverviewRow::Dungeon { name, levels, up } => OverviewLine::Dungeon {
                name: layouts::dungeon_name(name),
                levels: levels.map(|(a, b)| layouts::levels(a, b, *up)),
            },
            OverviewRow::Level {
                place,
                proto,
                note,
                here,
            } => OverviewLine::Level {
                badge: match place {
                    layouts::Place::Level(n) => n.to_string(),
                    layouts::Place::Astral => "✶".into(),
                    _ => "◆".into(),
                },
                title: match proto {
                    Some(p) => format!("{} [{}]", layouts::place(place), layouts::typed(p)),
                    None => layouts::place(place),
                },
                note: note.as_deref().map(layouts::note),
                here: here.map(layouts::here),
                current: *here == Some(layouts::Here::Are),
            },
            OverviewRow::Features(fs) => detail(
                "•",
                fs.iter()
                    .map(|f| layouts::upstart(&layouts::feature_text(f)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            ),
            OverviewRow::Special(s) => detail("◇", layouts::special(s)),
            OverviewRow::Summoned(l) => detail("◇", layouts::summoned(l)),
            OverviewRow::Branch { kind, to, level } => {
                detail("↳", layouts::branch(*kind, to, *level))
            }
            OverviewRow::RestingPlace => detail("✝", layouts::resting(false)),
            OverviewRow::Dead { who, how } => OverviewLine::Detail {
                mark: "",
                text: layouts::dead(who.as_deref(), how),
                deep: true,
            },
            OverviewRow::Other(t) => detail("", i18n::engine(EngineKind::Menu, t).into_owned()),
        }
    }
}

/// A window the client lays out (#vanquished, #genocided, #overview) as
/// words, for a page that shows it read-only (the end's disclosures): its
/// heading and its lines in the language now, true: a heading. None: no
/// such window.
pub fn layout_words(
    texts: &[&str],
    catalog: Option<&Catalog>,
) -> Option<(String, Vec<(String, bool)>)> {
    let plurals = layouts::kinds_by_plural(catalog);
    let singular = |p: &str| plurals.get(p).cloned();
    // a kind as NetHack's list says one: a common name small ("тритон",
    // as "a newt"), a unique one as it is
    let creature = |r: &CreatureRow| match r {
        CreatureRow::Class(c) => (c.clone(), true),
        CreatureRow::Kind {
            name,
            right,
            unique,
            ..
        } => {
            let name = if *unique { name.clone() } else { lowered(name) };
            match right.as_str() {
                "" | "1" => (name, false),
                n => (format!("{name} — {n}"), false),
            }
        }
    };
    // in English NetHack's own lines ("a newt", "2 jackals"), its heading
    // the page's
    let own = |texts: &[&str]| -> Vec<(String, bool)> {
        (texts.iter().skip(1))
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| (l.to_string(), l.ends_with(':')))
            .collect()
    };
    let english = i18n::lang() == Lang::En;
    if let Some(v) = layouts::vanquished(texts, &singular) {
        if english {
            return Some((tr!("vanquished-title"), own(texts)));
        }
        let mut lines: Vec<(String, bool)> =
            vanquished_rows(&v, catalog).iter().map(creature).collect();
        lines.extend((v.total.iter()).map(|&n| (tr!("vanquished-total", count = n), false)));
        return Some((tr!("vanquished-title"), lines));
    }
    if let Some(g) = layouts::genocided(texts) {
        if english {
            return Some((layouts::gone_heading(g.heading), own(texts)));
        }
        let mut lines: Vec<(String, bool)> = (genocided_rows(&g, &plurals, catalog).iter())
            .map(creature)
            .collect();
        if let Some(n) = g.genocided {
            lines.push((tr!("genocided-total", count = n), false));
        }
        if let Some(n) = g.extinct {
            lines.push((tr!("extinct-total", count = n), false));
        }
        return Some((layouts::gone_heading(g.heading), lines));
    }
    let rows = layouts::overview(texts)?;
    let lines = (overview_lines(&rows).into_iter())
        .map(|l| match l {
            OverviewLine::Dungeon { name, levels } => match levels {
                Some(levels) => (format!("{name}: {levels}"), true),
                None => (name, true),
            },
            OverviewLine::Level {
                badge,
                title,
                note,
                here,
                ..
            } => {
                let mut t = format!("{badge}. {title}");
                for more in [note, here].into_iter().flatten() {
                    t = format!("{t} — {more}");
                }
                (t, false)
            }
            OverviewLine::Detail { mark: "", text, .. } => (text, false),
            OverviewLine::Detail { mark, text, .. } => (format!("{mark} {text}"), false),
        })
        .collect();
    Some((tr!("overview-title"), lines))
}

/// `s` with its first letter lower-cased.
fn lowered(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

/// #vanquished's rows: the classes' headings, a kind a row with its count.
fn vanquished_rows(v: &Vanquished, catalog: Option<&Catalog>) -> Vec<CreatureRow> {
    v.rows
        .iter()
        .map(|r| match r {
            KillRow::Class(c) => CreatureRow::Class(layouts::class_name(c)),
            KillRow::Kind {
                name,
                count,
                unique,
            } => CreatureRow::Kind {
                symbol: creature_symbol(catalog, name),
                name: layouts::name(name),
                unique: *unique,
                right: count.to_string(),
            },
        })
        .collect()
}

/// #genocided's rows: the classes' headings, a species a row, the extinct
/// ones said so.
fn genocided_rows(
    g: &Genocided,
    plurals: &HashMap<String, String>,
    catalog: Option<&Catalog>,
) -> Vec<CreatureRow> {
    g.rows
        .iter()
        .map(|r| match r {
            GoneRow::Class(c) => CreatureRow::Class(layouts::class_name(c)),
            GoneRow::Species { plural, extinct } => CreatureRow::Kind {
                symbol: creature_symbol(
                    catalog,
                    plurals.get(plural).map_or(plural.as_str(), String::as_str),
                ),
                name: layouts::name(plural),
                unique: false,
                right: if *extinct {
                    tr!("genocided-extinct")
                } else {
                    String::new()
                },
            },
        })
        .collect()
}

/// The overview's rows as shown. The dead of a level come under their
/// heading: the hero under one of their own, the bones' heroes under the
/// other, which shows only where its words differ ("Final resting place
/// for" over both in English; "Здесь покоитесь вы", then "Здесь покоятся").
fn overview_lines(rows: &[OverviewRow]) -> Vec<OverviewLine> {
    let mut out = Vec::with_capacity(rows.len() + 1);
    let mut i = 0;
    while i < rows.len() {
        if rows[i] != OverviewRow::RestingPlace {
            out.push(OverviewLine::of(&rows[i]));
            i += 1;
            continue;
        }
        let dead: Vec<&OverviewRow> = rows[i + 1..]
            .iter()
            .take_while(|r| matches!(r, OverviewRow::Dead { .. }))
            .collect();
        let (hero, others): (Vec<&OverviewRow>, Vec<&OverviewRow>) = dead
            .iter()
            .partition(|r| matches!(r, OverviewRow::Dead { who: None, .. }));
        let heading = |text: String| OverviewLine::Detail {
            mark: "✝",
            text,
            deep: false,
        };
        let first = layouts::resting(!hero.is_empty());
        out.push(heading(first.clone()));
        out.extend(hero.iter().map(|r| OverviewLine::of(r)));
        if !hero.is_empty() && !others.is_empty() {
            let theirs = layouts::resting(false);
            if theirs != first {
                out.push(heading(theirs));
            }
        }
        out.extend(others.iter().map(|r| OverviewLine::of(r)));
        i += 1 + dead.len();
    }
    out
}

/// A thin gold line under a heading.
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

/// Where the hero is, on a pill at the end of the level's row: in gold
/// for the level the hero is on, dimmer at the game's end.
fn here_pill(text: &str, current: bool, look: &Look) -> Gd<Label> {
    let color = if current { theme::ACCENT } else { theme::GOLD };
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color { a: 0.14, ..color });
    sb.set_border_width_all(1);
    sb.set_border_color(Color { a: 0.8, ..color });
    sb.set_corner_radius_all(10);
    sb.set_content_margin(godot::builtin::Side::LEFT, 10.0);
    sb.set_content_margin(godot::builtin::Side::RIGHT, 10.0);
    sb.set_content_margin(godot::builtin::Side::TOP, 1.0);
    sb.set_content_margin(godot::builtin::Side::BOTTOM, 1.0);
    let w = look.text_w(text, &look.body_bold, BODY_SIZE - 2) + 22.0;
    let mut l = cell(text, color, Some(&look.body_bold), Some(w));
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.add_theme_font_size_override("font_size", BODY_SIZE - 2);
    l.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    l.add_theme_stylebox_override("normal", &sb);
    l
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
    /// Top of each row in the list, and its height (a spell's row is
    /// taller than an item's).
    tops: Vec<f32>,
    heights: Vec<f32>,
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
    /// The hint of a panel that says its own (the spells'), else the
    /// menu's by what it picks.
    own_hint: Option<String>,
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
        let hint = if let Some(own) = &self.own_hint {
            own.clone()
        } else if !pickable {
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

    /// Row `i`'s height.
    fn height(&self, i: usize) -> f32 {
        self.heights.get(i).copied().unwrap_or(ROW_H)
    }

    /// Is row `i` wholly in view?
    fn in_view(&self, i: usize) -> bool {
        let (v, h) = (self.list.top(), self.list.view_h);
        let rh = self.height(i);
        self.tops
            .get(i)
            .is_some_and(|&t| t >= v - 0.5 && t + rh <= v + h + 0.5)
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
        let rh = self.height(i);
        if top < v {
            self.list.scroll_to(top);
        } else if top + rh > v + h {
            let least = top + rh - h;
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

/// One command's row; rows are slots showing the commands in order.
#[derive(Clone)]
struct PaletteSlot {
    button: Gd<Button>,
    name: Gd<Label>,
    key: Gd<Label>,
    desc: Gd<Label>,
}

/// A section's heading: its fold mark, its title, how many it holds.
#[derive(Clone)]
struct HeadSlot {
    button: Gd<Button>,
    mark: Gd<TextureRect>,
    title: Gd<Label>,
    count: Gd<Label>,
}

/// A row shown: a section's heading (an index into `groups`) or a
/// command's slot (into `slots`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteLine {
    Head(usize),
    Slot(usize),
}

/// What Enter or A does on the row highlighted.
enum PalettePick {
    Cmd(String),
    Head(usize),
}

/// A section as the palette lays it out: its heading, then the slots for
/// as many commands as it holds; the commands in them now, and whether
/// they are folded away.
struct PaletteGroup {
    section: Section,
    head: HeadSlot,
    slots: std::ops::Range<usize>,
    cmds: Vec<usize>,
    folded: bool,
}

/// The command menu (`#`): every extended command in its section, each
/// section folded or not; typed text lists the matches alone, the best
/// first, no headings.
struct Palette {
    cmds: Vec<PaletteCmd>,
    groups: Vec<PaletteGroup>,
    /// The rows shown, top down, each `ROW_H` tall.
    lines: Vec<PaletteLine>,
    /// The command each slot shows (an index into `cmds`).
    slot_cmd: Vec<Option<usize>>,
    /// Typed text filters the list: the matches in one list.
    filtered: bool,
    /// The row highlighted (an index into `lines`): Enter runs it.
    selected: Option<usize>,
    slots: Vec<PaletteSlot>,
    /// The row styled as the highlighted one: restyling every row is a
    /// theme change each, too slow to do on every keystroke.
    styled: Option<PaletteLine>,
    list: Scroller,
    none: Gd<Label>,
    look: Look,
    /// A gamepad plays: each row reads as what it does, no name to type
    /// nor key, and no moves (the stick's).
    pad: bool,
    /// Goods not paid for: #pay first.
    owes: bool,
    /// The rows drawn as `pad` says (restyled only when it changes).
    drawn_pad: Option<bool>,
    /// Its heading: "Commands", or "All commands" (the game menu's entry)
    /// under a pad.
    title: Option<Gd<Label>>,
    /// The fold marks: a section folded (▸), open (▾).
    marks: [Gd<Texture2D>; 2],
    /// Its width, and whether it hangs from the HUD's Commands button (no
    /// Cancel, nothing dimmed; a click beside it closes it).
    width: f32,
    dropdown: bool,
    cancel_row: Gd<VBoxContainer>,
    /// Its list's height centred; hung from the button, it may show fewer
    /// rows.
    full_h: f32,
    /// The sections' folds as the player left them, while a wheel's
    /// "More…" shows one section alone.
    folds_kept: Option<Vec<bool>>,
}

impl Palette {
    /// The commands shown for `text`: the sections, or the matches alone.
    fn refilter(&mut self, text: &str) {
        let text = text.trim();
        self.filtered = !text.is_empty();
        self.slot_cmd.iter_mut().for_each(|s| *s = None);
        if self.filtered {
            let hits = palette_filter(&self.cmds, text);
            for (slot, c) in self.slot_cmd.iter_mut().zip(hits) {
                *slot = Some(c);
            }
        } else {
            let laid = command_menu::sections(&self.cmds, self.owes, self.pad);
            for (g, (_, idx)) in self.groups.iter_mut().zip(laid) {
                g.cmds = idx;
                g.cmds.truncate(g.slots.len());
                for (k, &c) in g.cmds.iter().enumerate() {
                    self.slot_cmd[g.slots.start + k] = Some(c);
                }
            }
        }
        self.relines();
        // the best match is Enter's; a pad's focus starts on the first
        // command (its A runs it); the keys' on none until an arrow
        self.selected = if self.filtered {
            (!self.lines.is_empty()).then_some(0)
        } else if self.pad {
            self.lines
                .iter()
                .position(|l| matches!(l, PaletteLine::Slot(_)))
        } else {
            None
        };
    }

    /// The rows shown, as the sections are folded now.
    fn relines(&mut self) {
        self.lines.clear();
        if self.filtered {
            let shown = self.slot_cmd.iter().enumerate();
            let shown = shown.filter(|(_, c)| c.is_some()).map(|(s, _)| s);
            self.lines.extend(shown.map(PaletteLine::Slot));
            return;
        }
        for (gi, g) in self.groups.iter().enumerate() {
            if g.cmds.is_empty() {
                continue;
            }
            self.lines.push(PaletteLine::Head(gi));
            if !g.folded {
                let rows = g.slots.start..g.slots.start + g.cmds.len();
                self.lines.extend(rows.map(PaletteLine::Slot));
            }
        }
    }

    /// The section of the row highlighted (none while filtering).
    fn selected_group(&self) -> Option<usize> {
        if self.filtered {
            return None;
        }
        match self.lines.get(self.selected?)? {
            PaletteLine::Head(g) => Some(*g),
            PaletteLine::Slot(s) => self.groups.iter().position(|g| g.slots.contains(s)),
        }
    }

    /// Fold section `g` (`fold`), or unfold it, or (None) the other way
    /// round; the highlight stays on its row, or goes to the heading of
    /// the rows it was on. False when nothing changed.
    fn fold(&mut self, g: usize, fold: Option<bool>) -> bool {
        if self.filtered {
            return false;
        }
        let Some(group) = self.groups.get_mut(g) else {
            return false;
        };
        let fold = fold.unwrap_or(!group.folded);
        if group.folded == fold {
            return false;
        }
        group.folded = fold;
        let was = self.selected.and_then(|i| self.lines.get(i)).copied();
        self.relines();
        let at = |l: PaletteLine| self.lines.iter().position(|&m| m == l);
        self.selected = was.and_then(|l| at(l).or_else(|| at(PaletteLine::Head(g))));
        true
    }

    /// What the row highlighted does.
    fn picked(&self) -> Option<PalettePick> {
        match *self.lines.get(self.selected?)? {
            PaletteLine::Head(g) => Some(PalettePick::Head(g)),
            PaletteLine::Slot(s) => {
                let c = self.slot_cmd.get(s).copied().flatten()?;
                Some(PalettePick::Cmd(self.cmds[c].name.clone()))
            }
        }
    }

    /// Section `only` alone open (a wheel's "More…"), the player's folds
    /// kept aside; None: the player's folds back.
    fn only(&mut self, only: Option<Section>) {
        match only {
            Some(s) => {
                if self.folds_kept.is_none() {
                    self.folds_kept = Some(self.groups.iter().map(|g| g.folded).collect());
                }
                for g in self.groups.iter_mut() {
                    g.folded = g.section != s;
                }
            }
            None => {
                if let Some(kept) = self.folds_kept.take() {
                    for (g, f) in self.groups.iter_mut().zip(kept) {
                        g.folded = f;
                    }
                }
            }
        }
    }

    /// The highlight on section `s`'s heading.
    fn on_section(&mut self, s: Option<Section>) {
        let Some(s) = s else {
            return;
        };
        if let Some(g) = self.groups.iter().position(|g| g.section == s) {
            self.selected = self.lines.iter().position(|&l| l == PaletteLine::Head(g));
        }
    }

    /// The highlight `step` rows on, kept within the list.
    fn step(&mut self, step: i64) {
        let n = self.lines.len() as i64;
        if n == 0 {
            return;
        }
        let cur = self.selected.map_or(-1, |s| s as i64);
        let next = if cur < 0 && step < 0 {
            n - 1
        } else {
            (cur + step).clamp(0, n - 1)
        };
        self.selected = Some(next as usize);
    }
}

/// The commands' names the palette lists, top down, a folded section's
/// too (self-tests).
fn listed_names(p: &Palette) -> Vec<String> {
    (p.slot_cmd.iter().flatten())
        .map(|&c| p.cmds[c].name.clone())
        .collect()
}

/// A command's row as it reads: what it does, its first letter a capital.
pub fn palette_row_words(desc: &str) -> String {
    capitalized(desc)
}

/// A small solid triangle, white (the row tints it), pointing right (a
/// section folded) or down (open).
fn fold_mark(down: bool) -> Gd<Texture2D> {
    const S: usize = 14;
    const SS: usize = 4;
    let mut px = vec![0u8; S * S * 4];
    let s = S as f32;
    for y in 0..S {
        for x in 0..S {
            let mut inside = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // along the point's way (u) and across it (v)
                    let (u, v) = if down { (fy, fx) } else { (fx, fy) };
                    let u = (u - 0.2 * s) / (0.6 * s);
                    if (0.0..=1.0).contains(&u) && (v - s / 2.0).abs() <= (1.0 - u) * 0.42 * s {
                        inside += 1;
                    }
                }
            }
            let i = (y * S + x) * 4;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, (inside * 255 / (SS * SS)) as u8]);
        }
    }
    godot::classes::Image::create_from_data(
        S as i32,
        S as i32,
        false,
        godot::classes::image::Format::RGBA8,
        &PackedByteArray::from(px.as_slice()),
    )
    .and_then(|image| godot::classes::ImageTexture::create_from_image(&image))
    .map_or_else(
        || godot::classes::ImageTexture::new_gd().upcast(),
        |t| t.upcast(),
    )
}

/// Show the palette's rows. The borrow is released before Godot is called.
fn fill_palette(palette: &Rc<RefCell<Palette>>) {
    struct Head {
        node: HeadSlot,
        shown: bool,
        folded: bool,
        count: usize,
        title: String,
    }
    let (pad, restyle) = {
        let p = palette.borrow();
        (p.pad, p.drawn_pad != Some(p.pad))
    };
    let (rows, heads, selected, styled, slots, look, marks, mut none) = {
        let p = palette.borrow();
        let mut shown = vec![false; p.slots.len()];
        let mut head_shown = vec![false; p.groups.len()];
        for l in &p.lines {
            match *l {
                PaletteLine::Head(g) => head_shown[g] = true,
                PaletteLine::Slot(s) => shown[s] = true,
            }
        }
        let rows: Vec<Option<(String, String, String, bool)>> = (p.slot_cmd.iter().zip(&shown))
            .map(|(c, &shown)| {
                c.map(|c| {
                    let c = &p.cmds[c];
                    (
                        format!("#{}", c.name),
                        c.key.clone(),
                        palette_row_words(&c.desc),
                        shown,
                    )
                })
            })
            .collect();
        let heads: Vec<Head> = (p.groups.iter().zip(head_shown))
            .map(|(g, shown)| Head {
                node: g.head.clone(),
                shown,
                folded: g.folded,
                count: g.cmds.len(),
                title: i18n::tr(g.section.title_id()),
            })
            .collect();
        let line = |i: Option<usize>| i.and_then(|i| p.lines.get(i)).copied();
        (
            rows,
            heads,
            line(p.selected),
            p.styled,
            p.slots.clone(),
            p.look.clone(),
            p.marks.clone(),
            p.none.clone(),
        )
    };
    for (i, mut slot) in slots.into_iter().enumerate() {
        let Some(Some((name, key, desc, shown))) = rows.get(i) else {
            slot.button.set_visible(false);
            continue;
        };
        slot.name.set_text(name);
        slot.key.set_text(key);
        slot.desc.set_text(desc);
        slot.button.set_visible(*shown);
    }
    for mut h in heads {
        h.node.button.set_visible(h.shown);
        h.node.title.set_text(&h.title);
        h.node.count.set_text(&h.count.to_string());
        h.node.mark.set_texture(&marks[usize::from(!h.folded)]);
    }
    // the highlight moved: the row it left plain again, the new one lit
    let button = |l: PaletteLine| {
        let p = palette.borrow();
        match l {
            PaletteLine::Head(g) => p.groups.get(g).map(|g| g.head.button.clone()),
            PaletteLine::Slot(s) => p.slots.get(s).map(|s| s.button.clone()),
        }
    };
    if styled != selected {
        if let Some(mut b) = styled.and_then(button) {
            look.style_row(&mut b, false, false);
        }
        if let Some(mut b) = selected.and_then(button) {
            look.style_row(&mut b, false, true);
        }
    }
    let empty = rows.iter().all(|r| r.as_ref().is_none_or(|r| !r.3)) && palette.borrow().filtered;
    none.set_visible(empty);
    let mut p = palette.borrow_mut();
    if restyle {
        if let Some(t) = p.title.as_mut() {
            t.set_text(&if pad {
                tr!("game-menu-commands")
            } else {
                tr!("palette-title")
            });
        }
        // under a pad the description is the row: no name, no key
        for slot in p.slots.iter_mut() {
            slot.name.set_visible(!pad);
            slot.key.set_visible(!pad);
        }
        p.drawn_pad = Some(pad);
    }
    p.styled = selected;
    match p.selected {
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
        /// The answers the question names by a word ("Right"), as shown.
        named: Vec<(char, String)>,
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
        /// The engine's window laid out from its English ("vanquished",
        /// "genocided", "overview", "sheet"), or None for its lines as
        /// written.
        layout: Option<&'static str>,
    },
    MessageMenu {
        letter: char,
        pick: bool,
    },
    /// The command menu as a wheel (`#`'s request, the list's other face).
    Wheel(Box<CommandWheel>),
    /// What can be done at a cell (the engine's #therecmdmenu), a popup
    /// by the cell.
    CellMenu(Box<CellMenu>),
    /// A container's things beside the pack (the looting's list).
    Loot(Box<LootPanel>),
}

/// The open container's panel as self-tests read it: each column's rows,
/// those marked, the keys' row.
pub type LootState = ([usize; 2], [Vec<usize>; 2], (usize, Option<usize>));

/// The open command wheel as self-tests read it: its ring, its wedges'
/// words, the one pointed at, its middle on screen, whether it is held.
pub type WheelShown = (
    command_wheel::Ring,
    Vec<String>,
    Option<usize>,
    Vector2,
    bool,
);

/// How the command wheel opens for the next `#`: where its middle goes
/// (None: the screen's), whether a key or button holds it, and whether
/// that was let go before it showed (a tap: it stays open).
#[derive(Debug, Clone, Copy)]
struct WheelNext {
    at: Option<Vector2>,
    held: bool,
    let_go: bool,
    since: std::time::Instant,
}

struct Open {
    req: u64,
    prompt: Prompt,
    kind: Kind,
    /// The command key of the RPG panel it is (`^X`'s sheet): pressed
    /// again, it closes the panel.
    own: Option<KeyInput>,
    /// It is one of the RPG panels (`panel_frame`'s).
    rpg: bool,
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
    /// field with the on-screen keyboard; the keyboard's words hidden (the
    /// pad's hints show instead).
    pad: bool,
    /// The hero carries goods not paid for: the palette lists #pay first.
    owes: bool,
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
    /// The keys the player moved: the palette shows its commands' keys as
    /// they are pressed now.
    keymap: nh_world::Keymap,
    /// The HUD's Commands button pressed, and when: the palette opening
    /// next hangs from it.
    palette_anchor: Option<(Rect2, std::time::Instant)>,
    /// `#` opens the wheel rather than the list (the settings' command
    /// menu), and the wheel the next `#` opens however it is set (held
    /// from the command prompt, a pad's "Commands").
    wheel_style: bool,
    wheel_next: Option<WheelNext>,
    /// The list opening next has only this section open (a "More…").
    palette_section: Option<Section>,
    /// The menu being opened, its lines as the game wrote them (its
    /// objects' icons go by their English names).
    opening_english: Vec<String>,
    /// The spells the hero knows, as the last `+` or `Z` listed them (the
    /// wheel's spells' ring).
    known_spells: Vec<spells::Spell>,
    /// The cell clicked (where on screen, what is known there) and when:
    /// the engine's menu of what can be done there opens by it.
    cell_anchor: Option<(Vector2, Vec<String>, std::time::Instant)>,
    /// The looting's list opening next shows as the container's panel:
    /// what it shows, the pack beside it.
    loot_next: Option<(nh_world::LootView, Vec<nh_protocol::InvItem>)>,
    /// The pack's marks, once the panel answered its list (the looting
    /// puts them in next).
    loot_put: Option<Vec<(char, Option<u32>)>>,
    /// The hero rendered in their gear (the inventory's doll): the
    /// character sheet's portrait.
    portrait: Option<Gd<Texture2D>>,
    /// The open character sheet's view (its portrait shows the render).
    sheet: Option<SheetView>,
    /// The own key of the RPG panel being opened (`Open::own`), and that
    /// it is one.
    opening_own: Option<KeyInput>,
    opening_rpg: bool,
    /// The entry a gamepad's focus starts on in the menu being opened
    /// (else its first pick).
    opening_cursor: Option<usize>,
    /// The open spells' panel: its spells, whether a pick reorders them
    /// (else it casts), the reorder's button, the full card.
    spells: Option<SpellsOpen>,
    /// The hero's Pw now (the spells' cards grey what it cannot pay), and
    /// the action bar's slots on screen (a spell's card dropped on one is
    /// bound to it).
    energy: Option<i64>,
    bar_slots: Rc<RefCell<Vec<Rect2>>>,
    /// What the game is to do once the open panel's window is answered
    /// (`take_follow`): cast the spell `+`'s panel picked, open the
    /// codex's other tab.
    follow: Option<Follow>,
    /// The open journal's view (the game puts the messages in).
    journal: Option<JournalView>,
    /// The codex's tab open.
    codex: Option<CodexTab>,
    /// The discoveries' cards with an icon, of all (self-tests).
    codex_icons: (usize, usize),
    /// The screen's foot the HUD's orbs and bar take: the RPG panels keep
    /// above it.
    bottom_room: f32,
}

/// What follows a panel's answer, at the command prompt it goes back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Follow {
    /// `Z`, then the spell by its name.
    Cast(String),
    /// A command's key (`\`).
    Key(i32),
    /// An extended command (#conduct).
    Ext(&'static str),
}

/// The codex's tabs: `\`'s discoveries, #conduct's challenges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexTab {
    Discoveries,
    Conduct,
}

struct SpellsOpen {
    book: Spellbook,
    reorder: bool,
    button: Option<Gd<Button>>,
    /// The full card of the spell under the pointer or the focus (None
    /// where the screen has no room for it).
    detail: Option<Rc<RefCell<spells_view::Detail>>>,
}

/// A row of a panel's list (`Dialogs::panel_rows`).
enum PanelRow {
    /// A menu entry's cells after its letter, and the label the menu
    /// colours (an entry no pick shows them as a line).
    Pick(Vec<Gd<Control>>, Gd<Label>),
    /// A heading, a note, a gap.
    Fixed(Gd<Control>),
}

/// A panel's list as `Dialogs::panel_rows` made it.
struct PanelRows {
    list: Scroller,
    rows: Vec<MenuRow>,
    tops: Vec<f32>,
    heights: Vec<f32>,
    max_top: f32,
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

    /// Where a pad's hints go while a dialog is open: in its panel's
    /// bottom, or under the command wheel's ring.
    pub fn hints_rect(&self) -> Option<Rect2> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Wheel(w)) => {
                let r = command_wheel::RING / 2.0 * w.scale();
                let c = w.centre();
                Some(Rect2::new(
                    Vector2::new(c.x - r - 120.0, c.y + r - 6.0),
                    Vector2::new(2.0 * r + 240.0, 60.0),
                ))
            }
            // under the popup, as wide as the hints need
            Some(Kind::CellMenu(_)) => self.panel_rect().map(|r| {
                Rect2::new(
                    Vector2::new(r.position.x - 260.0, r.end().y - 6.0),
                    Vector2::new(r.size.x + 520.0, 60.0),
                )
            }),
            _ => self.panel_rect(),
        }
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
            owes: false,
            warm: Vec::new(),
            warm_next: 0,
            warming: false,
            palette_pool: None,
            keymap: nh_world::Keymap::default(),
            palette_anchor: None,
            wheel_style: false,
            wheel_next: None,
            palette_section: None,
            opening_english: Vec::new(),
            known_spells: Vec::new(),
            cell_anchor: None,
            loot_next: None,
            loot_put: None,
            portrait: None,
            sheet: None,
            opening_own: None,
            opening_rpg: false,
            opening_cursor: None,
            spells: None,
            energy: None,
            bar_slots: Rc::new(RefCell::new(Vec::new())),
            follow: None,
            journal: None,
            codex: None,
            codex_icons: (0, 0),
            bottom_room: 0.0,
        }
    }

    /// The visible area in pixels. Headless Godot reports a square viewport
    /// as tall as it is wide; dialogs there are laid out for the window the
    /// project opens, so self-tests see the same scrolling as a player.
    fn screen(&self) -> Vector2 {
        let s = self.root.get_viewport_rect().size;
        let headless = DisplayServer::singleton().get_name() == "headless";
        // a run at a set size (`--size`) has that canvas, headless or not
        let square = (s.x - s.y).abs() < 1.0;
        if (headless && square) || s.x < 320.0 || s.y < 240.0 {
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

    /// The keys' hint under a dialog: the keyboard's words, hidden while a
    /// gamepad gives the input (its own hints show).
    fn hint(&self, col: &mut Gd<VBoxContainer>, text: &str, width: f32) -> Gd<Label> {
        let text = i18n::whole_parts(text);
        let mut hint = theme::styled_label(&text, Face::Body, 15, theme::TEXT_DIM);
        hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        hint.set_custom_minimum_size(Vector2::new(width, 0.0));
        pad_view::key_words(&hint, self.pad);
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
        let heights: Vec<f32> = state
            .entries
            .iter()
            .map(|e| {
                if row_kind(e) == RowKind::Spacer {
                    SPACER_H
                } else {
                    ROW_H
                }
            })
            .collect();
        let mut tops = Vec::with_capacity(state.entries.len());
        let mut y = 0.0;
        for h in &heights {
            tops.push(y);
            y += h;
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
                        let english = self.opening_english.get(i).map(String::as_str);
                        t.set_texture(&crate::icons::named_icon(g.tile, class, english));
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
            heights,
            max_top,
            colors,
            list,
            cursor: None,
            armed: false,
            status,
            hint,
            own_hint: None,
        };
        // a preselected item: show it
        if let Some(i) = view.state.entries.iter().position(|e| e.selected) {
            view.cursor = Some(i);
            view.reveal(i);
        }
        // a gamepad starts on the first entry (or the one asked for),
        // ready for A
        let asked = self
            .opening_cursor
            .take()
            .filter(|&i| view.state.entries.get(i).is_some_and(|e| e.selectable));
        if self.pad
            && view.cursor.is_none()
            && let Some(i) = asked.or_else(|| view.state.entries.iter().position(|e| e.selectable))
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
        let cmds = palette_cmds(catalog, &self.keymap);
        self.open_palette_cmds(req, cmds)
    }

    /// The list of `cmds` (the wheel's own when it gives way to it).
    fn open_palette_cmds(
        &mut self,
        req: u64,
        cmds: Vec<PaletteCmd>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        // a wheel's "More…": that section alone open, its heading on
        let section = self.palette_section.take();
        // hung from the HUD's Commands button just pressed (not one long
        // gone: a `#` it never opened)
        let anchor = (self.palette_anchor.take())
            .filter(|(_, at)| at.elapsed().as_secs_f32() < 2.0)
            .map(|(r, _)| r);
        // the palette put away serves again only if it reads the same: the
        // engine's translator may have loaded since it was made, or the
        // language be another
        let pool = self.palette_pool.take().and_then(|mut pool| {
            if pool.palette.borrow().cmds == cmds {
                return Some(pool);
            }
            pool.panel.queue_free();
            pool.shade.queue_free();
            None
        });
        if let Some(pool) = pool {
            let PalettePool {
                mut shade,
                mut panel,
                mut edit,
                palette,
                req: pooled,
            } = pool;
            pooled.set(req);
            edit.set_text("");
            edit.release_focus();
            pad_view::show_key_words(&panel, self.pad);
            {
                let mut p = palette.borrow_mut();
                p.pad = self.pad;
                p.owes = self.owes;
                p.only(section);
                p.refilter("");
                p.on_section(section);
            }
            fill_palette(&palette);
            self.place_palette(&mut panel, &mut shade, &palette, anchor);
            shade.set_visible(true);
            panel.set_visible(true);
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
            .map(|c| look.text_w(&format!("#{}", c.name), &look.body, NAME_SIZE))
            .fold(80.0f32, f32::max)
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
        let laid = command_menu::sections(&cmds, false, false);
        let lines = (cmds.len() + laid.len()) as f32;
        let view_h = (lines * ROW_H).clamp(ROW_H, max_h.min(PALETTE_ROWS * ROW_H));
        let list = Scroller::new(width, view_h, false);

        let (mut shade, mut panel, mut col) = self.frame(width, Some(&tr!("palette-title")));
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
        // a pad types no command: it picks a row
        pad_view::key_words(&edit, self.pad);
        col.add_child(&edit);
        let mut rows_box = VBoxContainer::new_alloc();
        rows_box.add_theme_constant_override("separation", 0);
        rows_box.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let marks = [fold_mark(false), fold_mark(true)];
        let mut slots = Vec::with_capacity(cmds.len());
        let mut groups = Vec::with_capacity(laid.len());
        for (section, idx) in &laid {
            let head = self.palette_head(&marks[1]);
            rows_box.add_child(&head.button);
            let start = slots.len();
            for _ in idx {
                let i = slots.len();
                let mut desc = cell("", theme::TEXT, Some(&look.body), None);
                desc.add_theme_font_size_override("font_size", BODY_SIZE);
                let mut name = cell("", theme::TEXT_DIM, Some(&look.body), Some(name_w));
                name.add_theme_font_size_override("font_size", NAME_SIZE);
                let mut key = cell("", theme::ACCENT, None, Some(key_w));
                key.add_theme_font_size_override("font_size", 14);
                key.set_horizontal_alignment(HorizontalAlignment::RIGHT);
                // the command and its key as the player types them
                i18n::verbatim(&name);
                i18n::verbatim(&key);
                // the palette is reused: its rows answer the request it is
                // open for now
                let button = self.row_button_at(
                    req_cell.clone(),
                    i,
                    &[desc.clone(), name.clone(), key.clone()],
                );
                rows_box.add_child(&button);
                slots.push(PaletteSlot {
                    button,
                    name,
                    key,
                    desc,
                });
            }
            groups.push(PaletteGroup {
                section: *section,
                head,
                slots: start..slots.len(),
                cmds: Vec::new(),
                folded: section.folded_at_first(),
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
        // a pad's B cancels (its strip says so); hung from the button, a
        // click beside it does (its holder hidden)
        pad_view::key_words(&row, self.pad);
        let mut cancel_row = VBoxContainer::new_alloc();
        cancel_row.add_child(&row);
        col.add_child(&cancel_row);
        // hung from the Commands button, a click beside it puts it away
        // (the shade takes the mouse only then)
        let (q, c) = (self.queue.clone(), req_cell.clone());
        shade
            .signals()
            .gui_input()
            .connect(move |e: Gd<InputEvent>| {
                if let Ok(b) = e.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                    && b.get_button_index() == MouseButton::LEFT
                {
                    let ev = DialogEvent::ExtCmd(None);
                    push(&q, UiEvent::Dialog { req: c.get(), ev });
                }
            });
        let title = col.get_child(0).and_then(|c| c.try_cast::<Label>().ok());
        let n = slots.len();
        let palette = Rc::new(RefCell::new(Palette {
            cmds,
            groups,
            lines: Vec::new(),
            slot_cmd: vec![None; n],
            filtered: false,
            selected: None,
            slots,
            styled: None,
            list,
            none,
            look,
            pad: self.pad,
            owes: self.owes,
            drawn_pad: None,
            title,
            marks,
            width,
            full_h: view_h,
            folds_kept: None,
            dropdown: false,
            cancel_row,
        }));
        // a heading clicked folds its section or unfolds it, and is the
        // row highlighted: local to the palette, it never touches the game
        let heads: Vec<Gd<Button>> = (palette.borrow().groups.iter())
            .map(|g| g.head.button.clone())
            .collect();
        for (g, b) in heads.into_iter().enumerate() {
            let p = palette.clone();
            b.signals().pressed().connect(move || {
                {
                    let mut p = p.borrow_mut();
                    p.fold(g, None);
                    p.selected = p.lines.iter().position(|&l| l == PaletteLine::Head(g));
                }
                fill_palette(&p);
            });
        }
        {
            let mut p = palette.borrow_mut();
            p.only(section);
            p.refilter("");
            p.on_section(section);
        }
        fill_palette(&palette);
        // refiltering is local to the palette: it never touches the game
        let p = palette.clone();
        edit.signals().text_changed().connect(move |text: GString| {
            p.borrow_mut().refilter(&text.to_string());
            fill_palette(&p);
        });
        self.place_palette(&mut panel, &mut shade, &palette, anchor);
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

    /// The wheel `#`'s request opens, if one: the one asked for (held from
    /// the command prompt, a pad's "Commands"), else the settings' choice;
    /// never for the warm-up's (it builds the list).
    fn wheel_for(&mut self, req: u64) -> Option<WheelNext> {
        if self.warming || req == 0 {
            return None;
        }
        let asked = (self.wheel_next.take()).filter(|n| n.since.elapsed().as_secs_f32() < 2.0);
        if asked.is_some() {
            return asked;
        }
        if !self.wheel_style {
            return None;
        }
        // the Commands button's: the wheel by it
        let at = (self.palette_anchor.take())
            .filter(|(_, at)| at.elapsed().as_secs_f32() < 2.0)
            .map(|(r, _)| r.center());
        Some(WheelNext {
            at,
            held: false,
            let_go: false,
            since: std::time::Instant::now(),
        })
    }

    fn open_wheel(
        &mut self,
        req: u64,
        catalog: Option<&Catalog>,
        next: WheelNext,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let cmds = palette_cmds(catalog, &self.keymap);
        // above the HUD's foot (its orbs, bar, buttons and a pad's hints),
        // as large as the room there lets it be
        let screen = self.root.get_size();
        let ring = command_wheel::RING;
        let foot = (self.bottom_room + WHEEL_FOOT).min(screen.y * 0.5);
        let room = Vector2::new(screen.x, screen.y - foot) - Vector2::new(2.0, 2.0) * PANEL_EDGE;
        let k = (room.x.min(room.y) / ring).clamp(0.4, 1.0);
        let half = ring / 2.0 * k + PANEL_EDGE;
        let at = next
            .at
            .unwrap_or(Vector2::new(screen.x / 2.0, (screen.y - foot) / 2.0));
        let clamp = |v: f32, low: f32, high: f32| {
            if high < low {
                (low + high) / 2.0
            } else {
                v.clamp(low, high)
            }
        };
        let centre = Vector2::new(
            clamp(at.x, half, screen.x - half),
            clamp(at.y, half, screen.y - foot - half + PANEL_EDGE),
        );
        // the map dimmed a little; the mouse is the wheel's everywhere
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.25));
        theme::full_rect_ignore(&shade);
        shade.set_mouse_filter(MouseFilter::STOP);
        let q = self.queue.clone();
        shade
            .signals()
            .gui_input()
            .connect(move |e: Gd<InputEvent>| {
                let ev = match e.try_cast::<InputEventMouseButton>() {
                    Ok(b) => {
                        let button = match b.get_button_index() {
                            MouseButton::LEFT => 1,
                            MouseButton::RIGHT => 2,
                            MouseButton::MIDDLE => 3,
                            _ => return,
                        };
                        // on the canvas (a self-test's mouse too)
                        DialogEvent::WheelButton {
                            button,
                            pressed: b.is_pressed(),
                            at: b.get_global_position(),
                        }
                    }
                    Err(e) => match e.try_cast::<godot::classes::InputEventMouseMotion>() {
                        Ok(m) => DialogEvent::WheelMove(m.get_global_position()),
                        Err(_) => return,
                    },
                };
                push(&q, UiEvent::Dialog { req, ev });
            });
        self.root.add_child(&shade);
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::IGNORE);
        panel.add_theme_stylebox_override("panel", &StyleBoxEmpty::new_gd());
        let r = ring / 2.0;
        place(
            &panel,
            [0.0; 4],
            [centre.x - r, centre.y - r, centre.x + r, centre.y + r],
        );
        panel.set_pivot_offset(Vector2::new(r, r));
        panel.set_scale(Vector2::new(k, k));
        let root = Control::new_alloc();
        panel.add_child(&root);
        self.root.add_child(&panel);
        let held = next.held && !next.let_go;
        // nothing pointed at: the mouse is at its middle, or it moves next
        let spells = self.known_spells.clone();
        let offer = command_wheel::WheelOffer {
            cmds,
            spells,
            owes: self.owes,
            pad: self.pad,
            held,
        };
        let wheel = CommandWheel::new(root, centre, k, offer);
        (Kind::Wheel(Box::new(wheel)), shade, panel)
    }

    /// What the wheel's pick asks: a command run, a cancel, nothing yet,
    /// or the list in its place (for the same request).
    fn wheel_act(&mut self, act: WheelAct) -> Option<Reply> {
        match act {
            WheelAct::Stay => None,
            WheelAct::Run(name) => Some(Reply::ExtCmd(Some(name))),
            WheelAct::Cancel => Some(Reply::ExtCmd(None)),
            WheelAct::List(section) => {
                self.wheel_to_list(section);
                None
            }
            // `#` answered with nothing; the spell cast at the prompt next
            WheelAct::Cast(name) => {
                self.follow = Some(Follow::Cast(name));
                Some(Reply::ExtCmd(None))
            }
        }
    }

    /// The wheel gives way to the list, for the same request: every
    /// section, or `section` alone open.
    fn wheel_to_list(&mut self, section: Option<Section>) {
        let Some(open) = self.open.take_if(|o| matches!(o.kind, Kind::Wheel(_))) else {
            return;
        };
        let Kind::Wheel(wheel) = &open.kind else {
            return;
        };
        let cmds = wheel.cmds().to_vec();
        let (req, prompt) = (open.req, open.prompt.clone());
        self.retire(open);
        self.palette_section = section;
        let (kind, shade, panel) = self.open_palette_cmds(req, cmds);
        let pad_room = add_pad_room(&panel);
        pad_view::show_key_words(&panel, self.pad);
        self.open = Some(Open {
            req,
            prompt,
            kind,
            own: None,
            rpg: false,
            shade,
            panel,
            pad_room,
        });
    }

    /// The next `#` opens the wheel: its middle at `at` on the canvas (the
    /// mouse), else in the screen's middle; `held` while the key or
    /// button that asked for it is down.
    pub fn set_wheel_next(&mut self, at: Option<Vector2>, held: bool) {
        self.wheel_next = Some(WheelNext {
            at,
            held,
            let_go: false,
            since: std::time::Instant::now(),
        });
    }

    /// A new game: no spell known yet (the wheel's ring fills at the
    /// first `+` or `Z`).
    pub fn forget_spells(&mut self) {
        self.known_spells.clear();
    }

    /// `#` opens the wheel rather than the list (the settings' choice).
    pub fn set_wheel_style(&mut self, wheel: bool) {
        self.wheel_style = wheel;
    }

    /// The key or button holding the wheel let go: its pick, or (the
    /// wheel not up yet) it opens to stay.
    pub fn wheel_let_go(&mut self) -> Option<Reply> {
        if let Some(next) = self.wheel_next.as_mut() {
            next.let_go = true;
            return None;
        }
        let act = match self.open.as_mut().map(|o| &mut o.kind) {
            Some(Kind::Wheel(w)) => w.let_go(),
            _ => return None,
        };
        self.wheel_act(act)
    }

    /// A stick pushed while the wheel is up.
    pub fn wheel_aim(&mut self, x: f32, y: f32) {
        if let Some(Kind::Wheel(w)) = self.open.as_mut().map(|o| &mut o.kind) {
            w.aim(x, y);
        }
    }

    /// The open wheel: its ring, its wedges' words, the one pointed at, its
    /// middle on screen, whether it is held (self-tests).
    pub fn wheel_shown(&self) -> Option<WheelShown> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Wheel(w)) => {
                let (ring, words, hover) = w.shown();
                Some((ring, words, hover, w.centre(), w.held()))
            }
            _ => None,
        }
    }

    /// Where the open wheel's wedge `i` is on screen (self-tests).
    pub fn wheel_wedge_rect(&self, i: usize) -> Option<Rect2> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Wheel(w)) => w.wedge_rect(i),
            _ => None,
        }
    }

    /// The open wheel's wedges (self-tests).
    pub fn wheel_items(&self) -> Option<Vec<command_wheel::WheelItem>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Wheel(w)) => Some(w.items().to_vec()),
            _ => None,
        }
    }

    /// The container's panel for the looting's list: the pack beside the
    /// container's things (`view`); `items` the list's entries.
    fn open_loot(
        &mut self,
        req: u64,
        view: &nh_world::LootView,
        pack: &[nh_protocol::InvItem],
        items: &[nh_protocol::MenuItem],
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let container = palette_row_words(&i18n::engine(EngineKind::Name, &view.container));
        let width = 2.0 * crate::loot_view::COLUMN_W + 24.0;
        let (shade, panel, mut col) = self.frame(width, Some(&container));
        let mut body = HBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", 24);
        let heads = [tr!("loot-pack"), tr!("loot-inside")];
        let put_items: &[nh_protocol::MenuItem] = if view.putting { items } else { &[] };
        let source = crate::loot_view::LootSource {
            pack,
            take: &view.take,
            put_items,
            putting: view.putting,
        };
        let loot = LootPanel::build(&mut body, heads, &source, &self.queue, req);
        col.add_child(&body);
        self.hint(&mut col, &tr!("loot-hint"), width);
        let buttons = [
            (tr!("loot-done"), dialog_ui(req, DialogEvent::MenuConfirm)),
            (tr!("dlg-cancel"), dialog_ui(req, DialogEvent::MenuCancel)),
        ];
        let made = self.buttons(&mut col, &buttons);
        for b in &made {
            // a pad's Start and B say them (its strip)
            pad_view::key_words(b, self.pad);
        }
        (Kind::Loot(Box::new(loot)), shade, panel)
    }

    /// The container's panel done (`done`) or given up: its answer to the
    /// list; the pack's marks kept for the put-in list to come.
    fn loot_end(&mut self, done: bool) -> Option<Reply> {
        let Some(Kind::Loot(l)) = self.open.as_ref().map(|o| &o.kind) else {
            return None;
        };
        let (reply, put) = match (done, l.putting()) {
            (true, false) => (l.reply(), l.pack_marks()),
            (true, true) => (l.reply(), Vec::new()),
            (false, _) => (Reply::Cancel, Vec::new()),
        };
        self.loot_put = Some(put);
        Some(reply)
    }

    /// The looting's next list shows as the container's panel.
    pub fn set_loot(&mut self, view: nh_world::LootView, pack: Vec<nh_protocol::InvItem>) {
        self.loot_next = Some((view, pack));
    }

    /// The pack's marks the container's panel left, once (the looting
    /// puts them in).
    pub fn take_loot_put(&mut self) -> Option<Vec<(char, Option<u32>)>> {
        self.loot_put.take()
    }

    /// The open container's panel: each column's rows, those marked, the
    /// keys' row (self-tests).
    pub fn loot_state(&self) -> Option<LootState> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Loot(l)) => Some((l.lens(), l.marked(), l.focus())),
            _ => None,
        }
    }

    /// The open container's panel's pack column, its letters top down
    /// (self-tests).
    pub fn loot_pack_letters(&self) -> Option<Vec<char>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Loot(l)) => Some(l.pack_letters()),
            _ => None,
        }
    }

    /// Where the open container's panel shows row `i` of column `col`
    /// (self-tests).
    pub fn loot_row_rect(&self, col: usize, i: usize) -> Option<Rect2> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Loot(l)) => l.row_rect(col, i),
            _ => None,
        }
    }

    /// The engine's menu of what can be done at a cell, a popup by it: `at`
    /// the cell on screen, `look` what is known there.
    fn open_cell_menu(
        &mut self,
        req: u64,
        items: &[nh_protocol::MenuItem],
        at: Vector2,
        look: &[String],
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        // the map left as it is; a click beside the popup cancels
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        theme::full_rect_ignore(&shade);
        shade.set_mouse_filter(MouseFilter::STOP);
        let q = self.queue.clone();
        shade
            .signals()
            .gui_input()
            .connect(move |e: Gd<InputEvent>| {
                if let Ok(b) = e.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                    && matches!(b.get_button_index(), MouseButton::LEFT | MouseButton::RIGHT)
                {
                    push(
                        &q,
                        UiEvent::Dialog {
                            req,
                            ev: DialogEvent::MenuCancel,
                        },
                    );
                }
            });
        self.root.add_child(&shade);
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::STOP);
        theme::apply_frame(&panel, Frame::Panel);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 4);
        panel.add_child(&col);
        let q = self.queue.clone();
        let (mut menu, _) = CellMenu::build(&mut col, look, items, self.pad, |i| {
            let q = q.clone();
            Box::new(move || {
                push(
                    &q,
                    UiEvent::Dialog {
                        req,
                        ev: DialogEvent::MenuClick(i),
                    },
                );
            })
        });
        self.root.add_child(&panel);
        // beside the cell, right of it where there is room, else left; above
        // the HUD's foot
        let size = panel.get_combined_minimum_size();
        let screen = self.root.get_size();
        let mut pos = at + Vector2::new(24.0, -size.y * 0.3);
        if pos.x + size.x > screen.x - PANEL_EDGE {
            pos.x = at.x - 24.0 - size.x;
        }
        let floor = (screen.y - self.bottom_room - PANEL_EDGE).max(size.y + PANEL_EDGE);
        pos.x = pos
            .x
            .clamp(PANEL_EDGE, (screen.x - size.x - PANEL_EDGE).max(PANEL_EDGE));
        pos.y = pos.y.clamp(PANEL_EDGE, (floor - size.y).max(PANEL_EDGE));
        place(
            &panel,
            [0.0; 4],
            [pos.x, pos.y, pos.x + size.x, pos.y + size.y],
        );
        // a pad's focus on looking, the one that does nothing (never on the
        // stairs: two stray A's would leave the level)
        if self.pad {
            let look_here = items
                .iter()
                .position(|i| i.str.as_deref() == Some(cell_menu::LOOK_HERE));
            menu.focus_item(look_here);
        }
        (Kind::CellMenu(Box::new(menu)), shade, panel)
    }

    /// The menu of what can be done at a cell opening next goes by it:
    /// `at` on the canvas, `look` what is known there, a line each.
    pub fn set_cell_anchor(&mut self, at: Vector2, look: Vec<String>) {
        self.cell_anchor = Some((at, look, std::time::Instant::now()));
    }

    /// The open cell's menu: each row's place among the engine's items
    /// and its rect on screen (self-tests).
    pub fn cell_menu_rows(&self) -> Option<Vec<(usize, Rect2)>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::CellMenu(m)) => Some(m.rows()),
            _ => None,
        }
    }

    /// A section's heading row: its fold mark, its title, how many it
    /// holds (`fill_palette` writes them).
    fn palette_head(&self, mark: &Gd<Texture2D>) -> HeadSlot {
        let mut b = Button::new_alloc();
        b.set_focus_mode(FocusMode::NONE);
        b.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
        b.add_theme_stylebox_override("focus", &self.look.no_focus);
        self.look.style_row(&mut b, false, false);
        let mut hbox = HBoxContainer::new_alloc();
        hbox.set_mouse_filter(MouseFilter::IGNORE);
        hbox.add_theme_constant_override("separation", 8);
        let mut m = TextureRect::new_alloc();
        m.set_texture(mark);
        m.set_expand_mode(ExpandMode::IGNORE_SIZE);
        m.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        m.set_custom_minimum_size(Vector2::new(12.0, 12.0));
        m.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        m.set_self_modulate(theme::GOLD);
        m.set_mouse_filter(MouseFilter::IGNORE);
        hbox.add_child(&m);
        let title = theme::styled_label("", Face::Caps, 15, theme::GOLD_BRIGHT);
        let mut title = title;
        title.set_vertical_alignment(VerticalAlignment::CENTER);
        hbox.add_child(&title);
        let mut count = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        count.set_vertical_alignment(VerticalAlignment::CENTER);
        count.set_h_size_flags(SizeFlags::EXPAND_FILL);
        hbox.add_child(&count);
        place(&hbox, [0.0, 0.0, 1.0, 1.0], [8.0, 0.0, -8.0, 0.0]);
        b.add_child(&hbox);
        HeadSlot {
            button: b,
            mark: m,
            title,
            count,
        }
    }

    /// The palette where it opens: hung from the HUD's Commands button
    /// (`anchor`), above it and its right edge at the button's, where it
    /// has the room, no Cancel, nothing dimmed, a click beside it closing
    /// it; else centred over the dimmed map.
    fn place_palette(
        &self,
        panel: &mut Gd<PanelContainer>,
        shade: &mut Gd<ColorRect>,
        palette: &Rc<RefCell<Palette>>,
        anchor: Option<Rect2>,
    ) {
        let mut p = palette.borrow_mut();
        let width = p.width;
        // as many rows as the room above the button holds (a few at least)
        p.cancel_row.set_visible(anchor.is_none());
        let full = p.full_h;
        p.list
            .scroll
            .set_custom_minimum_size(Vector2::new(width, full));
        let chrome = panel.get_combined_minimum_size().y - full;
        let rows = anchor.map(|a| {
            let room = a.position.y - PALETTE_GAP - PANEL_EDGE - chrome;
            (room / ROW_H).floor().min(full / ROW_H)
        });
        let anchor = anchor.filter(|_| rows.is_some_and(|r| r >= PALETTE_LEAST_ROWS));
        p.dropdown = anchor.is_some();
        p.cancel_row.set_visible(anchor.is_none());
        let view_h = match (anchor, rows) {
            (Some(_), Some(r)) => r * ROW_H,
            _ => full,
        };
        p.list.view_h = view_h;
        p.list
            .scroll
            .set_custom_minimum_size(Vector2::new(width, view_h));
        match anchor {
            Some(a) => {
                let screen = self.root.get_size();
                let right = a.end().x.min(screen.x - PANEL_EDGE);
                let bottom = a.position.y - PALETTE_GAP;
                // its frame's margins make it wider than its column: it
                // grows leftwards, its right edge at the button's
                let w = panel.get_combined_minimum_size().x.max(width);
                let left = (right - w).max(PANEL_EDGE);
                place(panel, [0.0; 4], [left, bottom - 40.0, left + w, bottom]);
                panel.set_v_grow_direction(GrowDirection::BEGIN);
                panel.set_h_grow_direction(GrowDirection::BEGIN);
                shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
                shade.set_mouse_filter(MouseFilter::STOP);
            }
            None => {
                // as tall as its rows again: a panel grows to what it holds
                // and never shrinks by itself
                place(
                    panel,
                    [0.5, 0.5, 0.5, 0.5],
                    [-width / 2.0, -40.0, width / 2.0, 40.0],
                );
                panel.set_v_grow_direction(GrowDirection::BOTH);
                panel.set_h_grow_direction(GrowDirection::BOTH);
                shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.45));
                shade.set_mouse_filter(MouseFilter::IGNORE);
            }
        }
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
            let q = self.queue.clone();
            let mut osk = Osk::new(&mut col, move |r, c| {
                push(&q, dialog_ui(req, DialogEvent::OskKey(r, c)));
            });
            osk.show();
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
        let prose = is_prose(lines);
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
        (Kind::Show { text, layout: None }, shade, panel)
    }

    /// #vanquished: a kind of creature a row (its symbol in its colour, its
    /// name, how many died), the headings of the list by class, the total.
    fn open_vanquished(
        &mut self,
        req: u64,
        v: &Vanquished,
        catalog: Option<&Catalog>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let rows = vanquished_rows(v, catalog);
        let totals: Vec<String> = v
            .total
            .iter()
            .map(|&n| tr!("vanquished-total", count = n))
            .collect();
        let title = tr!("vanquished-title");
        self.open_creatures(req, &title, &rows, &totals, "vanquished")
    }

    /// #genocided: a species a row, the extinct ones said so, the totals.
    fn open_genocided(
        &mut self,
        req: u64,
        g: &Genocided,
        plurals: &HashMap<String, String>,
        catalog: Option<&Catalog>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let rows = genocided_rows(g, plurals, catalog);
        let mut totals = Vec::new();
        if let Some(n) = g.genocided {
            totals.push(tr!("genocided-total", count = n));
        }
        if let Some(n) = g.extinct {
            totals.push(tr!("extinct-total", count = n));
        }
        let title = layouts::gone_heading(g.heading);
        self.open_creatures(req, &title, &rows, &totals, "genocided")
    }

    /// A list of creatures: a row each, under the headings of their
    /// classes, the totals under a rule.
    fn open_creatures(
        &mut self,
        req: u64,
        title: &str,
        rows: &[CreatureRow],
        totals: &[String],
        layout: &'static str,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let look = self.look.clone();
        let right_w = rows
            .iter()
            .filter_map(|r| match r {
                CreatureRow::Kind { right, .. } => {
                    Some(look.text_w(right, &look.body_bold, BODY_SIZE))
                }
                CreatureRow::Class(_) => None,
            })
            .fold(0.0f32, f32::max)
            + 12.0;
        let left_w = rows
            .iter()
            .map(|r| match r {
                CreatureRow::Class(c) => look.text_w(c, &look.title, BODY_SIZE + 3),
                CreatureRow::Kind { name, .. } => {
                    TOKEN_W + 12.0 + look.text_w(name, &look.body_bold, BODY_SIZE)
                }
            })
            .fold(0.0f32, f32::max);
        let totals_w = totals
            .iter()
            .map(|t| look.text_w(t, &look.body_bold, BODY_SIZE))
            .fold(0.0f32, f32::max);
        let title_w = look.text_w(title, &look.body_bold, 20);
        // the scroll bar and a little air
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(420.0);
        let width = (left_w + 12.0 + right_w + 30.0)
            .max(totals_w + 30.0)
            .max(title_w + 40.0)
            .clamp(420.0, max_w.min(760.0));
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 0);
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut content_h = 0.0;
        for r in rows {
            match r {
                CreatureRow::Class(c) => {
                    let mut l = cell(c, theme::ACCENT, Some(&look.title), None);
                    l.add_theme_font_size_override("font_size", BODY_SIZE + 3);
                    l.set_custom_minimum_size(Vector2::new(0.0, ROW_H + 4.0));
                    list.add_child(&l);
                    content_h += ROW_H + 4.0;
                }
                CreatureRow::Kind {
                    symbol,
                    name,
                    unique,
                    right,
                } => {
                    let mut row = HBoxContainer::new_alloc();
                    row.add_theme_constant_override("separation", 12);
                    row.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
                    row.add_child(&self.token(*symbol));
                    // a unique creature by its own name, in gold
                    let (color, font) = if *unique {
                        (theme::GOLD_BRIGHT, &look.body_bold)
                    } else {
                        (theme::TEXT, &look.body)
                    };
                    let mut n = cell(name, color, Some(font), None);
                    n.add_theme_font_size_override("font_size", BODY_SIZE);
                    row.add_child(&n);
                    let mut c = cell(
                        right,
                        theme::GOLD_BRIGHT,
                        Some(&look.body_bold),
                        Some(right_w),
                    );
                    c.add_theme_font_size_override("font_size", BODY_SIZE);
                    c.set_horizontal_alignment(HorizontalAlignment::RIGHT);
                    row.add_child(&c);
                    list.add_child(&row);
                    content_h += ROW_H;
                }
            }
        }
        let below = totals.len() as f32 * ROW_H + 12.0;
        let max_h = (self.screen().y - TEXT_CHROME - below).max(ROW_H * 4.0);
        let text = Scroller::new(width, content_h.min(max_h).max(ROW_H), false);
        let (shade, panel, mut col) = self.frame(width, Some(title));
        let mut scroll = text.scroll.clone();
        scroll.add_child(&list);
        col.add_child(&scroll);
        if !totals.is_empty() {
            col.add_child(&rule());
            for t in totals {
                let mut l = cell(t, theme::TEXT_DIM, Some(&look.body_bold), None);
                l.add_theme_font_size_override("font_size", BODY_SIZE);
                l.set_horizontal_alignment(HorizontalAlignment::RIGHT);
                col.add_child(&l);
            }
        }
        if content_h > max_h {
            self.hint(&mut col, &tr!("show-hint"), width);
        }
        self.buttons(
            &mut col,
            &[(tr!("dlg-ok"), dialog_ui(req, DialogEvent::Close))],
        );
        (
            Kind::Show {
                text,
                layout: Some(layout),
            },
            shade,
            panel,
        )
    }

    /// A creature's symbol on a badge, in its colour.
    fn token(&self, symbol: Option<(char, Color)>) -> Gd<Label> {
        let (text, color) = symbol.map_or((String::new(), theme::TEXT_DIM), |(c, color)| {
            (c.to_string(), color)
        });
        let mut t = cell(&text, color, Some(&self.look.bold), Some(TOKEN_W));
        t.add_theme_font_size_override("font_size", 17);
        t.set_horizontal_alignment(HorizontalAlignment::CENTER);
        t.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        t.set_custom_minimum_size(Vector2::new(TOKEN_W, 24.0));
        t.add_theme_stylebox_override("normal", &self.look.badge);
        // a map's letter, the same in every language
        i18n::verbatim(&t);
        t
    }

    /// #overview: a dungeon a heading with the levels reached, a level a
    /// row (its number on a badge, the player's note, where the hero is),
    /// what is on the level under it.
    /// The screen's foot the HUD's orbs and bar take (the game tells it
    /// every frame): the RPG panels keep above it.
    pub fn set_bottom_room(&mut self, room: f32) {
        self.bottom_room = room;
    }

    /// The most an RPG panel's scrolling content may be tall here: the
    /// panel, `chrome` more, between the screen's top and the HUD's foot.
    fn panel_room(&self, chrome: f32) -> f32 {
        let h = self.screen().y - self.bottom_room - 2.0 * PANEL_EDGE;
        (h - chrome).max(ROW_H * 4.0)
    }

    /// The width of an RPG panel's content here.
    fn panel_width(&self) -> f32 {
        let room = self.screen().x - SIDE_MARGIN - 40.0 - SCROLL_ROOM;
        room.clamp(640.0, PANEL_W)
    }

    /// An RPG panel (the character sheet): a wide frame over the dimmed
    /// game, its name, the key that opens it and a close button at the
    /// top, `content` scrolling under them, and how to scroll and close it
    /// at the foot. `own` is the panel's command as the game knows it: its
    /// key as the player presses it now shows, and closes the panel.
    /// An RPG panel's frame `outer` wide over the dimmed game: its name,
    /// the key that opens it (`own`, as the player presses it now; none
    /// for a panel no key opens) and a close button that sends `close`.
    /// Returns the shade, the frame, its column and the key's name.
    fn panel_frame(
        &mut self,
        req: u64,
        title: &str,
        own: Option<KeyInput>,
        close: DialogEvent,
        outer: f32,
    ) -> (Gd<ColorRect>, Gd<PanelContainer>, Gd<VBoxContainer>, String) {
        let (shade, mut panel, mut col) = self.frame(outer, None);
        // centred in the room above the HUD's orbs and bar, which stay in
        // sight under it
        let lift = self.bottom_room / 2.0;
        place(
            &panel,
            [0.5, 0.5, 0.5, 0.5],
            [-outer / 2.0, -40.0 - lift, outer / 2.0, 40.0 - lift],
        );
        panel.set_v_grow_direction(GrowDirection::BOTH);
        let key = own
            .and_then(|k| self.keymap.key_for(k))
            .map_or_else(|| "—".to_string(), |k| crate::rebind::short_name(&k));
        let mut head = HBoxContainer::new_alloc();
        head.add_theme_constant_override("separation", 14);
        let mut name = theme::styled_label(title, Face::Title, 28, theme::GOLD_BRIGHT);
        name.set_h_size_flags(SizeFlags::EXPAND_FILL);
        name.set_vertical_alignment(VerticalAlignment::CENTER);
        name.set_clip_text(true);
        head.add_child(&name);
        if own.is_some() {
            let mut k = theme::styled_label(&key, Face::Caps, 16, theme::TEXT_DIM);
            k.set_vertical_alignment(VerticalAlignment::CENTER);
            i18n::verbatim(&k);
            // the key is no pad player's: hidden while a gamepad plays
            pad_view::key_words(&k, self.pad);
            head.add_child(&k);
        }
        let mut button = theme::button("✕", &self.queue, dialog_ui(req, close));
        button.set_custom_minimum_size(Vector2::new(36.0, 32.0));
        let tip = if own.is_some() {
            tr!("panel-close-tip", key = key.as_str())
        } else {
            tr!("panel-cancel-tip")
        };
        crate::tips::set(&button, &tip);
        head.add_child(&button);
        col.add_child(&head);
        col.add_child(&rule());
        self.opening_own = own;
        self.opening_rpg = true;
        (shade, panel, col, key)
    }

    fn open_panel(
        &mut self,
        req: u64,
        layout: &'static str,
        title: &str,
        own: KeyInput,
        content: Gd<Control>,
        width: f32,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let outer = width + SCROLL_ROOM;
        let max_h = self.panel_room(PANEL_OWN_CHROME);
        let text = Scroller::new(outer, max_h, false);
        let (shade, panel, mut col, key) =
            self.panel_frame(req, title, Some(own), DialogEvent::Close, outer);
        let mut scroll = text.scroll.clone();
        scroll.add_child(&content);
        col.add_child(&scroll);
        self.hint(&mut col, &tr!("panel-hint", key = key.as_str()), outer);
        // the frame as tall as the content once it is laid out, up to the
        // screen's room (then it scrolls)
        let lift = self.bottom_room / 2.0;
        let (mut sc, c, mut p) = (scroll.clone(), content.clone(), panel.clone());
        content.signals().resized().connect(move || {
            let h = (c.get_size().y + 4.0).clamp(ROW_H, max_h);
            let now = sc.get_custom_minimum_size();
            if (now.y - h).abs() > 1.0 {
                sc.set_custom_minimum_size(Vector2::new(now.x, h));
                // the panel shrinks and grows back both ways, above the
                // HUD's foot
                p.set_offset(godot::builtin::Side::TOP, -40.0 - lift);
                p.set_offset(godot::builtin::Side::BOTTOM, 40.0 - lift);
            }
        });
        (
            Kind::Show {
                text,
                layout: Some(layout),
            },
            shade,
            panel,
        )
    }

    /// `^X`: the character sheet (`sheet_view`), each line said as the
    /// window's translation says it.
    fn open_sheet(
        &mut self,
        req: u64,
        s: &sheet::Sheet,
        english: &[TextLine],
        shown: &[TextLine],
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let said: HashMap<&str, &str> = if english.len() == shown.len() {
            english
                .iter()
                .zip(shown)
                .map(|(e, s)| (e.text.trim(), s.text.trim()))
                .collect()
        } else {
            HashMap::new()
        };
        let say = |line: &str| match said.get(line.trim()) {
            Some(t) => t.to_string(),
            None => i18n::engine(EngineKind::Window, line).into_owned(),
        };
        let width = self.panel_width();
        let view = sheet_view::view(s, &say, width, self.portrait.as_ref());
        let content = view.root.clone();
        let own = crate::rebind::ctrl('x');
        let opened = self.open_panel(req, "sheet", &tr!("sheet-title"), own, content, width);
        self.sheet = Some(view);
        opened
    }

    /// A panel's list (the spells', the skills'): a row of `heights[i]` per
    /// menu entry, a pick's made of its letter's badge and the cells `row`
    /// gives, another's of the control it gives; it scrolls in `col` as a
    /// menu's does.
    fn panel_rows(
        &mut self,
        req: u64,
        state: &MenuState,
        heights: Vec<f32>,
        width: f32,
        col: &mut Gd<VBoxContainer>,
        row: &mut dyn FnMut(usize, &MenuEntry) -> PanelRow,
    ) -> PanelRows {
        let look = self.look.clone();
        let mut tops = Vec::with_capacity(heights.len());
        let mut y = 0.0;
        for h in &heights {
            tops.push(y);
            y += h;
        }
        let max_h = self.panel_room(PANEL_OWN_CHROME + 40.0);
        let view_h = y.min(max_h).max(ROW_H);
        let list = Scroller::new(width, view_h, false);
        let max_top = if y > view_h {
            tops.iter()
                .copied()
                .find(|&t| t >= y - view_h)
                .unwrap_or(y - view_h)
        } else {
            0.0
        };
        let mut rows_box = VBoxContainer::new_alloc();
        rows_box.add_theme_constant_override("separation", 0);
        rows_box.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = Vec::with_capacity(state.entries.len());
        for (i, e) in state.entries.iter().enumerate() {
            match row(i, e) {
                PanelRow::Pick(mut cells, text) if e.selectable => {
                    let mut letter = cell(
                        &e.letter.map_or(String::new(), |l| l.to_string()),
                        theme::GOLD_BRIGHT,
                        Some(&look.body_bold),
                        Some(24.0),
                    );
                    letter.add_theme_font_size_override("font_size", 15);
                    letter.set_horizontal_alignment(HorizontalAlignment::CENTER);
                    letter.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                    letter.set_custom_minimum_size(Vector2::new(24.0, 22.0));
                    letter.add_theme_stylebox_override("normal", &look.badge);
                    i18n::verbatim(&letter);
                    cells.insert(0, letter.upcast());
                    let mut button = self.row_button(req, i, &cells);
                    button.set_custom_minimum_size(Vector2::new(0.0, heights[i]));
                    rows_box.add_child(&button);
                    rows.push(MenuRow::Item {
                        button,
                        mark: None,
                        text,
                    });
                }
                PanelRow::Pick(cells, _) => {
                    // an entry no pick: its cells as a line
                    let mut line = HBoxContainer::new_alloc();
                    line.set_mouse_filter(MouseFilter::IGNORE);
                    line.add_theme_constant_override("separation", 10);
                    line.set_custom_minimum_size(Vector2::new(0.0, heights[i]));
                    let mut pad = Control::new_alloc();
                    pad.set_custom_minimum_size(Vector2::new(24.0, 0.0));
                    line.add_child(&pad);
                    for c in cells {
                        line.add_child(&c);
                    }
                    let mut margin = MarginContainer::new_alloc();
                    margin.add_theme_constant_override("margin_left", 8);
                    margin.add_theme_constant_override("margin_right", 8);
                    margin.set_mouse_filter(MouseFilter::IGNORE);
                    margin.add_child(&line);
                    rows_box.add_child(&margin);
                    rows.push(MenuRow::Fixed);
                }
                PanelRow::Fixed(mut c) => {
                    c.set_custom_minimum_size(Vector2::new(0.0, heights[i]));
                    c.set_mouse_filter(MouseFilter::IGNORE);
                    rows_box.add_child(&c);
                    rows.push(MenuRow::Fixed);
                }
            }
        }
        let filler_h = (max_top + view_h - y).max(0.0);
        if filler_h > 0.0 {
            let mut filler = Control::new_alloc();
            filler.set_custom_minimum_size(Vector2::new(0.0, filler_h));
            filler.set_mouse_filter(MouseFilter::IGNORE);
            rows_box.add_child(&filler);
        }
        let mut scroll = list.scroll.clone();
        scroll.add_child(&rows_box);
        col.add_child(&scroll);
        col.add_child(&rule());
        PanelRows {
            list,
            rows,
            tops,
            heights,
            max_top,
        }
    }

    /// A panel's foot: how its keys work, and its buttons. Returns the
    /// status label (hidden) and the hint's.
    fn panel_foot(
        &mut self,
        col: &mut Gd<VBoxContainer>,
        buttons: &[Gd<Button>],
    ) -> (Gd<Label>, Gd<Label>) {
        let mut status = theme::label("");
        status.set_visible(false);
        col.add_child(&status);
        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 12);
        let mut hint = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        hint.set_vertical_alignment(VerticalAlignment::CENTER);
        pad_view::key_words(&hint, self.pad);
        foot.add_child(&hint);
        for b in buttons {
            foot.add_child(b);
        }
        col.add_child(&foot);
        (status, hint)
    }

    /// The panel's list as a menu the keys, the clicks and a gamepad
    /// drive, its hint `own_hint`.
    fn panel_menu(
        &self,
        state: MenuState,
        rows: PanelRows,
        items: &[nh_protocol::MenuItem],
        (status, hint): (Gd<Label>, Gd<Label>),
        own_hint: String,
    ) -> Kind {
        let colors = items.iter().map(|i| i.clr).collect();
        let mut view = MenuView {
            state,
            rows: rows.rows,
            tops: rows.tops,
            heights: rows.heights,
            max_top: rows.max_top,
            colors,
            list: rows.list,
            cursor: None,
            armed: false,
            status,
            hint,
            own_hint: Some(own_hint),
        };
        // a gamepad starts on the first pick, ready for A
        if self.pad
            && let Some(i) = view.state.entries.iter().position(|e| e.selectable)
        {
            view.cursor = Some(i);
            view.armed = true;
        }
        view.refresh(&self.look);
        Kind::Menu(Box::new(view))
    }

    /// spell.c's menus (`+`, `Z`, the swap's): the spells' panel, a card a
    /// spell, and beside the list the full card of the spell under the
    /// pointer or the focus. A pick casts it (from `+`'s, through `Z` once
    /// the menu is answered: `take_cast`), unless the player reorders the
    /// spells, when it is the engine's swap; `[sort spells]` is a row of
    /// its own. A card dragged onto the action bar binds the spell there.
    fn open_spells(
        &mut self,
        req: u64,
        how: PickHow,
        items: &[nh_protocol::MenuItem],
        book: Spellbook,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        use spells_view::{DETAIL_W, GAP, LIST_W};
        let state = MenuState::new(how, None, items);
        // the full card where the screen has room for it beside the list
        let room = self.panel_width();
        let with_card = room >= LIST_W + GAP + DETAIL_W;
        let list_w = LIST_W.min(room);
        let width = if with_card {
            list_w + GAP + DETAIL_W
        } else {
            list_w
        };
        let plain = |c| KeyInput::plain(Key::Char(c));
        let (title, own) = match book.purpose {
            Purpose::Cast => (tr!("spells-cast-title"), Some(plain('Z'))),
            Purpose::View => (tr!("spells-title"), Some(plain('+'))),
            // the spell by its name, not its letter (no pad player's key)
            Purpose::Swap(letter) => match book.spells.iter().find(|s| s.letter == letter) {
                Some(s) => (
                    tr!(
                        "spells-swap-title-named",
                        spell = spells_view::spell_name(&s.name)
                    ),
                    None,
                ),
                None => (tr!("spells-swap-title", letter = letter.to_string()), None),
            },
        };
        let (shade, panel, mut col, key) =
            self.panel_frame(req, &title, own, DialogEvent::MenuCancel, width);
        let heights: Vec<f32> = state
            .entries
            .iter()
            .map(|e| match () {
                _ if !e.selectable => 0.0,
                _ if book.sort.is_some_and(|(idx, _)| idx == e.idx) => spells_view::SORT_H,
                _ => spells_view::CARD_H,
            })
            .collect();
        let mut body = HBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", GAP as i32);
        let mut list_col = VBoxContainer::new_alloc();
        list_col.add_theme_constant_override("separation", 0);
        list_col.set_custom_minimum_size(Vector2::new(list_w, 0.0));
        let pw = self.energy;
        let rows = self.panel_rows(req, &state, heights, list_w, &mut list_col, &mut |_, e| {
            if !e.selectable {
                return PanelRow::Fixed(Control::new_alloc());
            }
            let (cells, text) = match book.spell(e.idx) {
                Some(s) => spells_view::cells(s, pw),
                None => spells_view::sort_cells(),
            };
            PanelRow::Pick(cells, text)
        });
        body.add_child(&list_col);
        let detail = with_card.then(|| {
            let d = spells_view::Detail::new(DETAIL_W, rows.list.view_h, &self.queue, self.pad);
            body.add_child(&d.root);
            Rc::new(RefCell::new(d))
        });
        col.add_child(&body);
        // each card: its letter no pad player's key; under the pointer, its
        // full card; dragged, onto the bar
        for (i, row) in rows.rows.iter().enumerate() {
            let MenuRow::Item { button, .. } = row else {
                continue;
            };
            let letter = button
                .get_child(0)
                .and_then(|h| h.get_child(0))
                .and_then(|l| l.try_cast::<Label>().ok());
            if let Some(letter) = letter {
                pad_view::key_words(&letter, self.pad);
            }
            let Some(s) = state.entries.get(i).and_then(|e| book.spell(e.idx)) else {
                continue;
            };
            spells_view::card_back(button, s);
            if let Some(d) = &detail {
                let (d, s) = (d.clone(), s.clone());
                let b = button.clone();
                b.signals()
                    .mouse_entered()
                    .connect(move || d.borrow_mut().show(&s, pw));
            }
            spells_view::draggable(button, s, self.bar_slots.clone(), self.queue.clone());
        }
        // `+`'s: the reorder, a pick then the engine's swap
        let reorders = book.spells.len() > 1;
        let button = (book.purpose == Purpose::View && reorders).then(|| {
            let ev = dialog_ui(req, DialogEvent::Reorder);
            let mut b = theme::button(&tr!("spells-reorder"), &self.queue, ev);
            b.set_custom_minimum_size(Vector2::new(132.0, 34.0));
            b
        });
        let foot = self.panel_foot(&mut col, button.as_slice());
        let purpose = book.purpose;
        // a single spell: nothing to reorder
        let hint = if purpose == Purpose::View && !reorders {
            spells_hint(Purpose::Cast, false, &key)
        } else {
            spells_hint(purpose, false, &key)
        };
        let kind = self.panel_menu(state, rows, items, foot, hint);
        // the full card of the focus (a gamepad's), else of the first spell
        if let Some(d) = &detail {
            let focus = match &kind {
                Kind::Menu(view) => view
                    .cursor
                    .and_then(|c| view.state.entries.get(c))
                    .and_then(|e| book.spell(e.idx)),
                _ => None,
            };
            if let Some(s) = focus.or(book.spells.first()) {
                d.borrow_mut().show(s, pw);
            }
        }
        self.spells = Some(SpellsOpen {
            book,
            reorder: false,
            button,
            detail,
        });
        (kind, shade, panel)
    }

    /// The spells' panel: its full card follows the keyboard's (a
    /// gamepad's) focus.
    fn spells_focus(&mut self) {
        let Some(sp) = self.spells.as_ref() else {
            return;
        };
        let Some(d) = sp.detail.as_ref() else {
            return;
        };
        let Some(Open {
            kind: Kind::Menu(view),
            ..
        }) = self.open.as_ref()
        else {
            return;
        };
        let focus = view
            .cursor
            .and_then(|c| view.state.entries.get(c))
            .and_then(|e| sp.book.spell(e.idx));
        if let Some(s) = focus {
            d.borrow_mut().show(s, self.energy);
        }
    }

    /// The spell the spells' panel's full card shows (self-tests).
    pub fn spells_card(&self) -> Option<String> {
        self.spells.as_ref()?.detail.as_ref()?.borrow().shown()
    }

    /// The full card's button that puts its spell on the bar, pressed
    /// (self-tests).
    pub fn press_spells_to_bar(&mut self) -> bool {
        let Some(d) = self.spells.as_ref().and_then(|sp| sp.detail.clone()) else {
            return false;
        };
        d.borrow_mut().press_to_bar();
        true
    }

    /// The spells' cards on the canvas, by their spells' English names
    /// (self-tests).
    pub fn spells_cards(&self) -> Vec<(String, Rect2)> {
        let (Some(sp), Some(open)) = (self.spells.as_ref(), self.open.as_ref()) else {
            return Vec::new();
        };
        let Kind::Menu(view) = &open.kind else {
            return Vec::new();
        };
        view.rows
            .iter()
            .zip(&view.state.entries)
            .filter_map(|(row, e)| match row {
                MenuRow::Item { button, .. } => {
                    let s = sp.book.spell(e.idx)?;
                    Some((s.name.clone(), button.get_global_rect()))
                }
                MenuRow::Fixed => None,
            })
            .collect()
    }

    /// The action bar's slots as the spells' cards know them (self-tests).
    pub fn bar_slots(&self) -> Vec<Rect2> {
        self.bar_slots.borrow().clone()
    }

    /// The words the open dialog shows (self-tests: a gamepad's has no
    /// keyboard key).
    pub fn panel_words(&self) -> Vec<String> {
        self.open
            .as_ref()
            .map(|o| pad_view::shown_words(&o.panel))
            .unwrap_or_default()
    }

    /// #enhance's window: the skills' panel, a skill a row by group, with
    /// its level and its mark; a skill that can advance is a pick that
    /// advances it. `own`: #enhance's key, if it has one.
    fn open_skills(
        &mut self,
        req: u64,
        how: PickHow,
        items: &[nh_protocol::MenuItem],
        skills: &crate::skills::Skills,
        own: Option<KeyInput>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        use crate::skills::{self as sk, Group};
        let state = MenuState::new(how, None, items);
        let width = SPELLS_W.min(self.panel_width());
        let title = match skills.slots {
            Some(n) => tr!("skills-title-slots", slots = n),
            None => tr!("skills-title"),
        };
        let close = if how == PickHow::None {
            DialogEvent::MenuConfirm
        } else {
            DialogEvent::MenuCancel
        };
        let (shade, panel, mut col, key) = self.panel_frame(req, &title, own, close, width);
        let heading = |e: &MenuEntry| !e.selectable && e.attr & 0x0f != 0;
        let heights: Vec<f32> = state
            .entries
            .iter()
            .map(|e| match () {
                _ if e.text.trim().is_empty() => 8.0,
                _ if heading(e) => skills_view::HEADING_H,
                _ if sk::skill(&e.text, None).is_some() => skills_view::ROW_H,
                _ => skills_view::NOTE_H,
            })
            .collect();
        let mut in_group = false;
        let rows = self.panel_rows(req, &state, heights, width, &mut col, &mut |_, e| {
            if e.text.trim().is_empty() {
                return PanelRow::Fixed(Control::new_alloc());
            }
            if heading(e) {
                in_group = true;
                let name = skills_view::group_name(Group::of(&e.text), e.text.trim());
                return PanelRow::Fixed(skills_view::heading(&name));
            }
            let pick = e.selectable.then_some(e.idx);
            match sk::skill(&e.text, pick) {
                Some(s) if in_group => {
                    let (cells, text) = skills_view::cells(&s);
                    PanelRow::Pick(cells, text)
                }
                _ => PanelRow::Fixed(skills_view::note(e.text.trim())),
            }
        });
        let foot = self.panel_foot(&mut col, &[]);
        let hint = if skills.advance {
            tr!("skills-hint-advance", key = key.as_str())
        } else {
            tr!("panel-hint", key = key.as_str())
        };
        let kind = self.panel_menu(state, rows, items, foot, hint);
        (kind, shade, panel)
    }

    /// The letter of the open menu's entry `idx` (self-tests).
    pub fn menu_letter(&self, idx: i32) -> Option<char> {
        match &self.open.as_ref()?.kind {
            Kind::Menu(view) => view.state.entries.iter().find(|e| e.idx == idx)?.letter,
            _ => None,
        }
    }

    /// What the open spells' panel's picks do, and whether they reorder
    /// (self-tests).
    pub fn spells_open(&self) -> Option<(Purpose, bool)> {
        self.spells.as_ref().map(|sp| (sp.book.purpose, sp.reorder))
    }

    /// What the game is to do once the engine is back at its command
    /// prompt, once: cast the spell `+`'s panel picked, open the codex's
    /// other tab.
    pub fn take_follow(&mut self) -> Option<Follow> {
        self.follow.take()
    }

    /// A reply of the spells' panel as the engine is to have it: a pick in
    /// `+`'s that casts closes its menu instead (the cast follows).
    fn spells_reply(&mut self, reply: Reply) -> Reply {
        let Some(sp) = self.spells.as_ref() else {
            return reply;
        };
        if sp.book.purpose != Purpose::View || sp.reorder {
            return reply;
        }
        let picked = match &reply {
            Reply::Menu(picked) => picked.first().map(|(idx, _)| *idx),
            _ => None,
        };
        let spell = picked.and_then(|idx| sp.book.spell(i32::try_from(idx).ok()?));
        match spell {
            Some(s) => {
                self.follow = Some(Follow::Cast(s.name.clone()));
                Reply::Cancel
            }
            None => reply,
        }
    }

    /// The spells' panel: a pick reorders now, or casts again.
    fn toggle_reorder(&mut self) {
        let key = self.open_own_key();
        let Some(sp) = self.spells.as_mut() else {
            return;
        };
        if sp.book.purpose != Purpose::View || sp.book.spells.len() < 2 {
            return;
        }
        sp.reorder = !sp.reorder;
        let hint = spells_hint(Purpose::View, sp.reorder, &key);
        if let Some(b) = sp.button.as_mut() {
            b.set_text(&if sp.reorder {
                tr!("spells-cast-instead")
            } else {
                tr!("spells-reorder")
            });
        }
        if let Some(Open {
            kind: Kind::Menu(view),
            ..
        }) = self.open.as_mut()
        {
            view.own_hint = Some(hint);
            view.refresh(&self.look);
        }
    }

    /// The open panel's own key's name as the player presses it now.
    fn open_own_key(&self) -> String {
        self.open
            .as_ref()
            .and_then(|o| o.own)
            .and_then(|k| self.keymap.key_for(k))
            .map_or_else(|| "—".to_string(), |k| crate::rebind::short_name(&k))
    }

    /// #chronicle: the journal (`journal_view`), the events logged beside
    /// the message history; the keys scroll the messages. The game puts
    /// the messages in (`fill_journal`).
    fn open_journal(
        &mut self,
        req: u64,
        j: &journal::Journal,
        catalog: Option<&Catalog>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let width = self.panel_width().min(JOURNAL_W);
        let height = self.panel_room(PANEL_OWN_CHROME).min(JOURNAL_H);
        let (_, messages_w) = journal_view::widths(width);
        let text = Scroller::new(messages_w, height - 40.0, false);
        let view = journal_view::view(j, width, height, text.scroll.clone());
        let own = ext_key(catalog, "chronicle");
        let (shade, panel, mut col, key) =
            self.panel_frame(req, &tr!("journal-title"), own, DialogEvent::Close, width);
        col.add_child(&view.root);
        let hint = match own {
            Some(_) => tr!("journal-hint", key = key.as_str()),
            None => tr!("journal-hint-esc"),
        };
        self.hint(&mut col, &hint, width);
        self.journal = Some(view);
        (
            Kind::Show {
                text,
                layout: Some("journal"),
            },
            shade,
            panel,
        )
    }

    /// The open journal still wants the messages.
    pub fn journal_wants_messages(&self) -> bool {
        self.journal
            .as_ref()
            .is_some_and(JournalView::wants_messages)
    }

    /// The open journal's messages as they show (self-tests).
    pub fn journal_messages(&self) -> Option<String> {
        self.journal.as_ref().map(JournalView::messages_text)
    }

    /// The open journal's messages: the log's, the newest in view.
    pub fn fill_journal<'a>(&mut self, log: impl Iterator<Item = &'a nh_world::Message>) {
        let Some(view) = self.journal.as_mut() else {
            return;
        };
        view.fill(log);
        if let Some(Open {
            kind: Kind::Show { text, .. },
            ..
        }) = self.open.as_mut()
        {
            text.scroll_to(1.0e9);
        }
    }

    /// The discoveries' cards: each object's name and looks as the
    /// window's translation says them, its appearance's icon. Also how
    /// many cards have an icon, of all.
    fn discoveries_view(
        &self,
        d: &codex::Discoveries,
        english: &[TextLine],
        shown: &[TextLine],
        catalog: Option<&Catalog>,
    ) -> (Gd<Control>, (usize, usize)) {
        let said: HashMap<&str, &str> = if english.len() == shown.len() {
            english
                .iter()
                .zip(shown)
                .map(|(e, s)| (e.text.as_str(), s.text.as_str()))
                .collect()
        } else {
            HashMap::new()
        };
        let say = |line: &str| -> String {
            said.get(line).map_or_else(
                || i18n::engine(EngineKind::Window, line).into_owned(),
                |t| t.to_string(),
            )
        };
        let heading = |h: &str| say(h).trim().to_string();
        // a card's words: its line as said, its looks split off the end
        let card = |e: &codex::Discovery| -> (String, Option<String>) {
            let mark = if e.unseen { "* " } else { "  " };
            let line = english
                .iter()
                .map(|l| l.text.as_str())
                .find(|l| l.strip_prefix(mark).is_some_and(|r| r.starts_with(&e.text)))
                .map_or_else(|| format!("{mark}{}", e.text), str::to_string);
            codex::card_words(&say(&line), e)
        };
        // an object's icon: its appearance's, as the inventory shows it
        let counts = std::cell::Cell::new((0, 0));
        let icon = |heading: &str, e: &codex::Discovery| -> Option<Gd<Texture2D>> {
            let looks = e.appearance.as_deref().unwrap_or(&e.name);
            let class = codex::class_of(heading);
            let found = catalog.zip(class).and_then(|(c, class)| {
                c.object_tiles
                    .iter()
                    .find(|t| t.class.starts_with(class) && t.appearance == looks)
            });
            // the relics and the artifacts have no class's icon
            if class.is_some() {
                let (with, all) = counts.get();
                counts.set((with + usize::from(found.is_some()), all + 1));
            }
            let t = found?;
            // an identified book's spell, as the pack shows it
            Some(crate::icons::named_icon(t.tile, class?, Some(&e.name)))
        };
        let view =
            codex_view::discoveries(d, self.panel_width().min(JOURNAL_W), &heading, &card, &icon);
        (view, counts.get())
    }

    /// `\` and #conduct: the codex, a tab each; the one shown is the
    /// engine's window, Tab or the other tab asks for the other.
    fn open_codex(
        &mut self,
        req: u64,
        tab: CodexTab,
        content: Gd<Control>,
        catalog: Option<&Catalog>,
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let width = self.panel_width().min(JOURNAL_W);
        let outer = width + SCROLL_ROOM;
        let own = match tab {
            CodexTab::Discoveries => ext_key(catalog, "known"),
            CodexTab::Conduct => ext_key(catalog, "conduct"),
        };
        let (shade, panel, mut col, key) =
            self.panel_frame(req, &tr!("codex-title"), own, DialogEvent::Close, outer);
        // the tabs
        let mut tabs = HBoxContainer::new_alloc();
        tabs.add_theme_constant_override("separation", 8);
        for (t, name) in [
            (CodexTab::Discoveries, tr!("codex-discoveries")),
            (CodexTab::Conduct, tr!("codex-conduct")),
        ] {
            let mut b = theme::button(&name, &self.queue, dialog_ui(req, DialogEvent::SwitchTab));
            b.set_custom_minimum_size(Vector2::new(160.0, 34.0));
            if t == tab {
                b.add_theme_stylebox_override("normal", &self.look.focus_button);
                b.add_theme_color_override("font_color", theme::GOLD_BRIGHT);
                b.set_disabled(true);
                b.add_theme_stylebox_override("disabled", &self.look.focus_button);
                b.add_theme_color_override("font_disabled_color", theme::GOLD_BRIGHT);
            }
            tabs.add_child(&b);
        }
        col.add_child(&tabs);
        let max_h = self.panel_room(PANEL_OWN_CHROME + 44.0);
        let text = Scroller::new(outer, max_h, false);
        let mut scroll = text.scroll.clone();
        scroll.add_child(&content);
        col.add_child(&scroll);
        let hint = match own {
            Some(_) => tr!("codex-hint", key = key.as_str()),
            None => tr!("codex-hint-esc"),
        };
        self.hint(&mut col, &hint, outer);
        // as tall as the content once it is laid out, up to the room
        let lift = self.bottom_room / 2.0;
        let (mut sc, c, mut p) = (scroll.clone(), content.clone(), panel.clone());
        content.signals().resized().connect(move || {
            let h = (c.get_size().y + 4.0).clamp(ROW_H, max_h);
            let now = sc.get_custom_minimum_size();
            if (now.y - h).abs() > 1.0 {
                sc.set_custom_minimum_size(Vector2::new(now.x, h));
                p.set_offset(godot::builtin::Side::TOP, -40.0 - lift);
                p.set_offset(godot::builtin::Side::BOTTOM, 40.0 - lift);
            }
        });
        self.codex = Some(tab);
        (
            Kind::Show {
                text,
                layout: Some("codex"),
            },
            shade,
            panel,
        )
    }

    /// The codex's other tab: this window answered, the other's command
    /// to follow (`take_follow`).
    fn switch_codex(&mut self) -> Option<Reply> {
        let tab = self.codex?;
        self.follow = Some(match tab {
            CodexTab::Discoveries => Follow::Ext("conduct"),
            CodexTab::Conduct => Follow::Key('\\' as i32),
        });
        Some(Reply::Ack)
    }

    /// The codex's tab open (self-tests).
    pub fn codex_tab(&self) -> Option<CodexTab> {
        self.codex
    }

    /// The discoveries' cards with an icon, of all (self-tests).
    pub fn codex_icons(&self) -> (usize, usize) {
        self.codex_icons
    }

    fn open_overview(
        &mut self,
        req: u64,
        rows: &[OverviewRow],
    ) -> (Kind, Gd<ColorRect>, Gd<PanelContainer>) {
        let look = self.look.clone();
        let lines = overview_lines(rows);
        let indent = BADGE_W + 12.0;
        let line_w = |l: &OverviewLine| match l {
            OverviewLine::Dungeon { name, levels } => {
                look.text_w(name, &look.title, BODY_SIZE + 4)
                    + 24.0
                    + levels
                        .as_deref()
                        .map_or(0.0, |t| look.text_w(t, &look.body, BODY_SIZE - 1))
            }
            OverviewLine::Level {
                title, note, here, ..
            } => {
                indent
                    + look.text_w(title, &look.body_bold, BODY_SIZE)
                    + note
                        .as_deref()
                        .map_or(0.0, |n| 12.0 + look.text_w(n, &look.body, BODY_SIZE))
                    + here.as_deref().map_or(0.0, |h| {
                        28.0 + look.text_w(h, &look.body_bold, BODY_SIZE - 2)
                    })
            }
            OverviewLine::Detail { text, deep, .. } => {
                indent
                    + if *deep { 22.0 } else { 0.0 }
                    + 20.0
                    + look.text_w(text, &look.body, BODY_SIZE - 1)
            }
        };
        let longest = lines.iter().map(line_w).fold(0.0f32, f32::max);
        let title = tr!("overview-title");
        let title_w = look.text_w(&title, &look.body_bold, 20);
        let max_w = (self.screen().x - 2.0 * SIDE_MARGIN).max(420.0);
        let width = (longest + 40.0).max(title_w + 40.0).clamp(480.0, max_w);
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 0);
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut content_h = 0.0;
        for (i, l) in lines.iter().enumerate() {
            let (row, h) = match l {
                OverviewLine::Dungeon { name, levels } => {
                    let mut head = HBoxContainer::new_alloc();
                    head.add_theme_constant_override("separation", 24);
                    let mut n = cell(name, theme::ACCENT, Some(&look.title), None);
                    n.add_theme_font_size_override("font_size", BODY_SIZE + 4);
                    head.add_child(&n);
                    if let Some(levels) = levels {
                        let w = look.text_w(levels, &look.body, BODY_SIZE - 1) + 4.0;
                        let mut r = cell(levels, theme::TEXT_DIM, Some(&look.body), Some(w));
                        r.add_theme_font_size_override("font_size", BODY_SIZE - 1);
                        r.set_horizontal_alignment(HorizontalAlignment::RIGHT);
                        head.add_child(&r);
                    }
                    head.set_custom_minimum_size(Vector2::new(0.0, ROW_H + 2.0));
                    let mut block = VBoxContainer::new_alloc();
                    block.add_theme_constant_override("separation", 2);
                    // air between dungeons
                    let gap = if i == 0 { 0.0 } else { 10.0 };
                    let mut air = Control::new_alloc();
                    air.set_custom_minimum_size(Vector2::new(0.0, gap));
                    block.add_child(&air);
                    block.add_child(&head);
                    block.add_child(&rule());
                    (block.upcast::<Control>(), ROW_H + 2.0 + gap + 7.0)
                }
                OverviewLine::Level {
                    badge,
                    title,
                    note,
                    here,
                    current,
                } => {
                    let mut row = HBoxContainer::new_alloc();
                    row.add_theme_constant_override("separation", 12);
                    let mut b = cell(
                        badge,
                        theme::GOLD_BRIGHT,
                        Some(&look.body_bold),
                        Some(BADGE_W),
                    );
                    b.add_theme_font_size_override("font_size", BODY_SIZE - 1);
                    b.set_horizontal_alignment(HorizontalAlignment::CENTER);
                    b.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                    b.set_custom_minimum_size(Vector2::new(BADGE_W, 24.0));
                    let style = if *current { &look.mark_on } else { &look.badge };
                    b.add_theme_stylebox_override("normal", style);
                    row.add_child(&b);
                    let w = look.text_w(title, &look.body_bold, BODY_SIZE) + 4.0;
                    let mut t = cell(title, theme::TEXT, Some(&look.body_bold), Some(w));
                    t.add_theme_font_size_override("font_size", BODY_SIZE);
                    row.add_child(&t);
                    // the player's note takes what room there is
                    let mut n = cell(
                        note.as_deref().unwrap_or(""),
                        theme::GOLD,
                        Some(&look.body),
                        None,
                    );
                    n.add_theme_font_size_override("font_size", BODY_SIZE);
                    row.add_child(&n);
                    if let Some(h) = here {
                        row.add_child(&here_pill(h, *current, &look));
                    }
                    row.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
                    (row.upcast::<Control>(), ROW_H)
                }
                OverviewLine::Detail { mark, text, deep } => {
                    let mut row = HBoxContainer::new_alloc();
                    row.add_theme_constant_override("separation", 6);
                    let mut pad = Control::new_alloc();
                    let deeper = if *deep { 22.0 } else { 0.0 };
                    pad.set_custom_minimum_size(Vector2::new(indent + deeper - 6.0, 0.0));
                    row.add_child(&pad);
                    let mut m = cell(mark, theme::GOLD_DIM, Some(&look.body_bold), Some(14.0));
                    m.add_theme_font_size_override("font_size", BODY_SIZE - 1);
                    i18n::verbatim(&m);
                    row.add_child(&m);
                    let mut t = cell(text, INFO_TEXT, Some(&look.body), None);
                    t.add_theme_font_size_override("font_size", BODY_SIZE - 1);
                    row.add_child(&t);
                    row.set_custom_minimum_size(Vector2::new(0.0, DETAIL_H));
                    (row.upcast::<Control>(), DETAIL_H)
                }
            };
            list.add_child(&row);
            content_h += h;
        }
        let max_h = (self.screen().y - TEXT_CHROME).max(ROW_H * 4.0);
        let text = Scroller::new(width, content_h.min(max_h).max(ROW_H), false);
        let (shade, panel, mut col) = self.frame(width, Some(&title));
        let mut scroll = text.scroll.clone();
        scroll.add_child(&list);
        col.add_child(&scroll);
        if content_h > max_h {
            self.hint(&mut col, &tr!("show-hint"), width);
        }
        self.buttons(
            &mut col,
            &[(tr!("dlg-ok"), dialog_ui(req, DialogEvent::Close))],
        );
        (
            Kind::Show {
                text,
                layout: Some("overview"),
            },
            shade,
            panel,
        )
    }

    /// Open the UI for Menu, Choice, Text, ExtCmd, Show, MessageMenu (other
    /// prompts: no-op).
    pub fn open(&mut self, req: u64, original: &Prompt, catalog: Option<&Catalog>) {
        self.close();
        self.opening_cursor = None;
        // the engine's words as the player reads them; the original is kept
        // (a switch of the language opens it again)
        let shown = shown_prompt(original);
        let prompt = &shown;
        let (kind, shade, panel) = match prompt {
            Prompt::Menu {
                how, title, items, ..
            } => {
                // #overview: laid out from the engine's English
                let english = match original {
                    Prompt::Menu { items, .. } => items.as_slice(),
                    _ => items.as_slice(),
                };
                let texts: Vec<&str> = english
                    .iter()
                    .map(|i| i.str.as_deref().unwrap_or(""))
                    .collect();
                let english_title = match original {
                    Prompt::Menu { title, .. } => title.as_deref(),
                    _ => None,
                };
                // #enhance's pick of a skill to advance
                let skill_lines: Vec<(String, bool, Option<i32>)> = english
                    .iter()
                    .map(|i| {
                        let heading = !i.selectable && i.attr & 0x0f != 0;
                        let text = i.str.clone().unwrap_or_default();
                        (text, heading, i.selectable.then_some(i.idx))
                    })
                    .collect();
                let skilled = skills::skills(english_title, &skill_lines);
                // what can be done here, on the hero: a gamepad starts on
                // looking, the one that does nothing (never on the stairs:
                // two stray A's would leave the level, or the dungeon)
                self.opening_cursor = (english_title == Some(THERE_MENU))
                    .then(|| texts.iter().position(|t| *t == LOOK_HERE))
                    .flatten();
                // the looting's list: the container's panel
                if let Some((view, pack)) = self.loot_next.take() {
                    let (kind, shade, panel) = self.open_loot(req, &view, &pack, english);
                    let pad_room = add_pad_room(&panel);
                    self.open = Some(Open {
                        req,
                        prompt: original.clone(),
                        kind,
                        own: None,
                        rpg: false,
                        shade,
                        panel,
                        pad_room,
                    });
                    return;
                }
                // what can be done at the cell clicked: a popup by it
                let anchor = (english_title == Some(cell_menu::TITLE) && *how == PickHow::One)
                    .then(|| self.cell_anchor.take())
                    .flatten()
                    .filter(|(_, _, at)| at.elapsed().as_secs_f32() < 2.0);
                if let Some((at, look, _)) = anchor {
                    let (kind, shade, panel) = self.open_cell_menu(req, english, at, &look);
                    self.open = Some(Open {
                        req,
                        prompt: original.clone(),
                        kind,
                        own: None,
                        rpg: false,
                        shade,
                        panel,
                        pad_room: None,
                    });
                    return;
                }
                let book = spells::spellbook(english_title, english);
                if let Some(b) = &book {
                    self.known_spells = b.spells.clone();
                }
                let opened = match (book, skilled) {
                    (Some(book), _) => Some(self.open_spells(req, *how, english, book)),
                    (None, Some(sk)) => {
                        let own = ext_key(catalog, "enhance");
                        Some(self.open_skills(req, *how, english, &sk, own))
                    }
                    _ => None,
                };
                if let Some(opened) = opened {
                    let (kind, shade, panel) = opened;
                    let pad_room = add_pad_room(&panel);
                    let own = self.opening_own.take();
                    let rpg = std::mem::take(&mut self.opening_rpg);
                    self.open = Some(Open {
                        req,
                        prompt: original.clone(),
                        kind,
                        own,
                        rpg,
                        shade,
                        panel,
                        pad_room,
                    });
                    return;
                }
                match layouts::overview(&texts) {
                    Some(rows) if *how == PickHow::None => self.open_overview(req, &rows),
                    // `m` #overview picks a level to annotate: the menu, in
                    // the language now
                    Some(_) => {
                        let shown: Vec<nh_protocol::MenuItem> = english
                            .iter()
                            .map(|i| nh_protocol::MenuItem {
                                str: i.str.as_deref().map(|t| {
                                    if t.trim().is_empty() {
                                        t.to_string()
                                    } else {
                                        layouts::overview_line(&layouts::overview_row(t))
                                    }
                                }),
                                ..i.clone()
                            })
                            .collect();
                        self.open_menu(req, *how, Some(&tr!("overview-title")), &shown)
                    }
                    None => {
                        // the lines as the game wrote them: an object's
                        // icon by its English name (a book's spell)
                        self.opening_english = texts.iter().map(|t| t.to_string()).collect();
                        let opened = self.open_menu(req, *how, title.as_deref(), items);
                        self.opening_english.clear();
                        opened
                    }
                }
            }
            Prompt::Choice {
                query,
                visible,
                allowed,
                default,
            } => {
                let english = match original {
                    Prompt::Choice { query, .. } => query.as_str(),
                    _ => query.as_str(),
                };
                // a debug death's two questions, each in its own words
                let debug = debug_end(english);
                let title = debug.as_ref().map_or(query.as_str(), |d| d.title.as_str());
                let width = self.fit_width(title.chars().count().min(80), 0.0, 420.0);
                let (shade, panel, mut col) = self.frame_at(width, Some(title), Place::Top);
                if quits(english) {
                    col.add_child(&quit_warning(width));
                }
                if let Some(d) = &debug {
                    col.add_child(&about_line(&d.about, width));
                }
                let mut named: Vec<(char, String)> = visible
                    .iter()
                    .filter(|c| !matches!(c, 'y' | 'n' | 'q' | 'a'))
                    .filter_map(|&c| answer_word(c, english, query).map(|w| (c, w)))
                    .collect();
                if let Some(d) = &debug {
                    named.extend([('y', d.yes.clone()), ('n', d.no.clone())]);
                }
                let word = |c: char| named.iter().find(|(n, _)| *n == c).map(|(_, w)| w.as_str());
                let items: Vec<(String, UiEvent)> = visible
                    .iter()
                    .map(|&c| {
                        let ev = dialog_ui(req, DialogEvent::Choice(c));
                        (named_choice(c, self.pad, word(c)), ev)
                    })
                    .collect();
                let buttons = self.buttons(&mut col, &items);
                let mut hint = tr!("choice-hint");
                if let Some(d) = *default {
                    let answer = named_choice(d, false, word(d));
                    hint = tr!("choice-hint-default", hint = hint, answer = answer);
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
                    named,
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
                    None => {
                        let opened =
                            self.open_text(req, query, *name, pickers::is_engraving(english));
                        // paranoid_confirmation:quit asks for the word
                        if quits(english) {
                            let width = self.fit_width(query.chars().count().min(70), 0.0, 560.0);
                            let mut warning = quit_warning(width);
                            if let Some(mut col) = opened.2.get_child(0) {
                                col.add_child(&warning);
                                col.move_child(&warning, 1);
                            } else {
                                warning.queue_free();
                            }
                        }
                        opened
                    }
                }
            }
            Prompt::ExtCmd => match self.wheel_for(req) {
                Some(next) => self.open_wheel(req, catalog, next),
                None => self.open_palette(req, catalog),
            },
            Prompt::Show { title, lines } => {
                // #vanquished, #genocided, #overview (a menu of no choice
                // comes as a text window): laid out from the engine's English
                let english = match original {
                    Prompt::Show { lines, .. } => lines.as_slice(),
                    _ => lines.as_slice(),
                };
                let texts: Vec<&str> = english.iter().map(|l| l.text.as_str()).collect();
                let plurals = layouts::kinds_by_plural(catalog);
                let singular = |p: &str| plurals.get(p).cloned();
                let english_title = match original {
                    Prompt::Show { title, .. } => title.as_deref(),
                    _ => None,
                };
                let skill_lines: Vec<(String, bool, Option<i32>)> = english
                    .iter()
                    .map(|l| (l.text.clone(), l.attr & 0x0f != 0, None))
                    .collect();
                let headed: Vec<(String, bool)> = english
                    .iter()
                    .map(|l| (l.text.clone(), l.attr & 0x0f != 0))
                    .collect();
                if let Some((items, book)) = spells::shown(english_title, &headed) {
                    self.known_spells = book.spells.clone();
                    // `+` with a single spell known: a menu of no choice,
                    // the spells' panel all the same (a pick casts it)
                    self.open_spells(req, PickHow::One, &items, book)
                } else if let Some(s) = sheet::sheet(&texts) {
                    self.open_sheet(req, &s, english, lines)
                } else if let Some(j) = journal::journal(&texts) {
                    self.open_journal(req, &j, catalog)
                } else if let Some(c) = codex::conduct(&texts) {
                    let content = codex_view::conduct(&c, self.panel_width().min(JOURNAL_W));
                    self.open_codex(req, CodexTab::Conduct, content, catalog)
                } else if let Some(d) = codex::discoveries(
                    &english
                        .iter()
                        .map(|l| (l.text.clone(), l.attr & 0x0f != 0))
                        .collect::<Vec<_>>(),
                ) {
                    let (content, icons) = self.discoveries_view(&d, english, lines, catalog);
                    self.codex_icons = icons;
                    self.open_codex(req, CodexTab::Discoveries, content, catalog)
                } else if let Some(sk) = skills::skills(english_title, &skill_lines) {
                    // #enhance with nothing to advance: its lines as a menu's
                    let items: Vec<nh_protocol::MenuItem> = english
                        .iter()
                        .enumerate()
                        .map(|(i, l)| nh_protocol::MenuItem {
                            win: 0,
                            idx: i as i32,
                            glyph: None,
                            selectable: false,
                            ch: 0,
                            gch: 0,
                            attr: l.attr,
                            clr: NO_COLOR,
                            str: Some(l.text.clone()),
                            preselected: false,
                            skipinvert: false,
                        })
                        .collect();
                    let own = ext_key(catalog, "enhance");
                    self.open_skills(req, PickHow::None, &items, &sk, own)
                } else if let Some(v) = layouts::vanquished(&texts, &singular) {
                    self.open_vanquished(req, &v, catalog)
                } else if let Some(g) = layouts::genocided(&texts) {
                    self.open_genocided(req, &g, &plurals, catalog)
                } else if let Some(rows) = layouts::overview(&texts) {
                    self.open_overview(req, &rows)
                } else {
                    self.open_show(title.as_deref(), lines, req)
                }
            }
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
        let own = self.opening_own.take();
        let rpg = std::mem::take(&mut self.opening_rpg);
        // the palette kept from before shows them as the input is now
        pad_view::show_key_words(&panel, self.pad);
        self.open = Some(Open {
            req,
            prompt: original.clone(),
            kind,
            own,
            rpg,
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

    /// The open dialog is one of the RPG panels (self-tests).
    pub fn rpg_open(&self) -> bool {
        self.open.as_ref().is_some_and(|o| o.rpg)
    }

    /// True when a LineEdit owns the keyboard (Text, ExtCmd).
    pub fn wants_text(&self) -> bool {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Text { .. } | Kind::Picker { .. }) => true,
            // the command menu's search only once typed into: before, its
            // rows take the arrows, Enter and ← →
            Some(Kind::ExtCmd { edit, .. }) => edit.has_focus(),
            _ => false,
        }
    }

    /// A letter typed with the command menu open, its search not yet
    /// typed into: the search takes the keyboard (under a pad, shown
    /// first), and the letter goes to it. False when no menu waits so.
    pub fn palette_starts_typing(&mut self) -> bool {
        let Some(Kind::ExtCmd { edit, .. }) = self.open.as_ref().map(|o| &o.kind) else {
            return false;
        };
        if edit.has_focus() {
            return false;
        }
        if self.pad {
            self.set_pad(false);
        }
        let Some(Kind::ExtCmd { edit, .. }) = self.open.as_mut().map(|o| &mut o.kind) else {
            return false;
        };
        // focused from the code a field only waits: editing, it takes keys
        edit.grab_focus();
        edit.edit();
        true
    }

    /// The command menu `#` opens next hangs from the HUD's Commands
    /// button at `button` (pressed now).
    pub fn set_palette_anchor(&mut self, button: Option<Rect2>) {
        self.palette_anchor = button.map(|r| (r, std::time::Instant::now()));
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
            Kind::Wheel(_) => "wheel",
            Kind::CellMenu(_) => "cellmenu",
            Kind::Loot(_) => "loot",
        })
    }

    /// The open dialog's panel: its size on screen (self-tests).
    pub fn panel_size(&self) -> Option<Vector2> {
        self.open.as_ref().map(|o| o.panel.get_size())
    }

    /// The engine's window the open dialog lays out from its English:
    /// "vanquished", "genocided", "overview".
    pub fn layout_name(&self) -> Option<&'static str> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Show { layout, .. }) => *layout,
            _ => None,
        }
    }

    /// The open text window reads in the body face, as prose (self-tests).
    pub fn show_prose(&self) -> Option<bool> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Show { text, layout: None }) => text
                .scroll
                .get_children()
                .iter_shared()
                .find_map(|c| c.try_cast::<RichTextLabel>().ok())
                .map(|l| l.has_theme_font_override("normal_font")),
            _ => None,
        }
    }

    /// The textures the open dialog shows (self-tests: the icons).
    pub fn panel_textures(&self) -> Vec<Gd<Texture2D>> {
        let Some(o) = self.open.as_ref() else {
            return Vec::new();
        };
        o.panel
            .find_children_ex("*")
            .type_("TextureRect")
            .owned(false)
            .done()
            .iter_shared()
            .filter_map(|n| n.try_cast::<TextureRect>().ok()?.get_texture())
            .collect()
    }

    /// Every label's text in the open dialog, in order (self-tests).
    pub fn panel_texts(&self) -> Vec<String> {
        fn walk(node: &Gd<Node>, out: &mut Vec<String>) {
            if let Ok(l) = node.clone().try_cast::<Label>() {
                out.push(l.get_text().to_string());
            }
            for c in node.get_children().iter_shared() {
                walk(&c, out);
            }
        }
        let mut out = Vec::new();
        if let Some(o) = self.open.as_ref() {
            walk(&o.panel.clone().upcast(), &mut out);
        }
        out
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
        self.sheet = None;
        self.spells = None;
        self.journal = None;
        self.codex = None;
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

    /// The keys the player moved, for the palette's keys and a panel's
    /// own.
    pub fn set_keymap(&mut self, keymap: &nh_world::Keymap) {
        if self.keymap != *keymap {
            self.keymap = keymap.clone();
        }
    }

    /// Whether a gamepad gives the input now: the open dialog's keyboard
    /// words hidden or shown again, its answers named without their keys
    /// or with them.
    pub fn set_pad(&mut self, on: bool) {
        if self.pad == on {
            return;
        }
        self.pad = on;
        let Some(open) = self.open.as_mut() else {
            return;
        };
        pad_view::show_key_words(&open.panel, on);
        if let Kind::Choice { buttons, named, .. } = &mut open.kind {
            for (c, b) in buttons.iter_mut() {
                let word = named.iter().find(|(n, _)| n == c).map(|(_, w)| w.as_str());
                b.set_text(&named_choice(*c, on, word));
            }
        }
        if let Kind::CellMenu(m) = &mut open.kind
            && on
            && m.focus().is_none()
        {
            let look = cell_menu_look_here(&open.prompt);
            m.focus_item(look);
        }
        if let Kind::ExtCmd { edit, palette, .. } = &open.kind {
            palette.borrow_mut().pad = on;
            palette.borrow_mut().refilter(&edit.get_text().to_string());
            fill_palette(palette);
        }
    }

    /// The hero carries goods not paid for (the palette lists #pay first).
    pub fn set_owes(&mut self, owes: bool) {
        self.owes = owes;
    }

    /// The hero's Pw now, for the spells' cards.
    pub fn set_energy(&mut self, pw: Option<i64>) {
        self.energy = pw;
    }

    /// The action bar's slots on screen, where a spell's card may drop.
    pub fn set_bar_slots(&mut self, rects: &[Rect2]) {
        if *self.bar_slots.borrow() != rects {
            *self.bar_slots.borrow_mut() = rects.to_vec();
        }
    }

    /// The hero rendered in their gear, for the character sheet's
    /// portrait.
    pub fn set_portrait(&mut self, render: Gd<Texture2D>) {
        self.portrait = Some(render);
    }

    /// Whether the open sheet's portrait shows the hero (self-tests).
    pub fn portrait_shown(&self) -> bool {
        self.sheet.as_ref().is_some_and(SheetView::rendered)
    }

    /// The portrait's render shows the hero now (or not yet).
    pub fn set_portrait_rendered(&mut self, rendered: bool) {
        if let Some(v) = self.sheet.as_mut() {
            v.set_rendered(rendered);
        }
    }

    /// Every word of the open dialog as shown (self-tests: no keyboard's
    /// words while a gamepad plays).
    pub fn shown_words(&self) -> Vec<String> {
        self.open
            .as_ref()
            .map(|o| pad_view::shown_words(&o.panel))
            .unwrap_or_default()
    }

    /// The open menu has more than a page to scroll through.
    pub fn menu_paged(&self) -> bool {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => view.max_top > 0.5,
            _ => false,
        }
    }

    /// The open menu picks something (not one only to read).
    pub fn menu_picks(&self) -> bool {
        matches!(self.open.as_ref().map(|o| &o.kind), Some(Kind::Menu(v)) if v.state.how != PickHow::None)
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
            OskOp::Move(dx, dy) => osk.step(dx, dy),
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

    /// A gamepad has somewhere to be in the open dialog: an answer of a
    /// question, a menu's row when it picks (self-tests). A dialog with
    /// nothing to choose (a text to read, a field) needs none.
    pub fn pad_focused(&self) -> bool {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Choice { buttons, focus, .. }) => buttons.is_empty() || focus.is_some(),
            Some(Kind::Menu(view)) => {
                view.state.how == PickHow::None
                    || view.state.entries.iter().all(|e| !e.selectable)
                    || view.cursor.is_some()
            }
            Some(Kind::CellMenu(m)) => m.focus().is_some() || m.rows().is_empty(),
            _ => true,
        }
    }

    /// The answer the open question's focus is on (self-tests).
    pub fn choice_focus(&self) -> Option<char> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Choice { buttons, focus, .. }) => {
                focus.and_then(|f| buttons.get(f)).map(|(c, _)| *c)
            }
            _ => None,
        }
    }

    /// The open menu's keyboard row (an index into `menu_entries`).
    pub fn menu_cursor(&self) -> Option<usize> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu(view)) => view.cursor,
            Some(Kind::CellMenu(m)) => m.focus(),
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
            Some(Kind::Show { text, .. }) => Some(text.top()),
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

    /// The open palette's rows as they read, a folded section's too: the
    /// name and the words of each command listed (self-tests).
    pub fn palette_rows(&self) -> Option<Vec<(String, String)>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::ExtCmd { palette, .. }) => {
                let p = palette.borrow();
                let filled = (p.slots.iter().zip(&p.slot_cmd)).filter(|(_, c)| c.is_some());
                let text = |l: &Gd<Label>| l.get_text().to_string();
                Some(
                    filled
                        .map(|(s, _)| (text(&s.name), text(&s.desc)))
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// The open palette's list on screen and the height it is made to
    /// show (self-tests).
    pub fn palette_view(&self) -> Option<(Rect2, f32)> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::ExtCmd { palette, .. }) => {
                let p = palette.borrow();
                Some((p.list.scroll.get_global_rect(), p.list.view_h))
            }
            _ => None,
        }
    }

    /// The commands the open palette lists, top down, a folded section's
    /// too (self-tests).
    pub fn palette_names(&self) -> Option<Vec<String>> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::ExtCmd { palette, .. }) => Some(listed_names(&palette.borrow())),
            _ => None,
        }
    }

    /// The open palette's sections as shown: each heading's words,
    /// whether it is folded, its commands; none while typed text filters
    /// (self-tests).
    pub fn palette_sections(&self) -> Option<Vec<(String, bool, Vec<String>)>> {
        let Some(Kind::ExtCmd { palette, .. }) = self.open.as_ref().map(|o| &o.kind) else {
            return None;
        };
        let p = palette.borrow();
        let heads = p.lines.iter().filter_map(|l| match l {
            PaletteLine::Head(g) => p.groups.get(*g),
            PaletteLine::Slot(_) => None,
        });
        Some(
            heads
                .map(|g| {
                    let names = g.cmds.iter().map(|&c| p.cmds[c].name.clone()).collect();
                    (g.head.title.get_text().to_string(), g.folded, names)
                })
                .collect(),
        )
    }

    /// The open palette's highlighted row: "#name" for a command, the
    /// heading's words for a section (self-tests).
    pub fn palette_highlight(&self) -> Option<String> {
        let Some(Kind::ExtCmd { palette, .. }) = self.open.as_ref().map(|o| &o.kind) else {
            return None;
        };
        let p = palette.borrow();
        match p.picked()? {
            PalettePick::Cmd(name) => Some(format!("#{name}")),
            PalettePick::Head(g) => Some(p.groups[g].head.title.get_text().to_string()),
        }
    }

    /// Where the open palette shows command `name`'s row, or section
    /// `name`'s heading (its words), on screen, if it does (self-tests:
    /// a click there).
    pub fn palette_row_rect(&self, name: &str) -> Option<Rect2> {
        let Some(Kind::ExtCmd { palette, .. }) = self.open.as_ref().map(|o| &o.kind) else {
            return None;
        };
        let p = palette.borrow();
        let button = p.lines.iter().find_map(|l| match *l {
            PaletteLine::Head(g) => {
                (p.groups[g].head.title.get_text() == name).then(|| p.groups[g].head.button.clone())
            }
            PaletteLine::Slot(s) => {
                let c = p.slot_cmd[s]?;
                (p.cmds[c].name == name).then(|| p.slots[s].button.clone())
            }
        })?;
        button
            .is_visible_in_tree()
            .then(|| button.get_global_rect())
    }

    /// How far the open palette's list is scrolled (self-tests).
    pub fn palette_top(&self) -> Option<f32> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::ExtCmd { palette, .. }) => Some(palette.borrow().list.top()),
            _ => None,
        }
    }

    /// The open palette hangs from the HUD's Commands button (self-tests).
    pub fn palette_dropdown(&self) -> bool {
        matches!(self.open.as_ref().map(|o| &o.kind),
            Some(Kind::ExtCmd { palette, .. }) if palette.borrow().dropdown)
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

    /// Type `text` into the open palette's field, as a player would.
    pub fn type_palette(&mut self, text: &str) -> bool {
        let Some(Kind::ExtCmd { edit, palette, .. }) = self.open.as_mut().map(|o| &mut o.kind)
        else {
            return false;
        };
        edit.set_text(text);
        palette.borrow_mut().refilter(text);
        fill_palette(palette);
        true
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

    /// A key for the open dialog: the reply it gives the engine, if any.
    pub fn key(&mut self, input: &KeyInput) -> Option<Reply> {
        // the spells' panel: Tab reorders, or casts again
        if self.spells.is_some()
            && input.key == Key::Tab
            && !input.echo
            && !input.mods.ctrl
            && !input.mods.alt
        {
            self.toggle_reorder();
            return None;
        }
        // the codex: Tab (a gamepad's bumper) to the other tab
        if self.codex.is_some() && input.key == Key::Tab && !input.echo {
            return self.switch_codex();
        }
        let reply = self.key_reply(input);
        self.spells_focus();
        Some(self.spells_reply(reply?))
    }

    fn key_reply(&mut self, input: &KeyInput) -> Option<Reply> {
        let open = self.open.as_mut()?;
        if let Kind::Wheel(w) = &mut open.kind {
            let act = w.key(input.key, input.mods.shift, input.echo);
            return self.wheel_act(act);
        }
        if let Kind::CellMenu(m) = &mut open.kind {
            return m.key(input);
        }
        if let Kind::Loot(l) = &mut open.kind {
            match input.key {
                Key::Escape if !input.echo => return self.loot_end(false),
                Key::Enter | Key::KeypadEnter if !input.echo => return self.loot_end(true),
                _ => {
                    l.key(input);
                    return None;
                }
            }
        }
        let escape = open.prompt.escape_reply();
        // an RPG panel's own command, as the player presses it now
        if open.own.is_some() && !input.echo && self.keymap.translate(*input) == open.own {
            return Some(escape);
        }
        match &mut open.kind {
            Kind::Menu(view) => view.key(input, &self.look),
            Kind::Choice {
                allowed,
                default,
                buttons,
                focus,
                ..
            } => {
                // the arrows (a gamepad) move between the answers
                let step = match nav(input) {
                    Some(Key::Left | Key::Up) => -1,
                    Some(Key::Right | Key::Down) => 1,
                    _ => 0,
                };
                if step != 0 && !buttons.is_empty() {
                    let n = buttons.len() as i32;
                    let first = default.and_then(|d| buttons.iter().position(|(c, _)| *c == d));
                    *focus = Some(match (*focus, first) {
                        // the first press shows the default answer (what
                        // Enter takes), as a gamepad starts on it
                        (None, Some(d)) => d,
                        (None, None) => usize::from(step < 0) * (n as usize - 1),
                        (Some(f), _) => (f as i32 + step).rem_euclid(n) as usize,
                    });
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
                    // Enter (a gamepad's A): the highlighted command runs, a
                    // heading folds or unfolds its section
                    Key::Enter | Key::KeypadEnter if !input.echo => {
                        let picked = palette.borrow().picked();
                        match picked {
                            Some(PalettePick::Cmd(name)) => return Some(Reply::ExtCmd(Some(name))),
                            Some(PalettePick::Head(g)) => {
                                palette.borrow_mut().fold(g, None);
                                fill_palette(palette);
                            }
                            None => {}
                        }
                        return None;
                    }
                    // ← folds the highlighted row's section, → unfolds it
                    // (the field's caret takes them while typing)
                    Key::Left | Key::Right if !edit.has_focus() => {
                        let changed = {
                            let mut p = palette.borrow_mut();
                            let fold = input.key == Key::Left;
                            p.selected_group().is_some_and(|g| p.fold(g, Some(fold)))
                        };
                        if changed {
                            fill_palette(palette);
                        }
                        return None;
                    }
                    Key::Up => -1,
                    Key::Down => 1,
                    Key::PageUp => -10,
                    Key::PageDown => 10,
                    Key::Tab => {
                        let text = edit.get_text().to_string();
                        let done = palette_complete(&palette.borrow().cmds, &text);
                        if done.is_empty() {
                            return None;
                        }
                        edit.set_text(&done);
                        edit.set_caret_column(done.chars().count() as i32);
                        palette.borrow_mut().refilter(&done);
                        fill_palette(palette);
                        return None;
                    }
                    _ => return None,
                };
                palette.borrow_mut().step(step);
                fill_palette(palette);
                None
            }
            Kind::Show { text, .. } => {
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
            // answered above
            Kind::Wheel(_) | Kind::CellMenu(_) | Kind::Loot(_) => None,
        }
    }

    /// A widget's event for the open dialog: the reply it gives the
    /// engine, if any.
    pub fn dialog_event(&mut self, req: u64, ev: &DialogEvent) -> Option<Reply> {
        if matches!(ev, DialogEvent::Reorder) {
            if self.open_req() == Some(req) {
                self.toggle_reorder();
            }
            return None;
        }
        if matches!(ev, DialogEvent::SwitchTab) {
            if self.open_req() != Some(req) {
                return None;
            }
            return self.switch_codex();
        }
        let reply = self.event_reply(req, ev)?;
        Some(self.spells_reply(reply))
    }

    fn event_reply(&mut self, req: u64, ev: &DialogEvent) -> Option<Reply> {
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
                own: None,
                rpg: false,
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
        if let Kind::Loot(l) = &mut open.kind {
            match ev {
                DialogEvent::LootToggle { column, row } => l.toggle(*column, *row),
                DialogEvent::LootCount { column, row, delta } => l.count(*column, *row, *delta),
                DialogEvent::MenuConfirm => return self.loot_end(true),
                DialogEvent::MenuCancel => return self.loot_end(false),
                _ => {}
            }
            return None;
        }
        if let Kind::CellMenu(m) = &mut open.kind {
            return match ev {
                DialogEvent::MenuClick(i) => m.pick(*i),
                DialogEvent::MenuCancel => Some(Reply::Cancel),
                _ => None,
            };
        }
        if let Kind::Wheel(w) = &mut open.kind {
            let act = match ev {
                DialogEvent::WheelMove(at) => {
                    w.point(*at);
                    WheelAct::Stay
                }
                // a click picks what it is on (in the middle, a cancel)
                DialogEvent::WheelButton {
                    button: 1,
                    pressed: true,
                    at,
                } => {
                    w.point(*at);
                    w.pick()
                }
                DialogEvent::WheelButton {
                    button: 2,
                    pressed: true,
                    ..
                } => WheelAct::Cancel,
                DialogEvent::ExtCmd(None) => WheelAct::Cancel,
                _ => WheelAct::Stay,
            };
            return self.wheel_act(act);
        }
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
            // typed in English (a veteran's wish): to the engine as typed
            (Kind::Picker { .. }, DialogEvent::TextSubmitted(text))
                if pickers::typed_in_english(text) =>
            {
                Some(Reply::Text(truncate_bytes(text.trim(), MAX_TEXT_BYTES)))
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
                // the field's own Enter: the row highlighted for what it
                // holds; a name given otherwise (a self-test's), that one
                let given = text.trim().to_lowercase();
                let typed = edit.get_text().to_string().trim().to_lowercase() == given;
                let named = (!typed)
                    .then(|| {
                        palette
                            .borrow()
                            .cmds
                            .iter()
                            .find(|c| c.name == given)
                            .map(|c| c.name.clone())
                    })
                    .flatten();
                if let Some(name) = named {
                    return Some(Reply::ExtCmd(Some(name)));
                }
                let picked = palette.borrow().picked();
                let chosen = match picked {
                    Some(PalettePick::Cmd(name)) => Some(name),
                    // a heading: its section folded or unfolded, the field
                    // kept
                    Some(PalettePick::Head(g)) => {
                        palette.borrow_mut().fold(g, None);
                        fill_palette(palette);
                        edit.call_deferred("grab_focus", &[]);
                        return None;
                    }
                    None => {
                        let t = text.trim().to_lowercase();
                        let p = palette.borrow();
                        p.cmds.iter().find(|c| c.name == t).map(|c| c.name.clone())
                    }
                };
                match chosen {
                    Some(name) => Some(Reply::ExtCmd(Some(name))),
                    None if text.trim().is_empty() => Some(Reply::ExtCmd(None)),
                    None => {
                        edit.call_deferred("grab_focus", &[]);
                        None
                    }
                }
            }
            (Kind::ExtCmd { palette, .. }, DialogEvent::MenuClick(i)) => {
                let p = palette.borrow();
                let c = p.slot_cmd.get(*i).copied().flatten()?;
                Some(Reply::ExtCmd(Some(p.cmds[c].name.clone())))
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
        // what the open panel kept of itself goes with it (not with a
        // dialog of the warm-up's put away)
        self.sheet = None;
        self.spells = None;
        self.journal = None;
        self.codex = None;
        if let Some(open) = self.open.take() {
            self.retire(open);
        }
    }

    /// A dialog closed: freed, the palette hidden for the next time.
    fn retire(&mut self, mut open: Open) {
        // the palette is hidden for the next time
        if let Kind::ExtCmd { edit, palette, req } = open.kind {
            // without the room its opening gave a gamepad's hints: the next
            // one adds its own, and a room kept each time made the panel a
            // gap taller with every opening
            if let Some(mut room) = open.pad_room.take() {
                if let Some(mut col) = room.get_parent() {
                    col.remove_child(&room);
                }
                room.queue_free();
            }
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
        // the character sheet, its faces and sizes
        Prompt::Show {
            title: None,
            lines: sheet::WIZARD.iter().map(|l| line(0, l)).collect(),
        },
        // the spells' panel, its medallions
        Prompt::Menu {
            win: 0,
            how: PickHow::One,
            title: Some("Currently known spells".into()),
            items: vec![
                item(
                    0,
                    '\0',
                    "    Name                 Level Category     Fail Retention",
                    7,
                    false,
                ),
                item(
                    1,
                    'a',
                    "force bolt             1   attack         0%      100%",
                    0,
                    true,
                ),
                item(
                    2,
                    'b',
                    "healing                1   healing       12%  76%-100%",
                    0,
                    true,
                ),
                item(3, '+', "[sort spells]", 0, true),
            ],
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

/// A text window as the player reads it: the translator has it whole (a
/// translation may run to another number of lines: those take the first
/// line's attribute).
fn shown_window(lines: &[TextLine]) -> Vec<TextLine> {
    let english: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    let Some(shown) = i18n::engine_window(&english) else {
        return lines.to_vec();
    };
    let shown = tables_kept(&english, shown);
    let first = lines.first().map_or(0, |l| l.attr);
    shown
        .into_iter()
        .enumerate()
        .map(|(i, text)| TextLine {
            attr: lines.get(i).map_or(first, |l| l.attr),
            text,
        })
        .collect()
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
                    str: i
                        .str
                        .as_deref()
                        .map(|t| row_kept(t, e(EngineKind::Menu, t))),
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
            lines: shown_window(lines),
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

/// The key of the catalog's extended command `name` (#enhance's M-e), if
/// it has one.
fn ext_key(catalog: Option<&Catalog>, name: &str) -> Option<KeyInput> {
    let cmd = catalog?.extcmds.iter().find(|e| e.name == name)?;
    (cmd.key != 0)
        .then(|| nh_world::key_of_code(cmd.key))
        .flatten()
}

/// What the spells' panel's keys do: a pick casts, or (`reorder`) begins
/// a swap; `key` closes it.
fn spells_hint(purpose: Purpose, reorder: bool, key: &str) -> String {
    match (purpose, reorder) {
        (Purpose::Swap(_), _) => tr!("spells-hint-swap"),
        (Purpose::View, true) => tr!("spells-hint-reorder", key = key),
        (Purpose::View, false) => tr!("spells-hint-view", key = key),
        (Purpose::Cast, _) => tr!("spells-hint-cast", key = key),
    }
}

/// A text window's lines are prose: none laid out in columns (a run of
/// spaces inside it that ends no sentence: NetHack's two spaces after one
/// are none), nor a rule or a bar of a picture.
fn is_prose(lines: &[TextLine]) -> bool {
    !lines.iter().any(|l| {
        let t = l.text.trim();
        table_cells(t).len() > 1 || t.contains('|') || t.contains("--")
    })
}

/// Prose's two spaces after a sentence, one: the body face is no
/// typewriter.
fn one_space_after_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("  ") {
        let (before, after) = rest.split_at(i);
        out.push_str(before);
        let gap = after.len() - after.trim_start_matches(' ').len();
        let next = &after[gap..];
        if gap == 2 && ends_sentence(&out) && !next.is_empty() {
            out.push(' ');
        } else {
            out.push_str(&after[..gap]);
        }
        rest = next;
    }
    out.push_str(rest);
    out
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
                let l = TextLine {
                    attr: l.attr,
                    text: one_space_after_sentences(&l.text),
                };
                show_text(std::slice::from_ref(&l))
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
pub(crate) enum OskKey {
    Char(String),
    Space,
    Back,
    Shift,
    Layout,
    Ok,
}

/// The meta that marks the on-screen keyboard's keys (self-tests: their
/// words name no key to press).
pub(crate) const OSK: &str = "on_screen_keyboard";

/// The keyboard's rows: digits and letters by layout, then the controls.
const OSK_LATIN: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm,.-"];
const OSK_CYRILLIC: [&str; 4] = ["1234567890", "йцукенгшщзхъ", "фывапролджэ", "ячсмитьбю.ё"];

/// The on-screen keyboard of a text question or the hero's name, driven
/// by a gamepad (or the mouse): the d-pad moves, A types, Y switches Latin
/// and Cyrillic. What the buttons do is the owner's to say (the pad's
/// strip under a question).
pub(crate) struct Osk {
    rows: Vec<Vec<(OskKey, Gd<Button>)>>,
    row: usize,
    col: usize,
    cyrillic: bool,
    upper: bool,
    focus: Gd<StyleBoxFlat>,
}

impl Osk {
    /// The keyboard at the end of `col`; a key clicked calls `clicked`
    /// with its row and column (the owner types it as A would).
    pub(crate) fn new(
        col: &mut Gd<VBoxContainer>,
        clicked: impl Fn(usize, usize) + Clone + 'static,
    ) -> Osk {
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
                (0..12)
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
                // a click types the key as A does (the owner knows which)
                let clicked = clicked.clone();
                let (rr, cc) = (r, c);
                b.signals().pressed().connect(move || clicked(rr, cc));
                line.add_child(&b);
                keys.push((k, b));
            }
            grid.add_child(&line);
            rows.push(keys);
        }
        // a keyboard's keys by nature (Space, Shift), not a key to press
        grid.set_meta(OSK, &true.to_variant());
        col.add_child(&grid);
        let mut osk = Osk {
            rows,
            row: 1,
            col: 0,
            cyrillic: false,
            upper: false,
            focus: theme::focus_style(),
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

    pub(crate) fn step(&mut self, dx: i32, dy: i32) {
        let rows = self.rows.len() as i32;
        self.row = (self.row as i32 + dy).rem_euclid(rows) as usize;
        let w = self.width(self.row) as i32;
        self.col = (self.col as i32 + dx).clamp(0, w - 1) as usize;
        self.show();
    }

    pub(crate) fn show(&mut self) {
        for (r, line) in self.rows.iter_mut().enumerate() {
            for (c, (_, b)) in line.iter_mut().enumerate() {
                if (r, c) == (self.row, self.col) {
                    b.add_theme_stylebox_override("normal", &self.focus);
                } else {
                    b.remove_theme_stylebox_override("normal");
                }
            }
        }
    }

    pub(crate) fn current(&self) -> OskKey {
        self.rows[self.row][self.col].0.clone()
    }

    pub(crate) fn at(&mut self, row: usize, col: usize) {
        if row < self.rows.len() && col < self.width(row) {
            self.row = row;
            self.col = col;
        }
    }

    pub(crate) fn switch(&mut self) {
        self.cyrillic = !self.cyrillic;
        self.relabel();
        self.col = self.col.min(self.width(self.row) - 1);
    }

    pub(crate) fn shift(&mut self) {
        self.upper = !self.upper;
        self.relabel();
    }
}

/// An empty row at the end of a dialog's column: room a gamepad's hints
/// take inside the panel (`Dialogs::set_pad_room`).
/// Where "Look at what is here" is among a cell menu's items.
fn cell_menu_look_here(prompt: &Prompt) -> Option<usize> {
    match prompt {
        Prompt::Menu { items, .. } => items
            .iter()
            .position(|i| i.str.as_deref() == Some(cell_menu::LOOK_HERE)),
        _ => None,
    }
}

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
                key: String::new(),
                also: Vec::new(),
                prefix: false,
            })
            .collect()
    }

    #[test]
    fn russian_words_and_descriptions_find_commands() {
        let mut c = cmds(&["pray", "offer", "eat", "genocided"]);
        c[0].desc = "помолиться богам о помощи".into();
        c[0].also = vec!["молитва".into(), "молиться".into()];
        c[1].desc = "принести жертву богам".into();
        c[1].also = vec!["жертва".into(), "алтарь".into()];
        c[2].desc = "съесть что-нибудь".into();
        c[3].desc = "список истреблённых и вымерших монстров".into();
        // a name in the player's language, a word's start
        assert_eq!(names(&c, &palette_filter(&c, "моли")), ["pray"]);
        assert_eq!(names(&c, &palette_filter(&c, "Жертв")), ["offer"]);
        // the description's words: "богам" is in both, shortest name first
        assert_eq!(names(&c, &palette_filter(&c, "бог")), ["pray", "offer"]);
        assert_eq!(names(&c, &palette_filter(&c, "съе")), ["eat"]);
        // ё typed as е
        assert_eq!(names(&c, &palette_filter(&c, "истребленн")), ["genocided"]);
        // a word's middle is no match: "ощи" ends "помощи"
        assert!(palette_filter(&c, "ощи").is_empty());
        // the English name still leads
        assert_eq!(names(&c, &palette_filter(&c, "pr")), ["pray"]);
    }

    #[test]
    fn a_word_s_start_goes_before_a_word_s_middle() {
        let mut c = cmds(&["quaff", "eat", "dip"]);
        c[0].also = vec!["выпить".into()];
        c[1].also = vec!["пища".into(), "питаться".into()];
        c[2].also = vec!["окунуть".into()];
        // "пит", looking for food: #eat's word starts so, quaff's ends so
        assert_eq!(names(&c, &palette_filter(&c, "пит")), ["eat", "quaff"]);
    }

    #[test]
    fn giving_up_finds_quit() {
        let mut c = cmds(&["pray", "quit", "quiver"]);
        c[1].also = vec!["give up".to_string(), "сдаться".to_string()];
        assert_eq!(names(&c, &palette_filter(&c, "give")), ["quit"]);
        assert_eq!(names(&c, &palette_filter(&c, "Сдат")), ["quit"]);
        assert_eq!(names(&c, &palette_filter(&c, "qu")), ["quit", "quiver"]);
        assert!(quits("Really quit without saving?"));
        assert!(quits("Really quit without saving? [yes|n] (n)"));
        assert!(quits(
            "\"Yes\" or \"No\": Really quit without saving? [yes|n] (n)"
        ));
        assert!(!quits("Really save?"));
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
        assert!(palette_cmds(None, &nh_world::Keymap::default()).is_empty());
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
        let none = nh_world::Keymap::default();
        assert_eq!(crate::rebind::bound_code(&none, 0), "");
        assert_eq!(crate::rebind::bound_code(&none, 4), "^D");
        // Alt as the player presses it, not NetHack's "M-p"
        assert!(!crate::rebind::bound_code(&none, 0x80 | 'p' as i32).starts_with("M-"));
        assert_eq!(crate::rebind::bound_code(&none, '#' as i32), "#");
    }

    #[test]
    fn an_answer_the_question_names_is_its_word() {
        // (the tests' language: English)
        let q = "Which ring-finger, Right or Left?";
        assert_eq!(answer_word('r', q, q).as_deref(), Some("Right"));
        assert_eq!(answer_word('l', q, q).as_deref(), Some("Left"));
        // the first word is the question's own, not an answer
        assert_eq!(answer_word('w', q, q), None);
        assert_eq!(answer_word('x', q, q), None);
        assert_eq!(named_choice('r', false, Some("Right")), "Right (r)");
        assert_eq!(named_choice('r', true, Some("Right")), "Right");
        // in Russian, the shown question's word for it
        i18n::set_lang(Lang::Ru);
        let shown = "На какую руку надеть кольцо: правую или левую?";
        let r = answer_word('r', q, shown);
        let l = answer_word('l', q, shown);
        i18n::set_lang(Lang::En);
        assert_eq!(r.as_deref(), Some("Правую"));
        assert_eq!(l.as_deref(), Some("Левую"));
    }

    #[test]
    fn choice_labels_spell_out_the_usual_answers() {
        assert_eq!(named_choice('y', false, None), "Yes (y)");
        assert_eq!(named_choice('n', false, None), "No (n)");
        assert_eq!(named_choice('q', false, None), "Cancel (q)");
        assert_eq!(named_choice('a', false, None), "All (a)");
        assert_eq!(named_choice('m', false, None), "m");
        assert_eq!(named_choice('?', false, None), "?");
        // a gamepad has no letters to press
        assert_eq!(named_choice('y', true, None), "Yes");
        assert_eq!(named_choice('n', true, None), "No");
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
    fn prose_with_two_spaces_after_its_sentences_is_no_table() {
        let line = |t: &str| TextLine {
            attr: 0,
            text: t.into(),
        };
        // a quest's words in English and in Russian read alike
        let english = [
            line("You receive a faint telepathic message from Norn:"),
            line("Your help is urgently needed at the Shrine of Destiny!  Look for a"),
            line("...ad aerarium.\"  The rest is \"Hurry!\"  Go."),
        ];
        assert!(is_prose(&english));
        assert!(is_prose(&[line(
            "Ты срочно нужна в Святилище Судьбы! Ищи…"
        )]));
        // a table, a picture, a gap after no sentence: monospace
        assert!(!is_prose(&[line(
            "protection             1   clerical       0%      100%"
        )]));
        assert!(!is_prose(&[line(
            "Status of the hero:  Level 3  HP 12(14)"
        )]));
        assert!(!is_prose(&[line("    |....|"), line("    ------")]));
        assert!(!is_prose(&[line("Strength!   18")]), "three spaces");
        // shown with one space after each sentence
        assert_eq!(
            one_space_after_sentences("needed!  Look for it.\"  Go:  now  here"),
            "needed! Look for it.\" Go: now  here"
        );
        assert_eq!(one_space_after_sentences("End.  "), "End.  ");
    }

    #[test]
    fn a_table_keeps_its_columns_in_another_language() {
        // cells parted by runs of spaces; prose's two spaces after a
        // sentence part nothing
        assert_eq!(
            table_cells("protection             1   clerical       0%      100%"),
            ["protection", "1", "clerical", "0%", "100%"]
        );
        assert_eq!(
            table_cells("Be careful!  New moon tonight."),
            ["Be careful!  New moon tonight."]
        );
        // a translation that runs a row's cells together: the English row
        let row = "protection             1   clerical       0%      100%";
        assert_eq!(row_kept(row, "защита 1 жречество 0% 100%".into()), row);
        assert_eq!(
            row_kept("Осторожно!  Сегодня новолуние.", "x".into()),
            "x",
            "prose is no table"
        );
        assert_eq!(
            row_kept("fruit   slime mold", "fruit   слизевой гриб".into()),
            "fruit   слизевой гриб"
        );
        // a window's block of rows laid out again in its columns
        let english = [
            "Name          Points",
            "force bolt    10",
            "sleep         200",
            "",
            "Prose stays.",
        ];
        let shown = vec![
            "Имя   Очки".to_string(),
            "силовой удар   10".to_string(),
            "сон   200".to_string(),
            String::new(),
            "Проза остаётся.".to_string(),
        ];
        assert_eq!(
            tables_kept(&english, shown),
            [
                "Имя           Очки",
                "силовой удар  10",
                "сон           200",
                "",
                "Проза остаётся."
            ]
        );
        // one row run together: the block keeps the English
        let shown = vec![
            "Имя   Очки".to_string(),
            "силовой удар 10".to_string(),
            "сон   200".to_string(),
            String::new(),
            "Проза остаётся.".to_string(),
        ];
        let kept = tables_kept(&english, shown);
        assert_eq!(&kept[..3], &english[..3]);
        assert_eq!(kept[4], "Проза остаётся.");
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
