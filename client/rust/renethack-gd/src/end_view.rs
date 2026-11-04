//! The end screen, met at every game's end: the tombstone NetHack carves
//! (a quit's put together the same way), or for an ascension or an escape
//! a page of its own, beside the hero and how the game went (NetHack's
//! summary, the turns, the deepest level, the score, the place in the top
//! ten, gold and level), the achievements this game holds (the ones first
//! earned in it marked) and the conduct kept; then the chronicle's major
//! events and the top ten as a table, this game's row lit. Under them a
//! new game, the title or quitting, for the mouse, the keys and a gamepad.

use std::collections::BTreeMap;

use godot::builtin::{Corner, Side};
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;

use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, ColorRect, Control, GridContainer, HBoxContainer, HFlowContainer, Label,
    MarginContainer, PanelContainer, ScrollContainer, StyleBoxFlat, TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_world::achievements::Achievement;

use crate::end_disclosed;
use crate::gamepad::{PadButton, PadKind};
use crate::i18n::{self, EngineKind, Lang};
use crate::icons::{self, Glyph};
use crate::layouts::{self, Tombstone};
use crate::pad_view;
use crate::theme::{self, Face, Frame};
use crate::tombstone;
use crate::tr;
use crate::ui_events::{UiEvent, UiQueue};

/// How a game ended, as the end screen tells it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EndHow {
    /// Every death; a panic and a trick too.
    #[default]
    Died,
    Quit,
    Escaped,
    Ascended,
}

impl EndHow {
    /// From the progress notice's `how`.
    pub fn of(how: Option<&str>) -> EndHow {
        match how {
            Some("quit") => EndHow::Quit,
            Some("escaped") => EndHow::Escaped,
            Some("ascended") => EndHow::Ascended,
            _ => EndHow::Died,
        }
    }
}

/// A row of the top ten as topten.c lists it: the place (none for this
/// game's entry when it did not make the list), the points, the engine's
/// words (the hero's name and code, what became of them and where, the
/// killer), the hit points and their most; this game's (or the hero's
/// best, which it did not beat) or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreRow {
    pub rank: Option<u32>,
    pub points: i64,
    pub words: String,
    pub hp: String,
    pub mine: bool,
}

/// Everything the end screen shows, gathered as the game ended. The
/// engine's words stay English: the page says them in the language now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndSummary {
    pub how: EndHow,
    /// The hero's name as the engine has it, and whether the hero was
    /// female.
    pub name: String,
    pub female: bool,
    /// The status line's title ("Hero the Stripling"), the role in the
    /// hero's gender ("Valkyrie") and the alignment ("Neutral").
    pub title: String,
    pub role: String,
    pub align: String,
    /// The end window's lines: the stone NetHack carved (a death's) and
    /// its summary ("You died in…"); none when the player skipped it.
    pub text: Vec<String>,
    /// The log's last messages: never shown, they tell a self-test how
    /// the game ended.
    pub last_messages: Vec<String>,
    /// What the game said as the hero died (`final_words`), for the
    /// death's beat.
    pub final_words: Vec<nh_world::Message>,
    /// The lines raw_print printed (the top ten), true: in bold.
    pub printed: Vec<(String, bool)>,
    /// What the game last told: the turn, the deepest level, the score;
    /// the status line's gold and experience level.
    pub turns: Option<i64>,
    pub deepest: Option<i64>,
    pub score: Option<i64>,
    pub gold: Option<i64>,
    pub level: Option<i64>,
    /// The year the game ended in (the stone's).
    pub year: i32,
    /// The conduct kept (the catalog's keys).
    pub conduct: Vec<&'static str>,
    /// The chronicle's major events: the turn, the engine's words.
    pub chronicle: Vec<(i64, String)>,
    /// The achievements this game holds; true: first earned in it.
    pub achievements: Vec<(Achievement, bool)>,
    /// What NetHack disclosed at the end, in its order (the possessions,
    /// the attributes, the creatures vanquished and genocided, the
    /// conduct, the overview).
    pub disclosed: Vec<Disclosure>,
    /// The roles by their codes, as the top ten names its heroes
    /// ("Hero-Sam-Hum-Mal-Law"): the role, and the role of a heroine if
    /// it has its own.
    pub roles: Vec<(String, String, Option<String>)>,
}

pub use crate::end_disclosed::{DisclosedItem, Disclosure, Sections};

/// The numbers the page shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Numbers {
    pub turns: Option<i64>,
    pub deepest: Option<i64>,
    pub score: Option<i64>,
    pub place: Option<u32>,
    pub gold: Option<i64>,
    pub level: Option<i64>,
}

impl EndSummary {
    /// The stone NetHack carved (a death's).
    pub fn stone(&self) -> Option<Tombstone> {
        layouts::tombstone(&self.text).map(|(t, _)| t)
    }

    pub fn scores(&self) -> Vec<ScoreRow> {
        score_rows(&self.printed)
    }

    /// This game's place in the top ten (none when it did not make it).
    pub fn place(&self) -> Option<u32> {
        place(&self.scores())
    }

    /// The numbers: the end window's where it tells them (they are final
    /// there), else what the game last told.
    pub fn numbers(&self) -> Numbers {
        let told = summary_numbers(&self.text);
        Numbers {
            turns: told.moves.or(self.turns),
            deepest: self.deepest,
            score: told.score.or(self.score),
            place: self.place(),
            gold: told.gold.or(self.stone().map(|t| t.gold)).or(self.gold),
            level: told.level.or(self.level),
        }
    }

    /// NetHack's summary below the stone, as written: without the stone,
    /// the farewell and the blank lines.
    pub fn summary(&self) -> Vec<&str> {
        let skip = layouts::tombstone(&self.text).map(|(_, at)| at);
        self.text
            .iter()
            .enumerate()
            .filter(|(i, _)| skip.as_ref().is_none_or(|r| !r.contains(i)))
            .map(|(_, l)| l.as_str())
            .filter(|l| !l.trim().is_empty() && !farewell(l))
            .collect()
    }

    /// What the engine said of the list: made it, beat or not the hero's
    /// best, not kept in debug mode.
    pub fn topten_notes(&self) -> Vec<&str> {
        let end = self
            .printed
            .iter()
            .position(|(l, _)| l.starts_with(HEADER))
            .unwrap_or(self.printed.len());
        self.printed[..end]
            .iter()
            .map(|(l, _)| l.trim())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

/// really_done's farewell, "Goodbye Hero the Valkyrie..." (or Farvel,
/// Aloha, Sayonara…): the page's title says it.
pub fn farewell(line: &str) -> bool {
    let l = line.trim();
    l.ends_with("...") && l.contains(" the ") && !l.starts_with("You")
}

/// This game's place among `rows`: its own row's, none when its row has
/// no place (the hero's best stood, or the list is longer).
pub fn place(rows: &[ScoreRow]) -> Option<u32> {
    let mine: Vec<&ScoreRow> = rows.iter().filter(|r| r.mine).collect();
    if mine.iter().any(|r| r.rank.is_none()) {
        return None;
    }
    mine.first().and_then(|r| r.rank)
}

/// The numbers really_done's summary tells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Told {
    pub score: Option<i64>,
    pub gold: Option<i64>,
    pub moves: Option<i64>,
    pub level: Option<i64>,
    pub hpmax: Option<i64>,
}

/// The numbers in the end window's summary: "… with 312 points,", "and 0
/// pieces of gold, after 540 moves.", "You were level 1 with a maximum of
/// 16 hit points when you died."
pub fn summary_numbers<S: AsRef<str>>(lines: &[S]) -> Told {
    let mut told = Told::default();
    for line in lines {
        let l = line.as_ref();
        told.score = told.score.or(number_between(l, " with ", " point"));
        told.gold = told.gold.or(number_between(l, "and ", " piece"));
        told.moves = told.moves.or(number_between(l, "after ", " move"));
        if l.trim_start().starts_with("You were level ") {
            told.level = told.level.or(number_between(l, "level ", " with"));
            told.hpmax = told
                .hpmax
                .or(number_between(l, "maximum of ", " hit point"));
        }
    }
    told
}

/// The number `before` it and `after` it in `line`.
fn number_between(line: &str, before: &str, after: &str) -> Option<i64> {
    line.match_indices(before).find_map(|(at, _)| {
        let rest = &line[at + before.len()..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        (digits > 0 && rest[digits..].starts_with(after))
            .then(|| rest[..digits].parse().ok())
            .flatten()
    })
}

/// topten.c's header, the line the list starts after.
const HEADER: &str = " No  Points     Name";
/// Where an entry's words start on each of its lines (`%3d %10ld  `).
const INDENT: usize = 16;

/// The top ten's rows from the lines the engine printed (raw_print, true:
/// in bold, this game's). An entry runs over lines; its hit points end
/// its last one ("-  [16]").
pub fn score_rows(lines: &[(String, bool)]) -> Vec<ScoreRow> {
    let Some(start) = lines.iter().position(|(l, _)| l.starts_with(HEADER)) else {
        return Vec::new();
    };
    let number = |s: &str| {
        let s = s.trim();
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .then(|| s.parse::<i64>().ok())
            .flatten()
    };
    let mut rows: Vec<ScoreRow> = Vec::new();
    for (line, bold) in &lines[start + 1..] {
        let (Some(head), Some(words)) = (line.get(..INDENT), line.get(INDENT..)) else {
            continue;
        };
        let words = words.trim();
        // "%3d" (blank for an entry without a place) and " %10ld  "
        let points = number(&head[4..14]).filter(|_| head[14..].trim().is_empty());
        let rank = number(&head[..3]).and_then(|r| u32::try_from(r).ok());
        match points {
            Some(points) if head[..3].trim().is_empty() || rank.is_some() => {
                rows.push(ScoreRow {
                    rank,
                    points,
                    words: words.to_string(),
                    hp: String::new(),
                    mine: *bold,
                });
            }
            // the entry's words go on
            None if head.trim().is_empty() && !words.is_empty() => {
                if let Some(r) = rows.last_mut() {
                    r.words.push(' ');
                    r.words.push_str(words);
                    r.mine |= *bold;
                }
            }
            _ => {}
        }
    }
    // the hit points and their most end each entry
    for r in &mut rows {
        let (words, hp) = split_hp(&r.words);
        r.hp = hp;
        r.words = words;
    }
    rows
}

/// An entry's words and its hit points ("… jackal.  -  [16]" → "…
/// jackal.", "- [16]").
fn split_hp(words: &str) -> (String, String) {
    let t = words.trim_end();
    let whole = || (t.to_string(), String::new());
    let Some(open) = t.rfind('[') else {
        return whole();
    };
    let most = &t[open..];
    if !most.ends_with(']') || !most[1..most.len() - 1].chars().all(|c| c.is_ascii_digit()) {
        return whole();
    }
    let before = t[..open].trim_end();
    match before.rfind("  ") {
        Some(k) => (
            before[..k].trim_end().to_string(),
            format!("{} {most}", before[k..].trim()),
        ),
        None => whole(),
    }
}

/// The conduct kept, by NetHack's counters (`u.uconduct`): the strongest
/// of the food conducts, the wishes' too; Sokoban's only once it was
/// entered.
pub fn conduct_kept(conduct: &BTreeMap<String, i64>, sokoban: bool) -> Vec<&'static str> {
    let kept = |c: &str| conduct.get(c) == Some(&0);
    let mut out = Vec::new();
    if kept("food") {
        out.push("end-conduct-foodless");
    } else if kept("unvegan") {
        out.push("end-conduct-vegan");
    } else if kept("unvegetarian") {
        out.push("end-conduct-vegetarian");
    }
    for (c, key) in [
        ("gnostic", "end-conduct-atheist"),
        ("weaphit", "end-conduct-weaponless"),
        ("killer", "end-conduct-pacifist"),
        ("literate", "end-conduct-illiterate"),
        ("polypiles", "end-conduct-polypileless"),
        ("polyselfs", "end-conduct-polyselfless"),
    ] {
        if kept(c) {
            out.push(key);
        }
    }
    if kept("wishes") {
        out.push("end-conduct-wishless");
    } else if kept("wisharti") {
        out.push("end-conduct-artiwishless");
    }
    if sokoban && kept("sokocheat") {
        out.push("end-conduct-sokoban");
    }
    if kept("pets") {
        out.push("end-conduct-petless");
    }
    out
}

/// The page at most this wide, in design pixels; its margins at least.
const MAX_WIDTH: f32 = 1720.0;
const MARGIN: f32 = 28.0;
/// The records' column's width to the story's (the disclosures' columns
/// below keep it).
const RECORDS_RATIO: f32 = 1.15;
/// The canvas's height the page needs besides its body: margins, the
/// title, the buttons and their hints.
const CHROME: f32 = 280.0;

/// How long the beat lasts, in seconds: the game darkens behind the words
/// that ended it, the stone fades in, then the rest of the page. No input
/// counts before it is over.
pub const BEAT_SECS: f32 = 1.55;
/// When each part of the beat comes and is whole, in seconds: the dark
/// over the game, the words, the stone, the rest of the page (the words
/// fade as it comes).
const BACKDROP: (f32, f32) = (0.0, 0.45);
const WORDS_IN: (f32, f32) = (0.15, 0.45);
const STONE_IN: (f32, f32) = (0.4, 1.0);
const PAGE_IN: (f32, f32) = (1.25, BEAT_SECS);

/// How far along `span` the time `t` is, eased: 0 before, 1 after.
fn ramp(t: f32, (from, to): (f32, f32)) -> f32 {
    let x = ((t - from) / (to - from).max(1e-3)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The end's beat on the page as built: what fades in and out of it.
pub struct Beat {
    /// The words that ended the game, where the story will be.
    words: Gd<Control>,
    /// The stone (a quit's, an escape's, an ascension's page).
    stone: Gd<Control>,
    /// The rest of the page: its title, rules, body, buttons and hints.
    rest: Vec<Gd<Control>>,
    /// The frame (its own drawing only: its children are above).
    panel: Gd<PanelContainer>,
    /// Over the page while it lasts: no click on a button not yet seen.
    blocker: Gd<Control>,
    over: bool,
}

impl Beat {
    /// The page `t` seconds into the beat; how dark the backdrop over the
    /// game is then (1: the game no longer shows).
    pub fn show(&mut self, t: f32) -> f32 {
        if self.over {
            return 1.0;
        }
        let fade = |c: &mut Gd<Control>, a: f32| {
            let mut m = c.get_modulate();
            m.a = a;
            c.set_modulate(m);
        };
        let page = ramp(t, PAGE_IN);
        fade(&mut self.words, ramp(t, WORDS_IN) * (1.0 - page));
        fade(&mut self.stone, ramp(t, STONE_IN));
        for c in &mut self.rest {
            fade(c, page);
        }
        let mut m = self.panel.get_self_modulate();
        m.a = page;
        self.panel.set_self_modulate(m);
        if t >= BEAT_SECS {
            self.over = true;
            self.words.queue_free();
            self.blocker.queue_free();
        }
        ramp(t, BACKDROP)
    }

    pub fn is_over(&self) -> bool {
        self.over
    }
}

/// What the game said last, from the log's last messages (newest last):
/// up to two lines of the last turn, without the "You die..." that the
/// beat says in its own words. For a death, the killer's line ("The
/// soldier ant bites!", "You drown."); for an ascension, the choir.
pub fn final_words(log: &[nh_world::Message]) -> Vec<nh_world::Message> {
    let said = |m: &&nh_world::Message| !m.history_only && !m.from_history;
    let Some(last) = log.iter().rposition(|m| said(&m)) else {
        return Vec::new();
    };
    let turn = log[last].turn;
    let restates = |t: &str| {
        let t = t.trim();
        t.is_empty()
            || t == "You die..."
            || t == "You turn to stone..."
            || t.starts_with("Do not pass Go")
    };
    let mut out: Vec<nh_world::Message> = log[..=last]
        .iter()
        .rev()
        .take_while(|m| m.turn == turn)
        .filter(|m| said(m) && !restates(&m.text))
        .take(2)
        .cloned()
        .collect();
    out.reverse();
    out
}

/// The end page as built: its controls for the screens' keys and pad.
pub struct EndPage {
    pub root: Gd<Control>,
    /// The frame, for the self-tests' measures.
    pub panel: Gd<PanelContainer>,
    /// A new game (the focus's first), the title, quit.
    pub buttons: Vec<Gd<Control>>,
    /// The body, scrolled by the keys and the pad.
    pub scroll: Gd<ScrollContainer>,
    /// Where the hints go, the keys' or the pad's.
    pub foot: Gd<HBoxContainer>,
    /// NetHack's disclosures, a part a tab.
    pub sections: Option<end_disclosed::Sections>,
    /// The beat it opens with (none when drawn again, in another
    /// language).
    pub beat: Option<Beat>,
}

/// The end page for `s` on a canvas of `canvas`, its hints for `pad`;
/// `beat`: it opens with the end's beat.
pub fn build(
    s: &EndSummary,
    queue: &UiQueue,
    canvas: Vector2,
    pad: Option<PadKind>,
    beat: bool,
) -> EndPage {
    let mut root = MarginContainer::new_alloc();
    theme::full_rect_ignore(&root);
    let side = MARGIN.max((canvas.x - MAX_WIDTH) / 2.0).round() as i32;
    for (edge, px) in [("left", side), ("right", side), ("top", 18), ("bottom", 18)] {
        root.add_theme_constant_override(&format!("margin_{edge}"), px);
    }
    let mut panel = theme::framed(Frame::Panel);
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 10);
    let title = match s.how {
        EndHow::Ascended => tr!("end-title-ascended", female = female(s)),
        EndHow::Escaped => tr!("end-title-escaped", female = female(s)),
        EndHow::Died | EndHow::Quit => tr!("end-title"),
    };
    let mut heading = theme::styled_label(&title, Face::Title, 34, theme::ACCENT);
    heading.set_horizontal_alignment(HorizontalAlignment::CENTER);
    col.add_child(&heading);
    let top_rule = rule(0.5);
    col.add_child(&top_rule);
    // the body: the stone, still, beside the hero's story and the
    // records, scrolled when they are longer than the canvas, centred
    // when shorter
    let mut body = HBoxContainer::new_alloc();
    body.add_theme_constant_override("separation", 34);
    body.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let scale = ((canvas.y - CHROME) / tombstone::SIZE.y).clamp(0.6, 1.2);
    let mut stone = marker(s, scale);
    stone.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    body.add_child(&stone);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_follow_focus(false);
    let mut middle = VBoxContainer::new_alloc();
    middle.set_alignment(AlignmentMode::CENTER);
    middle.set_h_size_flags(SizeFlags::EXPAND_FILL);
    middle.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let mut columns = HBoxContainer::new_alloc();
    columns.add_theme_constant_override("separation", 34);
    let mut story = story(s);
    story.set_stretch_ratio(1.0);
    columns.add_child(&story);
    let mut records = records(s);
    records.set_stretch_ratio(RECORDS_RATIO);
    columns.add_child(&records);
    // clear of the scroll bar, the last line above the body's bottom
    let mut room = MarginContainer::new_alloc();
    for (edge, px) in [("left", 0), ("right", 18), ("top", 6), ("bottom", 16)] {
        room.add_theme_constant_override(&format!("margin_{edge}"), px);
    }
    // NetHack's disclosures, under the hero's story and the records, a
    // part at a time, its columns as theirs
    let mut parts = VBoxContainer::new_alloc();
    parts.add_theme_constant_override("separation", 22);
    parts.add_child(&columns);
    let sections = end_disclosed::sections(&s.disclosed, RECORDS_RATIO);
    if let Some(told) = sections.as_ref() {
        parts.add_child(&told.root);
    }
    room.add_child(&parts);
    middle.add_child(&room);
    scroll.add_child(&middle);
    // the beat's words where the story and the records will be
    let mut stack = MarginContainer::new_alloc();
    stack.set_h_size_flags(SizeFlags::EXPAND_FILL);
    stack.add_child(&scroll);
    let words = beat.then(|| {
        let w = beat_words(s);
        stack.add_child(&w);
        w
    });
    body.add_child(&stack);
    col.add_child(&body);
    let bottom_rule = rule(0.5);
    col.add_child(&bottom_rule);
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 16);
    let mut made: Vec<Gd<Control>> = Vec::new();
    for (key, ev) in [
        ("end-new-game", UiEvent::NewGame),
        ("end-to-title", UiEvent::BackToTitle),
        ("end-quit", UiEvent::QuitApp),
    ] {
        let mut b: Gd<Button> = theme::button(&i18n::tr(key), queue, ev);
        b.set_custom_minimum_size(Vector2::new(220.0, 46.0));
        row.add_child(&b);
        made.push(b.upcast());
    }
    col.add_child(&row);
    let mut foot = HBoxContainer::new_alloc();
    foot.set_alignment(AlignmentMode::CENTER);
    foot.set_mouse_filter(MouseFilter::IGNORE);
    show_hints(&mut foot, pad, sections.is_some());
    col.add_child(&foot);
    panel.add_child(&col);
    root.add_child(&panel);
    let beat = words.map(|words| {
        let mut blocker = Control::new_alloc();
        blocker.set_mouse_filter(MouseFilter::STOP);
        root.add_child(&blocker);
        let mut b = Beat {
            words: words.upcast(),
            stone: stone.clone(),
            rest: vec![
                heading.upcast(),
                top_rule.upcast(),
                scroll.clone().upcast(),
                bottom_rule.upcast(),
                row.upcast(),
                foot.clone().upcast(),
            ],
            panel: panel.clone(),
            blocker,
            over: false,
        };
        b.show(0.0);
        b
    });
    EndPage {
        root: root.upcast(),
        panel,
        buttons: made,
        scroll,
        foot,
        sections,
        beat,
    }
}

/// The beat's words: what the game said as the hero died, then how the
/// game ended («Вы погибли — убита муравьём-солдатом»), centred.
fn beat_words(s: &EndSummary) -> Gd<Control> {
    let mut col = VBoxContainer::new_alloc();
    col.set_alignment(AlignmentMode::CENTER);
    col.add_theme_constant_override("separation", 16);
    col.set_mouse_filter(MouseFilter::IGNORE);
    // what killed the hero (an ascension's god, not its "You ascend…",
    // which the line under it says): an end the player chose says none
    let told = matches!(s.how, EndHow::Died | EndHow::Ascended);
    let shown = (s.final_words.iter())
        .filter(|_| told)
        .filter(|m| !m.text.starts_with("You ascend"));
    for m in shown {
        let said = layouts::upstart(&i18n::engine_message(m));
        let mut l = wrapped(&said, Face::Body, 22, theme::TEXT_DIM);
        l.set_horizontal_alignment(HorizontalAlignment::CENTER);
        col.add_child(&l);
    }
    let (line, color) = beat_line(s);
    let mut l = wrapped(&line, Face::TitleBold, 44, color);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    col.add_child(&l);
    col.upcast()
}

/// How the game ended, as the beat says it, and in what colour.
pub fn beat_line(s: &EndSummary) -> (String, Color) {
    let f = female(s);
    match s.how {
        EndHow::Died => {
            let cause = death_words(s);
            if cause.is_empty() {
                (tr!("end-beat-died-bare"), theme::HP)
            } else {
                (tr!("end-beat-died", cause = cause), theme::HP)
            }
        }
        EndHow::Quit => (tr!("end-beat-quit", female = f), theme::TEXT),
        EndHow::Escaped => (tr!("end-beat-escaped", female = f), theme::TEXT),
        EndHow::Ascended => (tr!("end-beat-ascended", female = f), theme::GOLD_BRIGHT),
    }
}

/// A death's words as the stone has them, else as the chronicle ends
/// (formatkiller's words both), in the language now.
fn death_words(s: &EndSummary) -> String {
    match s.stone() {
        Some(t) if !t.death.is_empty() => layouts::death(&t.death),
        _ => (s.chronicle.last())
            .map(|(_, t)| layouts::death(t))
            .unwrap_or_default(),
    }
}

/// The hints under the buttons, the keys' or the pad's; with `sections`,
/// how to go through the disclosures' tabs.
pub fn show_hints(foot: &mut Gd<HBoxContainer>, pad: Option<PadKind>, sections: bool) {
    for mut c in foot.get_children().iter_shared() {
        foot.remove_child(&c);
        c.queue_free();
    }
    match pad {
        Some(kind) => {
            use PadButton::*;
            let mut parts: Vec<(&[PadButton], String)> = vec![
                (&[Up, Down, Lb, Rb], tr!("hint-scroll")),
                (&[Left, Right], tr!("hint-choose")),
            ];
            if sections {
                parts.push((&[X, Y], tr!("hint-section")));
            }
            parts.extend([(&[A][..], tr!("hint-ok")), (&[B], tr!("end-to-title"))]);
            foot.add_child(&pad_view::buttons_row(kind, &parts, 15));
        }
        None => {
            let mut hint = tr!("end-hint");
            if sections {
                hint = format!("{hint} · {}", tr!("end-hint-sections"));
            }
            let mut l =
                theme::styled_label(&i18n::whole_parts(&hint), Face::Body, 15, theme::TEXT_DIM);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            foot.add_child(&l);
        }
    }
}

/// A thin gold line across.
fn rule(alpha: f32) -> Gd<ColorRect> {
    let mut r = ColorRect::new_alloc();
    r.set_color(Color {
        a: alpha,
        ..theme::GOLD_DIM
    });
    r.set_custom_minimum_size(Vector2::new(0.0, 1.0));
    r.set_mouse_filter(MouseFilter::IGNORE);
    r
}

/// The hero's gender for Fluent's selectors.
fn female(s: &EndSummary) -> &'static str {
    if s.female { "yes" } else { "no" }
}

/// The rank under the name on the stone: the status title in the
/// language now without the name ("the Stripling", «Новобранка»).
fn rank_of(s: &EndSummary) -> Option<String> {
    let title = s.title.trim();
    let name = s.name.trim().to_lowercase();
    if title.is_empty() || name.is_empty() {
        return None;
    }
    let shown = i18n::engine_title(title).into_owned();
    let at = shown.to_lowercase().find(&name)?;
    let rest = format!("{}{}", &shown[..at], &shown[at + name.len()..]);
    let rest = rest.trim_matches(|c: char| c.is_whitespace() || c == ',');
    (!rest.is_empty()).then(|| rest.to_string())
}

/// The stone (a death, a quit) or the page (an ascension, an escape), at
/// `scale` of its size.
fn marker(s: &EndSummary, scale: f32) -> Gd<Control> {
    let mut holder = Control::new_alloc();
    holder.set_mouse_filter(MouseFilter::IGNORE);
    holder.set_custom_minimum_size(tombstone::SIZE * scale);
    let rank = rank_of(s);
    let mut inner = match s.how {
        EndHow::Died => {
            let mut stone = s.stone().unwrap_or_else(|| Tombstone {
                name: s.name.clone(),
                gold: s.numbers().gold.unwrap_or(0),
                death: String::new(),
                year: s.year,
            });
            // the stone has room for 16 bytes: a Cyrillic name is cut at 8
            // letters, the hero's whole name carved instead
            if stone.name.len() < s.name.len()
                && s.name
                    .to_lowercase()
                    .starts_with(&stone.name.to_lowercase())
            {
                stone.name = s.name.clone();
            }
            tombstone::view_with(&stone, &death_words(s), rank.as_deref())
        }
        // NetHack carves no stone for a quit (outrip only for a death)
        EndHow::Ascended | EndHow::Escaped | EndHow::Quit => honour(s, rank.as_deref()),
    };
    inner.set_scale(Vector2::new(scale, scale));
    holder.add_child(&inner);
    holder.upcast()
}

/// An ascension's, an escape's or a quit's page, the stone's size and
/// shape: an emblem, the name and rank, what the hero did, the gold, the
/// year. A quit's is the night's: the hero laid the quest down.
fn honour(s: &EndSummary, rank: Option<&str>) -> Gd<Control> {
    let ascended = s.how == EndHow::Ascended;
    let quit = s.how == EndHow::Quit;
    let mut page = PanelContainer::new_alloc();
    page.set_custom_minimum_size(tombstone::SIZE);
    page.set_size(tombstone::SIZE);
    page.set_mouse_filter(MouseFilter::IGNORE);
    let glow = if ascended {
        theme::GOLD_BRIGHT
    } else if quit {
        theme::TEXT_DIM
    } else {
        theme::TEXT
    };
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(if ascended {
        Color::from_rgba(0.2, 0.145, 0.06, 1.0)
    } else if quit {
        Color::from_rgba(0.06, 0.06, 0.075, 1.0)
    } else {
        Color::from_rgba(0.075, 0.09, 0.105, 1.0)
    });
    sb.set_border_width_all(3);
    sb.set_border_color(if ascended {
        theme::GOLD
    } else {
        theme::GOLD_DIM
    });
    // an arch over the page, as over the stone
    for (corner, r) in [
        (Corner::TOP_LEFT, 140),
        (Corner::TOP_RIGHT, 140),
        (Corner::BOTTOM_LEFT, 6),
        (Corner::BOTTOM_RIGHT, 6),
    ] {
        sb.set_corner_radius(corner, r);
    }
    sb.set_corner_detail(16);
    sb.set_shadow_size(if ascended { 22 } else { 10 });
    sb.set_shadow_color(Color { a: 0.3, ..glow });
    for (side, px) in [
        (Side::LEFT, 26.0),
        (Side::RIGHT, 26.0),
        (Side::TOP, 64.0),
        (Side::BOTTOM, 30.0),
    ] {
        sb.set_content_margin(side, px);
    }
    page.add_theme_stylebox_override("panel", &sb);
    let mut col = VBoxContainer::new_alloc();
    col.set_alignment(AlignmentMode::CENTER);
    col.add_theme_constant_override("separation", 14);
    col.set_mouse_filter(MouseFilter::IGNORE);
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(120.0, 120.0));
    icon.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    icon.set_texture(&icons::emblem(if ascended {
        Glyph::Ascension
    } else if quit {
        Glyph::Moon
    } else {
        Glyph::Stairs
    }));
    icon.set_modulate(glow);
    col.add_child(&icon);
    let width = tombstone::SIZE.x - 52.0;
    let deed = if ascended {
        tr!("end-ascended", female = female(s))
    } else if quit {
        layouts::upstart(&tr!("end-stone-quit", female = female(s)))
    } else {
        tr!("end-escaped", female = female(s))
    };
    let mut words: Vec<(String, Face, i32, Color)> = vec![(
        layouts::typed(&s.name).to_uppercase(),
        Face::TitleBold,
        if s.name.chars().count() > 10 { 24 } else { 30 },
        glow,
    )];
    if let Some(r) = rank {
        words.push((r.to_string(), Face::Title, 18, theme::TEXT_DIM));
    }
    // the page's one face, the stone's (small capitals)
    words.push((deed, Face::Title, 22, theme::TEXT));
    if let Some(g) = s.numbers().gold {
        words.push((tr!("rip-gold", gold = g), Face::Title, 19, theme::GOLD));
    }
    words.push((s.year.to_string(), Face::Title, 22, theme::TEXT_DIM));
    for (text, face, size, color) in words {
        let mut l = theme::styled_label(&text, face, size, color);
        l.set_horizontal_alignment(HorizontalAlignment::CENTER);
        l.set_autowrap_mode(AutowrapMode::WORD_SMART);
        l.set_custom_minimum_size(Vector2::new(width, 0.0));
        l.set_mouse_filter(MouseFilter::IGNORE);
        col.add_child(&l);
    }
    page.add_child(&col);
    page.upcast()
}

/// A label that wraps in its column.
fn wrapped(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = theme::styled_label(text, face, size, color);
    l.set_autowrap_mode(AutowrapMode::WORD_SMART);
    l.set_h_size_flags(SizeFlags::EXPAND_FILL);
    l
}

/// A section's heading over a thin line.
fn section(text: &str) -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 3);
    col.add_child(&theme::styled_label(text, Face::Caps, 17, theme::GOLD));
    col.add_child(&rule(0.7));
    col
}

/// The hero and how the game went: the title and the role, NetHack's
/// summary, the numbers, the achievements, the conduct.
fn story(s: &EndSummary) -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 10);
    col.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let title = if s.title.trim().is_empty() {
        layouts::typed(&s.name)
    } else {
        i18n::engine_title(s.title.trim()).into_owned()
    };
    col.add_child(&wrapped(&title, Face::TitleBold, 30, theme::GOLD_BRIGHT));
    let who: Vec<String> = [
        (!s.role.is_empty()).then(|| i18n::engine_hero_role(&s.role).into_owned()),
        (!s.align.is_empty()).then(|| i18n::engine_hero_word(&s.align).into_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !who.is_empty() {
        col.add_child(&wrapped(
            &who.join("  ·  "),
            Face::Body,
            17,
            theme::TEXT_DIM,
        ));
    }
    // NetHack's own summary, in the language now
    let lines = s.summary();
    if !lines.is_empty() {
        let shown = i18n::engine_window(&lines)
            .unwrap_or_else(|| lines.iter().map(|l| l.to_string()).collect());
        let prose = shown
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        col.add_child(&wrapped(&prose, Face::Body, 17, theme::TEXT));
    }
    // the numbers, three a row
    let mut facts = GridContainer::new_alloc();
    facts.set_columns(3);
    facts.add_theme_constant_override("h_separation", 10);
    facts.add_theme_constant_override("v_separation", 10);
    let n = s.numbers();
    for (key, value) in [
        ("end-stat-turns", n.turns),
        ("end-stat-deepest", n.deepest),
        ("end-stat-score", n.score),
        ("end-stat-place", n.place.map(i64::from)),
        ("end-stat-gold", n.gold),
        ("end-stat-level", n.level),
    ] {
        if let Some(v) = value {
            facts.add_child(&fact(&i18n::tr(key), &v.to_string()));
        }
    }
    col.add_child(&facts);
    // the achievements this game holds
    col.add_child(&section(&tr!("end-achievements")));
    if s.achievements.is_empty() {
        col.add_child(&wrapped(
            &tr!("end-achievements-none"),
            Face::Body,
            16,
            theme::TEXT_DIM,
        ));
    } else {
        let mut medals = HFlowContainer::new_alloc();
        medals.add_theme_constant_override("h_separation", 16);
        medals.add_theme_constant_override("v_separation", 8);
        for (a, new) in &s.achievements {
            medals.add_child(&medal(a, *new));
        }
        col.add_child(&medals);
    }
    col
}

/// The conduct kept: NetHack's own words where it told them (its
/// achievements too), else the conduct the page reckoned; once either way.
fn conduct(s: &EndSummary) -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 10);
    col.set_h_size_flags(SizeFlags::EXPAND_FILL);
    col.add_child(&section(&tr!("end-conduct")));
    // what a conduct is, for a first game's player
    col.add_child(&wrapped(
        &tr!("end-conduct-about"),
        Face::Body,
        15,
        theme::TEXT_DIM,
    ));
    if let Some(lines) = end_disclosed::conduct_lines(&s.disclosed) {
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 3);
        for (text, heading) in lines {
            let mut l = if heading {
                theme::styled_label(text.trim_end_matches(':'), Face::Caps, 16, theme::GOLD)
            } else {
                wrapped(&text, Face::Body, 16, theme::TEXT)
            };
            l.set_autowrap_mode(AutowrapMode::WORD_SMART);
            list.add_child(&l);
        }
        col.add_child(&list);
        return col;
    }
    let conduct = if s.conduct.is_empty() {
        tr!("end-conduct-none")
    } else {
        let mut args = i18n::FluentArgs::new();
        args.set("female", i18n::Arg::fluent(female(s)));
        s.conduct
            .iter()
            .map(|k| i18n::tr_with(k, Some(&args)))
            .collect::<Vec<_>>()
            .join("  ·  ")
    };
    col.add_child(&wrapped(&conduct, Face::Body, 16, theme::TEXT));
    col
}

/// A number and its name, on a small plate.
fn fact(name: &str, value: &str) -> Gd<PanelContainer> {
    let mut plate = PanelContainer::new_alloc();
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(Color::from_rgba(0.035, 0.027, 0.02, 0.9));
    sb.set_border_width_all(1);
    sb.set_border_color(theme::GOLD_DIM);
    sb.set_corner_radius_all(4);
    for (side, px) in [
        (Side::LEFT, 12.0),
        (Side::RIGHT, 12.0),
        (Side::TOP, 4.0),
        (Side::BOTTOM, 6.0),
    ] {
        sb.set_content_margin(side, px);
    }
    plate.add_theme_stylebox_override("panel", &sb);
    plate.set_custom_minimum_size(Vector2::new(96.0, 0.0));
    plate.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", -2);
    let mut v = theme::styled_label(value, Face::TitleBold, 26, theme::GOLD_BRIGHT);
    v.set_horizontal_alignment(HorizontalAlignment::CENTER);
    // a number reads the same in every language
    i18n::verbatim(&v);
    col.add_child(&v);
    // 12 px at the Steam Deck's own scale (120 %)
    let mut n = theme::styled_label(name, Face::Caps, 15, theme::TEXT_DIM);
    n.set_horizontal_alignment(HorizontalAlignment::CENTER);
    col.add_child(&n);
    plate.add_child(&col);
    plate
}

/// An achievement's medallion and name; a new one marked.
fn medal(a: &Achievement, new: bool) -> Gd<HBoxContainer> {
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 8);
    let mut icon = TextureRect::new_alloc();
    icon.set_mouse_filter(MouseFilter::IGNORE);
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    icon.set_custom_minimum_size(Vector2::new(46.0, 46.0));
    if let Some(t) = crate::achievement_view::medallion(&a.id, false) {
        icon.set_texture(&t);
    }
    row.add_child(&icon);
    let mut words = VBoxContainer::new_alloc();
    words.set_alignment(AlignmentMode::CENTER);
    words.add_theme_constant_override("separation", -2);
    let (name, _) = crate::achievement_view::words(a);
    words.add_child(&theme::styled_label(&name, Face::BodyBold, 16, theme::TEXT));
    if new {
        words.add_child(&theme::styled_label(
            &tr!("end-achievement-new"),
            Face::Caps,
            13,
            theme::GOOD,
        ));
    }
    row.add_child(&words);
    row
}

/// The chronicle and the top ten, one over the other.
fn records(s: &EndSummary) -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 10);
    col.set_h_size_flags(SizeFlags::EXPAND_FILL);
    col.add_child(&section(&tr!("end-chronicle")));
    col.add_child(&chronicle(s));
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, 6.0));
    col.add_child(&gap);
    col.add_child(&section(&tr!("end-topten")));
    for note in s.topten_notes() {
        let words = i18n::engine(EngineKind::Window, note).into_owned();
        col.add_child(&wrapped(&words, Face::Body, 15, theme::TEXT_DIM));
    }
    col.add_child(&topten(s));
    // the conduct beside the story, the page's two columns of a length
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, 6.0));
    col.add_child(&gap);
    col.add_child(&conduct(s));
    col
}

/// The chronicle's major events: the turn, then what happened.
fn chronicle(s: &EndSummary) -> Gd<Control> {
    if s.chronicle.is_empty() {
        return wrapped(&tr!("end-chronicle-none"), Face::Body, 16, theme::TEXT_DIM).upcast();
    }
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(2);
    grid.add_theme_constant_override("h_separation", 14);
    grid.add_theme_constant_override("v_separation", 5);
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    for (turn, text) in &s.chronicle {
        let mut t = theme::styled_label(&turn.to_string(), Face::BodyBold, 15, theme::GOLD);
        t.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        t.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
        t.set_custom_minimum_size(Vector2::new(52.0, 0.0));
        i18n::verbatim(&t);
        grid.add_child(&t);
        let said = layouts::upstart(&i18n::engine(EngineKind::Message, text));
        grid.add_child(&wrapped(&said, Face::Body, 16, theme::TEXT));
    }
    grid.upcast()
}

/// The top ten as a table: the place, the points, the entry's words, the
/// hit points; this game's row lit. Nothing when the engine said why it
/// kept none (debug mode).
fn topten(s: &EndSummary) -> Gd<Control> {
    let rows = s.scores();
    if rows.is_empty() {
        if !s.topten_notes().is_empty() {
            return Control::new_alloc();
        }
        return wrapped(&tr!("end-topten-none"), Face::Body, 16, theme::TEXT_DIM).upcast();
    }
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(4);
    grid.add_theme_constant_override("h_separation", 12);
    grid.add_theme_constant_override("v_separation", 6);
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let cell = |text: &str, face: Face, color: Color, right: bool| {
        let mut l = theme::styled_label(text, face, 15, color);
        if right {
            l.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        }
        l.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
        l
    };
    for (key, right) in [
        ("end-topten-place", true),
        ("end-topten-points", true),
        ("end-topten-hero", false),
        ("end-topten-hp", true),
    ] {
        grid.add_child(&cell(&i18n::tr(key), Face::Caps, theme::GOLD_DIM, right));
    }
    for r in &rows {
        let color = if r.mine {
            theme::GOLD_BRIGHT
        } else {
            theme::TEXT
        };
        let face = if r.mine { Face::BodyBold } else { Face::Body };
        let rank = r.rank.map_or_else(|| "–".to_string(), |n| n.to_string());
        for (text, right) in [(rank, true), (r.points.to_string(), true)] {
            let c = cell(&text, face, color, right);
            i18n::verbatim(&c);
            grid.add_child(&c);
        }
        // NetHack's two spaces after a sentence are one in a proportional face
        let said = entry_words(r)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let said = readable_hero(&said, &s.roles);
        let mut words = cell(&said, face, color, false);
        words.set_autowrap_mode(AutowrapMode::WORD_SMART);
        words.set_h_size_flags(SizeFlags::EXPAND_FILL);
        grid.add_child(&words);
        let hp = cell(&r.hp, face, color, true);
        i18n::verbatim(&hp);
        grid.add_child(&hp);
    }
    grid.upcast()
}

/// An entry's hero as a player reads it: "Hero-Sam-Hum-Mal-Law died…" (the
/// name, then the codes of the role, race, gender and alignment) is
/// "Hero (самурай) died…", the role in the hero's gender and the language
/// now. Words that hold no such hero stay as they are.
fn readable_hero(said: &str, roles: &[(String, String, Option<String>)]) -> String {
    let (head, rest) = said.split_once(' ').unwrap_or((said, ""));
    let parts: Vec<&str> = head.split('-').collect();
    let n = parts.len();
    let code = |p: &str| p.len() == 3 && p.starts_with(|c: char| c.is_ascii_uppercase());
    if n < 5 || !parts[n - 4..].iter().all(|p| code(p)) {
        return said.to_string();
    }
    let Some((_, his, hers)) = roles.iter().find(|(c, ..)| c == parts[n - 4]) else {
        return said.to_string();
    };
    let english = hers
        .as_ref()
        .filter(|_| parts[n - 2] == "Fem")
        .unwrap_or(his);
    let role = i18n::engine(EngineKind::Name, english).to_lowercase();
    let name = parts[..n - 4].join("-");
    if rest.is_empty() {
        format!("{name} ({role})")
    } else {
        format!("{name} ({role}) {rest}")
    }
}

/// An entry's words in the language now. The engine's translator reads an
/// entry only as a line of the list (its reader wants the list's header
/// before it): the entry goes to it so, on one line, and comes back
/// wrapped again at the list's columns.
fn entry_words(r: &ScoreRow) -> String {
    if i18n::lang() != Lang::Ru {
        return i18n::engine(EngineKind::Window, &r.words).into_owned();
    }
    let rank = r.rank.map_or_else(String::new, |n| n.to_string());
    let line = format!("{rank:>3} {:>10}  {}", r.points, r.words);
    let Some(out) = i18n::engine_window(&[HEADER, &line]) else {
        return r.words.clone();
    };
    let words: Vec<&str> = out
        .iter()
        .skip(1)
        .map(|l| l.get(INDENT..).unwrap_or("").trim())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        r.words.clone()
    } else {
        words.join(" ")
    }
}

/// Words of three Latin letters or more in `texts` but those `allowed`
/// (the hero's name) and the top ten's codes ("Hero-Val-Hum-Fem-Neu"):
/// English left on a page in another language.
pub fn english_left(texts: &[String], allowed: &[&str]) -> Vec<String> {
    let allowed: Vec<String> = allowed.iter().map(|a| a.to_lowercase()).collect();
    let mut out = Vec::new();
    for text in texts {
        for token in text.split_whitespace() {
            let token = token.trim_matches(|c: char| !c.is_alphanumeric());
            if token.matches('-').count() >= 2 {
                continue;
            }
            for word in token.split(|c: char| !c.is_alphabetic()) {
                let latin =
                    word.chars().count() > 2 && word.chars().all(|c| c.is_ascii_alphabetic());
                if latin && !allowed.contains(&word.to_lowercase()) {
                    let around: String = text.trim().chars().take(60).collect();
                    out.push(format!("{word:?} in {around:?}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[(&str, bool)]) -> Vec<(String, bool)> {
        text.iter().map(|(l, b)| (l.to_string(), *b)).collect()
    }

    /// A line of the list as outentry prints it: the place (blank for
    /// none), the points, the words; then each line the words go on.
    fn entry(rank: Option<u32>, points: i64, words: &str) -> String {
        let rank = rank.map_or_else(String::new, |r| r.to_string());
        format!("{rank:>3} {points:>10}  {words}")
    }

    fn more(words: &str) -> String {
        format!("{:15} {words}", "")
    }

    #[test]
    fn a_top_ten_hero_reads_as_a_name_and_a_role() {
        let roles = vec![
            ("Sam".to_string(), "Samurai".to_string(), None),
            (
                "Pri".to_string(),
                "Priest".to_string(),
                Some("Priestess".to_string()),
            ),
        ];
        assert_eq!(
            readable_hero("Hero-Sam-Hum-Mal-Law quit in the Dungeons.", &roles),
            "Hero (samurai) quit in the Dungeons."
        );
        assert_eq!(
            readable_hero("Ann-Pri-Elf-Fem-Cha", &roles),
            "Ann (priestess)"
        );
        // no hero's codes: as it was
        for words in ["killed by a jackal", "Hero-Xyz-Hum-Mal-Law died", "a-b"] {
            assert_eq!(readable_hero(words, &roles), words);
        }
    }

    #[test]
    fn the_top_ten_reads_its_entries_and_this_game_s() {
        let printed =
            vec![
            ("You made the top ten list!".to_string(), false),
            (String::new(), false),
            (
                " No  Points     Name                                                   Hp [max]"
                    .to_string(),
                false,
            ),
            (
                entry(Some(1), 312, "Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom on"),
                false,
            ),
            (
                more("level 2.  Killed by a jackal.                          -  [16]"),
                false,
            ),
            (
                entry(Some(2), 78, "Hero-Val-Hum-Fem-Neu quit in The Dungeons of Doom on  "),
                true,
            ),
            (more("level 1.                                       12 [16]"), true),
        ];
        let rows = score_rows(&printed);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!((rows[0].rank, rows[0].points), (Some(1), 312));
        assert!(!rows[0].mine);
        assert_eq!(rows[0].hp, "- [16]");
        assert_eq!(
            rows[0].words,
            "Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom on level 2.  Killed by a jackal."
        );
        assert!(rows[1].mine);
        assert_eq!(rows[1].hp, "12 [16]");
        assert!(
            rows[1].words.ends_with("on level 1."),
            "{:?}",
            rows[1].words
        );
        assert_eq!(place(&rows), Some(2));
        // no list printed: no rows
        assert!(score_rows(&lines(&[("Goodbye", false)])).is_empty());
    }

    #[test]
    fn a_game_below_the_hero_s_best_has_no_place() {
        // the best, then this game's entry without a place, both in bold
        let printed = vec![
            (
                "You didn't beat your previous score of 312 points.".to_string(),
                false,
            ),
            (String::new(), false),
            (" No  Points     Name".to_string(), false),
            (
                entry(
                    Some(1),
                    312,
                    "Hero-Val-Hum-Fem-Neu died in The Dungeons of Doom",
                ),
                true,
            ),
            (
                more("on level 2.  Killed by a jackal.               -  [16]"),
                true,
            ),
            (
                entry(None, 4, "Hero-Val-Hum-Fem-Neu quit in The Dungeons of Doom"),
                true,
            ),
            (
                more("on level 1.                                16 [16]"),
                true,
            ),
        ];
        let rows = score_rows(&printed);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!((rows[1].rank, rows[1].points), (None, 4));
        assert_eq!(rows[1].hp, "16 [16]");
        assert_eq!(place(&rows), None);
        let s = EndSummary {
            printed,
            ..EndSummary::default()
        };
        assert_eq!(
            s.topten_notes(),
            ["You didn't beat your previous score of 312 points."]
        );
    }

    #[test]
    fn the_summary_tells_the_numbers_and_loses_the_farewell() {
        let text: Vec<String> = [
            "Goodbye Hero the Valkyrie...",
            "",
            "You died in The Dungeons of Doom on dungeon level 2 with 312 points,",
            "and 7 pieces of gold, after 540 moves.",
            "You were level 3 with a maximum of 28 hit points when you died.",
            "",
        ]
        .iter()
        .map(|l| l.to_string())
        .collect();
        let told = summary_numbers(&text);
        assert_eq!(
            told,
            Told {
                score: Some(312),
                gold: Some(7),
                moves: Some(540),
                level: Some(3),
                hpmax: Some(28),
            }
        );
        let s = EndSummary {
            text,
            turns: Some(530),
            score: Some(300),
            deepest: Some(2),
            ..EndSummary::default()
        };
        let n = s.numbers();
        // the window's numbers are final; the depth is the game's
        assert_eq!(
            (n.turns, n.score, n.gold, n.deepest),
            (Some(540), Some(312), Some(7), Some(2))
        );
        assert_eq!(s.summary().len(), 3);
        assert!(s.summary()[0].starts_with("You died"));
        // an ascension's: "You and Fido went to your reward with N points,"
        let reward = summary_numbers(&["You went to your reward with 98765 points,"]);
        assert_eq!(reward.score, Some(98765));
        assert_eq!(summary_numbers(&["nothing here"]), Told::default());
    }

    #[test]
    fn the_conduct_kept_names_the_strongest() {
        let counts = |pairs: &[(&str, i64)]| -> BTreeMap<String, i64> {
            pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
        };
        let fresh = counts(&[
            ("food", 0),
            ("unvegan", 0),
            ("unvegetarian", 0),
            ("gnostic", 0),
            ("weaphit", 0),
            ("killer", 0),
            ("literate", 0),
            ("polypiles", 0),
            ("polyselfs", 0),
            ("wishes", 0),
            ("wisharti", 0),
            ("sokocheat", 0),
            ("pets", 1),
        ]);
        let kept = conduct_kept(&fresh, false);
        assert_eq!(kept.first(), Some(&"end-conduct-foodless"));
        assert!(!kept.contains(&"end-conduct-vegan"));
        assert!(kept.contains(&"end-conduct-wishless"));
        assert!(!kept.contains(&"end-conduct-sokoban"), "never entered");
        assert!(!kept.contains(&"end-conduct-petless"));
        let ate_meat = counts(&[
            ("food", 3),
            ("unvegan", 2),
            ("unvegetarian", 1),
            ("wishes", 1),
        ]);
        let kept = conduct_kept(&ate_meat, true);
        assert!(!kept.iter().any(|k| k.contains("veg") || k.contains("food")));
        assert!(!kept.contains(&"end-conduct-wishless"));
    }

    #[test]
    fn the_final_words_are_the_death_s_turn_before_you_die() {
        let mut log = nh_world::MessageLog::new();
        for (text, turn) in [
            ("You see here a scroll.", 40),
            ("You hit the soldier ant.", 41),
            ("The soldier ant bites!", 41),
            ("The soldier ant stings!", 41),
            ("You die...", 41),
        ] {
            log.push(text.into(), 0, Some(turn), false);
        }
        let all: Vec<nh_world::Message> = log.iter().cloned().collect();
        let words: Vec<String> = final_words(&all).into_iter().map(|m| m.text).collect();
        assert_eq!(words, ["The soldier ant bites!", "The soldier ant stings!"]);
        // only the death's turn
        let mut log = nh_world::MessageLog::new();
        log.push("You see here a scroll.".into(), 0, Some(40), false);
        log.push("You die...".into(), 0, Some(41), false);
        let all: Vec<nh_world::Message> = log.iter().cloned().collect();
        assert!(final_words(&all).is_empty());
        // a death without "You die...": its own line
        let mut log = nh_world::MessageLog::new();
        log.push(
            "You begin eating the cockatrice corpse.".into(),
            0,
            Some(7),
            false,
        );
        log.push("You turn to stone.".into(), 0, Some(8), false);
        let all: Vec<nh_world::Message> = log.iter().cloned().collect();
        let words: Vec<String> = final_words(&all).into_iter().map(|m| m.text).collect();
        assert_eq!(words, ["You turn to stone."]);
    }

    #[test]
    fn the_beat_comes_in_its_order() {
        // the dark, then the words, the stone, the page; each eased
        assert_eq!(ramp(0.0, WORDS_IN), 0.0);
        assert_eq!(ramp(BEAT_SECS, PAGE_IN), 1.0);
        assert!(BACKDROP.1 <= STONE_IN.1 && STONE_IN.1 <= PAGE_IN.0);
        assert!((ramp(0.3, (0.2, 0.4)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn endings_are_told_apart() {
        assert_eq!(EndHow::of(Some("died")), EndHow::Died);
        assert_eq!(EndHow::of(Some("quit")), EndHow::Quit);
        assert_eq!(EndHow::of(Some("ascended")), EndHow::Ascended);
        assert_eq!(EndHow::of(Some("escaped")), EndHow::Escaped);
        assert_eq!(EndHow::of(Some("panicked")), EndHow::Died);
        assert_eq!(EndHow::of(None), EndHow::Died);
    }

    #[test]
    fn english_is_found_but_names_and_codes() {
        let texts = vec![
            "Убита шакалом".to_string(),
            "Hero-Val-Hum-Fem-Neu сдалась на 1-м уровне".to_string(),
            "Hero, Новобранка".to_string(),
            "312 Au".to_string(),
            "Restoring save file...".to_string(),
        ];
        let left = english_left(&texts, &["Hero"]);
        assert_eq!(left.len(), 3, "{left:?}");
        assert!(left[0].contains("Restoring"));
    }
}
