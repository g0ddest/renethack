//! The 3D map: floors, walls and features built per cell with the art
//! manifest's PBR materials (triplanar in world space, so the stone runs on
//! across cells); models for monsters, the hero, objects and trees from the
//! art library; a perspective camera following the hero, a flickering torch
//! on the hero and a fill light over each part of the level in view. Cell
//! (x, y) is the point (x, 0, y); one cell is one metre.
//!
//! Each cell's look is computed as plain data (`Look`) and its nodes are
//! touched only when the look changes. Meshes, materials and model instances
//! are shared or pooled by the art library; the cells' solids are drawn in
//! batches (`batch`), with the map's surface shader (`surface`), which also
//! draws the fog of war: what the hero sees now, what they remember, and
//! the rock the level is cut into fading into darkness.
//!
//! An entity that steps to a neighbouring cell (`MapState::take_dirty_moves`)
//! keeps its model node: the node is carried to the new cell and walks
//! there (`animator`); a fight the messages tell of turns the attacker to
//! its target. Every cell's look, picking and hovering stay on the world
//! model's cells; only the nodes are between them for a moment.

use std::collections::{HashMap, HashSet};

use godot::classes::base_material_3d::{BillboardMode, Feature, ShadingMode};
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::environment::{
    AmbientSource, BgMode, FogMode, GlowBlendMode, ReflectionSource, ToneMapper,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::label_3d::DrawFlags;
use godot::classes::light_3d::Param;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    Camera3D, CanvasLayer, ColorRect, Decal, DirectionalLight3D, Environment, FogMaterial,
    FogVolume, GeometryInstance3D, Label3D, Material, MeshInstance3D, Node3D, OmniLight3D,
    PackedScene, RenderingServer, Shader, ShaderMaterial, StandardMaterial3D, SurfaceTool,
    SystemFont, WorldEnvironment,
};
use godot::prelude::*;
use nh_art::{ArtManifest, Tint};
use nh_protocol::{Catalog, Glyph, GlyphKind, MonsterInfo, ObjectTile, mg};
use nh_world::{
    Branch, COLNO, Cell, Ident, ItemUse, MapState, Move, ROWNO, Terrain, UseKind, World,
    cell_terrain, in_field, locate_attack, parse_attack, terrain_of,
};

use crate::animator::{Motion, pace, yaw_toward};
use crate::art::{Art, Finish, Model, ModelLook, Pose, build_flat, no_shadow};
use crate::batch::{Batches, Slot};
use crate::branch_look::{BranchLook, Prop};
use crate::meshes::{
    MeshKey, bevel, cuboid, cylinder, dome, facets, plane, prism, rock, sphere, torus,
};
use crate::surface::{Fow, FowCell, Role, Seen, Surfaces, cell_index, sight};
use crate::theme::{self, nh_color};
use crate::vfx::{Vfx, VfxKind};

/// Low enough to see creatures from the side, high enough to see the floor
/// between walls: the pitch at the default distance; closer in the camera
/// looks more from the side, further out more from above.
const PITCH_DEG: f32 = 55.0;
const PITCH_NEAR_DEG: f32 = 47.0;
const PITCH_FAR_DEG: f32 = 60.0;
const DISTANCE: f32 = 11.0;
const MIN_DISTANCE: f32 = 7.0;
const MAX_DISTANCE: f32 = 22.0;
/// The camera's vertical field of view, degrees.
const FOV_DEG: f32 = 40.0;
/// The overview never goes further (a whole 80x21 level fits well within).
const MAX_OVERVIEW_DISTANCE: f32 = 80.0;
const FOLLOW_RATE: f32 = 6.0;
/// A per-channel curve through (1/4, shadows/4) and (3/4, highlights*3/4).
fn grade_curve(shadows: Color, highlights: Color) -> Gd<godot::classes::GradientTexture1D> {
    let mut g = godot::classes::Gradient::new_gd();
    let at = |c: Color, k: f32| Color::from_rgb(c.r * k, c.g * k, c.b * k);
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.25, 0.75, 1.0][..]));
    g.set_colors(&PackedColorArray::from(
        &[
            Color::from_rgb(0.0, 0.0, 0.0),
            at(shadows, 0.25),
            at(highlights, 0.75),
            Color::from_rgb(1.0, 1.0, 1.0),
        ][..],
    ));
    let mut t = godot::classes::GradientTexture1D::new_gd();
    t.set_gradient(&g);
    t.set_width(256);
    t
}

/// More cells changed at once than this: those near the hero (within
/// `NEAR_CELLS`) are drawn now, the rest over the next frames.
const MANY_CELLS: usize = 150;
const NEAR_CELLS: i32 = 8;
/// Where a feature's lamp burns on its cell.
fn lamp_offset(kind: Lamp) -> Vector3 {
    match kind {
        Lamp::Up => at(0.0, 1.5, -0.3),
        Lamp::Lava => at(0.0, 0.4, 0.0),
        Lamp::Down => at(0.0, -0.05, 0.3),
    }
}

/// Time per frame spent drawing a new level's cells (the rest wait for
/// the next frames).
const BUILD_BUDGET: std::time::Duration = std::time::Duration::from_millis(4);

/// Every cell of the level, the farthest from `hero` first (they are taken
/// from the end).
fn build_order(hero: Option<(i32, i32)>) -> Vec<(i32, i32)> {
    let (hx, hy) = hero.unwrap_or((COLNO / 2, ROWNO / 2));
    let mut cells: Vec<(i32, i32)> = (0..ROWNO)
        .flat_map(|y| (1..COLNO).map(move |x| (x, y)))
        .collect();
    cells.sort_by_key(|&(x, y)| std::cmp::Reverse((x - hx).pow(2) + (y - hy).pow(2)));
    cells
}

/// Seconds a new level takes to come up out of black.
const LEVEL_FADE_SECS: f32 = 0.25;
/// Seconds from the start of a strike to the blow landing.
const CONTACT_SECS: f32 = 0.16;
/// Descent per step when a pointer ray is walked through the raised geometry.
const PICK_STEP: f32 = 0.05;
/// Nothing drawn reaches higher (a giant on an altar).
const MAX_TOP: f32 = 2.8;

/// Walls stand taller than the hero; the ones in front of open ground are
/// cut down (`CUT_HEIGHT`), so the hero is never behind one.
const WALL_HEIGHT: f32 = 2.1;
/// A door's leaf; a lintel and the cap close the wall above it.
const DOOR_HEIGHT: f32 = 1.8;
const LINTEL: f32 = 0.2;
/// Walls and doors in front of open ground, seen from the camera's side.
const CUT_HEIGHT: f32 = 0.35;
/// The dark rock slab on top of a wall.
const CAP_HEIGHT: f32 = 0.1;
/// How high a built place's iron band runs round its walls.
const BAND_Y: f32 = 1.2;
/// A plinth along a wall's foot where open ground is next to it.
const PLINTH_HEIGHT: f32 = 0.22;
/// The rock around the level: its top, and cut down in front of open
/// ground (one cell or two north of it).
const ROCK_HEIGHT: f32 = 2.05;
const ROCK_CUT: f32 = 0.45;
/// Label3D font size; a letter's height is about `FONT_PX * pixel size`.
const FONT_PX: i32 = 96;
const PX_MONSTER: f32 = 0.0068;
const PX_FEATURE: f32 = 0.0062;
/// Where the camera aims, south of the hero at the default distance (the
/// log covers the bottom of the screen); it shrinks as the camera closes in.
const AIM_SOUTH: f32 = 1.2;

/// Brightness (%) of floors; what is remembered the surface shader darkens.
const SHADE_LIT: u8 = 92;
/// Stairs are of the floor's stone, a little darker than the floor.
const SHADE_STAIRS: u8 = 75;
/// The bedrock under and around the level, and the rock mass.
const SHADE_BEDROCK: u8 = 100;
/// The art gallery's ambient light, and how much its fills are raised.
const GALLERY_AMBIENT: f32 = 0.4;
/// The map's parts, shown one a frame behind the title (`show_parts`).
const SHOW_ALL: u8 = 4;
/// The glow from below round the hero where the floors burn: how far it
/// reaches and how bright.
const UNDERGLOW_RANGE: f32 = 4.5;
const UNDERGLOW_ENERGY: f32 = 0.9;
const GALLERY_FILL: f32 = 2.5;
const SHADE_ROCK: u8 = 70;
/// Stones at the foot of a wall: of the rock, darker still.
const SHADE_RUBBLE: u8 = 55;
/// A wall cut down in front of open ground.
const SHADE_RUIN: u8 = 65;
/// A lying corpse is this much darker than the living monster.
const CORPSE_DARKEN: f32 = 0.45;

const FLOOR_UNSEEN: Color = Color::from_rgb(0.19, 0.19, 0.21);
const DEEP: Color = Color::from_rgb(0.02, 0.02, 0.03);
/// Scratches of an engraving on the floor.
const ENGRAVING: Color = Color::from_rgb(0.05, 0.04, 0.035);
const HERO_RING: Color = Color::from_rgba(1.0, 0.83, 0.54, 0.2);
/// The core's cursor (getpos, travel): the theme's gold.
const CURSOR: Color = Color::from_rgba(1.0, 0.82, 0.42, 0.85);
/// The way an order would walk: pale gold dots exploring, red in a fight.
const PATH_EXPLORE: Color = Color::from_rgb(0.85, 0.72, 0.38);
const PATH_EXPLORE_GOAL: Color = Color::from_rgb(1.0, 0.86, 0.45);
const PATH_COMBAT: Color = Color::from_rgb(0.85, 0.28, 0.2);
const PATH_COMBAT_GOAL: Color = Color::from_rgb(1.0, 0.36, 0.25);
const PET_RING: Color = Color::from_rgba(0.37, 0.84, 0.75, 0.32);
/// Under a monster the pointer is on (not the hero's or a pet), and its
/// outline.
const HOSTILE_RING: Color = Color::from_rgba(0.88, 0.29, 0.23, 0.6);
/// The outline of a pet or an object under the pointer.
const HOVER_OUTLINE: Color = Color::from_rgba(0.95, 0.86, 0.62, 0.9);
/// The hero seen through what hides them.
const XRAY: Color = Color::from_rgba(0.55, 0.68, 1.0, 0.45);
/// The hero's own dim pool of light, over their head (no lamp: the client
/// does not know of one yet), and a cold rim light from behind that only
/// the hero's model takes (`RIM_LAYER`).
const HERO_LIGHT: Color = Color::from_rgb(0.9, 0.84, 0.76);
const HERO_LIGHT_ENERGY: f32 = 2.4;
const HERO_LIGHT_RANGE: f32 = 5.5;
/// A lamp in hand lights the scene; the light over the hero only fills
/// the shadow the body throws.
const LAMP_POOL: Color = Color::from_rgb(1.0, 0.72, 0.46);
const LAMP_POOL_ENERGY: f32 = 0.8;
const LAMP_POOL_RANGE: f32 = 4.5;
const RIM_LIGHT: Color = Color::from_rgb(0.62, 0.70, 1.0);
const RIM_LAYER: u32 = 1 << 1;
/// Torch sconces: lit by everything but their own flames (which would
/// blow them out, a hand away).
const SCONCE_LAYER: u32 = 1 << 2;
/// The level's own surfaces (decals land only on them, never on models).
const TERRAIN_LAYER: u32 = 1 << 3;
const ROOM_LIGHT: Color = Color::from_rgb(0.86, 0.80, 0.72);
const ROOM_LIGHT_ENERGY: f32 = 0.3;
/// Torches on the walls of a lit room: one every few cells of its north,
/// east and west walls; the ones nearest the hero cast shadows.
const TORCH: Color = Color::from_rgb(1.0, 0.66, 0.38);
const TORCH_RANGE: f32 = 5.5;
const TORCH_SHADOWS: usize = 3;
/// Torches made before any level is shown.
const TORCHES_AHEAD: usize = 16;
/// The branches' doors and candles made ahead.
const DOORS_AHEAD: usize = 2;
const CANDLES_AHEAD: usize = 32;
const CANDELABRAS_AHEAD: usize = 8;
/// A flame's flipbook: 16 x 4 frames of a real flame (Unity Labs, CC0).
const FLAME_BOOK: &str = "res://art/cc0/unity-labs/flipbooks/Flame02_16x4.png";
/// The flame of a torch, bright enough to glow.
const FLAME: Color = Color::from_rgb(1.0, 0.45, 0.12);
/// Light spilling down up stairs from the level above, and the cold glow
/// from the depths of down stairs.
const STAIR_UP_LIGHT: Color = Color::from_rgb(1.0, 0.72, 0.46);
const STAIR_DOWN_LIGHT: Color = Color::from_rgb(0.5, 0.6, 0.9);
/// Depth fog and the background: never pure black.
const DARKNESS: Color = Color::from_rgb(0.008, 0.009, 0.013);

/// What covers a solid: a manifest material (index, brightness %, how the
/// surface shader treats it) or a plain colour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Paint {
    Pbr(usize, u8, Role),
    Flat(Color, Finish),
    /// A shader of the map's own: water, lava, the air over lava.
    Liquid(Liquid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Liquid {
    Water,
    /// Foam along water's bank.
    Foam,
    Lava,
    Haze,
}

/// A light of a feature on its cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lamp {
    /// Warm, spilling down the stairs from above.
    Up,
    /// Cold, from deep down, with a little mist.
    Down,
    /// The red glow of lava.
    Lava,
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
    /// Shown only under the pointer and in the overview (the letters of
    /// stairs: the model says what it is, the letter only names it).
    hint: bool,
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
    /// Which of `models` is the monster or object on the cell (it is
    /// carried along when it steps to another cell).
    entity: Option<usize>,
    /// A wall at full height (a torch can hang on it).
    wall: bool,
    /// Something here lies below the ground (the bedrock under the level
    /// leaves it a hole).
    sunk: bool,
    /// A monster that is not the hero's nor a pet (red ring under the pointer).
    hostile: bool,
    /// The rock the level is cut into.
    rock: bool,
    /// What the fog of war makes of the cell.
    seen: Seen,
    /// A ring on the ground (a pet's).
    ring: Option<Color>,
    lamp: Option<Lamp>,
    /// An explosion shows here.
    blast: Option<Color>,
    /// A ray crosses here: (colour, yaw, height).
    ray: Option<(Color, f32, f32)>,
    /// Something glints here for a moment: (colour, height).
    glint: Option<(Color, f32)>,
    /// Masonry of a wall (grime gathers at its foot).
    masonry: bool,
    /// Models of the branch's own on the cell (doors, candles).
    props: Vec<PlacedProp>,
}

/// A branch's model on a cell: where, which way, how stretched.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PlacedProp {
    prop: Prop,
    pos: Vector3,
    /// Degrees about y.
    yaw: f32,
    scale: Vector3,
}

impl Look {
    fn is_empty(&self) -> bool {
        self.solids.is_empty()
            && self.letters.is_empty()
            && self.models.is_empty()
            && self.ray.is_none()
            && self.glint.is_none()
            && self.blast.is_none()
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
            hint: false,
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
/// The roles (as a new character names them).
pub(crate) const ROLES: [&str; 13] = [
    "archeologist",
    "barbarian",
    "caveman",
    "healer",
    "knight",
    "monk",
    "priest",
    "rogue",
    "ranger",
    "samurai",
    "tourist",
    "valkyrie",
    "wizard",
];

/// The monsters met most on the first levels, and the pets.
pub(crate) const WARM_MONSTERS: &[&str] = &[
    "kitten",
    "little dog",
    "pony",
    "newt",
    "jackal",
    "sewer rat",
    "grid bug",
    "fox",
    "coyote",
    "kobold",
    "large kobold",
    "goblin",
    "lichen",
    "gecko",
    "giant rat",
    "yellow mold",
    "acid blob",
    "floating eye",
    "gnome",
    "gnome lord",
    "hobbit",
    "kobold zombie",
    "gnome zombie",
    "homunculus",
    "giant bat",
    "hill orc",
    "dwarf",
    "shopkeeper",
];

/// The monster a role's hero is drawn as.
pub(crate) fn role_monster(role: &str) -> &str {
    match role {
        "caveman" | "cavewoman" => "cave dweller",
        "priest" | "priestess" => "cleric",
        r => r,
    }
}

/// The darkest a creature's tint goes (its brightest channel): a black
/// one is dark fur under the light, not a silhouette.
const MIN_TINT: f32 = 0.55;

/// `c` brightened to `MIN_TINT`, its hue kept.
fn lifted(c: Color) -> Color {
    let v = c.r.max(c.g).max(c.b);
    if v >= MIN_TINT || v <= 0.0 {
        return c;
    }
    let k = MIN_TINT / v;
    Color::from_rgba(c.r * k, c.g * k, c.b * k, c.a)
}

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
    /// Which way the monster here faces since it last stepped or fought.
    facing: Option<f32>,
    /// How the branch the level is in looks.
    branch: &'a BranchLook,
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

    /// A manifest material by name, as this branch has it.
    fn mat(&self, name: &str) -> Option<usize> {
        self.art.material(self.branch.material(name))
    }

    /// A manifest material as this branch has it.
    fn remap(&self, m: Option<usize>) -> Option<usize> {
        let name = self.art.material_at(m?).0;
        self.art.material(self.branch.material(name)).or(m)
    }
}

/// What a neighbour of a cell is, as far as its look goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Nothing known, or rock.
    Solid,
    /// A wall (or a door in one).
    Wall,
    /// Ground to stand on.
    Open,
}

/// The neighbours of a cell, by which a cell looks the way it does.
#[derive(Clone, Copy)]
struct Around {
    /// North, south, west, east.
    sides: [Side; 4],
    /// Which of them are water or lava, and which a room's floor.
    liquid: [bool; 4],
    floor: [bool; 4],
}

impl Around {
    fn of(near: &Near, catalog: &Catalog) -> Around {
        let mut sides = [Side::Solid; 4];
        let mut liquid = [false; 4];
        let mut floor = [false; 4];
        for (i, side) in sides.iter_mut().enumerate() {
            let cell = near.cells[i];
            let t = cell.and_then(|c| cell_terrain(c, catalog));
            liquid[i] = matches!(t, Some(Terrain::Pool | Terrain::Water | Terrain::Lava));
            floor[i] = matches!(t, Some(Terrain::Floor | Terrain::DarkFloor));
            *side = match t {
                Some(Terrain::Wall | Terrain::LavaWall) => Side::Wall,
                Some(Terrain::ClosedDoor | Terrain::OpenDoor | Terrain::Doorway) => Side::Wall,
                _ if is_open(cell, catalog) => Side::Open,
                _ => Side::Solid,
            };
        }
        Around {
            sides,
            liquid,
            floor,
        }
    }

    /// A door's wall runs along x (its passage along z): walls or doors
    /// east or west of it, or open ground north or south.
    fn wall_along_x(&self) -> bool {
        let wall = |i: usize| self.sides[i] == Side::Wall;
        let open = |i: usize| self.sides[i] == Side::Open;
        if wall(2) || wall(3) {
            return true;
        }
        if wall(0) || wall(1) {
            return false;
        }
        open(0) || open(1)
    }
}

/// The unit vectors towards north, south, west, east (x, z).
const SIDES: [(f32, f32); 4] = [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)];

/// Floor, walls and features. Sets `ground`. `cut`: open ground lies north
/// of the cell (the row behind a room's south wall, a doorway above a side
/// wall), so a wall or door here would hide it from the camera and is
/// drawn low.
fn terrain_look(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, near: &Near, ctx: &Ctx) {
    let around = Around::of(near, ctx.catalog);
    let cut = is_open(near.cells[0], ctx.catalog);
    terrain_base(look, t, sym, g, cut, &around, ctx);
    if engraved(sym) {
        // a few lines of runes cut into the floor, dark in their grooves
        let paint = Paint::Flat(ENGRAVING, Finish::Matte);
        for row in 0..3 {
            let z = -0.16 + 0.16 * row as f32;
            for k in 0..4 {
                let salt = 60 + (row * 4 + k) as u32 * 2;
                if ctx.noise(salt) < 0.15 {
                    continue;
                }
                let x = -0.24 + 0.16 * k as f32 + (ctx.noise(salt + 1) - 0.5) * 0.04;
                let yaw = (ctx.noise(salt + 2) - 0.5) * 120.0;
                let len = 0.06 + ctx.noise(salt + 3) * 0.05;
                look.turned(
                    cuboid(len, 0.004, 0.012),
                    paint,
                    at(x, 0.002, z),
                    at(0.0, yaw, 0.0),
                );
                look.turned(
                    cuboid(0.012, 0.004, len * 0.8),
                    paint,
                    at(x + 0.02, 0.002, z),
                    at(0.0, yaw * 0.5, 0.0),
                );
            }
        }
        for s in look.solids.iter_mut().filter(|s| s.paint == paint) {
            s.shadow = false;
        }
    }
    look.seen = match t {
        Terrain::Floor => Seen::LitFloor,
        Terrain::DarkFloor => Seen::DarkFloor,
        Terrain::Corridor if sym == "S_litcorr" => Seen::LitCorridor,
        Terrain::Stone | Terrain::Unknown | Terrain::Effect => Seen::Nothing,
        _ => Seen::Other,
    };
}

/// Something is engraved here (in a room or a corridor).
fn engraved(sym: &str) -> bool {
    matches!(sym, "S_engroom" | "S_engrcorr")
}

/// Corner pieces and junctions of walls: a pier, a little wider and taller.
fn is_pier(sym: &str) -> bool {
    matches!(
        sym,
        "S_tlcorn"
            | "S_trcorn"
            | "S_blcorn"
            | "S_brcorn"
            | "S_crwall"
            | "S_tuwall"
            | "S_tdwall"
            | "S_tlwall"
            | "S_trwall"
    )
}

/// A few stones at the foot of the walls beside a floor cell, placed by the
/// cell (the same every time): small, half sunk and of the rock, never
/// like something lying there to pick up.
fn rubble(
    look: &mut Look,
    around: &Around,
    rock: Option<usize>,
    ctx: &Ctx,
    edge: Side,
    chance: f32,
) {
    let Some(m) = rock else {
        return;
    };
    for (i, (dx, dz)) in SIDES.iter().enumerate() {
        if around.sides[i] != edge || ctx.noise(21 + i as u32) > chance {
            continue;
        }
        let n = 1 + (ctx.noise(31 + i as u32) * 2.99) as usize;
        for k in 0..n {
            let salt = 41 + (i * 4 + k) as u32 * 3;
            // three sizes of stone: one batch each across the level
            let r = [0.04, 0.065, 0.09][(ctx.noise(salt) * 2.99) as usize];
            let along = (ctx.noise(salt + 1) - 0.5) * 0.8;
            let from_wall = 0.42 - r - ctx.noise(salt + 2) * 0.08;
            let (x, z) = if *dx == 0.0 {
                (along, dz * from_wall)
            } else {
                (dx * from_wall, along)
            };
            let rot = at(ctx.noise(salt + 3) * 90.0, ctx.noise(salt + 4) * 360.0, 0.0);
            let paint = Paint::Pbr(m, SHADE_RUBBLE, Role::Trim);
            look.turned(facets(r, 5), paint, at(x, r * 0.25, z), rot);
            if let Some(s) = look.solids.last_mut() {
                s.shadow = false;
            }
        }
    }
    // the stones never make the cell taller to the pointer
    look.top = look.top.min(0.2);
}

/// Timber holding up a mine's rock: on some walls beside open ground, two
/// posts against the face and a beam across, placed by the cell.
fn mine_support(look: &mut Look, around: &Around, ctx: &Ctx) {
    let Some(wood) = ctx.mat("wood") else {
        return;
    };
    let wood = Paint::Pbr(wood, 75, Role::Prop);
    for (i, (dx, dz)) in SIDES.iter().enumerate() {
        // the camera does not see a south wall's face (its north side)
        if around.sides[i] != Side::Open || i == 0 || ctx.noise(71 + i as u32) > 0.28 {
            continue;
        }
        let out = 0.56;
        for s in [-0.38f32, 0.38] {
            let (mesh, pos) = if *dx == 0.0 {
                (bevel(0.14, 1.9, 0.14), at(s, 0.95, dz * out))
            } else {
                (bevel(0.14, 1.9, 0.14), at(dx * out, 0.95, s))
            };
            look.solid(mesh, wood, pos);
        }
        let beam = if *dx == 0.0 {
            bevel(1.0, 0.16, 0.16)
        } else {
            bevel(0.16, 0.16, 1.0)
        };
        let pos = if *dx == 0.0 {
            at(0.0, 1.9, dz * out)
        } else {
            at(dx * out, 1.9, 0.0)
        };
        look.solid(beam, wood, pos);
    }
}

/// What holds water or lava: a bed of stone `depth` down, and a bank of
/// stone on each side where the ground is not liquid too.
fn liquid_bed(look: &mut Look, around: &Around, ctx: &Ctx, depth: f32) {
    let Some(m) = ctx.mat("bedrock") else {
        return;
    };
    // dark and wet under the water: a bed, not a pale tank
    let stone = Paint::Pbr(m, 45, Role::Prop);
    look.ground_tile(plane(1.0, 1.0), stone, at(0.0, -depth, 0.0));
    for (i, (dx, dz)) in SIDES.iter().enumerate() {
        if around.liquid[i] {
            continue;
        }
        let (mesh, pos) = if *dx == 0.0 {
            (bevel(1.0, depth, 0.06), at(0.0, -depth / 2.0, dz * 0.47))
        } else {
            (bevel(0.06, depth, 1.0), at(dx * 0.47, -depth / 2.0, 0.0))
        };
        look.solid(mesh, stone, pos);
        if let Some(s) = look.solids.last_mut() {
            s.shadow = false;
        }
    }
}

/// A doorway's frame: two posts and a lintel, the wall closed above it by
/// a stretch of masonry under the cap. Cut down, only the posts' feet.
fn door_frame(look: &mut Look, along_x: bool, cut: bool, ctx: &Ctx, wood: Paint, posts: bool) {
    let role = if ctx.branch.cave {
        Role::Rock
    } else {
        Role::Wall
    };
    let masonry = ctx
        .remap(ctx.art.terrain(Terrain::Wall).material)
        .map(|m| Paint::Pbr(m, SHADE_LIT, role));
    let cap = ctx
        .mat(ctx.branch.cap)
        .filter(|_| !ctx.branch.cave)
        .map(|m| Paint::Pbr(m, SHADE_LIT, Role::Trim));
    let h = if cut { CUT_HEIGHT } else { DOOR_HEIGHT };
    // the posts stand on the wall's line, either side of the passage; in
    // a side wall the south one would stand between the camera and
    // whoever is in the doorway: only its foot is left
    for s in [-1.0f32, 1.0].into_iter().filter(|_| posts) {
        let h = if !along_x && s > 0.0 {
            h.min(CUT_HEIGHT)
        } else {
            h
        };
        let (mesh, pos) = if along_x {
            (bevel(0.16, h, 0.34), at(s * 0.42, h / 2.0, 0.0))
        } else {
            (bevel(0.34, h, 0.16), at(0.0, h / 2.0, s * 0.42))
        };
        look.solid(mesh, wood, pos);
    }
    if cut {
        return;
    }
    let lintel = if along_x {
        bevel(1.0, LINTEL, 0.36)
    } else {
        bevel(0.36, LINTEL, 1.0)
    };
    if posts {
        look.solid(lintel, wood, at(0.0, DOOR_HEIGHT + LINTEL / 2.0, 0.0));
    }
    let above = WALL_HEIGHT - CAP_HEIGHT - DOOR_HEIGHT - LINTEL;
    if let Some(masonry) = masonry
        && above > 0.01
    {
        let y = DOOR_HEIGHT + LINTEL + above / 2.0;
        look.solid(bevel(1.0, above, 1.0), masonry, at(0.0, y, 0.0));
    }
    // over a side wall's doorway the cap would lie right over whoever
    // stands in it, seen from the south: the wall's line breaks there
    if let Some(cap) = cap.filter(|_| along_x) {
        let y = WALL_HEIGHT - CAP_HEIGHT / 2.0;
        look.solid(bevel(1.04, CAP_HEIGHT, 1.04), cap, at(0.0, y, 0.0));
    }
}

/// A door's leaf of planks across the passage (closed) or swung open
/// against its side, with iron bands and a ring.
fn door_leaf(look: &mut Look, along_x: bool, open: bool, h: f32, wood: Paint, iron: Paint) {
    // across the passage: along the wall; open: along the passage, at
    // the side of the hinge
    let across = along_x != open;
    let pos = match (open, along_x) {
        (false, _) => Vector3::ZERO,
        (true, true) => at(-0.36, 0.0, 0.0),
        (true, false) => at(0.0, 0.0, -0.36),
    };
    let width = 0.68;
    let planks = 3;
    let w = width / planks as f32;
    for i in 0..planks {
        let off = -width / 2.0 + w * (i as f32 + 0.5);
        let depth = 0.07 + 0.01 * (i % 2) as f32;
        let (mesh, p) = if across {
            (bevel(w - 0.012, h, depth), at(pos.x + off, h / 2.0, pos.z))
        } else {
            (bevel(depth, h, w - 0.012), at(pos.x, h / 2.0, pos.z + off))
        };
        look.solid(mesh, wood, p);
    }
    for f in [0.22, 0.78] {
        let (mesh, p) = if across {
            (cuboid(width + 0.02, 0.06, 0.1), at(pos.x, h * f, pos.z))
        } else {
            (cuboid(0.1, 0.06, width + 0.02), at(pos.x, h * f, pos.z))
        };
        look.solid(mesh, iron, p);
    }
    if !open && h > 1.0 {
        // a ring on the side the camera sees
        let (p, rot) = if across {
            (at(0.18, h * 0.5, 0.06), at(90.0, 0.0, 0.0))
        } else {
            (at(0.06, h * 0.5, 0.18), at(0.0, 0.0, 90.0))
        };
        look.turned(torus(0.035, 0.05), iron, p, rot);
    }
}

fn terrain_base(
    look: &mut Look,
    t: Terrain,
    sym: &str,
    g: &Glyph,
    cut: bool,
    around: &Around,
    ctx: &Ctx,
) {
    let c = g.color;
    let mut art = ctx.art.terrain(t);
    art.material = ctx.remap(art.material);
    art.trim = ctx.remap(art.trim);
    // "S_v..." features sit in a vertical wall: the passage runs along x
    let vertical = sym.starts_with("S_v");
    let (wall_h, door_h) = if cut {
        (CUT_HEIGHT, CUT_HEIGHT)
    } else {
        (WALL_HEIGHT, DOOR_HEIGHT)
    };
    // in view: the floor's full brightness (the texture varies it; a
    // brightness per cell would show the grid); the surface shader
    // darkens what is remembered
    let lit_shade = SHADE_LIT;
    let floor_mat = ctx.mat("floor");
    let bedrock = ctx.mat("bedrock");
    let pbr = |m: Option<usize>, shade: u8, role: Role, fallback: Color| match m {
        Some(m) => Paint::Pbr(m, shade, role),
        None => Paint::Flat(fallback, Finish::Matte),
    };
    let main = |shade: u8| pbr(art.material, shade, Role::Prop, FLOOR_UNSEEN);
    let trim = |shade: u8| pbr(art.trim, shade, Role::Prop, DEEP);
    let ground = |shade: u8| pbr(art.material, shade, Role::Floor, FLOOR_UNSEEN);
    let tile = plane(1.0, 1.0);
    let floor = |look: &mut Look| {
        let paint = pbr(floor_mat, lit_shade, Role::Floor, FLOOR_UNSEEN);
        look.ground_tile(tile, paint, Vector3::ZERO);
    };
    let label = |look: &mut Look, y: f32| {
        if let Some(ch) = glyph_char(g) {
            look.letter(ch, lighter(nh_color(c), 0.25), y, PX_FEATURE, false);
            if let Some(l) = look.letters.last_mut() {
                l.hint = true;
            }
        }
    };
    match t {
        Terrain::Stone | Terrain::Effect | Terrain::Unknown => {}
        Terrain::Wall => {
            // masonry under a slab of dark rock a little wider than it; a
            // corner or junction is a pier, wider and taller
            let pier = is_pier(sym);
            let (w, top) = if pier {
                (1.08, wall_h + if cut { 0.06 } else { 0.18 })
            } else {
                (1.0, wall_h)
            };
            if ctx.branch.cave {
                // a cave's wall is the rock itself, broken and uneven
                let rock_paint = pbr(art.material, SHADE_LIT, Role::Rock, FLOOR_UNSEEN);
                let h = if cut { ROCK_CUT } else { wall_h };
                look.solid(rock(1.0, h, 1.0), rock_paint, Vector3::ZERO);
                if let Some(s) = look.solids.last_mut() {
                    s.shadow = !cut;
                }
                if ctx.branch.supports && !cut {
                    mine_support(look, around, ctx);
                }
            } else if cut {
                // cut down, it is the stump of a wall: courses of masonry
                // broken off unevenly, darker than the wall that stands
                let ruin = pbr(art.material, SHADE_RUIN, Role::Ruin, FLOOR_UNSEEN);
                look.solid(rock(w, top, w), ruin, Vector3::ZERO);
            } else {
                let body = top - CAP_HEIGHT;
                let masonry = pbr(art.material, SHADE_LIT, Role::Wall, FLOOR_UNSEEN);
                look.solid(bevel(w, body, w), masonry, at(0.0, body / 2.0, 0.0));
                let cap = pbr(ctx.mat(ctx.branch.cap), SHADE_LIT, Role::Trim, DEEP);
                look.solid(
                    bevel(w + 0.04, CAP_HEIGHT, w + 0.04),
                    cap,
                    at(0.0, top - CAP_HEIGHT / 2.0, 0.0),
                );
            }
            // a plinth of darker stone where there is ground to stand on
            let plinth = pbr(art.material, 62, Role::Trim, DEEP);
            for (i, (dx, dz)) in SIDES.iter().enumerate() {
                if around.sides[i] != Side::Open || ctx.branch.cave {
                    continue;
                }
                let h = PLINTH_HEIGHT.min(wall_h - 0.02);
                let out = w / 2.0 + 0.03;
                let (mesh, pos) = if *dx == 0.0 {
                    (bevel(w + 0.02, h, 0.08), at(0.0, h / 2.0, dz * out))
                } else {
                    (bevel(0.08, h, w + 0.02), at(dx * out, h / 2.0, 0.0))
                };
                look.solid(mesh, plinth, pos);
            }
            look.ground = top;
            look.wall = !cut;
            look.masonry = true;
            // candles on some walls' tops beside a room (no object ever
            // lies there)
            let beside_room = around.sides.contains(&Side::Open);
            let lights = ctx.noise(91);
            if ctx.branch.candles && !cut && !ctx.branch.cave && beside_room && lights < 0.14 {
                // a brass candelabra of three, its flames
                look.props.push(PlacedProp {
                    prop: Prop::Candelabra,
                    pos: at(0.0, top, 0.0),
                    yaw: 0.0,
                    scale: Vector3::new(1.3, 1.3, 1.3),
                });
                let flame = Paint::Flat(Color::from_rgb(1.0, 0.7, 0.35), Finish::Glow);
                for x in [-0.145f32, 0.0, 0.145] {
                    look.solid(sphere(0.014), flame, at(x, top + 0.535, 0.0));
                    if let Some(s) = look.solids.last_mut() {
                        s.shadow = false;
                    }
                }
            } else if ctx.branch.candles && !cut && !ctx.branch.cave && beside_room && lights < 0.3
            {
                for (i, dx) in [-0.18f32, 0.16].into_iter().enumerate() {
                    let h = 1.2 + 0.4 * ctx.noise(92 + i as u32);
                    look.props.push(PlacedProp {
                        prop: Prop::Candle,
                        pos: at(dx, top, 0.04 * i as f32),
                        yaw: ctx.noise(94 + i as u32) * 360.0,
                        scale: Vector3::new(1.2, h, 1.2),
                    });
                    // its flame
                    let flame = Paint::Flat(Color::from_rgb(1.0, 0.7, 0.35), Finish::Glow);
                    look.solid(
                        sphere(0.012),
                        flame,
                        at(dx, top + 0.22 * h + 0.015, 0.04 * i as f32),
                    );
                    if let Some(s) = look.solids.last_mut() {
                        s.shadow = false;
                    }
                }
            }
            // the face the camera sees: a tower's narrow pointed windows
            // (the night cold through them) and its banners
            let facing = !cut && !pier && !ctx.branch.cave && around.sides[1] == Side::Open;
            let decor = ctx.noise(97);
            let window = facing && ctx.branch.windows && decor < 0.25;
            let banner = facing && ctx.branch.banners && (0.25..0.6).contains(&decor);
            let face = w / 2.0 + 0.01;
            let decor_from = look.solids.len();
            if window {
                let night = Paint::Flat(Color::from_rgb(0.2, 0.26, 0.48), Finish::Glow);
                look.solid(cuboid(0.18, 0.8, 0.02), night, at(0.0, 1.32, face));
                let stone = pbr(ctx.mat(ctx.branch.cap), SHADE_LIT, Role::Trim, DEEP);
                look.solid(bevel(0.34, 0.06, 0.08), stone, at(0.0, 0.9, face + 0.02));
                for x in [-0.12f32, 0.12] {
                    look.solid(bevel(0.06, 0.84, 0.06), stone, at(x, 1.32, face + 0.02));
                }
                // the pointed arch over it
                for (x, z) in [(-0.06f32, -38.0f32), (0.06, 38.0)] {
                    look.turned(
                        bevel(0.17, 0.05, 0.06),
                        stone,
                        at(x, 1.79, face + 0.02),
                        at(0.0, 0.0, z),
                    );
                }
            }
            if banner {
                // a long cloth hung from an iron rod, a gold hem
                let rod = pbr(ctx.mat("dark_iron"), SHADE_LIT, Role::Trim, DEEP);
                look.turned(
                    cylinder(0.015, 0.015, 0.62),
                    rod,
                    at(0.0, 1.98, face + 0.05),
                    at(0.0, 0.0, 90.0),
                );
                let cloth = Paint::Flat(Color::from_rgb(0.3, 0.05, 0.08), Finish::Matte);
                look.solid(cuboid(0.5, 0.92, 0.012), cloth, at(0.0, 1.5, face + 0.04));
                let gold = pbr(ctx.mat("gold"), SHADE_LIT, Role::Trim, DEEP);
                look.solid(cuboid(0.5, 0.05, 0.016), gold, at(0.0, 1.06, face + 0.045));
                look.solid(cuboid(0.04, 0.88, 0.016), gold, at(0.0, 1.5, face + 0.045));
            }
            // a built place's iron bands round the faces beside a room
            if let (Some(band), false, false) = (ctx.branch.bands, cut, ctx.branch.cave) {
                let iron = pbr(ctx.mat(band), SHADE_LIT, Role::Trim, DEEP);
                let out = w / 2.0 + 0.02;
                for (i, (dx, dz)) in SIDES.iter().enumerate() {
                    if around.sides[i] != Side::Open || (i == 1 && (window || banner)) {
                        continue;
                    }
                    let (mesh, pos) = if *dx == 0.0 {
                        (bevel(w + 0.02, 0.07, 0.05), at(0.0, BAND_Y, dz * out))
                    } else {
                        (bevel(0.05, 0.07, w + 0.02), at(dx * out, BAND_Y, 0.0))
                    };
                    look.solid(mesh, iron, pos);
                }
            }
            for s in &mut look.solids[decor_from..] {
                s.shadow = false;
            }
        }
        Terrain::Floor | Terrain::DarkFloor => {
            look.ground_tile(tile, ground(lit_shade), Vector3::ZERO);
            look.lit = t == Terrain::Floor;
            rubble(look, around, bedrock, ctx, Side::Wall, 0.3);
        }
        Terrain::Corridor => {
            let lit = sym == "S_litcorr";
            look.ground_tile(
                tile,
                ground(if lit { lit_shade } else { 85 }),
                Vector3::ZERO,
            );
            look.lit = lit;
            // stones fallen from the rock along the trench's sides, and
            // kicked to its edges where it opens on a room's floor
            rubble(look, around, bedrock, ctx, Side::Solid, 0.55);
            let rooms = Around {
                sides: around
                    .floor
                    .map(|f| if f { Side::Open } else { Side::Solid }),
                ..*around
            };
            rubble(look, &rooms, bedrock, ctx, Side::Open, 0.7);
            // gravel trodden into the earth: a path, not a tile of colour
            if let Some(m) = bedrock {
                for k in 0..5u32 {
                    let r = 0.025 + 0.02 * ctx.noise(70 + k);
                    let (x, z) = (ctx.noise(75 + k) - 0.5, ctx.noise(80 + k) - 0.5);
                    let rot = at(ctx.noise(85 + k) * 90.0, ctx.noise(90 + k) * 360.0, 0.0);
                    let paint = Paint::Pbr(m, SHADE_RUBBLE, Role::Trim);
                    look.turned(facets(r, 5), paint, at(x * 0.8, r * 0.15, z * 0.8), rot);
                    if let Some(s) = look.solids.last_mut() {
                        s.shadow = false;
                    }
                }
            }
        }
        Terrain::Doorway | Terrain::BrokenDoor => {
            floor(look);
            let along_x = around.wall_along_x();
            let wood = pbr(ctx.mat("wood"), 70, Role::Door, DEEP);
            door_frame(look, along_x, cut, ctx, wood, true);
            // a worn wooden threshold across the passage
            let sill = if along_x {
                cuboid(0.9, 0.03, 0.2)
            } else {
                cuboid(0.2, 0.03, 0.9)
            };
            look.ground_tile(sill, wood, at(0.0, 0.015, 0.0));
            if t == Terrain::BrokenDoor {
                let wood = main(80);
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
        }
        Terrain::OpenDoor | Terrain::ClosedDoor => {
            floor(look);
            let open = t == Terrain::OpenDoor;
            // the passage runs along x through a vertical wall
            let along_x = !vertical;
            let wood = pbr(art.material, SHADE_LIT, Role::Door, FLOOR_UNSEEN);
            let frame = pbr(art.material, 62, Role::Door, FLOOR_UNSEEN);
            match ctx.branch.door.filter(|_| !open && !cut) {
                Some(prop) => {
                    // the branch's own door fills the opening, frame and all;
                    // the masonry above it closes the wall
                    door_frame(look, along_x, cut, ctx, frame, false);
                    let (sx, sy) = match prop {
                        Prop::CastleDoor => (0.96 / 2.01, (DOOR_HEIGHT + LINTEL) / 2.94),
                        _ => (0.96 / 2.96, (DOOR_HEIGHT + LINTEL) / 2.92),
                    };
                    look.props.push(PlacedProp {
                        prop,
                        pos: Vector3::ZERO,
                        yaw: if along_x { 0.0 } else { 90.0 },
                        scale: Vector3::new(sx, sy, 2.0),
                    });
                    look.reach(DOOR_HEIGHT + LINTEL);
                }
                None => {
                    door_frame(look, along_x, cut, ctx, frame, true);
                    door_leaf(look, along_x, open, door_h, wood, trim(SHADE_LIT));
                }
            }
            if !open {
                look.ground = door_h;
            }
        }
        Terrain::IronBars => {
            look.ground_tile(
                tile,
                pbr(floor_mat, lit_shade, Role::Floor, FLOOR_UNSEEN),
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
            // the floor, and a round of earth where the roots go down (a
            // square of earth reads as a tile out of place)
            floor(look);
            look.ground_tile(cylinder(0.44, 0.44, 0.01), ground(80), at(0.0, 0.005, 0.0));
            for i in 0..5 {
                let yaw = i as f32 * 72.0 + ctx.noise(60 + i) * 40.0;
                let (sin, cos) = yaw.to_radians().sin_cos();
                look.turned(
                    bevel(0.07, 0.05, 0.26),
                    ground(60),
                    at(sin * 0.2, 0.02, cos * 0.2),
                    at(-8.0, yaw, 0.0),
                );
                if let Some(s) = look.solids.last_mut() {
                    s.shadow = false;
                }
            }
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
            let stone = main(SHADE_STAIRS);
            // six worn steps rising to the north into an arch: the stair
            // goes up into the dark
            for i in 0..6 {
                let h = 0.1 * (i + 1) as f32;
                let z = 0.375 - 0.15 * i as f32;
                look.solid(bevel(0.76, h, 0.16), stone, at(0.0, h / 2.0, z));
            }
            let side = main(60);
            for x in [-0.44f32, 0.44] {
                look.solid(bevel(0.12, 0.7, 0.92), side, at(x, 0.35, 0.0));
            }
            let arch = trim(SHADE_LIT);
            for x in [-0.42f32, 0.42] {
                look.solid(bevel(0.16, 1.9, 0.3), arch, at(x, 0.95, -0.34));
            }
            look.solid(bevel(1.0, 0.26, 0.34), arch, at(0.0, 2.0, -0.34));
            // the dark beyond the top step
            look.solid(
                cuboid(0.7, 1.2, 0.04),
                Paint::Flat(DEEP, Finish::Matte),
                at(0.0, 1.2, -0.47),
            );
            // whoever stands here stands on the middle step
            look.ground = 0.4;
            look.lamp = Some(Lamp::Up);
            label(look, 1.1);
        }
        Terrain::StairsDown => {
            // a shaft with steps going down, away from the camera, in a
            // rim of stone blocks
            let rim = trim(SHADE_LIT);
            for (mesh, x, z) in [
                (bevel(1.0, 0.1, 0.12), 0.0, -0.44),
                (bevel(1.0, 0.1, 0.12), 0.0, 0.44),
                (bevel(0.12, 0.1, 0.76), -0.44, 0.0),
                (bevel(0.12, 0.1, 0.76), 0.44, 0.0),
            ] {
                look.solid(mesh, rim, at(x, 0.05, z));
            }
            // the shaft's walls of stone, going down into the dark
            let wall = main(55);
            look.solid(bevel(0.8, 1.0, 0.08), wall, at(0.0, -0.5, -0.36));
            for x in [-0.36f32, 0.36] {
                look.solid(bevel(0.08, 1.0, 0.72), wall, at(x, -0.5, 0.0));
            }
            // worn treads going down from the far side towards the camera,
            // their risers and pale worn nosings facing it and catching
            // the cold light from below; the lowest go under the near rim
            let tread = main(SHADE_LIT);
            let riser = main(50);
            let nosing = pbr(floor_mat, 100, Role::Prop, FLOOR_UNSEEN);
            for (i, z) in [-0.28f32, -0.1, 0.08, 0.26].into_iter().enumerate() {
                let top = -0.08 - 0.14 * i as f32;
                let h = top + 0.9;
                look.solid(bevel(0.72, h, 0.18), tread, at(0.0, top - h / 2.0, z));
                look.solid(bevel(0.72, 0.1, 0.02), riser, at(0.0, top - 0.06, z + 0.09));
                look.solid(
                    bevel(0.72, 0.03, 0.04),
                    nosing,
                    at(0.0, top - 0.005, z + 0.08),
                );
            }
            look.sunk = true;
            look.lamp = Some(Lamp::Down);
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
            look.solid(bevel(0.76, 0.45, 0.56), stone, at(0.0, 0.225, 0.0));
            look.solid(bevel(0.9, 0.06, 0.7), main(SHADE_LIT), at(0.0, 0.48, 0.0));
            // the altar's alignment colour as a runner cloth
            let cloth = Paint::Flat(darker(nh_color(c), 0.35), Finish::Matte);
            look.solid(cuboid(0.3, 0.012, 0.72), cloth, at(0.0, 0.516, 0.0));
            look.ground = 0.51;
        }
        Terrain::Throne => {
            floor(look);
            let gold = main(SHADE_LIT);
            look.solid(bevel(0.62, 0.2, 0.6), trim(SHADE_LIT), at(0.0, 0.1, 0.0));
            look.solid(bevel(0.6, 0.18, 0.55), gold, at(0.0, 0.29, 0.02));
            look.solid(bevel(0.6, 0.8, 0.12), gold, at(0.0, 0.78, -0.24));
            for x in [-0.28, 0.28] {
                look.solid(bevel(0.07, 0.22, 0.5), gold, at(x, 0.49, 0.0));
            }
            let velvet = Paint::Flat(Color::from_rgb(0.35, 0.04, 0.06), Finish::Matte);
            look.solid(cuboid(0.48, 0.03, 0.45), velvet, at(0.0, 0.395, 0.04));
            look.ground = 0.38;
        }
        Terrain::Fountain => {
            floor(look);
            let stone = main(SHADE_LIT);
            // a low basin of stone blocks round still water, a small
            // spout in its middle
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                let p = at(0.4 * a.sin(), 0.13, 0.4 * a.cos());
                look.turned(
                    bevel(0.33, 0.26, 0.12),
                    stone,
                    p,
                    at(0.0, i as f32 * 45.0, 0.0),
                );
            }
            let water = Paint::Flat(Color::from_rgb(0.04, 0.09, 0.12), Finish::Glossy);
            look.solid(cylinder(0.4, 0.4, 0.02), water, at(0.0, 0.2, 0.0));
            look.solid(bevel(0.16, 0.36, 0.16), stone, at(0.0, 0.2, 0.0));
            look.solid(cylinder(0.12, 0.05, 0.06), stone, at(0.0, 0.41, 0.0));
            look.ground = 0.26;
        }
        Terrain::Sink => {
            floor(look);
            let stone = main(SHADE_LIT);
            look.solid(bevel(0.6, 0.4, 0.5), stone, at(0.0, 0.2, 0.0));
            look.solid(
                plane(0.44, 0.34),
                Paint::Flat(DEEP, Finish::Glossy),
                at(0.0, 0.405, 0.0),
            );
            let metal = pbr(ctx.mat("metal"), SHADE_LIT, Role::Prop, DEEP);
            look.solid(cylinder(0.02, 0.02, 0.2), metal, at(0.0, 0.5, -0.2));
            look.ground = 0.4;
        }
        Terrain::Grave => {
            look.ground_tile(tile, ground(75), Vector3::ZERO);
            look.solid(bevel(0.5, 0.12, 0.62), main(60), at(0.0, 0.06, 0.12));
            let stone = trim(80);
            look.turned(
                bevel(0.48, 0.6, 0.1),
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
            let deep = if t == Terrain::Water { 0.1 } else { 0.05 };
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Liquid(Liquid::Water),
                at(0.0, -deep, 0.0),
            );
            if let Some(s) = look.solids.last_mut() {
                s.shadow = false;
            }
            // foam lapping at every bank, its stone side out
            for (i, (dx, dz)) in SIDES.iter().enumerate() {
                if around.liquid[i] {
                    continue;
                }
                let yaw = match (*dx as i32, *dz as i32) {
                    (0, -1) => 0.0,
                    (0, _) => 180.0,
                    (-1, _) => 90.0,
                    _ => -90.0,
                };
                look.turned(
                    plane(1.0, 0.24),
                    Paint::Liquid(Liquid::Foam),
                    at(dx * 0.38, -deep + 0.004, dz * 0.38),
                    at(0.0, yaw, 0.0),
                );
                if let Some(s) = look.solids.last_mut() {
                    s.shadow = false;
                }
            }
            liquid_bed(
                look,
                around,
                ctx,
                if t == Terrain::Water { 0.9 } else { 0.5 },
            );
            look.ground = -deep;
            look.sunk = true;
        }
        Terrain::Ice => look.ground_tile(tile, main(SHADE_LIT), Vector3::ZERO),
        Terrain::Lava => {
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Liquid(Liquid::Lava),
                at(0.0, -0.06, 0.0),
            );
            liquid_bed(look, around, ctx, 0.12);
            // the air shimmers over it, and it lights its banks: every
            // third cell of a lake, every cell of a narrow flow
            let banks = around.liquid.iter().filter(|l| !**l).count();
            if (ctx.x + 2 * ctx.y).rem_euclid(3) == 0 || banks >= 2 {
                look.ground_tile(
                    plane(1.0, 1.6),
                    Paint::Liquid(Liquid::Haze),
                    at(0.0, -0.06, 0.0),
                );
                look.lamp = Some(Lamp::Lava);
            }
            look.top = look.top.min(0.1);
            look.ground = -0.06;
            look.sunk = true;
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
            look.sunk = true;
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
            trap_look(look, sym, nh_color(c), ctx);
            // its letter names it under the pointer and in the overview
            label(look, 0.45);
        }
    }
}

/// A trap as the hero sees it, by its kind: jaws of iron, a pit, a trap
/// door, a pressure plate with the holes of darts, a loose board, a mine,
/// a vent, a web, or a circle of runes glowing faintly in the trap's
/// colour for the magical ones.
fn trap_look(look: &mut Look, sym: &str, color: Color, ctx: &Ctx) {
    let paint = |name: &str, shade: u8| {
        ctx.mat(name).map_or(Paint::Flat(DEEP, Finish::Matte), |m| {
            Paint::Pbr(m, shade, Role::Prop)
        })
    };
    let iron = paint("iron", SHADE_LIT);
    let wood = paint("wood", 80);
    let stone = paint("bedrock", 100);
    let hole = Paint::Flat(Color::from_rgb(0.006, 0.006, 0.008), Finish::Matte);
    let flat = |look: &mut Look, mesh: MeshKey, paint: Paint, pos: Vector3, yaw: f32| {
        look.turned(mesh, paint, pos, at(0.0, yaw, 0.0));
        if let Some(s) = look.solids.last_mut() {
            s.shadow = false;
        }
    };
    let rim = |look: &mut Look, paint: Paint| {
        for (x, z, w, d) in [
            (0.0, -0.4, 0.9, 0.1),
            (0.0, 0.4, 0.9, 0.1),
            (-0.4, 0.0, 0.1, 0.7),
            (0.4, 0.0, 0.1, 0.7),
        ] {
            look.solid(bevel(w, 0.05, d), paint, at(x, 0.025, z));
        }
    };
    match sym {
        "S_bear_trap" => {
            // two iron jaws open on the floor, teeth up, a chain to a peg
            for dz in [-0.1f32, 0.1] {
                flat(look, bevel(0.46, 0.03, 0.06), iron, at(0.0, 0.015, dz), 0.0);
                for i in 0..5 {
                    let x = -0.18 + 0.09 * i as f32;
                    look.solid(prism(0.04, 0.06, 0.03), iron, at(x, 0.06, dz));
                }
            }
            look.solid(bevel(0.1, 0.03, 0.1), iron, at(0.0, 0.03, 0.0));
            flat(
                look,
                cuboid(0.3, 0.015, 0.02),
                iron,
                at(0.3, 0.008, 0.18),
                35.0,
            );
        }
        "S_pit" | "S_spiked_pit" | "S_hole" => {
            flat(look, plane(0.78, 0.78), hole, at(0.0, 0.004, 0.0), 0.0);
            rim(look, stone);
            if sym == "S_spiked_pit" {
                for (x, z) in [
                    (-0.2, -0.15),
                    (0.15, -0.2),
                    (0.0, 0.1),
                    (-0.15, 0.22),
                    (0.22, 0.15),
                ] {
                    look.solid(prism(0.06, 0.2, 0.06), iron, at(x, 0.03, z));
                }
            }
        }
        "S_trap_door" => {
            flat(look, plane(0.8, 0.8), hole, at(0.0, 0.003, 0.0), 0.0);
            for i in 0..4 {
                let x = -0.29 + 0.195 * i as f32;
                flat(look, bevel(0.18, 0.04, 0.74), wood, at(x, 0.02, 0.0), 0.0);
            }
            for z in [-0.25f32, 0.25] {
                flat(look, cuboid(0.8, 0.02, 0.06), iron, at(0.0, 0.045, z), 0.0);
            }
        }
        "S_arrow_trap" | "S_dart_trap" => {
            flat(
                look,
                bevel(0.36, 0.03, 0.36),
                stone,
                at(0.0, 0.015, 0.0),
                12.0,
            );
            for (x, z) in [(-0.08, -0.08), (0.08, -0.06), (0.0, 0.09)] {
                flat(look, cylinder(0.02, 0.02, 0.01), hole, at(x, 0.032, z), 0.0);
            }
            let len = if sym == "S_arrow_trap" { 0.5 } else { 0.22 };
            look.turned(
                cylinder(0.01, 0.01, len),
                wood,
                at(0.2, 0.012, 0.25),
                at(90.0, 70.0, 0.0),
            );
        }
        "S_falling_rock_trap" => {
            for (i, (x, z)) in [(-0.1, 0.05), (0.12, -0.08), (0.05, 0.16), (-0.15, -0.14)]
                .into_iter()
                .enumerate()
            {
                let r = 0.06 + 0.03 * (i % 2) as f32;
                look.turned(
                    facets(r, 5),
                    stone,
                    at(x, r * 0.6, z),
                    at(20.0 * i as f32, 50.0 * i as f32, 0.0),
                );
            }
        }
        "S_squeaky_board" | "S_rolling_boulder_trap" => {
            // a loose board, lifted at one end
            look.turned(
                bevel(0.74, 0.04, 0.2),
                wood,
                at(0.0, 0.035, 0.0),
                at(0.0, 8.0, 4.0),
            );
            if sym == "S_rolling_boulder_trap" {
                flat(look, bevel(0.3, 0.02, 0.3), stone, at(0.0, 0.01, 0.3), 0.0);
            }
        }
        "S_land_mine" => {
            look.solid(dome(0.1), iron, at(0.0, 0.0, 0.0));
            look.solid(cylinder(0.015, 0.015, 0.06), iron, at(0.0, 0.1, 0.0));
        }
        "S_sleeping_gas_trap" | "S_rust_trap" | "S_fire_trap" => {
            // a vent in an iron grate, stained by what comes out of it
            flat(look, bevel(0.5, 0.03, 0.5), iron, at(0.0, 0.015, 0.0), 0.0);
            for i in 0..4 {
                let x = -0.15 + 0.1 * i as f32;
                flat(look, cuboid(0.04, 0.012, 0.4), hole, at(x, 0.031, 0.0), 0.0);
            }
            // a centimetre thick: a mesh of whole centimetres rounds a
            // thinner one to nothing, and Godot cannot light that
            let stain = Paint::Flat(darker(color, 0.6).with_alpha(0.5), Finish::Ghost);
            flat(
                look,
                cylinder(0.4, 0.4, 0.01),
                stain,
                at(0.0, 0.005, 0.0),
                0.0,
            );
        }
        "S_web" => {
            let silk = Paint::Flat(Color::from_rgb(0.85, 0.85, 0.82), Finish::Ghost);
            for i in 0..6 {
                flat(
                    look,
                    cuboid(0.9, 0.006, 0.008),
                    silk,
                    at(0.0, 0.02, 0.0),
                    30.0 * i as f32,
                );
            }
            for r in [0.15f32, 0.3] {
                flat(look, torus(r, r + 0.01), silk, at(0.0, 0.021, 0.0), 0.0);
            }
        }
        _ => {
            // magic: a disc of stone with a ring of runes glowing faintly
            flat(
                look,
                cylinder(0.36, 0.38, 0.03),
                stone,
                at(0.0, 0.015, 0.0),
                0.0,
            );
            let glow = Paint::Flat(darker(color, 0.35), Finish::Glow);
            flat(look, torus(0.27, 0.3), glow, at(0.0, 0.032, 0.0), 0.0);
            flat(look, torus(0.1, 0.12), glow, at(0.0, 0.032, 0.0), 0.0);
            for i in 0..6 {
                let a = (i as f32 * 60.0).to_radians();
                let p = at(0.2 * a.sin(), 0.032, 0.2 * a.cos());
                flat(look, cuboid(0.02, 0.005, 0.07), glow, p, i as f32 * 60.0);
            }
        }
    }
    // a trap is never taller to the pointer than a step
    look.top = look.top.min(0.3);
}

/// The rock the level is cut into, on a cell nothing is known of (or
/// solid stone): next to a corridor or a doorway, or on the outer side of
/// a wall (a wall and no floor next to it: a dark room's unexplored floor
/// is stone too). Corridors become trenches in it. In front of open ground
/// (one or two cells north) it is cut down so it hides nothing.
fn rock_look(look: &mut Look, near: &Near, ctx: &Ctx) {
    let catalog = ctx.catalog;
    let mut corridor = false;
    let mut wall = false;
    let mut floor = false;
    for c in near.cells.iter().flatten() {
        match cell_terrain(c, catalog) {
            Some(
                Terrain::Corridor
                | Terrain::Doorway
                | Terrain::OpenDoor
                | Terrain::ClosedDoor
                | Terrain::BrokenDoor,
            ) => corridor = true,
            Some(Terrain::Wall) => wall = true,
            Some(Terrain::Stone | Terrain::Unknown | Terrain::Effect) | None => {
                // an unseen floor under something shown there
                floor |= c.entity().is_some() && c.terrain.is_none();
            }
            Some(_) => floor = true,
        }
    }
    if !(corridor || (wall && !floor)) {
        return;
    }
    let Some(m) = ctx.mat("bedrock") else {
        return;
    };
    // one or two cells south of open ground (behind a room's south wall
    // cut down, too) it would hide the ground from the camera
    let h = if is_open(near.cells[0], catalog) || is_open(near.far_north, catalog) {
        ROCK_CUT
    } else {
        ROCK_HEIGHT
    };
    look.solid(
        rock(1.0, h, 1.0),
        Paint::Pbr(m, SHADE_ROCK, Role::Rock),
        Vector3::ZERO,
    );
    // broken up by the shader, it would shadow itself in speckles; the
    // walls keep the torches' light in the rooms
    if let Some(s) = look.solids.last_mut() {
        s.shadow = false;
    }
    // its broken top reaches a little higher
    look.reach(h + 0.08);
    look.ground = h;
    look.rock = true;
    look.seen = Seen::Rock;
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
                let tint = lifted(tint_color(r.tint, g.color));
                let yaw = match (hero, ctx.facing) {
                    (true, _) => ctx.hero_yaw,
                    (false, Some(yaw)) => yaw,
                    (false, None) => ctx.face_hero(),
                };
                look.model(ModelLook { art: r, tint, pose }, here, yaw);
                look.entity = Some(look.models.len() - 1);
            }
            look.hostile = !hero && g.flags & mg::PET == 0;
            if g.flags & mg::PET != 0 {
                look.ring = Some(PET_RING);
            }
            if g.flags & mg::DETECT == 0 && !hero {
                look.seen = Seen::Creature;
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
            if g.flags & mg::OBJPILE == 0 {
                look.entity = Some(look.models.len() - 1);
            }
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
                Some(m) => Paint::Pbr(m, 70, Role::Prop),
                None => Paint::Flat(Color::from_rgb(0.5, 0.5, 0.52), Finish::Matte),
            };
            look.solid(
                bevel(0.6, plinth, 0.6),
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
        GlyphKind::Zap => bright_flash(look, g, ctx),
        GlyphKind::Explosion => {
            bright_flash(look, g, ctx);
            look.blast = Some(lighter(nh_color(g.color), 0.2));
        }
        // the engulfer is drawn once, around the hero
        _ => {}
    }
}

/// A beam, explosion or sparkle for the moment it shows: a ray is a
/// piece of a glowing ray along its direction (the glyph says which), the
/// rest a burst of glints; the effects draw them (`Vfx`), no solid stands
/// in the cell.
fn bright_flash(look: &mut Look, g: &Glyph, ctx: &Ctx) {
    let color = lighter(nh_color(g.color), 0.3);
    let y = look.ground.max(0.0) + 0.6;
    let sym = if g.kind == GlyphKind::Zap {
        // a zap's direction is in its character
        match glyph_char(g) {
            Some('|') => "S_vbeam",
            Some('-') => "S_hbeam",
            Some('\\') => "S_lslant",
            Some('/') => "S_rslant",
            _ => "",
        }
    } else {
        cmap_sym(g, ctx.catalog).unwrap_or("")
    };
    let yaw = match sym {
        "S_vbeam" => Some(0.0),
        "S_hbeam" => Some(90.0),
        "S_lslant" => Some(45.0),
        "S_rslant" => Some(-45.0),
        _ => None,
    };
    match yaw {
        Some(yaw) => look.ray = Some((color, yaw, y)),
        None => look.glint = Some((color, y)),
    }
    look.reach(y + 0.3);
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

/// The neighbours of a cell: north (y - 1), south, west, east, then
/// north-west, north-east, south-west, south-east; and the cell two north.
#[derive(Clone, Copy, Default)]
struct Near<'a> {
    cells: [Option<&'a Cell>; 8],
    far_north: Option<&'a Cell>,
}

impl<'a> Near<'a> {
    fn of(map: &'a MapState, x: i32, y: i32) -> Near<'a> {
        Near {
            cells: [
                map.cell(x, y - 1),
                map.cell(x, y + 1),
                map.cell(x - 1, y),
                map.cell(x + 1, y),
                map.cell(x - 1, y - 1),
                map.cell(x + 1, y - 1),
                map.cell(x - 1, y + 1),
                map.cell(x + 1, y + 1),
            ],
            far_north: map.cell(x, y - 2),
        }
    }

    /// Only the orthogonal neighbours known (tests).
    #[cfg(test)]
    fn orth(cells: [Option<&'a Cell>; 4]) -> Near<'a> {
        let mut near = Near::default();
        near.cells[..4].copy_from_slice(&cells);
        near
    }

    /// The floor most neighbours show (lit, dark or corridor; ties in that
    /// order), as one of their terrain glyphs.
    fn floor(&self, catalog: &Catalog) -> Option<&'a Glyph> {
        let floors = [Terrain::Floor, Terrain::DarkFloor, Terrain::Corridor];
        let known: Vec<(Terrain, &Glyph)> = self.cells[..4]
            .iter()
            .copied()
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
            let terrain = terrain_of(sym);
            terrain_look(&mut look, terrain, sym, t, &near, ctx);
            if terrain == Terrain::Stone && cell.entity().is_none() {
                rock_look(&mut look, &near, ctx);
            }
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
                let open = Near::default();
                terrain_look(&mut look, terrain_of(sym), sym, g, &open, ctx);
            }
            // none known: the branch's floor (a flat grey tile reads as
            // a highlight under whoever stands there)
            None => {
                let paint = match ctx.mat("floor") {
                    Some(m) => Paint::Pbr(m, SHADE_LIT, Role::Floor),
                    None => Paint::Flat(FLOOR_UNSEEN, Finish::Matte),
                };
                look.ground_tile(plane(1.0, 1.0), paint, Vector3::ZERO);
            }
        },
        None => rock_look(&mut look, &near, ctx),
    }
    match &cell.glyph {
        Some(g) if g.kind == GlyphKind::Cmap => {
            if cmap_sym(g, catalog).is_some_and(|s| terrain_of(s) == Terrain::Effect) {
                bright_flash(&mut look, g, ctx);
                if cmap_sym(g, catalog).is_some_and(|s| s.starts_with("S_expl")) {
                    look.blast = Some(lighter(nh_color(g.color), 0.2));
                }
            }
        }
        Some(g) => entity_look(&mut look, g, ctx),
        None => {}
    }
    look
}

/// A feature's light (and mist) on its cell.
struct CellLamp {
    kind: Lamp,
    light: Gd<OmniLight3D>,
    mist: Option<Gd<FogVolume>>,
}

#[derive(Default)]
struct CellNodes {
    look: Look,
    /// The solids' places in the batches, one per solid of the look.
    solids: Vec<Slot>,
    letters: Vec<Gd<Label3D>>,
    models: Vec<Model>,
    ring: Option<Gd<Decal>>,
    lamp: Option<CellLamp>,
    props: Vec<(Prop, Gd<Node3D>)>,
}

impl CellNodes {
    fn free(
        self,
        art: &mut Art,
        batches: &mut Batches,
        lamps: &mut Vec<CellLamp>,
        props: &mut HashMap<Prop, Vec<Gd<Node3D>>>,
    ) {
        for (prop, mut node) in self.props {
            node.set_visible(false);
            props.entry(prop).or_default().push(node);
        }
        for slot in self.solids {
            batches.remove(slot);
        }
        for mut n in self.letters {
            n.queue_free();
        }
        for m in self.models {
            art.give(m);
        }
        if let Some(mut d) = self.ring {
            d.queue_free();
        }
        if let Some(l) = self.lamp {
            lamps.push(l.hidden());
        }
    }
}

impl CellLamp {
    /// Put out, to be lit again elsewhere: a light freed while the
    /// renderer pairs it with geometry upsets Godot (a crash at exit).
    fn hidden(mut self) -> CellLamp {
        self.light.set_visible(false);
        if let Some(m) = self.mist.as_mut() {
            m.set_visible(false);
        }
        self
    }
}

/// A model taken off the cell an entity left, for the cell it stepped to.
struct Carried {
    model: Model,
    placed: Placed,
    /// The world position it was at.
    from: Vector3,
    /// Its previous step had not ended when this one came: it runs.
    hurry: bool,
    /// Cells crossed: 1, or 2 for a fast monster's two steps in a turn.
    cells: i32,
}

/// What the animator did (self-tests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MotionStats {
    pub hero_steps: u32,
    pub other_steps: u32,
    pub strikes: u32,
    /// Blows that made their target reel.
    pub flinches: u32,
}

/// Frame times summed for `MapView::frame_stats`.
#[derive(Default)]
struct FrameStats {
    frames: u32,
    secs: f64,
    gpu: f64,
    cpu: f64,
    draws: f64,
    /// The longest frame.
    worst: f64,
    /// The last sync's time (ms) and models it built.
    last_sync: f64,
    last_built: usize,
    /// Seconds since the last level change.
    since_level: f64,
    /// Pipelines compiled so far (by kind) and video memory (MB), as of
    /// the frame before.
    pipelines: [i64; 5],
    vmem: f64,
    /// The process frame and the time of the last sync.
    clock: Option<(u64, std::time::Instant)>,
}

/// A torch on a wall: the sconce, its flame and its light.
struct Torch {
    node: Gd<Node3D>,
    /// The torch in its sconce, and a lantern hung there instead in the
    /// branches that have them (made when first needed).
    fire: Gd<Node3D>,
    lantern: Option<Gd<Node3D>>,
    flame: Gd<MeshInstance3D>,
    embers: Gd<godot::classes::GpuParticles3D>,
    light: Gd<OmniLight3D>,
    /// Where the light burns when it does not flicker.
    at: Vector3,
    /// Its own flicker (seconds ahead of the clock).
    phase: f64,
    shadow: bool,
}

pub struct MapView {
    root: Gd<Node3D>,
    cells_root: Gd<Node3D>,
    camera: Gd<Camera3D>,
    env: Gd<Environment>,
    cells: HashMap<(i32, i32), CellNodes>,
    generation: Option<u64>,
    art: Art,
    font: Gd<SystemFont>,
    hover: Gd<Decal>,
    /// The cell under the pointer.
    hover_cell: Option<(i32, i32)>,
    hostile_ring: Gd<Decal>,
    /// The model outlined under the pointer, and the outline materials
    /// (a hostile's, anything else's).
    outlined: Option<Gd<Node3D>>,
    outline: (Gd<Material>, Gd<Material>),
    /// The hero seen through walls, and whether it is on.
    xray: Option<Gd<Material>>,
    xray_on: bool,
    /// Where the camera is.
    eye: Vector3,
    /// Cells with letters shown only under the pointer or in the overview.
    hint_cells: HashSet<(i32, i32)>,
    hints_dirty: bool,
    cursor: Gd<Decal>,
    hero_ring: Gd<Decal>,
    ring_tex: Gd<godot::classes::Texture2D>,
    hero_light: Gd<OmniLight3D>,
    /// The glow from below round the hero where the floors burn.
    underglow: Gd<OmniLight3D>,
    rim: Gd<OmniLight3D>,
    /// The hero's model, on the rim light's layer too.
    rim_model: Option<Gd<Node3D>>,
    /// Fill lights over the parts of the level in view.
    room_lights: Vec<Gd<OmniLight3D>>,
    torches: Vec<Torch>,
    torch_scene: Option<Gd<PackedScene>>,
    flame_mat: Gd<Material>,
    halo_mat: Gd<Material>,
    /// The solids of every cell, batched.
    batches: Batches,
    /// The map's surface shader's materials, and the fog of war it reads.
    surfaces: Surfaces,
    fow: Fow,
    /// A cell's sight changed, or the hero moved: work the fog out again.
    fow_dirty: bool,
    vfx: Vfx,
    /// Low mist over the level.
    mist: Gd<FogVolume>,
    /// Profiling: time of the parts of the last sync (ms).
    prof: Vec<(&'static str, f64)>,
    /// Lamps of features put out, to be lit again elsewhere.
    spare_lamps: Vec<CellLamp>,
    /// The branches' models: their scenes, and those taken off cells.
    prop_scenes: HashMap<Prop, Option<Gd<PackedScene>>>,
    spare_props: HashMap<Prop, Vec<Gd<Node3D>>>,
    /// Water, lava and the air over lava: their materials.
    liquids: HashMap<Liquid, Gd<Material>>,
    /// The cells of a new level still to draw, the nearest the hero last.
    building: Vec<(i32, i32)>,
    /// The branch the level drawn is in, and how it looks.
    branch: Branch,
    branch_look: BranchLook,
    /// The post layer's material, and how far a new level is still
    /// hidden in black (1 when it is drawn, 0 when it shows).
    post_mat: Option<Gd<ShaderMaterial>>,
    fade: f32,
    /// Frame times being summed (RENETHACK_FRAME_STATS).
    stats_window: Option<FrameStats>,
    /// The art loaded ahead has been told of (RENETHACK_FRAME_STATS).
    preload_told: bool,
    /// The level played behind the title screen (render pipelines
    /// compiled ahead), until it is over; and whether the map is shown.
    rehearsal: Option<crate::rehearsal::Rehearsal>,
    rehearsed: bool,
    /// The shadow atlas has been made (the rehearsal casts a shadow).
    shadow_made: bool,
    /// A frame's time for drawing a new level's cells (`BUILD_BUDGET`, or
    /// less where the rehearsal draws a level's first frames).
    build_budget: std::time::Duration,
    /// How many of the map's parts draw behind the title (see
    /// `show_parts`).
    showing: u8,
    shadow_block: Option<Gd<MeshInstance3D>>,
    shadow_frames: u32,
    /// A card the rehearsal shows textures on (see `show_texture`).
    card: Option<Gd<MeshInstance3D>>,
    /// Embers made ahead for the first torch.
    spare_embers: Option<Gd<godot::classes::GpuParticles3D>>,
    /// When the last title frame began, and the art's and the
    /// rehearsal's work in it (RENETHACK_FRAME_STATS).
    title_clock: Option<std::time::Instant>,
    title_work: (std::time::Duration, std::time::Duration, Option<String>),
    /// The art gallery's light, and whether it is on.
    studio: Gd<DirectionalLight3D>,
    showcase: bool,
    shown: bool,
    /// The lit areas changed: place the fill lights again.
    lights_dirty: bool,
    /// Dark rock under the whole level, with a hole where something lies
    /// below the ground.
    bedrock: Gd<MeshInstance3D>,
    holes: HashSet<(i32, i32)>,
    bedrock_dirty: bool,
    /// The vignette and grain over the map, under the HUD.
    post: Gd<CanvasLayer>,
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
    /// Which way a monster faces since it stepped or fought, while the
    /// same one stands on the cell.
    facing: HashMap<(i32, i32), (Ident, f32)>,
    /// Models on their way to the cell an entity stepped to (this batch).
    incoming: HashMap<(i32, i32), Carried>,
    motions: Vec<Motion>,
    /// Cells whose step was cut short by the next batch.
    interrupted: HashSet<(i32, i32)>,
    /// The newest message already looked at for fights.
    log_seq: Option<u64>,
    /// Self-tests stop motions at this share to take a picture.
    hold: Option<f32>,
    stats: MotionStats,
    /// An order walks the hero: the length of its steps.
    order_pace: Option<f32>,
    /// The marks of the way shown, and whether it is a fight's.
    path_marks: Vec<Gd<Decal>>,
    soft_tex: Gd<godot::classes::Texture2D>,
    /// Cells with a ring under their creature (it follows the model).
    ring_cells: HashSet<(i32, i32)>,
    path_shown: (Vec<(i32, i32)>, bool),
    /// What the hero carries, drawn on their model.
    hero_gear: crate::hero::HeroGear,
    /// How far above the ground the camera aims (self-test close-ups).
    aim_lift: f32,
    /// The effects of the items the hero uses.
    hero_fx: crate::hero::HeroFx,
}

/// A marker projected on the level's surfaces (never on models): a ring
/// or a frame `size` metres across, hidden until placed.
fn marker(root: &mut Gd<Node3D>, texture: &Gd<godot::classes::Texture2D>, size: f32) -> Gd<Decal> {
    use godot::classes::decal::DecalTexture;
    let mut d = Decal::new_alloc();
    d.set_size(Vector3::new(size, 0.24, size));
    d.set_texture(DecalTexture::ALBEDO, texture);
    d.set_texture(DecalTexture::EMISSION, texture);
    d.set_albedo_mix(0.25);
    d.set_emission_energy(0.5);
    d.set_cull_mask(TERRAIN_LAYER);
    d.set_upper_fade(0.3);
    d.set_lower_fade(0.3);
    // on the ground only: never up the side of a wall or a step
    d.set_normal_fade(0.6);
    d.set_visible(false);
    root.add_child(&d);
    d
}

/// A low mist lying over the whole level, thin, in slow drifting patches.
fn ground_mist(root: &mut Gd<Node3D>) -> Gd<FogVolume> {
    use godot::classes::{FastNoiseLite, NoiseTexture3D};
    let mut noise = FastNoiseLite::new_gd();
    noise.set_frequency(0.03);
    let mut tex = NoiseTexture3D::new_gd();
    tex.set_width(64);
    tex.set_height(16);
    tex.set_depth(64);
    tex.set_seamless(true);
    tex.set_noise(&noise);
    let mut mat = FogMaterial::new_gd();
    mat.set_density(0.005);
    mat.set_albedo(Color::from_rgb(0.62, 0.64, 0.7));
    mat.set_height_falloff(2.0);
    mat.set_edge_fade(0.3);
    mat.set_density_texture(&tex);
    let mut fog = FogVolume::new_alloc();
    fog.set_size(Vector3::new(COLNO as f32 + 20.0, 0.6, ROWNO as f32 + 20.0));
    fog.set_position(Vector3::new(COLNO as f32 / 2.0, 0.25, ROWNO as f32 / 2.0));
    fog.set_material(&mat);
    root.add_child(&fog);
    fog
}

/// Every geometry under `node` drawn once more with `overlay` (None: not).
fn set_overlay(node: &Gd<Node3D>, overlay: Option<&Gd<Material>>) {
    for n in node
        .find_children_ex("*")
        .type_("GeometryInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if let Ok(mut g) = n.try_cast::<GeometryInstance3D>() {
            match overlay {
                Some(m) => g.set_material_overlay(m),
                None => g.set_material_overlay(Option::<&Gd<Material>>::None),
            }
        }
    }
}

/// The colour a creature bleeds, by its class (None: it does not).
fn blood_of(info: &MonsterInfo) -> Option<Color> {
    let rgb = Color::from_rgb;
    match info.class.as_str() {
        // the undead, elementals, vortices, golems, lights, fungi
        "Z" | "M" | "V" | "W" | "L" | "E" | "v" | "'" | "y" | "F" | "X" => None,
        // jellies, puddings and blobs: acid and ichor
        "j" | "P" | "b" => Some(rgb(0.25, 0.42, 0.08)),
        // insects and spiders
        "a" | "s" | "x" => Some(rgb(0.3, 0.36, 0.06)),
        _ => Some(rgb(0.35, 0.03, 0.03)),
    }
}

/// A torch's brightness at time `t`: a few slow waves and a quick one.
fn flicker(t: f64) -> f32 {
    let t = t as f32;
    1.0 + 0.07 * (t * 7.3).sin() + 0.05 * (t * 13.7 + 1.3).sin() + 0.03 * (t * 29.1 + 0.4).sin()
}

/// How far a flame's light sways at time `t`: a few centimetres, so the
/// shadows it casts move.
fn sway(t: f64) -> Vector3 {
    let t = t as f32;
    let tau = std::f32::consts::TAU;
    Vector3::new(
        0.02 * (t * 5.1 * tau).sin() + 0.01 * (t * 8.7 * tau + 0.7).sin(),
        0.0,
        0.02 * (t * 8.7 * tau).sin() + 0.01 * (t * 5.1 * tau + 2.1).sin(),
    )
}

/// The camera's pitch at a distance: from the side close in, from above
/// far out.
fn pitch_at(distance: f32) -> f32 {
    let deg = if distance <= DISTANCE {
        let t = ((distance - MIN_DISTANCE) / (DISTANCE - MIN_DISTANCE)).clamp(0.0, 1.0);
        PITCH_NEAR_DEG + (PITCH_DEG - PITCH_NEAR_DEG) * t
    } else {
        let t = ((distance - DISTANCE) / (MAX_DISTANCE - DISTANCE)).clamp(0.0, 1.0);
        PITCH_DEG + (PITCH_FAR_DEG - PITCH_DEG) * t
    };
    deg.to_radians()
}

/// Every mesh under `node` on these render layers.
fn set_layers(node: &Gd<Node3D>, mask: u32) {
    // geometry only: the lights the hero carries (a lamp) keep their own
    // layers, and a light's layers changed while the renderer pairs it
    // with geometry crashes Godot at exit
    for n in node
        .find_children_ex("*")
        .type_("GeometryInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if let Ok(mut g) = n.try_cast::<GeometryInstance3D>()
            && g.get_layer_mask() != mask
        {
            g.set_layer_mask(mask);
        }
    }
}

impl MapView {
    pub fn new(mut root: Gd<Node3D>) -> MapView {
        // darkness is the default: the level shows where light falls on it
        let mut env = Environment::new_gd();
        env.set_background(BgMode::COLOR);
        env.set_bg_color(DARKNESS);
        env.set_ambient_source(AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.28, 0.32, 0.46));
        env.set_ambient_light_energy(0.12);
        env.set_reflection_source(ReflectionSource::DISABLED);
        // AgX keeps the hue of fire and rolls the highlights off gently
        env.set_tonemapper(ToneMapper::AGX);
        env.set_tonemap_exposure(1.0);
        env.set_adjustment_enabled(true);
        // contrast pivots on mid-grey: anything above 1 crushes the darks
        // to pure black, and AgX has contrast enough
        env.set_adjustment_contrast(1.05);
        env.set_adjustment_saturation(0.92);
        env.set_fog_enabled(true);
        env.set_fog_mode(FogMode::DEPTH);
        env.set_fog_light_color(DARKNESS);
        env.set_fog_density(1.0);
        env.set_fog_depth_curve(1.6);
        // only flames and glowing things bloom
        env.set_glow_enabled(true);
        env.set_glow_normalized(false);
        for (level, intensity) in [0.0, 0.0, 0.6, 1.0, 0.8, 0.3, 0.0].into_iter().enumerate() {
            env.set_glow_level(level as i32, intensity);
        }
        env.set_glow_intensity(0.9);
        env.set_glow_strength(1.0);
        env.set_glow_bloom(0.0);
        env.set_glow_blend_mode(GlowBlendMode::SOFTLIGHT);
        env.set_glow_hdr_bleed_threshold(1.1);
        env.set_glow_hdr_bleed_scale(2.0);
        env.set_glow_hdr_luminance_cap(12.0);
        // contact shadows in corners and at the foot of walls
        env.set_ssao_enabled(true);
        env.set_ssao_radius(1.1);
        env.set_ssao_intensity(2.4);
        env.set_ssao_power(1.6);
        env.set_ssao_detail(0.6);
        env.set_ssao_horizon(0.06);
        env.set_ssao_sharpness(0.98);
        env.set_ssao_direct_light_affect(0.25);
        env.set_ssao_ao_channel_affect(0.6);
        // torchlight thrown back from the walls onto the floor
        env.set_ssil_enabled(true);
        env.set_ssil_radius(3.5);
        env.set_ssil_intensity(1.3);
        env.set_ssil_sharpness(0.98);
        env.set_ssil_normal_rejection(1.0);
        // a thin haze that lights up around the flames
        env.set_volumetric_fog_enabled(true);
        env.set_volumetric_fog_density(0.003);
        env.set_volumetric_fog_albedo(Color::from_rgb(0.60, 0.60, 0.66));
        env.set_volumetric_fog_emission(Color::from_rgb(0.0, 0.0, 0.0));
        env.set_volumetric_fog_anisotropy(0.35);
        env.set_volumetric_fog_length(28.0);
        env.set_volumetric_fog_detail_spread(2.0);
        env.set_volumetric_fog_gi_inject(0.0);
        env.set_volumetric_fog_ambient_inject(0.0);
        env.set_volumetric_fog_temporal_reprojection_enabled(true);
        env.set_volumetric_fog_temporal_reprojection_amount(0.9);
        let mut world_env = WorldEnvironment::new_alloc();
        world_env.set_environment(&env);
        root.add_child(&world_env);

        // a faint cold light from above: the shapes of remembered walls
        // stay barely readable outside any light
        let mut moon = DirectionalLight3D::new_alloc();
        moon.set_rotation_degrees(Vector3::new(-62.0, 25.0, 0.0));
        moon.set_color(Color::from_rgb(0.55, 0.62, 0.9));
        moon.set_param(Param::ENERGY, 0.14);
        moon.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        root.add_child(&moon);
        // and fainter still from the other side: no wall face is flat black
        let mut back = DirectionalLight3D::new_alloc();
        back.set_rotation_degrees(Vector3::new(-40.0, 205.0, 0.0));
        back.set_color(Color::from_rgb(0.5, 0.56, 0.8));
        back.set_param(Param::ENERGY, 0.07);
        back.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        root.add_child(&back);
        // the art gallery's own light (for review, never in a game)
        let mut studio = DirectionalLight3D::new_alloc();
        studio.set_rotation_degrees(Vector3::new(-50.0, -30.0, 0.0));
        studio.set_color(Color::from_rgb(1.0, 0.95, 0.88));
        studio.set_param(Param::ENERGY, 0.9);
        studio.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        studio.set_visible(false);
        root.add_child(&studio);

        let mut camera = Camera3D::new_alloc();
        camera.set_fov(FOV_DEG);
        camera.set_current(true);
        root.add_child(&camera);

        let mut cells_root = Node3D::new_alloc();
        root.add_child(&cells_root);
        cells_root.set_name("Cells");

        let ring_tex = crate::vfx::ring_texture();
        let mut hover = marker(&mut root, &crate::vfx::frame_texture(), 1.0);
        hover.set_modulate(Color::from_rgba(1.0, 0.88, 0.62, 0.2));
        let mut cursor = marker(&mut root, &crate::vfx::frame_texture(), 1.05);
        cursor.set_modulate(CURSOR);

        let mut hero_ring = marker(&mut root, &ring_tex, 0.95);
        hero_ring.set_modulate(HERO_RING);
        let mut hostile_ring = marker(&mut root, &ring_tex, 1.05);
        hostile_ring.set_modulate(HOSTILE_RING);
        let outline = |color: Color| -> Gd<Material> {
            match godot::tools::try_load::<Shader>("res://shaders/outline.gdshader") {
                Ok(shader) => {
                    let mut m = ShaderMaterial::new_gd();
                    m.set_shader(&shader);
                    m.set_shader_parameter("color", &color.to_variant());
                    m.upcast()
                }
                Err(_) => build_flat(color, Finish::Flat),
            }
        };
        let outline = (
            outline(HOSTILE_RING.with_alpha(0.9)),
            outline(HOVER_OUTLINE),
        );
        let xray = godot::tools::try_load::<Shader>("res://shaders/xray.gdshader")
            .ok()
            .map(|shader| {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(&shader);
                m.set_shader_parameter("color", &XRAY.to_variant());
                m.upcast::<Material>()
            });

        // over the hero's head: short shadows at their feet, not long ones
        // across the floor
        let mut hero_light = OmniLight3D::new_alloc();
        hero_light.set_color(HERO_LIGHT);
        hero_light.set_param(Param::ENERGY, HERO_LIGHT_ENERGY);
        hero_light.set_param(Param::RANGE, HERO_LIGHT_RANGE);
        hero_light.set_param(Param::ATTENUATION, 1.0);
        hero_light.set_param(Param::SHADOW_BIAS, 0.03);
        hero_light.set_param(Param::SHADOW_NORMAL_BIAS, 1.2);
        hero_light.set_param(Param::SHADOW_BLUR, 2.0);
        hero_light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        hero_light.set_shadow(true);
        hero_light.set_visible(false);
        root.add_child(&hero_light);

        // the glow of the fire under a branch's floors, low round the hero
        let mut underglow = OmniLight3D::new_alloc();
        underglow.set_param(Param::RANGE, UNDERGLOW_RANGE);
        underglow.set_param(Param::ENERGY, UNDERGLOW_ENERGY);
        underglow.set_param(Param::ATTENUATION, 1.4);
        underglow.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.6);
        underglow.set_shadow(false);
        underglow.set_visible(false);
        root.add_child(&underglow);

        let mut rim = OmniLight3D::new_alloc();
        rim.set_color(RIM_LIGHT);
        rim.set_param(Param::ENERGY, 3.0);
        rim.set_param(Param::RANGE, 3.0);
        rim.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        rim.set_shadow(false);
        rim.set_cull_mask(RIM_LAYER);
        rim.set_visible(false);
        root.add_child(&rim);

        let mut bedrock = MeshInstance3D::new_alloc();
        bedrock.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        bedrock.set_layer_mask(1 | TERRAIN_LAYER);
        root.add_child(&bedrock);
        let mist = ground_mist(&mut root);

        let mut post = CanvasLayer::new_alloc();
        post.set_layer(-1);
        let mut post_mat = None;
        if let Ok(shader) = godot::tools::try_load::<Shader>("res://shaders/vignette.gdshader") {
            let mut mat = ShaderMaterial::new_gd();
            mat.set_shader(&shader);
            post_mat = Some(mat.clone());
            let mut rect = ColorRect::new_alloc();
            rect.set_anchors_preset(LayoutPreset::FULL_RECT);
            rect.set_mouse_filter(MouseFilter::IGNORE);
            rect.set_material(&mat);
            post.add_child(&rect);
        }
        root.add_child(&post);

        let flame: Gd<Material> =
            match godot::tools::try_load::<Shader>("res://shaders/flame.gdshader") {
                Ok(shader) => {
                    let mut m = ShaderMaterial::new_gd();
                    m.set_shader(&shader);
                    if let Ok(book) =
                        godot::tools::try_load::<godot::classes::Texture2D>(FLAME_BOOK)
                    {
                        m.set_shader_parameter("flipbook", &book.to_variant());
                        m.set_shader_parameter("has_flipbook", &true.to_variant());
                    }
                    m.upcast()
                }
                Err(_) => {
                    let mut m = StandardMaterial3D::new_gd();
                    m.set_shading_mode(ShadingMode::UNSHADED);
                    m.set_albedo(FLAME);
                    m.set_feature(Feature::EMISSION, true);
                    m.set_emission(FLAME);
                    m.set_emission_energy_multiplier(3.0);
                    m.upcast()
                }
            };
        let torch_scene = godot::tools::try_load::<PackedScene>(
            "res://art/cc0/quaternius/props/Torch_Metal.gltf",
        )
        .ok();

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
        let hero_fx = crate::hero::HeroFx::new(root.clone());
        let fow = Fow::new();
        let surfaces = Surfaces::new(&fow, art.manifest());
        let batches = Batches::new(cells_root.clone(), 1 | TERRAIN_LAYER);
        let vfx = Vfx::new(root.clone(), TERRAIN_LAYER);
        let mut view = MapView {
            root,
            cells_root,
            camera,
            env,
            cells: HashMap::new(),
            generation: None,
            art,
            font: theme::mono_bold(),
            hover,
            hover_cell: None,
            hostile_ring,
            outlined: None,
            outline,
            xray,
            xray_on: false,
            eye: Vector3::ZERO,
            hint_cells: HashSet::new(),
            hints_dirty: false,
            cursor,
            hero_ring,
            ring_tex,
            hero_light,
            underglow,
            rim,
            rim_model: None,
            room_lights: Vec::new(),
            torches: Vec::new(),
            torch_scene,
            flame_mat: flame,
            halo_mat: {
                let mut m = StandardMaterial3D::new_gd();
                m.set_shading_mode(ShadingMode::UNSHADED);
                m.set_billboard_mode(BillboardMode::ENABLED);
                m.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA);
                m.set_blend_mode(godot::classes::base_material_3d::BlendMode::ADD);
                m.set_texture(
                    godot::classes::base_material_3d::TextureParam::ALBEDO,
                    &crate::vfx::soft_texture(),
                );
                m.set_albedo(Color::from_rgba(1.0, 0.55, 0.22, 0.22));
                m.set_flag(godot::classes::base_material_3d::Flags::DISABLE_FOG, true);
                m.upcast()
            },
            batches,
            surfaces,
            fow,
            fow_dirty: true,
            vfx,
            mist,
            post_mat,
            fade: 0.0,
            building: Vec::new(),
            liquids: HashMap::new(),
            spare_lamps: Vec::new(),
            prop_scenes: HashMap::new(),
            spare_props: HashMap::new(),
            prof: Vec::new(),
            branch: Branch::Main,
            branch_look: crate::branch_look::look_of(Branch::Main),
            stats_window: std::env::var_os("RENETHACK_FRAME_STATS").map(|_| FrameStats::default()),
            preload_told: false,
            rehearsal: None,
            rehearsed: false,
            shadow_made: false,
            build_budget: BUILD_BUDGET,
            showing: 0,
            shadow_block: None,
            shadow_frames: 0,
            card: None,
            spare_embers: None,
            title_clock: None,
            title_work: Default::default(),
            studio,
            showcase: false,
            shown: false,
            lights_dirty: false,
            bedrock,
            holes: HashSet::new(),
            bedrock_dirty: true,
            post,
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
            facing: HashMap::new(),
            incoming: HashMap::new(),
            motions: Vec::new(),
            interrupted: HashSet::new(),
            log_seq: None,
            hold: None,
            stats: MotionStats::default(),
            order_pace: None,
            path_marks: Vec::new(),
            soft_tex: crate::vfx::soft_texture(),
            ring_cells: HashSet::new(),
            path_shown: (Vec::new(), false),
            hero_gear: crate::hero::HeroGear::default(),
            aim_lift: 0.0,
            hero_fx,
        };
        view.set_branch(Branch::Main);
        // torches are made ahead by the rehearsal, a frame each (a
        // sconce's scene and embers cost a long frame when a lit room
        // first shows)
        view.place_camera();
        view
    }

    /// Rebuild on a new generation, else apply dirty cells; camera target =
    /// view_center, else hero; cursor marker when World.cursor differs from
    /// the hero, or getpos moves it. The hero ring marks `World::hero`,
    /// also when the hero is not drawn (invisible).
    pub fn sync(&mut self, world: &mut World, catalog: &Catalog, delta: f64) {
        let sync_start = std::time::Instant::now();
        let built_before = self.art.counts().1;
        self.art.begin_frame();
        self.clock += delta;
        self.prof.clear();
        let hero = world.hero();
        // the hero faces the way they last stepped
        if let (Some((x, y)), Some((px, py))) = (hero, self.hero_at)
            && (x, y) != (px, py)
            && (x - px).abs() <= 1
            && (y - py).abs() <= 1
        {
            self.hero_yaw = ((x - px) as f32).atan2((y - py) as f32).to_degrees();
        }
        if self.hero_at != hero {
            self.fow_dirty = true;
        }
        self.hero_at = hero;
        // another branch: its own look, and every cell drawn again in it
        let branch = world.branch();
        if branch != self.branch {
            self.set_branch(branch);
            self.generation = None;
        }
        let generation = world.map.generation();
        let new_level = self.generation != Some(generation);
        let started = std::time::Instant::now();
        if new_level {
            // a new level (or a redraw from scratch) never animates
            self.finish_motions();
            self.vfx.clear();
            self.facing.clear();
            self.generation = Some(generation);
            world.map.take_dirty();
            // built over the next frames, nearest the hero first (the
            // lights are placed once it is all drawn)
            self.building = build_order(hero);
            self.snap = true;
            self.lights_dirty = true;
        } else {
            // a cell's look depends on its neighbours: a wall on the cell
            // north of it (and the rock on the two), unseen ground under
            // something on all four, the rock on all eight
            let (mut dirty, moves) = world.map.take_dirty_moves();
            if !dirty.is_empty() {
                // the engine went on: the scene catches up at once
                self.finish_motions();
            }
            self.carry(&moves, world);
            let near: Vec<_> = dirty
                .iter()
                .flat_map(|&(x, y)| {
                    (-1..=1)
                        .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
                        .chain([(x, y + 2)])
                })
                .filter(|&(x, y)| in_field(x, y))
                .collect();
            dirty.extend(near);
            dirty.sort_unstable();
            dirty.dedup();
            // much changed at once (a map revealed, a redraw): what is
            // near the hero or moved now, the rest over the next frames
            let now = |&(x, y): &(i32, i32)| {
                dirty.len() <= MANY_CELLS
                    || moves.iter().any(|m| m.to == (x, y) || m.from == (x, y))
                    || hero.is_none_or(|(hx, hy)| (hx - x).abs().max((hy - y).abs()) <= NEAR_CELLS)
            };
            let (urgent, later): (Vec<_>, Vec<_>) = dirty.iter().copied().partition(now);
            for (x, y) in urgent {
                self.update_cell(x, y, world, catalog, hero);
            }
            if !later.is_empty() {
                self.building.extend(later);
                let (hx, hy) = hero.unwrap_or((COLNO / 2, ROWNO / 2));
                self.building
                    .sort_by_key(|&(x, y)| std::cmp::Reverse((x - hx).pow(2) + (y - hy).pow(2)));
                self.building.dedup();
            }
            // a carried model whose new cell did not take it
            for (_, c) in std::mem::take(&mut self.incoming) {
                self.art.give(c.model);
            }
        }
        if let Some(st) = self.stats_window.as_mut().filter(|_| new_level) {
            st.since_level = 0.0;
        }
        if new_level && self.stats_window.is_some() {
            godot_print!(
                "map: level drawn in {:.1} ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let __t = std::time::Instant::now();
        self.build_some(world, catalog, hero);
        self.complete_models();
        self.prof
            .push(("build", __t.elapsed().as_secs_f64() * 1000.0));
        self.watch_fights(world, catalog);
        self.advance_motions(delta as f32);
        self.hero_fx.advance(delta as f32, &mut self.vfx);
        if std::mem::take(&mut self.fow_dirty) {
            let __t = std::time::Instant::now();
            self.see(hero, new_level);
            self.prof
                .push(("see", __t.elapsed().as_secs_f64() * 1000.0));
        }
        if self.fow.advance(delta as f32) {
            self.surfaces.show_fow(&self.fow);
        }
        // the lights of a level still being drawn wait for all of it
        if self.building.is_empty() && std::mem::take(&mut self.lights_dirty) {
            let __t = std::time::Instant::now();
            self.place_room_lights();
            self.prof
                .push(("lights", __t.elapsed().as_secs_f64() * 1000.0));
        }
        let bounds = self.overview.and(self.known_bounds());
        if let Some(b) = bounds {
            // never closer than the player's own view
            let (centre, distance) = overview_frame(b, self.aspect());
            self.target = centre;
            self.overview = Some(distance.max(self.distance));
        } else if let Some((x, y)) = world.view_center.or(hero).or(world.cursor) {
            // following the hero as they walk, not the cell they are bound for
            let walking = self.hero_shown_at().filter(|_| Some((x, y)) == hero);
            self.target = match walking {
                Some(p) => Vector3::new(p.x, 0.0, p.z),
                None => Vector3::new(x as f32, 0.0, y as f32),
            };
        }
        match hero {
            Some((x, y)) => {
                let ground = self.ground(x, y);
                let p = match self.hero_shown_at() {
                    Some(p) => Vector3::new(p.x, ground, p.z),
                    None => Vector3::new(x as f32, ground, y as f32),
                };
                self.hero_ring.set_position(p);
                self.hero_ring.set_visible(true);
                // the walls and rock between the eye and the hero thin out
                RenderingServer::singleton()
                    .global_shader_parameter_set("hero_pos", &(p + at(0.0, 0.9, 0.0)).to_variant());
                self.hero_light.set_position(p + at(0.0, 2.4, 0.6));
                self.hero_light.set_visible(true);
                match self.branch_look.underglow {
                    Some(c) => {
                        self.underglow.set_color(c);
                        self.underglow.set_position(p + at(0.0, 0.2, 0.0));
                        self.underglow.set_visible(true);
                    }
                    None => self.underglow.set_visible(false),
                }
                // behind the hero, on the far side from the camera
                self.rim.set_position(p + at(0.0, 2.0, -0.9));
                self.rim.set_visible(true);
                // while a level is drawn the hero's cell may still hold the
                // last level's model
                if self.building.is_empty() {
                    self.rim_hero((x, y));
                }
                let __t = std::time::Instant::now();
                self.equip_hero((x, y), world, catalog, delta as f32);
                self.prof
                    .push(("equip", __t.elapsed().as_secs_f64() * 1000.0));
                self.show_through((x, y), p);
                self.light_torches(p);
            }
            None => {
                self.hero_ring.set_visible(false);
                self.hero_light.set_visible(false);
                self.underglow.set_visible(false);
                self.rim.set_visible(false);
                RenderingServer::singleton()
                    .global_shader_parameter_set("hero_pos", &at(0.0, -100.0, 0.0).to_variant());
            }
        }
        self.show_hover();
        if std::mem::take(&mut self.hints_dirty) {
            self.show_hints();
        }
        // its mesh is laid once a level is all drawn (a mesh swapped
        // under an instance the lights pair with, frame after frame,
        // crashes Godot at exit)
        if self.building.is_empty() && std::mem::take(&mut self.bedrock_dirty) {
            let __t = std::time::Instant::now();
            self.lay_bedrock();
            self.prof
                .push(("bedrock", __t.elapsed().as_secs_f64() * 1000.0));
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
                    .set_position(Vector3::new(x as f32, ground, y as f32));
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
        self.follow_rings();
        self.fade_in(new_level, delta as f32);
        self.vfx.process(delta as f32, self.focus);
        let __t = std::time::Instant::now();
        self.batches.flush();
        self.prof
            .push(("flush", __t.elapsed().as_secs_f64() * 1000.0));
        // a long frame is told of with the sync before it (this one's
        // numbers are for the next)
        self.frame_stats(delta);
        if let Some(stats) = self.stats_window.as_mut() {
            let ms = sync_start.elapsed().as_secs_f64() * 1000.0;
            if ms > 10.0 {
                godot_print!("map: a sync of {ms:.1} ms: {:?}", self.prof);
            }
            stats.last_sync = sync_start.elapsed().as_secs_f64() * 1000.0;
            stats.last_built = self.art.counts().1 - built_before;
        }
    }

    /// With RENETHACK_FRAME_STATS set, the frame and GPU times every two
    /// seconds (profiling a scene).
    fn frame_stats(&mut self, delta: f64) {
        let Some(stats) = self.stats_window.as_mut() else {
            return;
        };
        let Some(vp) = self.camera.get_viewport() else {
            return;
        };
        let rid = vp.get_viewport_rid();
        let mut rs = RenderingServer::singleton();
        if stats.frames == 0 {
            rs.viewport_set_measure_render_time(rid, true);
        }
        // the frame by the clock on the wall (from the last frame's sync to
        // this one's): Godot's process step is evened out against its
        // physics ticks, and a long frame can show there as a short one
        // and a longer one
        let now = std::time::Instant::now();
        let frame = godot::classes::Engine::singleton().get_process_frames();
        let wall = stats
            .clock
            .filter(|(n, _)| frame == n + 1)
            .map(|(_, t)| now.duration_since(t).as_secs_f64());
        stats.clock = Some((frame, now));
        let long = wall.unwrap_or(delta);
        stats.frames += 1;
        stats.secs += delta;
        stats.worst = stats.worst.max(long);
        stats.since_level += delta;
        // pipelines compiled, and video memory, since the frame before
        let perf = godot::classes::Performance::singleton();
        use godot::classes::performance::Monitor as M;
        let pipelines = [
            M::PIPELINE_COMPILATIONS_CANVAS,
            M::PIPELINE_COMPILATIONS_MESH,
            M::PIPELINE_COMPILATIONS_SURFACE,
            M::PIPELINE_COMPILATIONS_DRAW,
            M::PIPELINE_COMPILATIONS_SPECIALIZATION,
        ]
        .map(|m| perf.get_monitor(m) as i64);
        let vmem = perf.get_monitor(M::RENDER_VIDEO_MEM_USED) / 1e6;
        if long > 0.033 {
            let new: Vec<i64> = pipelines
                .iter()
                .zip(stats.pipelines)
                .map(|(a, b)| a - b)
                .collect();
            godot_print!(
                "map: a frame of {:.1} ms (Godot's step {:.1} ms), {:.2} s after a level change; the map's last sync took {:.1} ms and built {} models; pipelines compiled (canvas, mesh, surface, draw, specialization) {new:?}, video memory {:+.1} MB",
                long * 1000.0,
                delta * 1000.0,
                stats.since_level,
                stats.last_sync,
                stats.last_built,
                vmem - stats.vmem
            );
        }
        stats.pipelines = pipelines;
        stats.vmem = vmem;
        stats.gpu += rs.viewport_get_measured_render_time_gpu(rid);
        stats.cpu += rs.get_frame_setup_time_cpu();
        stats.draws += rs.get_rendering_info(
            godot::classes::rendering_server::RenderingInfo::TOTAL_DRAW_CALLS_IN_FRAME,
        ) as f64;
        if stats.secs >= 2.0 {
            let n = f64::from(stats.frames);
            let (b, solids) = self.batches.counts();
            // the GPU's own time needs timestamp queries, which Metal does
            // not give Godot: with vsync off the frame time bounds it
            let gpu = if stats.gpu > 0.0 {
                format!("{:.2} ms", stats.gpu / n)
            } else {
                "n/a".to_string()
            };
            godot_print!(
                "map: frame {:.2} ms (worst {:.1} ms), gpu {gpu}, render setup {:.2} ms, {:.0} draw calls; {b} batches, {solids} solids",
                stats.secs * 1000.0 / n,
                stats.worst * 1000.0,
                stats.cpu / n,
                stats.draws / n
            );
            *stats = FrameStats {
                frames: 1,
                since_level: stats.since_level,
                pipelines: stats.pipelines,
                vmem: stats.vmem,
                clock: stats.clock,
                ..FrameStats::default()
            };
        }
    }

    /// Draw the cells of a new level still to draw for at most the
    /// frame's budget; the lights and the fog of war again once all are.
    fn build_some(&mut self, world: &World, catalog: &Catalog, hero: Option<(i32, i32)>) {
        if self.building.is_empty() {
            return;
        }
        let start = std::time::Instant::now();
        // no hero (the game ended, a map cleared): nothing to cover with
        // a fade, and nothing to start from; at once
        let all = hero.is_none() || !self.root.is_visible();
        while let Some((x, y)) = self.building.pop() {
            self.update_cell(x, y, world, catalog, hero);
            if !all && start.elapsed() >= self.build_budget {
                break;
            }
        }
        if self.building.is_empty() {
            self.lights_dirty = true;
            self.fow_dirty = true;
        }
    }

    /// Light, haze and grade the branch's way (the level's cells are drawn
    /// again by the caller).
    fn set_branch(&mut self, branch: Branch) {
        self.branch = branch;
        let l = crate::branch_look::look_of(branch);
        self.branch_look = l;
        let env = &mut self.env;
        env.set_bg_color(l.darkness);
        env.set_fog_light_color(l.darkness);
        env.set_ambient_light_color(l.ambient);
        env.set_ambient_light_energy(if self.showcase {
            GALLERY_AMBIENT
        } else {
            l.ambient_energy
        });
        env.set_volumetric_fog_albedo(l.fog);
        env.set_volumetric_fog_density(l.fog_density);
        let g = l.grade;
        env.set_adjustment_saturation(g.saturation);
        env.set_adjustment_contrast(g.contrast);
        env.set_adjustment_brightness(g.brightness);
        env.set_adjustment_color_correction(&grade_curve(g.shadows, g.highlights));
        RenderingServer::singleton()
            .global_shader_parameter_set("branch_cracks", &l.cracks.to_variant());
        self.vfx.set_dust(l.dust, l.embers);
        self.lights_dirty = true;
        self.bedrock_dirty = true;
    }

    /// The branch the level drawn is in (self-tests).
    pub fn branch(&self) -> Branch {
        self.branch
    }

    /// The manifest material of the walls drawn now (self-tests check each
    /// branch has its own).
    pub fn wall_material(&self) -> String {
        let art = self.art.manifest();
        let wall = art.terrain(Terrain::Wall).material;
        wall.map(|m| self.branch_look.material(art.material_at(m).0).to_string())
            .unwrap_or_default()
    }

    /// A new level is built in one long frame: it comes up out of black
    /// over a quarter of a second after, instead of with a jolt.
    fn fade_in(&mut self, new_level: bool, delta: f32) {
        if new_level {
            self.fade = 1.0;
        } else if self.fade > 0.0 {
            // the long frame itself does not count
            self.fade = (self.fade - delta.min(1.0 / 30.0) / LEVEL_FADE_SECS).max(0.0);
        } else {
            return;
        }
        if let Some(m) = self.post_mat.as_mut() {
            let shown = 1.0 - self.fade;
            let f = 1.0 - shown * shown * (3.0 - 2.0 * shown);
            m.set_shader_parameter("fade", &f.to_variant());
        }
    }

    /// Work out what the hero sees and remembers (the fog of war).
    fn see(&mut self, hero: Option<(i32, i32)>, snap: bool) {
        let n = (COLNO * ROWNO) as usize;
        let mut cells = vec![Seen::Nothing; n];
        let mut walls = vec![false; n];
        for (&(x, y), c) in &self.cells {
            if let Some(i) = cell_index(x, y) {
                cells[i] = c.look.seen;
                walls[i] = c.look.masonry;
            }
        }
        let fow: Vec<FowCell> = sight(&cells, &walls, hero);
        self.fow.set(&fow, snap);
    }

    /// Effects: a burst on a cell (at the height of a body standing there).
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn burst_at(&mut self, kind: VfxKind, (x, y): (i32, i32)) {
        let p = Vector3::new(x as f32, self.ground(x, y).max(0.0) + 0.8, y as f32);
        self.vfx.burst(kind, p);
    }

    /// Effects: a ray from one cell to another (at chest height), with the
    /// kind's burst where it ends.
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn beam_between(&mut self, kind: VfxKind, from: (i32, i32), to: (i32, i32)) {
        let chest = |v: &Self, (x, y): (i32, i32)| {
            Vector3::new(x as f32, v.ground(x, y).max(0.0) + 1.0, y as f32)
        };
        let (a, b) = (chest(self, from), chest(self, to));
        self.vfx.beam(kind, a, b);
    }

    /// The effects, for bursts and beams at world positions.
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn vfx(&mut self) -> &mut Vfx {
        &mut self.vfx
    }

    /// One faint fill light over each connected area of floor in view (a
    /// big room's middle stays readable), and torches on its walls.
    fn place_room_lights(&mut self) {
        let lit: std::collections::HashSet<(i32, i32)> = self
            .cells
            .iter()
            .filter(|(_, n)| n.look.lit)
            .map(|(&k, _)| k)
            .collect();
        let areas = lit_areas(&lit);
        let mut used = 0;
        let mut sconces = Vec::new();
        let fills: Vec<(f32, f32, f32, f32)> = areas
            .iter()
            .filter(|a| a.len() >= 4)
            .flat_map(|a| fill_lights(a))
            .collect();
        for area in areas.iter().filter(|a| a.len() >= 4) {
            sconces.extend(torch_walls(area, |x, y| {
                self.cells.get(&(x, y)).is_some_and(|n| n.look.wall)
            }));
        }
        for (cx, cy, reach, energy) in fills {
            if used == self.room_lights.len() {
                let mut l = OmniLight3D::new_alloc();
                l.set_color(ROOM_LIGHT);
                l.set_shadow(false);
                l.set_param(Param::ATTENUATION, 1.0);
                // the haze glows around flames, not under a hall's fill
                l.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
                self.root.add_child(&l);
                self.room_lights.push(l);
            }
            let l = &mut self.room_lights[used];
            used += 1;
            l.set_position(Vector3::new(cx, 2.6, cy));
            l.set_param(Param::RANGE, reach);
            let showcase = if self.showcase { GALLERY_FILL } else { 1.0 };
            l.set_param(
                Param::ENERGY,
                energy * self.branch_look.fill_scale * showcase,
            );
            l.set_color(self.branch_look.fill);
            l.set_visible(true);
        }
        for l in self.room_lights.iter_mut().skip(used) {
            l.set_visible(false);
        }
        self.place_torches(&sconces);
    }

    /// Torches on these walls, each facing into its room: (wall cell,
    /// direction into the room).
    fn place_torches(&mut self, walls: &[TorchWall]) {
        for (i, &((x, y), (dx, dz))) in walls.iter().enumerate() {
            if i == self.torches.len() {
                let torch = self.new_torch();
                self.torches.push(torch);
            }
            let (fx, fz) = (dx as f32, dz as f32);
            // on the wall's face towards the room
            let face = Vector3::new(x as f32 + fx * 0.5, 0.0, y as f32 + fz * 0.5);
            let t = &mut self.torches[i];
            // a torch lit again elsewhere: the embers it left behind go
            // (they rise in the world, not on the torch)
            if !t.node.is_visible() {
                t.embers.restart();
            }
            let yaw = fx.atan2(fz);
            let basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw, 0.0));
            t.node
                .set_transform(Transform3D::new(basis, face + at(0.0, 1.45, 0.0)));
            t.node.set_visible(true);
            // at the flame, a little out from it
            t.at = face + Vector3::new(fx * 0.45, 2.0, fz * 0.45);
            t.phase = f64::from(cell_noise(x, y, 13)) * 10.0;
            t.flame
                .set_instance_shader_parameter("phase", &(t.phase as f32).to_variant());
            t.light.set_position(t.at);
            t.light.set_visible(true);
            let lanterns = self.branch_look.lanterns;
            t.fire.set_visible(!lanterns);
            if lanterns && t.lantern.is_none() {
                self.hang_lantern(i);
            }
            let t = &mut self.torches[i];
            if let Some(l) = t.lantern.as_mut() {
                l.set_visible(lanterns);
            }
        }
        for t in self.torches.iter_mut().skip(walls.len()) {
            t.node.set_visible(false);
            t.light.set_visible(false);
        }
    }

    /// A lantern hung on torch `i`'s bracket (the branches of lanterns
    /// show it instead of the torch).
    fn hang_lantern(&mut self, i: usize) {
        let scene = self
            .prop_scenes
            .entry(Prop::Lantern)
            .or_insert_with(|| godot::tools::try_load::<PackedScene>(Prop::Lantern.scene()).ok())
            .clone();
        if let Some(mut l) = scene
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())
        {
            // hung from a bracket over the room, out of its own light
            let mut lamp = Node3D::new_alloc();
            l.set_position(at(0.0, 0.8, 0.32));
            l.set_scale(Vector3::new(0.5, 0.5, 0.5));
            set_layers(&l, SCONCE_LAYER);
            lamp.add_child(&l);
            // its flames, glowing through the glass, and their halo
            let warm = self.art.flat(Color::from_rgb(1.0, 0.72, 0.4), Finish::Glow);
            let flame_mesh = self.art.mesh(sphere(0.025));
            for (x, z) in [(-0.06f32, 0.0f32), (0.06, 0.0), (0.0, 0.06)] {
                let mut core = MeshInstance3D::new_alloc();
                core.set_mesh(&flame_mesh);
                core.set_material_override(&warm);
                core.set_position(at(x, 0.55, 0.32 + z));
                no_shadow(&mut core);
                lamp.add_child(&core);
            }
            let mut halo = MeshInstance3D::new_alloc();
            let mut disc = godot::classes::QuadMesh::new_gd();
            disc.set_size(Vector2::new(0.8, 0.8));
            halo.set_mesh(&disc);
            halo.set_material_override(&self.halo_mat);
            halo.set_position(at(0.0, 0.57, 0.36));
            no_shadow(&mut halo);
            lamp.add_child(&halo);
            let t = &mut self.torches[i];
            t.node.add_child(&lamp);
            t.lantern = Some(lamp);
        }
    }

    fn new_torch(&mut self) -> Torch {
        let mut node = Node3D::new_alloc();
        // the torch itself (a lantern may hang there instead)
        let mut fire = Node3D::new_alloc();
        if let Some(mut sconce) = self
            .torch_scene
            .as_ref()
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())
        {
            sconce.set_scale(Vector3::new(1.2, 1.2, 1.2));
            for n in sconce
                .find_children_ex("*")
                .type_("GeometryInstance3D")
                .owned(false)
                .done()
                .iter_shared()
            {
                if let Ok(mut g) = n.try_cast::<godot::classes::GeometryInstance3D>() {
                    g.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                    g.set_layer_mask(SCONCE_LAYER);
                }
            }
            fire.add_child(&sconce);
        }
        // the flame licks up from the torch's head, embers rise from it
        let mut flame = MeshInstance3D::new_alloc();
        let mut quad = godot::classes::QuadMesh::new_gd();
        quad.set_size(Vector2::new(0.26, 0.52));
        quad.set_center_offset(at(0.0, 0.22, 0.0));
        flame.set_mesh(&quad);
        flame.set_material_override(&self.flame_mat);
        flame.set_position(at(0.0, 0.46, 0.3));
        no_shadow(&mut flame);
        fire.add_child(&flame);
        // and a warm halo in the air around it
        let mut halo = MeshInstance3D::new_alloc();
        let mut disc = godot::classes::QuadMesh::new_gd();
        disc.set_size(Vector2::new(0.9, 0.9));
        halo.set_mesh(&disc);
        halo.set_material_override(&self.halo_mat);
        halo.set_position(at(0.0, 0.62, 0.32));
        no_shadow(&mut halo);
        fire.add_child(&halo);
        let mut embers = self
            .spare_embers
            .take()
            .unwrap_or_else(|| self.vfx.embers());
        embers.set_position(at(0.0, 0.58, 0.3));
        fire.add_child(&embers);
        node.add_child(&fire);
        self.root.add_child(&node);
        let mut light = OmniLight3D::new_alloc();
        light.set_color(TORCH);
        light.set_cull_mask(!SCONCE_LAYER);
        light.set_param(Param::RANGE, TORCH_RANGE);
        light.set_param(Param::ATTENUATION, 1.2);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.5);
        light.set_param(Param::SHADOW_BIAS, 0.03);
        light.set_param(Param::SHADOW_NORMAL_BIAS, 1.2);
        light.set_param(Param::SHADOW_BLUR, 1.5);
        light.set_enable_distance_fade(true);
        light.set_distance_fade_begin(18.0);
        light.set_distance_fade_shadow(10.0);
        light.set_distance_fade_length(4.0);
        light.set_shadow(false);
        self.root.add_child(&light);
        Torch {
            node,
            fire,
            lantern: None,
            flame,
            embers,
            light,
            at: Vector3::ZERO,
            phase: 0.0,
            shadow: false,
        }
    }

    /// The torches flicker; the few nearest the hero cast shadows.
    fn light_torches(&mut self, hero: Vector3) {
        let mut near: Vec<(f32, usize)> = self
            .torches
            .iter()
            .enumerate()
            .filter(|(_, t)| t.light.is_visible())
            .map(|(i, t)| (t.at.distance_squared_to(hero), i))
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let shadowed: HashSet<usize> = near.iter().take(TORCH_SHADOWS).map(|&(_, i)| i).collect();
        let clock = self.clock;
        let (color, energy) = (self.branch_look.torch, self.branch_look.torch_energy);
        for (i, t) in self.torches.iter_mut().enumerate() {
            if !t.light.is_visible() {
                continue;
            }
            let time = clock + t.phase;
            t.light.set_color(color);
            t.light.set_param(Param::ENERGY, energy * flicker(time));
            t.light.set_position(t.at + sway(time));
            let shadow = shadowed.contains(&i);
            if shadow != t.shadow {
                t.shadow = shadow;
                t.light.set_shadow(shadow);
            }
        }
    }

    /// The hero's model (and only it) takes the rim light.
    fn rim_hero(&mut self, hero: (i32, i32)) {
        let node = self
            .cells
            .get(&hero)
            .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)))
            .map(|m| m.node.clone());
        if node.as_ref().map(|n| n.instance_id())
            == self.rim_model.as_ref().map(|n| n.instance_id())
        {
            return;
        }
        // a model given back to the pool may be on its way out: never
        // touch one being freed (Godot crashes at exit)
        let alive = |n: &Gd<Node3D>| n.is_instance_valid() && !n.is_queued_for_deletion();
        if let Some(old) = self.rim_model.take().filter(alive) {
            set_layers(&old, 1);
            set_overlay(&old, None);
        }
        if let Some(n) = node.as_ref().filter(|n| alive(n)) {
            set_layers(n, 1 | RIM_LAYER);
        }
        self.rim_model = node;
        self.xray_on = false;
    }

    /// The hero seen through what hides them from the camera: a wall, a
    /// door or rock between the eye and their chest.
    fn show_through(&mut self, hero: (i32, i32), at: Vector3) {
        let chest = at + Vector3::new(0.0, 1.0, 0.0);
        let hidden = hides(chest, self.eye, hero, |x, y| {
            self.cells
                .get(&(x, y))
                .filter(|n| n.look.ground >= 1.0)
                .map(|n| n.look.top)
        });
        if hidden != self.xray_on
            && let Some(n) = self.rim_model.as_ref().filter(|n| n.is_instance_valid())
        {
            set_overlay(n, if hidden { self.xray.as_ref() } else { None });
            self.xray_on = hidden;
        }
    }

    /// The hover frame, a red ring under a hostile monster there, and an
    /// outline round the monster or object (not the hero).
    fn show_hover(&mut self) {
        let cell = self.hover_cell.and_then(|c| Some((c, self.cells.get(&c)?)));
        let target = cell
            .filter(|(c, _)| Some(*c) != self.hero_at)
            .and_then(|(_, n)| {
                let m = n.models.get(n.look.entity?)?;
                Some((m.node.clone(), n.look.hostile))
            });
        let same = |a: Option<&Gd<Node3D>>, b: Option<&Gd<Node3D>>| {
            a.map(|n| n.instance_id()) == b.map(|n| n.instance_id())
        };
        if !same(self.outlined.as_ref(), target.as_ref().map(|t| &t.0)) {
            if let Some(old) = self
                .outlined
                .take()
                .filter(|n| n.is_instance_valid() && !n.is_queued_for_deletion())
            {
                set_overlay(&old, None);
            }
            if let Some((node, hostile)) = &target {
                let mat = if *hostile {
                    &self.outline.0
                } else {
                    &self.outline.1
                };
                set_overlay(node, Some(mat));
                self.outlined = Some(node.clone());
            }
        }
        // the hero's own cell has the hero's ring: no frame over it
        let Some((x, y)) = self.hover_cell.filter(|&c| Some(c) != self.hero_at) else {
            self.hover.set_visible(false);
            self.hostile_ring.set_visible(false);
            return;
        };
        let ground = self.ground(x, y);
        let p = Vector3::new(x as f32, ground, y as f32);
        self.hover.set_position(p);
        self.hover.set_visible(true);
        let hostile = self.cells.get(&(x, y)).is_some_and(|n| n.look.hostile);
        self.hostile_ring.set_position(p);
        self.hostile_ring.set_visible(hostile);
    }

    /// Letters of stairs and ladders: under the pointer or in the overview.
    fn show_hints(&mut self) {
        let all = self.overview.is_some();
        for c in &self.hint_cells {
            let Some(nodes) = self.cells.get_mut(c) else {
                continue;
            };
            let on = all || self.hover_cell == Some(*c);
            for (l, label) in nodes.look.letters.iter().zip(nodes.letters.iter_mut()) {
                if l.hint {
                    label.set_visible(on);
                }
            }
        }
    }

    /// A solid's material: the map's surface shader for manifest
    /// materials (the art library's plain one without it), or a colour.
    fn material(&mut self, paint: Paint) -> Gd<Material> {
        match paint {
            Paint::Pbr(m, shade, role) => {
                match self.surfaces.material(self.art.manifest(), m, shade, role) {
                    Some(mat) => mat,
                    None => self.art.surface(m, shade, true, Color::WHITE),
                }
            }
            Paint::Flat(c, f) => self.art.flat(c, f),
            Paint::Liquid(l) => self.liquid(l),
        }
    }

    /// The material of water, lava or the air over lava (made once).
    fn liquid(&mut self, l: Liquid) -> Gd<Material> {
        if let Some(m) = self.liquids.get(&l) {
            return m.clone();
        }
        let path = match l {
            Liquid::Water | Liquid::Foam => "res://shaders/water.gdshader",
            Liquid::Lava => "res://shaders/lava.gdshader",
            Liquid::Haze => "res://shaders/heat_haze.gdshader",
        };
        let mat: Gd<Material> = match godot::tools::try_load::<Shader>(path) {
            Ok(shader) => {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(&shader);
                m.set_shader_parameter("noise_tex", &self.surfaces.noise().to_variant());
                m.set_shader_parameter("fow_tex", &self.surfaces.fow_texture().to_variant());
                if l == Liquid::Foam {
                    m.set_shader_parameter("bank", &true.to_variant());
                    // over the water it laps on
                    m.set_render_priority(1);
                }
                if l == Liquid::Lava {
                    let flow = |map: &str| {
                        godot::tools::try_load::<godot::classes::Texture2D>(&format!(
                            "res://art/cc0/texturecan/lava_flow/lava_flow_{map}.jpg"
                        ))
                        .ok()
                    };
                    if let (Some(a), Some(e)) = (flow("albedo"), flow("emission")) {
                        m.set_shader_parameter("flow_albedo", &a.to_variant());
                        m.set_shader_parameter("flow_glow", &e.to_variant());
                        m.set_shader_parameter("has_flow", &true.to_variant());
                    }
                }
                m.upcast()
            }
            Err(_) => {
                let c = match l {
                    Liquid::Water => Color::from_rgba(0.05, 0.12, 0.15, 0.85),
                    Liquid::Foam => Color::from_rgba(0.7, 0.75, 0.75, 0.0),
                    Liquid::Lava => Color::from_rgb(1.0, 0.35, 0.05),
                    Liquid::Haze => Color::from_rgba(1.0, 1.0, 1.0, 0.0),
                };
                self.art.flat(
                    c,
                    if l == Liquid::Lava {
                        Finish::Glow
                    } else {
                        Finish::Ghost
                    },
                )
            }
        };
        self.liquids.insert(l, mat.clone());
        mat
    }

    /// The dark rock under the level, with holes for what lies below the
    /// ground (stairs down, water, lava).
    fn lay_bedrock(&mut self) {
        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        st.set_normal(Vector3::UP);
        let y = -0.02;
        let mut quad = |x0: f32, z0: f32, x1: f32, z1: f32| {
            for (x, z) in [(x0, z0), (x1, z0), (x1, z1), (x0, z0), (x1, z1), (x0, z1)] {
                st.add_vertex(Vector3::new(x, y, z));
            }
        };
        // the level's field cell by cell (runs of cells along each row),
        // and a wide margin around it
        let (left, right) = (0.5, COLNO as f32 - 0.5);
        let (top, bottom) = (-0.5, ROWNO as f32 - 0.5);
        let m = 60.0;
        quad(left - m, top - m, right + m, top);
        quad(left - m, bottom, right + m, bottom + m);
        quad(left - m, top, left, bottom);
        quad(right, top, right + m, bottom);
        for row in 0..ROWNO {
            let mut run: Option<i32> = None;
            for x in 1..=COLNO {
                let solid = x < COLNO && !self.holes.contains(&(x, row));
                match (solid, run) {
                    (true, None) => run = Some(x),
                    (false, Some(x0)) => {
                        let z = row as f32;
                        quad(x0 as f32 - 0.5, z - 0.5, x as f32 - 0.5, z + 0.5);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        if let Some(mesh) = st.commit() {
            self.bedrock.set_mesh(&mesh);
        }
        // the branch's own rock (Gehennom's dark basalt)
        let name = self.branch_look.material("bedrock");
        if let Some(m) = self.art.manifest().material(name) {
            let mat = self.material(Paint::Pbr(m, SHADE_BEDROCK, Role::Void));
            self.bedrock.set_material_override(&mat);
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
        if self.hover_cell != cell {
            self.hover_cell = cell;
            self.hints_dirty = true;
        }
        self.show_hover();
    }

    /// Wheel steps: positive moves the camera away. Ends the overview.
    pub fn zoom(&mut self, steps: f32) {
        self.hints_dirty |= self.overview.is_some();
        self.overview = None;
        self.distance = (self.distance + steps * 1.5).clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.aim_lift = 0.0;
        self.place_camera();
    }

    /// Self-tests: the camera this far away, nearer than a player may zoom
    /// (close-ups of what the hero holds); the next zoom clamps it again.
    pub fn set_distance(&mut self, distance: f32, lift: f32) {
        self.overview = None;
        self.distance = distance;
        self.aim_lift = lift;
        self.place_camera();
    }

    /// Frame everything known of the level, or go back to following the hero.
    pub fn toggle_overview(&mut self) {
        self.overview = match self.overview {
            Some(_) => None,
            None => Some(self.distance),
        };
        self.hints_dirty = true;
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
        // a game starts: the rehearsal is over, the gallery's light off
        self.rehearsal = None;
        self.build_budget = BUILD_BUDGET;
        self.set_showcase(false);
        self.building.clear();
        self.finish_motions();
        self.facing.clear();
        for (_, c) in std::mem::take(&mut self.incoming) {
            self.art.give(c.model);
        }
        for (_, nodes) in self.cells.drain() {
            nodes.free(
                &mut self.art,
                &mut self.batches,
                &mut self.spare_lamps,
                &mut self.spare_props,
            );
        }
        self.batches.flush();
        self.vfx.clear();
        self.fow_dirty = true;
        if let Some(old) = self
            .outlined
            .take()
            .filter(|n| n.is_instance_valid() && !n.is_queued_for_deletion())
        {
            set_overlay(&old, None);
        }
        self.generation = None;
        self.hover.set_visible(false);
        self.hover_cell = None;
        self.hostile_ring.set_visible(false);
        self.hint_cells.clear();
        self.ring_cells.clear();
        self.cursor.set_visible(false);
        self.hero_ring.set_visible(false);
        self.hero_light.set_visible(false);
        self.underglow.set_visible(false);
        self.rim.set_visible(false);
        if let Some(old) = self.rim_model.take().filter(|n| n.is_instance_valid()) {
            set_layers(&old, 1);
            set_overlay(&old, None);
        }
        for l in &mut self.room_lights {
            l.set_visible(false);
        }
        self.place_torches(&[]);
        self.holes.clear();
        self.bedrock_dirty = true;
        self.engulf.set_visible(false);
        self.snap = true;
        self.hero_at = None;
        self.set_path(&[], false);
        self.hero_gear.reset();
        self.hero_fx.clear();
    }

    /// The hero uses an item (spec decision 8): they turn the way it goes
    /// and play the use's clip with the item in hand; its effects follow.
    pub fn show_use(&mut self, u: &ItemUse, catalog: &Catalog, world: &World) {
        use crate::hero::{Fx, THROW_AT, in_hand, use_color, use_effects, use_hand, use_name};
        let Some(at) = self.hero_at else {
            return;
        };
        let tile = u.tile.and_then(|t| object_tile(catalog, t));
        let held = tile.and_then(|t| self.art.manifest().held(t));
        let color = use_color(u, tile, self.art.manifest());
        let dir = u.dir.filter(|&d| d != (0, 0));
        // where it goes: on until something solid, a few cells at most
        let reach = dir.map(|(dx, dy)| {
            let mut c = at;
            for _ in 0..7 {
                let next = (c.0 + dx, c.1 + dy);
                if !is_open(world.map.cell(next.0, next.1), catalog) {
                    break;
                }
                c = next;
            }
            (c, (dx, dy))
        });
        let Some(nodes) = self.cells.get_mut(&at) else {
            return;
        };
        let Some(i) = nodes.look.entity.filter(|&i| i < nodes.models.len()) else {
            return;
        };
        let placed = &mut nodes.look.models[i];
        let yaw_from = placed.yaw;
        let yaw_to = dir.map_or(yaw_from, |d| yaw_toward((0, 0), d));
        placed.yaw = yaw_to;
        let height = placed.look.art.height;
        let pos = Vector3::new(at.0 as f32, 0.0, at.1 as f32) + placed.pos;
        let m = &mut nodes.models[i];
        let mut clips = self.art.clips(m, false);
        clips.attack = self.art.use_clip(m, use_name(u.kind));
        let hand_slot = use_hand(u.kind);
        if let (Some(h), Some(secs)) = (held, in_hand(u.kind)) {
            self.art.hold_for(m, h, secs, hand_slot);
        } else {
            // a use with nothing in hand (a spell): the last item used is
            // put away, not left in a hand that opens
            self.art.put_down(m);
        }
        if u.kind == UseKind::Cast {
            let secs = clips
                .attack
                .as_deref()
                .and_then(|a| clips.player.as_ref()?.get_animation(a))
                .map_or(1.6, |a| a.get_length());
            self.art.steady(m, secs);
        }
        let anchor = self.art.slot_anchor(m, hand_slot);
        let node = m.node.clone();
        self.hero_yaw = yaw_to;
        if let Some(i) = self.motions.iter().position(|m| m.cell == at) {
            self.motions.swap_remove(i).finish(false);
        }
        self.motions.push(Motion::strike(
            node,
            at,
            true,
            pos,
            (yaw_from, yaw_to),
            Vector3::ZERO,
            clips,
        ));
        let facing = Vector3::new(yaw_to.to_radians().sin(), 0.0, yaw_to.to_radians().cos());
        let hand = pos + Vector3::new(0.0, height * 0.62, 0.0) + facing * 0.25;
        let ahead = reach.map(|((x, y), (dx, dy))| {
            let end = Vector3::new(
                x as f32 + dx as f32 * 0.45,
                0.0,
                y as f32 + dy as f32 * 0.45,
            );
            match u.kind {
                UseKind::Kick => pos + facing * 0.7 + Vector3::new(0.0, 0.15, 0.0),
                _ => end + Vector3::new(0.0, hand.y, 0.0),
            }
        });
        for (delay, fx) in use_effects(u, color, hand, anchor, ahead) {
            self.hero_fx.after(delay, fx);
        }
        if matches!(u.kind, UseKind::Throw | UseKind::Fire) {
            // never smaller than a hand's breadth in flight: a stone of
            // 6 cm would vanish at the camera's distance
            let model = held.and_then(|h| self.art.held_prop(h, (0.16 / h.scale).max(0.7)));
            let to = ahead.unwrap_or(hand + facing * 3.0);
            self.hero_fx.after(
                THROW_AT + 0.25,
                Fx::Burst {
                    name: "landing",
                    kind: VfxKind::Sparks,
                    at: to,
                    anchor: None,
                },
            );
            self.hero_fx.after(
                THROW_AT,
                Fx::Throw {
                    model,
                    from: hand,
                    to,
                },
            );
        }
    }

    /// The effects of the hero's uses (self-tests).
    pub fn hero_fx(&self) -> &crate::hero::HeroFx {
        &self.hero_fx
    }

    /// The hero's model shows their gear; with a lamp lit in hand, the
    /// light over them is only the lamp's warm helper (no shadow of its own).
    fn equip_hero(&mut self, at: (i32, i32), world: &World, catalog: &Catalog, delta: f32) {
        self.hero_gear
            .update(&world.inventory, catalog, self.art.manifest());
        let Some(m) = self
            .cells
            .get_mut(&at)
            .and_then(|n| n.look.entity.and_then(|i| n.models.get_mut(i)))
        else {
            return;
        };
        let lamp = self.art.equip(m, self.hero_gear.gear(), delta);
        if self.hero_gear.set_lamp(lamp) {
            let (color, energy, range) = if lamp {
                (LAMP_POOL, LAMP_POOL_ENERGY, LAMP_POOL_RANGE)
            } else {
                (HERO_LIGHT, HERO_LIGHT_ENERGY, HERO_LIGHT_RANGE)
            };
            self.hero_light.set_color(color);
            self.hero_light.set_param(Param::ENERGY, energy);
            self.hero_light.set_param(Param::RANGE, range);
            self.hero_light.set_shadow(!lamp);
        }
    }

    /// The clip the hero's model plays now (self-tests).
    pub fn hero_clip(&self) -> Option<String> {
        let p = self.hero_model()?.player()?;
        p.is_playing()
            .then(|| p.get_current_animation().to_string())
    }

    /// The hero's model (self-tests look at its gear).
    pub fn hero_model(&self) -> Option<&Model> {
        let nodes = self.cells.get(&self.hero_at?)?;
        nodes.look.entity.and_then(|i| nodes.models.get(i))
    }

    /// The art gallery's lighting (for review): a key light over the
    /// hall, the fills and the ambient raised. A new game ends it.
    pub fn set_showcase(&mut self, on: bool) {
        if self.showcase == on {
            return;
        }
        self.showcase = on;
        self.studio.set_visible(on);
        self.env.set_ambient_light_energy(if on {
            GALLERY_AMBIENT
        } else {
            self.branch_look.ambient_energy
        });
        self.lights_dirty = true;
    }

    pub fn set_visible(&mut self, on: bool) {
        self.shown = on;
        self.show_parts();
    }

    /// What of the map draws: all of it in a game; behind the title while
    /// the rehearsal plays, a part more each frame (`showing`), so that no
    /// one frame draws it all for the first time: the world, then the
    /// vignette, the mist, the dust.
    fn show_parts(&mut self) {
        let parts = if self.shown {
            SHOW_ALL
        } else if self.rehearsal.is_some() {
            self.showing
        } else {
            0
        };
        self.root.set_visible(parts >= 1);
        self.post.set_visible(parts >= 2);
        self.mist.set_visible(parts >= 3);
        self.vfx.set_dust_shown(parts >= 4);
    }

    /// The camera has caught up with its target and a new level has come
    /// up out of black (self-test screenshots).
    pub fn is_settled(&self) -> bool {
        self.focus.distance_to(self.target) < 0.05 && self.fade <= 0.0 && self.building.is_empty()
    }

    /// Load art ahead of need for a few milliseconds (every frame, also
    /// before a game starts); false when everything is loaded.
    pub fn preload_step(&mut self) -> bool {
        let now = std::time::Instant::now();
        // behind the title, a part more of the map drawn each frame
        if self.rehearsal.is_some() && !self.shown && self.showing < SHOW_ALL {
            self.showing += 1;
            self.show_parts();
        }
        let mut more = self.art.preload_step();
        let art = now.elapsed();
        if let Some(mut r) = self.rehearsal.take() {
            // what it draws loads in the background meanwhile; it plays
            // once the art is loaded (a game started ends it)
            if more {
                r.fetch();
                self.rehearsal = Some(r);
            } else if r.step(self, self.root.get_process_delta_time()) {
                self.rehearsal = Some(r);
                more = true;
            } else {
                self.clear();
                self.set_visible(self.shown);
                if self.stats_window.is_some() {
                    godot_print!(
                        "map: rehearsal over {:.1} s after start",
                        godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0
                    );
                }
            }
        }
        let rehearsal = now.elapsed() - art;
        let showing = self.showing;
        let doing = self.rehearsal.as_ref().map(|r| {
            format!(
                "{}, {showing} of the map's {SHOW_ALL} parts shown",
                r.doing()
            )
        });
        self.title_frame(now, art, rehearsal, doing);
        if !more && self.stats_window.is_some() && !std::mem::replace(&mut self.preload_told, true)
        {
            godot_print!(
                "map: art loaded ahead {:.1} s after start, {} models built",
                godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0,
                self.art.counts().1
            );
        }
        more
    }

    /// A frame's time for drawing a new level's cells (None: the game's).
    pub(crate) fn set_build_budget(&mut self, budget: Option<std::time::Duration>) {
        self.build_budget = budget.unwrap_or(BUILD_BUDGET);
    }

    /// A title frame left to other work (the dialogs' warm-up), instead of
    /// `preload_step`: nothing is loaded ahead in it, and the title's
    /// frame times stay a frame each.
    #[allow(dead_code)] // until the dialogs' warm-up keeps frames of its own
    pub fn title_tick(&mut self) {
        let none = std::time::Duration::ZERO;
        let doing = Some("a frame left to other work".to_string());
        self.title_frame(std::time::Instant::now(), none, none, doing);
    }

    /// With RENETHACK_FRAME_STATS, the title's frames over 33 ms, with the
    /// work done in the one before (`now`: this frame's turn, then its
    /// own work).
    fn title_frame(
        &mut self,
        now: std::time::Instant,
        art: std::time::Duration,
        rehearsal: std::time::Duration,
        doing: Option<String>,
    ) {
        let frame = self.title_clock.replace(now).map(|t| now.duration_since(t));
        if self.stats_window.is_some() && !self.shown {
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            if let Some(f) = frame.filter(|f| ms(*f) > 33.0) {
                let (art, rehearsal, doing) = &self.title_work;
                godot_print!(
                    "map: a title frame of {:.1} ms; the work before it: art {:.1} ms, rehearsal {:.1} ms ({})",
                    ms(f),
                    ms(*art),
                    ms(*rehearsal),
                    doing.as_deref().unwrap_or("-")
                );
            }
        }
        self.title_work = (art, rehearsal, doing);
    }

    /// Nothing is left to load ahead (self-tests start a game then).
    pub fn preloaded(&self) -> bool {
        self.rehearsed && self.art.preloaded() && self.rehearsal.is_none()
    }

    /// Build ahead the models a game shows first: the role's hero (first
    /// of all) when one is chosen, else every role's, the pets and the
    /// monsters of the first levels.
    pub fn warm_up(&mut self, catalog: &Catalog, role: Option<&str>) {
        if !std::mem::replace(&mut self.rehearsed, true) {
            let fetch = self.prefetch_paths();
            self.rehearsal = Some(crate::rehearsal::Rehearsal::new(catalog, fetch));
            self.set_visible(self.shown);
        }
        let names: Vec<&str> = match role {
            Some(role) => vec![role_monster(role)],
            None => ROLES
                .iter()
                .map(|r| role_monster(r))
                .chain(WARM_MONSTERS.iter().copied())
                .collect(),
        };
        // the rehearsal's monsters and objects are built ahead with the
        // art (a model built as a level is drawn costs a long frame)
        let mut looks = match &self.rehearsal {
            Some(r) => self.models_of(r.map(), catalog),
            None => Vec::new(),
        };
        for name in names {
            let Some(info) = catalog.monsters.iter().find(|m| m.name == name) else {
                continue;
            };
            for flags in [0, mg::FEMALE] {
                let r = self.art.manifest().monster(info, flags);
                let look = ModelLook {
                    art: r,
                    tint: lifted(tint_color(r.tint, info.color)),
                    pose: Pose::Alive,
                };
                if !looks.contains(&look) {
                    looks.push(look);
                }
            }
        }
        self.art.warm(&looks, role.is_some());
    }

    /// The models of the monsters and objects on `map`.
    fn models_of(&self, map: &MapState, catalog: &Catalog) -> Vec<ModelLook> {
        let mut looks = Vec::new();
        for y in 0..ROWNO {
            for x in 0..COLNO {
                let Some(cell) = map.cell(x, y).filter(|c| c.entity().is_some()) else {
                    continue;
                };
                let ctx = Ctx {
                    catalog,
                    art: self.art.manifest(),
                    x,
                    y,
                    hero: None,
                    hero_yaw: 0.0,
                    facing: None,
                    branch: &self.branch_look,
                };
                for p in look_of(cell, Near::of(map, x, y), &ctx).models {
                    if !looks.contains(&p.look) {
                        looks.push(p.look);
                    }
                }
            }
        }
        looks
    }

    /// Build the stand-ins of looks first seen in a busy frame, as far as
    /// the frame's budget goes.
    fn complete_models(&mut self) {
        if self.art.pending() == 0 {
            return;
        }
        let rim = self.rim_model.as_ref().map(|n| n.instance_id());
        for nodes in self.cells.values_mut() {
            for m in nodes.models.iter_mut().filter(|m| m.is_pending()) {
                if !self.art.complete(m) {
                    return;
                }
                // the hero's rim light takes the meshes now there
                if Some(m.node.instance_id()) == rim {
                    self.rim_model = None;
                }
            }
        }
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
        let distance = self.camera_distance();
        let pitch = pitch_at(distance);
        // the darkness closes in at the same depth behind what is framed,
        // however far the camera is
        self.env.set_fog_depth_begin(distance + 3.0);
        self.env.set_fog_depth_end(distance + 16.0);
        let offset = Vector3::new(0.0, pitch.sin(), pitch.cos()) * distance;
        // aim a little south of the hero, so the hero stands above the
        // log; close in, the same offset would push the hero off the top
        let south = AIM_SOUTH * distance / DISTANCE;
        let aim = self.focus + Vector3::new(0.0, self.aim_lift, south);
        self.camera.look_at_from_position(aim + offset, aim);
        self.eye = aim + offset;
        RenderingServer::singleton().global_shader_parameter_set("eye_pos", &self.eye.to_variant());
    }

    /// Rings stay under their creatures as the models walk between cells.
    fn follow_rings(&mut self) {
        for c in &self.ring_cells {
            let Some(n) = self.cells.get_mut(c) else {
                continue;
            };
            let at = n
                .look
                .entity
                .and_then(|i| n.models.get(i))
                .filter(|m| m.node.is_instance_valid() && m.node.is_inside_tree())
                .map(|m| m.node.get_global_position());
            if let (Some(p), Some(d)) = (at, n.ring.as_mut()) {
                d.set_global_position(Vector3::new(p.x, n.look.ground, p.z));
            }
        }
    }

    /// A pet's ring on its cell's ground.
    fn place_ring(&mut self, nodes: &mut CellNodes, look: &Look, origin: Vector3) {
        let cell = (origin.x.round() as i32, origin.z.round() as i32);
        if look.ring.is_some() {
            self.ring_cells.insert(cell);
        } else {
            self.ring_cells.remove(&cell);
        }
        match (look.ring, nodes.ring.as_mut()) {
            (Some(c), Some(d)) => {
                d.set_modulate(c);
                d.set_position(origin + at(0.0, look.ground, 0.0));
            }
            (Some(c), None) => {
                let mut d = marker(&mut self.root, &self.ring_tex, 0.95);
                d.set_modulate(c);
                d.set_position(origin + at(0.0, look.ground, 0.0));
                d.set_visible(true);
                nodes.ring = Some(d);
            }
            (None, Some(_)) => {
                if let Some(mut d) = nodes.ring.take() {
                    d.queue_free();
                }
            }
            (None, None) => {}
        }
    }

    /// A branch's models on a cell: those of the old look go back to the
    /// pool, the new ones are taken from it (or made).
    fn place_props(&mut self, nodes: &mut CellNodes, look: &Look, origin: Vector3) {
        for (prop, mut node) in std::mem::take(&mut nodes.props) {
            node.set_visible(false);
            self.spare_props.entry(prop).or_default().push(node);
        }
        for p in &look.props {
            let node = match self.spare_props.get_mut(&p.prop).and_then(Vec::pop) {
                Some(n) => Some(n),
                None => self.new_prop(p.prop),
            };
            let Some(mut node) = node else {
                continue;
            };
            let basis = Basis::from_euler(EulerOrder::YXZ, at(0.0, p.yaw.to_radians(), 0.0))
                * Basis::from_scale(p.scale);
            node.set_transform(Transform3D::new(basis, origin + p.pos));
            node.set_visible(true);
            nodes.props.push((p.prop, node));
        }
    }

    /// A branch's model, made (on the level, not yet placed).
    fn new_prop(&mut self, prop: Prop) -> Option<Gd<Node3D>> {
        let scene = self
            .prop_scenes
            .entry(prop)
            .or_insert_with(|| godot::tools::try_load::<PackedScene>(prop.scene()).ok())
            .clone();
        let node = scene
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())?;
        // a set of several: the one this prop is, alone and centred
        if let Some(part) = prop.part() {
            for child in node.get_children().iter_shared() {
                if child.get_name() != part {
                    child.free();
                } else if let Ok(mut c) = child.try_cast::<Node3D>() {
                    c.set_position(Vector3::ZERO);
                }
            }
        }
        self.cells_root.add_child(&node);
        Some(node)
    }

    /// Make ahead one more of what a level of `branch` takes from the
    /// pools (torches, their lanterns, the branch's doors and candles):
    /// a level that first shows many of them costs a long frame. False
    /// when there are enough.
    pub(crate) fn warm_pools(&mut self, branch: Branch) -> bool {
        let look = crate::branch_look::look_of(branch);
        // a shadow cast once, alone (a block under the hero's light): the
        // shadow atlas is made then, not with the first level
        match self.shadow_block.take() {
            None if !self.shadow_made => {
                self.shadow_made = true;
                // in the map's own stone, so the first real geometry (its
                // buffers, its pipelines) is drawn here too
                let mut block = MeshInstance3D::new_alloc();
                block.set_mesh(&self.art.mesh(cuboid(0.5, 0.5, 0.5)));
                block.set_position(self.focus + at(0.0, 0.25, 0.0));
                if let Some(m) = self.art.manifest().material("floor") {
                    let mat = self.material(Paint::Pbr(m, SHADE_LIT, Role::Floor));
                    block.set_material_override(&mat);
                }
                self.root.add_child(&block);
                self.hero_light.set_position(self.focus + at(0.0, 2.4, 0.6));
                self.hero_light.set_visible(true);
                self.shadow_block = Some(block);
                return true;
            }
            Some(block) if self.shadow_frames < 3 => {
                // drawn a few frames
                self.shadow_frames += 1;
                self.shadow_block = Some(block);
                return true;
            }
            Some(mut block) => {
                block.queue_free();
                self.hero_light.set_visible(false);
                return true;
            }
            None => {}
        }
        // the liquids' materials (their shaders and the lava's flow)
        for l in [Liquid::Water, Liquid::Foam, Liquid::Lava, Liquid::Haze] {
            if !self.liquids.contains_key(&l) {
                self.liquid(l);
                return true;
            }
        }
        // the embers' particles alone first (their shader is a long
        // compile), then the torches
        if self.spare_embers.is_none() && self.torches.is_empty() {
            self.spare_embers = Some(self.vfx.embers());
            return true;
        }
        if self.torches.len() < TORCHES_AHEAD {
            let mut t = self.new_torch();
            t.node.set_visible(false);
            t.light.set_visible(false);
            self.torches.push(t);
            return true;
        }
        if look.lanterns
            && let Some(i) = self.torches.iter().position(|t| t.lantern.is_none())
        {
            // its scene loaded in one frame, hung in the next
            if !self.load_prop(Prop::Lantern) {
                self.hang_lantern(i);
            }
            return true;
        }
        let wanted = [
            (look.door, DOORS_AHEAD),
            (look.candles.then_some(Prop::Candle), CANDLES_AHEAD),
            (look.candles.then_some(Prop::Candelabra), CANDELABRAS_AHEAD),
        ];
        for (prop, n) in wanted {
            let Some(prop) = prop else {
                continue;
            };
            if self.spare_props.get(&prop).map_or(0, Vec::len) >= n {
                continue;
            }
            if self.load_prop(prop) {
                return true;
            }
            if let Some(mut node) = self.new_prop(prop) {
                node.set_visible(false);
                self.spare_props.entry(prop).or_default().push(node);
                return true;
            }
        }
        false
    }

    /// Draw `texture` on a small card before the camera (None: no card),
    /// so it reaches the graphics card now, not with a level.
    pub(crate) fn show_texture(&mut self, texture: Option<&Gd<godot::classes::Texture2D>>) {
        let Some(t) = texture else {
            if let Some(mut card) = self.card.take() {
                card.queue_free();
            }
            return;
        };
        let card = self.card.get_or_insert_with(|| {
            let mut card = MeshInstance3D::new_alloc();
            let mut quad = godot::classes::QuadMesh::new_gd();
            quad.set_size(Vector2::new(0.1, 0.1));
            card.set_mesh(&quad);
            let mut m = StandardMaterial3D::new_gd();
            m.set_shading_mode(ShadingMode::UNSHADED);
            card.set_material_override(&m);
            no_shadow(&mut card);
            self.root.add_child(&card);
            card
        });
        let eye = self.camera.get_global_transform();
        card.set_global_transform(eye * Transform3D::new(Basis::IDENTITY, at(0.0, 0.0, -1.0)));
        if let Some(mut m) = card
            .get_material_override()
            .and_then(|m| m.try_cast::<StandardMaterial3D>().ok())
        {
            m.set_texture(godot::classes::base_material_3d::TextureParam::ALBEDO, t);
        }
    }

    /// Load a prop's scene if it is not yet: true when it was (that is
    /// the frame's work).
    fn load_prop(&mut self, prop: Prop) -> bool {
        if self.prop_scenes.contains_key(&prop) {
            return false;
        }
        let scene = godot::tools::try_load::<PackedScene>(prop.scene()).ok();
        self.prop_scenes.insert(prop, scene);
        true
    }

    /// What the rehearsal's levels draw, loaded ahead in the background:
    /// the map's textures. (A scene or a shader loaded so stalls the frame
    /// that takes it in: the rehearsal loads those one a frame.)
    fn prefetch_paths(&self) -> Vec<String> {
        let mut paths = crate::surface::Surfaces::texture_paths(self.art.manifest());
        paths.extend(
            [
                Prop::CastleDoor,
                Prop::IronGate,
                Prop::Lantern,
                Prop::Candle,
                Prop::Candelabra,
            ]
            .map(|p| p.scene().to_string()),
        );
        paths.extend(
            ["albedo", "emission"]
                .map(|m| format!("res://art/cc0/texturecan/lava_flow/lava_flow_{m}.jpg")),
        );
        paths
    }

    /// The light of stairs on their cell: warm from above up the stairs,
    /// cold from below down them, with a little mist in the shaft.
    fn place_lamp(&mut self, nodes: &mut CellNodes, look: &Look, origin: Vector3) {
        if nodes.lamp.as_ref().map(|l| l.kind) == look.lamp {
            return;
        }
        if let Some(l) = nodes.lamp.take() {
            self.spare_lamps.push(l.hidden());
        }
        let Some(kind) = look.lamp else {
            return;
        };
        // a lamp put out before, of the same kind, else a new one
        if let Some(i) = self.spare_lamps.iter().position(|l| l.kind == kind) {
            let mut l = self.spare_lamps.swap_remove(i);
            let pos = origin + lamp_offset(kind);
            l.light.set_position(pos);
            l.light.set_visible(true);
            if let Some(m) = l.mist.as_mut() {
                m.set_position(origin + at(0.0, -0.3, 0.0));
                m.set_visible(true);
            }
            nodes.lamp = Some(l);
            return;
        }
        let mut light = OmniLight3D::new_alloc();
        light.set_shadow(false);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.0);
        light.set_position(origin + lamp_offset(kind));
        let mut mist = None;
        match kind {
            Lamp::Up => {
                light.set_color(STAIR_UP_LIGHT);
                light.set_param(Param::ENERGY, 0.6);
                light.set_param(Param::RANGE, 2.5);
            }
            Lamp::Lava => {
                light.set_color(Color::from_rgb(1.0, 0.4, 0.12));
                light.set_param(Param::ENERGY, 2.6);
                light.set_param(Param::RANGE, 3.6);
                light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 2.0);
            }
            Lamp::Down => {
                light.set_color(STAIR_DOWN_LIGHT);
                // above the far steps, so their treads catch it
                light.set_param(Param::ENERGY, 2.2);
                light.set_param(Param::RANGE, 2.0);
                // a thin mist: the treads show through it
                let mut mat = FogMaterial::new_gd();
                mat.set_density(0.012);
                mat.set_albedo(Color::from_rgb(0.6, 0.66, 0.8));
                mat.set_edge_fade(0.5);
                let mut fog = FogVolume::new_alloc();
                fog.set_size(Vector3::new(1.0, 1.0, 1.0));
                fog.set_position(origin + at(0.0, -0.3, 0.0));
                fog.set_material(&mat);
                self.root.add_child(&fog);
                mist = Some(fog);
            }
        }
        self.root.add_child(&light);
        nodes.lamp = Some(CellLamp { kind, light, mist });
    }

    fn update_cell(
        &mut self,
        x: i32,
        y: i32,
        world: &World,
        catalog: &Catalog,
        hero: Option<(i32, i32)>,
    ) {
        let here = world
            .map
            .cell(x, y)
            .and_then(|c| c.glyph.as_ref())
            .and_then(Ident::of);
        let facing = self
            .facing
            .get(&(x, y))
            .filter(|(id, _)| Some(*id) == here)
            .map(|&(_, yaw)| yaw);
        let ctx = Ctx {
            catalog,
            art: self.art.manifest(),
            x,
            y,
            hero,
            hero_yaw: self.hero_yaw,
            facing,
            branch: &self.branch_look,
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
        let mut slots = std::mem::take(&mut nodes.solids).into_iter();
        let mut kept_slots = Vec::with_capacity(look.solids.len());
        for (i, s) in look.solids.iter().enumerate() {
            let slot = slots.next();
            if let Some(slot) = slot
                && old.solids.get(i) == Some(s)
            {
                kept_slots.push(slot);
                continue;
            }
            if let Some(slot) = slot {
                self.batches.remove(slot);
            }
            let mesh = self.art.mesh(s.mesh);
            let mat = self.material(s.paint);
            let xform = solid_transform(origin, s);
            kept_slots.push(
                self.batches
                    .add((x, y), (s.mesh, &mesh), &mat, s.shadow, xform),
            );
        }
        for slot in slots {
            self.batches.remove(slot);
        }
        nodes.solids = kept_slots;
        self.place_ring(&mut nodes, &look, origin);
        self.place_lamp(&mut nodes, &look, origin);
        if old.props != look.props {
            self.place_props(&mut nodes, &look, origin);
        }
        if let Some(c) = look.blast
            && old.blast != look.blast
        {
            self.vfx.burst(
                VfxKind::Explosion(c),
                origin + at(0.0, look.ground.max(0.0) + 0.6, 0.0),
            );
        }
        if let Some((c, y)) = look.glint
            && old.glint != look.glint
        {
            self.vfx
                .burst_tinted(VfxKind::Sparkle, origin + at(0.0, y, 0.0), c);
        }
        // the effects' rays stop at what stands here
        let solid = (look.ground >= 0.3).then_some(look.top.max(look.ground));
        if (old.ground >= 0.3).then_some(old.top.max(old.ground)) != solid {
            self.vfx.set_solid((x, y), solid);
        }
        // a ray lights up as it crosses the cell
        if let Some((c, yaw, y)) = look.ray
            && old.ray != look.ray
        {
            let dir = Vector3::new(yaw.to_radians().sin(), 0.0, yaw.to_radians().cos()) * 0.55;
            let mid = origin + at(0.0, y, 0.0);
            self.vfx.ray(c, mid - dir, mid + dir);
        }
        if (old.seen, old.masonry) != (look.seen, look.masonry) {
            self.fow_dirty = true;
        }
        // models: kept when the look is the same (only moved), else given
        // back to the pool and taken anew
        let mut kept: Vec<Model> = Vec::with_capacity(look.models.len());
        let mut old_models = std::mem::take(&mut nodes.models).into_iter();
        let mut carried = self.incoming.remove(&(x, y));
        let hero_here = world
            .map
            .cell(x, y)
            .and_then(|c| c.glyph.as_ref())
            .is_some_and(|g| g.flags & mg::HERO != 0);
        for (i, p) in look.models.iter().enumerate() {
            let before = old.models.get(i);
            let reuse = old_models.next();
            // the entity that stepped here: its own node walks in
            if look.entity == Some(i)
                && let Some(c) = carried.take_if(|c| c.placed.look == p.look)
            {
                if let Some(m) = reuse {
                    self.art.give(m);
                }
                let hero = world
                    .map
                    .cell(x, y)
                    .and_then(|c| c.glyph.as_ref())
                    .is_some_and(|g| g.flags & mg::HERO != 0);
                let (secs, run) = match self.order_pace {
                    // an order's step lasts its tick, never a hurried run
                    Some(p) if hero && c.cells == 1 => (p, false),
                    _ => pace(c.cells, c.hurry),
                };
                let clips = self.art.clips(&c.model, run);
                self.motions.push(Motion::step(
                    c.model.node.clone(),
                    (x, y),
                    hero,
                    (c.from, c.placed.yaw),
                    (origin + p.pos, p.yaw),
                    clips,
                    p.look.art.height,
                    (secs, run),
                ));
                if hero {
                    self.stats.hero_steps += 1;
                } else {
                    self.stats.other_steps += 1;
                }
                kept.push(c.model);
                continue;
            }
            let mut m = match (before, reuse) {
                (Some(b), Some(m)) if b.look == p.look => m,
                (_, other) => {
                    if let Some(m) = other {
                        self.art.give(m);
                    }
                    if hero_here && look.entity == Some(i) {
                        self.art.take_hero(&p.look)
                    } else {
                        self.art.take(&p.look)
                    }
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
        if let Some(c) = carried {
            self.art.give(c.model);
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
        if look.letters.iter().any(|l| l.hint) {
            self.hint_cells.insert((x, y));
            self.hints_dirty = true;
        } else {
            self.hint_cells.remove(&(x, y));
        }
        if look.sunk != self.holes.contains(&(x, y)) {
            if look.sunk {
                self.holes.insert((x, y));
            } else {
                self.holes.remove(&(x, y));
            }
            self.bedrock_dirty = true;
        }
        nodes.look = look;
        self.cells.insert((x, y), nodes);
    }

    /// Take the models of the entities that stepped off their cells, for
    /// the cells they stepped to; they face the way they went.
    fn carry(&mut self, moves: &[Move], world: &World) {
        for mv in moves {
            let yaw = yaw_toward(mv.from, mv.to);
            if !mv.ident.is_hero() {
                self.facing.insert(mv.to, (mv.ident, yaw));
            }
            let Some(nodes) = self.cells.get_mut(&mv.from) else {
                continue;
            };
            let Some(i) = nodes.look.entity.filter(|&i| i < nodes.models.len()) else {
                continue;
            };
            let model = nodes.models.remove(i);
            let placed = nodes.look.models.remove(i);
            nodes.look.entity = None;
            let origin = Vector3::new(mv.from.0 as f32, 0.0, mv.from.1 as f32);
            let from = origin + placed.pos;
            let hurry = self.interrupted.contains(&mv.from);
            if let Some(old) = self.incoming.insert(
                mv.to,
                Carried {
                    model,
                    placed,
                    from,
                    hurry,
                    cells: mv.steps(),
                },
            ) {
                self.art.give(old.model);
            }
        }
        // a cell whose entity changed forgets which way the old one faced
        self.facing.retain(|&(x, y), (id, _)| {
            world
                .map
                .cell(x, y)
                .and_then(|c| c.glyph.as_ref())
                .and_then(Ident::of)
                == Some(*id)
        });
    }

    /// Fights the new messages tell of: the attacker turns to its target
    /// and strikes.
    fn watch_fights(&mut self, world: &World, catalog: &Catalog) {
        let last = world.log.last_seq();
        let seen = match self.log_seq {
            // the log of a game just started or restored is not news
            Some(seen) if seen <= last => seen,
            _ => {
                self.log_seq = Some(last);
                return;
            }
        };
        self.log_seq = Some(last);
        let names = |m: i32| {
            usize::try_from(m)
                .ok()
                .and_then(|i| catalog.monsters.get(i))
                .map(|info| {
                    let mut n = vec![info.name.as_str()];
                    n.extend(info.male.as_deref());
                    n.extend(info.female.as_deref());
                    n
                })
                .unwrap_or_default()
        };
        let fights: Vec<_> = world
            .log
            .since(seen)
            .filter(|m| !m.from_history)
            .filter_map(|m| Some((parse_attack(&m.text)?, !m.text.contains(" miss"))))
            .filter_map(|(a, hit)| Some((locate_attack(&a, &world.map, names)?, hit)))
            .collect();
        for ((from, to), hit) in fights {
            self.strike(from, to, world);
            if hit {
                self.impact(from, to, world, catalog);
                // the one struck reels (its hit clip), and the blow
                // flashes on its chest as it lands
                let struck = self.cells.get(&to).and_then(|n| {
                    n.look
                        .entity
                        .and_then(|i| n.models.get(i).zip(n.look.models.get(i)))
                });
                if let Some((m, placed)) = struck {
                    let height = placed.look.art.height;
                    if self.art.flinch(m) {
                        self.stats.flinches += 1;
                    }
                    let toward = Vector3::new((from.0 - to.0) as f32, 0.0, (from.1 - to.1) as f32)
                        .normalized();
                    let chest = Vector3::new(to.0 as f32, self.ground(to.0, to.1), to.1 as f32)
                        + Vector3::new(0.0, height * 0.6, 0.0)
                        + toward * 0.25;
                    self.vfx.burst_after(VfxKind::Sparkle, chest, CONTACT_SECS);
                }
            }
        }
    }

    /// A blow lands: sparks and a flash where it meets the target, and the
    /// target bleeds (if it has blood), as the attack clip reaches it.
    fn impact(&mut self, from: (i32, i32), to: (i32, i32), world: &World, catalog: &Catalog) {
        let target = world.map.cell(to.0, to.1).and_then(|c| c.glyph.as_ref());
        let height = self
            .cells
            .get(&to)
            .and_then(|n| n.look.entity.and_then(|i| n.look.models.get(i)))
            .map_or(1.0, |p| p.look.art.height);
        let ground = self.ground(to.0, to.1);
        let toward = Vector3::new((from.0 - to.0) as f32, 0.0, (from.1 - to.1) as f32).normalized();
        let at = Vector3::new(
            to.0 as f32,
            ground + (height * 0.6).clamp(0.2, 1.2),
            to.1 as f32,
        ) + toward * 0.25;
        // sparks thrown back towards the striker
        self.vfx.hit(at, toward, CONTACT_SECS);
        let blood = target
            .filter(|g| g.kind == GlyphKind::Mon)
            .and_then(|g| monster_info(catalog, g.mon))
            .and_then(blood_of);
        if let Some(c) = blood {
            let floor = Vector3::new(to.0 as f32, ground, to.1 as f32);
            self.vfx
                .burst_after(VfxKind::Blood(c), floor.lerp(at, 0.5), CONTACT_SECS + 0.02);
        }
    }

    /// The entity at `from` turns to `to` and attacks.
    fn strike(&mut self, from: (i32, i32), to: (i32, i32), world: &World) {
        let yaw = yaw_toward(from, to);
        let ident = world
            .map
            .cell(from.0, from.1)
            .and_then(|c| c.glyph.as_ref())
            .and_then(Ident::of);
        let hero = ident.is_some_and(|i| i.is_hero());
        if hero {
            self.hero_yaw = yaw;
        } else if let Some(id) = ident {
            self.facing.insert(from, (id, yaw));
        }
        // still walking in: it arrives facing its target
        if let Some(m) = self
            .motions
            .iter_mut()
            .find(|m| m.cell == from && m.is_step())
        {
            m.yaw_to = yaw;
            if let Some(p) = self
                .cells
                .get_mut(&from)
                .and_then(|n| n.look.entity.and_then(|i| n.look.models.get_mut(i)))
            {
                p.yaw = yaw;
            }
            return;
        }
        // a blow after a blow: the first one ends
        if let Some(i) = self.motions.iter().position(|m| m.cell == from) {
            self.motions.swap_remove(i).finish(false);
        }
        let Some(nodes) = self.cells.get_mut(&from) else {
            return;
        };
        let Some(i) = nodes.look.entity.filter(|&i| i < nodes.models.len()) else {
            return;
        };
        let placed = &mut nodes.look.models[i];
        let yaw_from = placed.yaw;
        placed.yaw = yaw;
        let at = Vector3::new(from.0 as f32, 0.0, from.1 as f32) + placed.pos;
        let toward = Vector3::new((to.0 - from.0) as f32, 0.0, (to.1 - from.1) as f32).normalized();
        let model = &nodes.models[i];
        let node = model.node.clone();
        let clips = self.art.clips(model, false);
        self.motions.push(Motion::strike(
            node,
            from,
            hero,
            at,
            (yaw_from, yaw),
            toward,
            clips,
        ));
        self.stats.strikes += 1;
    }

    fn advance_motions(&mut self, delta: f32) {
        let hold = self.hold;
        let walking = self.order_pace.is_some();
        let mut i = 0;
        while i < self.motions.len() {
            if self.motions[i].advance(delta, hold) {
                let m = self.motions.swap_remove(i);
                let keep = walking && m.hero && m.is_step();
                m.finish(keep);
            } else {
                i += 1;
            }
        }
    }

    /// End every motion where the world model has its entity; remember the
    /// steps cut short (the next step there hurries).
    fn finish_motions(&mut self) {
        self.interrupted.clear();
        let walking = self.order_pace.is_some();
        for m in self.motions.drain(..) {
            let (cell, step) = (m.cell, m.is_step());
            let keep = walking && m.hero && step;
            if !m.finish(keep) && step {
                self.interrupted.insert(cell);
            }
        }
    }

    /// An order walks the hero: each step lasts `secs` (the tick) and the
    /// stride goes on from step to step. None: the order is over, the hero
    /// stands idle again once the last step is played.
    pub fn set_order_pace(&mut self, secs: Option<f32>) {
        if self.order_pace == secs {
            return;
        }
        self.order_pace = secs;
        if secs.is_none() && !self.steps_under_way().0 {
            self.settle_hero();
        }
    }

    /// The hero's model plays its idle clip (after an order's last step).
    fn settle_hero(&mut self) {
        let Some(at) = self.hero_at else {
            return;
        };
        let Some(nodes) = self.cells.get(&at) else {
            return;
        };
        let Some(model) = nodes.look.entity.and_then(|i| nodes.models.get(i)) else {
            return;
        };
        let mut clips = self.art.clips(model, false);
        if let (Some(p), Some(idle)) = (clips.player.as_mut(), clips.idle.as_ref())
            && p.is_instance_valid()
            && p.get_current_animation().to_string() != *idle
        {
            p.play_ex().name(idle.as_str()).custom_blend(0.2).done();
        }
    }

    /// The clip the hero's model plays now, and its idle clip (self-tests).
    pub fn hero_animation(&mut self) -> (Option<String>, Option<String>) {
        let Some(nodes) = self.hero_at.and_then(|at| self.cells.get(&at)) else {
            return (None, None);
        };
        let Some(model) = nodes.look.entity.and_then(|i| nodes.models.get(i)) else {
            return (None, None);
        };
        let clips = self.art.clips(model, false);
        let now = clips
            .player
            .as_ref()
            .filter(|p| p.is_instance_valid() && p.is_playing())
            .map(|p| p.get_current_animation().to_string());
        (now, clips.idle)
    }

    /// The hero's step is being played.
    pub fn hero_walking(&self) -> bool {
        self.steps_under_way().0
    }

    /// Marks on the cells of a way (the goal larger); red in a fight.
    /// Godot hears only of changes.
    pub fn set_path(&mut self, cells: &[(i32, i32)], combat: bool) {
        if self.path_shown.0 == cells && self.path_shown.1 == combat {
            return;
        }
        self.path_shown = (cells.to_vec(), combat);
        let (dot, goal) = if combat {
            (PATH_COMBAT, PATH_COMBAT_GOAL)
        } else {
            (PATH_EXPLORE, PATH_EXPLORE_GOAL)
        };
        for (i, &(x, y)) in cells.iter().enumerate() {
            if i == self.path_marks.len() {
                let d = marker(&mut self.root, &self.soft_tex, 0.3);
                self.path_marks.push(d);
            }
            let last = i + 1 == cells.len();
            let ground = self.ground(x, y);
            // in a fight a click takes the first step only: it stands out
            // the goal is where the order walks to (a soft disc, never a
            // ring a creature could be taken to stand in)
            let (tex, size, color) = if last {
                (&self.soft_tex, 0.5, goal)
            } else if combat && i == 0 {
                (&self.soft_tex, 0.4, goal)
            } else {
                (&self.soft_tex, 0.26, dot)
            };
            let d = &mut self.path_marks[i];
            d.set_texture(godot::classes::decal::DecalTexture::ALBEDO, tex);
            d.set_texture(godot::classes::decal::DecalTexture::EMISSION, tex);
            d.set_size(Vector3::new(size, 0.24, size));
            d.set_modulate(color.with_alpha(0.75));
            d.set_position(Vector3::new(x as f32, ground, y as f32));
            d.set_visible(true);
        }
        for d in self.path_marks.iter_mut().skip(cells.len()) {
            d.set_visible(false);
        }
    }

    /// The way shown now (self-tests).
    pub fn path_shown(&self) -> &[(i32, i32)] {
        &self.path_shown.0
    }

    /// Where the hero's model is while it walks (None: on its cell).
    fn hero_shown_at(&self) -> Option<Vector3> {
        self.motions
            .iter()
            .find(|m| m.hero && m.is_step())
            .map(|m| m.base)
    }

    /// Stop motions at this share of their way (self-test pictures); None
    /// lets them go on.
    pub fn hold_motions(&mut self, at: Option<f32>) {
        self.hold = at;
    }

    /// Every motion has reached the hold point (or there are none).
    pub fn motions_held(&self) -> bool {
        self.motions.iter().all(|m| m.is_held(self.hold))
    }

    /// Steps now under way: (the hero's, others').
    pub fn steps_under_way(&self) -> (bool, usize) {
        let steps = self.motions.iter().filter(|m| m.is_step());
        let hero = steps.clone().any(|m| m.hero);
        (hero, steps.filter(|m| !m.hero).count())
    }

    /// The hero's step's gait clip and the clip playing now (self-tests).
    pub fn hero_clips(&self) -> (Option<String>, Option<String>) {
        self.motions
            .iter()
            .find(|m| m.hero && m.is_step())
            .map(|m| m.clips_now())
            .unwrap_or_default()
    }

    /// The hero's step under way: (from, to, the yaw it ends with, where
    /// the model is now) (self-tests).
    pub fn hero_step(&self) -> Option<(Vector3, Vector3, f32, Vector3)> {
        self.motions
            .iter()
            .find(|m| m.hero && m.is_step())
            .map(|m| (m.from, m.to, m.yaw_to, m.base))
    }

    pub fn motion_stats(&self) -> MotionStats {
        self.stats
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

/// A wall cell a torch hangs on, and the way it faces into its room.
type TorchWall = ((i32, i32), (i32, i32));

/// The walls of a lit area a torch hangs on, and the way each faces into
/// the area: every fifth cell of the north walls and every fourth of the
/// east and west ones (the camera does not see the south wall's face), at
/// least one per area.
fn torch_walls(area: &[(i32, i32)], is_wall: impl Fn(i32, i32) -> bool) -> Vec<TorchWall> {
    let mut walls: Vec<TorchWall> = area
        .iter()
        .flat_map(|&(x, y)| {
            [
                ((x, y - 1), (0, 1)),
                ((x + 1, y), (-1, 0)),
                ((x - 1, y), (1, 0)),
            ]
        })
        .filter(|&((x, y), _)| is_wall(x, y))
        .collect();
    walls.sort_unstable();
    walls.dedup();
    let chosen: Vec<_> = walls
        .iter()
        .copied()
        .filter(|&((x, y), (dx, _))| {
            if dx == 0 {
                (x + 2 * y).rem_euclid(5) == 0
            } else {
                (y + 2 * x).rem_euclid(4) == 0
            }
        })
        .collect();
    if !chosen.is_empty() {
        return chosen;
    }
    // none fell on the pattern: the north wall's middle
    let north: Vec<_> = walls.iter().filter(|(_, (dx, _))| *dx == 0).collect();
    north
        .get(north.len() / 2)
        .or(walls.first().as_ref())
        .map(|&&w| vec![w])
        .unwrap_or_default()
}

/// Cells between the fill lights of a big lit area.
const FILL_STEP: i32 = 10;

/// The fill lights of a lit area: (x, z, range, energy). A room is lit
/// faintly from its middle; a hall wider than a light reaches gets one
/// every few cells, so its middle is not a dark pit between the torches.
fn fill_lights(area: &[(i32, i32)]) -> Vec<(f32, f32, f32, f32)> {
    let n = area.len() as f32;
    let cx = area.iter().map(|c| c.0 as f32).sum::<f32>() / n;
    let cy = area.iter().map(|c| c.1 as f32).sum::<f32>() / n;
    let reach = area
        .iter()
        .map(|c| ((c.0 as f32 - cx).powi(2) + (c.1 as f32 - cy).powi(2)).sqrt())
        .fold(0.0f32, f32::max);
    if reach <= FILL_STEP as f32 {
        return vec![(
            cx,
            cy,
            (reach * 1.35 + 2.5).clamp(3.5, 16.0),
            ROOM_LIGHT_ENERGY,
        )];
    }
    let (x0, x1) = area
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.0), b.max(c.0)));
    let (y0, y1) = area
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.1), b.max(c.1)));
    let cells: HashSet<(i32, i32)> = area.iter().copied().collect();
    let mut lights = Vec::new();
    let mut y = y0 + (y1 - y0) % FILL_STEP / 2;
    while y <= y1 {
        let mut x = x0 + (x1 - x0) % FILL_STEP / 2;
        while x <= x1 {
            if cells.contains(&(x, y)) {
                lights.push((
                    x as f32,
                    y as f32,
                    FILL_STEP as f32 * 1.3,
                    ROOM_LIGHT_ENERGY * 8.0,
                ));
            }
            x += FILL_STEP;
        }
        y += FILL_STEP;
    }
    if lights.is_empty() {
        lights.push((cx, cy, 16.0, ROOM_LIGHT_ENERGY));
    }
    lights
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
    let pitch = PITCH_FAR_DEG.to_radians();
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

/// Something stands between `eye` and `target` (not on the cell `own`):
/// `top(x, y)` is how high a cell's solid reaches, None where nothing
/// solid stands.
fn hides(
    target: Vector3,
    eye: Vector3,
    own: (i32, i32),
    top: impl Fn(i32, i32) -> Option<f32>,
) -> bool {
    let span = eye - target;
    let steps = (span.length() / 0.1).ceil().max(1.0) as i32;
    (1..steps).any(|i| {
        let p = target + span * (i as f32 / steps as f32);
        let cell = (p.x.round() as i32, p.z.round() as i32);
        p.y < MAX_TOP && cell != own && top(cell.0, cell.1).is_some_and(|t| t > p.y)
    })
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
        branch: BranchLook,
    }

    impl Fixture {
        fn new() -> Fixture {
            Fixture {
                cat: catalog(),
                art: manifest(),
                branch: crate::branch_look::look_of(Branch::Main),
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
                facing: None,
                branch: &self.branch,
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
    fn closed_doors_and_wall_tops_take_the_branch_s_props() {
        let f = Fixture::new();
        let cat = &f.cat;
        let wall = feature(cat, "S_hwall");
        let floor = feature(cat, "S_room");
        for (b, door) in [
            (Branch::Vlad, Prop::IronGate),
            (Branch::Ludios, Prop::CastleDoor),
        ] {
            let look = crate::branch_look::look_of(b);
            let ctx = Ctx {
                branch: &look,
                ..f.ctx()
            };
            // a door in a horizontal wall, rock north of it
            let near = Near::orth([None, Some(&floor), Some(&wall), Some(&wall)]);
            let d = look_of(&feature(cat, "S_hcdoor"), near, &ctx);
            assert_eq!(d.props.len(), 1, "{b:?}");
            assert_eq!(d.props[0].prop, door);
            assert!(d.top >= DOOR_HEIGHT, "{b:?}: it stands");
            // the main dungeon keeps its planks
            let main = look_of(&feature(cat, "S_hcdoor"), near, &f.ctx());
            assert!(main.props.is_empty());
        }
        // Vlad's walls beside a room carry candles here and there, the
        // same ones every time, never on a wall cut down
        let look = crate::branch_look::look_of(Branch::Vlad);
        let candles = (0..60)
            .filter(|&x| {
                let ctx = Ctx {
                    branch: &look,
                    ..f.ctx_at(x, 3, None)
                };
                let near = Near::orth([None, Some(&floor), None, None]);
                look_of(&wall, near, &ctx)
                    .props
                    .iter()
                    .any(|p| p.prop == Prop::Candle)
            })
            .count();
        assert!(candles > 2 && candles < 30, "{candles}");
    }

    #[test]
    fn every_branch_draws_in_materials_the_manifest_has() {
        let f = Fixture::new();
        let cat = &f.cat;
        let branches = [
            Branch::Mines,
            Branch::Sokoban,
            Branch::Gehennom,
            Branch::Quest,
            Branch::Ludios,
            Branch::Vlad,
            Branch::Planes(nh_world::Plane::Earth),
            Branch::Planes(nh_world::Plane::Fire),
            Branch::Planes(nh_world::Plane::Astral),
        ];
        let floor = feature(cat, "S_room");
        for b in branches {
            let look = crate::branch_look::look_of(b);
            for (_, to) in look.remap {
                assert!(f.art.material(to).is_some(), "{b:?}: no material {to}");
            }
            assert!(
                f.art.material(look.cap).is_some(),
                "{b:?}: no cap {}",
                look.cap
            );
            let ctx = Ctx {
                branch: &look,
                ..f.ctx()
            };
            let wall = look_of(
                &feature(cat, "S_hwall"),
                Near::orth([None, Some(&floor), None, None]),
                &ctx,
            );
            let name = |l: &Look| match l.solids[0].paint {
                Paint::Pbr(m, _, _) => f.art.material_at(m).0.to_string(),
                other => panic!("{other:?}"),
            };
            assert_eq!(name(&wall), look.material("masonry"), "{b:?}");
            // a cave's walls are its rock, not masonry under a cap
            if look.cave {
                assert!(matches!(wall.solids[0].mesh, MeshKey::Rock(..)), "{b:?}");
            }
        }
    }

    #[test]
    fn the_rock_lies_along_corridors_and_outside_walls_never_in_a_room() {
        let f = Fixture::new();
        let cat = &f.cat;
        let (corr, wall, floor) = (
            feature(cat, "S_corr"),
            feature(cat, "S_hwall"),
            feature(cat, "S_darkroom"),
        );
        let unknown = Cell::default();
        let stone = feature(cat, "S_stone");
        let at = |cells: [Option<&Cell>; 8], far: Option<&Cell>| {
            let near = Near {
                cells,
                far_north: far,
            };
            look_of(&unknown, near, &f.ctx())
        };
        let none = [None; 8];
        // nothing known around: nothing drawn
        assert!(at(none, None).is_empty());
        // beside a corridor (east of it): a rock at full height
        let mut beside = none;
        beside[2] = Some(&corr);
        let r = at(beside, None);
        assert!(r.rock && r.seen == Seen::Rock);
        assert!((r.ground - ROCK_HEIGHT).abs() < 1e-3);
        assert!(
            r.solids
                .iter()
                .all(|s| matches!(s.paint, Paint::Pbr(_, _, Role::Rock)))
        );
        // south of a corridor, or of a wall cut down in front of one: low
        let mut south = none;
        south[0] = Some(&corr);
        assert!((at(south, None).ground - ROCK_CUT).abs() < 1e-3);
        let mut behind = none;
        behind[0] = Some(&wall);
        assert!((at(behind, Some(&floor)).ground - ROCK_CUT).abs() < 1e-3);
        // outside a wall: rock; beside a wall and a floor (a dark room's
        // unexplored floor): nothing
        assert!(at(behind, None).rock);
        let mut inside = behind;
        inside[4] = Some(&floor);
        assert!(at(inside, None).is_empty());
        // solid stone shown as such is rock the same way
        let near = Near {
            cells: beside,
            far_north: None,
        };
        assert!(look_of(&stone, near, &f.ctx()).rock);
    }

    #[test]
    fn walls_have_caps_plinths_by_open_ground_and_piers_at_corners() {
        let f = Fixture::new();
        let cat = &f.cat;
        let floor = feature(cat, "S_room");
        let wall_with = |sym: &str, near: [Option<&Cell>; 4]| {
            look_of(&feature(cat, sym), Near::orth(near), &f.ctx())
        };
        // a north wall with the room south of it: body, cap, one plinth
        let north = wall_with("S_hwall", [None, Some(&floor), None, None]);
        assert_eq!(north.solids.len(), 3);
        assert!(
            north
                .solids
                .iter()
                .all(|s| matches!(s.mesh, MeshKey::Bevel(..)))
        );
        assert!(
            north.solids.iter().any(|s| s.pos.z > 0.5),
            "a plinth on the room's side"
        );
        assert!(north.masonry && north.wall);
        // a corner is a pier, taller than the walls
        let corner = wall_with("S_tlcorn", [None, Some(&floor), None, Some(&floor)]);
        assert!(corner.top > north.top);
        // a doorway in a side wall has a frame but no cap over the passage
        let wall = feature(cat, "S_vwall");
        let side_door = wall_with("S_ndoor", [Some(&wall), Some(&wall), Some(&floor), None]);
        let top_door = look_of(
            &feature(cat, "S_ndoor"),
            Near::orth([None, Some(&floor), Some(&feature(cat, "S_hwall")), None]),
            &f.ctx(),
        );
        let capped = |l: &Look| l.solids.iter().any(|s| s.pos.y > WALL_HEIGHT - CAP_HEIGHT);
        assert!(!capped(&side_door) && capped(&top_door));
        assert!(side_door.solids.len() >= 4, "floor, posts, lintel, sill");
    }

    #[test]
    fn the_hero_is_hidden_only_by_something_tall_between_them_and_the_eye() {
        let pitch = PITCH_DEG.to_radians();
        let hero = Vector3::new(10.0, 0.0, 5.0);
        let aim = hero + Vector3::new(0.0, 0.0, AIM_SOUTH);
        let eye = aim + Vector3::new(0.0, pitch.sin(), pitch.cos()) * DISTANCE;
        let chest = hero + Vector3::new(0.0, 1.0, 0.0);
        // open ground all round
        assert!(!hides(chest, eye, (10, 5), |_, _| None));
        // a full wall south of the hero hides them, one north does not
        let south = |x, y| ((x, y) == (10, 6)).then_some(WALL_HEIGHT);
        assert!(hides(chest, eye, (10, 5), south));
        let north = |x, y| ((x, y) == (10, 4)).then_some(WALL_HEIGHT);
        assert!(!hides(chest, eye, (10, 5), north));
        // walls beside the hero (a doorway, a corridor) do not
        let sides = |x: i32, y| (x != 10 && y == 5).then_some(WALL_HEIGHT);
        assert!(!hides(chest, eye, (10, 5), sides));
    }

    #[test]
    fn a_level_is_built_from_the_hero_outwards() {
        let order = build_order(Some((10, 5)));
        assert_eq!(order.len(), ((COLNO - 1) * ROWNO) as usize);
        // taken from the end: the hero's cell first, then its neighbours
        assert_eq!(order.last(), Some(&(10, 5)));
        let d = |&(x, y): &(i32, i32)| (x - 10).pow(2) + (y - 5).pow(2);
        assert!(order.windows(2).all(|w| d(&w[0]) >= d(&w[1])));
    }

    #[test]
    fn the_overview_fits_the_level_and_centres_it() {
        let wide = 16.0 / 9.0;
        // a whole level: 79 columns decide
        let (centre, full) = overview_frame((1, 0, 79, 20), wide);
        assert_eq!((centre.x, centre.z), (40.0, 10.0));
        let tan = (FOV_DEG.to_radians() / 2.0).tan();
        assert!(2.0 * full * tan * wide >= 81.0 - 1e-3, "{full}");
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
            // runes cut in it, dark in their grooves
            let runes: Vec<_> = e.solids.iter().skip(p.solids.len()).collect();
            assert!(runes.len() >= 6, "{engr}");
            assert!(
                runes
                    .iter()
                    .all(|s| s.paint == Paint::Flat(ENGRAVING, Finish::Matte))
            );
        }
    }

    #[test]
    fn floors_walls_and_corridors_are_textured_stone_masonry_and_dirt() {
        let f = Fixture::new();
        let cat = &f.cat;
        let pbr = |sym: &str| match f.look(&feature(cat, sym)).solids[0].paint {
            Paint::Pbr(m, shade, _) => (f.art.material_at(m).0.to_string(), shade),
            other => panic!("{sym}: {other:?}"),
        };
        assert_eq!(pbr("S_room").0, "floor");
        assert_eq!(pbr("S_corr").0, "dirt");
        assert_eq!(pbr("S_vwall").0, "masonry");
        let has = |sym: &str, name: &str| {
            f.look(&feature(cat, sym))
                .solids
                .iter()
                .any(|s| matches!(s.paint, Paint::Pbr(m, _, _) if f.art.material_at(m).0 == name))
        };
        assert!(has("S_hcdoor", "wood") && has("S_hcdoor", "iron"));
        assert!(has("S_altar", "marble") && has("S_fountain", "marble"));
        assert!(has("S_upstair", "floor") && has("S_bars", "iron"));
        // walls under a cap of dark rock
        assert!(has("S_vwall", "bedrock"));
        // the part of a room out of view is the same stone, remembered
        // (the surface shader darkens it by the fog of war)
        assert_eq!(pbr("S_darkroom"), pbr("S_room"));
        assert_eq!(f.look(&feature(cat, "S_room")).seen, Seen::LitFloor);
        assert_eq!(f.look(&feature(cat, "S_darkroom")).seen, Seen::DarkFloor);
        assert!(f.look(&feature(cat, "S_room")).lit);
        assert!(!f.look(&feature(cat, "S_darkroom")).lit);
        // one brightness for every floor cell in view: the texture varies
        // it (a brightness per cell would show the grid)
        let shade_at = |x, y| match look_of(
            &feature(cat, "S_room"),
            Near::default(),
            &f.ctx_at(x, y, None),
        )
        .solids[0]
            .paint
        {
            Paint::Pbr(_, s, _) => s,
            _ => 0,
        };
        let shades: std::collections::HashSet<u8> = (0..20).map(|x| shade_at(x, 3)).collect();
        assert_eq!(shades.len(), 1);
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
        assert_eq!(pet.ring, Some(PET_RING));
        let seen = look(monster(cat, "little dog", mg::DETECT));
        assert_eq!(seen.models[0].look.pose, Pose::Ghost);
        // a red ring under the pointer for all but the hero and pets
        assert!(newt.hostile && !pet.hostile);
        assert!(!look(monster(cat, "human", mg::HERO)).hostile);
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
                matches!(s.mesh, MeshKey::Bevel(..)) && s.pos.z.abs() < 0.1 && s.pos.x.abs() < 0.1
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
                Near::orth([Some(north), None, None, None]),
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
        // a door stands in its frame, the wall closed above it
        assert!(top("S_vcdoor", &feature(cat, "S_vwall")) >= DOOR_HEIGHT);
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
        // a piece of a ray along the cell (the effects draw it), no solid
        let (_, yaw, y) = beam.ray.expect("a ray");
        assert_eq!(yaw, 0.0);
        assert!(y > 0.3);
        assert_eq!(beam.solids.len(), 1, "only the floor");
        let spark = f.look(&on_floor(cat, cmap(cat, "S_ss1")));
        assert!(spark.glint.is_some() && spark.ray.is_none());
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
            f.look(&feature(cat, "S_room")).solids[0].paint
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
        let tile = |near: [Option<&Cell>; 4]| look_of(&under, Near::orth(near), &f.ctx()).solids[0];
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
        // walls alone say nothing about the floor: the branch's floor
        assert_eq!(tile([Some(&wall), None, None, None]).paint, alone.paint);
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
        // and its light sways a few centimetres, never up or down
        let sways: Vec<Vector3> = (0..200).map(|i| sway(f64::from(i) * 0.013)).collect();
        assert!(sways.iter().all(|v| v.y == 0.0 && v.length() < 0.05));
        assert!(sways.iter().any(|v| v.length() > 0.015));
    }

    #[test]
    fn the_camera_looks_more_from_above_further_out() {
        assert!((pitch_at(DISTANCE) - PITCH_DEG.to_radians()).abs() < 1e-6);
        assert!((pitch_at(MIN_DISTANCE) - PITCH_NEAR_DEG.to_radians()).abs() < 1e-6);
        assert!((pitch_at(MAX_OVERVIEW_DISTANCE) - PITCH_FAR_DEG.to_radians()).abs() < 1e-6);
        let steps: Vec<f32> = (0..30).map(|i| pitch_at(6.0 + i as f32)).collect();
        assert!(steps.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn torches_hang_on_the_walls_the_camera_sees_facing_in() {
        // a room of 10 x 3 floor cells, walls all round
        let area: Vec<(i32, i32)> = (5..15).flat_map(|x| (4..7).map(move |y| (x, y))).collect();
        let wall = |x: i32, y: i32| {
            (4..=15).contains(&x) && (3..=7).contains(&y) && !area.contains(&(x, y))
        };
        let torches = torch_walls(&area, wall);
        assert!(!torches.is_empty());
        for &((x, y), (dx, dz)) in &torches {
            assert!(wall(x, y));
            // the cell it faces is the room's; never the south wall
            assert!(area.contains(&(x + dx, y + dz)), "{x},{y}");
            assert!(y != 7);
        }
        // spaced out: no two side by side on one wall
        for a in &torches {
            for b in &torches {
                let d = (a.0.0 - b.0.0).abs() + (a.0.1 - b.0.1).abs();
                assert!(a == b || a.1 != b.1 || d > 1, "{a:?} {b:?}");
            }
        }
        // a closet with no wall on the pattern still gets one
        let closet = [(6, 5)];
        let one = torch_walls(&closet, |x, y| (x, y) != (6, 5) && (5..=7).contains(&x));
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn what_lies_below_the_ground_leaves_a_hole_in_the_bedrock() {
        let f = Fixture::new();
        let cat = &f.cat;
        assert!(f.look(&feature(cat, "S_dnstair")).sunk);
        assert!(f.look(&feature(cat, "S_pool")).sunk);
        assert!(!f.look(&feature(cat, "S_room")).sunk);
        assert!(!f.look(&feature(cat, "S_upstair")).sunk);
        // the stairs' letters only name them under the pointer
        let up = f.look(&feature(cat, "S_upstair"));
        assert!(!up.letters.is_empty() && up.letters.iter().all(|l| l.hint));
        // a full wall carries a torch, a wall cut down does not
        assert!(f.look(&feature(cat, "S_hwall")).wall);
    }
}
