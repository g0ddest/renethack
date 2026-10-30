//! Full-screen pages outside a game: title, settings, character creation,
//! the end of a game and errors.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use godot::classes::control::{FocusMode, GrowDirection, MouseFilter};
use godot::classes::{
    BaseButton, Button, CanvasLayer, CenterContainer, ColorRect, Control, Gradient,
    GradientTexture2D, HBoxContainer, Label, LineEdit, MarginContainer, OptionButton,
    PanelContainer, RichTextLabel, ScrollContainer, StyleBox, TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_link::SavedGame;
use nh_protocol::Catalog;
use nh_world::KeyProfile;

pub use crate::creation::Taken;
use crate::creation::{self, Form};
use crate::dialogs::{Osk, OskKey};
use crate::end_view::{self, EndSummary};
use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, Lang};
use crate::pad_view;
use crate::theme::{self, Face};
use crate::title_scene::Preview;
use crate::tr;
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, push};

pub struct Screens {
    root: Gd<Control>,
    /// The page's background: opaque, but on the title, where the scene
    /// behind it shows through.
    bg: Gd<ColorRect>,
    content: Option<Gd<Control>>,
    queue: UiQueue,
    current: Option<&'static str>,
    form: Option<Form>,
    end: Option<EndSummary>,
    /// What the title shows (the saves, a notice), for drawing it again.
    title: (Vec<SavedGame>, Option<String>),
    /// The settings page was opened from a game (Back returns to it).
    settings_in_game: bool,
    /// The achievements page was opened from a game.
    achievements_in_game: bool,
    /// The credits page was opened over a game.
    credits_in_game: bool,
    /// The title offers the prologue again (the profile keeps one).
    prologue: bool,
    /// The page's controls the keys and a gamepad move among.
    nav: Nav,
    /// The end page's body (the keys and the pad scroll it) and its
    /// hints.
    end_page: Option<(Gd<ScrollContainer>, Gd<HBoxContainer>, Gd<PanelContainer>)>,
    /// A gamepad gives the input: the hints are its buttons.
    pad: Option<PadKind>,
    /// The creation form's on-screen keyboard, typing the hero's name.
    name_keys: Option<NameKeys>,
}

/// The on-screen keyboard over the creation form (A on the name field
/// while a gamepad plays): the d-pad moves, A types, B erases (and with
/// nothing left closes), Y switches the letters, Start is done.
struct NameKeys {
    panel: Gd<PanelContainer>,
    osk: Osk,
    /// The name as typed so far.
    shown: Gd<Label>,
    field: Gd<LineEdit>,
}

/// What the keys and a gamepad do on the title, the creation form, the
/// end and an error: the focus up and down (a list's choice left and
/// right), the control it is on taken, the page left, its main deed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavInput {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    Start,
    /// Tab and Shift+Tab: the focus along (as down and up but on the
    /// end page, whose arrows scroll).
    Next,
    Prev,
    /// PgUp and PgDn, the bumpers: the end page's body a screen along.
    PageUp,
    PageDown,
    /// Y: the on-screen keyboard's other letters (Latin, Cyrillic).
    Layout,
}

/// A page's controls in their order, the one the focus is on, what going
/// back sends, and which control Start takes; the cards among them laid
/// out in a grid.
#[derive(Default)]
struct Nav {
    items: Vec<Gd<Control>>,
    at: usize,
    back: Option<UiEvent>,
    start: Option<usize>,
    grid: Option<Grid>,
}

/// Cards in a grid (the creation form's roles): ↑↓ go a row, ←→ a card
/// along it, and the card the focus is on is the one chosen; the focus
/// comes into the grid on the card chosen, and A takes it on past the
/// grid.
struct Grid {
    cards: std::ops::Range<usize>,
    columns: usize,
    /// The look of the card the focus is on.
    focus: Gd<StyleBox>,
}

impl NameKeys {
    /// The name as typed, a caret after it.
    fn show_name(&mut self) {
        let text = self.field.get_text().to_string();
        self.shown.set_text(&format!("{text}_"));
    }
}

/// The game's name on the title: a name, the same in every language.
const GAME_NAME: &str = "renethack";

/// How far an arrow scrolls the end page, in design pixels.
const END_STEP: f32 = 90.0;

/// How much the settings page's backdrop hides of the scene behind it.
const SETTINGS_SHADE: f32 = 0.62;

/// The share of the title's width the menu stands in (on the left).
const TITLE_MENU_WIDTH: f32 = 0.42;

/// The creation form's panels keep this far from the screen's edges.
const CREATION_EDGE: i32 = 28;

/// The title's shade over its scene (`across`): the ink of the pages
/// under the menu on the left, clearing towards the right; else (down)
/// the top edge, over the dark past the chamber, and a little of the
/// bottom.
fn title_shade(across: bool) -> Gd<GradientTexture2D> {
    let ink = |a: f32| theme::BG.with_alpha(a);
    let (offsets, alphas): (&[f32], &[f32]) = if across {
        (&[0.0, 0.3, 0.5, 0.68, 1.0], &[0.9, 0.78, 0.4, 0.0, 0.0])
    } else {
        (&[0.0, 0.17, 0.75, 1.0], &[0.95, 0.0, 0.0, 0.5])
    };
    let colors: Vec<Color> = alphas.iter().map(|&a| ink(a)).collect();
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(offsets));
    g.set_colors(&PackedColorArray::from(&colors[..]));
    let mut t = GradientTexture2D::new_gd();
    t.set_gradient(&g);
    let (w, h) = if across { (256, 4) } else { (4, 256) };
    t.set_width(w);
    t.set_height(h);
    t.set_fill_from(Vector2::new(0.0, 0.0));
    t.set_fill_to(if across {
        Vector2::new(1.0, 0.0)
    } else {
        Vector2::new(0.0, 1.0)
    });
    t
}

/// A picker of the interface's language, each in its own name; a choice
/// switches at once.
/// A list's choice `by` along, round, past what cannot be chosen, as a
/// pick would (its signal).
fn cycle(ob: &mut Gd<OptionButton>, by: i32) {
    let n = ob.get_item_count();
    if n == 0 {
        return;
    }
    let mut i = ob.get_selected().max(0);
    for _ in 0..n {
        i = (i + by).rem_euclid(n);
        if !ob.is_item_disabled(i) && !ob.is_item_separator(i) {
            break;
        }
    }
    ob.select(i);
    ob.emit_signal("item_selected", &[i64::from(i).to_variant()]);
}

fn language_option(queue: &UiQueue) -> Gd<OptionButton> {
    let mut ob = OptionButton::new_alloc();
    ob.set_focus_mode(FocusMode::NONE);
    // each language in its own name, whatever the language now
    i18n::verbatim(&ob);
    for lang in Lang::ALL {
        ob.add_item(lang.name());
    }
    let now = Lang::ALL
        .iter()
        .position(|&l| l == i18n::lang())
        .unwrap_or(0);
    ob.select(now as i32);
    let q = queue.clone();
    ob.signals().item_selected().connect(move |i: i64| {
        if let Some(&lang) = Lang::ALL.get(i as usize) {
            push(&q, UiEvent::SetLanguage(lang));
        }
    });
    ob
}

fn title_label(text: &str, size: i32) -> Gd<Label> {
    let mut l = theme::styled_label(text, theme::Face::Title, size, theme::ACCENT);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l
}

fn wide(mut b: Gd<Button>) -> Gd<Button> {
    b.set_custom_minimum_size(Vector2::new(460.0, 44.0));
    b
}

fn column() -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 12);
    col
}

fn row() -> Gd<HBoxContainer> {
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 12);
    row
}

/// Unix time as "YYYY-MM-DD HH:MM" in the player's own time zone.
pub fn format_time(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    format_civil(secs + local_offset(secs))
}

/// How far the player's time zone is ahead of UTC at `secs`, in seconds
/// (its summer time then, if any).
#[cfg(unix)]
fn local_offset(secs: i64) -> i64 {
    let t = secs as libc::time_t;
    // SAFETY: a zeroed tm is a valid one; localtime_r writes only into it
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    // a c_long: 32 bits on some systems
    tm.tm_gmtoff as i64
}

/// The system's offset from UTC now, as Godot has it.
#[cfg(not(unix))]
fn local_offset(_secs: i64) -> i64 {
    let zone = godot::classes::Time::singleton().get_time_zone_from_system();
    zone.get("bias")
        .map_or(0, |b| b.try_to::<i64>().unwrap_or(0))
        * 60
}

/// Seconds since the epoch as "YYYY-MM-DD HH:MM" (of UTC).
fn format_civil(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

impl Screens {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Screens {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        let mut bg = ColorRect::new_alloc();
        bg.set_color(theme::BG);
        theme::place(&bg, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        // a real page: clicks must not reach the map behind it
        bg.set_mouse_filter(MouseFilter::STOP);
        root.add_child(&bg);
        root.set_visible(false);
        layer.add_child(&root);
        Screens {
            root,
            bg,
            content: None,
            queue,
            current: None,
            form: None,
            end: None,
            title: (Vec::new(), None),
            settings_in_game: false,
            achievements_in_game: false,
            credits_in_game: false,
            prologue: false,
            nav: Nav::default(),
            end_page: None,
            pad: None,
            name_keys: None,
        }
    }

    /// Replace the page with a centred `inner`.
    fn page(&mut self, name: &'static str, inner: &Gd<Control>) {
        self.hide();
        let mut center = CenterContainer::new_alloc();
        theme::full_rect_ignore(&center);
        center.add_child(inner);
        self.root.add_child(&center);
        self.content = Some(center.upcast());
        self.current = Some(name);
        self.bg.set_color(theme::BG);
        self.root.set_visible(true);
    }

    /// The title page: the scene behind it shows through, darkened under
    /// `inner` (the menu), which stands on the left.
    fn title_page(&mut self, inner: &Gd<Control>) {
        self.hide();
        let mut page = Control::new_alloc();
        theme::full_rect_ignore(&page);
        for across in [true, false] {
            let mut shade = TextureRect::new_alloc();
            shade.set_texture(&title_shade(across));
            shade.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            shade.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
            shade.set_mouse_filter(MouseFilter::IGNORE);
            theme::place(&shade, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
            page.add_child(&shade);
        }
        let mut left = CenterContainer::new_alloc();
        left.set_mouse_filter(MouseFilter::IGNORE);
        theme::place(&left, [0.0, 0.0, TITLE_MENU_WIDTH, 1.0], [0.0; 4]);
        left.add_child(inner);
        page.add_child(&left);
        self.root.add_child(&page);
        self.content = Some(page.upcast());
        self.current = Some("title");
        // the page still takes the clicks: none reaches the scene
        self.bg.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        self.root.set_visible(true);
    }

    pub fn show_title(&mut self, saves: &[SavedGame], notice: Option<&str>) {
        self.title = (saves.to_vec(), notice.map(str::to_string));
        let mut col = column();
        // the game's name: not translated
        let name = title_label(GAME_NAME, 72);
        i18n::verbatim(&name);
        col.add_child(&name);
        let mut sub = theme::label(&tr!("title-subtitle"));
        sub.set_horizontal_alignment(HorizontalAlignment::CENTER);
        sub.add_theme_color_override("font_color", theme::TEXT_DIM);
        col.add_child(&sub);
        let mut gap = Control::new_alloc();
        gap.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        col.add_child(&gap);
        let mut items: Vec<Gd<Control>> = Vec::new();
        let mut add = |col: &mut Gd<VBoxContainer>, text: &str, ev: UiEvent| {
            let b = wide(theme::button(text, &self.queue, ev));
            col.add_child(&b);
            items.push(b.upcast());
        };
        add(&mut col, &tr!("title-new-game"), UiEvent::NewGame);
        for s in saves {
            let text = tr!(
                "title-continue",
                name = &s.display_name,
                time = format_time(s.modified)
            );
            add(&mut col, &text, UiEvent::ContinueGame(s.name.clone()));
        }
        add(
            &mut col,
            &tr!("title-achievements"),
            UiEvent::OpenAchievements,
        );
        if self.prologue {
            add(&mut col, &tr!("title-prologue"), UiEvent::ReplayPrologue);
        }
        add(&mut col, &tr!("title-settings"), UiEvent::OpenSettings);
        add(&mut col, &tr!("title-credits"), UiEvent::OpenCredits);
        add(&mut col, &tr!("title-quit"), UiEvent::QuitApp);
        if let Some(n) = notice {
            let mut l = theme::label(n);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            l.add_theme_color_override("font_color", theme::ACCENT);
            l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
            l.set_custom_minimum_size(Vector2::new(460.0, 0.0));
            col.add_child(&l);
        }
        self.form = None;
        self.title_page(&col.upcast());
        // the focus on the newest game to continue, else on a new one
        let at = usize::from(!saves.is_empty());
        self.set_nav(items, at, None, None);
    }

    /// The settings page (`settings_page::SettingsPage`); `in_game`:
    /// opened from a game, Back returns to it. The scene behind shows
    /// through, dimmed: a change of the graphics shows at once.
    pub fn show_settings(&mut self, page: &Gd<Control>, in_game: bool) {
        self.form = None;
        self.settings_in_game = in_game;
        self.page("settings", page);
        self.bg.set_color(theme::BG.with_alpha(SETTINGS_SHADE));
    }

    /// The settings page was opened from a game.
    pub fn settings_in_game(&self) -> bool {
        self.settings_in_game
    }

    /// The credits and licences page (`credits_page::CreditsPage`), over
    /// the scene as the settings are; `in_game`: opened over a game.
    pub fn show_credits(&mut self, page: &Gd<Control>, in_game: bool) {
        self.form = None;
        self.credits_in_game = in_game;
        self.page("credits", page);
        self.bg.set_color(theme::BG.with_alpha(SETTINGS_SHADE));
    }

    /// The credits page was opened over a game.
    pub fn credits_in_game(&self) -> bool {
        self.credits_in_game
    }

    /// The achievements page (`achievement_view::Page`); `in_game`:
    /// opened from a game, Back returns to it.
    pub fn show_achievements(&mut self, page: &Gd<Control>, in_game: bool) {
        self.form = None;
        self.achievements_in_game = in_game;
        self.page("achievements", page);
    }

    /// The achievements page was opened from a game.
    pub fn achievements_in_game(&self) -> bool {
        self.achievements_in_game
    }

    /// Whether the title offers the prologue again (from the next time it
    /// is drawn).
    pub fn set_prologue(&mut self, on: bool) {
        self.prologue = on;
    }

    /// The page again in the language now (the creation form keeps what
    /// was chosen and typed).
    pub fn relang(&mut self) {
        let at = self.nav.at;
        self.relang_page();
        if at < self.nav.items.len() {
            self.nav.at = at;
            self.show_nav();
        }
    }

    fn relang_page(&mut self) {
        match self.current {
            Some("title") => {
                let (saves, notice) = self.title.clone();
                self.show_title(&saves, notice.as_deref());
            }
            Some("creation") => {
                let Some(form) = self.form.clone() else {
                    return;
                };
                let choice = form.choice();
                let typed = form.name.get_text().to_string();
                self.show_creation(form.catalog.clone(), choice.profile, form.taken.clone());
                self.preset_creation(&choice);
                if let Some(f) = self.form.as_mut() {
                    f.name.set_text(&typed);
                    *f.rolled.borrow_mut() = form.rolled.borrow().clone();
                }

                // the name's check again (a saved game of that name)
                push(&self.queue, UiEvent::NameEdited(typed));
            }
            Some("end") => {
                if let Some(summary) = self.end.clone() {
                    self.show_end(&summary);
                }
            }
            _ => {}
        }
    }

    /// The character creation form; `keys`: the keys it offers first (the
    /// settings page's for a new character); `taken`: a saved game's name.
    /// The title scene shows through, its hero the one being made.
    pub fn show_creation(&mut self, catalog: Rc<Catalog>, keys_first: KeyProfile, taken: Taken) {
        let back = theme::button(&tr!("creation-back"), &self.queue, UiEvent::BackToTitle);
        let lang = language_option(&self.queue);
        let mut form = Form::new(catalog, keys_first, taken, &self.queue, lang, back);
        form.set_pad(self.pad.is_some());
        let mut page = Control::new_alloc();
        theme::full_rect_ignore(&page);
        let mut shade = TextureRect::new_alloc();
        shade.set_texture(&title_shade(false));
        shade.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        shade.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
        shade.set_mouse_filter(MouseFilter::IGNORE);
        theme::place(&shade, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        page.add_child(&shade);
        let mut margins = MarginContainer::new_alloc();
        margins.set_mouse_filter(MouseFilter::IGNORE);
        theme::place(&margins, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
            margins.add_theme_constant_override(side, CREATION_EDGE);
        }
        let mut panels = HBoxContainer::new_alloc();
        panels.set_mouse_filter(MouseFilter::IGNORE);
        panels.add_theme_constant_override("separation", 0);
        panels.add_child(&form.left);
        panels.add_child(&form.room);
        panels.add_child(&form.right);
        margins.add_child(&panels);
        page.add_child(&margins);
        let (items, cards, start) = form.nav_items();
        self.hide();
        self.root.add_child(&page);
        self.content = Some(page.upcast());
        self.current = Some("creation");
        // the page still takes the clicks: none reaches the scene
        self.bg.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        self.root.set_visible(true);
        self.form = Some(form);
        // the focus on the role chosen; Start begins the game, Esc or B
        // goes back to the title
        self.set_nav(items, 0, Some(UiEvent::BackToTitle), Some(start));
        self.nav.grid = Some(Grid {
            cards,
            columns: creation::CARD_COLUMNS,
            focus: creation::card_focus(),
        });
        self.nav.at = self.chosen_card().unwrap_or(0);
        self.show_nav();
    }

    /// The creation form shows its keys' row (self-tests: not while a
    /// gamepad plays).
    pub fn creation_keys_shown(&self) -> Option<bool> {
        self.form.as_ref().map(|f| f.keys_shown())
    }

    /// What the creation form has chosen (self-tests).
    pub fn creation_choice(&self) -> Option<CharacterChoice> {
        self.form.as_ref().map(|f| f.choice())
    }

    /// The hero the creation form is making, as the title scene is to
    /// show them, where the form leaves room (None: no form).
    pub fn creation_preview(&self) -> Option<Preview> {
        let form = self.form.as_ref()?;
        let width = self.root.get_viewport_rect().size.x.max(1.0);
        let room = form.room.get_global_rect();
        Some(form.preview((room.position.x + room.size.x / 2.0) / width))
    }

    /// Fill the form (self-tests).
    pub fn preset_creation(&mut self, choice: &CharacterChoice) {
        if let Some(form) = self.form.as_mut() {
            form.preset(choice);
        }
    }

    /// The name would restore a saved game: offer to continue it instead.
    pub fn set_name_taken(&mut self, taken: bool) {
        if let Some(form) = self.form.as_mut() {
            form.set_name_taken(taken);
        }
    }

    /// Why the name cannot be used (None: it can).
    pub fn set_name_error(&mut self, error: Option<&str>) {
        if let (Some(form), Some(e)) = (self.form.as_mut(), error) {
            form.set_name_error(e);
        }
    }

    /// The end page: the stone, the story, the records and the buttons;
    /// the focus on a new game.
    pub fn show_end(&mut self, summary: &EndSummary) {
        let page = end_view::build(summary, &self.queue, self.canvas(), self.pad);
        self.end = Some(summary.clone());
        self.form = None;
        self.hide();
        self.root.add_child(&page.root);
        self.content = Some(page.root.clone());
        self.current = Some("end");
        self.bg.set_color(theme::BG);
        self.root.set_visible(true);
        self.end_page = Some((page.scroll, page.foot, page.panel));
        // Start: a new game; Esc or B: the title
        self.set_nav(page.buttons, 0, Some(UiEvent::BackToTitle), Some(0));
    }

    /// The canvas the pages are laid out on, in design pixels.
    fn canvas(&self) -> Vector2 {
        let s = self.root.get_viewport_rect().size;
        // a headless run without a size has a 64-pixel window
        if s.x < 320.0 || s.y < 240.0 {
            theme::DESIGN
        } else {
            s
        }
    }

    /// The hints follow the input: a gamepad's buttons, or the keys.
    pub fn set_pad(&mut self, pad: Option<PadKind>) {
        if self.pad == pad {
            return;
        }
        self.pad = pad;
        if let Some((_, foot, _)) = self.end_page.as_mut() {
            end_view::show_hints(foot, pad);
        }
        if let Some(form) = self.form.as_mut() {
            form.set_pad(pad.is_some());
            // the focus off a row that hid
            let at = self.nav.at;
            if self
                .nav
                .items
                .get(at)
                .is_some_and(|c| !c.is_visible_in_tree())
            {
                self.nav_step(1);
            }
        }
    }

    /// The end page's body `by` pixels down (up when less than 0); a
    /// screen's worth for `page`.
    fn scroll_end(&mut self, by: f32, page: bool) {
        let Some((scroll, _, _)) = self.end_page.as_mut() else {
            return;
        };
        let step = if page {
            (scroll.get_size().y * 0.8).max(120.0) * by.signum()
        } else {
            by
        };
        let at = scroll.get_v_scroll() as f32 + step;
        scroll.set_v_scroll(at.max(0.0).round() as i32);
    }

    /// The end page's frame and its body's view, and how far the body
    /// scrolls (self-tests).
    pub fn end_rects(&self) -> Option<(Rect2, Rect2, i32)> {
        let (scroll, _, panel) = self
            .end_page
            .as_ref()
            .filter(|_| self.current == Some("end"))?;
        let range = scroll
            .get_v_scroll_bar()
            .map_or(0.0, |b| (b.get_max() - b.get_page()).max(0.0));
        Some((
            panel.get_global_rect(),
            scroll.get_global_rect(),
            range.round() as i32,
        ))
    }

    /// The end page's body scrolled so far (self-tests).
    pub fn end_scrolled(&self) -> Option<i32> {
        let (scroll, _, _) = self.end_page.as_ref()?;
        Some(scroll.get_v_scroll())
    }

    /// The end page's root (self-tests: its words).
    pub fn end_root(&self) -> Option<Gd<Control>> {
        self.content.clone().filter(|_| self.current == Some("end"))
    }

    /// The end page's scrolled body (self-tests).
    pub fn end_body(&self) -> Option<Gd<ScrollContainer>> {
        let (scroll, _, _) = self
            .end_page
            .as_ref()
            .filter(|_| self.current == Some("end"))?;
        Some(scroll.clone())
    }

    /// `can_continue`: the name of a save to offer.
    pub fn show_error(&mut self, what: &str, details: &str, can_continue: Option<&str>) {
        let mut panel = PanelContainer::new_alloc();
        let mut col = column();
        col.add_child(&title_label(&tr!("error-title"), 32));
        let mut l = theme::label(what);
        l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        l.set_custom_minimum_size(Vector2::new(900.0, 0.0));
        col.add_child(&l);
        if !details.trim().is_empty() {
            let mut text = RichTextLabel::new_alloc();
            text.set_focus_mode(FocusMode::NONE);
            text.set_custom_minimum_size(Vector2::new(900.0, 360.0));
            text.add_theme_color_override("default_color", theme::TEXT_DIM);
            text.set_text(details);
            col.add_child(&text);
        }
        let mut buttons = row();
        let mut items: Vec<Gd<Control>> = Vec::new();
        if let Some(name) = can_continue {
            let ev = UiEvent::ContinueGame(name.to_string());
            let text = tr!("error-continue", name = name);
            let b = theme::button(&text, &self.queue, ev);
            buttons.add_child(&b);
            items.push(b.upcast());
        }
        for (text, ev) in [
            (tr!("error-to-title"), UiEvent::BackToTitle),
            (tr!("error-quit"), UiEvent::QuitApp),
        ] {
            let b = theme::button(&text, &self.queue, ev);
            buttons.add_child(&b);
            items.push(b.upcast());
        }
        col.add_child(&buttons);
        panel.add_child(&col);
        self.form = None;
        self.page("error", &panel.upcast());
        self.set_nav(items, 0, Some(UiEvent::BackToTitle), None);
    }

    /// The page's controls the keys and a gamepad move among, the focus on
    /// `at`; `back`: what Esc (B) sends; `start`: the control Start takes.
    fn set_nav(
        &mut self,
        items: Vec<Gd<Control>>,
        at: usize,
        back: Option<UiEvent>,
        start: Option<usize>,
    ) {
        self.nav = Nav {
            items,
            at,
            back,
            start,
            grid: None,
        };
        self.show_nav();
    }

    /// The focus drawn on its control, gold-edged (a card chosen, as the
    /// one the focus is on always is, in every state).
    fn show_nav(&mut self) {
        let look: Gd<StyleBox> = theme::focus_style().upcast();
        let at = self.nav.at;
        let grid = self.nav.grid.as_ref();
        for (i, c) in self.nav.items.iter_mut().enumerate() {
            let card = grid.filter(|g| g.cards.contains(&i));
            let states: &[&str] = if card.is_some() {
                &["normal", "hover", "pressed", "hover_pressed"]
            } else {
                &["normal"]
            };
            for state in states {
                if i == at {
                    c.add_theme_stylebox_override(*state, card.map_or(&look, |g| &g.focus));
                } else {
                    c.remove_theme_stylebox_override(*state);
                }
            }
        }
    }

    /// The card chosen (pressed) in the grid, by its place in the page's
    /// controls.
    fn chosen_card(&self) -> Option<usize> {
        let grid = self.nav.grid.as_ref()?;
        grid.cards.clone().find(|&i| {
            self.nav.items[i]
                .clone()
                .try_cast::<BaseButton>()
                .is_ok_and(|b| b.is_pressed())
        })
    }

    /// In the grid, the focus `by` cards along (`across`: in its row) or
    /// `by` rows; out over its top or bottom edge, on to the controls
    /// before or after it. The card it comes to is chosen.
    fn grid_step(&mut self, by: i32, across: bool) {
        let Some(grid) = self.nav.grid.as_ref() else {
            return;
        };
        let (first, len, cols) = (grid.cards.start, grid.cards.len(), grid.columns);
        let at = self.nav.at - first;
        let to = if across {
            let row = at / cols * cols;
            (at as i32 + by).clamp(row as i32, (row + cols - 1).min(len - 1) as i32)
        } else {
            let to = at as i32 + by * cols as i32;
            if to < 0 {
                self.nav.at = first;
                return self.nav_step(-1);
            }
            if to >= len as i32 {
                // the last row may be short: down to its last card first
                if at / cols < (len - 1) / cols {
                    (len - 1) as i32
                } else {
                    self.nav.at = first + len - 1;
                    return self.nav_step(1);
                }
            } else {
                to
            }
        };
        self.nav.at = first + to as usize;
        if let Ok(mut card) = self.nav.items[self.nav.at].clone().try_cast::<Button>() {
            card.emit_signal("pressed", &[]);
        }
        self.show_nav();
    }

    /// A key or a gamepad's button on the page; false: not the page's.
    pub fn nav_input(&mut self, input: NavInput) -> bool {
        if self.name_keys.is_some() {
            self.name_keys_input(input);
            return true;
        }
        let Some(focused) = self.nav.items.get(self.nav.at).cloned() else {
            return false;
        };
        let at = self.nav.at;
        let in_grid = self
            .nav
            .grid
            .as_ref()
            .is_some_and(|g| g.cards.contains(&at));
        let end = self.current == Some("end");
        // Tab is down and up but where the arrows scroll
        let input = match input {
            NavInput::Next if !end => NavInput::Down,
            NavInput::Prev if !end => NavInput::Up,
            other => other,
        };
        match input {
            // the end page's body scrolls; its buttons are in a row
            NavInput::Up if end => self.scroll_end(-END_STEP, false),
            NavInput::Down if end => self.scroll_end(END_STEP, false),
            NavInput::PageUp => self.scroll_end(-1.0, true),
            NavInput::PageDown => self.scroll_end(1.0, true),
            NavInput::Prev => self.nav_step(-1),
            NavInput::Next => self.nav_step(1),
            NavInput::Up if in_grid => self.grid_step(-1, false),
            NavInput::Down if in_grid => self.grid_step(1, false),
            NavInput::Left if in_grid => self.grid_step(-1, true),
            NavInput::Right if in_grid => self.grid_step(1, true),
            // the card chosen: on to what comes after the cards
            NavInput::Accept if in_grid => {
                if let Some(g) = self.nav.grid.as_ref() {
                    self.nav.at = g.cards.end - 1;
                }
                self.nav_step(1);
            }
            NavInput::Up => self.nav_step(-1),
            NavInput::Down => self.nav_step(1),
            NavInput::Left | NavInput::Right => {
                let by = if input == NavInput::Left { -1 } else { 1 };
                match focused.try_cast::<OptionButton>() {
                    // a list's choice, one along
                    Ok(mut ob) => cycle(&mut ob, by),
                    Err(_) => self.nav_step(by),
                }
            }
            NavInput::Accept => self.nav_take(self.nav.at),
            // the creation form: back to the cards first (the card and
            // the name stay), then the title
            NavInput::Back if self.nav.grid.is_some() && !in_grid => {
                if let Ok(mut field) = focused.try_cast::<LineEdit>() {
                    field.release_focus();
                }
                let first = self.nav.grid.as_ref().map_or(0, |g| g.cards.start);
                self.nav.at = self.chosen_card().unwrap_or(first);
                self.show_nav();
            }
            NavInput::Back => match self.nav.back.clone() {
                Some(ev) => push(&self.queue, ev),
                None => return false,
            },
            NavInput::Start => self.nav_take(self.nav.start.unwrap_or(self.nav.at)),
            NavInput::Layout => return false,
        }
        true
    }

    /// The on-screen keyboard over the creation form, for `field` (the
    /// name): at the bottom, over the room where the hero stands, in the
    /// interface's letters first.
    fn open_name_keys(&mut self, field: Gd<LineEdit>) {
        let Some(mut page) = self.content.clone() else {
            return;
        };
        let mut panel = theme::framed(theme::Frame::Panel);
        theme::place(&panel, [0.5, 1.0, 0.5, 1.0], [0.0, 0.0, 0.0, -24.0]);
        panel.set_h_grow_direction(GrowDirection::BOTH);
        panel.set_v_grow_direction(GrowDirection::BEGIN);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 8);
        let mut shown = theme::styled_label("", Face::Title, 26, theme::ACCENT);
        shown.set_horizontal_alignment(HorizontalAlignment::CENTER);
        // a name: the same in every language
        i18n::verbatim(&shown);
        col.add_child(&shown);
        let q = self.queue.clone();
        let mut osk = Osk::new(&mut col, move |r, c| push(&q, UiEvent::NameKey(r, c)));
        if i18n::lang() != Lang::En {
            osk.switch();
        }
        osk.show();
        // what the pad's buttons do on it
        use crate::gamepad::PadButton::*;
        let hints: [(&[PadButton], &str); 5] = [
            (&[Up, Down, Left, Right], "hint-letter"),
            (&[A], "hint-type"),
            (&[B], "hint-erase"),
            (&[Y], "hint-layout"),
            (&[Start], "hint-ok"),
        ];
        let parts: Vec<(&[PadButton], String)> =
            hints.iter().map(|(b, key)| (*b, i18n::tr(key))).collect();
        let mut row = pad_view::buttons_row(self.pad.unwrap_or_default(), &parts, 15);
        row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        col.add_child(&row);
        panel.add_child(&col);
        page.add_child(&panel);
        // Steam's own keyboard too, which types into the field, as a text
        // question asks for it
        let steam = std::env::var("SteamDeck").is_ok_and(|v| v == "1")
            || std::env::var("SteamTenfoot").is_ok();
        if steam {
            godot::classes::Os::singleton().shell_open("steam://open/keyboard");
        }
        let mut keys = NameKeys {
            panel,
            osk,
            shown,
            field,
        };
        keys.show_name();
        self.name_keys = Some(keys);
    }

    /// The name's keyboard is up (self-tests).
    pub fn name_keys_open(&self) -> bool {
        self.name_keys.is_some()
    }

    /// The focus is on the creation form's cards (self-tests).
    pub fn on_cards(&self) -> bool {
        self.nav
            .grid
            .as_ref()
            .is_some_and(|g| g.cards.contains(&self.nav.at))
    }

    /// The name's keyboard put away (keys came instead of the pad).
    pub fn close_name_keys(&mut self) {
        if self.name_keys.is_none() {
            return;
        }
        if let Some(mut keys) = self.name_keys.take() {
            keys.panel.queue_free();
        }
        self.show_nav();
    }

    /// A key of the name's keyboard clicked: as if A typed it.
    pub fn name_key(&mut self, row: usize, col: usize) {
        if let Some(keys) = self.name_keys.as_mut() {
            keys.osk.at(row, col);
            keys.osk.show();
            self.name_keys_input(NavInput::Accept);
        }
    }

    /// The pad (or a key) on the name's keyboard.
    fn name_keys_input(&mut self, input: NavInput) {
        let Some(keys) = self.name_keys.as_mut() else {
            return;
        };
        let text = keys.field.get_text().to_string();
        let mut erased = text.clone();
        erased.pop();
        let typed = match input {
            NavInput::Up => return keys.osk.step(0, -1),
            NavInput::Down => return keys.osk.step(0, 1),
            NavInput::Left => return keys.osk.step(-1, 0),
            NavInput::Right => return keys.osk.step(1, 0),
            NavInput::Layout => return keys.osk.switch(),
            NavInput::Start => return self.close_name_keys(),
            NavInput::Back if text.is_empty() => return self.close_name_keys(),
            NavInput::Back => erased,
            NavInput::Accept => match keys.osk.current() {
                OskKey::Char(c) => format!("{text}{c}"),
                OskKey::Space => format!("{text} "),
                OskKey::Back => erased,
                OskKey::Shift => return keys.osk.shift(),
                OskKey::Layout => return keys.osk.switch(),
                OskKey::Ok => return self.close_name_keys(),
            },
            _ => return,
        };
        // no more than NetHack keeps
        if typed.len() > crate::names::MAX_BYTES {
            return;
        }
        keys.field.set_text(&typed);
        keys.field.set_caret_column(typed.chars().count() as i32);
        // as typing would: the form follows (a rolled name no longer
        // follows the gender; a saved game's name offers to continue)
        keys.field
            .emit_signal("text_changed", &[GString::from(&typed).to_variant()]);
        keys.show_name();
    }

    /// The focus `by` along, round, past what is hidden or disabled; a
    /// field it leaves gives up the keys.
    fn nav_step(&mut self, by: i32) {
        let n = self.nav.items.len() as i32;
        let usable = |c: &Gd<Control>| {
            c.is_visible_in_tree()
                && !c
                    .clone()
                    .try_cast::<godot::classes::BaseButton>()
                    .is_ok_and(|b| b.is_disabled())
        };
        let mut i = self.nav.at as i32;
        for _ in 0..n {
            i = (i + by).rem_euclid(n);
            if usable(&self.nav.items[i as usize]) {
                break;
            }
        }
        if let Ok(mut field) = self.nav.items[self.nav.at].clone().try_cast::<LineEdit>() {
            field.release_focus();
        }
        // into the grid: on the card chosen
        let cards = self.nav.grid.as_ref().map(|g| g.cards.clone());
        if let Some(cards) = cards
            && cards.contains(&(i as usize))
            && !cards.contains(&self.nav.at)
        {
            i = self.chosen_card().unwrap_or(cards.start) as i32;
        }
        self.nav.at = i as usize;
        self.show_nav();
    }

    /// The control `i` taken: a button pressed, a list opened, a field
    /// given the keys.
    fn nav_take(&mut self, i: usize) {
        let Some(c) = self.nav.items.get(i).cloned() else {
            return;
        };
        if let Ok(mut ob) = c.clone().try_cast::<OptionButton>() {
            ob.call_deferred("show_popup", &[]);
        } else if let Ok(mut b) = c.clone().try_cast::<Button>() {
            if !b.is_disabled() {
                b.emit_signal("pressed", &[]);
            }
        } else if let Ok(mut field) = c.try_cast::<LineEdit>() {
            if self.pad.is_some() {
                // no keys to type with: the keyboard on the screen
                self.open_name_keys(field);
                return;
            }
            // the keys typed go in at once (a field only focused takes
            // none until edited)
            field.grab_focus();
            field.edit();
        }
    }

    /// The keys are a field's now (a name being typed).
    pub fn typing(&self) -> bool {
        self.nav
            .items
            .get(self.nav.at)
            .and_then(|c| c.clone().try_cast::<LineEdit>().ok())
            .is_some_and(|f| f.has_focus())
    }

    /// The field the focus is on gives up the keys (the focus stays).
    pub fn stop_typing(&mut self) {
        if let Some(mut field) = self
            .nav
            .items
            .get(self.nav.at)
            .and_then(|c| c.clone().try_cast::<LineEdit>().ok())
        {
            field.release_focus();
        }
    }

    /// A list of the page is open (its own keys).
    pub fn list_open(&self) -> bool {
        self.nav.items.iter().any(|c| {
            c.clone()
                .try_cast::<OptionButton>()
                .ok()
                .and_then(|ob| ob.get_popup())
                .is_some_and(|p| p.is_visible())
        })
    }

    /// The words of the control the focus is on, an icon's tip
    /// (self-tests).
    pub fn nav_focused(&self) -> Option<String> {
        let c = self.nav.items.get(self.nav.at)?.clone();
        Some(match c.clone().try_cast::<Button>() {
            Ok(b) if b.get_text().is_empty() => crate::tips::tip_of(&c).unwrap_or_default(),
            Ok(b) => b.get_text().to_string(),
            Err(_) => c
                .try_cast::<LineEdit>()
                .map(|f| f.get_placeholder().to_string())
                .unwrap_or_default(),
        })
    }

    pub fn hide(&mut self) {
        if let Some(mut c) = self.content.take() {
            c.queue_free();
        }
        self.end_page = None;
        self.name_keys = None;
        self.nav = Nav::default();
        self.current = None;
        self.form = None;
        self.root.set_visible(false);
    }

    /// "title", "settings", "achievements", "creation", "end", "error" or
    /// None.
    pub fn current(&self) -> Option<&'static str> {
        self.current
    }

    /// What the end screen shows.
    pub fn end_summary(&self) -> Option<&EndSummary> {
        self.end.as_ref().filter(|_| self.current == Some("end"))
    }

    /// Is there a button with this text on the page? (self-tests)
    pub fn has_button(&self, text: &str) -> bool {
        fn walk(node: &Gd<Node>, text: &str) -> bool {
            if let Ok(b) = node.clone().try_cast::<Button>()
                && b.get_text().to_string().starts_with(text)
            {
                return true;
            }
            node.get_children().iter_shared().any(|c| walk(&c, text))
        }
        self.content
            .as_ref()
            .is_some_and(|c| walk(&c.clone().upcast(), text))
    }

    /// Is there a label with this text on the page? (self-tests)
    pub fn has_label(&self, text: &str) -> bool {
        fn walk(node: &Gd<Node>, text: &str) -> bool {
            if let Ok(l) = node.clone().try_cast::<Label>()
                && l.get_text() == text
            {
                return true;
            }
            node.get_children().iter_shared().any(|c| walk(&c, text))
        }
        self.content
            .as_ref()
            .is_some_and(|c| walk(&c.clone().upcast(), text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn times_format_as_dates() {
        assert_eq!(format_civil(1_768_694_400), "2026-01-18 00:00");
        assert_eq!(format_civil(0), "1970-01-01 00:00");
        assert_eq!(format_civil(951_782_400 + 3_661), "2000-02-29 01:01");
        assert_eq!(format_civil(-60), "1969-12-31 23:59");
    }

    #[test]
    #[cfg(unix)]
    fn times_show_in_the_player_s_time_zone() {
        let secs = 1_768_694_400;
        let t = UNIX_EPOCH + Duration::from_secs(secs as u64);
        let offset = local_offset(secs);
        assert!(offset.abs() <= 14 * 3600, "{offset}");
        assert_eq!(format_time(t), format_civil(secs + offset));
        // libc's own reading of it
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let t = secs as libc::time_t;
        assert!(!unsafe { libc::localtime_r(&t, &mut tm) }.is_null());
        assert_eq!(
            format_time(UNIX_EPOCH + Duration::from_secs(secs as u64)),
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min
            )
        );
    }
}
