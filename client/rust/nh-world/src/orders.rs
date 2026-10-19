//! Time in the style of BG3 (spec 4): the player gives orders — walk to a
//! cell, hold a key, rest until healed, go to the stairs — and the client
//! carries them out one engine action per tick; in a fight every action
//! waits for the player. An order stops on anything that deserves the
//! player's eye, and on any key or click, before its next step. A fight
//! is NetHack's own idea of a threat to a run or a travel (`lookaround`):
//! a hostile the hero sees beside them, or blows given or taken. One
//! farther off, a peaceful one or one only sensed stops nothing.
//!
//! Everything here is plain data: the game asks for the next action when a
//! command prompt waits and the tick has passed, and answers that very
//! prompt with it. An order is a run of single steps, so it can always
//! stop. A count (`20s`, `n5,`) is no order: it goes to the engine with
//! its command, which NetHack repeats and stops by its own rules.

use std::collections::VecDeque;
use std::time::Duration;

use nh_protocol::{Catalog, Glyph, GlyphKind, mg};

use crate::path::{Passage, Reach, chebyshev, dir_key, find_path, key_dir, passage};
use crate::{
    Cell, Key, KeyContext, KeyInput, KeyProfile, Prompt, Terrain, Who, World, bar_slot,
    cell_terrain, in_field, nethack_key, parse_attack,
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
    /// No fight: orders run on the tick.
    Exploration,
    /// A hostile beside the hero, or blows given or taken: every action
    /// waits for the player.
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
    /// It ran its course (a rest as long as it goes).
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
    /// A hostile came beside the hero.
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
            Stop::Hostile => "Stopped: a hostile beside you".into(),
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

/// Is what this cell at `at` shows something to fight? A monster that is
/// not the hero, not a pet, not sensed without being seen (telepathy,
/// detection: the host's `sensed`, MG_DETECT), not peaceful (as the engine
/// tells it of each monster, as farlook would); while hallucinating every
/// monster; a remembered unseen one next to the hero.
pub fn is_threat(
    cell: &Cell,
    at: (i32, i32),
    hero: Option<(i32, i32)>,
    hallucinating: bool,
) -> bool {
    let Some(g) = cell.entity() else {
        return false;
    };
    match g.kind {
        GlyphKind::Mon => {
            if g.flags & (mg::HERO | mg::DETECT) != 0 || cell.sensed {
                false
            } else if hallucinating {
                true
            } else if g.flags & mg::PET != 0 {
                false
            } else {
                !cell.peaceful
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

/// A threat beside the hero: what stops a run or a travel in NetHack
/// (`lookaround`), and what makes a fight of it here.
pub fn engaged(world: &World, catalog: &Catalog) -> bool {
    world.hero().is_some_and(|h| {
        threats(world, catalog)
            .into_iter()
            .any(|t| chebyshev(h, t) <= 1)
    })
}

/// Cells with a threat on them.
pub fn threats(world: &World, catalog: &Catalog) -> Vec<(i32, i32)> {
    let hero = world.hero();
    let hallu = hallucinating(world, catalog);
    let mut out = Vec::new();
    for y in 0..crate::ROWNO {
        for x in 1..crate::COLNO {
            let Some(cell) = world.map.cell(x, y) else {
                continue;
            };
            if is_threat(cell, (x, y), hero, hallu) {
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

/// Keys a held key repeats as an order: a step, a search, a rest.
pub fn repeatable(key: char, dirchars: &str) -> bool {
    key == 's' || key == '.' || key_dir(dirchars, key).is_some()
}

/// A count typed before a command (digits; with number_pad, `n` and
/// digits). The client reads it to show it as it is typed and to give a
/// bar slot its count; the command then goes to the engine with it
/// ([`counted`]).
///
/// [`counted`]: crate::counted
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

    /// A key at a command prompt under `profile`. Modern's top-row digits
    /// are the action bar, except after `n`, where they are the count (a
    /// slot then takes the count by a click, see [`take`]). Classic's are
    /// the count, as in NetHack, and its bar is Alt and the digits. The
    /// keypad moves in both (after `n` its digits are the count, as in
    /// tty); an arrow after `n` ends the count and steps.
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
                KeyProfile::Classic => !input.mods.alt,
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
        // number_pad gives an arrow a digit, which the count would take:
        // after `n` an arrow is the step the count is for
        let arrow = matches!(
            input.key,
            Key::Up
                | Key::Down
                | Key::Left
                | Key::Right
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        );
        if arrow && number_pad && self.typing() {
            return CommandInput::Command {
                key,
                count: self.take(),
            };
        }
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
    /// Messages already looked at.
    seen_seq: u64,
    last_hp: Option<i64>,
    /// The turn a threat was last beside the hero (or a blow struck).
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
    pub fn on_question(&mut self) {
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
            let mut hero_fights = false;
            if let Some(a) = parse_attack(&m.text) {
                match (&a.attacker, &a.target) {
                    (Who::Named(_), Who::Hero) | (Who::Hero, _) => hero_fights = true,
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
        let near = engaged(world, catalog);
        if near || hurt || fought {
            self.last_threat = Some(turn);
        }
        let was = self.mode;
        if near || hurt || fought {
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

    /// The monster on `at` drawn again as the engine tells a peaceful one.
    fn peaceful(world: &mut World, (x, y): (i32, i32)) {
        let g = world.map.cell(x, y).and_then(|c| c.glyph.clone()).unwrap();
        let told = crate::Told {
            peaceful: true,
            ..crate::Told::default()
        };
        world.map.print_with(x, y, &g, None, told);
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
        // a jackal told peaceful: no fight, the walk goes on
        peaceful(&mut w, (7, 0));
        assert_eq!(d.observe(&w, &cat), None);
        let order = Order::Walk {
            goal: (7, 0),
            arrival: Arrival::Attack,
        };
        assert_eq!(keys(&d.start(order, &w, &cat).unwrap()), "l");
        step(&mut w, &cat, (2, 0), (3, 0));
        // it comes a cell closer: the walk follows it
        draw(&mut w.map, &cat, 0, &["     J."]);
        peaceful(&mut w, (6, 0));
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
    fn a_held_step_stops_at_a_wall() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        d.observe(&w, &cat);
        let a = d.start(Order::Hold { key: 'l' }, &w, &cat).unwrap();
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
            "|J.......|",
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
        // a rest: one search
        stat(&mut w, "hp", "10");
        d.observe(&w, &cat);
        d.start(Order::Rest, &w, &cat).unwrap();
        d.observe(&w, &cat);
        assert_eq!(d.next(&w, &cat), None);
        assert_eq!(d.take_stop(), Some(Stop::OneAction));
        // holding a key goes on
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
        d.observe(&w, &cat);
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "s");
    }

    #[test]
    fn combat_starts_on_a_hostile_beside_the_hero_and_ends_after_three_quiet_turns() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        assert_eq!(d.observe(&w, &cat), None);
        assert_eq!(d.mode(), Mode::Exploration);
        // a pet is no threat, nor a monster sensed but not seen, even
        // beside the hero
        draw(&mut w.map, &cat, 0, &["  f"]);
        w.map.print(8, 2, &mon(FOE_MON, mg::DETECT), None);
        let sensed = crate::Told {
            sensed: true,
            ..crate::Told::default()
        };
        w.map.print_with(3, 2, &mon(FOE_MON, 0), None, sensed);
        assert_eq!(d.observe(&w, &cat), None);
        assert!(threats(&w, &cat).is_empty());
        // a jackal far off: a threat, but no fight (NetHack's runs stop
        // for one beside the hero)
        draw(&mut w.map, &cat, 2, &["  .    J"]);
        turn(&mut w, 5);
        assert_eq!(threats(&w, &cat), vec![(8, 2)]);
        assert_eq!(d.observe(&w, &cat), None);
        // it comes beside the hero
        draw(&mut w.map, &cat, 2, &["  J    ."]);
        turn(&mut w, 6);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        // it dies
        draw(&mut w.map, &cat, 2, &["  ."]);
        turn(&mut w, 7);
        assert_eq!(d.observe(&w, &cat), None);
        turn(&mut w, 8);
        assert_eq!(d.observe(&w, &cat), None, "two quiet turns");
        turn(&mut w, 9);
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
    fn peacefulness_is_each_monster_s_own() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        // two jackals beside the hero, one told peaceful: only the other
        // is a threat (the species says nothing)
        draw(&mut w.map, &cat, 2, &["  JJ"]);
        peaceful(&mut w, (3, 2));
        assert_eq!(threats(&w, &cat), vec![(4, 2)]);
        assert_eq!(d.observe(&w, &cat), None, "the hostile one is two off");
        draw(&mut w.map, &cat, 2, &["  J."]);
        peaceful(&mut w, (3, 2));
        draw(&mut w.map, &cat, 0, &["   J"]);
        assert_eq!(threats(&w, &cat), vec![(4, 0)]);
        assert_eq!(d.observe(&w, &cat), None);
        draw(&mut w.map, &cat, 0, &["  J."]);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
        // the hostile one gone, the peaceful one stays: calm after a while
        draw(&mut w.map, &cat, 0, &["  ."]);
        for t in 2..=4 {
            turn(&mut w, t);
            d.observe(&w, &cat);
        }
        assert_eq!(d.mode(), Mode::Exploration);
        // the peaceful one angered: drawn again, no longer told peaceful
        draw(&mut w.map, &cat, 2, &["  J"]);
        assert_eq!(threats(&w, &cat), vec![(3, 2)]);
        assert_eq!(d.observe(&w, &cat), Some(Mode::Combat));
    }

    #[test]
    fn hallucination_makes_every_monster_a_threat() {
        let (mut w, cat) = world_of(&ROOM);
        let mut d = TickDriver::new();
        draw(&mut w.map, &cat, 2, &["  J"]);
        peaceful(&mut w, (3, 2));
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
    fn an_order_stops_on_a_hostile_beside_the_hero_only() {
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
        // one far off, or one only sensed beside the way: the walk goes on
        step(&mut w, &cat, (2, 1), (3, 1));
        draw(&mut w.map, &cat, 2, &["        J"]);
        let sensed = crate::Told {
            sensed: true,
            ..crate::Told::default()
        };
        w.map.print_with(4, 0, &mon(FOE_MON, 0), None, sensed);
        assert_eq!(d.observe(&w, &cat), None);
        assert!(d.is_active());
        assert_eq!(keys(&d.next(&w, &cat).unwrap()), "l");
        // one beside the hero: a fight, and the walk stops
        step(&mut w, &cat, (3, 1), (4, 1));
        draw(&mut w.map, &cat, 2, &["    J    "]);
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
            d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
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
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
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
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
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
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
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
        d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
        d.on_question();
        assert_eq!(d.take_stop(), Some(Stop::Question));
        // a key, a click, a panel
        for why in [Stop::Key, Stop::Click, Stop::Panel, Stop::Focus] {
            d.start(Order::Hold { key: 's' }, &w, &cat).unwrap();
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
        // an arrow ends the count and is its step (n5 then 4 would be 54)
        c.feed_key(&plain('n'), m, pad);
        c.feed_key(&plain('5'), m, pad);
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::Left), m, pad),
            CommandInput::Command {
                key: '4' as i32,
                count: Some(5)
            }
        );
        c.feed_key(&plain('n'), m, pad);
        assert_eq!(
            c.feed_key(&KeyInput::plain(Key::PageDown), m, pad),
            CommandInput::Command {
                key: '3' as i32,
                count: None
            },
            "n and an arrow: one step"
        );
        assert_eq!(c.shown(), None);
        // Alt+digit is nothing in Modern (a second bar row later)
        assert_eq!(c.feed_key(&alt('2'), m, pad), CommandInput::Ignored);
        // a count, then a click on a slot
        c.feed_key(&plain('n'), m, pad);
        c.feed_key(&plain('7'), m, pad);
        assert_eq!(c.take(), Some(7));
        assert_eq!(c.shown(), None);

        // Classic: n steps, digits count as in NetHack, Alt+digits are
        // the bar
        let k = KeyProfile::Classic;
        assert_eq!(
            c.feed_key(&plain('n'), k, vi),
            CommandInput::Command {
                key: 'n' as i32,
                count: None
            }
        );
        assert_eq!(c.feed_key(&plain('0'), k, vi), CommandInput::Ignored);
        assert_eq!(c.feed_key(&plain('2'), k, vi), CommandInput::Typing);
        assert_eq!(c.feed_key(&plain('0'), k, vi), CommandInput::Typing);
        assert_eq!(c.shown().as_deref(), Some("Count: 20"));
        assert_eq!(
            c.feed_key(&plain('s'), k, vi),
            CommandInput::Command {
                key: 's' as i32,
                count: Some(20)
            }
        );
        assert_eq!(
            c.feed_key(&alt('3'), k, vi),
            CommandInput::Bar {
                slot: 2,
                count: None
            }
        );
        c.feed_key(&plain('4'), k, vi);
        assert_eq!(
            c.feed_key(&alt('2'), k, vi),
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
        c.feed_key(&plain('9'), k, vi);
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
