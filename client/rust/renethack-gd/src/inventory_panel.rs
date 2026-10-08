//! The inventory panel (ui-design §2, §3): NetHack's pack as a grid of
//! icons in pack order, a paper doll of what is worn and wielded, filter
//! tabs, the detail of the selected item, its context menu, drag and drop.
//!
//! It has three modes. **Browse** (`i`): clicks, drags and the menu run
//! macros at the command prompt, and command keys still go to the engine
//! as in tty. **Selection** (a getobj question): the suggested letters
//! pulse gold, the others are dimmed but clickable, a click answers with
//! the letter. **Menu** (an engine menu of inventory items: `D`, `A`...):
//! checkboxes, by NetHack's own `MenuState` rules.
//!
//! The panel never decides for the engine: it only turns clicks into the
//! keys a tty player would type (section 5), and shows only the doname
//! and what it says (no weight, no true name).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, Camera3D, CanvasLayer, ColorRect, Control, DirectionalLight3D, Environment,
    GridContainer, HBoxContainer, InputEvent, InputEventMouseButton, InputEventMouseMotion, Label,
    LineEdit, Node3D, PanelContainer, StyleBoxFlat, SubViewport, Texture2D, TextureRect, Time,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;
use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Case, Gender, Number};
use nh_protocol::{InvItem, MenuItem, PickHow, Reply, Slot};
use nh_world::{
    Buc, GridCell, InvFilter, ItemAction, ItemActionKind, ItemQuestion, Key, KeyInput, Macro,
    MenuEntry, MenuOutcome, MenuState, Pack, SlotBinding, actions_for, default_action, grid,
    parse_item_name, worn_accessory, worn_armor,
};

use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, EngineKind, Lang};
use crate::icons::{self, Glyph};
use crate::theme::{self, Face, Frame, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// The panel's design size (ui-design §2.2).
pub const WIDTH: f32 = 1416.0;
pub const HEIGHT: f32 = 760.0;
const HEADER: f32 = 52.0;
const PAD: f32 = 20.0;
const DOLL_W: f32 = 400.0;
const GRID_COL_W: f32 = 580.0;
const DETAIL_W: f32 = 356.0;
/// Cells: 8 × 7 of 64, gap 4.
const COLS: usize = 8;
const ROWS: usize = 7;
/// The fewest rows the grid shows.
const MIN_ROWS: usize = 3;
const CELL: f32 = 64.0;
const CELL_GAP: f32 = 4.0;
const GRID_W: f32 = COLS as f32 * CELL + (COLS - 1) as f32 * CELL_GAP;
const GRID_H: f32 = ROWS as f32 * CELL + (ROWS - 1) as f32 * CELL_GAP;
/// The bottom cluster of the HUD and the gap above it.
const CLUSTER_TOP: f32 = 188.0 + 8.0;
/// Double-click threshold.
const DOUBLE_SECS: f64 = 0.35;
/// Pixels the mouse moves before a press becomes a drag.
const DRAG_START: f32 = 6.0;
/// The suggested cells' pulse (ui-design §2.3: 1.0 s).
const PULSE_HZ: f64 = 1.0;

const BLESSED: Color = Color::from_rgb(0.373, 0.765, 0.851);
const UNCURSED: Color = Color::from_rgb(0.725, 0.663, 0.541);
const CURSED: Color = Color::from_rgb(0.753, 0.224, 0.169);
const PULSE_LOW: Color = theme::GOLD;
const PULSE_HIGH: Color = Color::from_rgb(0.941, 0.753, 0.376);

// ---- texts ----

/// The text of a label key ("item.wield", "cmd.pick_up", "inv.weapons")
/// in the language now; a key the catalogs lack reads as its humanized
/// name.
pub fn label(key: &str) -> String {
    let id = i18n::fluent_id(key);
    if i18n::has(&id) {
        return i18n::tr(&id);
    }
    let name = key.rsplit('.').next().unwrap_or(key).replace('_', " ");
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// What a class is called in the detail panel.
pub fn class_name(class: char) -> String {
    match class {
        ')' => tr!("class-weapon"),
        '[' => tr!("class-armor"),
        '=' => tr!("class-ring"),
        '"' => tr!("class-amulet"),
        '(' => tr!("class-tool"),
        '%' => tr!("class-comestible"),
        '!' => tr!("class-potion"),
        '?' => tr!("class-scroll"),
        '+' => tr!("class-spellbook"),
        '/' => tr!("class-wand"),
        '*' => tr!("class-gem"),
        '`' => tr!("class-boulder"),
        '0' => tr!("class-iron-ball"),
        '_' => tr!("class-iron-chain"),
        '.' => tr!("class-venom"),
        '$' => tr!("class-coins"),
        _ => tr!("class-item"),
    }
}

/// A line of the detail panel and its tone.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub text: String,
    pub color: Option<Color>,
}

fn fact(text: impl Into<String>) -> Fact {
    Fact {
        text: text.into(),
        color: None,
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The state words doname() writes before the name, as written.
const WORN_WORDS: [&str; 21] = [
    "rusty",
    "burnt",
    "corroded",
    "rotted",
    "rustproof",
    "fixed",
    "fireproof",
    "corrodeproof",
    "rotproof",
    "tempered",
    "cracked",
    "greased",
    "poisoned",
    "diluted",
    "empty",
    "locked",
    "unlocked",
    "broken",
    "trapped",
    "eaten",
    "used",
];

/// The parenthesized groups of a doname, first to last.
fn groups(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while let Some(open) = rest.strip_suffix(')').and_then(|r| r.rfind(" (")) {
        out.push(rest[open + 2..rest.len() - 1].to_string());
        rest = rest[..open].trim_end();
    }
    out.reverse();
    out
}

/// The parenthesized groups of an item's name as the player reads them.
/// The engine's translator says a group with the name it belongs to (a
/// state agrees with its item: надет, надета, надеты), so the name is
/// translated whole and its groups are taken from that; where that gives
/// other groups (a language that marks the whole name), each group alone.
fn shown_groups(text: &str, english: &[String]) -> Vec<String> {
    let whole = i18n::engine(EngineKind::Name, text);
    let shown = groups(&whole);
    if shown.len() == english.len() {
        return shown;
    }
    english
        .iter()
        .map(|g| i18n::engine(EngineKind::Name, g).into_owned())
        .collect()
}

/// The gender and number an item's own words take (Russian: its name's
/// noun in the lexicon; None in a language whose words do not agree, and
/// for a name the lexicon does not know).
fn agreement(item: &InvItem) -> Option<(Gender, Number)> {
    if i18n::lang() != Lang::Ru {
        return None;
    }
    let lex = Lexicon::ru();
    let head = lex.parse_object(&item.text)?.ru(lex).head;
    let plural = item.quan > 1 || head.plural_only;
    Some((
        head.gender,
        if plural { Number::Plur } else { Number::Sing },
    ))
}

/// The doname without its parenthesized groups.
fn bare(text: &str) -> &str {
    let mut rest = text.trim();
    while let Some(open) = rest.strip_suffix(')').and_then(|r| r.rfind(" (")) {
        rest = rest[..open].trim_end();
    }
    rest
}

/// The title of the detail panel: the doname without the article, the
/// count and the parentheses.
pub fn title_of(item: &InvItem) -> String {
    let rest = bare(&item.text);
    let mut words = rest.splitn(2, ' ');
    let first = words.next().unwrap_or("");
    let tail = words.next().unwrap_or("");
    let drop = matches!(first, "a" | "an" | "the" | "some") || first.parse::<i64>().is_ok();
    if drop && !tail.is_empty() {
        tail.to_string()
    } else {
        rest.to_string()
    }
}

/// What the doname says, one line each (ui-design §2.5): the curse
/// status, the enchantment, the state words, the usage in parentheses,
/// charges, shop prices, names. Nothing the doname does not say.
pub fn facts(item: &InvItem) -> Vec<Fact> {
    let name = parse_item_name(&item.text);
    let mut out = Vec::new();
    let english = groups(&item.text);
    let shown = shown_groups(&item.text, &english);
    for (g, shown) in english.iter().zip(shown) {
        if let Some((a, b)) = g.split_once(':')
            && let (Ok(r), Ok(n)) = (a.parse::<i32>(), b.parse::<i32>())
        {
            out.push(fact(tr!("fact-recharged", times = r, charges = n)));
        } else if let Ok(n) = g.parse::<i32>() {
            out.push(fact(tr!("fact-charges", n = n)));
        } else if g.starts_with("unpaid") || g.starts_with("for sale") || g == "no charge" {
            out.push(Fact {
                text: capitalized(&shown),
                color: Some(theme::WARN),
            });
        } else {
            out.push(Fact {
                text: capitalized(&shown),
                color: Some(theme::GOLD_BRIGHT),
            });
        }
    }
    // the curse status and the state words agree with the item
    let agrees = agreement(item);
    let form = match agrees {
        Some((_, Number::Plur)) => "pl",
        Some((Gender::Fem, _)) => "f",
        Some((Gender::Neut, _)) => "n",
        _ => "m",
    };
    match name.buc {
        Some(Buc::Blessed) => out.push(Fact {
            text: tr!("fact-blessed", form = form),
            color: Some(BLESSED),
        }),
        Some(Buc::Uncursed) => out.push(Fact {
            text: tr!("fact-uncursed", form = form),
            color: Some(UNCURSED),
        }),
        Some(Buc::Cursed) => out.push(Fact {
            text: tr!("fact-cursed", form = form),
            color: Some(CURSED),
        }),
        None => {}
    }
    if let Some(e) = name.enchantment {
        out.push(fact(tr!("fact-enchantment", value = format!("{e:+}"))));
    }
    // the state words, with their "very", "thoroughly", "partly"
    let words: Vec<&str> = bare(&item.text).split(' ').collect();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if WORN_WORDS.contains(&w) {
            let mut start = i;
            while start > 0 && matches!(words[start - 1], "very" | "thoroughly" | "partly") {
                start -= 1;
            }
            let state = words[start..=i].join(" ");
            let shown = match agrees.zip(Lexicon::ru().adjective(&state)) {
                Some(((g, num), a)) => a.form(g, num, false, Case::Nom).to_string(),
                None => i18n::engine(EngineKind::Name, &state).into_owned(),
            };
            out.push(fact(capitalized(&shown)));
        }
        if w == "named" || w == "called" || w == "containing" {
            break;
        }
        i += 1;
    }
    let text = bare(&item.text);
    if let Some(at) = text.find(" containing ") {
        let what = &text[at + 12..];
        // "containing 3 items": the count in the language's own plural
        let items = what
            .strip_suffix(" items")
            .or_else(|| what.strip_suffix(" item"))
            .and_then(|n| n.parse::<i64>().ok());
        match items {
            Some(n) => out.push(fact(tr!("fact-containing-items", n = n))),
            None => {
                let what = i18n::engine(EngineKind::Name, what).into_owned();
                out.push(fact(tr!("fact-containing", what = what)));
            }
        }
    }
    let stem = &name.stem;
    if let Some(at) = stem.find(" named ") {
        out.push(fact(tr!("fact-name", name = &stem[at + 7..])));
    }
    if let Some(at) = stem.find(" called ") {
        let end = stem
            .find(" named ")
            .filter(|&e| e > at)
            .unwrap_or(stem.len());
        out.push(fact(tr!("fact-called", name = &stem[at + 8..end])));
    }
    out
}

/// The small badge a cell shows for how an item is used.
fn state_glyph(item: &InvItem) -> Option<Glyph> {
    let has = |s: Slot| item.slots.contains(&s);
    if has(Slot::Weapon) {
        Some(Glyph::Sword)
    } else if has(Slot::Alternate) {
        Some(Glyph::Swap)
    } else if has(Slot::Quiver) {
        Some(Glyph::Quiver)
    } else if item.lit {
        Some(Glyph::Lamp)
    } else if worn_armor(item) || worn_accessory(item) || item.text.ends_with("(in use)") {
        Some(Glyph::Check)
    } else {
        None
    }
}

// ---- the paper doll ----

/// The doll's sockets (ui-design §2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DollSlot {
    Helmet,
    Cloak,
    Body,
    Shirt,
    Gloves,
    Boots,
    Eyes,
    Amulet,
    LeftRing,
    RightRing,
    Light,
    Leash,
    Main,
    Off,
    Alternate,
    Quiver,
}

impl DollSlot {
    /// The left column, the right column, the weapons row.
    pub const LEFT: [DollSlot; 6] = [
        DollSlot::Helmet,
        DollSlot::Cloak,
        DollSlot::Body,
        DollSlot::Shirt,
        DollSlot::Gloves,
        DollSlot::Boots,
    ];
    pub const RIGHT: [DollSlot; 6] = [
        DollSlot::Eyes,
        DollSlot::Amulet,
        DollSlot::LeftRing,
        DollSlot::RightRing,
        DollSlot::Light,
        DollSlot::Leash,
    ];
    pub const HANDS: [DollSlot; 4] = [
        DollSlot::Main,
        DollSlot::Off,
        DollSlot::Alternate,
        DollSlot::Quiver,
    ];

    pub fn name(self) -> String {
        match self {
            DollSlot::Helmet => tr!("doll-helmet"),
            DollSlot::Cloak => tr!("doll-cloak"),
            DollSlot::Body => tr!("doll-body"),
            DollSlot::Shirt => tr!("doll-shirt"),
            DollSlot::Gloves => tr!("doll-gloves"),
            DollSlot::Boots => tr!("doll-boots"),
            DollSlot::Eyes => tr!("doll-eyes"),
            DollSlot::Amulet => tr!("doll-amulet"),
            DollSlot::LeftRing => tr!("doll-left-ring"),
            DollSlot::RightRing => tr!("doll-right-ring"),
            DollSlot::Light => tr!("doll-light"),
            DollSlot::Leash => tr!("doll-leash"),
            DollSlot::Main => tr!("doll-main"),
            DollSlot::Off => tr!("doll-off"),
            DollSlot::Alternate => tr!("doll-alternate"),
            DollSlot::Quiver => tr!("doll-quiver"),
        }
    }

    /// The key of the short caption under a weapons-row socket.
    fn caption(self) -> &'static str {
        match self {
            DollSlot::Main => "doll-main-short",
            DollSlot::Off => "doll-off-short",
            DollSlot::Alternate => "doll-alternate-short",
            DollSlot::Quiver => "doll-quiver-short",
            _ => "",
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            DollSlot::Helmet => Glyph::Helmet,
            DollSlot::Cloak => Glyph::Cloak,
            DollSlot::Body => Glyph::Cuirass,
            DollSlot::Shirt => Glyph::Shirt,
            DollSlot::Gloves => Glyph::Gloves,
            DollSlot::Boots => Glyph::Boots,
            DollSlot::Eyes => Glyph::Blindfold,
            DollSlot::Amulet => Glyph::Amulet,
            DollSlot::LeftRing | DollSlot::RightRing => Glyph::Ring,
            DollSlot::Light => Glyph::Lamp,
            DollSlot::Leash => Glyph::Leash,
            DollSlot::Main => Glyph::Sword,
            DollSlot::Off => Glyph::Shield,
            DollSlot::Alternate => Glyph::Swap,
            DollSlot::Quiver => Glyph::Quiver,
        }
    }

    /// The engine's slot behind the socket (Light and Leash have none).
    fn slot(self, twoweap: bool) -> Option<Slot> {
        Some(match self {
            DollSlot::Helmet => Slot::Helmet,
            DollSlot::Cloak => Slot::Cloak,
            DollSlot::Body => Slot::Body,
            DollSlot::Shirt => Slot::Shirt,
            DollSlot::Gloves => Slot::Gloves,
            DollSlot::Boots => Slot::Boots,
            DollSlot::Eyes => Slot::Eyes,
            DollSlot::Amulet => Slot::Amulet,
            DollSlot::LeftRing => Slot::LeftRing,
            DollSlot::RightRing => Slot::RightRing,
            DollSlot::Main => Slot::Weapon,
            DollSlot::Off if twoweap => Slot::Alternate,
            DollSlot::Off => Slot::Shield,
            DollSlot::Alternate if twoweap => return None,
            DollSlot::Alternate => Slot::Alternate,
            DollSlot::Quiver => Slot::Quiver,
            DollSlot::Light | DollSlot::Leash => return None,
        })
    }

    /// What the socket shows: the items in it (lights and leashes may be
    /// several).
    pub fn items(self, pack: &Pack) -> Vec<&InvItem> {
        match self {
            DollSlot::Light => pack.lit().collect(),
            DollSlot::Leash => pack
                .items()
                .iter()
                .filter(|i| i.text.ends_with("(in use)") && i.text.contains("leash"))
                .collect(),
            other => other
                .slot(pack.twoweap())
                .and_then(|s| pack.in_slot(&s))
                .into_iter()
                .collect(),
        }
    }

    /// Whether dragging `item` here makes sense (the sockets that glow):
    /// by class, and for armor by the category its appearance shows.
    pub fn accepts(self, item: &InvItem) -> bool {
        let look = parse_item_name(&item.text).stem;
        match self {
            DollSlot::Main | DollSlot::Alternate => matches!(item.class, ')' | '('),
            DollSlot::Quiver => matches!(item.class, ')' | '*'),
            DollSlot::Off => item.class == '[' && armor_part(&look) == DollSlot::Off,
            DollSlot::Helmet
            | DollSlot::Cloak
            | DollSlot::Body
            | DollSlot::Shirt
            | DollSlot::Gloves
            | DollSlot::Boots => item.class == '[' && armor_part(&look) == self,
            DollSlot::Eyes => item.class == '(' && eyewear(&look),
            DollSlot::Amulet => item.class == '"',
            DollSlot::LeftRing | DollSlot::RightRing => item.class == '=',
            DollSlot::Light => item.class == '(' && light_source(&look),
            DollSlot::Leash => item.class == '(' && look.contains("leash"),
        }
    }

    /// The action that puts `item` here (ui-design §2.6 drag table).
    pub fn equip(self, item: &InvItem) -> Option<ItemActionKind> {
        use ItemActionKind::*;
        let wielded = item.slots.contains(&Slot::Weapon);
        Some(match self {
            DollSlot::Main => Wield,
            DollSlot::Alternate if wielded => SwapWeapons,
            DollSlot::Alternate => SetAlternate,
            DollSlot::Quiver => Quiver,
            DollSlot::Off
            | DollSlot::Helmet
            | DollSlot::Cloak
            | DollSlot::Body
            | DollSlot::Shirt
            | DollSlot::Gloves
            | DollSlot::Boots => Wear,
            DollSlot::LeftRing => PutOnLeft,
            DollSlot::RightRing => PutOnRight,
            DollSlot::Amulet | DollSlot::Eyes => PutOn,
            DollSlot::Light | DollSlot::Leash => Apply,
        })
    }

    /// The action that takes `item` out of here; the alternate weapon has
    /// none (NetHack has no single command for it).
    pub fn unequip(self, item: &InvItem) -> Option<ItemActionKind> {
        use ItemActionKind::*;
        Some(match self {
            DollSlot::Main => Unwield,
            DollSlot::Alternate => return None,
            DollSlot::Quiver => EmptyQuiver,
            DollSlot::Off if item.class != '[' => return None,
            DollSlot::Off
            | DollSlot::Helmet
            | DollSlot::Cloak
            | DollSlot::Body
            | DollSlot::Shirt
            | DollSlot::Gloves
            | DollSlot::Boots => TakeOff,
            DollSlot::Eyes | DollSlot::Amulet | DollSlot::LeftRing | DollSlot::RightRing => Remove,
            DollSlot::Light | DollSlot::Leash => Apply,
        })
    }
}

fn eyewear(look: &str) -> bool {
    matches!(look, "blindfold" | "towel" | "lenses" | "pair of lenses")
}

fn light_source(look: &str) -> bool {
    ["lamp", "lantern", "candle", "candelabrum"]
        .iter()
        .any(|w| look.contains(w))
}

/// The armor slot an appearance goes to (every appearance shows its
/// category: a "conical hat" is a hat).
fn armor_part(look: &str) -> DollSlot {
    // words, or their ends ("jackboots"): a "cape" is no "cap"
    let has = |words: &[&str]| {
        look.split([' ', '-']).any(|w| {
            words
                .iter()
                .any(|x| w == *x || (x.len() > 4 && w.ends_with(x)))
        })
    };
    if has(&[
        "helm",
        "hat",
        "cap",
        "fedora",
        "pot",
        "cornuthaum",
        "helmet",
    ]) {
        DollSlot::Helmet
    } else if has(&["shield", "roundshield"]) {
        DollSlot::Off
    } else if has(&[
        "cloak",
        "cape",
        "robe",
        "cope",
        "apron",
        "smock",
        "wrapping",
        "piece of cloth",
    ]) {
        DollSlot::Cloak
    } else if has(&["boots", "shoes"]) {
        DollSlot::Boots
    } else if has(&["gloves", "gauntlets"]) {
        DollSlot::Gloves
    } else if has(&["shirt"]) {
        DollSlot::Shirt
    } else {
        DollSlot::Body
    }
}

// ---- inputs and what they come to ----

/// Where a click or a drag starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvTarget {
    /// A grid cell with the item of this letter.
    Cell(char),
    /// The '-' pseudo-cell of selection mode.
    Hands,
    Doll(DollSlot),
    /// An action bar slot.
    Bar(usize),
    /// Outside the panel: the world (drop).
    World,
    /// Inside the panel, on nothing.
    Nothing,
}

/// What the panel's widgets queue (in `UiEvent::Inventory`).
#[derive(Debug, Clone, PartialEq)]
pub enum InvInput {
    /// `i` or the micro-button: open or close.
    Toggle,
    Close,
    Filter(InvFilter),
    Search(String),
    /// A click: `button` 1 left, 2 right; `double` the second of a
    /// double click.
    Click {
        target: InvTarget,
        button: i32,
        shift: bool,
        double: bool,
    },
    /// A drag began (the sockets that take it glow).
    DragStart(InvTarget),
    /// A drag ended over `to`.
    Drop {
        from: InvTarget,
        to: InvTarget,
        shift: bool,
    },
    /// A row of the context menu or a detail button.
    Action {
        letter: char,
        kind: ItemActionKind,
    },
    /// The context menu closes without a choice.
    CloseMenu,
    /// The count picker's OK and its value.
    Count(u32),
    CancelCount,
    /// Multi-select mode: the header's Confirm and Cancel.
    Confirm,
    Cancel,
    /// A gamepad picks up the focused item, or puts the one it holds down
    /// where the focus is (a drag and drop).
    Carry,
}

/// What the game does for the panel.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// Run this macro at the command prompt.
    Macro(Macro),
    /// Answer the pending request.
    Reply(Reply),
    /// Answer the getobj question: the count's digits (if any), then the
    /// letter.
    Pick { letter: char, count: Option<u32> },
    /// Put this in an action bar slot.
    Bind { slot: usize, binding: SlotBinding },
}

/// What a key does while the panel is open.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyUse {
    /// The panel took it.
    Used,
    /// The panel took it, and this follows.
    Then(Intent),
    /// Not the panel's: it goes on as if the panel were closed.
    Pass,
}

/// The mode the panel is in.
#[derive(Debug, Clone, PartialEq)]
enum Mode {
    Closed,
    Browse,
    Select {
        req: u64,
        question: ItemQuestion,
        query: String,
        /// Digits typed before the letter.
        count: Option<u32>,
        /// The panel was open before the command asked.
        was_open: bool,
    },
    Menu {
        req: u64,
        state: MenuState,
        was_open: bool,
    },
}

/// A second choice some actions need first.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Choose {
    /// `#adjust`: to which letter (with a count: split).
    Adjust { from: char, count: Option<u32> },
    /// `#dip`: into which item.
    Dip { from: char },
}

/// What the count picker is for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CountFor {
    /// Selection mode: then pick the letter.
    Pick(char),
    /// Drop this many.
    Drop(char),
    /// Split this many off (then choose the letter).
    Split(char),
    /// Menu mode: select that many of the entry.
    Entry(char),
}

// ---- the view ----

/// A socket of the grid or the doll.
struct Socket {
    root: Gd<PanelContainer>,
    icon: Gd<TextureRect>,
    /// The silhouette of an empty doll socket.
    ghost: Option<Gd<TextureRect>>,
    pill: Gd<PanelContainer>,
    letter: Gd<Label>,
    count: Gd<Label>,
    badge: Gd<TextureRect>,
    stripe: Gd<ColorRect>,
    class_mark: Gd<TextureRect>,
    check: Gd<Label>,
    /// Gold rim of the suggested cells (pulses).
    rim: Gd<PanelContainer>,
}

/// Mouse state shared with the sockets' signal closures.
#[derive(Default)]
struct Mouse {
    /// The press that may become a drag: where, from what, with Shift.
    press: Option<(Vector2, InvTarget, bool)>,
    dragging: bool,
    /// The last click, for double clicks.
    last: Option<(InvTarget, f64)>,
    /// Cell index → target, as the grid shows it now.
    cells: Vec<InvTarget>,
    /// The action bar's slots on screen (the HUD's), set every frame.
    bar: Vec<Rect2>,
    /// The panel frame and the grid sockets, the doll sockets on screen.
    frame: Option<Gd<Control>>,
    cell_nodes: Vec<Gd<Control>>,
    doll_nodes: Vec<(DollSlot, Gd<Control>)>,
    preview: Option<Gd<TextureRect>>,
    preview_tex: Option<Gd<Texture2D>>,
}

impl Mouse {
    /// What is under `pos` (canvas coordinates).
    fn target_at(&self, pos: Vector2) -> InvTarget {
        if let Some(i) = self.bar.iter().position(|r| r.contains_point(pos)) {
            return InvTarget::Bar(i);
        }
        for (i, n) in self.cell_nodes.iter().enumerate() {
            if n.is_visible_in_tree() && n.get_global_rect().contains_point(pos) {
                return self.cells.get(i).copied().unwrap_or(InvTarget::Nothing);
            }
        }
        for (slot, n) in &self.doll_nodes {
            if n.get_global_rect().contains_point(pos) {
                return InvTarget::Doll(*slot);
            }
        }
        match &self.frame {
            Some(f) if f.get_global_rect().contains_point(pos) => InvTarget::Nothing,
            _ => InvTarget::World,
        }
    }
}

fn now_secs() -> f64 {
    Time::singleton().get_ticks_msec() as f64 / 1000.0
}

/// Override a stylebox unless it is the one there already: an override is
/// a theme change, the node and its children work out their looks again.
fn set_style<T: Inherits<Control>>(node: &mut Gd<T>, name: &str, sb: &Gd<StyleBoxFlat>) {
    let mut c = node.clone().upcast::<Control>();
    if c.has_theme_stylebox_override(name)
        && c.get_theme_stylebox(name)
            .is_some_and(|s| s.instance_id() == sb.instance_id())
    {
        return;
    }
    c.add_theme_stylebox_override(name, sb);
}

fn flat(bg: Color, border: Color, width: i32, radius: i32) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(width);
    sb.set_border_color(border);
    sb.set_corner_radius_all(radius);
    sb.set_corner_detail(1);
    sb.set_content_margin_all(0.0);
    sb
}

/// The socket looks (ui-design §2.3 cell states).
struct Looks {
    empty: Gd<StyleBoxFlat>,
    normal: Gd<StyleBoxFlat>,
    hover: Gd<StyleBoxFlat>,
    selected: Gd<StyleBoxFlat>,
    equipped: Gd<StyleBoxFlat>,
    picked: Gd<StyleBoxFlat>,
    glow: Gd<StyleBoxFlat>,
    tab: Gd<StyleBoxFlat>,
    tab_on: Gd<StyleBoxFlat>,
}

impl Looks {
    fn new() -> Looks {
        let socket_bg = theme::SOCKET;
        let mut selected = flat(Color::from_rgb(0.07, 0.055, 0.04), theme::GOLD_BRIGHT, 2, 3);
        selected.set_shadow_size(8);
        selected.set_shadow_color(Color::from_rgba(0.906, 0.761, 0.478, 0.33));
        let mut hover = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), theme::GOLD, 1, 3);
        hover.set_draw_center(false);
        hover.set_shadow_size(4);
        hover.set_shadow_color(Color::from_rgba(0.72, 0.54, 0.23, 0.25));
        let mut glow = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), theme::GOLD, 2, 3);
        glow.set_shadow_size(6);
        glow.set_shadow_color(Color::from_rgba(0.94, 0.75, 0.38, 0.4));
        glow.set_draw_center(false);
        Looks {
            empty: flat(socket_bg, Color::from_rgb(0.165, 0.133, 0.106), 1, 3),
            normal: flat(Color::from_rgb(0.06, 0.047, 0.036), theme::IRON, 1, 3),
            hover,
            selected,
            equipped: flat(Color::from_rgb(0.085, 0.064, 0.04), theme::GOLD_DIM, 1, 3),
            picked: flat(Color::from_rgb(0.06, 0.07, 0.045), theme::GOOD, 2, 3),
            glow,
            tab: flat(Color::from_rgb(0.07, 0.055, 0.04), theme::IRON, 1, 3),
            tab_on: flat(Color::from_rgb(0.14, 0.1, 0.06), theme::GOLD_BRIGHT, 1, 3),
        }
    }
}

/// A texture rect that fills its parent, less `inset` on each side.
fn fill_icon(inset: f32) -> Gd<TextureRect> {
    let mut t = TextureRect::new_alloc();
    t.set_mouse_filter(MouseFilter::IGNORE);
    t.set_expand_mode(ExpandMode::IGNORE_SIZE);
    t.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    place(&t, [0.0, 0.0, 1.0, 1.0], [inset, inset, -inset, -inset]);
    t
}

impl Socket {
    fn new(side: f32, looks: &Looks) -> Socket {
        let mut root = PanelContainer::new_alloc();
        root.set_custom_minimum_size(Vector2::new(side, side));
        root.set_mouse_filter(MouseFilter::STOP);
        root.add_theme_stylebox_override("panel", &looks.empty);
        root.set_clip_contents(false);
        // everything inside is placed by anchors in a plain Control
        let mut inner = Control::new_alloc();
        inner.set_mouse_filter(MouseFilter::IGNORE);
        root.add_child(&inner);
        // the inner shadow of the socket
        let mut well = ColorRect::new_alloc();
        well.set_mouse_filter(MouseFilter::IGNORE);
        place(&well, [0.0, 0.0, 1.0, 1.0], [1.0, 1.0, -1.0, -1.0]);
        match theme::ui_material("ui_frame") {
            Some(mut m) => {
                for (n, v) in [
                    ("size", Vector2::new(side - 2.0, side - 2.0).to_variant()),
                    (
                        "top_color",
                        Color::from_rgba(0.02, 0.016, 0.012, 0.9).to_variant(),
                    ),
                    (
                        "bottom_color",
                        Color::from_rgba(0.07, 0.055, 0.042, 0.5).to_variant(),
                    ),
                    (
                        "outer_color",
                        Color::from_rgba(0.0, 0.0, 0.0, 0.0).to_variant(),
                    ),
                    (
                        "border_color",
                        Color::from_rgba(0.0, 0.0, 0.0, 0.0).to_variant(),
                    ),
                    ("border_px", 0.0f32.to_variant()),
                    ("inner_shadow_px", 10.0f32.to_variant()),
                    ("corner_cut_px", 2.0f32.to_variant()),
                    ("trim_px", 0.0f32.to_variant()),
                    ("shadow_px", 0.0f32.to_variant()),
                    ("shadow_alpha", 0.0f32.to_variant()),
                ] {
                    m.set_shader_parameter(n, &v);
                }
                well.set_material(&m);
            }
            None => well.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.25)),
        }
        inner.add_child(&well);
        let icon = fill_icon(6.0);
        inner.add_child(&icon);
        let mut class_mark = TextureRect::new_alloc();
        class_mark.set_mouse_filter(MouseFilter::IGNORE);
        class_mark.set_expand_mode(ExpandMode::IGNORE_SIZE);
        class_mark.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        place(&class_mark, [0.0, 1.0, 0.0, 1.0], [3.0, -17.0, 17.0, -3.0]);
        class_mark.set_modulate(Color::from_rgba(0.72, 0.54, 0.23, 0.5));
        class_mark.set_visible(false);
        inner.add_child(&class_mark);
        let mut stripe = ColorRect::new_alloc();
        stripe.set_mouse_filter(MouseFilter::IGNORE);
        place(&stripe, [0.0, 0.0, 0.0, 1.0], [1.0, 4.0, 4.0, -4.0]);
        stripe.set_visible(false);
        inner.add_child(&stripe);
        let mut pill = PanelContainer::new_alloc();
        pill.set_mouse_filter(MouseFilter::IGNORE);
        let mut pill_sb = flat(
            Color::from_rgba(0.043, 0.035, 0.031, 0.85),
            theme::GOLD_DIM,
            1,
            2,
        );
        pill_sb.set_content_margin(Side::LEFT, 4.0);
        pill_sb.set_content_margin(Side::RIGHT, 4.0);
        pill.add_theme_stylebox_override("panel", &pill_sb);
        place(&pill, [0.0, 0.0, 0.0, 0.0], [3.0, 3.0, 21.0, 19.0]);
        // not small caps: `a` and `A` are different items
        let mut letter = theme::styled_label("", Face::BodyBold, 14, theme::TEXT);
        letter.set_horizontal_alignment(HorizontalAlignment::CENTER);
        letter.set_vertical_alignment(VerticalAlignment::CENTER);
        pill.add_child(&letter);
        pill.set_visible(false);
        inner.add_child(&pill);
        let mut badge = TextureRect::new_alloc();
        badge.set_mouse_filter(MouseFilter::IGNORE);
        badge.set_expand_mode(ExpandMode::IGNORE_SIZE);
        badge.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        place(&badge, [1.0, 0.0, 1.0, 0.0], [-19.0, 3.0, -3.0, 19.0]);
        badge.set_modulate(theme::GOLD_BRIGHT);
        badge.set_visible(false);
        inner.add_child(&badge);
        let mut count = theme::styled_label("", Face::BodyBold, 15, theme::TEXT);
        theme::outline(&count, 5);
        place(&count, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, -4.0, -1.0]);
        count.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        count.set_vertical_alignment(VerticalAlignment::BOTTOM);
        inner.add_child(&count);
        let mut check = theme::styled_label("", Face::BodyBold, 18, theme::GOOD);
        theme::outline(&check, 5);
        place(&check, [1.0, 0.0, 1.0, 0.0], [-22.0, 0.0, -2.0, 22.0]);
        check.set_horizontal_alignment(HorizontalAlignment::CENTER);
        check.set_visible(false);
        inner.add_child(&check);
        let mut rim = PanelContainer::new_alloc();
        rim.set_mouse_filter(MouseFilter::IGNORE);
        rim.add_theme_stylebox_override("panel", &looks.glow);
        place(&rim, [0.0, 0.0, 1.0, 1.0], [-1.0, -1.0, 1.0, 1.0]);
        rim.set_visible(false);
        inner.add_child(&rim);
        // the hover rim follows the mouse by itself
        let mut hover = PanelContainer::new_alloc();
        hover.set_mouse_filter(MouseFilter::IGNORE);
        hover.add_theme_stylebox_override("panel", &looks.hover);
        place(&hover, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        hover.set_visible(false);
        inner.add_child(&hover);
        let mut h = hover.clone();
        root.signals()
            .mouse_entered()
            .connect(move || h.set_visible(true));
        let mut h = hover.clone();
        root.signals()
            .mouse_exited()
            .connect(move || h.set_visible(false));
        Socket {
            root,
            icon,
            ghost: None,
            pill,
            letter,
            count,
            badge,
            stripe,
            class_mark,
            check,
            rim,
        }
    }

    /// Show `item` (or nothing).
    fn show(&mut self, item: Option<&InvItem>) {
        let Some(item) = item else {
            self.icon.set_texture(None::<&Gd<Texture2D>>);
            self.pill.set_visible(false);
            self.count.set_text("");
            self.badge.set_visible(false);
            self.stripe.set_visible(false);
            self.class_mark.set_visible(false);
            self.check.set_visible(false);
            if let Some(g) = self.ghost.as_mut() {
                g.set_visible(true);
            }
            return;
        };
        self.icon
            .set_texture(&icons::item_icon(item.tile, item.class));
        self.letter.set_text(&item.letter.to_string());
        self.pill.set_visible(true);
        self.count.set_text(&if item.quan > 1 {
            item.quan.to_string()
        } else {
            String::new()
        });
        match state_glyph(item) {
            Some(g) => {
                self.badge.set_texture(&icons::glyph_icon(g));
                self.badge.set_visible(true);
            }
            None => self.badge.set_visible(false),
        }
        let buc = parse_item_name(&item.text).buc;
        match buc {
            Some(b) => {
                self.stripe.set_color(match b {
                    Buc::Blessed => BLESSED,
                    Buc::Uncursed => UNCURSED,
                    Buc::Cursed => CURSED,
                });
                self.stripe.set_visible(true);
            }
            None => self.stripe.set_visible(false),
        }
        if let Some(g) = self.ghost.as_mut() {
            g.set_visible(false);
        }
    }
}

/// The render layer only the hero's model and what it holds are on (the
/// map's rim light layer): the doll's camera sees nothing else.
const HERO_LAYER: u32 = 1 << 1;
/// The doll's render, twice the area it is shown in for sharpness.
const DOLL_PX: Vector2i = Vector2i::new(480, 880);

/// A camera in the map's own world that sees only the hero, lit by a key
/// and a fill that light only the hero, on a clear background: the doll
/// shows the hero in their gear as the map does.
struct DollView {
    viewport: Gd<SubViewport>,
    camera: Gd<Camera3D>,
    hero: Option<Gd<Node3D>>,
    /// When it last rendered: it renders at `DOLL_FPS`, the game's frames
    /// in between have one 3D view fewer to draw.
    last: f64,
}

/// The doll turns slowly and breathes: 30 renders a second are plenty.
const DOLL_FPS: f64 = 30.0;

impl DollView {
    fn new(parent: &mut Gd<Control>) -> DollView {
        let mut viewport = SubViewport::new_alloc();
        viewport.set_size(DOLL_PX);
        viewport.set_transparent_background(true);
        viewport.set_msaa_3d(godot::classes::viewport::Msaa::MSAA_4X);
        viewport.set_update_mode(UpdateMode::DISABLED);
        let mut camera = Camera3D::new_alloc();
        camera.set_cull_mask(HERO_LAYER);
        camera.set_fov(24.0);
        let mut env = Environment::new_gd();
        env.set_background(godot::classes::environment::BgMode::CLEAR_COLOR);
        env.set_ambient_source(godot::classes::environment::AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.55, 0.5, 0.46));
        env.set_ambient_light_energy(1.9);
        env.set_tonemapper(godot::classes::environment::ToneMapper::AGX);
        env.set_tonemap_exposure(1.25);
        camera.set_environment(&env);
        // a warm key from the upper left, a cold rim from behind
        for (rot, color, energy) in [
            // the key, not too steep: the legs get it too
            (
                Vector3::new(-0.3, -0.55, 0.0),
                Color::from_rgb(1.0, 0.9, 0.78),
                3.4,
            ),
            (
                Vector3::new(-0.2, 0.7, 0.0),
                Color::from_rgb(0.9, 0.8, 0.7),
                1.1,
            ),
            // a warm bounce from below, as off a lit floor
            (
                Vector3::new(0.45, 0.3, 0.0),
                Color::from_rgb(1.0, 0.82, 0.62),
                2.6,
            ),
            // and a level fill for the legs and the boots
            (
                Vector3::new(0.05, -0.35, 0.0),
                Color::from_rgb(0.95, 0.88, 0.8),
                1.2,
            ),
            (
                Vector3::new(-0.3, 2.6, 0.0),
                Color::from_rgb(0.62, 0.7, 1.0),
                1.6,
            ),
        ] {
            let mut light = DirectionalLight3D::new_alloc();
            light.set_rotation(rot);
            light.set_color(color);
            light.set_param(godot::classes::light_3d::Param::ENERGY, energy);
            light.set_cull_mask(HERO_LAYER);
            light.set_shadow(false);
            camera.add_child(&light);
        }
        viewport.add_child(&camera);
        parent.add_child(&viewport);
        DollView {
            viewport,
            camera,
            hero: None,
            last: f64::NEG_INFINITY,
        }
    }

    fn texture(&self) -> Gd<Texture2D> {
        self.viewport
            .get_texture()
            .expect("a viewport has a texture")
            .upcast()
    }

    /// Aim at the hero (slowly circling them); false when there is none.
    fn frame(&mut self, now: f64) -> bool {
        let at = self
            .hero
            .as_ref()
            .filter(|h| h.is_instance_valid() && h.is_inside_tree())
            .map(|h| {
                // the model's own transform (its yaw) is on its child
                let facing = h
                    .get_child(0)
                    .and_then(|c| c.try_cast::<Node3D>().ok())
                    .map_or(h.get_global_transform(), |c| c.get_global_transform());
                Transform3D::new(facing.basis, h.get_global_position())
            });
        let Some(t) = at else {
            return false;
        };
        // the last render stays shown; a new one once in a while (once
        // rendered, the viewport goes back to DISABLED on its own)
        if now - self.last < 1.0 / DOLL_FPS {
            return true;
        }
        self.last = now;
        self.viewport.set_update_mode(UpdateMode::ONCE);
        // in front of the model, a little above, turning slowly
        let face = t.basis.col_c().normalized();
        let yaw = (now * 0.25).sin() as f32 * 0.6;
        let dir = Basis::from_axis_angle(Vector3::UP, yaw) * face;
        let target = t.origin + Vector3::new(0.0, 0.82, 0.0);
        let eye = target + dir * 4.1 + Vector3::new(0.0, 0.4, 0.0);
        self.camera
            .set_global_transform(Transform3D::new(Basis::IDENTITY, eye).looking_at(target));
        true
    }
}

/// The inventory panel.
pub struct InventoryPanel {
    queue: UiQueue,
    root: Gd<Control>,
    shade: Gd<ColorRect>,
    frame: Gd<Control>,
    looks: Looks,
    mode: Mode,
    filter: InvFilter,
    search: String,
    /// The item the detail panel shows.
    selected: Option<char>,
    /// The keyboard's cell.
    focus: Option<usize>,
    choose: Option<Choose>,
    counting: Option<(CountFor, u32, u32)>,
    menu_for: Option<char>,
    drag_from: Option<InvTarget>,
    pack: Pack,
    /// What the grid shows, in cell order.
    cells_shown: Vec<InvTarget>,
    /// Something the view shows changed.
    dirty: bool,
    mouse: Rc<RefCell<Mouse>>,

    title: Gd<Label>,
    subtitle: Gd<Label>,
    letters: Gd<Label>,
    ac_chip: Gd<Label>,
    gold_chip: Gd<Label>,
    confirm: Gd<Button>,
    cancel: Gd<Button>,
    close: Gd<Button>,
    tabs: Vec<(InvFilter, Gd<Button>)>,
    suggested_tab: Gd<Button>,
    search_edit: Gd<LineEdit>,
    cells: Vec<Socket>,
    doll: Vec<(DollSlot, Socket)>,
    doll_summary: Gd<Label>,
    hint: Gd<Label>,
    /// "Your pack": the classes carried and how many, under the grid.
    pack_box: Gd<VBoxContainer>,
    pack_key: Vec<(InvFilter, usize)>,
    detail: Gd<VBoxContainer>,
    /// What the detail column shows (the item, and its actions or not):
    /// it is rebuilt only when that changes.
    detail_key: Option<(Option<InvItem>, bool)>,
    detail_empty: Gd<Label>,
    ctx_menu: Gd<PanelContainer>,
    ctx_rows: Gd<VBoxContainer>,
    count_box: Gd<PanelContainer>,
    count_label: Gd<Label>,
    /// AC, gold and burden, from the HUD's status.
    status: (Option<String>, Option<String>, Option<String>),
    /// A gamepad plays: the focus is always somewhere.
    pad: bool,
    pad_kind: Option<PadKind>,
    /// The keyboard (a gamepad) is on a doll socket: its index in `doll`.
    doll_focus: Option<usize>,
    /// A gamepad's "drag": the item picked up, put down with Y again.
    carrying: Option<InvTarget>,
    /// The context menu's rows and the one the d-pad is on.
    ctx_buttons: Vec<(ItemActionKind, Gd<Button>)>,
    ctx_focus: usize,
    /// Grid rows shown: the pack's and an empty one, at least 3; chosen
    /// when the panel opens and only growing while it is open, so cells
    /// never move under the pointer (0: closed).
    rows: usize,
    grid_top: f32,
    /// The hero's render on the doll, and the silhouette it replaces.
    doll_view: DollView,
    hero_rect: Gd<TextureRect>,
    figure: Gd<TextureRect>,
    /// The warm-up behind the title: frames it has been open (and the
    /// doll shown), or done.
    warm: Warm,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Warm {
    Not,
    Open { frames: u32, doll: u32 },
    Done,
}

/// What a frame of the panel's warm-up did: made the panel (hidden), drew
/// it for the first time, rendered the doll for the first time, showed
/// them again, or finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmStep {
    Panel,
    Drawn,
    Doll,
    Shown,
    Done,
}

/// The warm-up's frame the doll's render begins on.
const DOLL_AFTER: u32 = 3;

fn tab_button(glyph: Glyph, tip: &str, looks: &Looks) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_focus_mode(FocusMode::NONE);
    b.set_custom_minimum_size(Vector2::new(42.0, 36.0));
    b.set_tooltip_text(tip);
    for s in [
        "normal",
        "hover",
        "pressed",
        "hover_pressed",
        "disabled",
        "focus",
    ] {
        b.add_theme_stylebox_override(s, &looks.tab);
    }
    let mut hover = looks.tab.duplicate_resource();
    hover.set_border_color(theme::GOLD);
    b.add_theme_stylebox_override("hover", &hover);
    let mut t = fill_icon(3.0);
    t.set_texture(&icons::glyph_icon(glyph));
    t.set_modulate(theme::GOLD_BRIGHT);
    b.add_child(&t);
    b
}

fn filter_glyph(f: InvFilter) -> Glyph {
    match f {
        InvFilter::All => Glyph::Grid,
        InvFilter::Suggested => Glyph::Star,
        InvFilter::Weapons => Glyph::Sword,
        InvFilter::Armor => Glyph::Cuirass,
        InvFilter::Accessories => Glyph::Ring,
        InvFilter::Tools => Glyph::Sack,
        InvFilter::Food => Glyph::Drumstick,
        InvFilter::Potions => Glyph::Flask,
        InvFilter::ScrollsAndBooks => Glyph::Scroll,
        InvFilter::Wands => Glyph::Wand,
        InvFilter::GemsAndOther => Glyph::Gem,
        InvFilter::Equipped => Glyph::Check,
    }
}

fn separator() -> Gd<ColorRect> {
    let mut rule = ColorRect::new_alloc();
    rule.set_color(Color::from_rgba(0.43, 0.33, 0.16, 0.55));
    rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    rule.set_mouse_filter(MouseFilter::IGNORE);
    rule
}

fn chip(text: &str) -> Gd<Label> {
    let mut l = theme::styled_label(text, Face::BodyBold, 15, theme::TEXT);
    let mut sb = flat(Color::from_rgba(0.06, 0.045, 0.035, 0.9), theme::IRON, 1, 3);
    sb.set_content_margin(Side::LEFT, 8.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    sb.set_content_margin(Side::TOP, 2.0);
    sb.set_content_margin(Side::BOTTOM, 2.0);
    l.add_theme_stylebox_override("normal", &sb);
    l.set_vertical_alignment(VerticalAlignment::CENTER);
    l
}

/// Connect a socket's mouse: clicks, double clicks, drags.
fn wire_socket(
    node: &Gd<PanelContainer>,
    target: Rc<dyn Fn() -> InvTarget>,
    queue: &UiQueue,
    mouse: &Rc<RefCell<Mouse>>,
) {
    let q = queue.clone();
    let m = mouse.clone();
    node.signals()
        .gui_input()
        .connect(move |ev: Gd<InputEvent>| {
            let ev = match ev.try_cast::<InputEventMouseButton>() {
                Ok(b) => {
                    let pos = b.get_global_position();
                    let shift = b.is_shift_pressed();
                    let t = target();
                    match (b.get_button_index(), b.is_pressed()) {
                        (MouseButton::LEFT, true) => {
                            m.borrow_mut().press = Some((pos, t, shift));
                            m.borrow_mut().dragging = false;
                        }
                        (MouseButton::LEFT, false) => {
                            let mut mm = m.borrow_mut();
                            let press = mm.press.take();
                            let dragged = std::mem::take(&mut mm.dragging);
                            if let Some(p) = mm.preview.as_mut() {
                                p.set_visible(false);
                            }
                            let Some((_, from, shift)) = press else {
                                return;
                            };
                            if dragged {
                                let to = mm.target_at(pos);
                                drop(mm);
                                push(&q, UiEvent::Inventory(InvInput::Drop { from, to, shift }));
                            } else {
                                let now = now_secs();
                                let double = mm
                                    .last
                                    .is_some_and(|(l, at)| l == from && now - at < DOUBLE_SECS);
                                mm.last = if double { None } else { Some((from, now)) };
                                drop(mm);
                                push(
                                    &q,
                                    UiEvent::Inventory(InvInput::Click {
                                        target: from,
                                        button: 1,
                                        shift,
                                        double,
                                    }),
                                );
                            }
                        }
                        (MouseButton::RIGHT, true) => push(
                            &q,
                            UiEvent::Inventory(InvInput::Click {
                                target: t,
                                button: 2,
                                shift,
                                double: false,
                            }),
                        ),
                        _ => {}
                    }
                    return;
                }
                Err(ev) => ev,
            };
            let Ok(motion) = ev.try_cast::<InputEventMouseMotion>() else {
                return;
            };
            let pos = motion.get_global_position();
            let mut mm = m.borrow_mut();
            let Some((start, from, _)) = mm.press else {
                return;
            };
            if !mm.dragging && (pos - start).length() > DRAG_START {
                let draggable = matches!(from, InvTarget::Cell(_) | InvTarget::Doll(_));
                if !draggable {
                    return;
                }
                mm.dragging = true;
                let tex = mm.preview_tex.clone();
                if let Some(p) = mm.preview.as_mut() {
                    p.set_texture(tex.as_ref());
                    p.set_visible(tex.is_some());
                }
                drop(mm);
                push(&q, UiEvent::Inventory(InvInput::DragStart(from)));
                mm = m.borrow_mut();
            }
            if mm.dragging
                && let Some(p) = mm.preview.as_mut()
            {
                let size = p.get_size();
                p.set_global_position(pos - size * 0.5);
            }
        });
}

impl InventoryPanel {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> InventoryPanel {
        let looks = Looks::new();
        let mouse = Rc::new(RefCell::new(Mouse::default()));
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);

        // the world dimmed 25 % behind the panel; it stays a drop target
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.25));
        theme::full_rect_ignore(&shade);
        root.add_child(&shade);

        let mut frame = theme::framed(Frame::Panel);
        frame.set_mouse_filter(MouseFilter::STOP);
        frame.set_size(Vector2::new(WIDTH, HEIGHT));
        if let Some(mut sb) = frame
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin_all(0.0);
        }
        let mut body = Control::new_alloc();
        body.set_mouse_filter(MouseFilter::IGNORE);
        body.set_custom_minimum_size(Vector2::new(WIDTH, HEIGHT));
        frame.add_child(&body);
        root.add_child(&frame);
        // a click on the panel's background closes the context menu
        let q = queue.clone();
        frame
            .signals()
            .gui_input()
            .connect(move |ev: Gd<InputEvent>| {
                if let Ok(b) = ev.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                {
                    push(&q, UiEvent::Inventory(InvInput::CloseMenu));
                }
            });
        let frame: Gd<Control> = frame.upcast();

        // ---- header ----
        let mut title = theme::styled_label(&tr!("inv-title"), Face::Title, 30, theme::GOLD_BRIGHT);
        place(
            &title,
            [0.0, 0.0, 0.0, 0.0],
            [PAD + 4.0, 8.0, PAD + 380.0, HEADER],
        );
        title.set_vertical_alignment(VerticalAlignment::CENTER);
        body.add_child(&title);
        let mut subtitle = theme::styled_label("", Face::BodyBold, 18, theme::TEXT);
        place(
            &subtitle,
            [0.0, 0.0, 0.0, 0.0],
            [PAD + 200.0, 8.0, 980.0, HEADER],
        );
        subtitle.set_vertical_alignment(VerticalAlignment::CENTER);
        subtitle.set_clip_text(true);
        body.add_child(&subtitle);
        let mut head_right = HBoxContainer::new_alloc();
        head_right.set_mouse_filter(MouseFilter::IGNORE);
        head_right.add_theme_constant_override("separation", 8);
        head_right.set_alignment(godot::classes::box_container::AlignmentMode::END);
        place(
            &head_right,
            [1.0, 0.0, 1.0, 0.0],
            [-760.0, 12.0, -PAD, HEADER - 4.0],
        );
        let mut letters = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        letters.set_vertical_alignment(VerticalAlignment::CENTER);
        i18n::tip(&letters, "inv-letters-tip");
        letters.set_mouse_filter(MouseFilter::PASS);
        head_right.add_child(&letters);
        let ac_chip = chip("");
        head_right.add_child(&ac_chip);
        let gold_chip = chip("");
        head_right.add_child(&gold_chip);
        let mut confirm = theme::button("", &queue, UiEvent::Inventory(InvInput::Confirm));
        i18n::text(&confirm, "inv-confirm");
        confirm.add_theme_stylebox_override("normal", &theme::default_button_style());
        confirm.set_visible(false);
        head_right.add_child(&confirm);
        let mut cancel = theme::button("", &queue, UiEvent::Inventory(InvInput::Cancel));
        i18n::text(&cancel, "inv-cancel");
        cancel.set_visible(false);
        head_right.add_child(&cancel);
        let mut close = theme::button("✕", &queue, UiEvent::Inventory(InvInput::Close));
        i18n::tip(&close, "inv-close-tip");
        close.set_custom_minimum_size(Vector2::new(36.0, 32.0));
        head_right.add_child(&close);
        body.add_child(&head_right);
        let mut rule = separator();
        place(
            &rule,
            [0.0, 0.0, 1.0, 0.0],
            [PAD, HEADER + 2.0, -PAD, HEADER + 3.0],
        );
        rule.set_color(Color::from_rgba(0.72, 0.54, 0.23, 0.5));
        body.add_child(&rule);

        let top = HEADER + 16.0;
        let body_h = HEIGHT - top - 16.0;

        // ---- the paper doll, left ----
        let doll_x = PAD;
        let mut doll_bg = theme::framed(Frame::Socket);
        doll_bg.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &doll_bg,
            [0.0, 0.0, 0.0, 0.0],
            [doll_x, top, doll_x + DOLL_W, top + body_h],
        );
        body.add_child(&doll_bg);
        let mut doll_area = Control::new_alloc();
        doll_area.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &doll_area,
            [0.0, 0.0, 0.0, 0.0],
            [
                doll_x + 12.0,
                top + 14.0,
                doll_x + DOLL_W - 12.0,
                top + body_h,
            ],
        );
        body.add_child(&doll_area);
        let inner_w = DOLL_W - 24.0;
        let mut figure = TextureRect::new_alloc();
        figure.set_mouse_filter(MouseFilter::IGNORE);
        figure.set_expand_mode(ExpandMode::IGNORE_SIZE);
        figure.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        figure.set_texture(&icons::figure());
        figure.set_modulate(Color::from_rgba(0.72, 0.54, 0.23, 0.5));
        place(
            &figure,
            [0.0, 0.0, 0.0, 0.0],
            [76.0, -4.0, inner_w - 76.0, 448.0],
        );
        doll_area.add_child(&figure);
        // the hero, rendered in their gear, over the engraved figure
        let doll_view = DollView::new(&mut root);
        let mut hero_rect = TextureRect::new_alloc();
        hero_rect.set_mouse_filter(MouseFilter::IGNORE);
        hero_rect.set_expand_mode(ExpandMode::IGNORE_SIZE);
        hero_rect.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        hero_rect.set_texture(&doll_view.texture());
        place(
            &hero_rect,
            [0.0, 0.0, 0.0, 0.0],
            [72.0, -8.0, inner_w - 72.0, 452.0],
        );
        hero_rect.set_visible(false);
        doll_area.add_child(&hero_rect);
        let mut doll = Vec::new();
        let ghost_color = Color::from_rgba(0.72, 0.54, 0.23, 0.28);
        let mut slot_area = doll_area.clone();
        let mut add_slot = |slot: DollSlot, x: f32, y: f32, doll: &mut Vec<(DollSlot, Socket)>| {
            let mut s = Socket::new(CELL, &looks);
            place(&s.root, [0.0, 0.0, 0.0, 0.0], [x, y, x + CELL, y + CELL]);
            let mut g = fill_icon(12.0);
            g.set_texture(&icons::glyph_icon(slot.glyph()));
            g.set_modulate(ghost_color);
            if let Some(mut inner) = s
                .root
                .get_child(0)
                .and_then(|c| c.try_cast::<Control>().ok())
            {
                inner.add_child(&g);
                inner.move_child(&g, 1);
            }
            s.ghost = Some(g);
            s.root.set_tooltip_text(&slot.name());
            slot_area.add_child(&s.root);
            let t = slot;
            wire_socket(&s.root, Rc::new(move || InvTarget::Doll(t)), &queue, &mouse);
            mouse
                .borrow_mut()
                .doll_nodes
                .push((slot, s.root.clone().upcast()));
            doll.push((slot, s));
        };
        for (i, slot) in DollSlot::LEFT.iter().enumerate() {
            add_slot(*slot, 0.0, i as f32 * 76.0, &mut doll);
        }
        for (i, slot) in DollSlot::RIGHT.iter().enumerate() {
            add_slot(*slot, inner_w - CELL, i as f32 * 76.0, &mut doll);
        }
        let row_w = 4.0 * CELL + 3.0 * 24.0;
        let row_x = (inner_w - row_w) / 2.0;
        let row_y = 474.0;
        for (i, slot) in DollSlot::HANDS.iter().enumerate() {
            let x = row_x + i as f32 * (CELL + 24.0);
            add_slot(*slot, x, row_y, &mut doll);
            let mut cap = theme::styled_label("", Face::Caps, 13, theme::TEXT_DIM);
            i18n::text(&cap, slot.caption());
            cap.set_horizontal_alignment(HorizontalAlignment::CENTER);
            place(
                &cap,
                [0.0, 0.0, 0.0, 0.0],
                [
                    x - 12.0,
                    row_y + CELL + 2.0,
                    x + CELL + 12.0,
                    row_y + CELL + 20.0,
                ],
            );
            doll_area.add_child(&cap);
        }
        let mut doll_summary = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        doll_summary.set_autowrap_mode(AutowrapMode::WORD_SMART);
        doll_summary.set_horizontal_alignment(HorizontalAlignment::CENTER);
        place(
            &doll_summary,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, row_y + CELL + 30.0, inner_w, body_h - 18.0],
        );
        doll_area.add_child(&doll_summary);

        // ---- the grid, centre ----
        let gx = doll_x + DOLL_W + PAD;
        let mut tab_row = HBoxContainer::new_alloc();
        tab_row.set_mouse_filter(MouseFilter::IGNORE);
        tab_row.add_theme_constant_override("separation", 4);
        place(
            &tab_row,
            [0.0, 0.0, 0.0, 0.0],
            [gx, top, gx + GRID_COL_W, top + 32.0],
        );
        let mut suggested_tab = tab_button(Glyph::Star, "", &looks);
        i18n::tip(&suggested_tab, "inv-suggested-tip");
        let q = queue.clone();
        suggested_tab.signals().pressed().connect(move || {
            push(
                &q,
                UiEvent::Inventory(InvInput::Filter(InvFilter::Suggested)),
            )
        });
        suggested_tab.set_visible(false);
        tab_row.add_child(&suggested_tab);
        let mut tabs = Vec::new();
        for f in InvFilter::TABS {
            let b = tab_button(filter_glyph(f), &filter_tip(f), &looks);
            let q = queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::Inventory(InvInput::Filter(f))));
            tab_row.add_child(&b);
            tabs.push((f, b));
        }
        body.add_child(&tab_row);
        let mut search_edit = LineEdit::new_alloc();
        i18n::bind(&search_edit, "placeholder_text", "inv-search");
        search_edit.set_clear_button_enabled(true);
        place(
            &search_edit,
            [0.0, 0.0, 0.0, 0.0],
            [gx + 20.0, top + 42.0, gx + 240.0, top + 76.0],
        );
        let q = queue.clone();
        search_edit
            .signals()
            .text_changed()
            .connect(move |t: GString| {
                push(&q, UiEvent::Inventory(InvInput::Search(t.to_string())))
            });
        let mut se = search_edit.clone();
        search_edit
            .signals()
            .text_submitted()
            .connect(move |_t: GString| se.release_focus());
        body.add_child(&search_edit);
        let mut order = theme::styled_label("", Face::Body, 14, theme::TEXT_OFF);
        i18n::text(&order, "inv-pack-order");
        order.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        order.set_vertical_alignment(VerticalAlignment::CENTER);
        place(
            &order,
            [0.0, 0.0, 0.0, 0.0],
            [gx + 260.0, top + 42.0, gx + GRID_COL_W - 20.0, top + 76.0],
        );
        body.add_child(&order);
        let mut grid_box = GridContainer::new_alloc();
        grid_box.set_columns(COLS as i32);
        grid_box.add_theme_constant_override("h_separation", CELL_GAP as i32);
        grid_box.add_theme_constant_override("v_separation", CELL_GAP as i32);
        grid_box.set_mouse_filter(MouseFilter::IGNORE);
        let grid_x = gx + (GRID_COL_W - GRID_W) / 2.0;
        let grid_y = top + 88.0;
        place(
            &grid_box,
            [0.0, 0.0, 0.0, 0.0],
            [grid_x, grid_y, grid_x + GRID_W, grid_y + GRID_H],
        );
        let mut cells = Vec::new();
        for i in 0..COLS * ROWS {
            let s = Socket::new(CELL, &looks);
            grid_box.add_child(&s.root);
            let m = mouse.clone();
            wire_socket(
                &s.root,
                Rc::new(move || {
                    m.borrow()
                        .cells
                        .get(i)
                        .copied()
                        .unwrap_or(InvTarget::Nothing)
                }),
                &queue,
                &mouse,
            );
            mouse.borrow_mut().cell_nodes.push(s.root.clone().upcast());
            cells.push(s);
        }
        body.add_child(&grid_box);
        let mut hint = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        place(
            &hint,
            [0.0, 0.0, 0.0, 0.0],
            [gx, grid_y + GRID_H + 14.0, gx + GRID_COL_W, top + body_h],
        );
        body.add_child(&hint);
        // under a small grid: what the pack holds by class, each a filter
        let mut pack_box = VBoxContainer::new_alloc();
        pack_box.set_mouse_filter(MouseFilter::IGNORE);
        pack_box.add_theme_constant_override("separation", 10);
        place(
            &pack_box,
            [0.0, 0.0, 0.0, 0.0],
            [
                gx + 20.0,
                top + body_h,
                gx + GRID_COL_W - 20.0,
                top + body_h,
            ],
        );
        body.add_child(&pack_box);

        // ---- the detail, right ----
        let dx = gx + GRID_COL_W + PAD;
        let mut detail_bg = theme::framed(Frame::Socket);
        detail_bg.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &detail_bg,
            [0.0, 0.0, 0.0, 0.0],
            [dx, top, dx + DETAIL_W, top + body_h],
        );
        body.add_child(&detail_bg);
        let mut detail = VBoxContainer::new_alloc();
        detail.set_mouse_filter(MouseFilter::IGNORE);
        detail.add_theme_constant_override("separation", 8);
        place(
            &detail,
            [0.0, 0.0, 0.0, 0.0],
            [
                dx + 18.0,
                top + 16.0,
                dx + DETAIL_W - 18.0,
                top + body_h - 14.0,
            ],
        );
        body.add_child(&detail);
        let mut detail_empty = theme::styled_label("", Face::Body, 16, theme::TEXT_OFF);
        i18n::text(&detail_empty, "inv-detail-empty");
        detail_empty.set_autowrap_mode(AutowrapMode::WORD_SMART);
        detail_empty.set_horizontal_alignment(HorizontalAlignment::CENTER);
        detail_empty.set_vertical_alignment(VerticalAlignment::CENTER);
        place(
            &detail_empty,
            [0.0, 0.0, 0.0, 0.0],
            [dx + 24.0, top, dx + DETAIL_W - 24.0, top + body_h],
        );
        body.add_child(&detail_empty);

        // ---- the drag preview, the context menu, the count picker ----
        let mut preview = TextureRect::new_alloc();
        preview.set_mouse_filter(MouseFilter::IGNORE);
        preview.set_expand_mode(ExpandMode::IGNORE_SIZE);
        preview.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        preview.set_size(Vector2::new(56.0, 56.0));
        preview.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.85));
        preview.set_visible(false);
        preview.set_z_index(10);
        root.add_child(&preview);
        let mut ctx_menu = theme::framed(Frame::Tooltip);
        ctx_menu.set_mouse_filter(MouseFilter::STOP);
        ctx_menu.set_visible(false);
        ctx_menu.set_z_index(5);
        let mut ctx_rows = VBoxContainer::new_alloc();
        ctx_rows.add_theme_constant_override("separation", 0);
        ctx_rows.set_custom_minimum_size(Vector2::new(300.0, 0.0));
        ctx_menu.add_child(&ctx_rows);
        root.add_child(&ctx_menu);
        let mut count_box = theme::framed(Frame::Panel);
        count_box.set_mouse_filter(MouseFilter::STOP);
        count_box.set_visible(false);
        count_box.set_z_index(6);
        let mut count_col = VBoxContainer::new_alloc();
        count_col.add_theme_constant_override("separation", 10);
        let mut count_label = theme::styled_label("", Face::Title, 24, theme::GOLD_BRIGHT);
        count_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        count_label.set_custom_minimum_size(Vector2::new(300.0, 0.0));
        count_col.add_child(&count_label);
        let mut count_hint = theme::styled_label("", Face::Body, 14, theme::TEXT_DIM);
        i18n::text(&count_hint, "inv-count-hint");
        count_hint.set_horizontal_alignment(HorizontalAlignment::CENTER);
        count_hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        count_col.add_child(&count_hint);
        let mut count_buttons = HBoxContainer::new_alloc();
        count_buttons.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        count_buttons.add_theme_constant_override("separation", 12);
        let ok = theme::button("", &queue, UiEvent::Inventory(InvInput::Count(0)));
        i18n::text(&ok, "dlg-ok");
        count_buttons.add_child(&ok);
        let cancel_count = theme::button("", &queue, UiEvent::Inventory(InvInput::CancelCount));
        i18n::text(&cancel_count, "dlg-cancel");
        count_buttons.add_child(&cancel_count);
        count_col.add_child(&count_buttons);
        count_box.add_child(&count_col);
        root.add_child(&count_box);
        {
            let mut m = mouse.borrow_mut();
            m.frame = Some(frame.clone());
            m.preview = Some(preview);
        }
        root.set_visible(false);
        InventoryPanel {
            queue,
            root,
            shade,
            frame,
            looks,
            mode: Mode::Closed,
            filter: InvFilter::All,
            search: String::new(),
            selected: None,
            focus: None,
            choose: None,
            counting: None,
            menu_for: None,
            drag_from: None,
            pack: Pack::new(),
            cells_shown: Vec::new(),
            dirty: true,
            mouse,
            title,
            subtitle,
            letters,
            ac_chip,
            gold_chip,
            confirm,
            cancel,
            close,
            tabs,
            suggested_tab,
            search_edit,
            cells,
            doll,
            doll_summary,
            hint,
            pack_box,
            pack_key: Vec::new(),
            detail,
            detail_key: None,
            detail_empty,
            ctx_menu,
            ctx_rows,
            count_box,
            count_label,
            status: (None, None, None),
            pad: false,
            pad_kind: None,
            doll_focus: None,
            carrying: None,
            ctx_buttons: Vec::new(),
            ctx_focus: 0,
            rows: 0,
            grid_top: grid_y,
            doll_view,
            hero_rect,
            figure,
            warm: Warm::Not,
        }
    }

    /// One frame of the warm-up behind the title screen: the panel open on
    /// a sample pack with the doll rendering `hero`, so the first real
    /// opening finds its pipelines compiled, its buffers allocated and its
    /// textures loaded.
    pub fn warm_step(&mut self, hero: Option<Gd<Node3D>>) -> WarmStep {
        let (frames, doll) = match self.warm {
            Warm::Done => return WarmStep::Done,
            Warm::Not => {
                let item = |letter, class, text: &str, slots| InvItem {
                    letter,
                    class,
                    tile: -1,
                    quan: 1,
                    slots,
                    lit: false,
                    text: text.to_string(),
                };
                let mut pack = Pack::new();
                pack.replace(&nh_protocol::Inventory {
                    items: vec![
                        item(
                            'a',
                            ')',
                            "a long sword (weapon in hand)",
                            vec![Slot::Weapon],
                        ),
                        item('b', '[', "a leather armor (being worn)", vec![Slot::Body]),
                        item('c', '!', "a bubbly potion", vec![]),
                    ],
                    twoweap: false,
                });
                self.set_pack(&pack);
                self.open();
                self.selected = Some('a');
                // made in this frame, hidden; drawn from the next
                self.layout();
                self.redraw();
                self.dirty = false;
                self.warm = Warm::Open { frames: 1, doll: 0 };
                return WarmStep::Panel;
            }
            Warm::Open { frames, doll } => (frames, doll),
        };
        // the panel is made, then drawn, then the doll renders, each in a
        // frame of its own: each costs a frame's work the first time
        self.set_hero(hero.filter(|_| frames >= DOLL_AFTER));
        self.sync();
        let doll = doll + u32::from(self.doll_rendered());
        // a few frames with the doll rendered; without a hero, a few anyway
        if doll >= 3 || frames >= 60 {
            self.warm_end();
            return WarmStep::Done;
        }
        self.warm = Warm::Open {
            frames: frames + 1,
            doll,
        };
        match (frames, doll) {
            (1, _) => WarmStep::Drawn,
            (_, 1) if self.doll_rendered() => WarmStep::Doll,
            _ => WarmStep::Shown,
        }
    }

    /// The panel in the language now: its tabs' tooltips, and everything
    /// it draws again (bound labels change by themselves).
    pub fn relang(&mut self) {
        for (f, b) in self.tabs.iter_mut() {
            b.set_tooltip_text(&filter_tip(*f));
        }
        self.pack_key.clear();
        self.detail_key = None;
        self.close_menu();
        self.dirty = true;
    }

    /// The warm-up is over (done, or a game starts): nothing of it stays.
    pub fn warm_end(&mut self) {
        if matches!(self.warm, Warm::Open { .. }) {
            self.set_hero(None);
            self.reset();
        }
        self.warm = Warm::Done;
    }

    // ---- state ----

    /// The header's title as shown (self-tests).
    pub fn title_text(&self) -> String {
        self.title.get_text().to_string()
    }

    pub fn is_open(&self) -> bool {
        self.mode != Mode::Closed
    }

    /// "browse", "select", "menu" (self-tests); None when closed.
    pub fn mode_name(&self) -> Option<&'static str> {
        match self.mode {
            Mode::Closed => None,
            Mode::Browse => Some("browse"),
            Mode::Select { .. } => Some("select"),
            Mode::Menu { .. } => Some("menu"),
        }
    }

    /// The request the panel answers (selection and menu modes).
    pub fn request(&self) -> Option<u64> {
        match &self.mode {
            Mode::Select { req, .. } | Mode::Menu { req, .. } => Some(*req),
            _ => None,
        }
    }

    /// The open menu's entries (menu mode; self-tests).
    pub fn menu_entries(&self) -> Option<&[MenuEntry]> {
        match &self.mode {
            Mode::Menu { state, .. } => Some(&state.entries),
            _ => None,
        }
    }

    pub fn filter(&self) -> InvFilter {
        self.filter
    }

    pub fn selected(&self) -> Option<char> {
        self.selected
    }

    /// The grid's cells: the letter ('-' for the hands) and whether it is
    /// suggested (self-tests).
    pub fn shown(&self) -> Vec<(char, bool)> {
        let q = match &self.mode {
            Mode::Select { question, .. } => Some(question),
            _ => None,
        };
        self.cells_shown
            .iter()
            .filter_map(|t| match t {
                InvTarget::Cell(c) => Some((*c, q.is_none_or(|q| q.all || q.suggests(*c)))),
                InvTarget::Hands => Some(('-', q.is_some_and(|q| q.hands))),
                _ => None,
            })
            .collect()
    }

    /// The letters each doll slot shows (self-tests).
    pub fn doll_letters(&self) -> Vec<(DollSlot, Vec<char>)> {
        DollSlot::LEFT
            .iter()
            .chain(DollSlot::RIGHT.iter())
            .chain(DollSlot::HANDS.iter())
            .map(|s| (*s, s.items(&self.pack).iter().map(|i| i.letter).collect()))
            .collect()
    }

    /// The context menu's rows (self-tests).
    pub fn context_rows(&self) -> Option<Vec<ItemActionKind>> {
        let l = self.menu_for?;
        let item = self.pack.by_letter(l)?;
        Some(
            actions_for(item, &self.pack)
                .into_iter()
                .map(|a| a.kind)
                .collect(),
        )
    }

    /// The search field has the keyboard.
    pub fn wants_text(&self) -> bool {
        self.is_open() && self.search_edit.has_focus()
    }

    /// Is the panel under `pos` (then the map is not hovered)?
    pub fn covers(&self, pos: Vector2) -> bool {
        self.is_open() && self.frame.get_global_rect().contains_point(pos)
    }

    /// The HUD's action bar slots on screen (drop targets).
    pub fn set_bar_rects(&mut self, rects: Vec<Rect2>) {
        self.mouse.borrow_mut().bar = rects;
    }

    /// The HUD's AC, gold and burden, for the header.
    pub fn set_status(&mut self, ac: Option<String>, gold: Option<String>, burden: Option<String>) {
        let s = (ac, gold, burden);
        if self.status != s {
            self.status = s;
            self.dirty = true;
        }
    }

    /// A new inventory.
    pub fn set_pack(&mut self, pack: &Pack) {
        self.pack = pack.clone();
        // its actions depend on the rest of the pack
        self.detail_key = None;
        if self.selected.is_some_and(|l| pack.by_letter(l).is_none()) {
            self.selected = None;
        }
        if self.menu_for.is_some_and(|l| pack.by_letter(l).is_none()) {
            self.close_menu();
        }
        self.dirty = true;
    }

    // ---- modes ----

    /// Open in browse mode (a new opening starts on All).
    pub fn open(&mut self) {
        if self.mode == Mode::Closed {
            self.mode = Mode::Browse;
            self.filter = InvFilter::All;
        }
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.mode = Mode::Closed;
        self.rows = 0;
        self.carrying = None;
        self.doll_focus = None;
        self.choose = None;
        self.counting = None;
        self.drag_from = None;
        self.close_menu();
        self.search_edit.release_focus();
        self.dirty = true;
    }

    fn was_open(&self) -> bool {
        match &self.mode {
            Mode::Closed => false,
            Mode::Browse => true,
            Mode::Select { was_open, .. } | Mode::Menu { was_open, .. } => *was_open,
        }
    }

    /// A getobj question: selection mode on Suggested.
    pub fn open_select(&mut self, req: u64, question: ItemQuestion, query: &str) {
        let was_open = self.was_open();
        self.close_menu();
        self.choose = None;
        self.counting = None;
        self.mode = Mode::Select {
            req,
            question,
            query: query.to_string(),
            count: None,
            was_open,
        };
        self.filter = InvFilter::Suggested;
        self.focus = None;
        self.dirty = true;
    }

    /// An engine menu of inventory items: multi-select mode.
    pub fn open_menu(&mut self, req: u64, how: PickHow, title: Option<&str>, items: &[MenuItem]) {
        let was_open = self.was_open();
        self.close_menu();
        self.choose = None;
        self.counting = None;
        self.mode = Mode::Menu {
            req,
            state: MenuState::new(how, title.map(str::to_string), items),
            was_open,
        };
        self.filter = InvFilter::All;
        self.focus = None;
        self.dirty = true;
    }

    /// The request the panel showed was answered: back to browse mode if
    /// it was open before, else closed.
    pub fn end_request(&mut self) {
        match &self.mode {
            Mode::Select { was_open, .. } | Mode::Menu { was_open, .. } => {
                let keep = *was_open;
                self.counting = None;
                if keep {
                    self.mode = Mode::Browse;
                    self.filter = InvFilter::All;
                    self.dirty = true;
                } else {
                    self.close();
                }
            }
            _ => {}
        }
    }

    fn question(&self) -> Option<&ItemQuestion> {
        match &self.mode {
            Mode::Select { question, .. } => Some(question),
            _ => None,
        }
    }

    // ---- inputs ----

    /// A widget's input; what the game should do about it.
    pub fn input(&mut self, ev: InvInput) -> Option<Intent> {
        self.dirty = true;
        match ev {
            InvInput::Toggle => {
                if self.is_open() && self.request().is_none() {
                    self.close();
                } else {
                    self.open();
                }
                None
            }
            InvInput::Close => match &self.mode {
                Mode::Select { .. } => Some(Intent::Reply(Reply::Char(27))),
                Mode::Menu { .. } => Some(Intent::Reply(Reply::Cancel)),
                _ => {
                    self.close();
                    None
                }
            },
            InvInput::Filter(f) => {
                self.filter = f;
                self.focus = None;
                None
            }
            InvInput::Search(s) => {
                self.search = s;
                None
            }
            InvInput::Click {
                target,
                button,
                shift,
                double,
            } => self.click(target, button, shift, double),
            InvInput::DragStart(from) => {
                self.drag_from = Some(from);
                None
            }
            InvInput::Drop { from, to, shift } => {
                self.drag_from = None;
                self.dropped(from, to, shift)
            }
            InvInput::Action { letter, kind } => {
                self.close_menu();
                self.action(letter, kind)
            }
            InvInput::CloseMenu => {
                self.close_menu();
                None
            }
            InvInput::Count(n) => {
                let n = if n == 0 {
                    self.counting.map_or(0, |(_, v, _)| v)
                } else {
                    n
                };
                self.counted(n)
            }
            InvInput::CancelCount => {
                self.counting = None;
                None
            }
            InvInput::Confirm => match &self.mode {
                Mode::Menu { state, .. } => Some(Intent::Reply(state.confirm())),
                _ => None,
            },
            InvInput::Carry => self.carry(),
            InvInput::Cancel => match &self.mode {
                Mode::Menu { .. } => Some(Intent::Reply(Reply::Cancel)),
                Mode::Select { .. } => Some(Intent::Reply(Reply::Char(27))),
                _ => None,
            },
        }
    }

    /// The item a target shows.
    fn item_at(&self, t: InvTarget) -> Option<&InvItem> {
        match t {
            InvTarget::Cell(l) => self.pack.by_letter(l),
            InvTarget::Doll(s) => s.items(&self.pack).first().copied(),
            _ => None,
        }
    }

    fn click(
        &mut self,
        target: InvTarget,
        button: i32,
        shift: bool,
        double: bool,
    ) -> Option<Intent> {
        self.close_menu();
        let letter = match target {
            InvTarget::Hands => Some('-'),
            t => self.item_at(t).map(|i| i.letter),
        };
        // a second choice is pending: this is it
        if let Some(choose) = self.choose {
            let to = letter.filter(|c| *c != '-')?;
            self.choose = None;
            return self.chosen(choose, to);
        }
        match &mut self.mode {
            Mode::Closed => None,
            Mode::Select {
                question, count, ..
            } => {
                let l = letter?;
                let quan = self.pack.by_letter(l).map_or(1, |i| i.quan);
                if (shift || button == 2) && question.takes_count() && quan > 1 {
                    let start = count.unwrap_or(quan as u32).min(quan as u32);
                    self.counting = Some((CountFor::Pick(l), start, quan as u32));
                    return None;
                }
                let count = count.take();
                Some(Intent::Pick { letter: l, count })
            }
            Mode::Menu { state, .. } => {
                let l = letter?;
                let i = state
                    .entries
                    .iter()
                    .position(|e| e.selectable && e.letter == Some(l))?;
                let quan = self.pack.by_letter(l).map_or(1, |i| i.quan);
                if (shift || button == 2) && quan > 1 {
                    self.counting = Some((CountFor::Entry(l), quan as u32, quan as u32));
                    return None;
                }
                self.focus = self
                    .cells_shown
                    .iter()
                    .position(|t| *t == InvTarget::Cell(l));
                match state.click(i) {
                    MenuOutcome::Done(r) => Some(Intent::Reply(r)),
                    _ => None,
                }
            }
            Mode::Browse => {
                let l = letter.filter(|c| *c != '-')?;
                self.selected = Some(l);
                self.focus = self
                    .cells_shown
                    .iter()
                    .position(|t| *t == InvTarget::Cell(l));
                if button == 2 {
                    self.open_context(l, target);
                    return None;
                }
                if !double {
                    return None;
                }
                let item = self.pack.by_letter(l)?.clone();
                let kind = match target {
                    InvTarget::Doll(slot) => slot.unequip(&item),
                    _ => default_action(&item, &self.pack),
                }?;
                self.action(l, kind)
            }
        }
    }

    /// An action on the item at `letter`: a macro, or first a count or a
    /// second item.
    fn action(&mut self, letter: char, kind: ItemActionKind) -> Option<Intent> {
        use ItemActionKind::*;
        let item = self.pack.by_letter(letter)?.clone();
        match kind {
            Adjust => {
                self.choose = Some(Choose::Adjust {
                    from: letter,
                    count: None,
                });
                None
            }
            Split => {
                let q = item.quan.max(1) as u32;
                self.counting = Some((CountFor::Split(letter), (q / 2).max(1), q));
                None
            }
            DropSome => {
                let q = item.quan.max(1) as u32;
                self.counting = Some((CountFor::Drop(letter), q, q));
                None
            }
            Dip => {
                self.choose = Some(Choose::Dip { from: letter });
                None
            }
            k => Macro::item(k, &item, None).map(Intent::Macro),
        }
    }

    fn chosen(&mut self, choose: Choose, to: char) -> Option<Intent> {
        match choose {
            Choose::Adjust { from, count } => {
                let item = self.pack.by_letter(from)?;
                (from != to || count.is_some())
                    .then(|| Intent::Macro(Macro::adjust(item, to, count)))
            }
            Choose::Dip { from } => {
                let item = self.pack.by_letter(from)?;
                (from != to).then(|| Intent::Macro(Macro::dip(item, to)))
            }
        }
    }

    fn counted(&mut self, n: u32) -> Option<Intent> {
        let (what, _, max) = self.counting.take()?;
        let n = n.clamp(1, max.max(1));
        match what {
            CountFor::Pick(l) => {
                if let Mode::Select { count, .. } = &mut self.mode {
                    *count = None;
                }
                Some(Intent::Pick {
                    letter: l,
                    count: (n < max).then_some(n),
                })
            }
            CountFor::Drop(l) => {
                let item = self.pack.by_letter(l)?;
                Macro::item(ItemActionKind::Drop, item, Some(n)).map(Intent::Macro)
            }
            CountFor::Split(l) => {
                self.choose = Some(Choose::Adjust {
                    from: l,
                    count: Some(n),
                });
                None
            }
            CountFor::Entry(l) => {
                let Mode::Menu { state, .. } = &mut self.mode else {
                    return None;
                };
                for d in n.to_string().chars() {
                    state.key(d);
                }
                match state.key(l) {
                    MenuOutcome::Done(r) => Some(Intent::Reply(r)),
                    _ => None,
                }
            }
        }
    }

    /// A drag ended (ui-design §2.6).
    fn dropped(&mut self, from: InvTarget, to: InvTarget, shift: bool) -> Option<Intent> {
        if from == to {
            return None;
        }
        let item = self.item_at(from)?.clone();
        if let InvTarget::Bar(slot) = to {
            let action = match from {
                InvTarget::Doll(s) => s
                    .unequip(&item)
                    .or_else(|| default_action(&item, &self.pack)),
                _ => default_action(&item, &self.pack),
            }
            .or_else(|| actions_for(&item, &self.pack).first().map(|a| a.kind))?;
            return Some(Intent::Bind {
                slot,
                binding: SlotBinding::item(&item, action),
            });
        }
        // only browse mode acts on drags: a question waits for an answer
        if self.mode != Mode::Browse {
            return None;
        }
        let kind = match (from, to) {
            (_, InvTarget::World) if shift && item.quan > 1 => {
                let q = item.quan as u32;
                self.counting = Some((CountFor::Drop(item.letter), q, q));
                return None;
            }
            (_, InvTarget::World) => ItemActionKind::Drop,
            (InvTarget::Cell(_), InvTarget::Doll(slot)) => {
                if slot
                    .items(&self.pack)
                    .iter()
                    .any(|i| i.letter == item.letter)
                {
                    return None;
                }
                slot.equip(&item)?
            }
            (InvTarget::Doll(slot), InvTarget::Cell(_) | InvTarget::Nothing) => {
                slot.unequip(&item)?
            }
            (InvTarget::Doll(_), InvTarget::Doll(to)) => to.equip(&item)?,
            (InvTarget::Cell(a), InvTarget::Cell(b)) if a != b => {
                return Some(Intent::Macro(Macro::adjust(&item, b, None)));
            }
            _ => return None,
        };
        Macro::item(kind, &item, None).map(Intent::Macro)
    }

    fn open_context(&mut self, letter: char, at: InvTarget) {
        let Some(item) = self.pack.by_letter(letter).cloned() else {
            return;
        };
        let actions = actions_for(&item, &self.pack);
        // out of the box at once, so it sizes to the new rows only
        for mut c in self.ctx_rows.get_children().iter_shared() {
            self.ctx_rows.remove_child(&c);
            c.queue_free();
        }
        let title = i18n::engine(EngineKind::Name, &title_of(&item)).into_owned();
        let mut head = theme::styled_label(&title, Face::Title, 18, theme::GOLD_BRIGHT);
        head.set_autowrap_mode(AutowrapMode::WORD_SMART);
        head.set_custom_minimum_size(Vector2::new(284.0, 0.0));
        self.ctx_rows.add_child(&head);
        self.ctx_rows.add_child(&separator());
        self.ctx_buttons.clear();
        for a in &actions {
            let row = self.action_row(letter, a, 300.0, 32.0, false);
            self.ctx_rows.add_child(&row);
            self.ctx_buttons.push((a.kind, row));
        }
        self.ctx_focus = 0;
        self.show_ctx_focus();
        self.menu_for = Some(letter);
        self.ctx_menu.reset_size();
        // next to the cell, inside the window
        let node = match at {
            InvTarget::Cell(_) => self
                .cells_shown
                .iter()
                .position(|t| *t == at)
                .map(|i| self.cells[i].root.clone().upcast::<Control>()),
            InvTarget::Doll(s) => self
                .doll
                .iter()
                .find(|(d, _)| *d == s)
                .map(|(_, sock)| sock.root.clone().upcast::<Control>()),
            _ => None,
        };
        let view = self.root.get_viewport_rect().size;
        let rect = node
            .map(|n| n.get_global_rect())
            .unwrap_or(Rect2::new(view * 0.5, Vector2::ZERO));
        let size = self.ctx_menu.get_combined_minimum_size();
        let mut pos = Vector2::new(rect.position.x + rect.size.x + 6.0, rect.position.y);
        if pos.x + size.x > view.x - 8.0 {
            pos.x = rect.position.x - size.x - 6.0;
        }
        pos.y = pos.y.min(view.y - size.y - 8.0).max(8.0);
        self.ctx_menu.set_global_position(pos);
        self.ctx_menu.set_visible(true);
    }

    fn close_menu(&mut self) {
        self.menu_for = None;
        self.ctx_menu.set_visible(false);
    }

    /// A button for an action: its label, its NetHack keys dim at the
    /// right, "(2 actions)" when it takes two commands.
    fn action_row(
        &self,
        letter: char,
        a: &ItemAction,
        w: f32,
        h: f32,
        primary: bool,
    ) -> Gd<Button> {
        let mut b = Button::new_alloc();
        b.set_focus_mode(FocusMode::NONE);
        b.set_custom_minimum_size(Vector2::new(w, h));
        b.set_text_alignment(HorizontalAlignment::LEFT);
        let mut text = label(a.label_key);
        if a.two_actions {
            text = tr!("inv-two-actions", action = text);
        }
        b.set_text(&text);
        if !primary {
            for s in ["normal", "disabled"] {
                let mut sb = flat(
                    Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    0,
                    2,
                );
                sb.set_content_margin(Side::LEFT, 10.0);
                b.add_theme_stylebox_override(s, &sb);
            }
            let mut hover = flat(
                Color::from_rgba(0.72, 0.54, 0.23, 0.2),
                theme::GOLD_DIM,
                1,
                2,
            );
            hover.set_content_margin(Side::LEFT, 10.0);
            b.add_theme_stylebox_override("hover", &hover);
            b.add_theme_stylebox_override("pressed", &hover);
        } else {
            b.add_theme_stylebox_override("normal", &theme::default_button_style());
        }
        let face = if a.default {
            Face::BodyBold
        } else {
            Face::Body
        };
        b.add_theme_font_override("font", &theme::font(face));
        b.add_theme_font_size_override("font_size", if primary { 18 } else { 16 });
        // an extended command does not fit beside a narrow button's label:
        // the tooltip has it
        let short = primary || w > 200.0 || a.keys.len() <= 3;
        b.set_tooltip_text(&format!("{}  ({})", label(a.label_key), a.keys));
        let hint = if short { a.keys } else { "" };
        let mut keys = theme::styled_label(hint, Face::Mono, 14, theme::TEXT_DIM);
        keys.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        keys.set_vertical_alignment(VerticalAlignment::CENTER);
        place(&keys, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, -10.0, 0.0]);
        b.add_child(&keys);
        let q = self.queue.clone();
        let kind = a.kind;
        b.signals()
            .pressed()
            .connect(move || push(&q, UiEvent::Inventory(InvInput::Action { letter, kind })));
        b
    }

    // ---- keys ----

    /// A key while the panel is open; `command` when the engine waits at
    /// its command prompt.
    pub fn key(&mut self, k: &KeyInput, command: bool) -> KeyUse {
        if !self.is_open() || k.echo {
            return KeyUse::Pass;
        }
        self.dirty = true;
        let ch = match k.key {
            Key::Char(c) if !k.mods.ctrl && !k.mods.alt => Some(c),
            _ => None,
        };
        // the count picker
        if let Some((what, v, max)) = self.counting {
            match k.key {
                Key::Escape => self.counting = None,
                Key::Enter | Key::KeypadEnter => {
                    return self.counted(v).map_or(KeyUse::Used, KeyUse::Then);
                }
                Key::Left | Key::Down => {
                    self.counting = Some((what, v.saturating_sub(1).max(1), max))
                }
                Key::Right | Key::Up => self.counting = Some((what, (v + 1).min(max), max)),
                Key::Backspace => self.counting = Some((what, (v / 10).max(1), max)),
                Key::Char(d @ '0'..='9') => {
                    let fresh = v.saturating_mul(10).saturating_add(d as u32 - '0' as u32);
                    let n = if fresh > max {
                        d as u32 - '0' as u32
                    } else {
                        fresh
                    };
                    self.counting = Some((what, n.max(1), max));
                }
                _ => {}
            }
            return KeyUse::Used;
        }
        // a second item or letter being chosen
        if let Some(choose) = self.choose {
            if k.key == Key::Escape {
                self.choose = None;
                return KeyUse::Used;
            }
            if let Some(c) = ch.filter(|c| c.is_ascii_alphabetic()) {
                self.choose = None;
                return self.chosen(choose, c).map_or(KeyUse::Used, KeyUse::Then);
            }
            return KeyUse::Used;
        }
        // the context menu: Esc closes it, a row's key runs it, the arrows
        // and Enter (a gamepad) pick a row
        if let Some(l) = self.menu_for {
            if k.key == Key::Escape {
                self.close_menu();
                return KeyUse::Used;
            }
            let n = self.ctx_buttons.len();
            if n > 0 {
                match k.key {
                    Key::Up | Key::Down => {
                        let step = if k.key == Key::Up { n - 1 } else { 1 };
                        self.ctx_focus = (self.ctx_focus + step) % n;
                        self.show_ctx_focus();
                        return KeyUse::Used;
                    }
                    Key::Enter | Key::KeypadEnter => {
                        let kind = self.ctx_buttons[self.ctx_focus.min(n - 1)].0;
                        self.close_menu();
                        return self.action(l, kind).map_or(KeyUse::Used, KeyUse::Then);
                    }
                    _ => {}
                }
            }
            if let (Some(c), Some(item)) = (ch, self.pack.by_letter(l)) {
                let hit = actions_for(item, &self.pack)
                    .into_iter()
                    .find(|a| a.keys.len() == 1 && a.keys.starts_with(c));
                if let Some(a) = hit {
                    self.close_menu();
                    return self.action(l, a.kind).map_or(KeyUse::Used, KeyUse::Then);
                }
            }
        }
        if k.key == Key::Escape && self.carrying.take().is_some() {
            self.drag_from = None;
            return KeyUse::Used;
        }
        if k.key == Key::Tab {
            self.cycle_filter(if k.mods.shift { -1 } else { 1 });
            return KeyUse::Used;
        }
        // the doll's sockets (the keyboard or a gamepad): its columns, the
        // weapons row, and back to the grid on the right
        if self.mode == Mode::Browse
            && matches!(k.key, Key::Left | Key::Right | Key::Up | Key::Down)
        {
            if let Some(d) = self.doll_focus {
                let next = doll_step(d, k.key);
                match next {
                    DollStep::To(i) => self.doll_focus = Some(i),
                    DollStep::Grid(row) => {
                        self.doll_focus = None;
                        let n = self.cells_shown.len();
                        self.focus = (n > 0).then(|| (row * COLS).min(n - 1));
                    }
                    DollStep::Stay => {}
                }
                self.select_focus();
                return KeyUse::Used;
            }
            let at_left_edge = self.focus.is_none_or(|f| f % COLS == 0);
            if k.key == Key::Left && at_left_edge {
                let row = self.focus.map_or(0, |f| f / COLS).min(5);
                self.focus = None;
                self.doll_focus = Some(6 + row);
                self.select_focus();
                return KeyUse::Used;
            }
        }
        let step = match k.key {
            Key::Left => Some(-1),
            Key::Right => Some(1),
            Key::Up => Some(-(COLS as i32)),
            Key::Down => Some(COLS as i32),
            _ => None,
        };
        if let Some(s) = step {
            let n = self.cells_shown.len() as i32;
            if n > 0 {
                let f = self
                    .focus
                    .map_or(if s > 0 { -s.signum() } else { n }, |f| f as i32);
                self.focus = Some((f + s).clamp(0, n - 1) as usize);
                if let Some(InvTarget::Cell(l)) = self.focused()
                    && self.mode == Mode::Browse
                {
                    self.selected = Some(l);
                }
            }
            return KeyUse::Used;
        }
        let focused = self.focused();
        let target = self.focus_target();
        match &mut self.mode {
            Mode::Closed => KeyUse::Pass,
            Mode::Browse => match k.key {
                Key::Escape if command => {
                    self.close();
                    KeyUse::Used
                }
                // the focused item's first action (on the doll: off it)
                Key::Enter | Key::KeypadEnter => {
                    let Some(item) = target.and_then(|t| self.item_at(t)).cloned() else {
                        return KeyUse::Used;
                    };
                    let kind = match target {
                        Some(InvTarget::Doll(slot)) => slot.unequip(&item),
                        _ => default_action(&item, &self.pack),
                    };
                    kind.and_then(|kind| self.action(item.letter, kind))
                        .map_or(KeyUse::Used, KeyUse::Then)
                }
                Key::Char(' ') => {
                    if let Some(t) = target
                        && let Some(l) = self.item_at(t).map(|i| i.letter)
                    {
                        self.open_context(l, t);
                    }
                    KeyUse::Used
                }
                _ => KeyUse::Pass,
            },
            Mode::Select {
                question, count, ..
            } => match (k.key, ch) {
                (Key::Escape, _) => KeyUse::Then(Intent::Reply(Reply::Char(27))),
                (_, Some('?')) => {
                    self.filter = InvFilter::Suggested;
                    KeyUse::Used
                }
                (_, Some('*')) => {
                    self.filter = InvFilter::All;
                    KeyUse::Used
                }
                (_, Some(d @ '0'..='9')) if question.takes_count() => {
                    let v = count.unwrap_or(0).saturating_mul(10) + (d as u32 - '0' as u32);
                    *count = (v > 0).then_some(v);
                    KeyUse::Used
                }
                (Key::Backspace, _) => {
                    *count = count.and_then(|v| (v >= 10).then_some(v / 10));
                    KeyUse::Used
                }
                (Key::Enter | Key::KeypadEnter, _) => {
                    let letter = match focused {
                        Some(InvTarget::Cell(l)) => l,
                        Some(InvTarget::Hands) => '-',
                        _ => return KeyUse::Used,
                    };
                    KeyUse::Then(Intent::Pick {
                        letter,
                        count: count.take(),
                    })
                }
                (_, Some(c)) if c.is_ascii_graphic() => KeyUse::Then(Intent::Pick {
                    letter: c,
                    count: count.take(),
                }),
                _ => KeyUse::Used,
            },
            Mode::Menu { state, .. } => {
                let c = match k.key {
                    Key::Escape => '\u{1b}',
                    Key::Enter | Key::KeypadEnter => '\n',
                    Key::Char(' ') => {
                        // Space toggles the focused item
                        if let Some(InvTarget::Cell(l)) = focused
                            && let Some(i) = state
                                .entries
                                .iter()
                                .position(|e| e.selectable && e.letter == Some(l))
                        {
                            return match state.click(i) {
                                MenuOutcome::Done(r) => KeyUse::Then(Intent::Reply(r)),
                                _ => KeyUse::Used,
                            };
                        }
                        '\n'
                    }
                    Key::Char(c) => c,
                    _ => return KeyUse::Used,
                };
                if let Some(i) = state
                    .entries
                    .iter()
                    .position(|e| e.selectable && e.letter == Some(c))
                {
                    let l = state.entries[i].letter;
                    self.focus = self
                        .cells_shown
                        .iter()
                        .position(|t| Some(*t) == l.map(InvTarget::Cell));
                }
                let Mode::Menu { state, .. } = &mut self.mode else {
                    return KeyUse::Used;
                };
                match state.key(c) {
                    MenuOutcome::Done(r) => KeyUse::Then(Intent::Reply(r)),
                    _ => KeyUse::Used,
                }
            }
        }
    }

    /// What the keyboard is on: a doll socket or a cell.
    fn focus_target(&self) -> Option<InvTarget> {
        match self.doll_focus {
            Some(d) => self.doll.get(d).map(|(s, _)| InvTarget::Doll(*s)),
            None => self.focused(),
        }
    }

    /// The detail follows the focus (browse mode).
    fn select_focus(&mut self) {
        if self.mode == Mode::Browse
            && let Some(l) = self
                .focus_target()
                .and_then(|t| self.item_at(t))
                .map(|i| i.letter)
        {
            self.selected = Some(l);
        }
    }

    /// Y: pick the focused item up, or put the one held down at the focus
    /// (what a drag does).
    fn carry(&mut self) -> Option<Intent> {
        if self.mode != Mode::Browse {
            return None;
        }
        let at = self.focus_target();
        match self.carrying.take() {
            None => {
                let t = at.filter(|t| self.item_at(*t).is_some())?;
                self.carrying = Some(t);
                self.drag_from = Some(t);
                None
            }
            Some(from) => {
                self.drag_from = None;
                self.dropped(from, at.unwrap_or(InvTarget::Nothing), false)
            }
        }
    }

    /// A gamepad gives the input (the focus shows at once, the help names
    /// its buttons).
    pub fn set_pad(&mut self, kind: Option<PadKind>) {
        if self.pad_kind != kind {
            self.pad_kind = kind;
            self.pad = kind.is_some();
            self.dirty = true;
        }
    }

    /// The context menu's row the d-pad is on.
    fn show_ctx_focus(&mut self) {
        let focus = self.ctx_focus;
        let mut on = self.looks.selected.duplicate_resource();
        on.set_content_margin(Side::LEFT, 10.0);
        on.set_shadow_size(0);
        for (i, (_, b)) in self.ctx_buttons.iter_mut().enumerate() {
            if i == focus {
                b.add_theme_stylebox_override("normal", &on);
            } else {
                b.remove_theme_stylebox_override("normal");
                let mut sb = flat(
                    Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    Color::from_rgba(0.0, 0.0, 0.0, 0.0),
                    0,
                    2,
                );
                sb.set_content_margin(Side::LEFT, 10.0);
                b.add_theme_stylebox_override("normal", &sb);
            }
        }
    }

    fn focused(&self) -> Option<InvTarget> {
        self.focus.and_then(|f| self.cells_shown.get(f).copied())
    }

    fn cycle_filter(&mut self, dir: i32) {
        let mut tabs: Vec<InvFilter> = InvFilter::TABS.to_vec();
        if self.question().is_some() {
            tabs.insert(0, InvFilter::Suggested);
        }
        let i = tabs.iter().position(|f| *f == self.filter).unwrap_or(0) as i32;
        let n = tabs.len() as i32;
        self.filter = tabs[((i + dir) % n + n) as usize % n as usize];
        self.focus = None;
    }

    /// A dialog event for the menu this panel shows (the soak, the
    /// self-tests: the same events as the generic menu).
    pub fn dialog_event(&mut self, req: u64, ev: &crate::ui_events::DialogEvent) -> Option<Reply> {
        use crate::ui_events::DialogEvent;
        let Mode::Menu { req: r, state, .. } = &mut self.mode else {
            return None;
        };
        if *r != req {
            return None;
        }
        self.dirty = true;
        match ev {
            DialogEvent::MenuClick(i) => match state.click(*i) {
                MenuOutcome::Done(r) => Some(r),
                _ => None,
            },
            DialogEvent::MenuConfirm => Some(state.confirm()),
            DialogEvent::MenuCancel | DialogEvent::Close => Some(Reply::Cancel),
            _ => None,
        }
    }

    // ---- drawing ----

    /// Every frame while a game is on screen.
    pub fn sync(&mut self) {
        let open = self.is_open();
        if self.root.is_visible() != open {
            self.root.set_visible(open);
        }
        if !open {
            return;
        }
        self.layout();
        if std::mem::take(&mut self.dirty) {
            self.redraw();
        }
        self.pulse();
        let shown = self.doll_view.frame(now_secs());
        if self.hero_rect.is_visible() != shown {
            self.hero_rect.set_visible(shown);
            self.figure.set_visible(!shown);
        }
    }

    /// The hero's model on the map (None: none yet), for the doll.
    pub fn set_hero(&mut self, node: Option<Gd<Node3D>>) {
        self.doll_view.hero = node.filter(|n| n.is_instance_valid());
    }

    /// The panel's frame on screen while it is open (self-tests).
    pub fn frame_rect(&self) -> Option<Rect2> {
        self.is_open().then(|| self.frame.get_global_rect())
    }

    /// Whether the doll shows the hero's render (self-tests).
    pub fn doll_rendered(&self) -> bool {
        self.hero_rect.is_visible()
    }

    /// The share of the doll's render that is drawn on: next to none when
    /// its camera sees no hero. None without a renderer (self-tests).
    pub fn doll_drawn(&self) -> Option<f32> {
        self.doll_share(|c| c.a > 0.5)
    }

    /// The share of the doll's render whose colour `pick` takes. None
    /// without a renderer (self-tests).
    pub fn doll_share(&self, pick: fn(Color) -> bool) -> Option<f32> {
        let image = self.doll_view.viewport.get_texture()?.get_image()?;
        let (w, h) = (image.get_width(), image.get_height());
        if w == 0 || h == 0 {
            return None;
        }
        // every fourth pixel each way tells as much
        let cells = (0..h)
            .step_by(4)
            .flat_map(|y| (0..w).step_by(4).map(move |x| (x, y)));
        let (mut picked, mut all) = (0u32, 0u32);
        for (x, y) in cells {
            all += 1;
            picked += u32::from(pick(image.get_pixel(x, y)));
        }
        Some(picked as f32 / all as f32)
    }

    /// Place and scale the panel above the HUD's bottom cluster.
    fn layout(&mut self) {
        let view = self.root.get_viewport_rect().size;
        let room_h = view.y - CLUSTER_TOP - 8.0;
        let s = ((view.x - 32.0) / WIDTH).min(room_h / HEIGHT).min(1.0);
        let scale = Vector2::new(s, s);
        if self.frame.get_scale() != scale {
            self.frame.set_scale(scale);
        }
        let pos = Vector2::new(
            ((view.x - WIDTH * s) / 2.0).round(),
            (view.y - CLUSTER_TOP - HEIGHT * s).max(8.0).round(),
        );
        if self.frame.get_position() != pos {
            self.frame.set_position(pos);
        }
    }

    /// The suggested cells' gold rim, 1 s between GOLD and its bright tone.
    fn pulse(&mut self) {
        let Some(q) = self.question().cloned() else {
            return;
        };
        let t = (0.5 + 0.5 * (now_secs() * std::f64::consts::TAU * PULSE_HZ).sin()) as f32;
        let c = PULSE_LOW.lerp(PULSE_HIGH, t as f64);
        for (i, target) in self.cells_shown.iter().enumerate() {
            let on = match target {
                InvTarget::Cell(l) => !q.all && q.suggests(*l),
                InvTarget::Hands => q.hands,
                _ => false,
            };
            if on {
                self.cells[i].rim.set_self_modulate(c);
            }
        }
    }

    /// The rows the pack needs: its items (and the hands) and one empty
    /// row to drop on, at least `MIN_ROWS`.
    fn wanted_rows(&self) -> usize {
        let n = self.pack.items().len() + 1;
        (n.div_ceil(COLS) + 1).clamp(MIN_ROWS, ROWS)
    }

    fn redraw(&mut self) {
        let rows = self.wanted_rows().max(self.rows);
        if rows != self.rows {
            self.rows = rows;
            let bottom = self.grid_top + rows as f32 * (CELL + CELL_GAP) - CELL_GAP;
            self.hint.set_offset(Side::TOP, bottom + 14.0);
            // the hint takes two lines; the pack's summary goes under it
            let free = self.hint.get_offset(Side::BOTTOM) - (bottom + 80.0);
            self.pack_box.set_offset(Side::TOP, bottom + 80.0);
            self.pack_box.set_visible(free > 120.0);
        }
        let question = self.question().cloned();
        let menu_letters: Option<Vec<(char, bool, Option<i64>)>> = match &self.mode {
            Mode::Menu { state, .. } => Some(
                state
                    .entries
                    .iter()
                    .filter(|e| e.selectable)
                    .filter_map(|e| e.letter.map(|l| (l, e.selected, e.count)))
                    .collect(),
            ),
            _ => None,
        };
        // the cells
        let filter = if question.is_none() && self.filter == InvFilter::Suggested {
            InvFilter::All
        } else {
            self.filter
        };
        let cells = grid(&self.pack, filter, question.as_ref(), &self.search);
        let mut shown: Vec<InvTarget> = Vec::new();
        let mut class_starts = Vec::new();
        for c in &cells {
            match c {
                GridCell::Hands => {
                    shown.push(InvTarget::Hands);
                    class_starts.push(false);
                }
                GridCell::Item { item, class_start } => {
                    if let Some(m) = &menu_letters
                        && !m.iter().any(|(l, ..)| *l == item.letter)
                    {
                        continue;
                    }
                    shown.push(InvTarget::Cell(item.letter));
                    class_starts.push(*class_start);
                }
            }
        }
        shown.truncate(self.cells.len());
        self.mouse.borrow_mut().cells = shown.clone();
        self.cells_shown = shown.clone();
        if self.focus.is_some_and(|f| f >= shown.len()) {
            self.focus = None;
        }
        // a gamepad's focus starts on the first suggested item
        if self.pad && self.focus.is_none() && self.doll_focus.is_none() && !shown.is_empty() {
            let q = question.as_ref();
            self.focus = Some(
                shown
                    .iter()
                    .position(|t| match t {
                        InvTarget::Cell(l) => q.is_none_or(|q| q.all || q.suggests(*l)),
                        _ => false,
                    })
                    .unwrap_or(0),
            );
            if let Some(InvTarget::Cell(l)) = self.focus.and_then(|f| shown.get(f))
                && self.mode == Mode::Browse
            {
                self.selected = Some(*l);
            }
        }
        let drag_item = self.drag_from.and_then(|t| self.item_at(t)).cloned();
        let preview = self
            .selected
            .and_then(|l| self.pack.by_letter(l))
            .map(|i| icons::item_icon(i.tile, i.class));
        for (i, sock) in self.cells.iter_mut().enumerate() {
            let target = shown.get(i).copied();
            let item = match target {
                Some(InvTarget::Cell(l)) => self.pack.by_letter(l),
                _ => None,
            };
            sock.rim.set_visible(false);
            sock.check.set_visible(false);
            sock.root.set_modulate(Color::WHITE);
            let in_grid = i < rows * COLS;
            if sock.root.is_visible() != in_grid {
                sock.root.set_visible(in_grid);
            }
            match target {
                Some(InvTarget::Hands) => {
                    sock.show(None);
                    sock.icon.set_texture(&icons::glyph_icon(Glyph::Hand));
                    sock.icon.set_modulate(theme::GOLD_BRIGHT);
                    sock.letter.set_text("-");
                    sock.pill.set_visible(true);
                    let verb = question.as_ref().map_or("", |q| q.verb.as_str());
                    sock.root
                        .set_tooltip_text(&format!("{}  (-)", hands_label(verb)));
                    set_style(&mut sock.root, "panel", &self.looks.normal);
                    if question.as_ref().is_some_and(|q| q.hands) {
                        sock.rim.set_visible(true);
                    }
                }
                Some(InvTarget::Cell(_)) => {
                    let item = item.expect("a shown cell has its item");
                    sock.icon.set_modulate(Color::WHITE);
                    sock.show(Some(item));
                    sock.class_mark.set_visible(class_starts[i]);
                    if class_starts[i] {
                        sock.class_mark
                            .set_texture(&icons::glyph_icon(icons::class_glyph(item.class)));
                    }
                    let equipped = nh_world::equipped(item);
                    let mut style = if equipped {
                        &self.looks.equipped
                    } else {
                        &self.looks.normal
                    };
                    let focus = self.focus == Some(i);
                    if self.selected == Some(item.letter) && self.mode == Mode::Browse || focus {
                        style = &self.looks.selected;
                    }
                    let mut tip = i18n::engine(EngineKind::Name, &item.text).into_owned();
                    if let Some(q) = &question {
                        let on = q.all || q.suggests(item.letter);
                        sock.rim.set_visible(on && !q.all);
                        if !on {
                            sock.root
                                .set_modulate(Color::from_rgba(0.55, 0.52, 0.5, 0.4));
                        }
                        let verb = i18n::engine(EngineKind::Prompt, &q.verb).into_owned();
                        tip.push('\n');
                        tip.push_str(&tr!(
                            "inv-cell-select-tip",
                            letter = item.letter,
                            verb = verb
                        ));
                    } else if let Some(m) = &menu_letters {
                        if let Some((_, true, count)) = m.iter().find(|(l, ..)| *l == item.letter) {
                            {
                                style = &self.looks.picked;
                                sock.check.set_text(&match count {
                                    Some(n) => format!("{n}"),
                                    None => "✔".to_string(),
                                });
                                sock.check.set_visible(true);
                            }
                        }
                        tip.push('\n');
                        tip.push_str(&tr!("inv-cell-menu-tip"));
                    } else if let Some(a) = default_action(item, &self.pack) {
                        tip.push('\n');
                        tip.push_str(&tr!(
                            "inv-cell-tip",
                            action = label(a.label_key()),
                            keys = a.keys()
                        ));
                    }
                    if let Some(d) = &drag_item
                        && d.letter == item.letter
                    {
                        sock.root.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.4));
                    }
                    set_style(&mut sock.root, "panel", style);
                    sock.root.set_tooltip_text(&tip);
                }
                _ => {
                    sock.icon.set_modulate(Color::WHITE);
                    sock.show(None);
                    set_style(&mut sock.root, "panel", &self.looks.empty);
                    sock.root.set_tooltip_text("");
                }
            }
        }
        // the doll
        let doll_focus = self.doll_focus;
        for (di, (slot, sock)) in self.doll.iter_mut().enumerate() {
            let items = slot.items(&self.pack);
            let first = items.first().copied();
            sock.show(first);
            sock.badge.set_visible(false);
            if items.len() > 1 {
                sock.count.set_text(&items.len().to_string());
            }
            let mut style = if first.is_some() {
                &self.looks.equipped
            } else {
                &self.looks.empty
            };
            let focused = doll_focus == Some(di);
            if focused
                || first.is_some_and(|i| self.selected == Some(i.letter))
                    && self.mode == Mode::Browse
            {
                style = &self.looks.selected;
            }
            sock.rim.set_visible(false);
            sock.root.set_modulate(Color::WHITE);
            if let Some(d) = &drag_item {
                if slot.accepts(d) {
                    sock.rim.set_visible(true);
                    sock.rim.set_self_modulate(theme::GOLD_BRIGHT);
                } else {
                    sock.root.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.4));
                }
            }
            set_style(&mut sock.root, "panel", style);
            let tip = match first {
                Some(i) => {
                    let name = i18n::engine(EngineKind::Name, &i.text).into_owned();
                    let mut tip = tr!("inv-doll-tip", slot = slot.name(), item = name);
                    if let Some(k) = slot.unequip(i) {
                        tip.push('\n');
                        tip.push_str(&tr!(
                            "inv-doll-out-tip",
                            action = label(k.label_key()),
                            keys = k.keys()
                        ));
                    }
                    tip
                }
                None => tr!("inv-doll-empty-tip", slot = slot.name()),
            };
            sock.root.set_tooltip_text(&tip);
        }
        let mut summary = Vec::new();
        if let Some(ac) = &self.status.0 {
            summary.push(tr!("inv-ac", ac = ac));
        }
        if let Some(b) = self.status.2.as_ref().filter(|b| !b.is_empty()) {
            summary.push(i18n::engine(EngineKind::Status, b).into_owned());
        }
        let mut text = summary.join("  ·  ");
        text.push('\n');
        match self.pack.wielded() {
            Some(w) => {
                let name = i18n::engine(EngineKind::Name, &title_of(w)).into_owned();
                text.push_str(&tr!("inv-wielding", item = name));
            }
            None => text.push_str(&tr!("inv-empty-handed")),
        }
        self.doll_summary.set_text(&text);
        self.mouse.borrow_mut().preview_tex = preview;
        // the header
        let used = self
            .pack
            .items()
            .iter()
            .filter(|i| i.letter.is_ascii_alphabetic())
            .count();
        self.letters.set_text(&tr!("inv-letters", used = used));
        self.ac_chip
            .set_text(&tr!("inv-ac", ac = self.status.0.as_deref().unwrap_or("?")));
        self.gold_chip.set_text(&tr!(
            "inv-gold",
            gold = self.status.1.as_deref().unwrap_or("0")
        ));
        let (title, sub, hint) = self.texts(question.as_ref());
        self.title.set_text(&title);
        self.subtitle.set_text(&sub);
        let tw = theme::font(Face::Title)
            .get_string_size_ex(&title)
            .font_size(30)
            .done()
            .x;
        self.subtitle.set_offset(Side::LEFT, PAD + 4.0 + tw + 24.0);
        self.hint.set_text(&i18n::whole_parts(&hint));
        let menu = matches!(self.mode, Mode::Menu { .. });
        self.confirm.set_visible(
            menu && matches!(&self.mode, Mode::Menu { state, .. } if state.how == PickHow::Any),
        );
        self.cancel.set_visible(menu || question.is_some());
        self.close.set_visible(!menu && question.is_none());
        // the tabs
        self.suggested_tab.set_visible(question.is_some());
        let on = |b: &mut Gd<Button>, active: bool, looks: &Looks| {
            let sb = if active { &looks.tab_on } else { &looks.tab };
            set_style(b, "normal", sb);
            set_style(b, "pressed", sb);
        };
        on(
            &mut self.suggested_tab,
            filter == InvFilter::Suggested,
            &self.looks,
        );
        for (f, b) in self.tabs.iter_mut() {
            on(b, *f == filter, &self.looks);
        }
        self.draw_detail();
        self.draw_pack();
        // the count picker
        match self.counting {
            Some((what, v, max)) => {
                let key = match what {
                    CountFor::Pick(_) | CountFor::Entry(_) => "inv-count-how-many",
                    CountFor::Drop(_) => "inv-count-drop",
                    CountFor::Split(_) => "inv-count-split",
                };
                let verb = i18n::tr(key);
                self.count_label
                    .set_text(&tr!("inv-count", verb = verb, value = v, most = max));
                self.count_box.reset_size();
                let view = self.root.get_viewport_rect().size;
                let size = self.count_box.get_combined_minimum_size();
                self.count_box.set_global_position((view - size) * 0.5);
                self.count_box.set_visible(true);
            }
            None => self.count_box.set_visible(false),
        }
    }

    /// The header's title and subtitle, the hint under the grid.
    fn texts(&self, question: Option<&ItemQuestion>) -> (String, String, String) {
        let (title, sub, hint) = self.mouse_texts(question);
        match self.pad_kind {
            Some(kind) => (title, sub, self.pad_hint(kind, question)),
            None => (title, sub, hint),
        }
    }

    /// The help line for a gamepad: its buttons, no mouse.
    fn pad_hint(&self, kind: PadKind, question: Option<&ItemQuestion>) -> String {
        use PadButton::*;
        let b = |b| kind.label(b);
        let filter = tr!("inv-pad-filter", lb = b(Lb), rb = b(Rb));
        if self.choose.is_some() {
            return tr!("inv-pad-choose", a = b(A), b = b(B));
        }
        match &self.mode {
            Mode::Select { .. } => {
                let hint = tr!("inv-pad-select", a = b(A), b = b(B), filter = filter);
                if question.is_some_and(|q| q.all) {
                    tr!("inv-nothing-suggested", hint = hint)
                } else {
                    hint
                }
            }
            Mode::Menu { .. } => tr!(
                "inv-pad-menu",
                a = b(A),
                start = b(Start),
                b = b(B),
                filter = filter
            ),
            _ if self.carrying.is_some() => tr!("inv-pad-carrying", y = b(Y), b = b(B)),
            _ => tr!(
                "inv-pad-browse",
                a = b(A),
                x = b(X),
                y = b(Y),
                b = b(B),
                filter = filter
            ),
        }
    }

    fn mouse_texts(&self, question: Option<&ItemQuestion>) -> (String, String, String) {
        if let Some(choose) = self.choose {
            let (sub, hint) = match choose {
                Choose::Adjust { from, count } => (
                    match count {
                        Some(n) => tr!("inv-split-to", n = n, from = from),
                        None => tr!("inv-adjust-to", from = from),
                    },
                    tr!("inv-adjust-hint"),
                ),
                Choose::Dip { from } => (tr!("inv-dip-into", from = from), tr!("inv-dip-hint")),
            };
            return (tr!("inv-title"), sub, hint);
        }
        match &self.mode {
            Mode::Select { query, count, .. } => {
                let q = question.expect("selection mode has its question");
                let question = query.split(" [").next().unwrap_or(query);
                let mut sub = i18n::engine(EngineKind::Prompt, question).into_owned();
                if let Some(n) = count {
                    sub.push_str("   ");
                    sub.push_str(&tr!("inv-count-typed", n = *n));
                }
                let mut hint = tr!("inv-select-hint");
                if q.takes_count() {
                    hint = tr!("inv-select-hint-count", hint = hint);
                }
                if q.all {
                    hint = tr!("inv-nothing-suggested", hint = hint);
                }
                (tr!("inv-choose"), sub, hint)
            }
            Mode::Menu { state, .. } => {
                let title = state.title.as_deref().map_or(String::new(), |t| {
                    i18n::engine(EngineKind::Menu, t).into_owned()
                });
                let hint = if state.how == PickHow::Any {
                    tr!("inv-menu-any-hint")
                } else {
                    tr!("inv-menu-one-hint")
                };
                let n = state.entries.iter().filter(|e| e.selected).count();
                let sub = if state.how == PickHow::Any {
                    format!("{title}   {}", tr!("dlg-selected", n = n))
                } else {
                    title
                };
                (tr!("inv-choose"), sub, hint)
            }
            _ if self.carrying.is_some() => (
                tr!("inv-title"),
                tr!("inv-carrying"),
                tr!("inv-carrying-hint"),
            ),
            _ => (tr!("inv-title"), String::new(), tr!("inv-browse-hint")),
        }
    }

    /// "Your pack": a button per class carried (its count), which filters
    /// the grid to it; drawn again only when the counts change.
    fn draw_pack(&mut self) {
        let counts: Vec<(InvFilter, usize)> = InvFilter::TABS
            .iter()
            .skip(1)
            .map(|&f| {
                (
                    f,
                    self.pack
                        .items()
                        .iter()
                        .filter(|i| f.shows(i, None))
                        .count(),
                )
            })
            .filter(|&(_, n)| n > 0)
            .collect();
        if counts == self.pack_key {
            return;
        }
        self.pack_key = counts.clone();
        for mut c in self.pack_box.get_children().iter_shared() {
            c.queue_free();
        }
        let mut head =
            theme::styled_label(&tr!("inv-your-pack"), Face::Title, 21, theme::GOLD_BRIGHT);
        head.set_horizontal_alignment(HorizontalAlignment::CENTER);
        self.pack_box.add_child(&head);
        self.pack_box.add_child(&separator());
        let mut grid_box = GridContainer::new_alloc();
        grid_box.set_columns(2);
        grid_box.add_theme_constant_override("h_separation", 10);
        grid_box.add_theme_constant_override("v_separation", 6);
        for (f, n) in counts {
            let mut b = Button::new_alloc();
            b.set_focus_mode(FocusMode::NONE);
            b.set_custom_minimum_size(Vector2::new(260.0, 38.0));
            b.set_h_size_flags(SizeFlags::EXPAND_FILL);
            for st in ["normal", "disabled"] {
                b.add_theme_stylebox_override(st, &self.looks.tab);
            }
            let mut hover = self.looks.tab.duplicate_resource();
            hover.set_border_color(theme::GOLD);
            b.add_theme_stylebox_override("hover", &hover);
            b.add_theme_stylebox_override("pressed", &self.looks.tab_on);
            b.set_tooltip_text(&tr!(
                "inv-show-only",
                what = label(f.label_key()).to_lowercase()
            ));
            let mut row = HBoxContainer::new_alloc();
            row.set_mouse_filter(MouseFilter::IGNORE);
            row.add_theme_constant_override("separation", 8);
            place(&row, [0.0, 0.0, 1.0, 1.0], [8.0, 0.0, -10.0, 0.0]);
            let mut glyph = TextureRect::new_alloc();
            glyph.set_mouse_filter(MouseFilter::IGNORE);
            glyph.set_expand_mode(ExpandMode::IGNORE_SIZE);
            glyph.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            glyph.set_custom_minimum_size(Vector2::new(26.0, 26.0));
            glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            glyph.set_texture(&icons::glyph_icon(filter_glyph(f)));
            glyph.set_modulate(theme::GOLD);
            row.add_child(&glyph);
            let mut name = theme::styled_label(&label(f.label_key()), Face::Body, 16, theme::TEXT);
            name.set_h_size_flags(SizeFlags::EXPAND_FILL);
            name.set_vertical_alignment(VerticalAlignment::CENTER);
            row.add_child(&name);
            let mut count =
                theme::styled_label(&n.to_string(), Face::BodyBold, 16, theme::GOLD_BRIGHT);
            count.set_vertical_alignment(VerticalAlignment::CENTER);
            row.add_child(&count);
            b.add_child(&row);
            let q = self.queue.clone();
            b.signals()
                .pressed()
                .connect(move || push(&q, UiEvent::Inventory(InvInput::Filter(f))));
            grid_box.add_child(&b);
        }
        self.pack_box.add_child(&grid_box);
    }

    /// The detail column: the selected item's icon, name, facts, actions.
    fn draw_detail(&mut self) {
        let item = match &self.mode {
            Mode::Browse => self.selected.and_then(|l| self.pack.by_letter(l)).cloned(),
            // the item under the keyboard, else the first one suggested
            _ => match self.focused() {
                Some(InvTarget::Cell(l)) => self.pack.by_letter(l).cloned(),
                _ => self.cells_shown.iter().find_map(|t| match t {
                    InvTarget::Cell(l) => {
                        let on = self.question().is_none_or(|q| q.all || q.suggests(*l));
                        on.then(|| self.pack.by_letter(*l).cloned()).flatten()
                    }
                    _ => None,
                }),
            },
        };
        let key = (item.clone(), self.mode == Mode::Browse);
        if self.detail_key.as_ref() == Some(&key) {
            return;
        }
        self.detail_key = Some(key);
        for mut c in self.detail.get_children().iter_shared() {
            c.queue_free();
        }
        self.detail_empty.set_visible(item.is_none());
        let Some(item) = item else {
            return;
        };
        let mut icon_box = PanelContainer::new_alloc();
        icon_box.set_mouse_filter(MouseFilter::IGNORE);
        icon_box.add_theme_stylebox_override("panel", &self.looks.equipped);
        icon_box.set_custom_minimum_size(Vector2::new(136.0, 136.0));
        icon_box.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let mut icon = TextureRect::new_alloc();
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_texture(&icons::item_icon(item.tile, item.class));
        icon.set_custom_minimum_size(Vector2::new(128.0, 128.0));
        icon_box.add_child(&icon);
        self.detail.add_child(&icon_box);
        let title = i18n::engine(EngineKind::Name, &title_of(&item)).into_owned();
        let mut name = theme::styled_label(&title, Face::Title, 23, theme::GOLD_BRIGHT);
        name.set_autowrap_mode(AutowrapMode::WORD_SMART);
        name.set_horizontal_alignment(HorizontalAlignment::CENTER);
        self.detail.add_child(&name);
        let mut class_line = tr!(
            "inv-class-line",
            class = class_name(item.class),
            letter = item.letter
        );
        if item.quan > 1 {
            class_line.push_str(&format!(" · ×{}", item.quan));
        }
        let mut cl = theme::styled_label(&class_line, Face::Body, 15, theme::TEXT_DIM);
        cl.set_horizontal_alignment(HorizontalAlignment::CENTER);
        self.detail.add_child(&cl);
        self.detail.add_child(&separator());
        for f in facts(&item) {
            let mut l = theme::styled_label(
                &format!("◆  {}", f.text),
                Face::Body,
                16,
                f.color.unwrap_or(theme::TEXT),
            );
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            self.detail.add_child(&l);
        }
        self.detail.add_child(&separator());
        if self.mode == Mode::Browse {
            let actions = actions_for(&item, &self.pack);
            let mut rest = actions.iter();
            if let Some(first) = actions.iter().find(|a| a.default) {
                let b = self.action_row(item.letter, first, 320.0, 40.0, true);
                self.detail.add_child(&b);
            }
            let mut grid_box = GridContainer::new_alloc();
            grid_box.set_columns(2);
            grid_box.add_theme_constant_override("h_separation", 6);
            grid_box.add_theme_constant_override("v_separation", 2);
            let mut n = 0;
            for a in rest.by_ref() {
                if a.default || n >= 8 {
                    continue;
                }
                let b = self.action_row(item.letter, a, 157.0, 30.0, false);
                grid_box.add_child(&b);
                n += 1;
            }
            self.detail.add_child(&grid_box);
            self.detail.add_child(&separator());
        }
        let mut raw = theme::styled_label(
            &tr!(
                "inv-raw",
                text = i18n::engine(EngineKind::Name, &item.text).into_owned()
            ),
            Face::Body,
            14,
            theme::TEXT_OFF,
        );
        raw.set_autowrap_mode(AutowrapMode::WORD_SMART);
        self.detail.add_child(&raw);
    }

    /// Hide everything (the game ends).
    pub fn reset(&mut self) {
        self.close();
        self.pack = Pack::new();
        self.selected = None;
        self.search.clear();
        self.search_edit.set_text("");
        self.shade.set_visible(true);
        self.dirty = true;
        self.sync();
    }
}

/// Where an arrow goes from a doll socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DollStep {
    To(usize),
    /// Back to the grid, at this row.
    Grid(usize),
    Stay,
}

/// The doll's sockets as the keyboard walks them: the left column (0–5),
/// the right column (6–11), the weapons row (12–15) under both.
fn doll_step(i: usize, k: Key) -> DollStep {
    match (i, k) {
        (0..=5, Key::Up) => DollStep::To(i.saturating_sub(1)),
        (0..=4, Key::Down) | (6..=10, Key::Down) => DollStep::To(i + 1),
        (5, Key::Down) => DollStep::To(12),
        (11, Key::Down) => DollStep::To(15),
        (0..=5, Key::Right) => DollStep::To(i + 6),
        (6..=11, Key::Left) => DollStep::To(i - 6),
        (6..=11, Key::Up) => DollStep::To(if i == 6 { 6 } else { i - 1 }),
        (6..=11, Key::Right) => DollStep::Grid(i - 6),
        (12..=15, Key::Left) => DollStep::To(if i == 12 { 12 } else { i - 1 }),
        (12..=14, Key::Right) => DollStep::To(i + 1),
        (15, Key::Right) => DollStep::Grid(0),
        (12 | 13, Key::Up) => DollStep::To(5),
        (14 | 15, Key::Up) => DollStep::To(11),
        _ => DollStep::Stay,
    }
}

/// The '-' cell's label by the question's verb (ui-design §3).
pub fn hands_label(verb: &str) -> String {
    match verb {
        "wield" => tr!("hands-bare"),
        v if v.starts_with("write") || v.starts_with("engrave") => tr!("hands-fingers"),
        v if v.starts_with("ready") => tr!("hands-empty-quiver"),
        _ => tr!("hands-nothing"),
    }
}

/// A filter tab's tooltip: its name and its classes' symbols.
fn filter_tip(f: InvFilter) -> String {
    let classes = f.classes();
    if classes.is_empty() {
        label(f.label_key())
    } else {
        format!("{}  {}", label(f.label_key()), classes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(letter: char, class: char, text: &str, slots: &[Slot]) -> InvItem {
        InvItem {
            letter,
            class,
            tile: 1,
            quan: 1,
            slots: slots.to_vec(),
            lit: false,
            text: text.into(),
        }
    }

    /// The engine's translator in this test's thread, loaded; Russian.
    fn russian() {
        use crate::i18n::EngineText;
        let t = crate::engine_text::EngineTranslator::new();
        let start = std::time::Instant::now();
        while t.translate(Lang::Ru, EngineKind::Name, "newt").is_none() {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(120),
                "the translator does not load"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        i18n::set_engine_text(Some(Box::new(t)));
        i18n::set_lang(Lang::Ru);
    }

    #[test]
    fn the_detail_is_russian_and_agrees_with_the_item() {
        russian();
        let facts_of = |text: &str, quan: i64| -> Vec<String> {
            let it = InvItem {
                quan,
                ..item('a', ')', text, &[])
            };
            facts(&it).into_iter().map(|f| f.text).collect()
        };
        let whip = facts_of("a +2 bullwhip (weapon in right hand)", 1);
        let fedora = facts_of("an uncursed +0 fedora (being worn)", 1);
        let mail = facts_of("an uncursed ring mail (being worn)", 1);
        let ring = facts_of("a cursed ring of protection (on left hand)", 1);
        let daggers = facts_of("2 cursed -1 very rusty daggers (in quiver pouch)", 2);
        let gloves = facts_of(
            "a blessed greased +1 pair of leather gloves (being worn; slippery)",
            1,
        );
        let sack = facts_of("an uncursed sack containing 3 items", 1);
        i18n::set_lang(Lang::En);
        i18n::set_engine_text(None);
        // the usage as the translator says it with the name: no English left
        let latin = |t: &str| t.chars().any(|c| c.is_ascii_alphabetic());
        for f in [&whip, &fedora, &mail, &ring, &daggers, &gloves, &sack] {
            assert!(!f.iter().any(|t| latin(t)), "{f:?}");
        }
        // a hat is worn as a hat is (надета), a mail as a mail (надет)
        assert_ne!(fedora[0], mail[0]);
        // the curse status in the item's gender and number
        assert_eq!(fedora[1], "Не проклята");
        assert_eq!(mail[1], "Не проклят");
        assert_eq!(ring[1], "Проклято");
        assert_eq!(daggers[1], "Прокляты");
        assert_eq!(gloves[1], "Благословлены");
        // and the state words
        assert_eq!(daggers[3], "Очень ржавые");
        assert_eq!(gloves[3], "Смазанные");
        assert_eq!(sack, ["Не проклят", "Внутри 3 предмета"]);
    }

    #[test]
    fn the_detail_says_only_what_the_name_says() {
        let spear = item(
            'a',
            ')',
            "a blessed +1 spear (weapon in right hand)",
            &[Slot::Weapon],
        );
        let f: Vec<String> = facts(&spear).into_iter().map(|f| f.text).collect();
        assert_eq!(f, ["Weapon in right hand", "Blessed", "Enchantment +1"]);
        assert_eq!(title_of(&spear), "blessed +1 spear");
        let wand = item('f', '/', "a wand of striking (0:5)", &[]);
        let f: Vec<String> = facts(&wand).into_iter().map(|f| f.text).collect();
        assert_eq!(f, ["Recharged 0 times · 5 charges left"]);
        let mail = item(
            'c',
            '[',
            "an uncursed very rusty +0 ring mail (being worn)",
            &[Slot::Body],
        );
        let f: Vec<String> = facts(&mail).into_iter().map(|f| f.text).collect();
        assert_eq!(
            f,
            ["Being worn", "Uncursed", "Enchantment +0", "Very rusty"]
        );
        let potion = item(
            'g',
            '!',
            "2 cursed potions called fizzy (unpaid, 10 zorkmids)",
            &[],
        );
        let f: Vec<String> = facts(&potion).into_iter().map(|f| f.text).collect();
        assert_eq!(
            f,
            [
                "Unpaid, 10 zorkmids",
                "Cursed",
                "You called this type: fizzy"
            ]
        );
        assert_eq!(title_of(&potion), "cursed potions called fizzy");
        let food = item('d', '%', "a partly eaten food ration", &[]);
        let f: Vec<String> = facts(&food).into_iter().map(|f| f.text).collect();
        assert_eq!(f, ["Partly eaten"]);
        let named = item('b', ')', "a dagger named Sting", &[]);
        let f: Vec<String> = facts(&named).into_iter().map(|f| f.text).collect();
        assert_eq!(f, ["Name: Sting"]);
        // nothing ever mentions weight
        for i in [spear, wand, mail, potion, food, named] {
            assert!(
                facts(&i)
                    .iter()
                    .all(|f| !f.text.to_lowercase().contains("weight"))
            );
        }
    }

    #[test]
    fn the_keyboard_walks_the_doll() {
        assert_eq!(doll_step(0, Key::Up), DollStep::To(0));
        assert_eq!(doll_step(5, Key::Down), DollStep::To(12));
        assert_eq!(doll_step(2, Key::Right), DollStep::To(8));
        assert_eq!(doll_step(8, Key::Right), DollStep::Grid(2));
        assert_eq!(doll_step(15, Key::Up), DollStep::To(11));
        assert_eq!(doll_step(12, Key::Left), DollStep::To(12));
        assert_eq!(doll_step(0, Key::Left), DollStep::Stay);
    }

    #[test]
    fn labels_read_as_words() {
        assert_eq!(label("item.wield"), "Wield");
        assert_eq!(label("cmd.search"), "Search");
        assert_eq!(label("cmd.pick_up"), "Pick up");
        assert_eq!(label("inv.scrolls_and_books"), "Scrolls and books");
        assert_eq!(hands_label("wield"), "Bare hands");
        assert_eq!(hands_label("write with"), "Fingers");
        assert_eq!(hands_label("ready"), "Nothing (empty the quiver)");
    }

    #[test]
    fn the_doll_shows_the_slots_and_what_goes_where() {
        let mut pack = Pack::new();
        pack.replace(&nh_protocol::Inventory {
            items: vec![
                item(
                    'a',
                    ')',
                    "a +1 long sword (weapon in hand)",
                    &[Slot::Weapon],
                ),
                item(
                    'b',
                    ')',
                    "a +0 dagger (alternate weapon; not wielded)",
                    &[Slot::Alternate],
                ),
                item('c', '[', "a +3 small shield (being worn)", &[Slot::Shield]),
                item('d', '[', "a pair of hiking boots", &[]),
            ],
            twoweap: false,
        });
        let letters = |s: DollSlot| s.items(&pack).iter().map(|i| i.letter).collect::<String>();
        assert_eq!(letters(DollSlot::Main), "a");
        assert_eq!(letters(DollSlot::Alternate), "b");
        assert_eq!(letters(DollSlot::Off), "c");
        assert_eq!(letters(DollSlot::Boots), "");
        let boots = pack.by_letter('d').unwrap();
        assert!(DollSlot::Boots.accepts(boots));
        assert!(!DollSlot::Helmet.accepts(boots));
        assert_eq!(DollSlot::Boots.equip(boots), Some(ItemActionKind::Wear));
        let dagger = pack.by_letter('b').unwrap();
        assert_eq!(DollSlot::Main.equip(dagger), Some(ItemActionKind::Wield));
        assert_eq!(DollSlot::Alternate.unequip(dagger), None);
        let sword = pack.by_letter('a').unwrap();
        assert_eq!(
            DollSlot::Alternate.equip(sword),
            Some(ItemActionKind::SwapWeapons)
        );
        assert_eq!(DollSlot::Main.unequip(sword), Some(ItemActionKind::Unwield));
        assert_eq!(armor_part("conical hat"), DollSlot::Helmet);
        assert_eq!(armor_part("tattered cape"), DollSlot::Cloak);
        assert_eq!(armor_part("crystal plate mail"), DollSlot::Body);
        assert_eq!(armor_part("large round shield"), DollSlot::Off);
    }
}
