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
    /// A cape of cloth worn round the neck (the origin), to the hips: over
    /// the shoulders and down the back in folds, its collar rolled.
    Cape,
    /// A cloak as the cape, to the calves, flared clear of the legs.
    Cloak,
    /// A knight's tabard over his mail, worn round the neck (the origin):
    /// a panel down the chest and one down the back to the hips, joined
    /// over the shoulders.
    Tabard,
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
        Part::Cape => cloth(m, &CAPE),
        Part::Cloak => cloth(m, &CLOAK),
        Part::Tabard => tabard(m),
    }
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
}

/// A toga's band, its end to the waist.
const TOGA_WRAP: Wrap = Wrap {
    half: 0.15,
    end: 1.0,
};

/// A monk's robe wrapped as a kasaya: broad across the chest, its end to
/// the knees.
const KASAYA: Wrap = Wrap {
    half: 0.27,
    end: 1.7,
};

fn toga(m: &mut Merge, w: &Wrap) {
    // the band: `s` round it, `t` across it, a little off the body, in
    // folds along it; gathered narrow over the shoulder
    let band = |s: f32, t: f32| {
        let e = 1e-3;
        let p = round_through(&TOGA, s);
        let along = (round_through(&TOGA, s + e) - round_through(&TOGA, s - e)).normalized();
        let out = off_body(p);
        let across = along.cross(out).normalized();
        let shoulder = smoothstep(0.2, 0.42, p.y);
        let half = w.half * (1.0 - 0.4 * shoulder);
        let u = t * 2.0 - 1.0;
        let fold = 0.025 * (u * PI * 2.5 + s * 9.0).sin() * (1.0 - shoulder * 0.6);
        p + across * (u * half) + out * (0.05 + fold + 0.03 * (1.0 - u * u))
    };
    for face in [1.0f32, -1.0] {
        sheet(m, 64, 8, &|s, t| band(s, t), face);
    }
    // its end, hanging from the shoulder down the back, in folds
    let end = |s: f32, t: f32| {
        let u = t * 2.0 - 1.0;
        let fold = 0.04 * s * (u * PI * 2.0).sin();
        let top = v(0.24 + 0.12 * u, 0.42, -0.08 - 0.06 * (1.0 - u * u));
        let d = s * w.end;
        top + v(0.06 * d, -d, -0.24 * d - fold - 0.05 * d * d / w.end)
    };
    for face in [1.0f32, -1.0] {
        sheet(m, 12, 8, &end, face);
    }
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
    let base = m.pos.len() as i32;
    let e = 1e-3;
    for i in 0..=rows {
        for j in 0..=cols {
            let (s, t) = (frac(i, rows), frac(j, cols));
            let ds = at((s + e).min(1.0), t) - at((s - e).max(0.0), t);
            let dt = at(s, (t + e).min(1.0)) - at(s, (t - e).max(0.0));
            let n = dt.cross(ds).normalized() * face;
            m.vertex(at(s, t), n, Vector2::new(t, s));
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
