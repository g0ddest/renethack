//! The 3D map: floors, walls and features built per cell with the art
//! manifest's PBR materials (triplanar in world space, so the stone runs on
//! across cells); models for monsters, the hero, objects and trees from the
//! art library; a perspective camera following the hero, a flickering torch
//! on the hero and a fill light over each part of the level in view. Cell
//! (x, y) is the point (x, 0, y); one cell is one metre.
//!
//! Each cell's look is computed as plain data (`Look`) and its nodes are
//! touched only when the look changes. Meshes, materials and model instances
//! are shared or pooled by the art library.

use std::collections::HashMap;

use godot::classes::base_material_3d::BillboardMode;
use godot::classes::environment::{AmbientSource, BgMode, FogMode, ToneMapper};
use godot::classes::label_3d::DrawFlags;
use godot::classes::light_3d::Param;
use godot::classes::{
    Camera3D, DirectionalLight3D, Environment, Label3D, MeshInstance3D, Node3D, OmniLight3D,
    SystemFont, WorldEnvironment,
};
use godot::prelude::*;
use nh_art::{ArtManifest, Tint};
use nh_protocol::{Catalog, Glyph, GlyphKind, MonsterInfo, ObjectTile, mg};
use nh_world::{COLNO, Cell, MapState, ROWNO, Terrain, World, cell_terrain, in_field, terrain_of};

use crate::art::{Art, Finish, Model, ModelLook, Pose, build_flat, no_shadow};
use crate::meshes::{MeshKey, cuboid, cylinder, plane, sphere, torus};
use crate::theme::{self, nh_color};

/// Low enough to see creatures from the side, high enough to see the floor
/// between walls.
const PITCH_DEG: f32 = 52.0;
const DISTANCE: f32 = 11.0;
const MIN_DISTANCE: f32 = 6.0;
const MAX_DISTANCE: f32 = 30.0;
/// The camera's vertical field of view, degrees.
const FOV_DEG: f32 = 50.0;
/// The overview never goes further (a whole 80x21 level fits well within).
const MAX_OVERVIEW_DISTANCE: f32 = 70.0;
const FOLLOW_RATE: f32 = 8.0;
/// Descent per step when a pointer ray is walked through the raised geometry.
const PICK_STEP: f32 = 0.05;
/// Nothing drawn reaches higher (a giant on an altar).
const MAX_TOP: f32 = 2.8;

const WALL_HEIGHT: f32 = 1.35;
const DOOR_HEIGHT: f32 = 1.15;
/// Walls and doors in front of open ground, seen from the camera's side.
const CUT_HEIGHT: f32 = 0.3;
/// Label3D font size; a letter's height is about `FONT_PX * pixel size`.
const FONT_PX: i32 = 96;
const PX_MONSTER: f32 = 0.0068;
const PX_FEATURE: f32 = 0.0062;
const PX_TRAP: f32 = 0.0052;
/// Where the camera aims, south of the hero at the default distance (the
/// log covers the bottom of the screen); it shrinks as the camera closes in.
const AIM_SOUTH: f32 = 1.5;

/// Brightness (%) of floors in view, and of the part of a room remembered.
const SHADE_LIT: u8 = 100;
const SHADE_DARK: u8 = 58;
/// A lying corpse is this much darker than the living monster.
const CORPSE_DARKEN: f32 = 0.45;

const FLOOR_UNSEEN: Color = Color::from_rgb(0.19, 0.19, 0.21);
const DEEP: Color = Color::from_rgb(0.02, 0.02, 0.03);
/// Scratches of an engraving on the floor.
const ENGRAVING: Color = Color::from_rgb(0.74, 0.71, 0.62);
const HERO_RING: Color = Color::from_rgba(1.0, 0.82, 0.30, 0.55);
const PET_RING: Color = Color::from_rgba(1.0, 0.45, 0.75, 0.6);
const TORCH: Color = Color::from_rgb(1.0, 0.78, 0.55);
const TORCH_ENERGY: f32 = 2.2;
const ROOM_LIGHT: Color = Color::from_rgb(1.0, 0.86, 0.68);

/// What covers a solid: a manifest material (index, brightness %) or a
/// plain colour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Paint {
    Pbr(usize, u8),
    Flat(Color, Finish),
}

/// One mesh of a cell, relative to the cell's centre on the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Solid {
    mesh: MeshKey,
    paint: Paint,
    pos: Vector3,
    /// Euler angles in degrees.
    rot: Vector3,
    /// Casts a shadow (not the floor).
    shadow: bool,
}

/// A billboard character over a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Letter {
    ch: char,
    color: Color,
    pos: Vector3,
    pixel_size: f32,
    /// Drawn over walls and everything else.
    on_top: bool,
}

/// A model on a cell: what (the art library's look), where, which way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    look: ModelLook,
    pos: Vector3,
    /// Degrees about y; 0 faces the camera (south).
    yaw: f32,
}

/// How a cell looks; its nodes are touched only when this changes.
#[derive(Debug, Clone, Default, PartialEq)]
struct Look {
    solids: Vec<Solid>,
    letters: Vec<Letter>,
    models: Vec<Placed>,
    /// Where things stand on this cell and where markers lie.
    ground: f32,
    /// The top of everything drawn (pointer picking).
    top: f32,
    /// A floor in view: part of a lit area.
    lit: bool,
}

impl Look {
    fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.letters.is_empty() && self.models.is_empty()
    }

    fn reach(&mut self, top: f32) {
        self.top = self.top.max(top).min(MAX_TOP);
    }

    fn add(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3, rot: Vector3) {
        let reach = match mesh {
            MeshKey::Capsule(r, _) if rot.x != 0.0 || rot.z != 0.0 => crate::meshes::metres(r),
            _ => mesh.half_height(),
        };
        self.reach(pos.y + reach);
        self.solids.push(Solid {
            mesh,
            paint,
            pos,
            rot,
            shadow: true,
        });
    }

    fn solid(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3) {
        self.add(mesh, paint, pos, Vector3::ZERO);
    }

    fn turned(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3, rot: Vector3) {
        self.add(mesh, paint, pos, rot);
    }

    /// Flat ground: no shadow of its own.
    fn ground_tile(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3) {
        self.add(mesh, paint, pos, Vector3::ZERO);
        if let Some(s) = self.solids.last_mut() {
            s.shadow = false;
        }
    }

    fn letter(&mut self, ch: char, color: Color, y: f32, pixel_size: f32, on_top: bool) {
        self.reach(y + FONT_PX as f32 * pixel_size / 2.0);
        self.letters.push(Letter {
            ch,
            color,
            pos: Vector3::new(0.0, y, 0.0),
            pixel_size,
            on_top,
        });
    }

    fn model(&mut self, look: ModelLook, pos: Vector3, yaw: f32) {
        let r = look.art;
        let top = if look.pose == Pose::Corpse {
            pos.y + (r.height * 0.4).min(0.5)
        } else {
            pos.y + r.lift + r.height
        };
        self.reach(top);
        self.models.push(Placed { look, pos, yaw });
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

fn monster_info(catalog: &Catalog, mon: Option<i32>) -> Option<&MonsterInfo> {
    catalog.monsters.get(usize::try_from(mon?).ok()?)
}

/// An object's appearance tile: all the client knows of an object.
fn object_tile(catalog: &Catalog, tile: i32) -> Option<&ObjectTile> {
    catalog.object_tiles.iter().find(|t| t.tile == tile)
}

/// A stable pseudo-random number in 0..1 for a cell (and a salt).
fn cell_noise(x: i32, y: i32, salt: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// The colour a look multiplies its model by.
fn tint_color(tint: Tint, glyph_color: i32) -> Color {
    match tint {
        Tint::None => Color::WHITE,
        Tint::Glyph(s) => mix(Color::WHITE, nh_color(glyph_color), s),
        Tint::Rgb(c, s) => mix(Color::WHITE, Color::from_rgb(c[0], c[1], c[2]), s),
    }
}

/// What a cell's look depends on besides the cell and its neighbours.
struct Ctx<'a> {
    catalog: &'a Catalog,
    art: &'a ArtManifest,
    x: i32,
    y: i32,
    /// Where the hero is (monsters face them).
    hero: Option<(i32, i32)>,
    /// Which way the hero faces (degrees about y).
    hero_yaw: f32,
}

impl Ctx<'_> {
    fn noise(&self, salt: u32) -> f32 {
        cell_noise(self.x, self.y, salt)
    }

    /// Degrees about y that turn a model at this cell towards the hero; a
    /// little turned towards the camera when the hero is far or unknown.
    fn face_hero(&self) -> f32 {
        let jitter = (self.noise(7) - 0.5) * 50.0;
        match self.hero {
            Some((hx, hy)) if (hx, hy) != (self.x, self.y) => {
                let (dx, dz) = ((hx - self.x) as f32, (hy - self.y) as f32);
                if dx.abs() + dz.abs() > 12.0 {
                    jitter
                } else {
                    dx.atan2(dz).to_degrees()
                }
            }
            _ => jitter,
        }
    }

    fn mat(&self, name: &str) -> Option<usize> {
        self.art.material(name)
    }
}

/// Floor, walls and features. Sets `ground`. `cut`: open ground lies north
/// of the cell (the row behind a room's south wall, a doorway above a side
/// wall), so a wall or door here would hide it from the camera and is
/// drawn low.
fn terrain_look(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, cut: bool, ctx: &Ctx) {
    terrain_base(look, t, sym, g, cut, ctx);
    if engraved(sym) {
        for (x, z, yaw) in [(-0.06, -0.12, 18.0), (0.04, 0.02, -24.0), (0.0, 0.16, 8.0)] {
            let mesh = cuboid(0.46, 0.012, 0.035);
            let paint = Paint::Flat(ENGRAVING, Finish::Matte);
            look.turned(mesh, paint, at(x, 0.006, z), at(0.0, yaw, 0.0));
        }
    }
}

/// Something is engraved here (in a room or a corridor).
fn engraved(sym: &str) -> bool {
    matches!(sym, "S_engroom" | "S_engrcorr")
}

fn terrain_base(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, cut: bool, ctx: &Ctx) {
    let c = g.color;
    let art = ctx.art.terrain(t);
    // "S_v..." features sit in a vertical wall: the passage runs along x
    let vertical = sym.starts_with("S_v");
    let (wall_h, door_h) = if cut {
        (CUT_HEIGHT, CUT_HEIGHT)
    } else {
        (WALL_HEIGHT, DOOR_HEIGHT)
    };
    // in view: the floor's full brightness with a little variation per
    // cell; remembered, dark
    let lit_shade = SHADE_LIT - (ctx.noise(1) * 10.0) as u8;
    let floor_mat = ctx.mat("floor");
    let pbr = |m: Option<usize>, shade: u8, fallback: Color| match m {
        Some(m) => Paint::Pbr(m, shade),
        None => Paint::Flat(fallback, Finish::Matte),
    };
    let main = |shade: u8| pbr(art.material, shade, FLOOR_UNSEEN);
    let trim = |shade: u8| pbr(art.trim, shade, DEEP);
    let tile = plane(1.0, 1.0);
    let floor = |look: &mut Look| {
        look.ground_tile(tile, pbr(floor_mat, lit_shade, FLOOR_UNSEEN), Vector3::ZERO);
    };
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
                main(SHADE_LIT - (ctx.noise(2) * 8.0) as u8),
                at(0.0, wall_h / 2.0, 0.0),
            );
            look.ground = wall_h;
        }
        Terrain::Floor => {
            look.ground_tile(tile, main(lit_shade), Vector3::ZERO);
            look.lit = true;
        }
        Terrain::DarkFloor => look.ground_tile(tile, main(SHADE_DARK), Vector3::ZERO),
        Terrain::Corridor => {
            let lit = sym == "S_litcorr";
            let shade = if lit { lit_shade } else { 85 };
            look.ground_tile(tile, main(shade), Vector3::ZERO);
            look.lit = lit;
        }
        Terrain::Doorway => {
            floor(look);
            // a worn wooden threshold across the passage
            let sill = if vertical {
                cuboid(0.16, 0.03, 0.9)
            } else {
                cuboid(0.9, 0.03, 0.16)
            };
            look.ground_tile(sill, trim(80), at(0.0, 0.015, 0.0));
        }
        Terrain::BrokenDoor => {
            floor(look);
            let wood = main(85);
            look.turned(
                cuboid(0.34, 0.05, 0.08),
                wood,
                at(0.2, 0.025, 0.24),
                at(0.0, 30.0, 0.0),
            );
            look.turned(
                cuboid(0.26, 0.05, 0.07),
                wood,
                at(-0.2, 0.025, -0.2),
                at(0.0, -50.0, 0.0),
            );
        }
        Terrain::OpenDoor => {
            floor(look);
            // the leaf stands open against the side of the passage
            let (mesh, pos, band) = if vertical {
                (
                    cuboid(0.8, door_h, 0.08),
                    at(0.0, door_h / 2.0, -0.44),
                    cuboid(0.82, 0.06, 0.1),
                )
            } else {
                (
                    cuboid(0.08, door_h, 0.8),
                    at(-0.44, door_h / 2.0, 0.0),
                    cuboid(0.1, 0.06, 0.82),
                )
            };
            look.solid(mesh, main(SHADE_LIT), pos);
            for h in [0.25, 0.75] {
                look.solid(band, trim(SHADE_LIT), at(pos.x, door_h * h, pos.z));
            }
        }
        Terrain::ClosedDoor => {
            floor(look);
            let (mesh, band) = if vertical {
                (cuboid(0.18, door_h, 0.96), cuboid(0.2, 0.07, 0.98))
            } else {
                (cuboid(0.96, door_h, 0.18), cuboid(0.98, 0.07, 0.2))
            };
            look.solid(mesh, main(SHADE_LIT), at(0.0, door_h / 2.0, 0.0));
            for h in [0.22, 0.78] {
                look.solid(band, trim(SHADE_LIT), at(0.0, door_h * h, 0.0));
            }
            look.ground = door_h;
        }
        Terrain::IronBars => {
            look.ground_tile(
                tile,
                pbr(floor_mat, SHADE_DARK, FLOOR_UNSEEN),
                Vector3::ZERO,
            );
            let iron = main(SHADE_LIT);
            let bar = cylinder(0.03, 0.03, WALL_HEIGHT);
            let y = WALL_HEIGHT / 2.0;
            for (x, z) in [(-0.3, 0.0), (0.0, 0.0), (0.3, 0.0), (0.0, -0.3), (0.0, 0.3)] {
                look.solid(bar, iron, at(x, y, z));
            }
            let top = WALL_HEIGHT - 0.05;
            look.solid(cuboid(0.9, 0.06, 0.06), iron, at(0.0, top, 0.0));
            look.solid(cuboid(0.06, 0.06, 0.9), iron, at(0.0, top, 0.0));
        }
        Terrain::Tree => {
            look.ground_tile(tile, main(80), Vector3::ZERO);
            if let Some(tree) = art.model {
                let look_ = ModelLook {
                    art: tree,
                    tint: Color::WHITE,
                    pose: Pose::Alive,
                };
                look.model(look_, Vector3::ZERO, ctx.noise(3) * 360.0);
            }
        }
        Terrain::StairsUp => {
            floor(look);
            let stone = main(SHADE_LIT);
            // five steps rising to the north, between two side walls
            for i in 0..5 {
                let h = 0.09 * (i + 1) as f32;
                let z = 0.36 - 0.18 * i as f32;
                look.solid(cuboid(0.78, h, 0.18), stone, at(0.0, h / 2.0, z));
            }
            for x in [-0.44f32, 0.44] {
                look.solid(cuboid(0.1, 0.5, 0.92), main(80), at(x, 0.25, 0.0));
            }
            // whoever stands here stands on the middle step
            look.ground = 0.27;
            label(look, 0.95);
        }
        Terrain::StairsDown => {
            // a pit with steps going down, away from the camera, in a rim
            let stone = main(SHADE_LIT);
            let rim = main(80);
            for (mesh, x, z) in [
                (cuboid(1.0, 0.06, 0.1), 0.0, -0.45),
                (cuboid(1.0, 0.06, 0.1), 0.0, 0.45),
                (cuboid(0.1, 0.06, 0.8), -0.45, 0.0),
                (cuboid(0.1, 0.06, 0.8), 0.45, 0.0),
            ] {
                look.solid(mesh, rim, at(x, 0.03, z));
            }
            look.solid(
                cuboid(0.8, 0.6, 0.04),
                Paint::Flat(DEEP, Finish::Matte),
                at(0.0, -0.3, -0.38),
            );
            for (i, z) in [0.23f32, 0.0, -0.23].into_iter().enumerate() {
                let top = -0.12 * (i + 1) as f32;
                let h = top + 0.6;
                let pos = at(0.0, top - h / 2.0, z);
                look.solid(cuboid(0.8, h, 0.23), stone, pos);
            }
            label(look, 0.7);
        }
        Terrain::LadderUp | Terrain::LadderDown => {
            let up = t == Terrain::LadderUp;
            floor(look);
            if !up {
                look.ground_tile(
                    plane(0.6, 0.6),
                    Paint::Flat(DEEP, Finish::Matte),
                    at(0.0, 0.005, 0.0),
                );
            }
            let wood = main(SHADE_LIT);
            let h = if up { 1.3 } else { 0.5 };
            let rail = cylinder(0.03, 0.03, h);
            for x in [-0.2, 0.2] {
                look.solid(rail, wood, at(x, h / 2.0, -0.2));
            }
            let mut y = 0.2;
            while y < h {
                look.solid(cuboid(0.4, 0.03, 0.03), wood, at(0.0, y, -0.2));
                y += 0.3;
            }
            label(look, if up { 1.6 } else { 0.8 });
        }
        Terrain::Altar => {
            floor(look);
            let stone = main(90);
            look.solid(cuboid(0.76, 0.45, 0.56), stone, at(0.0, 0.225, 0.0));
            look.solid(cuboid(0.9, 0.06, 0.7), main(SHADE_LIT), at(0.0, 0.48, 0.0));
            // the altar's alignment colour as a runner cloth
            let cloth = Paint::Flat(darker(nh_color(c), 0.35), Finish::Matte);
            look.solid(cuboid(0.3, 0.012, 0.72), cloth, at(0.0, 0.516, 0.0));
            look.ground = 0.51;
        }
        Terrain::Throne => {
            floor(look);
            let gold = main(SHADE_LIT);
            look.solid(cuboid(0.62, 0.2, 0.6), trim(SHADE_LIT), at(0.0, 0.1, 0.0));
            look.solid(cuboid(0.6, 0.18, 0.55), gold, at(0.0, 0.29, 0.02));
            look.solid(cuboid(0.6, 0.8, 0.12), gold, at(0.0, 0.78, -0.24));
            for x in [-0.28, 0.28] {
                look.solid(cuboid(0.07, 0.22, 0.5), gold, at(x, 0.49, 0.0));
            }
            let velvet = Paint::Flat(Color::from_rgb(0.35, 0.04, 0.06), Finish::Matte);
            look.solid(cuboid(0.48, 0.03, 0.45), velvet, at(0.0, 0.395, 0.04));
            look.ground = 0.38;
        }
        Terrain::Fountain => {
            floor(look);
            let stone = main(SHADE_LIT);
            look.solid(cylinder(0.44, 0.47, 0.28), stone, at(0.0, 0.14, 0.0));
            look.solid(
                cylinder(0.38, 0.38, 0.02),
                trim(SHADE_LIT),
                at(0.0, 0.27, 0.0),
            );
            look.solid(cylinder(0.05, 0.08, 0.45), stone, at(0.0, 0.5, 0.0));
            look.solid(cylinder(0.14, 0.06, 0.08), stone, at(0.0, 0.74, 0.0));
            look.ground = 0.28;
        }
        Terrain::Sink => {
            floor(look);
            let stone = main(SHADE_LIT);
            look.solid(cuboid(0.6, 0.4, 0.5), stone, at(0.0, 0.2, 0.0));
            look.solid(
                plane(0.44, 0.34),
                Paint::Flat(DEEP, Finish::Glossy),
                at(0.0, 0.405, 0.0),
            );
            let metal = pbr(ctx.mat("metal"), SHADE_LIT, DEEP);
            look.solid(cylinder(0.02, 0.02, 0.2), metal, at(0.0, 0.5, -0.2));
            look.ground = 0.4;
        }
        Terrain::Grave => {
            look.ground_tile(tile, main(75), Vector3::ZERO);
            look.solid(cuboid(0.5, 0.12, 0.62), main(60), at(0.0, 0.06, 0.12));
            let stone = trim(80);
            look.turned(
                cuboid(0.48, 0.6, 0.1),
                stone,
                at(0.0, 0.3, -0.32),
                at(-6.0, 0.0, 3.0),
            );
            look.turned(
                cylinder(0.24, 0.24, 0.1),
                stone,
                at(0.0, 0.6, -0.33),
                at(84.0, 0.0, 3.0),
            );
            look.ground = 0.12;
        }
        Terrain::Pool | Terrain::Water => {
            let deep = if t == Terrain::Water { 0.1 } else { 0.06 };
            look.ground_tile(plane(1.0, 1.0), main(SHADE_LIT), at(0.0, -deep, 0.0));
            look.ground = -deep;
        }
        Terrain::Ice => look.ground_tile(tile, main(SHADE_LIT), Vector3::ZERO),
        Terrain::Lava => {
            look.ground_tile(plane(1.0, 1.0), main(SHADE_LIT), at(0.0, -0.04, 0.0));
            look.ground = -0.04;
        }
        Terrain::LavaWall => {
            let mesh = cuboid(1.0, wall_h, 1.0);
            look.solid(mesh, main(SHADE_LIT), at(0.0, wall_h / 2.0, 0.0));
            look.ground = wall_h;
        }
        Terrain::DrawbridgeDown => {
            look.solid(cuboid(1.0, 0.08, 1.0), main(SHADE_LIT), at(0.0, 0.04, 0.0));
            // two iron beams along the span
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
                look.solid(mesh, trim(SHADE_LIT), pos);
            }
            look.ground = 0.08;
        }
        Terrain::DrawbridgeUp => {
            look.ground_tile(plane(1.0, 1.0), trim(SHADE_LIT), at(0.0, -0.06, 0.0));
            let mesh = if vertical {
                cuboid(0.2, 1.1, 0.96)
            } else {
                cuboid(0.96, 1.1, 0.2)
            };
            look.solid(mesh, main(SHADE_LIT), at(0.0, 0.55, 0.0));
            look.ground = 1.1;
        }
        Terrain::Air => {
            let air = lighter(nh_color(c), 0.4);
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Flat(air, Finish::Ghost),
                Vector3::ZERO,
            );
        }
        Terrain::Cloud => {
            let cloud = lighter(nh_color(c), 0.2);
            look.solid(
                cuboid(0.96, 0.6, 0.96),
                Paint::Flat(cloud, Finish::Ghost),
                at(0.0, 0.3, 0.0),
            );
        }
        Terrain::Trap => {
            floor(look);
            let mark = darker(nh_color(c), 0.45);
            look.ground_tile(
                cylinder(0.34, 0.34, 0.02),
                Paint::Flat(mark, Finish::Matte),
                at(0.0, 0.01, 0.0),
            );
            if let Some(ch) = glyph_char(g) {
                look.letter(ch, nh_color(c), 0.3, PX_TRAP, false);
            }
        }
    }
}

/// Monsters, objects and the like on top of the terrain.
fn entity_look(look: &mut Look, g: &Glyph, ctx: &Ctx) {
    let ground = look.ground;
    let color = nh_color(g.color);
    let ch = glyph_char(g);
    let art = ctx.art;
    let here = at(0.0, ground, 0.0);
    match g.kind {
        GlyphKind::Mon => {
            let hero = g.flags & mg::HERO != 0;
            if let Some(info) = monster_info(ctx.catalog, g.mon) {
                let r = art.monster(info, g.flags);
                let pose = if g.flags & (mg::DETECT | mg::INVIS) != 0 {
                    Pose::Ghost
                } else {
                    Pose::Alive
                };
                let tint = tint_color(r.tint, g.color);
                let yaw = if hero { ctx.hero_yaw } else { ctx.face_hero() };
                look.model(ModelLook { art: r, tint, pose }, here, yaw);
            }
            if g.flags & mg::PET != 0 {
                let ring = torus(0.36, 0.42);
                let paint = Paint::Flat(PET_RING, Finish::Flat);
                look.ground_tile(ring, paint, at(0.0, ground + 0.02, 0.0));
            }
        }
        GlyphKind::Invisible => {
            let gray = Color::from_rgb(0.7, 0.72, 0.8);
            look.solid(
                sphere(0.3),
                Paint::Flat(gray, Finish::Ghost),
                at(0.0, ground + 0.35, 0.0),
            );
            if let Some(ch) = ch {
                look.letter(ch, gray, ground + 0.8, PX_MONSTER, true);
            }
        }
        GlyphKind::Warning => {
            if let Some(ch) = ch {
                look.letter(ch, color, ground + 0.5, PX_MONSTER, true);
            }
        }
        GlyphKind::Obj => {
            let Some(tile) = object_tile(ctx.catalog, g.tile) else {
                return;
            };
            let r = art.object(tile);
            let yaw = ctx.noise(5) * 360.0;
            let mut pos = here;
            if g.flags & mg::OBJPILE != 0
                && let Some(heap) = art.model_index("heap")
            {
                let spec = art.model_at(heap).1;
                let target = spec.target.unwrap_or(0.5);
                let heap_art = nh_art::Resolved {
                    model: heap,
                    scale: target / spec.size,
                    lift: 0.0,
                    rot: [0.0; 3],
                    height: 0.12,
                    tint: Tint::None,
                    skin: nh_art::Skin::Own,
                    level: nh_art::Level::Generic,
                };
                let heap_look = ModelLook {
                    art: heap_art,
                    tint: Color::WHITE,
                    pose: Pose::Alive,
                };
                look.model(heap_look, here, yaw + 40.0);
                pos.y += 0.1;
            }
            let tint = tint_color(r.tint, g.color);
            look.model(
                ModelLook {
                    art: r,
                    tint,
                    pose: Pose::Alive,
                },
                pos,
                yaw,
            );
        }
        GlyphKind::Body => {
            if let Some(info) = monster_info(ctx.catalog, g.mon) {
                let r = art.monster(info, g.flags);
                let tint = darker(tint_color(r.tint, g.color), CORPSE_DARKEN);
                let yaw = ctx.noise(6) * 360.0;
                look.model(
                    ModelLook {
                        art: r,
                        tint,
                        pose: Pose::Corpse,
                    },
                    here,
                    yaw,
                );
            }
        }
        GlyphKind::Statue => {
            let plinth = 0.1;
            let stone = match art.material("marble") {
                Some(m) => Paint::Pbr(m, 70),
                None => Paint::Flat(Color::from_rgb(0.5, 0.5, 0.52), Finish::Matte),
            };
            look.solid(
                cuboid(0.6, plinth, 0.6),
                stone,
                at(0.0, ground + plinth / 2.0, 0.0),
            );
            let r = art.statue(monster_info(ctx.catalog, g.mon), g.flags);
            let yaw = ctx.face_hero();
            let pos = at(0.0, ground + plinth, 0.0);
            look.model(
                ModelLook {
                    art: r,
                    tint: Color::WHITE,
                    pose: Pose::Statue,
                },
                pos,
                yaw,
            );
        }
        GlyphKind::Zap | GlyphKind::Explosion => bright_flash(look, g),
        // the engulfer is drawn once, around the hero
        _ => {}
    }
}

/// A beam, explosion or sparkle: a glowing ball for the moment it shows.
fn bright_flash(look: &mut Look, g: &Glyph) {
    let color = lighter(nh_color(g.color), 0.3);
    let y = look.ground.max(0.0) + 0.5;
    look.solid(
        sphere(0.28),
        Paint::Flat(color, Finish::Glow),
        at(0.0, y, 0.0),
    );
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
        // only what stands on open ground: the hero, a monster in sight, a
        // remembered object; a sensed monster or a warning passing through
        // rock would make walls flicker
        None => cell.entity().is_some_and(|g| {
            g.flags & mg::HERO != 0
                || (g.kind == GlyphKind::Mon && g.flags & mg::DETECT == 0)
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
fn look_of(cell: &Cell, near: Near, ctx: &Ctx) -> Look {
    let catalog = ctx.catalog;
    let mut look = Look::default();
    match &cell.terrain {
        Some(t) => {
            let sym = cmap_sym(t, catalog).unwrap_or("");
            let cut = is_open(near.north(), catalog);
            terrain_look(&mut look, terrain_of(sym), sym, t, cut, ctx);
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
                terrain_look(&mut look, terrain_of(sym), sym, g, false, ctx);
            }
            None => {
                let paint = Paint::Flat(FLOOR_UNSEEN, Finish::Matte);
                look.ground_tile(plane(1.0, 1.0), paint, Vector3::ZERO);
            }
        },
        None => {}
    }
    match &cell.glyph {
        Some(g) if g.kind == GlyphKind::Cmap => {
            if cmap_sym(g, catalog).is_some_and(|s| terrain_of(s) == Terrain::Effect) {
                bright_flash(&mut look, g);
            }
        }
        Some(g) => entity_look(&mut look, g, ctx),
        None => {}
    }
    look
}

#[derive(Default)]
struct CellNodes {
    look: Look,
    solids: Vec<Gd<MeshInstance3D>>,
    letters: Vec<Gd<Label3D>>,
    models: Vec<Model>,
}

impl CellNodes {
    fn free(self, art: &mut Art) {
        for mut n in self.solids {
            n.queue_free();
        }
        for mut n in self.letters {
            n.queue_free();
        }
        for m in self.models {
            art.give(m);
        }
    }
}

pub struct MapView {
    root: Gd<Node3D>,
    cells_root: Gd<Node3D>,
    camera: Gd<Camera3D>,
    cells: HashMap<(i32, i32), CellNodes>,
    generation: Option<u64>,
    art: Art,
    font: Gd<SystemFont>,
    hover: Gd<Node3D>,
    cursor: Gd<Node3D>,
    hero_ring: Gd<MeshInstance3D>,
    torch: Gd<OmniLight3D>,
    /// Fill lights over the parts of the level in view.
    room_lights: Vec<Gd<OmniLight3D>>,
    /// The lit areas changed: place the fill lights again.
    lights_dirty: bool,
    engulf: Gd<MeshInstance3D>,
    engulf_mat: Gd<godot::classes::StandardMaterial3D>,
    target: Vector3,
    focus: Vector3,
    distance: f32,
    /// The whole-level view is on: its distance, refitted as cells appear.
    overview: Option<f32>,
    snap: bool,
    /// Seconds since the start (the torch's flicker).
    clock: f64,
    /// Where the hero was and which way they face.
    hero_at: Option<(i32, i32)>,
    hero_yaw: f32,
}

/// A square outline over a cell: four thin bars.
fn frame(root: &mut Gd<Node3D>, color: Color, width: f32, height: f32) -> Gd<Node3D> {
    let mut node = Node3D::new_alloc();
    let mat = build_flat(color, Finish::Flat);
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
        no_shadow(&mut mi);
        node.add_child(&mi);
    }
    node.set_visible(false);
    root.add_child(&node);
    node
}

/// The torch's brightness at time `t`: a few slow waves and a quick one.
fn flicker(t: f64) -> f32 {
    let t = t as f32;
    1.0 + 0.07 * (t * 7.3).sin() + 0.05 * (t * 13.7 + 1.3).sin() + 0.03 * (t * 29.1 + 0.4).sin()
}

impl MapView {
    pub fn new(mut root: Gd<Node3D>) -> MapView {
        let mut env = Environment::new_gd();
        env.set_background(BgMode::COLOR);
        env.set_bg_color(Color::from_rgb(0.012, 0.012, 0.018));
        env.set_ambient_source(AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.5, 0.53, 0.68));
        env.set_ambient_light_energy(0.55);
        env.set_tonemapper(ToneMapper::ACES);
        env.set_tonemap_exposure(1.1);
        env.set_fog_enabled(true);
        env.set_fog_mode(FogMode::DEPTH);
        env.set_fog_light_color(Color::from_rgb(0.012, 0.012, 0.02));
        env.set_fog_density(1.0);
        env.set_fog_depth_begin(16.0);
        env.set_fog_depth_end(46.0);
        env.set_glow_enabled(true);
        env.set_glow_intensity(0.7);
        env.set_glow_bloom(0.02);
        env.set_glow_hdr_bleed_threshold(1.2);
        let mut world_env = WorldEnvironment::new_alloc();
        world_env.set_environment(&env);
        root.add_child(&world_env);

        // a faint cold light from above: walls and shapes stay readable
        // outside the torch's reach
        let mut moon = DirectionalLight3D::new_alloc();
        moon.set_rotation_degrees(Vector3::new(-62.0, 25.0, 0.0));
        moon.set_color(Color::from_rgb(0.62, 0.68, 0.9));
        moon.set_param(Param::ENERGY, 0.4);
        root.add_child(&moon);

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
        hero_ring.set_mesh(&torus(0.38, 0.44).build());
        hero_ring.set_material_override(&build_flat(HERO_RING, Finish::Flat));
        hero_ring.set_visible(false);
        no_shadow(&mut hero_ring);
        root.add_child(&hero_ring);

        let mut torch = OmniLight3D::new_alloc();
        torch.set_color(TORCH);
        torch.set_param(Param::ENERGY, TORCH_ENERGY);
        torch.set_param(Param::RANGE, 7.5);
        torch.set_param(Param::ATTENUATION, 1.1);
        torch.set_shadow(true);
        torch.set_visible(false);
        root.add_child(&torch);

        let engulf_mat = godot::classes::StandardMaterial3D::new_gd();
        let mut engulf = MeshInstance3D::new_alloc();
        engulf.set_mesh(&sphere(1.3).build());
        engulf.set_visible(false);
        root.add_child(&engulf);
        let mut engulf_mat = engulf_mat;
        engulf_mat.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA);
        engulf_mat.set_roughness(0.4);
        engulf.set_material_override(&engulf_mat);

        let art = Art::new(cells_root.clone());
        let center = Vector3::new(COLNO as f32 / 2.0, 0.0, ROWNO as f32 / 2.0);
        let mut view = MapView {
            root,
            cells_root,
            camera,
            cells: HashMap::new(),
            generation: None,
            art,
            font: theme::mono_bold(),
            hover,
            cursor,
            hero_ring,
            torch,
            room_lights: Vec::new(),
            lights_dirty: false,
            engulf,
            engulf_mat,
            target: center,
            focus: center,
            distance: DISTANCE,
            overview: None,
            snap: true,
            clock: 0.0,
            hero_at: None,
            hero_yaw: 0.0,
        };
        view.place_camera();
        view
    }

    /// Rebuild on a new generation, else apply dirty cells; camera target =
    /// view_center, else hero; cursor marker when World.cursor differs from
    /// the hero, or getpos moves it. The hero ring marks `World::hero`,
    /// also when the hero is not drawn (invisible).
    pub fn sync(&mut self, world: &mut World, catalog: &Catalog, delta: f64) {
        self.clock += delta;
        let hero = world.hero();
        // the hero faces the way they last stepped
        if let (Some((x, y)), Some((px, py))) = (hero, self.hero_at)
            && (x, y) != (px, py)
            && (x - px).abs() <= 1
            && (y - py).abs() <= 1
        {
            self.hero_yaw = ((x - px) as f32).atan2((y - py) as f32).to_degrees();
        }
        self.hero_at = hero;
        let generation = world.map.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            world.map.take_dirty();
            for y in 0..ROWNO {
                for x in 1..COLNO {
                    self.update_cell(x, y, world, catalog, hero);
                }
            }
            self.snap = true;
            self.lights_dirty = true;
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
                self.update_cell(x, y, world, catalog, hero);
            }
        }
        if std::mem::take(&mut self.lights_dirty) {
            self.place_room_lights();
        }
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
                self.hero_ring.set_position(p + at(0.0, 0.02, 0.0));
                self.hero_ring.set_visible(true);
                // held up and a little in front, towards the camera
                self.torch.set_position(p + at(0.25, 1.55, 0.35));
                self.torch
                    .set_param(Param::ENERGY, TORCH_ENERGY * flicker(self.clock));
                self.torch.set_visible(true);
            }
            None => {
                self.hero_ring.set_visible(false);
                self.torch.set_visible(false);
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

    /// One soft light over each connected area of floor in view.
    fn place_room_lights(&mut self) {
        let lit: std::collections::HashSet<(i32, i32)> = self
            .cells
            .iter()
            .filter(|(_, n)| n.look.lit)
            .map(|(&k, _)| k)
            .collect();
        let areas = lit_areas(&lit);
        let mut used = 0;
        for area in areas.iter().filter(|a| a.len() >= 4) {
            let n = area.len() as f32;
            let cx = area.iter().map(|c| c.0 as f32).sum::<f32>() / n;
            let cy = area.iter().map(|c| c.1 as f32).sum::<f32>() / n;
            let reach = area
                .iter()
                .map(|c| ((c.0 as f32 - cx).powi(2) + (c.1 as f32 - cy).powi(2)).sqrt())
                .fold(0.0f32, f32::max);
            if used == self.room_lights.len() {
                let mut l = OmniLight3D::new_alloc();
                l.set_color(ROOM_LIGHT);
                l.set_shadow(false);
                l.set_param(Param::ATTENUATION, 0.5);
                self.root.add_child(&l);
                self.room_lights.push(l);
            }
            let l = &mut self.room_lights[used];
            used += 1;
            let height = (2.2 + reach * 0.25).min(4.5);
            l.set_position(Vector3::new(cx, height, cy));
            l.set_param(Param::RANGE, (reach * 1.35 + 2.5).clamp(3.5, 16.0));
            l.set_param(Param::ENERGY, 0.75);
            l.set_visible(true);
        }
        for l in self.room_lights.iter_mut().skip(used) {
            l.set_visible(false);
        }
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
            nodes.free(&mut self.art);
        }
        self.generation = None;
        self.hover.set_visible(false);
        self.cursor.set_visible(false);
        self.hero_ring.set_visible(false);
        self.torch.set_visible(false);
        for l in &mut self.room_lights {
            l.set_visible(false);
        }
        self.engulf.set_visible(false);
        self.snap = true;
        self.hero_at = None;
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
    }

    /// The camera has caught up with its target (self-test screenshots).
    pub fn is_settled(&self) -> bool {
        self.focus.distance_to(self.target) < 0.05
    }

    /// Load art ahead of need for a few milliseconds (every frame, also
    /// before a game starts); false when everything is loaded.
    pub fn preload_step(&mut self) -> bool {
        self.art.preload_step()
    }

    /// The map generation last drawn (self-tests wait for a redraw).
    pub fn drawn_generation(&self) -> Option<u64> {
        self.generation
    }

    /// Cells with anything drawn (self-tests check the map is drawn).
    pub fn drawn_cells(&self) -> usize {
        self.cells.values().filter(|c| !c.look.is_empty()).count()
    }

    /// Models on the map now, and model instances ever built (self-tests
    /// check the pool reuses them).
    pub fn model_counts(&self) -> (usize, usize) {
        self.art.counts()
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

    fn update_cell(
        &mut self,
        x: i32,
        y: i32,
        world: &World,
        catalog: &Catalog,
        hero: Option<(i32, i32)>,
    ) {
        let ctx = Ctx {
            catalog,
            art: self.art.manifest(),
            x,
            y,
            hero,
            hero_yaw: self.hero_yaw,
        };
        let look = world
            .map
            .cell(x, y)
            .map(|c| look_of(c, Near::of(&world.map, x, y), &ctx))
            .unwrap_or_default();
        match self.cells.get(&(x, y)) {
            Some(n) if n.look == look => return,
            None if look.is_empty() => return,
            _ => {}
        }
        let mut nodes = self.cells.remove(&(x, y)).unwrap_or_default();
        let old = std::mem::take(&mut nodes.look);
        if old.lit != look.lit {
            self.lights_dirty = true;
        }
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
                mi.set_mesh(&self.art.mesh(s.mesh));
            }
            if before.map(|b| b.paint) != Some(s.paint) {
                let mat = match s.paint {
                    Paint::Pbr(m, shade) => self.art.surface(m, shade, true, Color::WHITE),
                    Paint::Flat(c, f) => self.art.flat(c, f),
                };
                mi.set_material_override(&mat);
            }
            if before.map(|b| b.shadow) != Some(s.shadow) {
                use godot::classes::geometry_instance_3d::ShadowCastingSetting as S;
                mi.set_cast_shadows_setting(if s.shadow { S::ON } else { S::OFF });
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
        // models: kept when the look is the same (only moved), else given
        // back to the pool and taken anew
        let mut kept: Vec<Model> = Vec::with_capacity(look.models.len());
        let mut old_models = std::mem::take(&mut nodes.models).into_iter();
        for (i, p) in look.models.iter().enumerate() {
            let before = old.models.get(i);
            let reuse = old_models.next();
            let mut m = match (before, reuse) {
                (Some(b), Some(m)) if b.look == p.look => m,
                (_, other) => {
                    if let Some(m) = other {
                        self.art.give(m);
                    }
                    self.art.take(&p.look)
                }
            };
            if before.map(|b| (b.pos, b.yaw, b.look)) != Some((p.pos, p.yaw, p.look)) {
                let basis =
                    Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, p.yaw.to_radians(), 0.0));
                m.node
                    .set_transform(Transform3D::new(basis, origin + p.pos));
            }
            kept.push(m);
        }
        for m in old_models {
            self.art.give(m);
        }
        nodes.models = kept;
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
        l.set_cast_shadows_setting(godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF);
        l
    }
}

/// Connected (4-neighbour) areas of lit cells.
fn lit_areas(lit: &std::collections::HashSet<(i32, i32)>) -> Vec<Vec<(i32, i32)>> {
    let mut seen = std::collections::HashSet::new();
    let mut areas = Vec::new();
    let mut cells: Vec<_> = lit.iter().copied().collect();
    cells.sort_unstable();
    for start in cells {
        if !seen.insert(start) {
            continue;
        }
        let mut area = vec![start];
        let mut i = 0;
        while i < area.len() {
            let (x, y) = area[i];
            for n in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if lit.contains(&n) && seen.insert(n) {
                    area.push(n);
                }
            }
            i += 1;
        }
        areas.push(area);
    }
    areas
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

    fn manifest() -> ArtManifest {
        ArtManifest::parse(include_str!("../../../godot/art/manifest.json")).unwrap()
    }

    struct Fixture {
        cat: Catalog,
        art: ArtManifest,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                cat: catalog(),
                art: manifest(),
            }
        }

        fn ctx(&self) -> Ctx<'_> {
            self.ctx_at(10, 10, None)
        }

        fn ctx_at(&self, x: i32, y: i32, hero: Option<(i32, i32)>) -> Ctx<'_> {
            Ctx {
                catalog: &self.cat,
                art: &self.art,
                x,
                y,
                hero,
                hero_yaw: 0.0,
            }
        }

        fn look(&self, cell: &Cell) -> Look {
            look_of(cell, Near::default(), &self.ctx())
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

    fn object(cat: &Catalog, class: &str, appearance: &str) -> Glyph {
        let t = cat
            .object_tiles
            .iter()
            .find(|t| t.class == class && t.appearance == appearance)
            .unwrap();
        Glyph {
            tile: t.tile,
            ..glyph(GlyphKind::Obj, class.chars().next().unwrap())
        }
    }

    fn feature(cat: &Catalog, sym: &str) -> Cell {
        Cell {
            glyph: Some(cmap(cat, sym)),
            bk: None,
            terrain: Some(cmap(cat, sym)),
        }
    }

    /// The camera's ray, PITCH_DEG down and looking north, meeting the
    /// ground at the centre of cell (10, 9).
    fn ray() -> (Vector3, Vector3) {
        let pitch = PITCH_DEG.to_radians();
        let back = Vector3::new(0.0, pitch.sin(), pitch.cos());
        (Vector3::new(10.0, 0.0, 9.0) + back * DISTANCE, -back)
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
        // a creature standing south of the floor cell covers it too
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
        let f = Fixture::new();
        let cat = &f.cat;
        for info in &cat.cmap {
            let t = terrain_of(&info.sym);
            let cell = Cell {
                glyph: Some(cmap(cat, &info.sym)),
                bk: None,
                terrain: (t != Terrain::Effect).then(|| cmap(cat, &info.sym)),
            };
            let look = f.look(&cell);
            let drawn = !matches!(t, Terrain::Stone);
            assert_eq!(!look.is_empty(), drawn, "{}", info.sym);
            assert!(look.top <= MAX_TOP, "{}", info.sym);
            if matches!(t, Terrain::Wall | Terrain::ClosedDoor) {
                assert!(look.top >= 1.0, "{} stands up", info.sym);
            }
        }
        // stairs and traps say what they are
        let letters = |sym: &str| {
            f.look(&feature(cat, sym))
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
        for (engr, plain) in [("S_engroom", "S_room"), ("S_engrcorr", "S_corr")] {
            let (e, p) = (f.look(&feature(cat, engr)), f.look(&feature(cat, plain)));
            assert_eq!(e.solids[0].paint, p.solids[0].paint, "{engr}");
            assert_eq!(e.solids.len(), p.solids.len() + 3, "{engr}");
        }
    }

    #[test]
    fn floors_walls_and_corridors_are_textured_stone_brick_and_dirt() {
        let f = Fixture::new();
        let cat = &f.cat;
        let pbr = |sym: &str| match f.look(&feature(cat, sym)).solids[0].paint {
            Paint::Pbr(m, shade) => (f.art.material_at(m).0.to_string(), shade),
            other => panic!("{sym}: {other:?}"),
        };
        assert_eq!(pbr("S_room").0, "floor");
        assert_eq!(pbr("S_corr").0, "dirt");
        assert_eq!(pbr("S_vwall").0, "brick");
        let has = |sym: &str, name: &str| {
            f.look(&feature(cat, sym))
                .solids
                .iter()
                .any(|s| matches!(s.paint, Paint::Pbr(m, _) if f.art.material_at(m).0 == name))
        };
        assert!(has("S_hcdoor", "wood") && has("S_hcdoor", "iron"));
        assert!(has("S_altar", "marble") && has("S_fountain", "water"));
        assert!(has("S_upstair", "marble") && has("S_bars", "iron"));
        // the part of a room out of view is darker than the part in view
        assert!(pbr("S_darkroom").1 < pbr("S_room").1);
        assert!(f.look(&feature(cat, "S_room")).lit);
        assert!(!f.look(&feature(cat, "S_darkroom")).lit);
        // floors vary a little from cell to cell, deterministically
        let shade_at = |x, y| match look_of(
            &feature(cat, "S_room"),
            Near::default(),
            &f.ctx_at(x, y, None),
        )
        .solids[0]
            .paint
        {
            Paint::Pbr(_, s) => s,
            _ => 0,
        };
        let shades: std::collections::HashSet<u8> = (0..20).map(|x| shade_at(x, 3)).collect();
        assert!(shades.len() > 1);
        assert_eq!(shade_at(4, 4), shade_at(4, 4));
        // floors cast no shadows, walls do
        assert!(!f.look(&feature(cat, "S_room")).solids[0].shadow);
        assert!(f.look(&feature(cat, "S_vwall")).solids[0].shadow);
        // a tree is a model on earth
        assert_eq!(f.look(&feature(cat, "S_tree")).models.len(), 1);
    }

    #[test]
    fn monsters_are_models_standing_by_size() {
        let f = Fixture::new();
        let cat = &f.cat;
        let look = |g: Glyph| f.look(&on_floor(cat, g));
        let newt = look(monster(cat, "newt", 0));
        let giant = look(monster(cat, "hill giant", 0));
        assert!(newt.top < giant.top);
        assert_eq!(newt.models.len(), 1);
        assert!(newt.letters.is_empty(), "models, not letters");
        let human = |flags| look(monster(cat, "human", flags)).models[0].look.art.model;
        assert_ne!(human(mg::FEMALE), human(0), "a woman has her own model");
        let pet = look(monster(cat, "little dog", mg::PET));
        assert!(
            pet.solids
                .iter()
                .any(|s| s.paint == Paint::Flat(PET_RING, Finish::Flat))
        );
        let seen = look(monster(cat, "little dog", mg::DETECT));
        assert_eq!(seen.models[0].look.pose, Pose::Ghost);
        // a monster on an altar stands on it
        let mut on_altar = on_floor(cat, monster(cat, "newt", 0));
        on_altar.terrain = Some(cmap(cat, "S_altar"));
        let alt = f.look(&on_altar);
        assert!(alt.models[0].pos.y > 0.5 && alt.top > newt.top + 0.4);
        // and on up stairs, on the step under the cell's centre
        let mut on_stairs = on_altar;
        on_stairs.terrain = Some(cmap(cat, "S_upstair"));
        let stairs = f.look(&on_stairs);
        let step = stairs
            .solids
            .iter()
            .filter(|s| {
                matches!(s.mesh, MeshKey::Box(..)) && s.pos.z.abs() < 0.1 && s.pos.x.abs() < 0.1
            })
            .map(|s| s.pos.y + s.mesh.half_height())
            .fold(0.0f32, f32::max);
        assert!(step > 0.2, "a step under the centre");
        assert!((stairs.models[0].pos.y - step).abs() < 0.01);
    }

    #[test]
    fn monsters_face_the_hero_and_the_hero_faces_their_way() {
        let f = Fixture::new();
        let cat = &f.cat;
        let cell = on_floor(cat, monster(cat, "jackal", 0));
        // the hero east of the monster: it turns +90 degrees (towards +x)
        let east = look_of(&cell, Near::default(), &f.ctx_at(10, 10, Some((13, 10))));
        assert!((east.models[0].yaw - 90.0).abs() < 0.1);
        let north = look_of(&cell, Near::default(), &f.ctx_at(10, 10, Some((10, 7))));
        assert!((north.models[0].yaw.abs() - 180.0).abs() < 0.1);
        let hero = on_floor(cat, monster(cat, "valkyrie", mg::HERO | mg::FEMALE));
        let ctx = Ctx {
            hero_yaw: -90.0,
            ..f.ctx_at(10, 10, Some((10, 10)))
        };
        assert_eq!(look_of(&hero, Near::default(), &ctx).models[0].yaw, -90.0);
    }

    #[test]
    fn corpses_lie_darker_and_statues_stand_in_stone_on_a_plinth() {
        let f = Fixture::new();
        let cat = &f.cat;
        let alive = f.look(&on_floor(cat, monster(cat, "jackal", 0)));
        let body = Glyph {
            kind: GlyphKind::Body,
            ..monster(cat, "jackal", 0)
        };
        let corpse = f.look(&on_floor(cat, body));
        let (a, c) = (alive.models[0].look, corpse.models[0].look);
        assert_eq!(c.pose, Pose::Corpse);
        assert_eq!(c.art.model, a.art.model);
        assert!(c.tint.r < a.tint.r && c.tint.g < a.tint.g);
        assert!(corpse.top < alive.top);
        let statue = Glyph {
            kind: GlyphKind::Statue,
            ..monster(cat, "jackal", 0)
        };
        let s = f.look(&on_floor(cat, statue));
        assert_eq!(s.models[0].look.pose, Pose::Statue);
        assert!(matches!(
            s.models[0].look.art.skin,
            nh_art::Skin::Material(_)
        ));
        assert!(s.models[0].pos.y >= 0.1, "on its plinth");
        assert!(s.solids.len() >= 2, "floor and plinth");
    }

    #[test]
    fn walls_in_front_of_open_ground_are_cut_down() {
        let f = Fixture::new();
        let cat = &f.cat;
        let floor = feature(cat, "S_room");
        let stone = feature(cat, "S_stone");
        let top = |sym: &str, north: &Cell| {
            look_of(
                &feature(cat, sym),
                Near([Some(north), None, None, None]),
                &f.ctx(),
            )
            .top
        };
        // the south wall of a room would hide the row behind it
        assert_eq!(top("S_hwall", &floor), CUT_HEIGHT);
        assert!((top("S_hcdoor", &floor) - CUT_HEIGHT).abs() < 0.01);
        // a side wall below a doorway would hide whoever stands in it
        assert_eq!(top("S_vwall", &feature(cat, "S_ndoor")), CUT_HEIGHT);
        // walls with rock or wall behind stand
        assert_eq!(top("S_vwall", &feature(cat, "S_vwall")), WALL_HEIGHT);
        assert!((top("S_vcdoor", &feature(cat, "S_vwall")) - DOOR_HEIGHT).abs() < 0.05);
        assert_eq!(top("S_hwall", &stone), WALL_HEIGHT);
        assert_eq!(top("S_hwall", &feature(cat, "S_hwall")), WALL_HEIGHT);
        assert_eq!(f.look(&feature(cat, "S_hwall")).top, WALL_HEIGHT);
        // on ground never shown, only what stays put cuts a wall: the hero
        // or an object, not a warning or a sensed monster passing by
        let unseen = |g: Glyph| Cell {
            glyph: Some(g),
            bk: None,
            terrain: None,
        };
        let hero = monster(cat, "newt", mg::HERO);
        assert_eq!(top("S_hwall", &unseen(hero)), CUT_HEIGHT);
        let obj = glyph(GlyphKind::Obj, '(');
        assert_eq!(top("S_hwall", &unseen(obj)), CUT_HEIGHT);
        let warning = glyph(GlyphKind::Warning, '3');
        assert_eq!(top("S_hwall", &unseen(warning)), WALL_HEIGHT);
        let sensed = monster(cat, "newt", mg::DETECT);
        assert_eq!(top("S_hwall", &unseen(sensed)), WALL_HEIGHT);
        let seen = monster(cat, "little dog", mg::PET);
        assert_eq!(top("S_hwall", &unseen(seen)), CUT_HEIGHT);
    }

    #[test]
    fn objects_are_models_by_appearance_and_piles_lie_on_a_heap() {
        let f = Fixture::new();
        let cat = &f.cat;
        let name = |l: &Look, i: usize| f.art.model_at(l.models[i].look.art.model).0.to_string();
        let chest = f.look(&on_floor(cat, object(cat, "(", "chest")));
        assert_eq!(name(&chest, 0), "chest");
        assert!(chest.letters.is_empty());
        let boulder = f.look(&on_floor(cat, object(cat, "`", "boulder")));
        assert_eq!(name(&boulder, 0), "boulder");
        assert!(boulder.top > 0.7);
        let pile = Glyph {
            flags: mg::OBJPILE,
            ..object(cat, "(", "chest")
        };
        let pile = f.look(&on_floor(cat, pile));
        assert_eq!(name(&pile, 0), "heap");
        assert!(pile.models[1].pos.y > chest.models[0].pos.y);
        // two potions of different appearances may differ; the same
        // appearance always looks the same
        let ruby = f.look(&on_floor(cat, object(cat, "!", "ruby")));
        assert_eq!(ruby, f.look(&on_floor(cat, object(cat, "!", "ruby"))));
    }

    #[test]
    fn effects_flash_over_what_is_there() {
        let f = Fixture::new();
        let cat = &f.cat;
        let mut cell = on_floor(cat, cmap(cat, "S_vbeam"));
        let beam = f.look(&cell);
        assert!(
            beam.solids
                .iter()
                .any(|s| matches!(s.paint, Paint::Flat(_, Finish::Glow)))
        );
        // the engulfer is drawn around the hero, not per cell
        cell.glyph = Some(glyph(GlyphKind::Swallow, '/'));
        assert_eq!(f.look(&cell).solids.len(), 1, "only the floor");
        // unexplored cells draw nothing; an unseen floor under a monster does
        assert!(f.look(&Cell::default()).is_empty());
        let under = Cell {
            glyph: Some(monster(cat, "newt", 0)),
            bk: None,
            terrain: None,
        };
        assert_eq!(
            f.look(&under).solids[0].paint,
            Paint::Flat(FLOOR_UNSEEN, Finish::Matte)
        );
    }

    #[test]
    fn unseen_ground_under_something_takes_the_floor_around_it() {
        let f = Fixture::new();
        let cat = &f.cat;
        let under = Cell {
            glyph: Some(glyph(GlyphKind::Obj, '(')),
            bk: None,
            terrain: None,
        };
        let tile = |near: [Option<&Cell>; 4]| look_of(&under, Near(near), &f.ctx()).solids[0];
        let (lit, dark, corr) = (
            feature(cat, "S_room"),
            feature(cat, "S_darkroom"),
            feature(cat, "S_corr"),
        );
        let wall = feature(cat, "S_hwall");
        let alone = f.look(&lit).solids[0];
        let dark_alone = f.look(&dark).solids[0];
        let corr_alone = f.look(&corr).solids[0];
        // an object in a lit room lies on the same floor as its neighbours
        let t = tile([Some(&wall), Some(&lit), Some(&lit), None]);
        assert_eq!((t.mesh, t.paint), (alone.mesh, alone.paint));
        // most neighbours decide; ties go to the lit floor
        assert_eq!(
            tile([Some(&dark), Some(&dark), Some(&lit), None]).paint,
            dark_alone.paint
        );
        assert_eq!(
            tile([Some(&dark), Some(&lit), None, None]).paint,
            alone.paint
        );
        assert_eq!(
            tile([None, None, Some(&corr), Some(&corr)]).paint,
            corr_alone.paint
        );
        // walls alone say nothing about the floor
        assert_eq!(
            tile([Some(&wall), None, None, None]).paint,
            Paint::Flat(FLOOR_UNSEEN, Finish::Matte)
        );
    }

    #[test]
    fn lit_cells_group_into_areas() {
        let lit: std::collections::HashSet<(i32, i32)> =
            [(1, 1), (2, 1), (2, 2), (10, 10), (11, 10)]
                .into_iter()
                .collect();
        let mut sizes: Vec<usize> = lit_areas(&lit).iter().map(Vec::len).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![2, 3]);
    }

    #[test]
    fn the_torch_flickers_gently() {
        let values: Vec<f32> = (0..200).map(|i| flicker(f64::from(i) * 0.05)).collect();
        let (lo, hi) = values
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(lo > 0.8 && hi < 1.2 && hi - lo > 0.1, "{lo}..{hi}");
    }
}
