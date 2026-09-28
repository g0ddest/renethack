//! What widgets tell the game. Signal closures only push these into the
//! queue; the game drains it in `process()`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use nh_world::KeyInput;

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// Translated keyboard (non-text contexts).
    Key(KeyInput),
    /// A key let go (ends holding it down).
    KeyUp(KeyInput),
    /// The window lost the keyboard: held keys are let go.
    FocusLost,
    /// Rest until HP and Pw are full (F5).
    Rest,
    /// A click on map cell (x, y); `button` 1 = left, 2 = right.
    MapClick {
        x: i32,
        y: i32,
        button: i32,
    },
    /// From the dialog opened for request `req`.
    Dialog {
        req: u64,
        ev: DialogEvent,
    },
    NewGame,
    /// Restore the save with this name.
    ContinueGame(String),
    StartCharacter(CharacterChoice),
    /// The name field on the creation screen changed.
    NameEdited(String),
    BackToTitle,
    QuitApp,
    CloseRequested,
    ToggleFullLog,
    /// Wheel steps: negative zooms in.
    Zoom(f32),
    /// Frame everything known of the level, or go back to the hero.
    ToggleOverview,
    /// A click on action bar slot `slot` (0-based, key 1..0); `button`
    /// 1 = left, 2 = right.
    ActionSlot {
        slot: usize,
        button: i32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DialogEvent {
    MenuClick(usize),
    MenuConfirm,
    MenuCancel,
    Choice(char),
    TextSubmitted(String),
    TextCancelled,
    ExtCmd(Option<String>),
    Close,
}

/// Role, race, gender and alignment codes ("random" allowed).
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterChoice {
    pub name: String,
    pub role: String,
    pub race: String,
    pub gender: String,
    pub align: String,
}

pub type UiQueue = Rc<RefCell<VecDeque<UiEvent>>>;

pub fn new_queue() -> UiQueue {
    Rc::new(RefCell::new(VecDeque::new()))
}

/// Push from a signal closure; never holds the borrow past the push.
pub fn push(queue: &UiQueue, ev: UiEvent) {
    queue.borrow_mut().push_back(ev);
}
