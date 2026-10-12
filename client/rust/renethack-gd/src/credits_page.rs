//! The credits and licences page (the title's Credits, the settings'
//! button): the documents in a list, the credits then the licences, and
//! the chosen one's text beside it (`credits.rs` draws them). ↑↓ (the
//! d-pad) choose, PgUp/PgDn (the bumpers) scroll the text, Home and End go
//! to its ends, Esc (B) goes back; the mouse clicks the list, scrolls the
//! text and follows its links in the browser.

use std::collections::HashMap;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, Control, FontVariation, HBoxContainer, Label, Os, PanelContainer, RichTextLabel,
    ScrollContainer, StyleBoxFlat, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::{Key, KeyInput};

use crate::credits::{self, Doc, Shipped};
use crate::gamepad::{PadButton, PadKind};
use crate::i18n;
use crate::pad_view;
use crate::theme::{self, Face, Frame};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// What the page's widgets tell the game.
#[derive(Debug, Clone, PartialEq)]
pub enum CreditsInput {
    /// A document of the list clicked.
    Pick(usize),
}

/// The page's size: as much of the canvas as it can take, within these.
const MIN_SIZE: Vector2 = Vector2::new(900.0, 560.0);
const MAX_SIZE: Vector2 = Vector2::new(1500.0, 900.0);
/// The list's width.
const LIST_W: f32 = 340.0;

pub struct CreditsPage {
    root: Gd<PanelContainer>,
    docs: Vec<Doc>,
    rows: Vec<Gd<Button>>,
    text: Gd<RichTextLabel>,
    selected: usize,
    shipped: Shipped,
    /// The documents drawn so far (the licences are long).
    drawn: HashMap<Doc, String>,
    row_normal: Gd<StyleBoxFlat>,
    row_current: Gd<StyleBoxFlat>,
    pad: Option<PadKind>,
}

fn row_style(current: bool) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_corner_radius_all(3);
    sb.set_content_margin(Side::LEFT, 14.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    if current {
        sb.set_bg_color(Color {
            a: 0.16,
            ..theme::ACCENT
        });
        sb.set_border_width(Side::LEFT, 3);
        sb.set_border_color(theme::GOLD_BRIGHT);
    } else {
        sb.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    }
    sb
}

/// A group's name in the list.
fn group(name: &str) -> Gd<Label> {
    let mut l = theme::styled_label(name, Face::Title, 21, theme::GOLD);
    l.set_custom_minimum_size(Vector2::new(0.0, 36.0));
    l.set_vertical_alignment(VerticalAlignment::BOTTOM);
    l
}

impl CreditsPage {
    /// The page as large as the canvas allows, open on the `selected`-th
    /// document; the hints for a gamepad of `pad`'s kind, else the keys.
    pub fn new(
        queue: &UiQueue,
        canvas: Vector2,
        shipped: Shipped,
        selected: usize,
        pad: Option<PadKind>,
    ) -> CreditsPage {
        let size = Vector2::new(
            (canvas.x - 120.0).clamp(MIN_SIZE.x, MAX_SIZE.x),
            (canvas.y - 100.0).clamp(MIN_SIZE.y, MAX_SIZE.y),
        );
        let mut root = theme::framed(Frame::Panel);
        root.set_mouse_filter(MouseFilter::STOP);
        root.set_custom_minimum_size(size);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 12);
        col.add_child(&theme::styled_label(
            &tr!("credits-title"),
            Face::Title,
            32,
            theme::ACCENT,
        ));

        let mut body = HBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", 20);
        body.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_custom_minimum_size(Vector2::new(LIST_W, 0.0));
        scroll.set_horizontal_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 2);
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let row_normal = row_style(false);
        let row_current = row_style(true);
        let mut docs = Vec::new();
        let mut rows = Vec::new();
        let groups = [
            (tr!("credits-group-credits"), Doc::credits().to_vec()),
            (tr!("credits-group-licences"), Doc::licences()),
        ];
        for (name, group_docs) in groups {
            list.add_child(&group(&name));
            for doc in group_docs {
                let i = docs.len();
                let mut b = Button::new_alloc();
                b.set_text(&doc.name());
                b.set_focus_mode(FocusMode::NONE);
                b.set_text_alignment(HorizontalAlignment::LEFT);
                b.set_custom_minimum_size(Vector2::new(0.0, 34.0));
                b.add_theme_font_override("font", &theme::font(Face::Body));
                b.add_theme_font_size_override("font_size", 18);
                b.add_theme_color_override("font_color", theme::TEXT);
                b.add_theme_color_override("font_hover_color", theme::GOLD_BRIGHT);
                for state in ["normal", "hover", "pressed", "hover_pressed", "focus"] {
                    b.add_theme_stylebox_override(state, &row_normal);
                }
                let q = queue.clone();
                b.signals()
                    .pressed()
                    .connect(move || push(&q, UiEvent::Credits(CreditsInput::Pick(i))));
                list.add_child(&b);
                docs.push(doc);
                rows.push(b);
            }
        }
        scroll.add_child(&list);
        body.add_child(&scroll);

        let mut text = RichTextLabel::new_alloc();
        text.set_use_bbcode(true);
        text.set_h_size_flags(SizeFlags::EXPAND_FILL);
        text.set_v_size_flags(SizeFlags::EXPAND_FILL);
        text.set_focus_mode(FocusMode::NONE);
        text.set_selection_enabled(true);
        text.set_autowrap_mode(AutowrapMode::WORD_SMART);
        // the long licences are laid out off the main thread
        text.set_threaded(true);
        text.add_theme_font_override("normal_font", &theme::font(Face::Body));
        text.add_theme_font_override("bold_font", &theme::font(Face::BodyBold));
        let mut italic = FontVariation::new_gd();
        italic.set_base_font(&theme::font(Face::Body));
        // a slant: the faces have no italic of their own
        italic.set_variation_transform(Transform2D::from_cols(
            Vector2::new(1.0, 0.0),
            Vector2::new(0.2, 1.0),
            Vector2::ZERO,
        ));
        text.add_theme_font_override("italics_font", &italic);
        text.add_theme_font_override("mono_font", &theme::font(Face::Mono));
        for (name, size) in [
            ("normal_font_size", 17),
            ("bold_font_size", 17),
            ("italics_font_size", 17),
            ("mono_font_size", 15),
        ] {
            text.add_theme_font_size_override(name, size);
        }
        text.add_theme_color_override("default_color", theme::TEXT);
        text.add_theme_constant_override("line_separation", 3);
        // the attributions and licences as they are written
        i18n::verbatim(&text);
        // a link opens in the browser
        text.signals().meta_clicked().connect(|meta: Variant| {
            Os::singleton().shell_open(&meta.to_string());
        });
        body.add_child(&text);
        col.add_child(&body);

        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 18);
        let mut hint = Self::hint(pad);
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        foot.add_child(&hint);
        foot.add_child(&theme::button(
            &tr!("credits-back"),
            queue,
            UiEvent::CloseCredits,
        ));
        col.add_child(&foot);
        root.add_child(&col);

        let mut page = CreditsPage {
            root,
            docs,
            rows,
            text,
            selected: 0,
            shipped,
            drawn: HashMap::new(),
            row_normal,
            row_current,
            pad,
        };
        page.select(selected);
        page
    }

    /// The keys under the page: the keyboard's, or a gamepad's buttons.
    fn hint(pad: Option<PadKind>) -> Gd<Control> {
        let Some(kind) = pad else {
            let mut l = theme::styled_label(
                &i18n::whole_parts(&tr!("credits-hint")),
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
        let parts: [(&[PadButton], String); 3] = [
            (&[Up, Down], tr!("hint-choose")),
            (&[Lb, Rb], tr!("hint-scroll")),
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

    /// Show the `i`-th document, from its top.
    pub fn select(&mut self, i: usize) {
        let Some(&doc) = self.docs.get(i) else {
            return;
        };
        self.selected = i;
        for (k, b) in self.rows.iter_mut().enumerate() {
            let sb = if k == i {
                &self.row_current
            } else {
                &self.row_normal
            };
            for state in ["normal", "hover", "pressed", "hover_pressed", "focus"] {
                b.add_theme_stylebox_override(state, sb);
            }
        }
        let shipped = &self.shipped;
        let bb = self
            .drawn
            .entry(doc)
            .or_insert_with(|| credits::bbcode(doc, shipped))
            .clone();
        self.text.set_text(&bb);
        self.text.scroll_to_line(0);
    }

    /// The document shown, and where it is in the list.
    pub fn selected(&self) -> (usize, Doc) {
        (self.selected, self.docs[self.selected])
    }

    /// The documents of the list.
    pub fn docs(&self) -> &[Doc] {
        &self.docs
    }

    /// Scroll the text by `pages` of its height (whole: to its ends).
    fn scroll(&mut self, pages: f64) {
        let Some(mut bar) = self.text.get_v_scroll_bar() else {
            return;
        };
        let page = f64::from(self.text.get_size().y) * 0.85;
        let to = (bar.get_value() + pages * page).clamp(0.0, bar.get_max());
        bar.set_value(to);
    }

    /// A key: ↑↓ choose, PgUp/PgDn scroll, Home/End go to the text's
    /// ends; true: Esc, back.
    pub fn key(&mut self, k: &KeyInput) -> bool {
        let n = self.docs.len();
        match k.key {
            Key::Up => self.select((self.selected + n - 1) % n),
            Key::Down => self.select((self.selected + 1) % n),
            Key::PageUp => self.scroll(-1.0),
            Key::PageDown => self.scroll(1.0),
            Key::Home => self.scroll(-1e9),
            Key::End => self.scroll(1e9),
            Key::Escape if !k.echo => return true,
            _ => {}
        }
        false
    }

    /// The gamepad whose buttons the hints show.
    pub fn pad(&self) -> Option<PadKind> {
        self.pad
    }

    /// The page's frame on screen (self-tests: it fits the canvas).
    pub fn rect(&self) -> Rect2 {
        self.root.get_global_rect()
    }

    /// The text shown, once laid out (self-tests).
    pub fn shown_text(&self) -> Option<String> {
        self.text
            .is_finished()
            .then(|| self.text.get_parsed_text().to_string())
    }

    /// How far the text is scrolled (self-tests).
    pub fn scrolled(&self) -> f64 {
        self.text.get_v_scroll_bar().map_or(0.0, |b| b.get_value())
    }
}
