//! The hero's bearing under a load and from hunger, and their hands at
//! their throat when strangled: skeleton modifiers after the animation,
//! each frame, whatever clip plays (the idle, a walk, a blow). A burdened
//! hero leans into the load from the small of the back, the head raised
//! to see ahead; a hero weak from hunger slumps, the upper back and the
//! neck bowed and the head hanging (`Bearing`, before the off hand's,
//! which then holds the haft where the bent arms are). A strangled hero
//! clutches their throat with both hands (`Throat`, after every other
//! modifier: the hands leave what they held). `hero_look.rs` makes them on
//! the hero's skeleton the first time they are wanted, eases them in and
//! out, and puts them to sleep at none.

use godot::classes::{ISkeletonModifier3D, Node3D, Skeleton3D, SkeletonModifier3D};
use godot::prelude::*;

use crate::off_hand::reach;

/// The bones bent, from the hips up: the pelvis, the three of the spine,
/// the neck, the head.
const CHAIN: [&str; 6] = [
    "pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "Head",
];
/// Each bone's share of a lean (the small of the back most) and of a bow
/// (the shoulders and the neck), from spine_01 to the head.
const LEAN_SHARE: [f32; 5] = [0.5, 0.3, 0.2, 0.0, 0.0];
const BOW_SHARE: [f32; 5] = [0.0, 0.2, 0.4, 0.4, 0.0];

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct Bearing {
    base: Base<SkeletonModifier3D>,
    /// The body the skeleton is of: its frame says which way is forward.
    body: Option<Gd<Node3D>>,
    /// Degrees forward: the back's lean, the shoulders' and neck's bow,
    /// the head's nod (negative: raised).
    lean: f32,
    bow: f32,
    nod: f32,
    bones: Option<[i32; 6]>,
    /// How far the bend took the head last frame: forward, and up (in the
    /// skeleton's units; self-tests).
    shift: (f32, f32),
}

impl Bearing {
    pub fn set_body(&mut self, body: Gd<Node3D>) {
        self.body = Some(body);
    }

    /// The angles to bend by (degrees): the lean, the bow, the nod.
    pub fn set_angles(&mut self, lean: f32, bow: f32, nod: f32) {
        (self.lean, self.bow, self.nod) = (lean, bow, nod);
    }

    /// How far the bend took the head last frame: forward, and up.
    pub fn shift(&self) -> (f32, f32) {
        self.shift
    }

    fn bones(&mut self, sk: &Gd<Skeleton3D>) -> Option<[i32; 6]> {
        if self.bones.is_none() {
            let found = CHAIN.map(|name| sk.find_bone(name));
            self.bones = found.iter().all(|&i| i >= 0).then_some(found);
        }
        self.bones
    }
}

/// Degrees each bone of the chain above the pelvis turns by.
fn shares(lean: f32, bow: f32, nod: f32) -> [f32; 5] {
    let mut out = [0.0; 5];
    for (i, o) in out.iter_mut().enumerate() {
        *o = lean * LEAN_SHARE[i] + bow * BOW_SHARE[i];
    }
    out[4] += nod;
    out
}

/// A bone's local rotation turned `deg` about `axis` (both in the
/// skeleton's space) more than it is, from its global basis and its
/// parent's, both as they were before any bone of the chain turned: every
/// turn is about the one axis, so the bones above take each turn below
/// them along and add their own.
fn bent(parent: Basis, own: Basis, axis: Vector3, deg: f32) -> Quaternion {
    let turn = Basis::from_axis_angle(axis, deg.to_radians());
    (parent.orthonormalized().inverse() * turn * own.orthonormalized()).get_quaternion()
}

#[godot_api]
impl ISkeletonModifier3D for Bearing {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let degs = shares(self.lean, self.bow, self.nod);
        if degs.iter().all(|&d| d == 0.0) {
            self.shift = (0.0, 0.0);
            return;
        }
        let Some(body) = self.body.clone().filter(|b| b.is_instance_valid()) else {
            return;
        };
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let Some(chain) = self.bones(&sk) else {
            return;
        };
        if !body.is_inside_tree() {
            return;
        }
        // the body's left in the skeleton's space: it faces +z, and a turn
        // about its left bends it forward
        let axes =
            sk.get_global_transform().affine_inverse().basis * body.get_global_transform().basis;
        let left = axes.col_a().normalized();
        let was = chain.map(|i| sk.get_bone_global_pose(i).basis);
        let head = chain[5];
        let before = sk.get_bone_global_pose(head).origin;
        for (k, &deg) in degs.iter().enumerate() {
            if deg != 0.0 {
                sk.set_bone_pose_rotation(chain[k + 1], bent(was[k], was[k + 1], left, deg));
            }
        }
        let moved = sk.get_bone_global_pose(head).origin - before;
        self.shift = (
            moved.dot(axes.col_c().normalized()),
            moved.dot(axes.col_b().normalized()),
        );
    }
}

/// The bones of a throat clutched: the neck and the head, and each arm's
/// clavicle, upper arm, lower arm and hand, the left's then the right's.
const NECK_BONES: [&str; 2] = ["neck_01", "Head"];
const ARM_BONES: [[&str; 4]; 2] = [
    ["clavicle_l", "upperarm_l", "lowerarm_l", "hand_l"],
    ["clavicle_r", "upperarm_r", "lowerarm_r", "hand_r"],
];
/// Where the palms grip the neck, in metres: out to either side of its
/// middle, and forward of it.
const THROAT_SIDE: f32 = 0.055;
const THROAT_FORWARD: f32 = 0.035;

#[derive(Clone, Copy)]
struct ThroatBones {
    neck: [i32; 2],
    arms: [[i32; 4]; 2],
}

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct Throat {
    base: Base<SkeletonModifier3D>,
    /// The body the skeleton is of: its frame says which way is forward
    /// and how many of the skeleton's units a metre is.
    body: Option<Gd<Node3D>>,
    /// From each hand's bone to its palm (the manifest's held slots), the
    /// left's then the right's.
    palms: [Transform3D; 2],
    bones: Option<ThroatBones>,
    /// How far a palm stopped short of the neck last frame, in metres.
    miss: f32,
}

impl Throat {
    pub fn set_body(&mut self, body: Gd<Node3D>, palms: [Transform3D; 2]) {
        self.body = Some(body);
        self.palms = palms;
    }

    /// How far a palm stopped short of the neck last frame, in metres.
    pub fn miss(&self) -> f32 {
        self.miss
    }

    fn bones(&mut self, sk: &Gd<Skeleton3D>) -> Option<ThroatBones> {
        if self.bones.is_none() {
            let neck = NECK_BONES.map(|n| sk.find_bone(n));
            let arms = ARM_BONES.map(|arm| arm.map(|n| sk.find_bone(n)));
            let all = neck.iter().chain(arms.iter().flatten()).all(|&i| i >= 0);
            self.bones = all.then_some(ThroatBones { neck, arms });
        }
        self.bones
    }
}

#[godot_api]
impl ISkeletonModifier3D for Throat {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let Some(body) = self.body.clone().filter(|b| b.is_instance_valid()) else {
            return;
        };
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let Some(b) = self.bones(&sk) else {
            return;
        };
        if !body.is_inside_tree() {
            return;
        }
        // the body's frame in the skeleton's space, and its units a metre
        let axes =
            sk.get_global_transform().affine_inverse().basis * body.get_global_transform().basis;
        let per_metre = axes.col_b().length().max(1e-6);
        let frame = axes.orthonormalized();
        let (left, forward) = (frame.col_a(), frame.col_c());
        let neck = sk.get_bone_global_pose(b.neck[0]).origin;
        let head = sk.get_bone_global_pose(b.neck[1]).origin;
        // the palms close round the neck, its line the haft they hold
        let middle = neck.lerp(head, 0.5) + forward * (THROAT_FORWARD * per_metre);
        let along = (head - neck).normalized();
        let mut miss: f32 = 0.0;
        for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
            let [clavicle, upper, lower, hand] = b.arms[i];
            let grip = middle + left * (side * THROAT_SIDE * per_metre);
            let (u, l, h, short) = reach(
                sk.get_bone_global_pose(upper),
                sk.get_bone_global_pose(lower),
                sk.get_bone_global_pose(hand),
                self.palms[i],
                grip,
                along,
            );
            let parent = sk.get_bone_global_pose(clavicle).basis.orthonormalized();
            sk.set_bone_pose_rotation(upper, (parent.inverse() * u).get_quaternion());
            sk.set_bone_pose_rotation(lower, (u.inverse() * l).get_quaternion());
            sk.set_bone_pose_rotation(hand, (l.inverse() * h).get_quaternion());
            miss = miss.max(short / per_metre);
        }
        self.miss = miss;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lean_bends_the_back_forward_and_the_head_follows() {
        // a rig's bones turned every which way at rest, the back straight
        // up, facing +z with its left +x
        let globals: [Basis; 6] = std::array::from_fn(|i| {
            let f = i as f32;
            Basis::from_euler(
                EulerOrder::XYZ,
                Vector3::new(0.3 * f, -0.5 + 0.2 * f, 0.7 * f),
            )
        });
        let degs = shares(20.0, 8.0, -5.0);
        // the new globals, the pelvis's as it was, each bone's from its
        // parent's and its new local
        let mut now = globals[0];
        let mut turned = 0.0;
        for k in 0..5 {
            let local =
                Basis::from_quaternion(bent(globals[k], globals[k + 1], Vector3::RIGHT, degs[k]));
            now = now * local;
            turned += degs[k];
            let want = Basis::from_axis_angle(Vector3::RIGHT, turned.to_radians()) * globals[k + 1];
            for v in [Vector3::RIGHT, Vector3::UP, Vector3::BACK] {
                assert!((now * v - want * v).length() < 1e-4, "{k}: {:?}", now * v);
            }
        }
        assert!((turned - 23.0).abs() < 1e-4, "{turned}");
        // a turn about +x by a lean takes up forward (+z) and down
        let top = Basis::from_axis_angle(Vector3::RIGHT, 20f32.to_radians()) * Vector3::UP;
        assert!(top.z > 0.3 && top.y < 0.95, "{top:?}");
    }

    #[test]
    fn a_bow_is_the_shoulders_and_neck_and_a_nod_the_head_alone() {
        let s = shares(0.0, 10.0, 0.0);
        assert_eq!(s[0], 0.0, "the small of the back stays");
        assert!((s.iter().sum::<f32>() - 10.0).abs() < 1e-5);
        let s = shares(0.0, 0.0, -6.0);
        assert_eq!(s, [0.0, 0.0, 0.0, 0.0, -6.0]);
    }
}
