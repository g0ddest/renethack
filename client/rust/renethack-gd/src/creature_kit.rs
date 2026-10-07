//! The creatures drawn in code that a game meets most (the sightings of
//! a soak: the kitten first), put together from the smooth shapes of
//! `organic.rs` instead of stacked primitives: a shape for each material
//! of each moving part, so a creature is a handful of nodes. Like the rest
//! of the kit, a body is one unit tall, stands on y = 0, faces +z and
//! names the pivots its animations move (Head, Tail, the legs).

use super::*;
use crate::organic::{self, Organic};

const ONE: [f32; 3] = [1.0; 3];
const FLAT: [f32; 3] = [0.0; 3];

/// A creature's legs by their pivots: front left, front right, back left,
/// back right (the walk swings them in pairs).
pub(super) const LEGS: [&str; 4] = ["LegFL", "LegFR", "LegBL", "LegBR"];

fn shape(o: Organic) -> MeshKey {
    MeshKey::Organic(o)
}

impl Kit<'_> {
    /// A cat: a lean body, a round head with pointed ears and eyes that
    /// catch the light, slim legs with a hock behind, a long tail carried
    /// high.
    pub(super) fn cat_body(&mut self, root: &mut Gd<Node3D>) {
        // (the pivots are the root's own: the animations name them so)
        self.skin(root, shape(Organic::CatBody), FLAT, FLAT, ONE);
        let mut head = self.pivot(root, "Head", [0.0, 0.75, 0.44]);
        // a kitten's big head: it reads as a cat from the camera's height
        head.set_scale(Vector3::splat(1.3));
        self.skin(&mut head, shape(Organic::CatHead), FLAT, FLAT, ONE);
        self.eyes(&mut head, [0.0, 0.028, 0.104], 0.052, 0.02, true);
        let nose = self
            .art
            .flat(Color::from_rgb(0.62, 0.36, 0.36), Finish::Matte);
        self.part(
            &mut head,
            sphere(0.014),
            &nose,
            [0.0, -0.03, 0.163],
            FLAT,
            [1.3, 0.8, 1.0],
        );
        let mut tail = self.pivot(root, "Tail", [0.0, 0.58, -0.42]);
        self.skin(&mut tail, shape(Organic::CatTail), FLAT, FLAT, ONE);
        for (i, name) in LEGS.into_iter().enumerate() {
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let (leg, at) = if i < 2 {
                (Organic::CatForeleg, organic::CAT_SHOULDER)
            } else {
                (Organic::CatHindleg, organic::CAT_HIP)
            };
            let mut pivot = self.pivot(root, name, [side * at[0], at[1], at[2]]);
            self.skin(&mut pivot, shape(leg), FLAT, FLAT, ONE);
        }
    }

    /// A newt, a gecko, a crocodile: a low flat body slung between legs
    /// out to the sides and bent at the elbow, a wedge head, a long tail
    /// that drags.
    pub(super) fn lizard_body(&mut self, root: &mut Gd<Node3D>) {
        self.skin(root, shape(Organic::LizardBody), FLAT, FLAT, ONE);
        let mut head = self.pivot(root, "Head", [0.0, 0.155, 0.44]);
        self.skin(&mut head, shape(Organic::LizardHead), FLAT, FLAT, ONE);
        self.eyes(&mut head, [0.0, 0.045, 0.06], 0.062, 0.022, false);
        let mut tail = self.pivot(root, "Tail", [0.0, 0.13, -0.38]);
        self.skin(&mut tail, shape(Organic::LizardTail), FLAT, FLAT, ONE);
        for (i, name) in LEGS.into_iter().enumerate() {
            let (front, right) = (i < 2, i % 2 == 1);
            let x = if right { 0.14 } else { -0.14 };
            let z = if front { 0.26 } else { -0.24 };
            let mut leg = self.pivot(root, name, [x, organic::LIZARD_HIP, z]);
            self.skin(
                &mut leg,
                shape(Organic::LizardLeg { front, right }),
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    /// An ant: head, humped middle, a waist of two knots, the great
    /// gaster; six jointed legs from the middle, elbowed feelers, jaws.
    pub(super) fn ant_body(&mut self, root: &mut Gd<Node3D>, grid: bool) {
        let y = organic::ANT_Y;
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::AntBody), FLAT, FLAT, ONE);
        self.eyes(&mut body, [0.0, y + 0.07, 0.29], 0.085, 0.03, grid);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::AntFeelers),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
        self.six_legs(root, false);
    }

    /// Six legs, three a side, in the groups the walk swings (LegsL,
    /// LegsR).
    fn six_legs(&mut self, root: &mut Gd<Node3D>, beetle: bool) {
        let dark = self.dark();
        for (right, name) in [(false, "LegsL"), (true, "LegsR")] {
            let mut group = self.pivot(root, name, [0.0, organic::legs_y(beetle), 0.0]);
            self.part(
                &mut group,
                shape(Organic::Legs { beetle, right }),
                &dark,
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    /// A beetle: domed wing cases with their seam, a small head with
    /// jaws, six legs under the shell.
    pub(super) fn beetle_body(&mut self, root: &mut Gd<Node3D>) {
        let y = organic::BEETLE_Y;
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::BeetleBody), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::BeetleJaws),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
        self.eyes(&mut body, [0.0, y + 0.04, 0.53], 0.075, 0.025, false);
        self.six_legs(root, true);
    }

    /// A long worm: one thick ringed body raised at the front, its round
    /// mouth set with teeth.
    pub(super) fn worm_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::WormBody), FLAT, FLAT, ONE);
        let dark = self.dark();
        let (y, z, r) = organic::WORM_MOUTH;
        self.part(
            &mut body,
            cylinder(r, r, 0.035),
            &dark,
            [0.0, y, z],
            [90.0, 0.0, 0.0],
            ONE,
        );
        let bone = self.bone();
        self.part(&mut body, shape(Organic::WormTeeth), &bone, FLAT, FLAT, ONE);
    }

    /// A lichen: a flat rosette of crinkled lobes on the floor, little cups
    /// among them.
    pub(super) fn lichen(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::LichenLobes), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::LichenCups),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A mold: a soft mound of furry tufts, spores like dust on them.
    pub(super) fn mold(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::MoldMound), FLAT, FLAT, ONE);
        let glint = self.glint();
        self.part(
            &mut body,
            shape(Organic::MoldSpecks),
            &glint,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A blob, a jelly, a pudding: a sagging dome of slime, a darker
    /// heart inside, bubbles caught in it.
    pub(super) fn blob_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::Blob), FLAT, FLAT, ONE);
        let heart = self.dark();
        self.part(
            &mut body,
            sphere(0.22),
            &heart,
            [0.05, 0.35, 0.05],
            FLAT,
            [1.0, 0.8, 1.0],
        );
        let glass = self
            .art
            .flat(Color::from_rgba(1.0, 1.0, 1.0, 0.5), Finish::Glass);
        self.part(
            &mut body,
            shape(Organic::BlobBubbles),
            &glass,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A floating eye: a great eyeball turned up to look at whoever looks
    /// down at it (the camera sees its iris whichever way it faces), the
    /// iris wide and its colour, red veins on the white, three tendrils
    /// hanging below.
    pub(super) fn eye_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        let mut ball = self.pivot(&mut body, "Ball", [0.0, 0.5, 0.0]);
        ball.set_rotation_degrees(Vector3::new(-55.0, 0.0, 0.0));
        let white = self
            .art
            .flat(Color::from_rgb(0.86, 0.8, 0.74), Finish::Glossy);
        self.part(&mut ball, sphere(organic::EYE_R), &white, FLAT, FLAT, ONE);
        // the iris rises from the white like a cornea, the pupil from it
        let iris = self.art.flat(self.tint, Finish::Glossy);
        self.part(
            &mut ball,
            dome(0.3),
            &iris,
            [0.0, 0.0, 0.294],
            [90.0, 0.0, 0.0],
            [1.0, 0.47, 1.0],
        );
        let pupil = self.eye();
        self.part(
            &mut ball,
            dome(0.13),
            &pupil,
            [0.0, 0.0, 0.405],
            [90.0, 0.0, 0.0],
            [1.0, 0.31, 1.0],
        );
        let glint = self.art.flat(Color::from_rgb(1.0, 1.0, 1.0), Finish::Glow);
        self.part(
            &mut ball,
            sphere(0.03),
            &glint,
            [0.09, 0.1, 0.43],
            FLAT,
            ONE,
        );
        let vein = self
            .art
            .flat(Color::from_rgb(0.62, 0.1, 0.08), Finish::Matte);
        self.part(&mut ball, shape(Organic::EyeVeins), &vein, FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::EyeTendrils),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A gas spore: a grey puffball floating, covered in knobs, thin
    /// threads trailing.
    pub(super) fn spore_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::SporeBall), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::SporeThreads),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }
}

/// Seconds a walking body takes for one step of each leg.
pub(super) const WALK_PERIOD: f32 = 0.6;

/// The walk of a body built here: its legs swung in pairs, the tail
/// swaying; none for the bodies that hop (or have no legs).
pub(super) fn walk_waves(kind: Proc, shape: Option<&str>) -> Vec<Wave> {
    let x = Vector3::RIGHT;
    let y = Vector3::UP;
    match (kind, shape) {
        // a trot: the legs swing fore and aft, a diagonal pair together
        (Proc::Beast, Some("cat")) => vec![
            Wave::Rock(LEGS[0], x, 26.0, 0.0),
            Wave::Rock(LEGS[3], x, 26.0, 0.0),
            Wave::Rock(LEGS[1], x, 26.0, 0.5),
            Wave::Rock(LEGS[2], x, 26.0, 0.5),
            Wave::Rock("Tail", y, 8.0, 0.25),
        ],
        // a sprawled leg swings round its hip, its foot along the floor;
        // turned the same way, a left foot goes forward as a right one
        // goes back, so a diagonal pair turns opposite ways
        (Proc::Lizard, _) => vec![
            Wave::Rock(LEGS[0], y, 28.0, 0.0),
            Wave::Rock(LEGS[1], y, 28.0, 0.0),
            Wave::Rock(LEGS[2], y, 28.0, 0.5),
            Wave::Rock(LEGS[3], y, 28.0, 0.5),
            Wave::Rock("Head", y, 6.0, 0.5),
            Wave::Rock("Tail", y, 16.0, 0.25),
        ],
        (Proc::Bug, _) => vec![
            Wave::Rock("LegsL", y, 16.0, 0.0),
            Wave::Rock("LegsR", y, 16.0, 0.5),
            Wave::Bob("Body", Vector3::ZERO, 0.012, 0.0),
        ],
        (Proc::Worm, _) => vec![Wave::Pulse("Body", 0.05, 0.0)],
        (Proc::Blob, _) => vec![Wave::Pulse("Body", 0.09, 0.0)],
        _ => Vec::new(),
    }
}
