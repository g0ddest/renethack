//! The creatures of the water, drawn in code from `organic_sea.rs`'s
//! shapes: a piranha, a shark (and any other fish), the eels, a jellyfish
//! and a kraken. Each is a few parts on the pivots its animations move
//! (the root's own, or a path down to them); in water it floats down to
//! its waterline (the model's `swim`), its back and fins above it.

use super::*;
use crate::organic::Organic;
use crate::organic::sea::{self, Eel, Fish, Part, Quarter};

const ONE: [f32; 3] = [1.0; 3];
const FLAT: [f32; 3] = [0.0; 3];

fn shape(p: Part) -> MeshKey {
    MeshKey::Organic(Organic::Sea(p))
}

/// The sea creatures by their model's shape in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sea {
    Fish(Fish),
    /// A giant eel, or an electric one (its spots glowing).
    Eel {
        electric: bool,
    },
    Jellyfish,
    Kraken,
}

impl Sea {
    pub(super) fn of(kind: Proc, shape: Option<&str>) -> Option<Sea> {
        Some(match (kind, shape) {
            (Proc::Fish, Some("piranha")) => Sea::Fish(Fish::Piranha),
            (Proc::Fish, Some("shark")) => Sea::Fish(Fish::Shark),
            (Proc::Worm, Some("eel")) => Sea::Eel { electric: false },
            (Proc::Worm, Some("electric")) => Sea::Eel { electric: true },
            (Proc::Fish, _) => Sea::Fish(Fish::Plain),
            (Proc::Blob, Some("jelly")) => Sea::Jellyfish,
            (Proc::Blob, Some("kraken")) => Sea::Kraken,
            _ => return None,
        })
    }
}

impl Kit<'_> {
    pub(super) fn sea(&mut self, root: &mut Gd<Node3D>, sea: Sea) {
        match sea {
            Sea::Fish(fish) => self.fish_body(root, fish),
            Sea::Eel { electric } => self.eel(root, electric),
            Sea::Jellyfish => self.jellyfish(root),
            Sea::Kraken => self.kraken(root),
        }
    }

    /// A fish: its body (the Body pivot bobs), its fins darker, its tail
    /// fin on the Tail pivot (it beats, and bobs with the body), its eyes;
    /// a piranha's teeth, a shark's gill slits.
    fn fish_body(&mut self, root: &mut Gd<Node3D>, fish: Fish) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Part::FishBody(fish)), FLAT, FLAT, ONE);
        let fin = self.dark();
        self.part(
            &mut body,
            shape(Part::FishFins(fish)),
            &fin,
            FLAT,
            FLAT,
            ONE,
        );
        let mut tail = self.pivot(root, "Tail", fish.tail());
        self.part(
            &mut tail,
            shape(Part::FishTail(fish)),
            &fin,
            FLAT,
            FLAT,
            ONE,
        );
        let ([x, y, z], r) = fish.eye();
        self.eyes(&mut body, [0.0, y, z], x, r, false);
        match fish {
            Fish::Piranha => {
                let teeth = self.bone();
                self.part(
                    &mut body,
                    shape(Part::FishMarks(fish)),
                    &teeth,
                    FLAT,
                    FLAT,
                    ONE,
                );
            }
            Fish::Shark => {
                let slits = self.dark();
                self.part(
                    &mut body,
                    shape(Part::FishMarks(fish)),
                    &slits,
                    FLAT,
                    FLAT,
                    ONE,
                );
            }
            Fish::Plain => {}
        }
    }

    /// An eel in an S, in three pieces that bend, each hung from the one
    /// before (Body, Body/Mid, Body/Mid/Tail: its swim rocks them in turn),
    /// the fin along its back darker; an electric eel's spots glowing along
    /// its flanks.
    fn eel(&mut self, root: &mut Gd<Node3D>, electric: bool) {
        let fin = self.dark();
        let spots = self
            .art
            .flat(Color::from_rgb(0.75, 0.95, 1.0), Finish::Glow);
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        let mut mid = self.pivot(&mut body, "Mid", sea::eel_joint(0));
        let mut tail = self.pivot(&mut mid, "Tail", sea::eel_joint(1));
        for (piece, node) in [
            (Eel::Front, &mut body),
            (Eel::Middle, &mut mid),
            (Eel::Tail, &mut tail),
        ] {
            self.skin(node, shape(Part::Eel(piece)), FLAT, FLAT, ONE);
            self.part(node, shape(Part::EelFin(piece)), &fin, FLAT, FLAT, ONE);
            if electric {
                self.part(node, shape(Part::EelSpots(piece)), &spots, FLAT, FLAT, ONE);
            }
        }
        let (eyes, r) = sea::eel_eyes();
        let eye = self.eye();
        for at in eyes {
            self.part(&mut body, sphere(r), &eye, at, FLAT, ONE);
        }
    }

    /// A jellyfish: its bell seen through, in its colour (the Body pivot
    /// pulses), the rings inside it paler, its frilled mouth arms and its
    /// tentacles trailing below.
    fn jellyfish(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Part::JellyBell), FLAT, FLAT, ONE);
        let t = self.tint;
        let pale = |k: f32, a: f32| {
            Color::from_rgba(
                t.r + (1.0 - t.r) * k,
                t.g + (1.0 - t.g) * k,
                t.b + (1.0 - t.b) * k,
                a,
            )
        };
        let rings = self.art.flat(pale(0.6, 0.85), Finish::Flat);
        self.part(&mut body, shape(Part::JellyRings), &rings, FLAT, FLAT, ONE);
        let arms = self.art.flat(pale(0.3, 0.55), Finish::Flat);
        self.part(&mut body, shape(Part::JellyArms), &arms, FLAT, FLAT, ONE);
        let strands = self.art.flat(pale(0.5, 0.45), Finish::Flat);
        self.part(
            &mut body,
            shape(Part::JellyStrands),
            &strands,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A kraken: its mantle and head (the Body pivot breathes), great
    /// eyes, its arms in four groups that each sway on their own pivot, the
    /// suckers under them paler.
    fn kraken(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Part::KrakenBody), FLAT, FLAT, ONE);
        let suckers = self.bone();
        for q in Quarter::ALL {
            let mut arms = self.pivot(root, q.pivot(), sea::KRAKEN_ARMS);
            self.skin(&mut arms, shape(Part::KrakenArms(q)), FLAT, FLAT, ONE);
            self.part(
                &mut arms,
                shape(Part::KrakenSuckers(q)),
                &suckers,
                FLAT,
                FLAT,
                ONE,
            );
        }
        // an eye like a goat's, its pupil a bar
        let ([x, y, z], r) = sea::KRAKEN_EYE;
        let white = self
            .art
            .flat(Color::from_rgb(0.85, 0.75, 0.35), Finish::Glossy);
        let pupil = self.eye();
        for s in [-1.0f32, 1.0] {
            self.part(&mut body, sphere(r), &white, [s * x, y, z], FLAT, ONE);
            self.part(
                &mut body,
                sphere(r),
                &pupil,
                [s * (x + r * 0.55), y, z],
                FLAT,
                [0.5, 0.25, 0.8],
            );
        }
    }
}

/// What the sea creatures move when idle besides what `proc_waves` moves
/// for their kind: a fish's tail bobs with its body; a wave runs down an
/// eel, each piece swinging after the one before; a jellyfish bobs; a
/// kraken's arms sway.
pub(super) fn idle_waves(kind: Proc, shape: Option<&str>) -> Vec<Wave> {
    let (x, z) = (Vector3::RIGHT, Vector3::BACK);
    match Sea::of(kind, shape) {
        Some(Sea::Fish(fish)) => {
            vec![Wave::Bob(
                "Tail",
                Vector3::from_array(fish.tail()),
                0.04,
                0.0,
            )]
        }
        Some(Sea::Eel { .. }) => swim(),
        Some(Sea::Jellyfish) => vec![Wave::Bob("Body", Vector3::ZERO, 0.04, 0.3)],
        // (one turn a pivot, about a slant: a second would override it)
        Some(Sea::Kraken) => Quarter::ALL
            .into_iter()
            .enumerate()
            .map(|(i, q)| {
                let slant = (x + z * if i % 2 == 0 { 0.8 } else { -0.8 }).normalized();
                Wave::Rock(q.pivot(), slant, 9.0, i as f32 * 0.27)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// An eel's swim: its middle and its tail swinging after its front, in
/// turn (their paths go down through the pieces they hang from).
pub(super) fn swim() -> Vec<Wave> {
    let y = Vector3::UP;
    vec![
        Wave::Rock("Body/Mid", y, 14.0, 0.8),
        Wave::Rock("Body/Mid/Tail", y, 18.0, 0.6),
    ]
}
