//! The first-steps guide on screen: "Your quest" at a new player's first
//! game (the goal, the screen, moving and fighting), and a hint card beside
//! the HUD, under the portrait block and clear of the map's middle where
//! the hero stands, the first time its occasion comes
//! (`nh_world::HintWatch`). Space, Enter, Esc, A, B or a click puts a card
//! away, or a while unread. Something over the map (a question, a dialog, a panel, a page)
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

use crate::gamepad::{PadButton, PadKind};
use crate::pad_view;
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
    /// How it goes: the keyboard's, the mouse's and B's words, or with a
    /// gamepad (`pad`) its button.
    close: Gd<Label>,
    foot: Gd<HBoxContainer>,
    pad: Option<PadKind>,
    pad_row: Option<Gd<HBoxContainer>>,
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
        let mut foot = HBoxContainer::new_alloc();
        foot.set_mouse_filter(MouseFilter::IGNORE);
        foot.set_alignment(godot::classes::box_container::AlignmentMode::END);
        let mut close = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        close.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
        foot.add_child(&close);
        col.add_child(&foot);
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
            foot,
            pad: None,
            pad_row: None,
            shown: None,
            going: None,
        }
    }

    /// A gamepad gives the input (B puts the card away), or the keyboard
    /// and the mouse do. The words follow (`rewrite`).
    pub fn set_pad(&mut self, pad: Option<PadKind>) {
        if self.pad == pad {
            return;
        }
        self.pad = pad;
        self.show_foot();
    }

    fn show_foot(&mut self) {
        if let Some(mut row) = self.pad_row.take() {
            row.queue_free();
        }
        self.close.set_text(&tr!("guide-card-close"));
        self.close.set_visible(self.pad.is_none());
        if let Some(kind) = self.pad {
            let parts: [(&[PadButton], String); 1] =
                [(&[PadButton::A, PadButton::B], tr!("hint-close"))];
            let row = pad_view::buttons_row(kind, &parts, 14);
            self.foot.add_child(&row);
            self.pad_row = Some(row);
        }
    }

    /// Show `card` with these words, its top-left at `at` (under the
    /// portrait block).
    pub fn show(&mut self, card: Guide, title: &str, body: &str, at: Vector2) {
        self.title.set_text(title);
        self.body.set_text(body);
        self.show_foot();
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

    /// The card up now, shown or held back hidden (not one going).
    pub fn current(&self) -> Option<Guide> {
        self.shown.filter(|_| self.going.is_none()).map(|(g, _)| g)
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

    /// Every word on the card as shown (self-tests: no keyboard's words
    /// while a gamepad plays).
    pub fn shown_words(&self) -> Vec<String> {
        pad_view::shown_words(&self.panel)
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
    /// The keys: the keyboard's hint, or a gamepad's buttons (`pad`) in
    /// its place.
    hint: Gd<Label>,
    foot: Gd<HBoxContainer>,
    pad: Option<PadKind>,
    pad_row: Option<Gd<HBoxContainer>>,
    tags: Gd<Control>,
    step: Option<usize>,
    /// The canvas and the named blocks the step was laid out for, and how
    /// many times a step was laid out (self-tests: not every frame).
    laid_for: (Vector2, Vec<(&'static str, Rect2)>),
    layouts: usize,
    /// The leaders drawn from names to their blocks (self-tests).
    leaders: Vec<Rect2>,
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
            foot,
            pad: None,
            pad_row: None,
            tags,
            step: None,
            laid_for: (Vector2::ZERO, Vec::new()),
            layouts: 0,
            leaders: Vec::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.step.is_some()
    }

    /// A gamepad gives the input (its buttons in the foot), or the
    /// keyboard and the mouse do (their hint).
    pub fn set_pad(&mut self, pad: Option<PadKind>) {
        if self.pad != pad {
            self.pad = pad;
            self.show_foot();
        }
    }

    /// The foot's keys for the step shown: A goes on (or plays), ← back,
    /// B skips.
    fn show_foot(&mut self) {
        if let Some(mut row) = self.pad_row.take() {
            row.queue_free();
        }
        self.hint.set_visible(self.pad.is_none());
        let (Some(kind), Some(i)) = (self.pad, self.step) else {
            return;
        };
        use PadButton::*;
        let last = i + 1 == QuestStep::ALL.len();
        let mut parts: Vec<(&[PadButton], String)> = vec![(
            &[A],
            if last {
                tr!("quest-play")
            } else {
                tr!("quest-next")
            },
        )];
        if i > 0 {
            parts.push((&[Left], tr!("quest-back")));
        }
        parts.push((&[B], tr!("quest-skip")));
        let mut row = pad_view::buttons_row(kind, &parts, 15);
        row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        self.foot.add_child(&row);
        self.foot.move_child(&row, 0);
        self.pad_row = Some(row);
    }

    /// Every word of the page as shown, its names of the HUD's blocks too
    /// (self-tests: no keyboard's words while a gamepad plays).
    pub fn shown_words(&self) -> Vec<String> {
        pad_view::shown_words(&self.root)
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
        self.show_foot();
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
        self.show_foot();
        // the screen's step lets the HUD show through, named
        self.shade.set_color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            if step == QuestStep::Screen { 0.3 } else { 0.62 },
        ));
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
        let view = self.root.get_viewport_rect().size;
        let page = Rect2::new(Vector2::new(view.x * 0.5 - w, view.y * (0.5 + y) - h), size);
        for mut c in self.tags.get_children().iter_shared() {
            self.tags.remove_child(&c);
            c.queue_free();
        }
        self.leaders.clear();
        if step == QuestStep::Screen {
            let mut made = Vec::new();
            for (block, words) in &words.tags {
                let Some((_, r)) = blocks.iter().find(|(name, _)| name == block) else {
                    continue;
                };
                self.tags.add_child(&ring(*r));
                let t = tag(words);
                // measured in the tree, in its own font
                self.tags.add_child(&t);
                let size = t.get_combined_minimum_size();
                made.push((t, *block, size));
            }
            let names: Vec<(&'static str, Vector2)> =
                made.iter().map(|(_, b, size)| (*b, *size)).collect();
            let spots = name_spots(&names, blocks, view, page);
            for ((mut t, _, _), (at, leader)) in made.into_iter().zip(spots) {
                place(
                    &t,
                    [0.0; 4],
                    [at.position.x, at.position.y, at.end().x, at.end().y],
                );
                t.reset_size();
                if let Some(l) = leader {
                    self.tags.add_child(&leader_line(l));
                    self.leaders.push(l);
                }
            }
        }
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

    /// The leaders from names to their blocks on screen (self-tests).
    pub fn leaders(&self) -> Vec<Rect2> {
        self.leaders.clone()
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

/// A HUD block's name, in a banner.
fn tag(words: &str) -> Gd<PanelContainer> {
    let mut p = theme::framed(Frame::Banner);
    p.set_mouse_filter(MouseFilter::IGNORE);
    let mut l = theme::styled_label(words, Face::Body, TAG_SIZE, theme::GOLD_BRIGHT);
    l.set_mouse_filter(MouseFilter::IGNORE);
    p.add_child(&l);
    p
}

/// A name's gap to its block, the steps it moves away by when there is no
/// room right beside it, and how far it may go.
const TAG_GAP: f32 = 6.0;
const TAG_STEP: f32 = 8.0;
const TAG_STEPS: usize = 24;
/// The only block a leader may cross: the XP bar hugs the action bar.
const LEADER_CROSSES: &str = "XP bar";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side {
    Above,
    Below,
    Left,
    Right,
}

/// Where each block's name goes (`tag_spot`), clear of the page `page` and
/// of each other: the foot's middle first (the action bar, the most
/// crowded), then the foot's from the middle out, then the rest in turn.
fn name_spots(
    names: &[(&'static str, Vector2)],
    blocks: &[(&'static str, Rect2)],
    view: Vector2,
    page: Rect2,
) -> Vec<(Rect2, Option<Rect2>)> {
    let rect_of = |b: &str| {
        blocks
            .iter()
            .find(|(n, _)| *n == b)
            .map_or(Rect2::new(Vector2::ZERO, Vector2::ZERO), |(_, r)| *r)
    };
    let crowd = |b: &str| {
        let r = rect_of(b);
        let c = r.position + r.size * 0.5;
        if c.y > view.y * 0.6 {
            (c.x - view.x * 0.5).abs()
        } else {
            f32::MAX
        }
    };
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by(|&a, &b| crowd(names[a].0).total_cmp(&crowd(names[b].0)));
    let mut taken = vec![page];
    let mut out = vec![(Rect2::new(Vector2::ZERO, Vector2::ZERO), None); names.len()];
    for i in order {
        let (block, size) = names[i];
        let r = rect_of(block);
        let (at, leader) =
            tag_spot(size, r, view, blocks, &taken).unwrap_or((Rect2::new(r.position, size), None));
        taken.push(at);
        taken.extend(leader);
        out[i] = (at, leader);
    }
    out
}

/// Where a block's name of `size` goes: the nearest spot beside the block
/// (above one at the foot, under one at the top or toward the middle, else
/// on its outer side) clear of every HUD block and of `taken` (the page,
/// the names and leaders placed), inside the screen; and, when it is not
/// right beside the block, a leader to it crossing nothing but the XP bar.
fn tag_spot(
    size: Vector2,
    block: Rect2,
    view: Vector2,
    blocks: &[(&'static str, Rect2)],
    taken: &[Rect2],
) -> Option<(Rect2, Option<Rect2>)> {
    let centre = block.position + block.size * 0.5;
    let on_left = centre.x < view.x / 2.0;
    let sides: &[Side] = if centre.y > view.y * 0.6 {
        if on_left {
            &[Side::Above, Side::Left]
        } else {
            &[Side::Above, Side::Right]
        }
    } else if centre.y < view.y * 0.4 {
        if on_left {
            &[Side::Below, Side::Right]
        } else {
            &[Side::Left, Side::Below]
        }
    } else if on_left {
        &[Side::Right, Side::Above, Side::Below]
    } else {
        &[Side::Left, Side::Above, Side::Below]
    };
    let clear = |r: Rect2| {
        r.position.x >= 4.0
            && r.position.y >= 4.0
            && r.end().x <= view.x - 4.0
            && r.end().y <= view.y - 4.0
            && !blocks.iter().any(|(_, b)| b.intersects_exclude_borders(r))
            && !taken.iter().any(|t| t.intersects_exclude_borders(r))
    };
    for k in 0..=TAG_STEPS {
        let d = TAG_GAP + k as f32 * TAG_STEP;
        for &side in sides {
            // along the side, from the middle out, overlapping the block
            // enough for a leader
            let (lo, hi, mid) = match side {
                Side::Above | Side::Below => (
                    block.position.x - size.x + 16.0,
                    block.end().x - 16.0,
                    centre.x - size.x / 2.0,
                ),
                Side::Left | Side::Right => (
                    block.position.y - size.y + 16.0,
                    block.end().y - 16.0,
                    centre.y - size.y / 2.0,
                ),
            };
            let mut along: Vec<f32> = (0..)
                .map(|i| lo + i as f32 * TAG_STEP)
                .take_while(|a| *a <= hi)
                .chain(std::iter::once(mid.clamp(lo, hi.max(lo))))
                .collect();
            along.sort_by(|a, b| (a - mid).abs().total_cmp(&(b - mid).abs()));
            for a in along {
                let at = match side {
                    Side::Above => Vector2::new(a, block.position.y - d - size.y),
                    Side::Below => Vector2::new(a, block.end().y + d),
                    Side::Left => Vector2::new(block.position.x - d - size.x, a),
                    Side::Right => Vector2::new(block.end().x + d, a),
                };
                let r = Rect2::new(at, size);
                if !clear(r) {
                    continue;
                }
                if k == 0 {
                    return Some((r, None));
                }
                if let Some(l) = leader(r, block, side, blocks, taken) {
                    return Some((r, Some(l)));
                }
            }
        }
    }
    None
}

/// A leader from the name at `r` to its block on `side`: a thin straight
/// line in the middle of a free lane, a few pixels off every other block
/// (and the rings round them), crossing none but the XP bar, nor anything
/// taken.
fn leader(
    r: Rect2,
    block: Rect2,
    side: Side,
    blocks: &[(&'static str, Rect2)],
    taken: &[Rect2],
) -> Option<Rect2> {
    // the ring's line is 3 px out from the block
    let ring = 3.0;
    let (lo, hi) = match side {
        Side::Above | Side::Below => (
            r.position.x.max(block.position.x) + 6.0,
            r.end().x.min(block.end().x) - 6.0,
        ),
        Side::Left | Side::Right => (
            r.position.y.max(block.position.y) + 6.0,
            r.end().y.min(block.end().y) - 6.0,
        ),
    };
    let line = |a: f32| match side {
        Side::Above => Rect2::new(
            Vector2::new(a - 1.0, r.end().y),
            Vector2::new(2.0, block.position.y - ring - r.end().y),
        ),
        Side::Below => Rect2::new(
            Vector2::new(a - 1.0, block.end().y + ring),
            Vector2::new(2.0, r.position.y - block.end().y - ring),
        ),
        Side::Left => Rect2::new(
            Vector2::new(r.end().x, a - 1.0),
            Vector2::new(block.position.x - ring - r.end().x, 2.0),
        ),
        Side::Right => Rect2::new(
            Vector2::new(block.end().x + ring, a - 1.0),
            Vector2::new(r.position.x - block.end().x - ring, 2.0),
        ),
    };
    let free = |l: Rect2| {
        !blocks.iter().any(|(name, b)| {
            *name != LEADER_CROSSES && *b != block && b.grow(4.0).intersects_exclude_borders(l)
        }) && !taken.iter().any(|t| t.intersects_exclude_borders(l))
    };
    // the free lanes across, the one nearest the middle, its middle
    let mut lanes: Vec<(f32, f32)> = Vec::new();
    let mut a = lo;
    while a <= hi {
        if free(line(a)) {
            match lanes.last_mut() {
                Some((_, end)) if a - *end <= 1.5 => *end = a,
                _ => lanes.push((a, a)),
            }
        }
        a += 1.0;
    }
    let mid = (lo + hi) / 2.0;
    let gap = |(s, e): (f32, f32)| (mid - mid.clamp(s, e)).abs();
    lanes
        .into_iter()
        .min_by(|x, y| gap(*x).total_cmp(&gap(*y)))
        .map(|(s, e)| line((s + e) / 2.0))
}

/// A leader drawn: a thin gold line.
fn leader_line(r: Rect2) -> Gd<ColorRect> {
    let mut l = ColorRect::new_alloc();
    l.set_color(theme::GOLD);
    l.set_mouse_filter(MouseFilter::IGNORE);
    place(
        &l,
        [0.0; 4],
        [r.position.x, r.position.y, r.end().x, r.end().y],
    );
    l
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect2 {
        Rect2::new(Vector2::new(x, y), Vector2::new(w, h))
    }

    /// A canvas, the HUD's blocks on it and the screen's step's page.
    type Screen = (Vector2, Vec<(&'static str, Rect2)>, Rect2);

    /// The HUD's blocks and the screen's step's page as a game shows them
    /// at 1280×800 (a 1600×1000 canvas, the compact layout) and at
    /// 1920×1080, and the Russian names' sizes.
    fn screens() -> [Screen; 2] {
        [
            (
                Vector2::new(1600.0, 1000.0),
                vec![
                    ("portrait", rect(24.0, 24.0, 520.0, 162.0)),
                    ("minimap", rect(1394.0, 24.0, 182.0, 152.0)),
                    ("mode badge", rect(1336.0, 182.0, 240.0, 32.0)),
                    ("log", rect(432.0, 752.0, 436.0, 114.0)),
                    ("action bar", rect(432.0, 900.0, 736.0, 88.0)),
                    ("XP bar", rect(432.0, 872.0, 736.0, 22.0)),
                    ("micro-buttons", rect(884.0, 838.0, 284.0, 28.0)),
                    ("HP orb", rect(280.0, 840.0, 148.0, 148.0)),
                    ("Pw orb", rect(1172.0, 840.0, 148.0, 148.0)),
                    ("XP words", rect(760.0, 868.0, 80.0, 30.0)),
                ],
                rect(400.0, 297.5, 800.0, 165.0),
            ),
            (
                Vector2::new(1920.0, 1080.0),
                vec![
                    ("portrait", rect(24.0, 24.0, 520.0, 176.0)),
                    ("minimap", rect(1714.0, 24.0, 182.0, 152.0)),
                    ("mode badge", rect(1656.0, 182.0, 240.0, 32.0)),
                    ("log", rect(24.0, 796.0, 404.0, 260.0)),
                    ("action bar", rect(592.0, 980.0, 736.0, 88.0)),
                    ("XP bar", rect(592.0, 952.0, 736.0, 22.0)),
                    ("micro-buttons", rect(1044.0, 918.0, 284.0, 28.0)),
                    ("HP orb", rect(440.0, 920.0, 148.0, 148.0)),
                    ("Pw orb", rect(1332.0, 920.0, 148.0, 148.0)),
                    ("XP words", rect(920.0, 948.0, 80.0, 30.0)),
                ],
                rect(560.0, 328.0, 800.0, 165.0),
            ),
        ]
    }

    #[test]
    fn the_tours_names_cover_no_block_and_lead_across_only_the_xp_bar() {
        let names = [
            ("portrait", Vector2::new(310.0, 40.0)),
            ("HP orb", Vector2::new(242.0, 40.0)),
            ("Pw orb", Vector2::new(191.0, 40.0)),
            ("minimap", Vector2::new(219.0, 40.0)),
            ("action bar", Vector2::new(202.0, 40.0)),
            ("log", Vector2::new(192.0, 40.0)),
        ];
        for (view, blocks, page) in screens() {
            let spots = name_spots(&names, &blocks, view, page);
            for (i, ((name, _), (at, leader))) in names.iter().zip(&spots).enumerate() {
                let inside = Rect2::new(Vector2::ZERO, view).encloses(*at);
                assert!(inside, "{view:?} {name}: {at:?} off the screen");
                for (b, r) in &blocks {
                    assert!(
                        !r.intersects_exclude_borders(*at),
                        "{view:?} {name} covers the {b}"
                    );
                }
                assert!(
                    !page.intersects_exclude_borders(*at),
                    "{view:?} {name} on the page"
                );
                for (other, (o, _)) in names.iter().zip(&spots).skip(i + 1) {
                    assert!(
                        !o.intersects_exclude_borders(*at),
                        "{view:?} {name} and {}",
                        other.0
                    );
                }
                if let Some(l) = leader {
                    for (b, r) in &blocks {
                        let crossed = r.intersects_exclude_borders(*l);
                        assert!(
                            !crossed || *b == "XP bar",
                            "{view:?} {name}'s leader crosses the {b}"
                        );
                    }
                }
            }
            // the bar's name has no room beside it: it leads to the bar
            let bar = names.iter().position(|(n, _)| *n == "action bar").unwrap();
            assert!(spots[bar].1.is_some(), "{view:?}: {:?}", spots[bar]);
        }
    }
}
