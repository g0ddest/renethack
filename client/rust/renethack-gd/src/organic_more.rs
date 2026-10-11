//! More creatures' shapes (`creature_kit`): the cockatrice, a rooster
//! with a serpent's tail, and its chick; the shrieker, a fungus with a
//! funnel for a cap, and the violet fungus with its grasping tendrils;
//! the gelatinous cube with what it has swallowed hanging in it.

use super::*;

/// The parts there are, by creature. A part is what one material covers,
/// on one pivot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A cockatrice's plump body, its neck and folded wings.
    CockBody,
    /// Its head (on the Head pivot): the skull; its comb and wattles; its
    /// beak.
    CockHead,
    CockComb,
    CockBeak,
    /// A leg from the hip (on its pivot), scaled, spurred, three toes
    /// forward and one back.
    CockLeg,
    /// The serpent's tail from its rump, lying in an S on the floor.
    CockTail,
    /// A chickatrice's round body; its head; a stubby leg; its tail.
    ChickBody,
    ChickHead,
    ChickLeg,
    ChickTail,
    /// A shrieker: a stalk flaring to a funnel, two buds at its foot; the
    /// funnel's inside; the dark throat at its bottom.
    Shrieker,
    ShriekerBell,
    ShriekerThroat,
    /// A violet fungus: a cluster of caps, tendrils curling out over the
    /// floor; the sticky ends of the tendrils.
    Violet,
    VioletTips,
    /// A gelatinous cube: the cube; the bones in it; the metal in it.
    Cube,
    CubeBones,
    CubeLoot,
}

/// Where a cockatrice's head, tail and legs join it (the legs' x is
/// either side), and how high its hips are; a chickatrice's.
pub const COCK_HEAD: [f32; 3] = [0.0, 0.8, 0.25];
pub const COCK_TAIL: [f32; 3] = [0.0, 0.47, -0.24];
pub const COCK_HIP: [f32; 3] = [0.08, 0.34, 0.02];
pub const CHICK_HEAD: [f32; 3] = [0.0, 0.5, 0.14];
pub const CHICK_TAIL: [f32; 3] = [0.0, 0.24, -0.2];
pub const CHICK_HIP: [f32; 3] = [0.08, 0.14, 0.02];

/// a plump body, the breast forward, rising into the neck
const COCK_BODY: &[Section] = &[
    s(0.52, -0.27, 0.03, 0.03),
    s(0.52, -0.23, 0.11, 0.12),
    s(0.48, -0.13, 0.17, 0.18),
    s(0.46, 0.0, 0.18, 0.2),
    s(0.48, 0.1, 0.16, 0.19),
    s(0.54, 0.17, 0.12, 0.14),
    s(0.62, 0.21, 0.08, 0.09),
    s(0.7, 0.23, 0.06, 0.06),
    s(0.76, 0.24, 0.05, 0.05),
    s(0.78, 0.24, 0.02, 0.02),
];

const UP: Vector3 = Vector3::UP;

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::CockBody => {
            m.loft(COCK_BODY, Transform3D::IDENTITY);
            // the wings folded along the sides, their long feathers
            // reaching back over the rump
            for side in [-1.0f32, 1.0] {
                let at = v(side * 0.16, 0.5, -0.02);
                m.ball(
                    at,
                    v(0.045, 0.13, 0.21),
                    [-12.0, side * 8.0, side * 6.0],
                    false,
                    16,
                );
                for k in 0..3 {
                    let tip = v(
                        side * (0.15 - 0.012 * k as f32),
                        0.44 + 0.03 * k as f32,
                        -0.24,
                    );
                    m.ball(
                        tip,
                        v(0.025, 0.035, 0.09),
                        [-20.0 + 8.0 * k as f32, 0.0, 0.0],
                        false,
                        10,
                    );
                }
            }
        }
        Part::CockHead => {
            m.ball(v(0.0, 0.0, 0.0), v(0.065, 0.07, 0.08), [0.0; 3], false, 14);
        }
        Part::CockComb => {
            // a comb of five lobes over the crown, wattles under the beak
            for k in 0..5 {
                let t = frac(k, 4) * 2.0 - 1.0;
                let at = v(0.0, 0.075 + 0.025 * (1.0 - t * t), 0.03 * t + 0.01);
                m.ball(at, v(0.012, 0.03, 0.02), [0.0; 3], false, 10);
            }
            for side in [-1.0f32, 1.0] {
                m.ball(
                    v(side * 0.018, -0.07, 0.07),
                    v(0.016, 0.03, 0.018),
                    [0.0; 3],
                    false,
                    10,
                );
            }
        }
        Part::CockBeak => {
            m.rod(
                v(0.0, -0.005, 0.06),
                v(0.0, -0.02, 0.13),
                (0.026, 0.004),
                10,
            );
        }
        Part::CockLeg => leg(m, COCK_HIP[1], 1.0),
        Part::CockTail => serpent_tail(m, 0.07, 0.9, COCK_TAIL[1]),
        Part::ChickBody => {
            m.ball(v(0.0, 0.25, 0.0), v(0.2, 0.19, 0.23), [0.0; 3], false, 20);
            for side in [-1.0f32, 1.0] {
                m.ball(
                    v(side * 0.18, 0.27, -0.02),
                    v(0.035, 0.08, 0.12),
                    [-10.0, 0.0, 0.0],
                    false,
                    12,
                );
            }
        }
        Part::ChickHead => {
            m.ball(v(0.0, 0.0, 0.0), Vector3::splat(0.13), [0.0; 3], false, 18);
            // a little beak (the same colour as the head: a chick's is pale)
            m.rod(v(0.0, -0.01, 0.11), v(0.0, -0.02, 0.17), (0.028, 0.004), 10);
        }
        Part::ChickLeg => leg(m, CHICK_HIP[1], 0.6),
        Part::ChickTail => serpent_tail(m, 0.045, 0.55, CHICK_TAIL[1]),
        Part::Shrieker => {
            m.loft(SHRIEKER, Transform3D::IDENTITY);
            // two buds at its foot
            for (x, z, r) in [(0.14f32, 0.06f32, 0.06f32), (-0.1, -0.12, 0.05)] {
                m.rod(v(x, 0.0, z), v(x, r * 1.6, z), (r * 0.5, r * 0.4), 10);
                m.ball(v(x, r * 1.6, z), v(r, r * 0.7, r), [0.0; 3], true, 12);
            }
        }
        Part::ShriekerBell => inside(m, 0.0, 0.72),
        Part::ShriekerThroat => inside(m, 0.72, 1.0),
        Part::Violet => {
            // a cluster of short caps, the tallest in the middle
            for (x, z, h, r) in [
                (0.0f32, 0.0f32, 0.34f32, 0.2f32),
                (0.17, 0.1, 0.22, 0.14),
                (-0.15, 0.12, 0.18, 0.12),
                (0.02, -0.17, 0.2, 0.13),
            ] {
                m.rod(v(x, 0.0, z), v(x, h, z), (r * 0.42, r * 0.3), 10);
                m.ball(v(x, h, z), v(r, r * 0.6, r), [0.0; 3], true, 16);
            }
            // tendrils curling out over the floor
            for k in 0..6 {
                let a = frac(k, 6) * TAU + 0.3;
                let (dir, side) = (v(a.cos(), 0.0, a.sin()), v(-a.sin(), 0.0, a.cos()));
                let path: Vec<Vector3> = (0..=12)
                    .map(|j| {
                        let t = frac(j, 12);
                        dir * (0.12 + 0.36 * t)
                            + side * (0.08 * (t * PI * 1.5).sin())
                            + v(0.0, 0.03 + 0.06 * (1.0 - t) * t, 0.0)
                    })
                    .collect();
                m.sweep(
                    &path,
                    |j| (0.022 - 0.014 * frac(j, 12), 0.022 - 0.014 * frac(j, 12)),
                    2.0,
                    false,
                    UP,
                );
            }
        }
        Part::VioletTips => {
            for k in 0..6 {
                let a = frac(k, 6) * TAU + 0.3;
                let (dir, side) = (v(a.cos(), 0.0, a.sin()), v(-a.sin(), 0.0, a.cos()));
                let end = dir * 0.48 + side * (0.08 * (PI * 1.5).sin()) + v(0.0, 0.03, 0.0);
                m.ball(end, Vector3::splat(0.022), [0.0; 3], false, 10);
            }
        }
        Part::Cube => {
            // a cube of jelly, its edges soft
            let path = [v(0.0, 0.46, -0.44), v(0.0, 0.46, 0.44)];
            m.sweep(&path, |_| (0.44, 0.44), 7.0, false, UP);
        }
        Part::CubeBones => {
            // a skull, a thighbone and a rib or two, hanging in it
            m.ball(
                v(0.1, 0.5, 0.05),
                v(0.11, 0.1, 0.12),
                [0.0, 30.0, 0.0],
                false,
                14,
            );
            m.ball(
                v(0.13, 0.43, 0.11),
                v(0.07, 0.035, 0.06),
                [0.0, 30.0, 0.0],
                false,
                10,
            );
            let (a, b) = (v(-0.28, 0.24, -0.15), v(0.0, 0.2, -0.28));
            m.rod(a, b, (0.022, 0.022), 8);
            m.ball(a, Vector3::splat(0.04), [0.0; 3], false, 10);
            m.ball(b, Vector3::splat(0.04), [0.0; 3], false, 10);
            for k in 0..3 {
                let c = v(-0.18, 0.62 - 0.07 * k as f32, 0.15);
                let rib: Vec<Vector3> = (0..=8)
                    .map(|j| {
                        let a = frac(j, 8) * PI;
                        c + v(0.12 * a.cos(), 0.0, 0.06 * a.sin())
                    })
                    .collect();
                m.sweep(&rib, |_| (0.012, 0.012), 2.0, false, UP);
            }
        }
        Part::CubeLoot => {
            // a sword and a few coins
            m.sweep(
                &[v(-0.3, 0.7, -0.25), v(0.25, 0.55, 0.28)],
                |_| (0.03, 0.006),
                6.0,
                false,
                UP,
            );
            m.rod(
                v(-0.33, 0.71, -0.28),
                v(-0.27, 0.69, -0.22),
                (0.02, 0.02),
                8,
            );
            for (x, y, z) in [
                (0.25f32, 0.2f32, -0.2f32),
                (0.3, 0.3, 0.1),
                (-0.05, 0.12, 0.25),
            ] {
                m.rod(v(x, y, z), v(x, y + 0.012, z), (0.045, 0.045), 14);
            }
        }
    }
}

/// a shrieker's stalk, flaring up into an open funnel (its mouth left
/// open: the throat inside is `throat`)
const SHRIEKER: &[Section] = &[
    s(0.0, 0.0, 0.08, 0.08),
    s(0.3, 0.0, 0.06, 0.06),
    s(0.45, 0.0, 0.07, 0.07),
    s(0.56, 0.0, 0.12, 0.12),
    s(0.66, 0.0, 0.2, 0.2),
    s(0.74, 0.0, 0.3, 0.3),
    s(0.78, 0.0, 0.34, 0.34),
];

/// The inside of a shrieker's funnel, a cone from just within its rim
/// down to the throat (`from` and `to` of the way down), facing up and
/// in towards whoever looks down into it.
fn inside(m: &mut Merge, from: f32, to: f32) {
    let (rows, sides) = (5, 24);
    m.rings(rows, sides, |i, j| {
        let t = from + (to - from) * i as f32 / (rows - 1) as f32;
        let a = TAU * j as f32 / sides as f32;
        let (r, y) = (0.325 * (1.0 - t), 0.775 - 0.24 * t);
        let out = v(a.cos(), 0.0, a.sin());
        // in by 0.325 as it falls 0.24: the normal across that slope
        (
            out * r + v(0.0, y, 0.0),
            (v(0.0, 0.325, 0.0) - out * 0.24).normalized(),
        )
    });
}

/// A bird's leg from its hip `high` up, `k` its size: a feathered thigh,
/// a bare shank to the floor, a spur behind, three toes forward and one
/// back.
fn leg(m: &mut Merge, high: f32, k: f32) {
    m.ball(
        v(0.0, -0.02 * k, 0.0),
        v(0.06, 0.08, 0.07) * k,
        [0.0; 3],
        false,
        12,
    );
    let ankle = v(0.0, 0.025 * k - high, 0.02 * k);
    m.rod(v(0.0, -0.08 * k, 0.0), ankle, (0.022 * k, 0.018 * k), 8);
    m.rod(
        ankle + v(0.0, 0.08 * k, -0.01 * k),
        ankle + v(0.0, 0.07 * k, -0.06 * k),
        (0.01 * k, 0.002 * k),
        6,
    );
    for t in [-1.0f32, 0.0, 1.0] {
        let tip = ankle + v(0.045 * t * k, -0.018 * k, 0.11 * k);
        m.rod(ankle, tip, (0.014 * k, 0.006 * k), 6);
    }
    m.rod(
        ankle,
        ankle + v(0.0, -0.018 * k, -0.06 * k),
        (0.012 * k, 0.005 * k),
        6,
    );
}

/// A serpent's tail from a rump `high` up: down to lie on the floor and
/// away in an S, `r` thick at its root and `len` long, its end a point.
fn serpent_tail(m: &mut Merge, r: f32, len: f32, high: f32) {
    let n = 24;
    let thick = |i: usize| r * (1.0 - 0.9 * frac(i, n).powf(1.4));
    let path: Vec<Vector3> = (0..=n)
        .map(|i| {
            let t = frac(i, n);
            let down = (t * 3.0).min(1.0);
            v(
                0.18 * len * (t * PI * 1.6).sin() * t,
                (thick(i) - high) * down * (2.0 - down),
                -len * 0.75 * t,
            )
        })
        .collect();
    m.sweep(&path, |i| (thick(i), thick(i) * 0.85), 2.0, false, UP);
}
