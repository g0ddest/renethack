//! The shapes of the objects that borrowed another's model (a rope coil
//! for a towel, a chain for a beartrap, a plate for a pie, a carrot for a
//! banana), built in code like the creatures; `prop_kit` gives each part
//! its material. An object lies or stands about one unit across, on
//! y = 0. A part is what one material covers. Besides them, the pieces a
//! creature wears on its bones: a serpent's tail, an arm more, bat's
//! wings, a ghost's trail of mist.

use super::*;

/// The parts there are, by object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A blindfold: its band dropped in a loop, knotted, the ends trailing.
    Blindfold,
    /// A towel folded over once; the two stripes across it.
    Towel,
    TowelStripes,
    /// A leash coiled on the floor with its hand loop; its snap hook.
    Leash,
    LeashClasp,
    /// A grappling hook's shank, flukes and eye; the rope tied to it.
    Hook,
    HookRope,
    /// A land mine's body; its pressure plate, prongs and bolts.
    Mine,
    MinePlate,
    /// A beartrap set open: its jaws and teeth, base, springs and pan.
    Beartrap,
    /// A harp's frame; its strings.
    Harp,
    HarpStrings,
    /// A drum's shell and hoops; its heads; its cords and two sticks.
    Drum,
    DrumHeads,
    DrumCords,
    /// A cream pie's dish, crust, cream and cherry.
    PieDish,
    PieCrust,
    PieCream,
    PieCherry,
    /// A candy bar's wrapper; its band; the bar where it is torn open.
    CandyWrapper,
    CandyBand,
    CandyBar,
    /// A fortune cookie's two lobes and fold; the slip of paper in it.
    Cookie,
    CookieSlip,
    /// A stack of three pancakes; butter; syrup over them.
    Pancakes,
    Butter,
    Syrup,
    /// Two lembas wafers; the leaf they are wrapped in.
    Lembas,
    LembasLeaf,
    /// A lump of royal jelly.
    Jelly,
    /// A banana; its stem and its tip.
    Banana,
    BananaEnds,
    /// A kelp frond with its float and holdfast.
    Kelp,
    /// A eucalyptus leaf with its stem and midrib.
    Leaf,
    /// What a mimic shows itself as: a strange, bulging, misshapen box.
    Strange,
    /// A serpent's tail worn under a body: from its waist (the origin, a
    /// unit above the floor) down and coiled out round it on the floor;
    /// the plates down the front of it.
    SerpentTail,
    SerpentBelly,
    /// A naga's: the serpent's body rising from the waist (the origin) to
    /// where a head sits on it; the plates up its front.
    SerpentNeck,
    SerpentNeckBelly,
    /// An arm worn on a body's side: out from its shoulder (the origin),
    /// bent at the elbow, the forearm raised to a fist; the curved blade
    /// in the fist.
    Arm,
    ArmBlade,
    /// A bat's wings raised from the shoulder blades (the origin): the
    /// skin stretched between the fingers; the arm and finger bones along
    /// its edges.
    BatWings,
    BatWingBones,
    /// A trail of mist worn under a body that floats: from its waist (the
    /// origin) down, narrowing, and flicked back behind it short of the
    /// floor.
    Wisp,
}

const UP: Vector3 = Vector3::UP;
const ACROSS: Vector3 = Vector3::RIGHT;

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

/// `n` steps from `a` to `b`, both ends in.
fn line(a: Vector3, b: Vector3, n: usize) -> Vec<Vector3> {
    (0..=n).map(|k| a.lerp(b, k as f32 / n as f32)).collect()
}

/// A quadratic curve from `a` towards `c` to `b`, `n` steps.
fn curve(a: Vector3, c: Vector3, b: Vector3, n: usize) -> Vec<Vector3> {
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            a * (1.0 - t) * (1.0 - t) + c * 2.0 * (1.0 - t) * t + b * t * t
        })
        .collect()
}

/// A circle flat on the floor (x, z) round `c`, `n` points (a closed path).
fn ring(c: Vector3, r: f32, n: usize) -> Vec<Vector3> {
    (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * TAU;
            c + v(r * a.cos(), 0.0, r * a.sin())
        })
        .collect()
}

/// A swell from nothing at either end to 1 in the middle of `t` in 0..1,
/// rounder as `p` falls (never below zero, so never NaN at the ends).
fn bulge(t: f32, p: f32) -> f32 {
    (PI * t).sin().max(0.0).powf(p)
}

/// How far along `0..n` the point `i` is.
fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::Blindfold => blindfold(m),
        Part::Towel => {
            // flat along, a half turn up and back over itself
            let mut path = line(v(0.0, 0.03, -0.36), v(0.0, 0.03, 0.27), 6);
            for k in 1..8 {
                let a = frac(k, 8) * PI;
                path.push(v(0.0, 0.07 - 0.04 * a.cos(), 0.27 + 0.04 * a.sin()));
            }
            path.extend(line(v(0.0, 0.11, 0.27), v(0.0, 0.11, -0.22), 5));
            m.sweep(&path, |_| (0.42, 0.024), 6.0, false, UP);
        }
        Part::TowelStripes => {
            for z in [-0.13f32, -0.06] {
                let across = [v(-0.425, 0.137, z), v(0.425, 0.137, z)];
                m.sweep(&across, |_| (0.022, 0.004), 4.0, false, UP);
            }
        }
        Part::Leash => {
            let coil = leash_coil();
            m.sweep(&coil, |_| (0.032, 0.008), 4.0, false, UP);
            // the hand loop at its outer end
            let start = coil[0];
            let lp: Vec<Vector3> = (0..20)
                .map(|k| {
                    let a = frac(k, 20) * TAU;
                    start + v(0.11 + 0.11 * a.cos(), 0.0, 0.065 * a.sin())
                })
                .collect();
            m.sweep(&lp, |_| (0.028, 0.008), 4.0, true, UP);
        }
        Part::LeashClasp => {
            // a snap hook at the inner end: a barrel, then a hook back
            let coil = leash_coil();
            let (end, before) = (coil[coil.len() - 1], coil[coil.len() - 2]);
            let ahead = (end - before).normalized();
            let tip = end + ahead * 0.07;
            m.rod(end, tip, (0.014, 0.014), 10);
            let side = ahead.cross(UP);
            let hook = curve(tip, tip + ahead * 0.06 + side * 0.03, tip + side * 0.05, 8);
            m.sweep(&hook, |_| (0.008, 0.008), 2.0, false, UP);
        }
        Part::Hook => hook(m),
        Part::HookRope => {
            // the rope from the eye, coiled behind it
            let eye = v(0.0, 0.03, -0.12);
            let mut rope = curve(eye, eye + v(0.05, 0.0, -0.08), v(0.12, 0.03, -0.28), 6);
            for k in 1..=40 {
                let t = frac(k, 40);
                let a = t * 2.2 * TAU;
                let r = 0.16 - 0.05 * t;
                rope.push(v(r * a.cos(), 0.03 + 0.025 * t, -0.28 + r * a.sin()));
            }
            m.sweep(&rope, |_| (0.022, 0.022), 2.0, false, UP);
        }
        Part::Mine => {
            m.rod(v(0.0, 0.0, 0.0), v(0.0, 0.15, 0.0), (0.47, 0.44), 28);
            m.ball(v(0.0, 0.15, 0.0), v(0.44, 0.07, 0.44), [0.0; 3], true, 28);
        }
        Part::MinePlate => {
            m.rod(v(0.0, 0.2, 0.0), v(0.0, 0.25, 0.0), (0.14, 0.13), 20);
            for k in 0..3 {
                let a = frac(k, 3) * TAU + 0.5;
                let at = v(0.07 * a.cos(), 0.25, 0.07 * a.sin());
                m.rod(at, at + v(0.0, 0.07, 0.0), (0.012, 0.005), 6);
            }
            for k in 0..10 {
                let a = frac(k, 10) * TAU;
                let at = v(0.39 * a.cos(), 0.17, 0.39 * a.sin());
                m.ball(at, Vector3::splat(0.022), [0.0; 3], true, 8);
            }
        }
        Part::Beartrap => beartrap(m),
        Part::Harp => harp(m),
        Part::HarpStrings => {
            let (box_line, neck) = harp_lines();
            for k in 1..9 {
                let t = frac(k, 9) * 0.9;
                let foot = box_line[0].lerp(box_line[1], 1.0 - t) + v(0.0, 0.0, 0.03);
                let head = neck[((neck.len() - 1) as f32 * t) as usize] + v(0.0, -0.03, 0.0);
                m.rod(foot, head, (0.004, 0.004), 5);
            }
        }
        Part::Drum => {
            m.rod(v(0.0, 0.02, 0.0), v(0.0, 0.46, 0.0), (0.36, 0.36), 28);
            for y in [0.04f32, 0.44] {
                m.sweep(
                    &ring(v(0.0, y, 0.0), 0.375, 32),
                    |_| (0.022, 0.032),
                    3.0,
                    true,
                    UP,
                );
            }
        }
        Part::DrumHeads => {
            m.ball(
                v(0.0, 0.46, 0.0),
                v(0.355, 0.014, 0.355),
                [0.0; 3],
                true,
                28,
            );
        }
        Part::DrumCords => {
            // zigzag cords from hoop to hoop
            for k in 0..12 {
                let a = frac(k, 12) * TAU;
                let b = a + PI / 12.0;
                let top = v(0.39 * a.cos(), 0.43, 0.39 * a.sin());
                let low = v(0.39 * b.cos(), 0.06, 0.39 * b.sin());
                let next = v(
                    0.39 * (a + PI / 6.0).cos(),
                    0.43,
                    0.39 * (a + PI / 6.0).sin(),
                );
                m.rod(top, low, (0.008, 0.008), 5);
                m.rod(low, next, (0.008, 0.008), 5);
            }
            // two sticks lying beside it
            for (x, z) in [(0.5f32, -0.05f32), (0.55, 0.06)] {
                let (a, b) = (v(x, 0.018, z - 0.32), v(x - 0.03, 0.018, z + 0.3));
                m.rod(a, b, (0.016, 0.012), 8);
                m.ball(b, Vector3::splat(0.03), [0.0; 3], false, 10);
            }
        }
        Part::PieDish => {
            m.rod(v(0.0, 0.0, 0.0), v(0.0, 0.07, 0.0), (0.4, 0.48), 32);
        }
        Part::PieCrust => {
            m.sweep(
                &ring(v(0.0, 0.085, 0.0), 0.44, 36),
                |_| (0.065, 0.045),
                2.0,
                true,
                UP,
            );
        }
        Part::PieCream => {
            m.ball(v(0.0, 0.08, 0.0), v(0.41, 0.13, 0.41), [0.0; 3], true, 28);
            for k in 0..8 {
                let a = frac(k, 8) * TAU;
                let at = v(0.29 * a.cos(), 0.16, 0.29 * a.sin());
                m.ball(at, v(0.075, 0.06, 0.075), [0.0; 3], false, 12);
                m.ball(
                    at + v(0.0, 0.05, 0.0),
                    v(0.03, 0.04, 0.03),
                    [0.0; 3],
                    false,
                    8,
                );
            }
            m.ball(v(0.0, 0.22, 0.0), v(0.1, 0.07, 0.1), [0.0; 3], false, 14);
        }
        Part::PieCherry => {
            m.ball(v(0.0, 0.32, 0.0), Vector3::splat(0.05), [0.0; 3], false, 12);
            let stem = curve(
                v(0.0, 0.36, 0.0),
                v(0.01, 0.42, 0.0),
                v(0.05, 0.44, 0.02),
                5,
            );
            m.sweep(&stem, |_| (0.006, 0.006), 2.0, false, ACROSS);
        }
        Part::CandyWrapper => {
            m.sweep(
                &line(v(-0.47, 0.065, 0.0), v(0.17, 0.065, 0.0), 6),
                |_| (0.2, 0.062),
                5.0,
                false,
                UP,
            );
            // the crimped end, flattened
            m.sweep(
                &line(v(-0.57, 0.065, 0.0), v(-0.47, 0.065, 0.0), 2),
                |_| (0.19, 0.012),
                6.0,
                false,
                UP,
            );
        }
        Part::CandyBand => {
            m.sweep(
                &line(v(-0.24, 0.065, 0.0), v(-0.02, 0.065, 0.0), 3),
                |_| (0.205, 0.067),
                5.0,
                false,
                UP,
            );
        }
        Part::CandyBar => {
            m.sweep(
                &line(v(0.15, 0.06, 0.0), v(0.47, 0.06, 0.0), 4),
                |_| (0.18, 0.052),
                7.0,
                false,
                UP,
            );
            // the grooves between its squares
            for x in [0.26f32, 0.37] {
                m.sweep(
                    &[v(x, 0.113, -0.17), v(x, 0.113, 0.17)],
                    |_| (0.012, 0.006),
                    2.0,
                    false,
                    UP,
                );
            }
        }
        Part::Cookie => {
            for side in [-1.0f32, 1.0] {
                m.ball(
                    v(side * 0.2, 0.26, 0.0),
                    v(0.24, 0.23, 0.46),
                    [0.0, side * 12.0, side * -28.0],
                    false,
                    18,
                );
            }
            // the fold where the lobes meet, underneath
            m.ball(v(0.0, 0.12, 0.0), v(0.18, 0.12, 0.4), [0.0; 3], false, 14);
        }
        Part::CookieSlip => {
            // a slip of paper poking out of the fold
            let slip = [v(0.0, 0.36, -0.1), v(0.0, 0.47, -0.36), v(0.03, 0.5, -0.52)];
            m.sweep(&slip, |_| (0.1, 0.008), 6.0, false, ACROSS);
        }
        Part::Pancakes => {
            for k in 0..3 {
                let at = v(0.012 * k as f32, 0.05 + 0.088 * k as f32, -0.01 * k as f32);
                let half = v(0.45 - 0.012 * k as f32, 0.045, 0.44);
                m.ball(at, half, [0.0, 20.0 * k as f32, 0.0], false, 24);
            }
        }
        Part::Butter => {
            m.sweep(
                &line(v(-0.04, 0.3, 0.02), v(0.12, 0.3, 0.02), 2),
                |_| (0.075, 0.028),
                7.0,
                false,
                UP,
            );
        }
        Part::Syrup => {
            m.ball(
                v(0.02, 0.272, -0.01),
                v(0.34, 0.022, 0.32),
                [0.0; 3],
                true,
                24,
            );
            for k in 0..4 {
                let a = frac(k, 4) * TAU + 0.6;
                let top = v(0.36 * a.cos(), 0.27, 0.35 * a.sin());
                let low = v(0.45 * a.cos(), 0.12 + 0.04 * k as f32, 0.44 * a.sin());
                m.rod(top, low, (0.022, 0.014), 8);
                m.ball(low, Vector3::splat(0.02), [0.0; 3], false, 8);
            }
        }
        Part::Lembas => {
            for (k, turn) in [(0usize, 0.0f32), (1, 0.35)] {
                let y = 0.03 + 0.058 * k as f32;
                let (s, c) = turn.sin_cos();
                let path = [v(-0.3 * s, y, -0.3 * c), v(0.3 * s, y, 0.3 * c)];
                m.sweep(&line(path[0], path[1], 4), |_| (0.3, 0.028), 5.0, false, UP);
            }
        }
        Part::LembasLeaf => {
            // a mallorn leaf folded over the stack from one side
            let path = curve(
                v(-0.42, 0.012, 0.08),
                v(-0.38, 0.16, 0.08),
                v(0.32, 0.125, 0.06),
                12,
            );
            m.sweep(
                &path,
                |i| (0.04 + 0.2 * bulge(frac(i, 12), 0.6), 0.006),
                2.6,
                false,
                UP,
            );
        }
        Part::Jelly => {
            m.ball(v(0.0, 0.17, 0.0), v(0.38, 0.17, 0.34), [0.0; 3], false, 20);
            m.ball(v(0.22, 0.12, 0.16), v(0.2, 0.12, 0.18), [0.0; 3], false, 14);
            m.ball(v(-0.2, 0.1, -0.14), v(0.22, 0.1, 0.2), [0.0; 3], false, 14);
        }
        Part::Banana => {
            let path = banana_path();
            let n = path.len() - 1;
            m.sweep(&path, |i| banana_r(i, n), 2.5, false, UP);
        }
        Part::BananaEnds => {
            let path = banana_path();
            let n = path.len() - 1;
            // the stem carried on level from the thin end, a little up
            let (a, b) = (path[0], path[1]);
            let back = v(a.x - b.x, 0.0, a.z - b.z).normalized();
            m.rod(a, a + back * 0.09 + v(0.0, 0.012, 0.0), (0.02, 0.016), 8);
            let tip = path[n];
            m.ball(
                tip,
                Vector3::splat(banana_r(n, n).0 * 1.1),
                [0.0; 3],
                false,
                10,
            );
        }
        Part::Kelp => {
            let path: Vec<Vector3> = (0..=24)
                .map(|k| {
                    let t = frac(k, 24);
                    v(
                        -0.46 + 0.94 * t,
                        0.008 + 0.006 * (t * 40.0).sin().abs(),
                        0.1 * (t * PI * 3.0).sin(),
                    )
                })
                .collect();
            m.sweep(
                &path,
                |i| (0.03 + 0.06 * (PI * frac(i, 24)).sin(), 0.005),
                3.0,
                false,
                UP,
            );
            // its float at the base, the holdfast's roots
            let base = path[0];
            m.ball(
                base + v(0.02, 0.04, 0.0),
                Vector3::splat(0.05),
                [0.0; 3],
                false,
                12,
            );
            for k in 0..4 {
                let to = base + v(-0.09, 0.0, (k as f32 - 1.5) * 0.045);
                m.rod(base, to, (0.01, 0.004), 6);
            }
        }
        Part::Leaf => {
            let path: Vec<Vector3> = (0..=16)
                .map(|k| {
                    let t = frac(k, 16);
                    v(
                        -0.46 + 0.92 * t,
                        0.012 + 0.02 * (PI * t).sin(),
                        0.1 * (PI * t).sin() - 0.06 * t,
                    )
                })
                .collect();
            let width = |i: usize| 0.004 + 0.12 * bulge(frac(i, 16), 0.8);
            m.sweep(&path, |i| (width(i), 0.006), 2.6, false, UP);
            // the midrib, a little raised, and the stem
            let rib: Vec<Vector3> = path.iter().map(|p| *p + v(0.0, 0.006, 0.0)).collect();
            m.sweep(&rib, |_| (0.008, 0.004), 2.0, false, UP);
            m.rod(path[0], path[0] + v(-0.1, 0.0, -0.025), (0.009, 0.007), 6);
        }
        Part::Strange => {
            m.sweep(
                &line(v(0.0, 0.24, -0.32), v(0.0, 0.24, 0.32), 6),
                |_| (0.34, 0.22),
                4.0,
                false,
                UP,
            );
            for (x, y, z, r) in [
                (0.3f32, 0.3f32, 0.1f32, 0.12f32),
                (-0.32, 0.18, -0.15, 0.1),
                (0.05, 0.44, 0.2, 0.09),
                (-0.1, 0.43, -0.25, 0.08),
                (0.2, 0.1, 0.33, 0.1),
                (-0.18, 0.28, 0.32, 0.07),
            ] {
                m.ball(v(x, y, z), Vector3::splat(r), [0.0; 3], false, 14);
            }
        }
        Part::SerpentTail => {
            let (path, r) = serpent_tail();
            // a little wide and low where it lies
            m.sweep(&path, |i| (r[i] * 1.08, r[i] * 0.92), 2.0, false, AHEAD);
        }
        Part::SerpentBelly => {
            // down to the floor
            plates(m, 12, (0.1, 0.82), |t| (tail_drop(t), tail_thick(t)));
        }
        Part::SerpentNeck => {
            let n = 10;
            let path: Vec<Vector3> = (0..=n).map(|k| neck_rise(frac(k, n))).collect();
            m.sweep(
                &path,
                |i| {
                    let r = neck_thick(frac(i, n));
                    (r * 1.08, r * 0.92)
                },
                2.0,
                false,
                AHEAD,
            );
        }
        Part::SerpentNeckBelly => {
            plates(m, 6, (0.08, 0.8), |t| (neck_rise(t), neck_thick(t)));
        }
        Part::Arm => {
            let path = arm_path();
            let n = path.len() - 1;
            // thick at the shoulder, thinner to the elbow, the forearm's
            // swell, the wrist
            let r = |i: usize| {
                let t = frac(i, n);
                0.062 - 0.026 * t + 0.008 * bulge(((t - 0.45) * 2.5).clamp(0.0, 1.0), 1.0)
            };
            m.sweep(&path, |i| (r(i), r(i) * 0.9), 2.0, false, ACROSS);
            let wrist = path[n];
            let ahead = (wrist - path[n - 1]).normalized();
            m.ball(
                wrist + ahead * 0.03,
                v(0.042, 0.048, 0.052),
                [0.0; 3],
                false,
                12,
            );
        }
        Part::ArmBlade => {
            let path = arm_path();
            let fist = path[path.len() - 1] + v(0.0, 0.0, 0.03);
            // the grip through the fist, the guard over it, the blade up
            // and curving forward, edge on
            m.rod(
                fist - v(0.0, 0.06, 0.0),
                fist + v(0.0, 0.05, 0.0),
                (0.014, 0.014),
                8,
            );
            m.ball(
                fist + v(0.0, 0.06, 0.0),
                v(0.024, 0.012, 0.05),
                [0.0; 3],
                false,
                10,
            );
            let blade = curve(
                fist + v(0.0, 0.065, 0.0),
                fist + v(0.0, 0.32, 0.03),
                fist + v(0.0, 0.46, 0.2),
                12,
            );
            m.sweep(
                &blade,
                |i| (0.028 - 0.025 * frac(i, 12).powf(1.6), 0.004),
                2.0,
                false,
                ACROSS,
            );
        }
        Part::BatWings => {
            for side in [-1.0f32, 1.0] {
                bat_wing(m, side);
            }
        }
        Part::Wisp => {
            let n = 24;
            let (a, b, c, d) = (
                v(0.0, 0.05, 0.0),
                v(0.0, -0.5, 0.06),
                v(0.0, -0.78, -0.3),
                v(0.06, -0.66, -0.62),
            );
            let path: Vec<Vector3> = (0..=n)
                .map(|k| {
                    let t = frac(k, n);
                    let u = 1.0 - t;
                    a * u * u * u + b * 3.0 * u * u * t + c * 3.0 * u * t * t + d * t * t * t
                })
                .collect();
            // a robe's hem at the hips, thinning to nothing
            let r = |i: usize| 0.004 + 0.17 * (1.0 - frac(i, n)).powf(1.3);
            m.sweep(&path, |i| (r(i) * 1.1, r(i) * 0.9), 2.0, false, AHEAD);
        }
        Part::BatWingBones => {
            for side in [-1.0f32, 1.0] {
                let w = BatWing::of(side);
                m.rod(w.root, w.elbow, (0.03, 0.024), 8);
                m.rod(w.elbow, w.wrist, (0.024, 0.017), 8);
                m.ball(w.elbow, Vector3::splat(0.026), [0.0; 3], false, 8);
                m.ball(w.wrist, Vector3::splat(0.021), [0.0; 3], false, 8);
                for tip in w.fingers {
                    m.rod(w.wrist, tip, (0.013, 0.004), 6);
                }
                // the claw of its thumb
                let thumb = w.wrist + v(-side * 0.02, 0.08, 0.02);
                m.rod(w.wrist, thumb, (0.013, 0.002), 6);
            }
        }
    }
}

/// The way a body faces.
const AHEAD: Vector3 = Vector3::BACK;

/// How thick a serpent's tail is at the waist, and where it reaches the
/// floor.
const TAIL_TOP: f32 = 0.15;
const TAIL_LOW: f32 = 0.13;

/// Where a serpent's tail is `t` of the way down from the waist (0) to the
/// floor (1): down, bending back and round to run sideways along the floor
/// where its coil begins (behind).
fn tail_drop(t: f32) -> Vector3 {
    let t = t.clamp(0.0, 1.0);
    let start = v(0.0, -1.0 + TAIL_LOW * 0.92, -0.17);
    let (a, b, c) = (
        v(0.0, 0.06, 0.0),
        v(0.0, -0.5, -0.03),
        start + v(0.14, 0.0, 0.0),
    );
    let u = 1.0 - t;
    a * u * u * u + b * 3.0 * u * u * t + c * 3.0 * u * t * t + start * t * t * t
}

/// Its radius there.
fn tail_thick(t: f32) -> f32 {
    TAIL_TOP - (TAIL_TOP - TAIL_LOW) * t.clamp(0.0, 1.0)
}

/// A serpent's tail and its radius at each point: down from the waist,
/// then a loop and a half spiralling out round where it came down, lying
/// on the floor (its centre as high as it is thick) and narrowing to a
/// point.
fn serpent_tail() -> (Vec<Vector3>, Vec<f32>) {
    let mut path: Vec<Vector3> = (0..=10).map(|k| tail_drop(frac(k, 10))).collect();
    let mut r: Vec<f32> = (0..=10).map(|k| tail_thick(frac(k, 10))).collect();
    let n = 40;
    for k in 1..=n {
        let t = frac(k, n);
        let turn = PI + t * 1.45 * TAU;
        let thick = TAIL_LOW - (TAIL_LOW - 0.016) * t.powf(1.2);
        let out = 0.17 + 0.28 * t;
        path.push(v(out * turn.sin(), -1.0 + thick * 0.92, out * turn.cos()));
        r.push(thick);
    }
    (path, r)
}

/// Plates across the front of a serpent's body (`at(t)`: a point of it and
/// its radius there), `n` of them over `span` of its length, each bowed
/// round it, a seam between it and the next.
fn plates(m: &mut Merge, n: usize, span: (f32, f32), at: impl Fn(f32) -> (Vector3, f32)) {
    for k in 0..n {
        let t = span.0 + (span.1 - span.0) * frac(k, n);
        let (mid, thick) = at(t);
        let along = (at(t + 0.01).0 - at(t - 0.01).0).normalized();
        let front = (AHEAD - along * along.dot(AHEAD)).normalized();
        let side = along.cross(front);
        let arc: Vec<Vector3> = (0..=8)
            .map(|j| {
                let phi = (-55.0 + 110.0 * frac(j, 8)).to_radians();
                mid + side * (thick * 1.08 * 1.03 * phi.sin())
                    + front * (thick * 0.92 * 1.03 * phi.cos())
            })
            .collect();
        m.sweep(&arc, |_| (0.007, 0.031), 4.0, false, along);
    }
}

/// Where a naga's body is `t` of the way up from the waist (0) to its
/// head (1): rising back, then forward under the head, as a snake rears.
fn neck_rise(t: f32) -> Vector3 {
    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;
    let (b, c, d) = (v(0.0, 0.2, -0.1), v(0.0, 0.42, -0.06), v(0.0, 0.56, 0.06));
    b * 3.0 * u * u * t + c * 3.0 * u * t * t + d * t * t * t
}

/// Its radius there: the tail's at the waist, a neck's at the top.
fn neck_thick(t: f32) -> f32 {
    TAIL_TOP - (TAIL_TOP - 0.062) * t.clamp(0.0, 1.0).powf(0.9)
}

/// An arm's path from the shoulder: out and down to the elbow, the forearm
/// forward and up to the wrist.
fn arm_path() -> Vec<Vector3> {
    let elbow = v(0.0, -0.2, 0.17);
    let mut path = curve(v(0.0, 0.0, 0.0), v(0.0, -0.12, 0.06), elbow, 6);
    let forearm = curve(elbow, v(0.0, -0.21, 0.3), v(0.0, -0.1, 0.42), 6);
    path.extend(forearm.into_iter().skip(1));
    path
}

/// One of a bat's wings (`side` 1 the right, out along +x): its arm from
/// the root by the elbow to the wrist, the fingers spread from there, and
/// where its skin joins the back low down. Raised high rather than spread
/// wide: a body keeps within its cell.
struct BatWing {
    root: Vector3,
    elbow: Vector3,
    wrist: Vector3,
    fingers: [Vector3; 4],
    low: Vector3,
}

impl BatWing {
    fn of(side: f32) -> Self {
        let p = |x: f32, y: f32, z: f32| v(side * x, y, z);
        BatWing {
            root: p(0.05, 0.03, 0.0),
            elbow: p(0.16, 0.22, -0.07),
            wrist: p(0.21, 0.56, -0.12),
            fingers: [
                p(0.22, 1.0, -0.17),
                p(0.36, 0.88, -0.22),
                p(0.44, 0.62, -0.24),
                p(0.4, 0.28, -0.2),
            ],
            low: p(0.07, -0.26, -0.01),
        }
    }
}

/// A bat's wing's skin: a panel fanned from the wrist between each finger
/// and the next, its edge cut in a scallop between their tips and bellied
/// back a little; then from the wrist and the elbow to the back. Both
/// faces, so it shows from behind.
fn bat_wing(m: &mut Merge, side: f32) {
    let w = BatWing::of(side);
    let [f1, f2, f3, f4] = w.fingers;
    let fans = [
        (w.wrist, f1, f2, 0.22),
        (w.wrist, f2, f3, 0.22),
        (w.wrist, f3, f4, 0.2),
        (w.wrist, f4, w.low, 0.14),
        (w.wrist, w.low, w.elbow, 0.0),
        (w.elbow, w.low, w.root, 0.0),
    ];
    for (apex, a, b, cut) in fans {
        let mid = (a + b) * 0.5;
        let at = |s: f32, t: f32| {
            let edge = a.lerp(b, t) - (mid - apex) * (cut * (PI * t).sin());
            let p = apex + (edge - apex) * s;
            p - AHEAD * (0.05 * (PI * s).sin() * (PI * t).sin())
        };
        for face in [1.0f32, -1.0] {
            fan_sheet(m, 6, 8, &at, face);
        }
    }
}

/// A sheet of `rows` by `cols` quads, `at(s, t)` its points (each of `s`
/// and `t` in 0..1, the row `s = 0` maybe a single point), its normals
/// turned the way `face` says.
fn fan_sheet(
    m: &mut Merge,
    rows: usize,
    cols: usize,
    at: &impl Fn(f32, f32) -> Vector3,
    face: f32,
) {
    let base = m.pos.len() as i32;
    let e = 1e-3;
    for i in 0..=rows {
        for j in 0..=cols {
            let (s, t) = (frac(i, rows), frac(j, cols));
            // measured a little way out from a point of a row
            let s1 = s.max(0.05);
            let ds = at((s1 + e).min(1.0), t) - at(s1 - e, t);
            let dt = at(s1, (t + e).min(1.0)) - at(s1, (t - e).max(0.0));
            let n = ds.cross(dt).normalized() * face;
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

fn blindfold(m: &mut Merge) {
    // the band, a loop dropped flat on the floor, a little rumpled
    let band: Vec<Vector3> = (0..40)
        .map(|k| {
            let a = frac(k, 40) * TAU;
            let r = 0.3 + 0.02 * (3.0 * a).sin();
            v(
                r * 1.15 * a.cos(),
                0.014 + 0.008 * (2.0 * a + 1.0).sin().max(0.0),
                r * a.sin(),
            )
        })
        .collect();
    m.sweep(&band, |_| (0.065, 0.01), 2.6, true, UP);
    // the knot at the back, its two ends trailing
    m.ball(
        v(-0.36, 0.03, 0.0),
        v(0.05, 0.035, 0.06),
        [0.0; 3],
        false,
        12,
    );
    for side in [-1.0f32, 1.0] {
        let tail: Vec<Vector3> = (0..8)
            .map(|k| {
                let t = frac(k, 7);
                v(
                    -0.38 - 0.13 * t,
                    0.012,
                    side * (0.03 + 0.15 * t + 0.03 * (t * 3.0).sin()),
                )
            })
            .collect();
        m.sweep(
            &tail,
            |i| (0.045 - 0.012 * frac(i, 7), 0.008),
            2.6,
            false,
            UP,
        );
    }
}

/// A leash's strap, coiled loose from its outer end inwards.
fn leash_coil() -> Vec<Vector3> {
    (0..=60)
        .map(|k| {
            let t = frac(k, 60);
            let a = t * 2.6 * TAU;
            let r = 0.36 - 0.17 * t;
            v(r * a.cos(), 0.012 + 0.012 * t, r * a.sin())
        })
        .collect()
}

fn hook(m: &mut Merge) {
    // lying on two of its flukes: the crown up off the floor, the shank
    // down to its eye
    let crown = v(0.0, 0.2, 0.18);
    let eye = v(0.0, 0.035, -0.3);
    m.rod(eye, crown, (0.03, 0.034), 10);
    m.ball(crown, Vector3::splat(0.05), [0.0; 3], false, 12);
    for k in 0..3 {
        let phi = (90.0 + 120.0 * k as f32).to_radians();
        let out = v(phi.cos(), phi.sin(), 0.0);
        let fluke = curve(
            crown,
            crown + out * 0.36 + v(0.0, 0.0, 0.2),
            crown + out * 0.24 - v(0.0, 0.0, 0.1),
            12,
        );
        let r = |i: usize| 0.034 - 0.028 * frac(i, 12);
        m.sweep(&fluke, |i| (r(i), r(i)), 2.0, false, ACROSS);
    }
    m.sweep(
        &ring(eye - v(0.0, 0.0, 0.045), 0.045, 16),
        |_| (0.013, 0.013),
        2.0,
        true,
        UP,
    );
}

fn beartrap(m: &mut Merge) {
    // the base bar, its springs at the ends, the pan in the middle
    m.sweep(
        &line(v(-0.48, 0.016, 0.0), v(0.48, 0.016, 0.0), 4),
        |_| (0.05, 0.016),
        5.0,
        false,
        UP,
    );
    for side in [-1.0f32, 1.0] {
        let spring: Vec<Vector3> = (0..=40)
            .map(|k| {
                let t = frac(k, 40);
                let a = t * 5.0 * TAU;
                v(
                    side * (0.34 + 0.12 * t),
                    0.06 + 0.035 * a.sin(),
                    0.035 * a.cos(),
                )
            })
            .collect();
        m.sweep(&spring, |_| (0.008, 0.008), 2.0, false, ACROSS);
    }
    m.rod(v(0.0, 0.016, 0.0), v(0.0, 0.045, 0.0), (0.09, 0.09), 16);
    // the jaws open flat, a half ring each side of the bar, set with teeth
    // pointing in
    for dir in [-1.0f32, 1.0] {
        let jaw: Vec<Vector3> = (0..=16)
            .map(|k| {
                let a = frac(k, 16) * PI;
                v(0.34 * a.cos(), 0.035, dir * 0.31 * a.sin())
            })
            .collect();
        m.sweep(&jaw, |_| (0.013, 0.03), 4.0, false, UP);
        for k in 1..10 {
            let a = frac(k, 10) * PI;
            let edge = v(0.325 * a.cos(), 0.05, dir * 0.295 * a.sin());
            let inward = v(-a.cos(), 0.0, -dir * a.sin());
            m.rod(
                edge,
                edge + inward * 0.07 + v(0.0, 0.035, 0.0),
                (0.013, 0.001),
                6,
            );
        }
    }
}

/// A harp's soundbox (its foot and top) and its neck's curve from the
/// top of the box over to the pillar.
fn harp_lines() -> ([Vector3; 2], Vec<Vector3>) {
    let (foot, top) = (v(0.0, 0.05, -0.22), v(0.0, 0.93, -0.34));
    let neck = curve(top, v(0.0, 1.03, 0.08), v(0.0, 0.74, 0.32), 14);
    ([foot, top], neck)
}

fn harp(m: &mut Merge) {
    let ([foot, top], neck) = harp_lines();
    // the soundbox up the back, wide at its foot
    m.sweep(
        &line(foot, top, 8),
        |i| (0.075 - 0.035 * frac(i, 8), 0.06 - 0.025 * frac(i, 8)),
        3.0,
        false,
        ACROSS,
    );
    // the neck over the top, the pillar down the front
    m.sweep(&neck, |_| (0.032, 0.045), 3.0, false, ACROSS);
    let pillar = curve(
        neck[neck.len() - 1],
        v(0.0, 0.4, 0.33),
        v(0.0, 0.05, 0.22),
        10,
    );
    m.sweep(
        &pillar,
        |i| (0.03 + 0.008 * frac(i, 10), 0.03 + 0.008 * frac(i, 10)),
        2.0,
        false,
        ACROSS,
    );
    // the base joining them on the floor
    m.sweep(
        &line(v(0.0, 0.03, -0.26), v(0.0, 0.03, 0.26), 4),
        |_| (0.07, 0.03),
        4.0,
        false,
        UP,
    );
}

/// A banana's curve, lying on its side: thick in the middle.
fn banana_path() -> Vec<Vector3> {
    (0..=16)
        .map(|k| {
            let t = frac(k, 16);
            let a = -0.6 + 1.2 * t;
            let (r, _) = banana_r(k, 16);
            v(
                0.82 * a.sin(),
                r * 0.95 + 0.002,
                0.82 * (1.0 - a.cos()) - 0.12,
            )
        })
        .collect()
}

/// A banana's half sizes `i` of `n` along it.
fn banana_r(i: usize, n: usize) -> (f32, f32) {
    let r = 0.02 + 0.07 * bulge(frac(i, n), 0.6);
    (r, r * 0.95)
}
