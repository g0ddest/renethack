//! How each branch of the dungeon looks (spec part 2, decision 1): the
//! materials of its walls, floors and rock, whether its walls are masonry
//! or the rock itself, its props, its light and its grade. Keyed by the
//! branch alone (and in the endgame by the plane), never by a special
//! level's name.

use godot::prelude::*;
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchLook {
    /// What stands in a closed door's frame instead of planks.
    pub door: Option<Prop>,
    /// Lanterns instead of torches on the walls.
    pub lanterns: bool,
    /// Candles stand on some walls' tops.
    pub candles: bool,
    /// Manifest materials and what they become in this branch.
    pub remap: &'static [(&'static str, &'static str)],
    /// The walls are the rock itself, broken and irregular (caves).
    pub cave: bool,
    /// Wooden props along the walls: mine supports.
    pub supports: bool,
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
    /// The haze: its albedo and its density.
    pub fog: Color,
    pub fog_density: f32,
    /// The darkness at the frame's depth.
    pub darkness: Color,
    /// Floors crack and glow from below (lava under them).
    pub cracks: f32,
    /// How thick the dust in the air is (1: the main dungeon's).
    pub dust: f32,
    pub grade: Grade,
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
    remap: &[],
    cave: false,
    supports: false,
    cap: "bedrock",
    torch: Color::from_rgb(1.0, 0.66, 0.38),
    torch_energy: 2.4,
    fill: Color::from_rgb(0.86, 0.80, 0.72),
    fill_scale: 1.0,
    ambient: Color::from_rgb(0.28, 0.32, 0.46),
    ambient_energy: 0.12,
    fog: Color::from_rgb(0.60, 0.60, 0.66),
    fog_density: 0.003,
    darkness: Color::from_rgb(0.008, 0.009, 0.013),
    cracks: 0.0,
    dust: 1.0,
    grade: NEUTRAL,
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
            ambient: rgb(0.36, 0.3, 0.26),
            fog: rgb(0.7, 0.6, 0.5),
            fog_density: 0.006,
            darkness: rgb(0.012, 0.009, 0.007),
            dust: 2.5,
            grade: Grade {
                saturation: 0.95,
                shadows: rgb(1.02, 0.98, 0.94),
                highlights: rgb(1.04, 0.98, 0.9),
                ..NEUTRAL
            },
            ..MAIN
        },
        // built: dressed stone, wood and iron, an even cold light
        Branch::Sokoban => BranchLook {
            remap: &[("masonry", "dressed"), ("floor", "planks")],
            cap: "iron",
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
        // red grade
        Branch::Gehennom => BranchLook {
            remap: &[
                ("masonry", "basalt"),
                ("bedrock", "basalt_rock"),
                ("floor", "basalt_floor"),
                ("dirt", "ash"),
            ],
            torch: rgb(1.0, 0.42, 0.18),
            torch_energy: 2.8,
            fill: rgb(1.0, 0.45, 0.25),
            fill_scale: 1.5,
            ambient: rgb(0.5, 0.18, 0.12),
            ambient_energy: 0.16,
            door: Some(Prop::IronGate),
            fog: rgb(0.7, 0.35, 0.25),
            fog_density: 0.005,
            darkness: rgb(0.02, 0.006, 0.004),
            cracks: 1.0,
            dust: 1.5,
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
            cap: "gilded",
            door: Some(Prop::CastleDoor),
            lanterns: true,
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
            door: Some(Prop::IronGate),
            lanterns: true,
            candles: true,
            torch: rgb(0.9, 0.6, 0.5),
            torch_energy: 2.0,
            ambient: rgb(0.3, 0.26, 0.46),
            fog: rgb(0.5, 0.48, 0.62),
            fog_density: 0.005,
            grade: Grade {
                saturation: 0.75,
                contrast: 1.12,
                shadows: rgb(0.98, 0.96, 1.08),
                highlights: rgb(0.98, 0.96, 1.02),
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
            ..MAIN
        },
        Plane::Air => BranchLook {
            ambient: rgb(0.6, 0.72, 0.95),
            ambient_energy: 0.6,
            fog: rgb(0.85, 0.9, 1.0),
            fog_density: 0.02,
            darkness: rgb(0.35, 0.45, 0.62),
            grade: Grade {
                saturation: 0.85,
                brightness: 1.1,
                shadows: rgb(0.96, 1.0, 1.1),
                highlights: rgb(1.0, 1.02, 1.06),
                ..NEUTRAL
            },
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
            grade: Grade {
                contrast: 1.12,
                shadows: rgb(1.1, 0.92, 0.86),
                highlights: rgb(1.08, 0.94, 0.82),
                ..NEUTRAL
            },
            ..MAIN
        },
        Plane::Water => BranchLook {
            ambient: rgb(0.2, 0.36, 0.6),
            ambient_energy: 0.35,
            fog: rgb(0.3, 0.5, 0.75),
            fog_density: 0.02,
            darkness: rgb(0.01, 0.04, 0.08),
            grade: Grade {
                saturation: 0.9,
                shadows: rgb(0.9, 1.0, 1.12),
                highlights: rgb(0.94, 1.02, 1.08),
                ..NEUTRAL
            },
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
            ..MAIN
        },
    }
}

impl BranchLook {
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
        assert!(look_of(Branch::Sokoban).fill_scale > main.fill_scale);
    }
}
