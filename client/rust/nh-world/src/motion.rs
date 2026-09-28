//! Who moved where, told from the map alone (spec 5.1, source 3: "the same
//! monster type moved to an adjacent cell"), and who attacked whom, told
//! from the messages. Both are cosmetic hints for the animator: the map
//! stays authoritative, and anything uncertain is left unanimated.

use nh_protocol::{Glyph, GlyphKind, mg};

use crate::map::MapState;

/// Glyph flags that tell one entity from another of the same kind: a pet
/// is not the wild one of its species, the hero is not a look-alike.
const IDENT_FLAGS: u32 =
    mg::HERO | mg::PET | mg::RIDDEN | mg::INVIS | mg::DETECT | mg::MALE | mg::FEMALE;

/// What an entity on the map is, as far as seeing it again one cell further
/// can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ident {
    Mon {
        mon: i32,
        flags: u32,
    },
    /// A single object by its appearance (a pushed boulder); piles never move.
    Obj {
        tile: i32,
    },
}

impl Ident {
    /// The identity of what a glyph shows, when it is something that walks
    /// or is pushed.
    pub fn of(g: &Glyph) -> Option<Ident> {
        match g.kind {
            GlyphKind::Mon => Some(Ident::Mon {
                mon: g.mon?,
                flags: g.flags & IDENT_FLAGS,
            }),
            GlyphKind::Obj if g.flags & mg::OBJPILE == 0 => Some(Ident::Obj { tile: g.tile }),
            _ => None,
        }
    }

    pub fn is_hero(&self) -> bool {
        matches!(self, Ident::Mon { flags, .. } if flags & mg::HERO != 0)
    }
}

/// A cell drawn in a batch: what stood on it before the batch and after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub at: (i32, i32),
    pub before: Option<Ident>,
    pub after: Option<Ident>,
}

/// An entity that left `from` and now stands on `to`, one or two cells
/// away (a fast monster may take two steps in one turn).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: (i32, i32),
    pub to: (i32, i32),
    pub ident: Ident,
}

impl Move {
    /// Steps taken: 1 or 2.
    pub fn steps(&self) -> i32 {
        chebyshev(self.from, self.to)
    }
}

fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// Pair each entity that vanished from a cell with one of the same identity
/// that appeared on a neighbouring cell (8 directions), or failing that two
/// cells away (two steps in one turn). Among several candidates the nearer
/// wins (a single step before two, orthogonal before diagonal), then the
/// order of the cells (deterministic); each entity moves at most once.
/// Unpaired ones simply appear and disappear; a jump of more than two cells
/// (running, a teleport) is never a move.
pub fn detect_moves(changes: &[Change]) -> Vec<Move> {
    let vanished: Vec<((i32, i32), Ident)> = changes
        .iter()
        .filter(|c| c.before.is_some() && c.before != c.after)
        .filter_map(|c| Some((c.at, c.before?)))
        .collect();
    let appeared: Vec<((i32, i32), Ident)> = changes
        .iter()
        .filter(|c| c.after.is_some() && c.before != c.after)
        .filter_map(|c| Some((c.at, c.after?)))
        .collect();
    let mut candidates: Vec<(i32, Move)> = Vec::new();
    for &(from, ident) in &vanished {
        for &(to, other) in &appeared {
            if other == ident && (1..=2).contains(&chebyshev(from, to)) {
                let d2 = (from.0 - to.0).pow(2) + (from.1 - to.1).pow(2);
                candidates.push((d2, Move { from, to, ident }));
            }
        }
    }
    candidates.sort_by_key(|&(d2, m)| (d2, m.from.1, m.from.0, m.to.1, m.to.0));
    let mut used_from = Vec::new();
    let mut used_to = Vec::new();
    let mut moves = Vec::new();
    for (_, m) in candidates {
        if used_from.contains(&m.from) || used_to.contains(&m.to) {
            continue;
        }
        used_from.push(m.from);
        used_to.push(m.to);
        moves.push(m);
    }
    moves.sort_by_key(|m| (m.to.1, m.to.0));
    moves
}

/// Who takes part in a fight, as a message names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    Hero,
    /// A monster by the name the message uses ("jackal", "little dog").
    Named(String),
}

/// "You hit the jackal.", "The jackal bites!", "The little dog bites the newt."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackMsg {
    pub attacker: Who,
    pub target: Who,
}

/// Verbs of the hero's melee ("You hit the X").
const HERO_VERBS: &[&str] = &[
    "hit", "miss", "kill", "destroy", "smite", "bite", "kick", "butt", "sting", "touch", "claw",
    "strike", "thrust", "scratch",
];
/// Verbs of a monster's melee ("The X bites!").
const MONSTER_VERBS: &[&str] = &[
    "hits",
    "misses",
    "bites",
    "claws",
    "kicks",
    "butts",
    "stings",
    "touches",
    "scratches",
    "strikes",
    "thrusts",
    "smites",
    "pecks",
    "lashes",
    "headbutts",
    "tentacles",
];

/// "the newt." / "the newt!" -> "newt".
fn named_target(rest: &str) -> Option<String> {
    let name = rest.strip_prefix("the ")?;
    let name = name.strip_suffix(['.', '!'])?;
    (!name.is_empty() && !name.contains([',', '.', '!'])).then(|| name.to_string())
}

/// A melee attack a message tells of, if it is one of the plain forms.
/// Anything else (ranged attacks, "it", named pets, passives) is None.
pub fn parse_attack(text: &str) -> Option<AttackMsg> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("You ") {
        let (verb, rest) = rest.split_once(' ')?;
        if !HERO_VERBS.contains(&verb) {
            return None;
        }
        return Some(AttackMsg {
            attacker: Who::Hero,
            target: Who::Named(named_target(rest)?),
        });
    }
    let rest = text.strip_prefix("The ")?;
    // the attacker's name runs up to the first known verb
    let words: Vec<&str> = rest.split(' ').collect();
    for i in 1..words.len() {
        let verb = words[i].trim_end_matches(['!', '.']);
        if !MONSTER_VERBS.contains(&verb) {
            continue;
        }
        let attacker = Who::Named(words[..i].join(" "));
        let tail = words[i + 1..].join(" ");
        let target = match (words[i].ends_with(['!', '.']), tail.as_str()) {
            (true, "") | (false, "you!") | (false, "you.") => Who::Hero,
            (false, t) => Who::Named(named_target(t)?),
            (true, _) => return None,
        };
        return Some(AttackMsg { attacker, target });
    }
    None
}

/// Where the fight a message tells of takes place: (attacker, target)
/// cells, adjacent. A monster is found by name (`names` gives a monster
/// index's names) among the monsters around; the target may already lie
/// dead. None when the message is not understood or nobody on the map fits.
pub fn locate_attack<'a>(
    msg: &AttackMsg,
    map: &MapState,
    names: impl Fn(i32) -> Vec<&'a str>,
) -> Option<((i32, i32), (i32, i32))> {
    let hero = map.hero();
    let is = |at: (i32, i32), who: &Who, dead_ok: bool| -> bool {
        match who {
            Who::Hero => hero == Some(at),
            Who::Named(name) => map
                .cell(at.0, at.1)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|g| {
                    let alive = g.kind == GlyphKind::Mon && g.flags & mg::HERO == 0;
                    let dead = dead_ok && g.kind == GlyphKind::Body;
                    (alive || dead) && g.mon.is_some_and(|m| names(m).contains(&name.as_str()))
                }),
        }
    };
    let around = |(x, y): (i32, i32)| {
        (-1..=1)
            .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
            .filter(move |&c| c != (x, y))
    };
    let cells: Vec<(i32, i32)> = match &msg.attacker {
        Who::Hero => hero.into_iter().collect(),
        Who::Named(_) => {
            let mut all: Vec<(i32, i32)> = (0..crate::map::ROWNO)
                .flat_map(|y| (1..crate::map::COLNO).map(move |x| (x, y)))
                .collect();
            all.sort_by_key(|&(x, y)| (y, x));
            all
        }
    };
    for from in cells {
        if !is(from, &msg.attacker, false) {
            continue;
        }
        let mut targets: Vec<(i32, i32)> =
            around(from).filter(|&t| is(t, &msg.target, true)).collect();
        targets.sort_by_key(|&(x, y)| (y, x));
        if let Some(&to) = targets.first() {
            return Some((from, to));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::tests::{floor, glyph, monster};

    const HERO: Ident = Ident::Mon {
        mon: 0,
        flags: mg::HERO,
    };
    const DOG: Ident = Ident::Mon {
        mon: 16,
        flags: mg::PET,
    };
    const JACKAL: Ident = Ident::Mon { mon: 3, flags: 0 };

    fn change(at: (i32, i32), before: Option<Ident>, after: Option<Ident>) -> Change {
        Change { at, before, after }
    }

    #[test]
    fn identity_keeps_what_tells_entities_apart() {
        let dog = monster(16, mg::PET | mg::OBJPILE);
        assert_eq!(Ident::of(&dog), Some(DOG));
        assert_ne!(
            Ident::of(&monster(16, 0)),
            Some(DOG),
            "a wild dog is not the pet"
        );
        assert!(Ident::of(&monster(0, mg::HERO)).unwrap().is_hero());
        let boulder = Glyph {
            tile: 400,
            ..glyph(GlyphKind::Obj, '0')
        };
        assert_eq!(Ident::of(&boulder), Some(Ident::Obj { tile: 400 }));
        let pile = Glyph {
            flags: mg::OBJPILE,
            ..boulder
        };
        assert_eq!(Ident::of(&pile), None);
        assert_eq!(Ident::of(&floor()), None);
        assert_eq!(Ident::of(&glyph(GlyphKind::Body, '%')), None);
    }

    #[test]
    fn a_step_to_a_neighbour_is_a_move() {
        let moves = detect_moves(&[
            change((10, 5), Some(HERO), None),
            change((11, 5), None, Some(HERO)),
        ]);
        assert_eq!(
            moves,
            vec![Move {
                from: (10, 5),
                to: (11, 5),
                ident: HERO
            }]
        );
        // diagonally too
        let moves = detect_moves(&[
            change((10, 5), Some(JACKAL), None),
            change((9, 4), None, Some(JACKAL)),
        ]);
        assert_eq!(moves.len(), 1);
    }

    #[test]
    fn two_steps_in_a_turn_are_one_quicker_move() {
        // a kitten (speed 18) moves twice: a knight's jump on the map
        let kitten = |at, before, after| change(at, before, after);
        let moves = detect_moves(&[
            kitten((15, 5), Some(DOG), None),
            kitten((14, 3), None, Some(DOG)),
        ]);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].steps(), 2);
        // a single step is preferred: the nearer pet takes the near cell
        let moves = detect_moves(&[
            change((10, 5), Some(JACKAL), None),
            change((12, 5), Some(JACKAL), None),
            change((13, 5), None, Some(JACKAL)),
            change((11, 5), None, Some(JACKAL)),
        ]);
        let pairs: Vec<_> = moves.iter().map(|m| (m.from, m.to, m.steps())).collect();
        assert_eq!(pairs, vec![((10, 5), (11, 5), 1), ((12, 5), (13, 5), 1)]);
    }

    #[test]
    fn far_jumps_other_kinds_and_standing_still_are_not_moves() {
        // three cells in one batch (running), a teleport
        assert!(
            detect_moves(&[
                change((10, 5), Some(HERO), None),
                change((13, 5), None, Some(HERO)),
            ])
            .is_empty()
        );
        assert!(
            detect_moves(&[
                change((10, 5), Some(HERO), None),
                change((30, 15), None, Some(HERO)),
            ])
            .is_empty()
        );
        // a jackal left, a pet arrived
        assert!(
            detect_moves(&[
                change((10, 5), Some(JACKAL), None),
                change((11, 5), None, Some(DOG)),
            ])
            .is_empty()
        );
        // redrawn in place
        assert!(detect_moves(&[change((10, 5), Some(DOG), Some(DOG))]).is_empty());
    }

    #[test]
    fn swapping_places_moves_both() {
        // the hero displaces the pet
        let moves = detect_moves(&[
            change((10, 5), Some(HERO), Some(DOG)),
            change((11, 5), Some(DOG), Some(HERO)),
        ]);
        assert_eq!(moves.len(), 2);
        assert!(moves.contains(&Move {
            from: (10, 5),
            to: (11, 5),
            ident: HERO
        }));
        assert!(moves.contains(&Move {
            from: (11, 5),
            to: (10, 5),
            ident: DOG
        }));
    }

    #[test]
    fn a_line_of_followers_moves_each_one_step() {
        // three jackals in a corridor step east together: the middle cells
        // keep a jackal, only the ends change
        let moves = detect_moves(&[
            change((10, 5), Some(JACKAL), None),
            change((13, 5), None, Some(JACKAL)),
        ]);
        assert!(moves.is_empty(), "the ends are too far apart: they snap");
        // two jackals side by side each take the nearest cell
        let moves = detect_moves(&[
            change((10, 5), Some(JACKAL), None),
            change((10, 6), Some(JACKAL), None),
            change((11, 5), None, Some(JACKAL)),
            change((11, 6), None, Some(JACKAL)),
        ]);
        assert_eq!(moves.len(), 2);
        assert!(moves.iter().all(|m| m.from.1 == m.to.1), "{moves:?}");
    }

    #[test]
    fn the_same_changes_always_pair_the_same_way() {
        let changes = [
            change((10, 5), Some(JACKAL), None),
            change((12, 5), Some(JACKAL), None),
            change((11, 4), None, Some(JACKAL)),
            change((20, 5), None, Some(JACKAL)),
        ];
        let a = detect_moves(&changes);
        let mut reversed = changes;
        reversed.reverse();
        assert_eq!(a, detect_moves(&reversed));
        assert_eq!(a.len(), 1, "one arrival: the other jackal vanished");
    }

    #[test]
    fn attack_messages_name_attacker_and_target() {
        let named = |s: &str| Who::Named(s.to_string());
        assert_eq!(
            parse_attack("You hit the jackal."),
            Some(AttackMsg {
                attacker: Who::Hero,
                target: named("jackal")
            })
        );
        assert_eq!(
            parse_attack("You kill the grid bug!").map(|m| m.target),
            Some(named("grid bug"))
        );
        assert_eq!(
            parse_attack("The jackal bites!"),
            Some(AttackMsg {
                attacker: named("jackal"),
                target: Who::Hero
            })
        );
        assert_eq!(
            parse_attack("The giant rat misses!").map(|m| m.attacker),
            Some(named("giant rat"))
        );
        assert_eq!(
            parse_attack("The little dog bites the newt."),
            Some(AttackMsg {
                attacker: named("little dog"),
                target: named("newt")
            })
        );
        assert_eq!(
            parse_attack("The kitten misses the sewer rat.").map(|m| m.target),
            Some(named("sewer rat"))
        );
        for other in [
            "You hit it.",
            "You see here a scroll labeled FOO.",
            "The door opens.",
            "The gnome lord swings his crossbow.",
            "You are hit by an arrow!",
            "Hachi bites the newt.",
            "You kill it!",
            "The jackal bites the dust of the ages, somehow",
        ] {
            assert_eq!(parse_attack(other), None, "{other}");
        }
    }

    #[test]
    fn a_fight_is_found_next_to_the_attacker() {
        let mut map = MapState::new();
        for x in 8..14 {
            for y in 3..8 {
                map.print(x, y, &floor(), None);
            }
        }
        map.print(10, 5, &monster(0, mg::HERO), None);
        map.print(11, 5, &monster(3, 0), None); // a jackal east of the hero
        map.print(12, 6, &monster(3, 0), None); // another, not adjacent to the hero
        map.print(12, 7, &monster(20, 0), None); // a newt next to it
        let names = |m: i32| match m {
            3 => vec!["jackal"],
            20 => vec!["newt"],
            _ => vec![],
        };
        let hit = parse_attack("You hit the jackal.").unwrap();
        assert_eq!(locate_attack(&hit, &map, names), Some(((10, 5), (11, 5))));
        let bite = parse_attack("The jackal bites!").unwrap();
        assert_eq!(locate_attack(&bite, &map, names), Some(((11, 5), (10, 5))));
        let fight = parse_attack("The jackal bites the newt.").unwrap();
        assert_eq!(locate_attack(&fight, &map, names), Some(((12, 6), (12, 7))));
        // nobody of that name around the hero
        let miss = parse_attack("You miss the newt.").unwrap();
        assert_eq!(locate_attack(&miss, &map, names), None);
        // the target just died: its corpse is where it stood
        let corpse = Glyph {
            mon: Some(3),
            ..glyph(GlyphKind::Body, '%')
        };
        map.print(11, 5, &corpse, None);
        let kill = parse_attack("You kill the jackal!").unwrap();
        assert_eq!(locate_attack(&kill, &map, names), Some(((10, 5), (11, 5))));
    }
}
