//! The first-steps guide on screen: "Your quest" at a new player's first
//! game (the goal, the screen, moving and fighting), and a hint card beside
//! the HUD, under the portrait block and clear of the map's middle where
//! the hero stands, the first time its occasion comes
//! (`nh_world::HintWatch`). Esc, B or a click puts a card away, or a while
//! unread. Something over the map (a question, a dialog, a panel, a page)
//! holds it back: hidden, its time stopped.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, HBoxContainer, InputEvent, InputEventMouseButton,
    Label, Panel, PanelContainer, StyleBoxFlat, VBoxContainer,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_world::Guide;

use crate::theme::{self, Face, Frame, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// The card's width and its words' sizes, in design pixels.
const CARD_W: f32 = 430.0;
const TITLE_SIZE: i32 = 21;
const BODY_SIZE: i32 = 17;
/// Seconds: coming in, shown unread at most, going.
const CARD_IN: f64 = 0.25;
const CARD_HOLD: f64 = 45.0;
const CARD_OUT: f64 = 0.3;

/// The card and the one it shows.
pub struct HintCard {
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    body: Gd<Label>,
    close: Gd<Label>,
    /// The card shown, and for how long; going: its fade's start.
    shown: Option<(Guide, f64)>,
    going: Option<f64>,
}

impl HintCard {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> HintCard {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        layer.add_child(&root);
        let mut panel = theme::framed(Frame::Hud);
        panel.set_mouse_filter(MouseFilter::STOP);
        on_click(&panel, &queue, UiEvent::HintClose);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 4);
        col.set_mouse_filter(MouseFilter::IGNORE);
        let kicker = theme::styled_label(&tr!("guide-card-kicker"), Face::Caps, 14, theme::GOLD);
        crate::i18n::text(&kicker, "guide-card-kicker");
        col.add_child(&kicker);
        // wrapped, as tall as their lines measure (`show`): a wrapped label
        // sizes itself late, and only grows
        let mut title = theme::styled_label("", Face::TitleBold, TITLE_SIZE, theme::GOLD_BRIGHT);
        title.set_autowrap_mode(AutowrapMode::WORD_SMART);
        title.set_clip_text(true);
        col.add_child(&title);
        let mut body = theme::styled_label("", Face::Body, BODY_SIZE, theme::TEXT);
        body.set_autowrap_mode(AutowrapMode::WORD_SMART);
        body.set_clip_text(true);
        col.add_child(&body);
        let mut close = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        close.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
        col.add_child(&close);
        for l in [&mut title, &mut body, &mut close] {
            l.set_mouse_filter(MouseFilter::IGNORE);
        }
        panel.add_child(&col);
        panel.set_visible(false);
        root.add_child(&panel);
        HintCard {
            panel,
            title,
            body,
            close,
            shown: None,
            going: None,
        }
    }

    /// Show `card` with these words, its top-left at `at` (under the
    /// portrait block).
    pub fn show(&mut self, card: Guide, title: &str, body: &str, at: Vector2) {
        self.title.set_text(title);
        self.body.set_text(body);
        self.close.set_text(&tr!("guide-card-close"));
        // the words' own height: a wrapped label sizes itself late
        let text_w = CARD_W - 36.0;
        fit_lines(&mut self.title, text_w);
        fit_lines(&mut self.body, text_w);
        place(
            &self.panel,
            [0.0, 0.0, 0.0, 0.0],
            [at.x, at.y, at.x + CARD_W, at.y],
        );
        self.panel.reset_size();
        self.panel
            .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        self.shown = Some((card, 0.0));
        self.going = None;
    }

    /// The words again (in another language).
    pub fn rewrite(&mut self, title: &str, body: &str) {
        if let Some((card, t)) = self.shown {
            let at = self.panel.get_position();
            self.show(card, title, body, at);
            self.shown = Some((card, t.max(CARD_IN)));
            self.panel.set_modulate(Color::WHITE);
        }
    }

    /// Put it away (a fade).
    pub fn dismiss(&mut self) {
        if let Some((_, t)) = self.shown
            && self.going.is_none()
        {
            self.going = Some(t);
        }
    }

    /// Gone at once (the hints turned off, a new game).
    pub fn clear(&mut self) {
        self.shown = None;
        self.going = None;
        self.panel.set_visible(false);
    }

    /// The time passes (`dt` seconds); `blocked`: something is over the
    /// map, the card waits hidden. True once it has gone.
    pub fn tick(&mut self, dt: f64, blocked: bool) -> bool {
        let Some((_, t)) = self.shown.as_mut() else {
            return false;
        };
        if blocked {
            self.panel.set_visible(false);
            return false;
        }
        *t += dt;
        let t = *t;
        if self.going.is_none() && t >= CARD_IN + CARD_HOLD {
            self.going = Some(t);
        }
        let alpha = match self.going {
            Some(from) if t - from >= CARD_OUT => {
                self.clear();
                return true;
            }
            Some(from) => 1.0 - (t - from) / CARD_OUT,
            None => (t / CARD_IN).min(1.0),
        };
        self.panel.set_modulate(Color::from_rgba(
            1.0,
            1.0,
            1.0,
            alpha.clamp(0.0, 1.0) as f32,
        ));
        self.panel.set_visible(true);
        false
    }

    /// No card, not even one going.
    pub fn idle(&self) -> bool {
        self.shown.is_none()
    }

    /// The card on screen now (self-tests), not one going.
    pub fn shown(&self) -> Option<Guide> {
        self.shown
            .filter(|_| self.panel.is_visible() && self.going.is_none())
            .map(|(g, _)| g)
    }

    /// Come in all the way, not yet going (self-tests: a shot of it).
    pub fn in_full(&self) -> bool {
        self.shown().is_some() && self.shown.is_some_and(|(_, t)| t >= CARD_IN)
    }

    /// The card's words on screen (self-tests).
    pub fn words(&self) -> (String, String) {
        (
            self.title.get_text().to_string(),
            self.body.get_text().to_string(),
        )
    }

    /// Where it is on screen while shown (self-tests: it fits, and keeps
    /// off the HUD and the map's middle).
    pub fn rect(&self) -> Option<Rect2> {
        self.panel
            .is_visible()
            .then(|| self.panel.get_global_rect())
    }
}

// ---- "Your quest": the page at a new player's first game ----

/// The quest page's steps, in turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestStep {
    /// The goal: the Amulet far below, back up, to the hero's god.
    Goal,
    /// The screen: the HUD's blocks, each named beside it.
    Screen,
    /// Moving and fighting: keys as bound, the mouse, the gamepad.
    Moves,
}

impl QuestStep {
    pub const ALL: [QuestStep; 3] = [QuestStep::Goal, QuestStep::Screen, QuestStep::Moves];
}

/// What the quest page's buttons ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestInput {
    Next,
    Back,
    Skip,
}

/// The words of a step, made by the game (keys as the player has bound
/// them): its title and lines; for the screen, each HUD block's name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestWords {
    pub title: String,
    pub lines: Vec<String>,
    /// The HUD's blocks named (the screen's step): block, words.
    pub tags: Vec<(&'static str, String)>,
}

/// The page's width, its words' sizes, in design pixels.
const QUEST_W: f32 = 760.0;
const QUEST_TITLE: i32 = 34;
const QUEST_BODY: i32 = 19;
const TAG_SIZE: i32 = 16;

/// "Your quest": over the game at a new player's first game (the engine
/// waits at its prompt; no turn goes by), a step at a time with Next,
/// Back and Skip; the HUD's tour names its blocks where they are.
pub struct QuestPage {
    root: Gd<Control>,
    shade: Gd<ColorRect>,
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    body: Gd<VBoxContainer>,
    back: Gd<Button>,
    next: Gd<Button>,
    hint: Gd<Label>,
    tags: Gd<Control>,
    step: Option<usize>,
    /// The canvas and the named blocks the step was laid out for, and how
    /// many times a step was laid out (self-tests: not every frame).
    laid_for: (Vector2, Vec<(&'static str, Rect2)>),
    layouts: usize,
}

impl QuestPage {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> QuestPage {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_visible(false);
        layer.add_child(&root);
        let mut shade = ColorRect::new_alloc();
        theme::full_rect_ignore(&shade);
        // the game behind waits: the mouse stops here, a click goes on
        shade.set_mouse_filter(MouseFilter::STOP);
        on_click(&shade, &queue, UiEvent::Quest(QuestInput::Next));
        root.add_child(&shade);
        let tags = Control::new_alloc();
        theme::full_rect_ignore(&tags);
        root.add_child(&tags);
        let mut panel = theme::framed(Frame::Panel);
        panel.set_mouse_filter(MouseFilter::STOP);
        on_click(&panel, &queue, UiEvent::Quest(QuestInput::Next));
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 14);
        col.set_custom_minimum_size(Vector2::new(QUEST_W, 0.0));
        let title = theme::styled_label("", Face::Title, QUEST_TITLE, theme::ACCENT);
        col.add_child(&title);
        let mut body = VBoxContainer::new_alloc();
        body.add_theme_constant_override("separation", 10);
        col.add_child(&body);
        let mut foot = HBoxContainer::new_alloc();
        foot.add_theme_constant_override("separation", 14);
        let mut hint = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        hint.set_h_size_flags(SizeFlags::EXPAND_FILL);
        hint.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        foot.add_child(&hint);
        let skip = theme::button(&tr!("quest-skip"), &queue, UiEvent::Quest(QuestInput::Skip));
        crate::i18n::text(&skip, "quest-skip");
        foot.add_child(&skip);
        let back = theme::button(&tr!("quest-back"), &queue, UiEvent::Quest(QuestInput::Back));
        crate::i18n::text(&back, "quest-back");
        foot.add_child(&back);
        let next = theme::button(&tr!("quest-next"), &queue, UiEvent::Quest(QuestInput::Next));
        foot.add_child(&next);
        col.add_child(&foot);
        panel.add_child(&col);
        root.add_child(&panel);
        QuestPage {
            root,
            shade,
            panel,
            title,
            body,
            back,
            next,
            hint,
            tags,
            step: None,
            laid_for: (Vector2::ZERO, Vec::new()),
            layouts: 0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.step.is_some()
    }

    /// The step shown (self-tests).
    pub fn step(&self) -> Option<QuestStep> {
        self.step.map(|i| QuestStep::ALL[i])
    }

    /// Open on its first step.
    pub fn open(&mut self) {
        self.step = Some(0);
        self.root.set_visible(true);
    }

    pub fn close(&mut self) {
        self.step = None;
        self.root.set_visible(false);
    }

    /// A step on or back; past the last, closed. True while open.
    pub fn go(&mut self, input: QuestInput) -> bool {
        let Some(i) = self.step else {
            return false;
        };
        match input {
            QuestInput::Skip => self.close(),
            QuestInput::Back => self.step = Some(i.saturating_sub(1)),
            QuestInput::Next if i + 1 >= QuestStep::ALL.len() => self.close(),
            QuestInput::Next => self.step = Some(i + 1),
        }
        self.is_open()
    }

    /// The step's words, and the HUD's blocks where they are now (the
    /// screen's step names each beside it).
    pub fn show(&mut self, words: &QuestWords, blocks: &[(&'static str, Rect2)]) {
        let Some(i) = self.step else {
            return;
        };
        let step = QuestStep::ALL[i];
        let last = i + 1 == QuestStep::ALL.len();
        self.title.set_text(&words.title);
        for mut c in self.body.get_children().iter_shared() {
            self.body.remove_child(&c);
            c.queue_free();
        }
        for line in &words.lines {
            let mut l = theme::styled_label(line, Face::Body, QUEST_BODY, theme::TEXT);
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            l.set_clip_text(true);
            // measured in the tree, in its own font
            self.body.add_child(&l);
            fit_lines(&mut l, QUEST_W);
        }
        self.back.set_visible(i > 0);
        self.next.set_text(&if last {
            tr!("quest-play")
        } else {
            tr!("quest-next")
        });
        self.hint
            .set_text(&crate::i18n::whole_parts(&tr!("quest-hint")));
        // the screen's step lets the HUD show through, named
        self.shade.set_color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            if step == QuestStep::Screen { 0.3 } else { 0.62 },
        ));
        for mut c in self.tags.get_children().iter_shared() {
            self.tags.remove_child(&c);
            c.queue_free();
        }
        if step == QuestStep::Screen {
            let view = self.root.get_viewport_rect().size;
            // the names keep off the blocks they name and off each other
            let named: Vec<Rect2> = blocks
                .iter()
                .filter(|(name, _)| words.tags.iter().any(|(b, _)| b == name))
                .map(|(_, r)| *r)
                .collect();
            let mut keep_off = named;
            for (block, words) in &words.tags {
                if let Some((_, r)) = blocks.iter().find(|(name, _)| name == block) {
                    self.tags.add_child(&ring(*r));
                    let t = tag(words, *r, view, &keep_off);
                    keep_off.push(t.get_rect());
                    self.tags.add_child(&t);
                }
            }
        }
        // as large as its words, in the middle; the screen's step higher,
        // clear of the bar and the orbs
        let y = if step == QuestStep::Screen {
            -0.12
        } else {
            0.0
        };
        let size = self.panel.get_combined_minimum_size();
        let (w, h) = (size.x / 2.0, size.y / 2.0);
        place(&self.panel, [0.5, 0.5 + y, 0.5, 0.5 + y], [-w, -h, w, h]);
        self.panel.reset_size();
        let named = blocks
            .iter()
            .filter(|(name, _)| words.tags.iter().any(|(b, _)| b == name))
            .copied()
            .collect();
        self.laid_for = (self.root.get_viewport_rect().size, named);
        self.layouts += 1;
    }

    /// How many times a step was laid out (self-tests).
    pub fn layouts(&self) -> usize {
        self.layouts
    }

    /// The canvas or the blocks named moved since the step was shown (a
    /// window resized): its names are to be placed again.
    pub fn moved(&self, blocks: &[(&'static str, Rect2)]) -> bool {
        if !self.is_open() {
            return false;
        }
        let (view, named) = &self.laid_for;
        let now: Vec<_> = blocks
            .iter()
            .filter(|(name, _)| named.iter().any(|(n, _)| n == name))
            .copied()
            .collect();
        *view != self.root.get_viewport_rect().size || now != *named
    }

    /// The page's panel on screen (self-tests: it fits the canvas).
    pub fn rect(&self) -> Option<Rect2> {
        self.is_open().then(|| self.panel.get_global_rect())
    }

    /// The HUD's names on screen (self-tests): each tag's words and rect.
    pub fn tags(&self) -> Vec<(String, Rect2)> {
        self.tags
            .get_children()
            .iter_shared()
            .filter_map(|c| c.try_cast::<PanelContainer>().ok())
            .map(|p| {
                let text = p
                    .get_child(0)
                    .and_then(|l| l.try_cast::<Label>().ok())
                    .map(|l| l.get_text().to_string())
                    .unwrap_or_default();
                (text, p.get_global_rect())
            })
            .collect()
    }

    /// The words of the step shown (self-tests): its title and lines.
    pub fn words(&self) -> (String, Vec<String>) {
        let lines = self
            .body
            .get_children()
            .iter_shared()
            .filter_map(|c| c.try_cast::<Label>().ok())
            .map(|l| l.get_text().to_string())
            .collect();
        (self.title.get_text().to_string(), lines)
    }
}

/// A left click on `c` (not on a button over it) pushes `ev`.
fn on_click<T: Inherits<Control>>(c: &Gd<T>, queue: &UiQueue, ev: UiEvent) {
    let q = queue.clone();
    c.clone()
        .upcast::<Control>()
        .signals()
        .gui_input()
        .connect(move |e: Gd<InputEvent>| {
            if let Ok(b) = e.try_cast::<InputEventMouseButton>()
                && b.is_pressed()
                && b.get_button_index() == MouseButton::LEFT
            {
                push(&q, ev.clone());
            }
        });
}

/// A wrapped label as tall as its lines at `width`, measured by its own
/// shaping (a wrapped label sizes itself late, and only grows).
fn fit_lines(label: &mut Gd<Label>, width: f32) {
    label.set_custom_minimum_size(Vector2::new(width, 0.0));
    label.set_size(Vector2::new(width, 4000.0));
    let lines = label.get_line_count().max(1) as f32;
    let spacing = label.get_theme_constant("line_spacing") as f32;
    let h = lines * label.get_line_height() as f32 + (lines - 1.0) * spacing;
    label.set_custom_minimum_size(Vector2::new(width, h.ceil()));
}

/// A HUD block's name beside it, where there is room: above a block at
/// the foot, under one at the top or toward the middle, else on its outer
/// side; clear of `keep_off` (the blocks named, the names placed).
fn tag(words: &str, block: Rect2, view: Vector2, keep_off: &[Rect2]) -> Gd<PanelContainer> {
    let mut p = theme::framed(Frame::Banner);
    p.set_mouse_filter(MouseFilter::IGNORE);
    let mut l = theme::styled_label(words, Face::Body, TAG_SIZE, theme::GOLD_BRIGHT);
    l.set_mouse_filter(MouseFilter::IGNORE);
    p.add_child(&l);
    let w = theme::font(Face::Body)
        .get_string_size_ex(words)
        .font_size(TAG_SIZE)
        .done()
        .x
        + 28.0;
    let size = Vector2::new(w, 34.0);
    let gap = 6.0;
    let centre = block.position + block.size * 0.5;
    let (x_mid, x_left, x_right) = (
        centre.x - w / 2.0,
        block.position.x - w - gap,
        block.end().x + gap,
    );
    // flush with the block's left or right edge
    let (x_flush_l, x_flush_r) = (block.position.x, block.end().x - w);
    let (y_mid, y_above, y_below) = (
        centre.y - size.y / 2.0,
        block.position.y - size.y - gap,
        block.end().y + gap,
    );
    let on_left = centre.x < view.x / 2.0;
    let tries: Vec<(f32, f32)> = if centre.y > view.y * 0.6 {
        let (inner, outer) = if on_left {
            (x_flush_l, x_flush_r)
        } else {
            (x_flush_r, x_flush_l)
        };
        let side = if on_left { x_left } else { x_right };
        vec![
            (x_mid, y_above),
            (outer, y_above),
            (inner, y_above),
            (side, y_mid),
        ]
    } else if centre.y < view.y * 0.4 {
        if on_left {
            vec![(x_flush_l, y_below), (x_mid, y_below), (x_right, y_mid)]
        } else {
            vec![(x_left, y_mid), (x_flush_r, y_below), (x_mid, y_below)]
        }
    } else if on_left {
        vec![(x_right, y_mid), (x_mid, y_above), (x_mid, y_below)]
    } else {
        vec![(x_left, y_mid), (x_mid, y_above), (x_mid, y_below)]
    };
    let room = |at: Vector2| {
        let r = Rect2::new(at, size);
        r.position.x >= 4.0
            && r.position.y >= 4.0
            && r.end().x <= view.x - 4.0
            && r.end().y <= view.y - 4.0
            && !keep_off.iter().any(|k| k.intersects_exclude_borders(r))
    };
    let at = tries
        .iter()
        .map(|&(x, y)| Vector2::new(x, y))
        .find(|&at| room(at))
        .unwrap_or_else(|| {
            // no room anywhere: the first place, inside the screen
            let (x, y) = tries[0];
            Vector2::new(
                x.clamp(4.0, (view.x - w - 4.0).max(4.0)),
                y.clamp(4.0, (view.y - size.y - 4.0).max(4.0)),
            )
        });
    place(&p, [0.0; 4], [at.x, at.y, at.x + w, at.y + size.y]);
    p
}

/// A block named on the screen's step: a thin gold line around it.
fn ring(block: Rect2) -> Gd<Panel> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_draw_center(false);
    sb.set_border_width_all(2);
    sb.set_border_color(theme::GOLD);
    sb.set_corner_radius_all(4);
    let mut p = Panel::new_alloc();
    p.add_theme_stylebox_override("panel", &sb);
    p.set_mouse_filter(MouseFilter::IGNORE);
    let r = block.grow(3.0);
    place(
        &p,
        [0.0; 4],
        [r.position.x, r.position.y, r.end().x, r.end().y],
    );
    p
}
