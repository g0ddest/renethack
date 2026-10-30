//! How each branch of the dungeon looks (spec part 2, decision 1): the
//! materials of its walls, floors and rock, whether its walls are masonry
//! or the rock itself, its props, its light and its grade. Keyed by the
//! branch alone (and in the endgame by the plane), never by a special
//! level's name: Gehennom's levels vary by a hash of the dungeon and the
//! depth, a lair looks its own once the overview names it, and a level
//! turns to a swamp once the hero has seen its water.

use godot::prelude::*;
use nh_protocol::LevelNotice;
use nh_world::{Branch, Plane};

/// The colour grade of a branch: the environment's adjustments and a
/// per-channel curve from its shadows to its highlights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grade {
    pub saturation: f32,
    pub contrast: f32,
    pub brightness: f32,
    /// Where the curve of each channel sits at a quarter and three
    /// quarters of the way (1, 1, 1: no change).
    pub shadows: Color,
    pub highlights: Color,
}

/// A model of a branch's own, placed by the map (never an object's look:
/// doors, gates, lamps on walls, candles on wall tops).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prop {
    /// A great double door of planks and iron (Poly Haven, CC0).
    CastleDoor,
    /// A gate of iron bars (Poly Haven, CC0).
    IronGate,
    /// A lantern hung on a wall instead of a torch (Poly Haven, CC0).
    Lantern,
    /// A candlestick standing on a wall's top (Poly Haven, CC0).
    Candle,
    /// A brass candelabra of three on a wall's top (Poly Haven, CC0: one
    /// of the set's three holders).
    Candelabra,
}

impl Prop {
    pub fn scene(self) -> &'static str {
        match self {
            Prop::CastleDoor => {
                "res://art/cc0/polyhaven/models/large_castle_door/large_castle_door.gltf"
            }
            Prop::IronGate => "res://art/cc0/polyhaven/models/large_iron_gate/large_iron_gate.gltf",
            Prop::Lantern => {
                "res://art/cc0/polyhaven/models/lantern_chandelier_01/lantern_chandelier_01.gltf"
            }
            Prop::Candle => {
                "res://art/cc0/polyhaven/models/wooden_candlestick/wooden_candlestick.gltf"
            }
            Prop::Candelabra => {
                "res://art/cc0/polyhaven/models/brass_candleholders/brass_candleholders.gltf"
            }
        }
    }

    /// The one node of the scene that is the prop (the rest is freed).
    pub fn part(self) -> Option<&'static str> {
        match self {
            Prop::Candelabra => Some("brass_candleholder_02"),
            _ => None,
        }
    }
}

/// What lies under the level's cells and round them: the rock, an open
/// drop to a sea of clouds far below (the Plane of Air: its air cells a
/// faint breeze, its clouds soft masses), or the deep (the Plane of Water:
/// dark, caustic-lit, the bubble of air a membrane in it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Under {
    Rock,
    Sky,
    Deep,
}

/// What a branch dresses its rooms with (map_view's `dressing`): never
/// anything that could pass for an object, a monster or loot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dress {
    /// Half-columns built into the faces of a room's north walls.
    pub pilasters: bool,
    /// Banners hung above head height (their cloths, manifest materials,
    /// one chosen by the cell), and whether they hang torn.
    pub banners: &'static [&'static str],
    pub torn: bool,
    /// Iron chains with shackles from a high ring, on side walls, and the
    /// iron of them and of the banners' rods (manifest name).
    pub chains: bool,
    pub iron: &'static str,
    /// Shallow empty niches in a north wall's face.
    pub alcoves: bool,
    /// Small cobwebs high in a room's far corners.
    pub cobwebs: bool,
    /// Moss at the foot of a room's walls, and cracks in its floor from
    /// them.
    pub moss: bool,
    pub cracks: bool,
    /// Seams of the fire under the floors glowing low in the walls (soot
    /// would not show on black stone).
    pub embers: bool,
    /// Rubble heaped and sunk in a room's corners.
    pub corners: bool,
    /// Rock grown into a cave's walls.
    pub formations: bool,
}

impl Dress {
    pub const NONE: Dress = Dress {
        pilasters: false,
        banners: &[],
        torn: false,
        chains: false,
        iron: "dark_iron",
        alcoves: false,
        cobwebs: false,
        moss: false,
        cracks: false,
        embers: false,
        corners: false,
        formations: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchLook {
    /// What stands in a closed door's frame instead of planks.
    pub door: Option<Prop>,
    /// Lanterns instead of torches on the walls.
    pub lanterns: bool,
    /// Candles stand on some walls' tops.
    pub candles: bool,
    /// Narrow pointed windows in some walls (a tower).
    pub windows: bool,
    /// An iron band round the walls beside a room (built places), by
    /// manifest name.
    pub bands: Option<&'static str>,
    /// Embers rising in the air instead of dust, and a glow from below
    /// round the hero (the fire under Gehennom's floors).
    pub embers: bool,
    pub underglow: Option<Color>,
    /// Manifest materials and what they become in this branch.
    pub remap: &'static [(&'static str, &'static str)],
    /// The walls are the rock itself, broken and irregular (caves).
    pub cave: bool,
    /// Wooden props along the walls: mine supports.
    pub supports: bool,
    /// Stones fallen from the rock lie at the foot of walls and along
    /// corridors (a built place's floors are swept: on Sokoban's planks
    /// every thing on the floor is a thing to reckon with).
    pub rubble: bool,
    /// What lies on a wall's top (its cap), by manifest name.
    pub cap: &'static str,
    pub torch: Color,
    pub torch_energy: f32,
    /// The rooms' faint fill, and how much stronger than the main
    /// dungeon's it is.
    pub fill: Color,
    pub fill_scale: f32,
    pub ambient: Color,
    pub ambient_energy: f32,
    /// The haze: its albedo and its density, and how much it glows by
    /// itself (1: the main dungeon's; it glows the more the denser it is).
    pub fog: Color,
    pub fog_density: f32,
    pub haze: f32,
    /// The darkness at the frame's depth.
    pub darkness: Color,
    /// Floors crack and glow from below (lava under them), in this colour.
    pub cracks: f32,
    pub crack_color: Color,
    /// Water the hero has seen on a level makes it a swamp (`swamp`).
    pub swamps: bool,
    /// How thick the dust in the air is (1: the main dungeon's).
    pub dust: f32,
    pub grade: Grade,
    /// What its rooms are dressed with.
    pub dress: Dress,
    pub under: Under,
}

const NEUTRAL: Grade = Grade {
    saturation: 0.92,
    contrast: 1.05,
    brightness: 1.0,
    shadows: Color::from_rgb(1.0, 1.0, 1.0),
    highlights: Color::from_rgb(1.0, 1.0, 1.0),
};

const MAIN: BranchLook = BranchLook {
    door: None,
    lanterns: false,
    candles: false,
    windows: false,
    bands: None,
    embers: false,
    underglow: None,
    remap: &[],
    cave: false,
    supports: false,
    rubble: true,
    cap: "bedrock",
    torch: Color::from_rgb(1.0, 0.66, 0.38),
    torch_energy: 2.4,
    fill: Color::from_rgb(0.86, 0.80, 0.72),
    fill_scale: 1.0,
    ambient: Color::from_rgb(0.28, 0.32, 0.46),
    ambient_energy: 0.12,
    fog: Color::from_rgb(0.60, 0.60, 0.66),
    fog_density: 0.003,
    haze: 1.0,
    darkness: Color::from_rgb(0.008, 0.009, 0.013),
    cracks: 0.0,
    crack_color: FIRE,
    swamps: false,
    dust: 1.0,
    grade: NEUTRAL,
    // a crypt, a barracks, a storeroom: worn, and some of it still hung
    dress: Dress {
        pilasters: true,
        banners: &["banner_red", "banner_ochre"],
        torn: false,
        chains: true,
        iron: "dark_iron",
        alcoves: true,
        cobwebs: true,
        moss: true,
        cracks: true,
        embers: false,
        corners: true,
        formations: false,
    },
    under: Under::Rock,
};

/// The look of a branch.
pub fn look_of(branch: Branch) -> BranchLook {
    let rgb = Color::from_rgb;
    match branch {
        Branch::Main => MAIN,
        // dug out of the rock: raw cave walls held up by timber, warm
        // lanterns, dust hanging in the air
        Branch::Mines => BranchLook {
            remap: &[
                ("masonry", "cave"),
                ("bedrock", "cave_rock"),
                ("floor", "earth"),
                ("dirt", "earth"),
            ],
            cave: true,
            supports: true,
            torch: rgb(1.0, 0.6, 0.3),
            torch_energy: 2.6,
            fill: rgb(0.9, 0.74, 0.56),
            // (the dark below Minetown: the rock a few cells off still
            // shows its shape, not brown murk)
            ambient: rgb(0.42, 0.36, 0.31),
            fog: rgb(0.7, 0.6, 0.5),
            fog_density: 0.0045,
            // thick air round the torches; in the dark it must not lie
            // over everything as brown murk
            haze: 0.3,
            darkness: rgb(0.012, 0.009, 0.007),
            dust: 2.5,
            dress: Dress {
                formations: true,
                ..Dress::NONE
            },
            grade: Grade {
                saturation: 0.95,
                shadows: rgb(1.02, 0.98, 0.94),
                highlights: rgb(1.04, 0.98, 0.9),
                ..NEUTRAL
            },
            ..MAIN
        },
        // built: brick, planks and iron, an even cold light
        Branch::Sokoban => BranchLook {
            remap: &[("masonry", "soko_wall"), ("floor", "planks")],
            // (every thing on its floor is a thing to reckon with)
            dress: Dress::NONE,
            rubble: false,
            cap: "iron",
            bands: Some("iron"),
            torch: rgb(0.86, 0.9, 1.0),
            torch_energy: 1.8,
            fill: rgb(0.8, 0.86, 0.96),
            fill_scale: 3.0,
            ambient: rgb(0.32, 0.38, 0.52),
            ambient_energy: 0.2,
            grade: Grade {
                saturation: 0.8,
                contrast: 1.0,
                shadows: rgb(0.96, 0.99, 1.06),
                highlights: rgb(0.98, 1.0, 1.04),
                ..NEUTRAL
            },
            ..MAIN
        },
        // basalt over fire: floors cracked and glowing, red light, a
        // red grade; walls of a paler, warmer stone than the floors
        // (black on black, its mazes did not read)
        Branch::Gehennom => BranchLook {
            remap: FIRES[0].remap,
            torch: rgb(1.0, 0.42, 0.18),
            torch_energy: 2.8,
            fill: rgb(1.0, 0.45, 0.25),
            fill_scale: 1.5,
            // the dark is dark: red in the haze over the fire, not a red
            // light over everything
            ambient: rgb(0.4, 0.26, 0.22),
            ambient_energy: 0.12,
            door: Some(Prop::IronGate),
            fog: rgb(0.62, 0.4, 0.32),
            fog_density: 0.004,
            darkness: rgb(0.011, 0.007, 0.006),
            cracks: 1.0,
            swamps: true,
            embers: true,
            underglow: Some(rgb(1.0, 0.36, 0.12)),
            dust: 1.5,
            dress: Dress {
                // in colours and iron the fire lights: black on its black
                // stone showed nothing
                banners: &["banner_crimson"],
                torn: true,
                chains: true,
                iron: "iron",
                cracks: true,
                embers: true,
                corners: true,
                ..Dress::NONE
            },
            grade: Grade {
                saturation: 1.0,
                contrast: 1.1,
                shadows: rgb(1.08, 0.94, 0.92),
                highlights: rgb(1.08, 0.92, 0.84),
                ..NEUTRAL
            },
            ..MAIN
        },
        // the quest home and its levels: an older, mossier stone in a
        // greener, quieter light
        Branch::Quest => BranchLook {
            remap: &[("masonry", "quest_wall"), ("floor", "quest_floor")],
            dress: Dress::NONE,
            torch: rgb(1.0, 0.76, 0.5),
            ambient: rgb(0.3, 0.38, 0.36),
            grade: Grade {
                saturation: 0.82,
                shadows: rgb(0.97, 1.02, 1.0),
                highlights: rgb(1.0, 1.02, 0.96),
                ..NEUTRAL
            },
            ..MAIN
        },
        // a treasury: pale walls, marble, gilded caps
        Branch::Ludios => BranchLook {
            remap: &[("masonry", "ludios_wall"), ("floor", "marble_floor")],
            rubble: false,
            cap: "gilded",
            door: Some(Prop::CastleDoor),
            lanterns: true,
            dress: Dress {
                pilasters: true,
                banners: &["banner_blue"],
                ..Dress::NONE
            },
            torch: rgb(1.0, 0.78, 0.5),
            fill_scale: 2.0,
            grade: Grade {
                saturation: 1.0,
                highlights: rgb(1.05, 1.0, 0.9),
                ..NEUTRAL
            },
            ..MAIN
        },
        // a gothic tower: dark stone, cold violet light
        Branch::Vlad => BranchLook {
            remap: &[("masonry", "gothic"), ("floor", "gothic_floor")],
            rubble: false,
            door: Some(Prop::IronGate),
            lanterns: true,
            candles: true,
            windows: true,
            bands: Some("dark_iron"),
            dress: Dress {
                pilasters: true,
                banners: &["banner_purple"],
                ..Dress::NONE
            },
            torch: rgb(0.9, 0.6, 0.5),
            torch_energy: 2.0,
            // a cold night in the tower: blue in the shadows and the haze
            ambient: rgb(0.24, 0.27, 0.5),
            fog: rgb(0.42, 0.46, 0.64),
            fog_density: 0.005,
            darkness: rgb(0.006, 0.007, 0.015),
            grade: Grade {
                saturation: 0.75,
                contrast: 1.12,
                shadows: rgb(0.9, 0.95, 1.14),
                highlights: rgb(0.95, 0.97, 1.08),
                ..NEUTRAL
            },
            ..MAIN
        },
        Branch::Planes(plane) => plane_look(plane),
    }
}

fn plane_look(plane: Plane) -> BranchLook {
    let rgb = Color::from_rgb;
    match plane {
        Plane::Earth => BranchLook {
            remap: &[
                ("masonry", "cave"),
                ("bedrock", "cave_rock"),
                ("floor", "earth"),
                ("dirt", "earth"),
            ],
            cave: true,
            ambient: rgb(0.4, 0.32, 0.24),
            ambient_energy: 0.2,
            fog: rgb(0.55, 0.45, 0.32),
            fog_density: 0.01,
            dust: 3.0,
            grade: Grade {
                shadows: rgb(1.04, 0.98, 0.9),
                ..NEUTRAL
            },
            dress: Dress::NONE,
            ..MAIN
        },
        // high in an open sky: clear air, the haze thin, a sea of clouds
        // far below
        Plane::Air => BranchLook {
            ambient: rgb(0.6, 0.72, 0.95),
            ambient_energy: 0.6,
            fog: rgb(0.85, 0.9, 1.0),
            fog_density: 0.006,
            darkness: rgb(0.55, 0.68, 0.88),
            dust: 0.0,
            under: Under::Sky,
            grade: Grade {
                saturation: 0.85,
                brightness: 1.1,
                shadows: rgb(0.96, 1.0, 1.1),
                highlights: rgb(1.0, 1.02, 1.06),
                ..NEUTRAL
            },
            dress: Dress::NONE,
            ..MAIN
        },
        Plane::Fire => BranchLook {
            remap: &[
                ("masonry", "basalt"),
                ("bedrock", "basalt_rock"),
                ("floor", "basalt_floor"),
            ],
            ambient: rgb(0.7, 0.25, 0.1),
            ambient_energy: 0.3,
            fog: rgb(1.0, 0.45, 0.2),
            fog_density: 0.012,
            darkness: rgb(0.08, 0.015, 0.005),
            cracks: 1.4,
            embers: true,
            grade: Grade {
                contrast: 1.12,
                shadows: rgb(1.1, 0.92, 0.86),
                highlights: rgb(1.08, 0.94, 0.82),
                ..NEUTRAL
            },
            dress: Dress::NONE,
            ..MAIN
        },
        // a bubble of air drifting in the deep: motes in the water about
        // it, the dark closing in
        Plane::Water => BranchLook {
            ambient: rgb(0.2, 0.36, 0.6),
            ambient_energy: 0.35,
            fog: rgb(0.3, 0.5, 0.75),
            fog_density: 0.02,
            darkness: rgb(0.01, 0.04, 0.08),
            dust: 2.0,
            under: Under::Deep,
            grade: Grade {
                saturation: 0.9,
                shadows: rgb(0.9, 1.0, 1.12),
                highlights: rgb(0.94, 1.02, 1.08),
                ..NEUTRAL
            },
            dress: Dress::NONE,
            ..MAIN
        },
        Plane::Astral => BranchLook {
            remap: &[("masonry", "ludios_wall"), ("floor", "astral_floor")],
            cap: "gilded",
            ambient: rgb(0.6, 0.58, 0.72),
            ambient_energy: 0.3,
            fill_scale: 2.0,
            fog: rgb(0.9, 0.86, 1.0),
            fog_density: 0.008,
            grade: Grade {
                saturation: 0.85,
                brightness: 1.05,
                highlights: rgb(1.04, 1.02, 0.96),
                ..NEUTRAL
            },
            dress: Dress::NONE,
            ..MAIN
        },
    }
}

/// The fire under Gehennom's floors, by default.
const FIRE: Color = Color::from_rgb(1.0, 0.28, 0.05);

/// One of the fires Gehennom's levels burn with (`look_of_level`): what
/// shows through the floors' cracks, the haze over it, and the stone.
struct Fire {
    crack: Color,
    fog: Color,
    fog_density: f32,
    remap: &'static [(&'static str, &'static str)],
}

const FIRES: [Fire; 3] = [
    // embers through black basalt
    Fire {
        crack: FIRE,
        fog: Color::from_rgb(0.62, 0.4, 0.32),
        fog_density: 0.004,
        remap: &[
            ("masonry", "ash_wall"),
            ("bedrock", "basalt_rock"),
            ("floor", "basalt_floor"),
            ("dirt", "ash"),
        ],
    },
    // blood-red under glassy obsidian, a darker haze
    Fire {
        crack: Color::from_rgb(0.95, 0.1, 0.05),
        fog: Color::from_rgb(0.5, 0.22, 0.2),
        fog_density: 0.005,
        remap: &[
            ("masonry", "ash_wall"),
            ("bedrock", "basalt_rock"),
            ("floor", "obsidian_floor"),
            ("dirt", "ash"),
        ],
    },
    // brimstone yellow through grey ash, a smoky haze
    Fire {
        crack: Color::from_rgb(1.0, 0.68, 0.16),
        fog: Color::from_rgb(0.58, 0.52, 0.42),
        fog_density: 0.006,
        remap: &[
            ("masonry", "ash_wall"),
            ("bedrock", "basalt_rock"),
            ("floor", "ash"),
            ("dirt", "ash"),
        ],
    },
];

/// Which of `FIRES` a level burns with: a hash of its dungeon's name and
/// its depth, all the hero knows of where they are (never which level it
/// is): the same every visit.
fn fire_of(dungeon: &str, depth: i32) -> usize {
    let mut h: u32 = 0x811c_9dc5;
    for b in dungeon.bytes().chain(depth.to_le_bytes()) {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    (h % FIRES.len() as u32) as usize
}

/// The look of the level the notice tells of: its branch's, a Gehennom
/// level's in its own fire (`fire_of`), a lair's once the overview names
/// it ("valley", "sanctum"). A lair changes only the light and the air:
/// the level's cells stay as they are drawn.
pub fn look_of_level(branch: Branch, level: Option<&LevelNotice>) -> BranchLook {
    let look = look_of(branch);
    let Some(level) = level.filter(|_| branch == Branch::Gehennom) else {
        return look;
    };
    let fire = &FIRES[fire_of(&level.dungeon, level.depth)];
    let look = BranchLook {
        crack_color: fire.crack,
        fog: fire.fog,
        fog_density: fire.fog_density,
        remap: fire.remap,
        ..look
    };
    let rgb = Color::from_rgb;
    match level.lair.as_deref() {
        // the Valley of the Dead: a graveyard's grey mist over a fire
        // banked low
        Some("valley") => BranchLook {
            fog: rgb(0.44, 0.46, 0.42),
            fog_density: 0.008,
            haze: 1.3,
            cracks: 0.5,
            ambient: rgb(0.32, 0.34, 0.36),
            grade: Grade {
                saturation: 0.85,
                shadows: rgb(0.97, 1.0, 1.04),
                highlights: rgb(1.02, 1.0, 0.98),
                ..look.grade
            },
            ..look
        },
        // Moloch's Sanctum: a crimson haze, its fire high
        Some("sanctum") => BranchLook {
            fog: rgb(0.5, 0.16, 0.26),
            fog_density: 0.006,
            cracks: 1.2,
            crack_color: rgb(1.0, 0.12, 0.18),
            grade: Grade {
                saturation: 1.1,
                ..look.grade
            },
            ..look
        },
        _ => look,
    }
}

/// The look a level takes once the hero has seen a swamp on it (Juiblex's,
/// `swamps`): a murky green haze, no fire showing through the floors, no
/// embers in the air nor glow from below. The light and the air alone:
/// the cells stay as they are drawn.
pub fn swamp(look: BranchLook) -> BranchLook {
    let rgb = Color::from_rgb;
    BranchLook {
        fog: rgb(0.36, 0.42, 0.3),
        fog_density: 0.009,
        haze: 1.2,
        cracks: 0.0,
        embers: false,
        underglow: None,
        dust: 1.0,
        ambient: rgb(0.3, 0.36, 0.28),
        grade: Grade {
            saturation: 0.85,
            shadows: rgb(0.96, 1.02, 0.96),
            highlights: rgb(1.0, 1.02, 0.96),
            ..look.grade
        },
        ..look
    }
}

impl BranchLook {
    /// What of the look the level's cells are drawn by: two looks the
    /// same in this draw the same cells (only the light and the air
    /// differ).
    pub fn cells(&self) -> impl PartialEq + use<> {
        (
            self.remap,
            self.cave,
            self.dress,
            self.under,
            self.rubble,
            self.cap,
            self.supports,
            self.candles,
            self.windows,
            self.door,
            self.bands,
            self.lanterns,
        )
    }

    /// The manifest material a name stands for in this branch.
    pub fn material<'a>(&self, name: &'a str) -> &'a str {
        self.remap
            .iter()
            .find(|(from, _)| *from == name)
            .map_or(name, |&(_, to)| to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_branch_but_the_main_dungeon_looks_its_own() {
        let main = look_of(Branch::Main);
        assert_eq!(main.material("masonry"), "masonry");
        let others = [
            Branch::Mines,
            Branch::Sokoban,
            Branch::Gehennom,
            Branch::Quest,
            Branch::Ludios,
            Branch::Vlad,
            Branch::Planes(Plane::Earth),
            Branch::Planes(Plane::Air),
            Branch::Planes(Plane::Fire),
            Branch::Planes(Plane::Water),
            Branch::Planes(Plane::Astral),
        ];
        for b in others {
            let l = look_of(b);
            assert_ne!(l, main, "{b:?}");
        }
        // the Mines are caves, Gehennom glows, Sokoban is lit evenly
        assert!(look_of(Branch::Mines).cave && look_of(Branch::Mines).supports);
        assert_eq!(look_of(Branch::Mines).material("masonry"), "cave");
        assert!(look_of(Branch::Gehennom).cracks > 0.0);
        assert_eq!(look_of(Branch::Gehennom).material("floor"), "basalt_floor");
        assert_eq!(look_of(Branch::Gehennom).material("masonry"), "ash_wall");
        assert!(look_of(Branch::Sokoban).fill_scale > main.fill_scale);
    }

    fn notice(dungeon: &str, depth: i32, lair: Option<&str>) -> LevelNotice {
        LevelNotice {
            dungeon: dungeon.into(),
            depth,
            plane: None,
            lair: lair.map(Into::into),
        }
    }

    #[test]
    fn gehennoms_levels_burn_each_its_own_fire_by_the_level() {
        let g = Branch::Gehennom;
        let at = |depth| look_of_level(g, Some(&notice("Gehennom", depth, None)));
        // the same every visit, and the fires all met over the branch
        for depth in 26..50 {
            assert_eq!(at(depth), at(depth));
        }
        let fires: std::collections::HashSet<usize> =
            (26..50).map(|d| fire_of("Gehennom", d)).collect();
        assert_eq!(fires.len(), FIRES.len());
        // every one draws the walls paler than the floors, and only
        // Gehennom varies so
        for d in 26..50 {
            assert_eq!(at(d).material("masonry"), "ash_wall");
        }
        let main = notice("The Dungeons of Doom", 5, None);
        assert_eq!(look_of_level(Branch::Main, Some(&main)), MAIN);
    }

    #[test]
    fn a_lair_and_a_swamp_change_the_air_not_the_cells() {
        let g = Branch::Gehennom;
        let plain = look_of_level(g, Some(&notice("Gehennom", 27, None)));
        for lair in ["valley", "sanctum"] {
            let l = look_of_level(g, Some(&notice("Gehennom", 27, Some(lair))));
            assert_ne!(l, plain, "{lair}");
            assert!(l.cells() == plain.cells(), "{lair}");
        }
        let s = swamp(plain);
        assert!(s.cells() == plain.cells());
        assert!(s.cracks == 0.0 && !s.embers && s.underglow.is_none());
        assert!(plain.swamps && !look_of(Branch::Main).swamps);
    }
}
