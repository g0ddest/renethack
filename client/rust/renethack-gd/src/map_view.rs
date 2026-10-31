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
//! An entity that steps to a neighbouring cell (`MapState::take_batch`)
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
    SystemFont, VisualInstance3D, WorldEnvironment,
};
use godot::prelude::*;
use nh_art::{ArtManifest, Tint};
use nh_protocol::{Catalog, Glyph, GlyphKind, MonsterInfo, ObjectTile, mg};
use nh_world::{
    Branch, COLNO, Cell, Ident, ItemUse, MapState, Move, ROWNO, Terrain, UseKind, Who, World,
    cell_terrain, in_field, locate_attack, parse_attack, terrain_of,
};

use crate::animator::{Motion, STEP_SECS, ease, pace, turn, yaw_toward};
use crate::art::{
    Art, CREATURE_LAYER, Finish, Model, ModelLook, Pose, build_flat, no_shadow, set_layer_mask,
};
use crate::audio::Sound;
use crate::batch::{Batches, Slot};
use crate::branch_look::{BranchLook, Prop, Under};
use crate::cues::Size;
use crate::engulf::{Engulfed, Flesh, Inside};
use crate::hearing;
use crate::meshes::{
    MeshKey, bevel, cuboid, cylinder, dome, facets, plane, prism, rock, sphere, torus,
};
use crate::seated::Mount;
use crate::surface::{
    Fow, FowCell, Ground, GroundField, Role, Seen, StandField, Surfaces, cell_index, sight,
};
use crate::theme::{self, nh_color};
use crate::vfx::{Blast, Ray, Vfx, VfxKind};

#[path = "acts.rs"]
mod acts;
#[path = "dressing.rs"]
mod dressing;
#[path = "drop.rs"]
mod drop;
#[path = "fall.rs"]
mod fall;
#[path = "hero_look.rs"]
mod hero_look;
#[path = "hero_pose.rs"]
mod hero_pose;
#[path = "opening.rs"]
mod opening;
#[path = "prologue_stage.rs"]
mod prologue_stage;

pub use acts::{ActKind, Acted, Aim};
pub use fall::{FallKind, Fell};
pub use hero_look::{Effects, Posture, Shown};
pub use hero_pose::{Held, HelplessPhase, STATE_CLIPS};
pub use opening::Opening;
pub use prologue_stage::{PrologueHero, STAIRS as PROLOGUE_STAIRS};

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
/// The whole level framed (the overview): how far past the frame the dark
/// closes in, metres more than the hero's view, and how long its air
/// takes to clear (seconds).
const OVERVIEW_CLEAR: f32 = 45.0;
const OVERVIEW_EASE: f32 = 0.35;
/// The haze thins as the camera moves out (`air_near`): the eye looks
/// through as much of it at any distance as at the default one, never
/// less than this share of it (at 22 m a lit room was a grey veil).
const AIR_LEAST: f32 = 0.3;
/// The exposure the camera gains as the haze thins with its distance,
/// for the haze's own glow the far view lost (at 22 m a lit room read a
/// third darker than at 11 m): 1 + this at twice the default distance,
/// as the haze thins.
const FAR_EXPOSURE: f32 = 0.3;
/// The ambient light the overview adds.
const OVERVIEW_AMBIENT: f32 = 0.3;
/// The vignette's strength over the map (vignette.gdshader's own).
const VIGNETTE: f32 = 0.4;
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
/// Seconds the scene waits for the end of the engine's drawing of the map
/// read in part, at most (the core ends each with a display of the map:
/// one missing, the scene takes what it has).
const UNDRAWN_MOST: f32 = 0.25;
/// Where a feature's lamp burns on its cell.
fn lamp_offset(kind: Lamp) -> Vector3 {
    match kind {
        Lamp::Up => at(0.0, 1.5, -0.3),
        Lamp::Lava => at(0.0, 0.4, 0.0),
        Lamp::Down => at(0.0, -0.05, 0.3),
        Lamp::Holy => at(0.0, 2.4, 0.0),
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
/// Seconds the title scene takes to come up out of black over a title
/// already shown (it is drawn under the start-up veil otherwise).
const TITLE_FADE_SECS: f32 = 0.8;
/// Under the start-up veil the title scene is drawn as fast as the
/// rehearsal; the brightness of its fires (as the branch's torches').
const TITLE_VEILED_BUILD: std::time::Duration = std::time::Duration::from_millis(20);
const TITLE_FIRE_ENERGY: f32 = 1.2;
/// The glow of a fire's coals, its charred logs, and its flames' energy
/// when there are several (the torches' is 1.3).
const EMBER_GLOW: Color = Color::from_rgb(1.0, 0.36, 0.08);
const CHARRED: Color = Color::from_rgb(0.035, 0.024, 0.018);
const MANY_FLAMES_ENERGY: f32 = 0.85;
/// Where the depth fog begins and is whole on the title (metres).
const TITLE_FOG: (f32, f32) = (11.5, 14.5);
/// The creation form's camera: as the title's, closer, lower and swaying
/// less, aiming this high beside the hero (whose figure is some 1.1 m
/// tall); the seconds it takes to come from the title's frame.
const CREATION_DISTANCE: f32 = 3.4;
const CREATION_PITCH_DEG: f32 = 9.0;
const CREATION_SWAY_DEG: f32 = 7.0;
const CREATION_AIM_UP: f32 = 0.66;
const CREATION_FRAME_SECS: f32 = 0.6;
/// The figure that stands for a hero whose role or gender is left to
/// chance (`set_creation_hero`).
const HOODED_FIGURE: &str = "figure_hooded";

/// Seconds from the start of a strike to the blow landing.
const CONTACT_SECS: f32 = 0.16;
/// Descent per step when a pointer ray is walked through the raised geometry.
const PICK_STEP: f32 = 0.05;
/// Nothing drawn reaches higher (a giant on an altar).
const MAX_TOP: f32 = 2.8;
/// No creature stands taller (metres): the line from the hero to the eye
/// is past them all above it.
const TALLEST: f32 = 7.0;
/// Whether the hero is hidden is told by the line from their chest to the
/// eye: this high on a body of a human's height (`HUMAN_TALL`: their
/// role's own), as far up a shorter body (a newt was lost behind a man,
/// the line passing over his head).
const CHEST: f32 = 1.0;
const HUMAN_TALL: f32 = 1.1;

/// Walls stand taller than the hero; the ones in front of open ground are
/// cut down (`CUT_HEIGHT`), so the hero is never behind one.
const WALL_HEIGHT: f32 = 2.1;
/// A door's leaf; a lintel and the cap close the wall above it.
const DOOR_HEIGHT: f32 = 1.8;
const LINTEL: f32 = 0.2;
/// Walls and doors in front of open ground, seen from the camera's side:
/// a kerb (at 0.35 m it read as a dark block the hero stood on).
const CUT_HEIGHT: f32 = 0.16;
/// A wall two cells in front of open ground, behind such a stump: broken
/// off low enough to hide nobody who stands there, however close the
/// camera (a full wall would stand between the eye and their feet).
const HALF_HEIGHT: f32 = 1.1;
/// The dark rock slab on top of a wall.
const CAP_HEIGHT: f32 = 0.1;
/// How high a built place's iron band runs round its walls.
const BAND_Y: f32 = 1.2;
/// A plinth along a wall's foot where open ground is next to it.
const PLINTH_HEIGHT: f32 = 0.22;
/// The rock around the level: its top, and cut down to a lip in front of
/// open ground (`rock_look`: the camera looks north and down, and a block
/// hides what lies behind it and to its sides).
const ROCK_HEIGHT: f32 = 2.05;
const ROCK_HALF: f32 = 1.05;
const ROCK_CUT: f32 = 0.35;
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
/// The faces of the rock mass, a little over the material's own colour:
/// in the hero's light they are what shows of the rock (its tops and the
/// ground beyond lie in the dark).
const SHADE_ROCK: u8 = 120;
/// Stones at the foot of a wall: of the rock, darker still.
const SHADE_RUBBLE: u8 = 55;
/// A wall cut down in front of open ground: lit nearly as the floor (darker
/// still, it read as a dark block).
const SHADE_RUIN: u8 = 85;
/// A lying corpse is this much darker than the living monster.
const CORPSE_DARKEN: f32 = 0.45;

const FLOOR_UNSEEN: Color = Color::from_rgb(0.19, 0.19, 0.21);
const DEEP: Color = Color::from_rgb(0.02, 0.02, 0.03);
/// Where water and lava lie: just under the floor, so that where their
/// shore wanders into a cell of ground they lie there too, shallows over
/// the stone (who swims or wades stands deeper).
const SURFACE_Y: f32 = -0.015;
/// Scratches of an engraving on the floor.
const ENGRAVING: Color = Color::from_rgb(0.05, 0.04, 0.035);
const HERO_RING: Color = Color::from_rgba(1.0, 0.83, 0.54, 0.2);
/// The hero's ring across, and the share of it its circle is. Round a
/// polymorphed hero whose body meets the floor wider than a human's (by
/// its lowest `FORM_FOOTING` of its height) it grows to ring the narrower
/// side `FORM_RING_OUT` out, up to `FORM_RING_MOST` across: a xorn's
/// dome hid it whole.
const HERO_RING_SIZE: f32 = 0.95;
const RING_CIRCLE: f32 = 0.84;
const FORM_FOOTING: f32 = 0.3;
const FORM_RING_OUT: f32 = 0.12;
const FORM_RING_MOST: f32 = 2.2;
/// Cells of water seen on a level of a branch that turns to a swamp
/// (Gehennom: Juiblex's) that make it one; a moat round a tower is fewer.
const SWAMP_WATER: usize = 40;
/// How deep the ring reaches, up and down from where the hero stands; on
/// a cell they stand raised on (the steps of a stair) down to the floor
/// and as far over, so that it lies on every step under them.
const RING_DEPTH: f32 = 0.24;
const RAISED: f32 = 0.15;
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
/// A creature the hero's body hides, its outline seen through it: a
/// hostile's, a pet's (fainter: one often follows behind them).
const XRAY_FOE: Color = Color::from_rgba(1.0, 0.3, 0.22, 0.85);
const XRAY_PET: Color = Color::from_rgba(0.37, 0.84, 0.75, 0.6);
/// The hero's body for what it hides behind it: half its width and its
/// depth, metres (a box round the model would take in a spear held up).
const BODY_HALF_WIDTH: f32 = 0.35;
const BODY_HALF_DEPTH: f32 = 0.15;
/// A creature seen through what hides it, and whether it is hostile.
type Behind = (Gd<Node3D>, bool);

/// How see-through a huge or gigantic creature is drawn while it hides
/// an object or a trap the hero knows of behind it.
const BIG_FADE: f32 = 0.55;
/// A creature behind the hero is seen through them when it is no taller
/// than this share of them (a rat, not a giant whose head shows over
/// theirs, nor a man).
const SMALL: f32 = 0.6;
/// The hero's own dim pool of light, over their head (no lamp: the client
/// does not know of one yet), and a cold rim light from behind that only
/// the hero's model takes (`RIM_LAYER`).
/// A thing thrown is at least this big in flight, metres across its
/// model's unit (a hand's breadth: at 16 cm a flint flew the size of the
/// thrower's head).
const FLIGHT_MIN: f32 = 0.09;
const HERO_LIGHT: Color = Color::from_rgb(0.9, 0.84, 0.76);
const HERO_LIGHT_ENERGY: f32 = 2.4;
const HERO_LIGHT_RANGE: f32 = 5.5;
/// How high the hero's light and their rim light hang; over a polymorphed
/// hero's form, this far over its head when that is higher (a flying
/// dragon's face lit from a hand away came out pink).
const HERO_LIGHT_HEIGHT: f32 = 2.4;
const FORM_LIGHT_OVER: f32 = 0.7;
const RIM_HEIGHT: f32 = 2.0;
const FORM_RIM_OVER: f32 = 0.3;
/// A lamp in hand lights the scene; the light over the hero only fills
/// the shadow the body throws.
const LAMP_POOL: Color = Color::from_rgb(1.0, 0.72, 0.46);
const LAMP_POOL_ENERGY: f32 = 0.8;
const LAMP_POOL_RANGE: f32 = 4.5;
const RIM_LIGHT: Color = Color::from_rgb(0.62, 0.70, 1.0);
const RIM_LAYER: u32 = 1 << 1;
/// The steed the hero rides: two of the hero's own fills light it with
/// them (`STEED_STUDIO`; a dark pony under a knight showed only its legs
/// in a dim room), their rim light and the doll's camera do not.
const STEED_LAYER: u32 = 1 << 5;
/// The hero's own key and fills on the map: which way each shines in the
/// studio's frame, its colour and its energy. They light the hero's model
/// alone (`RIM_LAYER`), whatever the dungeon's light: an outfit reads by
/// them. They were made for the inventory's doll, whose camera never saw
/// them (a camera takes only the lights on a layer it sees): they lit the
/// hero here instead, from wherever that camera last stood. A polymorphed
/// hero is out of their outfit and goes without them: a monster's model
/// is lit as the monsters about it are (under them a red dragon came out
/// pale pink).
const HERO_STUDIO: [(Vector3, Color, f32); 5] = [
    // the key, from the upper left, not too steep: the legs get it too
    (
        Vector3::new(-0.3, -0.55, 0.0),
        Color::from_rgb(1.0, 0.9, 0.78),
        3.4,
    ),
    (
        Vector3::new(-0.2, 0.7, 0.0),
        Color::from_rgb(0.9, 0.8, 0.7),
        1.1,
    ),
    // a warm bounce from below, as off a lit floor
    (
        Vector3::new(0.45, 0.3, 0.0),
        Color::from_rgb(1.0, 0.82, 0.62),
        2.6,
    ),
    // a level fill for the legs and the boots
    (
        Vector3::new(0.05, -0.35, 0.0),
        Color::from_rgb(0.95, 0.88, 0.8),
        1.2,
    ),
    // and a cold one from behind
    (
        Vector3::new(-0.3, 2.6, 0.0),
        Color::from_rgb(0.62, 0.7, 1.0),
        1.6,
    ),
];
/// Every creature seen is lit from behind and above, its back and outline
/// standing out of the floor whatever the light about it (six soldier ants
/// in a lit room were dark specks on the flagstones): the way the light
/// shines, and its energy (its colour the hero's rim's; no shine). With
/// the moon, the back light and the hero's five it is the eighth
/// directional light, the most Godot draws.
const CREATURE_LIGHT_WAY: Vector3 = Vector3::new(-55.0, 180.0, 0.0);
const CREATURE_LIGHT_ENERGY: f32 = 0.6;
/// When a healing's glow rises round the hero after the line telling of
/// it (seconds): after a potion's own glow (`hero::DRUNK_AT`).
const HEALED_AT: f32 = 1.15;
/// Which of `HERO_STUDIO`'s lights reach a ridden steed: the fills (all
/// of them made its brown coat a pale tan).
const STEED_STUDIO: [bool; 5] = [false, true, false, true, false];
/// Where the studio is seen from, for its lights' ways: south of the
/// hero and a little to the east and above, looking at them, as the
/// doll's camera stood when a game began.
const HERO_STUDIO_FROM: Vector3 = Vector3::new(1.78, 0.4, 3.69);
/// The inventory doll's own lights are on this layer: the doll's camera
/// sees it, the map's does not, so they light the hero on the doll alone.
pub const DOLL_LAYER: u32 = 1 << 4;
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
/// How big the sconce's model is drawn, and how much of its depth it
/// keeps on a side wall: the camera sees an east or west wall's torch
/// from its side, level with the head of whoever stands by the wall, so
/// there it keeps to the wall, clear of the middle of the cell by more
/// than half a body.
const SCONCE_SCALE: f32 = 1.2;
const SIDE_REACH: f32 = 0.55;
/// How much the haze in the air glows by itself (the fog's emission, as
/// strong as the fog is dense): the veil over what no light reaches.
const HAZE: f32 = 1.2;
/// A lantern in the torch's place hangs this far out and this high over
/// the torch's mount, and on a side wall smaller and closer by this much.
const LANTERN_OUT: f32 = 0.32;
const LANTERN_HANG: f32 = 0.8;
const SIDE_LANTERN: f32 = 0.55;
const TITLE_TORCH_SHADOWS: usize = 1;
/// Torches made before any level is shown.
const TORCHES_AHEAD: usize = 16;
/// The branches' doors and candles made ahead.
const DOORS_AHEAD: usize = 2;
const CANDLES_AHEAD: usize = 32;
const CANDELABRAS_AHEAD: usize = 8;
/// A flame's flipbook: 16 x 4 frames of a real flame (Unity Labs, CC0).
pub(crate) const FLAME_BOOK: &str = "res://art/cc0/unity-labs/flipbooks/Flame02_16x4.png";
/// The flame of a torch, bright enough to glow.
const FLAME: Color = Color::from_rgb(1.0, 0.45, 0.12);
/// Light spilling down up stairs from the level above, and the cold glow
/// from the depths of down stairs.
const STAIR_UP_LIGHT: Color = Color::from_rgb(1.0, 0.72, 0.46);
const STAIR_DOWN_LIGHT: Color = Color::from_rgb(0.5, 0.6, 0.9);
/// A mark's sheet (`marker`): how far under the cell's plane it lies
/// (metres), and how much more opaque than the decal it is.
const MARK_UNDER: f32 = 0.03;
const MARK_SHEET_ALPHA: f32 = 3.0;
/// How low the deep lies, its water cells and the ground round the level
/// (metres): under the marks' sheets.
const DEEP_BEDROCK: f32 = -0.3;
/// How thick an Astral Plane's floor slab is (metres).
const ASTRAL_SLAB: f32 = 0.35;
/// The light over an altar of the Astral Plane, falling on it from its
/// god (`Lamp::Holy`): colour, energy and reach.
const HOLY: (Color, f32, f32) = (Color::from_rgb(1.0, 0.88, 0.62), 3.2, 4.5);
/// Its shaft of light: how tall (it fades out in its lower half) and how
/// strong.
const SHAFT_TALL: f32 = 7.0;
const SHAFT_STRENGTH: f32 = 0.45;
/// The Planes (`Under`): how tall a bubble's skin stands on the Plane of
/// Water, the puffs a cloud cell of the Plane of Air is, and how far below
/// the level the sea of clouds lies (metres).
const MEMBRANE_TALL: f32 = 0.8;
/// A bubble's skin, from the floor up: how far each segment of it leans
/// in over the bubble (degrees) and how long it is (metres), a meniscus
/// curving in to about `MEMBRANE_TALL` high (a flat leaning wall of glass
/// made the bubble a square box).
const MENISCUS: [(f32, f32); 3] = [(12.0, 0.4), (38.0, 0.35), (66.0, 0.32)];
const CLOUD_PUFFS: u32 = 2;
/// How far behind whoever stands in a cloud its puffs are drawn (metres).
const CLOUD_BEHIND: f32 = 0.7;
const SKY_DROP: f32 = 30.0;
/// How wide the sea of clouds is (metres): past the overview's frame.
const SKY_SEA: f32 = 1400.0;
/// Gehennom's chasm: the glow of its smouldering depths where the ground
/// under the level lies far from anything known (map_surface.gdshader's
/// `branch_chasm`; what is not known yet reads 6-12 of 255, never black),
/// and in alpha how much of its own light that ground shows near what is
/// known (a maze's remembered floor, which NetHack leaves unmapped).
const CHASM: Color = Color::from_rgba(0.019, 0.0048, 0.0024, 0.6);
/// The light from above on the Planes of Air and Water (the moon's, the
/// level's own elsewhere): its way, colour and energy.
const SUN: (Vector3, Color, f32) = (
    Vector3::new(-55.0, 35.0, 0.0),
    Color::from_rgb(1.0, 0.96, 0.9),
    0.9,
);
const DEEP_LIGHT: (Vector3, Color, f32) = (
    Vector3::new(-80.0, 10.0, 0.0),
    Color::from_rgb(0.45, 0.75, 0.9),
    0.45,
);
/// Among the stars (the Astral Plane): a cold clear light from above.
const STAR_LIGHT: (Vector3, Color, f32) = (
    Vector3::new(-60.0, 20.0, 0.0),
    Color::from_rgb(0.78, 0.76, 1.0),
    0.55,
);
const MOON: (Vector3, Color, f32) = (
    Vector3::new(-62.0, 25.0, 0.0),
    Color::from_rgb(0.55, 0.62, 0.9),
    0.14,
);
/// Over a chasm (Gehennom) the light from above is the fire's, faint and
/// red, not the moon's.
const EMBER_LIGHT: (Vector3, Color, f32) = (
    Vector3::new(-62.0, 25.0, 0.0),
    Color::from_rgb(1.0, 0.55, 0.36),
    0.16,
);
/// The walls and doors a door is set in or joined to through walls, of
/// the level `t` tells the terrain of (`MapView::refresh_built`); a run of
/// them more than `most` long is the cave's own, a building against it.
fn built_walls(t: impl Fn(i32, i32) -> Option<Terrain>, most: usize) -> HashSet<(i32, i32)> {
    let door = |t: Option<Terrain>| {
        matches!(
            t,
            Some(Terrain::ClosedDoor | Terrain::OpenDoor | Terrain::Doorway | Terrain::BrokenDoor)
        )
    };
    let mut built = HashSet::new();
    let mut seen = HashSet::new();
    for (x, y) in (0..ROWNO).flat_map(|y| (0..COLNO).map(move |x| (x, y))) {
        if !door(t(x, y)) || seen.contains(&(x, y)) {
            continue;
        }
        let mut run = vec![(x, y)];
        seen.insert((x, y));
        let mut i = 0;
        while i < run.len() && run.len() <= most {
            let (cx, cy) = run[i];
            for n in [(cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)] {
                let tn = t(n.0, n.1);
                if (tn == Some(Terrain::Wall) || door(tn)) && seen.insert(n) {
                    run.push(n);
                }
            }
            i += 1;
        }
        if run.len() <= most {
            built.extend(run);
        }
    }
    built
}

/// The most walls and doors joined up that are still a building in a cave
/// branch (Minetown's temple is the biggest); more are the cave's own.
const BUILT_MOST: usize = 90;
/// Lava's light (`place_lava_lights`): its colour, energy and reach (3 to
/// 4 cells), one every so many cells over a big pool, and how many at
/// most on a level.
const LAVA_LIGHT: Color = Color::from_rgb(1.0, 0.42, 0.12);
const LAVA_ENERGY: f32 = 1.5;
const LAVA_RANGE: f32 = 3.5;
const LAVA_SPACING: f32 = 3.5;
const LAVA_LIGHTS: usize = 40;
/// The faint fill from the other side, so no wall face is flat black: a
/// cold one from above; over a chasm a warm one, low from the south, as if
/// from the fire below: the faces turned to the camera read by it.
const BACK: (Vector3, Color, f32) = (
    Vector3::new(-40.0, 205.0, 0.0),
    Color::from_rgb(0.5, 0.56, 0.8),
    0.07,
);
const EMBER_BACK: (Vector3, Color, f32) = (
    Vector3::new(6.0, 0.0, 0.0),
    Color::from_rgb(1.0, 0.5, 0.3),
    0.95,
);
/// Depth fog and the background: never pure black.
const DARKNESS: Color = Color::from_rgb(0.008, 0.009, 0.013);

/// What covers a solid: a manifest material (index, brightness %, how the
/// surface shader treats it) or a plain colour.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Paint {
    Pbr(usize, u8, Role),
    Flat(Color, Finish),
    /// A shader of the map's own: water, lava, the air over lava, the air
    /// where something unseen is.
    Liquid(Liquid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Liquid {
    Water,
    Lava,
    Haze,
    /// The air bent where something unseen is remembered.
    Unseen,
    // the Planes' (`Under`):
    /// An air cell of a bubble in the Plane of Water: its floor.
    Bubble,
    /// A soft mass of cloud on a cloud cell.
    Cloud,
    /// The deep round a bubble: its water cells and what lies under.
    Deep,
    /// The bubble's skin where its air meets the water.
    Membrane,
}

impl Liquid {
    pub const ALL: [Liquid; 8] = [
        Liquid::Water,
        Liquid::Lava,
        Liquid::Haze,
        Liquid::Unseen,
        Liquid::Bubble,
        Liquid::Cloud,
        Liquid::Deep,
        Liquid::Membrane,
    ];
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
    /// An Astral Plane's altar lit from above by its god, a shaft of light
    /// in a thin mist.
    Holy,
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
    /// The ground the cell is (the ground's shaders blend it).
    kind: Ground,
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
    /// The entity is the hero (not what an unseen hero stands on), or the
    /// steed under them.
    hero: bool,
    /// The entity is a steed under the hero: how the hero looks on it.
    rider: Option<ModelLook>,
    /// The entity wears a saddle.
    saddled: bool,
    /// The monster the entity is (the catalog's), and whether it is only
    /// sensed, not seen (telepathy).
    mon: Option<i32>,
    sensed: bool,
    /// What the monster seen holds in its hand, as the hero's held art.
    wields: Option<nh_art::HeldArt>,
    /// A wall at full height (a torch can hang on it).
    wall: bool,
    /// Something here lies below the ground (the bedrock under the level
    /// leaves it a hole).
    sunk: bool,
    /// The trap here opens in the floor (`opening`): cut away to a shaft
    /// with nothing standing on it.
    opening: Option<Opening>,
    /// A monster that is not the hero's nor a pet (red ring under the pointer).
    hostile: bool,
    /// The rock the level is cut into.
    rock: bool,
    /// What the fog of war makes of the cell.
    seen: Seen,
    /// A ring on the ground (a pet's).
    ring: Option<Color>,
    lamp: Option<Lamp>,
    /// An explosion shows here: (what of, the glyph's colour, whether this
    /// is its middle).
    blast: Option<(Blast, Color, bool)>,
    /// A ray crosses here: (what of, the glyph's colour, its yaw if it
    /// shows its way, height).
    ray: Option<(Ray, Color, Option<f32>, f32)>,
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

    /// A thin see-through sheet (a bubble's skin, a puff of cloud): no
    /// shadow of its own.
    fn sheet(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3, rot: Vector3) {
        self.add(mesh, paint, pos, rot);
        if let Some(s) = self.solids.last_mut() {
            s.shadow = false;
        }
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
        // a body over more than its cell takes the pointer by its cell's
        // ground, not by its height: the cells behind it and beside it
        // under its wings keep theirs
        if !r.overhangs {
            self.reach(top);
        }
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

/// The monsters met most on the first levels, and the pets, with one of
/// each model met later (a spider, a snake, a bee, a dragon): the
/// rehearsal draws each model once.
pub(crate) const WARM_MONSTERS: &[&str] = &[
    "kitten",
    "little dog",
    "pony",
    "newt",
    "jackal",
    "sewer rat",
    "grid bug",
    "fox",
    "cave spider",
    "kobold",
    "garter snake",
    "goblin",
    "lichen",
    "gecko",
    "killer bee",
    "yellow mold",
    "acid blob",
    "floating eye",
    "gnome",
    "gnome leader",
    "hobbit",
    "baby red dragon",
    "gnome zombie",
    "homunculus",
    "giant bat",
    "hill orc",
    "dwarf",
    "shopkeeper",
];

/// The monsters drawn with shaders of their own (a vortex's wind, a fire
/// elemental's flames, a trapper's hide): the rehearsal draws one of each,
/// so the first met in a game compiles nothing.
pub(crate) const SHADED_MONSTERS: &[&str] = &["dust vortex", "fire elemental", "trapper"];

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

/// How much light a thing on the floor gives, by what it is (its
/// appearance): none for a boulder, a rock or a stone; the full pool and
/// glint for what a player stoops for (wands, rings, amulets, gems,
/// potions, scrolls, books, gold); a faint pool for the rest, the heavy
/// things.
fn sheen_of(tile: &ObjectTile) -> crate::art::Sheen {
    use crate::art::Sheen;
    match tile.class.as_str() {
        "`" => Sheen::None,
        "*" if matches!(tile.appearance.as_str(), "rock" | "gray") => Sheen::None,
        "/" | "=" | "\"" | "*" | "!" | "?" | "+" | "$" => Sheen::Full,
        _ => Sheen::Faint,
    }
}

/// What undoes a branch's grade on a thing's colours, about: the inverse
/// of its curve's middle (between its shadows and its highlights), its
/// strongest channel at 1 (a thing on the floor keeps its own colour).
fn ungraded(g: &crate::branch_look::Grade) -> Color {
    let mid = |s: f32, h: f32| 1.0 / ((s + h) / 2.0).max(0.1);
    let (r, gr, b) = (
        mid(g.shadows.r, g.highlights.r),
        mid(g.shadows.g, g.highlights.g),
        mid(g.shadows.b, g.highlights.b),
    );
    let most = r.max(gr).max(b);
    Color::from_rgb(r / most, gr / most, b / most)
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
    /// The hero stands down in a pit (spiked: whether they saw its spikes).
    hero_pit: Option<bool>,
    /// How the branch the level is in looks.
    branch: &'a BranchLook,
    /// What the level's plan gives this cell (`dressing::plan`).
    volume: Option<dressing::Volume>,
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
    /// The ground of each (`Ground::None`: a wall, the rock, unknown).
    ground: [Ground; 4],
    /// A door or a feature (`may_matter`) here or round here, diagonals
    /// too.
    matters: bool,
    /// Open ground north of here (a doorway too), and two cells north (in
    /// front of the cell north of here).
    north_open: bool,
    far_open: bool,
    /// Open ground north and to a side, and two cells north and to a
    /// side: what a block here hides from a camera not straight south of
    /// it.
    aside_open: bool,
    far_aside_open: bool,
    /// The wall west or east of here stands cut down in front of open
    /// ground (a corner between it and a wall that stands joins it).
    beside_cut: bool,
    /// The corner south of here is cut down so.
    south_cut: bool,
}

/// How much of a wall stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stand {
    Full,
    /// Broken off half way up (`HALF_HEIGHT`).
    Half,
    /// A stump (`CUT_HEIGHT`).
    Cut,
}

impl Around {
    fn of(near: &Near, catalog: &Catalog) -> Around {
        let side_of = |cell: Option<&Cell>| match cell.and_then(|c| cell_terrain(c, catalog)) {
            Some(Terrain::Wall | Terrain::LavaWall) => Side::Wall,
            Some(Terrain::ClosedDoor | Terrain::OpenDoor | Terrain::Doorway) => Side::Wall,
            _ if is_open(cell, catalog) => Side::Open,
            _ => Side::Solid,
        };
        let mut sides = [Side::Solid; 4];
        let mut liquid = [false; 4];
        let mut floor = [false; 4];
        let mut ground = [Ground::None; 4];
        for (i, side) in sides.iter_mut().enumerate() {
            let cell = near.cells[i];
            let t = cell.and_then(|c| cell_terrain(c, catalog));
            liquid[i] = matches!(t, Some(Terrain::Pool | Terrain::Water | Terrain::Lava));
            floor[i] = matches!(t, Some(Terrain::Floor | Terrain::DarkFloor));
            ground[i] = cell.map_or(Ground::None, |c| cell_ground(c, catalog));
            *side = side_of(cell);
        }
        let open = |cell: Option<&Cell>| is_open(cell, catalog);
        let wall = |cell: Option<&Cell>| side_of(cell) == Side::Wall;
        let [n, s, w, e, nw, ne, sw, se] = near.cells;
        let south_pier = s
            .and_then(|c| c.terrain.as_ref())
            .and_then(|g| cmap_sym(g, catalog))
            .is_some_and(is_pier);
        Around {
            sides,
            liquid,
            floor,
            ground,
            matters: near
                .cells
                .iter()
                .any(|c| c.is_some_and(|c| may_matter(c, catalog))),
            north_open: open(n),
            far_open: open(near.far_north),
            aside_open: open(nw) || open(ne),
            far_aside_open: open(near.beyond[0]) || open(near.beyond[1]),
            beside_cut: !open(n) && ((wall(w) && open(nw)) || (wall(e) && open(ne))),
            south_cut: wall(s) && south_pier && ((wall(sw) && open(w)) || (wall(se) && open(e))),
        }
    }

    /// How much of a wall here stands, so that it hides nobody. Built
    /// walls: a stump in front of open ground (the camera looks north),
    /// a corner with the stumps it joins, and the wall behind a stump
    /// (open ground two cells north, or a corner cut down south of it)
    /// half its height, or a stump too between two stumps: a room's side
    /// walls step down towards the camera. A cave's rock has no sides to
    /// keep: a lip wherever open ground lies north of it or north and to
    /// a side, and half its height a row behind that.
    fn wall_stand(&self, pier: bool, cave: bool) -> Stand {
        let north_open = self.north_open;
        if cave {
            return if north_open || self.aside_open {
                Stand::Cut
            } else if self.far_open || self.far_aside_open {
                Stand::Half
            } else {
                Stand::Full
            };
        }
        if north_open || (pier && self.beside_cut) || (self.far_open && self.south_cut) {
            Stand::Cut
        } else if self.far_open || self.south_cut {
            Stand::Half
        } else {
            Stand::Full
        }
    }

    /// No door nor feature beside it, nor at its corners: a room's
    /// dressing may be here.
    fn plain(&self) -> bool {
        !self.matters
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

/// A terrain that matters in play, which a room's dressing keeps off and
/// away from: a door or a doorway, stairs, a ladder, an altar, a trap and
/// the like (not the floor, a wall, the rock, nor water, ice or lava).
fn matters(t: Terrain) -> bool {
    !matches!(
        t,
        Terrain::Stone
            | Terrain::Wall
            | Terrain::Floor
            | Terrain::DarkFloor
            | Terrain::Corridor
            | Terrain::Pool
            | Terrain::Water
            | Terrain::Ice
            | Terrain::Lava
            | Terrain::LavaWall
            | Terrain::Air
            | Terrain::Cloud
            | Terrain::Effect
            | Terrain::Unknown
    )
}

/// What the level's plan (`dressing::plan`) reads of the cell at (x, y).
fn plan_cell(map: &MapState, catalog: &Catalog, x: i32, y: i32) -> dressing::PlanCell {
    map.cell(x, y).map_or_else(Default::default, |c| {
        let terrain = cell_terrain(c, catalog);
        dressing::PlanCell {
            terrain,
            held: terrain.is_none() && c.entity().is_some(),
            matters: may_matter(c, catalog),
        }
    })
}

/// A cell that shows a terrain that `matters`, or may: a thing lying on
/// ground never shown bare (the engine tells no stairs nor altar under it:
/// an altar under an offering), or the hero on one (what they remember
/// under them is the thing). Another monster is taken to stand on floor
/// (it moves on).
fn may_matter(cell: &Cell, catalog: &Catalog) -> bool {
    match cell_terrain(cell, catalog) {
        Some(t) => matters(t),
        None => {
            let hides = |g: &Glyph| match g.kind {
                GlyphKind::Mon => g.flags & mg::HERO != 0,
                GlyphKind::Obj | GlyphKind::Body | GlyphKind::Statue => true,
                _ => false,
            };
            cell.rider.is_some() || cell.glyph.as_ref().is_some_and(hides)
        }
    }
}

/// Floor, walls and features. Sets `ground`. `cut`: open ground lies north
/// of the cell (the row behind a room's south wall, a doorway above a side
/// wall), so a wall or door here would hide it from the camera and is
/// drawn low.
fn terrain_look(look: &mut Look, t: Terrain, sym: &str, g: &Glyph, near: &Near, ctx: &Ctx) {
    let around = Around::of(near, ctx.catalog);
    let cut = is_open(near.cells[0], ctx.catalog);
    terrain_base(look, t, sym, g, cut, &around, ctx);
    look.kind = ground_of(t);
    ground_layers(look, ground_of(t), near, ctx);
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

/// The ground a terrain is, as the ground's shaders blend it with the
/// cells beside it: a wall, the rock, stairs or a bridge are none.
fn ground_of(t: Terrain) -> Ground {
    match t {
        Terrain::Pool | Terrain::Water => Ground::Water,
        Terrain::Lava => Ground::Lava,
        Terrain::Ice => Ground::Ice,
        Terrain::Floor
        | Terrain::DarkFloor
        | Terrain::Corridor
        | Terrain::Doorway
        | Terrain::BrokenDoor
        | Terrain::OpenDoor
        | Terrain::Tree
        | Terrain::Altar
        | Terrain::Grave
        | Terrain::Throne
        | Terrain::Sink
        | Terrain::Fountain
        | Terrain::Trap => Ground::Land,
        _ => Ground::None,
    }
}

/// The ground a cell is: its terrain's, or the floor's under whoever
/// stands where the engine sent no ground (it is drawn as the floor beside
/// it).
fn cell_ground(cell: &Cell, catalog: &Catalog) -> Ground {
    match cell_terrain(cell, catalog) {
        Some(t) => ground_of(t),
        None if cell.entity().is_some() => Ground::Land,
        None => Ground::None,
    }
}

/// Where another ground lies beside this cell's own (`own`), its surface
/// under or over this one's: each shows only where its kind wins over the
/// cells' beside it (`ground_tex`), so the line between them wanders
/// through both cells instead of running along their edge. Land is the
/// branch's floor; water and lava their surfaces just under it (shallows
/// over the stone, `SURFACE_Y`); ice its sheet.
fn ground_layers(look: &mut Look, own: Ground, near: &Near, ctx: &Ctx) {
    if own == Ground::None {
        return;
    }
    let mut near_kinds = [false; 5];
    for cell in near.cells.iter().flatten() {
        near_kinds[cell_ground(cell, ctx.catalog) as usize] = true;
    }
    for g in [Ground::Land, Ground::Water, Ground::Lava, Ground::Ice] {
        if g == own || !near_kinds[g as usize] {
            continue;
        }
        let (paint, y) = match g {
            Ground::Land => match ctx.mat("floor") {
                Some(m) => (Paint::Pbr(m, SHADE_LIT, Role::Floor), 0.0),
                None => continue,
            },
            Ground::Water => (Paint::Liquid(Liquid::Water), SURFACE_Y),
            Ground::Lava => (Paint::Liquid(Liquid::Lava), SURFACE_Y),
            Ground::Ice => match ctx.remap(ctx.art.terrain(Terrain::Ice).material) {
                Some(m) => (Paint::Pbr(m, SHADE_LIT, Role::Ice), 0.0),
                None => continue,
            },
            Ground::None => continue,
        };
        look.ground_tile(plane(1.0, 1.0), paint, at(0.0, y, 0.0));
    }
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
/// cell (the same every time): small, sunk in the floor to their middle
/// and of the rock, never like something lying there to pick up (a stone
/// standing on the floor reads as a ball). None where the branch's floors
/// are swept.
fn rubble(
    look: &mut Look,
    around: &Around,
    rock: Option<usize>,
    ctx: &Ctx,
    edge: Side,
    chance: f32,
) {
    let Some(m) = rock.filter(|_| ctx.branch.rubble) else {
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
            look.turned(facets(r, 5), paint, at(x, -r * 0.4, z), rot);
            if let Some(s) = look.solids.last_mut() {
                s.shadow = false;
            }
        }
    }
    // the stones never make the cell taller to the pointer
    look.top = look.top.min(0.2);
}

/// Timber holds a mine's rock up on this side (`SIDES`) of a wall cell
/// beside open ground, as the cell has it (the camera does not see a
/// south wall's face, its north side).
fn propped(ctx: &Ctx, i: usize) -> bool {
    ctx.branch.supports && i != 0 && ctx.noise(71 + i as u32) <= 0.28
}

/// Timber holding up a mine's rock: on some walls beside open ground, two
/// posts against the face and a beam across, placed by the cell.
fn mine_support(look: &mut Look, around: &Around, ctx: &Ctx) {
    let Some(wood) = ctx.mat("wood") else {
        return;
    };
    let wood = Paint::Pbr(wood, 75, Role::Prop);
    for (i, (dx, dz)) in SIDES.iter().enumerate() {
        if around.sides[i] != Side::Open || !propped(ctx, i) {
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
/// stone on each side where a wall or the rock stands (where ground lies
/// the shore wanders over both cells: `ground_layers`).
fn liquid_bed(look: &mut Look, around: &Around, ctx: &Ctx, depth: f32) {
    let Some(m) = ctx.mat("bedrock") else {
        return;
    };
    // dark and wet under the water: a bed, not a pale tank
    let stone = Paint::Pbr(m, 45, Role::Prop);
    look.ground_tile(plane(1.0, 1.0), stone, at(0.0, -depth, 0.0));
    for (i, (dx, dz)) in SIDES.iter().enumerate() {
        if around.liquid[i] || around.ground[i] != Ground::None {
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
            let corner = is_pier(sym);
            let stand = around.wall_stand(corner, ctx.branch.cave);
            // (a cave's rock has no piers)
            let pier = corner && !ctx.branch.cave;
            let (cut, half) = (stand == Stand::Cut, stand == Stand::Half);
            let wall_h = match stand {
                // (a cave's is the rock's lip)
                Stand::Cut if ctx.branch.cave => ROCK_CUT,
                Stand::Cut => CUT_HEIGHT,
                Stand::Half => HALF_HEIGHT,
                Stand::Full => WALL_HEIGHT,
            };
            let low = cut || half;
            let (w, top) = if pier {
                (1.08, wall_h + if low { 0.06 } else { 0.18 })
            } else {
                (1.0, wall_h)
            };
            if ctx.branch.cave {
                // a cave's wall is the rock itself, in lumps and uneven
                let rock_paint = pbr(art.material, SHADE_LIT, Role::Cave, FLOOR_UNSEEN);
                let h = if cut { ROCK_CUT } else { wall_h };
                look.solid(rock(1.0, h, 1.0), rock_paint, Vector3::ZERO);
                if let Some(s) = look.solids.last_mut() {
                    s.shadow = !cut;
                }
                if ctx.branch.supports && !low {
                    mine_support(look, around, ctx);
                }
                if !low {
                    dressing::cave(look, around, ctx, h, rock_paint);
                }
            } else if low {
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
            look.wall = !low;
            look.masonry = true;
            // candles on some walls' tops beside a room (no object ever
            // lies there)
            let beside_room = around.sides.contains(&Side::Open);
            let lights = ctx.noise(91);
            if ctx.branch.candles && !low && !ctx.branch.cave && beside_room && lights < 0.14 {
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
            } else if ctx.branch.candles && !low && !ctx.branch.cave && beside_room && lights < 0.3
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
            // (the night cold through them)
            let facing = !low && !pier && !ctx.branch.cave && around.sides[1] == Side::Open;
            let decor = ctx.noise(97);
            let window = facing && ctx.branch.windows && decor < 0.25;
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
            // what the branch dresses its rooms' walls with
            let dressed =
                !low && !ctx.branch.cave && dressing::wall(look, around, sym, ctx, top, window);
            // a built place's iron bands round the faces beside a room
            if let (Some(band), false, false) = (ctx.branch.bands, low, ctx.branch.cave) {
                let iron = pbr(ctx.mat(band), SHADE_LIT, Role::Trim, DEEP);
                let out = w / 2.0 + 0.02;
                for (i, (dx, dz)) in SIDES.iter().enumerate() {
                    if around.sides[i] != Side::Open || (i == 1 && (window || dressed)) {
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
            if ctx.branch.under == Under::Stars {
                astral_floor(look, around, ctx);
            } else {
                rubble(look, around, bedrock, ctx, Side::Wall, 0.3);
            }
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
            if let Some(m) = bedrock.filter(|_| ctx.branch.rubble) {
                for k in 0..5u32 {
                    let r = 0.025 + 0.02 * ctx.noise(70 + k);
                    let (x, z) = (ctx.noise(75 + k) - 0.5, ctx.noise(80 + k) - 0.5);
                    let rot = at(ctx.noise(85 + k) * 90.0, ctx.noise(90 + k) * 360.0, 0.0);
                    let paint = Paint::Pbr(m, SHADE_RUBBLE, Role::Trim);
                    look.turned(facets(r, 5), paint, at(x * 0.8, -r * 0.3, z * 0.8), rot);
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
            // the arch of the walls' own masonry (of black rock it stood in
            // a room as a dark monolith); it thins out between the eye and
            // the hero as a wall does
            let masonry = ctx.mat("masonry");
            let arch = pbr(masonry, SHADE_LIT, Role::Wall, DEEP);
            for x in [-0.42f32, 0.42] {
                look.solid(bevel(0.16, 1.9, 0.3), arch, at(x, 0.95, -0.34));
            }
            look.solid(bevel(1.0, 0.26, 0.34), arch, at(0.0, 2.0, -0.34));
            // the dark beyond the top step: the masonry all but black, so
            // that it thins out between the eye and whoever stands north
            // of the stair as the arch does (flat black, it hid all of a
            // hero there but the helmet)
            look.solid(
                cuboid(0.7, 1.2, 0.04),
                pbr(masonry, 4, Role::Wall, DEEP),
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
            // dressed stone on a step, carved panels on its face, a
            // cornice under an overhanging top
            look.solid(bevel(0.98, 0.1, 0.8), main(70), at(0.0, 0.05, 0.0));
            look.solid(bevel(0.78, 0.38, 0.56), main(90), at(0.0, 0.29, 0.0));
            for x in [-0.19f32, 0.19] {
                look.solid(bevel(0.28, 0.24, 0.02), main(62), at(x, 0.29, 0.285));
            }
            look.solid(
                bevel(0.86, 0.05, 0.64),
                main(SHADE_LIT),
                at(0.0, 0.505, 0.0),
            );
            look.solid(
                bevel(0.94, 0.07, 0.72),
                main(SHADE_LIT),
                at(0.0, 0.565, 0.0),
            );
            // the altar's alignment colour as a cloth over its front
            let cloth = Paint::Flat(darker(nh_color(c), 0.35), Finish::Matte);
            look.solid(cuboid(0.34, 0.012, 0.74), cloth, at(0.0, 0.606, 0.0));
            look.solid(cuboid(0.34, 0.3, 0.012), cloth, at(0.0, 0.46, 0.367));
            // a brass candelabra of three burning at its back
            look.props.push(PlacedProp {
                prop: Prop::Candelabra,
                pos: at(0.0, 0.6, -0.22),
                yaw: 0.0,
                scale: Vector3::new(1.0, 1.0, 1.0),
            });
            // among the stars, lit from above by its god
            if ctx.branch.under == Under::Stars {
                look.lamp = Some(Lamp::Holy);
            }
            let flame = Paint::Flat(Color::from_rgb(1.0, 0.7, 0.35), Finish::Glow);
            for x in [-0.112f32, 0.0, 0.112] {
                look.solid(sphere(0.012), flame, at(x, 0.6 + 0.41, -0.22));
                if let Some(s) = look.solids.last_mut() {
                    s.shadow = false;
                }
            }
            look.ground = 0.61;
        }
        Terrain::Throne => {
            floor(look);
            // a gilded chair of state on a dais of two steps: a tall back
            // in velvet between posts with gold balls, a crest over it,
            // arms on turned supports, a cushion (it was a box chair)
            let gold = main(SHADE_LIT);
            let step = trim(SHADE_LIT);
            look.solid(bevel(0.94, 0.08, 0.9), step, at(0.0, 0.04, 0.0));
            look.solid(bevel(0.76, 0.08, 0.72), step, at(0.0, 0.12, -0.04));
            look.solid(bevel(0.6, 0.16, 0.5), gold, at(0.0, 0.24, -0.04));
            let velvet = Paint::Flat(Color::from_rgb(0.36, 0.04, 0.07), Finish::Matte);
            look.solid(bevel(0.5, 0.07, 0.44), velvet, at(0.0, 0.35, -0.02));
            for x in [-0.27f32, 0.27] {
                look.solid(bevel(0.09, 1.08, 0.09), gold, at(x, 0.86, -0.26));
                look.solid(sphere(0.055), gold, at(x, 1.44, -0.26));
                // the arm on a turned support, a ball at its end
                look.solid(bevel(0.08, 0.06, 0.46), gold, at(x * 1.04, 0.56, -0.04));
                look.solid(cylinder(0.025, 0.035, 0.22), gold, at(x * 1.04, 0.42, 0.14));
                look.solid(sphere(0.04), gold, at(x * 1.04, 0.6, 0.2));
            }
            look.solid(bevel(0.46, 0.84, 0.05), velvet, at(0.0, 0.8, -0.25));
            look.solid(bevel(0.64, 0.09, 0.12), gold, at(0.0, 1.27, -0.26));
            look.solid(prism(0.34, 0.2, 0.07), gold, at(0.0, 1.41, -0.26));
            look.solid(sphere(0.045), gold, at(0.0, 1.53, -0.26));
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
            // water, rippling and glinting in the torches' light (it was
            // a still black disc: a dry well)
            look.ground_tile(
                cylinder(0.37, 0.37, 0.01),
                Paint::Liquid(Liquid::Water),
                at(0.0, 0.2, 0.0),
            );
            look.solid(bevel(0.16, 0.36, 0.16), stone, at(0.0, 0.2, 0.0));
            look.solid(cylinder(0.12, 0.05, 0.06), stone, at(0.0, 0.41, 0.0));
            // and a thin jet rising from the spout, falling back in drops
            let jet = Paint::Flat(Color::from_rgb(0.7, 0.85, 0.9), Finish::Ghost);
            look.ground_tile(cylinder(0.008, 0.016, 0.2), jet, at(0.0, 0.54, 0.0));
            for (i, a) in [20.0f32, 140.0, 260.0].into_iter().enumerate() {
                let (s, c) = a.to_radians().sin_cos();
                let r = 0.1 + 0.05 * i as f32;
                look.ground_tile(sphere(0.012), jet, at(r * s, 0.3 + 0.05 * i as f32, r * c));
            }
            look.top = look.top.min(0.5);
            look.ground = 0.26;
        }
        Terrain::Sink => {
            floor(look);
            // a stone basin on a pedestal, water standing in it, a metal
            // spout and its taps behind (it was a crate with a hole)
            let stone = main(SHADE_LIT);
            look.solid(cylinder(0.2, 0.25, 0.08), stone, at(0.0, 0.04, 0.0));
            look.solid(cylinder(0.11, 0.15, 0.52), stone, at(0.0, 0.34, 0.0));
            look.solid(cylinder(0.34, 0.18, 0.18), stone, at(0.0, 0.69, 0.0));
            look.solid(torus(0.3, 0.37), stone, at(0.0, 0.78, 0.0));
            // dark water over the basin's closed top, under the rim's
            // crown (the water's own surface is clear: over stone it
            // showed the stone)
            look.solid(
                cylinder(0.31, 0.31, 0.01),
                Paint::Flat(DEEP, Finish::Glossy),
                at(0.0, 0.785, 0.0),
            );
            look.ground_tile(
                cylinder(0.31, 0.31, 0.01),
                Paint::Liquid(Liquid::Water),
                at(0.0, 0.795, 0.0),
            );
            let metal = pbr(ctx.mat("metal"), SHADE_LIT, Role::Prop, DEEP);
            look.solid(cylinder(0.022, 0.03, 0.26), metal, at(0.0, 0.9, -0.32));
            look.turned(
                cylinder(0.018, 0.018, 0.16),
                metal,
                at(0.0, 1.02, -0.25),
                at(90.0, 0.0, 0.0),
            );
            for x in [-0.09f32, 0.09] {
                look.solid(sphere(0.03), metal, at(x, 0.88, -0.32));
            }
            look.ground = 0.78;
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
        Terrain::Water if ctx.branch.under == Under::Deep => {
            // the deep, no bed under it, as low as the ground round the
            // level (a ring's sheet over a swimmer shows over it)
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Liquid(Liquid::Deep),
                at(0.0, DEEP_BEDROCK, 0.0),
            );
            look.ground = -0.1;
            look.sunk = true;
        }
        Terrain::Pool | Terrain::Water => {
            // (who swims or wades stands deeper than its surface)
            let deep = if t == Terrain::Water { 0.1 } else { 0.05 };
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Liquid(Liquid::Water),
                at(0.0, SURFACE_Y, 0.0),
            );
            liquid_bed(
                look,
                around,
                ctx,
                if t == Terrain::Water { 0.9 } else { 0.5 },
            );
            look.ground = -deep;
            look.sunk = true;
        }
        Terrain::Ice => {
            let ice = pbr(art.material, SHADE_LIT, Role::Ice, FLOOR_UNSEEN);
            look.ground_tile(tile, ice, Vector3::ZERO);
        }
        Terrain::Lava => {
            look.ground_tile(
                plane(1.0, 1.0),
                Paint::Liquid(Liquid::Lava),
                at(0.0, SURFACE_Y, 0.0),
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
        Terrain::Air => match ctx.branch.under {
            // nothing: open air over the sea of clouds (or the stars) far
            // below
            Under::Sky | Under::Stars => {}
            Under::Deep => {
                look.ground_tile(
                    plane(1.0, 1.0),
                    Paint::Liquid(Liquid::Bubble),
                    Vector3::ZERO,
                );
                // its skin on each side that meets the water: a meniscus
                // rising from the cell's edge and curving in over the
                // bubble as a sphere's would (`MENISCUS`, segment by
                // segment), its face outwards (the shader makes the side
                // facing the eye, in front of the bubble, fainter)
                let sides = [
                    (Vector3::new(0.0, 0.0, -1.0), 180.0),
                    (Vector3::new(0.0, 0.0, 1.0), 0.0),
                    (Vector3::new(-1.0, 0.0, 0.0), 270.0),
                    (Vector3::new(1.0, 0.0, 0.0), 90.0),
                ];
                for ((out, yaw), water) in sides.into_iter().zip(around.liquid) {
                    if !water {
                        continue;
                    }
                    let mut base = out * 0.5;
                    for (lean, len) in MENISCUS {
                        let (s, c) = lean.to_radians().sin_cos();
                        let way = -out * s + Vector3::UP * c;
                        look.sheet(
                            plane(1.0, len),
                            Paint::Liquid(Liquid::Membrane),
                            base + way * (len / 2.0),
                            at(90.0 - lean, yaw, 0.0),
                        );
                        base += way * len;
                    }
                }
            }
            Under::Rock | Under::Chasm => {
                let air = lighter(nh_color(c), 0.4);
                look.ground_tile(
                    plane(1.0, 1.0),
                    Paint::Flat(air, Finish::Ghost),
                    Vector3::ZERO,
                );
            }
        },
        Terrain::Cloud if matches!(ctx.branch.under, Under::Sky | Under::Stars) => {
            // soft masses overlapping their neighbours' into a bank, each
            // cell's own by its noise
            for k in 0..CLOUD_PUFFS {
                let salt = 70 + k * 4;
                // (scattered past the cell's edges: a bank's rim is
                // ragged, never the cells' straight line)
                // (low: whoever is beside it stands over the bank, knee
                // deep, not hidden behind it)
                let size = 1.2 + ctx.noise(salt) * 1.0;
                let pos = at(
                    (ctx.noise(salt + 1) - 0.5) * 0.9,
                    0.1 + k as f32 * 0.3 + ctx.noise(salt + 2) * 0.2,
                    (ctx.noise(salt + 3) - 0.5) * 0.9,
                );
                look.sheet(
                    plane(size, size),
                    Paint::Liquid(Liquid::Cloud),
                    pos,
                    Vector3::ZERO,
                );
            }
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
            look.opening = Opening::of(sym);
            trap_look(look, sym, nh_color(c), ctx);
            // its letter names it under the pointer and in the overview
            label(look, 0.45);
        }
    }
}

/// A floor of the Astral Plane: a slab of marble floating over the stars,
/// its sides where nothing is beside it (the edge of what is known, or of
/// the plane) a gilded lip, and gold inlaid between its slabs of two by
/// two cells.
fn astral_floor(look: &mut Look, around: &Around, ctx: &Ctx) {
    let gold = ctx.mat("gilded").map_or(
        Paint::Flat(Color::from_rgb(0.8, 0.62, 0.3), Finish::Glossy),
        |m| Paint::Pbr(m, SHADE_LIT, Role::Trim),
    );
    if let Some(m) = ctx.mat("floor") {
        let side = Paint::Pbr(m, 70, Role::Prop);
        look.solid(
            cuboid(1.0, ASTRAL_SLAB, 1.0),
            side,
            at(0.0, -ASTRAL_SLAB / 2.0 - 0.005, 0.0),
        );
        if let Some(s) = look.solids.last_mut() {
            s.shadow = false;
        }
    }
    // north, south, west, east
    let edges = [
        (at(0.0, 0.0, -0.5), true),
        (at(0.0, 0.0, 0.5), true),
        (at(-0.5, 0.0, 0.0), false),
        (at(0.5, 0.0, 0.0), false),
    ];
    for ((pos, along_x), side) in edges.into_iter().zip(around.sides) {
        if side != Side::Solid {
            continue;
        }
        let lip = if along_x {
            cuboid(1.04, 0.05, 0.06)
        } else {
            cuboid(0.06, 0.05, 1.04)
        };
        look.solid(lip, gold, pos + at(0.0, -0.01, 0.0));
    }
    // the inlay: on the west and north edges of every other cell
    let inlay = |look: &mut Look, mesh, pos| {
        look.ground_tile(mesh, gold, pos);
    };
    if ctx.x.rem_euclid(2) == 0 && around.sides[2] == Side::Open {
        inlay(look, plane(0.035, 1.0), at(-0.5, 0.003, 0.0));
    }
    if ctx.y.rem_euclid(2) == 0 && around.sides[0] == Side::Open {
        inlay(look, plane(1.0, 0.035), at(0.0, 0.003, -0.5));
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
        // its rim, its spikes, its lid and its shaft are the opening's
        // (`opening`)
        "S_pit" | "S_spiked_pit" | "S_hole" | "S_trap_door" => {}
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
/// is stone too). A corridor runs under a face of it. In front of open
/// ground (up to three cells north, or north and to a side) it is cut
/// down to a lip, so it hides nothing and nobody: it stands tall only
/// behind what is open, as a room's north wall does.
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
    // cut down, too) it would hide the ground from the camera; south and
    // to a side, what stands there, seen from where the hero is; and a
    // row further still it would stand over the stumps and ledges in
    // front of that ground, a black block before all that is lit
    let front = [near.cells[0], near.cells[4], near.cells[5], near.far_north];
    let front_open = front
        .into_iter()
        .chain(near.beyond)
        .any(|c| is_open(c, catalog));
    // outside a room's south corners it steps down with the room's walls
    // (the corner a stump, the side wall above it half its height):
    // never a dark tower at the front of a room
    let corner = |i: usize, sym: &str| {
        near.cells[i]
            .and_then(|c| c.terrain.as_ref())
            .and_then(|g| cmap_sym(g, catalog))
            == Some(sym)
    };
    let [_, _, w, e, nw, ne, sw, se] = [0, 1, 2, 3, 4, 5, 6, 7];
    let by_corner = corner(e, "S_blcorn")
        || corner(ne, "S_blcorn")
        || corner(w, "S_brcorn")
        || corner(nw, "S_brcorn");
    let over_corner = corner(se, "S_blcorn") || corner(sw, "S_brcorn");
    let h = if front_open || by_corner {
        ROCK_CUT
    } else if over_corner {
        ROCK_HALF
    } else {
        ROCK_HEIGHT
    };
    // (a cave's in lumps like its walls)
    let role = if ctx.branch.cave {
        Role::Cave
    } else {
        Role::Rock
    };
    look.solid(
        rock(1.0, h, 1.0),
        Paint::Pbr(m, SHADE_ROCK, role),
        Vector3::ZERO,
    );
    // broken up by the shader, it would shadow itself in speckles; the
    // walls keep the torches' light in the rooms
    if let Some(s) = look.solids.last_mut() {
        s.shadow = false;
    }
    // (the shader breaks it in chunks: its top only ever lies lower)
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
            if let Some(model) = monster_look(g, ctx) {
                let yaw = match (hero, ctx.facing) {
                    (true, _) => ctx.hero_yaw,
                    (false, Some(yaw)) => yaw,
                    (false, None) => ctx.face_hero(),
                };
                // a swimmer in a liquid floats down to its waterline
                let swim = match ground < 0.0 {
                    true => art.model_at(model.art.model).1.swim * model.art.scale,
                    false => 0.0,
                };
                look.model(model, at(0.0, ground - swim, 0.0), yaw);
                look.entity = Some(look.models.len() - 1);
                look.hero = hero;
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
            // the air bent as if a body stood there, a cold shimmer round
            // it; its letter only under the pointer or in the overview (a
            // letter floating in the dark read as a text cursor)
            look.ground_tile(
                plane(0.9, 1.6),
                Paint::Liquid(Liquid::Unseen),
                at(0.0, ground, 0.0),
            );
            look.reach(ground + 1.2);
            if let Some(ch) = ch {
                let gray = Color::from_rgb(0.7, 0.72, 0.8);
                look.letter(ch, gray, ground + 0.8, PX_MONSTER, true);
                if let Some(l) = look.letters.last_mut() {
                    l.hint = true;
                }
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
            // a long thing lies along one of the cell's diagonals (the
            // longest line it has), a little askew; a short one any way
            let yaw = if crate::art::on_diagonal(&r) {
                let skew = (ctx.noise(8) - 0.5) * 2.0 * crate::art::WARE_JITTER;
                45.0 + 90.0 * (ctx.noise(5) * 3.99).floor() + skew
            } else {
                ctx.noise(5) * 360.0
            };
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
                    cap: nh_art::MAX_FOOTPRINT,
                    back: 0.0,
                    overhangs: false,
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
            // (in the branch's own colours, not its grade's: Gehennom's red
            // turned every thing pink; but gold, whose warmth the grade
            // only deepens: against it, it went slate grey)
            let c = tint_color(r.tint, g.color);
            let un = if tile.class == "$" {
                Color::WHITE
            } else {
                ungraded(&ctx.branch.grade)
            };
            let tint = Color::from_rgba(c.r * un.r, c.g * un.g, c.b * un.b, c.a);
            look.model(
                ModelLook {
                    art: r,
                    tint,
                    pose: Pose::Ware(sheen_of(tile)),
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
            // what it is of is in its number, and which of its cells
            let n = g.glyph.map_or(-1, |n| n - ctx.catalog.glyphs.explode);
            let (kind, heart) = match n {
                0..63 => (Blast::of_type(n / 9), n % 9 == 4),
                _ => (Blast::Fiery, false),
            };
            blast_flash(look, kind, heart, g);
        }
        // the engulfer is drawn once, around the hero
        _ => {}
    }
}

/// How a monster's glyph is drawn: its model at its size, tinted, and
/// see-through when only sensed or seen invisible.
fn monster_look(g: &Glyph, ctx: &Ctx) -> Option<ModelLook> {
    let info = monster_info(ctx.catalog, g.mon)?;
    let r = ctx.art.monster(info, g.flags);
    let pose = if g.flags & (mg::DETECT | mg::INVIS) != 0 {
        Pose::Ghost
    } else {
        Pose::Alive
    };
    let tint = lifted(tint_color(r.tint, g.color));
    Some(ModelLook { art: r, tint, pose })
}

/// A beam, explosion or sparkle for the moment it shows: a ray is a
/// piece of a ray along its direction (the glyph says which, and what it
/// is of), a wand of digging's beam a piece of a ray of dust whose way
/// its next piece shows, the rest a burst of glints; the effects draw
/// them (`Vfx`), no solid stands in the cell.
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
    let kind = match (g.kind, sym) {
        (GlyphKind::Zap, _) => zap_ray(g, ctx.catalog),
        (_, "S_digbeam") => Ray::Dig,
        _ => Ray::Plain,
    };
    match (yaw, sym) {
        (Some(yaw), _) => look.ray = Some((kind, color, Some(yaw), y)),
        (None, "S_digbeam" | "S_flashbeam") => look.ray = Some((kind, color, None, y)),
        _ => look.glint = Some((color, y)),
    }
    look.reach(y + 0.3);
}

/// What a zap glyph's ray is of: its beam type is in its number (four
/// glyphs, one per direction, for each).
fn zap_ray(g: &Glyph, catalog: &Catalog) -> Ray {
    g.glyph
        .map(|n| n - catalog.glyphs.zap)
        .filter(|&n| n >= 0)
        .map_or(Ray::Plain, |n| Ray::of_zap(n / 4))
}

/// Can something north of a wall be seen or stood on (so the wall would
/// hide it)?
pub(crate) fn is_open(cell: Option<&Cell>, catalog: &Catalog) -> bool {
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
/// north-west, north-east, south-west, south-east; the cell two north;
/// and the rest of what a block standing here would have in front of it,
/// seen from the camera: two north and one to each side, and three north.
#[derive(Clone, Copy, Default)]
struct Near<'a> {
    cells: [Option<&'a Cell>; 8],
    far_north: Option<&'a Cell>,
    beyond: [Option<&'a Cell>; 3],
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
            beyond: [
                map.cell(x - 1, y - 2),
                map.cell(x + 1, y - 2),
                map.cell(x, y - 3),
            ],
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
        // (the air of the Planes of Air and Water too)
        let floors = [
            Terrain::Floor,
            Terrain::DarkFloor,
            Terrain::Corridor,
            Terrain::Air,
        ];
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
    // engulfed: the engulfer is drawn round the hero, nothing per cell
    // (no floor the hero is not on)
    if cell
        .glyph
        .as_ref()
        .is_some_and(|g| g.kind == GlyphKind::Swallow)
    {
        return look;
    }
    match &cell.terrain {
        Some(t) => {
            let sym = cmap_sym(t, catalog).unwrap_or("");
            let mut terrain = terrain_of(sym);
            // nobody stands in a shut door: the engine draws whoever is in
            // a doorway with no ground under them (a door is the room's
            // floor to it), so a door last seen shut is the one they opened,
            // broke or slipped under. They stand on its floor, its leaf
            // swung aside, not on its top
            if terrain == Terrain::ClosedDoor && occupied(cell) {
                terrain = Terrain::OpenDoor;
            }
            terrain_look(&mut look, terrain, sym, t, &near, ctx);
            if terrain == Terrain::Stone && cell.entity().is_none() {
                rock_look(&mut look, &near, ctx);
            }
            // whoever stands in a cloud stands in front of it, never lost
            // in its puffs (they are drawn behind, not over)
            if terrain == Terrain::Cloud && cell.entity().is_some() {
                for s in &mut look.solids {
                    if s.paint == Paint::Liquid(Liquid::Cloud) {
                        s.pos.z -= CLOUD_BEHIND;
                    }
                }
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
                // (a bubble's air meets the water beside it all the same)
                let open = Near::default();
                let terrain = terrain_of(sym);
                let near_it = if terrain == Terrain::Air {
                    &near
                } else {
                    &open
                };
                terrain_look(&mut look, terrain, sym, g, near_it, ctx);
                // (its shore with the cells beside it as theirs)
                let kind = look.kind;
                ground_layers(&mut look, kind, &near, ctx);
            }
            // none known: the branch's floor (a flat grey tile reads as
            // a highlight under whoever stands there)
            None => {
                let paint = match ctx.mat("floor") {
                    Some(m) => Paint::Pbr(m, SHADE_LIT, Role::Floor),
                    None => Paint::Flat(FLOOR_UNSEEN, Finish::Matte),
                };
                look.ground_tile(plane(1.0, 1.0), paint, Vector3::ZERO);
                look.kind = Ground::Land;
                ground_layers(&mut look, Ground::Land, &near, ctx);
            }
        },
        None => rock_look(&mut look, &near, ctx),
    }
    // an opening in the floor: cut away with nobody on it, or the hero
    // down in the pit (a pit not seen till they fell in is their notice's)
    let hero_pit = ctx
        .hero_pit
        .filter(|_| ctx.hero == Some((ctx.x, ctx.y)))
        .map(|spiked| Opening::Pit { spiked });
    let unseen_floor = Paint::Flat(FLOOR_UNSEEN, Finish::Matte);
    match (hero_pit.or(look.opening), cell.entity().is_none()) {
        (Some(o), empty) if empty || hero_pit.is_some() => {
            look.opening = Some(o);
            opening::open_floor(&mut look, o, unseen_floor, ctx);
        }
        (Some(o), _) => opening::closed_floor(&mut look, o, unseen_floor, ctx),
        (None, _) => {}
    }
    // a room's floor dressed where it shows bare: nothing lies on it,
    // nothing is written on it, nothing opens in it
    let lies = |g: &Glyph| matches!(g.kind, GlyphKind::Obj | GlyphKind::Body | GlyphKind::Statue);
    if let Some(t) = &cell.terrain
        && look.opening.is_none()
        && !cell.glyph.as_ref().is_some_and(lies)
    {
        let sym = cmap_sym(t, catalog).unwrap_or("");
        if matches!(terrain_of(sym), Terrain::Floor | Terrain::DarkFloor) && !engraved(sym) {
            dressing::floor(&mut look, &Around::of(&near, catalog), ctx);
        }
    }
    if let Some(g) = &cell.glyph {
        glyph_look(&mut look, g, ctx);
        // a peaceful one, as the engine tells it: no hostile's red ring
        if cell.peaceful {
            look.hostile = false;
        }
        // a steed's glyph is all the map shows of the hero on it: the
        // engine tells who rides, and of a saddle on any monster
        if g.kind == GlyphKind::Mon && look.entity.is_some() {
            look.mon = g.mon;
            look.sensed = cell.sensed;
            look.wields = cell
                .wields
                .as_ref()
                .filter(|_| g.flags & mg::HERO == 0)
                .and_then(|w| object_tile(ctx.catalog, w.tile))
                .and_then(|t| ctx.art.held(t));
            look.saddled = cell.saddled;
            look.rider = cell
                .rider
                .as_ref()
                .filter(|_| g.flags & mg::RIDDEN != 0)
                .and_then(|r| monster_look(r, ctx));
        }
    }
    // what passes over the cell: a flash over who stands there, a missile
    // at the feet of the hero it reaches (another creature drawn over the
    // hero is on its way to its own cell). Who stood there stays the
    // cell's entity
    if let Some(g) = cell.over.as_ref().filter(|g| g.kind != GlyphKind::Mon) {
        let entity = look.entity;
        glyph_look(&mut look, g, ctx);
        look.entity = entity;
    }
    look
}

/// Someone or something is on the cell, not an effect passing over it.
fn occupied(cell: &Cell) -> bool {
    cell.glyph.as_ref().is_some_and(|g| {
        matches!(
            g.kind,
            GlyphKind::Mon
                | GlyphKind::Obj
                | GlyphKind::Body
                | GlyphKind::Statue
                | GlyphKind::Invisible
                | GlyphKind::Warning
        )
    })
}

/// What a glyph adds to its cell's look: an effect flashes, a creature or
/// a thing stands there (terrain is the cell's own, remembered).
fn glyph_look(look: &mut Look, g: &Glyph, ctx: &Ctx) {
    if g.kind != GlyphKind::Cmap {
        entity_look(look, g, ctx);
    } else if let Some(sym) = cmap_sym(g, ctx.catalog).filter(|&s| terrain_of(s) == Terrain::Effect)
    {
        match sym.strip_prefix("S_expl_") {
            Some(part) => blast_flash(look, Blast::Fiery, part == "mc", g),
            None => bright_flash(look, g, ctx),
        }
    }
}

/// A cell of an explosion for the moment it shows: the effects draw it
/// as what it is of (`Vfx::blast`), no glitter, no solid in the cell.
fn blast_flash(look: &mut Look, kind: Blast, heart: bool, g: &Glyph) {
    look.blast = Some((kind, lighter(nh_color(g.color), 0.2), heart));
    look.reach(look.ground.max(0.0) + 0.9);
}

/// A feature's light (and mist) on its cell.
struct CellLamp {
    kind: Lamp,
    light: Gd<OmniLight3D>,
    /// A stair's mist, an altar's shaft of light.
    mist: Option<Gd<Node3D>>,
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
    /// Put out, to be lit again elsewhere (lamps are reused, not freed).
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

/// The hero's own model while they ride: their cell shows the steed, and
/// they sit on its back.
struct Rider {
    model: Model,
    look: ModelLook,
    /// The steed's node they sit on and its back (None: no steed is drawn
    /// for them yet).
    on: Option<(Gd<Node3D>, Mount)>,
    /// Climbing up: where they stood on foot (the place, the yaw) and the
    /// seconds since they left it.
    from: Option<(Vector3, f32, f32)>,
    /// Put in the saddle once: one not seen climbing up (a game restored
    /// mounted, a level reached so) is seated from the first frame.
    seated: bool,
}

/// Seconds the hero takes from the ground to the steed's back, and how
/// high they hop on the way.
const MOUNT_SECS: f32 = 0.3;
const MOUNT_HOP: f32 = 0.12;
/// The top of a rider's head over the saddle, and how high a body's hips
/// are as it stands, in its own heights.
const RIDER_OVER_SEAT: f32 = 0.5;
const HIPS_HEIGHT: f32 = 0.57;

/// What the animator did (self-tests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MotionStats {
    pub hero_steps: u32,
    pub other_steps: u32,
    pub strikes: u32,
    /// Blows that made their target reel.
    pub flinches: u32,
    /// Deaths seen that fell.
    pub falls: u32,
    /// Frames whose rays waited for a breather's mouth to open.
    pub breaths_held: u32,
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
    sconce: Option<Gd<Node3D>>,
    flame: Gd<MeshInstance3D>,
    halo: Gd<MeshInstance3D>,
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
    /// The torches nearest the hero that cast shadows (the graphics'
    /// shadows: `set_graphics`).
    torch_shadows: usize,
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
    /// The model the ring was sized for: a polymorphed hero's (None: the
    /// hero's own size), and how high the hero stood.
    ring_sized: Option<(Option<InstanceId>, f32)>,
    ring_tex: Gd<godot::classes::Texture2D>,
    hero_light: Gd<OmniLight3D>,
    /// The glow from below round the hero where the floors burn.
    underglow: Gd<OmniLight3D>,
    rim: Gd<OmniLight3D>,
    /// The hero's key and fills (`HERO_STUDIO`), off while they are
    /// polymorphed.
    hero_studio: Gd<Node3D>,
    /// The light every creature seen takes (`CREATURE_LIGHT_WAY`), off in
    /// the gallery (its studio light takes the last of Godot's eight).
    creature_light: Gd<DirectionalLight3D>,
    /// The hero's model, on the rim light's layer too.
    rim_model: Option<Gd<Node3D>>,
    /// The hero's own model while they ride (their cell shows the steed).
    rider: Option<Rider>,
    /// The steed under them, on the layer their key and fills light.
    lit_steed: Option<Gd<Node3D>>,
    /// The trap the hero's cell was last drawn for (a pit opens it).
    hero_trap_drawn: Option<nh_world::TrapKind>,
    /// The hero dropping through the floor while the level they leave is
    /// held; the level (its generation) a drop was looked for.
    hero_drop: Option<drop::Dropping>,
    dropped: Option<u64>,
    /// A fall through the floor the lines told, its level still to come;
    /// a level come with none told while they stood on a hole, whether
    /// they went down it not known yet.
    fall_told: Option<Opening>,
    undecided: Option<drop::Undecided>,
    /// Seconds the engine's last drawing of the map has been read only in
    /// part (`World::map_drawn`).
    undrawn: f32,
    /// Monsters seen falling through the floor, dropping.
    drops: Vec<drop::Dropping>,
    /// The steed under a hero a wall hides, wearing the x-ray overlay
    /// with them.
    xray_steed: Option<Gd<Node3D>>,
    /// Small creatures the hero's body hides (in the cells north of them),
    /// seen through it, and whether each is hostile; the x-ray they wear,
    /// a hostile's and a pet's.
    xray_behind: Vec<(Gd<Node3D>, bool)>,
    xray_creature: Option<(Gd<Material>, Gd<Material>)>,
    /// The huge and gigantic creatures drawn see-through: each hides an
    /// object or a trap the hero knows of.
    big_faded: Vec<Gd<Node3D>>,
    /// Where a rider would stand if the saddle were the ground, facing
    /// their way: what looks at the hero alone (the inventory's doll)
    /// frames them from there as it does on foot.
    rider_view: Gd<Node3D>,
    /// Fill lights over the parts of the level in view.
    room_lights: Vec<Gd<OmniLight3D>>,
    /// Lava's own light, over each pool of it known (`place_lava_lights`).
    lava_lights: Vec<Gd<OmniLight3D>>,
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
    /// The ground of every cell, which the ground's shaders blend.
    ground: GroundField,
    /// Where something stands or lies (the fire's cracks keep clear).
    standing: StandField,
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
    /// The look of the level drawn (`look_of_level`), and whether the
    /// hero has seen it to be a swamp (`branch_look` is then its swamp's).
    level_look: BranchLook,
    swamp: bool,
    /// In a cave branch, the walls built (a door set in them or joined to
    /// one: Minetown's shops and temple), drawn in `built_look`.
    built: HashSet<(i32, i32)>,
    built_look: BranchLook,
    /// The level's plan: its arches and inlays (`dressing::plan`).
    volume: HashMap<(i32, i32), dressing::Volume>,
    /// The post layer's material, and how far a new level is still
    /// hidden in black (1 when it is drawn, 0 when it shows).
    post_mat: Option<Gd<ShaderMaterial>>,
    fade: f32,
    /// Frame times being summed (RENETHACK_FRAME_STATS).
    stats_window: Option<FrameStats>,
    /// RENETHACK_DUMP_SIGHTINGS=<file>: each turn, the creatures on the
    /// map and the art each is drawn with, appended as a JSON line (which
    /// bodies are seen most), and the turn last written.
    sightings: Option<(std::fs::File, Option<i64>)>,
    /// The art loaded ahead has been told of (RENETHACK_FRAME_STATS).
    preload_told: bool,
    /// The rehearsal's first draws done has been told of (stats).
    first_draws_told: bool,
    /// The level played behind the title screen (render pipelines
    /// compiled ahead), until it is over; and whether the map is shown.
    rehearsal: Option<crate::rehearsal::Rehearsal>,
    rehearsed: bool,
    /// A later launch: Godot's shader cache holds the pipelines, so no
    /// level is rehearsed. The title is laid at once; the rehearsal's
    /// models load with the art, its files in `unplayed` (kept for the
    /// levels that draw them).
    quick: bool,
    unplayed: Option<crate::rehearsal::Rehearsal>,
    /// One of each pipeline variant a game draws, made by the rehearsal;
    /// their materials kept for the session.
    variants: crate::variants::Variants,
    /// The shadow atlas has been made (the rehearsal casts a shadow).
    shadow_made: bool,
    /// A frame's time for drawing a new level's cells (`BUILD_BUDGET`, or
    /// less where the rehearsal draws a level's first frames).
    build_budget: std::time::Duration,
    /// How many of the map's parts draw behind the title (see
    /// `show_parts`).
    showing: u8,
    shadow_block: Option<Gd<MeshInstance3D>>,
    shadow_frames: u64,
    /// Cards the rehearsal shows textures on (see `show_textures`), and
    /// what it shows once (see `show_once`).
    cards: Vec<Gd<MeshInstance3D>>,
    shown_once: Vec<Gd<Node3D>>,
    embers_shown: bool,
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
    /// The sea of clouds far below the Plane of Air (made the first time),
    /// and the light from above (the moon's, the Planes' own sun).
    sky_sea: Option<Gd<MeshInstance3D>>,
    sea_mats: Vec<(&'static str, Gd<Material>)>,
    moon: Gd<DirectionalLight3D>,
    back: Gd<DirectionalLight3D>,
    /// The vignette and grain over the map, under the HUD.
    post: Gd<CanvasLayer>,
    /// The inside of what engulfed the hero, round them.
    engulfed: Engulfed,
    /// A beat of the prologue drawn instead of the title (its level is
    /// `title`), and its camera: where it is, what it looks at.
    stage: Option<prologue_stage::Stage>,
    /// The next beat's stage, built ahead and hidden.
    next_stage: Option<prologue_stage::Stage>,
    /// What the prologue's sets are built of, their scenes, materials and
    /// textures: made once (under the start-up veil) and kept.
    stage_kit: HashMap<&'static str, Gd<godot::classes::Resource>>,
    stage_camera: Option<(Vector3, Vector3)>,
    target: Vector3,
    focus: Vector3,
    distance: f32,
    /// The whole-level view is on: its distance, refitted as cells appear.
    overview: Option<f32>,
    /// How far the overview's clear air has come in (0 to 1).
    overview_k: f32,
    /// How much of the branch's haze the air holds at the camera's
    /// distance now (`air_near`), as last set.
    air_near: f32,
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
    /// Cells whose step was cut short by the next batch, and when.
    interrupted: HashSet<(i32, i32)>,
    cut_at: f64,
    /// The newest message already looked at for fights.
    log_seq: Option<u64>,
    /// What the map's changes sound like this frame, for the player
    /// (`take_sounds`).
    sounds: Vec<Sound>,
    /// The ray last seen lighting a cell: where, of what, when (a bolt's
    /// pieces light one cell after another: one travel heard).
    last_ray: Option<((i32, i32), Ray, f64)>,
    /// When the last explosion was heard.
    last_blast: f64,
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
    /// The scene behind the title menu once the rehearsal is over, the
    /// hero it shows (a role's name or code, a woman's look: a Valkyrie),
    /// and the catalog it is laid from (again when the title comes back
    /// after a game).
    title: Option<crate::title_scene::TitleScene>,
    /// The title scene is what the map draws (the title camera frames it).
    titled: bool,
    title_hero: (String, bool),
    title_catalog: Option<Catalog>,
    /// The hero the creation form is making, shown in the title's place
    /// (`set_creation_hero`), and how far the camera has gone from the
    /// title's frame to theirs (0 to 1).
    creation: Option<crate::title_scene::Preview>,
    creation_blend: f32,
    /// Whether the start-up veil is down (the game tells; None: never
    /// told, the rehearsal judges by itself).
    veiled: Option<bool>,
    /// The title scene's fires (made once, shown with it): each its node
    /// and its light, where that burns and its flicker's phase.
    title_fires: Vec<(Gd<Node3D>, Gd<OmniLight3D>, Vector3, f64)>,
    /// How far the 3D is hidden in black while it is not to be seen (the
    /// rehearsal behind the title page): 1 hidden, 0 shown.
    cover: f32,
    /// The effects of the items the hero uses.
    hero_fx: crate::hero::HeroFx,
    /// What the hero's body shows of their state.
    hero_pose: hero_pose::HeroPose,
    hero_look: hero_look::HeroLook,
    /// Creatures gone from their cells this frame, the deaths the frame's
    /// messages tell (by name, and the cell a blow found them on), the
    /// bodies falling, and the last fall to end (self-tests).
    gone: Vec<fall::Gone>,
    deaths: Vec<fall::Told>,
    falls: Vec<fall::Fall>,
    fell: Option<Fell>,
    /// What monsters were seen doing: the effects waiting their moment,
    /// and the acts shown lately (self-tests).
    act_fx: Vec<acts::ActFx>,
    acts_shown: Vec<Acted>,
    /// Bodies growing in where they came, and the cells a monster came in
    /// sight on lately, and when (where help summoned comes).
    arrivals: Vec<acts::Arrival>,
    appeared: Vec<((i32, i32), f64)>,
    /// Summoners told lately: where, when.
    summons: Vec<((i32, i32), f64)>,
    /// Seconds the rays drawn this frame wait (a breather's mouth opening
    /// first), and the rays waiting.
    ray_wait: f32,
    rays_waiting: Vec<acts::WaitingRay>,
    /// Cells whose monster takes what it holds in hand (or puts it down)
    /// once its body is built.
    to_arm: Vec<(i32, i32)>,
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
    // and the same mark on a sheet a little under the cell's plane, for
    // where there is no ground to land on (the Planes of Air and Water, a
    // bubble's see-through floor): a floor hides it
    let mut sheet = MeshInstance3D::new_alloc();
    sheet.set_name("Sheet");
    let mut quad = godot::classes::PlaneMesh::new_gd();
    quad.set_size(Vector2::new(1.0, 1.0));
    sheet.set_mesh(&quad);
    let mut m = StandardMaterial3D::new_gd();
    m.set_shading_mode(ShadingMode::UNSHADED);
    m.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA);
    m.set_texture(
        godot::classes::base_material_3d::TextureParam::ALBEDO,
        texture,
    );
    sheet.set_material_override(&m);
    sheet.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    sheet.set_position(Vector3::new(0.0, -MARK_UNDER, 0.0));
    sheet.set_scale(Vector3::new(size, 1.0, size));
    d.add_child(&sheet);
    root.add_child(&d);
    d
}

/// A mark's sheet (`marker`) and its material.
fn mark_sheet(d: &Gd<Decal>) -> Option<(Gd<MeshInstance3D>, Gd<StandardMaterial3D>)> {
    let sheet = d
        .get_node_or_null("Sheet")?
        .try_cast::<MeshInstance3D>()
        .ok()?;
    let m = sheet
        .get_material_override()?
        .try_cast::<StandardMaterial3D>()
        .ok()?;
    Some((sheet, m))
}

/// A mark's colour, on the ground and on its sheet (more opaque there:
/// the ground's own light shows through the decal).
fn mark_color(d: &mut Gd<Decal>, c: Color) {
    d.set_modulate(c);
    if let Some((_, mut m)) = mark_sheet(d) {
        m.set_albedo(c.with_alpha((c.a * MARK_SHEET_ALPHA).min(1.0)));
    }
}

/// A mark's size, on the ground and on its sheet.
fn mark_size(d: &mut Gd<Decal>, size: Vector3) {
    d.set_size(size);
    if let Some((mut sheet, _)) = mark_sheet(d) {
        sheet.set_scale(Vector3::new(size.x, 1.0, size.z));
    }
}

/// A mark's picture, on the ground and on its sheet.
fn mark_texture(d: &mut Gd<Decal>, tex: &Gd<godot::classes::Texture2D>) {
    d.set_texture(godot::classes::decal::DecalTexture::ALBEDO, tex);
    d.set_texture(godot::classes::decal::DecalTexture::EMISSION, tex);
    if let Some((_, mut m)) = mark_sheet(d) {
        m.set_texture(godot::classes::base_material_3d::TextureParam::ALBEDO, tex);
    }
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

/// A geometry under `node` is drawn once more with one of the overlays
/// `ids`.
fn wears(node: &Gd<Node3D>, ids: &[InstanceId]) -> bool {
    node.find_children_ex("*")
        .type_("GeometryInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<GeometryInstance3D>().ok())
        .any(|g| {
            g.get_material_overlay()
                .is_some_and(|m| ids.contains(&m.instance_id()))
        })
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

/// The height of a model's meshes (its bounds), in its own units.
fn model_height(model: &Gd<Node3D>) -> f32 {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for n in model
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if let Ok(mi) = n.try_cast::<MeshInstance3D>() {
            let b = mi.get_aabb();
            // the mesh's own transform up to the model (scales and offsets)
            let mut t = mi.get_transform();
            let mut p = mi.get_parent();
            while let Some(node) = p {
                if node.instance_id() == model.instance_id() {
                    break;
                }
                p = node.get_parent();
                if let Ok(n3) = node.try_cast::<Node3D>() {
                    t = n3.get_transform() * t;
                }
            }
            let b = t * b;
            lo = lo.min(b.position.y);
            hi = hi.max(b.position.y + b.size.y);
        }
    }
    if hi > lo { hi - lo } else { 0.0 }
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

/// Every mesh under `node` on these render layers (the lights the hero
/// carries, a lamp, keep their own). The model is in view: see
/// `art::set_layer_mask` for why the change goes through it.
fn set_layers(node: &Gd<Node3D>, mask: u32) {
    for n in node
        .find_children_ex("*")
        .type_("GeometryInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if let Ok(mut v) = n.try_cast::<VisualInstance3D>() {
            // a creature's stays in the creatures' light
            let mask = mask | v.get_layer_mask() & CREATURE_LAYER;
            set_layer_mask(&mut v, mask);
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
        moon.set_rotation_degrees(MOON.0);
        moon.set_color(MOON.1);
        moon.set_param(Param::ENERGY, MOON.2);
        moon.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        root.add_child(&moon);
        // and fainter still from the other side: no wall face is flat black
        let mut back = DirectionalLight3D::new_alloc();
        back.set_rotation_degrees(BACK.0);
        back.set_color(BACK.1);
        back.set_param(Param::ENERGY, BACK.2);
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
        // everything but the doll's lights
        let seen = camera.get_cull_mask() & !DOLL_LAYER;
        camera.set_cull_mask(seen);
        root.add_child(&camera);

        let mut cells_root = Node3D::new_alloc();
        root.add_child(&cells_root);
        cells_root.set_name("Cells");

        let ring_tex = crate::vfx::ring_texture();
        let mut hover = marker(&mut root, &crate::vfx::frame_texture(), 1.0);
        mark_color(&mut hover, Color::from_rgba(1.0, 0.88, 0.62, 0.2));
        let mut cursor = marker(&mut root, &crate::vfx::frame_texture(), 1.05);
        mark_color(&mut cursor, CURSOR);

        // the node the doll aims at, and the child it takes the way the
        // body faces from (as from a model's own node)
        let mut rider_view = Node3D::new_alloc();
        rider_view.set_name("RiderView");
        rider_view.add_child(&Node3D::new_alloc());
        root.add_child(&rider_view);
        let mut hero_ring = marker(&mut root, &ring_tex, HERO_RING_SIZE);
        mark_color(&mut hero_ring, HERO_RING);
        let mut hostile_ring = marker(&mut root, &ring_tex, 1.05);
        mark_color(&mut hostile_ring, HOSTILE_RING);
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
        let xray_shader = godot::tools::try_load::<Shader>("res://shaders/xray.gdshader").ok();
        let xray_of = |color: Color| {
            xray_shader.as_ref().map(|shader| {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(shader);
                m.set_shader_parameter("color", &color.to_variant());
                m.upcast::<Material>()
            })
        };
        let xray = xray_of(XRAY);
        // a creature behind the hero: its contour only, one line round it
        // (its mask marks where it is hidden, its grown shell draws
        // outside that)
        let mask = godot::tools::try_load::<Shader>("res://shaders/xray_mask.gdshader").ok();
        let contour = godot::tools::try_load::<Shader>("res://shaders/xray_contour.gdshader").ok();
        let contour_of = |color: Color| {
            let (mask, contour) = (mask.as_ref()?, contour.as_ref()?);
            let mut line = ShaderMaterial::new_gd();
            line.set_shader(contour);
            line.set_shader_parameter("color", &color.to_variant());
            line.set_render_priority(1);
            let mut m = ShaderMaterial::new_gd();
            m.set_shader(mask);
            m.set_next_pass(&line);
            Some(m.upcast::<Material>())
        };
        let xray_creature = contour_of(XRAY_FOE).zip(contour_of(XRAY_PET));

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
        let mut studio_frame = Node3D::new_alloc();
        studio_frame.set_transform(
            Transform3D::new(Basis::IDENTITY, HERO_STUDIO_FROM).looking_at(Vector3::ZERO),
        );
        for ((way, color, energy), steed) in HERO_STUDIO.into_iter().zip(STEED_STUDIO) {
            let mut light = DirectionalLight3D::new_alloc();
            light.set_rotation(way);
            light.set_color(color);
            light.set_param(Param::ENERGY, energy);
            light.set_cull_mask(if steed {
                RIM_LAYER | STEED_LAYER
            } else {
                RIM_LAYER
            });
            // (the mask keeps it off the level's surfaces, not out of the
            // level's haze: it would light the air of the whole map)
            light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
            light.set_shadow(false);
            studio_frame.add_child(&light);
        }
        root.add_child(&studio_frame);
        let mut creature_light = DirectionalLight3D::new_alloc();
        creature_light.set_rotation_degrees(CREATURE_LIGHT_WAY);
        creature_light.set_color(RIM_LIGHT);
        creature_light.set_param(Param::ENERGY, CREATURE_LIGHT_ENERGY);
        creature_light.set_param(Param::SPECULAR, 0.0);
        creature_light.set_cull_mask(CREATURE_LAYER);
        // (as the studio's: off the haze, which the mask does not keep it
        // out of)
        creature_light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        creature_light.set_shadow(false);
        root.add_child(&creature_light);

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

        let engulfed = Engulfed::new(&mut root);

        let art = Art::new(cells_root.clone());
        let center = Vector3::new(COLNO as f32 / 2.0, 0.0, ROWNO as f32 / 2.0);
        let hero_fx = crate::hero::HeroFx::new(root.clone());
        let hero_pose = hero_pose::HeroPose::new(&mut root);
        let hero_look = hero_look::HeroLook::new();
        let fow = Fow::new();
        let ground = GroundField::new();
        let standing = StandField::new();
        let surfaces = Surfaces::new(&fow, &ground, &standing, art.manifest());
        let batches = Batches::new(cells_root.clone(), 1 | TERRAIN_LAYER);
        let vfx = Vfx::new(root.clone(), TERRAIN_LAYER);
        let mut view = MapView {
            root,
            cells_root,
            camera,
            env,
            torch_shadows: TORCH_SHADOWS,
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
            ring_sized: None,
            ring_tex,
            hero_light,
            underglow,
            rim,
            hero_studio: studio_frame,
            creature_light,
            rim_model: None,
            rider: None,
            lit_steed: None,
            hero_trap_drawn: None,
            hero_drop: None,
            dropped: None,
            fall_told: None,
            undecided: None,
            undrawn: 0.0,
            drops: Vec::new(),
            xray_steed: None,
            xray_behind: Vec::new(),
            xray_creature,
            big_faded: Vec::new(),
            rider_view,
            room_lights: Vec::new(),
            lava_lights: Vec::new(),
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
            ground,
            standing,
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
            level_look: crate::branch_look::look_of(Branch::Main),
            swamp: false,
            built: HashSet::new(),
            volume: HashMap::new(),
            built_look: crate::branch_look::look_of(Branch::Main),
            stats_window: std::env::var_os("RENETHACK_FRAME_STATS").map(|_| FrameStats::default()),
            sightings: std::env::var_os("RENETHACK_DUMP_SIGHTINGS").and_then(|p| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .ok()
                    .map(|f| (f, None))
            }),
            preload_told: false,
            first_draws_told: false,
            rehearsal: None,
            variants: Default::default(),
            rehearsed: false,
            quick: false,
            unplayed: None,
            shadow_made: false,
            build_budget: BUILD_BUDGET,
            showing: 0,
            shadow_block: None,
            shadow_frames: 0,
            cards: Vec::new(),
            shown_once: Vec::new(),
            embers_shown: false,
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
            sky_sea: None,
            sea_mats: Vec::new(),
            moon,
            back,
            post,
            engulfed,
            stage: None,
            next_stage: None,
            stage_kit: HashMap::new(),
            stage_camera: None,
            target: center,
            focus: center,
            distance: DISTANCE,
            overview: None,
            overview_k: 0.0,
            air_near: 1.0,
            snap: true,
            clock: 0.0,
            hero_at: None,
            hero_yaw: 0.0,
            facing: HashMap::new(),
            incoming: HashMap::new(),
            motions: Vec::new(),
            interrupted: HashSet::new(),
            cut_at: 0.0,
            log_seq: None,
            sounds: Vec::new(),
            last_ray: None,
            last_blast: f64::NEG_INFINITY,
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
            hero_pose,
            hero_look,
            gone: Vec::new(),
            deaths: Vec::new(),
            falls: Vec::new(),
            fell: None,
            act_fx: Vec::new(),
            acts_shown: Vec::new(),
            arrivals: Vec::new(),
            appeared: Vec::new(),
            summons: Vec::new(),
            ray_wait: 0.0,
            rays_waiting: Vec::new(),
            to_arm: Vec::new(),
            title: None,
            titled: false,
            title_hero: ("valkyrie".to_string(), true),
            title_catalog: None,
            creation: None,
            creation_blend: 0.0,
            veiled: None,
            title_fires: Vec::new(),
            cover: 1.0,
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
        // the engine's drawing of the map read half (8 ms a frame: a loaded
        // machine may read a step's two cells, a mount's, frames apart):
        // the scene waits for the rest, the hero where it last drew them
        let drawn = if world.map_drawn() {
            self.undrawn = 0.0;
            true
        } else {
            self.undrawn += delta as f32;
            self.undrawn >= UNDRAWN_MOST
        };
        let hero = if drawn || self.generation != Some(world.map.generation()) {
            world.hero()
        } else {
            self.hero_at.or_else(|| world.hero())
        };
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
        let was_at = std::mem::replace(&mut self.hero_at, hero);
        // another branch, or a level drawn otherwise: its own look, and
        // every cell drawn again in it; a lair the overview names now, its
        // own light and air
        let branch = world.branch();
        let look = crate::branch_look::look_of_level(branch, world.level.as_ref());
        if branch != self.branch || look.cells() != self.level_look.cells() {
            self.level_look = look;
            self.swamp = false;
            self.set_branch(branch);
            self.generation = None;
        } else if look != self.level_look {
            self.level_look = look;
            self.set_branch(branch);
        }
        let generation = world.map.generation();
        let new_level = self.generation != Some(generation);
        // fallen through the floor: the level they leave held while they
        // drop into it, then black
        if new_level && self.hold_for_drop(world, catalog, generation, was_at, delta as f32) {
            // the floor opened under them shows now
            self.batches.flush();
            return;
        }
        let started = std::time::Instant::now();
        if new_level {
            // a new level (or a redraw from scratch) never animates
            self.finish_motions();
            self.ground.clear();
            self.standing.clear();
            // a swamp seen was that level's
            if std::mem::take(&mut self.swamp) {
                self.set_branch(self.branch);
            }
            self.finish_falls_where(|_| true);
            self.end_drops();
            self.forget_gone();
            self.vfx.clear();
            self.facing.clear();
            self.refresh_built(world, catalog);
            self.refresh_volume(world, catalog);
            self.generation = Some(generation);
            world.map.take_dirty();
            // built over the next frames, nearest the hero first (the
            // lights are placed once it is all drawn)
            self.building = build_order(hero);
            self.snap = true;
            self.lights_dirty = true;
        } else {
            // the lines are read every frame (watched below), the drawing
            // of the map once it is whole
            self.note_fall(world, hero != was_at);
            // a breath told: its rays wait for the mouth to open
            self.breath_coming(world, catalog);
        }
        if !new_level && drawn {
            // a cell's look depends on its neighbours: a wall on the cell
            // north of it (and the rock on the two), unseen ground under
            // something on all four, the rock on all eight
            let batch = world.map.take_batch();
            let (mut dirty, moves, changed) = (batch.dirty, batch.moves, batch.changed);
            // a fall whose cell shows something else again ends at once
            if !self.falls.is_empty() && !changed.is_empty() {
                self.finish_falls_where(|c| changed.contains(&c));
            }
            if !moves.is_empty() {
                // the engine went on: the scene catches up at once
                self.finish_motions();
            } else if !changed.is_empty() {
                // a motion whose cell shows another now catches up; one
                // that something only passes over (a missile in flight, a
                // ray) goes on to its end
                self.finish_motions_where(|m| changed.contains(&m.cell));
            }
            self.carry(&moves, world);
            self.hear_steps(&moves, world, catalog);
            self.carry_rider(was_at, world);
            self.flank_steed(was_at, hero, world);
            // walls found to be built (a door seen in them) drawn again
            dirty.extend(self.refresh_built(world, catalog));
            // and cells the level's plan gives something else now
            dirty.extend(self.refresh_volume(world, catalog));
            let near: Vec<_> = dirty
                .iter()
                .flat_map(|&(x, y)| {
                    (-1..=1)
                        .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
                        .chain([(x, y + 2), (x - 1, y + 2), (x + 1, y + 2), (x, y + 3)])
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
            // down in a pit or out of it where they stand: their floor opens
            // or closes
            if world.hero_trap != self.hero_trap_drawn
                && let Some((x, y)) = hero
            {
                self.hero_trap_drawn = world.hero_trap;
                self.update_cell(x, y, world, catalog, hero);
            }
            // the rays drawn now held for a breather's mouth, no later ones
            self.ray_wait = 0.0;
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
        self.arm_monsters();
        self.prof
            .push(("build", __t.elapsed().as_secs_f64() * 1000.0));
        self.seat_rider(hero);
        self.light_steed();
        self.watch_fights(world, catalog);
        // the deaths told fall; what no death claims goes
        self.claim_falls(catalog);
        self.greet_summoned();
        self.advance_motions(delta as f32);
        self.advance_falls(delta as f32);
        self.advance_drops(delta as f32, catalog);
        self.advance_acts(delta as f32);
        self.advance_arrivals(delta as f32);
        self.advance_rays(delta as f32);
        self.follow_steed(delta as f32);
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
        self.surfaces.show_ground(&mut self.ground);
        self.surfaces.show_standing(&mut self.standing);
        // water enough seen on a level of a branch that turns to a swamp:
        // a swamp, its light and air, for the rest of the visit
        if self.level_look.swamps && !self.swamp && self.ground.count(Ground::Water) >= SWAMP_WATER
        {
            self.swamp = true;
            self.set_branch(self.branch);
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
                let form = world.status.polymorphed();
                let head = self
                    .hero_stands()
                    .filter(|_| form)
                    .map(|(_, tall)| self.hero_pose.lift + tall);
                let over = |least: f32, by: f32| head.map_or(least, |h| (h + by).max(least));
                self.hero_light
                    .set_position(p + at(0.0, over(HERO_LIGHT_HEIGHT, FORM_LIGHT_OVER), 0.6));
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
                self.rim
                    .set_position(p + at(0.0, over(RIM_HEIGHT, FORM_RIM_OVER), -0.9));
                self.rim.set_visible(true);
                if self.hero_studio.is_visible() == form {
                    self.hero_studio.set_visible(!form);
                }
                self.size_hero_ring(form, ground);
                // while a level is drawn the hero's cell may still hold the
                // last level's model; while the engine's drawing is read in
                // part, the cell the world has them on is not drawn yet
                if self.building.is_empty() && drawn {
                    self.rim_hero(world.map.hero());
                }
                let __t = std::time::Instant::now();
                self.equip_hero((x, y), world, catalog, delta as f32);
                self.prof
                    .push(("equip", __t.elapsed().as_secs_f64() * 1000.0));
                self.show_through((x, y), p, world, catalog);
                self.light_torches(p);
            }
            None => {
                self.hero_ring.set_visible(false);
                self.hero_light.set_visible(false);
                self.underglow.set_visible(false);
                self.rim.set_visible(false);
                self.unshow_behind(&[]);
                RenderingServer::singleton()
                    .global_shader_parameter_set("hero_pos", &at(0.0, -100.0, 0.0).to_variant());
            }
        }
        self.pose_hero(world, catalog, delta as f32);
        self.look_hero(world, catalog, delta as f32);
        self.show_hover();
        if std::mem::take(&mut self.hints_dirty) {
            self.show_hints();
        }
        // its mesh is laid once a level is all drawn, not again in every
        // frame of the drawing
        if self.building.is_empty() && std::mem::take(&mut self.bedrock_dirty) {
            let __t = std::time::Instant::now();
            self.lay_bedrock();
            self.prof
                .push(("bedrock", __t.elapsed().as_secs_f64() * 1000.0));
        }
        let (x, y) = hero.unwrap_or_default();
        let floor = Vector3::new(x as f32, self.ground(x, y), y as f32);
        let inside = hero.and_then(|h| engulfer(world, catalog, h));
        self.engulfed.show(inside, floor, delta);
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
        self.ease_overview(delta as f32);
        let near = self.air_near();
        if (near - self.air_near).abs() > 0.01 {
            self.air_near = near;
            self.keep_prologue_air();
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
        self.tell_sightings(world, catalog);
        if let Some(stats) = self.stats_window.as_mut() {
            let ms = sync_start.elapsed().as_secs_f64() * 1000.0;
            if ms > 10.0 {
                godot_print!("map: a sync of {ms:.1} ms: {:?}", self.prof);
            }
            stats.last_sync = sync_start.elapsed().as_secs_f64() * 1000.0;
            stats.last_built = self.art.counts().1 - built_before;
        }
    }

    /// With RENETHACK_DUMP_SIGHTINGS set, once a turn: the creatures on
    /// the map but the hero, each with the model it is drawn as (and
    /// whether that is a procedural body).
    fn tell_sightings(&mut self, world: &World, catalog: &Catalog) {
        use std::io::Write;
        let Some((file, last)) = self.sightings.as_mut() else {
            return;
        };
        let turn = world.turn();
        if turn.is_none() || turn == *last {
            return;
        }
        *last = turn;
        let art = self.art.manifest();
        let mut seen = Vec::new();
        for y in 0..ROWNO {
            for x in 0..COLNO {
                let Some(g) = world.map.cell(x, y).and_then(|c| c.glyph.as_ref()) else {
                    continue;
                };
                if g.kind != GlyphKind::Mon || g.flags & mg::HERO != 0 {
                    continue;
                }
                let Some(info) = g
                    .mon
                    .and_then(|i| catalog.monsters.iter().find(|m| m.idx == i))
                else {
                    continue;
                };
                let (model, spec) = art.model_at(art.monster(info, g.flags).model);
                let body = if spec.proc.is_some() { "proc" } else { "scene" };
                seen.push(format!("[{:?},{:?},{:?}]", info.name, model, body));
            }
        }
        let _ = writeln!(
            file,
            "{{\"turn\":{},\"branch\":{:?},\"seen\":[{}]}}",
            turn.unwrap_or(0),
            format!("{:?}", self.branch),
            seen.join(",")
        );
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

    /// The graphics' side of the map: its environment's effects on or off,
    /// how many torches cast shadows (the renderer's side:
    /// `video::apply_graphics`).
    pub fn set_graphics(&mut self, g: &nh_world::Graphics) {
        use nh_world::Level;
        self.env.set_ssao_enabled(g.ssao != Level::Off);
        self.env.set_ssil_enabled(g.ssil != Level::Off);
        self.env.set_volumetric_fog_enabled(g.fog != Level::Off);
        self.torch_shadows = g.shadows.torches();
        // the pipelines the new settings draw with (another MSAA's, a pass
        // ambient occlusion adds), for every variant, not only those in
        // the scene now: built in the background, hidden
        if self.rehearsed && self.rehearsal.is_none() {
            self.variants.start(&mut self.root, false);
        }
    }

    /// The torches nearest the hero that cast shadows (self-tests).
    pub fn torch_shadows(&self) -> usize {
        self.torch_shadows
    }

    /// The effects the environment draws: ambient occlusion, indirect
    /// light, volumetric fog (self-tests).
    pub fn effects(&self) -> (bool, bool, bool) {
        (
            self.env.is_ssao_enabled(),
            self.env.is_ssil_enabled(),
            self.env.is_volumetric_fog_enabled(),
        )
    }

    /// Light, haze and grade the level's way (`level_look`, or its swamp's;
    /// the level's cells are drawn again by the caller when they differ).
    fn set_branch(&mut self, branch: Branch) {
        self.branch = branch;
        let l = if self.swamp {
            crate::branch_look::swamp(self.level_look)
        } else {
            self.level_look
        };
        self.branch_look = l;
        self.built_look = crate::branch_look::built(l);
        self.still_water();
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
        // the air's own faint glow, in its colour: what no light reaches
        // is dark, never black
        env.set_volumetric_fog_emission(l.fog);
        env.set_volumetric_fog_emission_energy(HAZE * l.haze);
        let g = l.grade;
        env.set_adjustment_saturation(g.saturation);
        env.set_adjustment_contrast(g.contrast);
        env.set_adjustment_brightness(g.brightness);
        env.set_adjustment_color_correction(&grade_curve(g.shadows, g.highlights));
        let mut rs = RenderingServer::singleton();
        rs.global_shader_parameter_set("branch_cracks", &l.cracks.to_variant());
        rs.global_shader_parameter_set("branch_crack_color", &l.crack_color.to_variant());
        let (way, color, energy) = match l.under {
            Under::Sky => SUN,
            Under::Deep => DEEP_LIGHT,
            Under::Rock => MOON,
            Under::Chasm => EMBER_LIGHT,
            Under::Stars => STAR_LIGHT,
        };
        self.moon.set_rotation_degrees(way);
        self.moon.set_color(color);
        self.moon.set_param(Param::ENERGY, energy);
        let (way, color, energy) = if l.under == Under::Chasm {
            EMBER_BACK
        } else {
            BACK
        };
        self.back.set_rotation_degrees(way);
        self.back.set_color(color);
        self.back.set_param(Param::ENERGY, energy);
        // the creatures' own light: a rim, or the branch's key
        let (way, color, energy) =
            l.creature_key
                .unwrap_or((CREATURE_LIGHT_WAY, RIM_LIGHT, CREATURE_LIGHT_ENERGY));
        self.creature_light.set_rotation_degrees(way);
        self.creature_light.set_color(color);
        self.creature_light.set_param(Param::ENERGY, energy);
        // the hero's own pool, as bright as the branch has it (a lamp in
        // hand keeps its own)
        if !self.hero_gear.lamp() {
            self.hero_light
                .set_param(Param::ENERGY, HERO_LIGHT_ENERGY * l.hero_light);
        }
        self.show_sky_sea(match l.under {
            Under::Sky => Some("res://shaders/sky_sea.gdshader"),
            Under::Stars => Some("res://shaders/stars.gdshader"),
            _ => None,
        });
        let chasm = if l.under == Under::Chasm {
            CHASM
        } else {
            Color::from_rgba(0.0, 0.0, 0.0, 0.0)
        };
        RenderingServer::singleton()
            .global_shader_parameter_set("branch_chasm", &chasm.to_variant());
        RenderingServer::singleton().global_shader_parameter_set(
            "branch_cave",
            &(if l.cave { 1.0f32 } else { 0.0 }).to_variant(),
        );
        self.vfx.set_dust(l.dust, l.embers);
        self.lights_dirty = true;
        self.bedrock_dirty = true;
        // (a prologue's scene keeps its own air)
        self.keep_prologue_air();
    }

    /// What lies far below the level, in `shader`: the Plane of Air's sea
    /// of clouds, the Astral Plane's stars (None: nothing). Made the first
    /// time it is shown, its material for each shader once.
    fn show_sky_sea(&mut self, shader: Option<&'static str>) {
        let Some(path) = shader else {
            if let Some(sea) = self.sky_sea.as_mut() {
                sea.set_visible(false);
            }
            return;
        };
        let mat = match self.sea_mats.iter().find(|(p, _)| *p == path) {
            Some((_, m)) => Some(m.clone()),
            None => godot::tools::try_load::<Shader>(path).ok().map(|shader| {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(&shader);
                let m = m.upcast::<Material>();
                self.sea_mats.push((path, m.clone()));
                m
            }),
        };
        let sea = self.sky_sea.get_or_insert_with(|| {
            let mut sea = MeshInstance3D::new_alloc();
            let mut quad = godot::classes::PlaneMesh::new_gd();
            quad.set_size(Vector2::new(SKY_SEA, SKY_SEA));
            sea.set_mesh(&quad);
            sea.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            // the middle of the level's field, far below it
            let mid = Vector3::new(COLNO as f32 / 2.0, -SKY_DROP, ROWNO as f32 / 2.0);
            sea.set_position(mid);
            self.root.add_child(&sea);
            sea
        });
        if let Some(m) = mat {
            sea.set_material_override(&m);
        }
        sea.set_visible(true);
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
        self.show_fade();
    }

    /// The 3D is all but black: a level arriving, or under the cover.
    pub fn dark(&self) -> bool {
        self.fade.max(self.cover) > 0.5
    }

    /// The black over the 3D: the new level's fade, or the cover.
    fn show_fade(&mut self) {
        if let Some(m) = self.post_mat.as_mut() {
            let shown = 1.0 - self.fade.max(self.cover);
            let f = 1.0 - shown * shown * (3.0 - 2.0 * shown);
            m.set_shader_parameter("fade", &f.to_variant());
        }
    }

    /// The cover over the 3D: down while the map plays its rehearsal
    /// behind the title page, lifted over the title scene once it is
    /// drawn (at once under the start-up veil), never over a game.
    fn uncover(&mut self, delta: f32) {
        let hidden = !self.shown && !self.titled;
        let target = if hidden { 1.0 } else { 0.0 };
        let next = if self.shown || hidden || self.veiled.unwrap_or(true) {
            target
        } else if self.building.is_empty() && self.fade <= 0.0 {
            (self.cover - delta.min(1.0 / 30.0) / TITLE_FADE_SECS).max(0.0)
        } else {
            self.cover
        };
        if next != self.cover {
            self.cover = next;
            self.show_fade();
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

    /// Splats of blood lying on the floor now (self-tests).
    pub fn splats_shown(&self) -> usize {
        self.vfx.splats_shown()
    }

    /// How far above the floor the hero floats now (self-tests).
    pub fn hero_lift(&self) -> f32 {
        self.hero_pose.lift
    }

    /// The settings' fewer screen effects: no sway, drift or hue.
    pub fn set_reduce_effects(&mut self, on: bool) {
        self.hero_look.reduced = on;
    }

    /// The hero's model wears the statues' own stone (self-tests).
    pub fn hero_in_stone(&self) -> bool {
        let Some(statue) = self.hero_look.statue() else {
            return false;
        };
        self.rim_model.as_ref().is_some_and(|n| {
            n.find_children_ex("*")
                .type_("GeometryInstance3D")
                .owned(false)
                .done()
                .iter_shared()
                .filter_map(|c| c.try_cast::<GeometryInstance3D>().ok())
                .any(|g| g.get_material_overlay().is_some_and(|m| m == statue))
        })
    }

    /// The hero's bearing under a load or from hunger, and their last
    /// step's pace (self-tests).
    pub fn hero_posture(&self) -> Posture {
        self.hero_look.posture()
    }

    /// The conditions' effects on the screen now (self-tests).
    pub fn hero_effects(&self) -> Effects {
        self.hero_look.effects(self.env.get_adjustment_brightness())
    }

    /// The conditions the hero's body shows, each with its share of the
    /// way, and whether their model wears the tint now (self-tests).
    pub fn hero_shown(&self, world: &World) -> (Vec<(Shown, f32)>, bool) {
        let tinted = self.rim_model.as_ref().is_some_and(|n| {
            n.find_children_ex("*")
                .type_("GeometryInstance3D")
                .owned(false)
                .done()
                .iter_shared()
                .filter_map(|c| c.try_cast::<GeometryInstance3D>().ok())
                .any(|g| g.get_material_overlay().is_some())
        });
        (self.hero_look.shown(world), tinted)
    }

    /// Whether a piece of a ray is drawn on the cell (self-tests).
    pub fn ray_drawn_at(&self, cell: (i32, i32)) -> bool {
        self.cells.get(&cell).is_some_and(|c| c.look.ray.is_some())
    }

    /// Whether a cell of an explosion is drawn on the cell (self-tests).
    pub fn blast_drawn_at(&self, cell: (i32, i32)) -> bool {
        self.cells
            .get(&cell)
            .is_some_and(|c| c.look.blast.is_some())
    }

    /// Seconds since the last explosion, and how long the newest tower of
    /// flame has stood (self-tests).
    pub fn since_blast(&self) -> (f32, Option<f32>) {
        self.vfx.since_blast()
    }

    /// Rays showing now, and how long the newest has (self-tests).
    pub fn rays_shown(&self) -> (usize, Option<f32>) {
        self.vfx.rays_shown()
    }

    /// Whether an effect still shows (self-tests).
    pub fn effects_busy(&self) -> bool {
        self.vfx.busy()
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
        let dress = self.branch_look.dress;
        for area in areas.iter().filter(|a| a.len() >= 4) {
            sconces.extend(torch_walls(
                area,
                |x, y| self.cells.get(&(x, y)).is_some_and(|n| n.look.wall),
                |&((x, y), into)| {
                    dressing::hangs(x, y, into, &dress, self.volume.get(&(x, y)).copied())
                },
            ));
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
        self.place_lava_lights();
    }

    /// In a cave branch, the walls built: those a door is set in or joined
    /// to one through walls (Minetown's shops and temple; the caves' own
    /// walls have no doors). A run of walls that joins up with more than
    /// `BUILT_MOST` is the cave's (a building against the rock). The walls
    /// whose look that changed.
    fn refresh_built(&mut self, world: &World, catalog: &Catalog) -> Vec<(i32, i32)> {
        let built = if self.level_look.cave {
            built_walls(
                |x, y| world.map.cell(x, y).and_then(|c| cell_terrain(c, catalog)),
                BUILT_MOST,
            )
        } else {
            HashSet::new()
        };
        let changed = built.symmetric_difference(&self.built).copied().collect();
        self.built = built;
        changed
    }

    /// The level's plan of arches and inlays as the map shows it now
    /// (`dressing::plan`); the cells whose part in it changed.
    fn refresh_volume(&mut self, world: &World, catalog: &Catalog) -> Vec<(i32, i32)> {
        let plan = dressing::plan(
            |x, y| plan_cell(&world.map, catalog, x, y),
            &self.level_look.dress,
        );
        let changed = plan
            .iter()
            .filter(|(k, v)| self.volume.get(k) != Some(v))
            .map(|(&k, _)| k)
            .chain(
                self.volume
                    .keys()
                    .filter(|k| !plan.contains_key(k))
                    .copied(),
            )
            .collect();
        self.volume = plan;
        changed
    }

    /// Lava is a light: one over the middle of each pool of it known, and
    /// over a big one one every `LAVA_SPACING` cells, lighting its walls
    /// and banks; at most `LAVA_LIGHTS`, the pools' middles first.
    fn place_lava_lights(&mut self) {
        let lava: HashSet<(i32, i32)> = self
            .cells
            .iter()
            .filter(|(_, n)| n.look.kind == Ground::Lava)
            .map(|(&k, _)| k)
            .collect();
        let mut spots: Vec<(usize, (i32, i32))> = Vec::new();
        for pool in lit_areas(&lava) {
            let n = pool.len() as f32;
            let (sx, sy) = pool
                .iter()
                .fold((0.0, 0.0), |(a, b), &(x, y)| (a + x as f32, b + y as f32));
            let mid = (sx / n, sy / n);
            let mut cells = pool;
            let off =
                |&(x, y): &(i32, i32)| (x as f32 - mid.0).powi(2) + (y as f32 - mid.1).powi(2);
            cells.sort_by(|a, b| off(a).total_cmp(&off(b)));
            let mut chosen: Vec<(i32, i32)> = Vec::new();
            for c in cells {
                let apart = |o: &(i32, i32)| (o.0 - c.0).pow(2) + (o.1 - c.1).pow(2);
                if chosen
                    .iter()
                    .all(|o| apart(o) as f32 >= LAVA_SPACING * LAVA_SPACING)
                {
                    chosen.push(c);
                }
            }
            spots.extend(chosen.into_iter().enumerate());
        }
        // (each pool's middle before any pool's second light)
        spots.sort_by_key(|&(rank, _)| rank);
        spots.truncate(LAVA_LIGHTS);
        for (i, &(_, (x, y))) in spots.iter().enumerate() {
            if i == self.lava_lights.len() {
                let mut l = OmniLight3D::new_alloc();
                l.set_color(LAVA_LIGHT);
                l.set_shadow(false);
                l.set_param(Param::ENERGY, LAVA_ENERGY);
                l.set_param(Param::RANGE, LAVA_RANGE);
                l.set_param(Param::ATTENUATION, 1.0);
                // the haze glows over it a little
                l.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.6);
                self.root.add_child(&l);
                self.lava_lights.push(l);
            }
            let l = &mut self.lava_lights[i];
            l.set_position(Vector3::new(x as f32, 0.6, y as f32));
            l.set_visible(true);
        }
        for l in self.lava_lights.iter_mut().skip(spots.len()) {
            l.set_visible(false);
        }
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
            // on the wall's face towards the room, or on a column's there
            let column = dx == 0
                && dressing::column_on(
                    x,
                    y,
                    &self.branch_look.dress,
                    self.volume.get(&(x, y)).copied(),
                );
            let out = 0.5 + if column { dressing::REACH } else { 0.0 };
            let face = Vector3::new(x as f32 + fx * out, 0.0, y as f32 + fz * out);
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
            // on a side wall it keeps to the wall
            let reach = if dx != 0 { SIDE_REACH } else { 1.0 };
            if let Some(sconce) = t.sconce.as_mut() {
                sconce.set_scale(Vector3::new(
                    SCONCE_SCALE,
                    SCONCE_SCALE,
                    SCONCE_SCALE * reach,
                ));
            }
            t.flame.set_position(at(0.0, 0.46, 0.3 * reach));
            t.halo.set_position(at(0.0, 0.62, 0.32 * reach));
            t.embers.set_position(at(0.0, 0.58, 0.3 * reach));
            // at the flame, a little out from it
            let out = 0.15 + 0.3 * reach;
            t.at = face + Vector3::new(fx * out, 2.0, fz * out);
            t.phase = f64::from(cell_noise(x, y, 13)) * 10.0;
            t.flame
                .set_instance_shader_parameter("phase", &(t.phase as f32).to_variant());
            t.light.set_position(t.at);
            t.light
                .set_param(Param::RANGE, self.branch_look.torch_range);
            t.light.set_visible(true);
            let lanterns = self.branch_look.lanterns;
            t.fire.set_visible(!lanterns);
            if lanterns && t.lantern.is_none() {
                self.hang_lantern(i);
            }
            let t = &mut self.torches[i];
            if let Some(l) = t.lantern.as_mut() {
                l.set_visible(lanterns);
                // smaller and closer on a side wall, hung as high
                let k = if dx != 0 { SIDE_LANTERN } else { 1.0 };
                l.set_scale(Vector3::new(k, k, k));
                l.set_position(at(0.0, LANTERN_HANG * (1.0 - k), 0.0));
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
            l.set_position(at(0.0, LANTERN_HANG, LANTERN_OUT));
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
                core.set_position(at(x, 0.55, LANTERN_OUT + z));
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
        let mut sconce = self
            .torch_scene
            .as_ref()
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok());
        if let Some(sconce) = sconce.as_mut() {
            sconce.set_scale(Vector3::new(SCONCE_SCALE, SCONCE_SCALE, SCONCE_SCALE));
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
            fire.add_child(&*sconce);
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
            sconce,
            flame,
            halo,
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
        // on the title only the nearest: it runs as long as the menu is read
        let shadows = if self.titled && !self.shown {
            TITLE_TORCH_SHADOWS.min(self.torch_shadows)
        } else {
            self.torch_shadows
        };
        let shadowed: HashSet<usize> = near.iter().take(shadows).map(|&(_, i)| i).collect();
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

    /// The hero's model (and only it) takes the rim light: `hero` is the
    /// cell the map draws them on. A hero not drawn (invisible, hiding)
    /// leaves the cell they were last known on to what else is there, a
    /// thing on the floor: its model is not theirs.
    fn rim_hero(&mut self, hero: Option<(i32, i32)>) {
        // on a steed their own model is the rider's, not the cell's
        let node = match &self.rider {
            Some(r) => Some(r.model.node.clone()),
            None => hero
                .and_then(|h| self.cells.get(&h))
                .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)))
                .map(|m| m.node.clone()),
        };
        if node.as_ref().map(|n| n.instance_id())
            == self.rim_model.as_ref().map(|n| n.instance_id())
        {
            return;
        }
        // a model given back to the pool may be on its way out: leave it be
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

    /// The hero's ring round their form: a polymorphed hero's model is
    /// measured where it meets the floor, once a model; as deep as the
    /// hero stands high (`ground`: `RING_DEPTH`).
    fn size_hero_ring(&mut self, form: bool, ground: f32) {
        let node = self
            .hero_model()
            .filter(|m| form && self.rider.is_none() && !m.is_pending())
            .map(|m| m.node.clone());
        let id = node.as_ref().map(|n| n.instance_id());
        if self.ring_sized == Some((id, ground)) {
            return;
        }
        self.ring_sized = Some((id, ground));
        let inner = node.and_then(|n| n.get_child(0)?.try_cast::<Node3D>().ok());
        let size = match inner.and_then(|n| Some((crate::art::footing(&n, FORM_FOOTING)?, n))) {
            Some((foot, n)) => {
                let s = n.get_scale();
                let narrower = (foot.size.x * s.x).min(foot.size.z * s.z);
                let across = (narrower / 2.0 + FORM_RING_OUT) * 2.0 / RING_CIRCLE;
                across.clamp(HERO_RING_SIZE, FORM_RING_MOST)
            }
            None => HERO_RING_SIZE,
        };
        let depth = if ground > RAISED {
            2.0 * ground + RING_DEPTH
        } else {
            RING_DEPTH
        };
        mark_size(&mut self.hero_ring, Vector3::new(size, depth, size));
    }

    /// The hero seen through what hides them from the camera: a wall, a
    /// door or rock between the eye and their chest, or a creature as tall
    /// as a giant standing there (in the cell south of them).
    fn show_through(&mut self, hero: (i32, i32), at: Vector3, world: &World, catalog: &Catalog) {
        let up = self
            .hero_stands()
            .map_or(CHEST, |(_, tall)| (CHEST * tall / HUMAN_TALL).min(CHEST));
        let chest = at + Vector3::new(0.0, up, 0.0);
        let walls = hides(chest, self.eye, hero, |x, y| {
            self.cells
                .get(&(x, y))
                .filter(|n| n.look.ground >= 1.0)
                .map(|n| n.look.top)
        });
        let hidden = walls || self.stands_before(chest, hero);
        if hidden != self.xray_on
            && let Some(n) = self.rim_model.as_ref().filter(|n| n.is_instance_valid())
        {
            // seen again: the tint their conditions show comes back
            let tint = self.hero_look.overlay();
            set_overlay(
                n,
                if hidden {
                    self.xray.as_ref()
                } else {
                    tint.as_ref()
                },
            );
            self.xray_on = hidden;
        }
        // the steed under them shows through with them
        let steed = self
            .rider
            .as_ref()
            .filter(|_| hidden)
            .and_then(|r| r.on.as_ref())
            .map(|(node, _)| node.clone());
        if steed.as_ref().map(|n| n.instance_id())
            != self.xray_steed.as_ref().map(|n| n.instance_id())
        {
            let alive = |n: &Gd<Node3D>| n.is_instance_valid() && !n.is_queued_for_deletion();
            if let Some(old) = self.xray_steed.take().filter(alive) {
                set_overlay(&old, None);
            }
            if let Some(n) = steed.as_ref().filter(|n| alive(n)) {
                set_overlay(n, self.xray.as_ref());
            }
            self.xray_steed = steed;
        }
        // and the other way: a rat behind them is not lost behind their
        // back (a foe there had the threat arrow over the hero's own head),
        // nor anything behind a body that overhangs its cell
        let mut behind = self.hidden_behind(hero);
        let (hidden, faded) = self.hidden_by_big(world, catalog);
        for (n, hostile) in hidden {
            if !behind
                .iter()
                .any(|(b, _)| b.instance_id() == n.instance_id())
            {
                behind.push((n, hostile));
            }
        }
        self.fade_big(faded);
        let key = |v: &[(Gd<Node3D>, bool)]| -> Vec<(InstanceId, bool)> {
            v.iter().map(|(n, h)| (n.instance_id(), *h)).collect()
        };
        if key(&behind) != key(&self.xray_behind) {
            self.unshow_behind(&behind);
            if let Some((foe, pet)) = self.xray_creature.as_ref() {
                for (n, hostile) in &behind {
                    // under the pointer too: its outline would not show
                    // through the hero, the x-ray does
                    if self
                        .outlined
                        .as_ref()
                        .is_some_and(|o| o.instance_id() == n.instance_id())
                    {
                        self.outlined = None;
                    }
                    set_overlay(n, Some(if *hostile { foe } else { pet }));
                }
            }
            self.xray_behind = behind;
        }
    }

    /// The creatures seen through the hero no longer (not in `keep`) are
    /// drawn as ever: those still in the x-ray (one that died there wears
    /// its fall's shade now).
    fn unshow_behind(&mut self, keep: &[(Gd<Node3D>, bool)]) {
        let ours: Vec<InstanceId> = self
            .xray_creature
            .iter()
            .flat_map(|(foe, pet)| [foe.instance_id(), pet.instance_id()])
            .collect();
        let alive = |n: &Gd<Node3D>| n.is_instance_valid() && !n.is_queued_for_deletion();
        for (old, _) in std::mem::take(&mut self.xray_behind) {
            let kept = keep
                .iter()
                .any(|(n, _)| n.instance_id() == old.instance_id());
            if !kept && alive(&old) && wears(&old, &ours) {
                set_overlay(&old, None);
            }
        }
    }

    /// The small creatures in the cells north of the hero (on `own`) that
    /// their body hides from the camera, the whole or a part (the x-ray
    /// draws only what is hidden), and whether each is hostile.
    fn hidden_behind(&self, own: (i32, i32)) -> Vec<(Gd<Node3D>, bool)> {
        let Some((feet, tall)) = self.hero_stands() else {
            return Vec::new();
        };
        let body = Aabb::new(
            feet - Vector3::new(BODY_HALF_WIDTH, 0.0, BODY_HALF_DEPTH),
            Vector3::new(2.0 * BODY_HALF_WIDTH, tall, 2.0 * BODY_HALF_DEPTH),
        );
        let mut hidden = Vec::new();
        for dy in [-1, -2] {
            for dx in -1..=1 {
                let Some(n) = self.cells.get(&(own.0 + dx, own.1 + dy)) else {
                    continue;
                };
                let Some(i) = n.look.entity.filter(|_| !n.look.hero) else {
                    continue;
                };
                let living = n
                    .look
                    .models
                    .get(i)
                    .is_some_and(|p| matches!(p.look.pose, Pose::Alive | Pose::Ghost));
                let Some(b) = n
                    .models
                    .get(i)
                    .filter(|_| living)
                    .and_then(|m| Some((m, world_box(&m.node)?)))
                else {
                    continue;
                };
                let (m, b) = b;
                if b.size.y > SMALL * tall {
                    continue;
                }
                // its middle, and its end nearest the hero (a rat's back
                // may show over their head while its snout is behind them)
                let c = b.center();
                let near = Vector3::new(c.x, c.y, b.end().z);
                if [c, near]
                    .iter()
                    .any(|p| body.intersects_segment(*p, self.eye))
                {
                    hidden.push((m.node.clone(), n.look.hostile));
                }
            }
        }
        hidden
    }

    /// The creatures on the cells round a huge or gigantic one (a body
    /// that overhangs its cell: behind it, and beside it under its wings)
    /// that it hides from the camera, the whole or a part, and whether
    /// each is hostile: smaller than it, not the hero (`stands_before`
    /// sees them through it); and the huge ones that hide an object or a
    /// trap the hero knows of there (drawn see-through: a trap is no model
    /// of its own to show through).
    fn hidden_by_big(&self, world: &World, catalog: &Catalog) -> (Vec<Behind>, Vec<Gd<Node3D>>) {
        // an object or a trap the map shows there
        let known = |x: i32, y: i32| {
            world
                .map
                .cell(x, y)
                .and_then(|c| c.glyph.as_ref())
                .is_some_and(|g| match g.kind {
                    GlyphKind::Obj | GlyphKind::Body | GlyphKind::Statue => true,
                    GlyphKind::Cmap => {
                        cmap_sym(g, catalog).is_some_and(|s| terrain_of(s) == Terrain::Trap)
                    }
                    _ => false,
                })
        };
        let mut faded: Vec<Gd<Node3D>> = Vec::new();
        let living = |n: &CellNodes, i: usize| {
            n.look
                .models
                .get(i)
                .is_some_and(|p| matches!(p.look.pose, Pose::Alive | Pose::Ghost))
        };
        let mut hidden: Vec<Behind> = Vec::new();
        for (&(x, y), n) in &self.cells {
            let Some(i) = n.look.entity.filter(|&i| !n.look.hero && living(n, i)) else {
                continue;
            };
            let big = n.look.models[i].look.art.cap > nh_art::MAX_FOOTPRINT;
            let Some(body) = n
                .models
                .get(i)
                .filter(|_| big)
                .and_then(|m| world_box(&m.node))
            else {
                continue;
            };
            // the cells behind it, and beside and before it under its wings
            for dy in -3..=1 {
                for dx in -2..=2 {
                    if (dx, dy) == (0, 0) {
                        continue;
                    }
                    let Some(o) = self.cells.get(&(x + dx, y + dy)) else {
                        continue;
                    };
                    // its middle on the ground and its near edge
                    let (ox, oz) = ((x + dx) as f32, (y + dy) as f32);
                    let spot = [
                        Vector3::new(ox, o.look.ground + 0.1, oz),
                        Vector3::new(ox, o.look.ground + 0.1, oz + 0.4),
                    ];
                    let own = n.models[i].node.instance_id();
                    if known(x + dx, y + dy)
                        && spot.iter().any(|p| body.intersects_segment(*p, self.eye))
                        && !faded.iter().any(|f| f.instance_id() == own)
                    {
                        faded.push(n.models[i].node.clone());
                    }
                    let Some(j) = o.look.entity.filter(|&j| !o.look.hero && living(o, j)) else {
                        continue;
                    };
                    let Some((m, b)) = o.models.get(j).and_then(|m| Some((m, world_box(&m.node)?)))
                    else {
                        continue;
                    };
                    if b.size.y >= body.size.y
                        || hidden
                            .iter()
                            .any(|(h, _)| h.instance_id() == m.node.instance_id())
                    {
                        continue;
                    }
                    let c = b.center();
                    let near = Vector3::new(c.x, c.y, b.end().z);
                    if [c, near]
                        .iter()
                        .any(|p| body.intersects_segment(*p, self.eye))
                    {
                        hidden.push((m.node.clone(), o.look.hostile));
                    }
                }
            }
        }
        (hidden, faded)
    }

    /// The huge creatures in `now` drawn see-through, those no longer
    /// as they were (a model given back to the pool is never another's
    /// faded; one that falls meanwhile, its fall fades again).
    fn fade_big(&mut self, now: Vec<Gd<Node3D>>) {
        let key =
            |v: &[Gd<Node3D>]| -> Vec<InstanceId> { v.iter().map(|n| n.instance_id()).collect() };
        if key(&now) == key(&self.big_faded) {
            return;
        }
        let see = |node: &Gd<Node3D>, k: f32| {
            for mut g in node
                .find_children_ex("*")
                .type_("GeometryInstance3D")
                .owned(false)
                .done()
                .iter_shared()
                .filter_map(|g| g.try_cast::<GeometryInstance3D>().ok())
            {
                g.set_transparency(k);
            }
        };
        for old in std::mem::take(&mut self.big_faded) {
            let kept = now.iter().any(|n| n.instance_id() == old.instance_id());
            if !kept && old.is_instance_valid() {
                see(&old, 0.0);
            }
        }
        for n in now.iter().filter(|n| n.is_instance_valid()) {
            see(n, BIG_FADE);
        }
        self.big_faded = now;
    }

    /// The huge and gigantic creatures drawn see-through now (self-tests).
    pub fn big_faded(&self) -> usize {
        self.big_faded.len()
    }

    /// A model on the cells the line from the hero's `chest` to the eye
    /// crosses, or beside them (a broad body reaches over), that the line
    /// goes through: what stands on the hero's own cell is not before them.
    fn stands_before(&self, chest: Vector3, own: (i32, i32)) -> bool {
        let span = self.eye - chest;
        let steps = (span.length() / 0.1).ceil().max(1.0) as i32;
        let hero = self.rim_model.as_ref().map(|n| n.instance_id());
        let mut looked: Vec<(i32, i32)> = Vec::new();
        for i in 1..steps {
            let p = chest + span * (i as f32 / steps as f32);
            // higher than any creature: nothing further on reaches the line
            if p.y > TALLEST {
                break;
            }
            let (x, y) = (p.x.round() as i32, p.z.round() as i32);
            // (a body overhanging its cell reaches over from two away)
            for cell in (-2..=2).map(|dx| (x + dx, y)) {
                if cell == own || looked.contains(&cell) {
                    continue;
                }
                looked.push(cell);
                let Some(n) = self.cells.get(&cell) else {
                    continue;
                };
                let before = n
                    .models
                    .iter()
                    .filter(|m| Some(m.node.instance_id()) != hero)
                    .filter_map(|m| world_box(&m.node))
                    .any(|b| b.intersects_segment(chest, self.eye));
                if before {
                    return true;
                }
            }
        }
        false
    }

    /// The hero is drawn through what hides them now (self-tests).
    pub fn hero_seen_through(&self) -> bool {
        self.xray_on
    }

    /// The inside of what engulfs the hero, drawn round them (self-tests).
    pub fn engulfed_inside(&self) -> Option<Inside> {
        self.engulfed.shown()
    }

    /// The creature on the cell is drawn through the hero who hides it,
    /// and in a hostile's colour (self-tests).
    pub fn seen_through_hero(&self, x: i32, y: i32) -> Option<bool> {
        let n = self.cells.get(&(x, y))?;
        let m = n.models.get(n.look.entity?)?;
        self.xray_behind
            .iter()
            .find(|(b, _)| b.instance_id() == m.node.instance_id())
            .map(|(_, hostile)| *hostile)
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
            })
            // one the hero hides wears its x-ray instead
            .filter(|(node, _)| {
                !self
                    .xray_behind
                    .iter()
                    .any(|(n, _)| n.instance_id() == node.instance_id())
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

    /// How much foam laps at the water's shores: none on a swamp (white,
    /// it ringed each of its still pools); the sea's and a moat's keep it.
    fn foam_amount(&self) -> f32 {
        if self.swamp { 0.0 } else { 1.0 }
    }

    /// The water's foam as the level is now (a swamp seen, or not).
    fn still_water(&mut self) {
        let foam = self.foam_amount();
        if let Some(mut m) = self
            .liquids
            .get(&Liquid::Water)
            .and_then(|m| m.clone().try_cast::<ShaderMaterial>().ok())
        {
            m.set_shader_parameter("foam_amount", &foam.to_variant());
        }
    }

    /// The material of water, lava or the air over lava (made once).
    fn liquid(&mut self, l: Liquid) -> Gd<Material> {
        if let Some(m) = self.liquids.get(&l) {
            return m.clone();
        }
        let path = match l {
            Liquid::Water => "res://shaders/water.gdshader",
            Liquid::Lava => "res://shaders/lava.gdshader",
            Liquid::Haze => "res://shaders/heat_haze.gdshader",
            Liquid::Unseen => "res://shaders/unseen.gdshader",
            Liquid::Bubble => "res://shaders/bubble.gdshader",
            Liquid::Cloud => "res://shaders/cloud.gdshader",
            Liquid::Deep => "res://shaders/deep.gdshader",
            Liquid::Membrane => "res://shaders/membrane.gdshader",
        };
        let mat: Gd<Material> = match godot::tools::try_load::<Shader>(path) {
            Ok(shader) => {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(&shader);
                m.set_shader_parameter("noise_tex", &self.surfaces.noise().to_variant());
                m.set_shader_parameter("fow_tex", &self.surfaces.fow_texture().to_variant());
                if matches!(
                    l,
                    Liquid::Water | Liquid::Lava | Liquid::Bubble | Liquid::Membrane
                ) {
                    let ground = self.surfaces.ground_texture();
                    m.set_shader_parameter("ground_tex", &ground.to_variant());
                }
                if l == Liquid::Membrane {
                    m.set_shader_parameter("tall", &MEMBRANE_TALL.to_variant());
                }
                if l == Liquid::Water {
                    m.set_shader_parameter("foam_amount", &self.foam_amount().to_variant());
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
                    Liquid::Lava => Color::from_rgb(1.0, 0.35, 0.05),
                    Liquid::Haze => Color::from_rgba(1.0, 1.0, 1.0, 0.0),
                    Liquid::Unseen => Color::from_rgba(0.7, 0.72, 0.8, 0.3),
                    Liquid::Bubble => Color::from_rgba(0.4, 0.65, 0.8, 0.35),
                    Liquid::Cloud => Color::from_rgba(0.9, 0.93, 1.0, 0.7),
                    Liquid::Deep => Color::from_rgb(0.01, 0.05, 0.1),
                    Liquid::Membrane => Color::from_rgba(0.7, 0.85, 1.0, 0.25),
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
    /// How high the bedrock lies: the deep lower (a mark's sheet under a
    /// bubble's floor shows over it).
    fn bedrock_y(&self) -> f32 {
        if self.branch_look.under == Under::Deep {
            DEEP_BEDROCK
        } else {
            -0.02
        }
    }

    /// The hero's ring is drawn where no ground takes it (the Planes of
    /// Air and Water): shown, on its sheet, with nothing of the level's
    /// own under the cell over the sheet (self-tests).
    pub fn hero_ring_drawn(&self) -> bool {
        let Some((sheet, _)) = mark_sheet(&self.hero_ring) else {
            return false;
        };
        let clear = !self.bedrock.is_visible() || sheet.get_global_position().y > self.bedrock_y();
        self.hero_ring.is_visible() && sheet.is_visible_in_tree() && clear
    }

    fn lay_bedrock(&mut self) {
        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        st.set_normal(Vector3::UP);
        let y = self.bedrock_y();
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
        // the branch's own rock (over Gehennom's chasm too: what is far
        // from anything known smoulders in it); on the Plane of Air none,
        // the sky; on the Plane of Water the deep
        self.bedrock
            .set_visible(!matches!(self.branch_look.under, Under::Sky | Under::Stars));
        if self.branch_look.under == Under::Deep {
            let mat = self.material(Paint::Liquid(Liquid::Deep));
            self.bedrock.set_material_override(&mat);
            return;
        }
        let name = self.branch_look.material("bedrock");
        if let Some(m) = self.art.manifest().material(name) {
            let mat = self.material(Paint::Pbr(m, SHADE_BEDROCK, Role::Void));
            self.bedrock.set_material_override(&mat);
        }
    }

    /// The map cell under a screen position: the first wall, door, creature
    /// or letter the camera ray passes through, else where it meets the ground.
    /// Where a point of the world is on screen (None: behind the camera)
    /// (self-tests).
    pub fn on_screen(&self, at: Vector3) -> Option<Vector2> {
        (!self.camera.is_position_behind(at)).then(|| self.camera.unproject_position(at))
    }

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

    /// Does no wall or rock stand between the camera and the ground in
    /// the middle of a cell, where the feet of whoever is there are
    /// (self-tests: nobody in a corridor is hidden)? On the screen or off
    /// to its side: the eye's own line to the cell.
    pub fn feet_in_view(&self, (x, y): (i32, i32)) -> bool {
        if !self.camera.is_inside_tree() {
            return false;
        }
        let origin = self.camera.get_global_position();
        let feet = Vector3::new(x as f32, 0.0, y as f32);
        // what stands on the ground (a creature nearer the camera) hides
        // no more than it would in a room
        let hit = pick_cell(origin, (feet - origin).normalized(), |cx, cy| {
            if (cx, cy) == (x, y) {
                0.0
            } else {
                self.ground(cx, cy)
            }
        });
        hit == Some((x, y))
    }

    /// How far the models on a cell reach beyond the cell's own square, in
    /// metres (0: all within it; self-tests: a thing on the floor lies on
    /// its own cell, not across its neighbours').
    pub fn overhang(&self, (x, y): (i32, i32)) -> f32 {
        let Some(nodes) = self.cells.get(&(x, y)) else {
            return 0.0;
        };
        nodes
            .models
            .iter()
            .flat_map(|m| crate::icon_bake::corners(&m.node))
            .map(|p| (p.x - x as f32).abs().max((p.z - y as f32).abs()) - 0.5)
            .fold(0.0, f32::max)
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

    /// The whole level framed, its air clears: the haze, the dark past the
    /// frame and the vignette go, a little light more falls everywhere,
    /// and what is remembered shows nearly as it is (map_surface's
    /// `overview`), over `OVERVIEW_EASE` seconds; back as it was when the
    /// camera follows the hero again.
    fn ease_overview(&mut self, delta: f32) {
        let want = if self.overview.is_some() { 1.0 } else { 0.0 };
        if self.overview_k == want && want == 0.0 {
            return;
        }
        let step = delta.min(1.0 / 30.0) / OVERVIEW_EASE;
        self.overview_k = if want > self.overview_k {
            (self.overview_k + step).min(want)
        } else {
            (self.overview_k - step).max(want)
        };
        let k = self.overview_k * self.overview_k * (3.0 - 2.0 * self.overview_k);
        // (a level changing under the overview sets the branch's again)
        let l = self.branch_look;
        self.air_near = self.air_near();
        self.keep_prologue_air();
        if !self.showcase {
            // a lit hall far from its torches, as plain as the rest
            let ambient = l.ambient_energy + OVERVIEW_AMBIENT * k;
            self.env.set_ambient_light_energy(ambient);
        }
        if let Some(m) = self.post_mat.as_mut() {
            m.set_shader_parameter("strength", &(VIGNETTE * (1.0 - k)).to_variant());
        }
        RenderingServer::singleton().global_shader_parameter_set("overview", &k.to_variant());
    }

    /// How much of the branch's haze the air holds: as much as the eye
    /// looks through at the default distance (the more the farther it
    /// is, `AIR_LEAST` at the least), none in the overview's clear air.
    fn air_near(&self) -> f32 {
        let k = self.overview_k * self.overview_k * (3.0 - 2.0 * self.overview_k);
        let far = (DISTANCE / self.camera_distance().max(1.0)).clamp(AIR_LEAST, 1.0);
        far * (1.0 - k)
    }

    /// The whole-level view is on (self-tests).
    pub fn in_overview(&self) -> bool {
        self.overview.is_some()
    }

    /// The overview framed, its air eased in all the way (self-tests).
    pub fn overview_shown(&self) -> bool {
        self.overview.is_some() && self.overview_k >= 1.0 && self.is_settled()
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

    /// Models built since the game started (the frame log).
    pub fn models_built(&self) -> usize {
        self.art.counts().1
    }

    /// The models drawn for the first time since the last call (the frame
    /// log).
    pub fn take_firsts(&mut self) -> Vec<String> {
        self.art.take_firsts()
    }

    /// Forget every cell (a new game).
    /// The next few of one of each pipeline variant a game draws, under the
    /// map (hidden; the canvas items under the veil): their pipelines are
    /// built from the next frame on (`variants.rs`).
    fn make_variants(&mut self) {
        if self.variants.step() || self.variants.made().0 == 0 {
            return;
        }
        let (made, unmade) = self.variants.made();
        if self.stats_window.is_some() {
            godot_print!(
                "map: {made} pipeline variants made {:.1} s after start",
                godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0
            );
        }
        for line in unmade {
            godot_warn!("renethack: a pipeline variant not made: {line}");
        }
    }

    /// The variants are being made (again): Godot builds their pipelines
    /// as they come.
    pub fn variants_busy(&self) -> bool {
        !self.variants.done()
    }

    /// The pipeline variants made, and those that could not be (self-tests).
    pub fn variants_made(&self) -> (usize, usize) {
        let (made, unmade) = self.variants.made();
        (made, unmade.len())
    }

    pub fn clear(&mut self) {
        // (no model goes back to the pool see-through)
        self.fade_big(Vec::new());
        // a game starts: the rehearsal is over, the title scene gone, the
        // gallery's light off, nothing of a prologue's scene left
        self.rehearsal = None;
        // those of the variants still to make are made in the game's first
        // frames (`tick`), then let go: a game started before the rehearsal
        // was over (the veil lifts at its cap) meets them made
        // (a later launch lays its title under the veil, the variants'
        // canvas items still to make)
        if !self.quick || !self.veiled.unwrap_or(true) {
            self.variants.release_canvas();
        }
        if self.variants.done() {
            self.variants.release();
        }
        self.end_stage();
        self.title = None;
        if std::mem::take(&mut self.titled) {
            self.env.set_bg_color(self.branch_look.darkness);
            self.env.set_fog_light_color(self.branch_look.darkness);
            self.keep_prologue_air();
        }
        for (node, light, _, _) in &mut self.title_fires {
            node.set_visible(false);
            light.set_visible(false);
        }
        self.build_budget = BUILD_BUDGET;
        self.set_showcase(false);
        self.building.clear();
        self.hero_pose.forget();
        self.hero_look.forget();
        self.finish_motions();
        self.finish_falls_where(|_| true);
        self.forget_drops();
        self.forget_gone();
        self.deaths.clear();
        self.act_fx.clear();
        self.arrivals.clear();
        self.appeared.clear();
        self.summons.clear();
        self.rays_waiting.clear();
        self.to_arm.clear();
        self.sounds.clear();
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
        self.hero_studio.set_visible(true);
        if let Some(old) = self.rim_model.take().filter(|n| n.is_instance_valid()) {
            set_layers(&old, 1);
            set_overlay(&old, None);
        }
        if let Some(old) = self.xray_steed.take().filter(|n| n.is_instance_valid()) {
            set_overlay(&old, None);
        }
        if let Some(old) = self.lit_steed.take().filter(|n| n.is_instance_valid()) {
            set_layers(&old, 1);
        }
        self.unshow_behind(&[]);
        if let Some(r) = self.rider.take() {
            self.art.give(r.model);
        }
        for l in &mut self.room_lights {
            l.set_visible(false);
        }
        self.place_torches(&[]);
        self.holes.clear();
        self.bedrock_dirty = true;
        self.engulfed.show(None, Vector3::ZERO, 0.0);
        self.snap = true;
        self.hero_at = None;
        // a new hero faces the camera, not the way the last one turned
        self.hero_yaw = 0.0;
        self.set_path(&[], false);
        self.hero_gear.reset();
        self.hero_fx.clear();
    }

    /// The hero uses an item (spec decision 8): they turn the way it goes
    /// and play the use's clip with the item in hand; its effects follow.
    pub fn show_use(&mut self, u: &ItemUse, catalog: &Catalog, world: &World) {
        use crate::hero::{
            Body, Fx, THROW_AT, in_hand, use_color, use_effects, use_hand, use_name,
        };
        let Some(at) = self.hero_at else {
            return;
        };
        let tile = u.tile.and_then(|t| object_tile(catalog, t));
        let held = tile.and_then(|t| self.art.manifest().held(t));
        self.hear_use(u, at, held.map(|h| h.held));
        if u.kind == UseKind::Pray {
            self.show_prayer(u.prayer.unwrap_or(nh_world::PrayerOutcome::Unanswered));
            return;
        }
        let color = use_color(u, tile, self.art.manifest());
        let dir = u.dir.filter(|&d| d != (0, 0));
        // a thing applied as a weapon is wielded (a pick-axe digging, a
        // polearm striking a spot picked on the map)
        let applied = held
            .filter(|_| u.kind == UseKind::Apply)
            .and_then(|h| self.art.manifest().held_at(h.held).1.apply.clone());
        let spot = u
            .spot
            .filter(|_| applied.is_some())
            .map(|(x, y)| (x - at.0, y - at.1))
            .filter(|&d| d != (0, 0));
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
        // an unseen hero has no model to show the use with
        let Some(nodes) = self.cells.get_mut(&at).filter(|n| n.look.hero) else {
            return;
        };
        let Some(i) = nodes.look.entity.filter(|&i| i < nodes.models.len()) else {
            return;
        };
        let placed = &mut nodes.look.models[i];
        let yaw_from = placed.yaw;
        let yaw_to = dir.or(spot).map_or(yaw_from, |d| yaw_toward((0, 0), d));
        placed.yaw = yaw_to;
        let pos = Vector3::new(at.0 as f32, 0.0, at.1 as f32) + placed.pos;
        // the body the map turns, and the hero's own, whose hands use the
        // thing: in the saddle the steed's and the rider's (their hands
        // as high above where they would be on foot as the seat is above
        // their hips)
        let node = nodes.models[i].node.clone();
        let riding = self.rider.is_some();
        let (m, height, lift) = match self.rider.as_mut() {
            Some(r) => {
                let height = r.look.art.height;
                let seat = r.on.as_ref().and_then(|(_, mount)| mount.at());
                let lift = seat.map_or(0.0, |s| s.y - pos.y - height * HIPS_HEIGHT);
                (&mut r.model, height, lift)
            }
            None => (&mut nodes.models[i], placed.look.art.height, 0.0),
        };
        let body = Body {
            feet: pos + Vector3::new(0.0, lift, 0.0),
            tall: height,
            node: Some(m.node.clone()),
        };
        let mut clips = self.art.clips(m, false);
        // from the saddle, its own way where it has one (a lance couched)
        let mounted = applied
            .as_ref()
            .filter(|_| riding)
            .and_then(|a| a.mounted.clone());
        // (its clip, how long it lasts, where the off hand grips)
        let applied = applied.and_then(|a| {
            let name = match &mounted {
                Some(m) => m.clip.clone(),
                None => a.clip(u.vertical == Some('>')).to_string(),
            };
            let clip = self.art.clip_on(m, name)?;
            let secs = clips
                .player
                .as_ref()?
                .get_animation(clip.as_str())?
                .get_length();
            Some((clip, secs, a.two.filter(|_| mounted.is_none())))
        });
        let (hand_slot, secs, two) = match &applied {
            Some((_, secs, two)) => ("hand_r", Some(*secs), *two),
            None => (use_hand(u.kind), in_hand(u.kind), None),
        };
        clips.attack = match applied {
            Some((clip, ..)) => Some(clip),
            None => self.art.use_clip(m, use_name(u.kind)),
        };
        if let (Some(h), Some(secs)) = (held, secs) {
            self.art
                .hold_for(m, h, secs, hand_slot, two, mounted.as_ref());
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
        let hand = pos + Vector3::new(0.0, lift + height * 0.62, 0.0) + facing * 0.25;
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
        let lets_go = anchor.clone();
        for (delay, fx) in use_effects(u, color, hand, anchor, ahead, body) {
            self.hero_fx.after(delay, fx);
        }
        if matches!(u.kind, UseKind::Throw | UseKind::Fire) {
            // never smaller than a hand's breadth in flight: a gem of 6 cm
            // would vanish at the camera's distance
            let model = held.and_then(|h| self.art.held_prop(h, (FLIGHT_MIN / h.scale).max(0.7)));
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
            // where it comes down (a hit is the message's)
            let lands = Sound::new("missile.land").at(to).after(THROW_AT + 0.25);
            self.sounds.push(lands);
            self.hero_fx.after(
                THROW_AT,
                Fx::Throw {
                    model,
                    from: hand,
                    to,
                    anchor: lets_go,
                },
            );
        }
    }

    /// A use of the hero's, heard whether or not their model shows it: by
    /// what the thing looks like (`use.apply.pick`), the kind of use, any
    /// use; a throw from a launcher as a throw.
    fn hear_use(&mut self, u: &ItemUse, at: (i32, i32), held: Option<usize>) {
        let (cue, when) = hearing::use_cue(u.kind);
        let mut names = Vec::new();
        if let Some(h) = held {
            names.push(format!("{cue}.{}", self.art.manifest().held_at(h).0));
        }
        // a spell by its school
        if let Some(school) = u.spell.as_deref().and_then(hearing::school) {
            names.push(format!("{cue}.{school}"));
        }
        names.push(cue.to_string());
        if u.kind == UseKind::Fire {
            names.push("use.throw".to_string());
        }
        names.push("use".to_string());
        let sound = Sound::exactly(names).at(self.sound_at(at)).after(when);
        self.sounds.push(sound.urgent());
    }

    /// The effects of the hero's uses (self-tests).
    pub fn hero_fx(&self) -> &crate::hero::HeroFx {
        &self.hero_fx
    }

    /// The gear the hero's model shows.
    pub fn hero_gear_shown(&self) -> &nh_art::Gear {
        self.hero_gear.gear()
    }

    /// The way the hero faces, in degrees (self-tests).
    pub fn hero_yaw(&self) -> f32 {
        self.hero_yaw
    }

    /// The hero's model shows their gear; with a lamp lit in hand, the
    /// light over them is only the lamp's warm helper (no shadow of its own).
    fn equip_hero(&mut self, at: (i32, i32), world: &World, catalog: &Catalog, delta: f32) {
        self.hero_gear
            .update(&world.inventory, catalog, self.art.manifest());
        // their own model: the rider's on a steed (whose back holds them),
        // else their cell's, on foot
        let (m, mount, settle) = match self.rider.as_mut() {
            Some(r) => {
                let mount = r.on.as_ref().map(|(_, mount)| mount);
                // not seen climbing up: in the saddle at once
                let at_once = mount.is_some() && !std::mem::replace(&mut r.seated, true);
                let settle = if at_once { f32::MAX } else { delta };
                (Some(&mut r.model), mount, settle)
            }
            None => {
                let own = self
                    .cells
                    .get_mut(&at)
                    .filter(|n| n.look.hero && n.look.rider.is_none())
                    .and_then(|n| n.look.entity.and_then(|i| n.models.get_mut(i)));
                (own, None, delta)
            }
        };
        let Some(m) = m else {
            return;
        };
        self.art.sit(m, mount, settle);
        // shown whole behind the creation form, a shield let down at the
        // side rather than over the body
        let lamp = match &self.creation {
            Some(_) => {
                let mut gear = self.hero_gear.gear().clone();
                gear.idle = self.art.manifest().idle_shield_down(&gear);
                self.art.equip(m, &gear, delta)
            }
            None => self.art.equip(m, self.hero_gear.gear(), delta),
        };
        if self.hero_gear.set_lamp(lamp) {
            let (color, energy, range) = if lamp {
                (LAMP_POOL, LAMP_POOL_ENERGY, LAMP_POOL_RANGE)
            } else {
                let energy = HERO_LIGHT_ENERGY * self.branch_look.hero_light;
                (HERO_LIGHT, energy, HERO_LIGHT_RANGE)
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

    /// The monster or object on a cell: its model's index, the scale its
    /// look asks for and the scale its instance has (self-tests: a pooled
    /// instance comes back at its own size).
    /// How wide and how long each monster of the catalog lies on the floor
    /// as it is drawn: its name (with " (F)" for a woman of her own model),
    /// its model, the sides in metres and the model's scale.
    pub fn footprints(&mut self, catalog: &Catalog) -> Vec<(String, String, [f32; 2], f32)> {
        let mut out = Vec::new();
        for info in &catalog.monsters {
            let male = self.art.manifest().monster(info, 0);
            for flags in [0, mg::FEMALE] {
                let r = self.art.manifest().monster(info, flags);
                if flags != 0 && r.model == male.model {
                    continue;
                }
                let look = ModelLook {
                    art: r,
                    tint: Color::WHITE,
                    pose: Pose::Alive,
                };
                let name = match flags {
                    0 => info.name.clone(),
                    _ => format!("{} (F)", info.name),
                };
                let model = self.art.manifest().model_at(r.model).0.to_string();
                out.push((name, model, self.art.footprint(&look), r.scale));
            }
        }
        out
    }

    pub fn entity_scale(&self, x: i32, y: i32) -> Option<(usize, f32, f32)> {
        let nodes = self.cells.get(&(x, y))?;
        let i = nodes.look.entity?;
        let want = nodes.look.models.get(i)?.look.art.scale;
        let m = nodes.models.get(i)?;
        let inner = m.node.get_child(0)?.try_cast::<Node3D>().ok()?;
        // its height's: a stocky or a fitted body is another size across
        Some((m.model_index(), want, inner.get_scale().y))
    }

    /// The rings on the ground, and how far each lies from the model it is
    /// for, on the ground's plane: a pet's from the creature on its cell,
    /// the hero's from the hero's model (None: there is none) (self-tests:
    /// a ring is under its creature, also while that walks between cells).
    pub fn ring_gaps(&self) -> Vec<(&'static str, (i32, i32), Option<f32>)> {
        let flat = |a: Vector3, b: Vector3| Vector2::new(a.x - b.x, a.z - b.z).length();
        let place = |m: &Model| {
            (m.node.is_instance_valid() && m.node.is_inside_tree())
                .then(|| m.node.get_global_position())
        };
        let mut gaps: Vec<_> = self
            .cells
            .iter()
            .filter_map(|(c, n)| {
                let ring = n.ring.as_ref()?;
                let model = n.look.entity.and_then(|i| n.models.get(i)).and_then(place);
                Some((
                    "pet",
                    *c,
                    model.map(|p| flat(p, ring.get_global_position())),
                ))
            })
            .collect();
        gaps.sort_by_key(|g| g.1);
        if let Some(c) = self.hero_at.filter(|_| self.hero_ring.is_visible()) {
            let model = self.hero_model().and_then(place);
            let gap = model.map(|p| flat(p, self.hero_ring.get_global_position()));
            gaps.push(("hero", c, gap));
        }
        gaps
    }

    /// The hero's cell, when the hero's own model is on it. A hero the map
    /// does not draw (invisible, hiding) leaves their cell to what else is
    /// there, a thing on the floor: its model is not theirs.
    fn hero_cell(&self) -> Option<&CellNodes> {
        self.cells.get(&self.hero_at?).filter(|n| n.look.hero)
    }

    /// Where the creature or thing on a cell stands now (between two cells
    /// while it walks) and how tall it is: the HUD hangs its arrow over a
    /// hostile's own head.
    pub fn entity_stands(&self, x: i32, y: i32) -> Option<(Vector3, f32)> {
        let nodes = self.cells.get(&(x, y))?;
        let i = nodes.look.entity?;
        let (model, placed) = (nodes.models.get(i)?, nodes.look.models.get(i)?);
        (model.node.is_instance_valid() && model.node.is_inside_tree())
            .then(|| (model.node.get_global_position(), placed.look.art.height))
    }

    /// Where the hero's model stands now (between two cells while it
    /// walks) and how tall it is: the HUD keeps its arrow off them.
    pub fn hero_stands(&self) -> Option<(Vector3, f32)> {
        let nodes = self.hero_cell()?;
        let i = nodes.look.entity?;
        let (model, placed) = (nodes.models.get(i)?, nodes.look.models.get(i)?);
        if !model.node.is_instance_valid() || !model.node.is_inside_tree() {
            return None;
        }
        let feet = model.node.get_global_position();
        // on a steed: up to the rider's head over its saddle
        let tall = match self.rider.as_ref().and_then(|r| Some((r, r.on.as_ref()?))) {
            Some((r, (_, mount))) => {
                let seat = mount.at().map_or(placed.look.art.height, |s| s.y - feet.y);
                seat + r.look.art.height * RIDER_OVER_SEAT
            }
            None => placed.look.art.height,
        };
        Some((feet, tall))
    }

    /// The hero's model (the doll shows it; self-tests look at its gear):
    /// on a steed, the rider's.
    pub fn hero_model(&self) -> Option<&Model> {
        if let Some(r) = &self.rider {
            return Some(&r.model);
        }
        let nodes = self.hero_cell()?;
        nodes.look.entity.and_then(|i| nodes.models.get(i))
    }

    /// The hero's model's name in the art manifest, and the name the
    /// hero's form calls for (self-tests: a polymorphed hero is drawn as
    /// what they are).
    pub fn hero_model_names(
        &self,
        world: &World,
        catalog: &Catalog,
    ) -> (Option<String>, Option<String>) {
        let manifest = self.art.manifest();
        let shown = self
            .hero_model()
            .filter(|m| !m.is_pending())
            .map(|m| manifest.model_at(m.model_index()).0.to_string());
        let wanted = world.map.hero_form().and_then(|g| {
            let info = monster_info(catalog, g.mon)?;
            let art = manifest.monster(info, g.flags);
            Some(manifest.model_at(art.model).0.to_string())
        });
        (shown, wanted)
    }

    /// The hero's ring shows, and the rim light is on their model now
    /// (self-tests).
    pub fn hero_marked(&self) -> (bool, bool) {
        let ring = self.hero_ring.is_visible();
        let rim = match (self.rim_model.as_ref(), self.hero_model()) {
            (Some(r), Some(m)) => *r == m.node,
            _ => false,
        };
        (ring, rim)
    }

    /// The places on the hero's model something is held in (self-tests).
    pub fn hero_held_slots(&self) -> Vec<&'static str> {
        self.hero_model()
            .map_or(Vec::new(), |m| self.art.held_slots(m))
    }

    /// The hero's own key and fills light them: not while polymorphed
    /// (self-tests).
    pub fn hero_studio_on(&self) -> bool {
        self.hero_studio.is_visible()
    }

    /// How wide across the hero's ring is (self-tests).
    pub fn hero_ring_across(&self) -> f32 {
        self.hero_ring.get_size().x
    }

    /// How far across the floor the middle of where the hero's model meets
    /// it (`FORM_FOOTING`) is from their ring's (self-tests: a form stands
    /// centred on its cell).
    pub fn hero_off_centre(&self) -> Option<f32> {
        let inner = self
            .hero_model()?
            .node
            .get_child(0)?
            .try_cast::<Node3D>()
            .ok()?;
        let foot = crate::art::footing(&inner, FORM_FOOTING)?;
        let at = inner.get_global_transform() * foot.center();
        let ring = self.hero_ring.get_global_position();
        Some(Vector2::new(at.x - ring.x, at.z - ring.z).length())
    }

    /// The steed the hero rides: its model, and where on its back they
    /// sit (None on foot, or while the steed is not drawn yet).
    pub fn steed(&self) -> Option<(&Model, &Mount)> {
        let (_, mount) = self.rider.as_ref()?.on.as_ref()?;
        let nodes = self.hero_cell()?;
        Some((nodes.look.entity.and_then(|i| nodes.models.get(i))?, mount))
    }

    /// The monster or object on a cell: its model (self-tests).
    pub fn entity_model(&self, x: i32, y: i32) -> Option<&Model> {
        let nodes = self.cells.get(&(x, y))?;
        nodes.look.entity.and_then(|i| nodes.models.get(i))
    }

    /// The node of the monster or object on a cell, and whether its meshes
    /// are on the hero's rim layer (self-tests: only the hero's are).
    pub fn entity_rim(&self, x: i32, y: i32) -> Option<(Gd<Node3D>, bool)> {
        let nodes = self.cells.get(&(x, y))?;
        let node = nodes.models.get(nodes.look.entity?)?.node.clone();
        let rim = Self::on_rim(&node);
        Some((node, rim))
    }

    /// Put a model on the hero's rim layer, or take it off (self-tests: a
    /// model in view changes layers as the hero's does).
    pub fn set_rim(node: &Gd<Node3D>, on: bool) {
        set_layers(node, if on { 1 | RIM_LAYER } else { 1 });
    }

    /// The x-ray overlay on the hero's model, as while a wall hides them,
    /// in a colour no hero wears and with no depth kept back for the model
    /// itself: it then shows on every part of the hero another part covers
    /// (self-tests: the doll's camera draws none of it). `false` takes it
    /// off and gives the overlay its own colour and depth back. False when
    /// there is no hero's model or no overlay to put on it.
    pub fn probe_xray(&mut self, on: bool) -> bool {
        let alive = |n: &Gd<Node3D>| n.is_instance_valid() && !n.is_queued_for_deletion();
        let (Some(node), Some(xray)) = (self.rim_model.clone().filter(alive), self.xray.clone())
        else {
            return false;
        };
        if let Ok(mut m) = xray.clone().try_cast::<ShaderMaterial>() {
            let (color, pull) = if on {
                (Color::from_rgba(1.0, 0.0, 1.0, 1.0), 0.0f32.to_variant())
            } else {
                (XRAY, Variant::nil())
            };
            m.set_shader_parameter("color", &color.to_variant());
            m.set_shader_parameter("pull", &pull);
        }
        set_overlay(&node, on.then_some(&xray));
        // as `show_through` left it: it puts its own on when a wall hides them
        self.xray_on = false;
        true
    }

    /// Every model on the map drawn once more with the hover's outline
    /// (`Some(false)`) or the x-ray (`Some(true)`), or neither (`None`):
    /// the pipelines a game may need over any model (the `variants`
    /// self-test's census).
    pub fn overlay_models(&mut self, xray: Option<bool>) {
        let overlay = match xray {
            Some(true) => self.xray.clone(),
            Some(false) => Some(self.outline.0.clone()),
            None => None,
        };
        for cell in self.cells.values() {
            for m in &cell.models {
                set_overlay(&m.node, overlay.as_ref());
            }
        }
    }

    /// The clips the hero's model rests and fights with now, the gear's:
    /// (idle, attack) (self-tests).
    pub fn hero_fight_clips(&self) -> (Option<String>, Option<String>) {
        let Some(m) = self.hero_model() else {
            return (None, None);
        };
        let c = self.art.clips(m, false);
        (c.idle, c.attack)
    }

    /// The art gallery's lighting (for review): a key light over the
    /// hall, the fills and the ambient raised. A new game ends it.
    pub fn set_showcase(&mut self, on: bool) {
        if self.showcase == on {
            return;
        }
        self.showcase = on;
        self.studio.set_visible(on);
        self.creature_light.set_visible(!on);
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
        let parts = if self.shown || self.title.is_some() {
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
        // the loader's threads shared with the rehearsal's own files
        let others = self.rehearsal.as_ref().or(self.unplayed.as_ref());
        self.art
            .set_others_loading(others.map_or(0, |r| r.loading()));
        let mut more = self.art.preload_step();
        let art = now.elapsed();
        if !more
            && self.rehearsed
            && self.stats_window.is_some()
            && !std::mem::replace(&mut self.preload_told, true)
        {
            godot_print!(
                "map: art loaded ahead {:.1} s after start, {} models built",
                godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0,
                self.art.counts().1
            );
        }
        // the art's own files first (a later launch's veil waits for them)
        if let Some(r) = self.unplayed.as_mut().filter(|_| !more) {
            r.fetch(crate::art::loads_at_once());
        }
        if self.quick && !self.veiled.unwrap_or(true) {
            // the veil is up over a later launch's title
            self.variants.release_canvas();
        }
        if self.rehearsal.is_some() || (self.quick && !self.variants.done()) {
            if !self.variants.done() {
                self.make_variants();
            }
        } else {
            // made again after the graphics changed
            self.variants.tick();
        }
        if let Some(mut r) = self.rehearsal.take() {
            // what it draws loads in the background meanwhile; it plays
            // once the art is loaded (a game started ends it)
            if more {
                let room = crate::art::loads_at_once().saturating_sub(self.art.loads_in_flight());
                r.fetch(room);
                self.rehearsal = Some(r);
            } else if r.step(self, self.root.get_process_delta_time()) {
                self.rehearsal = Some(r);
                more = true;
            } else {
                // the title scene follows it (below)
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
        // the title scene after the rehearsal, and again on the title
        // after a game (or wherever no game is shown: the pages over it
        // are opaque but the title's)
        if !self.shown && self.rehearsed && self.rehearsal.is_none() && self.title.is_none() {
            self.show_title_scene();
        }
        if let Some(mut t) = self.title.take() {
            if !self.shown {
                let veiled = self.veiled.unwrap_or(true);
                self.set_build_budget(veiled.then_some(TITLE_VEILED_BUILD));
                let (world, catalog) = t.parts();
                self.sync(world, catalog, self.root.get_process_delta_time());
            }
            self.title = Some(t);
        }
        self.title_frame_update(self.root.get_process_delta_time());
        if self.stats_window.is_some()
            && self.rehearsal_first_draws_done()
            && !std::mem::replace(&mut self.first_draws_told, true)
        {
            godot_print!(
                "map: the rehearsal's first draws done {:.1} s after start",
                godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0
            );
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
        more
    }

    /// A frame's time for drawing a new level's cells (None: the game's).
    pub(crate) fn set_build_budget(&mut self, budget: Option<std::time::Duration>) {
        self.build_budget = budget.unwrap_or(BUILD_BUDGET);
    }

    /// A title frame left to other work (the dialogs' warm-up), instead of
    /// `preload_step`: nothing is loaded ahead in it, and the title's
    /// frame times stay a frame each.
    pub fn title_tick(&mut self) {
        // the title scene's camera moves on all the same
        let delta = self.root.get_process_delta_time();
        if self.titled && !self.shown {
            self.clock += delta;
        }
        self.title_frame_update(delta);
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

    /// The rehearsal has drawn everything a game draws for the first time
    /// and the title scene after it is drawn: the long frames of first
    /// draws are behind, and the title has its scene (the start-up veil
    /// waits for this). False until `warm_up` has started the rehearsal;
    /// true once a game has started.
    pub fn rehearsal_first_draws_done(&self) -> bool {
        self.rehearsed
            && self.rehearsal.is_none()
            && self.variants.done()
            && (self.shown || self.title.is_none() || self.title_drawn())
    }

    /// The title scene is what the map draws (self-tests).
    pub fn title_shown(&self) -> bool {
        self.titled && !self.shown
    }

    /// Put the title camera's sway at `secs` (self-test pictures of both
    /// ends of it).
    pub(crate) fn set_title_clock(&mut self, secs: f64) {
        self.clock = secs;
        self.place_camera();
    }

    /// The title scene is all drawn and lit, up out of black.
    pub fn title_drawn(&self) -> bool {
        self.title.is_some()
            && self.building.is_empty()
            && self.fade <= 0.0
            && self.cover <= 0.0
            && self.models_pending() == 0
    }

    /// Whether the start-up veil is down: while it is, the rehearsal
    /// keeps no frame's budget (nothing of it shows).
    pub fn set_veiled(&mut self, veiled: bool) {
        self.veiled = Some(veiled);
    }

    /// Whether the rehearsal is drawn under the start-up veil; unless the
    /// game tells, until its first draws are done.
    pub(crate) fn under_veil(&self, first_draws_done: bool) -> bool {
        self.veiled.unwrap_or(!first_draws_done)
    }

    /// The hero the title scene shows: of `role` (a role's name or code,
    /// "Val"), a woman's look when `female`. Laid again when it changes.
    pub fn set_title_hero(&mut self, role: &str, female: bool) {
        let hero = (role.to_string(), female);
        if self.title_hero != hero {
            self.title_hero = hero;
            if self.titled && !self.shown {
                self.show_title_scene();
            }
        }
    }

    /// The title scene's hero: (role, a woman) (self-tests).
    pub fn title_hero(&self) -> (&str, bool) {
        (&self.title_hero.0, self.title_hero.1)
    }

    /// The hero the creation form is making, in the title hero's place,
    /// framed where the form leaves room (None: the form is gone, the
    /// title's own hero again). Only their cell is drawn again.
    pub fn set_creation_hero(&mut self, preview: Option<crate::title_scene::Preview>) {
        if preview == self.creation {
            return;
        }
        let dress = match &preview {
            Some(p)
                if self
                    .creation
                    .as_ref()
                    .is_some_and(|c| c.role == p.role && c.female == p.female) =>
            {
                None
            }
            Some(p) => Some((p.role.clone(), p.female)),
            None => Some((Some(self.title_hero.0.clone()), Some(self.title_hero.1))),
        };
        self.creation = preview;
        if let (Some((role, female)), Some(t)) = (dress, self.title.as_mut()) {
            t.dress(role.as_deref(), female);
        }
    }

    /// The hero the creation form shows (self-tests).
    pub fn creation_hero(&self) -> Option<&crate::title_scene::Preview> {
        self.creation.as_ref()
    }

    /// The camera frames the creation form's hero (self-tests).
    pub fn creation_framed(&self) -> bool {
        self.creation.is_some() && self.creation_blend >= 1.0
    }

    /// Lay the title scene; nothing before the rehearsal is over.
    fn show_title_scene(&mut self) {
        let Some(catalog) = self.title_catalog.clone() else {
            return;
        };
        self.clear();
        let (role, female) = &self.title_hero;
        let mut title = crate::title_scene::TitleScene::new(&catalog, role, *female);
        if let Some(p) = &self.creation {
            title.dress(p.role.as_deref(), p.female);
        }
        self.title = Some(title);
        self.titled = true;
        self.light_title_fires();
        // black past the far wall (the branch's darkness is a deep blue)
        self.env.set_bg_color(Color::BLACK);
        self.env.set_fog_light_color(Color::BLACK);
        self.show_parts();
    }

    /// A title frame: the camera round the hero, the fires flickering,
    /// the cover lifting.
    fn title_frame_update(&mut self, delta: f64) {
        let to = if self.creation.is_some() { 1.0 } else { 0.0 };
        let step = delta.min(1.0 / 30.0) as f32 / CREATION_FRAME_SECS;
        self.creation_blend = if to > self.creation_blend {
            (self.creation_blend + step).min(to)
        } else {
            (self.creation_blend - step).max(to)
        };
        if self.titled && !self.shown {
            self.hero_ring.set_visible(false);
            self.place_camera();
            let energy = self.branch_look.torch_energy;
            for (_, light, at, phase) in &mut self.title_fires {
                let time = self.clock + *phase;
                light.set_param(Param::ENERGY, energy * TITLE_FIRE_ENERGY * flicker(time));
                light.set_position(*at + sway(time));
            }
        }
        self.uncover(delta as f32);
    }

    /// Light the title scene's fires (made the first time).
    fn light_title_fires(&mut self) {
        use crate::title_scene::FIRES;
        if self.title_fires.is_empty() {
            for (i, &(fire, (x, z))) in FIRES.iter().enumerate() {
                let made = self.title_fire(fire, Vector3::new(x, 0.0, z), i as f64 * 2.3);
                self.title_fires.push(made);
            }
        }
        for (node, light, _, _) in &mut self.title_fires {
            node.set_visible(true);
            light.set_visible(true);
        }
    }

    /// One fire of the title scene: its model, a flame of the torches'
    /// (crossed, it stands free), their halo and embers, and a light.
    fn title_fire(
        &mut self,
        fire: crate::title_scene::Fire,
        place: Vector3,
        phase: f64,
    ) -> (Gd<Node3D>, Gd<OmniLight3D>, Vector3, f64) {
        let crate::title_scene::FireLook {
            scene,
            height,
            burns,
            size,
            flames,
            coals,
        } = fire.look();
        let mut node = Node3D::new_alloc();
        node.set_position(place);
        if let Some(mut model) = godot::tools::try_load::<PackedScene>(scene)
            .ok()
            .and_then(|s| s.instantiate())
            .and_then(|n| n.try_cast::<Node3D>().ok())
        {
            let tall = model_height(&model);
            if tall > 0.0 {
                let k = height / tall;
                model.set_scale(Vector3::new(k, k, k));
            }
            node.add_child(&model);
        }
        // the torches' flame (it turns to face the camera); a fire of
        // several a little dimmer, or where they cross they burn white
        let mut flame_mat = self.flame_mat.clone();
        if flames.len() > 1
            && let Ok(mut m) = self
                .flame_mat
                .duplicate_resource()
                .try_cast::<ShaderMaterial>()
        {
            m.set_shader_parameter("energy", &MANY_FLAMES_ENERGY.to_variant());
            flame_mat = m.upcast();
        }
        let mut quad = godot::classes::QuadMesh::new_gd();
        quad.set_size(Vector2::new(0.26 * size, 0.52 * size));
        quad.set_center_offset(at(0.0, 0.22 * size, 0.0));
        for (i, &(fx, fz)) in flames.iter().enumerate() {
            let mut flame = MeshInstance3D::new_alloc();
            flame.set_mesh(&quad);
            flame.set_material_override(&flame_mat);
            flame.set_position(at(fx, burns, fz));
            let p = phase as f32 + i as f32 * 3.7;
            flame.set_instance_shader_parameter("phase", &p.to_variant());
            no_shadow(&mut flame);
            node.add_child(&flame);
        }
        if coals > 0.0 {
            // charred logs across the pit, glowing coals between them
            let wood = self.art.flat(CHARRED, Finish::Matte);
            let log = self.art.mesh(cylinder(0.045, 0.05, coals * 1.7));
            for (k, yaw) in [15.0f32, 75.0, 135.0].into_iter().enumerate() {
                let mut l = MeshInstance3D::new_alloc();
                l.set_mesh(&log);
                l.set_material_override(&wood);
                l.set_position(at(0.0, 0.07 + 0.04 * k as f32, 0.0));
                l.set_rotation_degrees(at(90.0, yaw, 0.0));
                node.add_child(&l);
            }
            let glow = self.art.flat(EMBER_GLOW, Finish::Glow);
            let coal = self.art.mesh(sphere(0.045));
            for k in 0..9 {
                let a = (k as f32 * 137.5).to_radians();
                let r = coals * 0.75 * ((k as f32 + 0.5) / 9.0).sqrt();
                let mut c = MeshInstance3D::new_alloc();
                c.set_mesh(&coal);
                c.set_material_override(&glow);
                c.set_position(at(r * a.cos(), 0.04, r * a.sin()));
                c.set_scale(at(1.2, 0.45, 1.0));
                no_shadow(&mut c);
                node.add_child(&c);
            }
        }
        let mut halo = MeshInstance3D::new_alloc();
        let mut disc = godot::classes::QuadMesh::new_gd();
        disc.set_size(Vector2::new(0.9 * size, 0.9 * size));
        halo.set_mesh(&disc);
        halo.set_material_override(&self.halo_mat);
        halo.set_position(at(0.0, burns + 0.2 * size, 0.1));
        no_shadow(&mut halo);
        node.add_child(&halo);
        let mut embers = self.vfx.embers();
        embers.set_position(at(0.0, burns + 0.1, 0.0));
        node.add_child(&embers);
        self.root.add_child(&node);
        let lit = place + Vector3::new(0.0, burns + 0.4 * size, 0.0);
        let mut light = OmniLight3D::new_alloc();
        light.set_color(TORCH);
        light.set_param(Param::RANGE, TORCH_RANGE * size.max(1.0));
        light.set_param(Param::ATTENUATION, 1.2);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.5);
        light.set_position(lit);
        light.set_shadow(false);
        self.root.add_child(&light);
        (node, light, lit, phase)
    }

    /// How far the warm-up behind the start-up veil has come, 0 to 1: the
    /// art loaded ahead, the variants made, the rehearsal's stages.
    pub fn warm_progress(&self) -> f32 {
        if self.quick {
            // a later launch's veil waits for these (and the title)
            return (self.art.preload_progress() + self.variants.progress()) / 2.0;
        }
        let rehearsal = match &self.rehearsal {
            Some(r) => r.progress(),
            None if self.rehearsed => 1.0,
            None => 0.0,
        };
        (self.art.preload_progress() + self.variants.progress() + rehearsal) / 3.0
    }

    /// What is left of the warm-up, each part: the art's files and models
    /// loaded ahead, the pipeline variants made, the rehearsal over.
    pub fn warm_parts(&self) -> (bool, bool, bool) {
        (
            self.art.preloaded(),
            self.variants.done(),
            self.rehearsed && self.rehearsal.is_none(),
        )
    }

    /// Nothing is left to load ahead (self-tests start a game then).
    pub fn preloaded(&self) -> bool {
        self.rehearsed
            && self.art.preloaded()
            && self.rehearsal.is_none()
            && self.unplayed.as_ref().is_none_or(|r| r.fetched())
    }

    /// A later launch's warm-up (`warm_up`): no rehearsal, the title at
    /// once; set before it starts.
    pub fn set_quick_start(&mut self, quick: bool) {
        self.quick = quick;
        self.art.set_files_ahead(quick);
    }

    pub fn quick_start(&self) -> bool {
        self.quick
    }

    /// The time a frame's loading ahead may take (the game's but on a
    /// later launch's title, `game.rs` `preload_budget`).
    pub fn set_preload_budget(&mut self, budget: std::time::Duration) {
        self.art.set_preload_budget(budget);
    }

    /// Build ahead the models a game shows first: the role's hero (first
    /// of all) when one is chosen, else every role's, the pets and the
    /// monsters of the first levels.
    pub fn warm_up(&mut self, catalog: &Catalog, role: Option<&str>) {
        if !std::mem::replace(&mut self.rehearsed, true) {
            if self.stats_window.is_some() {
                godot_print!(
                    "map: the rehearsal starts {:.1} s after start",
                    godot::classes::Time::singleton().get_ticks_msec() as f64 / 1000.0
                );
            }
            let fetch = self.prefetch_paths();
            let rehearsal = crate::rehearsal::Rehearsal::new(catalog, fetch);
            if self.quick {
                self.unplayed = Some(rehearsal);
            } else {
                self.rehearsal = Some(rehearsal);
            }
            self.variants.start(&mut self.root, true);
            // the title scene after it is laid from the same catalog
            self.title_catalog = Some(catalog.clone());
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
        let mut looks = match self.rehearsal.as_ref().or(self.unplayed.as_ref()) {
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
        // the creation form's hooded figure, built ahead with the roles
        // (under load the form stood with no hero while it was built)
        if role.is_none()
            && let Some(hooded) = self.art.manifest().model_index(HOODED_FIGURE)
            && let Some(info) = catalog.monsters.iter().find(|m| m.name == "archeologist")
        {
            // as the title scene's hero stands for it (`update_cell`)
            let mut r = self.art.manifest().monster(info, 0);
            r.model = hooded;
            looks.push(ModelLook {
                art: r,
                tint: lifted(tint_color(r.tint, info.color)),
                pose: Pose::Alive,
            });
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
                    hero_pit: None,
                    branch: &self.branch_look,
                    volume: None,
                };
                let look = look_of(cell, Near::of(map, x, y), &ctx);
                let models = look.models.iter().map(|p| p.look);
                for l in models.chain(look.rider) {
                    if !looks.contains(&l) {
                        looks.push(l);
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
        if let Some(r) = self.rider.as_mut().filter(|r| r.model.is_pending()) {
            if !self.art.complete(&mut r.model) {
                return;
            }
            if Some(r.model.node.instance_id()) == rim {
                self.rim_model = None;
            }
        }
        for nodes in self.cells.values_mut() {
            for (i, m) in nodes.models.iter_mut().enumerate() {
                if !m.is_pending() {
                    continue;
                }
                if !self.art.complete(m) {
                    return;
                }
                // the hero's rim light takes the meshes now there
                if Some(m.node.instance_id()) == rim {
                    self.rim_model = None;
                }
                // and the back now there, its saddle
                if nodes.look.entity == Some(i) {
                    self.art.saddle(m, nodes.look.saddled);
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
        if self.titled
            && !self.shown
            && let Some((eye, aim)) = self.stage_camera
        {
            // a beat of the prologue: its own camera
            let d = (eye - aim).length();
            self.env.set_fog_depth_begin(d + 4.0);
            self.env.set_fog_depth_end(d + 16.0);
            self.camera.look_at_from_position(eye, aim);
            self.eye = eye;
            RenderingServer::singleton().global_shader_parameter_set("eye_pos", &eye.to_variant());
            return;
        }
        if self.titled && !self.shown {
            use crate::title_scene::{AIM, DISTANCE, PITCH_DEG, SWAY_DEG, SWAY_SECS};
            let phase = (self.clock as f32 / SWAY_SECS * std::f32::consts::TAU).sin();
            let orbit = |aim: Vector3, sway: f32, pitch: f32, distance: f32| {
                let (yaw, pitch) = ((sway * phase).to_radians(), pitch.to_radians());
                let offset = Vector3::new(
                    yaw.sin() * pitch.cos(),
                    pitch.sin(),
                    yaw.cos() * pitch.cos(),
                ) * distance;
                (aim + offset, aim)
            };
            let (mut eye, mut aim) = orbit(
                Vector3::new(AIM.0, AIM.1, AIM.2),
                SWAY_DEG,
                PITCH_DEG,
                DISTANCE,
            );
            // the creation form's hero, close, where the form leaves room
            if self.creation_blend > 0.0 {
                let at = self.creation.as_ref().map_or(0.5, |p| p.at);
                let (e, a) = orbit(
                    self.creation_aim(at),
                    CREATION_SWAY_DEG,
                    CREATION_PITCH_DEG,
                    CREATION_DISTANCE,
                );
                let t = self.creation_blend;
                let t = t * t * (3.0 - 2.0 * t);
                eye = eye.lerp(e, t);
                aim = aim.lerp(a, t);
            }
            // the dark closes in just past the far wall
            self.env.set_fog_depth_begin(TITLE_FOG.0);
            self.env.set_fog_depth_end(TITLE_FOG.1);
            self.camera.look_at_from_position(eye, aim);
            self.eye = eye;
            RenderingServer::singleton()
                .global_shader_parameter_set("eye_pos", &self.eye.to_variant());
            return;
        }
        let distance = self.camera_distance();
        let pitch = pitch_at(distance);
        // the darkness closes in at the same depth behind what is framed,
        // however far the camera is
        // (the whole level framed: none)
        let clear = OVERVIEW_CLEAR * self.overview_k;
        self.env.set_fog_depth_begin(distance + 3.0 + clear);
        self.env.set_fog_depth_end(distance + 16.0 + clear);
        let offset = Vector3::new(0.0, pitch.sin(), pitch.cos()) * distance;
        // aim a little south of the hero, so the hero stands above the
        // log; close in, the same offset would push the hero off the top
        let south = AIM_SOUTH * distance / DISTANCE;
        // a stunned or confused hero's view drifts a little
        let aim = self.focus + Vector3::new(0.0, self.aim_lift, south) + self.hero_look.drift;
        self.camera.look_at_from_position(aim + offset, aim);
        self.eye = aim + offset;
        RenderingServer::singleton().global_shader_parameter_set("eye_pos", &self.eye.to_variant());
    }

    /// Where the creation camera aims: beside the hero, so that they stand
    /// `at` (a share of the screen's width from its left).
    fn creation_aim(&self, at: f32) -> Vector3 {
        use crate::title_scene::HERO;
        let size = self
            .root
            .get_viewport()
            .map_or(Vector2::new(16.0, 10.0), |v| v.get_visible_rect().size);
        let aspect = size.x / size.y.max(1.0);
        let half = (FOV_DEG.to_radians() / 2.0).tan() * aspect * CREATION_DISTANCE;
        let across = (at.clamp(0.2, 0.8) - 0.5) * 2.0 * half;
        Vector3::new(HERO.0 as f32 - across, CREATION_AIM_UP, HERO.1 as f32)
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
                mark_color(d, c);
                d.set_position(origin + at(0.0, look.ground, 0.0));
            }
            (Some(c), None) => {
                let mut d = marker(&mut self.root, &self.ring_tex, 0.95);
                mark_color(&mut d, c);
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

    /// Make ahead one more of what the levels take from the pools, every
    /// branch's (torches, their lanterns, doors, gates, candles,
    /// candelabras), the first of each kind drawn once before the camera
    /// (its first draw: its pipelines, its buffers), and the dust once as
    /// embers: a level showing them later costs no long frame. False when
    /// there is all of it.
    pub(crate) fn warm_pools(&mut self) -> bool {
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
                self.shadow_frames = Self::frame();
                return true;
            }
            Some(block) if Self::frame() < self.shadow_frames + 3 => {
                // drawn a few frames (frames, not calls: under the veil
                // there are several calls a frame)
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
        for l in Liquid::ALL {
            if !self.liquids.contains_key(&l) {
                self.liquid(l);
                return true;
            }
        }
        // the dust once as Gehennom's embers
        if !self.embers_shown {
            self.embers_shown = true;
            self.vfx.set_dust(1.0, true);
            return true;
        }
        // the embers' particles alone first (their shader is a long
        // compile), then the torches
        if self.spare_embers.is_none() && self.torches.is_empty() {
            self.spare_embers = Some(self.vfx.embers());
            return true;
        }
        if self.torches.len() < TORCHES_AHEAD {
            let mut t = self.new_torch();
            t.light.set_visible(false);
            let first = self.torches.is_empty();
            t.node.set_visible(first);
            if first {
                self.show_once(t.node.clone());
            }
            self.torches.push(t);
            return true;
        }
        if let Some(i) = self.torches.iter().position(|t| t.lantern.is_none()) {
            // its scene loaded in one frame, hung in the next
            if !self.load_prop(Prop::Lantern) {
                self.hang_lantern(i);
                if i == 0 {
                    // the first drawn once, in its torch's place
                    let t = &mut self.torches[0];
                    t.fire.set_visible(false);
                    t.node.set_visible(true);
                    let node = t.node.clone();
                    self.show_once(node);
                }
            }
            return true;
        }
        let wanted = [
            (Prop::CastleDoor, DOORS_AHEAD),
            (Prop::IronGate, DOORS_AHEAD),
            (Prop::Candle, CANDLES_AHEAD),
            (Prop::Candelabra, CANDELABRAS_AHEAD),
        ];
        for (prop, n) in wanted {
            let have = self.spare_props.get(&prop).map_or(0, Vec::len);
            if have >= n {
                continue;
            }
            if self.load_prop(prop) {
                return true;
            }
            if let Some(mut node) = self.new_prop(prop) {
                node.set_visible(have == 0);
                if have == 0 {
                    self.show_once(node.clone());
                }
                self.spare_props.entry(prop).or_default().push(node);
                return true;
            }
        }
        false
    }

    /// The process frame (a few things wait frames, not calls).
    fn frame() -> u64 {
        godot::classes::Engine::singleton().get_process_frames()
    }

    /// A pooled thing drawn once before the camera, put away by
    /// `put_away_shown` (the next frame).
    fn show_once(&mut self, mut node: Gd<Node3D>) {
        let k = self.shown_once.len() as f32;
        let place = self.focus + at(-1.5 + (k % 4.0), 0.0, -1.0 + (k / 4.0).floor());
        node.set_global_position(place);
        self.shown_once.push(node);
    }

    /// What was drawn once put away (its torch's fire as it was), and the
    /// dust back to the branch's.
    pub(crate) fn put_away_shown(&mut self) {
        for mut node in std::mem::take(&mut self.shown_once) {
            node.set_visible(false);
        }
        if let Some(t) = self.torches.first_mut() {
            t.fire.set_visible(!self.branch_look.lanterns);
        }
        let l = self.branch_look;
        self.vfx.set_dust(l.dust, l.embers);
    }

    /// Models standing in for ones not built yet.
    pub(crate) fn models_pending(&self) -> usize {
        self.art.pending()
    }

    /// Draw `textures` on small cards before the camera (none: no cards),
    /// so they reach the graphics card now, not with a level.
    pub(crate) fn show_textures(&mut self, textures: &[Gd<godot::classes::Texture2D>]) {
        while self.cards.len() > textures.len() {
            if let Some(mut card) = self.cards.pop() {
                card.queue_free();
            }
        }
        let eye = self.camera.get_global_transform();
        for (i, t) in textures.iter().enumerate() {
            if i == self.cards.len() {
                let mut card = MeshInstance3D::new_alloc();
                let mut quad = godot::classes::QuadMesh::new_gd();
                quad.set_size(Vector2::new(0.1, 0.1));
                card.set_mesh(&quad);
                let mut m = StandardMaterial3D::new_gd();
                m.set_shading_mode(ShadingMode::UNSHADED);
                card.set_material_override(&m);
                no_shadow(&mut card);
                self.root.add_child(&card);
                self.cards.push(card);
            }
            let card = &mut self.cards[i];
            let x = -0.4 + 0.11 * i as f32;
            card.set_global_transform(eye * Transform3D::new(Basis::IDENTITY, at(x, 0.0, -1.0)));
            if let Some(mut m) = card
                .get_material_override()
                .and_then(|m| m.try_cast::<StandardMaterial3D>().ok())
            {
                m.set_texture(godot::classes::base_material_3d::TextureParam::ALBEDO, t);
            }
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
        // the prologue's sets (its warm-up draws one of each)
        paths.extend(prologue_stage::KIT.map(String::from));
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
                let y = if kind == Lamp::Holy {
                    SHAFT_TALL / 2.0
                } else {
                    -0.3
                };
                m.set_position(origin + at(0.0, y, 0.0));
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
            Lamp::Holy => {
                light.set_color(HOLY.0);
                light.set_param(Param::ENERGY, HOLY.1);
                light.set_param(Param::RANGE, HOLY.2);
                light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 2.5);
                // a shaft of light falling on it, soft at its sides and
                // fading upwards (the prayer's own, shaders/holy_light)
                if let Ok(shader) =
                    godot::tools::try_load::<Shader>("res://shaders/holy_light.gdshader")
                {
                    let mut mat = ShaderMaterial::new_gd();
                    mat.set_shader(&shader);
                    mat.set_shader_parameter("color", &HOLY.0.to_variant());
                    mat.set_shader_parameter("height", &SHAFT_TALL.to_variant());
                    mat.set_shader_parameter("strength", &SHAFT_STRENGTH.to_variant());
                    let mut shaft = MeshInstance3D::new_alloc();
                    shaft.set_mesh(&self.art.mesh(cylinder(0.55, 0.55, SHAFT_TALL)));
                    shaft.set_material_override(&mat);
                    shaft.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                    shaft.set_position(origin + at(0.0, SHAFT_TALL / 2.0, 0.0));
                    self.root.add_child(&shaft);
                    mist = Some(shaft.upcast());
                }
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
                mist = Some(fog.upcast());
            }
        }
        self.root.add_child(&light);
        nodes.lamp = Some(CellLamp { kind, light, mist });
    }

    /// The context a cell's look is made in, of the map as it is drawn
    /// (no world's: a level held while it is left).
    fn cell_ctx<'a>(&'a self, (x, y): (i32, i32), catalog: &'a Catalog) -> Ctx<'a> {
        Ctx {
            catalog,
            art: self.art.manifest(),
            x,
            y,
            hero: None,
            hero_yaw: self.hero_yaw,
            facing: None,
            hero_pit: None,
            branch: &self.branch_look,
            volume: None,
        }
    }

    /// A cell's floor drawn anew (opened as something drops through it, a
    /// trap door's lid taken off; shut again), all else as it was: its
    /// solids, the `opening` in it, the rock under the level cut where it
    /// is `sunk`.
    fn reshape(
        &mut self,
        (x, y): (i32, i32),
        solids: Vec<Solid>,
        opening: Option<Opening>,
        sunk: bool,
    ) {
        let Some(mut nodes) = self.cells.remove(&(x, y)) else {
            return;
        };
        let origin = Vector3::new(x as f32, 0.0, y as f32);
        let old = std::mem::take(&mut nodes.look.solids);
        let slots = std::mem::take(&mut nodes.solids);
        nodes.solids = self.place_solids((x, y), origin, slots, &old, &solids);
        nodes.look.solids = solids;
        nodes.look.opening = opening;
        if nodes.look.sunk != sunk {
            nodes.look.sunk = sunk;
            if sunk {
                self.holes.insert((x, y));
            } else {
                self.holes.remove(&(x, y));
            }
            self.bedrock_dirty = true;
        }
        self.cells.insert((x, y), nodes);
    }

    /// The batches' slots of a cell's solids, `old` drawn in `slots`, now
    /// `new`: a solid the same in the same place keeps its slot.
    fn place_solids(
        &mut self,
        (x, y): (i32, i32),
        origin: Vector3,
        slots: Vec<Slot>,
        old: &[Solid],
        new: &[Solid],
    ) -> Vec<Slot> {
        let mut slots = slots.into_iter();
        let mut kept_slots = Vec::with_capacity(new.len());
        for (i, s) in new.iter().enumerate() {
            let slot = slots.next();
            if let Some(slot) = slot
                && old.get(i) == Some(s)
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
        kept_slots
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
            hero_pit: world
                .hero_trap
                .filter(|t| t.is_pit())
                .map(|t| t == nh_world::TrapKind::SpikedPit),
            branch: if self.built.contains(&(x, y)) {
                &self.built_look
            } else {
                &self.branch_look
            },
            volume: self.volume.get(&(x, y)).copied(),
        };
        let mut look = world
            .map
            .cell(x, y)
            .map(|c| look_of(c, Near::of(&world.map, x, y), &ctx))
            .unwrap_or_default();
        // behind the creation form, a choice left to chance: the hooded
        // figure stands for the hero
        if look.hero
            && self.titled
            && !self.shown
            && self.creation.as_ref().is_some_and(|p| p.veiled())
            && let Some(i) = look.entity
            && let Some(hooded) = self.art.manifest().model_index(HOODED_FIGURE)
            && let Some(m) = look.models.get_mut(i)
        {
            m.look.art.model = hooded;
        }
        // the hero in a form longer than a cell stands on it as it is, not
        // set back (none bites into them; their step and their ring keep
        // to the middle)
        if look.hero
            && let Some(i) = look.entity
            && let Some(m) = look.models.get_mut(i)
        {
            m.look.art.back = 0.0;
        }
        self.ground.set(x, y, look.kind);
        self.standing
            .set(x, y, look.entity.is_some(), look.is_empty());
        match self.cells.get(&(x, y)) {
            Some(n) if n.look == look => return,
            None if look.is_empty() => return,
            _ => {}
        }
        let mut nodes = self.cells.remove(&(x, y)).unwrap_or_default();
        let old = std::mem::take(&mut nodes.look);
        if old.lit != look.lit || (old.kind == Ground::Lava) != (look.kind == Ground::Lava) {
            self.lights_dirty = true;
        }
        let origin = Vector3::new(x as f32, 0.0, y as f32);
        // only what changed crosses into the engine: a step redraws two
        // cells, a level arrives as a thousand of them
        let slots = std::mem::take(&mut nodes.solids);
        nodes.solids = self.place_solids((x, y), origin, slots, &old.solids, &look.solids);
        self.place_ring(&mut nodes, &look, origin);
        self.place_lamp(&mut nodes, &look, origin);
        if old.props != look.props {
            self.place_props(&mut nodes, &look, origin);
        }
        if let Some((kind, c, heart)) = look.blast
            && old.blast != look.blast
        {
            self.vfx
                .blast(kind, heart, c, origin + at(0.0, look.ground.max(0.0), 0.0));
            self.hear_blast(kind, (x, y));
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
        let here = (x, y);
        if let Some((kind, c, yaw, y)) = look.ray
            && old.ray != look.ray
        {
            self.hear_ray(kind, here);
            let mid = origin + at(0.0, y, 0.0);
            let along = yaw.map(|yaw| {
                let dir = Vector3::new(yaw.to_radians().sin(), 0.0, yaw.to_radians().cos()) * 0.55;
                (mid - dir, mid + dir)
            });
            self.draw_ray(kind, c, along, mid);
        }
        if (old.seen, old.masonry) != (look.seen, look.masonry) {
            self.fow_dirty = true;
        }
        // models: kept when the look is the same (only moved), else given
        // back to the pool and taken anew
        let mut kept: Vec<Model> = Vec::with_capacity(look.models.len());
        // a creature seen here and gone, not stepped off (it died, or went
        // out of sight): its model waits for the frame's messages (`fall`)
        let gone = old
            .entity
            .filter(|_| !old.hero && !old.sensed)
            .and_then(|i| old.models.get(i).zip(nodes.models.get(i)))
            .filter(|(b, _)| b.look.pose == Pose::Alive)
            .filter(|(b, _)| {
                let now = look.entity.and_then(|j| look.models.get(j));
                now.is_none_or(|p| p.look != b.look)
            })
            .zip(old.mon)
            .map(|((b, m), mon)| (m.node.instance_id(), *b, mon));
        // a monster seen come where none was (help summoned comes so)
        let came = look
            .entity
            .and_then(|j| look.models.get(j))
            .is_some_and(|p| p.look.pose == Pose::Alive && !look.hero && !look.sensed)
            && look.mon.is_some()
            && old.mon != look.mon;
        if came {
            self.appeared.push(((x, y), self.clock));
        }
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
                    self.give_old(m, gone, (x, y));
                }
                let hero = world
                    .map
                    .cell(x, y)
                    .and_then(|c| c.glyph.as_ref())
                    .is_some_and(|g| g.flags & mg::HERO != 0);
                let (secs, run) = match self.order_pace {
                    // an order's step lasts its tick, never a hurried run
                    Some(p) if hero && c.cells == 1 => (p, false),
                    // a burdened hero walks, slower the heavier the load
                    _ if hero => {
                        let (secs, run) = pace(c.cells, c.hurry);
                        self.hero_look.pace(secs, run)
                    }
                    _ => pace(c.cells, c.hurry),
                };
                let mut clips = self.art.clips(&c.model, run);
                // a floating hero glides, their legs still
                if hero && self.rider.is_none() && self.hero_look.floating() {
                    clips.gait = clips.idle.clone();
                }
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
            // the hero's own model: not the steed under them
            let own = hero_here && look.entity == Some(i) && look.rider.is_none();
            // off the steed: their model steps down from its back to
            // this cell (or stands where the steed fell under them)
            if own && let Some(r) = self.rider.take_if(|r| r.look == p.look) {
                if let Some(m) = reuse {
                    self.give_old(m, gone, (x, y));
                }
                let mut node = r.model.node.clone();
                let (from, to) = (node.get_position(), origin + p.pos);
                let facing = node.get_transform().basis.col_c();
                let yaw_from = facing.x.atan2(facing.z).to_degrees();
                if Vector2::new(from.x - to.x, from.z - to.z).length() > 0.05 {
                    let clips = self.art.clips(&r.model, false);
                    self.motions.push(Motion::step(
                        node,
                        (x, y),
                        true,
                        (from, yaw_from),
                        (to, p.yaw),
                        clips,
                        p.look.art.height,
                        pace(1, false),
                    ));
                } else {
                    let basis = Basis::from_euler(
                        EulerOrder::YXZ,
                        Vector3::new(0.0, p.yaw.to_radians(), 0.0),
                    );
                    node.set_transform(Transform3D::new(basis, to));
                }
                kept.push(r.model);
                continue;
            }
            // a creature on the cell (not the hero: their light rims them),
            // and the thing on top of what lies there: lit as one
            let creature = look.entity == Some(i)
                && matches!(p.look.pose, Pose::Alive | Pose::Ghost | Pose::Ware(_));
            let mut m = match (before, reuse) {
                (Some(b), Some(m)) if b.look == p.look => m,
                (_, other) => {
                    if let Some(m) = other {
                        self.give_old(m, gone, (x, y));
                    }
                    if own {
                        self.art.take_hero(&p.look)
                    } else if creature {
                        self.art.take_creature(&p.look)
                    } else {
                        self.art.take(&p.look)
                    }
                }
            };
            if before.map(|b| (b.pos, b.yaw, b.look)) != Some((p.pos, p.yaw, p.look)) {
                // a steed the hero gets on, or off, turns round where it
                // stands: its own steps, not a snap
                let ridden = look.entity == Some(i) && old.rider.is_some() != look.rider.is_some();
                let stood = before.filter(|b| ridden && (b.look, b.pos) == (p.look, p.pos));
                if let Some(b) = stood {
                    let clips = self.art.clips(&m, false);
                    self.motions.push(Motion::step(
                        m.node.clone(),
                        (x, y),
                        hero_here,
                        (origin + p.pos, b.yaw),
                        (origin + p.pos, p.yaw),
                        clips,
                        p.look.art.height,
                        (MOUNT_SECS, false),
                    ));
                } else {
                    let turn = Vector3::new(0.0, p.yaw.to_radians(), 0.0);
                    let basis = Basis::from_euler(EulerOrder::YXZ, turn);
                    m.node
                        .set_transform(Transform3D::new(basis, origin + p.pos));
                }
            }
            kept.push(m);
        }
        for m in old_models {
            self.give_old(m, gone, (x, y));
        }
        if let Some(c) = carried {
            self.art.give(c.model);
        }
        nodes.models = kept;
        // what a monster holds, taken in hand once its body is built
        if (look.wields.is_some() || old.wields.is_some()) && !self.to_arm.contains(&(x, y)) {
            self.to_arm.push((x, y));
        }
        // a saddle on the back of what stands here (a steed, a pet)
        if let Some(m) = look.entity.and_then(|i| nodes.models.get_mut(i)) {
            self.art.saddle(m, look.saddled);
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

    /// Whether the monster on `cell` holds something in its hand
    /// (self-tests).
    pub fn holds(&self, cell: (i32, i32)) -> bool {
        self.cells
            .get(&cell)
            .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)))
            .is_some_and(|m| self.art.holding(m))
    }

    /// The monsters of `to_arm` built by now take in hand what they hold
    /// (or put it down); those still stand-ins wait.
    fn arm_monsters(&mut self) {
        if self.to_arm.is_empty() {
            return;
        }
        let cells = std::mem::take(&mut self.to_arm);
        for cell in cells {
            let Some(nodes) = self.cells.get_mut(&cell) else {
                continue;
            };
            let Some(i) = nodes.look.entity.filter(|_| !nodes.look.hero) else {
                continue;
            };
            let held = nodes.look.wields;
            let Some(m) = nodes.models.get_mut(i) else {
                continue;
            };
            if m.is_pending() {
                self.to_arm.push(cell);
                continue;
            }
            self.art.arm(m, held);
        }
    }

    /// A cell's old model given back to the pool; the creature gone from
    /// it (`gone`: its node, how it stood, what it is) kept for its fall.
    fn give_old(&mut self, m: Model, gone: Option<(InstanceId, Placed, i32)>, cell: (i32, i32)) {
        match gone {
            Some((id, placed, mon)) if m.node.instance_id() == id => self.keep_gone(fall::Gone {
                cell,
                model: m,
                placed,
                mon,
                when: self.clock,
            }),
            _ => self.art.give(m),
        }
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
            // its last step cut short a moment ago: it runs
            let hurry =
                self.interrupted.contains(&mv.from) && self.clock - self.cut_at <= STEP_SECS as f64;
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

    /// The hero mounts: their own model leaves the cell they stood on (it
    /// would go back to the pool with that cell's look) and climbs onto
    /// the steed the map now draws them on.
    fn carry_rider(&mut self, from: Option<(i32, i32)>, world: &World) {
        let rides = world
            .map
            .hero()
            .and_then(|(x, y)| world.map.cell(x, y))
            .is_some_and(|c| c.rider.is_some());
        if !rides || self.rider.is_some() {
            return;
        }
        let Some(nodes) = from.and_then(|c| self.cells.get_mut(&c)) else {
            return;
        };
        // their own model: the hero's cell's entity, no steed's
        let own = nodes.look.hero && nodes.look.rider.is_none();
        let Some(i) = nodes.look.entity.filter(|&i| own && i < nodes.models.len()) else {
            return;
        };
        let model = nodes.models.remove(i);
        let placed = nodes.look.models.remove(i);
        nodes.look.entity = None;
        nodes.look.hero = false;
        let at = model.node.get_position();
        self.rider = Some(Rider {
            model,
            look: placed.look,
            on: None,
            from: Some((at, placed.yaw, 0.0)),
            seated: true,
        });
    }

    /// The hero gets off: the steed they leave turns a flank to the cell
    /// they step down to, not its head, which they would climb over.
    fn flank_steed(&mut self, from: Option<(i32, i32)>, hero: Option<(i32, i32)>, world: &World) {
        let (Some(from), Some(to)) = (from, hero) else {
            return;
        };
        if self.rider.is_none() || from == to {
            return;
        }
        // on foot now, the steed left behind where the pair stood
        let glyph = |(x, y): (i32, i32)| world.map.cell(x, y).and_then(|c| c.glyph.as_ref());
        let on_foot = glyph(to).is_some_and(|g| g.flags & mg::RIDDEN == 0);
        let (true, Some(id)) = (on_foot, glyph(from).and_then(Ident::of)) else {
            return;
        };
        let faced = self
            .cells
            .get(&from)
            .and_then(|n| n.look.entity.and_then(|i| n.look.models.get(i)))
            .map_or(self.hero_yaw, |p| p.yaw);
        // the flank nearer the way it faces now
        let toward = yaw_toward(from, to);
        let off = |yaw: f32| (yaw - faced + 540.0).rem_euclid(360.0) - 180.0;
        let flank = [toward + 90.0, toward - 90.0]
            .into_iter()
            .min_by(|a, b| off(*a).abs().total_cmp(&off(*b).abs()))
            .unwrap_or(toward);
        self.facing.insert(from, (id, flank));
    }

    /// The hero in the saddle: their own model for the steed the map
    /// draws on their cell (made here when they did not climb up from the
    /// ground: a game restored or a level reached mounted), bound to that
    /// steed's back; put away once the map draws no steed under them.
    fn seat_rider(&mut self, hero: Option<(i32, i32)>) {
        let steed = hero
            .and_then(|h| self.cells.get(&h))
            .filter(|n| n.look.hero)
            .and_then(|n| {
                let m = n.look.entity.and_then(|i| n.models.get(i))?;
                Some((n.look.rider?, m))
            });
        let Some((look, steed)) = steed else {
            if let Some(r) = self.rider.take() {
                self.art.give(r.model);
            }
            return;
        };
        // another form in the saddle (a polymorph): another model
        if let Some(r) = self.rider.take_if(|r| r.look != look) {
            self.art.give(r.model);
        }
        let r = self.rider.get_or_insert_with(|| Rider {
            model: self.art.take_hero(&look),
            look,
            on: None,
            from: None,
            seated: false,
        });
        let bound = r.on.as_ref().map(|(node, _)| node.instance_id());
        if bound != Some(steed.node.instance_id()) {
            r.on = self.art.mount(steed).map(|m| (steed.node.clone(), m));
        }
        // no steed built yet to sit on: not in the air meanwhile
        r.model.node.set_visible(r.on.is_some());
    }

    /// The steed under the hero on the layer their key and fills light,
    /// and a steed no longer ridden off it.
    fn light_steed(&mut self) {
        let steed = self
            .rider
            .as_ref()
            .and_then(|r| r.on.as_ref())
            .map(|(node, _)| node.clone());
        if steed.as_ref().map(|n| n.instance_id())
            == self.lit_steed.as_ref().map(|n| n.instance_id())
        {
            return;
        }
        // a model given back to the pool may be on its way out: leave it be
        let alive = |n: &Gd<Node3D>| n.is_instance_valid() && !n.is_queued_for_deletion();
        if let Some(old) = self.lit_steed.take().filter(alive) {
            set_layers(&old, 1);
        }
        if let Some(n) = steed.as_ref().filter(|n| alive(n)) {
            set_layers(n, 1 | STEED_LAYER);
        }
        self.lit_steed = steed;
    }

    /// The rider goes where the steed goes: their node on the steed's
    /// (the seat's solver lifts them onto its back), or on the way there
    /// from where they stood while they climb up.
    fn follow_steed(&mut self, delta: f32) {
        let Some(r) = self.rider.as_mut() else {
            return;
        };
        let Some((steed, _)) = r.on.as_ref().filter(|(n, _)| n.is_instance_valid()) else {
            return;
        };
        let to = steed.get_transform();
        let at = match r.from.as_mut() {
            Some((from, yaw, secs)) => {
                *secs += delta;
                let t = *secs / MOUNT_SECS;
                let facing = to.basis.col_c();
                let yaw = turn(*yaw, facing.x.atan2(facing.z).to_degrees(), t);
                let hop = Vector3::UP * (MOUNT_HOP * (std::f32::consts::PI * t.min(1.0)).sin());
                Transform3D::new(
                    Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw.to_radians(), 0.0)),
                    from.lerp(to.origin, ease(t)) + hop,
                )
            }
            None => to,
        };
        if r.from.is_some_and(|(.., secs)| secs >= MOUNT_SECS) {
            r.from = None;
        }
        r.model.node.set_transform(at);
        // as high over their own node as the saddle lifts them, facing as
        // their model does
        let seat = r.on.as_ref().and_then(|(_, mount)| mount.at());
        let lift = seat.map_or(0.0, |s| s.y - at.origin.y - r.look.art.height * HIPS_HEIGHT);
        let node = &r.model.node;
        let facing = node
            .get_child(0)
            .and_then(|c| c.try_cast::<Node3D>().ok())
            .map_or(node.get_global_transform(), |c| c.get_global_transform());
        self.rider_view.set_global_transform(Transform3D::new(
            facing.basis,
            node.get_global_position() + Vector3::UP * lift,
        ));
    }

    /// The node that stands for the hero where they alone are looked at
    /// (the inventory's doll): their model's on foot; in the saddle, one
    /// as far under them as their feet would be if they stood up there.
    pub fn hero_view(&self) -> Option<Gd<Node3D>> {
        match &self.rider {
            Some(r) if r.on.is_some() => Some(self.rider_view.clone()),
            _ => self.hero_model().map(|m| m.node.clone()),
        }
    }

    /// The hero's model is on the hero's own layer (`RIM_LAYER`, the one
    /// the portrait's and the doll's cameras see): it is put there once
    /// the level is drawn.
    pub fn hero_on_layer(&self) -> bool {
        match (&self.rim_model, self.hero_model()) {
            (Some(rim), Some(hero)) => rim.instance_id() == hero.node.instance_id(),
            _ => false,
        }
    }

    /// Whether a model's meshes are on the layer the hero's key and fills
    /// light besides their own: a ridden steed's (self-tests).
    pub fn on_steed_layer(node: &Gd<Node3D>) -> bool {
        node.find_children_ex("*")
            .type_("GeometryInstance3D")
            .owned(false)
            .done()
            .iter_shared()
            .filter_map(|n| n.try_cast::<GeometryInstance3D>().ok())
            .any(|g| g.get_layer_mask() & STEED_LAYER != 0)
    }

    /// Whether a model's meshes are on the hero's rim layer (self-tests:
    /// only the hero's own are).
    pub fn on_rim(node: &Gd<Node3D>) -> bool {
        node.find_children_ex("*")
            .type_("GeometryInstance3D")
            .owned(false)
            .done()
            .iter_shared()
            .filter_map(|n| n.try_cast::<GeometryInstance3D>().ok())
            .any(|g| g.get_layer_mask() & RIM_LAYER != 0)
    }

    /// The hero healed (a potion's, a spell's, a prayer's): a glow and
    /// stars rising round them, after a drink's own glow.
    fn show_healed(&mut self) {
        let Some((feet, tall)) = self.hero_stands() else {
            return;
        };
        let node = self.hero_model().map(|m| m.node.clone());
        self.hero_fx.after(
            HEALED_AT,
            crate::hero::Fx::Body {
                name: "healed",
                kind: VfxKind::Heal,
                feet,
                tall,
                anchor: node,
            },
        );
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
        // each blow told and found on the map; a death told after a blow
        // falls where that blow landed
        let mut fights = Vec::new();
        let mut deaths = Vec::new();
        let mut struck: Option<(String, (i32, i32))> = None;
        let mut unplaced = Vec::new();
        for m in world.log.since(seen).filter(|m| !m.from_history) {
            // what a monster does that the hero sees: a spell cast
            self.watch_act(&m.text, world, catalog);
            if crate::hero::healed(&m.text) {
                self.show_healed();
            }
            if let Some(a) = parse_attack(&m.text) {
                let hit = !m.text.contains(" miss");
                let cells = locate_attack(&a, &world.map, names);
                // a kill falls: where the blow found it, else where it
                // stood (a ray's, a missile's, a newt's that leaves nothing)
                if let Who::Named(n) = &a.target
                    && a.killed()
                {
                    self.deaths
                        .push(fall::Told::new(n, cells.map(|c| c.1), self.clock));
                }
                if let Some(cells) = cells {
                    if let Who::Named(n) = &a.target {
                        struck = Some((n.clone(), cells.1));
                    }
                    fights.push((cells, hit, a));
                } else if let Some(hero) = self
                    .hero_at
                    .filter(|_| a.attacker == Who::Hero || a.target == Who::Hero)
                {
                    // the hero's own fight is heard where they stand even
                    // when the map cannot show it (a newt killed leaves
                    // nothing behind)
                    unplaced.push((hero, hit, a));
                }
            } else if let Some(name) = hearing::died(&m.text) {
                let at = struck
                    .as_ref()
                    .filter(|(n, _)| n == name)
                    .map(|(_, at)| *at);
                self.deaths.push(fall::Told::new(name, at, self.clock));
                if let Some(at) = at {
                    deaths.push((name.to_string(), at));
                }
            } else if let Some(name) = fall::slain(&m.text) {
                // a ghost by its name, a pet: no fight the map can place
                self.deaths.push(fall::Told::new(name, None, self.clock));
            }
        }
        // blows told together (a fast hero's two, a monster's claws and
        // bite) sound one after another, as they were struck
        const NEXT_BLOW: f32 = 0.25;
        let mut blows = 0;
        for ((from, to), hit, a) in fights {
            let at_hero = a.target == Who::Hero;
            let start = self.sounds.len();
            self.hear_blow(from, to, &a, hit, world, catalog);
            for s in &mut self.sounds[start..] {
                s.delay += blows as f32 * NEXT_BLOW;
            }
            blows += 1;
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
                    // a blow at the hero in the saddle is the rider's to
                    // reel from, not the steed's
                    let m = match self.rider.as_ref().filter(|_| at_hero) {
                        Some(r) => &r.model,
                        None => m,
                    };
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
        for (at, hit, a) in unplaced {
            let start = self.sounds.len();
            self.hear_blow(at, at, &a, hit, world, catalog);
            for s in &mut self.sounds[start..] {
                s.delay += blows as f32 * NEXT_BLOW;
            }
            blows += 1;
        }
        // a death told after its blows falls after them
        let start = self.sounds.len();
        for (name, at) in deaths {
            self.hear_death(&name, at, catalog);
        }
        let last = blows.max(1) - 1;
        for s in &mut self.sounds[start..] {
            s.delay += last as f32 * NEXT_BLOW;
        }
    }

    /// Where a sound of the cell `(x, y)` is placed: a body's height
    /// over its ground.
    fn sound_at(&self, (x, y): (i32, i32)) -> Vector3 {
        Vector3::new(x as f32, self.ground(x, y) + 0.8, y as f32)
    }

    /// The steps of what moved, each on its floor: the hero's own (none
    /// while they float or fly; the steed's under a rider) and those of
    /// monsters the hero sees. One only sensed (telepathy, detection: the
    /// engine tells it, and a blind hero senses all) walks unheard.
    fn hear_steps(&mut self, moves: &[Move], world: &World, catalog: &Catalog) {
        // the foot lands a little into the step
        const LANDS: f32 = 0.08;
        if moves.is_empty() {
            return;
        }
        let senses = hearing::Senses::of(world, catalog);
        for mv in moves {
            let Some(cell) = world.map.cell(mv.to.0, mv.to.1) else {
                continue;
            };
            let Some(g) = cell.glyph.as_ref().filter(|g| g.kind == GlyphKind::Mon) else {
                continue;
            };
            let hero = g.flags & mg::HERO != 0;
            let sensed = cell.sensed || g.flags & mg::DETECT != 0 || senses.blind;
            if (!hero && sensed) || (hero && senses.afloat && cell.rider.is_none()) {
                continue;
            }
            let Some(info) = monster_info(catalog, g.mon) else {
                continue;
            };
            let floor = cell
                .terrain
                .as_ref()
                .and_then(|t| cmap_sym(t, catalog))
                .and_then(|sym| hearing::floor(terrain_of(sym), self.branch));
            let (Some(gait), Some(floor)) = (hearing::gait(info), floor) else {
                continue;
            };
            let at = self.sound_at(mv.to);
            for i in 0..mv.steps().max(1) {
                let mut step = Sound::exactly(hearing::step(floor, gait))
                    .at(at)
                    .sized(Size::of(&info.size))
                    .after(LANDS + i as f32 * STEP_SECS * 0.5);
                if hero {
                    step = step.urgent();
                }
                self.sounds.push(step);
            }
        }
    }

    /// A blow told: its swing where it starts, then as it lands its hit
    /// or its miss and the cry of whoever takes it (the hero's grunt, a
    /// creature's hurt, or its death when the blow killed).
    fn hear_blow(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        a: &nh_world::AttackMsg,
        hit: bool,
        world: &World,
        catalog: &Catalog,
    ) {
        let by_hero = a.attacker == Who::Hero;
        let held = by_hero
            .then(|| self.hero_gear.gear().hand_r.as_ref())
            .flatten()
            .map(|h| self.art.manifest().held_at(h.held).0.to_string());
        let with = |what: &str, last: &str| {
            let mut names = hearing::armed(what, held.as_deref());
            names.push(last.to_string());
            names
        };
        let body = hearing::blow(&a.verb);
        let (swing, landing) = if by_hero {
            (Some(with("swing", "swing")), with("hit", "hit"))
        } else if body.is_empty() {
            (Some(vec!["swing".to_string()]), vec!["hit".to_string()])
        } else {
            (None, vec![format!("hit.{body}"), "hit".to_string()])
        };
        let (from_at, to_at) = (self.sound_at(from), self.sound_at(to));
        let urgent = |s: Sound| if by_hero { s.urgent() } else { s };
        if let Some(names) = swing {
            self.sounds
                .push(urgent(Sound::exactly(names).at(from_at).after(0.06)));
        }
        if !hit || a.missed() {
            let miss = Sound::exactly(vec!["miss".to_string()]).at(to_at);
            self.sounds.push(urgent(miss.after(CONTACT_SECS)));
            return;
        }
        self.sounds.push(urgent(
            Sound::exactly(landing).at(to_at).after(CONTACT_SECS),
        ));
        match &a.target {
            Who::Hero => {
                let female = world
                    .map
                    .hero_form()
                    .is_some_and(|g| g.flags & mg::FEMALE != 0);
                let sex = if female { "female" } else { "male" };
                let names = vec![format!("hero.hurt.{sex}"), "hero.hurt".to_string()];
                let cry = Sound::exactly(names).at(to_at).urgent();
                self.sounds.push(cry.after(CONTACT_SECS + 0.05));
            }
            Who::Named(name) if a.killed() => self.hear_death(name, to, catalog),
            Who::Named(name) => {
                if let Some(info) = hearing::monster_named(catalog, name) {
                    let plan = hearing::plan(info);
                    let names = vec![format!("hurt.{plan}"), "hurt".to_string()];
                    let cry = Sound::exactly(names).at(to_at).sized(Size::of(&info.size));
                    self.sounds.push(cry.after(CONTACT_SECS + 0.05));
                }
            }
        }
    }

    /// A creature dies where the hero sees it: its last cry and its fall.
    fn hear_death(&mut self, name: &str, at: (i32, i32), catalog: &Catalog) {
        let Some(info) = hearing::monster_named(catalog, name) else {
            return;
        };
        let plan = hearing::plan(info);
        let names = vec![format!("die.{plan}"), "die".to_string()];
        let fall = Sound::exactly(names)
            .at(self.sound_at(at))
            .sized(Size::of(&info.size));
        self.sounds.push(fall.after(CONTACT_SECS + 0.1));
    }

    /// A ray seen lighting a cell: its travel is heard where it is first
    /// seen; the cells it lights next are the same bolt going on.
    fn hear_ray(&mut self, kind: Ray, cell: (i32, i32)) {
        /// A piece within this of the last of its kind is the same bolt.
        const GOES_ON: f64 = 0.4;
        let now = self.clock;
        let same = self
            .last_ray
            .is_some_and(|(_, k, t)| k == kind && now - t <= GOES_ON);
        self.last_ray = Some((cell, kind, now));
        if !same {
            let name = hearing::kinded("ray", hearing::ray_name(kind));
            let sound = Sound::new(name).at(self.sound_at(cell));
            // a breath's ray goes once the mouth is open
            self.sounds.push(sound.after(self.ray_wait));
        }
    }

    /// An explosion seen: heard once, where its first cell is drawn (its
    /// heart, or a cell beside it).
    fn hear_blast(&mut self, kind: Blast, cell: (i32, i32)) {
        /// Cells drawn within this of the first are the same explosion.
        const ONE_BLAST: f64 = 0.3;
        let now = self.clock;
        if now - self.last_blast <= ONE_BLAST {
            return;
        }
        self.last_blast = now;
        let name = hearing::kinded("blast", hearing::blast_name(kind));
        self.sounds.push(Sound::new(name).at(self.sound_at(cell)));
    }

    /// Where the last ray seen lit a cell (a bounce's or a hit's line
    /// sounds there).
    pub fn last_ray_cell(&self) -> Option<(i32, i32)> {
        self.last_ray.map(|(c, _, _)| c)
    }

    /// The sounds the map's changes made since the last call.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.sounds)
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
        self.strike_with(from, to, world, None);
    }

    /// As `strike`, with `clip` in place of the attack clip (a spell cast).
    fn strike_with(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        world: &World,
        clip: Option<String>,
    ) {
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
        let node = nodes.models[i].node.clone();
        // from the saddle the blow is the rider's: the steed under them
        // turns to it
        let mut clips = match self.rider.as_ref().filter(|_| hero) {
            Some(r) => self.art.clips(&r.model, false),
            None => self.art.clips(&nodes.models[i], false),
        };
        if hero && let Some(clip) = self.mounted_blow() {
            clips.attack = Some(clip);
        }
        if let Some(c) = clip {
            clips.attack = Some(c);
        }
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

    /// A blow from the saddle with what is wielded, where that has a way
    /// of its own there (a lance couched): in the rider's hand as its
    /// clip wants it, for as long as the clip lasts. The clip.
    fn mounted_blow(&mut self) -> Option<String> {
        let r = self.rider.as_mut()?;
        let h = self.hero_gear.gear().hand_r?;
        let held = &self.art.manifest().held_at(h.held).1;
        let mounted = held.apply.as_ref()?.mounted.clone()?;
        let clip = self.art.clip_on(&r.model, mounted.clip.clone())?;
        let secs = r.model.player()?.get_animation(clip.as_str())?.get_length();
        self.art
            .hold_for(&mut r.model, h, secs, "hand_r", None, Some(&mounted));
        Some(clip)
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
        self.finish_motions_where(|_| true);
    }

    /// End the motions `pick` takes, as `finish_motions`; the others go on.
    fn finish_motions_where(&mut self, pick: impl Fn(&Motion) -> bool) {
        // a batch with nothing under way keeps what the last cut short:
        // the engine may draw the cell a step leaves a frame before the
        // one it reaches, and that step still follows one cut short
        if !self.motions.iter().any(&pick) {
            return;
        }
        self.interrupted.clear();
        self.cut_at = self.clock;
        let walking = self.order_pace.is_some();
        let (done, going): (Vec<_>, Vec<_>) = self.motions.drain(..).partition(|m| pick(m));
        self.motions = going;
        for m in done {
            let (cell, step) = (m.cell, m.is_step());
            let keep = walking && m.hero && step;
            // one a self-test holds midway is cut short too
            if !m.arrived(self.hold) && step {
                self.interrupted.insert(cell);
            }
            m.finish(keep);
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

    /// The hero's model plays its idle clip (after an order's last step):
    /// the body that walked, the steed's under a rider.
    fn settle_hero(&mut self) {
        let walker = self
            .hero_cell()
            .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)));
        let Some(model) = walker else {
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
        let Some(model) = self.hero_model() else {
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

    /// The hero's blow is being played: their model (on a steed, the
    /// pair) lunges at what it strikes, off their cell (self-tests).
    pub fn hero_striking(&self) -> bool {
        self.motions.iter().any(|m| m.hero && !m.is_step())
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
            mark_texture(d, tex);
            mark_size(d, Vector3::new(size, 0.24, size));
            mark_color(d, color.with_alpha(0.75));
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

/// A torch hangs on the wall at (x, y) by the torches' pattern (`north`: a
/// room's north wall, else a side wall): every fifth cell of the north
/// walls, every fourth of the east and west ones.
fn torch_pattern(x: i32, y: i32, north: bool) -> bool {
    if north {
        (x + 2 * y).rem_euclid(5) == 0
    } else {
        (y + 2 * x).rem_euclid(4) == 0
    }
}

/// The walls of a lit area a torch hangs on, and the way each faces into
/// the area: the torches' pattern on the north, east and west walls (the
/// camera does not see the south wall's face), at least one per area. One
/// a room gets off the pattern hangs where the room's dressing leaves the
/// wall bare (`dressed`: something of it may hang there).
fn torch_walls(
    area: &[(i32, i32)],
    is_wall: impl Fn(i32, i32) -> bool,
    dressed: impl Fn(&TorchWall) -> bool,
) -> Vec<TorchWall> {
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
        .filter(|&((x, y), (dx, _))| torch_pattern(x, y, dx == 0))
        .collect();
    if !chosen.is_empty() {
        return chosen;
    }
    // none fell on the pattern: the north wall's middle, or the nearest
    // to it that is bare; else any bare wall
    let north: Vec<_> = walls.iter().filter(|(_, (dx, _))| *dx == 0).collect();
    let mid = north.len() / 2;
    let bare = (0..north.len())
        .filter(|&k| !dressed(north[k]))
        .min_by_key(|&k| k.abs_diff(mid))
        .map(|k| north[k]);
    bare.or(walls.iter().find(|w| !dressed(w)))
        .or(north.get(mid).copied())
        .or(walls.first())
        .map(|&w| vec![w])
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

/// The inside of what engulfs the hero, while they are swallowed: the
/// swallow glyphs round them name it (eight a monster), one that is not
/// NetHack's own engulfer a fold of hide.
fn engulfer(world: &World, catalog: &Catalog, (hx, hy): (i32, i32)) -> Option<Inside> {
    if !world.map.is_swallowed() {
        return None;
    }
    let g = (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (hx + dx, hy + dy)))
        .filter_map(|(x, y)| world.map.cell(x, y)?.glyph.as_ref())
        .find(|g| g.kind == GlyphKind::Swallow)?;
    let mon = g
        .glyph
        .and_then(|n| n.checked_sub(catalog.glyphs.swallow))
        .map(|n| n / 8);
    Some(
        monster_info(catalog, mon)
            .and_then(|m| Inside::of(&m.name))
            .unwrap_or(Inside::Maw(Flesh::Hide)),
    )
}

/// The box `node`'s meshes take in the world at rest (None: none shown).
fn world_box(node: &Gd<Node3D>) -> Option<Aabb> {
    if !node.is_instance_valid() || !node.is_visible_in_tree() {
        return None;
    }
    node.find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<MeshInstance3D>().ok())
        .filter(|m| m.is_visible_in_tree())
        .map(|m| m.get_global_transform() * m.get_aabb())
        // (a mesh that failed to build has NaN bounds: no box at all)
        .filter(|b| b.position.is_finite() && b.size.is_finite())
        .reduce(|a, b| a.merge(b))
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
                hero_pit: None,
                branch: &self.branch,
                volume: None,
            }
        }

        fn look(&self, cell: &Cell) -> Look {
            look_of(cell, Near::default(), &self.ctx())
        }

        /// The look of a cell of a level (`level`), as a level of `branch`.
        fn look_in(&self, map: &MapState, (x, y): (i32, i32), branch: Branch) -> Look {
            let look = crate::branch_look::look_of(branch);
            let plan = dressing::plan(|x, y| plan_cell(map, &self.cat, x, y), &look.dress);
            let ctx = Ctx {
                branch: &look,
                volume: plan.get(&(x, y)).copied(),
                ..self.ctx_at(x, y, None)
            };
            let cell = map.cell(x, y).cloned().unwrap_or_default();
            look_of(&cell, Near::of(map, x, y), &ctx)
        }
    }

    /// Where a level's picture lies on the map.
    const PICTURE_AT: (i32, i32) = (10, 5);

    /// A level from its picture: `.` a lit floor, `#` a corridor, `+` a
    /// doorway, `|` and `-` walls, `r 7 L J` their corners, `}` water, `~`
    /// ice, `^` lava, `>` stairs down, `_` an altar; the Planes' `W` deep
    /// water, `a` air and `c` cloud.
    fn level(cat: &Catalog, rows: &[&str]) -> MapState {
        let mut map = MapState::new();
        for (row, line) in rows.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let sym = match ch {
                    '.' => "S_room",
                    '#' => "S_corr",
                    '+' => "S_ndoor",
                    '|' => "S_vwall",
                    '-' => "S_hwall",
                    'r' => "S_tlcorn",
                    '7' => "S_trcorn",
                    'L' => "S_blcorn",
                    'J' => "S_brcorn",
                    '}' => "S_pool",
                    '~' => "S_ice",
                    '^' => "S_lava",
                    '>' => "S_dnstair",
                    '_' => "S_altar",
                    'W' => "S_water",
                    'a' => "S_air",
                    'c' => "S_cloud",
                    _ => continue,
                };
                let (x, y) = (PICTURE_AT.0 + col as i32, PICTURE_AT.1 + row as i32);
                map.print(x, y, &cmap(cat, sym), None);
            }
        }
        map
    }

    /// How high the cells of a level's picture stand, row by row.
    fn heights(f: &Fixture, map: &MapState, branch: Branch, cols: i32, rows: i32) -> Vec<Vec<f32>> {
        (0..rows)
            .map(|row| {
                (0..cols)
                    .map(|col| {
                        let at = (PICTURE_AT.0 + col, PICTURE_AT.1 + row);
                        f.look_in(map, at, branch).ground
                    })
                    .collect()
            })
            .collect()
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
            over: None,
            ..Cell::default()
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
            over: None,
            ..Cell::default()
        }
    }

    #[test]
    fn a_shore_lies_over_both_cells_and_banks_stand_only_by_walls() {
        let f = Fixture::new();
        let cat = &f.cat;
        // a pool between a floor and a wall, ice and lava beyond
        let map = level(cat, &["|..}}.", "|.}}~~", "|.^.~~"]);
        let at = |c: i32, r: i32| (PICTURE_AT.0 + c, PICTURE_AT.1 + r);
        let look = |c, r| f.look_in(&map, at(c, r), Branch::Main);
        let liquid = |l: &Look, liq: Liquid| {
            l.solids
                .iter()
                .filter(|s| s.paint == Paint::Liquid(liq))
                .map(|s| s.pos.y)
                .collect::<Vec<_>>()
        };
        let floors = |l: &Look| {
            l.solids
                .iter()
                .filter(|s| matches!(s.paint, Paint::Pbr(_, _, Role::Floor)))
                .count()
        };
        let ice = |l: &Look| {
            l.solids
                .iter()
                .any(|s| matches!(s.paint, Paint::Pbr(_, _, Role::Ice)))
        };
        // the floor beside the pool has the water under it, as high as the
        // pool's own; the pool a lip of the floor; each its own kind
        let floor = look(1, 0);
        assert_eq!(floor.kind, Ground::Land);
        assert_eq!(liquid(&floor, Liquid::Water), vec![SURFACE_Y]);
        let pool = look(3, 0);
        assert_eq!(pool.kind, Ground::Water);
        assert_eq!(liquid(&pool, Liquid::Water), vec![SURFACE_Y]);
        assert!(floors(&pool) >= 1, "no floor lip on the pool");
        // ice and the pool beside each other; lava and the floor
        assert!(ice(&look(3, 1)), "no ice under the pool by the ice");
        assert_eq!(liquid(&look(4, 1), Liquid::Water), vec![SURFACE_Y]);
        assert_eq!(liquid(&look(1, 2), Liquid::Lava), vec![SURFACE_Y]);
        assert!(floors(&look(2, 2)) >= 1, "no floor lip on the lava");
        // far from any other ground, nothing but its own
        let alone = look(5, 2);
        assert!(liquid(&alone, Liquid::Water).is_empty() && floors(&alone) == 0);
        // the pool's banks: none towards the ground west of it (the shore
        // wanders over both cells), one towards a wall west of another
        let west_bank = |l: &Look| {
            l.solids
                .iter()
                .filter(|s| matches!(s.paint, Paint::Pbr(_, 45, Role::Prop)))
                .any(|s| s.pos.y < 0.0 && s.pos.x < -0.4)
        };
        assert!(!west_bank(&pool));
        let by_wall = level(cat, &["|}"]);
        assert!(west_bank(&f.look_in(&by_wall, at(1, 0), Branch::Main)));
    }

    /// A hall of the main dungeon: a doorway in its north wall, stairs
    /// by it (a pilaster's cell at their corner), an altar on its floor.
    const HALL: [&str; 7] = [
        "r-----------+-----------7",
        "|..>....................|",
        "|.......................|",
        "|................._.....|",
        "|.......................|",
        "|.......................|",
        "L-----------------------J",
    ];

    /// A cell of a level as `branch` has it, and as it would be with no
    /// dressing.
    fn dressed_and_bare(
        f: &Fixture,
        map: &MapState,
        branch: Branch,
        at: (i32, i32),
    ) -> (Look, Look) {
        let bare = BranchLook {
            dress: crate::branch_look::Dress::NONE,
            ..crate::branch_look::look_of(branch)
        };
        let ctx = Ctx {
            branch: &bare,
            ..f.ctx_at(at.0, at.1, None)
        };
        let cell = map.cell(at.0, at.1).cloned().unwrap_or_default();
        let plain = look_of(&cell, Near::of(map, at.0, at.1), &ctx);
        (f.look_in(map, at, branch), plain)
    }

    /// The corners of the box round a solid as it is turned and placed on
    /// its cell.
    fn corners(s: &Solid) -> Vec<Vector3> {
        let m = crate::meshes::metres;
        let (half, lift) = match s.mesh {
            MeshKey::Box(x, y, z) | MeshKey::Prism(x, y, z) | MeshKey::Bevel(x, y, z, _) => {
                (Vector3::new(m(x), m(y), m(z)) / 2.0, 0.0)
            }
            // (standing on its foot)
            MeshKey::Rock(x, y, z) => (Vector3::new(m(x), m(y), m(z)) / 2.0, m(y) / 2.0),
            MeshKey::Cylinder(t, b, h) => {
                let r = m(t.max(b));
                (Vector3::new(r, m(h) / 2.0, r), 0.0)
            }
            MeshKey::Torus(i, o) => (Vector3::new(m(o), m(o - i) / 2.0, m(o)), 0.0),
            mesh => (Vector3::ONE * mesh.half_height(), 0.0),
        };
        // (a ball the same any way it is turned)
        let round = matches!(s.mesh, MeshKey::Sphere(_) | MeshKey::Facets(..));
        let t = solid_transform(Vector3::ZERO, s);
        let t = if round {
            Transform3D::new(Basis::IDENTITY, s.pos)
        } else {
            t
        };
        (0..8)
            .map(|k| {
                let sign = |bit: i32| if k & bit == 0 { -1.0 } else { 1.0 };
                let c = Vector3::new(sign(1) * half.x, sign(2) * half.y + lift, sign(4) * half.z);
                t * c
            })
            .collect()
    }

    /// What a room's dressing adds to a cell of a level.
    fn dressing_at(f: &Fixture, map: &MapState, branch: Branch, at: (i32, i32)) -> Vec<Solid> {
        let (dressed, bare) = dressed_and_bare(f, map, branch, at);
        let mut plain = bare.solids;
        dressed
            .solids
            .into_iter()
            .filter(|s| match plain.iter().position(|p| p == s) {
                Some(i) => {
                    plain.remove(i);
                    false
                }
                None => true,
            })
            .collect()
    }

    #[test]
    fn a_rooms_dressing_keeps_off_doors_and_features() {
        let f = Fixture::new();
        let map = level(&f.cat, &HALL);
        let dressing = |c: i32, r: i32| {
            dressing_at(&f, &map, Branch::Main, (PICTURE_AT.0 + c, PICTURE_AT.1 + r))
        };
        // the doorway, the stairs, the altar, and every cell round them
        for (c, r) in [(12, 0), (3, 1), (18, 3)] {
            for (dc, dr) in (-1..=1).flat_map(|dc| (-1..=1).map(move |dr| (dc, dr))) {
                let (c, r) = (c + dc, r + dr);
                if r >= 0 {
                    assert!(dressing(c, r).is_empty(), "dressed at {c},{r}");
                }
            }
        }
        // elsewhere its north wall is (a pilaster every third cell)
        assert!((1..24).any(|c| !dressing(c, 0).is_empty()));
    }

    #[test]
    fn a_rooms_dressing_keeps_off_what_the_hero_and_a_pile_may_hide() {
        let f = Fixture::new();
        let cat = &f.cat;
        // the hero come down by the north wall (a pilaster's cell at their
        // corner) and a pile by the west wall, on ground never shown bare:
        // the engine tells no stairs nor altar under them
        let mut map = level(
            cat,
            &[
                "r-----------------------7",
                "|..@....................|",
                "|.......................|",
                "|(......................|",
                "|.......................|",
                "L-----------------------J",
            ],
        );
        let (hero, pile) = (
            (PICTURE_AT.0 + 3, PICTURE_AT.1 + 1),
            (PICTURE_AT.0 + 1, PICTURE_AT.1 + 3),
        );
        let valkyrie = monster(cat, "valkyrie", mg::HERO | mg::FEMALE);
        map.print(hero.0, hero.1, &valkyrie, None);
        map.print(pile.0, pile.1, &object(cat, "(", "chest"), None);
        for (x, y) in [hero, pile] {
            for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
                let at = (x + dx, y + dy);
                assert!(
                    dressing_at(&f, &map, Branch::Main, at).is_empty(),
                    "dressed at {at:?}"
                );
            }
        }
        // the hero steps off plain floor: the wall by it is dressed
        map.print(hero.0, hero.1, &cmap(cat, "S_room"), None);
        let corner = (PICTURE_AT.0 + 2, PICTURE_AT.1);
        assert!(!dressing_at(&f, &map, Branch::Main, corner).is_empty());
    }

    #[test]
    fn a_rooms_dressing_hides_nobody_and_lies_flat_on_its_floor() {
        for branch in [Branch::Main, Branch::Gehennom] {
            a_rooms_dressing_hides_nobody_in(branch);
        }
    }

    fn a_rooms_dressing_hides_nobody_in(branch: Branch) {
        let f = Fixture::new();
        let map = level(&f.cat, &HALL);
        let silk = f.art.material("cobweb");
        let (mut webs, mut floor, mut seams) = (0, 0, 0);
        for r in 0..7 {
            for c in 0..25 {
                let at = (PICTURE_AT.0 + c, PICTURE_AT.1 + r);
                let (dressed, bare) = dressed_and_bare(&f, &map, branch, at);
                for s in dressing_at(&f, &map, branch, at) {
                    let box_ = corners(&s);
                    let low = box_.iter().map(|p| p.y).fold(f32::MAX, f32::min);
                    let top = box_.iter().map(|p| p.y).fold(f32::MIN, f32::max);
                    let out = box_
                        .iter()
                        .map(|p| p.x.abs().max(p.z.abs()))
                        .fold(0.0, f32::max);
                    if !dressed.masonry {
                        // on the floor and in its cell: flat in it, a sunk
                        // stone ankle high at most
                        floor += 1;
                        assert!(top <= 0.07, "{c},{r}: {s:?}");
                        assert!(s.pos.x.abs().max(s.pos.z.abs()) < 0.5, "{c},{r}: {s:?}");
                        continue;
                    }
                    // on a wall: never over its top (it would hide what is
                    // behind it); what stands up never stands out into the
                    // room where somebody stands by it (what lies flat on
                    // the floor hides nobody, and keeps to the wall's foot)
                    assert!(top <= WALL_HEIGHT + 0.01, "{c},{r}: {s:?}");
                    seams += usize::from(matches!(s.paint, Paint::Flat(_, Finish::Ember)));
                    if !matches!(s.paint, Paint::Pbr(m, _, _) if Some(m) == silk) {
                        // a hand past a face (a pier's stands a little out
                        // of a wall's), over anybody's head further
                        let reach = if top <= 0.02 {
                            0.75
                        } else if low >= dressing::OVERHEAD - 0.01 {
                            0.9
                        } else {
                            0.55 + dressing::REACH + 0.005
                        };
                        assert!(out <= reach, "{c},{r}: {s:?}");
                        continue;
                    }
                    // a cobweb high in a corner, over anybody's head
                    webs += 1;
                    assert!(low >= dressing::WEB_LOW - 0.01, "{c},{r}: {s:?}");
                    assert!(s.pos.x.abs().min(s.pos.z.abs()) > 0.5, "{c},{r}: {s:?}");
                }
                // nothing of it makes a cell taller to the pointer
                if !dressed.masonry {
                    assert_eq!(dressed.top, bare.top, "{c},{r}");
                }
            }
        }
        // the far corners hold a pier or a cobweb, Gehennom's walls seams
        // of its fire
        let dress = crate::branch_look::look_of(branch).dress;
        let corners = [(0, 0), (24, 0)]
            .iter()
            .filter(|&&(c, r)| {
                !dressing_at(&f, &map, branch, (PICTURE_AT.0 + c, PICTURE_AT.1 + r)).is_empty()
            })
            .count();
        assert!(floor > 0, "{branch:?}");
        assert!(webs == 0 || dress.cobwebs, "{branch:?}: {webs}");
        assert_eq!(corners > 0, dress.cobwebs || dress.piers, "{branch:?}");
        assert_eq!(seams > 0, dress.embers, "{branch:?}: {seams}");
    }

    #[test]
    fn a_rooms_floor_is_dressed_only_where_it_shows_bare() {
        let f = Fixture::new();
        let cat = &f.cat;
        let mut map = level(cat, &HALL);
        let inside =
            (1..6).flat_map(|r| (1..24).map(move |c| (PICTURE_AT.0 + c, PICTURE_AT.1 + r)));
        let mut dressed = inside.filter(|&at| !dressing_at(&f, &map, Branch::Main, at).is_empty());
        let (x, y) = dressed.next().expect("no floor dressed");
        // something lying there: nothing of it; somebody standing there:
        // all of it, as before
        let before = f.look_in(&map, (x, y), Branch::Main).solids;
        map.print(x, y, &object(cat, "(", "chest"), Some(&cmap(cat, "S_room")));
        assert!(dressing_at(&f, &map, Branch::Main, (x, y)).is_empty());
        map.print(x, y, &monster(cat, "jackal", 0), Some(&cmap(cat, "S_room")));
        let jackal = f.look_in(&map, (x, y), Branch::Main).solids;
        assert!(before.iter().all(|s| jackal.contains(s)));
    }

    #[test]
    fn a_rooms_plan_arches_its_long_walls_and_inlays_its_floor() {
        let f = Fixture::new();
        let map = level(&f.cat, &HALL);
        let t = |x: i32, y: i32| map.cell(x, y).and_then(|c| cell_terrain(c, &f.cat));
        let dress = crate::branch_look::look_of(Branch::Main).dress;
        let plan = dressing::plan(|x, y| plan_cell(&map, &f.cat, x, y), &dress);
        let (mut arches, mut lines, mut corners) = (0, 0, 0);
        for (&(x, y), v) in &plan {
            // nothing on or round the doorway, the stairs or the altar
            let near = (-1..=1).any(|dy| (-1..=1).any(|dx| t(x + dx, y + dy).is_some_and(matters)));
            assert!(!near, "{x},{y}: {v:?}");
            match *v {
                // whole arches, on a north wall over the room's floor
                dressing::Volume::Arch { west } => {
                    arches += 1;
                    let other = if west { (x + 1, y) } else { (x - 1, y) };
                    assert_eq!(
                        plan.get(&other),
                        Some(&dressing::Volume::Arch { west: !west }),
                        "{x},{y}"
                    );
                    assert_eq!(t(x, y), Some(Terrain::Wall));
                    assert_eq!(t(x, y + 1), Some(Terrain::Floor));
                }
                // lines in the floor's own cell
                dressing::Volume::Inlay {
                    lines: l,
                    n,
                    corner,
                } => {
                    assert_eq!(t(x, y), Some(Terrain::Floor));
                    for line in &l[..n as usize] {
                        assert!(
                            line[..4].iter().all(|v| v.abs() <= 0.5 + 1e-4),
                            "{x},{y}: {line:?}"
                        );
                    }
                    lines += n as usize;
                    corners += usize::from(corner.is_some());
                }
                // a small room's: there is none in the hall
                dressing::Volume::North(_) | dressing::Volume::Pier => {
                    panic!("{x},{y}: {v:?} in a hall")
                }
            }
        }
        assert!(
            arches >= 2 && lines > 20 && corners > 0,
            "{arches} {lines} {corners}"
        );
        // a branch that dresses none of it
        let sokoban = crate::branch_look::look_of(Branch::Sokoban).dress;
        assert!(dressing::plan(|x, y| plan_cell(&map, &f.cat, x, y), &sokoban).is_empty());
    }

    #[test]
    fn a_small_rooms_north_wall_is_dressed_whole_and_its_corners_are_piers() {
        let f = Fixture::new();
        let dress = crate::branch_look::look_of(Branch::Main).dress;
        let plan_of = |rows: &[&str]| {
            let map = level(&f.cat, rows);
            dressing::plan(|x, y| plan_cell(&map, &f.cat, x, y), &dress)
        };
        let at = |c: i32, r: i32| (PICTURE_AT.0 + c, PICTURE_AT.1 + r);
        use dressing::{Piece, Volume};
        // five wide: a niche between two columns, piers in both corners
        let plan = plan_of(&["r-----7", "|.....|", "|.....|", "|.....|", "L-----J"]);
        assert_eq!(plan.get(&at(0, 0)), Some(&Volume::Pier));
        assert_eq!(plan.get(&at(6, 0)), Some(&Volume::Pier));
        assert_eq!(
            plan.get(&at(3, 0)),
            Some(&Volume::North(Some(Piece::Alcove)))
        );
        assert_eq!(
            plan.get(&at(2, 0)),
            Some(&Volume::North(Some(Piece::Pilaster)))
        );
        assert_eq!(
            plan.get(&at(4, 0)),
            Some(&Volume::North(Some(Piece::Pilaster)))
        );
        assert_eq!(plan.get(&at(1, 0)), Some(&Volume::North(None)));
        // four wide: a pair of columns in the middle
        let plan = plan_of(&["r----7", "|....|", "|....|", "L----J"]);
        assert_eq!(
            plan.get(&at(2, 0)),
            Some(&Volume::North(Some(Piece::Pilaster)))
        );
        assert_eq!(
            plan.get(&at(3, 0)),
            Some(&Volume::North(Some(Piece::Pilaster)))
        );
        // a door beside the corner and in the north wall: nothing round them
        let plan = plan_of(&["r-+---7", "|.....|", "|.....+", "L-----J"]);
        assert_eq!(plan.get(&at(0, 0)), Some(&Volume::Pier));
        for c in 1..=3 {
            assert!(
                !matches!(plan.get(&at(c, 0)), Some(Volume::North(Some(_)))),
                "{c}"
            );
        }
        // a room seen in part (its south wall not yet): nothing
        assert!(plan_of(&["r-----7", "|.....|", "|.....|"]).is_empty());
        // a branch that builds none of it
        let sokoban = crate::branch_look::look_of(Branch::Sokoban).dress;
        let map = level(&f.cat, &["r-----7", "|.....|", "|.....|", "L-----J"]);
        assert!(dressing::plan(|x, y| plan_cell(&map, &f.cat, x, y), &sokoban).is_empty());
    }

    #[test]
    fn a_rooms_floor_is_inlaid_only_once_its_walls_are_seen_all_round() {
        let f = Fixture::new();
        let dress = crate::branch_look::look_of(Branch::Main).dress;
        let inlays = |rows: &[&str]| {
            let map = level(&f.cat, rows);
            dressing::plan(|x, y| plan_cell(&map, &f.cat, x, y), &dress)
                .values()
                .filter(|v| matches!(v, dressing::Volume::Inlay { .. }))
                .count()
        };
        assert!(inlays(&HALL) > 0);
        // its south wall not seen yet
        assert_eq!(inlays(&HALL[..6]), 0);
        // a dark room seen in part: what is known of its floor is a
        // rectangle, its east edge no wall
        let west: Vec<&str> = HALL.iter().map(|row| &row[..15]).collect();
        assert_eq!(inlays(&west), 0);
    }

    #[test]
    fn a_cave_s_walls_with_a_door_in_them_are_built() {
        fn terrain<'a>(
            map: &'a MapState,
            cat: &'a Catalog,
        ) -> impl Fn(i32, i32) -> Option<Terrain> + 'a {
            move |x, y| map.cell(x, y).and_then(|c| cell_terrain(c, cat))
        }
        let f = Fixture::new();
        let t = |map| terrain(map, &f.cat);
        let at = |c: i32, r: i32| (PICTURE_AT.0 + c, PICTURE_AT.1 + r);
        // a shop with its door in a cave of no doors: its walls only
        let town = level(
            &f.cat,
            &[
                "r-----------------------7",
                "|.......................|",
                "|..r--+--7..............|",
                "|..|.....|..............|",
                "|..L-----J..............|",
                "|.......................|",
                "L-----------------------J",
            ],
        );
        let built = built_walls(t(&town), BUILT_MOST);
        assert_eq!(built.len(), 16, "{built:?}");
        assert!(built.contains(&at(5, 2)) && built.contains(&at(3, 4)));
        assert!(!built.contains(&at(0, 0)) && !built.contains(&at(12, 6)));
        // one built against the cave's own walls: joined up with them, more
        // than a building's, the cave's
        let against = level(
            &f.cat,
            &[
                "r-----+--7--------------7",
                "|........|..............|",
                "|........|..............|",
                "L--------J--------------J",
            ],
        );
        assert!(built_walls(t(&against), 30).is_empty());
    }

    #[test]
    fn a_cobweb_never_hangs_by_a_torch() {
        let dress = crate::branch_look::look_of(Branch::Main).dress;
        let mut webs = 0;
        for (x, y) in (0..ROWNO).flat_map(|y| (0..COLNO).map(move |x| (x, y))) {
            for sx in [1, -1] {
                if !dressing::webbed(x, y, sx, &dress) {
                    continue;
                }
                webs += 1;
                // on the faces beside its corner no torch by the pattern,
                // nor one a room gets off it
                assert!(!torch_pattern(x + sx, y, true) && !torch_pattern(x, y + 1, false));
                assert!(dressing::hangs(x + sx, y, (0, 1), &dress, None));
                assert!(dressing::hangs(x, y + 1, (sx, 0), &dress, None));
            }
        }
        assert!(webs > 0);
    }

    #[test]
    fn every_branch_dresses_its_rooms_in_materials_of_its_own() {
        let art = manifest();
        let branches = [
            Branch::Main,
            Branch::Mines,
            Branch::Sokoban,
            Branch::Gehennom,
            Branch::Quest,
            Branch::Ludios,
            Branch::Vlad,
        ];
        for b in branches {
            let d = crate::branch_look::look_of(b).dress;
            for name in d.banners.iter().chain(["moss", "cobweb", d.iron].iter()) {
                assert!(art.material(name).is_some(), "{b:?}: {name}");
            }
        }
    }

    /// The camera's ray, PITCH_DEG down and looking north, meeting the
    /// ground at the centre of cell (10, 9).
    fn ray() -> (Vector3, Vector3) {
        let pitch = PITCH_DEG.to_radians();
        let back = Vector3::new(0.0, pitch.sin(), pitch.cos());
        (Vector3::new(10.0, 0.0, 9.0) + back * DISTANCE, -back)
    }

    /// The Planes of Air and Water looked like debug tiles: a grid of
    /// translucent cubes, flat blue squares. Air there is no floor (open
    /// over a sea of clouds far below), a cloud soft puffs of its own;
    /// water is the deep, with no bed under it, and the air of a bubble in
    /// it is a floor with a skin on each side the water meets.
    #[test]
    fn the_planes_of_air_and_water_are_sky_and_deep() {
        let f = Fixture::new();
        let liquids = |look: &Look| -> Vec<Liquid> {
            look.solids
                .iter()
                .filter_map(|s| match s.paint {
                    Paint::Liquid(l) => Some(l),
                    _ => None,
                })
                .collect()
        };
        let air = Branch::Planes(nh_world::Plane::Air);
        let water = Branch::Planes(nh_world::Plane::Water);
        let sky = level(&f.cat, &["acc", "aaa"]);
        let at = |col: i32, row: i32| (PICTURE_AT.0 + col, PICTURE_AT.1 + row);
        assert!(f.look_in(&sky, at(0, 0), air).solids.is_empty());
        let cloud = f.look_in(&sky, at(1, 0), air);
        assert_eq!(liquids(&cloud), [Liquid::Cloud; CLOUD_PUFFS as usize]);
        assert!(
            cloud.solids.iter().all(|s| !s.shadow),
            "a cloud throws no shadow"
        );
        // the deep round a bubble two cells wide: each air cell's skin (a
        // curved meniscus) on its three sides that meet the water, none
        // between them
        let deep = level(&f.cat, &["WWWW", "WaaW", "WWWW"]);
        let bubble = f.look_in(&deep, at(1, 1), water);
        let skins = liquids(&bubble)
            .iter()
            .filter(|&&l| l == Liquid::Membrane)
            .count();
        assert_eq!(liquids(&bubble)[0], Liquid::Bubble);
        assert_eq!(skins, 3 * MENISCUS.len());
        assert_eq!(liquids(&f.look_in(&deep, at(0, 0), water)), [Liquid::Deep]);
        // elsewhere the same cells as they were
        let main = f.look_in(&deep, at(1, 1), Branch::Main);
        assert!(liquids(&main).is_empty());
    }

    /// Every boulder and stone sat in a loot spotlight: what lies on the
    /// floor gives light by what it is.
    #[test]
    fn a_boulder_gives_no_light_and_a_ring_its_full_glint() {
        use crate::art::Sheen;
        let cat = catalog();
        let sheen = |class: &str, appearance: &str| {
            let t = cat
                .object_tiles
                .iter()
                .find(|t| t.class == class && t.appearance == appearance)
                .unwrap_or_else(|| panic!("no {class} {appearance}"));
            sheen_of(t)
        };
        assert_eq!(sheen("`", "boulder"), Sheen::None);
        assert_eq!(sheen("*", "rock"), Sheen::None);
        assert_eq!(sheen("*", "gray"), Sheen::None);
        assert_eq!(sheen("*", "red"), Sheen::Full);
        assert_eq!(sheen("0", "heavy iron ball"), Sheen::Faint);
        for class in ["/", "=", "\"", "!", "?"] {
            let t = cat.object_tiles.iter().find(|t| t.class == class).unwrap();
            assert_eq!(sheen_of(t), Sheen::Full, "{class}");
        }
    }

    /// The Astral Plane was a beige square in grey nothing: its floor is
    /// slabs of marble floating over the stars, a gilded lip where
    /// nothing is beside them, and its altars are lit from above.
    #[test]
    fn the_astral_plane_floats_among_the_stars() {
        let f = Fixture::new();
        let astral = Branch::Planes(nh_world::Plane::Astral);
        let map = level(&f.cat, &["...", "._.", "..."]);
        let at = |col: i32, row: i32| (PICTURE_AT.0 + col, PICTURE_AT.1 + row);
        let corner = f.look_in(&map, at(0, 0), astral);
        // the slab under it, and a lip on its north and west sides
        let lips = corner
            .solids
            .iter()
            .filter(|s| s.pos.y < 0.0 && s.pos.y > -0.1)
            .count();
        assert!(corner.solids.iter().any(|s| s.pos.y < -0.1), "no slab");
        assert_eq!(lips, 2, "{:?}", corner.solids);
        let middle = f.look_in(&map, at(1, 0), astral);
        let lips = middle
            .solids
            .iter()
            .filter(|s| s.pos.y < 0.0 && s.pos.y > -0.1)
            .count();
        assert_eq!(lips, 1);
        assert_eq!(f.look_in(&map, at(1, 1), astral).lamp, Some(Lamp::Holy));
        assert_eq!(f.look_in(&map, at(1, 1), Branch::Main).lamp, None);
    }

    #[test]
    fn a_swimmer_floats_down_to_its_waterline_in_water_only() {
        let f = Fixture::new();
        let cat = &f.cat;
        let shark = monster(cat, "shark", 0);
        let in_water = Cell {
            glyph: Some(shark.clone()),
            bk: None,
            terrain: Some(cmap(cat, "S_water")),
            over: None,
            ..Cell::default()
        };
        let y = |l: &Look| l.models[l.entity.unwrap()].pos.y;
        let (wet, dry) = (f.look(&in_water), f.look(&on_floor(cat, shark)));
        let info = cat.monsters.iter().find(|m| m.name == "shark").unwrap();
        let r = f.art.monster(info, 0);
        let swim = f.art.model_at(r.model).1.swim * r.scale;
        assert!(swim > 0.1, "{swim}");
        assert_eq!(y(&dry), 0.0);
        assert!(
            (y(&wet) - (wet.ground - swim)).abs() < 1e-5,
            "{} {}",
            y(&wet),
            wet.ground
        );
    }

    #[test]
    fn whoever_is_in_a_door_last_seen_shut_stands_on_its_floor() {
        let f = Fixture::new();
        let cat = &f.cat;
        let y = |l: &Look| l.models[l.entity.unwrap()].pos.y;
        for sym in ["S_hcdoor", "S_vcdoor"] {
            let shut = feature(cat, sym);
            assert!(
                f.look(&shut).ground >= CUT_HEIGHT,
                "{sym}: a shut door's top"
            );
            // in it as the engine draws them: no ground under them, the door
            // remembered shut
            for (who, g) in [
                ("an orc", monster(cat, "hill orc", 0)),
                ("the hero", monster(cat, "valkyrie", mg::HERO | mg::FEMALE)),
            ] {
                let cell = Cell {
                    glyph: Some(g),
                    ..shut.clone()
                };
                let look = f.look(&cell);
                assert_eq!(look.ground, 0.0, "{sym}: {who} stands on its floor");
                assert_eq!(y(&look), 0.0, "{sym}: {who}");
                // the leaf swung aside, as an open door's
                let open = sym.replace('c', "o");
                let drawn = f.look(&Cell {
                    glyph: cell.glyph.clone(),
                    ..feature(cat, &open)
                });
                assert_eq!(look.solids, drawn.solids, "{sym}: {who}");
            }
        }
        // a bolt passing over a shut door leaves it shut
        let mut bolt = feature(cat, "S_hcdoor");
        bolt.glyph = Some(Glyph {
            kind: GlyphKind::Zap,
            ..cmap(cat, "S_hcdoor")
        });
        assert!(f.look(&bolt).ground >= CUT_HEIGHT);
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
    fn a_rooms_walls_step_down_towards_the_camera() {
        let f = Fixture::new();
        // (a column of nothing known on each side: the rock outside)
        let map = level(
            &f.cat,
            &[
                "        ", " r----7 ", " |....| ", " |....+#", " |....| ", " |....| ", " |....| ",
                " L----J ", "        ",
            ],
        );
        let h = heights(&f, &map, Branch::Main, 8, 9);
        let (cut, half, full) = (CUT_HEIGHT, HALF_HEIGHT, WALL_HEIGHT);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        // the north wall stands, its corners a little taller
        assert!(h[1][2..6].iter().all(|&v| near(v, full)), "{:?}", h[1]);
        assert!(h[1][1] > full && h[1][6] > full);
        // the south wall is stumps (the floor lies north of it), and its
        // corners with it: no pier stands at the front of a room
        assert!(h[7][2..6].iter().all(|&v| near(v, cut)), "{:?}", h[7]);
        assert!(h[7][1] < 0.5 && h[7][6] < 0.5, "{:?}", h[7]);
        // the west wall: whole down to the cell over the corner, that one
        // broken off half way
        let west: Vec<f32> = (2..7).map(|row| h[row][1]).collect();
        assert!(west[..4].iter().all(|&v| near(v, full)), "{west:?}");
        assert!(near(west[4], half), "{west:?}");
        // the east wall: whole over its doorway; under it a stump, the
        // next half its height, and the one over the corner too
        let east: Vec<f32> = (2..7).map(|row| h[row][6]).collect();
        assert!(near(east[0], full) && near(east[1], 0.0), "{east:?}");
        assert!(near(east[2], cut), "{east:?}");
        assert!(near(east[3], half) && near(east[4], half), "{east:?}");
        // the rock outside steps down with them: tall beside the walls
        // that stand, half over the corner's row, a lip beside the corner
        // and south of it
        let outside: Vec<f32> = (1..9).map(|row| h[row][0]).collect();
        assert!(
            outside[..5].iter().all(|&v| near(v, ROCK_HEIGHT)),
            "{outside:?}"
        );
        assert!(near(outside[5], ROCK_HALF), "{outside:?}");
        assert!(
            near(outside[6], ROCK_CUT) && near(outside[7], ROCK_CUT),
            "{outside:?}"
        );
        assert!(h[8][1..7].iter().all(|&v| near(v, ROCK_CUT)), "{:?}", h[8]);
        // a doorway two cells over the corner: stumps all the way down
        let low = level(&f.cat, &["r--7", "|..+", "|..|", "|..|", "L--J"]);
        let h = heights(&f, &low, Branch::Main, 4, 5);
        assert!(
            near(h[2][3], cut) && near(h[3][3], cut) && h[4][3] < 0.5,
            "{h:?}"
        );
        // no torch hangs on what is not whole
        let (x, y) = (PICTURE_AT.0 + 3, PICTURE_AT.1 + 2);
        assert!(!f.look_in(&low, (x, y), Branch::Main).wall);
    }

    #[test]
    fn a_caves_rock_keeps_no_sides_before_open_ground() {
        let f = Fixture::new();
        let map = level(
            &f.cat,
            &[
                "r----7", "|....|", "|....|", "|....|", "L----J", "------", "------",
            ],
        );
        let h = heights(&f, &map, Branch::Mines, 6, 7);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        // the back wall stands
        assert!(h[0].iter().all(|&v| near(v, WALL_HEIGHT)), "{:?}", h[0]);
        // the rock at the sides: whole only beside the far row of the
        // floor (nothing open lies north of it); a lip from there on,
        // wherever open ground lies north and to a side
        assert!(near(h[1][0], WALL_HEIGHT) && near(h[1][5], WALL_HEIGHT));
        for row in &h[2..5] {
            assert!(near(row[0], ROCK_CUT) && near(row[5], ROCK_CUT), "{row:?}");
        }
        // in front of the floor a lip, behind the lip rock half its
        // height, and whole behind that
        assert!(h[4].iter().all(|&v| near(v, ROCK_CUT)), "{:?}", h[4]);
        assert!(h[5].iter().all(|&v| near(v, HALF_HEIGHT)), "{:?}", h[5]);
        assert!(
            h[6][1..5].iter().all(|&v| near(v, WALL_HEIGHT)),
            "{:?}",
            h[6]
        );
    }

    #[test]
    fn a_side_walls_torch_keeps_clear_of_whoever_stands_beside_it() {
        // the sconce's own depth (Torch_Metal), and half a body's width
        // about the middle of the cell, half a metre from the wall
        let depth = 0.39 * SCONCE_SCALE;
        let body = 0.5 - 0.25;
        assert!(depth * SIDE_REACH <= body + 0.02, "{}", depth * SIDE_REACH);
        // the lantern hung there instead: its middle and half its width
        // (the chandelier is 0.29 m across as it is drawn)
        let lantern = (LANTERN_OUT + 0.145) * SIDE_LANTERN;
        assert!(lantern <= body + 0.02, "{lantern}");
    }

    #[test]
    fn stones_lie_sunk_at_the_foot_of_walls_and_none_on_swept_floors() {
        let f = Fixture::new();
        let cat = &f.cat;
        let (floor, wall) = (feature(cat, "S_room"), feature(cat, "S_vwall"));
        // floors with a wall on both sides, all along a row
        let stones = |branch: Branch| -> Vec<Solid> {
            let look = crate::branch_look::look_of(branch);
            (1..40)
                .flat_map(|x| {
                    let ctx = Ctx {
                        branch: &look,
                        ..f.ctx_at(x, 7, None)
                    };
                    let near = Near::orth([None, None, Some(&wall), Some(&wall)]);
                    look_of(&floor, near, &ctx).solids
                })
                .filter(|s| matches!(s.mesh, MeshKey::Facets(..)))
                .collect()
        };
        for b in [Branch::Main, Branch::Mines, Branch::Gehennom] {
            let stones = stones(b);
            assert!(stones.len() > 5, "{b:?}: {}", stones.len());
            // sunk to their middle or deeper: a stone, not a ball on the floor
            for s in &stones {
                let MeshKey::Facets(r, _) = s.mesh else {
                    unreachable!()
                };
                assert!(s.pos.y < 0.0 && s.pos.y + crate::meshes::metres(r) > 0.0);
            }
        }
        // a built place's floors are swept (on Sokoban's planks a stone
        // reads as a thing to pick up)
        for b in [Branch::Sokoban, Branch::Ludios, Branch::Vlad] {
            assert!(stones(b).is_empty(), "{b:?}");
        }
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
                ..Near::default()
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
        let lip = at(south, None);
        assert!((lip.ground - ROCK_CUT).abs() < 1e-3);
        // a lip is no taller than it stands: it hides nobody's feet
        assert!((lip.top - ROCK_CUT).abs() < 1e-3);
        let mut behind = none;
        behind[0] = Some(&wall);
        assert!((at(behind, Some(&floor)).ground - ROCK_CUT).abs() < 1e-3);
        // south and to a side of a corridor (the camera sees past its
        // corner to what stands there): low too
        for i in [4, 5] {
            let mut diagonal = none;
            diagonal[i] = Some(&corr);
            assert!((at(diagonal, None).ground - ROCK_CUT).abs() < 1e-3, "{i}");
        }
        // behind a corridor (north of it, or north and to a side): tall
        for i in [1, 6, 7] {
            let mut back = none;
            back[i] = Some(&corr);
            assert!((at(back, None).ground - ROCK_HEIGHT).abs() < 1e-3, "{i}");
        }
        // outside a wall it stands tall, unless open ground lies a row
        // further in front of it still (it would stand, a black block,
        // over the stumps and ledges before that ground)
        for i in 0..3 {
            let mut beyond = [None; 3];
            beyond[i] = Some(&floor);
            let near = Near {
                cells: behind,
                far_north: None,
                beyond,
            };
            let r = look_of(&unknown, near, &f.ctx());
            assert!((r.ground - ROCK_CUT).abs() < 1e-3, "{i}");
        }
        assert!((at(behind, None).ground - ROCK_HEIGHT).abs() < 1e-3);
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
            ..Near::default()
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
                over: None,
                ..Cell::default()
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
    fn an_opening_in_the_floor_is_cut_away_with_nobody_on_it() {
        let f = Fixture::new();
        let cat = &f.cat;
        let tile = |l: &Look| {
            l.solids
                .iter()
                .any(|s| s.mesh == plane(1.0, 1.0) && s.pos == Vector3::ZERO)
        };
        for (sym, o) in [
            ("S_hole", Opening::Hole),
            ("S_trap_door", Opening::TrapDoor),
            ("S_pit", Opening::Pit { spiked: false }),
            ("S_spiked_pit", Opening::Pit { spiked: true }),
        ] {
            // nothing on it: its floor cut away round it to the shaft and
            // its bottom, the rock under the level cut too
            let open = f.look(&feature(cat, sym));
            assert_eq!(open.opening, Some(o), "{sym}");
            assert!(open.sunk && !tile(&open), "{sym}: open");
            let bottom = open.solids.iter().map(|s| s.pos.y).fold(0.0, f32::min);
            assert!((bottom + o.depth()).abs() < 0.01, "{sym}: down to {bottom}");
            // a trap door's lid shut over it
            let mut lid = open.clone();
            assert_eq!(
                opening::unlid(&mut lid).is_some(),
                o == Opening::TrapDoor,
                "{sym}"
            );
            // a newt on it (caught or not, the map cannot tell): whole
            let newt = Cell {
                glyph: Some(monster(cat, "newt", 0)),
                terrain: Some(cmap(cat, sym)),
                ..Cell::default()
            };
            let shut = f.look(&newt);
            assert!(!shut.sunk && tile(&shut), "{sym}: shut under a newt");
        }
        // the hero down in a pit they did not see (its notice tells it):
        // their floor open round them; the pit beside them is another's
        let hero = on_floor(cat, monster(cat, "valkyrie", mg::HERO | mg::FEMALE));
        let in_pit = |at: (i32, i32)| {
            let ctx = Ctx {
                hero_pit: Some(false),
                ..f.ctx_at(10, 10, Some(at))
            };
            look_of(&hero, Near::default(), &ctx)
        };
        let down = in_pit((10, 10));
        assert_eq!(down.opening, Some(Opening::Pit { spiked: false }));
        assert!(down.sunk && !tile(&down));
        let beside = in_pit((11, 10));
        assert_eq!(beside.opening, None);
        assert!(!beside.sunk && tile(&beside));
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
        // a red ring under the pointer for all but the hero, pets and the
        // peaceful
        assert!(newt.hostile && !pet.hostile);
        assert!(!look(monster(cat, "human", mg::HERO)).hostile);
        let peaceful = Cell {
            peaceful: true,
            ..on_floor(cat, monster(cat, "newt", 0))
        };
        assert!(!f.look(&peaceful).hostile);
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
        // and the wall below that stump their feet, seen from close: it
        // is broken off half way, and carries no torch
        let doorway = feature(cat, "S_ndoor");
        let stump = feature(cat, "S_vwall");
        let below = |sym: &str, ctx: &Ctx| {
            let near = Near {
                far_north: Some(&doorway),
                ..Near::orth([Some(&stump), None, None, None])
            };
            look_of(&feature(cat, sym), near, ctx)
        };
        let half = below("S_vwall", &f.ctx());
        assert_eq!(half.top, HALF_HEIGHT);
        assert!(!half.wall && half.masonry);
        assert!(
            half.solids
                .iter()
                .any(|s| matches!(s.paint, Paint::Pbr(_, _, Role::Ruin)))
        );
        // a cave's rock wall the same
        let mines = crate::branch_look::look_of(Branch::Mines);
        let cave = Ctx {
            branch: &mines,
            ..f.ctx()
        };
        assert_eq!(below("S_vwall", &cave).ground, HALF_HEIGHT);
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
            over: None,
            ..Cell::default()
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
    fn something_unseen_is_bent_air_with_its_letter_under_the_pointer() {
        let f = Fixture::new();
        let cat = &f.cat;
        let mut g = glyph(GlyphKind::Invisible, 'I');
        g.color = 7;
        let look = f.look(&on_floor(cat, g));
        // the air bent as if a body stood there, standing on the ground
        assert!(
            look.solids
                .iter()
                .any(|s| s.paint == Paint::Liquid(Liquid::Unseen) && !s.shadow)
        );
        // no ball, and the letter only under the pointer or in the overview
        assert!(
            !look
                .solids
                .iter()
                .any(|s| matches!(s.mesh, MeshKey::Sphere(_)))
        );
        assert_eq!(look.letters.len(), 1);
        assert!(look.letters[0].hint);
        // the pointer finds it as tall as a body
        assert!(look.top >= 1.0);
    }

    #[test]
    fn long_things_lie_along_a_diagonal_of_their_cell() {
        let f = Fixture::new();
        let cat = &f.cat;
        let ware = |x: i32, appearance: &str| {
            let look = look_of(
                &on_floor(cat, object(cat, ")", appearance)),
                Near::default(),
                &f.ctx_at(x, 4, None),
            );
            *look.models.last().expect("a model")
        };
        // a polearm: along one of the four diagonals, a few degrees askew,
        // and each cell's its own
        let mut ways = HashSet::new();
        for x in 1..40 {
            let p = ware(x, "long poleaxe");
            assert!(matches!(p.look.pose, Pose::Ware(_)));
            assert!(crate::art::on_diagonal(&p.look.art));
            let off = (p.yaw - 45.0).rem_euclid(90.0);
            let off = off.min(90.0 - off);
            assert!(off <= crate::art::WARE_JITTER + 1e-3, "{}", p.yaw);
            ways.insert(((p.yaw - 45.0) / 90.0).round() as i32);
        }
        assert_eq!(ways.len(), 4, "{ways:?}");
        // a dagger any way at all
        let daggers: HashSet<i32> = (1..40).map(|x| ware(x, "dagger").yaw as i32 / 30).collect();
        assert!(daggers.len() > 6, "{daggers:?}");
        assert!(!crate::art::on_diagonal(&ware(1, "dagger").look.art));
    }

    #[test]
    fn effects_flash_over_what_is_there() {
        let f = Fixture::new();
        let cat = &f.cat;
        let mut cell = on_floor(cat, cmap(cat, "S_vbeam"));
        let beam = f.look(&cell);
        // a piece of a ray along the cell (the effects draw it), no solid
        let (_, _, yaw, y) = beam.ray.expect("a ray");
        assert_eq!(yaw, Some(0.0));
        assert!(y > 0.3);
        assert_eq!(beam.solids.len(), 1, "only the floor");
        let spark = f.look(&on_floor(cat, cmap(cat, "S_ss1")));
        assert!(spark.glint.is_some() && spark.ray.is_none());
        // a zap glyph's ray is what its number says: four glyphs, one
        // per way, for each beam type (fire is NetHack's second)
        let fire = Glyph {
            glyph: Some(cat.glyphs.zap + 4 + 1),
            ..glyph(GlyphKind::Zap, '-')
        };
        let (kind, _, yaw, _) = f.look(&on_floor(cat, fire)).ray.expect("a ray");
        assert_eq!((kind, yaw), (Ray::Fire, Some(90.0)));
        // a wand of digging's beam: dust, its way told by its next piece
        let (kind, _, yaw, _) = f
            .look(&on_floor(cat, cmap(cat, "S_digbeam")))
            .ray
            .expect("a ray of dust");
        assert_eq!((kind, yaw), (Ray::Dig, None));
        // the engulfer is drawn around the hero, not per cell: not even a
        // floor round them
        cell.glyph = Some(glyph(GlyphKind::Swallow, '/'));
        assert!(f.look(&cell).is_empty());
        // unexplored cells draw nothing; an unseen floor under a monster does
        assert!(f.look(&Cell::default()).is_empty());
        let under = Cell {
            glyph: Some(monster(cat, "newt", 0)),
            bk: None,
            terrain: None,
            over: None,
            ..Cell::default()
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
            over: None,
            ..Cell::default()
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
        let torches = torch_walls(&area, wall, |_| false);
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
        let closet_wall = |x: i32, y: i32| (x, y) != (6, 5) && (5..=7).contains(&x);
        let one = torch_walls(&closet, closet_wall, |_| false);
        assert_eq!(one, vec![((6, 4), (0, 1))]);
        // off the pattern it keeps clear of the room's dressing
        let dressed = |w: &TorchWall| w.0 == (6, 4);
        let aside = torch_walls(&closet, closet_wall, dressed);
        assert_eq!(aside, vec![((5, 5), (1, 0))]);
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
        // what of an up stair stands as tall as a body thins out between
        // the eye and whoever stands north of it, as a wall does: none of
        // it is flat paint (the dark beyond its top step was)
        for s in up.solids.iter().filter(|s| s.pos.y > 0.5) {
            assert!(
                matches!(s.paint, Paint::Pbr(_, _, r) if !matches!(r, Role::Floor | Role::Void)),
                "{:?} at {:?}",
                s.paint,
                s.pos
            );
        }
        // a full wall carries a torch, a wall cut down does not
        assert!(f.look(&feature(cat, "S_hwall")).wall);
    }

    #[test]
    fn what_passes_over_a_cell_leaves_who_stands_there() {
        let f = Fixture::new();
        let cat = &f.cat;
        let under = |g: &Glyph, over: Glyph| Cell {
            over: Some(over),
            ..on_floor(cat, g.clone())
        };
        let alone = |g: &Glyph| f.look(&on_floor(cat, g.clone()));
        let hero = monster(cat, "valkyrie", mg::HERO | mg::FEMALE);
        let pet = monster(cat, "kitten", mg::PET);
        let dagger = object(cat, ")", "dagger");
        // a thrown dagger drawn on the hero's cell as it reaches them: the
        // hero is still the cell's entity, their model where it was among
        // the cell's, the dagger one more
        let (stood, hit) = (alone(&hero), f.look(&under(&hero, dagger.clone())));
        assert!(stood.hero && hit.hero);
        assert_eq!(hit.entity, stood.entity);
        assert_eq!(hit.models.len(), stood.models.len() + 1);
        assert_eq!(hit.models[..stood.models.len()], stood.models[..]);
        // a ray over the pet: the pet and its ring stay, the ray flashes
        let (stood, rayed) = (
            alone(&pet),
            f.look(&under(&pet, glyph(GlyphKind::Zap, '-'))),
        );
        assert!(stood.ray.is_none() && rayed.ray.is_some());
        assert_eq!(
            (rayed.entity, &rayed.models, rayed.ring),
            (stood.entity, &stood.models, stood.ring)
        );
        assert!(stood.ring.is_some() && !rayed.hero);
        // a sparkle over the hero, an explosion over a thing
        let sparkled = f.look(&under(&hero, cmap(cat, "S_ss1")));
        assert!(sparkled.hero && sparkled.glint.is_some());
        let blasted = f.look(&under(&dagger, glyph(GlyphKind::Explosion, '#')));
        assert_eq!(blasted.models, alone(&dagger).models);
        assert!(blasted.blast.is_some() && blasted.glint.is_none());
        // what an explosion is of, and its middle, are in its number: nine
        // glyphs a kind (a scroll of fire's is fiery, NetHack's sixth)
        let heart = Glyph {
            glyph: Some(cat.glyphs.explode + 5 * 9 + 4),
            ..glyph(GlyphKind::Explosion, ' ')
        };
        let (kind, _, middle) = f.look(&under(&dagger, heart)).blast.expect("a blast");
        assert_eq!((kind, middle), (Blast::Fiery, true));
        let edge = Glyph {
            glyph: Some(cat.glyphs.explode + 6 * 9),
            ..glyph(GlyphKind::Explosion, '/')
        };
        let (kind, _, middle) = f.look(&under(&dagger, edge)).blast.expect("a blast");
        assert_eq!((kind, middle), (Blast::Frosty, false));
        // the floor drawn over a hero who steps off it (the next cell comes
        // a frame later), or the pet they swap places with: nothing to add
        for over in [cmap(cat, "S_room"), pet] {
            assert_eq!(f.look(&under(&hero, over)), alone(&hero));
        }
        // what an unseen hero stands on is not the hero
        assert!(!alone(&dagger).hero);
    }
}
