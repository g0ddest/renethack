//! A level played behind the title screen: a room with every feature,
//! liquid, trap, door, monster and object class, drawn in each branch
//! that has its own models and lit as in the game, with effects fired
//! over it. Godot compiles the render pipelines (and allocates the shadow
//! atlas and the buffers) of what it draws for the first time: done here,
//! no frame of the first levels waits for it.
//!
//! It holds the game's own frame budget: what it draws is loaded first in
//! the background, what the map keeps in its pools (torches, lanterns,
//! the branches' props) is made ahead one piece a frame, each level is
//! drawn over frames as in the game, and its effects go off one a frame.

use godot::classes::{ResourceLoader, Texture2D};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, LevelNotice, mg};
use nh_world::{Branch, World};

use crate::map_view::{MapView, ROLES, WARM_MONSTERS, role_monster};
use crate::vfx::VfxKind;

/// The branches drawn: the main one, and those with models, walls,
/// liquids or lights of their own.
const STAGES: &[(&str, Branch)] = &[
    ("The Dungeons of Doom", Branch::Main),
    ("The Gnomish Mines", Branch::Mines),
    ("Gehennom", Branch::Gehennom),
    ("Fort Ludios", Branch::Ludios),
    ("Vlad's Tower", Branch::Vlad),
];

/// Frames a stage is held once it is drawn: an effect a frame, then the
/// pipelines compiled in the background.
const HOLD: u32 = 30;
/// A frame's time for drawing the first stage's cells (the game's is
/// 4 ms: the first level drawn compiles and allocates as it goes).
const FIRST_BUDGET: std::time::Duration = std::time::Duration::from_micros(1500);
/// A stage that never settles moves on after this many frames.
const MOST: u32 = 300;

/// The room's corners.
const X0: i32 = 30;
const X1: i32 = 50;
const Y0: i32 = 3;
const Y1: i32 = 15;
const HERO: (i32, i32) = (40, 14);

/// A scene (its meshes come with it), not a texture.
fn is_scene(path: &str) -> bool {
    [".gltf", ".glb", ".tscn", ".scn"]
        .iter()
        .any(|e| path.ends_with(e))
}

/// Ask for `path` to load in the background (gdext leaves the threaded
/// loads out without its threads feature: they are called by name).
fn request(path: &str) -> bool {
    let mut loader = ResourceLoader::singleton();
    loader.exists(path)
        && loader
            .call("load_threaded_request", &[path.to_variant()])
            .try_to::<i64>()
            .is_ok_and(|e| e == 0)
}

/// ResourceLoader.ThreadLoadStatus: still loading, and loaded.
const LOADING: i64 = 1;
const LOADED: i64 = 3;

/// Where a rehearsal is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// What the stages draw being loaded in the background.
    Fetch,
    /// What was loaded drawn once, a texture a frame: a texture loaded in
    /// the background reaches the graphics card when first drawn, and a
    /// level drawing them all at once waits for all of them.
    Upload,
    /// What the stage's level takes from the map's pools, made ahead.
    Pools,
    /// The stage's level drawn over frames.
    Build,
    /// Effects going off over it.
    Hold,
}

pub struct Rehearsal {
    world: World,
    catalog: Catalog,
    phase: Phase,
    stage: usize,
    frame: u32,
    /// Loads still running, scenes waiting their turn, and what they
    /// gave (kept until it is over).
    fetching: Vec<String>,
    queued: Vec<String>,
    fetched: Vec<Gd<Resource>>,
}

impl Rehearsal {
    /// A rehearsal of this game's catalog; `fetch` is what its stages
    /// draw, loaded now in the background.
    pub fn new(catalog: &Catalog, fetch: Vec<String>) -> Rehearsal {
        let mut world = World::new();
        world.set_catalog(catalog);
        // the textures all at once; the scenes one after another (a
        // scene's meshes compile their pipelines on the frame it arrives)
        let (mut scenes, textures): (Vec<String>, Vec<String>) =
            fetch.into_iter().partition(|p| is_scene(p));
        scenes.reverse();
        let fetching = textures.into_iter().filter(|p| request(p)).collect();
        let mut r = Rehearsal {
            world,
            catalog: catalog.clone(),
            phase: Phase::Fetch,
            stage: 0,
            frame: 0,
            fetching,
            queued: scenes,
            fetched: Vec::new(),
        };
        r.lay_level();
        r
    }

    /// Take what has loaded, and send for the next scene; true when
    /// nothing is loading any more (every frame, also while the art
    /// loads ahead).
    pub fn fetch(&mut self) -> bool {
        if !self.fetching.iter().any(|p| is_scene(p)) {
            while let Some(p) = self.queued.pop() {
                if request(&p) {
                    self.fetching.push(p);
                    break;
                }
            }
        }
        if self.fetching.is_empty() {
            return true;
        }
        let mut loader = ResourceLoader::singleton();
        let mut done = Vec::new();
        self.fetching.retain(|p| {
            let status = loader.call("load_threaded_get_status", &[p.to_variant()]);
            match status.try_to::<i64>() {
                Ok(LOADING) => true,
                Ok(LOADED) => {
                    done.push(p.clone());
                    false
                }
                // failed: drawn without it, loaded when it is needed
                _ => false,
            }
        });
        for p in done {
            let r = loader.call("load_threaded_get", &[p.to_variant()]);
            if let Ok(r) = r.try_to::<Gd<Resource>>() {
                self.fetched.push(r);
            }
        }
        self.fetching.is_empty() && self.queued.is_empty()
    }

    /// The map it plays (its monsters and objects are built ahead).
    pub fn map(&self) -> &nh_world::MapState {
        &self.world.map
    }

    /// Everything a game draws for the first time has been drawn: the
    /// first stage built, its effects gone off (its pipelines compiled,
    /// its buffers made).
    pub fn first_draws_done(&self) -> bool {
        self.stage > 0
    }

    /// What it is doing (RENETHACK_FRAME_STATS).
    pub fn doing(&self) -> String {
        format!(
            "{:?} of stage {} frame {}",
            self.phase, self.stage, self.frame
        )
    }

    /// One frame of it; false when it is over.
    pub fn step(&mut self, map: &mut MapView, delta: f64) -> bool {
        match self.phase {
            Phase::Fetch => {
                if self.fetch() {
                    self.phase = Phase::Upload;
                }
            }
            Phase::Upload => {
                let next = self.fetched.get(self.frame as usize).cloned();
                let texture = next
                    .as_ref()
                    .and_then(|r| r.clone().try_cast::<Texture2D>().ok());
                map.show_texture(texture.as_ref());
                self.frame += 1;
                if next.is_none() {
                    self.frame = 0;
                    self.phase = Phase::Pools;
                }
            }
            Phase::Pools => {
                // one piece a frame; the stage once there is all it takes
                if !map.warm_pools(STAGES[self.stage].1) {
                    self.enter(self.stage);
                    self.phase = Phase::Build;
                }
            }
            Phase::Build => {
                // the first level drawn draws everything for the first
                // time: a few of its cells a frame
                map.set_build_budget((self.stage == 0).then_some(FIRST_BUDGET));
                map.sync(&mut self.world, &self.catalog, delta);
                self.frame += 1;
                if map.is_settled() || self.frame >= MOST {
                    self.phase = Phase::Hold;
                    self.frame = 0;
                }
            }
            Phase::Hold => {
                self.act(map, self.frame);
                map.sync(&mut self.world, &self.catalog, delta);
                self.frame += 1;
                if self.frame >= HOLD {
                    map.set_hover(None);
                    map.set_path(&[], false);
                    self.stage += 1;
                    if self.stage == STAGES.len() {
                        return false;
                    }
                    self.phase = Phase::Pools;
                }
            }
        }
        true
    }

    fn enter(&mut self, stage: usize) {
        self.frame = 0;
        self.world.level = Some(LevelNotice {
            dungeon: STAGES[stage].0.to_string(),
            depth: 1,
            plane: None,
        });
        debug_assert_eq!(self.world.branch(), STAGES[stage].1);
    }

    /// The `n`th effect of a stage: a burst of every kind, beams, a ray,
    /// a blow, outlines (a hostile, a pet, an empty cell) and a path.
    fn act(&self, map: &mut MapView, n: u32) {
        let red = Color::from_rgb(1.0, 0.3, 0.2);
        let kinds = [
            VfxKind::Quaff(Color::from_rgb(0.4, 0.6, 1.0)),
            VfxKind::Read,
            VfxKind::Zap(red),
            VfxKind::Cast(Color::from_rgb(0.6, 0.4, 1.0)),
            VfxKind::Fire,
            VfxKind::Frost,
            VfxKind::Lightning,
            VfxKind::Poison,
            VfxKind::Explosion(red),
            VfxKind::Heal,
            VfxKind::Sparkle,
            VfxKind::Sparks,
            VfxKind::Blood(red),
        ];
        let at = |(x, y): (i32, i32)| Vector3::new(x as f32, 0.6, y as f32);
        let path: Vec<_> = (0..5).map(|i| (HERO.0 - i, HERO.1 - 1)).collect();
        let n = n as usize;
        match n.checked_sub(kinds.len()) {
            None => map.burst_at(kinds[n], (X0 + 2 + n as i32, 9)),
            Some(0) => map.beam_between(VfxKind::Lightning, (X0 + 2, 12), (X1 - 2, 12)),
            Some(1) => map.beam_between(VfxKind::Fire, HERO, (HERO.0, Y0 + 2)),
            Some(2) => map.vfx().ray(red, at(HERO), at((X1 - 3, 6))),
            Some(3) => map.vfx().hit(at((HERO.0 + 1, HERO.1)), at(HERO), 0.3),
            Some(4) => map.set_hover(Some((X0 + 9, Y0 + 2))),
            Some(5) => {
                map.set_hover(Some((X0 + 1, Y0 + 1)));
                map.set_path(&path, true);
            }
            Some(6) => {
                map.set_hover(Some((X0 + 4, Y0 + 6)));
                map.set_path(&path, false);
            }
            _ => {}
        }
    }

    fn lay_level(&mut self) {
        let cat = &self.catalog;
        let cmap = |sym: &str| -> Option<Glyph> {
            let info = cat.cmap.iter().find(|c| c.sym == sym)?;
            Some(Glyph {
                cmap: Some(info.idx),
                color: info.color,
                ..glyph(GlyphKind::Cmap, info.ch)
            })
        };
        let map = &mut self.world.map;
        let mut put = |x: i32, y: i32, sym: &str| {
            if let Some(g) = cmap(sym) {
                map.print(x, y, &g, None);
            }
        };
        // the room, its walls, doors and corridors
        for y in Y0..=Y1 {
            for x in X0..=X1 {
                let sym = match (x, y) {
                    (X0, Y0) => "S_tlcorn",
                    (X1, Y0) => "S_trcorn",
                    (X0, Y1) => "S_blcorn",
                    (X1, Y1) => "S_brcorn",
                    (_, Y0 | Y1) => "S_hwall",
                    (X0 | X1, _) => "S_vwall",
                    _ if x >= X1 - 3 && y <= Y0 + 3 => "S_darkroom",
                    _ => "S_room",
                };
                put(x, y, sym);
            }
        }
        put(X0 + 5, Y0, "S_hodoor");
        put(X0 + 10, Y0, "S_hcdoor");
        put(X0 + 15, Y0, "S_ndoor");
        put(X0, Y0 + 6, "S_vcdoor");
        put(X1, Y0 + 6, "S_vodoor");
        put(X0 + 15, Y1, "S_bars");
        put(X0 + 7, Y1, "S_tdwall");
        for y in 0..Y0 {
            put(X0 + 10, y, "S_corr");
        }
        for x in X1 + 1..X1 + 10 {
            put(x, Y0 + 6, "S_litcorr");
        }
        for x in X0 - 8..X0 {
            put(x, Y0 + 6, "S_corr");
        }
        // the features, liquids and traps
        let features = [
            "S_fountain",
            "S_altar",
            "S_sink",
            "S_throne",
            "S_grave",
            "S_tree",
            "S_upstair",
            "S_dnstair",
            "S_upladder",
            "S_dnladder",
            "S_brupstair",
            "S_brdnstair",
            "S_engroom",
            "S_cloud",
            "S_air",
        ];
        for (i, sym) in features.iter().enumerate() {
            put(X0 + 2 + i as i32, Y0 + 3, sym);
        }
        for y in Y0 + 8..=Y0 + 9 {
            for x in 0..3 {
                put(X0 + 2 + x, y, "S_pool");
                put(X0 + 6 + x, y, "S_lava");
                put(X0 + 10 + x, y, "S_ice");
                put(X0 + 14 + x, y, "S_water");
            }
        }
        put(X0 + 17, Y0 + 9, "S_lavawall");
        let traps = [
            "S_arrow_trap",
            "S_bear_trap",
            "S_land_mine",
            "S_rolling_boulder_trap",
            "S_sleeping_gas_trap",
            "S_fire_trap",
            "S_pit",
            "S_spiked_pit",
            "S_hole",
            "S_trap_door",
            "S_teleportation_trap",
            "S_magic_portal",
            "S_web",
            "S_magic_trap",
            "S_polymorph_trap",
            "S_vibrating_square",
        ];
        for (i, sym) in traps.iter().enumerate() {
            put(X0 + 2 + i as i32, Y0 + 7, sym);
        }
        // the heroes of every role and the monsters of the first levels,
        // the objects of every class, a beam and a blast over the floor
        let floor = cmap("S_room");
        let mut put_on_floor = |x: i32, y: i32, g: Glyph| {
            map.print(x, y, &g, floor.as_ref());
        };
        let monsters = ROLES
            .iter()
            .map(|r| role_monster(r))
            .chain(WARM_MONSTERS.iter().copied())
            .take(2 * (X1 - X0 - 1) as usize);
        for (i, name) in monsters.enumerate() {
            let Some(m) = cat.monsters.iter().find(|m| m.name == name) else {
                continue;
            };
            let (x, y) = (
                X0 + 1 + (i as i32 % (X1 - X0 - 1)),
                Y0 + 1 + i as i32 / (X1 - X0 - 1),
            );
            let pet = if i == 0 { mg::PET } else { 0 };
            put_on_floor(
                x,
                y,
                Glyph {
                    mon: Some(m.idx),
                    color: m.color,
                    flags: pet | if i % 2 == 0 { mg::FEMALE } else { mg::MALE },
                    ..glyph(GlyphKind::Mon, 'x' as i32)
                },
            );
        }
        let mut classes: Vec<&str> = Vec::new();
        for t in &cat.object_tiles {
            if !classes.contains(&t.class.as_str()) {
                classes.push(&t.class);
            }
        }
        for (i, class) in classes.iter().enumerate() {
            let Some(t) = cat.object_tiles.iter().rev().find(|t| t.class == *class) else {
                continue;
            };
            let ch = class.chars().next().map_or('x' as i32, |c| c as i32);
            put_on_floor(
                X0 + 2 + i as i32,
                Y0 + 5,
                Glyph {
                    tile: t.tile,
                    color: t.color.unwrap_or(7),
                    ..glyph(GlyphKind::Obj, ch)
                },
            );
        }
        for (i, sym) in ["S_vbeam", "S_expl_mc", "S_digbeam", "S_poisoncloud"]
            .iter()
            .enumerate()
        {
            if let Some(g) = cmap(sym) {
                put_on_floor(X0 + 3 + 2 * i as i32, Y1 - 2, g);
            }
        }
        // the hero by the south wall: seen through it
        if let Some(m) = cat
            .monsters
            .iter()
            .find(|m| m.name == role_monster(ROLES[0]))
        {
            put_on_floor(
                HERO.0,
                HERO.1,
                Glyph {
                    mon: Some(m.idx),
                    color: m.color,
                    flags: mg::HERO,
                    ..glyph(GlyphKind::Mon, '@' as i32)
                },
            );
        }
    }
}

fn glyph(kind: GlyphKind, ch: i32) -> Glyph {
    Glyph {
        glyph: None,
        ch,
        color: 7,
        flags: 0,
        tile: 0,
        kind,
        mon: None,
        cmap: None,
        level: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{EngineMsg, parse_line};

    fn catalog() -> Catalog {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_rehearsal_has_the_hero_every_liquid_and_monsters_and_objects() {
        let cat = catalog();
        let r = Rehearsal::new(&cat, Vec::new());
        assert_eq!(r.world.hero(), Some(HERO));
        assert_eq!(r.world.branch(), Branch::Main);
        let sym = |x, y| {
            let c = r.world.map.cell(x, y)?.terrain.as_ref()?.cmap?;
            cat.cmap.iter().find(|i| i.idx == c).map(|i| i.sym.as_str())
        };
        let syms: Vec<_> = (Y0..=Y1)
            .flat_map(|y| (X0..=X1).map(move |x| (x, y)))
            .filter_map(|(x, y)| sym(x, y))
            .collect();
        for s in [
            "S_pool",
            "S_lava",
            "S_ice",
            "S_water",
            "S_fountain",
            "S_hcdoor",
            "S_web",
        ] {
            assert!(syms.contains(&s), "no {s}");
        }
        let kinds = |k: GlyphKind| {
            (Y0..=Y1)
                .flat_map(|y| (X0..=X1).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    r.world
                        .map
                        .cell(x, y)
                        .and_then(|c| c.glyph.as_ref())
                        .is_some_and(|g| g.kind == k)
                })
                .count()
        };
        assert!(kinds(GlyphKind::Mon) > 30);
        assert!(kinds(GlyphKind::Obj) > 10);
    }

    #[test]
    fn every_stage_is_its_branch() {
        for (dungeon, branch) in STAGES {
            let notice = LevelNotice {
                dungeon: dungeon.to_string(),
                depth: 1,
                plane: None,
            };
            assert_eq!(Branch::of(&notice), *branch);
        }
    }
}
