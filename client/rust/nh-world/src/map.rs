use nh_protocol::{Glyph, GlyphKind, mg};

/// Map width in NetHack; column 0 is never used.
pub const COLNO: i32 = 80;
pub const ROWNO: i32 = 21;

/// Is (x, y) a map cell (x 1..COLNO, y 0..ROWNO)?
pub fn in_field(x: i32, y: i32) -> bool {
    (1..COLNO).contains(&x) && (0..ROWNO).contains(&y)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cell {
    /// Last print_glyph `g` (None: not drawn since the last clear).
    pub glyph: Option<Glyph>,
    pub bk: Option<Glyph>,
    /// Last known map feature (a Cmap glyph): `glyph` when Cmap, else `bk` when Cmap;
    /// kept when both are something else. Effect cmaps (beams, sparkles, S_goodpos)
    /// never become terrain: they are drawn for a moment over whatever is there.
    pub terrain: Option<Glyph>,
}

impl Cell {
    /// Non-Cmap glyph on top (monster, object, body, statue, invisible, warning, zap,
    /// explosion, swallow).
    pub fn entity(&self) -> Option<&Glyph> {
        self.glyph.as_ref().filter(|g| {
            matches!(
                g.kind,
                GlyphKind::Mon
                    | GlyphKind::Obj
                    | GlyphKind::Body
                    | GlyphKind::Statue
                    | GlyphKind::Invisible
                    | GlyphKind::Warning
                    | GlyphKind::Zap
                    | GlyphKind::Explosion
                    | GlyphKind::Swallow
            )
        })
    }
}

/// What the map window shows, cell by cell.
#[derive(Debug, Clone)]
pub struct MapState {
    cells: Vec<Cell>,
    dirty: Vec<(i32, i32)>,
    is_dirty: Vec<bool>,
    generation: u64,
    hero: Option<(i32, i32)>,
    /// Cmap indices that are brief effects, not terrain.
    effect_cmaps: Vec<bool>,
}

impl Default for MapState {
    fn default() -> Self {
        MapState::new()
    }
}

fn index(x: i32, y: i32) -> Option<usize> {
    in_field(x, y).then(|| (y * COLNO + x) as usize)
}

impl MapState {
    pub fn new() -> MapState {
        let n = (COLNO * ROWNO) as usize;
        MapState {
            cells: vec![Cell::default(); n],
            dirty: Vec::new(),
            is_dirty: vec![false; n],
            generation: 0,
            hero: None,
            effect_cmaps: Vec::new(),
        }
    }

    /// Cmap indices drawn only for a moment (see `Terrain::Effect`): they never
    /// replace a cell's remembered terrain.
    pub fn set_effect_cmaps(&mut self, cmaps: impl IntoIterator<Item = i32>) {
        self.effect_cmaps.clear();
        for c in cmaps.into_iter().filter_map(|c| usize::try_from(c).ok()) {
            if c >= self.effect_cmaps.len() {
                self.effect_cmaps.resize(c + 1, false);
            }
            self.effect_cmaps[c] = true;
        }
    }

    /// A Cmap glyph that is a lasting map feature.
    fn is_terrain(&self, g: &Glyph) -> bool {
        g.kind == GlyphKind::Cmap
            && !g
                .cmap
                .and_then(|c| usize::try_from(c).ok())
                .is_some_and(|c| self.effect_cmaps.get(c).copied().unwrap_or(false))
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&Cell> {
        index(x, y).map(|i| &self.cells[i])
    }

    /// Bumped by every clear: the view rebuilds everything.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Cells drawn since the last call, each once, in drawing order.
    pub fn take_dirty(&mut self) -> Vec<(i32, i32)> {
        for &(x, y) in &self.dirty {
            if let Some(i) = index(x, y) {
                self.is_dirty[i] = false;
            }
        }
        std::mem::take(&mut self.dirty)
    }

    /// The last cell drawn with MG_HERO; None once that cell is redrawn without it
    /// (invisible or hiding hero). `World::hero` falls back to curs() at a command.
    pub fn hero(&self) -> Option<(i32, i32)> {
        self.hero
    }

    /// Engulfed: swallow glyphs around the hero.
    pub fn is_swallowed(&self) -> bool {
        let Some((hx, hy)) = self.hero else {
            return false;
        };
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (hx + dx, hy + dy)))
            .filter_map(|(x, y)| self.cell(x, y))
            .any(|c| {
                c.glyph
                    .as_ref()
                    .is_some_and(|g| g.kind == GlyphKind::Swallow)
            })
    }

    /// print_glyph on the map window; cells outside the field are ignored.
    pub fn print(&mut self, x: i32, y: i32, g: &Glyph, bk: Option<&Glyph>) {
        let Some(i) = index(x, y) else {
            return;
        };
        let terrain = if g.kind == GlyphKind::Cmap {
            self.is_terrain(g).then_some(g)
        } else {
            bk.filter(|b| self.is_terrain(b))
        };
        let cell = &mut self.cells[i];
        if let Some(t) = terrain {
            cell.terrain = Some(t.clone());
        }
        cell.glyph = Some(g.clone());
        cell.bk = bk.cloned();
        if g.flags & mg::HERO != 0 {
            self.hero = Some((x, y));
        } else if self.hero == Some((x, y)) {
            self.hero = None;
        }
        if !self.is_dirty[i] {
            self.is_dirty[i] = true;
            self.dirty.push((x, y));
        }
    }

    /// clear_nhwindow on the map window: nothing is known any more.
    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.is_dirty.fill(false);
        self.dirty.clear();
        self.hero = None;
        self.generation += 1;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn glyph(kind: GlyphKind, ch: char) -> Glyph {
        Glyph {
            glyph: None,
            ch: ch as i32,
            color: 7,
            flags: 0,
            tile: 0,
            kind,
            mon: None,
            cmap: None,
            level: None,
        }
    }

    pub(crate) fn floor() -> Glyph {
        Glyph {
            cmap: Some(19), // S_room
            ..glyph(GlyphKind::Cmap, '.')
        }
    }

    pub(crate) fn monster(mon: i32, flags: u32) -> Glyph {
        Glyph {
            mon: Some(mon),
            flags,
            ..glyph(GlyphKind::Mon, 'd')
        }
    }

    fn unexplored() -> Glyph {
        glyph(GlyphKind::Unexplored, ' ')
    }

    #[test]
    fn map_remembers_terrain_under_entities() {
        let mut map = MapState::new();
        map.print(10, 5, &floor(), Some(&unexplored()));
        // a dog steps on it; the engine's bk says nothing useful
        map.print(10, 5, &monster(16, 0), Some(&unexplored()));
        let cell = map.cell(10, 5).unwrap();
        assert_eq!(cell.terrain, Some(floor()));
        assert_eq!(cell.entity().map(|g| g.kind), Some(GlyphKind::Mon));
        // an object with a known floor underneath
        let door = Glyph {
            cmap: Some(12),
            ..glyph(GlyphKind::Cmap, '.')
        };
        map.print(11, 5, &glyph(GlyphKind::Obj, ')'), Some(&door));
        assert_eq!(map.cell(11, 5).unwrap().terrain, Some(door));
        // the dog leaves: the floor is the glyph again, no entity
        map.print(10, 5, &floor(), Some(&unexplored()));
        assert_eq!(map.cell(10, 5).unwrap().entity(), None);
    }

    #[test]
    fn effects_over_an_entity_keep_the_terrain() {
        let mut map = MapState::new();
        map.set_effect_cmaps([82, 87]); // S_ss1, S_goodpos
        let effect = |cmap| Glyph {
            cmap: Some(cmap),
            ..glyph(GlyphKind::Cmap, '0')
        };
        map.print(10, 5, &floor(), Some(&unexplored()));
        map.print(10, 5, &monster(16, 0), Some(&unexplored()));
        // shieldeff sparkles over the dog, then newsym redraws it
        map.print(10, 5, &effect(82), Some(&unexplored()));
        let cell = map.cell(10, 5).unwrap();
        assert_eq!(cell.glyph, Some(effect(82)), "the view can still flash it");
        assert_eq!(cell.terrain, Some(floor()));
        map.print(10, 5, &monster(16, 0), Some(&unexplored()));
        assert_eq!(map.cell(10, 5).unwrap().terrain, Some(floor()));
        // a target marker on an unexplored cell leaves it unknown
        map.print(12, 5, &effect(87), None);
        assert_eq!(map.cell(12, 5).unwrap().terrain, None);
        // the effect set survives a clear; other cmaps are still terrain
        map.clear();
        map.print(10, 5, &floor(), None);
        map.print(10, 5, &effect(82), None);
        assert_eq!(map.cell(10, 5).unwrap().terrain, Some(floor()));
        map.set_effect_cmaps([-1]);
        map.print(10, 5, &effect(82), None);
        assert_eq!(map.cell(10, 5).unwrap().terrain, Some(effect(82)));
    }

    #[test]
    fn clear_bumps_generation_and_empties_cells() {
        let mut map = MapState::new();
        map.print(10, 5, &floor(), None);
        map.print(10, 6, &monster(0, mg::HERO), None);
        map.print(10, 5, &floor(), None);
        assert_eq!(map.take_dirty(), vec![(10, 5), (10, 6)]);
        assert!(map.take_dirty().is_empty());
        map.print(10, 5, &floor(), None);
        let before = map.generation();
        map.clear();
        assert_eq!(map.generation(), before + 1);
        assert_eq!(map.cell(10, 5), Some(&Cell::default()));
        assert_eq!(map.hero(), None);
        assert!(map.take_dirty().is_empty());
        // outside the field
        map.print(0, 5, &floor(), None);
        map.print(COLNO, 5, &floor(), None);
        map.print(5, ROWNO, &floor(), None);
        assert!(map.take_dirty().is_empty());
        assert_eq!(map.cell(0, 5), None);
    }

    #[test]
    fn hero_follows_mg_hero_and_forgets_an_unseen_hero() {
        let mut map = MapState::new();
        map.print(18, 5, &monster(0, mg::HERO), None);
        assert_eq!(map.hero(), Some((18, 5)));
        // a step east: the new cell is drawn, then the old one
        map.print(19, 5, &monster(0, mg::HERO), None);
        map.print(18, 5, &floor(), None);
        assert_eq!(map.hero(), Some((19, 5)));
        // turning invisible: the hero's cell is redrawn without MG_HERO
        map.print(19, 5, &floor(), None);
        assert_eq!(map.hero(), None);
        // engulfed: swallow glyphs around the hero
        map.print(19, 5, &monster(0, mg::HERO), None);
        assert!(!map.is_swallowed());
        map.print(18, 4, &glyph(GlyphKind::Swallow, '/'), None);
        assert!(map.is_swallowed());
    }
}
