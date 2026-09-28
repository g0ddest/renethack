//! Walking the known map: which cells a step may enter, by NetHack's rules
//! as far as the player can see them, and the shortest way between two
//! cells (A*). Only what the hero has seen counts: an unexplored cell is
//! never walked through.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use nh_protocol::{Catalog, GlyphKind, mg};

use crate::{COLNO, Cell, MapState, ROWNO, Terrain, cell_terrain, in_field};

/// W NW N NE E SE S SW: the order of `dirchars` ("hykulnjb").
pub const DIRS: [(i32, i32); 8] = [
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
];

/// The index of a step (dx, dy) in `DIRS`.
pub fn dir_index(dx: i32, dy: i32) -> Option<usize> {
    DIRS.iter().position(|&d| d == (dx, dy))
}

/// The key that steps (dx, dy) with these dirchars (`World::dirchars`).
pub fn dir_key(dirchars: &str, dx: i32, dy: i32) -> Option<char> {
    dirchars.chars().nth(dir_index(dx, dy)?)
}

/// The step a direction key makes with these dirchars.
pub fn key_dir(dirchars: &str, key: char) -> Option<(i32, i32)> {
    dirchars
        .chars()
        .take(8)
        .position(|c| c == key)
        .map(|i| DIRS[i])
}

/// Moves between two cells for a king (diagonals count one).
pub fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// What a cell is to a step, as far as the hero knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passage {
    /// Walkable ground.
    Open,
    /// An intact door, open or closed: never entered or left diagonally.
    /// A closed one opens when walked into (NetHack's `autoopen`).
    Door { closed: bool },
    /// A known trap: walked onto only as the goal.
    Trap,
    /// A monster that is not a pet, a remembered unseen one, a boulder:
    /// only the goal.
    Occupied,
    /// A pet: walked through by swapping places.
    Pet,
    /// Walls, stone, bars, trees, water, lava, a raised drawbridge.
    Blocked,
    /// Nothing seen there.
    Unknown,
}

impl Passage {
    fn is_door(self) -> bool {
        matches!(self, Passage::Door { .. })
    }
}

/// The ground under whatever stands on a cell.
fn ground(t: Terrain) -> Passage {
    use Terrain::*;
    match t {
        Floor | DarkFloor | Corridor | Doorway | BrokenDoor | StairsUp | StairsDown | LadderUp
        | LadderDown | Altar | Grave | Throne | Sink | Fountain | Ice | DrawbridgeDown | Air
        | Cloud => Passage::Open,
        OpenDoor => Passage::Door { closed: false },
        ClosedDoor => Passage::Door { closed: true },
        Trap => Passage::Trap,
        Stone | Wall | IronBars | Tree | Pool | Water | Lava | LavaWall | DrawbridgeUp => {
            Passage::Blocked
        }
        Effect | Unknown => Passage::Unknown,
    }
}

/// Is this object tile a boulder (by its appearance, the only thing known)?
fn is_boulder(catalog: &Catalog, tile: i32) -> bool {
    catalog
        .object_tiles
        .iter()
        .any(|t| t.tile == tile && t.appearance == "boulder")
}

/// A cell as a step sees it.
pub fn passage(cell: &Cell, catalog: &Catalog) -> Passage {
    let under = match cell_terrain(cell, catalog) {
        Some(t) => ground(t),
        None => Passage::Unknown,
    };
    let Some(g) = cell.entity() else {
        return under;
    };
    match g.kind {
        GlyphKind::Mon if g.flags & mg::HERO != 0 => under,
        GlyphKind::Mon if g.flags & mg::PET != 0 && under != Passage::Unknown => Passage::Pet,
        GlyphKind::Mon | GlyphKind::Invisible => Passage::Occupied,
        GlyphKind::Obj if is_boulder(catalog, g.tile) => Passage::Occupied,
        // an object or a corpse lies on something: the ground under it, or
        // plain floor when nothing else was ever seen there
        GlyphKind::Obj | GlyphKind::Body | GlyphKind::Statue if under == Passage::Unknown => {
            if g.kind == GlyphKind::Statue {
                Passage::Occupied
            } else {
                Passage::Open
            }
        }
        GlyphKind::Statue => Passage::Occupied,
        _ => under,
    }
}

/// Where a way ends: on the goal, or on a cell next to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reach {
    #[default]
    Onto,
    /// Any of the 8 neighbours (to fight what stands there).
    Adjacent,
    /// A neighbour straight N, S, E or W (to open a door).
    Orthogonal,
}

impl Reach {
    /// Does standing at `at` reach `goal`?
    pub fn reached(self, at: (i32, i32), goal: (i32, i32)) -> bool {
        let (dx, dy) = ((goal.0 - at.0).abs(), (goal.1 - at.1).abs());
        match self {
            Reach::Onto => at == goal,
            Reach::Adjacent => dx.max(dy) == 1,
            Reach::Orthogonal => dx + dy == 1,
        }
    }
}

/// Costs of a way, in tenths of a move: fewer moves first, then fewer
/// diagonals, then no pets and closed doors in the way.
const STEP: u32 = 10;
const DIAGONAL: u32 = 1;
const PET: u32 = 5;
const CLOSED_DOOR: u32 = 10;

/// The known map as passages, indexed like `MapState`.
struct Grid(Vec<Passage>);

impl Grid {
    fn of(map: &MapState, catalog: &Catalog) -> Grid {
        let mut cells = vec![Passage::Unknown; (COLNO * ROWNO) as usize];
        for y in 0..ROWNO {
            for x in 1..COLNO {
                if let Some(c) = map.cell(x, y) {
                    cells[(y * COLNO + x) as usize] = passage(c, catalog);
                }
            }
        }
        Grid(cells)
    }

    fn at(&self, (x, y): (i32, i32)) -> Passage {
        if in_field(x, y) {
            self.0[(y * COLNO + x) as usize]
        } else {
            Passage::Blocked
        }
    }
}

/// Can one step go from `a` to its neighbour `b` (`to` is what `b` is)?
fn step_ok(grid: &Grid, a: (i32, i32), b: (i32, i32), to: Passage, is_goal: bool) -> bool {
    let diagonal = a.0 != b.0 && a.1 != b.1;
    if diagonal && (grid.at(a).is_door() || to.is_door()) {
        return false;
    }
    match to {
        Passage::Open | Passage::Door { .. } | Passage::Pet => true,
        Passage::Trap | Passage::Occupied => is_goal,
        Passage::Blocked | Passage::Unknown => false,
    }
}

fn index((x, y): (i32, i32)) -> usize {
    (y * COLNO + x) as usize
}

/// The cells to walk from `from` to `goal`, the start excluded, the last
/// one the goal (or, with `Adjacent`/`Orthogonal`, a cell next to it).
/// `Some(vec![])` when `from` already reaches it; None when there is no
/// known way.
pub fn find_path(
    map: &MapState,
    catalog: &Catalog,
    from: (i32, i32),
    goal: (i32, i32),
    reach: Reach,
) -> Option<Vec<(i32, i32)>> {
    if !in_field(from.0, from.1) || !in_field(goal.0, goal.1) {
        return None;
    }
    if reach.reached(from, goal) {
        return Some(Vec::new());
    }
    let grid = Grid::of(map, catalog);
    let n = (COLNO * ROWNO) as usize;
    let mut best = vec![u32::MAX; n];
    let mut came = vec![usize::MAX; n];
    let h = |c: (i32, i32)| {
        let d = match reach {
            Reach::Onto => chebyshev(c, goal),
            _ => (chebyshev(c, goal) - 1).max(0),
        };
        d as u32 * STEP
    };
    let mut open = BinaryHeap::new();
    best[index(from)] = 0;
    // (f, tie-break on g: deeper first, then the cell) — deterministic
    open.push(Reverse((h(from), Reverse(0u32), index(from))));
    while let Some(Reverse((_, Reverse(g), i))) = open.pop() {
        if g > best[i] {
            continue;
        }
        let at = ((i as i32) % COLNO, (i as i32) / COLNO);
        if reach.reached(at, goal) {
            let mut way = vec![at];
            let mut j = i;
            while came[j] != index(from) {
                j = came[j];
                way.push(((j as i32) % COLNO, (j as i32) / COLNO));
            }
            way.reverse();
            return Some(way);
        }
        for (dx, dy) in DIRS {
            let next = (at.0 + dx, at.1 + dy);
            if !in_field(next.0, next.1) {
                continue;
            }
            let to = grid.at(next);
            // ending next to the goal: the goal itself is never entered
            let is_goal = reach == Reach::Onto && next == goal;
            if reach != Reach::Onto && next == goal {
                continue;
            }
            if !step_ok(&grid, at, next, to, is_goal) {
                continue;
            }
            let mut cost = g + STEP;
            if dx != 0 && dy != 0 {
                cost += DIAGONAL;
            }
            match to {
                Passage::Pet => cost += PET,
                Passage::Door { closed: true } => cost += CLOSED_DOOR,
                _ => {}
            }
            let j = index(next);
            if cost < best[j] {
                best[j] = cost;
                came[j] = i;
                open.push(Reverse((cost + h(next), Reverse(cost), j)));
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use nh_protocol::{EngineMsg, Glyph, parse_line};

    use super::*;
    use crate::map::tests::glyph;

    pub(crate) fn catalog() -> Catalog {
        match parse_line(include_str!("../tests/data/catalog.jsonl")).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    pub(crate) fn cmap(cat: &Catalog, sym: &str) -> Glyph {
        let c = cat.cmap.iter().find(|c| c.sym == sym).unwrap();
        Glyph {
            cmap: Some(c.idx),
            ..glyph(GlyphKind::Cmap, char::from_u32(c.ch as u32).unwrap_or('?'))
        }
    }

    pub(crate) const HERO_MON: i32 = 342;
    pub(crate) const PET_MON: i32 = 32; // kitten
    pub(crate) const FOE_MON: i32 = 12; // jackal

    pub(crate) fn mon(m: i32, flags: u32) -> Glyph {
        Glyph {
            mon: Some(m),
            flags,
            ..glyph(GlyphKind::Mon, 'd')
        }
    }

    /// A map from rows of text, the top row at y = `top`, x from 1:
    /// `.` floor, `#` corridor, `-` `|` walls, `d` doorway, `o` open door,
    /// `+` closed door, `}` water, `L` lava, `^` trap, `<` `>` stairs,
    /// `T` tree; on floor: `@` hero, `f` pet, `J` jackal, `0` boulder,
    /// `$` an object, `I` a remembered unseen monster. Space: unexplored.
    pub(crate) fn draw(map: &mut MapState, cat: &Catalog, top: i32, rows: &[&str]) {
        let boulder = cat
            .object_tiles
            .iter()
            .find(|t| t.appearance == "boulder")
            .unwrap()
            .tile;
        let gold = cat
            .object_tiles
            .iter()
            .find(|t| t.class == "$")
            .unwrap()
            .tile;
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                let (x, y) = (dx as i32 + 1, top + dy as i32);
                let sym = match ch {
                    '.' | '@' | 'f' | 'J' | '0' | '$' | 'I' => "S_room",
                    '#' => "S_corr",
                    '-' => "S_hwall",
                    '|' => "S_vwall",
                    'd' => "S_ndoor",
                    'o' => "S_vodoor",
                    '+' => "S_vcdoor",
                    '}' => "S_pool",
                    'L' => "S_lava",
                    '^' => "S_arrow_trap",
                    '<' => "S_upstair",
                    '>' => "S_dnstair",
                    'T' => "S_tree",
                    _ => continue,
                };
                let under = cmap(cat, sym);
                map.print(x, y, &under, None);
                let on = match ch {
                    '@' => Some(mon(HERO_MON, mg::HERO)),
                    'f' => Some(mon(PET_MON, mg::PET)),
                    'J' => Some(mon(FOE_MON, 0)),
                    '0' => Some(Glyph {
                        tile: boulder,
                        ..glyph(GlyphKind::Obj, '0')
                    }),
                    '$' => Some(Glyph {
                        tile: gold,
                        ..glyph(GlyphKind::Obj, '$')
                    }),
                    'I' => Some(glyph(GlyphKind::Invisible, 'I')),
                    _ => None,
                };
                if let Some(g) = on {
                    map.print(x, y, &g, Some(&under));
                }
            }
        }
        map.take_dirty();
    }

    fn way(
        rows: &[&str],
        from: (i32, i32),
        to: (i32, i32),
        reach: Reach,
    ) -> Option<Vec<(i32, i32)>> {
        let cat = catalog();
        let mut map = MapState::new();
        draw(&mut map, &cat, 0, rows);
        find_path(&map, &cat, from, to, reach)
    }

    /// Every step of a way is a king's move.
    fn steps_are_single(from: (i32, i32), way: &[(i32, i32)]) {
        let mut at = from;
        for &c in way {
            assert_eq!(chebyshev(at, c), 1, "{at:?} -> {c:?} in {way:?}");
            at = c;
        }
    }

    #[test]
    fn dir_keys_follow_dirchars() {
        assert_eq!(dir_key("hykulnjb><", -1, 0), Some('h'));
        assert_eq!(dir_key("hykulnjb><", 1, 1), Some('n'));
        assert_eq!(dir_key("hykulnjb><", 0, -1), Some('k'));
        assert_eq!(dir_key("47896321><", 1, -1), Some('9'));
        assert_eq!(dir_key("hykulnjb><", 0, 0), None);
        assert_eq!(dir_key("hykulnjb><", 2, 0), None);
        assert_eq!(key_dir("hykulnjb><", 'b'), Some((-1, 1)));
        assert_eq!(key_dir("hykulnjb><", '>'), None);
        assert_eq!(key_dir("47896321><", '6'), Some((1, 0)));
    }

    #[test]
    fn path_goes_round_walls() {
        let rows = [
            "-------", //
            "|..|..|", //
            "|..|..|", //
            "|.....|", //
            "-------",
        ];
        let w = way(&rows, (2, 1), (6, 1), Reach::Onto).unwrap();
        steps_are_single((2, 1), &w);
        assert_eq!(w.last(), Some(&(6, 1)));
        assert!(w.contains(&(4, 3)), "through the gap: {w:?}");
        // the fewest moves: diagonals down and up again
        assert_eq!(w.len(), 4, "{w:?}");
        // already there
        assert_eq!(way(&rows, (2, 1), (2, 1), Reach::Onto), Some(vec![]));
        // into a wall: no way
        assert_eq!(way(&rows, (2, 1), (4, 1), Reach::Onto), None);
    }

    #[test]
    fn path_never_enters_or_leaves_a_door_diagonally() {
        let rows = [
            "|.....", //
            "|.....", //
            "---o--", //
            "|.....", //
            "|.....",
        ];
        // from north-west of the door to south-east of it: straight
        // through the door, never cutting its corners
        let w = way(&rows, (3, 1), (5, 4), Reach::Onto).unwrap();
        steps_are_single((3, 1), &w);
        let door = w
            .iter()
            .position(|&c| c == (4, 2))
            .expect("through the door");
        let before = if door == 0 { (3, 1) } else { w[door - 1] };
        assert_eq!(before, (4, 1), "entered straight: {w:?}");
        assert_eq!(w[door + 1], (4, 3), "left straight: {w:?}");
    }

    #[test]
    fn path_takes_doorless_doorways_diagonally() {
        let rows = [
            "|.....", //
            "|.....", //
            "---d--", //
            "|.....", //
            "|.....",
        ];
        let w = way(&rows, (3, 1), (5, 3), Reach::Onto).unwrap();
        assert_eq!(w, vec![(4, 2), (5, 3)]);
    }

    #[test]
    fn path_walks_through_closed_doors() {
        let rows = [
            "|...|", //
            "|...|", //
            "--+--", //
            "|...|",
        ];
        let w = way(&rows, (2, 1), (3, 3), Reach::Onto).unwrap();
        assert_eq!(w, vec![(3, 1), (3, 2), (3, 3)]);
    }

    #[test]
    fn path_avoids_traps_water_and_lava_but_ends_on_a_trap() {
        let rows = [
            ".....", //
            ".^}L.", //
            ".....",
        ];
        let w = way(&rows, (1, 1), (5, 1), Reach::Onto).unwrap();
        for c in [(2, 1), (3, 1), (4, 1)] {
            assert!(!w.contains(&c), "{c:?} in {w:?}");
        }
        assert_eq!(w.len(), 4, "round them: {w:?}");
        // the trap itself as the goal
        assert_eq!(way(&rows, (1, 1), (2, 1), Reach::Onto), Some(vec![(2, 1)]));
        // water and lava never, not even as the goal
        assert_eq!(way(&rows, (2, 0), (3, 1), Reach::Onto), None);
        assert_eq!(way(&[".L."], (1, 0), (3, 0), Reach::Onto), None);
        // boulders and trees block
        assert_eq!(way(&[".0."], (1, 0), (3, 0), Reach::Onto), None);
        assert_eq!(way(&[".T."], (1, 0), (3, 0), Reach::Onto), None);
    }

    #[test]
    fn path_never_crosses_unknown_cells() {
        assert_eq!(way(&[".. .."], (1, 0), (5, 0), Reach::Onto), None);
        assert_eq!(way(&["....."], (1, 0), (7, 0), Reach::Onto), None);
        // corridors meet diagonally
        let w = way(&["##  ", "  ##"], (1, 0), (4, 1), Reach::Onto).unwrap();
        assert_eq!(w, vec![(2, 0), (3, 1), (4, 1)]);
    }

    #[test]
    fn path_ends_next_to_a_monster() {
        let rows = ["......J"];
        let w = way(&rows, (1, 0), (7, 0), Reach::Adjacent).unwrap();
        assert_eq!(w.last(), Some(&(6, 0)));
        assert_eq!(w.len(), 5);
        // onto the monster: only as the goal
        let w = way(&rows, (1, 0), (7, 0), Reach::Onto).unwrap();
        assert_eq!(w.last(), Some(&(7, 0)));
        // next to it already
        assert_eq!(way(&rows, (6, 0), (7, 0), Reach::Adjacent), Some(vec![]));
        // a door is opened from straight in front of it
        let rows = ["|...|", "--+--"];
        let w = way(&rows, (2, 0), (3, 1), Reach::Orthogonal).unwrap();
        assert_eq!(w, vec![(3, 0)]);
        assert!(Reach::Orthogonal.reached((3, 0), (3, 1)));
        assert!(!Reach::Orthogonal.reached((2, 0), (3, 1)));
    }

    #[test]
    fn path_swaps_with_pets_but_not_through_others() {
        let rows = ["|.|", "|f|", "|.|", "|J|", "|.|"];
        let w = way(&rows, (2, 0), (2, 2), Reach::Onto).unwrap();
        assert_eq!(w, vec![(2, 1), (2, 2)]);
        assert_eq!(way(&rows, (2, 2), (2, 4), Reach::Onto), None);
        // around a pet when there is room
        let rows = ["...", ".f.", "..."];
        let w = way(&rows, (1, 1), (3, 1), Reach::Onto).unwrap();
        assert!(!w.contains(&(2, 1)), "{w:?}");
        // a remembered unseen monster blocks too
        assert_eq!(way(&["|.I.|"], (2, 0), (4, 0), Reach::Onto), None);
        // the hero's own cell is ground
        let cat = catalog();
        let mut map = MapState::new();
        draw(&mut map, &cat, 0, &["..@.."]);
        assert_eq!(passage(map.cell(3, 0).unwrap(), &cat), Passage::Open);
        draw(&mut map, &cat, 1, &["$"]);
        assert_eq!(passage(map.cell(1, 1).unwrap(), &cat), Passage::Open);
    }
}
