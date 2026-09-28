//! Time in the style of BG3 (spec 4): the player gives orders — walk to a
//! cell, hold a key, search or rest for a while, go to the stairs — and the
//! client carries them out one engine action per tick while nothing hostile
//! is in view; in a fight every action waits for the player. An order stops
//! on anything that deserves the player's eye, and on any key or click,
//! before its next step.
//!
//! Everything here is plain data: the game asks for the next action when a
//! command prompt waits and the tick has passed, and answers that very
//! prompt with it. The engine's multi-turn commands (`G`, `_`, counts) are
//! never used: an order is a run of single steps, so it can always stop.

use std::collections::{BTreeSet, VecDeque};
use std::time::Duration;

use nh_protocol::{Catalog, Glyph, GlyphKind, mg};

use crate::path::{Passage, Reach, chebyshev, dir_key, find_path, key_dir, passage};
use crate::{
    Key, KeyContext, KeyInput, KeyProfile, Prompt, Terrain, Who, World, bar_slot, cell_terrain,
    in_field, nethack_key, parse_attack,
};

/// The tick: one engine action per tick while an order runs.
pub const DEFAULT_TICK_MS: u64 = 300;
pub const MIN_TICK_MS: u64 = 150;
pub const MAX_TICK_MS: u64 = 500;
/// Turns in a row without a visible threat that end a fight.
pub const CALM_TURNS: i64 = 3;
/// Searches a rest takes at most.
const REST_LIMIT: u32 = 1000;
/// The highest count a prefix takes (NetHack's LARGEST_INT).
const MAX_COUNT: u32 = 32767;

/// Species that are peaceful unless they show otherwise.
const PEACEFUL_BY_DEFAULT: [&str; 8] = [
    "shopkeeper",
    "watchman",
    "watch captain",
    "aligned priest",
    "high priest",
    "Oracle",
    "guard",
    "prisoner",
];

/// Messages that never stop an order: a pet swapped, sounds from afar, a
/// door the walk opened.
const HARMLESS: [&str; 4] = [
    "You swap places with",
    "You displace",
    "You hear",
    "The door opens.",
];

/// "Something lies here" when the hero steps onto objects.
const ITEMS_HERE: [&str; 4] = [
    "You see here",
    "There are several objects here",
    "There are many objects here",
    "There is ",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No hostile in view: orders run on the tick.
    Exploration,
    /// Every action waits for the player.
    Combat,
}

/// What a walk does when it gets there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    None,
    /// `,`: the engine shows its pick-up menu when there are several things.
    PickUp,
    /// `<` or `>`.
    Stairs(char),
    /// `o` and the direction, from a cell straight in front of the door.
    Open,
    /// `F` and the direction, from a cell next to the monster.
    Attack,
}

impl Arrival {
    pub fn reach(self) -> Reach {
        match self {
            Arrival::Open => Reach::Orthogonal,
            Arrival::Attack => Reach::Adjacent,
            _ => Reach::Onto,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Walk to `goal` by the known map (the way found again every step).
    Walk { goal: (i32, i32), arrival: Arrival },
    /// A key `left` more times (a count: `20s`, `5l`).
    Repeat { key: char, left: u32 },
    /// A key again every tick while the player holds it down.
    Hold { key: char },
    /// Search until HP and Pw are full.
    Rest,
}

/// Why an order ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The walk got there (and did what it came for).
    Arrived,
    /// The count ran out.
    Done,
    /// HP and Pw are full.
    Healed,
    /// In a fight an order takes one action.
    OneAction,
    /// The held key was let go.
    Released,
    /// The player pressed a key.
    Key,
    /// The player clicked the map.
    Click,
    /// The player opened a panel.
    Panel,
    /// The window lost the focus.
    Focus,
    /// The engine asks something the order did not expect.
    Question,
    /// A hostile came into view.
    Hostile,
    HpLost,
    Hunger,
    /// A new condition (confusion, blindness...).
    Condition,
    /// A message worth reading.
    Message(String),
    /// Another level.
    Level,
    /// The step went nowhere.
    Blocked,
    /// No known way there.
    NoPath,
    /// A new game or the end of one.
    Reset,
}

impl Stop {
    /// The order did what it was for (nothing to tell the player).
    pub fn is_completion(&self) -> bool {
        matches!(
            self,
            Stop::Arrived | Stop::Done | Stop::Healed | Stop::OneAction | Stop::Released
        )
    }

    /// What the HUD says about it.
    pub fn says(&self) -> String {
        match self {
            Stop::Arrived => "Arrived".into(),
            Stop::Done => "Done".into(),
            Stop::Healed => "Rested: HP and Pw are full".into(),
            Stop::OneAction => "One action in a fight".into(),
            Stop::Released => "Released".into(),
            Stop::Key | Stop::Click => "Stopped".into(),
            Stop::Panel => "Stopped: a panel opened".into(),
            Stop::Focus => "Stopped: the window lost focus".into(),
            Stop::Question => "Stopped: a question".into(),
            Stop::Hostile => "Stopped: a hostile in view".into(),
            Stop::HpLost => "Stopped: you are hurt".into(),
            Stop::Hunger => "Stopped: hunger changed".into(),
            Stop::Condition => "Stopped: your condition changed".into(),
            Stop::Message(m) => format!("Stopped: {m}"),
            Stop::Level => "Stopped: another level".into(),
            Stop::Blocked => "Stopped: the way is blocked".into(),
            Stop::NoPath => "No known way there".into(),
            Stop::Reset => "Stopped".into(),
        }
    }
}

/// One engine action: `keys[0]` answers the command prompt waiting now,
/// the rest answer the prompts it brings (the direction after `F` or `o`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub keys: Vec<char>,
}

impl Action {
    fn key(c: char) -> Action {
        Action { keys: vec![c] }
    }
}

/// Species known to be peaceful: the usual ones, and those the messages
/// call peaceful. Without the engine's MG_PEACEFUL (patch P1) this is the
/// best there is: an unknown peaceful counts as hostile until the player
/// looks at it (`;` says "peaceful").
#[derive(Debug, Clone)]
pub struct Peaceful {
    names: BTreeSet<String>,
}

impl Default for Peaceful {
    fn default() -> Self {
        Peaceful {
            names: PEACEFUL_BY_DEFAULT.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Every name a monster goes by: its species, male and female forms.
fn names_of(catalog: &Catalog, mon: i32) -> Vec<&str> {
    usize::try_from(mon)
        .ok()
        .and_then(|i| catalog.monsters.get(i))
        .map(|m| {
            let mut n = vec![m.name.as_str()];
            n.extend(m.male.as_deref());
            n.extend(m.female.as_deref());
            n
        })
        .unwrap_or_default()
}

impl Peaceful {
    pub fn is_peaceful(&self, catalog: &Catalog, mon: i32) -> bool {
        names_of(catalog, mon)
            .iter()
            .any(|n| self.names.contains(*n))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// "peaceful <name>" anywhere in `text` (a farlook, "Really attack the
    /// peaceful gnome lord?"): that species is peaceful.
    pub fn learn(&mut self, text: &str, catalog: &Catalog) {
        for (at, _) in text.match_indices("peaceful ") {
            let rest = &text[at + "peaceful ".len()..];
            let best = catalog
                .monsters
                .iter()
                .flat_map(|m| {
                    std::iter::once(m.name.as_str())
                        .chain(m.male.as_deref())
                        .chain(m.female.as_deref())
                })
                .filter(|n| {
                    rest.starts_with(n)
                        && !rest[n.len()..].starts_with(|c: char| c.is_alphanumeric())
                })
                .max_by_key(|n| n.len());
            if let Some(n) = best {
                self.names.insert(n.to_string());
            }
        }
    }

    /// It attacked the hero: not peaceful any more.
    pub fn forget(&mut self, name: &str) {
        self.names.remove(name);
    }
}

/// Is this glyph at `at` something to fight? A monster that is not the
/// hero, not a pet, not sensed without being seen, not a known peaceful;
/// while hallucinating every monster; a remembered unseen one next to the
/// hero.
pub fn is_threat(
    g: &Glyph,
    at: (i32, i32),
    hero: Option<(i32, i32)>,
    hallucinating: bool,
    peaceful: &Peaceful,
    catalog: &Catalog,
) -> bool {
    match g.kind {
        GlyphKind::Mon => {
            if g.flags & (mg::HERO | mg::DETECT) != 0 {
                false
            } else if hallucinating {
                true
            } else if g.flags & mg::PET != 0 {
                false
            } else {
                !g.mon.is_some_and(|m| peaceful.is_peaceful(catalog, m))
            }
        }
        GlyphKind::Invisible => hero.is_some_and(|h| chebyshev(h, at) == 1),
        _ => false,
    }
}

/// Is the hero hallucinating (condition "Hallu")?
pub fn hallucinating(world: &World, catalog: &Catalog) -> bool {
    catalog
        .conditions
        .iter()
        .any(|c| c.name == "Hallu" && world.status.conditions() & c.mask != 0)
}

/// Cells with a threat on them.
pub fn threats(world: &World, catalog: &Catalog, peaceful: &Peaceful) -> Vec<(i32, i32)> {
    let hero = world.hero();
    let hallu = hallucinating(world, catalog);
    let mut out = Vec::new();
    for y in 0..crate::ROWNO {
        for x in 1..crate::COLNO {
            let Some(g) = world.map.cell(x, y).and_then(|c| c.entity()) else {
                continue;
            };
            if is_threat(g, (x, y), hero, hallu, peaceful, catalog) {
                out.push((x, y));
            }
        }
    }
    out
}

/// What a left click on a cell asks for (spec 7.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickPlan {
    Order(Order),
    /// The engine's own click: the hero's cell (its "here" actions).
    Engine,
    /// Nothing to do there, and why.
    Nothing(&'static str),
}

/// A left click at `at`: a monster — attack it (a pet — walk there); an
/// object — walk there and pick it up; stairs — walk there and use them; a
/// closed door — walk up and open it; the hero — the engine's menu;
/// anything else seen — walk there.
pub fn click_order(world: &World, catalog: &Catalog, at: (i32, i32)) -> ClickPlan {
    if !in_field(at.0, at.1) {
        return ClickPlan::Nothing("off the map");
    }
    if world.hero() == Some(at) {
        return ClickPlan::Engine;
    }
    let Some(cell) = world.map.cell(at.0, at.1) else {
        return ClickPlan::Nothing("off the map");
    };
    let walk = |arrival| ClickPlan::Order(Order::Walk { goal: at, arrival });
    if let Some(g) = cell.entity() {
        match g.kind {
            GlyphKind::Mon if g.flags & mg::PET != 0 => return walk(Arrival::None),
            GlyphKind::Mon | GlyphKind::Invisible => return walk(Arrival::Attack),
            GlyphKind::Obj | GlyphKind::Body if passage(cell, catalog) != Passage::Occupied => {
                return walk(Arrival::PickUp);
            }
            _ => {}
        }
    }
    match cell_terrain(cell, catalog) {
        None => {
            if cell.entity().is_some() {
                walk(Arrival::None)
            } else {
                ClickPlan::Nothing("unexplored")
            }
        }
        Some(Terrain::StairsUp | Terrain::LadderUp) => walk(Arrival::Stairs('<')),
        Some(Terrain::StairsDown | Terrain::LadderDown) => walk(Arrival::Stairs('>')),
        Some(Terrain::ClosedDoor) => walk(Arrival::Open),
        Some(_) => walk(Arrival::None),
    }
}

/// `<` or `>` away from such stairs: walk to the nearest known ones and use
/// them. None when the hero stands on them (the key is the engine's) or no
/// known stairs can be reached.
pub fn stairs_order(world: &World, catalog: &Catalog, key: char) -> Option<Order> {
    let wanted: &[Terrain] = match key {
        '<' => &[Terrain::StairsUp, Terrain::LadderUp],
        '>' => &[Terrain::StairsDown, Terrain::LadderDown],
        _ => return None,
    };
    let hero = world.hero()?;
    let is_stairs = |(x, y): (i32, i32)| {
        world
            .map
            .cell(x, y)
            .and_then(|c| cell_terrain(c, catalog))
            .is_some_and(|t| wanted.contains(&t))
    };
    if is_stairs(hero) {
        return None;
    }
    let mut best: Option<(usize, (i32, i32))> = None;
    for y in 0..crate::ROWNO {
        for x in 1..crate::COLNO {
            if !is_stairs((x, y)) {
                continue;
            }
            if let Some(way) = find_path(&world.map, catalog, hero, (x, y), Reach::Onto)
                && best.is_none_or(|(n, _)| way.len() < n)
            {
                best = Some((way.len(), (x, y)));
            }
        }
    }
    best.map(|(_, goal)| Order::Walk {
        goal,
        arrival: Arrival::Stairs(key),
    })
}

/// Keys an order may repeat: a step, a search, a rest.
pub fn repeatable(key: char, dirchars: &str) -> bool {
    key == 's' || key == '.' || key_dir(dirchars, key).is_some()
}

/// A count typed before a command (digits; with number_pad, `n` and
/// digits). The client keeps it: the engine never sees a count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CountEntry {
    n: Option<u32>,
    prefix: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    /// The key went into the count.
    Typing,
    /// A command key, with the count typed before it (if any).
    Command(Option<u32>),
}

/// What a key pressed at the command prompt does under a key profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandInput {
    /// It went into the count (or cleared it: Esc).
    Typing,
    /// It means nothing here.
    Ignored,
    /// A NetHack command key, with the count typed before it.
    Command { key: i32, count: Option<u32> },
    /// A top-row digit: action bar slot 0–9, with the count before it.
    Bar { slot: usize, count: Option<u32> },
}

impl CountEntry {
    /// A command key (NetHack's code) at a command prompt.
    pub fn feed(&mut self, key: i32, number_pad: bool) -> Counted {
        let c = u32::try_from(key).ok().and_then(char::from_u32);
        let typing = self.typing();
        match c {
            Some('n') if number_pad && !typing => {
                self.prefix = true;
                Counted::Typing
            }
            Some(d @ '0'..='9') if (!number_pad || self.prefix) && (typing || d != '0') => {
                self.digit(d);
                Counted::Typing
            }
            // Esc forgets the count
            Some('\u{1b}') if typing => {
                self.clear();
                Counted::Typing
            }
            _ => Counted::Command(self.take()),
        }
    }

    /// A key at a command prompt under `profile`. The top-row digits are
    /// the action bar, except after Modern's `n`, where they are the
    /// count (a slot then takes the count by a click, see [`take`]).
    /// Classic's count is Alt and the top-row digits. The keypad moves in
    /// both (after `n` its digits are the count, as in tty).
    ///
    /// [`take`]: CountEntry::take
    pub fn feed_key(
        &mut self,
        input: &KeyInput,
        profile: KeyProfile,
        dirchars: &str,
    ) -> CommandInput {
        if let Key::Char(d @ '0'..='9') = input.key
            && !input.mods.ctrl
        {
            let counting = match profile {
                KeyProfile::Modern if input.mods.alt => return CommandInput::Ignored,
                KeyProfile::Modern => self.prefix,
                KeyProfile::Classic => input.mods.alt,
            };
            if !counting {
                return CommandInput::Bar {
                    slot: bar_slot(d).unwrap_or(0),
                    count: self.take(),
                };
            }
            if !self.typing() && d == '0' {
                return CommandInput::Ignored;
            }
            self.digit(d);
            return CommandInput::Typing;
        }
        let number_pad = profile.number_pad();
        let Some(key) = nethack_key(input, KeyContext::Command, number_pad, dirchars) else {
            return CommandInput::Ignored;
        };
        match self.feed(key, number_pad) {
            Counted::Typing => CommandInput::Typing,
            Counted::Command(count) => CommandInput::Command { key, count },
        }
    }

    /// The count typed so far, and a fresh start (a click on a bar slot).
    pub fn take(&mut self) -> Option<u32> {
        self.prefix = false;
        self.n.take()
    }

    fn typing(&self) -> bool {
        self.n.is_some() || self.prefix
    }

    fn digit(&mut self, d: char) {
        let d = d.to_digit(10).unwrap_or(0);
        self.n = Some(
            self.n
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(d)
                .min(MAX_COUNT),
        );
    }

    /// "Count: 20" while one is typed.
    pub fn shown(&self) -> Option<String> {
        match (self.n, self.prefix) {
            (Some(n), _) => Some(format!("Count: {n}")),
            (None, true) => Some("Count: ".to_string()),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        *self = CountEntry::default();
    }
}

/// The world as it was when an action went out.
#[derive(Debug, Clone)]
struct Before {
    hero: Option<(i32, i32)>,
    hunger: String,
    conditions: u64,
    generation: u64,
    dlvl: Option<i64>,
    seq: u64,
    /// A step: the cell it went for and what showed there.
    target: Option<((i32, i32), Option<Glyph>)>,
}

impl Before {
    fn of(world: &World, target: Option<(i32, i32)>) -> Before {
        Before {
            hero: world.hero(),
            hunger: world.status.get("hunger").unwrap_or("").to_string(),
            conditions: world.status.conditions(),
            generation: world.map.generation(),
            dlvl: world.status.number("leveldesc"),
            seq: world.log.last_seq(),
            target: target.map(|(x, y)| {
                let look = world.map.cell(x, y).and_then(|c| c.terrain.clone());
                ((x, y), look)
            }),
        }
    }
}

#[derive(Debug, Clone)]
struct Active {
    order: Order,
    /// In a fight: one action, then the order ends.
    one_shot: bool,
    actions: u32,
    before: Option<Before>,
    /// The last action was the arrival's: the order ends with it.
    finishing: bool,
    /// What an attack aims at, to follow it when it moves.
    target: Option<Glyph>,
}

/// Runs orders: the next action when the tick has passed, the mode, and
/// every reason to stop (see the module doc).
#[derive(Debug, Clone)]
pub struct TickDriver {
    tick: Duration,
    mode: Mode,
    active: Option<Active>,
    follow: VecDeque<char>,
    stop: Option<Stop>,
    peaceful: Peaceful,
    /// Messages already looked at.
    seen_seq: u64,
    last_hp: Option<i64>,
    /// The turn a threat was last seen (or a blow struck).
    last_threat: Option<i64>,
    /// Command prompts observed: the turn when the status has no T:.
    observed: i64,
}

impl Default for TickDriver {
    fn default() -> Self {
        TickDriver::new()
    }
}

impl TickDriver {
    pub fn new() -> TickDriver {
        TickDriver {
            tick: Duration::from_millis(DEFAULT_TICK_MS),
            mode: Mode::Exploration,
            active: None,
            follow: VecDeque::new(),
            stop: None,
            peaceful: Peaceful::default(),
            seen_seq: 0,
            last_hp: None,
            last_threat: None,
            observed: 0,
        }
    }

    /// The player's setting, 150–500 ms.
    pub fn set_tick_ms(&mut self, ms: u64) {
        self.tick = Duration::from_millis(ms.clamp(MIN_TICK_MS, MAX_TICK_MS));
    }

    /// Any tick (self-tests run faster than a player may choose).
    pub fn set_test_tick(&mut self, tick: Duration) {
        self.tick = tick;
    }

    pub fn tick(&self) -> Duration {
        self.tick
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn peaceful(&self) -> &Peaceful {
        &self.peaceful
    }

    pub fn order(&self) -> Option<&Order> {
        self.active.as_ref().map(|a| &a.order)
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// A new game: no order, exploring, nothing learned.
    pub fn reset(&mut self) {
        let tick = self.tick;
        *self = TickDriver::new();
        self.tick = tick;
    }

    /// Start an order at a command prompt: its first action, to send now.
    pub fn start(
        &mut self,
        order: Order,
        world: &World,
        catalog: &Catalog,
    ) -> Result<Action, Stop> {
        let one_shot = self.mode == Mode::Combat && !matches!(order, Order::Hold { .. });
        let target = match order {
            Order::Walk {
                goal,
                arrival: Arrival::Attack,
            } => world
                .map
                .cell(goal.0, goal.1)
                .and_then(|c| c.entity().cloned()),
            _ => None,
        };
        self.follow.clear();
        self.stop = None;
        self.active = Some(Active {
            order,
            one_shot,
            actions: 0,
            before: None,
            finishing: false,
            target,
        });
        self.next(world, catalog)
            .ok_or_else(|| self.stop.take().unwrap_or(Stop::Done))
    }

    /// End the order (the player, a panel, a question...).
    pub fn interrupt(&mut self, why: Stop) {
        if self.active.take().is_some() {
            self.stop = Some(why);
        }
        self.follow.clear();
    }

    /// Why the last order ended, once.
    pub fn take_stop(&mut self) -> Option<Stop> {
        self.stop.take()
    }

    fn end(&mut self, why: Stop) -> Option<Action> {
        self.interrupt(why);
        None
    }

    /// May the next action go? The tick has passed since the last one, and
    /// the hero's step has been played (or two ticks have passed).
    pub fn ready(&self, since_last: Duration, hero_walking: bool) -> bool {
        since_last >= self.tick && (!hero_walking || since_last >= self.tick * 2)
    }

    /// An action is under way: its next key answers the prompt that comes.
    pub fn expects_follow_up(&self) -> bool {
        !self.follow.is_empty()
    }

    /// The key for a prompt the action brought: a command or a direction
    /// question. Anything else stops the order (the player answers it).
    pub fn follow_up(&mut self, prompt: &Prompt) -> Option<char> {
        if self.follow.is_empty() {
            return None;
        }
        match prompt {
            Prompt::Command
            | Prompt::FreeKey {
                directions: true, ..
            } => self.follow.pop_front(),
            _ => {
                self.interrupt(Stop::Question);
                None
            }
        }
    }

    /// The engine asked something the order did not expect.
    pub fn on_question(&mut self, query: Option<&str>, catalog: &Catalog) {
        if let Some(q) = query {
            self.peaceful.learn(q, catalog);
        }
        if self.follow.is_empty() {
            self.interrupt(Stop::Question);
        }
    }

    /// The next action of the order, at a command prompt once `ready`; None
    /// when it has ended (`take_stop` says why).
    pub fn next(&mut self, world: &World, catalog: &Catalog) -> Option<Action> {
        let active = self.active.as_mut()?;
        if active.finishing {
            return self.end(Stop::Arrived);
        }
        if active.one_shot && active.actions > 0 {
            return self.end(Stop::OneAction);
        }
        let dirchars = world.dirchars.clone();
        let hero = world.hero();
        let (action, target, finishing) = match active.order.clone() {
            Order::Walk { goal, arrival } => {
                let Some(hero) = hero else {
                    return self.end(Stop::Blocked);
                };
                let goal = if arrival == Arrival::Attack {
                    let found = follow_target(world, goal, active.target.as_ref());
                    if let Some(g) = found {
                        active.order = Order::Walk { goal: g, arrival };
                    }
                    found.unwrap_or(goal)
                } else {
                    goal
                };
                let reach = arrival.reach();
                if reach.reached(hero, goal) {
                    let dir = dir_key(&dirchars, goal.0 - hero.0, goal.1 - hero.1);
                    let action = match arrival {
                        Arrival::None => None,
                        Arrival::PickUp => Some(Action::key(',')),
                        Arrival::Stairs(c) => Some(Action::key(c)),
                        Arrival::Open => {
                            let closed = world
                                .map
                                .cell(goal.0, goal.1)
                                .and_then(|c| cell_terrain(c, catalog))
                                == Some(Terrain::ClosedDoor);
                            dir.filter(|_| closed)
                                .map(|d| Action { keys: vec!['o', d] })
                        }
                        Arrival::Attack => {
                            let there = world
                                .map
                                .cell(goal.0, goal.1)
                                .and_then(|c| c.entity())
                                .is_some_and(|g| {
                                    matches!(g.kind, GlyphKind::Mon | GlyphKind::Invisible)
                                        && g.flags & mg::HERO == 0
                                });
                            dir.filter(|_| there).map(|d| Action { keys: vec!['F', d] })
                        }
                    };
                    match action {
                        Some(a) => (a, None, true),
                        None => return self.end(Stop::Arrived),
                    }
                } else {
                    let Some(way) = find_path(&world.map, catalog, hero, goal, reach) else {
                        return self.end(Stop::NoPath);
                    };
                    let Some(&next) = way.first() else {
                        return self.end(Stop::Arrived);
                    };
                    let Some(k) = dir_key(&dirchars, next.0 - hero.0, next.1 - hero.1) else {
                        return self.end(Stop::NoPath);
                    };
                    (Action::key(k), Some(next), false)
                }
            }
            Order::Repeat { key, left } => {
                if left == 0 {
                    return self.end(Stop::Done);
                }
                active.order = Order::Repeat {
                    key,
                    left: left - 1,
                };
                (Action::key(key), step_target(hero, &dirchars, key), false)
            }
            Order::Hold { key } => (Action::key(key), step_target(hero, &dirchars, key), false),
            Order::Rest => {
                let full = |a: &str, b: &str| match (world.status.number(a), world.status.number(b))
                {
                    (Some(v), Some(max)) => v >= max,
                    _ => true,
                };
                if full("hp", "hpmax") && full("energy", "energymax") {
                    return self.end(Stop::Healed);
                }
                if active.actions >= REST_LIMIT {
                    return self.end(Stop::Done);
                }
                (Action::key('s'), None, false)
            }
        };
        active.actions += 1;
        active.finishing = finishing;
        active.before = Some(Before::of(world, target));
        self.follow = action.keys.iter().skip(1).copied().collect();
        Some(action)
    }

    /// At every new command prompt (not a follow-up): learn from the new
    /// messages, update the mode, and stop the order when something
    /// happened since its last action. The new mode when it changed.
    pub fn observe(&mut self, world: &World, catalog: &Catalog) -> Option<Mode> {
        self.observed += 1;
        let turn = world.status.number("time").unwrap_or(self.observed);
        let mut fought = false;
        let mut news = Vec::new();
        let pets = pet_names(world, catalog);
        for m in world.log.since(self.seen_seq).filter(|m| !m.from_history) {
            self.peaceful.learn(&m.text, catalog);
            let mut hero_fights = false;
            if let Some(a) = parse_attack(&m.text) {
                match (&a.attacker, &a.target) {
                    (Who::Named(n), Who::Hero) => {
                        self.peaceful.forget(n);
                        hero_fights = true;
                    }
                    (Who::Hero, _) => hero_fights = true,
                    _ => {}
                }
            }
            fought |= hero_fights;
            let harmless = HARMLESS.iter().any(|h| m.text.starts_with(h))
                || (!hero_fights && about_a_pet(&m.text, &pets));
            news.push(News {
                seq: m.seq,
                text: m.text.clone(),
                urgent: m.urgent,
                harmless,
            });
        }
        self.seen_seq = world.log.last_seq();
        let hp = world.status.number("hp");
        let hurt = matches!((self.last_hp, hp), (Some(a), Some(b)) if b < a);
        self.last_hp = hp;
        let seen = !threats(world, catalog, &self.peaceful).is_empty();
        if seen || hurt || fought {
            self.last_threat = Some(turn);
        }
        let was = self.mode;
        if seen || hurt || fought {
            self.mode = Mode::Combat;
        } else if self.mode == Mode::Combat
            && self.last_threat.is_none_or(|t| turn - t >= CALM_TURNS)
        {
            self.mode = Mode::Exploration;
        }
        let entered_combat = was == Mode::Exploration && self.mode == Mode::Combat;
        if self.active.as_ref().is_some_and(|a| a.finishing) {
            // the arrival's own action was the last
            self.interrupt(Stop::Arrived);
        } else if let Some(why) = self.check(world, entered_combat, hurt, &news) {
            self.interrupt(why);
        }
        (self.mode != was).then_some(self.mode)
    }

    /// What stops the active order after its last action, if anything.
    fn check(
        &self,
        world: &World,
        entered_combat: bool,
        hurt: bool,
        news: &[News],
    ) -> Option<Stop> {
        let active = self.active.as_ref()?;
        let before = active.before.as_ref()?;
        if entered_combat {
            return Some(if hurt { Stop::HpLost } else { Stop::Hostile });
        }
        if hurt {
            return Some(Stop::HpLost);
        }
        if world.status.get("hunger").unwrap_or("") != before.hunger {
            return Some(Stop::Hunger);
        }
        if world.status.conditions() & !before.conditions != 0 {
            return Some(Stop::Condition);
        }
        if world.map.generation() != before.generation
            || world.status.number("leveldesc") != before.dlvl
        {
            return Some(Stop::Level);
        }
        let hero = world.hero();
        // things lie where the walk ends: its arrival deals with them
        let at_goal = match &active.order {
            Order::Walk { goal, arrival } => {
                hero.is_some_and(|h| arrival.reach().reached(h, *goal))
            }
            _ => false,
        };
        for n in news.iter().filter(|n| n.seq > before.seq) {
            let items = ITEMS_HERE.iter().any(|h| n.text.starts_with(h));
            if n.urgent || !(n.harmless || (items && at_goal)) {
                return Some(Stop::Message(n.text.clone()));
            }
        }
        if let Some((cell, look)) = &before.target
            && hero == before.hero
            && world
                .map
                .cell(cell.0, cell.1)
                .and_then(|c| c.terrain.clone())
                == *look
        {
            return Some(Stop::Blocked);
        }
        None
    }

    /// Learn from a question the engine asks ("Really attack the peaceful
    /// watchman?").
    pub fn note_query(&mut self, query: &str, catalog: &Catalog) {
        self.peaceful.learn(query, catalog);
    }

    /// The way an active walk still has to go, from the hero.
    pub fn planned_path(&self, world: &World, catalog: &Catalog) -> Option<Vec<(i32, i32)>> {
        let Order::Walk { goal, arrival } = self.order()? else {
            return None;
        };
        find_path(&world.map, catalog, world.hero()?, *goal, arrival.reach())
    }
}

/// A message the last command brought.
#[derive(Debug, Clone)]
struct News {
    seq: u64,
    text: String,
    urgent: bool,
    /// Never worth stopping for (unless urgent).
    harmless: bool,
}

/// The names of the pets in view.
fn pet_names<'a>(world: &World, catalog: &'a Catalog) -> Vec<&'a str> {
    let mut names = Vec::new();
    for y in 0..crate::ROWNO {
        for x in 1..crate::COLNO {
            if let Some(g) = world.map.cell(x, y).and_then(|c| c.entity())
                && g.kind == GlyphKind::Mon
                && g.flags & mg::PET != 0
                && let Some(m) = g.mon
            {
                names.extend(names_of(catalog, m));
            }
        }
    }
    names
}

/// "The little dog picks up a bag.", "Your kitten eats a newt corpse.":
/// what a pet in view does.
fn about_a_pet(text: &str, pets: &[&str]) -> bool {
    pets.iter().any(|n| {
        ["The ", "Your "].iter().any(|p| {
            text.strip_prefix(p)
                .and_then(|r| r.strip_prefix(n))
                .is_some_and(|r| r.starts_with(' '))
        })
    })
}

/// Where a step key takes the hero, if it is a step.
fn step_target(hero: Option<(i32, i32)>, dirchars: &str, key: char) -> Option<(i32, i32)> {
    let (x, y) = hero?;
    let (dx, dy) = key_dir(dirchars, key)?;
    Some((x + dx, y + dy))
}

/// The monster an attack aims at: still at `goal`, or the same kind of
/// monster a cell or two away.
fn follow_target(world: &World, goal: (i32, i32), target: Option<&Glyph>) -> Option<(i32, i32)> {
    let target = target?;
    let same = |at: (i32, i32)| {
        world
            .map
            .cell(at.0, at.1)
            .and_then(|c| c.entity())
            .is_some_and(|g| g.kind == target.kind && g.mon == target.mon)
    };
    if same(goal) {
        return Some(goal);
    }
    let mut near: Vec<(i32, i32)> = (-2..=2)
        .flat_map(|dy| (-2..=2).map(move |dx| (goal.0 + dx, goal.1 + dy)))
        .filter(|&c| c != goal && same(c))
        .collect();
    near.sort_by_key(|&c| (chebyshev(c, goal), c.1, c.0));
    near.first().copied()
}

#[cfg(test)]
mod tests {
    use nh_protocol::StatusUpdate;

    use super::*;
    use crate::Mods;
    use crate::path::tests::{FOE_MON, HERO_MON, catalog, draw, mon};

    fn stat(world: &mut World, field: &str, value: &str) {
        world.status.apply(&StatusUpdate {
            field: field.into(),
            value: Some(value.into()),
            conds: None,
            chg: 0,
            percent: 0,
            color: 0,
        });
    }

    fn conds(world: &mut World, bits: u64) {
        world.status.apply(&StatusUpdate {
            field: "condition".into(),
            value: None,
            conds: Some(bits),
            chg: 0,
            percent: 0,
            color: 0,
        });
    }

    fn say(world: &mut World, text: &str) {
        world.log.push(text.to_string(), 0, None, false);
    }

    /// A world with a map, the hero on it, full HP, turn 1.
    fn world_of(rows: &[&str]) -> (World, Catalog) {
        let cat = catalog();
        let mut w = World::new();
        w.set_catalog(&cat);
        draw(&mut w.map, &cat, 0, rows);
        for (f, v) in [
            ("hp", "16"),
            ("hpmax", "16"),
            ("energy", "5"),
            ("energymax", "5"),
            ("hunger", ""),
            ("time", "1"),
            ("leveldesc", "Dlvl:1"),
        ] {
            stat(&mut w, f, v);
        }
        (w, cat)
    }

    /// The hero steps from `from` to `to` (as the engine draws it).
    fn step(w: &mut World, cat: &Catalog, from: (i32, i32), to: (i32, i32)) {
        let floor = crate::path::tests::cmap(cat, "S_room");
        w.map
            .print(to.0, to.1, &mon(HERO_MON, mg::HERO), Some(&floor));
        w.map.print(from.0, from.1, &floor, None);
        w.map.take_dirty();
    }

    fn turn(w: &mut World, t: i64) {
        stat(w, "time", &t.to_string());
    }

    const ROOM: [&str; 3] = [
        "|........|", //
        "|@.......|", //
        "|........|",
    ];

    fn keys(a: &Action) -> String {
        a.keys.iter().collect()
    }

    #[test]
    fn walk_steps_one_cell_per_action_and_arrives() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        assert_eq!(d.observe(&w, &cat), None);
        let order = Order::Walk {
            goal: (5, 1),
            arrival: Arrival::None,
        };
        let a = d.start(order, &w, &cat).unwrap();
        assert_eq!(keys(&a), "l");
        let mut at = (2, 1);
        for t in 2..4 {
            step(&mut w, &cat, at, (at.0 + 1, 1));
            at.0 += 1;
            turn(&mut w, t);
            assert_eq!(d.observe(&w, &cat), None);
            assert!(d.is_active());
            let a = d.next(&w, &cat).unwrap();
            assert_eq!(keys(&a), "l");
        }
        step(&mut w, &cat, at, (5, 1));
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Arrived));
        assert!(!d.is_active());
        assert!(Stop::Arrived.is_completion());
    }

    #[test]
    fn walk_replans_around_a_new_obstacle() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d
            .start(
                Order::Walk {
                    goal: (6, 1),
                    arrival: Arrival::None,
                },
                &w,
                &cat,
            )
            .unwrap();
        assert_eq!(keys(&a), "l");
        step(&mut w, &cat, (2, 1), (3, 1));
        // a boulder shows up on the straight way
        draw(&mut w.map, &cat, 1, &["   0"]);
        turn(&mut w, 2);
        d.observe(&w, &cat);
        let a = d.next(&w, &cat).unwrap();
        assert!(matches!(keys(&a).as_str(), "u" | "n"), "round it: {a:?}");
        // the way is walled off: no path
        draw(&mut w.map, &cat, 0, &["   |", "   |", "   |"]);
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::NoPath));
    }

    #[test]
    fn arrival_picks_up_climbs_opens_and_attacks() {
        let rows = [
            "|.....|", //
            "|@$.>.|", //
            "---+---",
        ];
        let (mut w, cat) = world_of(&rows);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        // an object: walk there, pick up, done
        let plan = click_order(&w, &cat, (3, 1));
        let ClickPlan::Order(order) = plan else {
            panic!("{plan:?}");
        };
        assert_eq!(keys(&d.start(order, &w, &cat).unwrap()), "l");
        step(&mut w, &cat, (2, 1), (3, 1));
        say(&mut w, "You see here a gold piece.");
        d.observe(&w, &cat);
        assert!(
            d.is_active(),
            "an item at the goal is what the walk came for"
        );
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), ",");
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Arrived));
        // the stairs: walk and go down
        let ClickPlan::Order(order) = click_order(&w, &cat, (5, 1)) else {
            panic!();
        };
        assert_eq!(
            order,
            Order::Walk {
                goal: (5, 1),
                arrival: Arrival::Stairs('>')
            }
        );
        assert_eq!(stairs_order(&w, &cat, '>'), Some(order.clone()));
        assert_eq!(stairs_order(&w, &cat, '<'), None, "no known way up");
        d.start(order, &w, &cat).unwrap();
        step(&mut w, &cat, (3, 1), (4, 1));
        d.observe(&w, &cat);
        d.next(&w, &cat).unwrap();
        step(&mut w, &cat, (4, 1), (5, 1));
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), ">");
        assert_eq!(
            stairs_order(&w, &cat, '>'),
            None,
            "on the stairs: the engine's key"
        );
        // a closed door: from straight in front of it, 'o' and the direction
        let ClickPlan::Order(order) = click_order(&w, &cat, (4, 2)) else {
            panic!();
        };
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d.start(order, &w, &cat).unwrap();
        assert_eq!(keys(&a), "h", "to the cell above the door");
        step(&mut w, &cat, (5, 1), (4, 1));
        d.observe(&w, &cat);
        let a = d.next(&w, &cat).unwrap();
        assert_eq!(keys(&a), "oj");
        assert!(d.expects_follow_up());
        let dir = Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        };
        assert_eq!(d.follow_up(&dir), Some('j'));
        assert!(!d.expects_follow_up());
        // a monster next to the hero: F and the direction
        draw(&mut w.map, &cat, 0, &["   J"]);
        let ClickPlan::Order(order) = click_order(&w, &cat, (4, 0)) else {
            panic!();
        };
        let mut d = TickDriver::new();
        let a = d.start(order, &w, &cat).unwrap();
        assert_eq!(keys(&a), "Fk");
        assert_eq!(d.follow_up(&Prompt::Command), Some('k'));
        // the hero: the engine's own menu
        assert_eq!(click_order(&w, &cat, (4, 1)), ClickPlan::Engine);
        assert_eq!(
            click_order(&w, &cat, (40, 10)),
            ClickPlan::Nothing("unexplored")
        );
    }

    #[test]
    fn an_attack_follows_its_target() {
        let rows = ["|@.....|"];
        let (mut w, cat) = world_of(&rows);
        draw(&mut w.map, &cat, 0, &["      J"]);
        let mut d = TickDriver::new();
        // a jackal known to be peaceful: no fight, the walk goes on
        d.note_query("Really attack the peaceful jackal?", &cat);
        assert_eq!(d.observe(&w, &cat), None);
        let order = Order::Walk {
            goal: (7, 0),
            arrival: Arrival::Attack,
        };
        assert_eq!(keys(&d.start(order, &w, &cat).unwrap()), "l");
        step(&mut w, &cat, (2, 0), (3, 0));
        // it comes a cell closer: the walk follows it
        draw(&mut w.map, &cat, 0, &["     J."]);
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        step(&mut w, &cat, (3, 0), (4, 0));
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        step(&mut w, &cat, (4, 0), (5, 0));
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "Fl");
        assert_eq!(
            d.order(),
            Some(&Order::Walk {
                goal: (6, 0),
                arrival: Arrival::Attack
            })
        );
        // gone: nothing to strike, the walk ends
        let mut d = TickDriver::new();
        let order = Order::Walk {
            goal: (6, 0),
            arrival: Arrival::Attack,
        };
        draw(&mut w.map, &cat, 0, &["     ."]);
        assert_eq!(d.start(order, &w, &cat), Err(Stop::Arrived));
    }

    #[test]
    fn repeat_sends_the_key_n_times() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d
            .start(Order::Repeat { key: 's', left: 3 }, &w, &cat)
            .unwrap();
        assert_eq!(keys(&a), "s");
        for t in 2..4 {
            turn(&mut w, t);
            d.observe(&w, &cat);
            assert_eq!(keys(&d.next(&w, &cat).unwrap()), "s");
        }
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Done));
        // steps: five cells east, a wall stops it early
        let a = d
            .start(Order::Repeat { key: 'l', left: 20 }, &w, &cat)
            .unwrap();
        assert_eq!(keys(&a), "l");
        let mut at = (2, 1);
        while at.0 < 9 {
            step(&mut w, &cat, at, (at.0 + 1, 1));
            at.0 += 1;
            d.observe(&w, &cat);
            assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        }
        // the engine did not move the hero into the wall
        d.observe(&w, &cat);
        assert_eq!(d.take_stop(), Some(Stop::Blocked));
    }

    #[test]
    fn rest_stops_when_hp_and_pw_are_full() {
        let (w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        assert_eq!(d.start(Order::Rest, &w, &cat), Err(Stop::Healed));
        let (mut w, cat) = world_of(&ROOM);
        stat(&mut w, "hp", "10");
        stat(&mut w, "energy", "2");
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        assert_eq!(keys(&d.start(Order::Rest, &w, &cat).unwrap()), "s");
        stat(&mut w, "hp", "16");
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "s", "Pw is not full yet");
        stat(&mut w, "energy", "5");
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Healed));
    }

    #[test]
    fn hold_repeats_until_released() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d.start(Order::Hold { key: 'l' }, &w, &cat).unwrap();
        assert_eq!(keys(&a), "l");
        step(&mut w, &cat, (2, 1), (3, 1));
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        d.interrupt(Stop::Released);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Released));
        assert!(repeatable('l', "hykulnjb><"));
        assert!(repeatable('s', "hykulnjb><"));
        assert!(repeatable('.', "hykulnjb><"));
        assert!(!repeatable('i', "hykulnjb><"));
        assert!(!repeatable('L', "hykulnjb><"), "running is the engine's");
        assert!(!repeatable('>', "hykulnjb><"));
    }

    #[test]
    fn combat_orders_take_one_action() {
        let (mut w, cat) = world_of(&[
            "|@.......|", //
            "|.......J|",
        ]);
        let mut d = TickDriver::new();
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        let a = d
            .start(
                Order::Walk {
                    goal: (6, 0),
                    arrival: Arrival::None,
                },
                &w,
                &cat,
            )
            .unwrap();
        assert_eq!(keys(&a), "l");
        step(&mut w, &cat, (2, 0), (3, 0));
        turn(&mut w, 2);
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::OneAction));
        // a search count: one search
        d.start(Order::Repeat { key: 's', left: 5 }, &w, &cat)
            .unwrap();
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        // holding a key goes on
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "s");
    }

    #[test]
    fn combat_starts_on_a_hostile_and_ends_after_three_quiet_turns() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        assert_eq!(d.observe(&w, &cat), None);
        assert_eq!(d.mode(), Mode::Exploration);
        // a pet is no threat, nor a monster sensed but not seen
        draw(&mut w.map, &cat, 0, &["  f"]);
        w.map.print(8, 2, &mon(FOE_MON, mg::DETECT), None);
        assert_eq!(d.observe(&w, &cat), None);
        // a jackal
        draw(&mut w.map, &cat, 2, &["       J"]);
        turn(&mut w, 5);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        assert_eq!(threats(&w, &cat, d.peaceful()), vec![(8, 2)]);
        // it dies
        draw(&mut w.map, &cat, 2, &["       ."]);
        turn(&mut w, 6);
        assert_eq!(d.observe(&w, &cat), None);
        turn(&mut w, 7);
        assert_eq!(d.observe(&w, &cat), None, "two quiet turns");
        turn(&mut w, 8);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Exploration));
        // a remembered unseen monster: a threat only next to the hero
        draw(&mut w.map, &cat, 0, &["      I"]);
        assert_eq!(d.observe(&w, &cat), None);
        draw(&mut w.map, &cat, 0, &["  I"]);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
    }

    #[test]
    fn combat_starts_when_hp_drops_or_the_hero_fights() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        stat(&mut w, "hp", "12");
        assert_eq!(
            d.observe(&w, &cat),
            Some(Mode::Combat),
            "hurt by something unseen"
        );
        for t in 2..=4 {
            turn(&mut w, t);
            d.observe(&w, &cat);
        }
        assert_eq!(d.mode(), Mode::Exploration);
        say(&mut w, "You hit the newt.");
        turn(&mut w, 5);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        for t in 6..=8 {
            turn(&mut w, t);
            d.observe(&w, &cat);
        }
        assert_eq!(d.mode(), Mode::Exploration);
        say(&mut w, "The jackal bites!");
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        // without T: on the status, command prompts count
        let (mut w, cat) = world_of(&ROOM);
        w.status = crate::Status::new();
        stat(&mut w, "hp", "16");
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        say(&mut w, "You hit the newt.");
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        d.observe(&w, &cat);
        d.observe(&w, &cat);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Exploration));
    }

    #[test]
    fn peaceful_species_learned_from_messages_and_queries() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        // a jackal is hostile until the game says otherwise
        draw(&mut w.map, &cat, 2, &["       J"]);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        say(&mut w, "d  a dog or other canine (peaceful jackal)");
        for t in 2..=4 {
            turn(&mut w, t);
            d.observe(&w, &cat);
        }
        assert_eq!(d.mode(), Mode::Exploration);
        assert!(d.peaceful().contains("jackal"));
        // the longest name wins: "gnome lord", not "gnome"
        let mut p = Peaceful::default();
        p.learn("Really attack the peaceful gnome lord?", &cat);
        assert!(p.contains("gnome lord") && !p.contains("gnome"));
        // a name must end where the word ends
        let mut p = Peaceful::default();
        p.learn("the peaceful newtish thing", &cat);
        assert!(!p.contains("newt"));
        // the usual ones
        assert!(Peaceful::default().is_peaceful(&cat, 271), "shopkeeper");
        d.note_query("Really attack the peaceful newt?", &cat);
        assert!(d.peaceful().contains("newt"));
    }

    #[test]
    fn a_peaceful_that_attacks_is_hostile_again() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.note_query("Really attack the peaceful jackal?", &cat);
        draw(&mut w.map, &cat, 2, &["       J"]);
        assert_eq!(d.observe(&w, &cat), None);
        say(&mut w, "The jackal bites!");
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        assert!(!d.peaceful().contains("jackal"));
    }

    #[test]
    fn hallucination_makes_every_monster_a_threat() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.note_query("peaceful jackal", &cat);
        draw(&mut w.map, &cat, 2, &["       J"]);
        assert_eq!(d.observe(&w, &cat), None);
        let hallu = cat
            .conditions
            .iter()
            .find(|c| c.name == "Hallu")
            .unwrap()
            .mask;
        conds(&mut w, hallu);
        assert!(hallucinating(&w, &cat));
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
    }

    #[test]
    fn an_order_stops_on_a_hostile() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        d.start(
            Order::Walk {
                goal: (8, 1),
                arrival: Arrival::None,
            },
            &w,
            &cat,
        )
        .unwrap();
        step(&mut w, &cat, (2, 1), (3, 1));
        draw(&mut w.map, &cat, 2, &["        J"]);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        assert!(!d.is_active());
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::Hostile));
    }

    #[test]
    fn an_order_stops_on_hp_loss_hunger_and_new_conditions() {
        let run = |change: &dyn Fn(&mut World)| {
            let (mut w, cat) = world_of(&ROOM);
            let mut d = TickDriver::new();
            d.observe(&w, &cat);
            d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
                .unwrap();
            change(&mut w);
            d.observe(&w, &cat);
            assert!(!d.is_active());
            d.take_stop()
        };
        assert_eq!(run(&|w| stat(w, "hp", "15")), Some(Stop::HpLost));
        assert_eq!(run(&|w| stat(w, "hunger", "Hungry")), Some(Stop::Hunger));
        assert_eq!(run(&|w| conds(w, 8)), Some(Stop::Condition));
        assert_eq!(run(&|w| stat(w, "leveldesc", "Dlvl:2")), Some(Stop::Level));
        assert_eq!(
            run(&|w| w.map.clear()),
            Some(Stop::Level),
            "a new level drawn"
        );
        // HP going up is fine; so is losing a condition
        let (mut w, cat) = world_of(&ROOM);
        conds(&mut w, 8);
        stat(&mut w, "hp", "10");
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
            .unwrap();
        stat(&mut w, "hp", "11");
        conds(&mut w, 0);
        d.observe(&w, &cat);
        assert!(d.is_active());
    }

    #[test]
    fn an_order_stops_on_a_message_but_not_on_a_pet_swap() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        say(&mut w, "an old message");
        d.observe(&w, &cat);
        d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
            .unwrap();
        say(&mut w, "You swap places with your kitten.");
        say(&mut w, "You hear some noises in the distance.");
        d.observe(&w, &cat);
        assert!(d.is_active());
        d.next(&w, &cat).unwrap();
        say(&mut w, "You find a hidden passage.");
        d.observe(&w, &cat);
        assert_eq!(
            d.take_stop(),
            Some(Stop::Message("You find a hidden passage.".into()))
        );
        // an item underfoot on the way (not at the goal)
        d.observe(&w, &cat);
        d.start(
            Order::Walk {
                goal: (8, 1),
                arrival: Arrival::None,
            },
            &w,
            &cat,
        )
        .unwrap();
        step(&mut w, &cat, (2, 1), (3, 1));
        say(&mut w, "You see here a dagger.");
        d.observe(&w, &cat);
        assert_eq!(
            d.take_stop(),
            Some(Stop::Message("You see here a dagger.".into()))
        );
        // an urgent message, even a harmless-looking one
        let (mut w, cat) = world_of(&ROOM);
        stat(&mut w, "hp", "10");
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        d.start(Order::Rest, &w, &cat).unwrap();
        w.log
            .push("You hear the shopkeeper.".into(), 16, None, false);
        d.observe(&w, &cat);
        assert!(matches!(d.take_stop(), Some(Stop::Message(_))));
    }

    #[test]
    fn what_a_pet_does_never_stops_an_order() {
        let (mut w, cat) = world_of(&["|@.f....|"]);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
            .unwrap();
        say(&mut w, "The kitten picks up a bag.");
        say(&mut w, "Your kitten eats a newt corpse.");
        say(&mut w, "The door opens.");
        d.observe(&w, &cat);
        assert!(d.is_active());
        d.next(&w, &cat).unwrap();
        // a kitten that is not in view, or one that bites the hero
        say(&mut w, "The kittenish thing waves.");
        d.observe(&w, &cat);
        assert!(!d.is_active());
        assert!(about_a_pet("The kitten bites the newt.", &["kitten"]));
        assert!(!about_a_pet("The kitten bites the newt.", &["little dog"]));
        assert!(!about_a_pet("The kittens are here.", &["kitten"]));
    }

    #[test]
    fn an_order_stops_when_a_step_goes_nowhere() {
        let (w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        d.start(Order::Hold { key: 'h' }, &w, &cat).unwrap();
        // west is the wall: nothing moved
        d.observe(&w, &cat);
        assert_eq!(d.take_stop(), Some(Stop::Blocked));
        // a closed door opens at the first try: the walk goes on
        let rows = ["|@+..|"];
        let (mut w, cat) = world_of(&rows);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d
            .start(
                Order::Walk {
                    goal: (5, 0),
                    arrival: Arrival::None,
                },
                &w,
                &cat,
            )
            .unwrap();
        assert_eq!(keys(&a), "l");
        draw(&mut w.map, &cat, 0, &["  o"]);
        d.observe(&w, &cat);
        assert!(d.is_active(), "the door opened");
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        // ...but a locked one stays shut and says so
        say(&mut w, "This door is locked.");
        d.observe(&w, &cat);
        assert!(!d.is_active());
    }

    #[test]
    fn an_order_stops_on_an_unexpected_question() {
        let (w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
            .unwrap();
        d.on_question(Some("Really attack the peaceful newt?"), &cat);
        assert_eq!(d.take_stop(), Some(Stop::Question));
        assert!(d.peaceful().contains("newt"));
        // a key, a click, a panel
        for why in [Stop::Key, Stop::Click, Stop::Panel, Stop::Focus] {
            d.start(Order::Repeat { key: 's', left: 9 }, &w, &cat)
                .unwrap();
            d.interrupt(why.clone());
            assert_eq!(d.next(&w, &cat), None);
            assert_eq!(d.take_stop(), Some(why));
        }
        // nothing to stop: nothing said
        d.interrupt(Stop::Key);
        assert_eq!(d.take_stop(), None);
    }

    #[test]
    fn follow_ups_answer_command_and_direction_only() {
        let rows = ["|@J.|"];
        let (w, cat) = world_of(&rows);
        let mut d = TickDriver::new();
        let order = Order::Walk {
            goal: (3, 0),
            arrival: Arrival::Attack,
        };
        assert_eq!(keys(&d.start(order.clone(), &w, &cat).unwrap()), "Fl");
        let menu = Prompt::Choice {
            query: "Really attack?".into(),
            visible: vec!['y', 'n'],
            allowed: vec!['y', 'n'],
            default: None,
        };
        assert_eq!(d.follow_up(&menu), None);
        assert_eq!(d.take_stop(), Some(Stop::Question));
        assert!(!d.expects_follow_up());
        d.start(order, &w, &cat).unwrap();
        let letters = Prompt::FreeKey {
            query: "What do you want to eat?".into(),
            directions: false,
        };
        assert_eq!(d.follow_up(&letters), None);
        assert_eq!(d.follow_up(&Prompt::Command), None, "nothing more to send");
    }

    #[test]
    fn ready_waits_for_the_tick_and_the_step() {
        let mut d = TickDriver::new();
        let ms = Duration::from_millis;
        assert!(!d.ready(ms(299), false));
        assert!(d.ready(ms(300), false));
        assert!(!d.ready(ms(350), true), "the step still plays");
        assert!(d.ready(ms(600), true), "not longer than two ticks");
        d.set_tick_ms(50);
        assert_eq!(d.tick(), ms(MIN_TICK_MS));
        d.set_tick_ms(9000);
        assert_eq!(d.tick(), ms(MAX_TICK_MS));
        d.set_tick_ms(250);
        assert_eq!(d.tick(), ms(250));
        d.set_test_tick(ms(20));
        assert_eq!(d.tick(), ms(20));
        d.reset();
        assert_eq!(d.tick(), ms(20), "a new game keeps the setting");
    }

    #[test]
    fn count_entry_reads_digits_and_n_prefix() {
        let mut c = CountEntry::default();
        assert_eq!(c.feed('2' as i32, false), Counted::Typing);
        assert_eq!(c.feed('0' as i32, false), Counted::Typing);
        assert_eq!(c.shown().as_deref(), Some("Count: 20"));
        assert_eq!(c.feed('s' as i32, false), Counted::Command(Some(20)));
        assert_eq!(c.shown(), None);
        assert_eq!(c.feed('s' as i32, false), Counted::Command(None));
        // a leading zero is no count
        assert_eq!(c.feed('0' as i32, false), Counted::Command(None));
        // Esc forgets it
        c.feed('5' as i32, false);
        assert_eq!(c.feed(27, false), Counted::Typing);
        assert_eq!(c.feed('l' as i32, false), Counted::Command(None));
        assert_eq!(
            c.feed(27, false),
            Counted::Command(None),
            "Esc alone is a key"
        );
        // number_pad: digits are directions, n starts a count
        assert_eq!(c.feed('4' as i32, true), Counted::Command(None));
        assert_eq!(c.feed('n' as i32, true), Counted::Typing);
        assert_eq!(c.shown().as_deref(), Some("Count: "));
        c.feed('1' as i32, true);
        c.feed('2' as i32, true);
        assert_eq!(c.feed('s' as i32, true), Counted::Command(Some(12)));
        // capped
        for _ in 0..8 {
            c.feed('9' as i32, false);
        }
        assert_eq!(c.feed('s' as i32, false), Counted::Command(Some(MAX_COUNT)));
    }

    #[test]
    fn profiles_split_digits_between_count_and_bar() {
        let plain = |c| KeyInput::plain(Key::Char(c));
        let alt = |c| KeyInput {
            mods: Mods {
                alt: true,
                ..Mods::default()
            },
            ..plain(c)
        };
        let pad = "47896321><";
        let vi = "hykulnjb><";
        let mut c = CountEntry::default();
        let m = KeyProfile::Modern;
        // Modern: digits are the bar, n20 then s is a count of 20
        assert_eq!(
            c.feed_key(&plain('3'), m, pad),
            CommandInput::Bar {
                slot: 2,
                count: None
            }
        );
        assert_eq!(
            c.feed_key(&plain('0'), m, pad),
            CommandInput::Bar {
                slot: 9,
                count: None
            }
        );
        assert_eq!(c.feed_key(&plain('n'), m, pad), CommandInput::Typing);
        assert_eq!(c.feed_key(&plain('2'), m, pad), CommandInput::Typing);
        assert_eq!(c.feed_key(&plain('0'), m, pad), CommandInput::Typing);
        assert_eq!(c.shown().as_deref(), Some("Count: 20"));
        assert_eq!(
            c.feed_key(&plain('s'), m, pad),
            CommandInput::Command {
                key: 's' as i32,
                count: Some(20)
            }
        );
        // the keypad moves, after n it counts
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::Keypad(2)), m, pad),
            CommandInput::Command {
                key: '2' as i32,
                count: None
            }
        );
        c.feed_key(&plain('n'), m, pad);
        c.feed_key(&KeyInput::plain(Key::Keypad(5)), m, pad);
        assert_eq!(c.take(), None, "keypad 5 is '.', a rest");
        c.feed_key(&plain('n'), m, pad);
        c.feed_key(&KeyInput::plain(Key::Keypad(3)), m, pad);
        assert_eq!(c.take(), Some(3));
        // Alt+digit is nothing in Modern (a second bar row later)
        assert_eq!(c.feed_key(&alt('2'), m, pad), CommandInput::Ignored);
        // a count, then a click on a slot
        c.feed_key(&plain('n'), m, pad);
        c.feed_key(&plain('7'), m, pad);
        assert_eq!(c.take(), Some(7));
        assert_eq!(c.shown(), None);

        // Classic: n steps, Alt+digits count, digits are the bar
        let k = KeyProfile::Classic;
        assert_eq!(
            c.feed_key(&plain('n'), k, vi),
            CommandInput::Command {
                key: 'n' as i32,
                count: None
            }
        );
        assert_eq!(c.feed_key(&alt('0'), k, vi), CommandInput::Ignored);
        assert_eq!(c.feed_key(&alt('2'), k, vi), CommandInput::Typing);
        assert_eq!(c.feed_key(&alt('0'), k, vi), CommandInput::Typing);
        assert_eq!(
            c.feed_key(&plain('s'), k, vi),
            CommandInput::Command {
                key: 's' as i32,
                count: Some(20)
            }
        );
        c.feed_key(&alt('4'), k, vi);
        assert_eq!(
            c.feed_key(&plain('2'), k, vi),
            CommandInput::Bar {
                slot: 1,
                count: Some(4)
            }
        );
        // the keypad moves by vi-keys
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::Keypad(6)), k, vi),
            CommandInput::Command {
                key: 'l' as i32,
                count: None
            }
        );
        // Esc forgets a count; Ctrl+digit is nothing
        c.feed_key(&alt('9'), k, vi);
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::Escape), k, vi),
            CommandInput::Typing
        );
        assert_eq!(c.shown(), None);
        let ctrl = KeyInput {
            mods: Mods {
                ctrl: true,
                ..Mods::default()
            },
            ..plain('1')
        };
        assert_eq!(c.feed_key(&ctrl, k, vi), CommandInput::Ignored);
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::F(5)), k, vi),
            CommandInput::Ignored
        );
    }

    #[test]
    fn click_plans_by_what_is_there() {
        let rows = [
            "|.....|", //
            "|@f<..|", //
            "---+---",
        ];
        let (w, cat) = world_of(&rows);
        let walk = |goal, arrival| ClickPlan::Order(Order::Walk { goal, arrival });
        assert_eq!(
            click_order(&w, &cat, (3, 1)),
            walk((3, 1), Arrival::None),
            "the pet"
        );
        assert_eq!(
            click_order(&w, &cat, (4, 1)),
            walk((4, 1), Arrival::Stairs('<'))
        );
        assert_eq!(click_order(&w, &cat, (4, 2)), walk((4, 2), Arrival::Open));
        assert_eq!(click_order(&w, &cat, (6, 0)), walk((6, 0), Arrival::None));
        assert_eq!(
            click_order(&w, &cat, (0, 0)),
            ClickPlan::Nothing("off the map")
        );
    }

    #[test]
    fn the_planned_path_follows_the_hero() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        assert_eq!(d.planned_path(&w, &cat), None);
        d.start(
            Order::Walk {
                goal: (6, 1),
                arrival: Arrival::None,
            },
            &w,
            &cat,
        )
        .unwrap();
        assert_eq!(d.planned_path(&w, &cat).unwrap().len(), 4);
        step(&mut w, &cat, (2, 1), (3, 1));
        assert_eq!(d.planned_path(&w, &cat).unwrap().len(), 3);
    }
}
