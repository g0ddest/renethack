//! The game's menu: Esc (a gamepad's B or Start) at the command prompt
//! with nothing else up. On the left the game's own: back to the game,
//! the settings, save and leave, or give up (NetHack's #quit: its own
//! question, under a warning, is the confirmation). On the right every
//! panel, so a gamepad reaches each: the spells, the character, the
//! skills, the dungeon's overview, the journal, the codex, the
//! achievements, the help, and every command (the palette). No letter
//! picks an entry, and the focus opens on going back: a stray key never
//! gives up.

use godot::classes::control::MouseFilter;
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, HBoxContainer, Label, PanelContainer, VBoxContainer,
};
use godot::prelude::*;

use crate::gamepad::{PadButton, PadKind};
use crate::pad_view;
use crate::theme::{self, Face, Frame, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue};

/// What an entry of the menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMenuPick {
    Resume,
    Settings,
    /// The engine's #save: "Really save?", then the title.
    SaveLeave,
    /// The engine's #quit: "Really quit without saving?".
    GiveUp,
    /// The panels, as their keys or commands open them.
    Spells,
    Character,
    Skills,
    Overview,
    Journal,
    Codex,
    Achievements,
    Help,
    /// The palette: every extended command.
    Commands,
}

impl GameMenuPick {
    /// The menu's columns, each top down: the game's own, the panels.
    pub const COLUMNS: [&'static [GameMenuPick]; 2] = [
        &[
            GameMenuPick::Resume,
            GameMenuPick::Settings,
            GameMenuPick::SaveLeave,
            GameMenuPick::GiveUp,
        ],
        &[
            GameMenuPick::Spells,
            GameMenuPick::Character,
            GameMenuPick::Skills,
            GameMenuPick::Overview,
            GameMenuPick::Journal,
            GameMenuPick::Codex,
            GameMenuPick::Achievements,
            GameMenuPick::Help,
            GameMenuPick::Commands,
        ],
    ];

    fn words(self) -> String {
        match self {
            GameMenuPick::Resume => tr!("game-menu-resume"),
            GameMenuPick::Settings => tr!("game-menu-settings"),
            GameMenuPick::SaveLeave => tr!("game-menu-save"),
            GameMenuPick::GiveUp => tr!("game-menu-give-up"),
            GameMenuPick::Spells => tr!("spells-title"),
            GameMenuPick::Character => tr!("sheet-title"),
            GameMenuPick::Skills => tr!("skills-title"),
            GameMenuPick::Overview => tr!("overview-title"),
            GameMenuPick::Journal => tr!("journal-title"),
            GameMenuPick::Codex => tr!("codex-title"),
            GameMenuPick::Achievements => tr!("achievements-title"),
            GameMenuPick::Help => tr!("game-menu-help"),
            GameMenuPick::Commands => tr!("game-menu-commands"),
        }
    }
}

/// A column's width and an entry's height, in design pixels.
const COLUMN_W: f32 = 300.0;
const ENTRY_H: f32 = 36.0;

pub struct GameMenu {
    root: Gd<Control>,
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    /// Each column's entries, top down.
    columns: [Vec<(GameMenuPick, Gd<Button>)>; 2],
    about: Gd<Label>,
    hint: Gd<Label>,
    /// The foot: the keyboard's hint, or a gamepad's buttons (`pad`).
    foot: Gd<VBoxContainer>,
    pad: Option<PadKind>,
    pad_row: Option<Gd<HBoxContainer>>,
    /// The entry the focus is on: its column and its row.
    focus: (usize, usize),
    focus_look: Gd<godot::classes::StyleBoxFlat>,
}

impl GameMenu {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> GameMenu {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_visible(false);
        layer.add_child(&root);
        let mut shade = ColorRect::new_alloc();
        theme::full_rect_ignore(&shade);
        shade.set_color(theme::SHADE);
        // the game behind waits: the mouse stops here
        shade.set_mouse_filter(MouseFilter::STOP);
        root.add_child(&shade);
        let mut panel: Gd<PanelContainer> = theme::framed(Frame::Panel);
        panel.set_mouse_filter(MouseFilter::STOP);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        let title = theme::styled_label("", Face::Title, 30, theme::ACCENT);
        col.add_child(&title);
        let mut sides = HBoxContainer::new_alloc();
        sides.add_theme_constant_override("separation", 24);
        let mut columns: [Vec<(GameMenuPick, Gd<Button>)>; 2] = [Vec::new(), Vec::new()];
        let mut about = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        for (c, picks) in GameMenuPick::COLUMNS.iter().enumerate() {
            let mut side = VBoxContainer::new_alloc();
            side.add_theme_constant_override("separation", 6);
            side.set_custom_minimum_size(Vector2::new(COLUMN_W, 0.0));
            for &pick in picks.iter() {
                let mut b = theme::button("", &queue, UiEvent::GameMenu(pick));
                b.set_custom_minimum_size(Vector2::new(COLUMN_W, ENTRY_H));
                if pick == GameMenuPick::GiveUp {
                    b.add_theme_color_override("font_color", theme::DANGER);
                    b.add_theme_color_override("font_hover_color", theme::DANGER);
                }
                side.add_child(&b);
                columns[c].push((pick, b));
            }
            // what giving up means, under its entry
            if c == 0 {
                about.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
                about.set_custom_minimum_size(Vector2::new(COLUMN_W, 0.0));
                side.add_child(&about);
            }
            sides.add_child(&side);
        }
        col.add_child(&sides);
        let mut foot = VBoxContainer::new_alloc();
        foot.set_mouse_filter(MouseFilter::IGNORE);
        let hint = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        foot.add_child(&hint);
        col.add_child(&foot);
        panel.add_child(&col);
        root.add_child(&panel);
        let focus_look = {
            let mut sb = godot::classes::StyleBoxFlat::new_gd();
            sb.set_bg_color(Color::from_rgba(0.3, 0.22, 0.09, 1.0));
            sb.set_border_width_all(2);
            sb.set_border_color(theme::GOLD_BRIGHT);
            sb.set_corner_radius_all(3);
            sb.set_content_margin_all(6.0);
            sb
        };
        GameMenu {
            root,
            panel,
            title,
            columns,
            about,
            hint,
            foot,
            pad: None,
            pad_row: None,
            focus: (0, 0),
            focus_look,
        }
    }

    /// A gamepad gives the input (its buttons under the menu), or the
    /// keyboard does (its keys).
    pub fn set_pad(&mut self, pad: Option<PadKind>) {
        if self.pad != pad {
            self.pad = pad;
            self.show_foot();
        }
    }

    fn show_foot(&mut self) {
        if let Some(mut row) = self.pad_row.take() {
            row.queue_free();
        }
        self.hint.set_visible(self.pad.is_none());
        if let Some(kind) = self.pad {
            use PadButton::*;
            let parts: [(&[PadButton], String); 3] = [
                (&[Up, Down, Left, Right], tr!("hint-choose")),
                (&[A], tr!("hint-ok")),
                (&[B], tr!("hint-back")),
            ];
            let row = pad_view::buttons_row(kind, &parts, 15);
            self.foot.add_child(&row);
            self.pad_row = Some(row);
        }
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    /// Open, its words in the language now, the focus on going back.
    pub fn open(&mut self) {
        self.title.set_text(&tr!("game-menu-title"));
        for (pick, b) in self.columns.iter_mut().flatten() {
            b.set_text(&pick.words());
        }
        self.about.set_text(&tr!("game-menu-give-up-about"));
        self.hint
            .set_text(&crate::i18n::whole_parts(&tr!("game-menu-hint")));
        // its words in the language now
        self.show_foot();
        self.focus = (0, 0);
        self.show_focus();
        self.root.set_visible(true);
        // as large as its words, a little above the middle
        let size = self.panel.get_combined_minimum_size();
        let (w, h) = (size.x / 2.0, size.y / 2.0);
        place(&self.panel, [0.5, 0.45, 0.5, 0.45], [-w, -h, w, h]);
        self.panel.reset_size();
    }

    /// The menu's panel on screen while open (self-tests: it fits).
    pub fn rect(&self) -> Option<Rect2> {
        self.is_open().then(|| self.panel.get_global_rect())
    }

    pub fn close(&mut self) {
        self.root.set_visible(false);
    }

    /// The focus a step up or down its column, round.
    pub fn step(&mut self, by: i32) {
        let (c, r) = self.focus;
        let n = self.columns[c].len() as i32;
        self.focus = (c, (r as i32 + by).rem_euclid(n) as usize);
        self.show_focus();
    }

    /// The focus to the other column, as high as it can stay.
    pub fn side(&mut self, by: i32) {
        let (c, r) = self.focus;
        let c = (c as i32 + by).rem_euclid(2) as usize;
        self.focus = (c, r.min(self.columns[c].len() - 1));
        self.show_focus();
    }

    /// The focus on the next entry, the columns one after the other
    /// (Tab).
    pub fn next(&mut self) {
        let (c, r) = self.focus;
        self.focus = if r + 1 < self.columns[c].len() {
            (c, r + 1)
        } else {
            ((c + 1) % 2, 0)
        };
        self.show_focus();
    }

    /// The entry the focus is on.
    pub fn focused(&self) -> GameMenuPick {
        let (c, r) = self.focus;
        self.columns[c][r].0
    }

    fn show_focus(&mut self) {
        let focus = self.focus;
        let look = self.focus_look.clone();
        for (c, column) in self.columns.iter_mut().enumerate() {
            for (r, (_, b)) in column.iter_mut().enumerate() {
                if (c, r) == focus {
                    b.add_theme_stylebox_override("normal", &look);
                } else {
                    b.remove_theme_stylebox_override("normal");
                }
            }
        }
    }

    /// Every word of the menu as shown (self-tests): its foot the
    /// keyboard's hint, or a gamepad's buttons and theirs.
    pub fn shown_words(&self) -> Vec<String> {
        pad_view::shown_words(&self.panel)
    }

    /// The entries' words on screen, the columns one after the other, and
    /// the focused one's place among them (self-tests).
    pub fn entries(&self) -> (Vec<String>, usize) {
        let words = self
            .columns
            .iter()
            .flatten()
            .map(|(_, b)| b.get_text().to_string())
            .collect();
        let (c, r) = self.focus;
        let at = self.columns[..c].iter().map(Vec::len).sum::<usize>() + r;
        (words, at)
    }
}
