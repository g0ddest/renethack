//! What the hero's body shows of their state, as the game tells the
//! player: a trap they are caught in (the host's hero notice: the hero's
//! glyph hides the trap under them).

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

#[cfg(test)]
mod tests {
    use super::*;

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
