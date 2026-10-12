//! The settings page, from the title's button and the HUD's gear: the
//! window and how often it is drawn, the interface, a section at a time.
//! A row is a setting, its value between arrows: ↑↓ (the d-pad) choose a
//! row, ←→ change it, Enter (A) steps it on, Tab or PgUp/PgDn (the
//! bumpers) turn to another section, Esc (B) goes back; the mouse clicks
//! the arrows and the tabs. A change applies at once and is kept in the
//! profile: the game does both (`RenethackGame::settings_step`), then
//! shows the page what is now (`SettingsPage::show`).

use std::cell::RefCell;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::image::Format;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, Image, ImageTexture, Label, PanelContainer,
    StyleBoxFlat, Texture2D, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::{FPS_CAPS, Key, KeyInput, Settings, WindowMode, step_through, step_window_size};

use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, Lang};
use crate::pad_view;
use crate::theme::{self, Face, Frame};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// The page's sections, in the order of their tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Display,
    Interface,
}

impl Section {
    pub const ALL: [Section; 2] = [Section::Display, Section::Interface];

    fn name(self) -> String {
        match self {
            Section::Display => tr!("settings-display"),
            Section::Interface => tr!("settings-interface"),
        }
    }

    /// Its rows, top to bottom.
    pub fn fields(self) -> &'static [Field] {
        match self {
            Section::Display => &[
                Field::WindowMode,
                Field::WindowSize,
                Field::Vsync,
                Field::FpsCap,
            ],
            Section::Interface => &[Field::Language],
        }
    }
}

/// A row's setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WindowMode,
    WindowSize,
    Vsync,
    FpsCap,
    Language,
}

impl Field {
    fn name(self) -> String {
        match self {
            Field::WindowMode => tr!("settings-window-mode"),
            Field::WindowSize => tr!("settings-window-size"),
            Field::Vsync => tr!("settings-vsync"),
            Field::FpsCap => tr!("settings-fps-cap"),
            Field::Language => tr!("settings-language"),
        }
    }

    /// What it does, under the rows while it has the focus.
    fn about(self, v: &View) -> String {
        match self {
            Field::WindowMode => tr!("settings-window-mode-about"),
            Field::WindowSize if !resizable(v) => tr!("settings-window-size-fullscreen"),
            Field::WindowSize => tr!("settings-window-size-about"),
            Field::Vsync => tr!("settings-vsync-about"),
            Field::FpsCap => tr!("settings-fps-cap-about"),
            Field::Language => tr!("settings-language-about"),
        }
    }
}

/// What the page shows: the settings, and what they depend on.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub settings: Settings,
    pub lang: Lang,
    /// The window's size now: the size row shows it while the window
    /// keeps the size it opened with.
    pub window: (u32, u32),
    /// The screen's: the window sizes offered fit it.
    pub screen: (u32, u32),
}

/// The window size is the player's to choose (in a window only).
fn resizable(v: &View) -> bool {
    v.settings.display.mode == WindowMode::Windowed
}

/// A row's value as the page shows it.
pub fn value_text(field: Field, v: &View) -> String {
    let d = &v.settings.display;
    match field {
        Field::WindowMode => match d.mode {
            WindowMode::Windowed => tr!("settings-window-windowed"),
            WindowMode::Borderless => tr!("settings-window-borderless"),
            WindowMode::Exclusive => tr!("settings-window-exclusive"),
        },
        Field::WindowSize => {
            let (w, h) = if resizable(v) {
                d.size.unwrap_or(v.window)
            } else {
                v.window
            };
            tr!("settings-size", width = w, height = h)
        }
        Field::Vsync if d.vsync => tr!("settings-on"),
        Field::Vsync => tr!("settings-off"),
        Field::FpsCap if d.fps_cap == 0 => tr!("settings-fps-none"),
        Field::FpsCap => tr!("settings-fps", fps = d.fps_cap),
        Field::Language => v.lang.name().to_string(),
    }
}

/// What a step of a row changes.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Settings(Settings),
    Lang(Lang),
}

/// `field` a step on (`dir` 1) or back (-1); `wrap`: past the last value
/// to the first (Enter, A). None: nothing changes (a row that cannot
/// change now, or at its end).
pub fn step(field: Field, v: &View, dir: i32, wrap: bool) -> Option<Change> {
    let mut s = v.settings;
    let d = &mut s.display;
    match field {
        Field::WindowMode => d.mode = step_through(&WindowMode::ALL, d.mode, dir, wrap),
        Field::WindowSize => {
            if !resizable(v) {
                return None;
            }
            let now = d.size.unwrap_or(v.window);
            d.size = Some(step_window_size(now, v.screen, dir, wrap));
        }
        Field::Vsync => d.vsync = step_through(&[false, true], d.vsync, dir, wrap),
        Field::FpsCap => d.fps_cap = step_through(&FPS_CAPS, d.fps_cap, dir, wrap),
        Field::Language => {
            let lang = step_through(&Lang::ALL, v.lang, dir, wrap);
            return (lang != v.lang).then_some(Change::Lang(lang));
        }
    }
    (s != v.settings).then_some(Change::Settings(s))
}

/// What the page's widgets tell the game.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsInput {
    /// A section's tab clicked.
    Tab(Section),
    /// A row hovered or clicked: it has the focus.
    Focus(Field),
    /// A row's arrow clicked: its value a step back (-1) or on (1).
    Step(Field, i32),
}

/// What a key on the page asks of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    /// A step of the row with the focus (see `step`).
    Step(Field, i32, bool),
    Back,
}

/// A row: its button (the focus's frame, the mouse's target), its name,
/// its value between arrows.
struct Row {
    field: Field,
    button: Gd<Button>,
    name: Gd<Label>,
    value: Gd<Label>,
    left: Gd<Button>,
    right: Gd<Button>,
}

/// Row height, and the gap between rows, in design pixels.
const ROW_H: f32 = 44.0;
const ROW_GAP: i32 = 4;
/// The value's column between its arrows, and its text's size.
const VALUE_W: f32 = 340.0;
const VALUE_SIZE: i32 = 19;
/// The page's width inside its frame.
const PAGE_W: f32 = 1000.0;

pub struct SettingsPage {
    root: Gd<PanelContainer>,
    tabs: Vec<(Section, Gd<Button>)>,
    bodies: Vec<(Section, Gd<VBoxContainer>)>,
    rows: Vec<Row>,
    about: Gd<Label>,
    section: Section,
    /// The row with the focus, among its section's.
    focus: usize,
    view: View,
    row_normal: Gd<StyleBoxFlat>,
    row_current: Gd<StyleBoxFlat>,
    tab_normal: Gd<StyleBoxFlat>,
    tab_current: Gd<StyleBoxFlat>,
    /// The gamepad whose buttons the hints show (None: the keyboard's).
    pad: Option<PadKind>,
}

fn flat(bg: Color, border: Option<(Side, i32, Color)>) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_corner_radius_all(3);
    sb.set_content_margin_all(0.0);
    if let Some((side, width, color)) = border {
        sb.set_border_width(side, width);
        sb.set_border_color(color);
    }
    sb
}

/// Every state of a button drawn with `sb`.
fn style_all(b: &mut Gd<Button>, sb: &Gd<StyleBoxFlat>) {
    for state in [
        "normal",
        "hover",
        "pressed",
        "hover_pressed",
        "disabled",
        "focus",
    ] {
        b.add_theme_stylebox_override(state, sb);
    }
}

thread_local! {
    /// The arrows' triangles, pointing left and right.
    static ARROWS: RefCell<Option<[Gd<Texture2D>; 2]>> = const { RefCell::new(None) };
}

/// The arrows' triangle on screen, design pixels across (painted at three
/// times that: crisp at any UI scale).
const ARROW_W: i32 = 12;

/// A solid triangle pointing left or right, white (its button tints it),
/// its edges smoothed.
fn triangle(right: bool) -> Option<Gd<Texture2D>> {
    const W: usize = 36;
    const H: usize = 48;
    const SS: usize = 4;
    let mut px = vec![0u8; W * H * 4];
    let (w, h) = (W as f32, H as f32);
    for y in 0..H {
        for x in 0..W {
            let mut inside = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // from the base (u 0) to the point (u 1)
                    let u = if right { fx / w } else { 1.0 - fx / w };
                    if (fy - h / 2.0).abs() <= (1.0 - u) * h / 2.0 {
                        inside += 1;
                    }
                }
            }
            let i = (y * W + x) * 4;
            px[i..i + 4].copy_from_slice(&[255, 255, 255, (inside * 255 / (SS * SS)) as u8]);
        }
    }
    let image = Image::create_from_data(
        W as i32,
        H as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(px.as_slice()),
    )?;
    Some(ImageTexture::create_from_image(&image)?.upcast())
}

/// A row's arrow, ◀ or ▶ (`right`), in gold.
fn arrow(right: bool, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_flat(true);
    b.set_focus_mode(FocusMode::NONE);
    b.set_custom_minimum_size(Vector2::new(40.0, 36.0));
    let icons = ARROWS.with(|a| {
        a.borrow_mut()
            .get_or_insert_with(|| {
                let blank = || -> Gd<Texture2D> { ImageTexture::new_gd().upcast() };
                [
                    triangle(false).unwrap_or_else(blank),
                    triangle(true).unwrap_or_else(blank),
                ]
            })
            .clone()
    });
    b.set_button_icon(&icons[usize::from(right)]);
    b.set_icon_alignment(HorizontalAlignment::CENTER);
    b.add_theme_constant_override("icon_max_width", ARROW_W);
    for (state, color) in [
        ("icon_normal_color", theme::GOLD),
        ("icon_focus_color", theme::GOLD),
        ("icon_hover_color", theme::GOLD_BRIGHT),
        ("icon_pressed_color", theme::GOLD_BRIGHT),
        ("icon_hover_pressed_color", theme::GOLD_BRIGHT),
        ("icon_disabled_color", theme::TEXT_OFF),
    ] {
        b.add_theme_color_override(state, color);
    }
    let q = queue.clone();
    b.signals().pressed().connect(move || push(&q, ev.clone()));
    b
}

/// Let the arrows go while Godot still runs (`icons::clear`).
pub fn clear() {
    ARROWS.with(|a| a.borrow_mut().take());
}

impl SettingsPage {
    /// The page showing `view`, open on `section` with the focus on its
    /// `focus`-th row; the hints for a gamepad of `pad`'s kind, else for
    /// the keyboard.
    pub fn new(
        view: View,
        queue: &UiQueue,
        section: Section,
        focus: usize,
        pad: Option<PadKind>,
    ) -> SettingsPage {
        let mut root = theme::framed(Frame::Panel);
        root.set_mouse_filter(MouseFilter::STOP);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 12);
        col.set_custom_minimum_size(Vector2::new(PAGE_W, 0.0));
        let title = theme::styled_label(&tr!("settings-title"), Face::Title, 32, theme::ACCENT);
        col.add_child(&title);

        // the sections' tabs, the current one underlined in gold
        let tab_normal = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), None);
        let tab_current = flat(
            Color {
                a: 0.12,
                ..theme::ACCENT
            },
            Some((Side::BOTTOM, 2, theme::GOLD_BRIGHT)),
        );
        let mut tab_row = HBoxContainer::new_alloc();
        tab_row.add_theme_constant_override("separation", 6);
        let mut tabs = Vec::new();
        for s in Section::ALL {
            let mut b = Button::new_alloc();
            b.set_text(&s.name());
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(150.0, 40.0));
            b.add_theme_font_override("font", &theme::font(Face::Title));
            b.add_theme_font_size_override("font_size", 24);
            b.add_theme_color_override("font_hover_color", theme::TEXT);
            let q = queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::Settings(SettingsInput::Tab(s))));
            tab_row.add_child(&b);
            tabs.push((s, b));
        }
        col.add_child(&tab_row);
        let mut rule = ColorRect::new_alloc();
        rule.set_color(Color {
            a: 0.6,
            ..theme::GOLD_DIM
        });
        rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
        rule.set_mouse_filter(MouseFilter::IGNORE);
        col.add_child(&rule);

        // the rows, each section's in a body of its own: one shows
        let row_normal = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), None);
        let row_current = flat(
            Color {
                a: 0.16,
                ..theme::ACCENT
            },
            Some((Side::LEFT, 3, theme::GOLD_BRIGHT)),
        );
        let most = Section::ALL
            .iter()
            .map(|s| s.fields().len())
            .max()
            .unwrap_or(1) as f32;
        let mut holder = VBoxContainer::new_alloc();
        holder.set_custom_minimum_size(Vector2::new(0.0, most * (ROW_H + ROW_GAP as f32)));
        let mut bodies = Vec::new();
        let mut rows = Vec::new();
        for s in Section::ALL {
            let mut body = VBoxContainer::new_alloc();
            body.add_theme_constant_override("separation", ROW_GAP);
            for &field in s.fields() {
                let row = Self::row(field, queue, &row_normal);
                body.add_child(&row.button);
                rows.push(row);
            }
            holder.add_child(&body);
            bodies.push((s, body));
        }
        col.add_child(&holder);

        let mut about = theme::styled_label("", Face::Body, 17, theme::TEXT_DIM);
        about.set_autowrap_mode(AutowrapMode::WORD_SMART);
        about.set_custom_minimum_size(Vector2::new(PAGE_W, 52.0));
        about.set_vertical_alignment(VerticalAlignment::TOP);
        col.add_child(&about);

        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 18);
        let mut hint = Self::hint(pad);
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        foot.add_child(&hint);
        foot.add_child(&theme::button(
            &tr!("settings-back"),
            queue,
            UiEvent::CloseSettings,
        ));
        col.add_child(&foot);
        root.add_child(&col);

        let mut page = SettingsPage {
            root,
            tabs,
            bodies,
            rows,
            about,
            section,
            focus,
            view: view.clone(),
            row_normal,
            row_current,
            tab_normal,
            tab_current,
            pad,
        };
        page.show(&view);
        page.turn_to(section, focus);
        page
    }

    /// A row of `field`: its name on the left, its value between arrows
    /// on the right.
    fn row(field: Field, queue: &UiQueue, normal: &Gd<StyleBoxFlat>) -> Row {
        let mut button = Button::new_alloc();
        button.set_focus_mode(FocusMode::NONE);
        button.set_custom_minimum_size(Vector2::new(0.0, ROW_H));
        style_all(&mut button, normal);
        let q = queue.clone();
        button
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::Settings(SettingsInput::Focus(field))));
        let q = queue.clone();
        button
            .signals()
            .mouse_entered()
            .connect(move || push(&q, UiEvent::Settings(SettingsInput::Focus(field))));
        let mut line = HBoxContainer::new_alloc();
        line.set_mouse_filter(MouseFilter::IGNORE);
        line.add_theme_constant_override("separation", 6);
        theme::place(&line, [0.0, 0.0, 1.0, 1.0], [18.0, 0.0, -10.0, 0.0]);
        let mut name = theme::styled_label(&field.name(), Face::Body, 19, theme::TEXT);
        name.set_vertical_alignment(VerticalAlignment::CENTER);
        name.set_h_size_flags(SizeFlags::EXPAND_FILL);
        line.add_child(&name);
        let left = arrow(
            false,
            queue,
            UiEvent::Settings(SettingsInput::Step(field, -1)),
        );
        line.add_child(&left);
        let mut value = theme::styled_label("", Face::BodyBold, VALUE_SIZE, theme::GOLD_BRIGHT);
        value.set_horizontal_alignment(HorizontalAlignment::CENTER);
        value.set_vertical_alignment(VerticalAlignment::CENTER);
        value.set_custom_minimum_size(Vector2::new(VALUE_W, 0.0));
        value.set_clip_text(true);
        if field == Field::Language {
            // each language in its own name, whatever the language now
            i18n::verbatim(&value);
        }
        line.add_child(&value);
        let right = arrow(
            true,
            queue,
            UiEvent::Settings(SettingsInput::Step(field, 1)),
        );
        line.add_child(&right);
        button.add_child(&line);
        Row {
            field,
            button,
            name,
            value,
            left,
            right,
        }
    }

    /// The keys under the page: the keyboard's, or a gamepad's buttons.
    fn hint(pad: Option<PadKind>) -> Gd<Control> {
        let Some(kind) = pad else {
            let mut l = theme::styled_label(
                &i18n::whole_parts(&tr!("settings-hint")),
                Face::Body,
                16,
                theme::TEXT_DIM,
            );
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            return l.upcast();
        };
        use PadButton::*;
        let mut strip = HBoxContainer::new_alloc();
        strip.set_mouse_filter(MouseFilter::IGNORE);
        strip.add_theme_constant_override("separation", 16);
        let parts: [(&[PadButton], String); 4] = [
            (&[Up, Down], tr!("hint-choose")),
            (&[Left, Right], tr!("hint-change")),
            (&[Lb, Rb], tr!("hint-section")),
            (&[B], tr!("hint-back")),
        ];
        for (buttons, what) in parts {
            let mut item = HBoxContainer::new_alloc();
            item.set_mouse_filter(MouseFilter::IGNORE);
            item.add_theme_constant_override("separation", 4);
            for &b in buttons {
                item.add_child(&pad_view::medallion(kind, b));
            }
            let mut l = theme::styled_label(&what, Face::Body, 16, theme::TEXT_DIM);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            item.add_child(&l);
            strip.add_child(&item);
        }
        strip.upcast()
    }

    /// What the screens show.
    pub fn root(&self) -> Gd<Control> {
        self.root.clone().upcast()
    }

    /// The values of `view` in the rows; a row that cannot change now
    /// dimmed, its arrows gone.
    pub fn show(&mut self, view: &View) {
        self.view = view.clone();
        for r in &mut self.rows {
            r.value.set_text(&value_text(r.field, view));
            let live = r.field != Field::WindowSize || resizable(view);
            let (name, value) = if live {
                (theme::TEXT, theme::GOLD_BRIGHT)
            } else {
                (theme::TEXT_OFF, theme::TEXT_OFF)
            };
            r.name.add_theme_color_override("font_color", name);
            r.value.add_theme_color_override("font_color", value);
            for a in [&mut r.left, &mut r.right] {
                a.set_disabled(!live);
                a.set_modulate(if live {
                    Color::WHITE
                } else {
                    Color::from_rgba(1.0, 1.0, 1.0, 0.0)
                });
            }
        }
        self.show_about();
    }

    /// What the page shows now.
    pub fn view(&self) -> &View {
        &self.view
    }

    /// The gamepad whose buttons the hints show.
    pub fn pad(&self) -> Option<PadKind> {
        self.pad
    }

    fn show_about(&mut self) {
        let text = self
            .field()
            .map(|f| f.about(&self.view))
            .unwrap_or_default();
        self.about.set_text(&text);
    }

    /// Turn to `section`, the focus on its `focus`-th row.
    fn turn_to(&mut self, section: Section, focus: usize) {
        self.section = section;
        for (s, body) in &mut self.bodies {
            body.set_visible(*s == section);
        }
        for (s, tab) in &mut self.tabs {
            let current = *s == section;
            style_all(
                tab,
                if current {
                    &self.tab_current
                } else {
                    &self.tab_normal
                },
            );
            let color = if current {
                theme::GOLD_BRIGHT
            } else {
                theme::TEXT_DIM
            };
            tab.add_theme_color_override("font_color", color);
            tab.add_theme_color_override("font_pressed_color", color);
        }
        let n = section.fields().len();
        self.focus = focus.min(n.saturating_sub(1));
        self.style_rows();
        self.show_about();
    }

    fn style_rows(&mut self) {
        let field = self.field();
        for r in &mut self.rows {
            let sb = if Some(r.field) == field {
                &self.row_current
            } else {
                &self.row_normal
            };
            style_all(&mut r.button, sb);
        }
    }

    /// The section shown.
    pub fn section(&self) -> Section {
        self.section
    }

    /// The row with the focus.
    pub fn field(&self) -> Option<Field> {
        self.section.fields().get(self.focus).copied()
    }

    /// Where the page is: its section and the focus's row (to build it
    /// again in another language).
    pub fn place(&self) -> (Section, usize) {
        (self.section, self.focus)
    }

    /// Give `field` the focus (its section shown).
    pub fn focus_field(&mut self, field: Field) {
        for s in Section::ALL {
            if let Some(i) = s.fields().iter().position(|&f| f == field) {
                if s != self.section || i != self.focus {
                    self.turn_to(s, i);
                }
                return;
            }
        }
    }

    /// A tab clicked.
    pub fn tab(&mut self, section: Section) {
        if section != self.section {
            self.turn_to(section, 0);
        }
    }

    /// The next section (`dir` 1) or the one before, round.
    fn turn(&mut self, dir: i32) {
        let next = step_through(&Section::ALL, self.section, dir, true);
        self.turn_to(next, 0);
    }

    /// A key: ↑↓ choose a row, Tab and PgUp/PgDn a section (here); ←→
    /// and Enter change the row's value and Esc goes back (the game's).
    pub fn key(&mut self, k: &KeyInput) -> Option<PageKey> {
        let n = self.section.fields().len();
        match k.key {
            Key::Up if n > 0 => {
                self.focus = (self.focus + n - 1) % n;
                self.style_rows();
                self.show_about();
            }
            Key::Down if n > 0 => {
                self.focus = (self.focus + 1) % n;
                self.style_rows();
                self.show_about();
            }
            Key::Tab if !k.echo => self.turn(if k.mods.shift { -1 } else { 1 }),
            Key::PageUp if !k.echo => self.turn(-1),
            Key::PageDown if !k.echo => self.turn(1),
            Key::Left => return self.field().map(|f| PageKey::Step(f, -1, false)),
            Key::Right => return self.field().map(|f| PageKey::Step(f, 1, false)),
            Key::Enter | Key::KeypadEnter | Key::Char(' ') if !k.echo => {
                return self.field().map(|f| PageKey::Step(f, 1, true));
            }
            Key::Escape if !k.echo => return Some(PageKey::Back),
            _ => {}
        }
        None
    }

    /// The page's frame on screen (self-tests: it fits the canvas).
    pub fn rect(&self) -> Rect2 {
        self.root.get_global_rect()
    }

    /// The values wider than their place between the arrows (self-tests:
    /// every value fits, in every language).
    pub fn overflows(&self) -> Vec<String> {
        let font = theme::font(Face::BodyBold);
        self.rows
            .iter()
            .filter_map(|r| {
                let text = r.value.get_text();
                let w = font
                    .get_string_size_ex(&text)
                    .font_size(VALUE_SIZE)
                    .done()
                    .x;
                let room = r.value.get_size().x.max(VALUE_W);
                (w > room + 0.5).then(|| format!("{:?} \"{text}\" ({w:.0} > {room:.0})", r.field))
            })
            .collect()
    }

    /// The rows of the section shown: each one's name and value as the
    /// player reads them (self-tests).
    pub fn shown_rows(&self) -> Vec<(String, String)> {
        let fields = self.section.fields();
        self.rows
            .iter()
            .filter(|r| fields.contains(&r.field))
            .map(|r| {
                (
                    r.name.get_text().to_string(),
                    r.value.get_text().to_string(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_world::Display;

    fn view() -> View {
        View {
            settings: Settings::default(),
            lang: Lang::En,
            window: (1600, 900),
            screen: (1920, 1080),
        }
    }

    fn changed(c: Option<Change>) -> Settings {
        match c {
            Some(Change::Settings(s)) => s,
            other => panic!("not a change of the settings: {other:?}"),
        }
    }

    #[test]
    fn a_step_changes_its_row_s_setting() {
        let v = view();
        let s = changed(step(Field::WindowMode, &v, 1, false));
        assert_eq!(s.display.mode, WindowMode::Borderless);
        let s = changed(step(Field::FpsCap, &v, 1, false));
        assert_eq!(s.display.fps_cap, 30);
        let s = changed(step(Field::Vsync, &v, -1, false));
        assert!(!s.display.vsync);
        // at the end: nothing changes, unless it wraps (Enter)
        assert_eq!(step(Field::Vsync, &v, 1, false), None);
        assert!(!changed(step(Field::Vsync, &v, 1, true)).display.vsync);
        assert_eq!(
            step(Field::Language, &v, 1, false),
            Some(Change::Lang(Lang::Ru))
        );
    }

    #[test]
    fn the_window_size_steps_from_the_window_s_own_in_a_window_only() {
        let v = view();
        let s = changed(step(Field::WindowSize, &v, 1, false));
        assert_eq!(s.display.size, Some((1920, 1080)));
        let s = changed(step(Field::WindowSize, &v, -1, false));
        assert_eq!(s.display.size, Some((1366, 768)));
        let full = View {
            settings: Settings {
                display: Display {
                    mode: WindowMode::Borderless,
                    ..Display::default()
                },
                ..Settings::default()
            },
            ..view()
        };
        assert_eq!(step(Field::WindowSize, &full, 1, false), None);
    }

    #[test]
    fn every_section_has_rows_and_every_row_one_section() {
        for s in Section::ALL {
            assert!(!s.fields().is_empty(), "{s:?}");
        }
        let all: Vec<Field> = Section::ALL
            .iter()
            .flat_map(|s| s.fields().iter().copied())
            .collect();
        for (i, f) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(f), "{f:?} twice");
        }
    }
}
