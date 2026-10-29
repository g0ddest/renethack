//! Full-screen pages outside a game: title, settings, character creation,
//! the end of a game and errors.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use godot::classes::control::{FocusMode, MouseFilter};
use godot::classes::{
    BaseButton, Button, CanvasLayer, CenterContainer, ColorRect, Control, Gradient,
    GradientTexture2D, HBoxContainer, Label, LineEdit, MarginContainer, OptionButton,
    PanelContainer, RichTextLabel, StyleBox, TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_link::SavedGame;
use nh_protocol::Catalog;
use nh_world::KeyProfile;

pub use crate::creation::Taken;
use crate::creation::{self, Form};
use crate::i18n::{self, EngineKind, Lang};
use crate::layouts;
use crate::theme::{self, bbcode_escape, hex};
use crate::title_scene::Preview;
use crate::tombstone;
use crate::tr;
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, push};

/// What the end screen shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndSummary {
    pub last_messages: Vec<String>,
    /// The summary and tombstone window (empty if the player skipped it).
    pub text: Vec<String>,
    /// The top ten list.
    pub scores: Vec<String>,
}

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

/// The game's name on the title: a name, the same in every language.
const GAME_NAME: &str = "renethack";

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

/// Lines without blank ones at either end, and at most one blank line
/// between paragraphs.
fn paragraphs<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for l in lines {
        if l.trim().is_empty() && out.last().is_none_or(|p| p.trim().is_empty()) {
            continue;
        }
        out.push(l);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
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
        let form = Form::new(catalog, keys_first, taken, &self.queue, lang, back);
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

    pub fn show_end(&mut self, summary: &EndSummary) {
        let mut panel = PanelContainer::new_alloc();
        let mut col = column();
        col.add_child(&title_label(&tr!("end-title"), 32));
        // the tombstone is drawn: its lines apart from the window's others
        let stone = layouts::tombstone(&summary.text);
        let rest: Vec<&str> = match &stone {
            Some((_, at)) => summary.text[..at.start]
                .iter()
                .chain(&summary.text[at.end..])
                .map(String::as_str)
                .collect(),
            None => summary.text.iter().map(String::as_str).collect(),
        };
        let rest = paragraphs(&rest);
        let mut text = RichTextLabel::new_alloc();
        text.set_use_bbcode(true);
        text.set_focus_mode(FocusMode::NONE);
        let width = if stone.is_some() { 700.0 } else { 980.0 };
        text.set_custom_minimum_size(Vector2::new(width, 600.0));
        // the top ten's columns line up in the monospace face
        text.add_theme_font_override("mono_font", &theme::font(theme::Face::Mono));
        text.add_theme_font_size_override("mono_font_size", theme::MONO_SIZE - 1);
        let mut parts: Vec<String> = Vec::new();
        if !summary.last_messages.is_empty() {
            let msgs: Vec<String> = summary
                .last_messages
                .iter()
                .map(|m| bbcode_escape(&i18n::engine(EngineKind::Message, m)))
                .collect();
            parts.push(format!(
                "[color={}]{}[/color]",
                hex(theme::TEXT_DIM),
                msgs.join("\n")
            ));
        }
        if !rest.is_empty() {
            let shown = i18n::engine_window(&rest)
                .unwrap_or_else(|| rest.iter().map(|l| l.to_string()).collect());
            let lines: Vec<String> = shown.iter().map(|l| bbcode_escape(l)).collect();
            parts.push(lines.join("\n"));
        }
        if !summary.scores.is_empty() {
            // the list whole: an entry runs over lines mid-sentence
            let english: Vec<&str> = summary.scores.iter().map(String::as_str).collect();
            let lines: Vec<String> = i18n::engine_window(&english)
                .unwrap_or_else(|| summary.scores.clone())
                .iter()
                .map(|l| bbcode_escape(l))
                .collect();
            parts.push(format!(
                "[code][color={}]{}[/color][/code]",
                hex(theme::ACCENT),
                lines.join("\n")
            ));
        }
        text.set_text(&parts.join("\n\n"));
        match &stone {
            Some((t, _)) => {
                let mut body = HBoxContainer::new_alloc();
                body.add_theme_constant_override("separation", 28);
                body.add_child(&tombstone::view(t));
                body.add_child(&text);
                col.add_child(&body);
            }
            None => col.add_child(&text),
        }
        let mut buttons = row();
        let new_game = theme::button(&tr!("end-new-game"), &self.queue, UiEvent::NewGame);
        buttons.add_child(&new_game);
        let to_title = theme::button(&tr!("end-to-title"), &self.queue, UiEvent::BackToTitle);
        buttons.add_child(&to_title);
        col.add_child(&buttons);
        panel.add_child(&col);
        self.end = Some(summary.clone());
        self.form = None;
        self.page("end", &panel.upcast());
        // Start: a new game; Esc or B: the title
        let items = vec![new_game.upcast(), to_title.upcast()];
        self.set_nav(items, 0, Some(UiEvent::BackToTitle), Some(0));
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
        let Some(focused) = self.nav.items.get(self.nav.at).cloned() else {
            return false;
        };
        let at = self.nav.at;
        let in_grid = self
            .nav
            .grid
            .as_ref()
            .is_some_and(|g| g.cards.contains(&at));
        match input {
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
            NavInput::Back => match self.nav.back.clone() {
                Some(ev) => push(&self.queue, ev),
                None => return false,
            },
            NavInput::Start => self.nav_take(self.nav.start.unwrap_or(self.nav.at)),
        }
        true
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
