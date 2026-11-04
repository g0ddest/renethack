//! A container's things beside the pack (`#loot`, apply on a bag): two
//! columns, the pack's and the container's, each item marked to move
//! (all of a stack, or a count of it), then one «Готово». The panel
//! answers the engine's own list it stands for (`nh_world::LootFlow`):
//! the container's marks answer "Take out what?", and the pack's go to
//! "Put in what?" after it. Nothing moves but by those menus.
//!
//! A click marks or unmarks a row, its − and + set how many; the keys:
//! ↑↓ a row, Tab the other column, Space marks, ← → how many, Enter
//! done, Esc cancel; a pad: the d-pad, LB/RB the other column, A marks,
//! Start done, B cancel.

use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{Button, HBoxContainer, Label, ScrollContainer, TextureRect, VBoxContainer};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::{InvItem, MenuItem, Reply};

use crate::i18n::{self, EngineKind};
use crate::icons;
use crate::theme::{self, Face, place};
use crate::ui_events::{DialogEvent, UiEvent, UiQueue, push};

/// A row's height, a column's width and the rows a column shows.
const ROW_H: f32 = 40.0;
pub const COLUMN_W: f32 = 430.0;
const COLUMN_ROWS: f32 = 9.0;

/// The pack's column, and the container's.
pub const PACK: usize = 0;
pub const CONTAINER: usize = 1;

/// A row: an item to move, how much of it there is, how much is marked.
struct Row {
    button: Gd<Button>,
    mark: Gd<Label>,
    count: Gd<Label>,
    minus: Gd<Button>,
    plus: Gd<Button>,
    /// The pack's letter (the pack's column), or the menu entry's index
    /// (the container's).
    letter: char,
    idx: u32,
    quan: u32,
    marked: Option<u32>,
}

/// One column's rows and its scroll.
struct Column {
    rows: Vec<Row>,
    scroll: Gd<ScrollContainer>,
}

/// The panel's state.
pub struct LootPanel {
    columns: [Column; 2],
    /// The keys' (a pad's) row: its column and its place.
    focus: (usize, Option<usize>),
    /// The request is the put-in list (no list to take out came first):
    /// the pack's marks answer it.
    putting: bool,
    /// The put-in list's entries (its letters: the pack's rows it lists).
    put_items: Vec<MenuItem>,
    styles: [Gd<godot::classes::StyleBoxFlat>; 3],
}

fn row_style(state: usize) -> Gd<godot::classes::StyleBoxFlat> {
    let mut sb = godot::classes::StyleBoxFlat::new_gd();
    let gold = |a: f32| Color { a, ..theme::GOLD };
    sb.set_bg_color(match state {
        0 => gold(0.0),
        1 => gold(0.14),
        _ => gold(0.26),
    });
    sb.set_corner_radius_all(3);
    sb.set_content_margin_all(0.0);
    if state == 2 {
        sb.set_border_width(godot::builtin::Side::LEFT, 3);
        sb.set_border_color(theme::GOLD_BRIGHT);
    }
    sb
}

/// How many a menu entry's or a pack item's words say there are.
fn quantity(text: &str) -> u32 {
    nh_world::parse_item_name(text)
        .count
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
        .max(1)
}

/// What the panel lists: the pack, the container's things (the take-out
/// list's entries), and, when the request is the put-in list, its
/// entries (`putting`).
#[derive(Clone, Copy)]
pub struct LootSource<'a> {
    pub pack: &'a [InvItem],
    pub take: &'a [MenuItem],
    pub put_items: &'a [MenuItem],
    pub putting: bool,
}

/// What a row is: its icon, its words, its letter or entry, how many.
struct RowOf {
    icon: Gd<godot::classes::Texture2D>,
    words: String,
    letter: char,
    idx: u32,
    quan: u32,
}

impl LootPanel {
    /// The panel's two columns in `body`: `pack` (the pack's things, the
    /// put-in list's when it is the request), `take` the container's (the
    /// take-out list's entries). `req`: the request its rows answer.
    pub fn build(
        body: &mut Gd<HBoxContainer>,
        heads: [String; 2],
        source: &LootSource,
        queue: &UiQueue,
        req: u64,
    ) -> LootPanel {
        let LootSource {
            pack,
            take,
            put_items,
            putting,
        } = *source;
        let pack_rows: Vec<RowOf> = pack
            .iter()
            .filter(|it| !putting || put_items.iter().any(|m| m.ch == it.letter as i32))
            .map(|it| RowOf {
                icon: icons::inv_icon(it),
                words: i18n::engine(EngineKind::Name, &it.text).into_owned(),
                letter: it.letter,
                idx: 0,
                quan: u32::try_from(it.quan).unwrap_or(1).max(1),
            })
            .collect();
        let take_rows: Vec<RowOf> = take
            .iter()
            .filter(|m| m.selectable)
            .map(|m| {
                let text = m.str.as_deref().unwrap_or("");
                let icon = match &m.glyph {
                    Some(g) => {
                        let class = u32::try_from(g.ch).ok().and_then(char::from_u32);
                        icons::item_icon(g.tile, class.unwrap_or('?'))
                    }
                    None => icons::emblem(icons::Glyph::Sack),
                };
                RowOf {
                    icon,
                    words: i18n::engine(EngineKind::Name, text).into_owned(),
                    letter: u32::try_from(m.ch)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap_or(' '),
                    idx: u32::try_from(m.idx).unwrap_or(0),
                    quan: quantity(text),
                }
            })
            .collect();
        let styles = [row_style(0), row_style(1), row_style(2)];
        let columns = [(PACK, pack_rows), (CONTAINER, take_rows)].map(|(c, rows)| {
            let mut col = VBoxContainer::new_alloc();
            col.add_theme_constant_override("separation", 6);
            col.set_custom_minimum_size(Vector2::new(COLUMN_W, 0.0));
            let mut head = theme::styled_label(&heads[c], Face::Caps, 16, theme::GOLD_BRIGHT);
            head.set_autowrap_mode(AutowrapMode::WORD_SMART);
            col.add_child(&head);
            let mut scroll = ScrollContainer::new_alloc();
            scroll.set_custom_minimum_size(Vector2::new(COLUMN_W, ROW_H * COLUMN_ROWS));
            scroll
                .set_horizontal_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
            let mut list = VBoxContainer::new_alloc();
            list.add_theme_constant_override("separation", 2);
            list.set_h_size_flags(SizeFlags::EXPAND_FILL);
            scroll.add_child(&list);
            col.add_child(&scroll);
            if rows.is_empty() {
                let none =
                    theme::styled_label(&crate::tr!("loot-empty"), Face::Body, 16, theme::TEXT_OFF);
                list.add_child(&none);
            }
            let mut made = Vec::new();
            for (i, r) in rows.into_iter().enumerate() {
                made.push(Self::row(&mut list, r, &styles, queue, req, c, i));
            }
            body.add_child(&col);
            Column { rows: made, scroll }
        });
        LootPanel {
            columns,
            focus: (if putting { PACK } else { CONTAINER }, None),
            putting,
            put_items: put_items.to_vec(),
            styles,
        }
    }

    fn row(
        list: &mut Gd<VBoxContainer>,
        r: RowOf,
        styles: &[Gd<godot::classes::StyleBoxFlat>; 3],
        queue: &UiQueue,
        req: u64,
        col: usize,
        i: usize,
    ) -> Row {
        let mut b = Button::new_alloc();
        b.set_focus_mode(FocusMode::NONE);
        b.set_custom_minimum_size(Vector2::new(COLUMN_W - 14.0, ROW_H));
        for s in ["normal", "disabled", "focus"] {
            b.add_theme_stylebox_override(s, &styles[0]);
        }
        for s in ["hover", "pressed", "hover_pressed"] {
            b.add_theme_stylebox_override(s, &styles[1]);
        }
        let mut hbox = HBoxContainer::new_alloc();
        hbox.set_mouse_filter(MouseFilter::IGNORE);
        hbox.add_theme_constant_override("separation", 6);
        let mut mark = theme::styled_label("", Face::BodyBold, 18, theme::GOOD);
        mark.set_custom_minimum_size(Vector2::new(18.0, 0.0));
        mark.set_vertical_alignment(VerticalAlignment::CENTER);
        hbox.add_child(&mark);
        let mut icon = TextureRect::new_alloc();
        icon.set_mouse_filter(MouseFilter::IGNORE);
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_custom_minimum_size(Vector2::new(28.0, 28.0));
        icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        icon.set_texture(&r.icon);
        hbox.add_child(&icon);
        let mut words: Gd<Label> = theme::styled_label(&r.words, Face::Body, 16, theme::TEXT);
        words.set_vertical_alignment(VerticalAlignment::CENTER);
        words.set_h_size_flags(SizeFlags::EXPAND_FILL);
        words.set_clip_text(true);
        words
            .set_text_overrun_behavior(godot::classes::text_server::OverrunBehavior::TRIM_ELLIPSIS);
        hbox.add_child(&words);
        // how many of a stack: − N +, while it is marked
        let small = |text: &str, d: i32| {
            let mut s = Button::new_alloc();
            s.set_text(text);
            s.set_focus_mode(FocusMode::NONE);
            s.set_custom_minimum_size(Vector2::new(26.0, 26.0));
            s.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            s.set_visible(false);
            let q = queue.clone();
            s.signals().pressed().connect(move || {
                let ev = DialogEvent::LootCount {
                    column: col,
                    row: i,
                    delta: d,
                };
                push(&q, UiEvent::Dialog { req, ev });
            });
            s
        };
        let minus = small("−", -1);
        hbox.add_child(&minus);
        let mut count = theme::styled_label("", Face::BodyBold, 15, theme::GOLD_BRIGHT);
        count.set_vertical_alignment(VerticalAlignment::CENTER);
        count.set_horizontal_alignment(HorizontalAlignment::CENTER);
        count.set_custom_minimum_size(Vector2::new(34.0, 0.0));
        hbox.add_child(&count);
        let plus = small("+", 1);
        hbox.add_child(&plus);
        place(&hbox, [0.0, 0.0, 1.0, 1.0], [6.0, 0.0, -6.0, 0.0]);
        b.add_child(&hbox);
        let q = queue.clone();
        b.signals().pressed().connect(move || {
            let ev = DialogEvent::LootToggle {
                column: col,
                row: i,
            };
            push(&q, UiEvent::Dialog { req, ev });
        });
        list.add_child(&b);
        let mut row = Row {
            button: b,
            mark,
            count,
            minus,
            plus,
            letter: r.letter,
            idx: r.idx,
            quan: r.quan,
            marked: None,
        };
        Self::show(&mut row, false, styles);
        row
    }

    /// A row as it is now: its mark, its count, its look.
    fn show(r: &mut Row, focus: bool, styles: &[Gd<godot::classes::StyleBoxFlat>; 3]) {
        r.mark.set_text(if r.marked.is_some() { "✔" } else { "" });
        let stack = r.quan > 1 && r.marked.is_some();
        // unmarked, the words say how many ("5 arrows"): the count shows
        // only what is chosen of a stack
        r.count.set_text(&match r.marked {
            Some(n) if stack => format!("{n}/{}", r.quan),
            _ => String::new(),
        });
        r.minus.set_visible(stack);
        r.plus.set_visible(stack);
        let state = if focus {
            2
        } else if r.marked.is_some() {
            1
        } else {
            0
        };
        for s in ["normal", "disabled", "focus"] {
            r.button.add_theme_stylebox_override(s, &styles[state]);
        }
    }

    fn redraw(&mut self, col: usize, i: usize) {
        let focus = self.focus == (col, Some(i));
        let styles = self.styles.clone();
        if let Some(r) = self.columns[col].rows.get_mut(i) {
            Self::show(r, focus, &styles);
        }
    }

    /// Row `i` of column `col` marked, all of it, or unmarked.
    pub fn toggle(&mut self, col: usize, i: usize) {
        let Some(r) = self.columns.get_mut(col).and_then(|c| c.rows.get_mut(i)) else {
            return;
        };
        r.marked = match r.marked {
            Some(_) => None,
            None => Some(r.quan),
        };
        self.set_focus(col, Some(i));
        self.redraw(col, i);
    }

    /// How many of row `i`'s stack are marked, `d` more or fewer (one at
    /// least, all at most).
    pub fn count(&mut self, col: usize, i: usize, d: i32) {
        let Some(r) = self.columns.get_mut(col).and_then(|c| c.rows.get_mut(i)) else {
            return;
        };
        if let Some(n) = r.marked {
            r.marked = Some((n as i64 + d as i64).clamp(1, r.quan as i64) as u32);
        }
        self.redraw(col, i);
    }

    fn set_focus(&mut self, col: usize, i: Option<usize>) {
        let was = self.focus;
        self.focus = (col, i);
        if let (c, Some(j)) = was {
            self.redraw(c, j);
        }
        if let Some(j) = i {
            self.redraw(col, j);
            let row = &self.columns[col].rows[j];
            let mut scroll = self.columns[col].scroll.clone();
            scroll.call_deferred("ensure_control_visible", &[row.button.to_variant()]);
        }
    }

    /// The pack's marks: letters and counts (None: all of it).
    pub fn pack_marks(&self) -> Vec<(char, Option<u32>)> {
        self.columns[PACK]
            .rows
            .iter()
            .filter_map(|r| {
                let n = r.marked?;
                Some((r.letter, (n < r.quan).then_some(n)))
            })
            .collect()
    }

    /// The answer to the request: the container's marks for the take-out
    /// list, the pack's for the put-in list (nothing marked: none).
    pub fn reply(&self) -> Reply {
        let picks: Vec<(u32, i64)> = if self.putting {
            self.pack_marks()
                .iter()
                .filter_map(|(letter, n)| {
                    let m = self.put_items.iter().find(|m| m.ch == *letter as i32)?;
                    Some((u32::try_from(m.idx).ok()?, n.map_or(-1, i64::from)))
                })
                .collect()
        } else {
            self.columns[CONTAINER]
                .rows
                .iter()
                .filter_map(|r| {
                    let n = r.marked?;
                    Some((r.idx, if n < r.quan { n as i64 } else { -1 }))
                })
                .collect()
        };
        if picks.is_empty() {
            Reply::Cancel
        } else {
            Reply::Menu(picks)
        }
    }

    /// Whether the request is the put-in list.
    pub fn putting(&self) -> bool {
        self.putting
    }

    /// A key: ↑↓ a row, Tab (PgUp PgDn: a pad's bumpers) the other
    /// column, Space marks, ← → how many. Enter and Esc are the caller's.
    pub fn key(&mut self, k: &nh_world::KeyInput) {
        use nh_world::Key;
        let (col, at) = self.focus;
        let n = self.columns[col].rows.len();
        match k.key {
            Key::Up | Key::Down if n > 0 => {
                let next = match (at, k.key == Key::Down) {
                    (None, _) => 0,
                    (Some(i), true) => (i + 1).min(n - 1),
                    (Some(i), false) => i.saturating_sub(1),
                };
                self.set_focus(col, Some(next));
            }
            Key::Tab | Key::PageUp | Key::PageDown if !k.echo => {
                let other = 1 - col;
                let m = self.columns[other].rows.len();
                let i = at.map(|i| i.min(m.saturating_sub(1)));
                self.set_focus(other, i.filter(|_| m > 0).or((m > 0).then_some(0)));
            }
            Key::Char(' ') if !k.echo => {
                if let Some(i) = at {
                    self.toggle(col, i);
                }
            }
            Key::Left | Key::Right => {
                if let Some(i) = at {
                    self.count(col, i, if k.key == Key::Left { -1 } else { 1 });
                }
            }
            _ => {}
        }
    }

    /// The rows marked in each column, as their words' places (self-tests).
    pub fn marked(&self) -> [Vec<usize>; 2] {
        [PACK, CONTAINER].map(|c| {
            (self.columns[c].rows.iter().enumerate())
                .filter(|(_, r)| r.marked.is_some())
                .map(|(i, _)| i)
                .collect()
        })
    }

    /// The pack's column's letters, top down (self-tests).
    pub fn pack_letters(&self) -> Vec<char> {
        self.columns[PACK].rows.iter().map(|r| r.letter).collect()
    }

    /// How many rows each column has (self-tests).
    pub fn lens(&self) -> [usize; 2] {
        [PACK, CONTAINER].map(|c| self.columns[c].rows.len())
    }

    /// Where row `i` of column `col` is on screen (self-tests).
    pub fn row_rect(&self, col: usize, i: usize) -> Option<Rect2> {
        Some(self.columns.get(col)?.rows.get(i)?.button.get_global_rect())
    }

    /// The keys' row (self-tests).
    pub fn focus(&self) -> (usize, Option<usize>) {
        self.focus
    }
}
