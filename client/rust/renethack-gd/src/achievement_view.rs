//! The achievements on screen (phase S3 of the achievements): the toast
//! that tells of one just earned, under the prompt banner, one at a time
//! and never over a dialog or a panel; and the achievements page, from the
//! title and from the HUD: every medallion, earned or locked (a hidden one
//! a "?" until it is earned), the chosen one's name and description and,
//! once earned, by whom, on which turn and when. The arrows (the d-pad)
//! choose; Esc (B) goes back.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::image::Format;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, CanvasLayer, Control, GridContainer, HBoxContainer, Image, ImageTexture, Label, Panel,
    PanelContainer, StyleBoxFlat, Texture2D, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::achievements::{Achievement, Achievements, Store, Unlock};

use crate::achievement_bake::{self, GAME_DIR};
use crate::i18n;
use crate::screens::format_time;
use crate::theme::{self, Face, Frame, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

thread_local! {
    static TEXTURES: RefCell<HashMap<String, Gd<Texture2D>>> = RefCell::new(HashMap::new());
}

/// Let the cached medallions go while Godot still runs (`icons::clear`).
pub fn clear() {
    TEXTURES.with(|t| t.borrow_mut().clear());
}

/// A medallion as the bake drew it, earned or locked.
fn medallion(id: &str, locked: bool) -> Option<Gd<Texture2D>> {
    let path = format!("{GAME_DIR}/{id}{}.png", if locked { "_locked" } else { "" });
    if let Some(t) = TEXTURES.with(|t| t.borrow().get(&path).cloned()) {
        return Some(t);
    }
    let t = godot::tools::try_load::<Texture2D>(&path).ok()?;
    TEXTURES.with(|m| m.borrow_mut().insert(path, t.clone()));
    Some(t)
}

/// The medallion of a hidden achievement not yet earned: a locked one
/// with nothing on it (a "?" goes over it).
fn mystery() -> Option<Gd<Texture2D>> {
    const KEY: &str = "?";
    if let Some(t) = TEXTURES.with(|t| t.borrow().get(KEY).cloned()) {
        return Some(t);
    }
    let side = 256;
    let px = achievement_bake::locked(&achievement_bake::medallion(side, None, false));
    let image = Image::create_from_data(
        side as i32,
        side as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(px.as_slice()),
    )?;
    let t: Gd<Texture2D> = ImageTexture::create_from_image(&image)?.upcast();
    TEXTURES.with(|m| m.borrow_mut().insert(KEY.to_string(), t.clone()));
    Some(t)
}

fn picture(size: f32) -> Gd<TextureRect> {
    let mut r = TextureRect::new_alloc();
    r.set_expand_mode(ExpandMode::IGNORE_SIZE);
    r.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    r.set_custom_minimum_size(Vector2::new(size, size));
    r.set_mouse_filter(MouseFilter::IGNORE);
    r
}

/// A "?" over a hidden medallion, `size` across.
fn question(size: f32) -> Gd<Label> {
    let mut q = theme::styled_label("?", Face::TitleBold, (size * 0.5) as i32, theme::TEXT_DIM);
    theme::outline(&q, 6);
    q.set_horizontal_alignment(HorizontalAlignment::CENTER);
    q.set_vertical_alignment(VerticalAlignment::CENTER);
    place(&q, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
    // a mark, the same in every language
    i18n::verbatim(&q);
    q
}

/// Its name and description as the player reads them: the catalogs',
/// else the definition's English.
fn words(a: &Achievement) -> (String, String) {
    let pick = |key: &str, english: &str| {
        if i18n::has(key) {
            i18n::tr(key)
        } else {
            english.to_string()
        }
    };
    (pick(&a.name, &a.name_en), pick(&a.desc, &a.desc_en))
}

// ---- the toast ----

/// Seconds a toast takes to come, stays, and takes to go.
const TOAST_IN: f64 = 0.3;
const TOAST_HOLD: f64 = 4.5;
const TOAST_OUT: f64 = 0.6;
/// Its top: under the prompt banner and the ribbon of a new mode.
const TOAST_Y: f32 = 150.0;
const TOAST_ICON: f32 = 72.0;
const TOAST_W: f32 = 560.0;
const TOAST_NAME: i32 = 24;
const TOAST_DESC: i32 = 16;

/// An achievement just earned, told under the prompt banner; several
/// wait their turn. Something over the map (a dialog, a panel, a page)
/// holds it back: it is hidden and its time stops.
pub struct Toast {
    /// Full-screen, the mouse goes through: the panel's anchors.
    root: Gd<Control>,
    panel: Gd<PanelContainer>,
    icon: Gd<TextureRect>,
    kicker: Gd<Label>,
    name: Gd<Label>,
    desc: Gd<Label>,
    waiting: VecDeque<Achievement>,
    /// The one told now, and for how long it has been shown.
    shown: Option<(Achievement, f64)>,
}

impl Toast {
    pub fn new(mut layer: Gd<CanvasLayer>) -> Toast {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        layer.add_child(&root);
        let mut panel = theme::framed(Frame::Banner);
        panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut hbox = HBoxContainer::new_alloc();
        hbox.add_theme_constant_override("separation", 14);
        hbox.set_mouse_filter(MouseFilter::IGNORE);
        let icon = picture(TOAST_ICON);
        hbox.add_child(&icon);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 0);
        col.set_alignment(AlignmentMode::CENTER);
        col.set_h_size_flags(SizeFlags::EXPAND_FILL);
        col.set_mouse_filter(MouseFilter::IGNORE);
        let kicker = theme::styled_label("", Face::Caps, 14, theme::GOLD);
        col.add_child(&kicker);
        // the name on one line; the description wrapped, as tall as its
        // lines measure (a wrapped label sizes itself late, and only grows)
        let mut name = theme::styled_label("", Face::TitleBold, TOAST_NAME, theme::GOLD_BRIGHT);
        theme::outline(&name, 4);
        name.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        name.set_clip_text(true);
        col.add_child(&name);
        let mut desc = theme::styled_label("", Face::Body, TOAST_DESC, theme::TEXT);
        desc.set_autowrap_mode(AutowrapMode::WORD_SMART);
        desc.set_clip_text(true);
        col.add_child(&desc);
        hbox.add_child(&col);
        panel.add_child(&hbox);
        panel.set_visible(false);
        root.add_child(&panel);
        Toast {
            root,
            panel,
            icon,
            kicker,
            name,
            desc,
            waiting: VecDeque::new(),
            shown: None,
        }
    }

    /// Tell of `a` after those already waiting.
    pub fn push(&mut self, a: Achievement) {
        self.waiting.push_back(a);
    }

    /// The time passes (`dt` seconds); `blocked`: something covers the
    /// map, the toast waits hidden.
    pub fn tick(&mut self, dt: f64, blocked: bool) {
        if blocked {
            self.panel.set_visible(false);
            return;
        }
        if self.shown.is_none() {
            let Some(next) = self.waiting.pop_front() else {
                self.panel.set_visible(false);
                return;
            };
            self.shown = Some((next, 0.0));
            self.fill();
        }
        let Some((_, t)) = self.shown.as_mut() else {
            return;
        };
        *t += dt;
        let t = *t;
        if t >= TOAST_IN + TOAST_HOLD + TOAST_OUT {
            self.shown = None;
            self.panel.set_visible(false);
            return;
        }
        let alpha = if t < TOAST_IN {
            t / TOAST_IN
        } else if t > TOAST_IN + TOAST_HOLD {
            1.0 - (t - TOAST_IN - TOAST_HOLD) / TOAST_OUT
        } else {
            1.0
        };
        self.panel.set_modulate(Color::from_rgba(
            1.0,
            1.0,
            1.0,
            alpha.clamp(0.0, 1.0) as f32,
        ));
        self.panel.set_visible(true);
    }

    /// The one shown, its words in the language now, in the middle of the
    /// top of the screen.
    fn fill(&mut self) {
        let Some((a, _)) = &self.shown else {
            return;
        };
        let (name, desc) = words(a);
        self.kicker.set_text(&tr!("achievements-unlocked"));
        self.name.set_text(&name);
        self.desc.set_text(&desc);
        if let Some(t) = medallion(&a.id, false) {
            self.icon.set_texture(&t);
        }
        // as wide as the top of the screen has room for, between the
        // portrait block and the minimap, its frame included
        let width = crate::hud::banner_room(self.root.get_viewport_rect().size.x).min(TOAST_W);
        let frame = self.panel.get_theme_stylebox("panel").map_or(32.0, |s| {
            s.get_content_margin(Side::LEFT) + s.get_content_margin(Side::RIGHT)
        });
        let text_w = (width - TOAST_ICON - 14.0 - frame).floor();
        let line = |face: Face, size: i32| theme::font(face).get_height_ex().font_size(size).done();
        self.name
            .set_custom_minimum_size(Vector2::new(text_w, line(Face::TitleBold, TOAST_NAME)));
        let wrapped = theme::font(Face::Body)
            .get_multiline_string_size_ex(&desc)
            .width(text_w)
            .font_size(TOAST_DESC)
            .done();
        self.desc.set_custom_minimum_size(Vector2::new(
            text_w,
            wrapped.y.max(line(Face::Body, TOAST_DESC)).ceil(),
        ));
        let half = (width / 2.0).floor();
        place(
            &self.panel,
            [0.5, 0.0, 0.5, 0.0],
            [-half, TOAST_Y, half, TOAST_Y],
        );
        self.panel.reset_size();
    }

    /// The words again in the language now.
    pub fn relang(&mut self) {
        self.fill();
    }

    /// The id of the achievement told now (self-tests).
    pub fn shown(&self) -> Option<&str> {
        self.shown
            .as_ref()
            .filter(|_| self.panel.is_visible())
            .map(|(a, _)| a.id.as_str())
    }

    /// Fully come in, not yet going (self-tests: its picture).
    pub fn settled(&self) -> bool {
        self.panel.is_visible()
            && self
                .shown
                .as_ref()
                .is_some_and(|(_, t)| (TOAST_IN..TOAST_IN + TOAST_HOLD).contains(t))
    }

    /// Where it is on screen while shown (self-tests).
    pub fn rect(&self) -> Option<Rect2> {
        self.panel
            .is_visible()
            .then(|| self.panel.get_global_rect())
    }
}

// ---- the page ----

/// Medallions in a row of the grid.
const COLUMNS: usize = 10;
/// The detail pane's width and its medallion's side.
const DETAIL_W: f32 = 340.0;
const DETAIL_ICON: f32 = 160.0;

/// One achievement on the page: its definition and, once earned, when.
struct Entry {
    a: Achievement,
    earned: Option<Unlock>,
}

impl Entry {
    /// Hidden and not earned: shown as a "?".
    fn secret(&self) -> bool {
        self.a.hidden && self.earned.is_none()
    }
}

/// The achievements page: a grid of medallions and the chosen one's
/// story. Built anew each time it opens (and on a switch of language).
pub struct Page {
    root: Gd<PanelContainer>,
    entries: Vec<Entry>,
    marks: Vec<Gd<Panel>>,
    selected: usize,
    detail_icon: Gd<TextureRect>,
    detail_secret: Gd<Label>,
    detail_name: Gd<Label>,
    detail_desc: Gd<Label>,
    detail_when: Gd<Label>,
}

impl Page {
    /// Every achievement of `all`, earned as `store` has them; the grid's
    /// medallions as large as the screen (`canvas`) allows.
    pub fn new(all: &Achievements, store: &Store, queue: &UiQueue, canvas: Vector2) -> Page {
        let entries: Vec<Entry> = all
            .all()
            .iter()
            .map(|a| Entry {
                a: a.clone(),
                earned: store.unlocked.get(&a.id).cloned(),
            })
            .collect();
        let rows = entries.len().div_ceil(COLUMNS) as f32;
        // the title, the count, the hint and the frame take about 330
        let cell = ((canvas.y - 330.0) / rows).clamp(52.0, 84.0).floor();
        let gap = (cell * 0.08).round().max(4.0);

        let mut root = theme::framed(Frame::Panel);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        let mut head = HBoxContainer::new_alloc();
        head.add_theme_constant_override("separation", 18);
        let mut title =
            theme::styled_label(&tr!("achievements-title"), Face::Title, 32, theme::ACCENT);
        title.set_vertical_alignment(VerticalAlignment::BOTTOM);
        head.add_child(&title);
        let earned = entries.iter().filter(|e| e.earned.is_some()).count();
        let mut count = theme::styled_label(
            &tr!(
                "achievements-progress",
                earned = earned,
                total = entries.len()
            ),
            Face::Body,
            18,
            theme::TEXT_DIM,
        );
        count.set_vertical_alignment(VerticalAlignment::BOTTOM);
        count.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        head.add_child(&count);
        col.add_child(&head);

        let mut body = HBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", 24);
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(COLUMNS as i32);
        grid.add_theme_constant_override("h_separation", gap as i32);
        grid.add_theme_constant_override("v_separation", gap as i32);
        let mut marks = Vec::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            let mut b = Button::new_alloc();
            b.set_flat(true);
            b.set_focus_mode(godot::classes::control::FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(cell, cell));
            let q = queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::AchievementPick(i)));
            let q = queue.clone();
            b.signals()
                .mouse_entered()
                .connect(move || push(&q, UiEvent::AchievementPick(i)));
            let mut pic = picture(cell);
            place(&pic, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
            let texture = if e.secret() {
                mystery()
            } else {
                medallion(&e.a.id, e.earned.is_none())
            };
            if let Some(t) = texture {
                pic.set_texture(&t);
            }
            b.add_child(&pic);
            if e.secret() {
                b.add_child(&question(cell));
            }
            let mut mark = Panel::new_alloc();
            mark.add_theme_stylebox_override("panel", &selection_frame());
            mark.set_mouse_filter(MouseFilter::IGNORE);
            place(&mark, [0.0, 0.0, 1.0, 1.0], [-3.0, -3.0, 3.0, 3.0]);
            mark.set_visible(false);
            b.add_child(&mark);
            marks.push(mark);
            grid.add_child(&b);
        }
        body.add_child(&grid);

        let mut detail = VBoxContainer::new_alloc();
        detail.add_theme_constant_override("separation", 8);
        detail.set_custom_minimum_size(Vector2::new(DETAIL_W, 0.0));
        let mut holder = Control::new_alloc();
        let big = DETAIL_ICON.min(cell * 2.4).max(110.0);
        holder.set_custom_minimum_size(Vector2::new(big, big));
        holder.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let detail_icon = picture(big);
        place(&detail_icon, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        holder.add_child(&detail_icon);
        let detail_secret = question(big);
        holder.add_child(&detail_secret);
        detail.add_child(&holder);
        let mut detail_name = theme::styled_label("", Face::TitleBold, 26, theme::GOLD_BRIGHT);
        detail_name.set_horizontal_alignment(HorizontalAlignment::CENTER);
        detail_name.set_autowrap_mode(AutowrapMode::WORD_SMART);
        detail_name.set_custom_minimum_size(Vector2::new(DETAIL_W, 0.0));
        detail.add_child(&detail_name);
        let mut detail_desc = theme::styled_label("", Face::Body, 18, theme::TEXT);
        detail_desc.set_horizontal_alignment(HorizontalAlignment::CENTER);
        detail_desc.set_autowrap_mode(AutowrapMode::WORD_SMART);
        detail_desc.set_custom_minimum_size(Vector2::new(DETAIL_W, 0.0));
        detail.add_child(&detail_desc);
        let mut detail_when = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        detail_when.set_horizontal_alignment(HorizontalAlignment::CENTER);
        detail_when.set_autowrap_mode(AutowrapMode::WORD_SMART);
        detail_when.set_custom_minimum_size(Vector2::new(DETAIL_W, 0.0));
        detail.add_child(&detail_when);
        body.add_child(&detail);
        col.add_child(&body);

        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 18);
        let mut hint = theme::styled_label(
            &i18n::whole_parts(&tr!("achievements-hint")),
            Face::Body,
            16,
            theme::TEXT_DIM,
        );
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        hint.set_vertical_alignment(VerticalAlignment::CENTER);
        foot.add_child(&hint);
        foot.add_child(&theme::button(
            &tr!("achievements-back"),
            queue,
            UiEvent::CloseAchievements,
        ));
        col.add_child(&foot);
        root.add_child(&col);

        let mut page = Page {
            root,
            entries,
            marks,
            selected: 0,
            detail_icon,
            detail_secret,
            detail_name,
            detail_desc,
            detail_when,
        };
        // the first earned one, else the first
        let first = page
            .entries
            .iter()
            .position(|e| e.earned.is_some())
            .unwrap_or(0);
        page.select(first);
        page
    }

    /// What the screens show.
    pub fn root(&self) -> Gd<Control> {
        self.root.clone().upcast()
    }

    /// Choose the `i`-th medallion: its story in the detail pane.
    pub fn select(&mut self, i: usize) {
        let Some(e) = self.entries.get(i) else {
            return;
        };
        for (k, m) in self.marks.iter_mut().enumerate() {
            m.set_visible(k == i);
        }
        self.selected = i;
        let secret = e.secret();
        let texture = if secret {
            mystery()
        } else {
            medallion(&e.a.id, e.earned.is_none())
        };
        if let Some(t) = texture {
            self.detail_icon.set_texture(&t);
        }
        self.detail_secret.set_visible(secret);
        let (name, desc) = if secret {
            (
                tr!("achievements-hidden-name"),
                tr!("achievements-hidden-desc"),
            )
        } else {
            words(&e.a)
        };
        self.detail_name.set_text(&name);
        self.detail_desc.set_text(&desc);
        let when = match &e.earned {
            Some(u) => {
                let at =
                    std::time::UNIX_EPOCH + std::time::Duration::from_secs(u.time.max(0) as u64);
                let date = format_time(at);
                let date = date.split(' ').next().unwrap_or(&date).to_string();
                tr!(
                    "achievements-earned",
                    character = &u.character,
                    turn = u.turn,
                    date = date
                )
            }
            None => tr!("achievements-locked"),
        };
        self.detail_when.set_text(&when);
    }

    /// Move the choice by a step of the arrows or the d-pad (rows wrap
    /// to the next).
    pub fn step(&mut self, dx: i32, dy: i32) {
        let n = self.entries.len() as i32;
        let mut i = self.selected as i32 + dx + dy * COLUMNS as i32;
        if dy != 0 && !(0..n).contains(&i) {
            // past the top or the bottom: stay in the column
            i = self.selected as i32;
        }
        self.select(i.clamp(0, n - 1) as usize);
    }

    /// The medallion chosen now (self-tests).
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The ids of the earned achievements shown, and of the secrets
    /// (self-tests).
    pub fn earned_ids(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.earned.is_some())
            .map(|e| e.a.id.as_str())
            .collect()
    }

    pub fn secret_count(&self) -> usize {
        self.entries.iter().filter(|e| e.secret()).count()
    }

    /// The page's frame on screen (self-tests: it fits the canvas).
    pub fn rect(&self) -> Rect2 {
        self.root.get_global_rect()
    }
}

/// The gold frame round the chosen medallion.
fn selection_frame() -> Gd<StyleBoxFlat> {
    let mut s = StyleBoxFlat::new_gd();
    s.set_bg_color(Color::from_rgba(1.0, 0.85, 0.5, 0.08));
    s.set_border_color(theme::GOLD_BRIGHT);
    s.set_border_width_all(3);
    s.set_corner_radius_all(10);
    s
}
