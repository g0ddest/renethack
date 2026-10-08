//! The play screen's HUD (ui-design §1): the portrait block, the HP and Pw
//! orbs, the XP bar, the action bar and its micro-buttons, the message
//! log, the minimap with the mode badge and the order line, the prompt
//! banner, the hover tooltip and the full log.
//!
//! Only the HUD's blocks take the mouse (`MOUSE_FILTER_STOP`); everything
//! else lets it through to the map.

use std::collections::VecDeque;
use std::f64::consts::TAU;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, GrowDirection, MouseFilter, SizeFlags};
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, CanvasLayer, ColorRect, Control, HBoxContainer, HFlowContainer, InputEvent,
    InputEventMouseButton, Label, PanelContainer, Polygon2D, RichTextLabel, ShaderMaterial,
    StyleBoxFlat, Time, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::{Key, KeyInput, Message, Mods, Status, World};

use crate::action_bar::{self, ActionBar};
use crate::i18n::{self, EngineKind};
use crate::minimap::{self, Minimap};
use crate::orb::{self, Orb};
use crate::theme::{self, Face, Frame, bbcode_escape, hex, hex_alpha, place};
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue, push};

/// Messages kept in the log panel; the wheel scrolls through them (the
/// full log has the rest).
const LOG_LINES: usize = 200;
/// The screen edge the blocks keep, in design pixels.
const EDGE: f32 = 24.0;
const PORTRAIT_W: f32 = 520.0;
const PORTRAIT_H: f32 = 176.0;
const LOG_W: f32 = 404.0;
const LOG_H: f32 = 260.0;
/// The docked log of the compact layout: on the XP bar, from the bar's
/// left end, clear of the gamepad's hints on the right; this many lines.
const LOG_COMPACT_W: f32 = 436.0;
const LOG_COMPACT_LINES: f32 = 4.0;
/// The micro-buttons over the bar's right end: how many, how wide, the gap
/// (the docked log keeps clear of them).
const MICRO_BUTTONS: usize = 8;
const MICRO_W: f32 = 32.0;
const MICRO_GAP: i32 = 4;
/// The log's text: Alegreya Sans 16 and 2 px between lines.
const LOG_FONT: i32 = 16;
const LOG_LINE_GAP: i32 = 2;
/// The orbs beside the bar's frame, never over its slots: 148 + 4 + 736
/// + 4 + 148.
const CLUSTER_W: f32 = 1040.0;
const CLUSTER_H: f32 = 176.0;
const CLUSTER_BOTTOM: f32 = 12.0;
/// The action bar's frame: the slots plus 12 of padding each side.
const BAR_W: f32 = action_bar::WIDTH + 24.0;
const BAR_H: f32 = action_bar::SLOT + 24.0;
const MINIMAP_W: f32 = 424.0;
const MINIMAP_H: f32 = 150.0;
const MODE_W: f32 = 240.0;
/// Seconds before a log line fades to `LOG_FADED` (the compact log sooner).
const LOG_FADE_SECS: f64 = 10.0;
const LOG_FADE_COMPACT_SECS: f64 = 6.0;
const LOG_FADED: f32 = 0.45;
/// Tooltip offset from the mouse.
const TOOLTIP_GAP: f32 = 18.0;
/// How long a toast stays (ui-design §4.2: "Slot 4 cleared · Undo").
const TOAST_SECS: f64 = 4.0;
/// A line of the full log, and its title, rule and padding.
const FULL_LOG_LINE: f32 = 24.0;
const FULL_LOG_CHROME: f32 = 120.0;
/// The mode ribbon, from the top.
const FLASH_Y: f32 = 76.0;
/// Deadly chips pulse this many times a second (a 1.2 s cycle).
const PULSE_HZ: f64 = 1.0 / 1.2;
/// Seconds the banner of a new mode stays, fading out at the end.
const FLASH_SECS: f64 = 1.6;
/// The fade: short, a half-faded plaque over a lit wall reads as a smudge.
const FLASH_FADE: f64 = 0.25;
/// Seconds a changed attribute stays green or red.
const ATTR_FLASH_SECS: f64 = 1.5;

const COMBAT_TEXT: Color = Color::from_rgb(1.0, 0.42, 0.32);
const EXPLORE_TEXT: Color = theme::GOLD;
/// The mode badge's frame is tinted red in combat.
const COMBAT_TINT: Color = Color::from_rgb(1.0, 0.62, 0.55);

// ---- what the HUD shows (plain data, tested without Godot) ----

/// How alarming a chip is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Kills unless cured: red and pulsing.
    Deadly,
    /// Dangerous now.
    Bad,
    /// Impairs the hero.
    Warn,
    /// Worth knowing.
    Info,
}

/// Conditions that kill unless cured (botl.c names; "Strangled" in case a
/// catalog spells it out).
const DEADLY: [&str; 6] = [
    "Stone",
    "Slime",
    "Strngl",
    "Strangled",
    "FoodPois",
    "TermIll",
];
const BAD: [&str; 5] = ["InLava", "Grab", "Parlyz", "Out", "Zzz"];
const IMPAIRED: [&str; 12] = [
    "Blind", "Conf", "Stun", "Hallu", "Deaf", "Held", "Trap", "WLegs", "Iron", "Submrg", "Teth",
    "Slip",
];

pub fn condition_tone(name: &str) -> Tone {
    if DEADLY.contains(&name) {
        Tone::Deadly
    } else if BAD.contains(&name) {
        Tone::Bad
    } else if IMPAIRED.contains(&name) {
        Tone::Warn
    } else {
        Tone::Info
    }
}

/// The words of the deadly banner for a deadly condition.
pub fn deadly_words(name: &str) -> Option<String> {
    Some(match name {
        "Stone" => tr!("deadly-stone"),
        "Slime" => tr!("deadly-slime"),
        "Strngl" | "Strangled" => tr!("deadly-strangled"),
        "FoodPois" => tr!("deadly-food-poisoning"),
        "TermIll" => tr!("deadly-terminally-ill"),
        _ => return None,
    })
}

/// An attribute's tag on the portrait (St, Dx...).
fn attr_tag(tag: &str) -> String {
    match tag {
        "St" => tr!("attr-st"),
        "Dx" => tr!("attr-dx"),
        "Co" => tr!("attr-co"),
        "In" => tr!("attr-in"),
        "Wi" => tr!("attr-wi"),
        "Ch" => tr!("attr-ch"),
        other => other.to_string(),
    }
}

/// Fainting leads straight to starving: as deadly as stoning.
fn hunger_tone(hunger: &str) -> Tone {
    match hunger {
        "Satiated" | "Hungry" => Tone::Warn,
        "Fainting" | "Fainted" | "Starved" => Tone::Deadly,
        _ => Tone::Bad,
    }
}

fn encumbrance_tone(cap: &str) -> Tone {
    match cap {
        "Burdened" => Tone::Warn,
        _ => Tone::Bad,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub text: String,
    pub tone: Tone,
}

/// Where the hero is, as the player reads it: "Dlvl 1" and "Tutorial 1"
/// in the interface's words, the engine's names of other places ("Home 2",
/// "Fort Ludios", "Astral") through its translator.
fn place_text(view: &StatusView) -> Option<String> {
    let (tag, v) = view.fields.first()?;
    Some(match tag.as_str() {
        "$" | "AC" | "XL" | "Exp" | "HD" | "T" | "S" => return None,
        "Dlvl" => tr!("hud-dlvl", level = v.as_str()),
        "Tutorial" => tr!("hud-tutorial-level", level = v.as_str()),
        _ => i18n::engine(EngineKind::Status, &view.place()?).into_owned(),
    })
}

/// The status as the portrait's tooltip says it in full: name and title,
/// alignment; place, gold, armour, level, experience, turn, score; hit
/// points and power; the attributes; hunger, encumbrance, conditions.
fn status_tip(view: &StatusView) -> String {
    let mut head = i18n::engine(EngineKind::Status, &view.title).into_owned();
    if let Some(a) = &view.align {
        head.push_str("  ·  ");
        head.push_str(&i18n::engine_hero_word(a));
    }
    let mut facts: Vec<String> = place_text(view).into_iter().collect();
    for (tag, v) in &view.fields {
        let v = v.as_str();
        facts.extend(match tag.as_str() {
            "$" => Some(tr!("hud-gold-tip", gold = v)),
            "AC" => Some(tr!("hud-ac-tip", ac = v)),
            "XL" => Some(tr!("hud-level", level = v)),
            "HD" => Some(tr!("hud-hit-dice", hd = v)),
            "Exp" => Some(tr!("hud-exp", exp = v)),
            "T" => Some(tr!("hud-turn", turn = v)),
            "S" => Some(tr!("hud-score", score = v)),
            _ => None,
        });
    }
    let bars: Vec<String> = [("orb-hp", view.hp), ("orb-pw", view.pw)]
        .into_iter()
        .filter_map(|(key, p)| {
            p.map(|(value, most)| {
                let mut args = i18n::FluentArgs::new();
                args.set("value", i18n::Arg::fluent(value));
                args.set("most", i18n::Arg::fluent(most));
                i18n::tr_with(key, Some(&args))
            })
        })
        .collect();
    let attrs: Vec<String> = view
        .attrs
        .iter()
        .map(|(tag, v)| format!("{} {v}", attr_tag(tag)))
        .collect();
    let chips: Vec<String> = view
        .chips()
        .map(|c| i18n::engine(EngineKind::Status, &c.text).into_owned())
        .collect();
    [
        head,
        facts.join("  ·  "),
        bars.join("  ·  "),
        attrs.join("  "),
        chips.join("  "),
    ]
    .into_iter()
    .filter(|l| !l.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

/// What the status shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusView {
    /// Name and rank title ("Hero the Stripling").
    pub title: String,
    pub align: Option<String>,
    /// (tag, value): Dlvl, $, AC, XL and Exp (or HD when polymorphed), T, S.
    /// An empty tag shows the value alone ("Home 2").
    pub fields: Vec<(String, String)>,
    pub hp: Option<(i64, i64)>,
    pub pw: Option<(i64, i64)>,
    /// St Dx Co In Wi Ch.
    pub attrs: Vec<(&'static str, String)>,
    /// Hunger and encumbrance, when not the usual.
    pub needs: Vec<Chip>,
    /// The conditions in catalog order.
    pub conditions: Vec<Chip>,
}

impl StatusView {
    /// `conditions`: names of the conditions in effect (`Status::condition_names`).
    pub fn new(status: &Status, conditions: &[String]) -> StatusView {
        let text = |f: &str| status.get(f).filter(|v| !v.is_empty()).map(str::to_string);
        let pair = |a: &str, b: &str| Some((status.number(a)?, status.number(b)?));
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut field = |tag: &str, value: String| fields.push((tag.to_string(), value));
        if let Some(d) = text("leveldesc") {
            match d.split_once(':') {
                Some((tag, v)) => field(tag.trim(), v.trim().to_string()),
                None => field("", d),
            }
        }
        if let Some(g) = text("gold") {
            let n = status.number("gold");
            field(
                "$",
                n.map_or_else(|| g.trim_matches(':').to_string(), |n| n.to_string()),
            );
        }
        if let Some(ac) = text("ac") {
            field("AC", ac);
        }
        // a polymorphed hero has hit dice instead of an experience level
        match (text("hitdice"), text("xlevel")) {
            (Some(hd), None) => field("HD", hd),
            (_, Some(xl)) => {
                field("XL", xl);
                if let Some(e) = text("exp") {
                    field("Exp", e);
                }
            }
            (None, None) => {}
        }
        if let Some(t) = text("time") {
            field("T", t);
        }
        if let Some(s) = text("score") {
            field("S", s);
        }
        let attrs = [
            ("St", "str"),
            ("Dx", "dex"),
            ("Co", "con"),
            ("In", "int"),
            ("Wi", "wis"),
            ("Ch", "cha"),
        ]
        .into_iter()
        .filter_map(|(tag, f)| text(f).map(|v| (tag, v)))
        .collect();
        let mut needs = Vec::new();
        if let Some(h) = text("hunger") {
            let tone = hunger_tone(&h);
            needs.push(Chip { text: h, tone });
        }
        if let Some(c) = text("cap") {
            let tone = encumbrance_tone(&c);
            needs.push(Chip { text: c, tone });
        }
        let conditions = conditions
            .iter()
            .map(|c| Chip {
                text: c.clone(),
                tone: condition_tone(c),
            })
            .collect();
        StatusView {
            title: text("title").unwrap_or_default(),
            align: text("align"),
            fields,
            hp: pair("hp", "hpmax"),
            pw: pair("energy", "energymax"),
            attrs,
            needs,
            conditions,
        }
    }

    /// The value of the field tagged `tag`.
    pub fn field(&self, tag: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, v)| v.as_str())
    }

    /// Where the hero is ("Dlvl 1", "Home 2"): the first field.
    pub fn place(&self) -> Option<String> {
        let (tag, v) = self.fields.first()?;
        match tag.as_str() {
            "$" | "AC" | "XL" | "Exp" | "HD" | "T" | "S" => None,
            "" => Some(v.clone()),
            _ => Some(format!("{tag} {v}")),
        }
    }

    /// Hunger, encumbrance, then the conditions.
    pub fn chips(&self) -> impl Iterator<Item = &Chip> {
        self.needs.iter().chain(&self.conditions)
    }

    /// The status as the classic bottom lines ("Dlvl:1  $0  AC:6",
    /// "HP:16(16)  Pw:2(2)"...): the block's tooltip, and self-tests.
    pub fn lines(&self) -> Vec<String> {
        let mut head = self.title.clone();
        if let Some(a) = &self.align {
            head.push_str(&format!("  {a}"));
        }
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|(tag, v)| match tag.as_str() {
                "" => v.clone(),
                "$" => format!("${v}"),
                _ => format!("{tag}:{v}"),
            })
            .collect();
        let bars: Vec<String> = [("HP", self.hp), ("Pw", self.pw)]
            .into_iter()
            .filter_map(|(tag, p)| p.map(|(v, m)| format!("{tag}:{v}({m})")))
            .collect();
        let attrs: Vec<String> = self
            .attrs
            .iter()
            .map(|(tag, v)| format!("{tag}:{v}"))
            .collect();
        let mut lines = vec![head, fields.join("  "), bars.join("  "), attrs.join(" ")];
        let chips: Vec<&str> = self.chips().map(|c| c.text.as_str()).collect();
        if !chips.is_empty() {
            lines.push(chips.join(" "));
        }
        lines
    }
}

/// Experience points needed for level `lev + 1` (exper.c `newuexp`: rule
/// knowledge, the same for every character).
pub fn newuexp(lev: i64) -> i64 {
    if lev < 1 {
        0
    } else if lev < 10 {
        10 * (1 << lev)
    } else if lev < 20 {
        10_000 * (1 << (lev - 10))
    } else {
        10_000_000 * (lev - 19)
    }
}

/// How far the hero is from level `lev` to the next: (share, next level's
/// threshold).
pub fn xp_progress(lev: i64, exp: i64) -> (f32, i64) {
    let (from, to) = (newuexp(lev - 1), newuexp(lev));
    let share = if to > from {
        ((exp - from) as f64 / (to - from) as f64).clamp(0.0, 1.0) as f32
    } else {
        1.0
    };
    (share, to)
}

/// pray.c `critically_low_hp(FALSE)`: the engine's own "low HP" (what
/// prayer fixes), from the shown HP and the experience level.
pub fn critically_low_hp(hp: i64, max: i64, lev: i64) -> bool {
    let max = max.min(15 * lev);
    let rank = if lev <= 2 {
        0
    } else if lev <= 30 {
        (lev + 2) / 4
    } else {
        8
    };
    let divisor = match rank {
        0 | 1 => 5,
        2 | 3 => 6,
        4 | 5 => 7,
        6 | 7 => 8,
        _ => 9,
    };
    hp <= 5 || hp * divisor <= max
}

/// An attribute's value for comparing ("18/50" above "18", "18/**" above
/// all of them).
pub fn attr_rank(v: &str) -> Option<f32> {
    match v.split_once('/') {
        Some((base, "**")) => Some(base.trim().parse::<f32>().ok()? + 1.0),
        Some((base, pct)) => {
            Some(base.trim().parse::<f32>().ok()? + pct.parse::<f32>().ok()? / 100.0)
        }
        None => v.trim().parse().ok(),
    }
}

/// Where the threat arrow goes for a hostile at screen point `at` in a
/// view of `view`: None while it is in the frame (away from the HUD's
/// bottom cluster), else a point on the inner edge toward it and the
/// angle it points at.
pub fn edge_arrow(at: Option<Vector2>, view: Vector2) -> Option<(Vector2, f32)> {
    let at = at?;
    // the frame the player watches: the edges less a margin, above the bar
    let (left, top) = (ARROW_MARGIN, ARROW_MARGIN);
    let (right, bottom) = (
        view.x - ARROW_MARGIN,
        view.y - CLUSTER_BOTTOM - CLUSTER_H - ARROW_MARGIN,
    );
    if (left..=right).contains(&at.x) && (top..=bottom).contains(&at.y) {
        return None;
    }
    let c = Vector2::new(view.x * 0.5, (top + bottom) * 0.5);
    let d = at - c;
    if d.length() < 1.0 {
        return None;
    }
    // scale the way from the centre so it meets the inner rectangle
    let tx = if d.x.abs() > 1e-3 {
        (if d.x > 0.0 { right - c.x } else { left - c.x }) / d.x
    } else {
        f32::MAX
    };
    let ty = if d.y.abs() > 1e-3 {
        (if d.y > 0.0 { bottom - c.y } else { top - c.y }) / d.y
    } else {
        f32::MAX
    };
    let t = tx.min(ty);
    Some((c + d * t, d.y.atan2(d.x)))
}

/// The arrow keeps this far from the screen's edges.
const ARROW_MARGIN: f32 = 48.0;

/// And its middle this far from a panel of the HUD: half its length and a
/// little air.
const ARROW_PAD: f32 = 28.0;

/// The arrow at `p` off the HUD's own blocks (the portrait, the minimap
/// and its badge, the log): a point one of `panels` covers leaves it by
/// its nearest side that keeps the arrow in the frame, and points at
/// `target` from there.
pub fn off_panels(
    p: Vector2,
    angle: f32,
    target: Vector2,
    view: Vector2,
    panels: &[Rect2],
) -> (Vector2, f32) {
    let (left, top) = (ARROW_MARGIN, ARROW_MARGIN);
    let (right, bottom) = (
        view.x - ARROW_MARGIN,
        view.y - CLUSTER_BOTTOM - CLUSTER_H - ARROW_MARGIN,
    );
    let in_frame = |q: &Vector2| (left..=right).contains(&q.x) && (top..=bottom).contains(&q.y);
    let mut at = p;
    // the way out of one may end on its neighbour (the badge under the
    // minimap)
    for _ in 0..4 {
        let Some(r) = panels
            .iter()
            .map(|r| r.grow(ARROW_PAD))
            .find(|r| r.contains_point(at))
        else {
            break;
        };
        let outs = [
            Vector2::new(r.position.x - 1.0, at.y),
            Vector2::new(r.end().x + 1.0, at.y),
            Vector2::new(at.x, r.position.y - 1.0),
            Vector2::new(at.x, r.end().y + 1.0),
        ];
        let nearest = outs
            .into_iter()
            .filter(in_frame)
            .min_by(|a, b| a.distance_to(at).total_cmp(&b.distance_to(at)));
        let Some(out) = nearest else {
            break;
        };
        at = out;
    }
    if at == p {
        return (p, angle);
    }
    let d = target - at;
    if d.length() > ARROW_PAD {
        (at, d.y.atan2(d.x))
    } else {
        (at, angle)
    }
}

/// The arrow over a hostile's head at `p` off the hero's own body (`hero`,
/// on screen): a hostile in the cell south of the hero stands before them,
/// and what is over its head is the hero's waist. The arrow then goes
/// beside the hero, level with where it was, and points at `target` from
/// there. `side` is the side it went to last (true: the right), kept
/// while it stays beside them, so that it does not jump over as the
/// hostile sways; None again once it is clear.
pub fn off_hero(
    p: Vector2,
    angle: f32,
    target: Vector2,
    view: Vector2,
    hero: Option<Rect2>,
    side: &mut Option<bool>,
) -> (Vector2, f32) {
    let Some(body) = hero
        .map(|r| r.grow(ARROW_PAD))
        .filter(|r| r.contains_point(p))
    else {
        *side = None;
        return (p, angle);
    };
    let x = |right: bool| {
        if right {
            body.end().x + 1.0
        } else {
            body.position.x - 1.0
        }
    };
    let in_frame = |x: f32| (ARROW_MARGIN..=view.x - ARROW_MARGIN).contains(&x);
    // the side the hostile leans to, if the frame has room there
    let wish = side.unwrap_or(target.x >= body.center().x);
    let right = if in_frame(x(wish)) { wish } else { !wish };
    *side = Some(right);
    let at = Vector2::new(x(right), p.y);
    let d = target - at;
    (at, d.y.atan2(d.x))
}

/// The docked log's bottom-left corner (from the bottom centre): the action
/// bar's left end, on the XP bar like the micro-buttons.
fn compact_log_corner() -> (f32, f32) {
    let bar_left = -CLUSTER_W / 2.0 + (CLUSTER_W - BAR_W) / 2.0;
    let xp_top = -CLUSTER_BOTTOM - BAR_H - 6.0 - 22.0;
    (bar_left, xp_top - 6.0)
}

/// The log frame's margin (Frame::Hud).
const LOG_MARGIN: f32 = 12.0;

/// The compact layout (ui-design §1.6): the log docks over the cluster
/// when there is no room for it beside the cluster.
pub fn compact(virtual_width: f32) -> bool {
    (virtual_width - CLUSTER_W) / 2.0 - 40.0 < 380.0
}

/// "valkyrie" → "Valkyrie", "wood nymph" → "Wood Nymph".
fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How a message looks in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// Arrived after the player's last input (and not restored from a save).
    pub new: bool,
    pub urgent: bool,
}

pub fn message_look(m: &Message, input_seq: u64) -> Look {
    Look {
        new: m.seq > input_seq && !m.from_history,
        urgent: m.urgent,
    }
}

/// A message as BBCode: new ones bright, old ones dimmed, urgent ones in
/// the warning colour (and bold while new); `faded` ones at 45 %.
fn message_bbcode(m: &Message, input_seq: u64, faded: bool) -> String {
    let look = message_look(m, input_seq);
    // the engine's English is kept; the log draws it in the language now
    let text = bbcode_escape(&i18n::engine_message(m));
    let color = match (look.urgent, look.new) {
        (true, true) => theme::DANGER,
        (true, false) => theme::DANGER.lerp(theme::TEXT_DIM, 0.45),
        (false, true) => theme::TEXT,
        (false, false) => theme::TEXT_DIM,
    };
    let color = if faded {
        Color {
            a: LOG_FADED,
            ..color
        }
    } else {
        color
    };
    if look.new {
        format!("[color={}][b]{text}[/b][/color]", hex_alpha(color))
    } else {
        format!("[color={}]{text}[/color]", hex_alpha(color))
    }
}

// ---- Godot nodes ----

/// (background, border, text) of a chip.
fn chip_colors(tone: Tone) -> (Color, Color, Color) {
    match tone {
        Tone::Deadly => (
            Color::from_rgba(0.42, 0.05, 0.04, 0.95),
            Color::from_rgb(1.0, 0.36, 0.28),
            Color::from_rgb(1.0, 0.92, 0.88),
        ),
        Tone::Bad => (
            Color::from_rgba(0.3, 0.12, 0.03, 0.92),
            Color::from_rgb(0.95, 0.5, 0.2),
            Color::from_rgb(1.0, 0.72, 0.45),
        ),
        Tone::Warn => (
            Color::from_rgba(0.22, 0.16, 0.05, 0.92),
            theme::WARN,
            Color::from_rgb(0.98, 0.82, 0.52),
        ),
        Tone::Info => (
            Color::from_rgba(0.1, 0.08, 0.06, 0.92),
            theme::GOLD_DIM,
            theme::TEXT,
        ),
    }
}

/// The deadly chips' pulse now: 0..1.
fn pulse_now(now: f64) -> f32 {
    (0.5 + 0.5 * (now * TAU * PULSE_HZ).sin()) as f32
}

fn chip_style(bg: Color, border: Color) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(1);
    sb.set_border_color(border);
    sb.set_corner_radius_all(3);
    sb.set_corner_detail(1);
    sb.set_content_margin(Side::LEFT, 8.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    sb.set_content_margin(Side::TOP, 1.0);
    sb.set_content_margin(Side::BOTTOM, 1.0);
    sb
}

/// An engraved glyph of `ui_icon.gdshader`, `size` pixels square.
fn icon(which: i32, size: f32, color: Color) -> Gd<ColorRect> {
    let mut r = ColorRect::new_alloc();
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.set_custom_minimum_size(Vector2::new(size, size));
    r.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    match theme::ui_material("ui_icon") {
        Some(mut m) => {
            m.set_shader_parameter("icon", &which.to_variant());
            m.set_shader_parameter("color", &color.to_variant());
            r.set_material(&m);
        }
        None => r.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0)),
    }
    r
}

/// A medallion of `ui_ring.gdshader`.
fn ring(size: f32, inner: f32, rim: Color) -> (Gd<ColorRect>, Option<Gd<ShaderMaterial>>) {
    let mut r = ColorRect::new_alloc();
    r.set_mouse_filter(MouseFilter::IGNORE);
    r.set_custom_minimum_size(Vector2::new(size, size));
    r.set_size(Vector2::new(size, size));
    r.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    let m = theme::ui_material("ui_ring");
    match &m {
        Some(m) => {
            let mut m = m.clone();
            m.set_shader_parameter("inner", &inner.to_variant());
            m.set_shader_parameter("rim", &rim.to_variant());
            r.set_material(&m);
        }
        None => r.set_color(rim),
    }
    (r, m)
}

/// A value chip with a glyph: AC, gold.
struct ValueChip {
    panel: Gd<PanelContainer>,
    text: Gd<Label>,
}

impl ValueChip {
    fn new(glyph: i32) -> ValueChip {
        let mut panel = PanelContainer::new_alloc();
        panel.set_mouse_filter(MouseFilter::PASS);
        panel.add_theme_stylebox_override(
            "panel",
            &chip_style(Color::from_rgba(0.06, 0.045, 0.035, 0.9), theme::IRON),
        );
        let mut row = hbox(4);
        row.add_child(&icon(glyph, 18.0, theme::GOLD));
        let text = theme::styled_label("", Face::BodyBold, 16, theme::TEXT);
        row.add_child(&text);
        panel.add_child(&row);
        panel.set_custom_minimum_size(Vector2::new(0.0, 26.0));
        ValueChip { panel, text }
    }

    fn set(&mut self, value: Option<&str>, tip: &str) {
        self.text.set_text(value.unwrap_or(""));
        self.panel.set_visible(value.is_some());
        self.panel.set_tooltip_text(tip);
    }
}

/// A chip on screen (hunger, encumbrance, a condition) and what pulses in it.
struct ShownChip {
    chip: Chip,
    node: Gd<PanelContainer>,
    style: Gd<StyleBoxFlat>,
    gem: Option<Gd<ShaderMaterial>>,
}

/// A need chip (hunger, encumbrance): text in a toned frame.
fn need_chip(c: &Chip) -> ShownChip {
    let (bg, border, fg) = chip_colors(c.tone);
    let style = chip_style(bg, border);
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::IGNORE);
    p.add_theme_stylebox_override("panel", &style);
    p.set_custom_minimum_size(Vector2::new(0.0, 26.0));
    let word = i18n::engine(EngineKind::Status, &c.text);
    p.add_child(&theme::styled_label(&word, Face::BodyBold, 16, fg));
    ShownChip {
        chip: c.clone(),
        node: p,
        style,
        gem: None,
    }
}

/// A condition: a gem set in a ring, toned by the condition's danger, and
/// its name (the glyphs of phase I replace the name with an icon).
fn condition_chip(c: &Chip) -> ShownChip {
    let (bg, border, fg) = chip_colors(c.tone);
    let rim = match c.tone {
        Tone::Info => theme::GOLD,
        _ => border,
    };
    let mut style = chip_style(Color { a: 0.85, ..bg }, Color { a: 0.8, ..border });
    style.set_content_margin(Side::LEFT, 3.0);
    let mut p = PanelContainer::new_alloc();
    p.set_mouse_filter(MouseFilter::PASS);
    p.add_theme_stylebox_override("panel", &style);
    p.set_custom_minimum_size(Vector2::new(0.0, 26.0));
    let word = i18n::engine(EngineKind::Status, &c.text);
    p.set_tooltip_text(&*word);
    let mut row = hbox(5);
    let (gem, material) = ring(20.0, 0.62, rim);
    if let Some(m) = &material {
        let mut m = m.clone();
        m.set_shader_parameter("face_center", &rim.lerp(Color::WHITE, 0.15).to_variant());
        m.set_shader_parameter("face_edge", &rim.darkened(0.7).to_variant());
    }
    row.add_child(&gem);
    row.add_child(&theme::styled_label(&word, Face::BodyBold, 15, fg));
    p.add_child(&row);
    ShownChip {
        chip: c.clone(),
        node: p,
        style,
        gem: material,
    }
}

/// Seconds since the engine started (for fades).
fn now_secs() -> f64 {
    Time::singleton().get_ticks_msec() as f64 / 1000.0
}

/// The banner's opacity `t` seconds after the mode changed.
pub fn flash_alpha(t: f64) -> f32 {
    let fade = FLASH_FADE;
    if !(0.0..FLASH_SECS).contains(&t) {
        0.0
    } else if t < FLASH_SECS - fade {
        1.0
    } else {
        ((FLASH_SECS - t) / fade) as f32
    }
}

/// Replace a scrolling label's text; show the end (`follow`) or stay where
/// the wheel left it.
fn set_scrolled_text(label: &mut Gd<RichTextLabel>, text: &str, follow: bool) {
    let bar = label.get_v_scroll_bar();
    let keep = bar.as_ref().filter(|_| !follow).map(|b| b.get_value());
    label.set_text(text);
    // counting lines lays the text out, so the scroll range is current
    let last = (label.get_line_count() - 1).max(0);
    match (keep, bar) {
        (Some(v), Some(mut b)) => b.set_value(v),
        _ => label.scroll_to_line(last),
    }
}

fn rich(mouse: MouseFilter) -> Gd<RichTextLabel> {
    let mut r = RichTextLabel::new_alloc();
    r.set_use_bbcode(true);
    r.set_focus_mode(FocusMode::NONE);
    r.set_selection_enabled(false);
    r.set_mouse_filter(mouse);
    r
}

fn vbox(separation: i32) -> Gd<VBoxContainer> {
    let mut b = VBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", separation);
    b
}

fn hbox(separation: i32) -> Gd<HBoxContainer> {
    let mut b = HBoxContainer::new_alloc();
    b.set_mouse_filter(MouseFilter::IGNORE);
    b.add_theme_constant_override("separation", separation);
    b
}

fn ctrl(c: char) -> KeyInput {
    KeyInput {
        key: Key::Char(c),
        mods: Mods {
            ctrl: true,
            ..Mods::default()
        },
        echo: false,
    }
}

/// An icon-only button (36×28) with a tooltip naming it and its key.
fn micro_button(glyph: i32, tip: &'static str, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = theme::button("", queue, ev);
    b.set_custom_minimum_size(Vector2::new(MICRO_W, 28.0));
    i18n::tip(&b, tip);
    b.set_mouse_filter(MouseFilter::STOP);
    for state in ["normal", "hover", "pressed", "hover_pressed", "disabled"] {
        if let Some(mut sb) = b.get_theme_stylebox(state) {
            // the default content margins are for words
            sb = sb.duplicate_resource();
            sb.set_content_margin_all(0.0);
            b.add_theme_stylebox_override(state, &sb);
        }
    }
    let mut g = icon(glyph, 20.0, theme::GOLD);
    place(&g, [0.5, 0.5, 0.5, 0.5], [-10.0, -10.0, 10.0, 10.0]);
    g.set_v_size_flags(SizeFlags::FILL);
    b.add_child(&g);
    b
}

/// How wide a banner at the top centre may be on a canvas `width` wide:
/// it fits between the portrait block and the minimap.
pub fn banner_room(width: f32) -> f32 {
    let room = width - 2.0 * (EDGE + PORTRAIT_W.max(MINIMAP_W) + 16.0);
    room.clamp(320.0, 960.0)
}

/// Queue a key from a signal: the same as pressing it.
fn key_event(k: KeyInput) -> UiEvent {
    UiEvent::Key(k)
}

/// (HP, Pw) on the orbs, and each slot's (filled, enabled).
pub type ClusterView = (Option<(i64, i64)>, Option<(i64, i64)>, Vec<(bool, bool)>);

pub struct Hud {
    root: Gd<Control>,
    /// The blocks that take the mouse (then the map is not hovered).
    solid: Vec<Gd<Control>>,

    // portrait block, top left
    portrait_panel: Gd<PanelContainer>,
    initial: Gd<Label>,
    plaque: Gd<Label>,
    title: Gd<Label>,
    subtitle: Gd<Label>,
    ac: ValueChip,
    gold: ValueChip,
    need_box: Gd<HBoxContainer>,
    needs: Vec<ShownChip>,
    cond_box: Gd<HFlowContainer>,
    conditions: Vec<ShownChip>,
    attrs: Gd<RichTextLabel>,
    /// The attributes last shown, and the ones flashing (index, up, since).
    attr_values: Vec<(&'static str, String)>,
    attr_flash: Vec<(usize, bool, f64)>,
    role: Option<String>,
    align_text: Option<String>,

    // bottom cluster
    hp: Orb,
    pw: Orb,
    xp_fill: Gd<ColorRect>,
    xp_label: Gd<Label>,
    bar: ActionBar,
    /// The orbs, the XP bar, the micro-buttons and the bar, by name
    /// (self-tests: what must fit).
    cluster_parts: Vec<(&'static str, Gd<Control>)>,

    // the log, bottom left (docked over the cluster when compact)
    log_panel: Gd<PanelContainer>,
    log: Gd<RichTextLabel>,
    /// The log's title and history button (not in the compact layout).
    log_head: Gd<HBoxContainer>,
    log_compact: Option<bool>,
    /// The width the log's lines were wrapped at (another width: shown
    /// again, its whole lines are others).
    log_width: f32,
    /// When each message was first shown (seq, seconds).
    arrivals: VecDeque<(u64, f64)>,
    transient_panel: Gd<PanelContainer>,
    transient: Gd<Label>,
    /// getpos's description of the cursor's cell, next to that cell.
    cursor_note_panel: Gd<PanelContainer>,
    cursor_note: Gd<Label>,
    /// Where on screen the getpos cursor is (None: not in getpos).
    cursor_at: Option<Vector2>,
    /// The transient line shown (the engine's words) and whether next to
    /// the cursor (None: to show again).
    transient_shown: Option<(String, bool)>,
    /// The arrow at the screen's edge toward a hostile out of the frame.
    threat: Gd<Control>,
    /// The side of the hero the arrow went to, while their body is where
    /// it would be (true: the right).
    threat_side: Option<bool>,
    /// The hero's body on screen, as the arrow was last placed by.
    hero_box: Option<Rect2>,

    // minimap, mode badge and order line, top right
    minimap_panel: Gd<PanelContainer>,
    /// The map's well: as wide as the part of the level shown needs.
    minimap_well: Gd<ColorRect>,
    /// A modal panel (the inventory) is open: the corners and the log
    /// step back instead of peeking out clipped around it.
    panel_open: bool,
    /// And it asks the engine's question itself (the banner steps back).
    panel_asks: bool,
    minimap: Minimap,
    minimap_caption: Gd<Label>,
    mode_panel: Gd<PanelContainer>,
    mode_label: Gd<Label>,
    order_label: Gd<Label>,

    // centre
    prompt_row: Gd<HBoxContainer>,
    prompt_panel: Gd<PanelContainer>,
    prompt: Gd<Label>,
    flash: Gd<Label>,
    /// The ribbon the mode word sits in.
    flash_ribbon: Gd<HBoxContainer>,
    flash_since: Option<f64>,
    deadly: Gd<Label>,
    combat: Option<bool>,

    tooltip_panel: Gd<PanelContainer>,
    tooltip: Gd<Label>,
    tooltip_text: Option<String>,
    toast_panel: Gd<PanelContainer>,
    /// The row the toast is centred in: over the cluster, or over the
    /// docked log.
    toast_lane: Gd<HBoxContainer>,
    toast: Gd<Label>,
    toast_undo: Gd<Button>,
    toast_until: f64,
    full_log_shade: Gd<ColorRect>,
    full_log_panel: Gd<PanelContainer>,
    full_log: Gd<RichTextLabel>,
    full_log_count: Gd<Label>,

    status_text: String,
    /// (newest message, input seq, full log open, faded lines) last shown.
    log_key: Option<(u64, u64, bool, usize)>,
    full_log_dirty: bool,
    /// The full log jumps to the newest message on its next rebuild.
    full_log_follow: bool,
    /// The canvas size the layout was made for.
    laid_out: Option<Vector2>,
}

impl Hud {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Hud {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        layer.add_child(&root);
        let mut solid: Vec<Gd<Control>> = Vec::new();

        // ---- portrait block, top left ----
        let mut portrait_panel = theme::framed(Frame::Hud);
        portrait_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &portrait_panel,
            [0.0, 0.0, 0.0, 0.0],
            [EDGE, EDGE, EDGE + PORTRAIT_W, EDGE + PORTRAIT_H],
        );
        let mut block = Control::new_alloc();
        block.set_mouse_filter(MouseFilter::IGNORE);
        let (mut medallion, _) = ring(124.0, 0.9, theme::GOLD);
        medallion.set_position(Vector2::new(0.0, 0.0));
        block.add_child(&medallion);
        let mut initial = theme::styled_label("", Face::TitleBold, 64, theme::GOLD_BRIGHT);
        theme::outline(&initial, 6);
        place(&initial, [0.0, 0.0, 0.0, 0.0], [0.0, 0.0, 124.0, 118.0]);
        initial.set_horizontal_alignment(HorizontalAlignment::CENTER);
        initial.set_vertical_alignment(VerticalAlignment::CENTER);
        block.add_child(&initial);
        let mut plaque_panel = theme::framed(Frame::Banner);
        plaque_panel.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &plaque_panel,
            [0.0, 0.0, 0.0, 0.0],
            [30.0, 112.0, 94.0, 138.0],
        );
        let mut plaque_style = theme::frame_style(Frame::Banner);
        plaque_style.set_content_margin_all(0.0);
        if plaque_panel.get_material().is_none() {
            plaque_panel.add_theme_stylebox_override("panel", &plaque_style);
        } else if let Some(mut sb) = plaque_panel
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin_all(0.0);
        }
        let mut plaque = theme::styled_label("", Face::Caps, 15, theme::GOLD_BRIGHT);
        plaque.set_horizontal_alignment(HorizontalAlignment::CENTER);
        plaque.set_vertical_alignment(VerticalAlignment::CENTER);
        plaque_panel.add_child(&plaque);
        block.add_child(&plaque_panel);

        let mut col = vbox(4);
        place(&col, [0.0, 0.0, 1.0, 1.0], [140.0, -4.0, 0.0, 0.0]);
        let mut title = theme::styled_label("", Face::Title, 27, theme::GOLD_BRIGHT);
        title.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        title.set_clip_text(true);
        col.add_child(&title);
        let mut subtitle = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        subtitle.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
        subtitle.set_clip_text(true);
        col.add_child(&subtitle);
        let mut chip_row = hbox(6);
        chip_row.set_custom_minimum_size(Vector2::new(0.0, 26.0));
        let ac = ValueChip::new(5);
        chip_row.add_child(&ac.panel);
        let gold = ValueChip::new(6);
        chip_row.add_child(&gold.panel);
        let need_box = hbox(6);
        chip_row.add_child(&need_box);
        col.add_child(&chip_row);
        let mut cond_box = HFlowContainer::new_alloc();
        cond_box.set_mouse_filter(MouseFilter::IGNORE);
        cond_box.add_theme_constant_override("h_separation", 6);
        cond_box.add_theme_constant_override("v_separation", 4);
        col.add_child(&cond_box);
        block.add_child(&col);
        let mut attrs = rich(MouseFilter::STOP);
        attrs.set_fit_content(true);
        attrs.set_scroll_active(false);
        attrs.set_autowrap_mode(AutowrapMode::OFF);
        attrs.add_theme_font_override("normal_font", &theme::font(Face::Body));
        attrs.add_theme_font_size_override("normal_font_size", 16);
        i18n::tip(&attrs, "hud-attrs-tip");
        place(&attrs, [0.0, 1.0, 1.0, 1.0], [140.0, -24.0, 0.0, 0.0]);
        let q = queue.clone();
        attrs
            .signals()
            .gui_input()
            .connect(move |ev: Gd<InputEvent>| {
                if let Ok(b) = ev.try_cast::<InputEventMouseButton>()
                    && b.is_pressed()
                    && b.get_button_index() == MouseButton::LEFT
                {
                    push(&q, key_event(ctrl('x')));
                }
            });
        block.add_child(&attrs);
        portrait_panel.add_child(&block);
        root.add_child(&portrait_panel);
        solid.push(portrait_panel.clone().upcast());

        // ---- bottom cluster: orbs, XP bar, micro-buttons, action bar ----
        let mut cluster = Control::new_alloc();
        cluster.set_mouse_filter(MouseFilter::IGNORE);
        place(
            &cluster,
            [0.5, 1.0, 0.5, 1.0],
            [
                -CLUSTER_W / 2.0,
                -CLUSTER_BOTTOM - CLUSTER_H,
                CLUSTER_W / 2.0,
                -CLUSTER_BOTTOM,
            ],
        );
        let bar_x = (CLUSTER_W - BAR_W) / 2.0;
        let mut bar_frame = theme::framed(Frame::Hud);
        bar_frame.set_mouse_filter(MouseFilter::STOP);
        place(
            &bar_frame,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, CLUSTER_H - BAR_H, bar_x + BAR_W, CLUSTER_H],
        );
        let bar = ActionBar::new(&queue);
        let mut bar_node = bar.node();
        bar_node.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        bar_node.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        bar_frame.add_child(&bar_node);
        cluster.add_child(&bar_frame);
        let mut cluster_parts: Vec<(&'static str, Gd<Control>)> =
            vec![("action bar", bar_frame.clone().upcast())];
        solid.push(bar_frame.upcast());

        let xp_y = CLUSTER_H - BAR_H - 6.0 - 22.0;
        let mut xp_frame = theme::framed(Frame::Socket);
        xp_frame.set_mouse_filter(MouseFilter::STOP);
        place(
            &xp_frame,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, xp_y, bar_x + BAR_W, xp_y + 22.0],
        );
        let mut xp_box = Control::new_alloc();
        xp_box.set_mouse_filter(MouseFilter::IGNORE);
        let mut xp_track = ColorRect::new_alloc();
        xp_track.set_mouse_filter(MouseFilter::IGNORE);
        xp_track.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.5));
        place(&xp_track, [0.0, 0.5, 1.0, 0.5], [8.0, -6.0, -8.0, 6.0]);
        xp_box.add_child(&xp_track);
        let mut xp_fill = ColorRect::new_alloc();
        xp_fill.set_mouse_filter(MouseFilter::IGNORE);
        xp_fill.set_color(theme::XP);
        place(&xp_fill, [0.0, 0.5, 0.0, 0.5], [8.0, -6.0, 8.0, 6.0]);
        let mut xp_shine = ColorRect::new_alloc();
        xp_shine.set_mouse_filter(MouseFilter::IGNORE);
        xp_shine.set_color(Color::from_rgba(1.0, 0.92, 0.7, 0.45));
        place(&xp_shine, [0.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 3.0]);
        xp_fill.add_child(&xp_shine);
        xp_box.add_child(&xp_fill);
        let mut xp_label = theme::styled_label("", Face::BodyBold, 14, theme::TEXT);
        theme::outline(&xp_label, 5);
        place(&xp_label, [0.0, 0.0, 1.0, 1.0], [0.0, -4.0, 0.0, 4.0]);
        xp_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        xp_label.set_vertical_alignment(VerticalAlignment::CENTER);
        xp_box.add_child(&xp_label);
        xp_frame.add_child(&xp_box);
        if let Some(mut sb) = xp_frame
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin_all(0.0);
        }
        i18n::tip(&xp_frame, "hud-xp-tip");
        cluster.add_child(&xp_frame);
        cluster_parts.push(("XP bar", xp_frame.clone().upcast()));
        solid.push(xp_frame.clone().upcast());

        let mut micro = hbox(MICRO_GAP);
        micro.set_alignment(AlignmentMode::END);
        let micro_y = xp_y - 6.0 - 28.0;
        place(
            &micro,
            [0.0, 0.0, 0.0, 0.0],
            [bar_x, micro_y, bar_x + BAR_W, micro_y + 28.0],
        );
        let plain = |c| key_event(KeyInput::plain(Key::Char(c)));
        let buttons: [(i32, &'static str, UiEvent); MICRO_BUTTONS] = [
            (0, "hud-inventory-tip", plain('i')),
            (1, "hud-spells-tip", plain('+')),
            (2, "hud-character-tip", key_event(ctrl('x'))),
            (3, "hud-overview-tip", key_event(ctrl('o'))),
            (4, "hud-history-tip", UiEvent::ToggleFullLog),
            (8, "hud-achievements-tip", UiEvent::OpenAchievements),
            (9, "hud-help-tip", UiEvent::ToggleHelp),
            (7, "hud-settings-tip", UiEvent::OpenSettings),
        ];
        for (glyph, tip, ev) in buttons {
            let b = micro_button(glyph, tip, &queue, ev);
            micro.add_child(&b);
            solid.push(b.upcast());
        }
        cluster.add_child(&micro);
        cluster_parts.push(("micro-buttons", micro.clone().upcast()));

        let mut hp = Orb::new("orb-hp", theme::HP_DEEP, theme::HP);
        let mut pw = Orb::new("orb-pw", theme::PW_DEEP, theme::PW);
        let orb_y = CLUSTER_H - orb::SIZE;
        hp.node().set_position(Vector2::new(0.0, orb_y));
        pw.node()
            .set_position(Vector2::new(CLUSTER_W - orb::SIZE, orb_y));
        cluster.add_child(&hp.node());
        cluster.add_child(&pw.node());
        solid.push(hp.node());
        solid.push(pw.node());
        hp.reset();
        pw.reset();
        root.add_child(&cluster);
        cluster_parts.push(("HP orb", hp.node()));
        cluster_parts.push(("Pw orb", pw.node()));

        // ---- the log, bottom left; the wheel scrolls it ----
        let mut log_panel = theme::framed(Frame::Hud);
        log_panel.set_mouse_filter(MouseFilter::STOP);
        log_panel.set_self_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        let mut log_col = vbox(2);
        let mut log_head = hbox(4);
        log_head.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
        let mut log_title = theme::styled_label("", Face::Caps, 14, theme::TEXT_DIM);
        i18n::text(&log_title, "log-title");
        theme::outline(&log_title, 4);
        log_title.set_h_size_flags(SizeFlags::EXPAND_FILL);
        log_title.set_vertical_alignment(VerticalAlignment::CENTER);
        log_head.add_child(&log_title);
        let history = micro_button(4, "hud-history-tip", &queue, UiEvent::ToggleFullLog);
        log_head.add_child(&history);
        log_col.add_child(&log_head.clone());
        let mut log_area = vbox(0);
        log_area.set_alignment(AlignmentMode::END);
        log_area.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut log = rich(MouseFilter::STOP);
        log.set_scroll_follow(true);
        log.add_theme_font_override("normal_font", &theme::font(Face::Body));
        log.add_theme_font_override("bold_font", &theme::font(Face::BodyBold));
        log.add_theme_font_size_override("normal_font_size", LOG_FONT);
        log.add_theme_font_size_override("bold_font_size", LOG_FONT);
        log.add_theme_constant_override("line_separation", LOG_LINE_GAP);
        theme::outline(&log, 5);
        log.add_theme_constant_override("outline_size", 5);
        log.set_v_size_flags(SizeFlags::SHRINK_END);
        log_area.add_child(&log);
        log_col.add_child(&log_area);
        log_panel.add_child(&log_col);
        root.add_child(&log_panel);
        solid.push(log_panel.clone().upcast());

        // transient message (getpos autodescribe...), its own line above the log
        let mut transient_panel = theme::framed(Frame::Banner);
        transient_panel.set_mouse_filter(MouseFilter::IGNORE);
        transient_panel.set_v_grow_direction(GrowDirection::BEGIN);
        let mut transient = theme::styled_label("", Face::Body, 16, theme::GOLD_BRIGHT);
        transient.set_clip_text(true);
        transient_panel.add_child(&transient);
        transient_panel.set_visible(false);
        root.add_child(&transient_panel);
        let mut cursor_note_panel = theme::framed(Frame::Tooltip);
        cursor_note_panel.set_mouse_filter(MouseFilter::IGNORE);
        let cursor_note = theme::styled_label("", Face::Body, 16, theme::GOLD_BRIGHT);
        cursor_note_panel.add_child(&cursor_note);
        cursor_note_panel.set_visible(false);
        root.add_child(&cursor_note_panel);

        // an arrow (pointing right before it is turned) with a dark rim
        let mut threat = Control::new_alloc();
        threat.set_mouse_filter(MouseFilter::IGNORE);
        let tip = PackedVector2Array::from(&[
            Vector2::new(22.0, 0.0),
            Vector2::new(-14.0, -16.0),
            Vector2::new(-6.0, 0.0),
            Vector2::new(-14.0, 16.0),
        ]);
        let mut rim = Polygon2D::new_alloc();
        rim.set_polygon(&tip);
        rim.set_color(Color::from_rgba(0.02, 0.01, 0.01, 0.85));
        rim.set_scale(Vector2::new(1.25, 1.25));
        threat.add_child(&rim);
        let mut arrow = Polygon2D::new_alloc();
        arrow.set_polygon(&tip);
        arrow.set_color(theme::DANGER);
        threat.add_child(&arrow);
        threat.set_visible(false);
        i18n::tip(&threat, "hud-threat-tip");
        root.add_child(&threat);

        // ---- minimap, top right; the mode and the order under it ----
        let mut minimap_panel = theme::framed(Frame::Hud);
        minimap_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &minimap_panel,
            [1.0, 0.0, 1.0, 0.0],
            [-EDGE - MINIMAP_W, EDGE, -EDGE, EDGE + MINIMAP_H],
        );
        let mut mm_col = vbox(4);
        let minimap = Minimap::new(&queue);
        let mut well = ColorRect::new_alloc();
        let minimap_well = well.clone();
        well.set_mouse_filter(MouseFilter::IGNORE);
        well.set_color(Color::from_rgba(0.07, 0.055, 0.04, 0.85));
        well.set_custom_minimum_size(Vector2::new(minimap::WIDTH as f32, minimap::HEIGHT as f32));
        let mut map_node = minimap.node();
        place(&map_node, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        map_node.set_custom_minimum_size(Vector2::ZERO);
        well.add_child(&map_node);
        mm_col.add_child(&well);
        let mut minimap_caption = theme::styled_label("", Face::Body, 15, theme::TEXT_DIM);
        minimap_caption.set_horizontal_alignment(HorizontalAlignment::CENTER);
        mm_col.add_child(&minimap_caption);
        minimap_panel.add_child(&mm_col);
        root.add_child(&minimap_panel);
        solid.push(minimap_panel.clone().upcast());
        let minimap_frame = minimap_panel.clone();

        let mut mode_panel = theme::framed(Frame::Banner);
        mode_panel.set_mouse_filter(MouseFilter::IGNORE);
        // a longer word than the badge widens it leftwards, on screen
        mode_panel.set_h_grow_direction(GrowDirection::BEGIN);
        place(
            &mode_panel,
            [1.0, 0.0, 1.0, 0.0],
            [
                -EDGE - MODE_W,
                EDGE + MINIMAP_H + 8.0,
                -EDGE,
                EDGE + MINIMAP_H + 40.0,
            ],
        );
        if let Some(mut sb) = mode_panel
            .get_theme_stylebox("panel")
            .and_then(|s| s.try_cast::<StyleBoxFlat>().ok())
        {
            sb.set_content_margin(Side::TOP, 2.0);
            sb.set_content_margin(Side::BOTTOM, 2.0);
        }
        let mut mode_label = theme::styled_label("", Face::Title, 18, EXPLORE_TEXT);
        mode_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        mode_label.set_vertical_alignment(VerticalAlignment::CENTER);
        mode_panel.add_child(&mode_label);
        mode_panel.set_visible(false);
        root.add_child(&mode_panel);
        let mut order_label = theme::styled_label("", Face::Body, 16, theme::GOLD_BRIGHT);
        theme::outline(&order_label, 5);
        place(
            &order_label,
            [1.0, 0.0, 1.0, 0.0],
            [
                -EDGE - 400.0,
                EDGE + MINIMAP_H + 46.0,
                -EDGE,
                EDGE + MINIMAP_H + 70.0,
            ],
        );
        order_label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        order_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        order_label.set_visible(false);
        root.add_child(&order_label);

        // ---- the prompt banner, top centre ----
        let mut prompt_row = hbox(0);
        prompt_row.set_alignment(AlignmentMode::CENTER);
        place(&prompt_row, [0.0, 0.0, 1.0, 0.0], [0.0, EDGE, 0.0, EDGE]);
        let mut prompt_panel = theme::framed(Frame::Banner);
        prompt_panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut prompt = theme::styled_label("", Face::BodyBold, 19, theme::GOLD_BRIGHT);
        prompt.set_horizontal_alignment(HorizontalAlignment::CENTER);
        prompt.set_vertical_alignment(VerticalAlignment::CENTER);
        prompt.set_autowrap_mode(AutowrapMode::WORD_SMART);
        prompt.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        prompt_panel.add_child(&prompt);
        prompt_panel.set_visible(false);
        prompt_row.add_child(&prompt_panel);
        root.add_child(&prompt_row);

        // ---- the banner of a new mode, big, under the prompt banner ----
        // a ribbon under the prompt banner, off the hero (who is in the
        // middle of the screen at every zoom)
        let mut flash_row = hbox(0);
        flash_row.set_alignment(AlignmentMode::CENTER);
        place(
            &flash_row,
            [0.0, 0.0, 1.0, 0.0],
            [0.0, FLASH_Y, 0.0, FLASH_Y],
        );
        let mut flash_panel = theme::framed(Frame::Banner);
        flash_panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut flash = theme::styled_label("", Face::TitleBold, 34, EXPLORE_TEXT);
        flash.set_horizontal_alignment(HorizontalAlignment::CENTER);
        flash.set_custom_minimum_size(Vector2::new(300.0, 0.0));
        theme::outline(&flash, 6);
        flash_panel.add_child(&flash);
        flash_row.add_child(&flash_panel);
        flash_row.set_visible(false);
        root.add_child(&flash_row);
        let flash_ribbon = flash_row.clone();

        // ---- the deadly banner, centre ----
        let mut deadly = theme::styled_label("", Face::TitleBold, 42, theme::DANGER);
        place(&deadly, [0.5, 0.0, 0.5, 0.0], [-360.0, 236.0, 360.0, 300.0]);
        deadly.set_horizontal_alignment(HorizontalAlignment::CENTER);
        deadly.set_vertical_alignment(VerticalAlignment::CENTER);
        theme::outline(&deadly, 12);
        deadly.set_visible(false);
        root.add_child(&deadly);

        // ---- full log, F9; scrolls with the wheel ----
        // a modal the size of the inventory panel, over the dimmed world
        let mut full_log_shade = ColorRect::new_alloc();
        full_log_shade.set_color(theme::SHADE);
        theme::full_rect_ignore(&full_log_shade);
        full_log_shade.set_visible(false);
        root.add_child(&full_log_shade);
        let mut full_log_panel = theme::framed(Frame::Panel);
        full_log_panel.set_mouse_filter(MouseFilter::STOP);
        place(
            &full_log_panel,
            [0.5, 0.5, 0.5, 0.5],
            [-600.0, -380.0, 600.0, 380.0],
        );
        let mut full_col = vbox(8);
        let mut full_head = hbox(12);
        let full_title = theme::styled_label("", Face::Title, 28, theme::GOLD_BRIGHT);
        i18n::text(&full_title, "log-history-title");
        full_head.add_child(&full_title);
        let mut full_log_count = theme::styled_label("", Face::Body, 16, theme::TEXT_DIM);
        full_log_count.set_h_size_flags(SizeFlags::EXPAND_FILL);
        full_log_count.set_vertical_alignment(VerticalAlignment::CENTER);
        full_head.add_child(&full_log_count);
        let close = theme::button("", &queue, UiEvent::ToggleFullLog);
        i18n::text(&close, "log-history-close");
        full_head.add_child(&close);
        full_col.add_child(&full_head);
        let mut rule = ColorRect::new_alloc();
        rule.set_color(Color {
            a: 0.6,
            ..theme::GOLD_DIM
        });
        rule.set_custom_minimum_size(Vector2::new(0.0, 1.0));
        rule.set_mouse_filter(MouseFilter::IGNORE);
        full_col.add_child(&rule);
        let mut full_log = rich(MouseFilter::STOP);
        full_log.set_scroll_follow(true);
        full_log.set_v_size_flags(SizeFlags::EXPAND_FILL);
        full_log.add_theme_font_override("normal_font", &theme::font(Face::Body));
        full_log.add_theme_font_override("bold_font", &theme::font(Face::BodyBold));
        full_log.add_theme_font_size_override("normal_font_size", 17);
        full_log.add_theme_font_size_override("bold_font_size", 17);
        full_log.add_theme_constant_override("line_separation", 3);
        full_col.add_child(&full_log);
        full_log_panel.add_child(&full_col);
        full_log_panel.set_visible(false);
        root.add_child(&full_log_panel);
        solid.push(full_log_panel.clone().upcast());

        // ---- the toast lane: over the bar it is about, under any panel ----
        let mut toast_row = hbox(0);
        toast_row.set_alignment(AlignmentMode::CENTER);
        let toast_y = -CLUSTER_BOTTOM - CLUSTER_H + 2.0;
        place(
            &toast_row,
            [0.0, 1.0, 1.0, 1.0],
            [0.0, toast_y, 0.0, toast_y],
        );
        let mut toast_panel = theme::framed(Frame::Banner);
        toast_panel.set_mouse_filter(MouseFilter::STOP);
        let mut toast_box = hbox(14);
        let mut toast = theme::styled_label("", Face::BodyBold, 16, theme::GOLD_BRIGHT);
        toast.set_vertical_alignment(VerticalAlignment::CENTER);
        toast_box.add_child(&toast);
        let toast_undo = theme::button("", &queue, UiEvent::SlotUndo);
        i18n::text(&toast_undo, "bar-undo");
        toast_box.add_child(&toast_undo);
        toast_panel.add_child(&toast_box);
        toast_panel.set_visible(false);
        toast_row.add_child(&toast_panel);
        root.add_child(&toast_row);
        let toast_lane = toast_row.clone();
        solid.push(toast_panel.clone().upcast());

        // ---- tooltip, follows the mouse; on top of everything ----
        let mut tooltip_panel = theme::framed(Frame::Tooltip);
        tooltip_panel.set_mouse_filter(MouseFilter::IGNORE);
        let tooltip = theme::styled_label("", Face::Body, 16, theme::TEXT);
        tooltip_panel.add_child(&tooltip);
        tooltip_panel.set_visible(false);
        root.add_child(&tooltip_panel);

        let mut hud = Hud {
            root,
            solid,
            portrait_panel,
            initial,
            plaque,
            title,
            subtitle,
            ac,
            gold,
            need_box,
            needs: Vec::new(),
            cond_box,
            conditions: Vec::new(),
            attrs,
            attr_values: Vec::new(),
            attr_flash: Vec::new(),
            role: None,
            align_text: None,
            hp,
            pw,
            xp_fill,
            xp_label,
            bar,
            log_panel,
            log,
            log_head,
            cluster_parts,
            log_compact: None,
            log_width: 0.0,
            arrivals: VecDeque::new(),
            transient_panel,
            transient,
            cursor_note_panel,
            cursor_note,
            cursor_at: None,
            threat_side: None,
            hero_box: None,
            transient_shown: None,
            threat,
            minimap_panel: minimap_frame,
            minimap_well,
            panel_open: false,
            panel_asks: false,
            minimap,
            minimap_caption,
            mode_panel,
            mode_label,
            order_label,
            prompt_row,
            prompt_panel,
            prompt,
            flash,
            flash_ribbon,
            flash_since: None,
            deadly,
            combat: None,
            tooltip_panel,
            tooltip,
            tooltip_text: None,
            toast_panel,
            toast_lane,
            toast,
            toast_undo,
            toast_until: 0.0,
            full_log_shade,
            full_log_panel,
            full_log,
            full_log_count,
            status_text: String::new(),
            log_key: None,
            full_log_dirty: true,
            full_log_follow: true,
            laid_out: None,
        };
        hud.reset();
        hud
    }

    /// A short notice at the top centre for a few seconds; `undo` offers
    /// the Undo button.
    pub fn toast(&mut self, text: &str, undo: bool) {
        self.toast.set_text(text);
        self.toast_undo.set_visible(undo);
        self.toast_panel.reset_size();
        self.toast_panel.set_visible(true);
        self.toast_until = now_secs() + TOAST_SECS;
    }

    /// The hero's role (or form) as the map shows it: "Valkyrie".
    pub fn role(&self) -> Option<&str> {
        self.role.as_deref()
    }

    /// The action bar's slots (their bindings drive it).
    pub fn action_bar(&mut self) -> &mut ActionBar {
        &mut self.bar
    }

    /// Exploring or fighting; `announce` shows the big banner too.
    pub fn set_mode(&mut self, combat: bool, announce: bool) {
        if self.combat == Some(combat) && !announce {
            return;
        }
        self.combat = Some(combat);
        let (text, badge, word) = if combat {
            (COMBAT_TEXT, tr!("mode-combat-badge"), tr!("mode-combat"))
        } else {
            (EXPLORE_TEXT, tr!("mode-explore-badge"), tr!("mode-explore"))
        };
        self.mode_label.set_text(&badge);
        self.mode_label.add_theme_color_override("font_color", text);
        self.mode_panel
            .set_self_modulate(if combat { COMBAT_TINT } else { Color::WHITE });
        self.mode_panel.set_visible(true);
        if announce {
            self.flash.set_text(&word);
            self.flash.add_theme_color_override("font_color", text);
            self.flash_since = Some(now_secs());
            self.flash_ribbon.set_modulate(Color::WHITE);
            self.flash_ribbon.set_visible(true);
        }
    }

    /// Where on screen getpos's cursor is, while it is up.
    pub fn set_cursor_at(&mut self, at: Option<Vector2>) {
        self.cursor_at = at;
    }

    /// A hostile the hero sees at `at` on screen: when it is out of the
    /// frame, an arrow at the edge points to it.
    pub fn set_threat(&mut self, at: Option<Vector2>, hero: Option<Rect2>, now: f64) {
        let view = self.root.get_viewport_rect().size;
        self.hero_box = hero;
        // in a fight a hostile in the frame gets the arrow over its head
        // (a doorway's wall may hide it), pointing down and bobbing; not
        // on the hero it stands in front of: beside them then
        let mut side = self.threat_side.take();
        let over = at.filter(|_| self.combat == Some(true)).map(|p| {
            let head = Vector2::new(p.x, p.y - 70.0);
            let down = std::f32::consts::FRAC_PI_2;
            let (q, angle) = off_hero(head, down, p, view, hero, &mut side);
            let bob = 6.0 * (now * TAU * 1.2).sin() as f32;
            (Vector2::new(q.x, (q.y - bob).max(ARROW_MARGIN)), angle)
        });
        let edge = edge_arrow(at, view);
        // kept only while the arrow is beside the hero
        self.threat_side = side.filter(|_| over.is_some() && edge.is_none());
        let edge = edge.or(over);
        match edge {
            Some((p, angle)) => {
                // never on the HUD's own blocks: the hostile may stand
                // under one, or the edge's corner lie on it
                let (p, angle) = match at {
                    Some(target) => {
                        let panels: Vec<Rect2> =
                            self.blocks().into_iter().map(|(_, r)| r).collect();
                        off_panels(p, angle, target, view, &panels)
                    }
                    None => (p, angle),
                };
                self.threat.set_position(p);
                self.threat.set_rotation(angle);
                let a = 0.65 + 0.35 * pulse_now(now);
                self.threat.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
                self.threat.set_visible(true);
            }
            None => self.threat.set_visible(false),
        }
    }

    /// Where the threat arrow is, while it shows (self-tests).
    pub fn threat_arrow(&self) -> Option<Vector2> {
        self.threat.is_visible().then(|| self.threat.get_position())
    }

    /// The hero's body on screen and the side of it the threat arrow
    /// keeps to while it would lie on them (self-tests).
    pub fn threat_beside(&self) -> (Option<Rect2>, Option<bool>) {
        (self.hero_box, self.threat_side)
    }

    /// The minimap's frame takes the shape of what it shows (right
    /// aligned under the corner, as tall as ever).
    fn fit_minimap(&mut self) {
        let h = minimap::HEIGHT as f32;
        let w = (h * self.minimap.aspect())
            .clamp(h, minimap::WIDTH as f32)
            .round();
        if self.minimap_well.get_custom_minimum_size().x == w {
            return;
        }
        self.minimap_well
            .set_custom_minimum_size(Vector2::new(w, h));
        let frame_w = w + (MINIMAP_W - minimap::WIDTH as f32);
        self.minimap_panel.set_offset(Side::LEFT, -EDGE - frame_w);
    }

    /// What the order does or why it ended; None hides the line.
    pub fn set_order_line(&mut self, text: Option<&str>) {
        let text = text.unwrap_or("");
        if self.order_label.get_text() != text {
            self.order_label.set_text(text);
            self.order_label.set_visible(!text.is_empty());
        }
    }

    /// (the mode badge, the order line, the banner on screen) (self-tests).
    pub fn mode_view(&self) -> (Option<String>, Option<String>, bool) {
        let badge = self
            .mode_panel
            .is_visible()
            .then(|| self.mode_label.get_text().to_string());
        let order = self
            .order_label
            .is_visible()
            .then(|| self.order_label.get_text().to_string());
        (badge, order, self.flash_ribbon.is_visible())
    }

    /// (HP, Pw) the orbs show, and the ten slots' (filled, enabled) (self-tests).
    pub fn cluster_view(&self) -> ClusterView {
        (self.hp.view().0, self.pw.view().0, self.bar.slot_states())
    }

    /// The banner fades out.
    fn fade_flash(&mut self, now: f64) {
        let Some(since) = self.flash_since else {
            return;
        };
        let a = flash_alpha(now - since);
        if a <= 0.0 {
            self.flash_since = None;
            self.flash_ribbon.set_visible(false);
        } else {
            self.flash_ribbon
                .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
        }
    }

    /// What the HUD draws itself, again in the language now (bound labels
    /// change by themselves): the status, its chips, the log from the
    /// engine's English, the mode, the orbs' tooltips.
    pub fn relang(&mut self) {
        self.status_text.clear();
        for mut c in self.needs.drain(..).chain(self.conditions.drain(..)) {
            c.node.queue_free();
        }
        self.log_key = None;
        self.full_log_dirty = true;
        self.show_subtitle();
        if let Some(combat) = self.combat.take() {
            self.set_mode(combat, false);
            // the banner still fading out says its word again
            if self.flash_since.is_some() {
                let word = if combat {
                    tr!("mode-combat")
                } else {
                    tr!("mode-explore")
                };
                self.flash.set_text(&word);
            }
        }
        self.hp.relang();
        self.pw.relang();
        self.tooltip_text = None;
        self.transient_shown = None;
    }

    /// Called every frame while a game is on screen.
    pub fn sync(&mut self, world: &mut World, catalog: Option<&Catalog>) {
        let now = now_secs();
        self.layout();
        if let Some(cat) = catalog {
            self.sync_role(world, cat);
        }
        if world.status.take_changed() || self.status_text.is_empty() {
            self.show_status(&world.status, catalog, now);
        }
        self.hp.tick(now);
        self.pw.tick(now);
        self.pulse(now);
        self.fade_attrs(now);
        self.fade_flash(now);
        if self.toast_panel.is_visible() {
            // the last second fades out
            let left = self.toast_until - now;
            if left <= 0.0 {
                self.toast_panel.set_visible(false);
            } else {
                let a = left.min(1.0) as f32;
                self.toast_panel
                    .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
            }
        }
        self.hover_log();
        if let Some(cat) = catalog {
            self.minimap.sync(&world.map, cat, now);
            self.fit_minimap();
        }
        let full_open = self.full_log_panel.is_visible();
        let last_seq = world.log.last_seq();
        let faded = self.note_arrivals(world, now);
        let key = (last_seq, world.input_seq(), full_open, faded);
        if self.log.get_size().x != self.log_width {
            self.log_key = None;
        }
        if self.log_key != Some(key) {
            let news = self.log_key.is_none_or(|(seen, ..)| seen != last_seq);
            self.log_key = Some(key);
            self.show_log(world, news, now);
            self.full_log_dirty |= news;
            self.full_log_follow |= news;
        }
        if full_open && self.full_log_dirty {
            self.full_log_dirty = false;
            let follow = std::mem::take(&mut self.full_log_follow);
            self.show_full_log(world, follow);
        }
        let transient = world.transient.as_deref().unwrap_or("").trim();
        let at_cursor = self.cursor_at.is_some();
        let shown = self
            .transient_shown
            .as_ref()
            .is_some_and(|(t, c)| t == transient && *c == at_cursor);
        if !shown {
            self.transient_shown = Some((transient.to_string(), at_cursor));
            let words = if transient.is_empty() {
                String::new()
            } else {
                i18n::engine(EngineKind::Message, transient).into_owned()
            };
            // getpos describes the cursor's cell: next to that cell
            let (docked, note) = if at_cursor {
                ("", words.as_str())
            } else {
                (words.as_str(), "")
            };
            self.transient.set_text(docked);
            self.transient_panel.set_visible(!docked.is_empty());
            self.cursor_note.set_text(note);
            self.cursor_note_panel.reset_size();
            self.cursor_note_panel.set_visible(!note.is_empty());
        }
        let note = at_cursor && !transient.is_empty();
        if let Some(at) = self.cursor_at.filter(|_| note) {
            let view = self.root.get_viewport_rect().size;
            let size = self.cursor_note_panel.get_size();
            let mut p = at + Vector2::new(36.0, -size.y * 0.5);
            if p.x + size.x > view.x - 8.0 {
                p.x = at.x - 36.0 - size.x;
            }
            p.y = p.y.clamp(8.0, view.y - size.y - 8.0);
            if self.cursor_note_panel.get_position() != p {
                self.cursor_note_panel.set_position(p);
            }
        }
    }

    /// Place the blocks that depend on the canvas size (the compact
    /// layout, the prompt banner's width).
    fn layout(&mut self) {
        let size = self.root.get_viewport_rect().size;
        if self.laid_out == Some(size) {
            return;
        }
        self.laid_out = Some(size);
        let compact = compact(size.x);
        if self.log_compact != Some(compact) {
            self.log_compact = Some(compact);
            if compact {
                // docked on the XP bar, from the bar's left end, as high
                // as its lines and the frame
                let (left, bottom) = compact_log_corner();
                let line = theme::font(Face::Body)
                    .get_height_ex()
                    .font_size(LOG_FONT)
                    .done()
                    + LOG_LINE_GAP as f32;
                let h = 2.0 * LOG_MARGIN + LOG_COMPACT_LINES * line.ceil() + 2.0;
                place(
                    &self.log_panel,
                    [0.5, 1.0, 0.5, 1.0],
                    [left, bottom - h, left + LOG_COMPACT_W, bottom],
                );
            } else {
                place(
                    &self.log_panel,
                    [0.0, 1.0, 0.0, 1.0],
                    [EDGE, -EDGE - LOG_H, EDGE + LOG_W, -EDGE],
                );
            }
            // the attribute strip goes to the tooltip and the Character panel
            self.attrs.set_visible(!compact);
            let h = if compact {
                PORTRAIT_H - 14.0
            } else {
                PORTRAIT_H
            };
            self.log_head.set_visible(!compact);
            self.portrait_panel.set_offset(Side::BOTTOM, EDGE + h);
            let log_top = self.log_panel.get_offset(Side::TOP);
            let (l, r) = (
                self.log_panel.get_offset(Side::LEFT),
                self.log_panel.get_offset(Side::RIGHT),
            );
            let anchor = self.log_panel.get_anchor(Side::LEFT);
            place(
                &self.transient_panel,
                [anchor, 1.0, anchor, 1.0],
                [l, log_top - 6.0, r, log_top - 6.0],
            );
            // the toast is over the cluster, or over the log docked there
            let toast_y = if compact {
                log_top - 6.0
            } else {
                -CLUSTER_BOTTOM - CLUSTER_H + 2.0
            };
            self.toast_lane.set_offset(Side::TOP, toast_y);
            self.toast_lane.set_offset(Side::BOTTOM, toast_y);
            self.toast_lane.set_v_grow_direction(if compact {
                GrowDirection::BEGIN
            } else {
                GrowDirection::END
            });
            self.log_key = None;
        }
        let max = banner_room(size.x);
        self.prompt.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        self.prompt_row.set_meta("max_width", &max.to_variant());
        self.fit_prompt();
    }

    /// The prompt banner as wide as its text, 560–960 where there is room.
    fn fit_prompt(&mut self) {
        let max = self
            .prompt_row
            .get_meta("max_width")
            .try_to::<f32>()
            .unwrap_or(960.0);
        let text = self.prompt.get_text().to_string();
        let font = theme::font(Face::BodyBold);
        let w = font.get_string_size_ex(&text).font_size(19).done().x;
        let width = (w + 8.0).clamp(560.0f32.min(max), max);
        self.prompt
            .set_custom_minimum_size(Vector2::new(width, 24.0));
        self.prompt_panel.reset_size();
    }

    /// The hero's form, from the glyph under the hero: the portrait's
    /// initial and the subtitle.
    fn sync_role(&mut self, world: &World, catalog: &Catalog) {
        let name = world
            .map
            .hero()
            .and_then(|(x, y)| world.map.cell(x, y))
            .and_then(|c| c.glyph.as_ref())
            .filter(|g| g.kind == GlyphKind::Mon && g.flags & mg::HERO != 0)
            .and_then(|g| {
                let m = catalog.monsters.get(usize::try_from(g.mon?).ok()?)?;
                let gendered = if g.flags & mg::FEMALE != 0 {
                    m.female.as_ref()
                } else if g.flags & mg::MALE != 0 {
                    m.male.as_ref()
                } else {
                    None
                };
                Some(title_case(gendered.unwrap_or(&m.name)))
            });
        if name.is_some() && name != self.role {
            self.role = name;
            self.show_subtitle();
        }
    }

    fn show_subtitle(&mut self) {
        let role = self
            .role
            .as_deref()
            .map(|r| i18n::engine(EngineKind::Name, r).into_owned());
        // the alignment describes the hero: it agrees with the hero
        let align = self
            .align_text
            .as_deref()
            .map(|a| i18n::engine_hero_word(a).into_owned());
        let parts: Vec<String> = [role, align].into_iter().flatten().collect();
        self.subtitle.set_text(&parts.join("  ·  "));
        let initial = self
            .role
            .as_deref()
            .and_then(|r| r.chars().next())
            .map_or(String::new(), |c| c.to_string());
        self.initial.set_text(&initial);
    }

    fn show_status(&mut self, status: &Status, catalog: Option<&Catalog>, now: f64) {
        let conditions = catalog.map_or_else(Vec::new, |c| status.condition_names(c));
        let view = StatusView::new(status, &conditions);
        self.status_text = view.lines().join("\n");
        self.portrait_panel.set_tooltip_text(&status_tip(&view));
        self.title
            .set_text(&*i18n::engine(EngineKind::Status, &view.title));
        if self.align_text != view.align {
            self.align_text = view.align.clone();
            self.show_subtitle();
        }
        self.ac.set(
            view.field("AC"),
            &tr!("hud-ac-tip", ac = view.field("AC").unwrap_or("")),
        );
        self.gold.set(
            view.field("$"),
            &tr!("hud-gold-tip", gold = view.field("$").unwrap_or("")),
        );
        let level = view.field("XL").and_then(|v| v.parse::<i64>().ok());
        let plaque = match (level, view.field("HD")) {
            (Some(l), _) => tr!("hud-level", level = l),
            (None, Some(hd)) => tr!("hud-hit-dice", hd = hd),
            (None, None) => String::new(),
        };
        self.plaque.set_text(&plaque);

        self.hp.set(view.hp, now);
        self.pw.set(view.pw, now);
        let low = match (view.hp, level) {
            (Some((h, m)), Some(l)) => critically_low_hp(h, m, l),
            _ => false,
        };
        self.hp.set_low(low);

        // the XP bar
        let exp = view.field("Exp").and_then(|v| v.parse::<i64>().ok());
        let (share, label) = match (level, exp) {
            (Some(l), Some(e)) => {
                let (share, next) = xp_progress(l, e);
                (share, tr!("hud-xp", level = l, exp = e, next = next))
            }
            (Some(l), None) => (0.0, tr!("hud-level", level = l)),
            (None, _) => (
                0.0,
                view.field("HD")
                    .map_or(String::new(), |hd| tr!("hud-hit-dice", hd = hd)),
            ),
        };
        let w = (BAR_W - 16.0) * share;
        self.xp_fill.set_offset(Side::RIGHT, 8.0 + w);
        self.xp_fill.set_visible(share > 0.0);
        self.xp_label.set_text(&label);

        // the minimap's caption: where and when
        let place = place_text(&view);
        let turn = view.field("T").map(|t| tr!("hud-turn", turn = t));
        let caption: Vec<String> = [place, turn].into_iter().flatten().collect();
        self.minimap_caption.set_text(&caption.join("  ·  "));

        self.show_attrs(&view.attrs, now);
        self.show_chips(&view);
    }

    fn show_attrs(&mut self, attrs: &[(&'static str, String)], now: f64) {
        if self.attr_values.len() == attrs.len() {
            for (i, ((_, old), (_, new))) in self.attr_values.iter().zip(attrs).enumerate() {
                if old == new {
                    continue;
                }
                if let (Some(a), Some(b)) = (attr_rank(old), attr_rank(new)) {
                    self.attr_flash.retain(|(j, ..)| *j != i);
                    self.attr_flash.push((i, b > a, now));
                }
            }
        }
        self.attr_values = attrs.to_vec();
        self.draw_attrs();
    }

    fn draw_attrs(&mut self) {
        let dim = hex(theme::TEXT_DIM);
        let text: Vec<String> = self
            .attr_values
            .iter()
            .enumerate()
            .map(|(i, (tag, v))| {
                let color = match self.attr_flash.iter().find(|(j, ..)| *j == i) {
                    Some((_, true, _)) => theme::GOOD,
                    Some((_, false, _)) => theme::DANGER,
                    None => theme::TEXT,
                };
                format!(
                    "[color={dim}]{}[/color] [color={}]{}[/color]",
                    bbcode_escape(&attr_tag(tag)),
                    hex(color),
                    bbcode_escape(v)
                )
            })
            .collect();
        self.attrs.set_text(&text.join("    "));
    }

    /// Changed attributes go back to parchment after a while.
    fn fade_attrs(&mut self, now: f64) {
        let before = self.attr_flash.len();
        self.attr_flash
            .retain(|(_, _, since)| now - since < ATTR_FLASH_SECS);
        if self.attr_flash.len() != before {
            self.draw_attrs();
        }
    }

    fn show_chips(&mut self, view: &StatusView) {
        let same = |shown: &[ShownChip], chips: &[Chip]| {
            shown.len() == chips.len() && shown.iter().zip(chips).all(|(a, b)| a.chip == *b)
        };
        if !same(&self.needs, &view.needs) {
            for mut c in self.needs.drain(..) {
                c.node.queue_free();
            }
            for c in &view.needs {
                let shown = need_chip(c);
                self.need_box.add_child(&shown.node);
                self.needs.push(shown);
            }
        }
        if !same(&self.conditions, &view.conditions) {
            for mut c in self.conditions.drain(..) {
                c.node.queue_free();
            }
            for c in &view.conditions {
                let shown = condition_chip(c);
                self.cond_box.add_child(&shown.node);
                self.conditions.push(shown);
            }
            self.cond_box.set_visible(!self.conditions.is_empty());
            let words: Vec<String> = view
                .conditions
                .iter()
                .filter_map(|c| deadly_words(&c.text))
                .collect();
            self.deadly.set_text(&words.join("   "));
            self.deadly.set_visible(!words.is_empty());
        }
    }

    /// Deadly chips, their gems and the deadly banner glow and fade.
    fn pulse(&mut self, now: f64) {
        let any = |v: &[ShownChip]| v.iter().any(|c| c.chip.tone == Tone::Deadly);
        if !any(&self.needs) && !any(&self.conditions) && !self.deadly.is_visible() {
            return;
        }
        let k = pulse_now(now);
        let (bg, ..) = chip_colors(Tone::Deadly);
        let glow = bg.lerp(Color::from_rgb(0.95, 0.2, 0.15), k as f64);
        for c in self.needs.iter_mut().chain(self.conditions.iter_mut()) {
            if c.chip.tone == Tone::Deadly {
                c.style.set_bg_color(glow);
                if let Some(m) = &mut c.gem {
                    m.set_shader_parameter("glow", &k.to_variant());
                }
            }
        }
        if self.deadly.is_visible() {
            self.deadly
                .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.55 + 0.45 * k));
        }
    }

    /// The log's frame shows while the mouse is over it.
    fn hover_log(&mut self) {
        let mouse = self.root.get_global_mouse_position();
        let over = self.log_panel.is_visible_in_tree()
            && self.log_panel.get_global_rect().contains_point(mouse);
        // docked on the cluster (the compact layout) it keeps its
        // backing, as the bar does, or its lines would float over the scene
        let rest = if self.log_compact == Some(true) {
            1.0
        } else {
            0.0
        };
        let target = if over { 1.0 } else { rest };
        let mut m = self.log_panel.get_self_modulate();
        if (m.a - target).abs() > 0.01 {
            m.a += (target - m.a) * 0.25;
            if (m.a - target).abs() <= 0.01 {
                m.a = target;
            }
            self.log_panel.set_self_modulate(m);
            // the title and the history button come with the frame
            self.log_head
                .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, m.a));
        }
    }

    /// Remember when each message came; how many of the log's lines have
    /// faded by `now`.
    fn note_arrivals(&mut self, world: &World, now: f64) -> usize {
        let last = self.arrivals.back().map_or(0, |(s, _)| *s);
        for m in world.log.since(last) {
            // restored history is old from the start
            let at = if m.from_history {
                f64::NEG_INFINITY
            } else {
                now
            };
            self.arrivals.push_back((m.seq, at));
        }
        while self.arrivals.len() > LOG_LINES {
            self.arrivals.pop_front();
        }
        let secs = self.fade_secs();
        self.arrivals
            .iter()
            .filter(|(_, at)| now - at > secs)
            .count()
    }

    fn fade_secs(&self) -> f64 {
        if self.log_compact == Some(true) {
            LOG_FADE_COMPACT_SECS
        } else {
            LOG_FADE_SECS
        }
    }

    /// `follow`: new messages came, show the newest; else keep the place
    /// the player scrolled to.
    fn show_log(&mut self, world: &World, follow: bool, now: f64) {
        let input_seq = world.input_seq();
        let secs = self.fade_secs();
        let recent: Vec<&Message> = world.log.iter().rev().take(LOG_LINES).collect();
        let lines: Vec<String> = recent
            .iter()
            .rev()
            .map(|m| {
                let at = self
                    .arrivals
                    .iter()
                    .rev()
                    .find(|(s, _)| *s == m.seq)
                    .map_or(f64::NEG_INFINITY, |(_, at)| *at);
                message_bbcode(m, input_seq, now - at > secs)
            })
            .collect();
        set_scrolled_text(&mut self.log, &lines.join("\n"), follow);
        // the log sits on the panel's bottom edge and grows up to fill it,
        // in whole lines: the top one is never cut in half
        let h = self.log.get_content_height() as f32;
        let room = self.log.get_parent_control().map_or(h, |p| p.get_size().y);
        let shown = if h <= room {
            h
        } else {
            let n = self.log.get_line_count();
            (0..n)
                .map(|i| h - self.log.get_line_offset(i))
                .find(|&rest| rest <= room)
                .unwrap_or(room)
        };
        self.log
            .set_custom_minimum_size(Vector2::new(0.0, shown.max(1.0)));
        self.log_width = self.log.get_size().x;
    }

    fn show_full_log(&mut self, world: &World, follow: bool) {
        let input_seq = world.input_seq();
        let dim = hex(theme::TEXT_OFF);
        let lines: Vec<String> = world
            .log
            .iter()
            .map(|m| {
                let turn = m.turn.map_or(String::new(), |t| tr!("hud-turn", turn = t));
                format!(
                    "[color={dim}]{turn:<8}[/color]  {}",
                    message_bbcode(m, input_seq, false)
                )
            })
            .collect();
        self.full_log_count
            .set_text(&tr!("log-count", n = world.log.len()));
        set_scrolled_text(&mut self.full_log, &lines.join("\n"), follow);
        // as tall as its lines (a long line wraps: count it twice), up to
        // the inventory panel's size
        let rows: usize = world
            .log
            .iter()
            .map(|m| 1 + usize::from(m.text.chars().count() > 110))
            .sum();
        let h = (rows as f32 * FULL_LOG_LINE + FULL_LOG_CHROME).clamp(260.0, 760.0);
        let half = (h / 2.0).round();
        self.full_log_panel.set_offset(Side::TOP, -half);
        self.full_log_panel.set_offset(Side::BOTTOM, half);
    }

    /// The status as plain text.
    pub fn status_text(&self) -> &str {
        &self.status_text
    }

    /// Is a block that takes the mouse under `pos` (then the map is not hovered)?
    pub fn covers(&self, pos: Vector2) -> bool {
        self.root.is_visible()
            && self.solid.iter().any(|p| {
                p.is_visible_in_tree()
                    && p.get_mouse_filter() != MouseFilter::IGNORE
                    && p.get_global_rect().contains_point(pos)
            })
    }

    /// What the prompt line shows, if it is up (self-tests).
    pub fn prompt_line(&self) -> Option<String> {
        self.prompt_panel
            .is_visible()
            .then(|| self.prompt.get_text().to_string())
    }

    /// The full message log is open (self-tests).
    pub fn full_log_open(&self) -> bool {
        self.full_log_panel.is_visible()
    }

    pub fn set_prompt_line(&mut self, text: Option<&str>) {
        match text.filter(|t| !t.is_empty()) {
            Some(t) => {
                if self.prompt.get_text() != t {
                    self.prompt.set_text(t);
                    self.fit_prompt();
                }
                self.prompt_panel.set_visible(true);
            }
            None => self.prompt_panel.set_visible(false),
        }
    }

    /// Show `text` next to the mouse at `screen_pos`, kept inside the
    /// window; `None` hides it. Cheap when nothing changed (called every frame).
    pub fn set_tooltip(&mut self, text: Option<&str>, screen_pos: Vector2) {
        let Some(t) = text.filter(|t| !t.is_empty()) else {
            if self.tooltip_text.take().is_some() {
                self.tooltip_panel.set_visible(false);
            }
            return;
        };
        if self.tooltip_text.as_deref() != Some(t) {
            self.tooltip_text = Some(t.to_string());
            self.tooltip.set_text(t);
            self.tooltip_panel.reset_size();
            self.tooltip_panel.set_visible(true);
        }
        let size = self.tooltip_panel.get_size();
        let view = self.root.get_viewport_rect().size;
        let mut pos = screen_pos + Vector2::new(TOOLTIP_GAP, TOOLTIP_GAP);
        if pos.x + size.x > view.x {
            pos.x = screen_pos.x - TOOLTIP_GAP - size.x;
        }
        if pos.y + size.y > view.y {
            pos.y = screen_pos.y - TOOLTIP_GAP - size.y;
        }
        let pos = Vector2::new(pos.x.max(0.0), pos.y.max(0.0));
        if self.tooltip_panel.get_position() != pos {
            self.tooltip_panel.set_position(pos);
        }
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
        if !on {
            self.set_tooltip(None, Vector2::ZERO);
            self.set_full_log(false);
        }
    }

    pub fn toggle_full_log(&mut self) {
        let on = !self.full_log_panel.is_visible();
        self.set_full_log(on);
    }

    /// Open the full log (^P), scrolled to the newest message.
    pub fn open_full_log(&mut self) {
        self.set_full_log(true);
    }

    fn set_full_log(&mut self, on: bool) {
        self.full_log_panel.set_visible(on);
        self.full_log_shade.set_visible(on);
        self.full_log_dirty = true;
        self.full_log_follow = true;
        self.step_back();
    }

    /// The HUD's blocks on screen, by name, where they are (self-tests:
    /// they must fit the canvas and not overlap). Blocks stepped back
    /// under a panel are not on screen.
    pub fn blocks(&self) -> Vec<(&'static str, Rect2)> {
        let shown = |c: &Gd<Control>| c.is_visible_in_tree() && c.get_modulate().a > 0.0;
        // a row's rect is as wide as the row: its children's are what shows
        let rect = |c: &Gd<Control>| {
            let kids: Vec<Rect2> = c
                .get_children()
                .iter_shared()
                .filter_map(|k| k.try_cast::<Control>().ok())
                .filter(|k| k.is_visible())
                .map(|k| k.get_global_rect())
                .collect();
            match (c.is_class("HBoxContainer"), kids.split_first()) {
                (true, Some((first, rest))) => rest.iter().fold(*first, |a, b| a.merge(*b)),
                _ => c.get_global_rect(),
            }
        };
        [
            ("portrait", self.portrait_panel.clone().upcast::<Control>()),
            ("minimap", self.minimap_panel.clone().upcast()),
            ("mode badge", self.mode_panel.clone().upcast()),
            ("log", self.log_panel.clone().upcast()),
            ("prompt", self.prompt_panel.clone().upcast()),
        ]
        .into_iter()
        .chain(self.cluster_parts.iter().cloned())
        .filter(|(name, c)| {
            // the banner steps back through its row
            let row = *name != "prompt" || self.prompt_row.get_modulate().a > 0.0;
            shown(c) && row
        })
        .map(|(name, c)| (name, rect(&c)))
        .collect()
    }

    /// The hero's gender became known or changed: the words that agree
    /// with the hero again.
    pub fn hero_changed(&mut self) {
        self.show_subtitle();
        self.status_text.clear();
    }

    /// The log as it shows now (self-tests).
    pub fn log_shown(&self) -> String {
        self.log.get_parsed_text().to_string()
    }

    /// Where the log is (the gamepad's hints keep right of it).
    pub fn log_rect(&self) -> Rect2 {
        self.log_panel.get_global_rect()
    }

    /// The log shows whole lines: its view is as high as its last lines
    /// (none cut at the top).
    pub fn log_lines_whole(&self) -> bool {
        let h = self.log.get_content_height() as f32;
        let view = self.log.get_size().y;
        view >= h - 0.5
            || (0..self.log.get_line_count())
                .any(|i| (h - self.log.get_line_offset(i) - view).abs() < 0.5)
    }

    /// A modal panel of the game's (the inventory) opens or closes;
    /// `asks`: it shows the engine's question in its own header.
    pub fn set_panel_open(&mut self, on: bool, asks: bool) {
        if (self.panel_open, self.panel_asks) != (on, asks) {
            self.panel_open = on;
            self.panel_asks = asks;
            self.step_back();
        }
    }

    /// Under a modal panel the portrait, the minimap, the mode badge, the
    /// order line and the log fade out (the orbs and the bar stay).
    fn step_back(&mut self) {
        let away = self.panel_open || self.full_log_panel.is_visible();
        let m = Color::from_rgba(1.0, 1.0, 1.0, if away { 0.0 } else { 1.0 });
        let filter = if away {
            MouseFilter::IGNORE
        } else {
            MouseFilter::STOP
        };
        for mut c in [
            self.portrait_panel.clone().upcast::<Control>(),
            self.minimap_panel.clone().upcast(),
            self.log_panel.clone().upcast(),
        ] {
            c.set_modulate(m);
            c.set_mouse_filter(filter);
        }
        // the question is in the panel's header: the banner would only
        // peek out behind its top edge on a short screen
        let asked = self.panel_open && self.panel_asks;
        self.prompt_row.set_modulate(Color::from_rgba(
            1.0,
            1.0,
            1.0,
            if asked { 0.0 } else { 1.0 },
        ));
        for mut c in [
            self.mode_panel.clone().upcast::<Control>(),
            self.order_label.clone().upcast(),
            self.transient_panel.clone().upcast(),
            self.threat.clone().upcast(),
        ] {
            c.set_modulate(m);
        }
    }

    /// Forget what was shown (a new game).
    pub fn reset(&mut self) {
        self.status_text.clear();
        self.log_key = None;
        self.full_log_dirty = true;
        self.full_log_follow = true;
        self.title.set_text("");
        self.subtitle.set_text("");
        self.initial.set_text("");
        self.plaque.set_text("");
        self.role = None;
        self.align_text = None;
        self.ac.set(None, "");
        self.gold.set(None, "");
        self.attrs.set_text("");
        self.attr_values.clear();
        self.attr_flash.clear();
        for mut c in self.needs.drain(..).chain(self.conditions.drain(..)) {
            c.node.queue_free();
        }
        self.cond_box.set_visible(false);
        self.deadly.set_visible(false);
        self.hp.reset();
        self.pw.reset();
        self.xp_fill.set_visible(false);
        self.xp_label.set_text("");
        self.minimap.reset();
        self.minimap_caption.set_text("");
        self.arrivals.clear();
        self.log.set_text("");
        self.full_log.set_text("");
        self.transient.set_text("");
        self.transient_panel.set_visible(false);
        self.cursor_note.set_text("");
        self.cursor_note_panel.set_visible(false);
        self.cursor_at = None;
        self.threat.set_visible(false);
        self.set_prompt_line(None);
        self.set_tooltip(None, Vector2::ZERO);
        self.set_full_log(false);
        self.combat = None;
        self.mode_panel.set_visible(false);
        self.set_order_line(None);
        self.flash_since = None;
        self.flash_ribbon.set_visible(false);
        self.toast_panel.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::StatusUpdate;
    use nh_world::{MessageLog, NO_COLOR};

    fn status(fields: &[(&str, &str)]) -> Status {
        let mut s = Status::new();
        for (f, v) in fields {
            s.apply(&StatusUpdate {
                field: f.to_string(),
                value: Some(v.to_string()),
                conds: None,
                chg: 0,
                percent: 0,
                color: NO_COLOR,
            });
        }
        s
    }

    const START: [(&str, &str); 18] = [
        ("title", "Hero the Stripling            "),
        ("str", "16"),
        ("dex", "15"),
        ("con", "17"),
        ("int", "9"),
        ("wis", "11"),
        ("cha", "7"),
        ("align", "Neutral"),
        ("cap", ""),
        ("gold", "\\G0C9F0F2E:0"),
        ("energy", "2"),
        ("energymax", "2"),
        ("ac", "6"),
        ("time", "1"),
        ("hunger", ""),
        ("hp", "16"),
        ("hpmax", "16"),
        ("leveldesc", "Dlvl:1  "),
    ];

    #[test]
    fn the_place_and_the_tooltip_are_in_the_language_now() {
        let mut s = status(&START);
        s.apply(&StatusUpdate {
            field: "xlevel".into(),
            value: Some("1".into()),
            conds: None,
            chg: 0,
            percent: 0,
            color: NO_COLOR,
        });
        let v = StatusView::new(&s, &[]);
        assert_eq!(place_text(&v).as_deref(), Some("Dlvl 1"));
        let tip = status_tip(&v);
        for words in [
            "Hero the Stripling  ·  Neutral",
            "Dlvl 1",
            "Gold 0",
            "Lv 1",
            "T 1",
            "St 16",
        ] {
            assert!(tip.contains(words), "{words:?} not in {tip:?}");
        }
        crate::i18n::set_lang(crate::i18n::Lang::Ru);
        let place = place_text(&v);
        let tip = status_tip(&v);
        crate::i18n::set_lang(crate::i18n::Lang::En);
        assert_eq!(place.as_deref(), Some("Глубина 1"));
        for words in ["Глубина 1", "Золото 0", "Ур 1", "Ход 1", "Сил 16"] {
            assert!(tip.contains(words), "{words:?} not in {tip:?}");
        }
        // a place with a name of its own: the engine's words
        let home = StatusView::new(&status(&[("leveldesc", "Home 2")]), &[]);
        assert_eq!(place_text(&home).as_deref(), Some("Home 2"));
    }

    #[test]
    fn a_new_hero_reads_like_the_bottom_lines() {
        let mut s = status(&START);
        s.apply(&StatusUpdate {
            field: "xlevel".into(),
            value: Some("1".into()),
            conds: None,
            chg: 0,
            percent: 0,
            color: NO_COLOR,
        });
        let v = StatusView::new(&s, &[]);
        assert_eq!(v.title, "Hero the Stripling");
        assert_eq!(v.align.as_deref(), Some("Neutral"));
        assert_eq!(v.hp, Some((16, 16)));
        assert_eq!(v.pw, Some((2, 2)));
        assert_eq!(v.chips().count(), 0, "no hunger, no burden");
        assert_eq!(v.field("AC"), Some("6"));
        assert_eq!(v.field("$"), Some("0"));
        assert_eq!(v.place().as_deref(), Some("Dlvl 1"));
        let text = v.lines().join("\n");
        for part in [
            "HP:16(16)",
            "Pw:2(2)",
            "Dlvl:1",
            "$0",
            "AC:6",
            "XL:1",
            "T:1",
            "St:16 Dx:15 Co:17 In:9 Wi:11 Ch:7",
        ] {
            assert!(text.contains(part), "{part:?} missing from {text:?}");
        }
    }

    #[test]
    fn experience_points_come_with_the_level_and_hit_dice_replace_both() {
        let v = StatusView::new(&status(&[("xlevel", "3"), ("exp", "27")]), &[]);
        let tags: Vec<&str> = v.fields.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tags, ["XL", "Exp"]);
        assert_eq!(v.place(), None);
        let v = StatusView::new(&status(&[("hitdice", "4")]), &[]);
        assert_eq!(v.fields, [("HD".to_string(), "4".to_string())]);
        assert!(v.lines()[1].contains("HD:4"));
    }

    #[test]
    fn other_places_show_their_whole_name() {
        let v = StatusView::new(&status(&[("leveldesc", "Home 2 ")]), &[]);
        assert_eq!(v.fields, [(String::new(), "Home 2".to_string())]);
        assert_eq!(v.lines()[1], "Home 2");
        assert_eq!(v.place().as_deref(), Some("Home 2"));
    }

    #[test]
    fn hunger_burden_and_conditions_become_chips() {
        let s = status(&[("hunger", "Weak"), ("cap", "Burdened")]);
        let conds = ["Blind".to_string(), "Stone".to_string(), "Fly".to_string()];
        let v = StatusView::new(&s, &conds);
        let chips: Vec<(&str, Tone)> = v.chips().map(|c| (c.text.as_str(), c.tone)).collect();
        assert_eq!(
            chips,
            [
                ("Weak", Tone::Bad),
                ("Burdened", Tone::Warn),
                ("Blind", Tone::Warn),
                ("Stone", Tone::Deadly),
                ("Fly", Tone::Info),
            ]
        );
        assert_eq!(v.needs.len(), 2);
        assert_eq!(v.conditions.len(), 3);
        assert_eq!(v.lines().last().unwrap(), "Weak Burdened Blind Stone Fly");
    }

    #[test]
    fn fainting_is_deadly_weak_is_bad() {
        assert_eq!(hunger_tone("Hungry"), Tone::Warn);
        assert_eq!(hunger_tone("Weak"), Tone::Bad);
        assert_eq!(hunger_tone("Fainting"), Tone::Deadly);
        assert_eq!(hunger_tone("Fainted"), Tone::Deadly);
    }

    #[test]
    fn the_five_deadly_conditions_are_deadly() {
        for name in [
            "Stone",
            "Slime",
            "Strngl",
            "Strangled",
            "FoodPois",
            "TermIll",
        ] {
            assert_eq!(condition_tone(name), Tone::Deadly, "{name}");
            assert!(deadly_words(name).is_some(), "{name} has a banner");
        }
        for name in ["Blind", "Conf", "Lev", "Ride", "Unknown"] {
            assert_ne!(condition_tone(name), Tone::Deadly, "{name}");
            assert_eq!(deadly_words(name), None);
        }
    }

    #[test]
    fn experience_thresholds_follow_newuexp() {
        assert_eq!(newuexp(0), 0);
        assert_eq!(newuexp(1), 20);
        assert_eq!(newuexp(2), 40);
        assert_eq!(newuexp(9), 5120);
        assert_eq!(newuexp(10), 10_000);
        assert_eq!(newuexp(19), 5_120_000);
        assert_eq!(newuexp(20), 10_000_000);
        assert_eq!(xp_progress(1, 0), (0.0, 20));
        assert_eq!(xp_progress(1, 10), (0.5, 20));
        assert_eq!(xp_progress(3, 57), ((57.0 - 40.0) / 40.0, 80));
        assert_eq!(xp_progress(2, 500).0, 1.0);
    }

    #[test]
    fn low_hp_is_prayers_rule() {
        // five or fewer is always low
        assert!(critically_low_hp(5, 100, 10));
        // level 1-5: a fifth of the maximum
        assert!(critically_low_hp(6, 30, 3));
        assert!(!critically_low_hp(7, 30, 3));
        // level 6-13: a sixth
        assert!(critically_low_hp(10, 60, 8));
        assert!(!critically_low_hp(11, 60, 8));
        // a huge maximum counts as 15 per level
        assert!(critically_low_hp(6, 1000, 2));
        assert!(!critically_low_hp(16, 16, 1));
    }

    #[test]
    fn attributes_compare_with_their_percentile() {
        assert_eq!(attr_rank("16"), Some(16.0));
        assert!(attr_rank("18/50") > attr_rank("18/10"));
        assert!(attr_rank("18/**") > attr_rank("18/99"));
        assert!(attr_rank("19") > attr_rank("18/**").map(|v| v - 0.5));
        assert_eq!(attr_rank("x"), None);
    }

    #[test]
    fn the_threat_arrow_points_from_the_edge() {
        let view = Vector2::new(1920.0, 1080.0);
        // on screen: no arrow
        assert_eq!(edge_arrow(Some(Vector2::new(900.0, 500.0)), view), None);
        assert_eq!(edge_arrow(None, view), None);
        // far to the right: on the right edge, pointing right
        let (p, a) = edge_arrow(Some(Vector2::new(4000.0, 380.0)), view).unwrap();
        assert!((p.x - (1920.0 - ARROW_MARGIN)).abs() < 0.5);
        assert!(a.abs() < 0.1);
        // above: on the top edge, pointing up
        let (p, a) = edge_arrow(Some(Vector2::new(960.0, -900.0)), view).unwrap();
        assert!((p.y - ARROW_MARGIN).abs() < 0.5);
        assert!((a + std::f32::consts::FRAC_PI_2).abs() < 0.1);
    }

    #[test]
    fn the_threat_arrow_keeps_off_the_panels() {
        let view = Vector2::new(1920.0, 1080.0);
        let rect =
            |x: f32, y: f32, w: f32, h: f32| Rect2::new(Vector2::new(x, y), Vector2::new(w, h));
        // the portrait, the minimap and the badge under it
        let panels = [
            rect(24.0, 24.0, 520.0, 176.0),
            rect(1716.0, 24.0, 180.0, 150.0),
            rect(1656.0, 182.0, 240.0, 32.0),
        ];
        let on_panel = |p: Vector2| {
            panels
                .iter()
                .any(|r| r.grow(ARROW_PAD - 0.5).contains_point(p))
        };
        let in_frame = |p: Vector2| {
            (ARROW_MARGIN..=1920.0 - ARROW_MARGIN).contains(&p.x) && p.y >= ARROW_MARGIN
        };
        // a hostile far up and to the left: the edge there is the portrait's
        for target in [Vector2::new(-900.0, -400.0), Vector2::new(-2000.0, -500.0)] {
            let (p, a) = edge_arrow(Some(target), view).unwrap();
            assert!(on_panel(p), "{p:?}");
            let (q, b) = off_panels(p, a, target, view, &panels);
            assert!(!on_panel(q) && in_frame(q), "{q:?}");
            let d = target - q;
            assert!(
                (d.y.atan2(d.x) - b).abs() < 1e-3,
                "it points at the hostile"
            );
        }
        // up and to the right: clear of the minimap and of its badge
        let target = Vector2::new(2500.0, -200.0);
        let (p, a) = edge_arrow(Some(target), view).unwrap();
        assert!(on_panel(p), "{p:?}");
        let (q, _) = off_panels(p, a, target, view, &panels);
        assert!(!on_panel(q) && in_frame(q), "{q:?}");
        // a hostile in the frame under the portrait: its marker beside it
        let head = Vector2::new(300.0, 120.0);
        let (q, _) = off_panels(head, 1.5, Vector2::new(300.0, 190.0), view, &panels);
        assert!(!on_panel(q) && in_frame(q), "{q:?}");
        // clear of every panel: where it was
        let free = Vector2::new(900.0, ARROW_MARGIN);
        assert_eq!(off_panels(free, 0.3, target, view, &panels), (free, 0.3));
    }

    #[test]
    fn the_threat_arrow_keeps_off_the_hero() {
        let view = Vector2::new(1920.0, 1080.0);
        let rect =
            |x: f32, y: f32, w: f32, h: f32| Rect2::new(Vector2::new(x, y), Vector2::new(w, h));
        let down = std::f32::consts::FRAC_PI_2;
        // the hero in the middle of the screen, feet at y 550
        let hero = rect(915.0, 380.0, 90.0, 170.0);
        let on_hero = |p: Vector2| hero.grow(ARROW_PAD - 0.5).contains_point(p);
        // a hostile in the cell south of them: over its head is their
        // waist; the arrow goes beside them and points at it from there
        let target = Vector2::new(962.0, 610.0);
        let head = Vector2::new(target.x, target.y - 70.0);
        assert!(on_hero(head));
        let mut side = None;
        let (p, a) = off_hero(head, down, target, view, Some(hero), &mut side);
        assert!(!on_hero(p) && p.y == head.y && p.x > hero.end().x, "{p:?}");
        assert_eq!(side, Some(true), "the side the hostile leans to");
        let d = target - p;
        assert!(
            (d.y.atan2(d.x) - a).abs() < 1e-3,
            "it points at the hostile"
        );
        // the hostile sways past their middle: the arrow stays on its side
        let swayed = Vector2::new(955.0, 610.0);
        let head = Vector2::new(swayed.x, swayed.y - 70.0);
        let (q, _) = off_hero(head, down, swayed, view, Some(hero), &mut side);
        assert!(q.x > hero.end().x && side == Some(true), "{q:?}");
        // no room on that side of the frame: the other
        let by_edge = rect(1800.0, 380.0, 90.0, 170.0);
        let (mut side, target) = (None, Vector2::new(1850.0, 610.0));
        let head = Vector2::new(target.x, target.y - 70.0);
        let (q, _) = off_hero(head, down, target, view, Some(by_edge), &mut side);
        assert!(q.x < by_edge.position.x && side == Some(false), "{q:?}");
        // a hostile north of the hero: its arrow is clear of them where it
        // is, and no side is kept
        let mut side = Some(true);
        let target = Vector2::new(960.0, 300.0);
        let head = Vector2::new(target.x, target.y - 70.0);
        let clear = off_hero(head, down, target, view, Some(hero), &mut side);
        assert_eq!((clear, side), ((head, down), None));
        // a hero not on screen
        let off = off_hero(head, down, target, view, None, &mut side);
        assert_eq!(off, (head, down));
    }

    #[test]
    fn the_log_docks_when_the_canvas_is_narrow() {
        assert!(!compact(1920.0));
        assert!(compact(1600.0));
        assert!(!compact(2560.0));
    }

    #[test]
    fn the_docked_log_sits_on_the_xp_bar_beside_the_hp_orb() {
        let (left, bottom) = compact_log_corner();
        // the bar's left end, past the HP orb and its gap
        assert_eq!(left, -CLUSTER_W / 2.0 + orb::SIZE + 4.0);
        // the micro-buttons' bottom, 6 above the XP bar
        assert_eq!(bottom, -CLUSTER_BOTTOM - BAR_H - 6.0 - 22.0 - 6.0);
        // clear of the micro-buttons at the bar's right end
        let n = MICRO_BUTTONS as f32;
        let micro_left = left + BAR_W - (n * MICRO_W + (n - 1.0) * MICRO_GAP as f32);
        assert!(left + LOG_COMPACT_W + 12.0 <= micro_left);
    }

    #[test]
    fn forms_read_as_titles() {
        assert_eq!(title_case("valkyrie"), "Valkyrie");
        assert_eq!(title_case("wood nymph"), "Wood Nymph");
    }

    #[test]
    fn the_mode_banner_holds_then_fades() {
        assert_eq!(flash_alpha(-0.1), 0.0);
        assert_eq!(flash_alpha(0.0), 1.0);
        assert_eq!(flash_alpha(FLASH_SECS * 0.5), 1.0);
        let late = flash_alpha(FLASH_SECS - FLASH_FADE * 0.5);
        assert!(late > 0.0 && late < 1.0, "{late}");
        assert_eq!(flash_alpha(FLASH_SECS), 0.0);
    }

    #[test]
    fn messages_after_the_last_input_are_new() {
        let mut log = MessageLog::new();
        log.push("restored".into(), 0, None, true);
        log.push("old".into(), 0, Some(1), false);
        log.push("You die...".into(), nh_world::ATR_URGENT, Some(2), false);
        log.push("fresh".into(), 0, Some(2), false);
        let looks: Vec<Look> = log.iter().map(|m| message_look(m, 2)).collect();
        let at = |new, urgent| Look { new, urgent };
        assert_eq!(
            looks,
            [
                at(false, false),
                at(false, false),
                at(true, true),
                at(true, false)
            ]
        );
        // restored history is never new
        assert!(!message_look(log.iter().next().unwrap(), 0).new);
    }
}
