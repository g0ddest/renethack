//! What the hero's body shows of their state, as the game tells the
//! player: a trap they are caught in (the host's hero notice: the hero's
//! glyph hides the trap under them), and a spell of helplessness (the
//! status line's conditions Zzz, Out, Parlyz).

use nh_protocol::Catalog;

/// A trap the hero is caught in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrapKind {
    Pit,
    /// A pit whose spikes the hero has seen.
    SpikedPit,
    BearTrap,
    Web,
    /// Sinking into lava.
    Lava,
    /// Stuck in the floor.
    Floor,
    /// Chained to a buried iron ball.
    BuriedBall,
    /// One this client does not know.
    Other,
}

impl TrapKind {
    /// The hero notice's name of a trap.
    pub fn of(name: &str) -> TrapKind {
        match name {
            "pit" => TrapKind::Pit,
            "spiked_pit" => TrapKind::SpikedPit,
            "bear_trap" => TrapKind::BearTrap,
            "web" => TrapKind::Web,
            "lava" => TrapKind::Lava,
            "floor" => TrapKind::Floor,
            "buried_ball" => TrapKind::BuriedBall,
            _ => TrapKind::Other,
        }
    }

    /// A pit of either kind: the hero stands in it, below the floor.
    pub fn is_pit(self) -> bool {
        matches!(self, TrapKind::Pit | TrapKind::SpikedPit)
    }
}

/// How the hero lies helpless, as the status line tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Helpless {
    /// Asleep (Zzz).
    Asleep,
    /// Fainted, passed out, knocked out (Out).
    Unconscious,
    /// Paralysed or frozen (Parlyz).
    Paralyzed,
}

impl Helpless {
    /// Its condition on the status line, in the order the engine tests
    /// them: one at a time shows.
    const CONDITIONS: [(&str, Helpless); 3] = [
        ("Out", Helpless::Unconscious),
        ("Parlyz", Helpless::Paralyzed),
        ("Zzz", Helpless::Asleep),
    ];
}

/// A spell of helplessness the hero went through: how, and for how many
/// turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelplessSpan {
    pub kind: Helpless,
    pub turns: u32,
    /// Which spell it is in the game (counting up from 1).
    pub serial: u64,
}

/// The spells of helplessness, followed through the status line. A
/// helpless hero is asked for no command: a whole spell passes before the
/// next command prompt, and the client shows it after it, at that prompt.
/// The status line misses a hero passed out drunk (the engine sets no Out
/// for it): the messages tell that one.
#[derive(Debug, Clone, Default)]
pub struct HelplessWatch {
    /// The conditions' masks (the catalog's).
    masks: Vec<(u64, Helpless)>,
    /// The spell under way, the turn it began, and whether the messages
    /// told it (not the status line).
    now: Option<(Helpless, i64, bool)>,
    /// The last spell over.
    last: Option<HelplessSpan>,
    serial: u64,
    /// The spell over since the command prompt before the last one, and
    /// the serial of the last spell so given.
    fresh: Option<HelplessSpan>,
    given: u64,
}

/// Cursed booze (potion.c): the hero passes out, and awakes.
const PASSED_OUT: &str = "You pass out.";
const AWAKE: &str = "You awake with a headache";

impl HelplessWatch {
    pub fn set_catalog(&mut self, catalog: &Catalog) {
        self.masks = Helpless::CONDITIONS
            .iter()
            .filter_map(|&(name, kind)| {
                let c = catalog.conditions.iter().find(|c| c.name == name)?;
                Some((c.mask, kind))
            })
            .collect();
    }

    /// The status line's conditions now, at `turn`.
    pub fn conditions(&mut self, conds: u64, turn: i64) {
        let kind = self
            .masks
            .iter()
            .find(|(mask, _)| conds & mask != 0)
            .map(|&(_, kind)| kind);
        match self.now {
            // the messages told it, and end it
            Some((_, _, true)) if kind.is_none() => return,
            Some((k, _, false)) if Some(k) == kind => return,
            None if kind.is_none() => return,
            _ => {}
        }
        self.end(turn);
        if let Some(k) = kind {
            self.begin(k, turn, false);
        }
    }

    /// A message, at `turn`.
    pub fn message(&mut self, text: &str, turn: i64) {
        match self.now {
            None if text == PASSED_OUT => self.begin(Helpless::Unconscious, turn, true),
            Some((_, _, true)) if text.starts_with(AWAKE) => self.end(turn),
            _ => {}
        }
    }

    /// A command prompt, at `turn`: the hero can act, so a spell the
    /// messages told is over; the spell over since the last command prompt
    /// is the fresh one.
    pub fn command(&mut self, turn: i64) {
        if matches!(self.now, Some((_, _, true))) {
            self.end(turn);
        }
        self.fresh = self.last.filter(|l| l.serial > self.given);
        if let Some(f) = self.fresh {
            self.given = f.serial;
        }
    }

    fn begin(&mut self, kind: Helpless, turn: i64, told: bool) {
        self.serial += 1;
        self.now = Some((kind, turn, told));
    }

    fn end(&mut self, turn: i64) {
        if let Some((kind, since, _)) = self.now.take() {
            self.last = Some(HelplessSpan {
                kind,
                turns: turns(since, turn),
                serial: self.serial,
            });
        }
    }

    /// The spell under way.
    pub fn now(&self) -> Option<Helpless> {
        self.now.map(|(kind, ..)| kind)
    }

    /// The spell the hero went through before this command prompt (None
    /// once they were asked for a command again).
    pub fn fresh(&self) -> Option<HelplessSpan> {
        self.fresh
    }

    /// A new game: nothing under way, nothing to show.
    pub fn reset(&mut self) {
        self.now = None;
        self.fresh = None;
    }
}

fn turns(since: i64, turn: i64) -> u32 {
    u32::try_from(turn - since).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::tests::catalog;

    /// A watch, and the masks of the conditions by name.
    fn watch() -> (HelplessWatch, impl Fn(&[&str]) -> u64) {
        let cat = catalog();
        let mut w = HelplessWatch::default();
        w.set_catalog(&cat);
        let masks = move |names: &[&str]| {
            cat.conditions
                .iter()
                .filter(|c| names.contains(&c.name.as_str()))
                .fold(0, |m, c| m | c.mask)
        };
        (w, masks)
    }

    fn fresh(w: &HelplessWatch) -> Option<(Helpless, u32)> {
        w.fresh().map(|s| (s.kind, s.turns))
    }

    #[test]
    fn a_spell_of_sleep_is_given_at_the_command_prompt_after_it() {
        let (mut w, masks) = watch();
        w.command(9);
        assert_eq!(fresh(&w), None);
        // fallen asleep in a trap at turn 10, awake at 22
        w.conditions(masks(&["Zzz", "Trap"]), 10);
        assert_eq!(w.now(), Some(Helpless::Asleep));
        assert_eq!(fresh(&w), None);
        w.conditions(masks(&["Trap"]), 22);
        w.command(22);
        assert_eq!(fresh(&w), Some((Helpless::Asleep, 12)));
        // once: the hero acts, and is asked again
        w.command(23);
        assert_eq!(fresh(&w), None);
        // the next one is another spell
        w.conditions(masks(&["Parlyz"]), 30);
        w.conditions(0, 31);
        w.command(31);
        assert_eq!(fresh(&w), Some((Helpless::Paralyzed, 1)));
        assert_eq!(w.fresh().unwrap().serial, 2);
    }

    #[test]
    fn a_hero_passed_out_drunk_is_told_by_the_messages() {
        let (mut w, masks) = watch();
        w.message("You pass out.", 40);
        // the status line shows only the confusion meanwhile
        w.conditions(masks(&["Conf"]), 40);
        assert_eq!(w.now(), Some(Helpless::Unconscious));
        w.message("You awake with a headache.", 47);
        w.command(47);
        assert_eq!(fresh(&w), Some((Helpless::Unconscious, 7)));
        // the end missed: the command prompt ends it
        w.message("You pass out.", 60);
        w.command(64);
        assert_eq!(fresh(&w), Some((Helpless::Unconscious, 4)));
    }

    #[test]
    fn a_sleeper_knocked_out_is_another_spell() {
        let (mut w, masks) = watch();
        w.conditions(masks(&["Zzz"]), 5);
        w.conditions(masks(&["Out"]), 8);
        assert_eq!(w.now(), Some(Helpless::Unconscious));
        w.conditions(0, 12);
        w.command(12);
        assert_eq!(
            w.fresh().map(|s| (s.kind, s.serial)),
            Some((Helpless::Unconscious, 2))
        );
    }

    #[test]
    fn the_notices_names_are_trap_kinds() {
        for (name, kind) in [
            ("pit", TrapKind::Pit),
            ("spiked_pit", TrapKind::SpikedPit),
            ("bear_trap", TrapKind::BearTrap),
            ("web", TrapKind::Web),
            ("lava", TrapKind::Lava),
            ("floor", TrapKind::Floor),
            ("buried_ball", TrapKind::BuriedBall),
            ("a trap of the future", TrapKind::Other),
        ] {
            assert_eq!(TrapKind::of(name), kind, "{name}");
        }
        assert!(TrapKind::Pit.is_pit() && TrapKind::SpikedPit.is_pit());
        assert!(!TrapKind::BearTrap.is_pit());
    }
}
