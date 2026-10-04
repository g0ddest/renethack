//! The scene behind the title menu: a small torchlit chamber in the
//! game's own art, which the map draws as a level of its own. An altar
//! under the torches of the far wall, a fountain, rubble in a corner, and
//! the hero of the role last played (a Valkyrie the first time) standing
//! in their gear, in the dust of the main dungeon. The near side is open
//! to the camera, which sways slowly round the hero (the map's title
//! camera).

use nh_protocol::{Catalog, Glyph, GlyphKind, InvItem, Inventory, LevelNotice, Slot, mg};
use nh_world::World;

use crate::map_view::role_monster;
use crate::rehearsal::glyph;

/// The chamber's walls: the far (north) one at `Y0`, the side ones at
/// `X0` and `X1`; its floor runs on to `Y1`, open to the camera.
/// (The map hangs torches where (x + 2y) % 5 == 0 on a north wall and
/// (y + 2x) % 4 == 0 on the others: three on the far wall, one over the
/// altar, and two on each side wall.)
const X0: i32 = 33;
const X1: i32 = 47;
const Y0: i32 = 5;
const Y1: i32 = 13;
/// Where the hero stands, the altar behind them, the fountain to one side.
pub const HERO: (i32, i32) = (40, 10);
const ALTAR: (i32, i32) = (40, 7);
const FOUNTAIN: (i32, i32) = (36, 8);
/// NetHack's red, the colour of the altar's cloth.
const CLR_RED: i32 = 1;
/// The rubble in the far corners: boulders and stones.
const BOULDERS: &[(i32, i32)] = &[(46, 6), (34, 6)];
const ROCKS: &[(i32, i32)] = &[(45, 6), (46, 7), (35, 6), (34, 7), (45, 10)];

/// The title camera: it aims a little above the floor beside the hero,
/// from this far and this high, and sways this many degrees to either
/// side over a period of this many seconds.
pub const AIM: (f32, f32, f32) = (HERO.0 as f32 - 1.4, 1.0, HERO.1 as f32 - 0.6);
pub const DISTANCE: f32 = 7.2;
pub const PITCH_DEG: f32 = 30.0;
pub const SWAY_DEG: f32 = 18.0;
pub const SWAY_SECS: f32 = 48.0;

/// The scene's own fires, beside what the map draws: an iron brazier on
/// each side of the altar, and a fire pit before the hero, to one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fire {
    Brazier,
    Pit,
}

/// Where they stand (x, z on the floor).
pub const FIRES: &[(Fire, (f32, f32))] = &[
    (Fire::Brazier, (ALTAR.0 as f32 - 1.3, ALTAR.1 as f32 + 0.1)),
    (Fire::Brazier, (ALTAR.0 as f32 + 1.3, ALTAR.1 as f32 + 0.1)),
    (Fire::Pit, (HERO.0 as f32 + 1.8, HERO.1 as f32 + 0.5)),
];

/// How a fire looks: its model, how tall that is drawn, how high the
/// fire burns, how big its flames are and where they lick up (x, z), and
/// how wide its bed of glowing embers is (0: none shows).
pub struct FireLook {
    pub scene: &'static str,
    pub height: f32,
    pub burns: f32,
    pub size: f32,
    pub flames: &'static [(f32, f32)],
    pub coals: f32,
}

impl Fire {
    pub fn look(self) -> FireLook {
        match self {
            Fire::Brazier => FireLook {
                scene: "res://art/cc0/sigilsvault/dungeon/props/Brazier.glb",
                height: 1.0,
                burns: 1.0,
                size: 1.0,
                flames: &[(0.0, 0.0)],
                coals: 0.0,
            },
            Fire::Pit => FireLook {
                scene: "res://art/cc0/polyhaven/models/stone_fire_pit/stone_fire_pit.gltf",
                height: 0.3,
                burns: 0.14,
                size: 1.75,
                flames: &[(-0.1, 0.0), (0.1, -0.06), (0.0, 0.1)],
                coals: 0.3,
            },
        }
    }
}

/// What a role's hero holds on the title, as the game names the things
/// unidentified (the first one the catalog has): in hand, on the arm, on
/// the back.
type Held = (
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

fn held(role: &str) -> Held {
    match role {
        "archeologist" => (&["bullwhip"], &[], &["pick-axe"]),
        "barbarian" => (&["two-handed sword"], &[], &["axe"]),
        "caveman" | "cavewoman" => (&["club"], &[], &[]),
        "healer" => (&["scalpel"], &[], &[]),
        "knight" => (
            &["long sword"],
            &["small shield", "wooden shield"],
            &["lance"],
        ),
        "monk" => (&[], &[], &[]),
        "priest" | "priestess" => (&["mace"], &[], &[]),
        "rogue" => (&["short sword"], &[], &["dagger"]),
        "ranger" => (&["dagger"], &[], &["bow"]),
        "samurai" => (&["samurai sword"], &[], &["long bow"]),
        "tourist" => (&["expensive camera"], &[], &[]),
        "wizard" => (&["staff"], &[], &[]),
        // a Valkyrie, and anyone else
        _ => (
            &["long sword"],
            &["small shield", "wooden shield"],
            &["dagger"],
        ),
    }
}

/// The title's chamber, ready for the map to draw.
pub struct TitleScene {
    world: World,
    catalog: Catalog,
}

impl TitleScene {
    /// The chamber with a hero of `role` (its name, "valkyrie", or the
    /// catalog's name or code for it), a woman's look when `female`.
    pub fn new(catalog: &Catalog, role: &str, female: bool) -> TitleScene {
        let role = catalog
            .roles
            .iter()
            .find(|r| {
                r.name.eq_ignore_ascii_case(role)
                    || r.code.eq_ignore_ascii_case(role)
                    || r.name_female
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(role))
            })
            .map_or_else(|| role.to_lowercase(), |r| r.name.to_lowercase());
        let mut world = World::new();
        world.set_catalog(catalog);
        world.level = Some(LevelNotice {
            dungeon: "The Dungeons of Doom".to_string(),
            depth: 1,
            plane: None,
        });
        lay(&mut world, catalog, &role, female);
        world.inventory.replace(&gear(catalog, &role));
        TitleScene {
            world,
            catalog: catalog.clone(),
        }
    }

    /// The world and catalog the map draws it from.
    pub fn parts(&mut self) -> (&mut World, &Catalog) {
        (&mut self.world, &self.catalog)
    }
}

/// The hero's things: what `held` names, wielded, worn and kept.
fn gear(catalog: &Catalog, role: &str) -> Inventory {
    let (hand, arm, back) = held(role);
    let mut items = Vec::new();
    let mut letters = 'a'..='z';
    for (names, slot) in [
        (hand, Slot::Weapon),
        (arm, Slot::Shield),
        (back, Slot::Alternate),
    ] {
        let found = names.iter().find_map(|name| {
            catalog
                .object_tiles
                .iter()
                .find(|t| t.appearance == *name)
                .map(|t| (name, t))
        });
        if let (Some((name, t)), Some(letter)) = (found, letters.next()) {
            items.push(InvItem {
                letter,
                class: t.class.chars().next().unwrap_or(')'),
                tile: t.tile,
                quan: 1,
                slots: vec![slot],
                lit: false,
                text: format!("a {name}"),
            });
        }
    }
    Inventory {
        items,
        twoweap: false,
    }
}

/// The chamber's cells, its features, the rubble and the hero.
fn lay(world: &mut World, cat: &Catalog, role: &str, female: bool) {
    let cmap = |sym: &str| -> Option<Glyph> {
        let info = cat.cmap.iter().find(|c| c.sym == sym)?;
        Some(Glyph {
            cmap: Some(info.idx),
            color: info.color,
            ..glyph(GlyphKind::Cmap, info.ch)
        })
    };
    let floor = cmap("S_room");
    let map = &mut world.map;
    // (a colour of its own: the altar's cloth is the glyph's colour)
    let mut put = |x: i32, y: i32, sym: &str, color: Option<i32>| {
        if let Some(g) = cmap(sym) {
            let g = Glyph {
                color: color.unwrap_or(g.color),
                ..g
            };
            map.print(x, y, &g, None);
        }
    };
    for y in Y0..=Y1 {
        for x in X0..=X1 {
            let sym = match (x, y) {
                (X0, Y0) => "S_tlcorn",
                (X1, Y0) => "S_trcorn",
                (_, Y0) => "S_hwall",
                (X0 | X1, _) => "S_vwall",
                _ => "S_room",
            };
            put(x, y, sym, None);
        }
    }
    put(ALTAR.0, ALTAR.1, "S_altar", Some(CLR_RED));
    put(FOUNTAIN.0, FOUNTAIN.1, "S_fountain", None);
    let mut put_on_floor = |x: i32, y: i32, g: Glyph| {
        map.print(x, y, &g, floor.as_ref());
    };
    let object = |appearance: &str| -> Option<Glyph> {
        let t = cat
            .object_tiles
            .iter()
            .find(|t| t.appearance == appearance)?;
        let ch = t.class.chars().next().map_or('*' as i32, |c| c as i32);
        Some(Glyph {
            tile: t.tile,
            color: t.color.unwrap_or(7),
            ..glyph(GlyphKind::Obj, ch)
        })
    };
    for (cells, what) in [(BOULDERS, "boulder"), (ROCKS, "rock")] {
        if let Some(g) = object(what) {
            for &(x, y) in cells {
                put_on_floor(x, y, g.clone());
            }
        }
    }
    if let Some(m) = cat.monsters.iter().find(|m| m.name == role_monster(role)) {
        let gender = if female { mg::FEMALE } else { mg::MALE };
        put_on_floor(
            HERO.0,
            HERO.1,
            Glyph {
                mon: Some(m.idx),
                color: m.color,
                flags: mg::HERO | gender,
                ..glyph(GlyphKind::Mon, '@' as i32)
            },
        );
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
    fn the_chamber_has_its_hero_altar_fountain_and_rubble() {
        let cat = catalog();
        let mut t = TitleScene::new(&cat, "Val", true);
        let (world, _) = t.parts();
        assert_eq!(world.hero(), Some(HERO));
        let sym = |x, y| {
            let c = world.map.cell(x, y)?.terrain.as_ref()?.cmap?;
            cat.cmap.iter().find(|i| i.idx == c).map(|i| i.sym.clone())
        };
        assert_eq!(sym(ALTAR.0, ALTAR.1).as_deref(), Some("S_altar"));
        assert_eq!(sym(FOUNTAIN.0, FOUNTAIN.1).as_deref(), Some("S_fountain"));
        assert_eq!(sym(X0, Y0).as_deref(), Some("S_tlcorn"));
        // open to the camera: no wall on the near side
        assert_eq!(sym(HERO.0, Y1).as_deref(), Some("S_room"));
        let objects = BOULDERS
            .iter()
            .chain(ROCKS)
            .filter(|&&(x, y)| {
                world
                    .map
                    .cell(x, y)
                    .and_then(|c| c.glyph.as_ref())
                    .is_some_and(|g| g.kind == GlyphKind::Obj)
            })
            .count();
        assert_eq!(objects, BOULDERS.len() + ROCKS.len());
    }

    #[test]
    fn the_hero_holds_the_role_s_weapon() {
        let cat = catalog();
        // by the catalog's code, name or the engine's name
        for role in ["Val", "Valkyrie", "valkyrie"] {
            let mut t = TitleScene::new(&cat, role, true);
            let (world, _) = t.parts();
            let wielded = world.inventory.wielded().map(|i| i.text.as_str());
            assert_eq!(wielded, Some("a long sword"), "{role}");
        }
        let mut t = TitleScene::new(&cat, "Wiz", false);
        let (world, _) = t.parts();
        assert_eq!(
            world.inventory.wielded().map(|i| i.text.as_str()),
            Some("a staff")
        );
        // a monk fights bare-handed
        let mut t = TitleScene::new(&cat, "Monk", false);
        assert!(t.parts().0.inventory.wielded().is_none());
    }
}
