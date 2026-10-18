//! What opens in a floor: a pit, a hole, a trap door. With nothing on
//! it (or the hero down in the pit), its cell's floor is cut away round
//! it to a shaft, its walls of what the level is dug in, the rock under
//! the level cut too: the hero sinks into a pit and drops through a hole
//! as into one, not through the floor's surface (they were seen drowning
//! in it). A pit is as deep as the hero sinks into it, its earthen bottom
//! under their feet; a hole and a trap door's shaft go down into the
//! dark. Under a monster (whose glyph hides whether it is caught in it)
//! the floor stays whole, the opening a dark patch on it. A child of
//! map_view.

use godot::prelude::*;

use super::{DEEP, Finish, Look, MeshKey, Paint, Role, Solid, at, bevel, cuboid, plane, prism};

/// An opening in the floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// A pit, its spikes when the hero has seen them.
    Pit {
        spiked: bool,
    },
    Hole,
    /// A hole under a lid of planks.
    TrapDoor,
}

impl Opening {
    /// The opening a trap's symbol is (None: a trap on the floor).
    pub fn of(sym: &str) -> Option<Opening> {
        match sym {
            "S_pit" => Some(Opening::Pit { spiked: false }),
            "S_spiked_pit" => Some(Opening::Pit { spiked: true }),
            "S_hole" => Some(Opening::Hole),
            "S_trap_door" => Some(Opening::TrapDoor),
            _ => None,
        }
    }

    /// How far its shaft goes down (metres).
    pub fn depth(self) -> f32 {
        match self {
            Opening::Pit { .. } => PIT_DEPTH,
            _ => SHAFT_DEPTH,
        }
    }
}

/// The opening's side; how deep a pit is (the hero sinks 0.85 m into
/// it), and a hole's shaft; the shaft's walls' thickness, and how far down
/// they show before the dark takes them.
pub const OPENING: f32 = 0.78;
const PIT_DEPTH: f32 = 0.9;
const SHAFT_DEPTH: f32 = 2.6;
const WALL: f32 = 0.05;
const SHAFT_LIT: f32 = 0.7;

/// The floor tile the terrain lays on a cell.
fn is_floor_tile(s: &Solid) -> bool {
    s.mesh == plane(1.0, 1.0) && s.pos == Vector3::ZERO
}

/// The paint of the floor laid on the cell (None: none laid).
fn floor_paint(look: &Look) -> Option<Paint> {
    look.solids
        .iter()
        .find(|s| is_floor_tile(s))
        .map(|s| s.paint)
}

/// A ground tile: on the floor, no shadow of its own.
fn tile(look: &mut Look, mesh: MeshKey, paint: Paint, pos: Vector3) {
    look.ground_tile(mesh, paint, pos);
}

/// The paint of the named material in the level's branch, at `shade`.
fn paint(ctx: &super::Ctx, name: &str, shade: u8, role: Role) -> Paint {
    ctx.mat(name).map_or(Paint::Flat(DEEP, Finish::Matte), |m| {
        Paint::Pbr(m, shade, role)
    })
}

/// The opening drawn whole: the floor round it, its rim, its shaft going
/// down, and the rock under the level cut (`sunk`). `floor`: the floor's
/// paint, when the cell has none laid (the trap's own terrain lays none).
pub(super) fn open_floor(look: &mut Look, o: Opening, floor: Paint, ctx: &super::Ctx) {
    let floor = floor_paint(look).unwrap_or(floor);
    look.solids.retain(|s| !is_floor_tile(s));
    // the floor left round the opening
    let edge = (1.0 - OPENING) / 2.0;
    let mid = 0.5 - edge / 2.0;
    for (mesh, x, z) in [
        (plane(1.0, edge), 0.0, -mid),
        (plane(1.0, edge), 0.0, mid),
        (plane(edge, OPENING), -mid, 0.0),
        (plane(edge, OPENING), mid, 0.0),
    ] {
        tile(look, mesh, floor, at(x, 0.0, z));
    }
    let depth = o.depth();
    let (wall, bottom) = match o {
        Opening::Pit { .. } => (
            paint(ctx, "dirt", 70, Role::Prop),
            paint(ctx, "dirt", 45, Role::Floor),
        ),
        _ => (
            paint(ctx, "bedrock", 35, Role::Prop),
            Paint::Flat(DEEP, Finish::Matte),
        ),
    };
    // the shaft's four walls: lit near the top, the dark below
    let lit = depth.min(SHAFT_LIT);
    let half = OPENING / 2.0 + WALL / 2.0;
    let walls = |look: &mut Look, top: f32, h: f32, paint: Paint| {
        for (mesh, x, z) in [
            (cuboid(OPENING + 2.0 * WALL, h, WALL), 0.0, -half),
            (cuboid(OPENING + 2.0 * WALL, h, WALL), 0.0, half),
            (cuboid(WALL, h, OPENING), -half, 0.0),
            (cuboid(WALL, h, OPENING), half, 0.0),
        ] {
            look.solid(mesh, paint, at(x, top - h / 2.0, z));
            if let Some(s) = look.solids.last_mut() {
                s.shadow = false;
            }
        }
    };
    walls(look, 0.0, lit, wall);
    if depth > lit {
        walls(look, -lit, depth - lit, Paint::Flat(DEEP, Finish::Matte));
    }
    tile(look, plane(OPENING, OPENING), bottom, at(0.0, -depth, 0.0));
    match o {
        Opening::Pit { spiked } => {
            rim(look, ctx);
            if spiked {
                spikes(look, ctx, -depth);
            }
        }
        Opening::Hole => rim(look, ctx),
        Opening::TrapDoor => lid(look, ctx),
    }
    look.sunk = true;
}

/// The opening under something standing on it: the floor whole, a dark
/// patch on it where it opens (a pit's rim and spikes round it).
pub(super) fn closed_floor(look: &mut Look, o: Opening, floor: Paint, ctx: &super::Ctx) {
    if floor_paint(look).is_none() {
        tile(look, plane(1.0, 1.0), floor, Vector3::ZERO);
    }
    let dark = Paint::Flat(Color::from_rgb(0.006, 0.006, 0.008), Finish::Matte);
    let side = if o == Opening::TrapDoor { 0.8 } else { OPENING };
    tile(look, plane(side, side), dark, at(0.0, 0.004, 0.0));
    match o {
        Opening::Pit { spiked } => {
            rim(look, ctx);
            if spiked {
                spikes(look, ctx, 0.0);
            }
        }
        Opening::Hole => rim(look, ctx),
        Opening::TrapDoor => lid(look, ctx),
    }
}

/// The planks of a trap door's lid, and the iron bands across them.
const PLANK: (f32, f32, f32) = (0.18, 0.04, 0.74);
const BAND: (f32, f32, f32) = (0.8, 0.02, 0.06);

/// A trap door's lid, shut over its shaft.
fn lid(look: &mut Look, ctx: &super::Ctx) {
    let wood = wood(ctx);
    let iron = paint(ctx, "iron", super::SHADE_LIT, Role::Prop);
    for i in 0..4 {
        let x = -0.29 + 0.195 * i as f32;
        look.solid(bevel(PLANK.0, PLANK.1, PLANK.2), wood, at(x, 0.02, 0.0));
        if let Some(s) = look.solids.last_mut() {
            s.shadow = false;
        }
    }
    for z in [-0.25f32, 0.25] {
        look.solid(cuboid(BAND.0, BAND.1, BAND.2), iron, at(0.0, 0.045, z));
        if let Some(s) = look.solids.last_mut() {
            s.shadow = false;
        }
    }
}

/// The trap door's lid taken off the look (it swings down on its own as
/// something drops through), and the paints it was of: the wood's and the
/// iron's (None: no lid).
pub(super) fn unlid(look: &mut Look) -> Option<(Paint, Paint)> {
    let plank = bevel(PLANK.0, PLANK.1, PLANK.2);
    let band = cuboid(BAND.0, BAND.1, BAND.2);
    let wood = look.solids.iter().find(|s| s.mesh == plank)?.paint;
    let iron = look.solids.iter().find(|s| s.mesh == band)?.paint;
    look.solids.retain(|s| s.mesh != plank && s.mesh != band);
    Some((wood, iron))
}

/// The wood a trap door's lid is of.
pub(super) fn wood(ctx: &super::Ctx) -> Paint {
    paint(ctx, "wood", 80, Role::Prop)
}

/// What the opening laid on the floor shut (`closed_floor`) or round a
/// shaft (its rim, its spikes, its lid): taken off before it is laid
/// open (`open_floor` lays its own).
pub(super) fn is_closed_part(s: &Solid) -> bool {
    let patch = s.mesh == plane(OPENING, OPENING) || s.mesh == plane(0.8, 0.8);
    (patch && s.pos.y > 0.0)
        || s.mesh == bevel(0.9, 0.05, 0.1)
        || s.mesh == bevel(0.1, 0.05, 0.7)
        || s.mesh == prism(0.06, 0.2, 0.06)
        || s.mesh == bevel(PLANK.0, PLANK.1, PLANK.2)
        || s.mesh == cuboid(BAND.0, BAND.1, BAND.2)
}

impl super::MapView {
    /// The opening drawn on a cell: which, whether its floor is cut away
    /// round it to the shaft (else whole, a dark patch on it), and whether
    /// a trap door's lid lies shut over it (None: none drawn).
    pub fn opening_drawn(&self, cell: (i32, i32)) -> Option<(Opening, bool, bool)> {
        let look = &self.cells.get(&cell)?.look;
        let open = look.sunk && !look.solids.iter().any(is_floor_tile);
        let plank = bevel(PLANK.0, PLANK.1, PLANK.2);
        let lid = look.solids.iter().any(|s| s.mesh == plank);
        Some((look.opening?, open, lid))
    }
}

/// Stone blocks round the opening's edge.
fn rim(look: &mut Look, ctx: &super::Ctx) {
    let stone = paint(ctx, "bedrock", 100, Role::Prop);
    for (x, z, w, d) in [
        (0.0, -0.4, 0.9, 0.1),
        (0.0, 0.4, 0.9, 0.1),
        (-0.4, 0.0, 0.1, 0.7),
        (0.4, 0.0, 0.1, 0.7),
    ] {
        look.solid(bevel(w, 0.05, d), stone, at(x, 0.025, z));
    }
}

/// A spiked pit's spikes, standing on `floor` (its bottom, or the floor
/// itself while it is shut).
fn spikes(look: &mut Look, ctx: &super::Ctx, floor: f32) {
    let iron = paint(ctx, "iron", super::SHADE_LIT, Role::Prop);
    for (x, z) in [
        (-0.2, -0.15),
        (0.15, -0.2),
        (0.0, 0.1),
        (-0.15, 0.22),
        (0.22, 0.15),
    ] {
        look.solid(prism(0.06, 0.2, 0.06), iron, at(x, floor + 0.03, z));
    }
}
