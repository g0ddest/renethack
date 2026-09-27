//! The 3D map from simple geometry: floor tiles, wall blocks and a primitive
//! for every dungeon feature; capsules for monsters with their letter above,
//! small cubes for objects; a perspective camera following the hero. Cell
//! (x, y) is the point (x, 0, y); one cell is one metre.
//!
//! Each cell's look is computed as plain data (`Look`) and its nodes are
//! touched only when the look changes. Meshes and materials are shared
//! through caches keyed by shape and colour.

use std::collections::HashMap;

use godot::classes::base_material_3d::{BillboardMode, Feature, ShadingMode, Transparency};
use godot::classes::environment::{AmbientSource, BgMode};
use godot::classes::label_3d::DrawFlags;
use godot::classes::light_3d::Param;
use godot::classes::{
    BoxMesh, Camera3D, CapsuleMesh, CylinderMesh, DirectionalLight3D, Environment, Label3D, Mesh,
    MeshInstance3D, Node3D, OmniLight3D, PlaneMesh, SphereMesh, StandardMaterial3D, SystemFont,
    TorusMesh, WorldEnvironment,
};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, mg};
use nh_world::{COLNO, Cell, MapState, ROWNO, Terrain, World, cell_terrain, in_field, terrain_of};

use crate::theme::{self, nh_color};

/// Steep enough that a row of depth takes more screen height than a
/// monster and its letter: a neighbour north or south of the hero stays in
/// its own row (see `monster_letters_keep_to_their_rows`).
const PITCH_DEG: f32 = 64.0;
const DISTANCE: f32 = 12.0;
const MIN_DISTANCE: f32 = 6.0;
const MAX_DISTANCE: f32 = 30.0;
/// The camera's vertical field of view, degrees.
const FOV_DEG: f32 = 50.0;
/// The overview never goes further (a whole 80x21 level fits well within).
const MAX_OVERVIEW_DISTANCE: f32 = 70.0;
const FOLLOW_RATE: f32 = 8.0;
/// Descent per step when a pointer ray is walked through the raised geometry.
const PICK_STEP: f32 = 0.05;
/// Nothing drawn reaches higher (a letter over a tall monster on an altar).
const MAX_TOP: f32 = 2.8;

const WALL_HEIGHT: f32 = 1.2;
const DOOR_HEIGHT: f32 = 1.05;
/// Walls and doors in front of open ground, seen from the camera's side.
const CUT_HEIGHT: f32 = 0.3;
/// Label3D font size; a letter's height is about `FONT_PX * pixel size`.
const FONT_PX: i32 = 96;
const PX_MONSTER: f32 = 0.0068;
const PX_OBJECT: f32 = 0.0048;
const PX_FEATURE: f32 = 0.0062;
const PX_TRAP: f32 = 0.0052;
/// A monster's letter floats this far over the top of its body.
const MONSTER_LABEL_LIFT: f32 = 0.16;
/// Where the camera aims, south of the hero at the default distance (the
/// log covers the bottom of the screen); it shrinks as the camera closes in.
const AIM_SOUTH: f32 = 1.5;

const FLOOR: Color = Color::from_rgb(0.30, 0.29, 0.27);
const FLOOR_DARK: Color = Color::from_rgb(0.12, 0.12, 0.14);
const FLOOR_UNSEEN: Color = Color::from_rgb(0.19, 0.19, 0.21);
const CORRIDOR: Color = Color::from_rgb(0.19, 0.17, 0.15);
const CORRIDOR_LIT: Color = Color::from_rgb(0.27, 0.24, 0.20);
const DOORWAY: Color = Color::from_rgb(0.29, 0.22, 0.15);
const WALL: Color = Color::from_rgb(0.40, 0.40, 0.44);
const STONE: Color = Color::from_rgb(0.52, 0.52, 0.55);
const WOOD: Color = Color::from_rgb(0.42, 0.26, 0.11);
const EARTH: Color = Color::from_rgb(0.17, 0.14, 0.10);
const DEEP: Color = Color::from_rgb(0.03, 0.03, 0.05);
const STATUE: Color = Color::from_rgb(0.58, 0.58, 0.60);
/// Scratches of an engraving on the floor.
const ENGRAVING: Color = Color::from_rgb(0.74, 0.71, 0.62);
const HERO_RING: Color = Color::from_rgba(1.0, 0.82, 0.30, 0.9);
const PET_RING: Color = Color::from_rgba(1.0, 0.45, 0.75, 0.9);
const HERO_LIGHT: Color = Color::from_rgb(1.0, 0.86, 0.66);

/// Mesh sizes in centimetres, so meshes can be cached by shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MeshKey {
    /// Width (x), height (y), depth (z).
    Box(u16, u16, u16),
    /// Width (x), depth (z); faces up.
    Plane(u16, u16),
    /// Radius, total height.
    Capsule(u16, u16),
    /// Top radius, bottom radius, height.
    Cylinder(u16, u16, u16),
    Sphere(u16),
    /// Inner and outer radius; lies flat.
    Torus(u16, u16),
}

fn cm(metres: f32) -> u16 {
    (metres * 100.0).round().clamp(0.0, f32::from(u16::MAX)) as u16
}

fn metres(cm: u16) -> f32 {
    f32::from(cm) / 100.0
}

fn cuboid(x: f32, y: f32, z: f32) -> MeshKey {
    MeshKey::Box(cm(x), cm(y), cm(z))
}

fn plane(x: f32, z: f32) -> MeshKey {
    MeshKey::Plane(cm(x), cm(z))
}

fn capsule(radius: f32, height: f32) -> MeshKey {
    MeshKey::Capsule(cm(radius), cm(height.max(2.0 * radius)))
}

fn cylinder(top: f32, bottom: f32, height: f32) -> MeshKey {
    MeshKey::Cylinder(cm(top), cm(bottom), cm(height))
}

fn sphere(radius: f32) -> MeshKey {
    MeshKey::Sphere(cm(radius))
}

fn torus(inner: f32, outer: f32) -> MeshKey {
    MeshKey::Torus(cm(inner), cm(outer))
}

impl MeshKey {
    /// Half the height of the upright mesh.
    fn half_height(self) -> f32 {
        match self {
            MeshKey::Box(_, y, _) => metres(y) / 2.0,
            MeshKey::Plane(..) => 0.0,
            MeshKey::Capsule(_, h) | MeshKey::Cylinder(_, _, h) => metres(h) / 2.0,
            MeshKey::Sphere(r) => metres(r),
            MeshKey::Torus(i, o) => metres(o.saturating_sub(i)) / 2.0,
        }
    }

    fn build(self) -> Gd<Mesh> {
        match self {
            MeshKey::Box(x, y, z) => {
                let mut m = BoxMesh::new_gd();
                m.set_size(Vector3::new(metres(x), metres(y), metres(z)));
                m.upcast()
            }
            MeshKey::Plane(x, z) => {
                let mut m = PlaneMesh::new_gd();
                m.set_size(Vector2::new(metres(x), metres(z)));
                m.upcast()
            }
            MeshKey::Capsule(r, h) => {
                let mut m = CapsuleMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(metres(h));
                m.set_radial_segments(16);
                m.set_rings(4);
                m.upcast()
            }
            MeshKey::Cylinder(t, b, h) => {
                let mut m = CylinderMesh::new_gd();
                m.set_top_radius(metres(t));
                m.set_bottom_radius(metres(b));
                m.set_height(metres(h));
                m.set_radial_segments(16);
                m.set_rings(1);
                m.upcast()
            }
            MeshKey::Sphere(r) => {
                let mut m = SphereMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(2.0 * metres(r));
                m.set_radial_segments(16);
                m.set_rings(8);
                m.upcast()
            }
            MeshKey::Torus(i, o) => {
                let mut m = TorusMesh::new_gd();
                m.set_inner_radius(metres(i));
                m.set_outer_radius(metres(o));
                m.set_rings(24);
                m.set_ring_segments(6);
                m.upcast()
            }
        }
    }
}

/// How a surface is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Finish {
    Matte,
    /// Water, ice, metal.
    Glossy,
    /// Lava: lit from inside.
    Ember,
    /// Beams and explosions: full colour, no shading.
    Glow,
    /// Detected and invisible monsters, clouds, air.
    Ghost,
    /// Rings and markers: unshaded, alpha from the colour.
    Flat,
}

fn build_material(color: Color, finish: Finish) -> Gd<StandardMaterial3D> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_albedo(color);
    m.set_roughness(0.9);
    match finish {
        Finish::Matte => {}
        Finish::Glossy => {
            m.set_roughness(0.25);
            m.set_specular(0.8);
        }
        Finish::Ember => {
            m.set_feature(Feature::EMISSION, true);
            m.set_emission(color);
            m.set_emission_energy_multiplier(1.3);
        }
        Finish::Glow => m.set_shading_mode(ShadingMode::UNSHADED),
        Finish::Ghost => {
            m.set_transparency(Transparency::ALPHA);
            m.set_albedo(color.with_alpha(0.4));
            m.set_roughness(0.5);
        }
        Finish::Flat => {
            m.set_shading_mode(ShadingMode::UNSHADED);
            m.set_transparency(Transparency::ALPHA);
        }
    }
    m
}

/// One mesh of a cell, relative to the cell's centre on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Solid {
    mesh: MeshKey,
    color: Color,
    finish: Finish,
    pos: Vector3,
    /// Euler angles in degrees.
    rot: Vector3,
}

/// A billboard character over a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Letter {
    ch: char,
    color: Color,
    pos: Vector3,
    pixel_size: f32,
    /// Drawn over walls and everything else (monster letters stay readable).
    on_top: bool,
}

/// How a cell looks; its nodes are touched only when this changes.
#[derive(Debug, Clone, Default, PartialEq)]
struct Look {
    solids: Vec<Solid>,
    letters: Vec<Letter>,
    /// Where things stand on this cell and where markers lie.
    ground: f32,
    /// The top of everything drawn (pointer picking).
    top: f32,
}

impl Look {
    fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.letters.is_empty()
    }

    fn solid(&mut self, mesh: MeshKey, color: Color, finish: Finish, pos: Vector3) {
        self.turned(mesh, color, finish, pos, Vector3::ZERO);
    }

    fn turned(&mut self, mesh: MeshKey, color: Color, finish: Finish, pos: Vector3, rot: Vector3) {
        // only capsules are ever laid down (corpses); boxes only turn about y
        let reach = match mesh {
            MeshKey::Capsule(r, _) if rot.x != 0.0 || rot.z != 0.0 => metres(r),
            _ => mesh.half_height(),
        };
        self.top = self.top.max(pos.y + reach).min(MAX_TOP);
        self.solids.push(Solid {
            mesh,
            color,
            finish,
            pos,
            rot,
        });
    }

    fn letter(&mut self, ch: char, color: Color, y: f32, pixel_size: f32, on_top: bool) {
        let reach = FONT_PX as f32 * pixel_size / 2.0;
        self.top = self.top.max(y + reach).min(MAX_TOP);
        self.letters.push(Letter {
            ch,
            color,
            pos: Vector3::new(0.0, y, 0.0),
            pixel_size,
            on_top,
        });
    }
}

fn at(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

/// Linear mix of two colours (pure: works without the engine).
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn darker(c: Color, t: f32) -> Color {
    mix(c, Color::from_rgba(0.0, 0.0, 0.0, c.a), t)
}

fn lighter(c: Color, t: f32) -> Color {
    mix(c, Color::from_rgba(1.0, 1.0, 1.0, c.a), t)
}

/// A base material colour nudged towards a NetHack colour; gray leaves it.
fn tinted(base: Color, nh: i32) -> Color {
    if matches!(nh & 0xff, 7 | 8) {
        base
    } else {
        mix(base, nh_color(nh), 0.35)
    }
}

fn color_key(c: Color) -> u32 {
    c.to_u32(godot::builtin::ColorChannelOrder::RGBA)
}

fn glyph_char(g: &Glyph) -> Option<char> {
    u32::try_from(g.ch)
        .ok()
        .and_then(char::from_u32)
        .filter(|c| !c.is_control() && *c != ' ')
}

fn cmap_sym<'a>(g: &Glyph, catalog: &'a Catalog) -> Option<&'a str> {
    let i = usize::try_from(g.cmap?).ok()?;
    catalog.cmap.get(i).map(|c| c.sym.as_str())
}

/// Monster height by catalog size.
fn monster_height(catalog: &Catalog, mon: Option<i32>) -> f32 {
    let size = mon
        .and_then(|m| usize::try_from(m).ok())
        .and_then(|m| catalog.monsters.get(m))
        .map_or("medium", |m| m.size.as_str());
    match size {
        "tiny" => 0.35,
        "small" => 0.55,
        "large" => 1.05,
        "huge" => 1.3,
        "gigantic" => 1.6,
        _ => 0.8,
    }
}

fn is_boulder(g: &Glyph, catalog: &Catalog) -> bool {
    catalog
        .object_tiles
        .iter()
        .any(|t| t.tile == g.tile && t.appearance == "boulder")
}

/// Floor, walls and features. Sets `ground`. `cut`: open ground lies north
/// of the cell (the row behind a room's south wall, a doorway above a side
/// wall), so a wall or door here would hide it from the camera and is
/// drawn low.
fn terrain_look(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, cut: bool) {
    terrain_base(look, t, sym, g, cut);
    if engraved(sym) {
        for (x, z, yaw) in [(-0.06, -0.12, 18.0), (0.04, 0.02, -24.0), (0.0, 0.16, 8.0)] {
            let mesh = cuboid(0.46, 0.012, 0.035);
            look.turned(
                mesh,
                ENGRAVING,
                Finish::Matte,
                at(x, 0.006, z),
                at(0.0, yaw, 0.0),
            );
        }
    }
}

/// Something is engraved here (in a room or a corridor).
fn engraved(sym: &str) -> bool {
    matches!(sym, "S_engroom" | "S_engrcorr")
}

fn terrain_base(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, cut: bool) {
    let c = g.color;
    // "S_v..." features sit in a vertical wall: the passage runs along x
    let vertical = sym.starts_with("S_v");
    let (wall_h, door_h) = if cut {
        (CUT_HEIGHT, CUT_HEIGHT)
    } else {
        (WALL_HEIGHT, DOOR_HEIGHT)
    };
    let tile = plane(0.96, 0.96);
    let floor = |look: &mut Look| look.solid(tile, FLOOR, Finish::Matte, Vector3::ZERO);
    let label = |look: &mut Look, y: f32| {
        if let Some(ch) = glyph_char(g) {
            look.letter(ch, lighter(nh_color(c), 0.25), y, PX_FEATURE, false);
        }
    };
    match t {
        Terrain::Stone | Terrain::Effect | Terrain::Unknown => {}
        Terrain::Wall => {
            look.solid(
                cuboid(1.0, wall_h, 1.0),
                tinted(WALL, c),
                Finish::Matte,
                at(0.0, wall_h / 2.0, 0.0),
            );
            look.ground = wall_h;
        }
        // an engraving's colour (bright blue) would make it a pool: the
        // floor keeps its own colour and gets scratches (below)
        Terrain::Floor if engraved(sym) => look.solid(tile, FLOOR, Finish::Matte, Vector3::ZERO),
        Terrain::Floor => look.solid(tile, tinted(FLOOR, c), Finish::Matte, Vector3::ZERO),
        Terrain::DarkFloor => look.solid(tile, FLOOR_DARK, Finish::Matte, Vector3::ZERO),
        Terrain::Corridor => {
            let color = if sym == "S_litcorr" {
                CORRIDOR_LIT
            } else {
                CORRIDOR
            };
            look.solid(plane(0.8, 0.8), color, Finish::Matte, Vector3::ZERO);
        }
        Terrain::Doorway => look.solid(tile, DOORWAY, Finish::Matte, Vector3::ZERO),
        Terrain::BrokenDoor => {
            look.solid(tile, DOORWAY, Finish::Matte, Vector3::ZERO);
            let wood = tinted(WOOD, c);
            let y = at(0.0, 30.0, 0.0);
            look.turned(
                cuboid(0.34, 0.05, 0.08),
                wood,
                Finish::Matte,
                at(0.2, 0.025, 0.24),
                y,
            );
            let y = at(0.0, -50.0, 0.0);
            look.turned(
                cuboid(0.26, 0.05, 0.07),
                wood,
                Finish::Matte,
                at(-0.2, 0.025, -0.2),
                y,
            );
        }
        Terrain::OpenDoor => {
            look.solid(tile, DOORWAY, Finish::Matte, Vector3::ZERO);
            // the leaf stands open against the side of the passage
            let (mesh, pos) = if vertical {
                (cuboid(0.8, door_h, 0.08), at(0.0, door_h / 2.0, -0.44))
            } else {
                (cuboid(0.08, door_h, 0.8), at(-0.44, door_h / 2.0, 0.0))
            };
            look.solid(mesh, tinted(WOOD, c), Finish::Matte, pos);
        }
        Terrain::ClosedDoor => {
            look.solid(tile, DOORWAY, Finish::Matte, Vector3::ZERO);
            let mesh = if vertical {
                cuboid(0.18, door_h, 0.96)
            } else {
                cuboid(0.96, door_h, 0.18)
            };
            let pos = at(0.0, door_h / 2.0, 0.0);
            look.solid(mesh, tinted(WOOD, c), Finish::Matte, pos);
            look.ground = door_h;
        }
        Terrain::IronBars => {
            look.solid(tile, FLOOR_DARK, Finish::Matte, Vector3::ZERO);
            let iron = darker(nh_color(c), 0.3);
            let bar = cylinder(0.035, 0.035, WALL_HEIGHT);
            let y = WALL_HEIGHT / 2.0;
            for (x, z) in [(-0.3, 0.0), (0.0, 0.0), (0.3, 0.0), (0.0, -0.3), (0.0, 0.3)] {
                look.solid(bar, iron, Finish::Glossy, at(x, y, z));
            }
            let top = WALL_HEIGHT - 0.05;
            look.solid(
                cuboid(0.9, 0.05, 0.05),
                iron,
                Finish::Glossy,
                at(0.0, top, 0.0),
            );
            look.solid(
                cuboid(0.05, 0.05, 0.9),
                iron,
                Finish::Glossy,
                at(0.0, top, 0.0),
            );
        }
        Terrain::Tree => {
            look.solid(tile, EARTH, Finish::Matte, Vector3::ZERO);
            look.solid(
                cylinder(0.07, 0.1, 0.5),
                WOOD,
                Finish::Matte,
                at(0.0, 0.25, 0.0),
            );
            let leaves = darker(nh_color(c), 0.35);
            look.solid(
                cylinder(0.0, 0.42, 1.0),
                leaves,
                Finish::Matte,
                at(0.0, 1.0, 0.0),
            );
        }
        Terrain::StairsUp => {
            floor(look);
            let stone = tinted(STONE, c);
            for (i, z) in [0.27f32, 0.0, -0.27].into_iter().enumerate() {
                let h = 0.12 * (i + 1) as f32;
                look.solid(
                    cuboid(0.8, h, 0.27),
                    stone,
                    Finish::Matte,
                    at(0.0, h / 2.0, z),
                );
            }
            // whoever stands here stands on the middle step
            look.ground = 0.24;
            label(look, 0.95);
        }
        Terrain::StairsDown => {
            // a pit with steps going down, away from the camera, in a rim
            let stone = tinted(STONE, c);
            let rim = darker(stone, 0.2);
            for (mesh, x, z) in [
                (cuboid(0.96, 0.05, 0.08), 0.0, -0.44),
                (cuboid(0.96, 0.05, 0.08), 0.0, 0.44),
                (cuboid(0.08, 0.05, 0.8), -0.44, 0.0),
                (cuboid(0.08, 0.05, 0.8), 0.44, 0.0),
            ] {
                look.solid(mesh, rim, Finish::Matte, at(x, 0.025, z));
            }
            look.solid(
                cuboid(0.8, 0.6, 0.04),
                DEEP,
                Finish::Matte,
                at(0.0, -0.3, -0.38),
            );
            for (i, z) in [0.23f32, 0.0, -0.23].into_iter().enumerate() {
                let top = -0.12 * (i + 1) as f32;
                let h = top + 0.6;
                let pos = at(0.0, top - h / 2.0, z);
                look.solid(cuboid(0.8, h, 0.23), stone, Finish::Matte, pos);
            }
            label(look, 0.7);
        }
        Terrain::LadderUp | Terrain::LadderDown => {
            let up = t == Terrain::LadderUp;
            floor(look);
            if !up {
                look.solid(plane(0.6, 0.6), DEEP, Finish::Matte, at(0.0, 0.005, 0.0));
            }
            let wood = tinted(WOOD, c);
            let h = if up { 1.3 } else { 0.5 };
            let rail = cylinder(0.03, 0.03, h);
            for x in [-0.2, 0.2] {
                look.solid(rail, wood, Finish::Matte, at(x, h / 2.0, -0.2));
            }
            let mut y = 0.2;
            while y < h {
                look.solid(
                    cuboid(0.4, 0.03, 0.03),
                    wood,
                    Finish::Matte,
                    at(0.0, y, -0.2),
                );
                y += 0.3;
            }
            label(look, if up { 1.6 } else { 0.8 });
        }
        Terrain::Altar => {
            floor(look);
            let stone = tinted(STONE, c);
            look.solid(
                cuboid(0.76, 0.45, 0.56),
                stone,
                Finish::Matte,
                at(0.0, 0.225, 0.0),
            );
            let slab = lighter(stone, 0.15);
            look.solid(
                cuboid(0.9, 0.06, 0.7),
                slab,
                Finish::Matte,
                at(0.0, 0.48, 0.0),
            );
            look.ground = 0.51;
        }
        Terrain::Throne => {
            floor(look);
            let gold = darker(nh_color(c), 0.15);
            look.solid(
                cuboid(0.6, 0.35, 0.55),
                gold,
                Finish::Glossy,
                at(0.0, 0.175, 0.02),
            );
            look.solid(
                cuboid(0.6, 0.75, 0.12),
                gold,
                Finish::Glossy,
                at(0.0, 0.725, -0.24),
            );
            look.ground = 0.35;
        }
        Terrain::Fountain => {
            floor(look);
            look.solid(
                cylinder(0.42, 0.45, 0.25),
                STONE,
                Finish::Matte,
                at(0.0, 0.125, 0.0),
            );
            let water = nh_color(c);
            look.solid(
                cylinder(0.36, 0.36, 0.02),
                water,
                Finish::Glossy,
                at(0.0, 0.25, 0.0),
            );
            look.solid(
                cylinder(0.05, 0.07, 0.5),
                STONE,
                Finish::Matte,
                at(0.0, 0.4, 0.0),
            );
            look.ground = 0.25;
        }
        Terrain::Sink => {
            floor(look);
            let stone = tinted(STONE, c);
            look.solid(
                cuboid(0.6, 0.4, 0.5),
                stone,
                Finish::Glossy,
                at(0.0, 0.2, 0.0),
            );
            look.solid(plane(0.44, 0.34), DEEP, Finish::Matte, at(0.0, 0.405, 0.0));
            look.ground = 0.4;
        }
        Terrain::Grave => {
            look.solid(tile, EARTH, Finish::Matte, Vector3::ZERO);
            let mound = lighter(EARTH, 0.1);
            look.solid(
                cuboid(0.5, 0.1, 0.6),
                mound,
                Finish::Matte,
                at(0.0, 0.05, 0.12),
            );
            let stone = tinted(STONE, c);
            look.solid(
                cuboid(0.5, 0.6, 0.1),
                stone,
                Finish::Matte,
                at(0.0, 0.3, -0.32),
            );
            look.ground = 0.1;
        }
        Terrain::Pool | Terrain::Water => {
            let deep = if t == Terrain::Water { 0.55 } else { 0.35 };
            let water = darker(nh_color(c), deep);
            look.solid(plane(1.0, 1.0), water, Finish::Glossy, at(0.0, -0.04, 0.0));
            look.ground = -0.04;
        }
        Terrain::Ice => {
            let ice = lighter(nh_color(c), 0.3);
            look.solid(plane(1.0, 1.0), ice, Finish::Glossy, Vector3::ZERO);
        }
        Terrain::Lava => {
            look.solid(
                plane(1.0, 1.0),
                nh_color(c),
                Finish::Ember,
                at(0.0, -0.04, 0.0),
            );
            look.ground = -0.04;
        }
        Terrain::LavaWall => {
            let mesh = cuboid(1.0, wall_h, 1.0);
            look.solid(mesh, nh_color(c), Finish::Ember, at(0.0, wall_h / 2.0, 0.0));
            look.ground = wall_h;
        }
        Terrain::DrawbridgeDown => {
            let wood = tinted(WOOD, c);
            look.solid(
                cuboid(1.0, 0.08, 1.0),
                wood,
                Finish::Matte,
                at(0.0, 0.04, 0.0),
            );
            // two beams along the span
            let mesh = if vertical {
                cuboid(1.0, 0.1, 0.06)
            } else {
                cuboid(0.06, 0.1, 1.0)
            };
            for d in [-0.3, 0.3] {
                let pos = if vertical {
                    at(0.0, 0.09, d)
                } else {
                    at(d, 0.09, 0.0)
                };
                look.solid(mesh, darker(wood, 0.2), Finish::Matte, pos);
            }
            look.ground = 0.08;
        }
        Terrain::DrawbridgeUp => {
            let water = Color::from_rgb(0.05, 0.1, 0.3);
            look.solid(plane(1.0, 1.0), water, Finish::Glossy, at(0.0, -0.04, 0.0));
            let mesh = if vertical {
                cuboid(0.2, 1.1, 0.96)
            } else {
                cuboid(0.96, 1.1, 0.2)
            };
            look.solid(mesh, tinted(WOOD, c), Finish::Matte, at(0.0, 0.55, 0.0));
            look.ground = 1.1;
        }
        Terrain::Air => {
            let air = lighter(nh_color(c), 0.4);
            look.solid(plane(1.0, 1.0), air, Finish::Ghost, Vector3::ZERO);
        }
        Terrain::Cloud => {
            let cloud = lighter(nh_color(c), 0.2);
            look.solid(
                cuboid(0.96, 0.6, 0.96),
                cloud,
                Finish::Ghost,
                at(0.0, 0.3, 0.0),
            );
        }
        Terrain::Trap => {
            floor(look);
            let mark = darker(nh_color(c), 0.45);
            look.solid(
                cylinder(0.34, 0.34, 0.02),
                mark,
                Finish::Matte,
                at(0.0, 0.01, 0.0),
            );
            if let Some(ch) = glyph_char(g) {
                look.letter(ch, nh_color(c), 0.3, PX_TRAP, false);
            }
        }
    }
}

/// Monsters, objects and the like on top of the terrain.
fn entity_look(look: &mut Look, g: &Glyph, catalog: &Catalog) {
    let ground = look.ground;
    let color = nh_color(g.color);
    let ch = glyph_char(g);
    match g.kind {
        GlyphKind::Mon => {
            let h = monster_height(catalog, g.mon);
            let r = (h * 0.3).clamp(0.12, 0.38);
            let ghost = g.flags & (mg::DETECT | mg::INVIS) != 0;
            let finish = if ghost { Finish::Ghost } else { Finish::Matte };
            look.solid(capsule(r, h), color, finish, at(0.0, ground + h / 2.0, 0.0));
            if g.flags & mg::PET != 0 {
                let ring = torus(0.34, 0.42);
                look.solid(ring, PET_RING, Finish::Flat, at(0.0, ground + 0.03, 0.0));
            }
            if let Some(ch) = ch {
                let color = if g.flags & mg::HERO != 0 {
                    Color::WHITE
                } else {
                    lighter(color, 0.15)
                };
                look.letter(ch, color, ground + h + MONSTER_LABEL_LIFT, PX_MONSTER, true);
            }
        }
        GlyphKind::Invisible => {
            let gray = Color::from_rgb(0.7, 0.72, 0.8);
            look.solid(
                sphere(0.3),
                gray,
                Finish::Ghost,
                at(0.0, ground + 0.35, 0.0),
            );
            if let Some(ch) = ch {
                look.letter(
                    ch,
                    gray,
                    ground + 0.65 + MONSTER_LABEL_LIFT,
                    PX_MONSTER,
                    true,
                );
            }
        }
        GlyphKind::Warning => {
            if let Some(ch) = ch {
                look.letter(ch, color, ground + 0.5, PX_MONSTER, true);
            }
        }
        GlyphKind::Obj => {
            let mut top = ground;
            if is_boulder(g, catalog) {
                look.solid(
                    sphere(0.42),
                    color,
                    Finish::Matte,
                    at(0.0, ground + 0.42, 0.0),
                );
                top += 0.84;
            } else if g.flags & mg::OBJPILE != 0 {
                for (s, yaw) in [(0.28f32, 10.0f32), (0.23, 38.0), (0.18, 64.0)] {
                    let pos = at(0.0, top + s / 2.0, 0.0);
                    let rot = at(0.0, yaw, 0.0);
                    look.turned(cuboid(s, s, s), color, Finish::Matte, pos, rot);
                    top += s;
                }
            } else {
                let s = 0.24;
                let pos = at(0.0, top + s / 2.0, 0.0);
                look.turned(
                    cuboid(s, s, s),
                    color,
                    Finish::Matte,
                    pos,
                    at(0.0, 20.0, 0.0),
                );
                top += s;
            }
            if let Some(ch) = ch {
                look.letter(ch, lighter(color, 0.1), top + 0.26, PX_OBJECT, false);
            }
        }
        GlyphKind::Body => {
            let h = monster_height(catalog, g.mon) * 0.8;
            let r = (h * 0.22).clamp(0.08, 0.3);
            let pos = at(0.0, ground + r, 0.0);
            let lying = at(0.0, 25.0, 90.0);
            look.turned(capsule(r, h), darker(color, 0.2), Finish::Matte, pos, lying);
            if let Some(ch) = ch {
                look.letter(ch, color, ground + 2.0 * r + 0.25, PX_OBJECT, false);
            }
        }
        GlyphKind::Statue => {
            let h = monster_height(catalog, g.mon);
            let r = (h * 0.3).clamp(0.12, 0.38);
            let plinth = 0.1;
            look.solid(
                cuboid(0.56, plinth, 0.56),
                STONE,
                Finish::Matte,
                at(0.0, ground + 0.05, 0.0),
            );
            let pos = at(0.0, ground + plinth + h / 2.0, 0.0);
            look.solid(capsule(r, h), STATUE, Finish::Matte, pos);
            if let Some(ch) = ch {
                look.letter(ch, color, ground + plinth + h + 0.3, PX_OBJECT, false);
            }
        }
        GlyphKind::Zap | GlyphKind::Explosion => bright_cube(look, g),
        // the engulfer is drawn once, around the hero
        _ => {}
    }
}

/// A beam, explosion or sparkle: a bright cube for the moment it shows.
fn bright_cube(look: &mut Look, g: &Glyph) {
    let color = lighter(nh_color(g.color), 0.3);
    let y = look.ground.max(0.0) + 0.5;
    look.solid(cuboid(0.5, 0.5, 0.5), color, Finish::Glow, at(0.0, y, 0.0));
}

/// Can something north of a wall be seen or stood on (so the wall would
/// hide it)?
fn is_open(cell: Option<&Cell>, catalog: &Catalog) -> bool {
    let Some(cell) = cell else {
        return false;
    };
    match cell_terrain(cell, catalog) {
        Some(t) => !matches!(
            t,
            Terrain::Stone | Terrain::Wall | Terrain::LavaWall | Terrain::Effect | Terrain::Unknown
        ),
        // only what stays put: the hero, or a remembered object; a sensed
        // monster or a warning passing through rock would make walls flicker
        None => cell.entity().is_some_and(|g| {
            g.flags & mg::HERO != 0
                || matches!(g.kind, GlyphKind::Obj | GlyphKind::Body | GlyphKind::Statue)
        }),
    }
}

/// The orthogonal neighbours of a cell: north (y - 1), south, west, east.
#[derive(Clone, Copy, Default)]
struct Near<'a>([Option<&'a Cell>; 4]);

impl<'a> Near<'a> {
    fn of(map: &'a MapState, x: i32, y: i32) -> Near<'a> {
        Near([
            map.cell(x, y - 1),
            map.cell(x, y + 1),
            map.cell(x - 1, y),
            map.cell(x + 1, y),
        ])
    }

    fn north(&self) -> Option<&'a Cell> {
        self.0[0]
    }

    /// The floor most neighbours show (lit, dark or corridor; ties in that
    /// order), as one of their terrain glyphs.
    fn floor(&self, catalog: &Catalog) -> Option<&'a Glyph> {
        let floors = [Terrain::Floor, Terrain::DarkFloor, Terrain::Corridor];
        let known: Vec<(Terrain, &Glyph)> = self
            .0
            .into_iter()
            .filter_map(|c| {
                let g = c?.terrain.as_ref()?;
                Some((terrain_of(cmap_sym(g, catalog)?), g))
            })
            .filter(|(t, _)| floors.contains(t))
            .collect();
        let mut best: Option<(usize, &Glyph)> = None;
        for t in floors {
            let n = known.iter().filter(|(k, _)| *k == t).count();
            if n > best.map_or(0, |(m, _)| m) {
                best = known.iter().find(|(k, _)| *k == t).map(|(_, g)| (n, *g));
            }
        }
        best.map(|(_, g)| g)
    }
}

/// A cell's look; `near` are its neighbours (walls in front of open ground
/// are cut down, unseen ground takes the floor around it).
fn look_of(cell: &Cell, near: Near, catalog: &Catalog) -> Look {
    let mut look = Look::default();
    match &cell.terrain {
        Some(t) => {
            let sym = cmap_sym(t, catalog).unwrap_or("");
            let cut = is_open(near.north(), catalog);
            terrain_look(&mut look, terrain_of(sym), sym, t, cut);
        }
        // print_glyph sends an unexplored background under monsters and
        // objects: something stands there, so it is walkable; it looks
        // like the floor around it, when that is known
        None if cell.entity().is_some() => match near.floor(catalog) {
            Some(g) => {
                // the floor, not a neighbour's engraving
                let sym = match cmap_sym(g, catalog).unwrap_or("") {
                    "S_engroom" => "S_room",
                    "S_engrcorr" => "S_corr",
                    sym => sym,
                };
                terrain_look(&mut look, terrain_of(sym), sym, g, false);
            }
            None => {
                let tile = plane(0.96, 0.96);
                look.solid(tile, FLOOR_UNSEEN, Finish::Matte, Vector3::ZERO);
            }
        },
        None => {}
    }
    match &cell.glyph {
        Some(g) if g.kind == GlyphKind::Cmap => {
            if cmap_sym(g, catalog).is_some_and(|s| terrain_of(s) == Terrain::Effect) {
                bright_cube(&mut look, g);
            }
        }
        Some(g) => entity_look(&mut look, g, catalog),
        None => {}
    }
    look
}

#[derive(Default)]
struct CellNodes {
    look: Look,
    solids: Vec<Gd<MeshInstance3D>>,
    letters: Vec<Gd<Label3D>>,
}

impl CellNodes {
    fn free(self) {
        for mut n in self.solids {
            n.queue_free();
        }
        for mut n in self.letters {
            n.queue_free();
        }
    }
}

pub struct MapView {
    root: Gd<Node3D>,
    cells_root: Gd<Node3D>,
    camera: Gd<Camera3D>,
    cells: HashMap<(i32, i32), CellNodes>,
    generation: Option<u64>,
    meshes: HashMap<MeshKey, Gd<Mesh>>,
    materials: HashMap<(u32, Finish), Gd<StandardMaterial3D>>,
    font: Gd<SystemFont>,
    hover: Gd<Node3D>,
    cursor: Gd<Node3D>,
    hero_ring: Gd<MeshInstance3D>,
    hero_light: Gd<OmniLight3D>,
    engulf: Gd<MeshInstance3D>,
    engulf_mat: Gd<StandardMaterial3D>,
    target: Vector3,
    focus: Vector3,
    distance: f32,
    /// The whole-level view is on: its distance, refitted as cells appear.
    overview: Option<f32>,
    snap: bool,
}

/// A square outline over a cell: four thin bars.
fn frame(root: &mut Gd<Node3D>, color: Color, width: f32, height: f32) -> Gd<Node3D> {
    let mut node = Node3D::new_alloc();
    let mat = build_material(color, Finish::Flat);
    let along_x = cuboid(1.0, height, width).build();
    let along_z = cuboid(width, height, 1.0).build();
    let e = 0.5 - width / 2.0;
    for (mesh, pos) in [
        (&along_x, at(0.0, 0.0, -e)),
        (&along_x, at(0.0, 0.0, e)),
        (&along_z, at(-e, 0.0, 0.0)),
        (&along_z, at(e, 0.0, 0.0)),
    ] {
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(mesh);
        mi.set_material_override(&mat);
        mi.set_position(pos);
        node.add_child(&mi);
    }
    node.set_visible(false);
    root.add_child(&node);
    node
}

impl MapView {
    pub fn new(mut root: Gd<Node3D>) -> MapView {
        let mut env = Environment::new_gd();
        env.set_background(BgMode::COLOR);
        env.set_bg_color(theme::BG);
        env.set_ambient_source(AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.7, 0.72, 0.85));
        env.set_ambient_light_energy(0.65);
        let mut world_env = WorldEnvironment::new_alloc();
        world_env.set_environment(&env);
        root.add_child(&world_env);

        let mut sun = DirectionalLight3D::new_alloc();
        sun.set_rotation_degrees(Vector3::new(-60.0, 30.0, 0.0));
        sun.set_param(Param::ENERGY, 0.75);
        root.add_child(&sun);

        let mut camera = Camera3D::new_alloc();
        camera.set_fov(FOV_DEG);
        camera.set_current(true);
        root.add_child(&camera);

        let mut cells_root = Node3D::new_alloc();
        root.add_child(&cells_root);
        cells_root.set_name("Cells");

        let hover = frame(&mut root, Color::from_rgba(1.0, 1.0, 1.0, 0.5), 0.04, 0.02);
        let cursor = frame(
            &mut root,
            Color::from_rgba(0.35, 0.95, 1.0, 0.95),
            0.09,
            0.08,
        );

        let mut hero_ring = MeshInstance3D::new_alloc();
        hero_ring.set_mesh(&torus(0.36, 0.46).build());
        hero_ring.set_material_override(&build_material(HERO_RING, Finish::Flat));
        hero_ring.set_visible(false);
        root.add_child(&hero_ring);

        let mut hero_light = OmniLight3D::new_alloc();
        hero_light.set_color(HERO_LIGHT);
        hero_light.set_param(Param::ENERGY, 1.4);
        hero_light.set_param(Param::RANGE, 6.5);
        hero_light.set_visible(false);
        root.add_child(&hero_light);

        let engulf_mat = build_material(Color::WHITE, Finish::Ghost);
        let mut engulf = MeshInstance3D::new_alloc();
        engulf.set_mesh(&sphere(1.3).build());
        engulf.set_material_override(&engulf_mat);
        engulf.set_visible(false);
        root.add_child(&engulf);

        let center = Vector3::new(COLNO as f32 / 2.0, 0.0, ROWNO as f32 / 2.0);
        let mut view = MapView {
            root,
            cells_root,
            camera,
            cells: HashMap::new(),
            generation: None,
            meshes: HashMap::new(),
            materials: HashMap::new(),
            font: theme::mono_bold(),
            hover,
            cursor,
            hero_ring,
            hero_light,
            engulf,
            engulf_mat,
            target: center,
            focus: center,
            distance: DISTANCE,
            overview: None,
            snap: true,
        };
        view.place_camera();
        view
    }

    /// Rebuild on a new generation, else apply dirty cells; camera target =
    /// view_center, else hero; cursor marker when World.cursor differs from
    /// the hero, or getpos moves it. The hero ring marks `World::hero`,
    /// also when the hero is not drawn (invisible).
    pub fn sync(&mut self, world: &mut World, catalog: &Catalog, delta: f64) {
        let generation = world.map.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            world.map.take_dirty();
            for y in 0..ROWNO {
                for x in 1..COLNO {
                    self.update_cell(x, y, world, catalog);
                }
            }
            self.snap = true;
        } else {
            // a cell's look depends on its neighbours: a wall on the cell
            // north of it, unseen ground under something on all four
            let mut dirty = world.map.take_dirty();
            let near: Vec<_> = dirty
                .iter()
                .flat_map(|&(x, y)| [(x, y + 1), (x, y - 1), (x + 1, y), (x - 1, y)])
                .filter(|&(x, y)| in_field(x, y))
                .collect();
            dirty.extend(near);
            dirty.sort_unstable();
            dirty.dedup();
            for (x, y) in dirty {
                self.update_cell(x, y, world, catalog);
            }
        }
        let hero = world.hero();
        let bounds = self.overview.and(self.known_bounds());
        if let Some(b) = bounds {
            // never closer than the player's own view
            let (centre, distance) = overview_frame(b, self.aspect());
            self.target = centre;
            self.overview = Some(distance.max(self.distance));
        } else if let Some((x, y)) = world.view_center.or(hero).or(world.cursor) {
            self.target = Vector3::new(x as f32, 0.0, y as f32);
        }
        match hero {
            Some((x, y)) => {
                let ground = self.ground(x, y);
                let p = Vector3::new(x as f32, ground, y as f32);
                self.hero_ring.set_position(p + at(0.0, 0.03, 0.0));
                self.hero_ring.set_visible(true);
                self.hero_light.set_position(p + at(0.0, 1.8, 0.3));
                self.hero_light.set_visible(true);
            }
            None => {
                self.hero_ring.set_visible(false);
                self.hero_light.set_visible(false);
            }
        }
        match hero.and_then(|h| engulfer_color(world, h)) {
            Some(color) => {
                let (x, y) = hero.unwrap_or_default();
                self.engulf_mat.set_albedo(color.with_alpha(0.35));
                self.engulf
                    .set_position(Vector3::new(x as f32, 0.6, y as f32));
                self.engulf.set_visible(true);
            }
            None => self.engulf.set_visible(false),
        }
        // the core's cursor is on the hero at every command: shown only
        // when elsewhere, or while getpos moves it
        let shown = |c: &(i32, i32)| world.getpos || (hero.is_some() && Some(*c) != hero);
        match world.cursor.filter(shown) {
            Some((x, y)) => {
                let ground = self.ground(x, y);
                self.cursor
                    .set_position(Vector3::new(x as f32, ground + 0.05, y as f32));
                self.cursor.set_visible(true);
            }
            None => self.cursor.set_visible(false),
        }
        if self.snap {
            self.focus = self.target;
            self.snap = false;
        } else {
            let t = (FOLLOW_RATE * delta as f32).clamp(0.0, 1.0);
            self.focus = self.focus.lerp(self.target, t);
        }
        self.place_camera();
    }

    /// The map cell under a screen position: the first wall, door, creature
    /// or letter the camera ray passes through, else where it meets the ground.
    pub fn cell_at(&self, screen_pos: Vector2) -> Option<(i32, i32)> {
        if !self.camera.is_inside_tree() {
            return None;
        }
        let origin = self.camera.project_ray_origin(screen_pos);
        let dir = self.camera.project_ray_normal(screen_pos);
        pick_cell(origin, dir, |x, y| {
            self.cells.get(&(x, y)).map_or(0.0, |n| n.look.top)
        })
    }

    pub fn set_hover(&mut self, cell: Option<(i32, i32)>) {
        match cell {
            Some((x, y)) => {
                let ground = self.ground(x, y);
                self.hover
                    .set_position(Vector3::new(x as f32, ground + 0.03, y as f32));
                self.hover.set_visible(true);
            }
            None => self.hover.set_visible(false),
        }
    }

    /// Wheel steps: positive moves the camera away. Ends the overview.
    pub fn zoom(&mut self, steps: f32) {
        self.overview = None;
        self.distance = (self.distance + steps * 1.5).clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.place_camera();
    }

    /// Frame everything known of the level, or go back to following the hero.
    pub fn toggle_overview(&mut self) {
        self.overview = match self.overview {
            Some(_) => None,
            None => Some(self.distance),
        };
    }

    /// The whole-level view is on (self-tests).
    pub fn in_overview(&self) -> bool {
        self.overview.is_some()
    }

    /// The camera's distance now (self-tests).
    pub fn camera_distance(&self) -> f32 {
        self.overview.unwrap_or(self.distance)
    }

    /// The smallest box of cells with anything drawn: (x0, y0, x1, y1).
    fn known_bounds(&self) -> Option<(i32, i32, i32, i32)> {
        self.cells
            .iter()
            .filter(|(_, n)| !n.look.is_empty())
            .map(|(&(x, y), _)| (x, y, x, y))
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }

    /// Width over height of the window.
    fn aspect(&self) -> f32 {
        self.camera
            .get_viewport()
            .map(|vp| vp.get_visible_rect().size)
            .filter(|s| s.y > 0.0)
            .map_or(16.0 / 9.0, |s| s.x / s.y)
    }

    /// Forget every cell (a new game).
    pub fn clear(&mut self) {
        for (_, nodes) in self.cells.drain() {
            nodes.free();
        }
        self.generation = None;
        self.hover.set_visible(false);
        self.cursor.set_visible(false);
        self.hero_ring.set_visible(false);
        self.hero_light.set_visible(false);
        self.engulf.set_visible(false);
        self.snap = true;
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
    }

    /// The camera has caught up with its target (self-test screenshots).
    pub fn is_settled(&self) -> bool {
        self.focus.distance_to(self.target) < 0.05
    }

    /// Cells with anything drawn (self-tests check the map is drawn).
    pub fn drawn_cells(&self) -> usize {
        self.cells.values().filter(|c| !c.look.is_empty()).count()
    }

    /// The height things stand at on a cell (0 when nothing is known).
    fn ground(&self, x: i32, y: i32) -> f32 {
        self.cells.get(&(x, y)).map_or(0.0, |n| n.look.ground)
    }

    fn place_camera(&mut self) {
        let pitch = PITCH_DEG.to_radians();
        let distance = self.camera_distance();
        let offset = Vector3::new(0.0, pitch.sin(), pitch.cos()) * distance;
        // aim a little south of the hero, so the hero stands above the
        // log; close in, the same offset would push the hero off the top
        let south = AIM_SOUTH * distance / DISTANCE;
        let aim = self.focus + Vector3::new(0.0, 0.0, south);
        self.camera.look_at_from_position(aim + offset, aim);
    }

    fn update_cell(&mut self, x: i32, y: i32, world: &World, catalog: &Catalog) {
        let look = world
            .map
            .cell(x, y)
            .map(|c| look_of(c, Near::of(&world.map, x, y), catalog))
            .unwrap_or_default();
        match self.cells.get(&(x, y)) {
            Some(n) if n.look == look => return,
            None if look.is_empty() => return,
            _ => {}
        }
        let mut nodes = self.cells.remove(&(x, y)).unwrap_or_default();
        let old = std::mem::take(&mut nodes.look);
        let origin = Vector3::new(x as f32, 0.0, y as f32);
        // only what changed crosses into the engine: a step redraws two
        // cells, a level arrives as a thousand of them
        for (i, s) in look.solids.iter().enumerate() {
            let before = old.solids.get(i);
            if i == nodes.solids.len() {
                nodes.solids.push(MeshInstance3D::new_alloc());
            }
            let mi = &mut nodes.solids[i];
            if before.map(|b| b.mesh) != Some(s.mesh) {
                let mesh = self.meshes.entry(s.mesh).or_insert_with(|| s.mesh.build());
                mi.set_mesh(&*mesh);
            }
            if before.map(|b| (b.color, b.finish)) != Some((s.color, s.finish)) {
                let mat = self
                    .materials
                    .entry((color_key(s.color), s.finish))
                    .or_insert_with(|| build_material(s.color, s.finish));
                mi.set_material_override(&*mat);
            }
            if before.map(|b| (b.pos, b.rot)) != Some((s.pos, s.rot)) {
                mi.set_transform(solid_transform(origin, s));
            }
            if i >= old.solids.len() {
                if mi.is_inside_tree() {
                    mi.set_visible(true);
                } else {
                    self.cells_root.add_child(&*mi);
                }
            }
        }
        for mi in nodes
            .solids
            .iter_mut()
            .skip(look.solids.len())
            .take(old.solids.len().saturating_sub(look.solids.len()))
        {
            mi.set_visible(false);
        }
        for (i, l) in look.letters.iter().enumerate() {
            let before = old.letters.get(i);
            if i == nodes.letters.len() {
                let label = self.new_label();
                nodes.letters.push(label);
            }
            let label = &mut nodes.letters[i];
            if before.map(|b| b.ch) != Some(l.ch) {
                label.set_text(&l.ch.to_string());
            }
            if before.map(|b| b.color) != Some(l.color) {
                label.set_modulate(l.color);
            }
            if before.map(|b| b.pixel_size) != Some(l.pixel_size) {
                label.set_pixel_size(l.pixel_size);
            }
            if before.map(|b| b.pos) != Some(l.pos) {
                label.set_position(origin + l.pos);
            }
            if before.map(|b| b.on_top) != Some(l.on_top) {
                label.set_draw_flag(DrawFlags::DISABLE_DEPTH_TEST, l.on_top);
                label.set_render_priority(if l.on_top { 2 } else { 0 });
                label.set_outline_render_priority(if l.on_top { 1 } else { -1 });
            }
            if i >= old.letters.len() {
                if label.is_inside_tree() {
                    label.set_visible(true);
                } else {
                    self.cells_root.add_child(&*label);
                }
            }
        }
        for label in nodes
            .letters
            .iter_mut()
            .skip(look.letters.len())
            .take(old.letters.len().saturating_sub(look.letters.len()))
        {
            label.set_visible(false);
        }
        nodes.look = look;
        self.cells.insert((x, y), nodes);
    }

    fn new_label(&mut self) -> Gd<Label3D> {
        let mut l = Label3D::new_alloc();
        l.set_billboard_mode(BillboardMode::ENABLED);
        l.set_font(&self.font);
        l.set_font_size(FONT_PX);
        l.set_outline_size(18);
        l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.85));
        l
    }
}

/// The overview's aim and camera distance for the cells `(x0, y0, x1, y1)`
/// in a window of this aspect: the box with a cell of margin fits the
/// width, and the height above the message log (the bottom quarter).
fn overview_frame((x0, y0, x1, y1): (i32, i32, i32, i32), aspect: f32) -> (Vector3, f32) {
    let centre = Vector3::new((x0 + x1) as f32 / 2.0, 0.0, (y0 + y1) as f32 / 2.0);
    let (w, h) = ((x1 - x0 + 3) as f32, (y1 - y0 + 3) as f32);
    let tan = (FOV_DEG.to_radians() / 2.0).tan();
    let pitch = PITCH_DEG.to_radians();
    let across = w / (2.0 * tan * aspect);
    // a row of depth looks sin(pitch) tall; three quarters of the screen
    let down = h * pitch.sin() / (2.0 * tan * 0.75);
    let distance = across.max(down).clamp(MIN_DISTANCE, MAX_OVERVIEW_DISTANCE);
    (centre, distance)
}

/// Where a solid sits: the cell origin plus its offset, turned by its
/// Euler angles (degrees, Godot's YXZ order).
fn solid_transform(origin: Vector3, s: &Solid) -> Transform3D {
    let rot = s.rot * (std::f32::consts::PI / 180.0);
    let basis = Basis::from_euler(EulerOrder::YXZ, rot);
    Transform3D::new(basis, origin + s.pos)
}

/// The engulfer's colour while the hero is swallowed.
fn engulfer_color(world: &World, (hx, hy): (i32, i32)) -> Option<Color> {
    if !world.map.is_swallowed() {
        return None;
    }
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (hx + dx, hy + dy)))
        .filter_map(|(x, y)| world.map.cell(x, y)?.glyph.as_ref())
        .find(|g| g.kind == GlyphKind::Swallow)
        .map(|g| nh_color(g.color))
}

/// Walk a ray down from the tallest geometry in small steps; the first cell
/// whose column (0..height) holds the point is hit. A ray that hits nothing
/// raised picks the cell where it meets y = 0.
fn pick_cell(
    origin: Vector3,
    dir: Vector3,
    height: impl Fn(i32, i32) -> f32,
) -> Option<(i32, i32)> {
    if dir.y > -1e-4 {
        return None;
    }
    let ground = -origin.y / dir.y;
    if ground <= 0.0 {
        return None;
    }
    let cell = |t: f32| {
        let p = origin + dir * t;
        (p.x.round() as i32, p.z.round() as i32, p.y)
    };
    let step = PICK_STEP / -dir.y;
    let mut t = ((MAX_TOP - origin.y) / dir.y).max(0.0);
    while t < ground {
        let (x, y, h) = cell(t);
        if in_field(x, y) && height(x, y) >= h {
            return Some((x, y));
        }
        t += step;
    }
    let (x, y, _) = cell(ground);
    in_field(x, y).then_some((x, y))
}

#[cfg(test)]
mod tests {
    use nh_protocol::{EngineMsg, parse_line};

    use super::*;

    fn catalog() -> Catalog {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    fn glyph(kind: GlyphKind, ch: char) -> Glyph {
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

    fn cmap(cat: &Catalog, sym: &str) -> Glyph {
        let info = cat.cmap.iter().find(|c| c.sym == sym).unwrap();
        Glyph {
            cmap: Some(info.idx),
            color: info.color,
            ..glyph(GlyphKind::Cmap, char::from_u32(info.ch as u32).unwrap())
        }
    }

    fn on_floor(cat: &Catalog, g: Glyph) -> Cell {
        Cell {
            glyph: Some(g),
            bk: None,
            terrain: Some(cmap(cat, "S_room")),
        }
    }

    fn monster(cat: &Catalog, name: &str, flags: u32) -> Glyph {
        let m = cat.monsters.iter().find(|m| m.name == name).unwrap();
        Glyph {
            mon: Some(m.idx),
            flags,
            color: m.color,
            ..glyph(GlyphKind::Mon, 'x')
        }
    }

    /// The camera's ray, PITCH_DEG down and looking north, meeting the
    /// ground at the centre of cell (10, 9).
    fn ray() -> (Vector3, Vector3) {
        let pitch = PITCH_DEG.to_radians();
        let back = Vector3::new(0.0, pitch.sin(), pitch.cos());
        (Vector3::new(10.0, 0.0, 9.0) + back * DISTANCE, -back)
    }

    /// Screen rows a monster's letter covers, in rows of depth from its
    /// cell's centre (the camera looks north, PITCH_DEG down; parallel
    /// projection): a height h rises h * cot(pitch) rows, a billboard of
    /// height s spans s / sin(pitch) rows. A letter ('@', capitals) fills
    /// about three quarters of the font's line.
    fn letter_rows(h: f32) -> (f32, f32) {
        let pitch = PITCH_DEG.to_radians();
        let centre = (h + MONSTER_LABEL_LIFT) / pitch.tan();
        let half = 0.75 * FONT_PX as f32 * PX_MONSTER / 2.0 / pitch.sin();
        (centre - half, centre + half)
    }

    #[test]
    fn monster_letters_keep_to_their_rows() {
        let cat = catalog();
        let height = |name: &str| monster_height(&cat, monster(&cat, name, 0).mon);
        // tiny to large, the hero among them: the letter of the monster a
        // row north starts above the top of this one's
        for south in ["newt", "kitten", "human", "jackal", "tiger"] {
            for north in ["newt", "kitten", "human", "jackal"] {
                let (_, top) = letter_rows(height(south));
                let (bottom, _) = letter_rows(height(north));
                assert!(
                    top < 1.0 + bottom,
                    "{north}'s letter a row north of {south}'s overlaps it"
                );
            }
        }
        // and a medium monster's letter stays off the next row's centre
        let (_, top) = letter_rows(height("human"));
        assert!(top < 1.0, "{top}");
    }

    #[test]
    fn the_overview_fits_the_level_and_centres_it() {
        let wide = 16.0 / 9.0;
        // a whole level: 79 columns decide
        let (centre, full) = overview_frame((1, 0, 79, 20), wide);
        assert_eq!((centre.x, centre.z), (40.0, 10.0));
        let tan = (FOV_DEG.to_radians() / 2.0).tan();
        assert!(2.0 * full * tan * wide >= 81.0, "{full}");
        assert!(full <= MAX_OVERVIEW_DISTANCE);
        // a single room is closer, but never closer than the nearest zoom
        let (_, room) = overview_frame((30, 5, 40, 10), wide);
        assert!(room < full && room >= MIN_DISTANCE, "{room}");
        let (_, cell) = overview_frame((30, 5, 30, 5), wide);
        assert_eq!(cell, MIN_DISTANCE);
        // a tall narrow window: the width still fits
        let (_, narrow) = overview_frame((1, 0, 79, 20), 1.0);
        assert!(narrow > full);
    }

    #[test]
    fn a_ray_over_flat_ground_picks_where_it_lands() {
        let (origin, dir) = ray();
        assert_eq!(pick_cell(origin, dir, |_, _| 0.0), Some((10, 9)));
    }

    #[test]
    fn a_wall_in_front_takes_the_ray() {
        let (origin, dir) = ray();
        // the south wall of the room hides the floor cell north of it
        let h = |x, y| if (x, y) == (10, 10) { WALL_HEIGHT } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 10)));
        // a letter standing south of the floor cell covers it too
        let h = |x, y| if (x, y) == (10, 10) { 1.2 } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 10)));
        // a raised cell behind the landing point is never reached
        let h = |x, y| if (x, y) == (10, 8) { WALL_HEIGHT } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 9)));
    }

    #[test]
    fn a_ray_away_from_the_ground_picks_nothing() {
        let (origin, dir) = ray();
        assert_eq!(pick_cell(origin, -dir, |_, _| 0.0), None);
        let off_map = Vector3::new(-50.0, 0.0, 9.0) + (origin - Vector3::new(10.0, 0.0, 9.0));
        assert_eq!(pick_cell(off_map, dir, |_, _| 0.0), None);
    }

    #[test]
    fn every_map_feature_is_drawn_within_reach() {
        let cat = catalog();
        for info in &cat.cmap {
            let t = terrain_of(&info.sym);
            let cell = Cell {
                glyph: Some(cmap(&cat, &info.sym)),
                bk: None,
                terrain: (t != Terrain::Effect).then(|| cmap(&cat, &info.sym)),
            };
            let look = look_of(&cell, Near::default(), &cat);
            let drawn = !matches!(t, Terrain::Stone);
            assert_eq!(!look.is_empty(), drawn, "{}", info.sym);
            assert!(look.top <= MAX_TOP, "{}", info.sym);
            if matches!(t, Terrain::Wall | Terrain::ClosedDoor) {
                assert!(look.top >= 1.0, "{} stands up", info.sym);
            }
        }
        // stairs and traps say what they are
        let letters = |sym: &str| {
            let cell = on_floor(&cat, cmap(&cat, sym));
            let cell = Cell {
                terrain: cell.glyph.clone(),
                ..cell
            };
            look_of(&cell, Near::default(), &cat)
                .letters
                .iter()
                .map(|l| l.ch)
                .collect::<String>()
        };
        assert_eq!(letters("S_upstair"), "<");
        assert_eq!(letters("S_dnladder"), ">");
        assert_eq!(letters("S_bear_trap"), "^");
        assert_eq!(letters("S_room"), "");
        // engravings: the plain floor or corridor with scratches, never
        // tinted their bright blue (a pool's colour)
        let look = |sym: &str| {
            let g = cmap(&cat, sym);
            let cell = Cell {
                glyph: Some(g.clone()),
                bk: None,
                terrain: Some(g),
            };
            look_of(&cell, Near::default(), &cat)
        };
        for (engr, plain) in [("S_engroom", "S_room"), ("S_engrcorr", "S_corr")] {
            let (e, p) = (look(engr), look(plain));
            assert_eq!(e.solids[0].color, p.solids[0].color, "{engr}");
            assert_eq!(e.solids.len(), p.solids.len() + 3, "{engr}");
        }
    }

    #[test]
    fn monsters_stand_by_size_with_their_letter_above() {
        let cat = catalog();
        let look = |g: Glyph| look_of(&on_floor(&cat, g), Near::default(), &cat);
        let newt = look(monster(&cat, "newt", 0));
        let giant = look(monster(&cat, "hill giant", 0));
        assert!(newt.top < giant.top);
        for l in [&newt, &giant] {
            let body = l
                .solids
                .iter()
                .find(|s| matches!(s.mesh, MeshKey::Capsule(..)));
            let body = body.unwrap();
            assert!(l.letters[0].pos.y > body.pos.y + body.mesh.half_height());
            assert!(l.letters[0].on_top, "monster letters stay readable");
        }
        let pet = look(monster(&cat, "little dog", mg::PET));
        assert!(pet.solids.iter().any(|s| s.color == PET_RING));
        let seen = look(monster(&cat, "little dog", mg::DETECT));
        assert!(seen.solids.iter().any(|s| s.finish == Finish::Ghost));
        // a monster on an altar stands on it
        let mut on_altar = on_floor(&cat, monster(&cat, "newt", 0));
        on_altar.terrain = Some(cmap(&cat, "S_altar"));
        let alt = look_of(&on_altar, Near::default(), &cat);
        assert!(alt.top > newt.top + 0.4);
        // and on up stairs, on the step under the cell's centre
        let mut on_stairs = on_altar;
        on_stairs.terrain = Some(cmap(&cat, "S_upstair"));
        let stairs = look_of(&on_stairs, Near::default(), &cat);
        let step = stairs
            .solids
            .iter()
            .filter(|s| matches!(s.mesh, MeshKey::Box(..)) && s.pos.z.abs() < 0.1)
            .map(|s| s.pos.y + s.mesh.half_height())
            .fold(0.0f32, f32::max);
        assert!(step > 0.2, "a step under the centre");
        assert!(stairs.ground >= step - 0.001);
        let body = stairs
            .solids
            .iter()
            .find(|s| matches!(s.mesh, MeshKey::Capsule(..)))
            .unwrap();
        assert!(body.pos.y - body.mesh.half_height() >= step - 0.001);
    }

    #[test]
    fn walls_in_front_of_open_ground_are_cut_down() {
        let cat = catalog();
        let feature = |sym: &str| Cell {
            glyph: Some(cmap(&cat, sym)),
            bk: None,
            terrain: Some(cmap(&cat, sym)),
        };
        let floor = feature("S_room");
        let stone = feature("S_stone");
        let top = |sym: &str, north: &Cell| {
            look_of(&feature(sym), Near([Some(north), None, None, None]), &cat).top
        };
        // the south wall of a room would hide the row behind it
        assert_eq!(top("S_hwall", &floor), CUT_HEIGHT);
        assert_eq!(top("S_hcdoor", &floor), CUT_HEIGHT);
        // a side wall below a doorway would hide whoever stands in it
        assert_eq!(top("S_vwall", &feature("S_ndoor")), CUT_HEIGHT);
        // walls with rock or wall behind stand
        assert_eq!(top("S_vwall", &feature("S_vwall")), WALL_HEIGHT);
        assert_eq!(top("S_vcdoor", &feature("S_vwall")), DOOR_HEIGHT);
        assert_eq!(top("S_hwall", &stone), WALL_HEIGHT);
        assert_eq!(top("S_hwall", &feature("S_hwall")), WALL_HEIGHT);
        assert_eq!(
            look_of(&feature("S_hwall"), Near::default(), &cat).top,
            WALL_HEIGHT
        );
        // on ground never shown, only what stays put cuts a wall: the hero
        // or an object, not a warning or a sensed monster passing by
        let unseen = |g: Glyph| Cell {
            glyph: Some(g),
            bk: None,
            terrain: None,
        };
        let hero = monster(&cat, "newt", mg::HERO);
        assert_eq!(top("S_hwall", &unseen(hero)), CUT_HEIGHT);
        let obj = glyph(GlyphKind::Obj, '(');
        assert_eq!(top("S_hwall", &unseen(obj)), CUT_HEIGHT);
        let warning = glyph(GlyphKind::Warning, '3');
        assert_eq!(top("S_hwall", &unseen(warning)), WALL_HEIGHT);
        let sensed = monster(&cat, "newt", mg::DETECT);
        assert_eq!(top("S_hwall", &unseen(sensed)), WALL_HEIGHT);
    }

    #[test]
    fn objects_are_small_cubes_and_piles_stack() {
        let cat = catalog();
        let one = look_of(
            &on_floor(&cat, glyph(GlyphKind::Obj, ')')),
            Near::default(),
            &cat,
        );
        let pile = Glyph {
            flags: mg::OBJPILE,
            ..glyph(GlyphKind::Obj, ')')
        };
        let pile = look_of(&on_floor(&cat, pile), Near::default(), &cat);
        let cubes = |l: &Look| {
            l.solids
                .iter()
                .filter(|s| matches!(s.mesh, MeshKey::Box(..)))
                .count()
        };
        assert_eq!(cubes(&one), 1);
        assert_eq!(cubes(&pile), 3);
        assert!(pile.top > one.top);
        assert_eq!(one.letters[0].ch, ')');
        assert!(!one.letters[0].on_top);
    }

    #[test]
    fn effects_flash_over_what_is_there() {
        let cat = catalog();
        let mut cell = on_floor(&cat, cmap(&cat, "S_vbeam"));
        let beam = look_of(&cell, Near::default(), &cat);
        assert!(beam.solids.iter().any(|s| s.finish == Finish::Glow));
        // the engulfer is drawn around the hero, not per cell
        cell.glyph = Some(glyph(GlyphKind::Swallow, '/'));
        assert_eq!(
            look_of(&cell, Near::default(), &cat).solids.len(),
            1,
            "only the floor"
        );
        // unexplored cells draw nothing; an unseen floor under a monster does
        assert!(look_of(&Cell::default(), Near::default(), &cat).is_empty());
        let under = Cell {
            glyph: Some(monster(&cat, "newt", 0)),
            bk: None,
            terrain: None,
        };
        assert_eq!(
            look_of(&under, Near::default(), &cat).solids[0].color,
            FLOOR_UNSEEN
        );
    }

    #[test]
    fn unseen_ground_under_something_takes_the_floor_around_it() {
        let cat = catalog();
        let ground = |sym: &str| Cell {
            glyph: Some(cmap(&cat, sym)),
            bk: None,
            terrain: Some(cmap(&cat, sym)),
        };
        let under = Cell {
            glyph: Some(glyph(GlyphKind::Obj, '(')),
            bk: None,
            terrain: None,
        };
        let tile = |near: [Option<&Cell>; 4]| look_of(&under, Near(near), &cat).solids[0];
        let (lit, dark, corr) = (ground("S_room"), ground("S_darkroom"), ground("S_corr"));
        let wall = ground("S_hwall");
        let alone = look_of(&ground("S_room"), Near::default(), &cat).solids[0];
        // an object in a lit room lies on the same floor as its neighbours
        let t = tile([Some(&wall), Some(&lit), Some(&lit), None]);
        assert_eq!((t.mesh, t.color), (alone.mesh, alone.color));
        // most neighbours decide; ties go to the lit floor
        assert_eq!(
            tile([Some(&dark), Some(&dark), Some(&lit), None]).color,
            FLOOR_DARK
        );
        assert_eq!(
            tile([Some(&dark), Some(&lit), None, None]).color,
            alone.color
        );
        let t = tile([None, None, Some(&corr), Some(&corr)]);
        assert_eq!(t.mesh, plane(0.8, 0.8));
        // walls alone say nothing about the floor
        assert_eq!(tile([Some(&wall), None, None, None]).color, FLOOR_UNSEEN);
    }
}
