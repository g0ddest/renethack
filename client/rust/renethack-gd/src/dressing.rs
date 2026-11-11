//! A room's dressing (`BranchLook::dress`): what is built into its walls
//! or worn into its floor, so that it reads as a crypt, a barracks or a
//! hall rather than a bare box. Never anything that could pass for an
//! object, a monster or loot: nothing loose and nothing on the floor that
//! stands up from it; nothing on or round a door or a feature; on the
//! floor, nothing where something lies.
//!
//! On the faces the camera sees, a room's north wall (square on) and its
//! side walls (aslant): pilasters, banners and alcoves on the north wall,
//! seams of fire in Gehennom; chains on the side walls; moss at their
//! feet; small cobwebs high in the far corners, well above anybody's head.
//! On its bare floor: cracks from the foot of its walls, rubble heaped and
//! sunk in its corners. In a cave, rock grown into its walls from the
//! floor to their tops. Each is chosen by its cell on a fixed pattern that
//! keeps the torches' cells free: a level looks the same every visit and
//! as more of it comes into view.
//!
//! Volume goes into the walls, never onto the floor (a player would read
//! whatever stands on a floor cell as in the way): columns engaged in the
//! north walls, blind arches between them on a long one, or buttresses
//! where a branch builds no columns; thick piers in the far corners;
//! buttresses and high braziers (never alight) on the side walls. Below
//! head height nothing stands out more than a hand from a wall, above it
//! an impost or a pier's capital may; what has mass casts a shadow. A
//! bigger room's floor has a band of another stone inlaid in it, flat
//! (the level's `plan` lays the arches and the inlays out). Each is part
//! of its cell's batched solids, of a few shared meshes, in the map's own
//! materials (what hangs on a wall thins out with it round the hero). A
//! child of map_view.

use godot::prelude::*;
use nh_world::Terrain;

use std::collections::HashMap;

use super::{
    Around, CAP_HEIGHT, Ctx, Finish, Look, PLINTH_HEIGHT, Paint, Role, SHADE_LIT, SHADE_RUBBLE,
    SIDES, Side, WALL_HEIGHT, at, bevel, cell_noise, cuboid, cylinder, facets, is_pier, prism,
    rock, torch_pattern, torus,
};
use crate::branch_look::{Brazier, Dress};

/// How far below head height what stands out of a wall may reach past its
/// face, and from what height up it may overhang the room's air further.
pub(super) const REACH: f32 = 0.13;
pub(super) const OVERHEAD: f32 = 1.9;

/// What the face of a room's north wall shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Piece {
    /// A half-column built into it.
    Pilaster,
    Banner,
    /// A shallow empty niche.
    Alcove,
    /// Seams of the fire under the floors, glowing up from its foot.
    Embers,
    /// A buttress against it, where the branch builds no columns.
    Buttress,
}

/// What the face of a room's north wall at (x, y) shows by the branch's
/// dressing: a column on every third cell (a torch falling there hangs on
/// it), or where it builds none a buttress on every fourth, a banner
/// between them on some, an alcove or seams of fire on a few; nothing
/// else where the torches' pattern hangs one.
fn north_piece(x: i32, y: i32, dress: &Dress) -> Option<Piece> {
    // (a torch on a column's cell hangs on the column)
    if dress.pilasters && x.rem_euclid(3) == 0 {
        return Some(Piece::Pilaster);
    }
    north_between(x, y, dress)
}

/// What a north wall's face shows between its columns (`north_piece`), or
/// where the level's plan sets them (a small room's).
fn north_between(x: i32, y: i32, dress: &Dress) -> Option<Piece> {
    if torch_pattern(x, y, true) {
        return None;
    }
    if dress.buttresses && !dress.pilasters && x.rem_euclid(4) == 1 {
        return Some(Piece::Buttress);
    }
    let pick = cell_noise(x, y, 131);
    if !dress.banners.is_empty() && pick < 0.3 {
        Some(Piece::Banner)
    } else if dress.alcoves && pick > 0.94 {
        Some(Piece::Alcove)
    } else if dress.embers && pick < 0.65 {
        Some(Piece::Embers)
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
        && !pier_here(x, y, dress)
        && cell_noise(x, y, 136) < 0.45
        && !torch_pattern(x + sx, y, true)
        && !torch_pattern(x, y + 1, false)
}

/// What the face of the north wall at (x, y) shows: as the level's plan
/// sets it (a small room's), or by the branch's pattern.
fn north_face(x: i32, y: i32, dress: &Dress, volume: Option<Volume>) -> Option<Piece> {
    match volume {
        Some(Volume::North(Some(p))) => Some(p),
        // (a small room's wall bare but for a banner: its niche its own)
        Some(Volume::North(None)) => north_between(x, y, dress).filter(|&p| p != Piece::Alcove),
        _ => north_piece(x, y, dress),
    }
}

/// A column stands on the face of the north wall at (x, y) (a torch
/// falling there hangs on it).
pub(super) fn column_on(x: i32, y: i32, dress: &Dress, volume: Option<Volume>) -> bool {
    north_face(x, y, dress, volume) == Some(Piece::Pilaster)
}

/// Something of the dressing may hang on the face of the wall at (x, y)
/// facing `into` (the way into its room), or beside it a cobweb: a torch
/// hung there would meet it (`volume`: the level's plan for the cell).
pub(super) fn hangs(
    x: i32,
    y: i32,
    into: (i32, i32),
    dress: &Dress,
    volume: Option<Volume>,
) -> bool {
    match into {
        (0, _) => {
            north_face(x, y, dress, volume).is_some()
                || webbed(x - 1, y, 1, dress)
                || webbed(x + 1, y, -1, dress)
        }
        (dx, _) => {
            let side = if dx > 0 { 3 } else { 2 };
            chain_here(x, y, side, dress)
                || buttress_here(x, y, side, dress)
                || brazier_here(x, y, side, dress)
                || webbed(x, y - 1, dx, dress)
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
    let arch = match ctx.volume {
        Some(Volume::Arch { west }) if around.floor[1] && !pier && !taken => Some(west),
        _ => None,
    };
    let piece = north_face(ctx.x, ctx.y, dress, ctx.volume)
        .filter(|_| around.floor[1] && !pier && !taken && arch.is_none());
    // (what has mass casts a shadow: it reads as standing out by it)
    let mut massive = Vec::new();
    if let Some(west) = arch {
        let a = look.solids.len();
        arch_half(look, ctx, face, west);
        massive.push(a..look.solids.len());
    }
    match piece {
        Some(Piece::Pilaster) => {
            let a = look.solids.len();
            column(look, ctx, top, face);
            massive.push(a..look.solids.len());
        }
        Some(Piece::Buttress) => {
            let a = look.solids.len();
            buttress(look, ctx, top, face, true);
            massive.push(a..look.solids.len());
        }
        Some(Piece::Banner) => banner(look, ctx, face),
        Some(Piece::Alcove) => alcove(look, ctx, face),
        Some(Piece::Embers) => embers(look, ctx, face),
        None => {}
    }
    for side in [2, 3] {
        if !around.floor[side] || pier {
            continue;
        }
        let fx = SIDES[side].0 * face;
        if chain_here(ctx.x, ctx.y, side, dress) {
            chain(look, ctx, fx);
        } else if buttress_here(ctx.x, ctx.y, side, dress) {
            let a = look.solids.len();
            buttress(look, ctx, top, fx, false);
            massive.push(a..look.solids.len());
        } else if brazier_here(ctx.x, ctx.y, side, dress) {
            brazier(look, ctx, fx);
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
    let pier_planned = ctx.volume == Some(Volume::Pier);
    if let Some(sx) = room.filter(|_| standing && (pier_planned || pier_here(ctx.x, ctx.y, dress)))
    {
        let a = look.solids.len();
        corner_pier(look, ctx, top, sx as f32);
        massive.push(a..look.solids.len());
    } else if let Some(sx) = room.filter(|&sx| standing && webbed(ctx.x, ctx.y, sx, dress)) {
        cobweb(look, ctx, sx as f32);
    }
    for s in &mut look.solids[first..] {
        s.shadow = false;
    }
    for r in massive {
        for s in &mut look.solids[r] {
            s.shadow = true;
        }
    }
    piece.is_some() || arch.is_some()
}

/// Where a room's columns spring their arches from, and how high their
/// crown rises (`column`, `arch_half`).
const SPRING: f32 = 1.62;
const CROWN: f32 = 1.98;

/// A column engaged in the face: a round shaft half sunk in the wall on a
/// stepped plinth, a capital over it, and above head height an impost
/// block standing out into the room's air (the arches spring from it).
/// Gilded capitals in a branch that has them.
fn column(look: &mut Look, ctx: &Ctx, top: f32, face: f32) {
    let (Some(stone), Some(dark)) = (masonry(ctx, 108), masonry(ctx, 75)) else {
        return;
    };
    let capital = if ctx.branch.dress.gilded {
        trim(ctx, "gold", 60).unwrap_or(dark)
    } else {
        dark
    };
    // the plinth in two steps
    look.solid(bevel(0.52, 0.16, 2.0 * REACH), dark, at(0.0, 0.08, face));
    look.solid(bevel(0.44, 0.14, 1.6 * REACH), stone, at(0.0, 0.23, face));
    // the shaft, sunk a little past half in the wall
    let shaft = SPRING - 0.3;
    look.solid(
        cylinder(0.2, 0.2, shaft),
        stone,
        at(0.0, 0.3 + shaft / 2.0, face + REACH - 0.2),
    );
    look.solid(
        bevel(0.48, 0.14, 2.0 * REACH),
        capital,
        at(0.0, SPRING + 0.07, face),
    );
    // the impost, over anybody's head (none where a torch hangs on the
    // column: its flame stands there)
    if !torch_pattern(ctx.x, ctx.y, true) {
        let impost = (top.min(WALL_HEIGHT) - CAP_HEIGHT - OVERHEAD).max(0.08);
        look.solid(
            bevel(0.56, impost, 0.5),
            dark,
            at(0.0, OVERHEAD + impost / 2.0, face + 0.1),
        );
    }
}

/// Half of a blind arch between two columns on a long north wall (`west`:
/// the column it springs from stands west of this cell): the face under
/// it a recess of the wall's stone in shade (never dark as a way through),
/// the ring of its voussoirs standing out from the face, its crown at the
/// edge of the cell it shares with the other half.
fn arch_half(look: &mut Look, ctx: &Ctx, face: f32, west: bool) {
    let (Some(stone), Some(dark)) = (masonry(ctx, SHADE_LIT), masonry(ctx, 50)) else {
        return;
    };
    let sx = if west { -1.0 } else { 1.0 };
    // the arch spans from one column's side to the other's: its middle at
    // this cell's edge with the other half
    let mid = -sx * 0.5;
    let half_span = 1.25;
    let rise = CROWN - SPRING;
    // the recess: dark under the arch, up to its springing and in strips
    // under its curve, each as wide as the curve is at its height
    look.solid(
        cuboid(1.0, SPRING - 0.3, 0.01),
        dark,
        at(0.0, 0.3 + (SPRING - 0.3) / 2.0, face + 0.004),
    );
    for k in 0..4 {
        let h = rise / 4.0;
        let y0 = SPRING + h * k as f32;
        let frac = ((y0 + h / 2.0 - SPRING) / rise).min(0.95);
        let w = half_span * (1.0 - frac * frac).sqrt();
        let x0 = mid + sx * w;
        let (lo, hi) = (x0.min(mid).max(-0.5), x0.max(mid).min(0.5));
        if hi - lo > 0.02 {
            look.solid(
                cuboid(hi - lo, h, 0.01),
                dark,
                at((lo + hi) / 2.0, y0 + h / 2.0, face + 0.004),
            );
        }
    }
    // the voussoirs along the curve, each turned to it
    for k in 0..5 {
        let a = std::f32::consts::FRAC_PI_2 * (k as f32 + 0.5) / 5.0;
        let (x, y) = (mid + sx * half_span * a.cos(), SPRING + rise * a.sin());
        if x.abs() > 0.5 {
            continue;
        }
        let tilt = (rise * a.cos()).atan2(half_span * a.sin()).to_degrees();
        look.turned(
            bevel(0.2, 0.13, 0.12),
            stone,
            at(x, y, face + 0.05),
            at(0.0, 0.0, -sx * tilt),
        );
    }
}

/// A pier the room's corner is built into at a corner cell (`sx`: the
/// room east of it, or west): a thick chamfered block filling the corner,
/// a hand into the room each way, its capital, and over anybody's head an
/// impost reaching further.
fn corner_pier(look: &mut Look, ctx: &Ctx, top: f32, sx: f32) {
    let (Some(stone), Some(dark)) = (masonry(ctx, SHADE_LIT), masonry(ctx, 80)) else {
        return;
    };
    // the corner's faces at 0.55 (a pier's), the block a hand past them
    // and its capital a little more
    let edge = 0.55 + REACH - 0.02;
    let size = edge + 0.35;
    let mid = edge - size / 2.0;
    look.solid(
        bevel(size, SPRING, size),
        stone,
        at(sx * mid, SPRING / 2.0, mid),
    );
    look.solid(
        bevel(size + 0.04, 0.12, size + 0.04),
        dark,
        at(sx * mid, SPRING + 0.06, mid),
    );
    let impost = (top.min(WALL_HEIGHT) - CAP_HEIGHT - OVERHEAD).max(0.08);
    let reach = edge + 0.16;
    let wide = size + 0.3;
    let at_mid = reach - wide / 2.0;
    look.solid(
        bevel(wide, impost, wide),
        dark,
        at(sx * at_mid, OVERHEAD + impost / 2.0, at_mid),
    );
}

/// A pier fills a room's corner at a corner cell (by the cell).
fn pier_here(x: i32, y: i32, dress: &Dress) -> bool {
    dress.piers && (dress.heavy || cell_noise(x, y, 171) < 0.6)
}

/// A buttress against the face of the side wall at (x, y) towards `side`:
/// every fourth cell, never where a torch hangs.
fn buttress_here(x: i32, y: i32, side: usize, dress: &Dress) -> bool {
    dress.buttresses && !torch_pattern(x, y, false) && (y + side as i32).rem_euclid(4) == 1
}

/// A buttress against a wall's face (`face`: how far it stands from the
/// middle, west or east of it on a side wall, south on a `north` one): a
/// hand deep up to two thirds of the wall, half as deep above a string
/// course; of heavy blocks where the branch builds so.
fn buttress(look: &mut Look, ctx: &Ctx, top: f32, face: f32, north: bool) {
    let (Some(stone), Some(dark)) = (masonry(ctx, SHADE_LIT), masonry(ctx, 80)) else {
        return;
    };
    let heavy = ctx.branch.dress.heavy;
    let wide = if heavy { 0.56 } else { 0.42 };
    // (deep and wide as on a side wall, turned for a north one)
    let block = |deep: f32, h: f32, wide: f32| {
        if north {
            bevel(wide, h, deep)
        } else {
            bevel(deep, h, wide)
        }
    };
    let place = |y: f32| {
        if north {
            at(0.0, y, face)
        } else {
            at(face, y, 0.0)
        }
    };
    let low = (top - CAP_HEIGHT) * 0.66;
    look.solid(block(2.0 * REACH, low, wide), stone, place(low / 2.0));
    let high = top - CAP_HEIGHT - low;
    look.solid(
        block(REACH, high, wide - 0.08),
        if heavy { dark } else { stone },
        place(low + high / 2.0),
    );
    look.solid(
        block(2.0 * REACH + 0.02, 0.06, wide + 0.04),
        dark,
        place(low),
    );
}

/// A brazier on the side wall at (x, y) towards `side`: a few, never with
/// a torch, a chain or a buttress.
fn brazier_here(x: i32, y: i32, side: usize, dress: &Dress) -> bool {
    dress.braziers != Brazier::None
        && !torch_pattern(x, y, false)
        && !chain_here(x, y, side, dress)
        && !buttress_here(x, y, side, dress)
        && cell_noise(x, y, 175 + side as u32) < 0.12
}

/// An iron brazier on a bracket high on a side wall's face (`fx`), over
/// anybody's head and never alight: cold ash in it, or in a branch over
/// fire embers glowing dull (they light nothing).
fn brazier(look: &mut Look, ctx: &Ctx, fx: f32) {
    let Some(iron) = trim(ctx, ctx.branch.dress.iron, SHADE_LIT) else {
        return;
    };
    let out = fx.signum();
    let y = OVERHEAD + 0.06;
    // the bracket, then the bowl a little out from the face
    look.solid(
        cuboid(0.18, 0.04, 0.06),
        iron,
        at(fx + out * 0.09, y - 0.03, 0.0),
    );
    look.solid(
        cylinder(0.14, 0.08, 0.1),
        iron,
        at(fx + out * 0.2, y + 0.05, 0.0),
    );
    let fill = match ctx.branch.dress.braziers {
        Brazier::Ember => Paint::Flat(Color::from_rgb(0.55, 0.14, 0.03), Finish::Ember),
        _ => Paint::Flat(Color::from_rgb(0.22, 0.21, 0.2), Finish::Matte),
    };
    look.solid(
        cylinder(0.12, 0.12, 0.01),
        fill,
        at(fx + out * 0.2, y + 0.1, 0.0),
    );
}

/// A long cloth of one of the branch's colours (by the cell) hung above
/// head height from an iron rod, pointed at its foot, a dull gold band
/// across its top and a lozenge on it; torn to tatters in Gehennom.
fn banner(look: &mut Look, ctx: &Ctx, face: f32) {
    let dress = &ctx.branch.dress;
    let n = dress.banners.len();
    let name = dress.banners[(ctx.noise(132) * n as f32) as usize % n];
    let (Some(cloth), Some(iron)) = (trim(ctx, name, SHADE_LIT), trim(ctx, dress.iron, SHADE_LIT))
    else {
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

/// Seams of the fire under the floors in the face, glowing up from the
/// plinth (soot would not show on Gehennom's black stone): two, each
/// wandering up half a metre or so, thin and flat in the stone.
fn embers(look: &mut Look, ctx: &Ctx, face: f32) {
    let glow = Paint::Flat(Color::from_rgb(0.75, 0.2, 0.04), Finish::Ember);
    for k in 0..2u32 {
        let salt = 300 + k * 40;
        let mut p = Vector2::new((ctx.noise(salt) - 0.5) * 0.7, PLINTH_HEIGHT);
        let mut lean = (ctx.noise(salt + 1) - 0.5) * 0.6;
        let steps = 3 + (ctx.noise(salt + 2) * 3.0) as u32;
        for i in 0..steps {
            let n = |j: u32| ctx.noise(salt + 3 + i * 2 + j);
            lean = (lean + (n(0) - 0.5) * 0.8).clamp(-0.7, 0.7);
            // (two lengths: shared meshes)
            let len = [0.1, 0.15][usize::from(n(1) < 0.5)];
            let to = p + Vector2::new(lean.sin(), lean.cos()) * len;
            if to.x.abs() > 0.45 {
                break;
            }
            let mid = (p + to) / 2.0;
            look.turned(
                cuboid(0.02, len, 0.01),
                glow,
                at(mid.x, mid.y, face + 0.006),
                at(0.0, 0.0, -lean.to_degrees()),
            );
            p = to;
        }
    }
}

/// An iron chain on a side wall's face (`fx`: the face, west or east of
/// the middle), hung from a ring set flat in the stone, a shackle at its
/// end, its links turned each across the last.
fn chain(look: &mut Look, ctx: &Ctx, fx: f32) {
    let Some(iron) = trim(ctx, ctx.branch.dress.iron, SHADE_LIT) else {
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
    let (first, top) = (look.solids.len(), look.top);
    // (a rug goes on up to a door's side; nothing else of the floor's
    // dressing on it)
    if rug(look, ctx) {
        for s in &mut look.solids[first..] {
            s.shadow = false;
        }
        look.top = top;
        return;
    }
    if !around.plain() {
        return;
    }
    inlay(look, ctx);
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

/// What the level's plan (`plan`) gives a cell: half of a blind arch on a
/// north wall between two columns (`west`: the column it springs from
/// stands west of it), or lines inlaid in a room's floor (the first `n`,
/// each from and to in the cell's own metres, x and z, and how wide;
/// `corner`: a band's corner there, a stone set where its lines meet).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Volume {
    Arch {
        west: bool,
    },
    Inlay {
        lines: [[f32; 5]; 4],
        n: u8,
        corner: Option<[f32; 2]>,
    },
    /// The face of a small room's north wall: as set (a pair of columns,
    /// a niche between), or (None) as the pattern sets it but for its
    /// columns.
    North(Option<Piece>),
    /// A pier built into a small room's corner.
    Pier,
    /// A rug laid in a small room's floor: its rectangle in the cell's own
    /// metres (x0, z0, x1, z1), reaching past the cell where it goes on.
    Rug {
        rect: [f32; 4],
    },
}

/// The least run of north wall over a room's floor that has arches, the
/// least room (wide, deep) that has a band inlaid, and the least that has
/// a lozenge in its middle too.
const ARCADE_LEAST: i32 = 7;
/// The widest room that is small: its north wall dressed as a whole (a
/// pair of columns, or a niche between two), its corners piers, rather
/// than by the pattern (which may leave one bare).
const SMALL_MOST: i32 = 6;
const BAND_LEAST: (i32, i32) = (5, 4);
const LOZENGE_LEAST: (i32, i32) = (9, 5);
/// A band's width, how far inside its cells' outer edges it runs (to its
/// middle), its fillet's width, and how far inside the band the fillet
/// runs (middle to middle); a lozenge's width; a band's corner stone.
const BAND: f32 = 0.14;
const BAND_IN: f32 = 0.1;
const FILLET: f32 = 0.04;
const FILLET_IN: f32 = 0.13;
const LOZENGE: f32 = 0.1;
const CORNER: f32 = 0.2;
/// A rug's margin to its room's walls (metres from the room's edge) and to
/// what it leaves bare beside it, and the least stretch of bare floor it
/// lies on (cells); its border's width and how far in from its edge it
/// runs, and how dark its cloths lie (worn, never a bright
/// patch that could pass for something lying there).
const RUG_MARGIN: f32 = 0.55;
const RUG_BESIDE: f32 = 0.3;
const RUG_LEAST: (i32, i32) = (2, 2);
const RUG_BORDER: f32 = 0.09;
const RUG_BORDER_IN: f32 = 0.07;
/// The thin line inside the border: its width, and its gap to the border.
const RUG_LINE: f32 = 0.025;
const RUG_LINE_IN: f32 = 0.06;
const RUG_SHADE: u8 = 48;
const RUG_BORDER_SHADE: u8 = 52;
/// A rug's fringe on its short ends: tassels of its border's wool this
/// wide, one every this many metres along the end, reaching out this far
/// (or the shorter, for a frayed one), lighter than the border.
const FRINGE_WIDE: f32 = 0.02;
const FRINGE_STEP: f32 = 0.055;
const FRINGE_LONG: (f32, f32) = (0.09, 0.06);
const FRINGE_SHADE: u8 = 58;

/// What the plan reads of a cell: its terrain as known; whether something
/// stands or lies on ground never shown bare (it is inside a room all the
/// same); whether it matters in play or may (`map_view::may_matter`).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PlanCell {
    pub terrain: Option<Terrain>,
    pub held: bool,
    pub matters: bool,
}

/// The level's plan by what each cell is (`cell`) and the branch's dressing:
/// on a long north wall over a room's floor, a blind arch between two of
/// its columns (a torch on the wall there hangs in it), whole or none; in
/// a room of `BAND_LEAST` or more, a band of another stone inlaid round
/// it a cell in from its walls, a fillet inside the band, and in one of
/// `LOZENGE_LEAST` or more a lozenge in its middle. Nothing on or round a
/// door or a feature. All of it from what has been seen: a run of wall
/// as far as it is known, a room once its floor and its walls all round
/// are (it may grow as more comes into view, never ahead of it).
pub(super) fn plan(
    cell: impl Fn(i32, i32) -> PlanCell,
    dress: &Dress,
) -> HashMap<(i32, i32), Volume> {
    use nh_world::{COLNO, ROWNO};
    let mut out = HashMap::new();
    // (each cell read once: the rooms and the plain round them read most
    // several times over)
    let grid: Vec<PlanCell> = (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .map(|(x, y)| cell(x, y))
        .collect();
    let cell = |x: i32, y: i32| {
        let on = (0..COLNO).contains(&x) && (0..ROWNO).contains(&y);
        if on {
            grid[(y * COLNO + x) as usize]
        } else {
            PlanCell::default()
        }
    };
    let t = |x: i32, y: i32| cell(x, y).terrain;
    let floor = |x: i32, y: i32| matches!(t(x, y), Some(Terrain::Floor | Terrain::DarkFloor));
    let plain =
        |x: i32, y: i32| (-1..=1).all(|dy| (-1..=1).all(|dx| !cell(x + dx, y + dy).matters));
    if dress.arcades && dress.pilasters {
        let north = |x: i32, y: i32| t(x, y) == Some(Terrain::Wall) && floor(x, y + 1);
        for y in 0..ROWNO {
            let mut x = 0;
            while x < COLNO {
                if !north(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < COLNO && north(x, y) {
                    x += 1;
                }
                if x - start < ARCADE_LEAST {
                    continue;
                }
                for c in (start..x - 3).filter(|c| c.rem_euclid(3) == 0) {
                    if (c..=c + 3).all(|cx| plain(cx, y)) {
                        out.insert((c + 1, y), Volume::Arch { west: true });
                        out.insert((c + 2, y), Volume::Arch { west: false });
                    }
                }
            }
        }
    }
    // (a room's rectangle takes in what stands in it: stairs, an
    // altar, a fountain, a pool)
    let inside = |x: i32, y: i32| {
        cell(x, y).held
            || matches!(
                t(x, y),
                Some(
                    Terrain::Floor
                        | Terrain::DarkFloor
                        | Terrain::StairsUp
                        | Terrain::StairsDown
                        | Terrain::LadderUp
                        | Terrain::LadderDown
                        | Terrain::Altar
                        | Terrain::Grave
                        | Terrain::Throne
                        | Terrain::Sink
                        | Terrain::Fountain
                        | Terrain::Trap
                        | Terrain::Tree
                        | Terrain::Pool
                        | Terrain::Water
                        | Terrain::Ice
                        | Terrain::Lava
                )
            )
    };
    // (what bounds a room: a dark one's floor seen in part is a
    // rectangle too, its edge no wall)
    let bound = |x: i32, y: i32| {
        matches!(
            t(x, y),
            Some(
                Terrain::Wall
                    | Terrain::Doorway
                    | Terrain::OpenDoor
                    | Terrain::ClosedDoor
                    | Terrain::BrokenDoor
                    | Terrain::IronBars
            )
        )
    };
    // the rooms known whole, their walls seen all round (a dark one's
    // floor seen in part is a rectangle too, its edge no wall)
    let rooms: Vec<(i32, i32, i32, i32)> =
        if dress.inlay.is_some() || dress.pilasters || dress.piers || dress.rug.is_some() {
            rooms(&inside)
                .into_iter()
                .filter(|&(x0, y0, x1, y1)| {
                    (x0 - 1..=x1 + 1).all(|x| bound(x, y0 - 1) && bound(x, y1 + 1))
                        && (y0..=y1).all(|y| bound(x0 - 1, y) && bound(x1 + 1, y))
                })
                .collect()
        } else {
            Vec::new()
        };
    // a small room's north wall dressed as a whole: a niche in its middle
    // with a column either side, or a pair of columns; piers in its
    // corners (by the pattern a room of a few cells got none)
    for &(x0, y0, x1, _) in rooms.iter().filter(|r| r.2 - r.0 < SMALL_MOST) {
        let (y, w) = (y0 - 1, x1 - x0 + 1);
        if dress.piers {
            for cx in [x0 - 1, x1 + 1] {
                if plain(cx, y) {
                    out.insert((cx, y), Volume::Pier);
                }
            }
        }
        if !dress.pilasters {
            continue;
        }
        for x in x0..=x1 {
            out.entry((x, y)).or_insert(Volume::North(None));
        }
        let mid = x0 + (w - 1) / 2;
        let pieces = if w % 2 == 1 {
            let niche = w >= 3 && dress.alcoves && !torch_pattern(mid, y, true);
            [
                niche.then_some((mid, Piece::Alcove)),
                (w >= 5).then_some((mid - 1, Piece::Pilaster)),
                (w >= 5).then_some((mid + 1, Piece::Pilaster)),
            ]
        } else {
            [
                Some((mid, Piece::Pilaster)),
                Some((mid + 1, Piece::Pilaster)),
                None,
            ]
        };
        for (x, piece) in pieces.into_iter().flatten() {
            if plain(x, y) {
                out.insert((x, y), Volume::North(Some(piece)));
            }
        }
    }
    // a small room's rug: over its biggest stretch of bare floor known (no
    // feature, stairs, door nor trap under it, nothing hidden by it: what
    // lies on it is drawn on it), a margin in from the walls and less
    // from what it leaves bare beside it
    if dress.rug.is_some() {
        let bare = |x: i32, y: i32| floor(x, y) && !cell(x, y).matters;
        for &room in &rooms {
            let (rx0, ry0, rx1, ry1) = room;
            let (w, h) = (rx1 - rx0 + 1, ry1 - ry0 + 1);
            let small = dress.inlay.is_none() || w < BAND_LEAST.0 || h < BAND_LEAST.1;
            if !small {
                continue;
            }
            let Some((x0, y0, x1, y1)) = bare_stretch(room, &bare) else {
                continue;
            };
            let side = |at_wall: bool| if at_wall { RUG_MARGIN } else { RUG_BESIDE };
            let (rx0, ry0) = (
                x0 as f32 - 0.5 + side(x0 == rx0),
                y0 as f32 - 0.5 + side(y0 == ry0),
            );
            let (rx1, ry1) = (
                x1 as f32 + 0.5 - side(x1 == rx1),
                y1 as f32 + 0.5 - side(y1 == ry1),
            );
            for x in x0..=x1 {
                for y in y0..=y1 {
                    let (fx, fy) = (x as f32, y as f32);
                    out.insert(
                        (x, y),
                        Volume::Rug {
                            rect: [rx0 - fx, ry0 - fy, rx1 - fx, ry1 - fy],
                        },
                    );
                }
            }
        }
    }
    if dress.inlay.is_some() {
        let mut lines: HashMap<(i32, i32), Vec<[f32; 5]>> = HashMap::new();
        let mut corners = HashMap::new();
        for &(x0, y0, x1, y1) in &rooms {
            let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
            if w < BAND_LEAST.0 || h < BAND_LEAST.1 {
                continue;
            }
            // the band just inside the cells a cell in from the walls,
            // along their outer edges, its fillet inside it
            let edge = 0.5 + BAND_IN;
            let (bx0, by0, bx1, by1) = (
                x0 as f32 + edge,
                y0 as f32 + edge,
                x1 as f32 - edge,
                y1 as f32 - edge,
            );
            let i = FILLET_IN;
            let rings = [
                ((bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1), BAND),
                (
                    (bx0 + i, by0 + i),
                    (bx1 - i, by0 + i),
                    (bx0 + i, by1 - i),
                    (bx1 - i, by1 - i),
                    FILLET,
                ),
            ];
            for (nw, ne, sw, se, wide) in rings {
                for (a, b) in [(nw, ne), (sw, se), (nw, sw), (ne, se)] {
                    for x in x0 + 1..x1 {
                        for y in y0 + 1..y1 {
                            if let Some(seg) = clip(a, b, x, y) {
                                let [a0, a1, b0, b1] = seg;
                                lines
                                    .entry((x, y))
                                    .or_default()
                                    .push([a0, a1, b0, b1, wide]);
                            }
                        }
                    }
                }
            }
            for (cx, cy) in [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)] {
                let (x, y) = (cx.round() as i32, cy.round() as i32);
                corners.insert((x, y), [cx - x as f32, cy - y as f32]);
            }
            if w < LOZENGE_LEAST.0 || h < LOZENGE_LEAST.1 {
                continue;
            }
            // the lozenge inside the band, its corners a cell in from the
            // middles of the band's sides
            let (ix0, iy0, ix1, iy1) = (bx0 + 1.0, by0 + 1.0, bx1 - 1.0, by1 - 1.0);
            let (cx, cy) = ((ix0 + ix1) / 2.0, (iy0 + iy1) / 2.0);
            let tips = [(cx, iy0), (ix1, cy), (cx, iy1), (ix0, cy)];
            for k in 0..4 {
                let (a, b) = (tips[k], tips[(k + 1) % 4]);
                for x in x0 + 1..x1 {
                    for y in y0 + 1..y1 {
                        if let Some([a0, a1, b0, b1]) = clip(a, b, x, y) {
                            lines
                                .entry((x, y))
                                .or_default()
                                .push([a0, a1, b0, b1, LOZENGE]);
                        }
                    }
                }
            }
        }
        for ((x, y), segs) in lines {
            if !floor(x, y) || !plain(x, y) {
                continue;
            }
            let mut arr = [[0.0; 5]; 4];
            let n = segs.len().min(4);
            arr[..n].copy_from_slice(&segs[..n]);
            out.insert(
                (x, y),
                Volume::Inlay {
                    lines: arr,
                    n: n as u8,
                    corner: corners.get(&(x, y)).copied(),
                },
            );
        }
    }
    out
}

/// A rectangle of cells (x0, y0, x1, y1), its corners' cells in it.
type Rect = (i32, i32, i32, i32);

/// The biggest rectangle of cells in `room` (x0, y0, x1, y1) all `bare`,
/// at least `RUG_LEAST` (cells, either way round), the one nearest the
/// room's middle among the biggest.
fn bare_stretch(room: Rect, bare: &impl Fn(i32, i32) -> bool) -> Option<Rect> {
    let (x0, y0, x1, y1) = room;
    let (mx, my) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    // (the rectangle, its area, how far off the middle)
    let mut best: Option<(Rect, i32, f32)> = None;
    for ax in x0..=x1 {
        for ay in y0..=y1 {
            for bx in ax..=x1 {
                for by in ay..=y1 {
                    let (w, h) = (bx - ax + 1, by - ay + 1);
                    let fits = (w >= RUG_LEAST.0 && h >= RUG_LEAST.1)
                        || (w >= RUG_LEAST.1 && h >= RUG_LEAST.0);
                    if !fits || !(ax..=bx).all(|x| (ay..=by).all(|y| bare(x, y))) {
                        continue;
                    }
                    let off =
                        ((ax + bx) as f32 / 2.0 - mx).abs() + ((ay + by) as f32 / 2.0 - my).abs();
                    let better =
                        best.is_none_or(|(_, area, o)| w * h > area || (w * h == area && off < o));
                    if better {
                        best = Some(((ax, ay, bx, by), w * h, off));
                    }
                }
            }
        }
    }
    best.map(|(r, _, _)| r)
}

/// The rooms of the level as their floors show (`floor`: inside a room):
/// each area of it known that is a whole rectangle (x0, y0, x1, y1).
fn rooms(floor: &impl Fn(i32, i32) -> bool) -> Vec<(i32, i32, i32, i32)> {
    use nh_world::{COLNO, ROWNO};
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (x, y) in (0..ROWNO).flat_map(|y| (0..COLNO).map(move |x| (x, y))) {
        if !floor(x, y) || !seen.insert((x, y)) {
            continue;
        }
        let mut area = vec![(x, y)];
        let mut i = 0;
        while i < area.len() {
            let (ax, ay) = area[i];
            for n in [(ax + 1, ay), (ax - 1, ay), (ax, ay + 1), (ax, ay - 1)] {
                if floor(n.0, n.1) && seen.insert(n) {
                    area.push(n);
                }
            }
            i += 1;
        }
        let (x0, x1) = (
            area.iter().map(|a| a.0).min(),
            area.iter().map(|a| a.0).max(),
        );
        let (y0, y1) = (
            area.iter().map(|a| a.1).min(),
            area.iter().map(|a| a.1).max(),
        );
        if let (Some(x0), Some(x1), Some(y0), Some(y1)) = (x0, x1, y0, y1)
            && area.len() as i32 == (x1 - x0 + 1) * (y1 - y0 + 1)
        {
            out.push((x0, y0, x1, y1));
        }
    }
    out
}

/// The part of the line from `a` to `b` (level coordinates) inside cell
/// (x, y), in the cell's own metres; None if it does not cross it.
fn clip(a: (f32, f32), b: (f32, f32), x: i32, y: i32) -> Option<[f32; 4]> {
    let (lo_x, hi_x) = (x as f32 - 0.5, x as f32 + 0.5);
    let (lo_y, hi_y) = (y as f32 - 0.5, y as f32 + 0.5);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.0 - lo_x),
        (dx, hi_x - a.0),
        (-dy, a.1 - lo_y),
        (dy, hi_y - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t1 - t0 > 0.02).then_some([
        a.0 + dx * t0 - x as f32,
        a.1 + dy * t0 - y as f32,
        a.0 + dx * t1 - x as f32,
        a.1 + dy * t1 - y as f32,
    ])
}

/// The part of the rug the plan lays on this cell (`Volume::Rug`): its
/// cloth, flat on the floor, and its border's strips a little in from its
/// edge. Whether there was one.
fn rug(look: &mut Look, ctx: &Ctx) -> bool {
    let Some(Volume::Rug { rect }) = ctx.volume else {
        return false;
    };
    let Some((cloth, edge)) = ctx.branch.dress.rug else {
        return false;
    };
    let (Some(cloth), Some(edge), Some(wool)) = (
        trim(ctx, cloth, RUG_SHADE),
        trim(ctx, edge, RUG_BORDER_SHADE),
        trim(ctx, edge, FRINGE_SHADE),
    ) else {
        return false;
    };
    // a rectangle (x0, z0, x1, z1) in the cell's own metres, cut to the
    // cell, laid `y` up: flat on the floor, casting no shadow (a thin
    // slab's own shadow fell on the floor beside it in a hard band)
    let lay = |look: &mut Look, r: [f32; 4], paint: Paint, y: f32| {
        let [x0, z0, x1, z1] = [r[0].max(-0.5), r[1].max(-0.5), r[2].min(0.5), r[3].min(0.5)];
        if x1 - x0 > 0.005 && z1 - z0 > 0.005 {
            look.ground_tile(
                cuboid(x1 - x0, 0.004, z1 - z0),
                paint,
                at((x0 + x1) / 2.0, y, (z0 + z1) / 2.0),
            );
        }
    };
    lay(look, rect, cloth, 0.003);
    let [x0, z0, x1, z1] = rect;
    // its border, and a thin line inside it
    for (i, b) in [
        (RUG_BORDER_IN, RUG_BORDER),
        (RUG_BORDER_IN + RUG_BORDER + RUG_LINE_IN, RUG_LINE),
    ] {
        for strip in [
            [x0 + i, z0 + i, x1 - i, z0 + i + b],
            [x0 + i, z1 - i - b, x1 - i, z1 - i],
            [x0 + i, z0 + i + b, x0 + i + b, z1 - i - b],
            [x1 - i - b, z0 + i + b, x1 - i, z1 - i - b],
        ] {
            lay(look, strip, edge, 0.0045);
        }
    }
    fringe(look, rect, wool);
    true
}

/// The fringe along a rug's short ends (`rect` in the cell's own metres):
/// tassels reaching out from its edge, a few frayed short and each a
/// little askew; those whose middle is in this cell.
fn fringe(look: &mut Look, rect: [f32; 4], wool: Paint) {
    let [x0, z0, x1, z1] = rect;
    // along x when the rug is longer across z: its ends lie at z0 and z1
    let across = x1 - x0 <= z1 - z0;
    let (a0, a1) = if across { (x0, x1) } else { (z0, z1) };
    let ends = if across { [z0, z1] } else { [x0, x1] };
    let n = ((a1 - a0 - 2.0 * RUG_BORDER_IN) / FRINGE_STEP)
        .floor()
        .max(0.0) as i32;
    let start = (a0 + a1) / 2.0 - (n as f32 - 1.0) / 2.0 * FRINGE_STEP;
    for (side, &end) in ends.iter().enumerate() {
        let out = if side == 0 { -1.0 } else { 1.0 };
        for i in 0..n {
            // which tassel: the same each time the cell is drawn
            let h = (i as u32)
                .wrapping_mul(2_654_435_761)
                .wrapping_add((end * 1000.0) as i32 as u32)
                .rotate_left(13)
                .wrapping_mul(0x85eb_ca6b);
            let long = if h.is_multiple_of(5) {
                FRINGE_LONG.1
            } else {
                FRINGE_LONG.0
            };
            let along = start + i as f32 * FRINGE_STEP;
            let mid = end + out * long / 2.0;
            let (x, z) = if across { (along, mid) } else { (mid, along) };
            if !(-0.5..0.5).contains(&x) || !(-0.5..0.5).contains(&z) {
                continue;
            }
            let askew = ((h >> 8) % 21) as f32 - 10.0;
            let mesh = if across {
                cuboid(FRINGE_WIDE, 0.01, long)
            } else {
                cuboid(long, 0.01, FRINGE_WIDE)
            };
            look.sheet(mesh, wool, at(x, 0.002, z), Vector3::new(0.0, askew, 0.0));
        }
    }
}

/// How light an inlay's stone is: held down, so it reads as another stone
/// set in the floor and never as a line drawn over it.
const INLAY_SHADE: u8 = 62;

/// The inlay the plan gives a floor cell: its lines, flat in the floor, a
/// corner stone where a band's lines meet.
fn inlay(look: &mut Look, ctx: &Ctx) {
    let Some(Volume::Inlay { lines, n, corner }) = ctx.volume else {
        return;
    };
    let Some(stone) = ctx
        .branch
        .dress
        .inlay
        .and_then(|name| trim(ctx, name, INLAY_SHADE))
    else {
        return;
    };
    for &[x0, z0, x1, z1, wide] in &lines[..n as usize] {
        let len = (x1 - x0).hypot(z1 - z0);
        if len < 0.02 {
            continue;
        }
        let yaw = -(z1 - z0).atan2(x1 - x0).to_degrees();
        // a band or a lozenge laid in stones with joints between, a
        // fillet in one
        let stones = if wide > FILLET {
            (len / 0.5).round().max(1.0)
        } else {
            1.0
        };
        let (step, joint) = (len / stones, if stones > 1.0 { 0.03 } else { 0.0 });
        for k in 0..stones as usize {
            let t = (k as f32 + 0.5) / stones;
            look.turned(
                cuboid(step - joint, 0.004, wide),
                stone,
                at(x0 + (x1 - x0) * t, 0.003, z0 + (z1 - z0) * t),
                at(0.0, yaw, 0.0),
            );
        }
    }
    if let Some([x, z]) = corner {
        look.solid(cuboid(CORNER, 0.005, CORNER), stone, at(x, 0.0035, z));
    }
}
