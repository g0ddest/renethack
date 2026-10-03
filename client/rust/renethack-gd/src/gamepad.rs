//! The gamepad (spec decision 5, part 2): joypad buttons and sticks turn
//! into the keys, clicks and panel inputs the keyboard and the mouse give,
//! by what is on screen. The layout:
//!
//! | Input | In the world | In menus, dialogs, the inventory |
//! |---|---|---|
//! | left stick, d-pad | walk (8 directions; held: walk on) | move the focus |
//! | right stick | the world cursor (hover; getpos moves the engine's) | move the focus |
//! | A | the cursor cell's action (a left click) | choose, toggle |
//! | B | Esc | back, cancel |
//! | X | search | the item's actions (inventory) |
//! | Y | inventory | pick up / put down an item (inventory) |
//! | LB / RB + A B X Y | action bar slots 1–4 / 5–8 (page 2: 9, 0) | filters, pages |
//! | LB / RB + d-pad ← → | the bar's other page | |
//! | LT (hold) | the radial menu; a stick picks, letting go runs it | |
//! | RT | fire | |
//! | Start | the command palette | confirm |
//! | Back | the message history | |
//! | R3 | the cursor back on the hero | |
//!
//! Nothing here touches Godot but the event types: `Pad` is fed buttons,
//! axes and the time, and says what to do.

use godot::global::{JoyAxis, JoyButton};
use nh_world::{Key, KeyInput, Mods};

/// Stick deflection that counts.
pub const DEAD_ZONE: f32 = 0.35;
/// A held direction walks on (a held key) after this.
const HOLD_SECS: f64 = 0.3;
/// In menus a held direction steps again after `NAV_FIRST`, then every
/// `NAV_EVERY`.
const NAV_FIRST: f64 = 0.38;
const NAV_EVERY: f64 = 0.12;
/// The world cursor steps a cell this often while the right stick is held
/// (faster when pushed all the way).
const CURSOR_SLOW: f64 = 0.2;
const CURSOR_FAST: f64 = 0.08;
/// A trigger past this is pressed.
const TRIGGER_ON: f32 = 0.5;
/// The radial's entries are picked past this deflection.
const RADIAL_PICK: f32 = 0.5;

/// The controller family, for the glyphs on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadKind {
    #[default]
    Xbox,
    PlayStation,
    Deck,
}

impl PadKind {
    /// From the name the driver gives the device.
    pub fn from_name(name: &str) -> PadKind {
        let n = name.to_lowercase();
        if n.contains("steam deck") || n.contains("valve") || n.contains("steam virtual") {
            PadKind::Deck
        } else if [
            "playstation",
            "dualsense",
            "dualshock",
            "ps4",
            "ps5",
            "sony",
            "wireless controller",
        ]
        .iter()
        .any(|w| n.contains(w))
        {
            PadKind::PlayStation
        } else {
            PadKind::Xbox
        }
    }

    /// What a button is called on this controller: letters, or the shapes
    /// of the PlayStation face buttons (drawn as text, no logos).
    pub fn label(self, b: PadButton) -> &'static str {
        use PadButton::*;
        match (self, b) {
            (PadKind::PlayStation, A) => "✕",
            (PadKind::PlayStation, B) => "○",
            (PadKind::PlayStation, X) => "□",
            (PadKind::PlayStation, Y) => "△",
            (PadKind::PlayStation, Lb) => "L1",
            (PadKind::PlayStation, Rb) => "R1",
            (PadKind::PlayStation, Lt) => "L2",
            (PadKind::PlayStation, Rt) => "R2",
            (PadKind::PlayStation, Start) => "Options",
            (PadKind::PlayStation, Back) => "Create",
            (PadKind::Deck, Lb) => "L1",
            (PadKind::Deck, Rb) => "R1",
            (PadKind::Deck, Lt) => "L2",
            (PadKind::Deck, Rt) => "R2",
            (PadKind::Deck, Start) => "☰",
            (PadKind::Deck, Back) => "⧉",
            (_, A) => "A",
            (_, B) => "B",
            (_, X) => "X",
            (_, Y) => "Y",
            (_, Lb) => "LB",
            (_, Rb) => "RB",
            (_, Lt) => "LT",
            (_, Rt) => "RT",
            (_, Start) => "Menu",
            (_, Back) => "View",
            (_, L3) => "L3",
            (_, R3) => "R3",
            (_, Up) => "↑",
            (_, Down) => "↓",
            (_, Left) => "←",
            (_, Right) => "→",
        }
    }

    /// The colour of a face button's medallion (Xbox and the Deck colour
    /// their letters, PlayStation its shapes).
    pub fn tint(self, b: PadButton) -> [f32; 3] {
        use PadButton::*;
        match (self, b) {
            (PadKind::PlayStation, A) => [0.55, 0.7, 1.0],
            (PadKind::PlayStation, B) => [1.0, 0.45, 0.45],
            (PadKind::PlayStation, X) => [0.95, 0.6, 0.9],
            (PadKind::PlayStation, Y) => [0.4, 0.9, 0.75],
            (_, A) => [0.45, 0.85, 0.35],
            (_, B) => [1.0, 0.4, 0.35],
            (_, X) => [0.4, 0.65, 1.0],
            (_, Y) => [1.0, 0.85, 0.3],
            _ => [0.91, 0.86, 0.77],
        }
    }
}

/// The buttons the client uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Lt,
    Rt,
    Start,
    Back,
    L3,
    R3,
    Up,
    Down,
    Left,
    Right,
}

impl PadButton {
    pub fn from_joy(b: JoyButton) -> Option<PadButton> {
        use PadButton::*;
        Some(match b {
            JoyButton::A => A,
            JoyButton::B => B,
            JoyButton::X => X,
            JoyButton::Y => Y,
            JoyButton::LEFT_SHOULDER => Lb,
            JoyButton::RIGHT_SHOULDER => Rb,
            JoyButton::START => Start,
            JoyButton::BACK => Back,
            JoyButton::LEFT_STICK => L3,
            JoyButton::RIGHT_STICK => R3,
            JoyButton::DPAD_UP => Up,
            JoyButton::DPAD_DOWN => Down,
            JoyButton::DPAD_LEFT => Left,
            JoyButton::DPAD_RIGHT => Right,
            _ => return None,
        })
    }

    pub fn joy(self) -> JoyButton {
        use PadButton::*;
        match self {
            A => JoyButton::A,
            B => JoyButton::B,
            X => JoyButton::X,
            Y => JoyButton::Y,
            Lb => JoyButton::LEFT_SHOULDER,
            Rb => JoyButton::RIGHT_SHOULDER,
            Start => JoyButton::START,
            Back => JoyButton::BACK,
            L3 => JoyButton::LEFT_STICK,
            R3 => JoyButton::RIGHT_STICK,
            Up => JoyButton::DPAD_UP,
            Down => JoyButton::DPAD_DOWN,
            Left => JoyButton::DPAD_LEFT,
            Right => JoyButton::DPAD_RIGHT,
            // the triggers are axes
            Lt | Rt => JoyButton::INVALID,
        }
    }
}

/// One of 8 directions, in NetHack's dirchars order: W NW N NE E SE S SW.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dir8(pub u8);

impl Dir8 {
    /// The direction a stick points (x right, y down), or None inside the
    /// dead zone; 45° sectors centred on the 8 directions.
    pub fn of(x: f32, y: f32) -> Option<Dir8> {
        if (x * x + y * y).sqrt() < DEAD_ZONE {
            return None;
        }
        // 0 = east, counter-clockwise in screen terms (y down: north is -y)
        let a = (-y).atan2(x).to_degrees();
        let sector = ((a + 360.0 + 22.5) / 45.0).floor() as i32 % 8;
        // sector: 0 E, 1 NE, 2 N, 3 NW, 4 W, 5 SW, 6 S, 7 SE
        Some(Dir8([4, 3, 2, 1, 0, 7, 6, 5][sector as usize]))
    }

    /// The nearest of the 4 cardinal directions (menus and lists).
    pub fn cardinal(x: f32, y: f32) -> Option<Dir8> {
        if (x * x + y * y).sqrt() < DEAD_ZONE {
            return None;
        }
        Some(if x.abs() > y.abs() {
            Dir8(if x > 0.0 { 4 } else { 0 })
        } else {
            Dir8(if y > 0.0 { 6 } else { 2 })
        })
    }

    /// The key that gives this direction (the arrows, Home/PgUp/End/PgDn:
    /// the key map turns them into the profile's direction keys).
    pub fn key(self) -> Key {
        match self.0 {
            0 => Key::Left,
            1 => Key::Home,
            2 => Key::Up,
            3 => Key::PageUp,
            4 => Key::Right,
            5 => Key::PageDown,
            6 => Key::Down,
            _ => Key::End,
        }
    }

    /// The cell step (x right, y down).
    pub fn delta(self) -> (i32, i32) {
        [
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
        ][self.0 as usize % 8]
    }
}

/// What the screen shows, for what a button means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadCtx {
    /// The command prompt over the map.
    World,
    /// getpos: the engine moves a cursor.
    Getpos,
    /// getdir: the stick gives a direction.
    Direction,
    /// An engine menu (pick-any or not).
    Menu {
        any: bool,
    },
    /// A yes/no question.
    Choice,
    /// A message with a letter to pick.
    Message {
        letter: char,
    },
    /// A text field: the on-screen keyboard.
    Text,
    /// The inventory panel: browse, a question, or a menu of items.
    PanelBrowse,
    PanelSelect,
    PanelMenu,
    /// Any other dialog (a text window, the palette, --More--).
    Other,
}

impl PadCtx {
    /// The sticks walk (rather than move a focus).
    fn walks(self) -> bool {
        matches!(self, PadCtx::World | PadCtx::Getpos | PadCtx::Direction)
    }
}

/// The on-screen keyboard's operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OskOp {
    Move(i32, i32),
    Press,
    Back,
    Layout,
    Submit,
}

/// What the game does for the pad.
#[derive(Debug, Clone, PartialEq)]
pub enum PadOut {
    /// As if this key were pressed (`echo`: held).
    Key(KeyInput),
    KeyUp(KeyInput),
    /// A left click on the world cursor's cell.
    Activate,
    /// Move the world cursor a cell.
    Cursor(i32, i32),
    /// The world cursor back on the hero.
    CursorHome,
    /// Action bar slot (0-based).
    Slot(usize),
    /// The bar's other page.
    Page(usize),
    /// The inventory: pick up or put down the focused item.
    Carry,
    /// The message history.
    History,
    /// The radial menu: open, its highlighted entry, run one (None: none).
    RadialOpen,
    RadialHover(Option<usize>),
    RadialClose(Option<usize>),
    Osk(OskOp),
}

fn key(k: Key) -> KeyInput {
    KeyInput::plain(k)
}

fn ch(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

/// Entries of the radial menu, clockwise from the top.
pub const RADIAL: [RadialEntry; 8] = [
    RadialEntry::Here,
    RadialEntry::PickUp,
    RadialEntry::Fight,
    RadialEntry::Kick,
    RadialEntry::Rest,
    RadialEntry::Pray,
    RadialEntry::Travel,
    RadialEntry::Save,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadialEntry {
    /// The cursor cell's actions (the engine's #therecmdmenu, as a right
    /// click).
    Here,
    PickUp,
    /// `F`: fight in a direction (the stick gives it).
    Fight,
    Kick,
    Rest,
    Pray,
    Travel,
    Save,
}

impl RadialEntry {
    /// The key of its caption.
    pub fn label_key(self) -> &'static str {
        match self {
            RadialEntry::Here => "radial-here",
            RadialEntry::PickUp => "radial-pick-up",
            RadialEntry::Fight => "radial-fight",
            RadialEntry::Kick => "radial-kick",
            RadialEntry::Rest => "radial-rest",
            RadialEntry::Pray => "radial-pray",
            RadialEntry::Travel => "radial-travel",
            RadialEntry::Save => "radial-save",
        }
    }
}

/// Which radial entry a stick points at (8 sectors, the first at the top).
pub fn radial_pick(x: f32, y: f32) -> Option<usize> {
    if (x * x + y * y).sqrt() < RADIAL_PICK {
        return None;
    }
    // clockwise from north
    let a = x.atan2(-y).to_degrees();
    Some((((a + 360.0 + 22.5) / 45.0).floor() as i32 % 8) as usize)
}

/// A stick's state.
#[derive(Debug, Clone, Copy, Default)]
struct Stick {
    x: f32,
    y: f32,
    /// The direction it holds and since when; the last repeat.
    held: Option<(Dir8, f64)>,
    last: f64,
    /// The held key has gone out as a held key (echo).
    echoed: bool,
    /// It picked a radial entry: nothing until it is back in the centre.
    locked: bool,
}

/// The pad's state and its translation.
#[derive(Debug, Default)]
pub struct Pad {
    left: Stick,
    right: Stick,
    dpad: [bool; 4],
    lb: bool,
    rb: bool,
    /// A bumper was used with a face button or the d-pad: letting it go
    /// alone is not a filter change.
    bumper_used: bool,
    page: usize,
    lt: bool,
    rt: bool,
    radial: Option<Option<usize>>,
    /// The pad gave the last input (its glyphs show).
    pub active: bool,
    pub kind: PadKind,
}

impl Pad {
    pub fn new() -> Pad {
        Pad::default()
    }

    /// The bar page (0: slots 1–8, 1: 9 and 0).
    pub fn page(&self) -> usize {
        self.page
    }

    /// The slot a bumper and a face button use on `page`.
    pub fn slot_for(page: usize, rb: bool, face: PadButton) -> Option<usize> {
        let i = match face {
            PadButton::A => 0,
            PadButton::B => 1,
            PadButton::X => 2,
            PadButton::Y => 3,
            _ => return None,
        };
        let slot = page * 8 + usize::from(rb) * 4 + i;
        (slot < crate::action_bar::SLOTS).then_some(slot)
    }

    /// The bumper and face button of `slot` (for its label).
    pub fn chord_of(slot: usize) -> (usize, bool, PadButton) {
        let page = slot / 8;
        let rb = slot % 8 >= 4;
        let face = [PadButton::A, PadButton::B, PadButton::X, PadButton::Y][slot % 4];
        (page, rb, face)
    }

    /// A button pressed or let go.
    pub fn button(&mut self, b: PadButton, pressed: bool, ctx: PadCtx, now: f64) -> Vec<PadOut> {
        self.active = true;
        use PadButton::*;
        let mut out = Vec::new();
        match b {
            Up | Down | Left | Right => {
                let i = [Up, Down, Left, Right]
                    .iter()
                    .position(|d| *d == b)
                    .unwrap_or(0);
                // a bumper and ← → turn the bar's page
                if pressed && (self.lb || self.rb) && matches!(b, Left | Right) {
                    self.bumper_used = true;
                    self.page = 1 - self.page;
                    out.push(PadOut::Page(self.page));
                    return out;
                }
                self.dpad[i] = pressed;
                let x = f32::from(u8::from(self.dpad[3])) - f32::from(u8::from(self.dpad[2]));
                let y = f32::from(u8::from(self.dpad[1])) - f32::from(u8::from(self.dpad[0]));
                let mut st = self.left;
                st.x = x;
                st.y = y;
                self.left = st;
                self.stick_moved(true, ctx, now, &mut out);
                return out;
            }
            Lb | Rb => {
                if b == Lb {
                    self.lb = pressed;
                } else {
                    self.rb = pressed;
                }
                if pressed {
                    self.bumper_used = false;
                    return out;
                }
                if self.bumper_used {
                    return out;
                }
                // a bumper alone: filters in the inventory, pages in menus
                match ctx {
                    PadCtx::PanelBrowse | PadCtx::PanelSelect | PadCtx::PanelMenu => {
                        out.push(PadOut::Key(KeyInput {
                            key: Key::Tab,
                            mods: Mods {
                                shift: b == Lb,
                                ..Mods::default()
                            },
                            echo: false,
                        }));
                    }
                    PadCtx::Menu { .. } | PadCtx::Other => {
                        out.push(PadOut::Key(key(if b == Lb {
                            Key::PageUp
                        } else {
                            Key::PageDown
                        })));
                    }
                    _ => {}
                }
                return out;
            }
            _ => {}
        }
        if !pressed {
            return out;
        }
        // a bumper held: the face buttons are the action bar
        if (self.lb || self.rb) && ctx == PadCtx::World && matches!(b, A | B | X | Y) {
            self.bumper_used = true;
            if let Some(slot) = Pad::slot_for(self.page, self.rb, b) {
                out.push(PadOut::Slot(slot));
            }
            return out;
        }
        let o = match (b, ctx) {
            (A, PadCtx::World) => PadOut::Activate,
            (A, PadCtx::Getpos) => PadOut::Key(ch('.')),
            (A, PadCtx::Direction) => PadOut::Key(ch('.')),
            (A, PadCtx::Menu { any: true }) | (A, PadCtx::PanelMenu) => PadOut::Key(ch(' ')),
            (A, PadCtx::Message { letter }) => PadOut::Key(ch(letter)),
            (A, PadCtx::Text) => PadOut::Osk(OskOp::Press),
            (A, _) => PadOut::Key(key(Key::Enter)),
            (B, PadCtx::Text) => PadOut::Osk(OskOp::Back),
            (B, _) => PadOut::Key(key(Key::Escape)),
            (X, PadCtx::World) => PadOut::Key(ch('s')),
            (X, PadCtx::PanelBrowse) => PadOut::Key(ch(' ')),
            (X, _) => return out,
            (Y, PadCtx::World) => PadOut::Key(ch('i')),
            (Y, PadCtx::PanelBrowse) => PadOut::Carry,
            (Y, PadCtx::Text) => PadOut::Osk(OskOp::Layout),
            (Y, _) => return out,
            (Start, PadCtx::World) => PadOut::Key(ch('#')),
            (Start, PadCtx::Text) => PadOut::Osk(OskOp::Submit),
            (Start, PadCtx::PanelMenu | PadCtx::Menu { .. }) => PadOut::Key(key(Key::Enter)),
            (Start, _) => PadOut::Key(key(Key::Enter)),
            (Back, _) => PadOut::History,
            (R3, _) => PadOut::CursorHome,
            (L3, PadCtx::World) => PadOut::Key(key(Key::F(8))),
            _ => return out,
        };
        out.push(o);
        out
    }

    /// An axis moved.
    pub fn axis(&mut self, a: JoyAxis, v: f32, ctx: PadCtx, now: f64) -> Vec<PadOut> {
        let mut out = Vec::new();
        match a {
            JoyAxis::LEFT_X | JoyAxis::LEFT_Y => {
                if a == JoyAxis::LEFT_X {
                    self.left.x = v;
                } else {
                    self.left.y = v;
                }
                if v.abs() > DEAD_ZONE {
                    self.active = true;
                }
                if self.radial.is_some() {
                    self.radial_hover(&mut out);
                } else {
                    self.stick_moved(true, ctx, now, &mut out);
                }
            }
            JoyAxis::RIGHT_X | JoyAxis::RIGHT_Y => {
                if a == JoyAxis::RIGHT_X {
                    self.right.x = v;
                } else {
                    self.right.y = v;
                }
                if v.abs() > DEAD_ZONE {
                    self.active = true;
                }
                if self.radial.is_some() {
                    self.radial_hover(&mut out);
                } else {
                    self.stick_moved(false, ctx, now, &mut out);
                }
            }
            JoyAxis::TRIGGER_LEFT => {
                let on = v > TRIGGER_ON;
                if on == self.lt {
                    return out;
                }
                self.lt = on;
                self.active = true;
                if on && ctx == PadCtx::World {
                    self.radial = Some(None);
                    out.push(PadOut::RadialOpen);
                } else if !on && let Some(sel) = self.radial.take() {
                    self.left.locked = true;
                    self.right.locked = true;
                    self.left.held = None;
                    self.right.held = None;
                    out.push(PadOut::RadialClose(sel));
                }
            }
            JoyAxis::TRIGGER_RIGHT => {
                let on = v > TRIGGER_ON;
                if on == self.rt {
                    return out;
                }
                self.rt = on;
                self.active = true;
                if on && ctx == PadCtx::World {
                    out.push(PadOut::Key(ch('f')));
                }
            }
            _ => {}
        }
        out
    }

    fn radial_hover(&mut self, out: &mut Vec<PadOut>) {
        // either stick picks; the one pushed further wins
        let (l, r) = (self.left, self.right);
        let (x, y) = if l.x.hypot(l.y) >= r.x.hypot(r.y) {
            (l.x, l.y)
        } else {
            (r.x, r.y)
        };
        let sel = radial_pick(x, y);
        if self.radial != Some(sel) {
            self.radial = Some(sel);
            out.push(PadOut::RadialHover(sel));
        }
    }

    /// A stick's direction may have changed: press, let go, step.
    fn stick_moved(&mut self, left: bool, ctx: PadCtx, now: f64, out: &mut Vec<PadOut>) {
        let st = if left { self.left } else { self.right };
        let world_cursor = !left && ctx == PadCtx::World;
        let dir = if ctx.walks() {
            Dir8::of(st.x, st.y)
        } else {
            Dir8::cardinal(st.x, st.y)
        };
        if st.locked {
            if dir.is_none() {
                let st = if left {
                    &mut self.left
                } else {
                    &mut self.right
                };
                st.locked = false;
            }
            return;
        }
        let old = st.held.map(|(d, _)| d);
        if dir == old {
            return;
        }
        // the old direction is let go
        if let Some(d) = old
            && ctx == PadCtx::World
            && !world_cursor
        {
            out.push(PadOut::KeyUp(key(d.key())));
        }
        let st = if left {
            &mut self.left
        } else {
            &mut self.right
        };
        st.held = dir.map(|d| (d, now));
        st.last = now;
        st.echoed = false;
        if let Some(d) = dir {
            out.push(if world_cursor {
                let (dx, dy) = d.delta();
                PadOut::Cursor(dx, dy)
            } else {
                nav_out(ctx, d)
            });
        }
    }

    /// The time passes: held directions repeat.
    pub fn tick(&mut self, ctx: PadCtx, now: f64) -> Vec<PadOut> {
        let mut out = Vec::new();
        if self.radial.is_some() {
            return out;
        }
        for left in [true, false] {
            let st = if left { self.left } else { self.right };
            let Some((d, since)) = st.held else {
                continue;
            };
            let world_cursor = !left && ctx == PadCtx::World;
            let st = if left {
                &mut self.left
            } else {
                &mut self.right
            };
            // the first repeat waits longer than the next ones
            let first = st.last == since;
            if ctx == PadCtx::World && !world_cursor {
                // walking: held, the key goes out once more as held, and
                // the order walks on until it is let go
                if !st.echoed && now - since >= HOLD_SECS {
                    st.echoed = true;
                    out.push(PadOut::Key(KeyInput {
                        echo: true,
                        ..key(d.key())
                    }));
                }
                continue;
            }
            let every = if !world_cursor {
                NAV_EVERY
            } else if st.x.hypot(st.y) > 0.9 {
                CURSOR_FAST
            } else {
                CURSOR_SLOW
            };
            let wait = if first { NAV_FIRST } else { every };
            if now - st.last >= wait {
                st.last = now;
                out.push(if world_cursor {
                    let (dx, dy) = d.delta();
                    PadOut::Cursor(dx, dy)
                } else {
                    nav_out(ctx, d)
                });
            }
        }
        out
    }

    /// Forget held buttons (the window lost the focus, a dialog changed
    /// what the sticks do).
    pub fn release_all(&mut self) {
        self.left = Stick::default();
        self.right = Stick::default();
        self.dpad = [false; 4];
        self.lb = false;
        self.rb = false;
        self.radial = None;
    }
}

/// A step of the focus: on the on-screen keyboard its own move, else
/// the direction's key.
fn nav_out(ctx: PadCtx, d: Dir8) -> PadOut {
    if ctx == PadCtx::Text {
        let (dx, dy) = d.delta();
        PadOut::Osk(OskOp::Move(dx, dy))
    } else {
        PadOut::Key(key(d.key()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticks_snap_to_eight_sectors_past_the_dead_zone() {
        assert_eq!(Dir8::of(0.1, 0.1), None);
        assert_eq!(Dir8::of(1.0, 0.0), Some(Dir8(4)));
        assert_eq!(Dir8::of(-1.0, 0.0), Some(Dir8(0)));
        assert_eq!(Dir8::of(0.0, -1.0), Some(Dir8(2)));
        assert_eq!(Dir8::of(0.0, 1.0), Some(Dir8(6)));
        assert_eq!(Dir8::of(0.7, -0.7), Some(Dir8(3)));
        assert_eq!(Dir8::of(-0.7, 0.7), Some(Dir8(7)));
        // 20° off east is still east; 30° is north-east
        let r = |deg: f32| (deg.to_radians().cos(), -deg.to_radians().sin());
        let (x, y) = r(20.0);
        assert_eq!(Dir8::of(x, y), Some(Dir8(4)));
        let (x, y) = r(30.0);
        assert_eq!(Dir8::of(x, y), Some(Dir8(3)));
        assert_eq!(Dir8::cardinal(0.7, -0.6), Some(Dir8(4)));
        assert_eq!(Dir8(5).key(), Key::PageDown);
        assert_eq!(Dir8(1).delta(), (-1, -1));
    }

    #[test]
    fn walking_presses_then_holds_then_lets_go() {
        let mut p = Pad::new();
        let out = p.axis(JoyAxis::LEFT_X, 1.0, PadCtx::World, 0.0);
        assert_eq!(out, vec![PadOut::Key(key(Key::Right))]);
        assert!(p.tick(PadCtx::World, 0.1).is_empty());
        let out = p.tick(PadCtx::World, 0.35);
        assert_eq!(
            out,
            vec![PadOut::Key(KeyInput {
                echo: true,
                ..key(Key::Right)
            })]
        );
        assert!(p.tick(PadCtx::World, 0.8).is_empty(), "held once is enough");
        let out = p.axis(JoyAxis::LEFT_X, 0.0, PadCtx::World, 1.0);
        assert_eq!(out, vec![PadOut::KeyUp(key(Key::Right))]);
    }

    #[test]
    fn menus_step_the_focus_and_repeat() {
        let mut p = Pad::new();
        let ctx = PadCtx::Menu { any: true };
        let out = p.button(PadButton::Down, true, ctx, 0.0);
        assert_eq!(out, vec![PadOut::Key(key(Key::Down))]);
        assert!(p.tick(ctx, 0.2).is_empty());
        assert_eq!(p.tick(ctx, 0.4), vec![PadOut::Key(key(Key::Down))]);
        assert_eq!(p.tick(ctx, 0.55), vec![PadOut::Key(key(Key::Down))]);
        p.button(PadButton::Down, false, ctx, 0.6);
        assert!(p.tick(ctx, 1.0).is_empty());
        // A toggles in a pick-any menu, Start confirms
        assert_eq!(
            p.button(PadButton::A, true, ctx, 1.1),
            vec![PadOut::Key(ch(' '))]
        );
        assert_eq!(
            p.button(PadButton::Start, true, ctx, 1.2),
            vec![PadOut::Key(key(Key::Enter))]
        );
    }

    #[test]
    fn bumpers_and_face_buttons_are_the_bar() {
        let mut p = Pad::new();
        let w = PadCtx::World;
        p.button(PadButton::Lb, true, w, 0.0);
        assert_eq!(p.button(PadButton::B, true, w, 0.0), vec![PadOut::Slot(1)]);
        p.button(PadButton::Lb, false, w, 0.1);
        p.button(PadButton::Rb, true, w, 0.2);
        assert_eq!(p.button(PadButton::Y, true, w, 0.2), vec![PadOut::Slot(7)]);
        assert_eq!(
            p.button(PadButton::Right, true, w, 0.3),
            vec![PadOut::Page(1)]
        );
        p.button(PadButton::Rb, false, w, 0.4);
        p.button(PadButton::Lb, true, w, 0.5);
        assert_eq!(p.button(PadButton::A, true, w, 0.5), vec![PadOut::Slot(8)]);
        assert_eq!(p.button(PadButton::B, true, w, 0.5), vec![PadOut::Slot(9)]);
        assert!(
            p.button(PadButton::X, true, w, 0.5).is_empty(),
            "no slot 11"
        );
        assert_eq!(Pad::chord_of(9), (1, false, PadButton::B));
        assert_eq!(Pad::chord_of(5), (0, true, PadButton::B));
        // alone, the face buttons are A, B, X, Y
        p.button(PadButton::Lb, false, w, 0.6);
        assert_eq!(
            p.button(PadButton::X, true, w, 0.7),
            vec![PadOut::Key(ch('s'))]
        );
        assert_eq!(
            p.button(PadButton::Y, true, w, 0.7),
            vec![PadOut::Key(ch('i'))]
        );
        // a bumper alone in the inventory turns the filter
        p.button(PadButton::Rb, true, PadCtx::PanelBrowse, 0.8);
        let out = p.button(PadButton::Rb, false, PadCtx::PanelBrowse, 0.9);
        assert!(matches!(&out[..], [PadOut::Key(k)] if k.key == Key::Tab && !k.mods.shift));
    }

    #[test]
    fn the_right_stick_moves_the_cursor_and_the_trigger_opens_the_radial() {
        let mut p = Pad::new();
        let w = PadCtx::World;
        assert_eq!(
            p.axis(JoyAxis::RIGHT_Y, -1.0, w, 0.0),
            vec![PadOut::Cursor(0, -1)]
        );
        assert!(p.tick(w, 0.2).is_empty());
        assert_eq!(p.tick(w, 0.5), vec![PadOut::Cursor(0, -1)]);
        p.axis(JoyAxis::RIGHT_Y, 0.0, w, 0.6);
        assert_eq!(
            p.axis(JoyAxis::TRIGGER_LEFT, 1.0, w, 1.0),
            vec![PadOut::RadialOpen]
        );
        // east is the third entry clockwise from the top
        let out = p.axis(JoyAxis::LEFT_X, 1.0, w, 1.1);
        assert_eq!(out, vec![PadOut::RadialHover(Some(2))]);
        assert_eq!(
            p.axis(JoyAxis::TRIGGER_LEFT, 0.0, w, 1.2),
            vec![PadOut::RadialClose(Some(2))]
        );
        // the stick that picked does not walk on its way back
        assert!(p.axis(JoyAxis::LEFT_Y, -0.5, w, 1.3).is_empty());
        assert!(p.axis(JoyAxis::LEFT_X, 0.0, w, 1.3).is_empty());
        assert!(p.axis(JoyAxis::LEFT_Y, 0.0, w, 1.4).is_empty());
        assert_eq!(
            p.axis(JoyAxis::LEFT_X, 1.0, w, 1.5),
            vec![PadOut::Key(key(Key::Right))]
        );
        assert_eq!(radial_pick(0.0, -1.0), Some(0));
        assert_eq!(radial_pick(-0.8, -0.8), Some(7));
        assert_eq!(radial_pick(0.1, 0.0), None);
    }

    #[test]
    fn controllers_are_told_apart_by_name() {
        assert_eq!(
            PadKind::from_name("Xbox Series X Controller"),
            PadKind::Xbox
        );
        assert_eq!(
            PadKind::from_name("DualSense Wireless Controller"),
            PadKind::PlayStation
        );
        assert_eq!(PadKind::from_name("Steam Deck"), PadKind::Deck);
        assert_eq!(PadKind::PlayStation.label(PadButton::A), "✕");
        assert_eq!(PadKind::Xbox.label(PadButton::Lb), "LB");
        assert_eq!(PadKind::Deck.label(PadButton::Rb), "R1");
    }
}
