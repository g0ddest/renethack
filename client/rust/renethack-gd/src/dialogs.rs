//! Modal dialogs for menus, questions, text input, the extended-command
//! palette and text windows. At most one is open, for one request; every
//! event it queues carries that request's id.

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::control::{FocusMode, GrowDirection, MouseFilter};
use godot::classes::item_list::SelectMode;
use godot::classes::{
    CanvasLayer, ColorRect, Control, HBoxContainer, ItemList, Label, LineEdit, PanelContainer,
    RichTextLabel, VBoxContainer,
};
use godot::prelude::*;
use nh_protocol::{Catalog, ESC, PickHow, Reply};
use nh_world::{Key, KeyInput, MenuEntry, MenuOutcome, MenuState, Prompt, TextLine};

use crate::theme::{self, bbcode_escape, hex, nh_color, place};
use crate::ui_events::{DialogEvent, UiEvent, UiQueue, push};

/// NetHack's BUFSZ less the NUL.
const MAX_TEXT_BYTES: usize = 255;
const ESC_CHAR: char = '\u{1b}';

/// One extended command as the palette lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteCmd {
    pub name: String,
    pub desc: String,
    pub key: i32,
}

/// Commands matching `text`: prefix matches first, then other substring
/// matches, each shortest first (then by name).
pub fn palette_filter(cmds: &[PaletteCmd], text: &str) -> Vec<usize> {
    let text = text.trim().to_lowercase();
    let mut prefix: Vec<usize> = Vec::new();
    let mut inner: Vec<usize> = Vec::new();
    for (i, c) in cmds.iter().enumerate() {
        if c.name.starts_with(&text) {
            prefix.push(i);
        } else if c.name.contains(&text) {
            inner.push(i);
        }
    }
    let by_len = |v: &mut Vec<usize>| {
        v.sort_by(|&a, &b| {
            let (a, b) = (&cmds[a].name, &cmds[b].name);
            a.len().cmp(&b.len()).then(a.cmp(b))
        })
    };
    if !text.is_empty() {
        by_len(&mut prefix);
        by_len(&mut inner);
    }
    prefix.extend(inner);
    prefix
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

struct Palette {
    cmds: Vec<PaletteCmd>,
    shown: Vec<usize>,
    selected: Option<usize>,
}

impl Palette {
    fn refilter(&mut self, text: &str) {
        self.shown = palette_filter(&self.cmds, text);
        self.selected = (!text.trim().is_empty() && !self.shown.is_empty()).then_some(0);
    }
}

/// Show the palette's rows (no borrow is held while Godot is called).
fn fill_palette(palette: &Rc<RefCell<Palette>>, list: &mut Gd<ItemList>) {
    let (rows, selected) = {
        let p = palette.borrow();
        let rows: Vec<String> = p
            .shown
            .iter()
            .map(|&i| {
                let c = &p.cmds[i];
                let key = key_name(c.key);
                format!("{:<16} {:<5} {}", c.name, key, c.desc)
            })
            .collect();
        (rows, p.selected)
    };
    list.clear();
    for r in &rows {
        list.add_item(r);
    }
    match selected {
        Some(i) => {
            list.select(i as i32);
            list.ensure_current_is_visible();
        }
        None => list.deselect_all(),
    }
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

enum Kind {
    Menu {
        state: MenuState,
        list: Gd<ItemList>,
        count: Gd<Label>,
    },
    Choice {
        allowed: Vec<char>,
        default: Option<char>,
    },
    Text {
        edit: Gd<LineEdit>,
    },
    ExtCmd {
        edit: Gd<LineEdit>,
        list: Gd<ItemList>,
        palette: Rc<RefCell<Palette>>,
    },
    Show {
        text: Gd<RichTextLabel>,
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
}

pub struct Dialogs {
    root: Gd<Control>,
    queue: UiQueue,
    open: Option<Open>,
}

fn menu_row(e: &MenuEntry, how: PickHow) -> String {
    let mut s = String::new();
    if e.selectable {
        let mark = match (e.selected, e.count) {
            (true, Some(n)) => format!("[{n}]"),
            (true, None) => "[+]".to_string(),
            (false, _) => "[ ]".to_string(),
        };
        if how == PickHow::Any {
            s.push_str(&mark);
            s.push(' ');
        }
        match e.letter {
            Some(l) => s.push_str(&format!("{l} - ")),
            None => s.push_str("    "),
        }
    }
    if let Some(ch) = e
        .glyph
        .as_ref()
        .and_then(|g| u32::try_from(g.ch).ok())
        .and_then(char::from_u32)
        .filter(|c| !c.is_control())
    {
        s.push(ch);
        s.push(' ');
    }
    s.push_str(&e.text);
    s
}

fn fill_menu(state: &MenuState, list: &mut Gd<ItemList>, count: &mut Gd<Label>) {
    let scroll = list.get_v_scroll_bar().map(|b| b.get_value());
    list.clear();
    for (i, e) in state.entries.iter().enumerate() {
        list.add_item(&menu_row(e, state.how));
        let i = i as i32;
        if !e.selectable {
            list.set_item_selectable(i, false);
            let color = if e.text.is_empty() {
                theme::TEXT_DIM
            } else {
                theme::ACCENT
            };
            list.set_item_custom_fg_color(i, color);
        } else if e.selected {
            list.set_item_custom_fg_color(i, Color::from_rgb(1.0, 1.0, 1.0));
            list.set_item_custom_bg_color(i, Color::from_rgba(0.95, 0.78, 0.35, 0.18));
        } else if e.skipinvert {
            list.set_item_custom_fg_color(i, Color::from_rgb(0.7, 0.75, 0.9));
        } else if let Some(g) = &e.glyph {
            list.set_item_custom_fg_color(i, nh_color(g.color).lerp(theme::TEXT, 0.5));
        }
    }
    if let (Some(v), Some(mut bar)) = (scroll, list.get_v_scroll_bar()) {
        bar.set_value(v);
    }
    count.set_text(&match state.typed_count() {
        Some(n) => format!("Count: {n}"),
        None => String::new(),
    });
}

fn scroll_by(list: &mut Gd<ItemList>, pages: f64) {
    if let Some(mut bar) = list.get_v_scroll_bar() {
        let page = list.get_size().y as f64 * 0.9;
        let v = bar.get_value() + pages * page;
        bar.set_value(v);
    }
}

fn scroll_text(text: &mut Gd<RichTextLabel>, pages: f64) {
    if let Some(mut bar) = text.get_v_scroll_bar() {
        let page = text.get_size().y as f64 * 0.9;
        let v = bar.get_value() + pages * page;
        bar.set_value(v);
    }
}

fn choice_label(c: char) -> String {
    match c {
        'y' => "Yes (y)".to_string(),
        'n' => "No (n)".to_string(),
        'q' => "Cancel (q)".to_string(),
        'a' => "All (a)".to_string(),
        c => c.to_string(),
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

impl Dialogs {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Dialogs {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);
        Dialogs {
            root,
            queue,
            open: None,
        }
    }

    /// A centred panel with a column; `width` in pixels.
    fn frame(
        &mut self,
        width: f32,
        title: Option<&str>,
    ) -> (Gd<ColorRect>, Gd<PanelContainer>, Gd<VBoxContainer>) {
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.35));
        theme::full_rect_ignore(&shade);
        self.root.add_child(&shade);
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &panel,
            [0.5, 0.5, 0.5, 0.5],
            [-width / 2.0, -40.0, width / 2.0, 40.0],
        );
        panel.set_h_grow_direction(GrowDirection::BOTH);
        panel.set_v_grow_direction(GrowDirection::BOTH);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        if let Some(t) = title.filter(|t| !t.is_empty()) {
            let mut l = theme::label(t);
            l.add_theme_color_override("font_color", theme::ACCENT);
            l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
            l.set_custom_minimum_size(Vector2::new(width - 40.0, 0.0));
            col.add_child(&l);
        }
        panel.add_child(&col);
        self.root.add_child(&panel);
        (shade, panel, col)
    }

    fn buttons(&self, col: &mut Gd<VBoxContainer>, items: &[(String, UiEvent)]) {
        let mut row = HBoxContainer::new_alloc();
        row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        row.add_theme_constant_override("separation", 12);
        for (text, ev) in items {
            row.add_child(&theme::button(text, &self.queue, ev.clone()));
        }
        col.add_child(&row);
    }

    fn item_list(&self, req: u64, rows: usize, width: f32) -> Gd<ItemList> {
        let mut list = ItemList::new_alloc();
        list.set_focus_mode(FocusMode::NONE);
        list.set_select_mode(SelectMode::SINGLE);
        list.set_allow_search(false);
        let height = (rows.max(3) as f32 * 24.0 + 16.0).min(560.0);
        list.set_custom_minimum_size(Vector2::new(width - 40.0, height));
        let q = self.queue.clone();
        list.signals()
            .item_clicked()
            .connect(move |i: i64, _pos: Vector2, button: i64| {
                if button == 1 {
                    let ev = DialogEvent::MenuClick(i as usize);
                    push(&q, UiEvent::Dialog { req, ev });
                }
            });
        list
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

    /// Open the UI for Menu, Choice, Text, ExtCmd, Show, MessageMenu (other
    /// prompts: no-op).
    pub fn open(&mut self, req: u64, prompt: &Prompt, catalog: Option<&Catalog>) {
        self.close();
        let (kind, shade, panel) = match prompt {
            Prompt::Menu {
                how, title, items, ..
            } => {
                let state = MenuState::new(*how, title.clone(), items);
                let width = 760.0;
                let (shade, panel, mut col) = self.frame(width, title.as_deref());
                let mut list = self.item_list(req, state.entries.len(), width);
                let mut count = theme::label("");
                count.add_theme_color_override("font_color", theme::ACCENT);
                fill_menu(&state, &mut list, &mut count);
                col.add_child(&list);
                col.add_child(&count);
                let hint = match how {
                    PickHow::Any => "letters pick · . all · - none · Enter OK · Esc cancel",
                    _ => "a letter or a click picks · Esc cancels",
                };
                let mut hint = theme::label(hint);
                hint.add_theme_color_override("font_color", theme::TEXT_DIM);
                col.add_child(&hint);
                self.buttons(
                    &mut col,
                    &[
                        ("OK".into(), dialog_ui(req, DialogEvent::MenuConfirm)),
                        ("Cancel".into(), dialog_ui(req, DialogEvent::MenuCancel)),
                    ],
                );
                (Kind::Menu { state, list, count }, shade, panel)
            }
            Prompt::Choice {
                query,
                visible,
                allowed,
                default,
            } => {
                let (shade, panel, mut col) = self.frame(600.0, Some(query));
                let buttons: Vec<(String, UiEvent)> = visible
                    .iter()
                    .map(|&c| (choice_label(c), dialog_ui(req, DialogEvent::Choice(c))))
                    .collect();
                self.buttons(&mut col, &buttons);
                let kind = Kind::Choice {
                    allowed: allowed.clone(),
                    default: *default,
                };
                (kind, shade, panel)
            }
            Prompt::Text { query, .. } => {
                let (shade, panel, mut col) = self.frame(600.0, Some(query));
                let mut edit = self.line_edit(req);
                col.add_child(&edit);
                self.buttons(
                    &mut col,
                    &[("Cancel".into(), dialog_ui(req, DialogEvent::TextCancelled))],
                );
                edit.call_deferred("grab_focus", &[]);
                (Kind::Text { edit }, shade, panel)
            }
            Prompt::ExtCmd => {
                let cmds: Vec<PaletteCmd> = catalog
                    .map(|c| {
                        c.extcmds
                            .iter()
                            .map(|e| PaletteCmd {
                                name: e.name.clone(),
                                desc: e.desc.clone(),
                                key: e.key,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let width = 760.0;
                let (shade, panel, mut col) = self.frame(width, Some("Extended command"));
                let mut edit = self.line_edit(req);
                edit.set_placeholder("type a command; Tab completes, Up/Down choose");
                col.add_child(&edit);
                let mut list = self.item_list(req, 16, width);
                col.add_child(&list);
                self.buttons(
                    &mut col,
                    &[("Cancel".into(), dialog_ui(req, DialogEvent::ExtCmd(None)))],
                );
                let palette = Rc::new(RefCell::new(Palette {
                    shown: palette_filter(&cmds, ""),
                    cmds,
                    selected: None,
                }));
                fill_palette(&palette, &mut list);
                // refiltering is local to the palette: it never touches the game
                let (p, mut l) = (palette.clone(), list.clone());
                edit.signals().text_changed().connect(move |text: GString| {
                    p.borrow_mut().refilter(&text.to_string());
                    fill_palette(&p, &mut l);
                });
                edit.call_deferred("grab_focus", &[]);
                (
                    Kind::ExtCmd {
                        edit,
                        list,
                        palette,
                    },
                    shade,
                    panel,
                )
            }
            Prompt::Show { title, lines } => {
                let width = 820.0;
                let (shade, panel, mut col) = self.frame(width, title.as_deref());
                let mut text = RichTextLabel::new_alloc();
                text.set_use_bbcode(true);
                text.set_focus_mode(FocusMode::NONE);
                text.set_text(&show_text(lines));
                let height = (lines.len().max(2) as f32 * 21.0 + 12.0).min(620.0);
                text.set_custom_minimum_size(Vector2::new(width - 40.0, height));
                col.add_child(&text);
                self.buttons(
                    &mut col,
                    &[("OK".into(), dialog_ui(req, DialogEvent::Close))],
                );
                (Kind::Show { text }, shade, panel)
            }
            Prompt::MessageMenu { letter, mesg, pick } => {
                let (shade, panel, mut col) = self.frame(600.0, Some(mesg));
                let buttons = if *pick {
                    vec![
                        (
                            format!("{letter}"),
                            dialog_ui(req, DialogEvent::Choice(*letter)),
                        ),
                        (
                            "Cancel".to_string(),
                            dialog_ui(req, DialogEvent::Choice(ESC_CHAR)),
                        ),
                    ]
                } else {
                    vec![("OK".to_string(), dialog_ui(req, DialogEvent::Close))]
                };
                self.buttons(&mut col, &buttons);
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
        self.open = Some(Open {
            req,
            prompt: prompt.clone(),
            kind,
            shade,
            panel,
        });
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// True when a LineEdit owns the keyboard (Text, ExtCmd).
    pub fn wants_text(&self) -> bool {
        matches!(
            self.open.as_ref().map(|o| &o.kind),
            Some(Kind::Text { .. } | Kind::ExtCmd { .. })
        )
    }

    /// The request the open dialog serves.
    pub fn open_req(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.req)
    }

    /// "menu", "choice", "text", "extcmd", "show", "message".
    pub fn kind_name(&self) -> Option<&'static str> {
        self.open.as_ref().map(|o| match o.kind {
            Kind::Menu { .. } => "menu",
            Kind::Choice { .. } => "choice",
            Kind::Text { .. } => "text",
            Kind::ExtCmd { .. } => "extcmd",
            Kind::Show { .. } => "show",
            Kind::MessageMenu { .. } => "message",
        })
    }

    pub fn menu_entries(&self) -> Option<&[MenuEntry]> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Menu { state, .. }) => Some(&state.entries),
            _ => None,
        }
    }

    /// The text field's contents.
    pub fn text(&self) -> Option<String> {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Text { edit } | Kind::ExtCmd { edit, .. }) => {
                Some(edit.get_text().to_string())
            }
            _ => None,
        }
    }

    /// Does the text field have the keyboard?
    pub fn text_has_focus(&self) -> bool {
        match self.open.as_ref().map(|o| &o.kind) {
            Some(Kind::Text { edit } | Kind::ExtCmd { edit, .. }) => edit.has_focus(),
            _ => false,
        }
    }

    pub fn key(&mut self, input: &KeyInput) -> Option<Reply> {
        let open = self.open.as_mut()?;
        let escape = open.prompt.escape_reply();
        match &mut open.kind {
            Kind::Menu { state, list, count } => {
                let c = match input.key {
                    Key::PageUp => '<',
                    Key::PageDown => '>',
                    Key::Up | Key::Down if !input.mods.ctrl && !input.mods.alt => {
                        scroll_by(list, if input.key == Key::Up { -0.1 } else { 0.1 });
                        return None;
                    }
                    _ => typed(input)?,
                };
                match state.key(c) {
                    MenuOutcome::Done(reply) => Some(reply),
                    MenuOutcome::PageUp => {
                        scroll_by(list, -1.0);
                        None
                    }
                    MenuOutcome::PageDown => {
                        scroll_by(list, 1.0);
                        None
                    }
                    MenuOutcome::Pending => {
                        fill_menu(state, list, count);
                        None
                    }
                }
            }
            Kind::Choice { allowed, default } => match typed(input)? {
                ESC_CHAR => Some(escape),
                '\n' | ' ' => default.map(|d| Reply::Char(d as i32)),
                c if allowed.contains(&c) => Some(Reply::Char(c as i32)),
                _ => None,
            },
            // a held key never answers: Esc cancels only when pressed
            Kind::Text { .. } => (input.key == Key::Escape && !input.echo).then_some(escape),
            Kind::ExtCmd {
                edit,
                list,
                palette,
            } => {
                let step = match input.key {
                    Key::Escape | Key::Tab if input.echo => return None,
                    Key::Escape => return Some(escape),
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
                        fill_palette(palette, list);
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
                fill_palette(palette, list);
                None
            }
            Kind::Show { text } => match input.key {
                Key::PageUp | Key::Char('<') => {
                    scroll_text(text, -1.0);
                    None
                }
                Key::PageDown | Key::Char('>') => {
                    scroll_text(text, 1.0);
                    None
                }
                Key::Up | Key::Down => {
                    scroll_text(text, if input.key == Key::Up { -0.1 } else { 0.1 });
                    None
                }
                _ => match typed(input)? {
                    '\n' | ' ' | ESC_CHAR => Some(Reply::Ack),
                    _ => None,
                },
            },
            Kind::MessageMenu { letter, pick } => match typed(input)? {
                ESC_CHAR => Some(escape),
                c if *pick && c == *letter => Some(Reply::Char(c as i32)),
                '\n' | ' ' if !*pick => Some(Reply::Char(0)),
                _ => None,
            },
        }
    }

    pub fn dialog_event(&mut self, req: u64, ev: &DialogEvent) -> Option<Reply> {
        let open = self.open.as_mut().filter(|o| o.req == req)?;
        let escape = open.prompt.escape_reply();
        match (&mut open.kind, ev) {
            (Kind::Menu { state, list, count }, DialogEvent::MenuClick(i)) => match state.click(*i)
            {
                MenuOutcome::Done(reply) => Some(reply),
                _ => {
                    fill_menu(state, list, count);
                    None
                }
            },
            (Kind::Menu { state, .. }, DialogEvent::MenuConfirm) => Some(state.confirm()),
            (Kind::Menu { .. }, DialogEvent::MenuCancel) => Some(Reply::Cancel),
            (Kind::Choice { allowed, .. }, DialogEvent::Choice(c)) => {
                allowed.contains(c).then_some(Reply::Char(*c as i32))
            }
            (Kind::Text { .. }, DialogEvent::TextSubmitted(s)) => {
                Some(Reply::Text(truncate_bytes(s, MAX_TEXT_BYTES)))
            }
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
        if let Some(mut open) = self.open.take() {
            open.panel.queue_free();
            open.shade.queue_free();
        }
    }
}

fn show_text(lines: &[TextLine]) -> String {
    lines
        .iter()
        .map(|l| {
            let t = bbcode_escape(&l.text);
            match l.attr {
                1 => format!("[b]{t}[/b]"),
                0 => t,
                // dim, underline, blink, inverse: an accent is enough here
                _ => format!("[color={}]{t}[/color]", hex(theme::ACCENT)),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    }

    #[test]
    fn key_names_read_like_nethack() {
        assert_eq!(key_name(0), "");
        assert_eq!(key_name(4), "^D");
        assert_eq!(key_name(0x80 | 'p' as i32), "M-p");
        assert_eq!(key_name('#' as i32), "#");
    }
}
