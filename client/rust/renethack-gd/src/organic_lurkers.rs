//! The creatures that lie in wait (`creature_kit`): a piercer, a cone of
//! stone (or iron, or glass) grown in rings like a stalactite, on a
//! fleshy foot with a mouth in it; a trapper or a lurker above, a broad
//! flat cloak of a body, humped in its middle and lumpy, thin and ragged
//! at its rim, its edge rippling, its maw in the front of its rim. They
//! have no eyes. Each stands on y = 0 and faces +z.

use super::*;

/// The parts there are. A part is what one material covers, on one pivot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A piercer's cone, ringed and leaning a little.
    Piercer,
    /// Its foot, a fleshy rim round the cone's base; the mouth in its
    /// front.
    PiercerFoot,
    PiercerMouth,
    /// A cloak: its back and its belly, meeting at its ragged rim (the
    /// belly's u moved on by `BELLY_U`).
    Cloak,
    /// Its maw, a dark wet slit in the front of its rim, and the teeth
    /// along its lip.
    CloakMaw,
    CloakTeeth,
}

/// What a cloak's belly adds to its texture's u (its shader tells its
/// underside from its back by it).
pub const BELLY_U: f32 = 2.0;

/// How far its rim is drawn in at its front, round its maw, of the way.
const MAW_NOTCH: f32 = 0.1;

fn v(x: f32, y: f32, z: f32) -> Vector3 {
    Vector3::new(x, y, z)
}

fn frac(i: usize, n: usize) -> f32 {
    i as f32 / n as f32
}

/// The cone's height.
const PIERCER_TALL: f32 = 1.0;

/// The piercer's rings from its base up (along z, stood up by the
/// loft's turn): narrowing to a point, a ridge of growth every so often,
/// leaning back a little near the top.
fn piercer_rings() -> Vec<Section> {
    let n = 22;
    (0..=n)
        .map(|k| {
            let h = frac(k, n);
            let ridge = 1.0 + 0.07 * (h * 38.0).sin() * (1.0 - h);
            let r = (0.3 * (1.0 - h).powf(0.95) + 0.008) * ridge;
            Section {
                // (y here is how far it leans back once stood up)
                y: 0.05 * (h * PI * 0.8).sin(),
                z: 0.02 + (PIERCER_TALL - 0.02) * h,
                rx: r,
                ry: r * 0.94,
                n: 2.3,
            }
        })
        .collect()
}

/// How far a cloak's rim reaches at `a` round it (from its front, +z):
/// an oval wider than it is long, in lobes, drawn in round its maw.
fn cloak_rim(a: f32) -> f32 {
    let (across, along) = (0.6, 0.48);
    let oval = across * along / ((along * a.sin()).powi(2) + (across * a.cos()).powi(2)).sqrt();
    let maw = 1.0 - MAW_NOTCH * a.cos().max(0.0).powi(16);
    oval * maw * (1.0 + 0.07 * (5.0 * a).sin() + 0.04 * (11.0 * a + 1.0).sin())
}

/// Its back (`top`) or its belly `r` of the way out to its rim at `a`
/// round it: humped in the middle, thin at the rim, the edge rippling,
/// lumps of muscle across its back (not a leaf's ribs).
fn cloak_at(r: f32, a: f32, top: bool) -> Vector3 {
    let out = cloak_rim(a) * r;
    let hump = (1.0 - r * r).max(0.0).powf(0.7);
    let ripple = 0.018 * r * r * (6.0 * a).sin();
    let y = if top {
        let lump = ((3.0 * a + 5.0 * r).sin() * (7.0 * a - 3.0 * r + 1.3).sin()).max(-0.4);
        0.06 + 0.075 * hump + 0.01 * lump * (r * PI).sin()
    } else {
        0.06 - 0.025 * hump
    };
    v(out * a.sin(), y + ripple, out * a.cos())
}

pub(super) fn build(m: &mut Merge, part: Part) {
    // the loft's z stood up as y
    let stood = Transform3D::new(
        Basis::from_euler(EulerOrder::YXZ, v(-FRAC_PI_2, 0.0, 0.0)),
        Vector3::ZERO,
    );
    match part {
        Part::Piercer => m.loft(&piercer_rings(), stood),
        Part::PiercerFoot => {
            let ring: Vec<Vector3> = (0..24)
                .map(|k| {
                    let a = frac(k, 24) * TAU;
                    let wobble = 1.0 + 0.06 * (a * 5.0).sin();
                    v(0.29 * wobble * a.cos(), 0.035, 0.29 * wobble * a.sin())
                })
                .collect();
            m.sweep(&ring, |_| (0.06, 0.035), 2.0, true, Vector3::UP);
        }
        Part::PiercerMouth => {
            // a wide slit in the foot's front, lips drawn back
            m.ball(
                v(0.0, 0.06, 0.33),
                v(0.11, 0.022, 0.03),
                [0.0; 3],
                false,
                14,
            );
        }
        Part::Cloak => {
            const ROWS: usize = 12;
            const SIDES: usize = 48;
            for top in [true, false] {
                let first = m.uv.len();
                m.rings(ROWS, SIDES, |i, j| {
                    let r = frac(i, ROWS - 1);
                    let a = frac(j, SIDES) * TAU;
                    let p = cloak_at(r, a, top);
                    // the surface's normal from its directions there,
                    // up from the back, down from the belly
                    let e = 1e-3;
                    let along =
                        cloak_at((r + e).min(1.0), a, top) - cloak_at((r - e).max(0.0), a, top);
                    let round =
                        cloak_at(r.max(0.02), a + e, top) - cloak_at(r.max(0.02), a - e, top);
                    let n = along.cross(round);
                    let n = if (n.y > 0.0) == top { n } else { -n };
                    let n = if n.length_squared() > 1e-14 {
                        n.normalized()
                    } else if top {
                        Vector3::UP
                    } else {
                        Vector3::DOWN
                    };
                    (p, n)
                });
                if !top {
                    for uv in &mut m.uv[first..] {
                        uv.x += BELLY_U;
                    }
                }
            }
        }
        Part::CloakMaw => {
            // in the notch of the rim, its lips a little over its back
            let z = cloak_rim(0.0) - 0.04;
            m.ball(v(0.0, 0.072, z), v(0.17, 0.028, 0.06), [0.0; 3], false, 18);
        }
        Part::CloakTeeth => {
            let z = cloak_rim(0.0) - 0.006;
            for k in 0..9 {
                let x = -0.14 + 0.28 * frac(k, 8);
                let side = x / 0.14;
                // shorter towards the corners of its mouth, which curve
                // back with the lip
                let long = 0.034 * (1.0 - 0.45 * side.abs());
                let at = v(x, 0.092 - long * 0.4, z - 0.05 * side * side);
                m.ball(at, v(0.014, long, 0.012), [0.0; 3], false, 6);
            }
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
    fn every_lurker_shape_faces_out() {
        for part in [
            Part::Piercer,
            Part::PiercerFoot,
            Part::PiercerMouth,
            Part::Cloak,
            Part::CloakMaw,
            Part::CloakTeeth,
        ] {
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
    fn a_piercer_stands_a_unit_tall_on_its_foot() {
        let m = merged(Part::Piercer);
        let (low, high) = m.span();
        assert!(low.abs() < 0.03, "{low}");
        assert!((high - PIERCER_TALL).abs() < 0.03, "{high}");
        // its point up, its base wide
        let top = m
            .pos
            .iter()
            .filter(|p| p.y > 0.9)
            .map(|p| p.x.abs())
            .fold(0.0, f32::max);
        let base = m
            .pos
            .iter()
            .filter(|p| p.y < 0.1)
            .map(|p| p.x.abs())
            .fold(0.0, f32::max);
        assert!(top < 0.06 && base > 0.25, "{top} {base}");
    }

    #[test]
    fn a_cloak_lies_flat_within_its_cell() {
        let m = merged(Part::Cloak);
        let (low, high) = m.span();
        assert!(low > 0.0 && high < 0.18, "{low} {high}");
        let across = m.pos.iter().map(|p| p.x.abs()).fold(0.0, f32::max);
        let along = m.pos.iter().map(|p| p.z.abs()).fold(0.0, f32::max);
        assert!(across < 0.68 && along < 0.6, "{across} {along}");
        assert!(across > along, "wider than long");
        // its belly told apart by its u, its back's within one turn
        let (belly, back): (Vec<&Vector2>, Vec<&Vector2>) =
            m.uv.iter().partition(|uv| uv.x > BELLY_U - 0.5);
        assert_eq!(belly.len(), back.len());
        assert!(back.iter().all(|uv| (0.0..=1.0).contains(&uv.x)));
    }

    #[test]
    fn a_cloaks_maw_is_in_the_front_of_its_rim() {
        let rim = cloak_rim(0.0);
        for part in [Part::CloakMaw, Part::CloakTeeth] {
            let m = merged(part);
            let front = m.pos.iter().map(|p| p.z).fold(f32::MIN, f32::max);
            assert!(
                front > rim - 0.08 && front < rim + 0.06,
                "{part:?} {front} {rim}"
            );
            assert!(
                m.pos.iter().all(|p| p.x.abs() < 0.2 && p.y > 0.0),
                "{part:?}"
            );
        }
    }
}
