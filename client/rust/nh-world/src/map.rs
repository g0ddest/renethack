use nh_protocol::{Glyph, GlyphKind, mg};

use crate::motion::{Change, Ident, Move, detect_moves};

/// Map width in NetHack; column 0 is never used.
pub const COLNO: i32 = 80;
pub const ROWNO: i32 = 21;

/// Is (x, y) a map cell (x 1..COLNO, y 0..ROWNO)?
pub fn in_field(x: i32, y: i32) -> bool {
    (1..COLNO).contains(&x) && (0..ROWNO).contains(&y)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cell {
    /// What the cell shows: the last print_glyph `g` (None: not drawn since
    /// the last clear), but not one that only passes over it (`over`).
    pub glyph: Option<Glyph>,
    pub bk: Option<Glyph>,
    /// Last known map feature (a Cmap glyph): `glyph` when Cmap, else `bk` when Cmap;
    /// kept when both are something else. Effect cmaps (beams, sparkles, S_goodpos)
    /// never become terrain: they are drawn for a moment over whatever is there.
    pub terrain: Option<Glyph>,
    /// What is drawn over the cell for a moment, `glyph` staying what it
    /// shows: a ray, an explosion or a sparkle over anything; over the hero,
    /// whatever the engine draws there (a missile that reaches them, the
    /// floor they step off before it draws them on the next cell). The
    /// engine draws the cell again when it has passed.
    pub over: Option<Glyph>,
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
    /// What each dirty cell showed before it was first drawn in this batch.
    before: Vec<Option<Glyph>>,
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
            before: Vec::new(),
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

    /// A glyph shown for a moment over whatever a cell shows: a ray, an
    /// explosion, an effect cmap.
    fn is_effect(&self, g: &Glyph) -> bool {
        match g.kind {
            GlyphKind::Zap | GlyphKind::Explosion => true,
            GlyphKind::Cmap => !self.is_terrain(g),
            _ => false,
        }
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
        self.take_dirty_moves().0
    }

    /// Cells drawn since the last call (as `take_dirty`), and the entities
    /// that stepped to a neighbouring cell among them (see `detect_moves`).
    pub fn take_dirty_moves(&mut self) -> (Vec<(i32, i32)>, Vec<Move>) {
        let before = std::mem::take(&mut self.before);
        let dirty = std::mem::take(&mut self.dirty);
        let mut changes = Vec::new();
        for (&(x, y), was) in dirty.iter().zip(&before) {
            let Some(i) = index(x, y) else {
                continue;
            };
            self.is_dirty[i] = false;
            let before = was.as_ref().and_then(Ident::of);
            let after = self.cells[i].glyph.as_ref().and_then(Ident::of);
            if before.is_some() || after.is_some() {
                changes.push(Change {
                    at: (x, y),
                    before,
                    after,
                });
            }
        }
        (dirty, detect_moves(&changes))
    }

    /// The last cell drawn with MG_HERO: the hero stays there, whatever is
    /// drawn over them, until the engine draws them elsewhere. None once it
    /// asks for a command with the cell showing something else (`settle`:
    /// an invisible or hiding hero); `World::hero` falls back to curs() then.
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
        self.touch(i, (x, y));
        let hero = g.flags & mg::HERO != 0;
        // an effect passes over what the cell shows, and anything at all
        // over the hero: they are there until drawn elsewhere
        let passes = !hero
            && self.cells[i].glyph.as_ref().is_some_and(|under| {
                self.hero == Some((x, y)) || (self.is_effect(g) && !self.is_effect(under))
            });
        let cell = &mut self.cells[i];
        if let Some(t) = terrain {
            cell.terrain = Some(t.clone());
        }
        if passes {
            cell.over = Some(g.clone());
        } else {
            cell.glyph = Some(g.clone());
            cell.over = None;
        }
        cell.bk = bk.cloned();
        if hero
            && let Some(left) = self.hero.replace((x, y))
            && left != (x, y)
        {
            self.uncover(left);
        }
    }

    /// The engine asks for a command: nothing passes over the hero any
    /// more. A cell of theirs that still shows something else shows it for
    /// good: they are not drawn at all (invisible, hiding).
    pub fn settle(&mut self) {
        if let Some((x, y)) = self.hero
            && self.cell(x, y).is_some_and(|c| c.over.is_some())
        {
            self.uncover((x, y));
            self.hero = None;
        }
    }

    /// The cell is drawn in this batch: what it showed before is kept.
    fn touch(&mut self, i: usize, at: (i32, i32)) {
        if !self.is_dirty[i] {
            self.is_dirty[i] = true;
            self.dirty.push(at);
            self.before.push(self.cells[i].glyph.clone());
        }
    }

    /// The hero is not on the cell any more: it shows what was drawn over
    /// them.
    fn uncover(&mut self, (x, y): (i32, i32)) {
        let Some(i) = index(x, y).filter(|&i| self.cells[i].over.is_some()) else {
            return;
        };
        self.touch(i, (x, y));
        let cell = &mut self.cells[i];
        cell.glyph = cell.over.take();
    }

    /// clear_nhwindow on the map window: nothing is known any more.
    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.is_dirty.fill(false);
        self.dirty.clear();
        self.before.clear();
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
        assert_eq!(cell.over, Some(effect(82)), "the view can still flash it");
        assert_eq!(
            cell.glyph,
            Some(monster(16, 0)),
            "over the dog, still there"
        );
        assert_eq!(cell.terrain, Some(floor()));
        map.print(10, 5, &monster(16, 0), Some(&unexplored()));
        let cell = map.cell(10, 5).unwrap();
        assert_eq!((&cell.over, &cell.terrain), (&None, &Some(floor())));
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
    fn a_batch_of_drawing_tells_who_stepped_where() {
        let mut map = MapState::new();
        for x in 8..14 {
            map.print(x, 5, &floor(), None);
        }
        map.print(10, 5, &monster(0, mg::HERO), None);
        map.print(11, 5, &monster(16, mg::PET), None);
        map.take_dirty();
        // the hero steps east and swaps places with the pet (the hero's old
        // cell drawn twice in the batch: the first "before" counts)
        map.print(11, 5, &monster(0, mg::HERO), None);
        map.print(10, 5, &floor(), None);
        map.print(10, 5, &monster(16, mg::PET), None);
        let (dirty, moves) = map.take_dirty_moves();
        assert_eq!(dirty, vec![(11, 5), (10, 5)]);
        let pairs: Vec<_> = moves.iter().map(|m| (m.from, m.to)).collect();
        assert_eq!(pairs, vec![((11, 5), (10, 5)), ((10, 5), (11, 5))]);
        // nothing new: no moves
        assert_eq!(map.take_dirty_moves(), (vec![], vec![]));
        // a clear forgets the batch: a new level never animates
        map.print(11, 5, &floor(), None);
        map.clear();
        map.print(12, 5, &monster(0, mg::HERO), None);
        assert!(map.take_dirty_moves().1.is_empty());
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
        // turning invisible: the hero's cell is redrawn without MG_HERO,
        // and still shows the floor when the engine asks for a command
        map.print(19, 5, &floor(), None);
        assert_eq!(map.hero(), Some((19, 5)), "it may only pass over them");
        map.take_dirty();
        map.settle();
        assert_eq!(map.hero(), None);
        let cell = map.cell(19, 5).unwrap();
        assert_eq!((&cell.glyph, &cell.over), (&Some(floor()), &None));
        assert_eq!(map.take_dirty(), vec![(19, 5)], "the view draws it again");
        // engulfed: swallow glyphs around the hero
        map.print(19, 5, &monster(0, mg::HERO), None);
        assert!(!map.is_swallowed());
        map.print(18, 4, &glyph(GlyphKind::Swallow, '/'), None);
        assert!(map.is_swallowed());
    }

    #[test]
    fn the_hero_stays_under_what_is_drawn_over_them() {
        let mut map = MapState::new();
        let hero = monster(0, mg::HERO);
        let dagger = glyph(GlyphKind::Obj, ')');
        for x in 8..14 {
            map.print(x, 5, &floor(), None);
        }
        map.print(10, 5, &hero, None);
        map.take_dirty();
        // a step east, then a thrown dagger flies in from the east and
        // reaches them: tmp_at draws it on their cell for a moment
        map.print(10, 5, &floor(), None);
        map.print(11, 5, &hero, None);
        map.print(13, 5, &dagger, None);
        let (_, moves) = map.take_dirty_moves();
        assert_eq!(moves.len(), 1, "the hero's step");
        map.print(13, 5, &floor(), None);
        map.print(12, 5, &dagger, None);
        map.take_dirty();
        map.print(12, 5, &floor(), None);
        map.print(11, 5, &dagger, None);
        assert_eq!(map.hero(), Some((11, 5)), "not back on the cell they left");
        let cell = map.cell(11, 5).unwrap();
        assert_eq!(
            (&cell.glyph, &cell.over),
            (&Some(hero.clone()), &Some(dagger))
        );
        assert_eq!(cell.entity(), Some(&hero));
        // the cell is drawn again for the dagger, and nobody went anywhere
        // (the dagger did: it is not on the cell it flew from)
        let (dirty, moves) = map.take_dirty_moves();
        assert_eq!(dirty, vec![(12, 5), (11, 5)]);
        assert!(moves.is_empty(), "{moves:?}");
        // the flight over, newsym draws the hero again; a command changes nothing
        map.print(11, 5, &hero, None);
        map.settle();
        let cell = map.cell(11, 5).unwrap();
        assert_eq!((&cell.glyph, &cell.over), (&Some(hero), &None));
        assert_eq!(map.hero(), Some((11, 5)));
    }

    #[test]
    fn a_step_drawn_in_two_batches_is_still_a_step() {
        let mut map = MapState::new();
        let hero = monster(0, mg::HERO);
        for x in 8..14 {
            map.print(x, 5, &floor(), None);
        }
        map.print(10, 5, &hero, None);
        map.take_dirty();
        // the cell they leave is drawn, and a frame comes before the next
        map.print(10, 5, &floor(), None);
        assert_eq!(map.hero(), Some((10, 5)));
        assert_eq!(map.cell(10, 5).unwrap().entity(), Some(&hero));
        assert_eq!(map.take_dirty_moves(), (vec![(10, 5)], vec![]));
        // drawn on the next cell: the one they left shows its floor
        map.print(11, 5, &hero, None);
        assert_eq!(map.hero(), Some((11, 5)));
        let left = map.cell(10, 5).unwrap();
        assert_eq!((&left.glyph, &left.over), (&Some(floor()), &None));
        let (dirty, moves) = map.take_dirty_moves();
        assert_eq!(dirty, vec![(11, 5), (10, 5)]);
        let pairs: Vec<_> = moves.iter().map(|m| (m.from, m.to)).collect();
        assert_eq!(pairs, vec![((10, 5), (11, 5))]);
    }

    #[test]
    fn an_effect_passes_over_what_a_cell_shows() {
        let mut map = MapState::new();
        map.set_effect_cmaps([82]); // S_ss1
        let ray = glyph(GlyphKind::Zap, '-');
        let blast = glyph(GlyphKind::Explosion, '#');
        let sparkle = Glyph {
            cmap: Some(82),
            ..glyph(GlyphKind::Cmap, '0')
        };
        let pet = monster(16, mg::PET);
        let sword = glyph(GlyphKind::Obj, ')');
        map.print(10, 5, &pet, None);
        map.print(11, 5, &sword, None);
        map.print(12, 5, &floor(), None);
        map.take_dirty();
        for effect in [&ray, &blast, &sparkle] {
            for (x, under) in [(10, &pet), (11, &sword), (12, &floor())] {
                map.print(x, 5, effect, None);
                let cell = map.cell(x, 5).unwrap();
                assert_eq!(
                    (cell.glyph.as_ref(), cell.over.as_ref()),
                    (Some(under), Some(effect))
                );
            }
            // drawn again for the effect; nobody appeared or went
            let (dirty, moves) = map.take_dirty_moves();
            assert_eq!((dirty.len(), moves.len()), (3, 0));
        }
        // the engine draws the cells again when it is over: the pet died
        map.print(10, 5, &glyph(GlyphKind::Body, '%'), None);
        map.print(11, 5, &sword, None);
        let cell = map.cell(10, 5).unwrap();
        assert_eq!(cell.glyph.as_ref().map(|g| g.kind), Some(GlyphKind::Body));
        assert!(cell.over.is_none() && map.cell(11, 5).unwrap().over.is_none());
        // over a cell that shows nothing, or another effect, it is all there is
        map.print(20, 5, &ray, None);
        map.print(20, 5, &blast, None);
        let cell = map.cell(20, 5).unwrap();
        assert_eq!((&cell.glyph, &cell.over), (&Some(blast), &None));
        // a thing over a creature that is not the hero replaces it: it may
        // have left
        map.print(10, 6, &pet, None);
        map.print(10, 6, &sword, None);
        assert_eq!(map.cell(10, 6).unwrap().glyph, Some(sword));
    }
}
