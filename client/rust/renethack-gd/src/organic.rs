//! Smooth shapes for the creatures drawn in code (`creature_kit.rs`): a
//! body, a head, a tail or a limb lofted as one mesh through
//! cross-sections along a path, with normals of its own, so a creature
//! reads as flesh and fur instead of a stack of balls; and the many small
//! pieces of one material and one moving part (tufts on a mound, knobs on
//! a ball, a lizard's leg and its toes) merged into one mesh, so they draw
//! as one part instead of a node and a draw call each.
//!
//! A path lies in the creature's plane of symmetry (y up, z forward, x to
//! its side); each section is an ellipse (a superellipse, squarer as its
//! exponent grows) across it. Between the sections given, the loft runs
//! through smoothly (Catmull-Rom). A shape is one unit tall like the
//! bodies it is part of, and is cached by its name.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use godot::classes::mesh::PrimitiveType;
use godot::classes::{BoxMesh, Mesh, SurfaceTool};
use godot::prelude::*;

/// The shapes there are, by creature and part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Organic {
    /// A cat's body, from the root of its tail to its neck.
    CatBody,
    /// Its head and ears.
    CatHead,
    CatTail,
    /// A foreleg down from the shoulder (`CAT_SHOULDER`), its paw.
    CatForeleg,
    /// A hind leg down from the hip (`CAT_HIP`), the hock behind, its paw.
    CatHindleg,
    LizardBody,
    LizardTail,
    LizardHead,
    /// A lizard's leg out from the hip (`LIZARD_HIP` high), its toes
    /// fanned on the floor.
    LizardLeg {
        front: bool,
        right: bool,
    },
    /// An ant's head, middle, waist and gaster.
    AntBody,
    /// Its feelers and jaws.
    AntFeelers,
    /// The three legs of a side of an ant or a beetle, from where they
    /// join it (`legs_y`).
    Legs {
        beetle: bool,
        right: bool,
    },
    /// A beetle's wing cases, its middle and head.
    BeetleBody,
    /// The seam down its wing cases, its jaws.
    BeetleJaws,
    WormBody,
    /// The teeth round a worm's mouth.
    WormTeeth,
    /// A jelly and the two lobes slumped beside it.
    Blob,
    /// The bubbles caught in it.
    BlobBubbles,
    /// The veins over a floating eye's white (its ball `EYE_R` round).
    EyeVeins,
    /// The tendrils hanging under it.
    EyeTendrils,
    /// A lichen's rosette of crinkled lobes.
    LichenLobes,
    /// The little cups among them.
    LichenCups,
    /// A mold's mound of furry tufts.
    MoldMound,
    /// The spores glinting on it.
    MoldSpecks,
    /// A gas spore's knobbed ball.
    SporeBall,
    /// The threads trailing under it.
    SporeThreads,
}

/// Where a cat's legs join its body (out to the side, up, forward): the
/// shoulders and the hips. Its leg shapes reach from there to the floor.
pub const CAT_SHOULDER: [f32; 3] = [0.075, 0.47, 0.26];
pub const CAT_HIP: [f32; 3] = [0.085, 0.5, -0.27];

/// How high a lizard's legs join its body: low, the body slung between
/// them, its belly near the floor.
pub const LIZARD_HIP: f32 = 0.12;

/// How high an ant's middle is, where its legs join it; a beetle's.
pub const ANT_Y: f32 = 0.36;
pub const BEETLE_Y: f32 = 0.3;

/// The height the legs of an ant or a beetle join it at.
pub const fn legs_y(beetle: bool) -> f32 {
    if beetle { BEETLE_Y * 0.9 } else { ANT_Y }
}

/// A floating eye's ball: its radius.
pub const EYE_R: f32 = 0.42;

/// One cross-section: where its centre is on the path (y, z), its half
/// width across (x) and half height, and how square it is (2: an
/// ellipse).
#[derive(Debug, Clone, Copy)]
struct Section {
    y: f32,
    z: f32,
    rx: f32,
    ry: f32,
    n: f32,
}

const fn s(y: f32, z: f32, rx: f32, ry: f32) -> Section {
    Section {
        y,
        z,
        rx,
        ry,
        n: 2.0,
    }
}

/// Points around a section, and steps along the loft between two given
/// sections.
const AROUND: usize = 20;
const BETWEEN: usize = 4;

/// a lean back sloping from the shoulders, a deep chest, the
/// belly tucked up, round haunches
const CAT_BODY: &[Section] = &[
    s(0.57, -0.44, 0.02, 0.02),
    s(0.57, -0.41, 0.1, 0.11),
    s(0.56, -0.33, 0.16, 0.19),
    s(0.55, -0.22, 0.155, 0.18),
    s(0.55, -0.06, 0.14, 0.15),
    s(0.56, 0.1, 0.145, 0.17),
    s(0.57, 0.23, 0.15, 0.2),
    s(0.62, 0.32, 0.115, 0.15),
    s(0.67, 0.37, 0.075, 0.085),
    s(0.71, 0.4, 0.055, 0.055),
    s(0.73, 0.42, 0.02, 0.02),
];

/// the round skull, full cheeks, a short muzzle and a small chin
const CAT_HEAD: &[Section] = &[
    s(0.02, -0.11, 0.02, 0.02),
    s(0.02, -0.09, 0.08, 0.08),
    s(0.02, -0.04, 0.12, 0.11),
    s(0.00, 0.03, 0.13, 0.11),
    s(-0.03, 0.09, 0.10, 0.08),
    s(-0.04, 0.13, 0.065, 0.055),
    s(-0.04, 0.155, 0.035, 0.03),
    s(-0.04, 0.165, 0.01, 0.01),
];

/// carried high and curling back at the tip
const CAT_TAIL: &[Section] = &[
    s(0.0, 0.03, 0.012, 0.012),
    s(0.0, 0.0, 0.05, 0.05),
    s(0.08, -0.06, 0.045, 0.045),
    s(0.2, -0.12, 0.04, 0.04),
    s(0.33, -0.14, 0.035, 0.035),
    s(0.43, -0.11, 0.03, 0.03),
    s(0.48, -0.06, 0.025, 0.025),
    s(0.49, -0.04, 0.006, 0.006),
];

/// a straight foreleg: elbow, slim forearm, down to the wrist (from
/// the shoulder; z is forward; the paw is apart)
const CAT_FORELEG: &[Section] = &[
    s(0.02, 0.0, 0.012, 0.012),
    s(0.0, 0.0, 0.066, 0.078),
    s(-0.1, -0.005, 0.054, 0.06),
    s(-0.22, 0.0, 0.04, 0.044),
    s(-0.36, 0.005, 0.035, 0.037),
    s(-0.42, 0.01, 0.032, 0.032),
    s(-0.43, 0.01, 0.012, 0.012),
];

/// a thick thigh forward, the hock back, then down to the ankle
const CAT_HINDLEG: &[Section] = &[
    s(0.03, 0.0, 0.012, 0.012),
    s(0.0, 0.0, 0.095, 0.105),
    s(-0.09, 0.025, 0.08, 0.088),
    s(-0.18, 0.03, 0.06, 0.06),
    s(-0.27, -0.01, 0.042, 0.042),
    s(-0.34, -0.03, 0.035, 0.035),
    s(-0.42, -0.02, 0.032, 0.032),
    s(-0.44, -0.015, 0.012, 0.012),
];

/// a pointed ear, flat front to back
const CAT_EAR: &[Section] = &[
    s(0.0, 0.0, 0.058, 0.016),
    s(0.035, 0.0, 0.046, 0.014),
    s(0.07, 0.0, 0.026, 0.01),
    s(0.1, 0.0, 0.008, 0.005),
    s(0.11, 0.0, 0.002, 0.002),
];

/// a low, flat body: the hips, the belly wider than it is deep, the
/// shoulders and the neck
const LIZARD_BODY: &[Section] = &[
    s(0.12, -0.38, 0.02, 0.02),
    s(0.12, -0.34, 0.1, 0.07),
    s(0.14, -0.24, 0.17, 0.11),
    s(0.15, -0.06, 0.19, 0.12),
    s(0.15, 0.12, 0.17, 0.11),
    s(0.15, 0.26, 0.13, 0.095),
    s(0.15, 0.36, 0.085, 0.07),
    s(0.15, 0.42, 0.06, 0.055),
    s(0.15, 0.44, 0.02, 0.02),
];

/// as long again as the body, tapering to a point that drags
const LIZARD_TAIL: &[Section] = &[
    s(0.0, 0.04, 0.02, 0.02),
    s(0.0, 0.0, 0.1, 0.07),
    s(-0.015, -0.2, 0.075, 0.055),
    s(-0.05, -0.45, 0.05, 0.04),
    s(-0.09, -0.7, 0.03, 0.025),
    s(-0.12, -0.92, 0.014, 0.012),
    s(-0.13, -1.0, 0.004, 0.004),
];

/// a flat wedge: the skull, wide jaws, a blunt snout
const LIZARD_HEAD: &[Section] = &[
    s(0.0, -0.07, 0.02, 0.02),
    s(0.0, -0.05, 0.07, 0.055),
    s(0.005, 0.02, 0.085, 0.055),
    s(0.0, 0.1, 0.07, 0.042),
    s(-0.008, 0.17, 0.045, 0.028),
    s(-0.012, 0.21, 0.025, 0.017),
    s(-0.013, 0.225, 0.006, 0.006),
];

/// an ant's rear: the great oval gaster, its tip down
const ANT_GASTER: &[Section] = &[
    s(0.06, 0.02, 0.02, 0.02),
    s(0.06, 0.0, 0.1, 0.09),
    s(0.07, -0.1, 0.19, 0.17),
    s(0.06, -0.24, 0.2, 0.18),
    s(0.02, -0.38, 0.15, 0.13),
    s(-0.03, -0.46, 0.07, 0.06),
    s(-0.05, -0.49, 0.01, 0.01),
];

/// the narrow middle the legs come from, humped over the waist
const ANT_THORAX: &[Section] = &[
    s(0.0, -0.13, 0.02, 0.02),
    s(0.0, -0.12, 0.055, 0.05),
    s(0.02, -0.06, 0.07, 0.07),
    s(0.03, 0.02, 0.075, 0.08),
    s(0.01, 0.1, 0.06, 0.06),
    s(0.0, 0.14, 0.03, 0.03),
    s(0.0, 0.15, 0.01, 0.01),
];

/// a broad head, flat in front, the mandibles apart
const ANT_HEAD: &[Section] = &[
    s(0.0, -0.08, 0.02, 0.02),
    s(0.0, -0.07, 0.07, 0.06),
    s(0.01, -0.02, 0.11, 0.09),
    s(0.0, 0.05, 0.11, 0.085),
    s(-0.01, 0.1, 0.08, 0.06),
    s(-0.015, 0.12, 0.04, 0.03),
    s(-0.015, 0.125, 0.01, 0.01),
];

/// a beetle's wing cases: a long dome, squarer than an ellipse
const BEETLE_SHELL: &[Section] = &[
    Section {
        y: 0.0,
        z: -0.5,
        rx: 0.03,
        ry: 0.02,
        n: 2.5,
    },
    Section {
        y: 0.02,
        z: -0.46,
        rx: 0.2,
        ry: 0.15,
        n: 2.5,
    },
    Section {
        y: 0.04,
        z: -0.3,
        rx: 0.3,
        ry: 0.25,
        n: 2.6,
    },
    Section {
        y: 0.05,
        z: -0.08,
        rx: 0.32,
        ry: 0.27,
        n: 2.6,
    },
    Section {
        y: 0.04,
        z: 0.14,
        rx: 0.3,
        ry: 0.24,
        n: 2.6,
    },
    Section {
        y: 0.02,
        z: 0.26,
        rx: 0.24,
        ry: 0.17,
        n: 2.5,
    },
    Section {
        y: 0.0,
        z: 0.3,
        rx: 0.12,
        ry: 0.08,
        n: 2.3,
    },
    Section {
        y: 0.0,
        z: 0.31,
        rx: 0.02,
        ry: 0.02,
        n: 2.0,
    },
];

/// a long worm's body, from the tail end to the mouth: thick, its rings
/// standing out (every section another ring)
const WORM_BODY: &[Section] = &[
    s(0.088, -1.75, 0.035, 0.035),
    s(0.105, -1.68, 0.14, 0.123),
    s(0.175, -1.505, 0.21, 0.193),
    s(0.228, -1.365, 0.263, 0.245),
    s(0.298, -1.19, 0.298, 0.28),
    s(0.333, -1.05, 0.35, 0.333),
    s(0.385, -0.875, 0.367, 0.35),
    s(0.403, -0.735, 0.42, 0.403),
    s(0.438, -0.56, 0.42, 0.403),
    s(0.438, -0.42, 0.473, 0.455),
    s(0.455, -0.245, 0.455, 0.438),
    s(0.455, -0.105, 0.49, 0.473),
    s(0.473, 0.07, 0.473, 0.455),
    s(0.473, 0.21, 0.507, 0.49),
    s(0.49, 0.385, 0.473, 0.455),
    s(0.507, 0.525, 0.507, 0.49),
    s(0.542, 0.7, 0.455, 0.438),
    s(0.578, 0.84, 0.438, 0.42),
    s(0.63, 0.98, 0.385, 0.367),
    s(0.665, 1.05, 0.263, 0.245),
    s(0.682, 1.067, 0.035, 0.035),
];

/// a jelly settled on the floor: spread wide at its foot, sagging, its
/// top a soft dome (the path runs up its middle; z is its depth)
const BLOB_DOME: &[Section] = &[
    Section {
        y: 0.0,
        z: 0.0,
        rx: 0.648,
        ry: 0.567,
        n: 2.2,
    },
    Section {
        y: 0.081,
        z: 0.0,
        rx: 0.702,
        ry: 0.621,
        n: 2.2,
    },
    Section {
        y: 0.243,
        z: 0.014,
        rx: 0.675,
        ry: 0.608,
        n: 2.1,
    },
    Section {
        y: 0.432,
        z: 0.027,
        rx: 0.581,
        ry: 0.527,
        n: 2.0,
    },
    Section {
        y: 0.621,
        z: 0.027,
        rx: 0.432,
        ry: 0.392,
        n: 2.0,
    },
    Section {
        y: 0.756,
        z: 0.014,
        rx: 0.257,
        ry: 0.23,
        n: 2.0,
    },
    Section {
        y: 0.824,
        z: 0.0,
        rx: 0.081,
        ry: 0.068,
        n: 2.0,
    },
    Section {
        y: 0.837,
        z: 0.0,
        rx: 0.014,
        ry: 0.014,
        n: 2.0,
    },
];

impl Organic {
    pub fn build(self) -> Gd<Mesh> {
        self.merged().done()
    }

    /// Half its height (as `MeshKey::half_height`).
    pub fn half_height(self) -> f32 {
        let (lo, hi) = self.merged().span();
        (hi - lo) / 2.0
    }

    /// Its pieces, merged.
    fn merged(self) -> Merge {
        let mut m = Merge::default();
        let here = Transform3D::IDENTITY;
        match self {
            Organic::CatBody => m.loft(CAT_BODY, here),
            Organic::CatHead => {
                m.loft(CAT_HEAD, here);
                for s in [-1.0f32, 1.0] {
                    let ear = place(
                        [s * 0.068, 0.075, -0.035],
                        [-12.0, s * -14.0, s * -22.0],
                        [1.15; 3],
                    );
                    m.loft(CAT_EAR, ear);
                }
            }
            Organic::CatTail => m.loft(CAT_TAIL, here),
            Organic::CatForeleg => {
                m.loft(CAT_FORELEG, here);
                paw(&mut m, CAT_SHOULDER[1], 0.03);
            }
            Organic::CatHindleg => {
                m.loft(CAT_HINDLEG, here);
                paw(&mut m, CAT_HIP[1], 0.0);
            }
            Organic::LizardBody => m.loft(LIZARD_BODY, here),
            Organic::LizardTail => m.loft(LIZARD_TAIL, here),
            Organic::LizardHead => m.loft(LIZARD_HEAD, here),
            Organic::LizardLeg { front, right } => lizard_leg(&mut m, front, right),
            Organic::AntBody => ant_body(&mut m),
            Organic::AntFeelers => ant_feelers(&mut m),
            Organic::Legs { beetle, right } => legs(&mut m, beetle, right),
            Organic::BeetleBody => beetle_body(&mut m),
            Organic::BeetleJaws => beetle_jaws(&mut m),
            Organic::WormBody => m.loft(WORM_BODY, here),
            Organic::WormTeeth => worm_teeth(&mut m),
            Organic::Blob => blob(&mut m),
            Organic::BlobBubbles => {
                for (x, y, z, r) in [
                    (0.24, 0.54, 0.3, 0.047),
                    (-0.2, 0.4, 0.4, 0.034),
                    (0.07, 0.68, -0.135, 0.04),
                ] {
                    m.ball(
                        Vector3::new(x, y, z),
                        Vector3::splat(r),
                        [0.0; 3],
                        false,
                        10,
                    );
                }
            }
            Organic::EyeVeins => eye_veins(&mut m),
            Organic::EyeTendrils => eye_tendrils(&mut m),
            Organic::LichenLobes => lichen_lobes(&mut m),
            Organic::LichenCups => {
                for (x, z) in [
                    (0.1f32, 0.05f32),
                    (-0.12, 0.12),
                    (0.02, -0.15),
                    (0.2, -0.12),
                ] {
                    m.rod(
                        Vector3::new(x, 0.04, z),
                        Vector3::new(x, 0.08, z),
                        (0.018, 0.03),
                        10,
                    );
                }
            }
            Organic::MoldMound => mold_mound(&mut m),
            Organic::MoldSpecks => {
                for k in 0..10 {
                    let a = k as f32 * 1.7;
                    let r = 0.26 * ((k as f32 + 0.5) / 10.0).sqrt();
                    let at = Vector3::new(r * a.cos(), 0.24 - r * 0.4, r * a.sin() * 0.9);
                    m.ball(at, Vector3::splat(0.01), [0.0; 3], false, 6);
                }
            }
            Organic::SporeBall => spore_ball(&mut m),
            Organic::SporeThreads => {
                for k in 0..4 {
                    let a = (k as f32 / 4.0 * 360.0 + 45.0).to_radians();
                    let top = Vector3::new(0.12 * a.cos(), 0.18, 0.12 * a.sin());
                    let tip = top + Vector3::new(0.05 * a.cos(), -0.17, 0.05 * a.sin());
                    m.rod(top, tip, (0.012, 0.003), 6);
                }
            }
        }
        m
    }
}

/// Placed: at `pos`, turned (degrees, as a node's rotation), scaled.
fn place(pos: [f32; 3], turn: [f32; 3], scale: [f32; 3]) -> Transform3D {
    let scale = Basis::from_scale(Vector3::new(scale[0], scale[1], scale[2]));
    Transform3D::new(turned(turn) * scale, Vector3::new(pos[0], pos[1], pos[2]))
}

/// A rotation by degrees about x, y and z, in a node's order.
fn turned(deg: [f32; 3]) -> Basis {
    let rad = Vector3::new(deg[0], deg[1], deg[2]) * (PI / 180.0);
    Basis::from_euler(EulerOrder::YXZ, rad)
}

/// A cat's paw on the floor under a leg that joins the body `high` up.
fn paw(m: &mut Merge, high: f32, ahead: f32) {
    let at = Vector3::new(0.0, 0.025 - high, ahead);
    m.ball(at, Vector3::new(0.043, 0.024, 0.056), [0.0; 3], false, 14);
}

fn lizard_leg(m: &mut Merge, front: bool, right: bool) {
    let side = if right { 1.0 } else { -1.0 };
    // out and a little up to the elbow (the knee forward on a hind leg),
    // then a short drop to the foot
    let elbow = Vector3::new(side * 0.17, 0.02, if front { 0.0 } else { 0.05 });
    let foot = Vector3::new(
        side * 0.22,
        0.02 - LIZARD_HIP,
        if front { 0.05 } else { 0.0 },
    );
    m.rod(Vector3::ZERO, elbow, (0.05, 0.04), 12);
    m.ball(elbow, Vector3::splat(0.04), [0.0; 3], false, 12);
    m.rod(elbow, foot, (0.038, 0.03), 12);
    m.ball(foot, Vector3::new(0.041, 0.017, 0.041), [0.0; 3], false, 12);
    // four long toes fanned flat on the floor: forward and out on a front
    // foot, out and back on a hind one
    let ahead: f32 = if front { 30.0 } else { 95.0 };
    for (t, long) in [(-1.5f32, 0.05f32), (-0.5, 0.07), (0.5, 0.07), (1.5, 0.05)] {
        let a = (ahead + t * 30.0).to_radians();
        let tip = foot + Vector3::new(side * a.sin() * long, -0.012, a.cos() * long);
        m.rod(foot, tip, (0.014, 0.007), 8);
    }
}

fn ant_body(m: &mut Merge) {
    let y = ANT_Y;
    m.loft(ANT_THORAX, place([0.0, y, 0.0], [0.0; 3], [1.0; 3]));
    m.loft(
        ANT_GASTER,
        place([0.0, y - 0.02, -0.24], [0.0; 3], [1.0; 3]),
    );
    // the waist: two knots
    for (z, r) in [(-0.16f32, 0.04f32), (-0.2, 0.045)] {
        m.ball(
            Vector3::new(0.0, y + 0.01, z),
            Vector3::new(r, r * 1.2, r),
            [0.0; 3],
            false,
            12,
        );
    }
    m.loft(ANT_HEAD, place([0.0, y + 0.03, 0.23], [0.0; 3], [1.0; 3]));
}

fn ant_feelers(m: &mut Merge) {
    let y = ANT_Y;
    for s in [-1.0f32, 1.0] {
        // feelers: up from the brow, then forward from the elbow
        let base = Vector3::new(s * 0.04, y + 0.1, 0.31);
        let elbow = base + Vector3::new(s * 0.05, 0.16, 0.02);
        let tip = elbow + Vector3::new(s * 0.04, 0.02, 0.2);
        m.rod(base, elbow, (0.009, 0.008), 6);
        m.rod(elbow, tip, (0.008, 0.005), 6);
        // jaws closing in front
        let jaw = Vector3::new(s * 0.06, y - 0.01, 0.33);
        m.rod(
            jaw,
            jaw + Vector3::new(-s * 0.04, -0.01, 0.07),
            (0.018, 0.006),
            8,
        );
    }
}

/// Three legs on a side, between the front and back of the middle, each
/// up to a knee and down to a thin foot.
fn legs(m: &mut Merge, beetle: bool, right: bool) {
    let side = if right { 1.0 } else { -1.0 };
    let y = legs_y(beetle);
    let (z, reach) = if beetle {
        ((0.22, -0.18), 0.62)
    } else {
        ((0.12f32, -0.08f32), 0.55)
    };
    for i in 0..3 {
        let zi = z.0 + (z.1 - z.0) * i as f32 / 2.0;
        // the front legs reach forward, the back ones back
        let fan = (1.0 - i as f32) * 0.18;
        let hip = Vector3::new(side * 0.05, 0.0, zi);
        let knee = Vector3::new(side * reach * 0.5, 0.14, zi + fan * 0.6);
        let ankle = Vector3::new(side * reach * 0.82, -y * 0.7, zi + fan);
        let toe = Vector3::new(side * reach, -y + 0.01, zi + fan * 1.2);
        m.rod(hip, knee, (0.022, 0.016), 8);
        m.ball(knee, Vector3::splat(0.017), [0.0; 3], false, 8);
        m.rod(knee, ankle, (0.016, 0.01), 8);
        m.rod(ankle, toe, (0.009, 0.005), 6);
    }
}

fn beetle_body(m: &mut Merge) {
    let y = BEETLE_Y;
    m.loft(BEETLE_SHELL, place([0.0, y, 0.0], [0.0; 3], [1.0; 3]));
    m.ball(
        Vector3::new(0.0, y + 0.02, 0.36),
        Vector3::new(0.18, 0.105, 0.15),
        [0.0; 3],
        false,
        16,
    );
    m.ball(
        Vector3::new(0.0, y, 0.48),
        Vector3::new(0.1, 0.075, 0.09),
        [0.0; 3],
        false,
        14,
    );
}

fn beetle_jaws(m: &mut Merge) {
    let y = BEETLE_Y;
    // the seam down the middle of the wing cases, along their top
    let top: Vec<Vector3> = smooth(BEETLE_SHELL)
        .iter()
        .filter(|r| r.ry > 0.12)
        .map(|r| Vector3::new(0.0, y + r.y + r.ry + 0.002, r.z))
        .collect();
    for pair in top.windows(2) {
        m.rod(pair[0], pair[1], (0.007, 0.007), 6);
    }
    for s in [-1.0f32, 1.0] {
        let jaw = Vector3::new(s * 0.05, y - 0.02, 0.55);
        m.rod(
            jaw,
            jaw + Vector3::new(-s * 0.05, 0.0, 0.1),
            (0.022, 0.008),
            8,
        );
    }
}

/// A worm's mouth: its centre (y, z) and radius.
pub const WORM_MOUTH: (f32, f32, f32) = (0.68, 1.05, 0.21);

fn worm_teeth(m: &mut Merge) {
    let (y, z, r) = WORM_MOUTH;
    for k in 0..8 {
        let a = (k as f32 * 45.0).to_radians();
        let at = Vector3::new(0.8 * r * a.cos(), y + 0.8 * r * a.sin(), z);
        let tip = Vector3::new(0.4 * r * a.cos(), y + 0.4 * r * a.sin(), z - 0.035);
        m.rod(at, tip, (0.028, 0.005), 6);
    }
}

fn blob(m: &mut Merge) {
    m.loft(BLOB_DOME, Transform3D::IDENTITY);
    // a lobe slumped to one side, another behind
    m.loft(
        BLOB_DOME,
        place([0.43, 0.0, 0.24], [0.0, 30.0, 0.0], [0.45, 0.5, 0.45]),
    );
    m.loft(
        BLOB_DOME,
        place([-0.38, 0.0, -0.3], [0.0, -20.0, 0.0], [0.5, 0.42, 0.5]),
    );
}

/// From the iris back over the ball, bending gently, in short runs so the
/// straight pieces keep to the curve; every other one forks.
fn eye_veins(m: &mut Merge) {
    for k in 0..8 {
        let a = k as f32 / 8.0 * TAU + 0.3;
        let bend = if k % 2 == 0 { 0.07 } else { -0.06 };
        let at = |polar: f32, off: f32| {
            let p = polar.to_radians();
            let q = a + off + bend * ((polar - 48.0) / 40.0 * 3.0).sin();
            Vector3::new(p.sin() * q.cos(), p.sin() * q.sin(), p.cos()) * (EYE_R + 0.002)
        };
        let run: Vec<Vector3> = (0..6).map(|i| at(48.0 + i as f32 * 8.0, 0.0)).collect();
        for pair in run.windows(2) {
            m.rod(pair[0], pair[1], (0.006, 0.006), 5);
        }
        if k % 2 == 0 {
            let fork: Vec<Vector3> = (0..3)
                .map(|i| at(56.0 + i as f32 * 8.0, i as f32 * 0.1))
                .collect();
            for pair in fork.windows(2) {
                m.rod(pair[0], pair[1], (0.0045, 0.0045), 5);
            }
        }
    }
}

fn eye_tendrils(m: &mut Merge) {
    for k in 0..3 {
        let a = (k as f32 / 3.0 * 360.0 + 90.0).to_radians();
        let top = Vector3::new(0.12 * a.cos(), 0.16, 0.12 * a.sin());
        let mid = top + Vector3::new(0.04 * a.cos(), -0.14, 0.04 * a.sin());
        let tip = mid + Vector3::new(-0.02 * a.cos(), -0.12, 0.03);
        m.rod(top, mid, (0.03, 0.018), 8);
        m.rod(mid, tip, (0.018, 0.004), 8);
    }
}

/// Three rings of lobes round a middle one, each a flattened cap tipped
/// out.
fn lichen_lobes(m: &mut Merge) {
    for ring in 0..3 {
        let n = 5 + ring * 3;
        let r = 0.12 + ring as f32 * 0.14;
        for k in 0..n {
            let a = (k as f32 / n as f32 + ring as f32 * 0.13) * TAU;
            let size = 0.17 - ring as f32 * 0.03;
            let half = Vector3::new(1.0, 0.45 + 0.12 * (k % 2) as f32, 0.7) * size;
            let turn = [8.0 * a.sin(), -a.to_degrees(), 14.0];
            m.ball(
                Vector3::new(r * a.cos(), 0.0, r * a.sin()),
                half,
                turn,
                true,
                12,
            );
        }
    }
    m.ball(
        Vector3::ZERO,
        Vector3::new(0.16, 0.1, 0.16),
        [0.0; 3],
        true,
        14,
    );
}

/// A low mound, tufts in a spiral over it.
fn mold_mound(m: &mut Merge) {
    m.ball(
        Vector3::ZERO,
        Vector3::new(0.4, 0.22, 0.36),
        [0.0; 3],
        true,
        18,
    );
    for k in 0..26 {
        let t = k as f32 / 26.0;
        let a = k as f32 * 2.4;
        let r = 0.36 * t.sqrt();
        let h = 0.22 * (1.0 - t * t).max(0.0);
        let size = 0.1 - 0.04 * t;
        let at = Vector3::new(r * a.cos(), h + 0.02, r * a.sin() * 0.9);
        m.ball(
            at,
            Vector3::new(size, size * 0.8, size),
            [0.0; 3],
            false,
            10,
        );
    }
}

/// The ball, knobs spread evenly over it (a golden-angle spiral).
fn spore_ball(m: &mut Merge) {
    m.ball(
        Vector3::new(0.0, 0.5, 0.0),
        Vector3::splat(0.36),
        [0.0; 3],
        false,
        20,
    );
    for k in 0..40 {
        let t = (k as f32 + 0.5) / 40.0;
        let y = 1.0 - 2.0 * t;
        let r = (1.0 - y * y).sqrt();
        let a = k as f32 * 2.39996;
        let p = Vector3::new(r * a.cos(), y, r * a.sin()) * 0.35;
        m.ball(
            Vector3::new(p.x, 0.5 + p.y, p.z),
            Vector3::splat(0.045),
            [0.0; 3],
            false,
            8,
        );
    }
}

/// Catmull-Rom between `b` and `c` (with `a` before and `d` after) at `t`.
fn spline(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * b
        + (c - a) * t
        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
        + (3.0 * b - a - 3.0 * c + d) * t3)
}

/// The sections given and those between them.
fn smooth(given: &[Section]) -> Vec<Section> {
    let n = given.len();
    let at = |i: isize| given[i.clamp(0, n as isize - 1) as usize];
    let mut out = Vec::new();
    for i in 0..n.saturating_sub(1) {
        let (a, b, c, d) = (
            at(i as isize - 1),
            at(i as isize),
            at(i as isize + 1),
            at(i as isize + 2),
        );
        for k in 0..BETWEEN {
            let t = k as f32 / BETWEEN as f32;
            let f = |g: fn(&Section) -> f32| spline(g(&a), g(&b), g(&c), g(&d), t);
            out.push(Section {
                y: f(|c| c.y),
                z: f(|c| c.z),
                // never through zero between two thin ends
                rx: f(|c| c.rx).max(0.002),
                ry: f(|c| c.ry).max(0.002),
                n: b.n + (c.n - b.n) * t,
            });
        }
    }
    if let Some(last) = given.last() {
        out.push(*last);
    }
    out
}

/// A point of a superellipse of exponent `n` at angle `a`: (x, y).
fn around(a: f32, rx: f32, ry: f32, n: f32) -> (f32, f32) {
    let (c, s) = (a.cos(), a.sin());
    let e = 2.0 / n;
    (
        rx * c.signum() * c.abs().powf(e),
        ry * s.signum() * s.abs().powf(e),
    )
}

/// A mesh being built, its pieces added one by one: the vertices (each
/// with its normal and texture coordinates) and the triangles.
#[derive(Default)]
struct Merge {
    pos: Vec<Vector3>,
    nrm: Vec<Vector3>,
    uv: Vec<Vector2>,
    idx: Vec<i32>,
}

impl Merge {
    fn vertex(&mut self, p: Vector3, n: Vector3, uv: Vector2) -> i32 {
        self.pos.push(p);
        self.nrm.push(n);
        self.uv.push(uv);
        self.pos.len() as i32 - 1
    }

    /// A triangle, wound to face the way its corners' normals do (Godot's
    /// front faces wind clockwise seen from outside); a sliver at a pole
    /// either way.
    fn tri(&mut self, a: i32, b: i32, c: i32) {
        let p = |i: i32| self.pos[i as usize];
        let n = self.nrm[a as usize] + self.nrm[b as usize] + self.nrm[c as usize];
        if (p(b) - p(a)).cross(p(c) - p(a)).dot(n) > 0.0 {
            self.idx.extend([a, c, b]);
        } else {
            self.idx.extend([a, b, c]);
        }
    }

    /// A surface of `rows` rings of `sides` points, `at(ring, point)`
    /// giving each point and its normal: the rings joined in quads, each
    /// closed round (its first point again last, for the texture).
    fn rings(
        &mut self,
        rows: usize,
        sides: usize,
        at: impl Fn(usize, usize) -> (Vector3, Vector3),
    ) {
        let base = self.pos.len() as i32;
        let along = rows.max(2) - 1;
        for i in 0..rows {
            for j in 0..=sides {
                let (p, n) = at(i, j % sides);
                let uv = Vector2::new(j as f32 / sides as f32, i as f32 / along as f32);
                self.vertex(p, n, uv);
            }
        }
        let w = sides as i32 + 1;
        for i in 0..rows.saturating_sub(1) as i32 {
            for j in 0..sides as i32 {
                let a = base + i * w + j;
                self.tri(a, a + 1, a + w);
                self.tri(a + 1, a + w + 1, a + w);
            }
        }
    }

    /// An ellipsoid of half sizes `half` round `at`, turned (degrees, as a
    /// node's rotation); only its top half for a `cap` (open, its flat
    /// side at `at`). `sides` points round it.
    fn ball(&mut self, at: Vector3, half: Vector3, turn: [f32; 3], cap: bool, sides: usize) {
        let r = turned(turn);
        let (rows, top) = if cap {
            (sides / 4 + 2, FRAC_PI_2)
        } else {
            (sides / 2 + 1, PI)
        };
        self.rings(rows, sides, |i, j| {
            let v = top * i as f32 / (rows - 1) as f32;
            let u = TAU * j as f32 / sides as f32;
            let d = Vector3::new(v.sin() * u.cos(), v.cos(), v.sin() * u.sin());
            // the normal of the stretched ball: its direction shrunk
            // where the ball is stretched
            (at + r * (d * half), (r * (d / half)).normalized())
        });
    }

    /// A rod from `a` to `b`, its radius `r.0` at `a` and `r.1` at `b`,
    /// closed at both ends.
    fn rod(&mut self, a: Vector3, b: Vector3, r: (f32, f32), sides: usize) {
        let len = (b - a).length();
        if len < 1e-5 {
            return;
        }
        let axis = (b - a) / len;
        let other = if axis.y.abs() < 0.9 {
            Vector3::UP
        } else {
            Vector3::RIGHT
        };
        let u = axis.cross(other).normalized();
        let w = axis.cross(u);
        let round = |j: usize| {
            let t = TAU * j as f32 / sides as f32;
            u * t.cos() + w * t.sin()
        };
        // the side leans in as the rod narrows
        let lean = (r.0 - r.1) / len;
        self.rings(2, sides, |i, j| {
            let (c, radius) = if i == 0 { (a, r.0) } else { (b, r.1) };
            (c + round(j) * radius, (round(j) + axis * lean).normalized())
        });
        for (c, radius, n) in [(a, r.0, -axis), (b, r.1, axis)] {
            let mid = self.vertex(c, n, Vector2::new(0.5, 0.5));
            for j in 0..sides {
                self.vertex(c + round(j) * radius, n, Vector2::new(0.5, 0.5));
            }
            for j in 0..sides as i32 {
                let next = (j + 1) % sides as i32;
                self.tri(mid, mid + 1 + j, mid + 1 + next);
            }
        }
    }

    /// The loft through `given`, placed by `xf`: a grid of `AROUND` points
    /// by the sections, its normals the cross of the grid's directions.
    fn loft(&mut self, given: &[Section], xf: Transform3D) {
        let rings = smooth(given);
        let m = rings.len();
        if m < 2 {
            return;
        }
        // each section's frame: along the path, the side (x), and up
        let along = |i: usize| {
            let (a, b) = (rings[i.saturating_sub(1)], rings[(i + 1).min(m - 1)]);
            Vector2::new(b.y - a.y, b.z - a.z).normalized()
        };
        let mut pos = vec![Vector3::ZERO; m * AROUND];
        for (i, r) in rings.iter().enumerate() {
            let t = along(i);
            // up across the path, in its plane: the tangent turned a quarter
            let up = Vector2::new(t.y, -t.x);
            // keep "up" pointing up on a path that runs forward
            let up = if up.x < 0.0 && t.y.abs() > 0.5 {
                -up
            } else {
                up
            };
            for j in 0..AROUND {
                let a = j as f32 / AROUND as f32 * TAU;
                let (x, h) = around(a, r.rx, r.ry, r.n);
                pos[i * AROUND + j] = Vector3::new(x, r.y + up.x * h, r.z + up.y * h);
            }
        }
        let at = |i: usize, j: usize| pos[i * AROUND + j % AROUND];
        let centre = |i: usize| Vector3::new(0.0, rings[i].y, rings[i].z);
        let normal = |i: usize, j: usize| {
            let p = at(i, j);
            let du = at(i, j + 1) - at(i, j + AROUND - 1);
            let dv = at((i + 1).min(m - 1), j) - at(i.saturating_sub(1), j);
            let n = du.cross(dv);
            // out from the path whichever way the grid winds
            let n = if n.dot(p - centre(i)) < 0.0 { -n } else { n };
            if n.length_squared() > 1e-12 {
                n.normalized()
            } else {
                (p - centre(i)).normalized()
            }
        };
        // normals turn with the shape, not stretched with it
        let turn = xf.basis.inverse().transposed();
        self.rings(m, AROUND, |i, j| {
            (xf * at(i, j), (turn * normal(i, j)).normalized())
        });
    }

    /// The lowest and the highest of it (y).
    fn span(&self) -> (f32, f32) {
        self.pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        })
    }

    fn done(self) -> Gd<Mesh> {
        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        for k in 0..self.pos.len() {
            st.set_normal(self.nrm[k]);
            st.set_uv(self.uv[k]);
            st.add_vertex(self.pos[k]);
        }
        for i in self.idx {
            st.add_index(i);
        }
        st.generate_tangents();
        match st.commit() {
            Some(mesh) => mesh.upcast(),
            None => BoxMesh::new_gd().upcast(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[Organic] = &[
        Organic::CatBody,
        Organic::CatHead,
        Organic::CatTail,
        Organic::CatForeleg,
        Organic::CatHindleg,
        Organic::LizardBody,
        Organic::LizardTail,
        Organic::LizardHead,
        Organic::LizardLeg {
            front: true,
            right: false,
        },
        Organic::LizardLeg {
            front: false,
            right: true,
        },
        Organic::AntBody,
        Organic::AntFeelers,
        Organic::Legs {
            beetle: false,
            right: false,
        },
        Organic::Legs {
            beetle: true,
            right: true,
        },
        Organic::BeetleBody,
        Organic::BeetleJaws,
        Organic::WormBody,
        Organic::WormTeeth,
        Organic::Blob,
        Organic::BlobBubbles,
        Organic::EyeVeins,
        Organic::EyeTendrils,
        Organic::LichenLobes,
        Organic::LichenCups,
        Organic::MoldMound,
        Organic::MoldSpecks,
        Organic::SporeBall,
        Organic::SporeThreads,
    ];

    #[test]
    fn the_loft_runs_through_every_section_given() {
        let rings = smooth(CAT_BODY);
        assert_eq!(rings.len(), (CAT_BODY.len() - 1) * BETWEEN + 1);
        for (k, g) in CAT_BODY.iter().enumerate() {
            let r = rings[k * BETWEEN];
            assert!((r.y - g.y).abs() < 1e-5 && (r.z - g.z).abs() < 1e-5);
        }
        assert!(rings.iter().all(|r| r.rx > 0.0 && r.ry > 0.0));
    }

    #[test]
    fn an_ellipse_section_reaches_its_half_sizes() {
        let (x, _) = around(0.0, 0.3, 0.2, 2.0);
        let (_, y) = around(FRAC_PI_2, 0.3, 0.2, 2.0);
        assert!((x - 0.3).abs() < 1e-5 && (y - 0.2).abs() < 1e-5);
    }

    #[test]
    fn every_shape_faces_out() {
        for &o in ALL {
            let m = o.merged();
            assert!(!m.idx.is_empty() && m.idx.len() % 3 == 0, "{o:?}");
            assert!(m.idx.iter().all(|&i| (i as usize) < m.pos.len()), "{o:?}");
            assert!(
                m.nrm.iter().all(|n| (n.length() - 1.0).abs() < 1e-3),
                "{o:?}"
            );
            for t in m.idx.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
                let face = (m.pos[b] - m.pos[a]).cross(m.pos[c] - m.pos[a]);
                // clockwise from outside: the cross points in
                assert!(face.dot(m.nrm[a] + m.nrm[b] + m.nrm[c]) <= 1e-9, "{o:?}");
            }
        }
    }

    #[test]
    fn a_stretched_ball_keeps_its_normals_true() {
        let mut m = Merge::default();
        m.ball(
            Vector3::ZERO,
            Vector3::new(0.4, 0.1, 0.2),
            [0.0, 30.0, 0.0],
            false,
            12,
        );
        // the normal of an ellipsoid (turned) is its point divided by the
        // squares of its half sizes
        let back = turned([0.0, 30.0, 0.0]).inverse();
        for (p, n) in m.pos.iter().zip(&m.nrm) {
            let q = back * *p;
            let want =
                (back.inverse() * Vector3::new(q.x / 0.16, q.y / 0.01, q.z / 0.04)).normalized();
            assert!(n.dot(want) > 0.999, "{p:?} {n:?} {want:?}");
        }
    }

    #[test]
    fn the_creatures_stand_on_the_floor() {
        // a cat's paws, a lizard's toes, an ant's and a beetle's feet,
        // from where the legs join the body
        let lowest = |o: Organic| o.merged().span().0;
        for (o, high) in [
            (Organic::CatForeleg, CAT_SHOULDER[1]),
            (Organic::CatHindleg, CAT_HIP[1]),
            (
                Organic::LizardLeg {
                    front: true,
                    right: true,
                },
                LIZARD_HIP,
            ),
            (
                Organic::LizardLeg {
                    front: false,
                    right: false,
                },
                LIZARD_HIP,
            ),
            (
                Organic::Legs {
                    beetle: false,
                    right: true,
                },
                legs_y(false),
            ),
            (
                Organic::Legs {
                    beetle: true,
                    right: false,
                },
                legs_y(true),
            ),
        ] {
            let floor = high + lowest(o);
            assert!(floor.abs() < 0.012, "{o:?} {floor}");
        }
        // the cat's back at the shoulders, its head above it
        let top = Organic::CatBody.merged().span().1;
        assert!(top > 0.6 && top < 0.8, "{top}");
        assert!(Organic::LizardBody.merged().span().0 > 0.0);
    }
}
