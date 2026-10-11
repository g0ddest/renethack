//! The big cats' shapes (`creature_kit::big_cat`): a heavier build than a
//! housecat's, a broad head with a long muzzle, thick legs on broad paws,
//! a long tail hung low (a lynx's short bob); and the marks of a coat laid
//! on those lofted surfaces: a tiger's stripes, a jaguar's rosettes, a
//! lynx's ear tufts, ruff and black-tipped tail, a displacer beast's
//! shoulder tentacles.

use super::*;

/// Where a big cat's legs join its body (out to the side, up, forward), as
/// `CAT_SHOULDER` and `CAT_HIP` for a cat's.
pub const SHOULDER: [f32; 3] = [0.1, 0.47, 0.27];
pub const HIP: [f32; 3] = [0.11, 0.5, -0.29];
/// A displacer beast's middle legs, between the two pairs.
pub const MIDDLE: [f32; 3] = [0.115, 0.47, -0.01];
/// The pivots of the head, the tail and a tentacle (its right one; the
/// left is mirrored).
pub const HEAD_AT: [f32; 3] = [0.0, 0.68, 0.5];
pub const TAIL_AT: [f32; 3] = [0.0, 0.56, -0.45];
pub const TENTACLE_AT: [f32; 3] = [0.09, 0.74, 0.22];

/// Leg shapes are a cat's, this much thicker.
const THICK: f32 = 1.35;

/// a deep chest and broad shoulders, the belly hardly tucked, a thick
/// neck carried low
const BODY: &[Section] = &[
    s(0.56, -0.46, 0.03, 0.03),
    s(0.56, -0.43, 0.13, 0.14),
    s(0.55, -0.34, 0.2, 0.23),
    s(0.53, -0.2, 0.19, 0.21),
    s(0.52, -0.04, 0.18, 0.2),
    s(0.53, 0.12, 0.19, 0.23),
    s(0.55, 0.25, 0.2, 0.25),
    s(0.59, 0.36, 0.15, 0.19),
    s(0.62, 0.43, 0.11, 0.13),
    s(0.64, 0.47, 0.07, 0.07),
    s(0.65, 0.49, 0.02, 0.02),
];

/// a broad skull, wide cheeks and a long, heavy muzzle
const HEAD: &[Section] = &[
    s(0.03, -0.14, 0.03, 0.03),
    s(0.03, -0.12, 0.1, 0.1),
    s(0.03, -0.06, 0.15, 0.13),
    s(0.01, 0.02, 0.16, 0.13),
    s(-0.03, 0.09, 0.13, 0.1),
    s(-0.05, 0.15, 0.1, 0.08),
    s(-0.055, 0.2, 0.08, 0.065),
    s(-0.055, 0.225, 0.05, 0.045),
    s(-0.055, 0.235, 0.015, 0.015),
];

/// a small round ear
const EAR: &[Section] = &[
    s(0.0, 0.0, 0.045, 0.014),
    s(0.025, 0.0, 0.042, 0.013),
    s(0.045, 0.0, 0.032, 0.011),
    s(0.058, 0.0, 0.018, 0.008),
    s(0.062, 0.0, 0.004, 0.003),
];

/// a lynx's tall pointed ear
const LYNX_EAR: &[Section] = &[
    s(0.0, 0.0, 0.045, 0.014),
    s(0.04, 0.0, 0.036, 0.012),
    s(0.08, 0.0, 0.022, 0.009),
    s(0.11, 0.0, 0.009, 0.005),
    s(0.12, 0.0, 0.002, 0.002),
];

/// hung low from the rump, its end turned back up off the floor
const TAIL: &[Section] = &[
    s(0.0, 0.03, 0.015, 0.015),
    s(0.0, 0.0, 0.05, 0.05),
    s(-0.07, -0.05, 0.046, 0.046),
    s(-0.17, -0.08, 0.042, 0.042),
    s(-0.28, -0.1, 0.039, 0.039),
    s(-0.38, -0.12, 0.037, 0.037),
    s(-0.45, -0.15, 0.036, 0.036),
    s(-0.48, -0.2, 0.035, 0.035),
    s(-0.47, -0.25, 0.034, 0.034),
    s(-0.43, -0.27, 0.031, 0.031),
    s(-0.41, -0.275, 0.01, 0.01),
];

/// a lynx's short bob, carried level
const BOB: &[Section] = &[
    s(0.0, 0.03, 0.015, 0.015),
    s(0.0, 0.0, 0.045, 0.045),
    s(0.01, -0.06, 0.042, 0.042),
    s(0.0, -0.11, 0.036, 0.036),
    s(-0.02, -0.14, 0.026, 0.026),
    s(-0.025, -0.15, 0.008, 0.008),
];

/// a displacer beast's tentacle: up from the shoulder, arched forward
const TENTACLE: &[Section] = &[
    s(0.0, -0.02, 0.012, 0.012),
    s(0.0, 0.0, 0.035, 0.035),
    s(0.1, -0.04, 0.031, 0.031),
    s(0.2, -0.05, 0.028, 0.028),
    s(0.29, -0.01, 0.025, 0.025),
    s(0.34, 0.07, 0.023, 0.023),
    s(0.35, 0.16, 0.021, 0.021),
    s(0.32, 0.24, 0.02, 0.02),
    s(0.31, 0.26, 0.006, 0.006),
];

/// The marks of a big cat's coat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coat {
    /// A tiger's stripes, rings on its tail, a black tip.
    Stripes,
    /// A jaguar's rosettes, spots on its head, rings by its tail's tip.
    Rosettes,
    /// A lynx's black ear tufts and tail tip.
    Lynx,
}

impl Coat {
    /// The colour of its marks on `part`: black, or a lynx's dappling
    /// a dark shade of its own fur.
    pub fn black_on(self, part: Part) -> bool {
        !(self == Coat::Lynx && part == Part::Body)
    }
}

/// Where on a big cat a coat's marks lie (each moves with its own pivot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Body,
    Head,
    Tail,
}

pub(super) fn body(m: &mut Merge) {
    m.loft(BODY, Transform3D::IDENTITY);
}

pub(super) fn head(m: &mut Merge, lynx: bool) {
    m.loft(HEAD, Transform3D::IDENTITY);
    for side in [-1.0f32, 1.0] {
        let (ear, at, turn) = if lynx {
            (
                LYNX_EAR,
                [side * 0.085, 0.1, -0.06],
                [-8.0, side * -12.0, side * -14.0],
            )
        } else {
            (
                EAR,
                [side * 0.1, 0.1, -0.06],
                [-14.0, side * -20.0, side * -30.0],
            )
        };
        m.loft(ear, place(at, turn, [1.0; 3]));
    }
    if lynx {
        ruff(m);
    }
}

/// A lynx's ruff: long fur hanging from its cheeks down past the jaw.
fn ruff(m: &mut Merge) {
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let t = k as f32 / 4.0;
            let at = Vector3::new(
                side * (0.145 - 0.035 * t),
                -0.05 - 0.055 * t,
                0.06 - 0.05 * t,
            );
            let turn = [20.0 + 25.0 * t, side * 25.0, side * (35.0 + 20.0 * t)];
            m.ball(at, Vector3::new(0.062, 0.026, 0.042), turn, false, 10);
        }
    }
}

pub(super) fn tail(m: &mut Merge, lynx: bool) {
    m.loft(if lynx { BOB } else { TAIL }, Transform3D::IDENTITY);
}

/// A big cat's leg: a cat's, thicker, on a broad paw.
pub(super) fn leg(m: &mut Merge, hind: bool) {
    let thick = place([0.0; 3], [0.0; 3], [THICK, 1.0, THICK]);
    let (shape, high, ahead) = if hind {
        (CAT_HINDLEG, HIP[1], 0.0)
    } else {
        (CAT_FORELEG, SHOULDER[1], 0.035)
    };
    m.loft(shape, thick);
    let at = Vector3::new(0.0, 0.031 - high, ahead);
    m.ball(at, Vector3::new(0.058, 0.031, 0.072), [0.0; 3], false, 14);
}

/// A displacer beast's tentacle (the right one; `right` false mirrors
/// it), ending in a pad set with spikes.
pub(super) fn tentacle(m: &mut Merge, right: bool) {
    let side = if right { 1.0 } else { -1.0 };
    let xf = place([0.0; 3], [0.0, side * -18.0, side * -12.0], [1.0; 3]);
    m.loft(TENTACLE, xf);
    let pad = xf * Vector3::new(0.0, 0.32, 0.25);
    m.ball(
        pad,
        Vector3::new(0.045, 0.03, 0.055),
        [20.0, 0.0, 0.0],
        false,
        12,
    );
    for k in 0..5 {
        let a = k as f32 / 5.0 * TAU;
        let out = Vector3::new(0.04 * a.cos(), 0.025 * a.sin(), 0.02);
        m.rod(pad + out * 0.8, pad + out * 1.9, (0.009, 0.001), 6);
    }
}

/// The marks of `coat` on `part`, laid on its surface.
pub(super) fn coat(m: &mut Merge, coat: Coat, part: Part) {
    let here = Transform3D::IDENTITY;
    match (coat, part) {
        (Coat::Stripes, Part::Body) => stripes(m, &Surface::of(BODY), 12, (0.1, 0.85), here),
        (Coat::Stripes, Part::Head) => {
            let head = Surface::of(HEAD);
            let len = head.len();
            // three short bars across the brow, two down each cheek
            for k in 0..3 {
                let s = len * (0.3 + 0.075 * k as f32);
                let half = |a: f32| 0.006 * (1.0 - ((a - FRAC_PI_2) / 0.45).powi(2)).max(0.2);
                m.band(
                    &head,
                    (FRAC_PI_2 - 0.45, FRAC_PI_2 + 0.45),
                    |_| s,
                    half,
                    here,
                );
            }
            for (a0, a1) in [(-0.35f32, 0.45f32), (PI - 0.45, PI + 0.35)] {
                for k in 0..2 {
                    let s = len * (0.42 + 0.1 * k as f32);
                    m.band(&head, (a0, a1), |a| s - 0.02 * (a - a0), |_| 0.006, here);
                }
            }
        }
        (Coat::Stripes, Part::Tail) => tail_rings(
            m,
            &Surface::of(TAIL),
            &[0.22, 0.36, 0.5, 0.62, 0.73, 0.83],
            here,
        ),
        (Coat::Rosettes, Part::Body) => rosettes(m, &Surface::of(BODY), here),
        (Coat::Rosettes, Part::Head) => {
            let head = Surface::of(HEAD);
            let len = head.len();
            for i in 0..4 {
                for j in 0..5 {
                    let s = len * (0.15 + 0.13 * i as f32 + 0.03 * (j % 2) as f32);
                    let a = 0.2 + (PI - 0.4) * j as f32 / 4.0;
                    m.spot(&head, s, a, 0.008, here);
                }
            }
        }
        (Coat::Rosettes, Part::Tail) => {
            let tail = Surface::of(TAIL);
            let len = tail.len();
            for i in 0..7 {
                for j in 0..3 {
                    let a = j as f32 / 3.0 * TAU + 0.6 * (i % 2) as f32;
                    m.spot(&tail, len * (0.12 + 0.08 * i as f32), a, 0.011, here);
                }
            }
            tail_rings(m, &tail, &[0.75, 0.84], here);
        }
        (Coat::Lynx, Part::Head) => {
            // a black tuft up from each ear's tip
            for side in [-1.0f32, 1.0] {
                let ear = place(
                    [side * 0.085, 0.1, -0.06],
                    [-8.0, side * -12.0, side * -14.0],
                    [1.0; 3],
                );
                let tip = ear * Vector3::new(0.0, 0.11, 0.0);
                let top = ear * Vector3::new(0.0, 0.21, 0.015);
                m.rod(tip, top, (0.01, 0.002), 6);
            }
        }
        (Coat::Lynx, Part::Tail) => {
            let bob = Surface::of(BOB);
            let len = bob.len();
            m.band(&bob, (0.0, TAU), |_| len * 0.85, |_| len * 0.16, here);
        }
        (Coat::Lynx, Part::Body) => {
            // a faint dappling down the flanks and haunches, scattered (a
            // golden-angle walk), more of it towards the back
            let body = Surface::of(BODY);
            let len = body.len();
            for k in 0..46 {
                let f = (k as f32 * 0.618_034).fract();
                let s = len * (0.08 + 0.7 * f * f.sqrt());
                let a = -0.3 + (PI + 0.6) * (k as f32 * 0.414_214).fract();
                let r = 0.006 + 0.004 * (k as f32 * 2.7).sin().abs();
                m.spot(&body, s, a, r, here);
            }
        }
    }
}

/// Stripes across a back and down its sides: `n` of them between the
/// fractions `span` of its length, each in two halves that meet a little
/// apart on the spine, slanting back as they go down and narrowing to
/// points at the belly.
fn stripes(m: &mut Merge, body: &Surface, n: usize, span: (f32, f32), xf: Transform3D) {
    let len = body.len();
    for k in 0..n {
        let s0 = len * (span.0 + (span.1 - span.0) * k as f32 / (n - 1) as f32);
        let jag = if k % 2 == 0 { 0.012 } else { -0.01 };
        for (a0, a1, off) in [
            (-0.5f32, FRAC_PI_2 - 0.04, jag),
            (FRAC_PI_2 + 0.04, PI + 0.5, -jag),
        ] {
            let mid = |a: f32| s0 + off - 0.045 * (1.0 - a.sin());
            let half = |a: f32| 0.005 + 0.014 * ((a - a0) / (a1 - a0) * PI).sin();
            m.band(body, (a0, a1), mid, half, xf);
        }
    }
}

/// Dark rings round a tail at the fractions `at` of its length, and its
/// tip black.
fn tail_rings(m: &mut Merge, tail: &Surface, at: &[f32], xf: Transform3D) {
    let len = tail.len();
    for &f in at {
        let wave = move |a: f32| len * f + 0.008 * (a * 2.0).sin();
        m.band(tail, (0.0, TAU), wave, |_| 0.011, xf);
    }
    m.band(tail, (0.0, TAU), |_| len * 0.95, |_| len * 0.05, xf);
}

/// A jaguar's rosettes over a back and sides: rings of five spots, in
/// staggered rows, none on the belly.
fn rosettes(m: &mut Merge, body: &Surface, xf: Transform3D) {
    let len = body.len();
    for i in 0..8 {
        let s = len * (0.09 + 0.105 * i as f32);
        for j in 0..6 {
            let a = -0.35 + (PI + 0.7) * (j as f32 + 0.5 * (i % 2) as f32) / 6.0;
            if a > PI + 0.35 {
                continue;
            }
            // a little off the grid, so the rows don't show
            let ds = 0.012 * ((i * 7 + j) as f32 * 2.3).sin();
            let da = 0.08 * ((i * 5 + j) as f32 * 1.7).cos();
            let r = 0.034 + 0.006 * ((i + j) % 3) as f32;
            let g = body.girth(s).max(0.01);
            for k in 0..5 {
                let t = k as f32 / 5.0 * TAU + 0.4 * i as f32;
                m.spot(
                    body,
                    s + ds + r * t.cos(),
                    a + da + r * t.sin() / g,
                    r * 0.36,
                    xf,
                );
            }
        }
    }
}
