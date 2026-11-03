//! The command menu as a wheel: the other face of `#`'s request, beside
//! the list (`Palette`). Its first ring is the menu's sections and "All
//! commands…" (the list); a section picked turns the wheel into a ring of
//! its commands, as many as a ring holds big and labelled, and "More…"
//! (the list, that section alone open) when it has more. Every command
//! stays reachable as NetHack has it: through a ring, a "More…" or the
//! list.
//!
//! It opens held (Tab or the middle mouse button pressed at the command
//! prompt: letting go on a wedge picks it, in the middle cancels; a tap
//! leaves it open) or open (`#` or the Commands button with the wheel
//! chosen in the settings, a pad's "Commands" on LT's radial): then a
//! click, Enter or a pad's A picks. The mouse points at a wedge, a stick
//! too; the arrows and Tab step round; Esc, a right click or a pad's B
//! cancels, Backspace goes back to the sections.

use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{ColorRect, Control, Label, ShaderMaterial, TextureRect, VBoxContainer};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_world::Key;

use crate::command_menu::{self, Section};
use crate::dialogs::{PaletteCmd, palette_row_words};
use crate::i18n;
use crate::icons::{self, Glyph};
use crate::theme::{self, Face};
use crate::tr;

/// The sections the first ring offers, clockwise from the top, before
/// "All commands…" (the moves are the list's: a key or the stick does
/// them better).
pub const RING_SECTIONS: [Section; 5] = [
    Section::Common,
    Section::Items,
    Section::Actions,
    Section::Info,
    Section::Game,
];

/// The most wedges a ring has: a section with more shows one fewer of
/// its commands and "More…".
pub const MOST: usize = 12;

/// The wheel's size and its ring's radii (shares of the half size, as
/// `ui_radial.gdshader` has them), and a wedge's icon and caption box,
/// in design pixels.
pub const RING: f32 = 600.0;
const INNER: f32 = 0.4;
const OUTER: f32 = 0.97;
const CELL: Vector2 = Vector2::new(118.0, 78.0);
const ICON: f32 = 36.0;
/// Let go this soon after it opened, nothing pointed at: a tap, the
/// wheel stays open.
pub const TAP_SECS: f32 = 0.3;

/// A wedge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelItem {
    /// A section: its ring.
    Section(Section),
    /// The list, every section.
    All,
    /// A command (an index into the commands).
    Cmd(usize),
    /// The list, this section alone open.
    More(Section),
}

/// The ring shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ring {
    Sections,
    Of(Section),
}

/// What a pick did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WheelAct {
    /// Nothing to answer yet (a section's ring shown, a step round).
    Stay,
    /// Run this command.
    Run(String),
    /// The list instead, with one section alone open, or all.
    List(Option<Section>),
    Cancel,
}

/// The wedges of `ring`: the sections and "All commands…", or a
/// section's commands in the menu's order (#pay first while goods are
/// owed), the last wedge "More…" when they do not all fit.
pub fn ring_items(ring: Ring, cmds: &[PaletteCmd], owes: bool, pad: bool) -> Vec<WheelItem> {
    match ring {
        Ring::Sections => RING_SECTIONS
            .iter()
            .map(|&s| WheelItem::Section(s))
            .chain(std::iter::once(WheelItem::All))
            .collect(),
        Ring::Of(s) => {
            let laid = command_menu::sections(cmds, owes, pad);
            let idx = laid
                .into_iter()
                .find(|(t, _)| *t == s)
                .map(|(_, i)| i)
                .unwrap_or_default();
            if idx.len() <= MOST {
                idx.into_iter().map(WheelItem::Cmd).collect()
            } else {
                (idx.into_iter().take(MOST - 1).map(WheelItem::Cmd))
                    .chain(std::iter::once(WheelItem::More(s)))
                    .collect()
            }
        }
    }
}

/// The wedge of `count` (the first at the top, clockwise) at `d` from the
/// middle; none within `dead` of it.
pub fn wedge_at(count: usize, d: Vector2, dead: f32) -> Option<usize> {
    if count == 0 || d.length() < dead {
        return None;
    }
    let seg = 360.0 / count as f32;
    let a = d.x.atan2(-d.y).to_degrees();
    let a = (a + 360.0 + seg / 2.0) % 360.0;
    Some(((a / seg).floor() as usize).min(count - 1))
}

/// A command's word on a wedge: its short name, else what it does.
fn short_name(c: &PaletteCmd) -> String {
    let short = tr!("wheel-cmd", cmd = c.name.as_str());
    if short.trim() == "-" {
        palette_row_words(&c.desc)
    } else {
        short
    }
}

/// A command's emblem on a wedge (the action bar's where it has one).
fn cmd_glyph(name: &str) -> Glyph {
    match name {
        "pray" | "offer" | "turn" => Glyph::Ankh,
        "chat" | "untrap" | "wipe" => Glyph::Hand,
        "pay" | "showgold" => Glyph::Coins,
        "search" => Glyph::Lens,
        "wait" | "sit" => Glyph::Moon,
        "kick" | "jump" => Glyph::Boots,
        "open" | "close" | "loot" | "force" | "tip" | "pickup" | "drop" | "droptype"
        | "inventory" | "inventtype" | "adjust" => Glyph::Sack,
        "eat" => Glyph::Drumstick,
        "quaff" | "dip" => Glyph::Flask,
        "read" | "annotate" | "chronicle" | "save" | "saveoptions" => Glyph::Scroll,
        "wear" | "takeoff" | "takeoffall" | "seearmor" | "attributes" => Glyph::Cuirass,
        "wield" | "seeweapon" | "vanquished" | "genocided" => Glyph::Sword,
        "throw" => Glyph::Drop,
        "fire" | "quiver" => Glyph::Quiver,
        "zap" | "engrave" => Glyph::Wand,
        "puton" | "remove" | "seerings" => Glyph::Ring,
        "seeamulet" => Glyph::Amulet,
        "swap" | "twoweapon" => Glyph::Swap,
        "invoke" => Glyph::Gem,
        "rub" | "seetools" => Glyph::Lamp,
        "ride" => Glyph::Leash,
        "apply" | "monster" => Glyph::Burst,
        "cast" | "showspells" | "known" | "knownclass" | "help" | "?" => Glyph::Book,
        "enhance" | "conduct" => Glyph::Star,
        "overview" | "up" | "down" | "travel" | "retravel" | "teleport" => Glyph::Stairs,
        "terrain" | "look" | "lookaround" | "glance" | "whatis" | "whatdoes" | "showtrap" => {
            Glyph::Eye
        }
        "seeall" => Glyph::Check,
        "quit" => Glyph::Flames,
        _ => Glyph::Grid,
    }
}

fn section_glyph(s: Section) -> Glyph {
    match s {
        Section::Common => Glyph::Star,
        Section::Items => Glyph::Sack,
        Section::Actions => Glyph::Hand,
        Section::Info => Glyph::Eye,
        Section::Game => Glyph::Scroll,
        Section::Moves => Glyph::Boots,
    }
}

/// A wedge's caption box: its emblem over its word.
struct Cell {
    col: Gd<VBoxContainer>,
    icon: Gd<TextureRect>,
    word: Gd<Label>,
}

/// The wheel on screen, its rings and what it points at.
pub struct CommandWheel {
    material: Option<Gd<ShaderMaterial>>,
    cells: Vec<Cell>,
    title: Gd<Label>,
    hint: Gd<Label>,
    cmds: Vec<PaletteCmd>,
    owes: bool,
    pad: bool,
    ring: Ring,
    items: Vec<WheelItem>,
    hover: Option<usize>,
    /// Opened held (Tab, the middle button): letting go picks.
    held: bool,
    opened: std::time::Instant,
    /// Its middle on the canvas, and its size there (a share of `RING`).
    centre: Vector2,
    scale: f32,
}

impl CommandWheel {
    /// The wheel in `root` (the ring's square, `RING` wide, its middle at
    /// `centre` on the canvas), on the sections' ring.
    pub fn new(
        mut root: Gd<Control>,
        centre: Vector2,
        scale: f32,
        cmds: Vec<PaletteCmd>,
        owes: bool,
        pad: bool,
        held: bool,
    ) -> CommandWheel {
        root.set_mouse_filter(MouseFilter::IGNORE);
        let mut ring = ColorRect::new_alloc();
        ring.set_mouse_filter(MouseFilter::IGNORE);
        ring.set_size(Vector2::new(RING, RING));
        // its own material: the count and the wedge lit are this wheel's
        let material = theme::ui_material("ui_radial")
            .and_then(|m| m.duplicate_resource().try_cast::<ShaderMaterial>().ok());
        match &material {
            Some(m) => {
                let mut m = m.clone();
                m.set_shader_parameter("inner", &INNER.to_variant());
                m.set_shader_parameter("outer", &OUTER.to_variant());
                ring.set_material(&m);
            }
            None => ring.set_color(Color::from_rgba(0.07, 0.055, 0.04, 0.8)),
        }
        root.add_child(&ring);
        let mut cells = Vec::with_capacity(MOST);
        for _ in 0..MOST {
            let mut col = VBoxContainer::new_alloc();
            col.set_mouse_filter(MouseFilter::IGNORE);
            col.add_theme_constant_override("separation", 2);
            col.set_size(CELL);
            col.set_pivot_offset(CELL * 0.5);
            let mut icon = TextureRect::new_alloc();
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_custom_minimum_size(Vector2::new(ICON, ICON));
            icon.set_h_size_flags(SizeFlags::SHRINK_CENTER);
            col.add_child(&icon);
            let mut word = theme::styled_label("", Face::BodyBold, 14, theme::TEXT);
            theme::outline(&word, 4);
            word.set_horizontal_alignment(HorizontalAlignment::CENTER);
            word.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
            word.set_custom_minimum_size(Vector2::new(CELL.x, 0.0));
            col.add_child(&word);
            col.set_visible(false);
            root.add_child(&col);
            cells.push(Cell { col, icon, word });
        }
        let mut middle = VBoxContainer::new_alloc();
        middle.set_mouse_filter(MouseFilter::IGNORE);
        middle.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        let inner_w = RING * INNER * 0.86;
        middle.set_size(Vector2::new(inner_w, inner_w));
        middle.set_position(Vector2::new(RING - inner_w, RING - inner_w) * 0.5);
        let mut title = theme::styled_label("", Face::Title, 20, theme::GOLD_BRIGHT);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        title.set_custom_minimum_size(Vector2::new(inner_w, 0.0));
        middle.add_child(&title);
        let mut hint = theme::styled_label("", Face::Body, 13, theme::TEXT_DIM);
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        hint.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        hint.set_custom_minimum_size(Vector2::new(inner_w, 0.0));
        // the command's key, as the player types it
        i18n::verbatim(&hint);
        middle.add_child(&hint);
        root.add_child(&middle);
        let mut w = CommandWheel {
            material,
            cells,
            title,
            hint,
            cmds,
            owes,
            pad,
            ring: Ring::Sections,
            items: Vec::new(),
            hover: None,
            held,
            opened: std::time::Instant::now(),
            centre,
            scale,
        };
        w.show_ring(Ring::Sections);
        w
    }

    /// `ring` on the wheel, nothing pointed at.
    fn show_ring(&mut self, ring: Ring) {
        self.ring = ring;
        self.items = ring_items(ring, &self.cmds, self.owes, self.pad);
        let n = self.items.len().max(1);
        if let Some(m) = self.material.as_mut() {
            m.set_shader_parameter("count", &(n as i32).to_variant());
        }
        let mid = RING / 2.0 * (INNER + OUTER) / 2.0;
        let items = self.items.clone();
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let Some(item) = items.get(i) else {
                cell.col.set_visible(false);
                continue;
            };
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            let c = Vector2::new(RING / 2.0, RING / 2.0) + Vector2::new(a.sin(), -a.cos()) * mid;
            cell.col.set_position(c - CELL * 0.5);
            let (glyph, word) = match *item {
                WheelItem::Section(s) => (section_glyph(s), i18n::tr(s.title_id())),
                WheelItem::All => (Glyph::List, tr!("wheel-all")),
                WheelItem::More(_) => (Glyph::List, tr!("wheel-more")),
                WheelItem::Cmd(c) => (cmd_glyph(&self.cmds[c].name), short_name(&self.cmds[c])),
            };
            cell.icon.set_texture(&icons::emblem(glyph));
            cell.word.set_text(&word);
            cell.col.set_visible(true);
        }
        self.set_hover(None);
    }

    /// The wedge pointed at.
    fn set_hover(&mut self, hover: Option<usize>) {
        self.hover = hover.filter(|&i| i < self.items.len());
        if let Some(m) = self.material.as_mut() {
            let i = self.hover.map_or(-1, |i| i as i32);
            m.set_shader_parameter("selected", &i.to_variant());
        }
        for (i, cell) in self.cells.iter_mut().enumerate() {
            let on = Some(i) == self.hover;
            cell.col.set_modulate(if on {
                Color::from_rgb(1.0, 0.95, 0.8)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.8)
            });
            cell.col.set_scale(if on {
                Vector2::new(1.1, 1.1)
            } else {
                Vector2::ONE
            });
        }
        let (title, hint) = match self.hover.and_then(|i| self.items.get(i)) {
            None => match self.ring {
                Ring::Sections => (tr!("palette-title"), tr!("wheel-hint")),
                Ring::Of(s) => (i18n::tr(s.title_id()), tr!("wheel-hint")),
            },
            Some(WheelItem::Section(s)) => (i18n::tr(s.title_id()), tr!("wheel-section-hint")),
            Some(WheelItem::All) => (tr!("wheel-all"), tr!("wheel-all-hint")),
            Some(WheelItem::More(s)) => (tr!("wheel-more"), i18n::tr(s.title_id())),
            Some(WheelItem::Cmd(c)) => {
                let c = &self.cmds[*c];
                let key = if self.pad || c.key.is_empty() {
                    format!("#{}", c.name)
                } else {
                    format!("#{} · {}", c.name, c.key)
                };
                (palette_row_words(&c.desc), key)
            }
        };
        self.title.set_text(&title);
        self.hint.set_text(&hint);
    }

    /// The mouse at `at` on the canvas: the wedge it points at.
    pub fn point(&mut self, at: Vector2) {
        let d = at - self.centre();
        let hover = wedge_at(self.items.len(), d, RING / 2.0 * INNER * self.scale);
        if hover != self.hover {
            self.set_hover(hover);
        }
    }

    /// A stick pushed to (`x`, `y`): the wedge it points at (let go, the
    /// wedge stays).
    pub fn aim(&mut self, x: f32, y: f32) {
        if let Some(i) = wedge_at(self.items.len(), Vector2::new(x, y), 0.5)
            && Some(i) != self.hover
        {
            self.set_hover(Some(i));
        }
    }

    /// The wheel's middle on the canvas.
    pub fn centre(&self) -> Vector2 {
        self.centre
    }

    /// Its size on the canvas, a share of `RING`.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The wedge `d` round from the one pointed at (the first, or the
    /// last, when none is).
    pub fn step(&mut self, d: i32) {
        let n = self.items.len() as i32;
        if n == 0 {
            return;
        }
        let next = match self.hover {
            Some(i) => (i as i32 + d).rem_euclid(n),
            None if d > 0 => 0,
            None => n - 1,
        };
        self.set_hover(Some(next as usize));
    }

    /// The wedge pointed at picked: a section's ring, or what the game is
    /// to do; nothing pointed at cancels.
    pub fn pick(&mut self) -> WheelAct {
        match self.hover.and_then(|i| self.items.get(i)).copied() {
            None => WheelAct::Cancel,
            Some(WheelItem::Section(s)) => {
                self.held = false;
                self.show_ring(Ring::Of(s));
                WheelAct::Stay
            }
            Some(WheelItem::All) => WheelAct::List(None),
            Some(WheelItem::More(s)) => WheelAct::List(Some(s)),
            Some(WheelItem::Cmd(c)) => WheelAct::Run(self.cmds[c].name.clone()),
        }
    }

    /// The key or button that opened it let go: a pick, a tap (it stays
    /// open), or a cancel (let go in the middle).
    pub fn let_go(&mut self) -> WheelAct {
        if !self.held {
            return WheelAct::Stay;
        }
        self.held = false;
        if self.hover.is_none() && self.opened.elapsed().as_secs_f32() < TAP_SECS {
            return WheelAct::Stay;
        }
        self.pick()
    }

    /// Back to the sections' ring; on it, a cancel.
    pub fn back(&mut self) -> WheelAct {
        match self.ring {
            Ring::Sections => WheelAct::Cancel,
            Ring::Of(_) => {
                self.show_ring(Ring::Sections);
                WheelAct::Stay
            }
        }
    }

    /// A key: Esc cancels, Backspace goes back, Enter picks, the arrows
    /// and Tab step round.
    pub fn key(&mut self, key: Key, shift: bool, echo: bool) -> WheelAct {
        match key {
            Key::Escape if !echo => WheelAct::Cancel,
            Key::Backspace if !echo => self.back(),
            Key::Enter | Key::KeypadEnter if !echo => self.pick(),
            Key::Right | Key::Down => {
                self.step(1);
                WheelAct::Stay
            }
            Key::Left | Key::Up => {
                self.step(-1);
                WheelAct::Stay
            }
            Key::Tab => {
                self.step(if shift { -1 } else { 1 });
                WheelAct::Stay
            }
            _ => WheelAct::Stay,
        }
    }

    /// Whether it was opened held and is not let go yet.
    pub fn held(&self) -> bool {
        self.held
    }

    /// The ring shown, its wedges' words, the one pointed at (self-tests).
    pub fn shown(&self) -> (Ring, Vec<String>, Option<usize>) {
        let words = (self.cells.iter().take(self.items.len()))
            .map(|c| c.word.get_text().to_string())
            .collect();
        (self.ring, words, self.hover)
    }

    /// Where wedge `i`'s caption is on screen (self-tests: the mouse goes
    /// there).
    pub fn wedge_rect(&self, i: usize) -> Option<Rect2> {
        (i < self.items.len()).then(|| {
            let col = &self.cells[i].col;
            let c = col.get_global_transform() * (CELL * 0.5);
            let size = CELL * self.scale;
            Rect2::new(c - size * 0.5, size)
        })
    }

    /// The wedges, as the game knows them (self-tests).
    pub fn items(&self) -> &[WheelItem] {
        &self.items
    }

    /// The commands it was made with (the list it gives way to).
    pub fn cmds(&self) -> &[PaletteCmd] {
        &self.cmds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(names: &[&str]) -> Vec<PaletteCmd> {
        names
            .iter()
            .map(|n| PaletteCmd {
                name: n.to_string(),
                desc: String::new(),
                key: String::new(),
                also: Vec::new(),
                prefix: false,
            })
            .collect()
    }

    #[test]
    fn the_first_ring_is_the_sections_and_the_list() {
        let c = cmds(&["pray", "save"]);
        let ring = ring_items(Ring::Sections, &c, false, false);
        assert_eq!(ring.len(), 6);
        assert_eq!(ring[0], WheelItem::Section(Section::Common));
        assert_eq!(ring[5], WheelItem::All);
        assert!(!ring.contains(&WheelItem::Section(Section::Moves)));
    }

    #[test]
    fn a_long_section_ends_in_more_and_pay_leads_while_owed() {
        let names = [
            "pray", "chat", "pay", "search", "wait", "open", "close", "kick", "loot", "eat",
            "quaff", "read", "wear", "takeoff", "wield", "apply",
        ];
        let c = cmds(&names);
        let ring = ring_items(Ring::Of(Section::Common), &c, false, false);
        assert_eq!(ring.len(), MOST);
        assert_eq!(ring[MOST - 1], WheelItem::More(Section::Common));
        assert_eq!(ring[0], WheelItem::Cmd(0));
        let owed = ring_items(Ring::Of(Section::Common), &c, true, false);
        assert_eq!(owed[0], WheelItem::Cmd(2));
        // a short one: all of it, no More
        let c = cmds(&["pray", "chat", "pay"]);
        let ring = ring_items(Ring::Of(Section::Common), &c, false, false);
        assert_eq!(ring, [0, 1, 2].map(WheelItem::Cmd));
    }

    #[test]
    fn a_wedge_is_found_clockwise_from_the_top() {
        let up = Vector2::new(0.0, -100.0);
        assert_eq!(wedge_at(6, up, 10.0), Some(0));
        assert_eq!(wedge_at(6, Vector2::new(100.0, -30.0), 10.0), Some(1));
        assert_eq!(wedge_at(6, Vector2::new(0.0, 100.0), 10.0), Some(3));
        assert_eq!(wedge_at(6, Vector2::new(-100.0, 10.0), 10.0), Some(4));
        assert_eq!(wedge_at(12, Vector2::new(-1.0, -100.0), 10.0), Some(0));
        assert_eq!(wedge_at(12, Vector2::new(3.0, 3.0), 10.0), None);
        assert_eq!(wedge_at(0, up, 10.0), None);
    }
}
