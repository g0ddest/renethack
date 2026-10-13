//! The creatures of the water (`sea_kit`): a fish (a piranha's deep body
//! with its underbite of teeth, a shark's long one with its tall fin and
//! the long upper lobe of its tail), an eel swimming in an S, in three
//! pieces that bend, a jellyfish's bell and what trails from it, a kraken's
//! mantle and its arms. Each stands on y = 0 and faces +z; in water it
//! floats down to its waterline (the model's `swim` in the manifest).

use super::*;

/// The fish there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fish {
    /// A fish of no kind of its own: a piranha's shape without its teeth.
    Plain,
    Piranha,
    Shark,
}

/// The parts there are. A part is what one material covers, on one pivot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A fish's body, its head and the root of its tail.
    FishBody(Fish),
    /// Its fins on its back, under it and at its sides.
    FishFins(Fish),
    /// Its tail fin (on the Tail pivot, at `Fish::tail`).
    FishTail(Fish),
    /// A piranha's teeth; a shark's gill slits.
    FishMarks(Fish),
    /// An eel's pieces, from its head: the front (on Body), the middle (on
    /// Mid, at `eel_joint(0)`) and the tail (on Tail, at `eel_joint(1)`
    /// from Mid's).
    Eel(Eel),
    /// The fin along its back and under its tail, piece by piece.
    EelFin(Eel),
    /// An electric eel's glowing spots along its flanks, piece by piece.
    EelSpots(Eel),
    /// A jellyfish's bell, its frilled rim; the four rings seen through it;
    /// its four frilled mouth arms; its tentacles.
    JellyBell,
    JellyRings,
    JellyArms,
    JellyStrands,
    /// A kraken's mantle and head, the fins at its tip.
    KrakenBody,
    /// A quarter of its arms (on its own pivot at `KRAKEN_ARMS`), and the
    /// suckers along them.
    KrakenArms(Quarter),
    KrakenSuckers(Quarter),
}

/// An eel's pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Eel {
    Front,
    Middle,
    Tail,
}

/// A kraken's arms by where they reach: front or back, its left or right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quarter {
    FrontLeft,
    FrontRight,
    BackLeft,
    BackRight,
}

impl Quarter {
    pub const ALL: [Quarter; 4] = [
        Quarter::FrontLeft,
        Quarter::FrontRight,
        Quarter::BackLeft,
        Quarter::BackRight,
    ];

    /// The pivot it hangs on (the animations name it).
    pub fn pivot(self) -> &'static str {
        match self {
            Quarter::FrontLeft => "ArmsFL",
            Quarter::FrontRight => "ArmsFR",
            Quarter::BackLeft => "ArmsBL",
            Quarter::BackRight => "ArmsBR",
        }
    }

    /// Its arms' directions (degrees from ahead, to its left), and whether
    /// each is one of the two long tentacles.
    fn arms(self) -> &'static [(f32, bool)] {
        match self {
            Quarter::FrontLeft => &[(14.0, true), (40.0, false), (72.0, false)],
            Quarter::FrontRight => &[(-14.0, true), (-40.0, false), (-72.0, false)],
            Quarter::BackLeft => &[(106.0, false), (140.0, false)],
            Quarter::BackRight => &[(-106.0, false), (-140.0, false)],
        }
    }
}

impl Fish {
    /// Where its tail fin joins it.
    pub fn tail(self) -> [f32; 3] {
        match self {
            Fish::Plain | Fish::Piranha => [0.0, 0.27, -0.4],
            Fish::Shark => [0.0, 0.15, -0.44],
        }
    }

    /// Its eyes: where the right one is (the left mirrors it), and how big.
    pub fn eye(self) -> ([f32; 3], f32) {
        match self {
            Fish::Plain | Fish::Piranha => ([0.078, 0.31, 0.3], 0.03),
            Fish::Shark => ([0.066, 0.2, 0.38], 0.015),
        }
    }

    fn body(self) -> &'static [Section] {
        match self {
            Fish::Plain | Fish::Piranha => PIRANHA_BODY,
            Fish::Shark => SHARK_BODY,
        }
    }
}

/// a deep body flattened from side to side, the blunt head and the jaw
/// jutting under it
const PIRANHA_BODY: &[Section] = &[
    s(0.27, -0.42, 0.012, 0.03),
    s(0.27, -0.38, 0.03, 0.06),
    s(0.27, -0.28, 0.06, 0.13),
    s(0.26, -0.12, 0.09, 0.21),
    s(0.255, 0.04, 0.1, 0.24),
    s(0.25, 0.18, 0.095, 0.22),
    s(0.245, 0.29, 0.08, 0.17),
    s(0.235, 0.37, 0.06, 0.11),
    s(0.225, 0.42, 0.035, 0.06),
    s(0.22, 0.44, 0.01, 0.02),
];

/// a long torpedo, thickest before its middle, the snout a point
const SHARK_BODY: &[Section] = &[
    s(0.15, -0.46, 0.01, 0.015),
    s(0.15, -0.42, 0.025, 0.04),
    s(0.155, -0.3, 0.055, 0.07),
    s(0.16, -0.12, 0.1, 0.115),
    s(0.165, 0.06, 0.12, 0.135),
    s(0.165, 0.22, 0.11, 0.12),
    s(0.16, 0.34, 0.085, 0.085),
    s(0.155, 0.43, 0.05, 0.05),
    s(0.15, 0.49, 0.018, 0.02),
    s(0.148, 0.51, 0.004, 0.004),
];

/// a kraken's head between its arms and the long mantle behind it,
/// rising a little to its tip
const KRAKEN_BODY: &[Section] = &[
    s(0.2, 0.32, 0.04, 0.03),
    s(0.22, 0.28, 0.12, 0.1),
    s(0.25, 0.2, 0.17, 0.14),
    s(0.27, 0.1, 0.15, 0.13),
    s(0.3, 0.0, 0.2, 0.18),
    s(0.34, -0.15, 0.22, 0.2),
    s(0.38, -0.3, 0.19, 0.17),
    s(0.42, -0.42, 0.13, 0.12),
    s(0.45, -0.5, 0.06, 0.06),
    s(0.46, -0.53, 0.01, 0.01),
];

/// Where a kraken's arms join its head (their pivots).
pub const KRAKEN_ARMS: [f32; 3] = [0.0, 0.15, 0.22];
/// Where its eyes are (the right one), and how big.
pub const KRAKEN_EYE: ([f32; 3], f32) = ([0.155, 0.31, 0.19], 0.045);

/// How high an eel swims (its middle), the turns of its S.
const EEL_Y: f32 = 0.085;

/// The point along an eel's middle `t` of the way from its snout (0) to
/// its tail's tip (1): an S, swinging wider towards the tail.
fn eel_at(t: f32) -> Vector3 {
    let swing = (0.05 + 0.15 * t) * (TAU * 0.9 * t + 0.4).sin();
    v(swing, EEL_Y, 0.62 - 1.24 * t)
}

/// Its half sizes there, across and up: the snout narrow, the body
/// thickest before its middle, the tail flattened to an edge.
fn eel_half(t: f32) -> (f32, f32) {
    let head = smooth(0.0, 0.1, t);
    let across = 0.068 * (0.2 + 0.8 * head) * (1.0 - 0.9 * smooth(0.45, 1.0, t));
    let up = 0.078 * (0.15 + 0.85 * head) * (1.0 - 0.85 * smooth(0.5, 1.0, t));
    (across, up)
}

/// The stretch of the way (from its snout) each piece covers: they
/// overlap at the joints, where the next piece's pivot is.
fn eel_span(piece: Eel) -> (f32, f32) {
    match piece {
        Eel::Front => (0.0, 0.41),
        Eel::Middle => (0.37, 0.73),
        Eel::Tail => (0.69, 1.0),
    }
}

/// Where an eel bends: 0 at its middle piece's pivot (from the body's),
/// 1 at its tail's (from the middle's).
pub fn eel_joint(k: usize) -> [f32; 3] {
    let at = match k {
        0 => eel_at(0.39),
        _ => eel_at(0.71) - eel_at(0.39),
    };
    [at.x, at.y, at.z]
}

/// The point a piece's own space starts from (its pivot, in the body's).
fn eel_origin(piece: Eel) -> Vector3 {
    match piece {
        Eel::Front => Vector3::ZERO,
        Eel::Middle => eel_at(0.39),
        Eel::Tail => eel_at(0.71),
    }
}

/// Where an eel's eyes are (on either side of its head, which its S
/// turns a little), and how big.
pub fn eel_eyes() -> ([[f32; 3]; 2], f32) {
    let t = 0.05;
    let (across, up) = eel_half(t);
    let ahead = (eel_at(t - 0.01) - eel_at(t + 0.01)).normalized();
    let side = ahead.cross(Vector3::UP).normalized();
    let at = |s: f32| {
        let p = eel_at(t) + side * (s * across * 0.75) + v(0.0, up * 0.55, 0.0);
        [p.x, p.y, p.z]
    };
    ([at(-1.0), at(1.0)], 0.013)
}

fn smooth(a: f32, b: f32, t: f32) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

/// Across (y up, z ahead) in the creature's plane of symmetry.
const SIDEWAYS: (Vector3, Vector3) = (Vector3::BACK, Vector3::UP);

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::FishBody(fish) => {
            m.loft(fish.body(), Transform3D::IDENTITY);
            if fish == Fish::Piranha {
                // the lower jaw jutting under the head
                m.ball(
                    v(0.0, 0.15, 0.37),
                    v(0.06, 0.035, 0.08),
                    [-8.0, 0.0, 0.0],
                    false,
                    14,
                );
            }
        }
        Part::FishFins(fish) => fins(m, fish),
        Part::FishTail(fish) => {
            let tail: &[(f32, f32)] = match fish {
                // forked, its two lobes alike
                Fish::Plain | Fish::Piranha => &[
                    (0.03, 0.03),
                    (-0.14, 0.18),
                    (-0.21, 0.17),
                    (-0.13, 0.0),
                    (-0.21, -0.17),
                    (-0.14, -0.18),
                    (0.03, -0.03),
                ],
                // the upper lobe long and swept up
                Fish::Shark => &[
                    (0.04, 0.02),
                    (-0.1, 0.12),
                    (-0.22, 0.3),
                    (-0.25, 0.29),
                    (-0.17, 0.08),
                    (-0.14, 0.0),
                    (-0.16, -0.1),
                    (-0.12, -0.11),
                    (0.04, -0.02),
                ],
            };
            m.plate(Vector3::ZERO, SIDEWAYS, tail, 0.012);
        }
        Part::FishMarks(Fish::Piranha) => {
            // a row of sharp teeth along the jaw, pointing up
            for k in 0..7 {
                let t = frac(k, 6) * 2.0 - 1.0;
                let at = v(0.05 * t, 0.175, 0.43 - 0.06 * t * t);
                m.rod(at, at + v(0.0, 0.035, 0.004), (0.009, 0.001), 6);
            }
        }
        Part::FishMarks(Fish::Shark) => {
            // five gill slits down each side behind the eye
            for side in [-1.0f32, 1.0] {
                for k in 0..5 {
                    let z = 0.29 - 0.025 * k as f32;
                    let x = side * 0.105;
                    m.rod(v(x, 0.1, z), v(x, 0.2, z + 0.01), (0.005, 0.005), 6);
                }
            }
        }
        Part::FishMarks(Fish::Plain) => {}
        Part::Eel(piece) => {
            let path = eel_path(piece);
            let (t0, t1) = eel_span(piece);
            let n = path.len() - 1;
            m.sweep(
                &path,
                |k| eel_half(t0 + (t1 - t0) * frac(k, n)),
                2.0,
                false,
                Vector3::UP,
            );
        }
        Part::EelFin(piece) => {
            // along its back from a quarter of the way, and under its tail
            for (from, up) in [(0.22f32, 1.0f32), (0.58, -1.0)] {
                let (t0, t1) = eel_span(piece);
                let (t0, t1) = (t0.max(from), t1);
                if t1 - t0 < 0.02 {
                    continue;
                }
                let fin = |t: f32| {
                    0.045 * smooth(from, from + 0.12, t) * (1.0 + 0.6 * smooth(0.82, 1.0, t))
                };
                let n = 12;
                let path: Vec<Vector3> = (0..=n)
                    .map(|k| {
                        let t = t0 + (t1 - t0) * frac(k, n);
                        let (_, half_up) = eel_half(t);
                        eel_at(t) - eel_origin(piece)
                            + v(0.0, up * (half_up + fin(t) * 0.5 - 0.01), 0.0)
                    })
                    .collect();
                m.sweep(
                    &path,
                    |k| {
                        let t = t0 + (t1 - t0) * frac(k, n);
                        (0.005, fin(t) * 0.5 + 0.004)
                    },
                    2.0,
                    false,
                    Vector3::UP,
                );
            }
        }
        Part::EelSpots(piece) => {
            let (t0, t1) = eel_span(piece);
            for k in 0..16 {
                let t = 0.12 + 0.78 * frac(k, 15);
                if t < t0 || t >= t1 {
                    continue;
                }
                let at = eel_at(t);
                let ahead = (eel_at(t - 0.01) - eel_at(t + 0.01)).normalized();
                let side = ahead.cross(Vector3::UP).normalized();
                let (across, up) = eel_half(t);
                let yaw = ahead.x.atan2(ahead.z).to_degrees();
                for s in [-1.0f32, 1.0] {
                    let p = at - eel_origin(piece)
                        + side * (s * across * 0.93)
                        + v(0.0, up * 0.25, 0.0);
                    m.ball(p, v(0.007, 0.012, 0.016), [0.0, yaw, 0.0], false, 8);
                }
            }
        }
        Part::JellyBell => {
            m.ball(v(0.0, 0.72, 0.0), v(0.32, 0.28, 0.32), [0.0; 3], true, 28);
            // its rim frilled in lobes
            for k in 0..14 {
                let a = frac(k, 14) * TAU;
                m.ball(
                    v(0.31 * a.cos(), 0.715, 0.31 * a.sin()),
                    v(0.05, 0.018, 0.03),
                    [0.0, -a.to_degrees() + 90.0, 0.0],
                    false,
                    10,
                );
            }
        }
        Part::JellyRings => {
            // four rings round its middle, under the top of the bell
            for k in 0..4 {
                let a = frac(k, 4) * TAU + 0.785;
                let c = v(0.1 * a.cos(), 0.82, 0.1 * a.sin());
                let ring: Vec<Vector3> = (0..16)
                    .map(|j| {
                        let b = frac(j, 16) * TAU;
                        c + v(0.055 * b.cos(), 0.0, 0.055 * b.sin())
                    })
                    .collect();
                m.sweep(&ring, |_| (0.014, 0.01), 2.0, true, Vector3::UP);
            }
        }
        Part::JellyArms => {
            // four frilled arms hanging from its mouth, waving
            for k in 0..4 {
                let a = frac(k, 4) * TAU;
                let n = 14;
                let path: Vec<Vector3> = (0..=n)
                    .map(|j| {
                        let t = frac(j, n);
                        let wave = 0.05 * (t * 9.0 + k as f32).sin() * t;
                        v(
                            (0.03 + 0.06 * t) * a.cos() + wave * a.sin(),
                            0.74 - 0.6 * t,
                            (0.03 + 0.06 * t) * a.sin() - wave * a.cos(),
                        )
                    })
                    .collect();
                m.sweep(
                    &path,
                    |j| {
                        let t = frac(j, n);
                        (
                            0.04 * (1.0 - 0.6 * t) * (1.0 + 0.3 * (t * 23.0).sin()),
                            0.008,
                        )
                    },
                    2.0,
                    false,
                    Vector3::new(a.cos(), 0.0, a.sin()),
                );
            }
        }
        Part::JellyStrands => {
            // fine tentacles from the rim, trailing down in curls
            for k in 0..16 {
                let a = frac(k, 16) * TAU + 0.2;
                let n = 16;
                let len = 0.55 + 0.15 * ((k * 7) % 5) as f32 / 4.0;
                let path: Vec<Vector3> = (0..=n)
                    .map(|j| {
                        let t = frac(j, n);
                        let r = 0.29 - 0.08 * t;
                        let curl = 0.04 * (t * 11.0 + k as f32 * 1.3).sin() * t;
                        v(
                            r * a.cos() + curl * a.sin(),
                            0.7 - len * t,
                            r * a.sin() - curl * a.cos(),
                        )
                    })
                    .collect();
                m.sweep(
                    &path,
                    |j| {
                        let r = 0.006 * (1.0 - 0.7 * frac(j, n));
                        (r, r)
                    },
                    2.0,
                    false,
                    Vector3::new(a.cos(), 0.0, a.sin()),
                );
            }
        }
        Part::KrakenBody => {
            m.loft(KRAKEN_BODY, Transform3D::IDENTITY);
            // the two fins at its mantle's tip
            for side in [-1.0f32, 1.0] {
                let out = v(side, 0.15, 0.0).normalized();
                m.plate(
                    v(side * 0.05, 0.44, -0.42),
                    (out, Vector3::BACK),
                    &[(0.0, 0.1), (0.12, 0.02), (0.13, -0.04), (0.0, -0.1)],
                    0.016,
                );
            }
        }
        Part::KrakenArms(quarter) => {
            for &(deg, long) in quarter.arms() {
                let path = kraken_arm(deg, long);
                let n = path.len() - 1;
                m.sweep(
                    &path,
                    |k| {
                        let t = frac(k, n);
                        let r = 0.05 * (1.0 - 0.88 * t);
                        (r, r * 0.8)
                    },
                    2.0,
                    false,
                    Vector3::UP,
                );
                if long {
                    // its club
                    let tip = path[n];
                    let back = path[n - 2];
                    m.ball(
                        tip + (tip - back) * 0.5,
                        v(0.035, 0.022, 0.06),
                        [0.0, (tip - back).x.atan2((tip - back).z).to_degrees(), 0.0],
                        false,
                        12,
                    );
                }
            }
        }
        Part::KrakenSuckers(quarter) => {
            for &(deg, long) in quarter.arms() {
                let path = kraken_arm(deg, long);
                let n = path.len() - 1;
                for k in (2..n - 1).step_by(2) {
                    let t = frac(k, n);
                    let r = 0.05 * (1.0 - 0.88 * t);
                    // under the arm, where it curls up out of the water
                    let ahead = (path[k + 1] - path[k - 1]).normalized();
                    let side = ahead.cross(Vector3::UP).normalized();
                    let under = side.cross(ahead).normalized() * -1.0;
                    let at = path[k] + under * (r * 0.8);
                    m.ball(at, v(0.6 * r, 0.25 * r, 0.6 * r), [0.0; 3], false, 8);
                }
            }
        }
    }
}

/// An eel piece's middle line, from its pivot.
fn eel_path(piece: Eel) -> Vec<Vector3> {
    let (t0, t1) = eel_span(piece);
    let n = 14;
    (0..=n)
        .map(|k| {
            let t = t0 + (t1 - t0) * frac(k, n);
            eel_at(t) - eel_origin(piece)
        })
        .collect()
}

/// One of a kraken's arms from its pivot, reaching `deg` from ahead (to
/// its left) along the water, waving, curling up out of it at its end; a
/// long tentacle reaching further.
fn kraken_arm(deg: f32, long: bool) -> Vec<Vector3> {
    let n = 16;
    let reach = if long { 0.52 } else { 0.46 };
    (0..=n)
        .map(|k| {
            let t = frac(k, n);
            // it turns a little as it goes, curling at its end
            let a = (deg + 30.0 * t * t * deg.signum()).to_radians();
            let out = v(a.sin(), 0.0, a.cos()) * (0.06 + reach * t);
            let wave = 0.03 * (t * 7.0 + deg).sin();
            let rise = -0.08 * (t * PI).sin() + 0.2 * smooth(0.65, 1.0, t);
            out + v(0.0, rise + wave, 0.0)
        })
        .collect()
}

/// A plate's outline, the points round it.
type Outline = &'static [(f32, f32)];

/// A fish's fins but its tail: on its back, under it, at its sides.
fn fins(m: &mut Merge, fish: Fish) {
    let (back, under, side): (Outline, Outline, ([f32; 3], f32)) = match fish {
        Fish::Plain | Fish::Piranha => (
            &[
                (0.0, 0.48),
                (-0.08, 0.6),
                (-0.2, 0.58),
                (-0.3, 0.4),
                (-0.18, 0.42),
                (-0.06, 0.46),
            ],
            &[(-0.1, 0.07), (-0.3, 0.16), (-0.34, 0.06), (-0.22, 0.0)],
            ([0.09, 0.2, 0.2], 0.12),
        ),
        Fish::Shark => (
            &[
                (0.14, 0.26),
                (0.02, 0.42),
                (-0.05, 0.56),
                (-0.07, 0.52),
                (-0.04, 0.38),
                (-0.1, 0.25),
                (0.02, 0.24),
            ],
            &[(-0.27, 0.12), (-0.33, 0.04), (-0.36, 0.05), (-0.36, 0.12)],
            ([0.1, 0.1, 0.18], 0.27),
        ),
    };
    m.plate(Vector3::ZERO, SIDEWAYS, back, 0.012);
    m.plate(Vector3::ZERO, SIDEWAYS, under, 0.01);
    if fish == Fish::Shark {
        // a second, small fin on its back near the tail
        m.plate(
            Vector3::ZERO,
            SIDEWAYS,
            &[(-0.27, 0.21), (-0.31, 0.29), (-0.34, 0.28), (-0.35, 0.2)],
            0.01,
        );
    }
    // the fins at its sides, swept back and down
    let ([x, y, z], reach) = side;
    for s in [-1.0f32, 1.0] {
        let out = v(s, -0.35, 0.0).normalized();
        m.plate(
            v(s * x, y, z),
            (out, Vector3::FORWARD),
            &[
                (0.0, 0.0),
                (reach * 0.3, 0.0),
                (reach, reach * 0.45),
                (reach * 0.95, reach * 0.6),
                (reach * 0.35, reach * 0.45),
                (0.0, reach * 0.3),
            ],
            0.01,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merged(part: Part) -> Merge {
        let mut m = Merge::default();
        build(&mut m, part);
        m
    }

    const ALL: &[Part] = &[
        Part::FishBody(Fish::Piranha),
        Part::FishBody(Fish::Shark),
        Part::FishFins(Fish::Piranha),
        Part::FishFins(Fish::Shark),
        Part::FishTail(Fish::Piranha),
        Part::FishTail(Fish::Shark),
        Part::FishMarks(Fish::Piranha),
        Part::FishMarks(Fish::Shark),
        Part::Eel(Eel::Front),
        Part::Eel(Eel::Middle),
        Part::Eel(Eel::Tail),
        Part::EelFin(Eel::Front),
        Part::EelFin(Eel::Middle),
        Part::EelFin(Eel::Tail),
        Part::EelSpots(Eel::Front),
        Part::EelSpots(Eel::Middle),
        Part::EelSpots(Eel::Tail),
        Part::JellyBell,
        Part::JellyRings,
        Part::JellyArms,
        Part::JellyStrands,
        Part::KrakenBody,
        Part::KrakenArms(Quarter::FrontLeft),
        Part::KrakenArms(Quarter::BackRight),
        Part::KrakenSuckers(Quarter::FrontRight),
        Part::KrakenSuckers(Quarter::BackLeft),
    ];

    #[test]
    fn every_sea_shape_faces_out() {
        for &part in ALL {
            let m = merged(part);
            assert!(
                !m.idx.is_empty() && m.idx.len().is_multiple_of(3),
                "{part:?}"
            );
            assert!(
                m.nrm.iter().all(|n| (n.length() - 1.0).abs() < 1e-3),
                "{part:?}"
            );
            for t in m.idx.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
                let face = (m.pos[b] - m.pos[a]).cross(m.pos[c] - m.pos[a]);
                assert!(face.dot(m.nrm[a] + m.nrm[b] + m.nrm[c]) <= 1e-9, "{part:?}");
            }
        }
    }

    #[test]
    fn a_fish_stands_on_the_floor_within_its_cell() {
        for fish in [Fish::Piranha, Fish::Shark] {
            let mut m = merged(Part::FishBody(fish));
            build(&mut m, Part::FishFins(fish));
            let (low, high) = m.span();
            assert!(low.abs() < 0.03, "{fish:?} {low}");
            assert!(high > 0.45 && high < 0.65, "{fish:?} {high}");
            let long = m.pos.iter().map(|p| p.z).fold(f32::MIN, f32::max)
                - m.pos.iter().map(|p| p.z).fold(f32::MAX, f32::min);
            assert!(long < 1.0, "{fish:?} {long}");
        }
    }

    #[test]
    fn an_eels_pieces_meet_at_their_joints() {
        // each piece's end overlaps the next one's start, in the body's
        // space (the pivots placed as the kit places them)
        let front = merged(Part::Eel(Eel::Front));
        let mid = merged(Part::Eel(Eel::Middle));
        let j0 = Vector3::from_array(eel_joint(0));
        let near = |m: &Merge, at: Vector3, off: Vector3| {
            m.pos.iter().any(|p| (*p + off - at).length() < 0.05)
        };
        assert!(near(&front, j0, Vector3::ZERO));
        assert!(near(&mid, j0, j0));
        // it lies on the floor and within a cell and a bit
        let (low, _) = front.span();
        assert!(low.abs() < 0.02, "{low}");
        let tail = merged(Part::Eel(Eel::Tail));
        let j1 = j0 + Vector3::from_array(eel_joint(1));
        let zs = front
            .pos
            .iter()
            .map(|p| p.z)
            .chain(tail.pos.iter().map(|p| p.z + j1.z));
        let (lo, hi) = zs.fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)));
        assert!(hi - lo < 1.3, "{}", hi - lo);
    }

    #[test]
    fn a_kraken_spreads_its_arms_within_its_cell() {
        let mut reach = 0.0f32;
        for q in Quarter::ALL {
            let m = merged(Part::KrakenArms(q));
            let pivot = Vector3::from_array(KRAKEN_ARMS);
            for p in &m.pos {
                let at = *p + pivot;
                reach = reach.max(at.x.abs()).max(at.z.abs());
            }
        }
        assert!(reach < 0.7, "{reach}");
        assert!(reach > 0.45, "{reach}");
    }
}
