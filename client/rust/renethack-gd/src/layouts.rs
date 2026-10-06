//! The engine's windows the client lays out itself: the catalog knows
//! their lines only as layout, and the translator hands them back
//! untouched. These are the tombstone (rip.c), #vanquished and #genocided
//! (insight.c), and #overview (dungeon.c). Each is parsed from its English
//! into parts, and each part is said in the language now: the client's
//! own words from Fluent, the engine's names through its translator or,
//! where a phrase needs another case, the lexicon.
//!
//! No Godot here: the views are in `dialogs` and `screens`.

use std::collections::HashMap;
use std::ops::Range;

use nh_i18n::Case;
use nh_i18n::lexicon::Lexicon;
use nh_i18n::names::makeplural;
use nh_protocol::{Catalog, MonsterInfo};

use crate::i18n::{self, EngineKind, Lang};
use crate::tr;

// ---- the tombstone ----

/// What genl_outrip writes on the stone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tombstone {
    /// The hero's name as typed (the stone has room for 16 characters).
    pub name: String,
    /// The gold the hero died with.
    pub gold: i64,
    /// How the hero died, in formatkiller's words ("killed by a jackal"),
    /// put together again from the stone's lines.
    pub death: String,
    pub year: i32,
}

/// The third line of rip_txt: the stone's face starts two lines below.
const REST: &str = "/    REST    \\";

/// The tombstone among a window's lines, and the lines it takes.
pub fn tombstone<S: AsRef<str>>(lines: &[S]) -> Option<(Tombstone, Range<usize>)> {
    let line = |i: usize| lines.get(i).map(AsRef::as_ref);
    let rest = lines.iter().position(|l| l.as_ref().trim() == REST)?;
    let top = rest.checked_sub(2)?;
    if line(top)?.trim() != "----------" {
        return None;
    }
    // the face between the stone's sides
    let face = |i: usize| -> Option<&str> {
        let l = line(i)?;
        let (a, b) = (l.find('|')?, l.rfind('|')?);
        (b > a).then(|| l[a + 1..b].trim())
    };
    let name = face(rest + 4)?.to_string();
    let gold = face(rest + 5)?.strip_suffix("Au")?.trim().parse().ok()?;
    // the death's words, wrapped at the stone's width
    let death = (rest + 6..rest + 10)
        .filter_map(face)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let year = face(rest + 10)?.parse().ok()?;
    // the flowers' line and the ground's end the stone
    let end = (rest + 13).min(lines.len());
    Some((
        Tombstone {
            name,
            gold,
            death,
            year,
        },
        top..end,
    ))
}

// ---- #vanquished ----

/// A line of the vanquished list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KillRow {
    /// A heading of the list by class ("Dog or other canine", "Rider").
    Class(String),
    /// A kind of creature and how many died.
    Kind {
        /// Its English name in the singular ("jackal"), a unique one's as
        /// the engine names it ("the Oracle", "Medusa"); the plural when
        /// no kind is known by it.
        name: String,
        count: i64,
        /// One of a kind: the Oracle, Medusa.
        unique: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vanquished {
    pub rows: Vec<KillRow>,
    /// "N creatures vanquished." (a list of one kind has none).
    pub total: Option<i64>,
}

/// The vanquished list in a window's lines; `singular` finds a kind by its
/// plural ("jackals").
pub fn vanquished<S: AsRef<str>>(
    lines: &[S],
    singular: &dyn Fn(&str) -> Option<String>,
) -> Option<Vanquished> {
    let mut texts = lines
        .iter()
        .map(AsRef::as_ref)
        .filter(|l| !l.trim().is_empty());
    if texts.next()? != "Vanquished creatures:" {
        return None;
    }
    let mut rows = Vec::new();
    let mut total = None;
    for l in texts {
        if let Some(n) = l
            .trim()
            .strip_suffix(" creatures vanquished.")
            .and_then(|n| n.parse().ok())
        {
            total = Some(n);
        } else {
            rows.push(kill_row(l, singular));
        }
    }
    Some(Vanquished { rows, total })
}

/// "the Oracle (twice)", "Medusa (3 times)": the name and the count.
fn times(text: &str) -> (&str, Option<i64>) {
    let Some(open) = text.strip_suffix(')').and_then(|t| t.rfind(" (")) else {
        return (text, None);
    };
    let n = match &text[open + 2..text.len() - 1] {
        "twice" => Some(2),
        "thrice" => Some(3),
        t => t.strip_suffix(" times").and_then(|n| n.parse().ok()),
    };
    match n {
        Some(n) => (&text[..open], Some(n)),
        None => (text, None),
    }
}

fn kill_row(line: &str, singular: &dyn Fn(&str) -> Option<String>) -> KillRow {
    let text = line.trim();
    // every creature's line is indented in a list by class but for a
    // count of three digits and a unique one with "the" (the list's
    // other orders put those at the margin too); a heading never is
    let at_margin = !line.starts_with(' ');
    if at_margin && !text.starts_with("the ") && !text.starts_with(|c: char| c.is_ascii_digit()) {
        return KillRow::Class(text.to_string());
    }
    let (body, repeated) = times(text);
    if repeated.is_none() {
        if let Some(name) = body.strip_prefix("a ").or_else(|| body.strip_prefix("an ")) {
            return KillRow::Kind {
                name: name.to_string(),
                count: 1,
                unique: false,
            };
        }
        if let Some((n, plural)) = body.split_once(' ')
            && let Ok(count) = n.parse::<i64>()
        {
            return KillRow::Kind {
                name: singular(plural).unwrap_or_else(|| plural.to_string()),
                count,
                unique: false,
            };
        }
    }
    KillRow::Kind {
        name: body.to_string(),
        count: repeated.unwrap_or(1),
        unique: true,
    }
}

// ---- #genocided ----

/// The heading of the list of species gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoneHeading {
    Genocided,
    Extinct,
    GenocidedOrExtinct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoneRow {
    /// A heading of the list by class.
    Class(String),
    /// A species, by its English plural ("jackals").
    Species { plural: String, extinct: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genocided {
    pub heading: GoneHeading,
    pub rows: Vec<GoneRow>,
    pub genocided: Option<i64>,
    pub extinct: Option<i64>,
}

pub fn genocided<S: AsRef<str>>(lines: &[S]) -> Option<Genocided> {
    let mut texts = lines
        .iter()
        .map(AsRef::as_ref)
        .filter(|l| !l.trim().is_empty());
    let heading = match texts.next()? {
        "Genocided species:" => GoneHeading::Genocided,
        "Extinct species:" => GoneHeading::Extinct,
        "Genocided or extinct species:" => GoneHeading::GenocidedOrExtinct,
        _ => return None,
    };
    let mut out = Genocided {
        heading,
        rows: Vec::new(),
        genocided: None,
        extinct: None,
    };
    let count = |t: &str, tail: &str| t.strip_suffix(tail).and_then(|n| n.parse().ok());
    for l in texts {
        let text = l.trim();
        if let Some(n) = count(text, " species genocided.") {
            out.genocided = Some(n);
        } else if let Some(n) = count(text, " species extinct.") {
            out.extinct = Some(n);
        } else if l.starts_with(' ') {
            let (plural, extinct) = match text.strip_suffix(" (extinct)") {
                Some(p) => (p, true),
                None => (text, false),
            };
            out.rows.push(GoneRow::Species {
                plural: plural.to_string(),
                extinct,
            });
        } else {
            out.rows.push(GoneRow::Class(text.to_string()));
        }
    }
    Some(out)
}

// ---- #overview ----

/// Where a level line of #overview has the hero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Here {
    Are,
    /// The game over: escaped from here.
    LeftFrom,
    /// The game over: died (or quit) here.
    Were,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Level(i32),
    Astral,
    /// A plane of the endgame by its element ("Earth").
    Plane(String),
    /// Anything else the engine names a level (as it says it).
    Other(String),
}

/// How many of a feature the hero has seen: the engine counts to three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    One,
    Two,
    Many,
}

impl Seen {
    /// The Fluent selector.
    fn key(self) -> &'static str {
        match self {
            Seen::One => "one",
            Seen::Two => "two",
            Seen::Many => "many",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureKind {
    /// Shops: one by its kind ("general store"), more by their count.
    Shop(Option<String>),
    Temple,
    Altar,
    Throne,
    Fountain,
    Sink,
    Grave,
    Tree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    pub kind: FeatureKind,
    pub seen: Seen,
    /// The hero's god, when every altar seen is to them.
    pub god: Option<String>,
}

/// The castle's drawbridge tune, when the hero knows of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tune {
    /// The notes ("ABCDE").
    Notes(String),
    /// Only that there are five.
    FiveNotes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Special {
    Oracle,
    Sokoban {
        solved: bool,
    },
    BigRoom,
    Rogue,
    Home {
        no_way_back: bool,
    },
    /// The quest done for its leader.
    QuestDone(String),
    QuestGiven(String),
    Ludios,
    Castle(Option<Tune>),
    Valley,
    Gateway,
    Sanctum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    Stairs { up: bool },
    OneWay { up: bool },
    Portal,
    SealedPortal,
    Connection,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverviewRow {
    /// A dungeon's heading: its name and the levels reached (none for
    /// one level, and in the endgame).
    Dungeon {
        name: String,
        levels: Option<(i32, i32)>,
        /// The dungeon builds up (Sokoban): levels from the bottom.
        up: bool,
    },
    Level {
        place: Place,
        /// Debug mode's name of a special level ("oracle").
        proto: Option<String>,
        /// The player's annotation.
        note: Option<String>,
        here: Option<Here>,
    },
    Features(Vec<Feature>),
    Special(Special),
    /// The quest's leader summoned the hero.
    Summoned(String),
    Branch {
        kind: BranchKind,
        to: String,
        /// The level it leads to, for a branch up.
        level: Option<i32>,
    },
    /// "Final resting place for", the dead below.
    RestingPlace,
    /// A hero dead here: the hero of this game (None) or a bones' one.
    Dead {
        who: Option<String>,
        how: String,
    },
    /// A line the parser does not know: shown as the engine wrote it.
    Other(String),
}

/// The overview in a menu's lines (None: another menu). It starts with a
/// dungeon's heading and has a level.
pub fn overview<S: AsRef<str>>(lines: &[S]) -> Option<Vec<OverviewRow>> {
    let rows: Vec<OverviewRow> = lines
        .iter()
        .map(AsRef::as_ref)
        .filter(|l| !l.trim().is_empty())
        .map(overview_row)
        .collect();
    let starts = matches!(rows.first(), Some(OverviewRow::Dungeon { .. }));
    let leveled = rows.iter().any(|r| matches!(r, OverviewRow::Level { .. }));
    (starts && leveled).then_some(rows)
}

/// The rows' indent: a level's line has three spaces outside a menu, what
/// is on the level six, the dead nine.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

pub fn overview_row(line: &str) -> OverviewRow {
    let text = line.trim();
    let other = || OverviewRow::Other(text.to_string());
    match indent(line) {
        9.. => dead_row(text).unwrap_or_else(other),
        6.. => level_detail(text).unwrap_or_else(other),
        _ => level_row(text)
            .or_else(|| dungeon_row(text))
            .unwrap_or_else(other),
    }
}

fn dead_row(text: &str) -> Option<OverviewRow> {
    // the last one ends the sentence
    let body = text.strip_suffix([',', '.'])?;
    if let Some(how) = body.strip_prefix("you, ") {
        return Some(OverviewRow::Dead {
            who: None,
            how: how.to_string(),
        });
    }
    let (who, how) = body.split_once(", ")?;
    Some(OverviewRow::Dead {
        who: Some(who.to_string()),
        how: how.to_string(),
    })
}

fn dungeon_row(text: &str) -> Option<OverviewRow> {
    if let Some(name) = text.strip_suffix(':') {
        return Some(OverviewRow::Dungeon {
            name: name.to_string(),
            levels: None,
            up: false,
        });
    }
    let (name, range) = text.split_once(": levels ")?;
    let (from, up, to) = match range.split_once(" up to ") {
        Some((a, b)) => (a, true, b),
        None => {
            let (a, b) = range.split_once(" to ")?;
            (a, false, b)
        }
    };
    Some(OverviewRow::Dungeon {
        name: name.to_string(),
        levels: Some((from.parse().ok()?, to.parse().ok()?)),
        up,
    })
}

fn level_row(text: &str) -> Option<OverviewRow> {
    let (head, mut rest) = text.split_once(':')?;
    let place = if let Some(n) = head.strip_prefix("Level ") {
        Place::Level(n.parse().ok()?)
    } else if head == "Astral Plane" {
        Place::Astral
    } else if let Some(element) = head.strip_prefix("Plane of ") {
        Place::Plane(element.to_string())
    } else if head.starts_with("unknown plane #") {
        Place::Other(head.to_string())
    } else {
        return None;
    };
    let mut here = None;
    for (tail, h) in [
        (" <- You are here.", Here::Are),
        (" <- You left from here.", Here::LeftFrom),
        (" <- You were here.", Here::Were),
    ] {
        if let Some(r) = rest.strip_suffix(tail) {
            rest = r;
            here = Some(h);
            break;
        }
    }
    let mut proto = None;
    if let Some(r) = rest.strip_prefix(" [")
        && let Some((p, r)) = r.split_once(']')
    {
        proto = Some(p.to_string());
        rest = r;
    }
    let mut note = None;
    if let Some(n) = rest.strip_prefix(" \"").and_then(|r| r.strip_suffix('"')) {
        note = Some(n.to_string());
        rest = "";
    }
    rest.is_empty().then_some(OverviewRow::Level {
        place,
        proto,
        note,
        here,
    })
}

/// A line under a level: what is there, what it is, where it leads.
fn level_detail(text: &str) -> Option<OverviewRow> {
    let special = |s: Special| Some(OverviewRow::Special(s));
    match text {
        "Oracle of Delphi." => return special(Special::Oracle),
        "Solved." => return special(Special::Sokoban { solved: true }),
        "Unsolved." => return special(Special::Sokoban { solved: false }),
        "A very big room." => return special(Special::BigRoom),
        "A primitive area." => return special(Special::Rogue),
        "Home." => return special(Special::Home { no_way_back: false }),
        "Home (no way back...)." => return special(Special::Home { no_way_back: true }),
        "Fort Ludios." => return special(Special::Ludios),
        "The castle." => return special(Special::Castle(None)),
        "Valley of the Dead." => return special(Special::Valley),
        "Gateway to Moloch's Sanctum." => return special(Special::Gateway),
        "Moloch's Sanctum." => return special(Special::Sanctum),
        "Final resting place for" => return Some(OverviewRow::RestingPlace),
        _ => {}
    }
    let between = |head: &str, tail: &str| -> Option<String> {
        Some(text.strip_prefix(head)?.strip_suffix(tail)?.to_string())
    };
    if let Some(leader) = between("Completed quest for ", ".") {
        return special(Special::QuestDone(leader));
    }
    if let Some(leader) = between("Given quest by ", ".") {
        return special(Special::QuestGiven(leader));
    }
    if let Some(leader) = between("Summoned by ", ".") {
        return Some(OverviewRow::Summoned(leader));
    }
    if let Some(notes) = between(
        "The castle (play notes \"",
        "\" to open or close drawbridge).",
    ) {
        return special(Special::Castle(Some(Tune::Notes(notes))));
    }
    if text == "The castle (play 5-note tune to open or close drawbridge)." {
        return special(Special::Castle(Some(Tune::FiveNotes)));
    }
    branch_row(text).or_else(|| features(text).map(OverviewRow::Features))
}

fn branch_row(text: &str) -> Option<OverviewRow> {
    const KINDS: [(&str, BranchKind); 8] = [
        ("Sealed portal", BranchKind::SealedPortal),
        ("Portal", BranchKind::Portal),
        ("Connection", BranchKind::Connection),
        ("One way stairs up", BranchKind::OneWay { up: true }),
        ("One way stairs down", BranchKind::OneWay { up: false }),
        ("Stairs up", BranchKind::Stairs { up: true }),
        ("Stairs down", BranchKind::Stairs { up: false }),
        ("(unknown)", BranchKind::Unknown),
    ];
    let body = text.strip_suffix('.')?;
    let (kind, to) = KINDS.iter().find_map(|(head, kind)| {
        body.strip_prefix(head)
            .and_then(|r| r.strip_prefix(" to "))
            .map(|to| (*kind, to))
    })?;
    // a branch up says the level it leads to
    let (to, level) = match to.rsplit_once(", level ") {
        Some((place, n)) if n.parse::<i32>().is_ok() => (place, n.parse().ok()),
        _ => (to, None),
    };
    Some(OverviewRow::Branch {
        kind,
        to: to.to_string(),
        level,
    })
}

/// "Some shops, a temple and an altar to Anhur, a fountain.": what the
/// hero has seen on a level.
fn features(text: &str) -> Option<Vec<Feature>> {
    let body = text.strip_suffix('.')?;
    // the engine made the list a sentence: its first letter up
    let mut chars = body.chars();
    let body: String = chars.next()?.to_lowercase().chain(chars).collect();
    let mut out: Vec<Feature> = Vec::new();
    for chunk in body.split(", ") {
        // temples and altars, and the god of every altar seen
        let (chunk, god) = match chunk.split_once(" to ") {
            Some((c, g)) => (c, Some(g.to_string())),
            None => (chunk, None),
        };
        for part in chunk.split(" and ") {
            out.push(feature(part)?);
        }
        if let Some(g) = god {
            out.last_mut()?.god = Some(g);
        }
    }
    Some(out)
}

fn feature(part: &str) -> Option<Feature> {
    let (word, name) = part.split_once(' ')?;
    let seen = match word {
        "a" | "an" => Seen::One,
        "some" => Seen::Two,
        "many" => Seen::Many,
        _ => return None,
    };
    let kind = match name.strip_suffix('s').unwrap_or(name) {
        _ if name == "shops" => FeatureKind::Shop(None),
        "temple" => FeatureKind::Temple,
        "altar" => FeatureKind::Altar,
        "throne" => FeatureKind::Throne,
        "fountain" => FeatureKind::Fountain,
        "sink" => FeatureKind::Sink,
        "grave" => FeatureKind::Grave,
        "tree" => FeatureKind::Tree,
        // a single shop goes by its kind
        _ if seen == Seen::One => FeatureKind::Shop(Some(name.to_string())),
        _ => return None,
    };
    Some(Feature {
        kind,
        seen,
        god: None,
    })
}

// ---- the parts in the language now ----

/// A name of the engine's as the player reads it, its first letter up.
pub fn name(english: &str) -> String {
    upstart(&i18n::engine(EngineKind::Name, english))
}

/// `english` (an entry of the lexicon's `section`) as the player reads it
/// in `case`: the lexicon's form in Russian, else the engine's words.
fn name_in(section: &str, english: &str, case: Case) -> String {
    if i18n::lang() == Lang::Ru
        && let Some(noun) = Lexicon::ru().noun(section, english)
    {
        return noun.singular(case).to_string();
    }
    i18n::engine(EngineKind::Name, english).into_owned()
}

/// The first letter in upper case.
pub fn upstart(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The engine's words that the player typed (the hero's name, a level's
/// annotation): the same in every language.
pub fn typed(text: &str) -> String {
    match i18n::lang() {
        // the pseudo-language's check takes it as the engine's words
        Lang::Pseudo => i18n::engine(EngineKind::Name, text).into_owned(),
        _ => text.to_string(),
    }
}

/// How the hero died, as formatkiller says it.
pub fn death(english: &str) -> String {
    i18n::engine(EngineKind::Window, english).into_owned()
}

/// The vanquished list's kinds by their plurals: the catalog's monsters
/// (each of their names).
pub fn kinds_by_plural(catalog: Option<&Catalog>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for m in catalog.map_or(&[][..], |c| &c.monsters) {
        for n in [Some(&m.name), m.male.as_ref(), m.female.as_ref()]
            .into_iter()
            .flatten()
        {
            out.entry(makeplural(n)).or_insert_with(|| n.clone());
        }
    }
    out
}

/// A heading of a list by class ("Dog or other canine") as the player
/// reads it.
pub fn class_name(english: &str) -> String {
    if english == "Rider" {
        return tr!("vanquished-rider");
    }
    let key = english.to_lowercase();
    if i18n::lang() == Lang::Ru
        && let Some(text) = Lexicon::ru().get("monclass", &key).and_then(|e| e.fixed())
    {
        return upstart(text);
    }
    upstart(&i18n::engine(EngineKind::Name, english))
}

/// The catalog's monster of this English name (a unique one's article
/// aside: "the Oracle"), for its symbol and colour.
pub fn monster<'a>(catalog: Option<&'a Catalog>, english: &str) -> Option<&'a MonsterInfo> {
    let bare = english.strip_prefix("the ").unwrap_or(english);
    catalog?.monsters.iter().find(|m| {
        [Some(&m.name), m.male.as_ref(), m.female.as_ref()]
            .into_iter()
            .flatten()
            .any(|n| n == bare || n == english)
    })
}

pub fn gone_heading(h: GoneHeading) -> String {
    match h {
        GoneHeading::Genocided => tr!("genocided-title"),
        GoneHeading::Extinct => tr!("genocided-title-extinct"),
        GoneHeading::GenocidedOrExtinct => tr!("genocided-title-both"),
    }
}

pub fn dungeon_name(english: &str) -> String {
    name(english)
}

pub fn levels(from: i32, to: i32, up: bool) -> String {
    if up {
        tr!("overview-levels-up", from = from, to = to)
    } else {
        tr!("overview-levels", from = from, to = to)
    }
}

pub fn place(p: &Place) -> String {
    match p {
        Place::Level(n) => tr!("overview-level", level = *n),
        Place::Astral => tr!("overview-astral"),
        Place::Plane(element) => match element.as_str() {
            "Earth" => tr!("overview-plane-earth"),
            "Air" => tr!("overview-plane-air"),
            "Fire" => tr!("overview-plane-fire"),
            "Water" => tr!("overview-plane-water"),
            other => i18n::engine(EngineKind::Menu, other).into_owned(),
        },
        Place::Other(text) => i18n::engine(EngineKind::Menu, text).into_owned(),
    }
}

pub fn here(h: Here) -> String {
    match h {
        Here::Are => tr!("overview-here"),
        Here::LeftFrom => tr!("overview-left-from"),
        Here::Were => tr!("overview-were"),
    }
}

pub fn note(text: &str) -> String {
    tr!("overview-note", note = typed(text))
}

pub fn feature_text(f: &Feature) -> String {
    let seen = f.seen.key();
    let god = f.god.as_deref().map(|g| name_in("god", g, Case::Gen));
    match (&f.kind, god) {
        (FeatureKind::Shop(Some(kind)), _) => i18n::engine(EngineKind::Name, kind).into_owned(),
        (FeatureKind::Shop(None), _) => tr!("overview-shops", seen = seen),
        (FeatureKind::Temple, Some(god)) => tr!("overview-temples-to", seen = seen, god = god),
        (FeatureKind::Temple, None) => tr!("overview-temples", seen = seen),
        (FeatureKind::Altar, Some(god)) => tr!("overview-altars-to", seen = seen, god = god),
        (FeatureKind::Altar, None) => tr!("overview-altars", seen = seen),
        (FeatureKind::Throne, _) => tr!("overview-thrones", seen = seen),
        (FeatureKind::Fountain, _) => tr!("overview-fountains", seen = seen),
        (FeatureKind::Sink, _) => tr!("overview-sinks", seen = seen),
        (FeatureKind::Grave, _) => tr!("overview-graves", seen = seen),
        (FeatureKind::Tree, _) => tr!("overview-trees", seen = seen),
    }
}

pub fn special(s: &Special) -> String {
    let leader = |l: &str| name_in("monster", l, Case::Gen);
    match s {
        Special::Oracle => tr!("overview-oracle"),
        Special::Sokoban { solved: true } => tr!("overview-sokoban-solved"),
        Special::Sokoban { solved: false } => tr!("overview-sokoban-unsolved"),
        Special::BigRoom => tr!("overview-bigroom"),
        Special::Rogue => tr!("overview-rogue"),
        Special::Home { no_way_back: false } => tr!("overview-home"),
        Special::Home { no_way_back: true } => tr!("overview-home-lost"),
        Special::QuestDone(l) => tr!("overview-quest-done", leader = leader(l)),
        Special::QuestGiven(l) => tr!("overview-quest-given", leader = leader(l)),
        Special::Ludios => tr!("overview-ludios"),
        Special::Castle(None) => tr!("overview-castle"),
        Special::Castle(Some(Tune::Notes(n))) => tr!("overview-castle-notes", notes = typed(n)),
        Special::Castle(Some(Tune::FiveNotes)) => tr!("overview-castle-tune"),
        Special::Valley => tr!("overview-valley"),
        Special::Gateway => tr!("overview-gateway"),
        Special::Sanctum => tr!("overview-sanctum"),
    }
}

pub fn summoned(leader: &str) -> String {
    tr!(
        "overview-summoned",
        leader = name_in("monster", leader, Case::Gen)
    )
}

pub fn branch(kind: BranchKind, to: &str, level: Option<i32>) -> String {
    // "в Гномьи копи": the place it leads to in the accusative
    let place = name_in("place", to, Case::Acc);
    let text = match kind {
        BranchKind::Stairs { up: true } => tr!("overview-stairs-up", place = place),
        BranchKind::Stairs { up: false } => tr!("overview-stairs-down", place = place),
        BranchKind::OneWay { up: true } => tr!("overview-one-way-up", place = place),
        BranchKind::OneWay { up: false } => tr!("overview-one-way-down", place = place),
        BranchKind::Portal => tr!("overview-portal", place = place),
        BranchKind::SealedPortal => tr!("overview-sealed-portal", place = place),
        BranchKind::Connection => tr!("overview-connection", place = place),
        BranchKind::Unknown => tr!("overview-unknown-way", place = place),
    };
    match level {
        Some(n) => tr!("overview-branch-level", branch = text, level = n),
        None => text,
    }
}

/// The heading over the dead of a level: over the hero ("Здесь покоитесь
/// вы": the death's words say the hero in the third person, as on the
/// stone, which a "вы" before them would not agree with) or over the
/// bones' heroes.
pub fn resting(hero: bool) -> String {
    if hero {
        tr!("overview-resting-you")
    } else {
        tr!("overview-resting")
    }
}

pub fn dead(who: Option<&str>, how: &str) -> String {
    match who {
        None => tr!("overview-dead-you", how = death(how)),
        Some(w) => tr!("overview-dead", who = typed(w), how = death(how)),
    }
}

/// An overview's line said in one line (the overview as a menu: `m`
/// before #overview picks a level to annotate).
pub fn overview_line(row: &OverviewRow) -> String {
    match row {
        OverviewRow::Dungeon {
            name,
            levels: Some((a, b)),
            up,
        } => {
            format!("{}: {}", dungeon_name(name), levels(*a, *b, *up))
        }
        OverviewRow::Dungeon { name, .. } => dungeon_name(name),
        OverviewRow::Level {
            place: p,
            proto,
            note: n,
            here: h,
        } => {
            let mut out = place(p);
            if let Some(proto) = proto {
                out.push_str(&format!(" [{}]", typed(proto)));
            }
            if let Some(n) = n {
                out.push(' ');
                out.push_str(&note(n));
            }
            if let Some(h) = h {
                out.push_str(&format!(" ← {}", here(*h)));
            }
            out
        }
        OverviewRow::Features(fs) => {
            upstart(&fs.iter().map(feature_text).collect::<Vec<_>>().join(", "))
        }
        OverviewRow::Special(s) => special(s),
        OverviewRow::Summoned(l) => summoned(l),
        OverviewRow::Branch { kind, to, level } => branch(*kind, to, *level),
        OverviewRow::RestingPlace => tr!("overview-resting"),
        OverviewRow::Dead { who, how } => dead(who.as_deref(), how),
        OverviewRow::Other(text) => i18n::engine(EngineKind::Menu, text).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rip_txt with the hero's lines put in as genl_outrip centres them.
    fn stone(name: &str, gold: &str, death: [&str; 4], year: &str) -> Vec<String> {
        let face = |t: &str| {
            let mut line: Vec<char> = "                  |                  |".chars().collect();
            // STONE_LINE_CENT - ((strlen(text) + 1) >> 1)
            let at = 28 - t.len().div_ceil(2);
            for (i, c) in t.chars().enumerate() {
                line[at + i] = c;
            }
            line.into_iter().collect::<String>()
        };
        let mut out = vec![
            String::new(),
            "                       ----------".to_string(),
            "                      /          \\".to_string(),
            "                     /    REST    \\".to_string(),
            "                    /      IN      \\".to_string(),
            "                   /     PEACE      \\".to_string(),
            "                  /                  \\".to_string(),
            face(name),
            face(gold),
        ];
        out.extend(death.iter().map(|d| face(d)));
        out.extend([
            face(year),
            "                 *|     *  *  *      | *".to_string(),
            "        _________)/\\\\_//(\\/(/\\)/\\//\\/|_)_______".to_string(),
            String::new(),
            String::new(),
            "Goodbye Hero the Valkyrie...".to_string(),
        ]);
        out
    }

    #[test]
    fn the_stone_says_the_name_the_gold_the_death_and_the_year() {
        let lines = stone(
            "Hero",
            "123 Au",
            ["killed by her", "own player", "", ""],
            "2026",
        );
        let (t, range) = tombstone(&lines).expect("a tombstone");
        assert_eq!(
            t,
            Tombstone {
                name: "Hero".into(),
                gold: 123,
                death: "killed by her own player".into(),
                year: 2026,
            }
        );
        assert_eq!(range, 1..16);
        assert_eq!(lines[range.end + 2], "Goodbye Hero the Valkyrie...");
        // a window without one
        assert_eq!(tombstone(&["Goodbye Hero the Valkyrie..."]), None);
    }

    #[test]
    fn the_vanquished_list_by_any_order() {
        let singular = |p: &str| match p {
            "jackals" => Some("jackal".to_string()),
            "sewer rats" => Some("sewer rat".to_string()),
            _ => None,
        };
        // the traditional order: three digits, a unique one, the total
        let lines = [
            "Vanquished creatures:",
            "",
            "  a newt",
            " 12 jackals",
            "123 sewer rats",
            "the Oracle (twice)",
            "    Medusa",
            "  2 woodchucks",
            "",
            "139 creatures vanquished.",
        ];
        let v = vanquished(&lines, &singular).expect("the list");
        let kind = |name: &str, count: i64, unique: bool| KillRow::Kind {
            name: name.into(),
            count,
            unique,
        };
        assert_eq!(
            v.rows,
            [
                kind("newt", 1, false),
                kind("jackal", 12, false),
                kind("sewer rat", 123, false),
                kind("the Oracle", 2, true),
                kind("Medusa", 1, true),
                // no kind of this plural: the plural stays
                kind("woodchucks", 2, false),
            ]
        );
        assert_eq!(v.total, Some(139));
        // by class: the headings at the margin, the creatures indented
        let lines = [
            "Vanquished creatures:",
            "",
            "Dog or other canine",
            "    2 jackals",
            "Rider",
            " the Oracle (3 times)",
        ];
        let v = vanquished(&lines, &singular).expect("the list");
        assert_eq!(
            v.rows,
            [
                KillRow::Class("Dog or other canine".into()),
                kind("jackal", 2, false),
                KillRow::Class("Rider".into()),
                kind("the Oracle", 3, true),
            ]
        );
        assert_eq!(v.total, None);
        assert_eq!(vanquished(&["Things that are here:"], &singular), None);
    }

    #[test]
    fn the_genocided_list_and_its_counts() {
        let lines = [
            "Genocided or extinct species:",
            "",
            " kobolds",
            " large kobolds",
            " mumakil (extinct)",
            "",
            "2 species genocided.",
            "1 species extinct.",
        ];
        let g = genocided(&lines).expect("the list");
        assert_eq!(g.heading, GoneHeading::GenocidedOrExtinct);
        let species = |p: &str, extinct: bool| GoneRow::Species {
            plural: p.into(),
            extinct,
        };
        assert_eq!(
            g.rows,
            [
                species("kobolds", false),
                species("large kobolds", false),
                species("mumakil", true),
            ]
        );
        assert_eq!((g.genocided, g.extinct), (Some(2), Some(1)));
        let by_class = [
            "Genocided species:",
            "",
            "Kobold",
            " kobolds",
            "",
            "1 species genocided.",
        ];
        assert_eq!(
            genocided(&by_class).expect("the list").rows,
            [GoneRow::Class("Kobold".into()), species("kobolds", false)]
        );
    }

    #[test]
    fn the_overview_of_a_game_in_progress() {
        let lines = [
            "The Dungeons of Doom: levels 1 to 5",
            "   Level 1:",
            "      Some shops, a temple and an altar to Anhur, many fountains.",
            "   Level 2: \"stash: wands\"",
            "      A general store, a sink.",
            "      Stairs down to The Gnomish Mines.",
            "   Level 5: [oracle] <- You are here.",
            "      Oracle of Delphi.",
            "The Gnomish Mines: levels 3 to 4",
            "   Level 4:",
            "      Final resting place for",
            "         Sven, killed by a dwarf,",
            "         you, killed by a jackal.",
            "Sokoban: levels 6 up to 5",
            "   Level 6:",
            "      Unsolved.",
            "      Stairs down to The Dungeons of Doom, level 6.",
        ];
        let rows = overview(&lines).expect("an overview");
        let level = |n: i32, proto: Option<&str>, note: Option<&str>, here: Option<Here>| {
            OverviewRow::Level {
                place: Place::Level(n),
                proto: proto.map(str::to_string),
                note: note.map(str::to_string),
                here,
            }
        };
        let f = |kind: FeatureKind, seen: Seen, god: Option<&str>| Feature {
            kind,
            seen,
            god: god.map(str::to_string),
        };
        assert_eq!(
            rows,
            [
                OverviewRow::Dungeon {
                    name: "The Dungeons of Doom".into(),
                    levels: Some((1, 5)),
                    up: false,
                },
                level(1, None, None, None),
                OverviewRow::Features(vec![
                    f(FeatureKind::Shop(None), Seen::Two, None),
                    f(FeatureKind::Temple, Seen::One, None),
                    f(FeatureKind::Altar, Seen::One, Some("Anhur")),
                    f(FeatureKind::Fountain, Seen::Many, None),
                ]),
                level(2, None, Some("stash: wands"), None),
                OverviewRow::Features(vec![
                    f(
                        FeatureKind::Shop(Some("general store".into())),
                        Seen::One,
                        None
                    ),
                    f(FeatureKind::Sink, Seen::One, None),
                ]),
                OverviewRow::Branch {
                    kind: BranchKind::Stairs { up: false },
                    to: "The Gnomish Mines".into(),
                    level: None,
                },
                level(5, Some("oracle"), None, Some(Here::Are)),
                OverviewRow::Special(Special::Oracle),
                OverviewRow::Dungeon {
                    name: "The Gnomish Mines".into(),
                    levels: Some((3, 4)),
                    up: false,
                },
                level(4, None, None, None),
                OverviewRow::RestingPlace,
                OverviewRow::Dead {
                    who: Some("Sven".into()),
                    how: "killed by a dwarf".into(),
                },
                OverviewRow::Dead {
                    who: None,
                    how: "killed by a jackal".into(),
                },
                OverviewRow::Dungeon {
                    name: "Sokoban".into(),
                    levels: Some((6, 5)),
                    up: true,
                },
                level(6, None, None, None),
                OverviewRow::Special(Special::Sokoban { solved: false }),
                OverviewRow::Branch {
                    kind: BranchKind::Stairs { up: false },
                    to: "The Dungeons of Doom".into(),
                    level: Some(6),
                },
            ]
        );
    }

    #[test]
    fn the_overview_of_the_endgame_and_as_a_menu() {
        // the end of the game: the planes, no level numbers
        let lines = [
            "The Elemental Planes:",
            "   Plane of Earth:",
            "   Astral Plane: <- You were here.",
        ];
        let rows = overview(&lines).expect("an overview");
        assert_eq!(
            rows[1..],
            [
                OverviewRow::Level {
                    place: Place::Plane("Earth".into()),
                    proto: None,
                    note: None,
                    here: None,
                },
                OverviewRow::Level {
                    place: Place::Astral,
                    proto: None,
                    note: None,
                    here: Some(Here::Were),
                },
            ]
        );
        // `m` #overview: a menu, the levels without their indent
        let menu = ["The Dungeons of Doom:", "Level 1: <- You are here."];
        assert!(matches!(
            overview(&menu).expect("an overview")[1],
            OverviewRow::Level {
                here: Some(Here::Are),
                ..
            }
        ));
        // the castle and its tune, a summons, a portal
        let lines = [
            "The Dungeons of Doom: levels 1 to 27",
            "   Level 27:",
            "      The castle (play notes \"ABCDE\" to open or close drawbridge).",
            "      Summoned by Norn.",
            "      Portal to The Quest.",
        ];
        let rows = overview(&lines).expect("an overview");
        assert_eq!(
            rows[2..],
            [
                OverviewRow::Special(Special::Castle(Some(Tune::Notes("ABCDE".into())))),
                OverviewRow::Summoned("Norn".into()),
                OverviewRow::Branch {
                    kind: BranchKind::Portal,
                    to: "The Quest".into(),
                    level: None,
                },
            ]
        );
        // another menu
        assert_eq!(overview(&["Weapons", "a - a long sword"]), None);
    }

    #[test]
    fn the_overview_in_english_words() {
        let feature = |kind: FeatureKind, seen: Seen, god: Option<&str>| {
            feature_text(&Feature {
                kind,
                seen,
                god: god.map(str::to_string),
            })
        };
        assert_eq!(feature(FeatureKind::Fountain, Seen::One, None), "fountain");
        assert_eq!(
            feature(FeatureKind::Fountain, Seen::Two, None),
            "2 fountains"
        );
        assert_eq!(
            feature(FeatureKind::Altar, Seen::Many, Some("Anhur")),
            "many altars to Anhur"
        );
        assert_eq!(
            feature(
                FeatureKind::Shop(Some("general store".into())),
                Seen::One,
                None
            ),
            "general store"
        );
        assert_eq!(
            branch(BranchKind::Stairs { up: true }, "Sokoban", Some(6)),
            "Stairs up to Sokoban, level 6"
        );
        assert_eq!(place(&Place::Level(3)), "Level 3");
        assert_eq!(levels(1, 5, false), "levels 1–5");
        assert_eq!(dead(None, "killed by a jackal"), "you, killed by a jackal");
        assert_eq!(resting(true), resting(false));
    }

    #[test]
    fn the_overview_in_russian_words() {
        crate::i18n::set_lang(Lang::Ru);
        let shown = [
            // a place in the accusative, a god and a leader in the genitive
            branch(BranchKind::Stairs { up: false }, "The Gnomish Mines", None),
            feature_text(&Feature {
                kind: FeatureKind::Altar,
                seen: Seen::One,
                god: Some("Anhur".into()),
            }),
            special(&Special::QuestGiven("Norn".into())),
            levels(1, 5, false),
            class_name("Dog or other canine"),
            // the hero under a heading of their own, the death's words alone
            resting(true),
            dead(None, "killed by a jackal"),
        ];
        crate::i18n::set_lang(Lang::En);
        assert_eq!(
            shown,
            [
                "Лестница вниз в Гномьи копи",
                "алтарь Анхура",
                "Задание получено от Норны",
                "уровни 1–5",
                "Собака или другое псовое",
                "Здесь покоитесь вы",
                "killed by a jackal",
            ]
        );
    }
}
