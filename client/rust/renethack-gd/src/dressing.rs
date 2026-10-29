//! A room's dressing (`BranchLook::dress`): what is built into its walls
//! or worn into its floor, so that it reads as a crypt, a barracks or a
//! hall rather than a bare box. Never anything that could pass for an
//! object, a monster or loot: nothing loose and nothing on the floor that
//! stands up from it; nothing on or round a door or a feature; on the
//! floor, nothing where something lies.
//!
//! On the faces the camera sees, a room's north wall (square on) and its
//! side walls (aslant): pilasters, banners and alcoves on the north wall,
//! soot in Gehennom; chains on the side walls; moss at their feet; small
//! cobwebs high in the far corners, well above anybody's head. On its
//! bare floor: cracks from the foot of its walls, rubble heaped and sunk
//! in its corners. In a cave, rock grown into its walls from the floor to
//! their tops. Each is chosen by its cell on a fixed pattern that keeps
//! the torches' cells free: a level looks the same every visit and as
//! more of it comes into view. Each is part of its cell's batched solids,
//! of a few shared meshes, in the map's own materials (what hangs on a
//! wall thins out with it round the hero). A child of map_view.

use godot::prelude::*;
use nh_world::Terrain;

use super::{
    Around, CAP_HEIGHT, Ctx, Look, PLINTH_HEIGHT, Paint, Role, SHADE_LIT, SHADE_RUBBLE, SIDES,
    Side, WALL_HEIGHT, at, bevel, cell_noise, cuboid, cylinder, facets, is_pier, prism, rock,
    torch_pattern, torus,
};
use crate::branch_look::Dress;

/// What the face of a room's north wall shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// A half-column built into it.
    Pilaster,
    Banner,
    /// A shallow empty niche.
    Alcove,
    /// Soot up it from fires long out.
    Soot,
}

/// What the face of a room's north wall at (x, y) shows by the branch's
/// dressing: a pilaster on every third cell, a banner between them on
/// some, an alcove or soot on a few; nothing where the torches' pattern
/// hangs one.
fn north_piece(x: i32, y: i32, dress: &Dress) -> Option<Piece> {
    if torch_pattern(x, y, true) {
        return None;
    }
    if dress.pilasters && x.rem_euclid(3) == 0 {
        return Some(Piece::Pilaster);
    }
    let pick = cell_noise(x, y, 131);
    if !dress.banners.is_empty() && pick < 0.3 {
        Some(Piece::Banner)
    } else if dress.alcoves && pick > 0.94 {
        Some(Piece::Alcove)
    } else if dress.soot && pick < 0.65 {
        Some(Piece::Soot)
    } else {
        None
    }
}

/// A chain hangs on the face of the side wall at (x, y) towards `side`
/// (west or east, as `SIDES`): on a few, never where a torch does.
fn chain_here(x: i32, y: i32, side: usize, dress: &Dress) -> bool {
    dress.chains && !torch_pattern(x, y, false) && cell_noise(x, y, 135 + side as u32) < 0.07
}

/// A cobweb may hang in the corner at (x, y), a room's north-west corner
/// (`sx` 1: the room east of it) or north-east (-1): by the cell, and
/// never where a torch hangs on either face beside it (the flame in it).
pub(super) fn webbed(x: i32, y: i32, sx: i32, dress: &Dress) -> bool {
    dress.cobwebs
        && cell_noise(x, y, 136) < 0.45
        && !torch_pattern(x + sx, y, true)
        && !torch_pattern(x, y + 1, false)
}

/// Something of the dressing may hang on the face of the wall at (x, y)
/// facing `into` (the way into its room), or beside it a cobweb: a torch
/// hung there would meet it.
pub(super) fn hangs(x: i32, y: i32, into: (i32, i32), dress: &Dress) -> bool {
    match into {
        (0, _) => {
            north_piece(x, y, dress).is_some()
                || webbed(x - 1, y, 1, dress)
                || webbed(x + 1, y, -1, dress)
        }
        (dx, _) => {
            chain_here(x, y, if dx > 0 { 3 } else { 2 }, dress) || webbed(x, y - 1, dx, dress)
        }
    }
}

/// The wall's own stone (`shade`), as it stands in the branch.
fn masonry(ctx: &Ctx, shade: u8) -> Option<Paint> {
    let m = ctx.remap(ctx.art.terrain(Terrain::Wall).material)?;
    Some(Paint::Pbr(m, shade, Role::Trim))
}

/// A manifest material on what hangs on a wall (None without it).
fn trim(ctx: &Ctx, name: &str, shade: u8) -> Option<Paint> {
    ctx.mat(name).map(|m| Paint::Pbr(m, shade, Role::Trim))
}

/// A built wall standing at full height, `top` high: what hangs on its
/// faces towards a room's floor, moss at their feet, a cobweb high in
/// the corner a room makes at a corner pier. `taken`: its north face
/// shows something already (a tower's window). Whether the face towards
/// the room south of it got something.
pub(super) fn wall(
    look: &mut Look,
    around: &Around,
    sym: &str,
    ctx: &Ctx,
    top: f32,
    taken: bool,
) -> bool {
    let dress = &ctx.branch.dress;
    if !around.plain() {
        return false;
    }
    let pier = is_pier(sym);
    // (a pier stands a little wider than the walls it joins)
    let face = if pier { 0.55 } else { 0.51 };
    let first = look.solids.len();
    let piece = north_piece(ctx.x, ctx.y, dress).filter(|_| around.floor[1] && !pier && !taken);
    match piece {
        Some(Piece::Pilaster) => pilaster(look, ctx, top, face),
        Some(Piece::Banner) => banner(look, ctx, face),
        Some(Piece::Alcove) => alcove(look, ctx, face),
        Some(Piece::Soot) => soot(look, ctx, face),
        None => {}
    }
    for side in [2, 3] {
        if around.floor[side] && !pier && chain_here(ctx.x, ctx.y, side, dress) {
            chain(look, ctx, SIDES[side].0 * face);
        }
    }
    if dress.moss {
        moss(look, around, ctx, face);
    }
    // the corner of a room south-east of a north-west corner, south-west
    // of a north-east one, its walls standing full height on both sides
    let wall = |i: usize| around.sides[i] == Side::Wall;
    let room = match sym {
        "S_tlcorn" if wall(1) && wall(3) => Some(1),
        "S_trcorn" if wall(1) && wall(2) => Some(-1),
        _ => None,
    };
    let standing = !around.aside_open && !around.far_aside_open;
    if let Some(sx) = room.filter(|&sx| standing && webbed(ctx.x, ctx.y, sx, dress)) {
        cobweb(look, ctx, sx as f32);
    }
    for s in &mut look.solids[first..] {
        s.shadow = false;
    }
    piece.is_some()
}

/// A half-column built into the face: a base standing out over the
/// plinth, its shaft, a capital under the wall's cap.
fn pilaster(look: &mut Look, ctx: &Ctx, top: f32, face: f32) {
    let (Some(stone), Some(dark)) = (masonry(ctx, SHADE_LIT), masonry(ctx, 80)) else {
        return;
    };
    let base = 0.3;
    let capital = top - CAP_HEIGHT - 0.14;
    look.solid(
        bevel(0.38, base, 0.12),
        dark,
        at(0.0, base / 2.0, face + 0.06),
    );
    let shaft = capital - base;
    look.solid(
        bevel(0.26, shaft, 0.08),
        stone,
        at(0.0, base + shaft / 2.0, face + 0.04),
    );
    look.solid(
        bevel(0.38, 0.14, 0.12),
        dark,
        at(0.0, capital + 0.07, face + 0.06),
    );
}

/// A long cloth of one of the branch's colours (by the cell) hung above
/// head height from an iron rod, pointed at its foot, a dull gold band
/// across its top and a lozenge on it; torn to tatters in Gehennom.
fn banner(look: &mut Look, ctx: &Ctx, face: f32) {
    let dress = &ctx.branch.dress;
    let n = dress.banners.len();
    let name = dress.banners[(ctx.noise(132) * n as f32) as usize % n];
    let (Some(cloth), Some(iron)) = (
        trim(ctx, name, SHADE_LIT),
        trim(ctx, "dark_iron", SHADE_LIT),
    ) else {
        return;
    };
    look.turned(
        cylinder(0.015, 0.015, 0.62),
        iron,
        at(0.0, 1.98, face + 0.05),
        at(0.0, 0.0, 90.0),
    );
    if dress.torn {
        look.solid(cuboid(0.5, 0.6, 0.01), cloth, at(0.0, 1.66, face + 0.04));
        for (x, len) in [(-0.17f32, 0.3f32), (0.0, 0.18), (0.16, 0.4)] {
            look.solid(
                cuboid(0.14, len, 0.01),
                cloth,
                at(x, 1.36 - len / 2.0, face + 0.04),
            );
        }
        return;
    }
    look.solid(cuboid(0.5, 0.74, 0.01), cloth, at(0.0, 1.59, face + 0.04));
    look.turned(
        prism(0.5, 0.22, 0.01),
        cloth,
        at(0.0, 1.11, face + 0.04),
        at(0.0, 0.0, 180.0),
    );
    if let Some(gold) = trim(ctx, "gold", 55) {
        look.solid(cuboid(0.5, 0.04, 0.02), gold, at(0.0, 1.9, face + 0.045));
        look.turned(
            cuboid(0.1, 0.1, 0.02),
            gold,
            at(0.0, 1.52, face + 0.045),
            at(0.0, 0.0, 45.0),
        );
    }
}

/// A shallow empty niche in the face: its back in shadow, framed in the
/// wall's stone on a sill, a pointed arch over it.
fn alcove(look: &mut Look, ctx: &Ctx, face: f32) {
    let (Some(stone), Some(dark)) = (masonry(ctx, SHADE_LIT), trim(ctx, "bedrock", 25)) else {
        return;
    };
    look.solid(cuboid(0.4, 0.78, 0.01), dark, at(0.0, 1.19, face + 0.005));
    for x in [-0.24f32, 0.24] {
        look.solid(bevel(0.08, 0.84, 0.08), stone, at(x, 1.22, face + 0.03));
    }
    look.solid(bevel(0.58, 0.06, 0.12), stone, at(0.0, 0.77, face + 0.05));
    // the arch, as a tower's windows have it
    for (x, z) in [(-0.12f32, -34.0f32), (0.12, 34.0)] {
        look.turned(
            bevel(0.3, 0.07, 0.08),
            stone,
            at(x, 1.7, face + 0.03),
            at(0.0, 0.0, z),
        );
    }
}

/// Soot up the face from a fire long out: two dark tongues, one over the
/// other, rising from the wall's foot.
fn soot(look: &mut Look, ctx: &Ctx, face: f32) {
    let Some(soot) = trim(ctx, "bedrock", 12) else {
        return;
    };
    for k in 0..2u32 {
        // (two sizes of each: shared meshes)
        let pick = |salt: u32| usize::from(ctx.noise(salt) < 0.5);
        let w = [[0.62, 0.5], [0.4, 0.3]][k as usize][pick(140 + k)];
        let h = [[1.4, 1.1], [0.9, 0.7]][k as usize][pick(142 + k)];
        let x = (ctx.noise(144 + k) - 0.5) * 0.3;
        look.solid(
            prism(w, h, 0.01),
            soot,
            at(x, 0.2 + h / 2.0, face + 0.006 + 0.004 * k as f32),
        );
    }
}

/// An iron chain on a side wall's face (`fx`: the face, west or east of
/// the middle), hung from a ring set flat in the stone, a shackle at its
/// end, its links turned each across the last.
fn chain(look: &mut Look, ctx: &Ctx, fx: f32) {
    let Some(iron) = trim(ctx, "dark_iron", SHADE_LIT) else {
        return;
    };
    let out = fx.signum();
    let z = (ctx.noise(137) - 0.5) * 0.4;
    let flat = at(0.0, 0.0, 90.0);
    let across = at(0.0, 90.0, 90.0);
    look.turned(torus(0.03, 0.05), iron, at(fx + out * 0.01, 1.92, z), flat);
    let x = fx + out * 0.035;
    for k in 0..8 {
        let turn = if k % 2 == 0 { flat } else { across };
        look.turned(
            torus(0.01, 0.03),
            iron,
            at(x, 1.86 - 0.05 * k as f32, z),
            turn,
        );
    }
    look.turned(torus(0.04, 0.06), iron, at(x, 1.43, z), across);
}

/// Moss at the foot of the faces towards a room's floor that the camera
/// sees, in clumps: up the plinth's face and a little over it, over its
/// top, tucked into the floor's edge against it. Dark green, flat to the
/// stone and ragged (round tufts, never a bar or a block): none of it
/// lies out on the floor (a lichen or a mold does).
fn moss(look: &mut Look, around: &Around, ctx: &Ctx, face: f32) {
    let Some(moss) = trim(ctx, "moss", 70) else {
        return;
    };
    // the plinth's face
    let front = face + 0.06;
    for (i, &(dx, dz)) in SIDES.iter().enumerate().skip(1) {
        if !around.floor[i] || ctx.noise(141 + i as u32) > 0.2 {
            continue;
        }
        // `u` along the face, `out` from the wall's middle; a tuft facing
        // out of the face, or lying flat
        let place = |u: f32, y: f32, out: f32| {
            if dx == 0.0 {
                at(u, y, dz * out)
            } else {
                at(dx * out, y, u)
            }
        };
        let upright = if dx == 0.0 {
            at(90.0, 0.0, 0.0)
        } else {
            at(0.0, 0.0, 90.0)
        };
        for k in 0..3u32 {
            let salt = 150 + i as u32 * 20 + k * 6;
            if k == 1 && ctx.noise(salt) < 0.4 {
                continue;
            }
            let clump = (k as f32 - 1.0) * 0.3 + (ctx.noise(salt + 1) - 0.5) * 0.1;
            for t in 0..6u32 {
                let n = |j: u32| ctx.noise(salt * 7 + t * 3 + j);
                let r = [0.05, 0.07, 0.09][(n(0) * 2.99) as usize];
                let u = clump + (n(1) - 0.5) * 0.2;
                let tuft = cylinder(r, r, 0.01);
                match t {
                    // up the plinth's face, the highest over its top
                    0..=2 => {
                        let y = 0.04 + n(2) * (PLINTH_HEIGHT - 0.04);
                        look.turned(tuft, moss, place(u, y, front + 0.006), upright);
                    }
                    // over its top
                    3 => look.solid(tuft, moss, place(u, PLINTH_HEIGHT, face + 0.03)),
                    // in the floor's edge against it
                    _ => look.solid(tuft, moss, place(u, 0.0, front + r * 0.2)),
                }
            }
        }
    }
}

/// A strand of silk from `a` to `b`.
fn strand(look: &mut Look, a: Vector3, b: Vector3, silk: Paint) {
    let d = b - a;
    let flat = d.x.hypot(d.z);
    let yaw = (-d.z).atan2(d.x).to_degrees();
    let rise = d.y.atan2(flat).to_degrees();
    look.turned(
        cuboid(d.length(), 0.01, 0.01),
        silk,
        (a + b) / 2.0,
        at(0.0, yaw, rise),
    );
}

/// Where a cobweb's hub is, at the top of its corner, and how high the
/// lowest of it hangs (well over anybody's head).
pub(super) const WEB_TOP: f32 = WALL_HEIGHT - 0.13;
pub(super) const WEB_LOW: f32 = WEB_TOP - 0.24;
const _: () = assert!(WEB_LOW > 1.7, "a cobweb over anybody's head");

/// A small cobweb high in the corner a room makes at a corner pier (`sx`:
/// the room east of it, or west): from its hub at the top of the corner a
/// fan of strands down and out to a thread sagging across the corner
/// between the walls, two rings across them; it faces the camera, not
/// edge on (a line, it would read as a shaft stuck in the wall). Its
/// shape is the same in every corner (shared meshes).
fn cobweb(look: &mut Look, ctx: &Ctx, sx: f32) {
    let Some(silk) = trim(ctx, "cobweb", SHADE_LIT) else {
        return;
    };
    let reach = 0.26;
    let rim_y = WEB_LOW + 0.04;
    // the corner: the pier's edge, the faces of the walls either side
    let hub = at(sx * 0.555, WEB_TOP, 0.555);
    let side = at(sx * 0.52, rim_y, 0.555 + reach);
    let north = at(sx * (0.555 + reach), rim_y, 0.52);
    let rim: Vec<Vector3> = (0..4)
        .map(|k| {
            let sag = if k == 1 || k == 2 { 0.04 } else { 0.0 };
            side.lerp(north, k as f32 / 3.0) - at(0.0, sag, 0.0)
        })
        .collect();
    for pair in rim.windows(2) {
        strand(look, pair[0], pair[1], silk);
    }
    for &r in &rim {
        strand(look, hub, r, silk);
    }
    for t in [0.45f32, 0.8] {
        let ring: Vec<Vector3> = rim.iter().map(|&r| hub.lerp(r, t)).collect();
        for pair in ring.windows(2) {
            strand(look, pair[0], pair[1], silk);
        }
    }
}

/// A room's floor cell that shows bare floor (nothing lies on it, nothing
/// is written on it, nothing opens in it): cracks running from the foot
/// of its walls, rubble heaped and sunk in its corner. They never make
/// the cell taller to the pointer.
pub(super) fn floor(look: &mut Look, around: &Around, ctx: &Ctx) {
    let dress = &ctx.branch.dress;
    if !around.plain() {
        return;
    }
    let (first, top) = (look.solids.len(), look.top);
    if dress.cracks {
        cracks(look, around, ctx);
    }
    if dress.corners && ctx.branch.rubble {
        corner_rubble(look, around, ctx);
    }
    for s in &mut look.solids[first..] {
        s.shadow = false;
    }
    look.top = top;
}

/// A crack in the floor from the foot of a wall here and there: thin,
/// dark as a gap, in the floor's own stone, wandering half a metre or
/// so out, forked; never out of its cell.
fn cracks(look: &mut Look, around: &Around, ctx: &Ctx) {
    let Some(m) = ctx.remap(ctx.art.terrain(Terrain::Floor).material) else {
        return;
    };
    let dark = Paint::Pbr(m, 10, Role::Floor);
    // a stretch of it, if it stays in the cell: where it ends
    let segment = |look: &mut Look, from: Vector3, heading: f32, len: f32| {
        let to = from + at(heading.cos(), 0.0, heading.sin()) * len;
        if to.x.abs().max(to.z.abs()) > 0.46 {
            return None;
        }
        look.turned(
            cuboid(len, 0.01, 0.02),
            dark,
            (from + to) / 2.0,
            at(0.0, -heading.to_degrees(), 0.0),
        );
        Some(to)
    };
    for (i, &(dx, dz)) in SIDES.iter().enumerate() {
        if around.sides[i] != Side::Wall || ctx.noise(201 + i as u32) > 0.12 {
            continue;
        }
        // from the foot of the wall's plinth, away from it
        let along = (ctx.noise(205 + i as u32) - 0.5) * 0.6;
        let mut p = if dx == 0.0 {
            at(along, 0.0, dz * 0.43)
        } else {
            at(dx * 0.43, 0.0, along)
        };
        let mut heading = (-dz).atan2(-dx);
        for k in 0..4u32 {
            let salt = 210 + i as u32 * 10 + k * 2;
            heading += (ctx.noise(salt) - 0.5) * 0.9;
            let len = [0.11, 0.15][usize::from(ctx.noise(salt + 1) < 0.5)];
            let Some(to) = segment(look, p, heading, len) else {
                break;
            };
            p = to;
            // a fork, off one side
            if k == 1 || (k == 2 && ctx.noise(salt + 9) < 0.5) {
                let turn = if ctx.noise(salt + 7) < 0.5 {
                    0.75
                } else {
                    -0.75
                };
                if let Some(q) = segment(look, p, heading + turn, 0.11) {
                    segment(look, q, heading + turn * 0.4, 0.11);
                }
            }
        }
    }
}

/// Rubble heaped in a room's corner against both walls, sunk to near its
/// middle as the stones at the walls' feet are: the big chunks in the
/// corner, fallen from the walls and of their stone, the small of the
/// rock spilled out of it.
fn corner_rubble(look: &mut Look, around: &Around, ctx: &Ctx) {
    let (Some(rock), Some(fallen)) = (ctx.mat("bedrock"), masonry(ctx, 70)) else {
        return;
    };
    let wall = |i: usize| around.sides[i] == Side::Wall;
    let corner = [(0usize, 2usize), (0, 3), (1, 2), (1, 3)]
        .into_iter()
        .find(|&(a, b)| wall(a) && wall(b));
    let Some((a, b)) = corner.filter(|_| ctx.noise(151) < 0.5) else {
        return;
    };
    let (cx, cz) = (SIDES[b].0, SIDES[a].1);
    let rock = Paint::Pbr(rock, SHADE_RUBBLE, Role::Trim);
    let n = 6 + (ctx.noise(152) * 2.99) as u32;
    for k in 0..n {
        let salt = 160 + k * 4;
        let size = match k {
            0 | 1 => 3,
            2 => 2,
            _ => (ctx.noise(salt) * 2.99) as usize,
        };
        // (the wall-foot stones' sizes, and one bigger)
        let r = [0.04, 0.065, 0.09, 0.12][size];
        let spread = 0.06 + 0.06 * k as f32;
        let x = cx * (0.43 - r * 0.7 - ctx.noise(salt + 1) * spread);
        let z = cz * (0.43 - r * 0.7 - ctx.noise(salt + 2) * spread);
        let rot = at(ctx.noise(salt + 3) * 90.0, ctx.noise(salt) * 360.0, 0.0);
        let paint = if size == 3 { fallen } else { rock };
        look.turned(facets(r, 5), paint, at(x, -r * 0.45, z), rot);
    }
}

/// A cave's wall standing at full height, `top` high, of `rock`: rock
/// grown into its faces from the floor to its top where open ground lies
/// south, east or west of it (the camera sees no north face), never
/// where timber holds the rock up. Lumpy as the wall is, and one with
/// it: no stalagmite standing free (a piercer is one).
pub(super) fn cave(look: &mut Look, around: &Around, ctx: &Ctx, top: f32, rock_paint: Paint) {
    if !ctx.branch.dress.formations || !around.plain() {
        return;
    }
    for (i, &(dx, dz)) in SIDES.iter().enumerate().skip(1) {
        if around.sides[i] != Side::Open || super::propped(ctx, i) {
            continue;
        }
        if ctx.noise(181 + i as u32) > 0.25 {
            continue;
        }
        let wide = [0.36, 0.5][usize::from(ctx.noise(185 + i as u32) < 0.5)];
        let along = (ctx.noise(188 + i as u32) - 0.5) * 0.5;
        let h = top - 0.1;
        // half in the rock, half out of it
        let (mesh, pos) = if dx == 0.0 {
            (rock(wide, h, 0.4), at(along, 0.0, dz * 0.5))
        } else {
            (rock(0.4, h, wide), at(dx * 0.5, 0.0, along))
        };
        look.solid(mesh, rock_paint, pos);
    }
}
