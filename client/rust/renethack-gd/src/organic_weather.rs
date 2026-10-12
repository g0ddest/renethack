//! The shapes of weather that moves about as a creature (`creature_kit`'s
//! vortices and elementals): a twister of translucent bands wound round a
//! funnel, a fog cloud's swirl of puffs, an air elemental's broader
//! twister with arms of wind, a fire elemental's tongues of flame rising
//! into a figure. They stand one unit tall on y = 0 and spin about y.

use super::*;

/// The parts there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A vortex's bands, wound round its funnel from the floor up.
    Twister,
    /// A fog cloud's puffs, in a slow, low swirl.
    Fog,
    /// An air elemental's broader twister, two arms of wind and a crown
    /// of cloud.
    Gale,
    /// A fire elemental: tongues of flame rising round a hotter core, two
    /// for arms, one for a head.
    Flames,
    /// The denser core up a vortex's middle.
    Core,
    /// Bits caught up in it: stones in dust, shards in ice.
    Debris,
    /// A fire elemental's hotter heart, inside its flames.
    Heart,
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::Twister => twister(m, 5, 0.1, 0.36, 1.0, Vector3::ZERO),
        Part::Fog => {
            for k in 0..16 {
                let t = frac(k, 16);
                let a = t * 2.6 * TAU;
                let r = 0.14 + 0.32 * t.sqrt();
                let at = v(
                    r * a.cos(),
                    0.16 + 0.32 * (1.0 - t) + 0.08 * (a * 1.7).sin(),
                    r * a.sin(),
                );
                let size = 0.24 - 0.09 * t;
                m.ball(at, v(size, size * 0.7, size), [0.0; 3], false, 12);
            }
        }
        Part::Gale => {
            twister(m, 6, 0.12, 0.42, 1.0, Vector3::ZERO);
            // arms of wind wound out from its middle
            for side in [-1.0f32, 1.0] {
                let arm: Vec<Vector3> = (0..=14)
                    .map(|k| {
                        let t = frac(k, 14);
                        let a = side * t * 1.8 * TAU;
                        let r = 0.05 + 0.08 * t;
                        v(side * (0.18 + 0.32 * t), 0.55 + 0.12 * t, 0.0)
                            + v(0.0, r * a.cos(), r * a.sin())
                    })
                    .collect();
                m.sweep(
                    &arm,
                    |k| (0.01, 0.03 + 0.03 * frac(k, 14)),
                    2.0,
                    false,
                    Vector3::BACK,
                );
            }
            // a crown of cloud over it
            for k in 0..7 {
                let a = frac(k, 7) * TAU;
                m.ball(
                    v(0.36 * a.cos(), 0.96, 0.36 * a.sin()),
                    v(0.13, 0.08, 0.13),
                    [0.0; 3],
                    false,
                    12,
                );
            }
        }
        Part::Flames => {
            // tongues rising from a ring on the floor, curling as they go
            for k in 0..11 {
                let a = frac(k, 11) * TAU;
                let h = 0.6 + 0.3 * (k as f32 * 2.3).sin().abs();
                let tongue: Vec<Vector3> = (0..=10)
                    .map(|j| {
                        let t = frac(j, 10);
                        let swirl = a + t * 1.4;
                        let r = 0.22 * (1.0 - 0.7 * t);
                        v(r * swirl.cos(), h * t, r * swirl.sin())
                    })
                    .collect();
                m.sweep(
                    &tongue,
                    |j| {
                        let w = 0.13 * (1.0 - frac(j, 10)).powf(0.7) + 0.005;
                        (w, w)
                    },
                    2.0,
                    false,
                    Vector3::BACK,
                );
            }
            // two tongues reaching out for arms, one up for a head
            for side in [-1.0f32, 1.0] {
                let arm = [
                    v(side * 0.12, 0.62, 0.0),
                    v(side * 0.3, 0.72, 0.04),
                    v(side * 0.44, 0.9, 0.06),
                ];
                m.sweep(
                    &arm,
                    |j| {
                        let w = 0.08 * (1.0 - frac(j, 2) * 0.85);
                        (w, w)
                    },
                    2.0,
                    false,
                    Vector3::BACK,
                );
            }
            m.ball(v(0.0, 0.92, 0.02), v(0.12, 0.15, 0.12), [0.0; 3], false, 14);
            m.rod(v(0.0, 1.0, 0.02), v(0.0, 1.12, -0.03), (0.08, 0.004), 10);
        }
        Part::Core => {
            // a twisted rope of it up the middle, thin at the floor
            let path: Vec<Vector3> = (0..=16)
                .map(|j| {
                    let t = frac(j, 16);
                    let a = t * 3.0 * TAU;
                    v(0.03 * a.cos(), 0.02 + 0.9 * t, 0.03 * a.sin())
                })
                .collect();
            m.sweep(
                &path,
                |j| {
                    let r = 0.03 + 0.07 * frac(j, 16);
                    (r, r)
                },
                2.0,
                false,
                Vector3::BACK,
            );
        }
        Part::Debris => {
            for k in 0..14 {
                let t = frac(k, 14);
                let a = t * 3.3 * TAU;
                let r = 0.12 + 0.24 * t;
                let at = v(r * a.cos(), 0.08 + 0.8 * t, r * a.sin());
                let size = 0.035 - 0.012 * t;
                m.ball(
                    at,
                    v(size, size * 0.7, size * 1.3),
                    [30.0 * k as f32, 50.0 * k as f32, 0.0],
                    false,
                    6,
                );
            }
        }
        Part::Heart => {
            m.ball(v(0.0, 0.42, 0.0), v(0.12, 0.3, 0.12), [0.0; 3], false, 14);
            m.ball(
                v(0.0, 0.88, 0.02),
                Vector3::splat(0.08),
                [0.0; 3],
                false,
                12,
            );
        }
    }
}

/// `n` bands wound round a funnel from `low` across at the floor to
/// `high` across at its top `tall` up, from `at`.
fn twister(m: &mut Merge, n: usize, low: f32, high: f32, tall: f32, at: Vector3) {
    for k in 0..n {
        let phase = frac(k, n) * TAU;
        let path: Vec<Vector3> = (0..=28)
            .map(|j| {
                let t = frac(j, 28);
                let a = phase + t * 2.8 * TAU;
                let r = low + (high - low) * t.powf(1.25);
                at + v(r * a.cos(), 0.02 + (tall - 0.04) * t, r * a.sin())
            })
            .collect();
        // each band its own breadth, broader towards the top
        let broad = 0.6 + 0.4 * ((k as f32 * 1.7).sin() * 0.5 + 0.5);
        m.sweep(
            &path,
            |j| (0.006, (0.03 + 0.09 * frac(j, 28)) * broad),
            2.0,
            false,
            Vector3::UP,
        );
    }
}
