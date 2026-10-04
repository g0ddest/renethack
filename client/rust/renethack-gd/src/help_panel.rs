//! The in-game help (localization phase R8): NetHack's Guidebook in the
//! player's language, by chapters, with a search. The books are made by
//! `tools/help/guidebook.py` (`client/help/guidebook.{en,ru}.json`): the
//! English from NetHack's own Guidebook, the Russian from Vadim
//! Velikodniy's translation.

use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, FontVariation, HBoxContainer, Label, LineEdit,
    PanelContainer, RichTextLabel, ScrollContainer, StyleBoxFlat, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use nh_world::{Key, KeyInput};
use serde::Deserialize;

use crate::i18n::{self, Lang};
use crate::theme::{self, Face, Frame, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

const EN: &str = include_str!("../../../help/guidebook.en.json");
const RU: &str = include_str!("../../../help/guidebook.ru.json");

/// The most rows the list shows (chapters, or a search's finds).
const ROWS: usize = 90;
/// What a search lists at most.
const MAX_HITS: usize = 60;
/// Characters of a find's paragraph around the words found.
const SNIPPET: usize = 90;

/// A Guidebook, as `tools/help/guidebook.py` makes it.
#[derive(Debug, Deserialize)]
pub struct Book {
    pub lang: String,
    pub title: String,
    pub credit: String,
    pub chapters: Vec<Chapter>,
}

#[derive(Debug, Deserialize)]
pub struct Chapter {
    pub number: String,
    pub title: String,
    pub level: u8,
    /// BBCode, one paragraph each.
    pub paragraphs: Vec<String>,
}

impl Book {
    /// The book in `lang` (English for a language without one).
    pub fn of(lang: Lang) -> Book {
        let source = match lang {
            Lang::Ru => RU,
            Lang::En | Lang::Pseudo => EN,
        };
        serde_json::from_str(source).expect("client/help/guidebook.*.json is a book")
    }
}

/// A paragraph's words: its BBCode less the tags.
pub fn plain(bb: &str) -> String {
    let mut out = String::with_capacity(bb.len());
    let mut rest = bb;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find(']') {
            Some(j) => {
                if &tail[..=j] == "[lb]" {
                    out.push('[');
                }
                rest = &tail[j + 1..];
            }
            None => {
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Lower case, ё as е: how a search compares.
fn fold(text: &str) -> String {
    text.to_lowercase().replace('ё', "е")
}

/// A place a search found the words: a chapter, its paragraph, the words
/// around them.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub chapter: usize,
    pub paragraph: usize,
    pub snippet: String,
}

/// Where the book has `query` (at least two letters), in the book's
/// order, at most `max`.
pub fn search(book: &Book, index: &[Vec<String>], query: &str, max: usize) -> Vec<Hit> {
    let q = fold(query.trim());
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for (c, paragraphs) in index.iter().enumerate() {
        for (p, text) in paragraphs.iter().enumerate() {
            if !text.contains(&q) {
                continue;
            }
            hits.push(Hit {
                chapter: c,
                paragraph: p,
                snippet: snippet(&plain(&book.chapters[c].paragraphs[p]), &q),
            });
            if hits.len() == max {
                return hits;
            }
        }
    }
    hits
}

/// The words around the first place `q` (folded) is in `words`.
fn snippet(words: &str, q: &str) -> String {
    let text: Vec<char> = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let folded: Vec<char> = text
        .iter()
        .map(|&c| match c.to_lowercase().next().unwrap_or(c) {
            'ё' => 'е',
            l => l,
        })
        .collect();
    let q: Vec<char> = q.chars().collect();
    let at = (0..folded.len())
        .find(|&i| folded[i..].starts_with(&q))
        .unwrap_or(0);
    let start = at.saturating_sub(SNIPPET / 3);
    let end = (start + SNIPPET).min(text.len());
    let mut s: String = text[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < text.len() {
        s.push('…');
    }
    s
}

/// What the help panel was told by its widgets.
#[derive(Debug, Clone, PartialEq)]
pub enum HelpInput {
    /// The search typed.
    Search(String),
    /// A row of the list clicked.
    Row(usize),
    Close,
}

/// The book open in the panel, and its words for a search.
struct Open {
    lang: Lang,
    book: Book,
    index: Vec<Vec<String>>,
}

pub struct HelpPanel {
    root: Gd<Control>,
    panel: Gd<PanelContainer>,
    queue: UiQueue,
    heading: Gd<Label>,
    search: Gd<LineEdit>,
    scroll: Gd<ScrollContainer>,
    rows: Vec<(Gd<Button>, Gd<Label>)>,
    text: Gd<RichTextLabel>,
    credit: Gd<Label>,
    hint: Gd<Label>,
    open: Option<Open>,
    /// The chapter shown.
    chapter: usize,
    /// A search's finds (None: the list is the chapters).
    hits: Option<Vec<Hit>>,
    /// The list's highlighted row.
    selected: usize,
    row_normal: Gd<StyleBoxFlat>,
    row_current: Gd<StyleBoxFlat>,
    pad: bool,
}

impl HelpPanel {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> HelpPanel {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        root.set_visible(false);
        layer.add_child(&root);
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.55));
        theme::full_rect_ignore(&shade);
        shade.set_mouse_filter(MouseFilter::STOP);
        root.add_child(&shade);

        let mut panel = theme::framed(Frame::Panel);
        panel.set_mouse_filter(MouseFilter::STOP);
        place(&panel, [0.5, 0.5, 0.5, 0.5], [-640.0, -400.0, 640.0, 400.0]);
        root.add_child(&panel);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        panel.add_child(&col);

        // the heading: the panel's name and the book's, a close button
        let mut head = HBoxContainer::new_alloc();
        head.add_theme_constant_override("separation", 12);
        let mut heading = theme::styled_label("", Face::Title, 30, theme::GOLD_BRIGHT);
        heading.set_h_size_flags(SizeFlags::EXPAND_FILL);
        heading.set_clip_text(true);
        head.add_child(&heading);
        let mut close = theme::button("×", &queue, UiEvent::Help(HelpInput::Close));
        close.set_custom_minimum_size(Vector2::new(40.0, 36.0));
        i18n::tip(&close, "help-close-tip");
        head.add_child(&close);
        col.add_child(&head);

        let mut body = HBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", 16);
        body.set_v_size_flags(SizeFlags::EXPAND_FILL);
        // left: the search and the list of chapters (or finds)
        let mut left = VBoxContainer::new_alloc();
        left.add_theme_constant_override("separation", 8);
        left.set_custom_minimum_size(Vector2::new(360.0, 0.0));
        let mut search = LineEdit::new_alloc();
        search.set_clear_button_enabled(true);
        i18n::bind(&search, "placeholder_text", "help-search-placeholder");
        let q = queue.clone();
        search
            .signals()
            .text_changed()
            .connect(move |t: GString| push(&q, UiEvent::Help(HelpInput::Search(t.to_string()))));
        left.add_child(&search);
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_horizontal_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 0);
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let flat = |bg: Color, border: Option<Color>| {
            let mut sb = StyleBoxFlat::new_gd();
            sb.set_bg_color(bg);
            sb.set_corner_radius_all(3);
            sb.set_content_margin_all(0.0);
            if let Some(b) = border {
                sb.set_border_width(godot::builtin::Side::LEFT, 3);
                sb.set_border_color(b);
            }
            sb
        };
        let row_normal = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), None);
        let row_current = flat(
            Color {
                a: 0.16,
                ..theme::ACCENT
            },
            Some(theme::GOLD_BRIGHT),
        );
        let mut rows = Vec::with_capacity(ROWS);
        for i in 0..ROWS {
            let mut b = Button::new_alloc();
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(0.0, 28.0));
            for state in [
                "normal",
                "hover",
                "pressed",
                "hover_pressed",
                "disabled",
                "focus",
            ] {
                b.add_theme_stylebox_override(state, &row_normal);
            }
            let mut l = theme::styled_label("", Face::Body, 16, theme::TEXT);
            // the book's own words, in its own language
            i18n::verbatim(&l);
            l.set_vertical_alignment(VerticalAlignment::CENTER);
            l.set_clip_text(true);
            place(&l, [0.0, 0.0, 1.0, 1.0], [8.0, 0.0, -6.0, 0.0]);
            b.add_child(&l);
            let q = queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::Help(HelpInput::Row(i))));
            b.set_visible(false);
            list.add_child(&b);
            rows.push((b, l));
        }
        scroll.add_child(&list);
        left.add_child(&scroll);
        body.add_child(&left);
        // right: the chapter
        let mut text = RichTextLabel::new_alloc();
        text.set_use_bbcode(true);
        text.set_h_size_flags(SizeFlags::EXPAND_FILL);
        text.set_v_size_flags(SizeFlags::EXPAND_FILL);
        text.set_focus_mode(FocusMode::NONE);
        text.set_selection_enabled(true);
        text.set_autowrap_mode(AutowrapMode::WORD_SMART);
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
            ("normal_font_size", 18),
            ("bold_font_size", 18),
            ("italics_font_size", 18),
            ("mono_font_size", 16),
        ] {
            text.add_theme_font_size_override(name, size);
        }
        text.add_theme_color_override("default_color", theme::TEXT);
        text.add_theme_constant_override("table_h_separation", 14);
        text.add_theme_constant_override("table_v_separation", 4);
        text.add_theme_constant_override("line_separation", 3);
        // the book's own words, in its own language
        i18n::verbatim(&text);
        body.add_child(&text);
        col.add_child(&body);
        let mut credit = theme::styled_label("", Face::Body, 13, theme::TEXT_DIM);
        credit.set_autowrap_mode(AutowrapMode::WORD_SMART);
        i18n::verbatim(&credit);
        col.add_child(&credit);
        let mut hint = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        col.add_child(&hint);
        HelpPanel {
            root,
            panel,
            queue,
            heading,
            search,
            scroll,
            rows,
            text,
            credit,
            hint,
            open: None,
            chapter: 0,
            hits: None,
            selected: 0,
            row_normal,
            row_current,
            pad: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    /// The panel's frame on screen (None: closed): the gamepad's hints dock
    /// in it; it must fit the screen.
    pub fn frame_rect(&self) -> Option<Rect2> {
        self.is_open().then(|| self.panel.get_global_rect())
    }

    /// The search field has the keyboard.
    pub fn wants_text(&self) -> bool {
        self.is_open() && self.search.has_focus()
    }

    pub fn set_pad(&mut self, on: bool) {
        if self.pad != on {
            self.pad = on;
            self.show_hint();
        }
    }

    /// Open on the chapter last read, in the book of the language now.
    pub fn open(&mut self) {
        self.load();
        self.fit();
        self.root.set_visible(true);
        self.show_list();
        self.show_chapter(self.chapter, None);
        self.show_hint();
        if !self.pad {
            self.search.call_deferred("grab_focus", &[]);
        }
    }

    pub fn close(&mut self) {
        self.search.release_focus();
        self.root.set_visible(false);
    }

    /// The book of the language now (kept while it stays).
    fn load(&mut self) {
        let lang = i18n::lang();
        if self.open.as_ref().is_some_and(|o| o.lang == lang) {
            return;
        }
        let book = Book::of(lang);
        let index = book
            .chapters
            .iter()
            .map(|c| c.paragraphs.iter().map(|p| fold(&plain(p))).collect())
            .collect();
        self.chapter = 0;
        self.hits = None;
        self.search.set_text("");
        self.heading
            .set_text(&tr!("help-title", book = book.title.clone()));
        self.credit.set_text(&book.credit);
        self.open = Some(Open { lang, book, index });
    }

    /// The panel as big as the screen allows, about 1280×800 of the design.
    fn fit(&mut self) {
        let view = self.root.get_viewport_rect().size;
        let w = (view.x - 80.0).clamp(640.0, 1280.0) / 2.0;
        let h = (view.y - 120.0).clamp(400.0, 800.0) / 2.0;
        place(&self.panel, [0.5, 0.5, 0.5, 0.5], [-w, -h, w, h]);
    }

    /// The language changed: the book of the new one.
    pub fn relang(&mut self) {
        if self.is_open() {
            self.open();
        }
    }

    pub fn input(&mut self, input: HelpInput) {
        match input {
            HelpInput::Close => self.close(),
            HelpInput::Search(q) => {
                let Some(o) = &self.open else {
                    return;
                };
                self.hits = (q.trim().chars().count() >= 2)
                    .then(|| search(&o.book, &o.index, &q, MAX_HITS));
                self.selected = 0;
                self.show_list();
            }
            HelpInput::Row(i) => self.pick(i),
        }
    }

    /// The list's row `i`: a chapter, or a find in one.
    fn pick(&mut self, i: usize) {
        self.selected = i;
        match self.hits.as_ref().and_then(|h| h.get(i)).cloned() {
            Some(hit) => self.show_chapter(hit.chapter, Some(hit.paragraph)),
            None if self.hits.is_none() => self.show_chapter(i, None),
            None => {}
        }
        self.style_rows();
    }

    /// A key while the panel is open: true when it took it.
    pub fn key(&mut self, input: &KeyInput) -> bool {
        if input.key == Key::Escape {
            if !input.echo {
                self.close();
            }
            return true;
        }
        let n = match &self.hits {
            Some(h) => h.len(),
            None => self.open.as_ref().map_or(0, |o| o.book.chapters.len()),
        };
        let step = match input.key {
            Key::Up => -1,
            Key::Down => 1,
            Key::PageUp | Key::PageDown => {
                let mut bar = self.text.get_v_scroll_bar().expect("a scroll bar");
                let page = self.text.get_size().y * 0.85;
                let by = if input.key == Key::PageUp {
                    -page
                } else {
                    page
                };
                let v = bar.get_value() + by as f64;
                bar.set_value(v);
                return true;
            }
            Key::Home | Key::End => {
                let mut bar = self.text.get_v_scroll_bar().expect("a scroll bar");
                let v = if input.key == Key::Home {
                    0.0
                } else {
                    bar.get_max()
                };
                bar.set_value(v);
                return true;
            }
            _ => return false,
        };
        if n > 0 {
            let next = (self.selected as i64 + step).clamp(0, n as i64 - 1) as usize;
            self.pick(next);
            self.reveal(next);
        }
        true
    }

    /// Scroll the list so row `i` shows.
    fn reveal(&mut self, i: usize) {
        if let Some((b, _)) = self.rows.get(i) {
            self.scroll.ensure_control_visible(b);
        }
    }

    fn show_list(&mut self) {
        let Some(o) = &self.open else {
            return;
        };
        let items: Vec<(String, f32, bool)> = match &self.hits {
            Some(hits) => hits
                .iter()
                .map(|h| {
                    let c = &o.book.chapters[h.chapter];
                    (
                        format!("{} {}\n{}", c.number, c.title, h.snippet),
                        48.0,
                        true,
                    )
                })
                .collect(),
            None => o
                .book
                .chapters
                .iter()
                .map(|c| {
                    let pad = "    ".repeat(usize::from(c.level.saturating_sub(1)));
                    (format!("{pad}{} {}", c.number, c.title), 28.0, false)
                })
                .collect(),
        };
        let none = self.hits.as_ref().is_some_and(|h| h.is_empty());
        for (i, (b, l)) in self.rows.iter_mut().enumerate() {
            match items.get(i) {
                Some((text, h, wrap)) => {
                    l.set_text(text);
                    l.set_autowrap_mode(if *wrap {
                        AutowrapMode::WORD_SMART
                    } else {
                        AutowrapMode::OFF
                    });
                    b.set_custom_minimum_size(Vector2::new(0.0, *h));
                    b.set_visible(true);
                }
                None if i == 0 && none => {
                    l.set_text(&tr!("help-nothing"));
                    b.set_custom_minimum_size(Vector2::new(0.0, 28.0));
                    b.set_visible(true);
                }
                None => b.set_visible(false),
            }
        }
        if self.hits.is_none() {
            self.selected = self.chapter;
        }
        self.style_rows();
        let sel = self.selected;
        self.reveal(sel);
    }

    fn style_rows(&mut self) {
        let (normal, current) = (self.row_normal.clone(), self.row_current.clone());
        for (i, (b, l)) in self.rows.iter_mut().enumerate() {
            let sb = if i == self.selected {
                &current
            } else {
                &normal
            };
            for state in ["normal", "hover", "pressed", "hover_pressed"] {
                b.add_theme_stylebox_override(state, sb);
            }
            l.add_theme_color_override(
                "font_color",
                if i == self.selected {
                    theme::GOLD_BRIGHT
                } else {
                    theme::TEXT
                },
            );
        }
    }

    /// Chapter `i`, scrolled to its paragraph `para` (a search's find).
    fn show_chapter(&mut self, i: usize, para: Option<usize>) {
        let Some(o) = &self.open else {
            return;
        };
        let Some(c) = o.book.chapters.get(i) else {
            return;
        };
        self.chapter = i;
        let mut bb = format!(
            "[font_size=26][color={}]{} {}[/color][/font_size]\n\n",
            theme::hex(theme::GOLD_BRIGHT),
            c.number,
            theme::bbcode_escape(&c.title)
        );
        // where each paragraph starts among the label's paragraphs
        let mut starts = Vec::with_capacity(c.paragraphs.len());
        for p in &c.paragraphs {
            starts.push(bb.matches('\n').count());
            bb.push_str(p);
            bb.push_str("\n\n");
        }
        self.text.set_text(&bb);
        let line = para.and_then(|p| starts.get(p)).copied().unwrap_or(0);
        self.text
            .call_deferred("scroll_to_paragraph", &[(line as i32).to_variant()]);
    }

    /// The keys, under the text (a gamepad's are in its strip of hints).
    fn show_hint(&mut self) {
        let hint = if self.pad {
            String::new()
        } else {
            i18n::whole_parts(&tr!("help-hint"))
        };
        self.hint.set_text(&hint);
    }

    /// The chapters' titles and the one shown (self-tests).
    pub fn view(&self) -> Option<(Vec<String>, usize, Option<usize>)> {
        let o = self.open.as_ref()?;
        let titles = o.book.chapters.iter().map(|c| c.title.clone()).collect();
        Some((titles, self.chapter, self.hits.as_ref().map(Vec::len)))
    }

    /// The text of the chapter shown (self-tests).
    pub fn shown_text(&self) -> String {
        self.text.get_parsed_text().to_string()
    }

    /// The book's language (self-tests).
    pub fn book_lang(&self) -> Option<String> {
        self.open.as_ref().map(|o| o.book.lang.clone())
    }

    /// Queue a search as the field would (self-tests).
    pub fn type_search(&mut self, text: &str) {
        self.search.set_text(text);
        push(
            &self.queue,
            UiEvent::Help(HelpInput::Search(text.to_string())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_books_have_their_chapters() {
        for lang in [Lang::En, Lang::Ru] {
            let book = Book::of(lang);
            assert!(
                book.chapters.len() > 40,
                "{lang:?}: {}",
                book.chapters.len()
            );
            assert!(book.chapters.iter().all(|c| !c.title.is_empty()));
            let paragraphs: usize = book.chapters.iter().map(|c| c.paragraphs.len()).sum();
            assert!(paragraphs > 800, "{lang:?}: {paragraphs} paragraphs");
        }
        assert_eq!(Book::of(Lang::Ru).lang, "ru");
        assert_eq!(Book::of(Lang::En).lang, "en");
    }

    #[test]
    fn tags_go_and_brackets_stay_for_a_search() {
        assert_eq!(
            plain("[b]Gold[/b] — the [code][lb]x][/code]"),
            "Gold — the [x]"
        );
        assert_eq!(plain("no tags"), "no tags");
    }

    #[test]
    fn a_search_finds_words_in_either_book() {
        for (lang, words) in [(Lang::Ru, "амулет йендора"), (Lang::En, "amulet of yendor")]
        {
            let book = Book::of(lang);
            let index: Vec<Vec<String>> = book
                .chapters
                .iter()
                .map(|c| c.paragraphs.iter().map(|p| fold(&plain(p))).collect())
                .collect();
            let hits = search(&book, &index, words, 10);
            assert!(!hits.is_empty(), "{lang:?}: nothing for {words}");
            assert!(hits.len() <= 10);
            let hit = &hits[0];
            assert!(
                fold(&hit.snippet).contains(words),
                "{lang:?}: {:?} lacks {words}",
                hit.snippet
            );
        }
        let book = Book::of(Lang::En);
        assert!(
            search(&book, &[], "a", 10).is_empty(),
            "one letter finds nothing"
        );
    }
}
