//! The pieces of an outfit drawn in code (`object_kit`'s helmets and
//! garments): a winged helmet's wings of long feathers, a fur pelt over
//! the shoulders and down the back, a toga's drape over one shoulder, a
//! cape and a cloak of cloth. They are in the units of the kit that
//! places them (a helmet's, a garment's), y up, z forward, the wearer's
//! left +x.

use super::*;

/// The parts there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// One wing of a winged helmet, from its root (the origin) on the
    /// helmet's side: long feathers fanned up and back in the plane x = 0,
    /// short ones over their roots on both faces.
    Wing,
    /// A pelt worn round the neck (about the origin): a rolled collar, a
    /// cape over the shoulders and down the back to the waist in folds,
    /// its hem and edges ragged with tufts.
    Pelt,
    /// A toga worn about the chest (the origin): a broad band of cloth in
    /// folds over the left shoulder, across the chest and the back to the
    /// right hip, and its end hanging from the shoulder down the back.
    Toga,
    /// A monk's robe worn as the toga is, broader, its end to the knees.
    Kasaya,
    /// The border woven along a kasaya's edges.
    KasayaBorder,
    /// A god's himation: a kasaya's drape in deep folds, its end to the
    /// calves.
    Himation,
    /// The border woven along its edges.
    HimationBorder,
    /// A cape of cloth worn round the neck (the origin), to the hips: over
    /// the shoulders and down the back in folds, its collar rolled.
    Cape,
    /// A cloak as the cape, to the calves, flared clear of the legs.
    Cloak,
    /// A knight's tabard over his mail, worn round the neck (the origin):
    /// a panel down the chest and one down the back to the hips, joined
    /// over the shoulders.
    Tabard,
    /// One of a samurai's sode, worn on either upper arm (the origin at
    /// the shoulder, out from the arm +x, up the arm +y): lacquered lames
    /// curved round the arm under a broader top plate, each hanging a
    /// little further out than the one over it, so that every lame's top
    /// edge catches the light.
    SodeLames,
    /// The silk braid laced down its lames (odoshi), the lacquer showing
    /// above it on each.
    SodeLacing,
    /// The cross-knots along the foot of its lowest lame.
    SodeKnots,
    /// The rim rolled along its top plate.
    SodeRim,
    /// A sash round the waist (the origin): a flat band of cloth, its
    /// knot in front a little to the side.
    Obi,
    /// A samurai's hakama from the waist (the origin) to the ankles: wide
    /// trousers falling like a skirt in knife pleats, pressed flat at the
    /// waist and opening toward the hem.
    Hakama,
}

const UP: Vector3 = Vector3::UP;
const ACROSS: Vector3 = Vector3::RIGHT;

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::Wing => wing(m),
        Part::Pelt => pelt(m),
        Part::Toga => toga(m, &TOGA_WRAP),
        Part::Kasaya => toga(m, &KASAYA),
        Part::KasayaBorder => wrap_border(m, &KASAYA),
        Part::Himation => toga(m, &HIMATION),
        Part::HimationBorder => wrap_border(m, &HIMATION),
        Part::Cape => cloth(m, &CAPE),
        Part::Cloak => cloth(m, &CLOAK),
        Part::Tabard => tabard(m),
        Part::SodeLames => sode_lames(m),
        Part::SodeLacing => sode_lacing(m),
        Part::SodeKnots => sode_knots(m),
        Part::SodeRim => sode_rim(m),
        Part::Obi => obi(m),
        Part::Hakama => hakama(m),
    }
}

/// A sode's curve round the arm: its radius, how far in from the arm's
/// outside the curve's centre lies, and half the angle it spans
/// (radians): a broad plate, clear of the arm.
const SODE_R: f32 = 0.24;
const SODE_IN: f32 = 0.135;
const SODE_SPAN: f32 = 0.62;
/// How far its foot swings out from the shoulder (radians about the
/// front-back axis through the origin).
const SODE_TILT: f32 = 0.2;
/// Its top plate: its top over the shoulder, its height, half its
/// thickness.
const SODE_TOP: f32 = 0.05;
const PLATE_H: f32 = 0.045;
const PLATE_THICK: f32 = 0.0065;
/// The lames under it: how many, each one's height, how far under the
/// last one's top the next one's starts (they overlap), how much further
/// out each hangs, half a lame's thickness.
const LAMES: usize = 6;
const LAME_H: f32 = 0.05;
const LAME_STEP: f32 = 0.043;
const LAME_OUT: f32 = 0.0075;
const LAME_THICK: f32 = 0.0045;
/// Cords of braid down each lame, close side by side, from this share of
/// its height below its top (the lacquer shows above them) to its foot;
/// half a cord's width and thickness.
const CORDS: usize = 19;
const CORD_FROM: f32 = 0.3;
const CORD_HALF: f32 = 0.0064;
const CORD_THICK: f32 = 0.0015;
/// Cross-knots along the lowest lame.
const KNOTS: usize = 6;

/// A point of the sode's curve at `a` (radians, 0 in the middle, + to the
/// front), `out` further out than the curve, `y` up, its foot swung out.
fn sode_at(a: f32, out: f32, y: f32) -> Vector3 {
    let r = SODE_R + out;
    let (x, z) = (r * a.cos() - SODE_IN, r * a.sin());
    let (s, c) = SODE_TILT.sin_cos();
    v(x * c - y * s, x * s + y * c, z)
}

/// The top of lame `i` (0 the highest, under the plate), and how far out
/// it hangs.
fn lame(i: usize) -> (f32, f32) {
    let top = SODE_TOP - PLATE_H + 0.007 - i as f32 * LAME_STEP;
    (top, (i + 1) as f32 * LAME_OUT)
}

/// Out from the sode's face at `a`.
fn outward(a: f32) -> Vector3 {
    (sode_at(a, 1.0, 0.0) - sode_at(a, 0.0, 0.0)).normalized()
}

/// The sode's up at its foot's swing.
fn sode_up() -> Vector3 {
    let (s, c) = SODE_TILT.sin_cos();
    v(-s, c, 0.0)
}

/// A curved slab along the sode's curve, `half_h` high round `y`, `out`
/// further out, `half_t` thick: square-cut with rounded edges.
fn sode_slab(m: &mut Merge, y: f32, out: f32, half_h: f32, half_t: f32) {
    let n = 18;
    let path: Vec<Vector3> = (0..=n)
        .map(|k| sode_at((frac(k, n) * 2.0 - 1.0) * SODE_SPAN, out, y))
        .collect();
    m.sweep(&path, |_| (half_t, half_h), 8.0, false, sode_up());
}

fn sode_lames(m: &mut Merge) {
    sode_slab(m, SODE_TOP - PLATE_H / 2.0, 0.0, PLATE_H / 2.0, PLATE_THICK);
    for i in 0..LAMES {
        let (top, out) = lame(i);
        sode_slab(m, top - LAME_H / 2.0, out, LAME_H / 2.0, LAME_THICK);
    }
}

/// Where the `k`th of `n` things across the sode lies (radians), clear of
/// its ends.
fn across(k: usize, n: usize) -> f32 {
    ((k as f32 + 0.5) / n as f32 * 2.0 - 1.0) * SODE_SPAN * 0.95
}

/// A flat braid along `path`, `half_w` wide across `width` (square to the
/// path) and `half_t` thick, lying on a surface whose outside is
/// `toward`: six sides, its normals as a flat cord's, rounded a little
/// toward its edges (whole faces lit, not stripes).
fn braid(
    m: &mut Merge,
    path: &[Vector3],
    width: Vector3,
    toward: Vector3,
    (half_w, half_t): (f32, f32),
) {
    const SIDES: usize = 6;
    let n = path.len();
    let along = |i: usize| (path[(i + 1).min(n - 1)] - path[i.saturating_sub(1)]).normalized();
    m.rings(n, SIDES, |i, j| {
        let t = along(i);
        let w = (width - t * t.dot(width)).normalized();
        let mut out = t.cross(w).normalized();
        if out.dot(toward) < 0.0 {
            out = -out;
        }
        let a = TAU * j as f32 / SIDES as f32;
        let (s, c) = a.sin_cos();
        let face = if s >= -1e-3 { 1.0 } else { -1.0 };
        (
            path[i] + w * (c * half_w) + out * (s * half_t),
            (w * (c * 0.35) + out * face).normalized(),
        )
    });
}

fn sode_lacing(m: &mut Merge) {
    // (the lowest lame is cross-knotted instead)
    for i in 0..LAMES - 1 {
        let (top, out) = lame(i);
        let face = out + LAME_THICK + CORD_THICK;
        let from = top - CORD_FROM * LAME_H;
        let foot = top - LAME_H;
        for k in 0..CORDS {
            let a = across(k, CORDS);
            // out of the lame's face, down it and round under its foot
            let path = [
                sode_at(a, face - 0.002, from + 0.003),
                sode_at(a, face, from),
                sode_at(a, face, foot + 0.003),
                sode_at(a, out + 0.001, foot - 0.003),
            ];
            let width = v(-a.sin(), 0.0, a.cos());
            braid(m, &path, width, outward(a), (CORD_HALF, CORD_THICK));
        }
    }
}

fn sode_knots(m: &mut Merge) {
    let (top, out) = lame(LAMES - 1);
    let face = out + LAME_THICK + CORD_THICK;
    let (hi, lo) = (top - 0.012, top - LAME_H + 0.01);
    let d = SODE_SPAN / KNOTS as f32 * 0.5;
    for k in 0..KNOTS {
        let a = across(k, KNOTS);
        for (a0, a1) in [(a - d, a + d), (a + d, a - d)] {
            let path: Vec<Vector3> = (0..=4)
                .map(|j| {
                    let t = frac(j, 4);
                    // a little proud of the lame where the two cross
                    let lift = 0.0015 * (PI * t).sin();
                    sode_at(a0 + (a1 - a0) * t, face + lift, hi + (lo - hi) * t)
                })
                .collect();
            let width = (path[4] - path[0]).cross(outward(a));
            braid(m, &path, width, outward(a), (CORD_HALF, CORD_THICK * 1.3));
        }
    }
}

fn sode_rim(m: &mut Merge) {
    let n = 18;
    let path: Vec<Vector3> = (0..=n)
        .map(|k| sode_at((frac(k, n) * 2.0 - 1.0) * SODE_SPAN, 0.0, SODE_TOP))
        .collect();
    m.sweep(
        &path,
        |_| (PLATE_THICK * 1.25, 0.0045),
        2.0,
        false,
        sode_up(),
    );
}

/// An obi's round the waist: half its width across and front to back,
/// half its height, half its thickness.
const OBI_X: f32 = 0.465;
const OBI_Z: f32 = 0.363;
const OBI_HALF: f32 = 0.11;
const OBI_THICK: f32 = 0.028;

fn obi(m: &mut Merge) {
    let n = 48;
    let round: Vec<Vector3> = (0..n)
        .map(|k| {
            let a = frac(k, n) * TAU;
            v(OBI_X * a.sin(), 0.0, OBI_Z * a.cos())
        })
        .collect();
    m.sweep(&round, |_| (OBI_THICK, OBI_HALF), 8.0, true, UP);
    // its knot, a flat bow off to one side of the front
    let (s, c) = 0.3f32.sin_cos();
    let at = v(OBI_X * s, 0.0, OBI_Z * c + OBI_THICK * 2.0);
    let front = v(s * 0.4, 0.0, c).normalized();
    let side = UP.cross(front).normalized();
    for k in [-1.0f32, 1.0] {
        let lobe: Vec<Vector3> = (0..=6)
            .map(|j| {
                let t = frac(j, 6);
                at + side * (k * 0.12 * t)
                    + UP * (0.03 * (PI * t).sin() * k)
                    + front * (0.03 * (PI * t).sin())
            })
            .collect();
        m.sweep(&lobe, |j| (0.022, 0.07 + 0.02 * frac(j, 6)), 6.0, false, UP);
    }
    m.sweep(
        &[at - front * 0.01, at + front * 0.03],
        |_| (0.05, 0.06),
        6.0,
        false,
        UP,
    );
}

/// A hakama's height, its girth at the waist and how far it flares out
/// to the hem, its pleats round it and how deep they open at the hem.
const HAKAMA_LEN: f32 = 1.6;
const HAKAMA_TOP: f32 = 0.36;
const HAKAMA_FLARE: f32 = 0.31;
const PLEATS: f32 = 16.0;
const PLEAT_DEEP: f32 = 0.06;

/// A point of the hakama `s` down from the waist (0) to the hem (1), `t`
/// round it from the back (0, 1), through the front (0.5).
fn hakama_at(s: f32, t: f32) -> Vector3 {
    let a = (t * 2.0 - 1.0) * PI;
    let r = HAKAMA_TOP + HAKAMA_FLARE * s.powf(0.75);
    // knife pleats: each a long slope ending in a sharp fold, pressed
    // flat at the waist
    let x = (t * PLEATS).fract();
    let fold = if x < 0.8 { x / 0.8 } else { (1.0 - x) / 0.2 };
    let deep = PLEAT_DEEP * smoothstep(0.03, 0.5, s) * (0.4 + 0.6 * s);
    let r = r + deep * (fold - 0.5);
    let hem = 0.012 * s.powi(4) * (a * 5.0).sin();
    v(r * a.sin(), 0.08 - HAKAMA_LEN * s + hem, 0.8 * r * a.cos())
}

fn hakama(m: &mut Merge) {
    for face in [1.0f32, -1.0] {
        sheet(m, 22, 16 * 10, &hakama_at, face);
    }
    // the band at the waist
    let n = 48;
    let waist: Vec<Vector3> = (0..n).map(|k| hakama_at(0.0, frac(k, n))).collect();
    m.sweep(&waist, |_| (0.02, 0.05), 6.0, true, UP);
}

/// A wing's long feathers, front to back: how far back from straight up
/// each points (degrees), and how long it is.
const FEATHERS: [(f32, f32); 7] = [
    (4.0, 0.3),
    (16.0, 0.4),
    (29.0, 0.5),
    (42.0, 0.58),
    (55.0, 0.62),
    (68.0, 0.58),
    (81.0, 0.48),
];

/// The short feathers over their roots.
const COVERTS: [(f32, f32); 5] = [
    (10.0, 0.2),
    (26.0, 0.25),
    (42.0, 0.27),
    (58.0, 0.26),
    (74.0, 0.22),
];

fn wing(m: &mut Merge) {
    for (k, &(a, len)) in FEATHERS.iter().enumerate() {
        // each a hair to one side of the last, so the overlaps stay clean
        let x = if k % 2 == 0 { 0.006 } else { -0.006 };
        feather(m, x, a, len, 0.055);
    }
    for &(a, len) in &COVERTS {
        for x in [0.02f32, -0.02] {
            feather(m, x, a, len, 0.06);
        }
    }
}

/// A feather from the root out `len` at `a` degrees back from straight
/// up, its tip curling further back: a quill at the root, broadest past
/// its middle, pointed, flat across x.
fn feather(m: &mut Merge, x: f32, a: f32, len: f32, half_width: f32) {
    let (s, c) = a.to_radians().sin_cos();
    let out = v(0.0, c, -s);
    let back = v(0.0, -s, -c);
    let n = 10;
    let path: Vec<Vector3> = (0..=n)
        .map(|k| {
            let u = frac(k, n);
            v(x, 0.0, 0.0) + out * (0.03 + u * len) + back * (0.08 * len * u * u)
        })
        .collect();
    m.sweep(
        &path,
        |k| {
            let u = frac(k, n);
            let swell = (PI * u.powf(1.15)).sin().max(0.02).powf(0.7);
            (half_width * swell, 0.012 * (1.0 - 0.6 * u))
        },
        2.0,
        false,
        ACROSS,
    );
}

/// How a drape over the shoulders hangs: how high its top lies on the
/// neck, where its hem is (below the
/// neck), how far out it flares there, and how much further behind (clear
/// of the legs as they swing); how deep its folds are and how many round
/// it; how ragged its hem; between which shares of its length it narrows
/// from all round the shoulders to behind them.
struct Hang {
    top: f32,
    hem: f32,
    flare: f32,
    back: f32,
    folds: f32,
    fold_count: f32,
    rag: f32,
    narrows: (f32, f32),
}

/// A pelt, to the waist.
const PELT: Hang = Hang {
    top: 0.2,
    hem: -1.0,
    flare: 0.56,
    back: 0.12,
    folds: 0.06,
    fold_count: 6.0,
    rag: 0.05,
    narrows: (0.12, 0.42),
};

/// A cape of cloth, to the hips.
const CAPE: Hang = Hang {
    top: 0.11,
    hem: -1.35,
    flare: 0.62,
    back: 0.2,
    folds: 0.06,
    fold_count: 7.0,
    rag: 0.01,
    narrows: (0.09, 0.32),
};

/// A cloak of cloth, to the calves.
const CLOAK: Hang = Hang {
    top: 0.11,
    hem: -2.1,
    flare: 0.74,
    back: 0.34,
    folds: 0.07,
    fold_count: 8.0,
    rag: 0.012,
    narrows: (0.06, 0.22),
};

/// A drape hung so: `s` from the neck (0) to the hem (1), `t` round it
/// from one edge (0) to the other (1).
fn drape(h: &Hang, s: f32, t: f32) -> Vector3 {
    // its profile, out from the middle and down: over the shoulder in a
    // curve, then hanging down the back
    // (a lower top a little wider round the neck)
    let (a, b, c, d) = (
        (0.22f32 + 0.2 * (0.2 - h.top), h.top),
        (0.5, 0.17),
        (0.5, -0.2),
        (h.flare, h.hem),
    );
    let k = 1.0 - s;
    let w = [k * k * k, 3.0 * k * k * s, 3.0 * k * s * s, s * s * s];
    let r = w[0] * a.0 + w[1] * b.0 + w[2] * c.0 + w[3] * d.0;
    let y = w[0] * a.1 + w[1] * b.1 + w[2] * c.1 + w[3] * d.1;
    // all the way round over the shoulders, only behind further down
    let reach = 80.0 + 100.0 * (1.0 - smoothstep(h.narrows.0, h.narrows.1, s));
    let a = ((t * 2.0 - 1.0) * reach).to_radians();
    // folds that deepen toward the hem
    let fold = 1.0 + h.folds * s * (a * h.fold_count).sin();
    let rz = (r * 0.66 + h.back * s) * fold;
    let hem = h.rag * s.powi(3) * (a * 9.0 + 1.3).sin();
    v(r * fold * a.sin(), y + hem, -rz * a.cos())
}

/// The pelt's cape.
fn cape(s: f32, t: f32) -> Vector3 {
    drape(&PELT, s, t)
}

/// A cloak or a cape of cloth: the drape, hemmed, and a rolled collar.
fn cloth(m: &mut Merge, h: &Hang) {
    let at = |s: f32, t: f32| drape(h, s, t);
    for face in [1.0f32, -1.0] {
        sheet(m, 24, 36, &at, face);
    }
    // (low on the neck: a cloth's collar lies on the shoulders, under the
    // chin)
    let n = 32;
    let neck = |a: f32| v(0.27 * a.sin(), 0.1 + 0.03 * a.cos(), -0.2 * a.cos());
    let collar: Vec<Vector3> = (0..n).map(|k| neck(frac(k, n) * TAU)).collect();
    m.sweep(&collar, |_| (0.04, 0.04), 2.0, true, UP);
    // the hem turned up: a band of the cloth along its foot
    let foot: Vec<Vector3> = (0..=36).map(|k| drape(h, 1.0, frac(k, 36))).collect();
    m.sweep(&foot, |_| (0.012, 0.022), 2.0, false, UP);
}

fn pelt(m: &mut Merge) {
    for face in [1.0f32, -1.0] {
        sheet(m, 14, 32, &cape, face);
    }
    // a roll of fur round the neck under the ruff, so no gap shows
    let n = 32;
    let neck = |a: f32| v(0.26 * a.sin(), 0.17 + 0.03 * a.cos(), -0.2 * a.cos());
    let roll: Vec<Vector3> = (0..n).map(|k| neck(frac(k, n) * TAU)).collect();
    m.sweep(&roll, |_| (0.07, 0.06), 2.0, true, UP);
    // the ruff: locks fallen out over the shoulders, and shorter ones
    // standing round the neck
    for k in 0..34 {
        let a = (k as f32 + 0.5) / 34.0 * TAU;
        let out = v(a.sin(), 0.0, -a.cos() * 0.8).normalized();
        let len = 0.2 + 0.1 * jitter(k, 1);
        let dir = out * 0.75 + v(0.0, -0.65 - 0.3 * jitter(k, 5), 0.0);
        let flat = out * 0.65 + v(0.0, 0.75, 0.0);
        lock(m, neck(a) + v(0.0, 0.03, 0.0), dir, flat, len, 0.075, 0.025);
    }
    for k in 0..22 {
        let a = k as f32 / 22.0 * TAU;
        let out = v(a.sin(), 0.0, -a.cos() * 0.8).normalized();
        let len = 0.13 + 0.04 * jitter(k, 2);
        let dir = out * 0.55 + v(0.0, 0.8, 0.0);
        let flat = out * 0.8 + v(0.0, -0.55, 0.0);
        lock(m, neck(a) * 0.92, dir, flat, len, 0.06, 0.02);
    }
    // locks hanging from the hem, and down the edges behind the arms
    for k in 0..=24 {
        let t = frac(k, 24);
        let root = cape(0.97, t);
        let out = v(root.x, 0.0, root.z).normalized();
        let len = 0.13 + 0.08 * jitter(k, 3);
        lock(
            m,
            root,
            v(0.0, -1.0, 0.0) + out * 0.25,
            out,
            len,
            0.05,
            0.018,
        );
    }
    for (t, side) in [(0.0f32, -1.0f32), (1.0, 1.0)] {
        for k in 0..8 {
            let s = 0.38 + frac(k, 8) * 0.55;
            let root = cape(s, t);
            let along = (cape(s, t) - cape(s, t - side * 0.02)).normalized();
            let out = v(root.x, 0.0, root.z).normalized();
            let len = 0.1 + 0.05 * jitter(k, 4);
            lock(m, root, along + v(0.0, -0.8, 0.0), out, len, 0.045, 0.016);
        }
    }
}

/// A steady scatter in 0..1 for the `k`th of a row of things (`row` tells
/// rows apart), so a pelt's locks differ and stay as they were.
fn jitter(k: usize, row: usize) -> f32 {
    let x = (k as f32 * 12.9898 + row as f32 * 78.233).sin() * 43758.547;
    x - x.floor()
}

/// A lock of fur from `root` out along `dir` (drooping as it goes),
/// narrowing to a round tip, flat across `flat`.
fn lock(
    m: &mut Merge,
    root: Vector3,
    dir: Vector3,
    flat: Vector3,
    len: f32,
    half_width: f32,
    half_thick: f32,
) {
    let dir = dir.normalized();
    let n = 6;
    let path: Vec<Vector3> = (0..=n)
        .map(|k| {
            let u = frac(k, n);
            root + dir * (u * len) + v(0.0, -0.25 * len * u * u, 0.0)
        })
        .collect();
    // the thin way across the lock: `flat` square to where it heads
    let flat = (flat - dir * dir.dot(flat)).normalized();
    m.sweep(
        &path,
        |k| {
            let u = frac(k, n);
            // round-tipped, not a spike
            let taper = (1.0 - u * u).max(0.0).sqrt().max(0.2);
            (half_width * taper, half_thick * (1.0 - 0.7 * u))
        },
        2.0,
        false,
        flat,
    );
}

/// Where a toga's band runs, round from over the left shoulder: down
/// across the chest to the right hip, and back up across the back.
const TOGA: [[f32; 3]; 8] = [
    [0.24, 0.44, 0.0],
    [0.12, 0.2, 0.3],
    [-0.14, -0.3, 0.33],
    [-0.36, -0.78, 0.18],
    [-0.4, -0.86, -0.12],
    [-0.16, -0.42, -0.32],
    [0.1, 0.06, -0.3],
    [0.26, 0.36, -0.16],
];

/// A point `s` (0..1) round a closed curve through `pts` (Catmull-Rom).
fn round_through(pts: &[[f32; 3]], s: f32) -> Vector3 {
    let n = pts.len();
    let f = s.rem_euclid(1.0) * n as f32;
    let i = f.floor() as usize;
    let t = f - i as f32;
    let p = |k: usize| {
        let q = pts[(i + k + n - 1) % n];
        v(q[0], q[1], q[2])
    };
    let (a, b, c, d) = (p(0), p(1), p(2), p(3));
    (b * 2.0
        + (c - a) * t
        + (a * 2.0 - b * 5.0 + c * 4.0 - d) * (t * t)
        + (b * 3.0 - a - c * 3.0 + d) * (t * t * t))
        * 0.5
}

/// Out from the body at `p`: from the chest's middle, or over the
/// shoulder from the shoulder's.
fn off_body(p: Vector3) -> Vector3 {
    let chest = v(p.x, 0.0, p.z);
    let shoulder = p - v(0.22, 0.2, 0.0);
    let k = smoothstep(0.15, 0.4, p.y);
    (chest.normalized() * (1.0 - k) + shoulder.normalized() * k).normalized()
}

/// How broad a wrap over the shoulder is (half its width across the chest)
/// and how far its end hangs down the back.
struct Wrap {
    half: f32,
    end: f32,
    /// How deep its folds are along the band, and how many across it;
    /// how many quads across (enough for its folds to stay smooth).
    fold: f32,
    folds: f32,
    across: usize,
}

/// A toga's band, its end to the waist.
const TOGA_WRAP: Wrap = Wrap {
    half: 0.15,
    end: 1.0,
    fold: 0.025,
    folds: 2.5,
    across: 8,
};

/// A monk's robe wrapped as a kasaya: broad across the chest, its end to
/// the knees.
const KASAYA: Wrap = Wrap {
    half: 0.27,
    end: 1.7,
    fold: 0.025,
    folds: 2.5,
    across: 10,
};

/// A god's himation: as broad as a kasaya, in deep folds, its end to the
/// calves.
const HIMATION: Wrap = Wrap {
    half: 0.24,
    end: 2.1,
    fold: 0.06,
    folds: 4.5,
    across: 28,
};

fn toga(m: &mut Merge, w: &Wrap) {
    // the band: `s` round it, `t` across it, a little off the body, in
    // folds along it; gathered narrow over the shoulder
    let band = |s: f32, t: f32| toga_band(w, s, t * 2.0 - 1.0);
    // the cloth's weave as square on the band as on a cloak
    let length: f32 = (0..64)
        .map(|k| {
            (round_through(&TOGA, frac(k + 1, 64)) - round_through(&TOGA, frac(k, 64))).length()
        })
        .sum();
    let span = Vector2::new(2.0 * w.half / length, 1.0) * length;
    for face in [1.0f32, -1.0] {
        spanned(m, 96, w.across, &|s, t| band(s, t), face, span);
    }
    for face in [1.0f32, -1.0] {
        spanned(
            m,
            16,
            w.across,
            &|s, t| toga_end(w, s, t),
            face,
            Vector2::new(0.24, w.end),
        );
    }
}

/// A toga's end, hanging from the shoulder down the back in folds: `s`
/// down it, `t` across.
fn toga_end(w: &Wrap, s: f32, t: f32) -> Vector3 {
    let u = t * 2.0 - 1.0;
    let fold = 1.6 * w.fold * s * (u * PI * w.folds * 0.8).sin();
    let top = v(0.24 + 0.12 * u, 0.42, -0.08 - 0.06 * (1.0 - u * u));
    let d = s * w.end;
    top + v(0.06 * d, -d, -0.24 * d - fold - 0.05 * d * d / w.end)
}

/// A toga's band (`toga`'s), at `s` round it and `u` across (-1..1),
/// a little off the body.
fn toga_band(w: &Wrap, s: f32, u: f32) -> Vector3 {
    let e = 1e-3;
    let p = round_through(&TOGA, s);
    let along = (round_through(&TOGA, s + e) - round_through(&TOGA, s - e)).normalized();
    let out = off_body(p);
    let across = along.cross(out).normalized();
    let shoulder = smoothstep(0.2, 0.42, p.y);
    let half = w.half * (1.0 - 0.4 * shoulder);
    let fold = w.fold * (u * PI * w.folds + s * 9.0).sin() * (1.0 - shoulder * 0.6);
    p + across * (u * half) + out * (0.05 + fold + 0.03 * (1.0 - u * u))
}

/// The border woven along a wrap's edges: round both edges of its band,
/// and down both edges and along the foot of its end.
fn wrap_border(m: &mut Merge, w: &Wrap) {
    let n = 96;
    for u in [-1.0f32, 1.0] {
        let edge: Vec<Vector3> = (0..n).map(|k| toga_band(w, frac(k, n), u * 0.97)).collect();
        m.sweep(&edge, |_| (0.022, 0.012), 2.0, true, UP);
    }
    for t in [0.02f32, 0.98] {
        let side: Vec<Vector3> = (0..=16).map(|k| toga_end(w, frac(k, 16), t)).collect();
        m.sweep(&side, |_| (0.022, 0.012), 2.0, false, ACROSS);
    }
    let foot: Vec<Vector3> = (0..=12).map(|k| toga_end(w, 0.99, frac(k, 12))).collect();
    m.sweep(&foot, |_| (0.022, 0.012), 2.0, false, UP);
}

/// How far a tabard reaches: half its width, its top and its hem below
/// the neck.
const TABARD_HALF: f32 = 0.31;
const TABARD_TOP: f32 = -0.04;
const TABARD_HEM: f32 = -1.25;

/// A tabard's panel (`front` 1, the back -1): `s` from the shoulders (0)
/// to the hem (1), `t` across; over the chest's swell, then hanging
/// straight, its folds deepening down it.
fn panel(front: f32, s: f32, t: f32) -> Vector3 {
    let x = (t * 2.0 - 1.0) * TABARD_HALF;
    let y = TABARD_TOP + (TABARD_HEM - TABARD_TOP) * s;
    // clear of the chest (a woman's too) and of the belt's buckle
    let (base, swell) = if front > 0.0 {
        (0.24, 0.18)
    } else {
        (0.2, 0.08)
    };
    let off = base + swell * smoothstep(0.0, 0.2, s) + 0.06 * (s - 0.5).max(0.0);
    let fold = 0.012 * s * (t * PI * 5.0).sin();
    v(x, y, front * (off - 0.3 * x * x + fold))
}

fn tabard(m: &mut Merge) {
    for front in [1.0f32, -1.0] {
        for face in [1.0f32, -1.0] {
            sheet(m, 12, 10, &|s, t| panel(front, s, t), face);
        }
    }
    // over each shoulder by the neck, from the front panel's top to the
    // back's
    for side in [-1.0f32, 1.0] {
        let strap = |s: f32, t: f32| {
            let x = side * (0.13 + 0.17 * t);
            let a = (s - 0.5) * PI;
            let (z0, z1) = (panel(1.0, 0.0, 0.5).z, panel(-1.0, 0.0, 0.5).z);
            let z = (z0 + z1) / 2.0 - (z0 - z1) / 2.0 * a.sin();
            v(x, TABARD_TOP + 0.2 * a.cos(), z + 0.3 * x * x * a.sin())
        };
        for face in [1.0f32, -1.0] {
            sheet(m, 8, 3, &strap, face);
        }
    }
}

/// A sheet of `rows` by `cols` quads, `at(s, t)` its points (each of `s`
/// and `t` in 0..1), its normals turned the way `face` says.
fn sheet(m: &mut Merge, rows: usize, cols: usize, at: &impl Fn(f32, f32) -> Vector3, face: f32) {
    spanned(m, rows, cols, at, face, Vector2::ONE);
}

/// A sheet as `sheet` lays one, its texture `span` units across (along
/// `t`) and along (`s`): a long band's cloth as square as a cloak's.
fn spanned(
    m: &mut Merge,
    rows: usize,
    cols: usize,
    at: &impl Fn(f32, f32) -> Vector3,
    face: f32,
    span: Vector2,
) {
    let base = m.pos.len() as i32;
    let e = 1e-3;
    for i in 0..=rows {
        for j in 0..=cols {
            let (s, t) = (frac(i, rows), frac(j, cols));
            let ds = at((s + e).min(1.0), t) - at((s - e).max(0.0), t);
            let dt = at(s, (t + e).min(1.0)) - at(s, (t - e).max(0.0));
            let n = dt.cross(ds).normalized() * face;
            m.vertex(at(s, t), n, Vector2::new(t * span.x, s * span.y));
        }
    }
    let c = cols as i32 + 1;
    for i in 0..rows as i32 {
        for j in 0..cols as i32 {
            let a = base + i * c + j;
            m.tri(a, a + 1, a + c);
            m.tri(a + 1, a + c + 1, a + c);
        }
    }
}
