//! The game's menu: Esc (a gamepad's B) at the command prompt with nothing
//! else up. Back to the game, the settings, save and leave, or give up
//! (NetHack's #quit: its own question, under a warning, is the
//! confirmation). No letter picks an entry, and the focus opens on going
//! back: a stray key never gives up.

use godot::classes::control::MouseFilter;
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, Label, PanelContainer, VBoxContainer,
};
use godot::prelude::*;

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
}

impl GameMenuPick {
    pub const ALL: [GameMenuPick; 4] = [
        GameMenuPick::Resume,
        GameMenuPick::Settings,
        GameMenuPick::SaveLeave,
        GameMenuPick::GiveUp,
    ];

    fn words(self) -> String {
        match self {
            GameMenuPick::Resume => tr!("game-menu-resume"),
            GameMenuPick::Settings => tr!("game-menu-settings"),
            GameMenuPick::SaveLeave => tr!("game-menu-save"),
            GameMenuPick::GiveUp => tr!("game-menu-give-up"),
        }
    }
}

/// The menu's width, in design pixels.
const MENU_W: f32 = 380.0;

pub struct GameMenu {
    root: Gd<Control>,
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    buttons: Vec<(GameMenuPick, Gd<Button>)>,
    about: Gd<Label>,
    hint: Gd<Label>,
    focus: usize,
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
        col.set_custom_minimum_size(Vector2::new(MENU_W, 0.0));
        let title = theme::styled_label("", Face::Title, 30, theme::ACCENT);
        col.add_child(&title);
        let mut buttons = Vec::new();
        for pick in GameMenuPick::ALL {
            let mut b = theme::button("", &queue, UiEvent::GameMenu(pick));
            b.set_custom_minimum_size(Vector2::new(MENU_W, 42.0));
            if pick == GameMenuPick::GiveUp {
                b.add_theme_color_override("font_color", theme::DANGER);
                b.add_theme_color_override("font_hover_color", theme::DANGER);
            }
            col.add_child(&b);
            buttons.push((pick, b));
        }
        // what giving up means, under its entry
        let mut about = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        about.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        about.set_custom_minimum_size(Vector2::new(MENU_W, 0.0));
        col.add_child(&about);
        let hint = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        col.add_child(&hint);
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
            buttons,
            about,
            hint,
            focus: 0,
            focus_look,
        }
    }

    pub fn is_open(&self) -> bool {
        self.root.is_visible()
    }

    /// Open, its words in the language now, the focus on going back.
    pub fn open(&mut self) {
        self.title.set_text(&tr!("game-menu-title"));
        for (pick, b) in &mut self.buttons {
            b.set_text(&pick.words());
        }
        self.about.set_text(&tr!("game-menu-give-up-about"));
        self.hint
            .set_text(&crate::i18n::whole_parts(&tr!("game-menu-hint")));
        self.focus = 0;
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

    /// The focus a step up or down, round.
    pub fn step(&mut self, by: i32) {
        let n = self.buttons.len() as i32;
        self.focus = (self.focus as i32 + by).rem_euclid(n) as usize;
        self.show_focus();
    }

    /// The entry the focus is on.
    pub fn focused(&self) -> GameMenuPick {
        self.buttons[self.focus].0
    }

    fn show_focus(&mut self) {
        let focus = self.focus;
        let look = self.focus_look.clone();
        for (i, (_, b)) in self.buttons.iter_mut().enumerate() {
            if i == focus {
                b.add_theme_stylebox_override("normal", &look);
            } else {
                b.remove_theme_stylebox_override("normal");
            }
        }
    }

    /// The entries' words on screen, the one focused (self-tests).
    pub fn entries(&self) -> (Vec<String>, usize) {
        let words = self
            .buttons
            .iter()
            .map(|(_, b)| b.get_text().to_string())
            .collect();
        (words, self.focus)
    }
}
