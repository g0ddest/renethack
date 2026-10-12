//! The shapes of weather that moves about as a creature (`creature_kit`'s
//! vortices and elementals): a vortex's funnel, smooth for the bands its
//! shader flows over it; a fog cloud's swirl of puffs; an air elemental's
//! broader funnel with arms of wind; what a vortex
//! carries (a fire elemental is the torches' flames: no shape of its
//! own). They stand one unit tall on y = 0 and spin about y.

use super::*;

/// The parts there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A vortex's funnel, from the floor up.
    Twister,
    /// A fog cloud's puffs, in a slow, low swirl.
    Fog,
    /// An air elemental's broader funnel and two arms of wind.
    Gale,
    /// The denser core up a vortex's middle.
    Core,
    /// Bits caught up in it: stones in dust, shards in ice.
    Debris,
}

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

/// What a funnel's inner shell adds to its texture's u (the shader tells
/// the shells apart by it).
pub const INNER_U: f32 = 2.0;

pub(super) fn build(m: &mut Merge, part: Part) {
    match part {
        Part::Twister => funnel(m, 0.1, 0.36, 1.0),
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
                m.ball(at, v(size, size * 0.7, size), [0.0; 3], false, 16);
            }
        }
        Part::Gale => {
            funnel(m, 0.12, 0.46, 1.0);
            // arms of wind wound out from its middle, thinning to nothing
            for side in [-1.0f32, 1.0] {
                let arm: Vec<Vector3> = (0..=16)
                    .map(|k| {
                        let t = frac(k, 16);
                        let a = side * t * 1.8 * TAU;
                        let r = 0.04 + 0.07 * t;
                        v(side * (0.16 + 0.34 * t), 0.55 + 0.14 * t, 0.0)
                            + v(0.0, r * a.cos(), r * a.sin())
                    })
                    .collect();
                m.sweep(
                    &arm,
                    |k| {
                        let w = 0.1 * (1.0 - frac(k, 16)).powf(0.7) + 0.006;
                        (w, w)
                    },
                    2.0,
                    false,
                    Vector3::BACK,
                );
            }
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
                    let r = 0.008 + 0.018 * frac(j, 16);
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
                // inside the funnel at every height
                let r = 0.08 + 0.16 * t;
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
    }
}

/// A funnel round y, `low` across at the floor and `high` at its top
/// `tall` up, open at both ends, its axis swaying a little: an outer shell
/// and a narrower, lower inner one (its u moved on by `INNER_U`).
fn funnel(m: &mut Merge, low: f32, high: f32, tall: f32) {
    const ROWS: usize = 24;
    const SIDES: usize = 40;
    for (shell, k, top) in [(0.0, 1.0, tall), (INNER_U, 0.5, tall * 0.85)] {
        let first = m.uv.len();
        m.rings(ROWS, SIDES, |i, j| {
            let t = frac(i, ROWS - 1);
            let a = frac(j, SIDES) * TAU;
            let r = k * (low + (high - low) * t.powf(1.4));
            // the slope of its side (across over up), for the normal
            let slope = k * (high - low) * 1.4 * t.powf(0.4) / top;
            let sway = v(
                0.04 * (t * PI * 1.3).sin(),
                0.0,
                0.03 * (t * PI * 0.9 + 1.0).sin(),
            );
            let p = sway + v(r * a.cos(), 0.01 + (top - 0.02) * t, r * a.sin());
            (p, v(a.cos(), -slope, a.sin()).normalized())
        });
        for uv in &mut m.uv[first..] {
            uv.x += shell;
        }
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

    #[test]
    fn a_funnel_stands_on_the_floor_a_unit_tall() {
        for part in [Part::Twister, Part::Gale] {
            let m = merged(part);
            let (low, high) = m.span();
            assert!(low.abs() < 0.02, "{part:?} {low}");
            assert!(high > 0.9 && high < 1.0, "{part:?} {high}");
            // no wider than a cell
            let across = m
                .pos
                .iter()
                .map(|p| p.x.abs().max(p.z.abs()))
                .fold(0.0, f32::max);
            assert!(across < 0.55, "{part:?} {across}");
        }
    }

    #[test]
    fn a_funnels_inner_shell_is_told_apart_by_its_u() {
        let m = merged(Part::Twister);
        let (inner, outer): (Vec<&Vector2>, Vec<&Vector2>) = m.uv.iter().partition(|uv| uv.x > 1.5);
        assert_eq!(inner.len(), outer.len());
        assert!(outer.iter().all(|uv| (0.0..=1.0).contains(&uv.x)));
        assert!(
            inner
                .iter()
                .all(|uv| (INNER_U..=INNER_U + 1.0).contains(&uv.x))
        );
        // its texture's v runs from the foot to the top
        assert!(m.uv.iter().all(|uv| (0.0..=1.0).contains(&uv.y)));
    }
}
