//! The off hand on a two-handed weapon: a skeleton modifier that puts the
//! left hand on the haft wherever the right hand and its clip take the
//! weapon (at rest, walking, swinging; a pick-axe digging). UAL has no
//! two-handed melee clip, so the left arm is bent to the grip by two-bone
//! IK after the animation, each frame: the palm turned (the least) to hold
//! the haft, the wrist where the palm meets the grip, the elbow bending the
//! way the clip bends it. A weapon held steady in both hands (a two-handed
//! sword or an axe up at the ready, a staff upright at the side) also
//! keeps the right arm in its guard, the first pose of its blow, whatever
//! the legs do, and follows the blow while it plays (or the clip it is
//! applied with: a mattock digging). `equip.rs` makes the
//! modifier when a hero first holds such a thing, fades its influence in
//! and out and puts it to sleep at none, so nothing two-handed held costs
//! nothing. Godot 4.7's TwoBoneIK3D would do the arm, but our bindings are
//! of the 4.5 API, which has none.

use godot::classes::animation::TrackType;
use godot::classes::{
    Animation, AnimationPlayer, ISkeletonModifier3D, Skeleton3D, SkeletonModifier3D, Time,
};
use godot::prelude::*;

/// The bones of the right arm the guard holds: upper, lower, hand.
const GUARD_BONES: [&str; 3] = ["upperarm_r", "lowerarm_r", "hand_r"];

/// The bones of the two arms.
#[derive(Clone, Copy)]
struct Bones {
    /// The right arm the guard holds: upper, lower, hand.
    right: [i32; 3],
    clavicle_l: i32,
    upper_l: i32,
    lower_l: i32,
    hand_l: i32,
}

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct OffHand {
    base: Base<SkeletonModifier3D>,
    /// From the right hand's bone to its held frame, and from the left
    /// hand's bone to its palm (the manifest's slots).
    right_slot: Transform3D,
    left_slot: Transform3D,
    /// Where the off hand grips, from the right hand's grip, in the held
    /// frame; none: nothing held in both hands.
    two: Option<Vector3>,
    bones: Option<Bones>,
    /// The guard of what is wielded, if it holds one.
    guard: Option<Guard>,
    /// How far the palm stopped short of its grip last frame (the arm too
    /// short to reach), in the skeleton's units.
    miss: f32,
    /// Microseconds spent solving, and the frames solved.
    spent: (u64, u64),
}

/// What a guard is made of: the model's player and the clips (name, clip)
/// that move the right arm themselves, the blow first, the clips the
/// weapon is applied with after it. The first pose of the blow is the
/// guard.
pub type GuardClips = (Gd<AnimationPlayer>, Vec<(String, Gd<Animation>)>);

/// A guard: the model's player and its clips, each with its rotation
/// tracks of the right arm (upper, lower, hand).
struct Guard {
    player: Gd<AnimationPlayer>,
    clips: Vec<(String, Gd<Animation>, [i32; 3])>,
}

impl OffHand {
    /// The hands' slots (bone space to the held frame).
    pub fn set_slots(&mut self, right: Transform3D, left: Transform3D) {
        self.right_slot = right;
        self.left_slot = left;
    }

    /// The off hand's grip in the held frame, or none.
    pub fn set_grip(&mut self, two: Option<Vector3>) {
        self.two = two;
    }

    /// The right arm held in a guard, or not.
    pub fn set_guard(&mut self, guard: Option<GuardClips>) {
        self.guard = guard.and_then(|(player, clips)| {
            let tracks = |clip: &Gd<Animation>| {
                let track = |bone: &str| {
                    (0..clip.get_track_count()).find(|&t| {
                        clip.track_get_type(t) == TrackType::ROTATION_3D
                            && clip
                                .track_get_path(t)
                                .to_string()
                                .ends_with(&format!(":{bone}"))
                    })
                };
                let [u, l, h] = GUARD_BONES;
                Some([track(u)?, track(l)?, track(h)?])
            };
            // the blow has the arm's tracks, or there is no guard; another
            // clip without them is left to the player
            let mut clips = clips.into_iter();
            let (blow, clip) = clips.next()?;
            let first = (blow, tracks(&clip)?, clip);
            let rest = clips.filter_map(|(name, clip)| Some((name, tracks(&clip)?, clip)));
            let clips = std::iter::once(first)
                .chain(rest)
                .map(|(name, tracks, clip)| (name, clip, tracks))
                .collect();
            Some(Guard { player, clips })
        });
    }

    /// The right arm in the guard, or as the clip now assigned has it when
    /// that is the blow or another of the guard's (paused in it too).
    fn hold_up(&self, sk: &mut Gd<Skeleton3D>, right: [i32; 3]) {
        let Some(g) = &self.guard else {
            return;
        };
        let now = g.player.get_assigned_animation();
        let playing = g.clips.iter().find(|(name, ..)| now == name.as_str());
        let (clip, tracks, t) = match (playing, g.clips.first()) {
            (Some((_, clip, tracks)), _) => {
                (clip, tracks, g.player.get_current_animation_position())
            }
            (None, Some((_, clip, tracks))) => (clip, tracks, 0.0),
            (None, None) => return,
        };
        for (bone, track) in right.into_iter().zip(*tracks) {
            sk.set_bone_pose_rotation(bone, clip.rotation_track_interpolate(track, t));
        }
    }

    /// How far the palm stopped short of its grip last frame (self-tests).
    pub fn miss(&self) -> f32 {
        self.miss
    }

    /// Microseconds spent solving, and the frames solved (self-tests).
    pub fn spent(&self) -> (u64, u64) {
        self.spent
    }

    fn bones(&mut self, sk: &Gd<Skeleton3D>) -> Option<Bones> {
        if self.bones.is_none() {
            let find = |n: &str| Some(sk.find_bone(n)).filter(|&i| i >= 0);
            let upper_l = find("upperarm_l")?;
            let [u, l, h] = GUARD_BONES;
            self.bones = Some(Bones {
                right: [find(u)?, find(l)?, find(h)?],
                clavicle_l: sk.get_bone_parent(upper_l),
                upper_l,
                lower_l: find("lowerarm_l")?,
                hand_l: find("hand_l")?,
            });
        }
        self.bones
    }
}

/// The least rotation taking direction `from` to direction `to`.
fn arc(from: Vector3, to: Vector3) -> Basis {
    let (f, t) = (from.normalized(), to.normalized());
    let axis = f.cross(t);
    let (sin, cos) = (axis.length(), f.dot(t));
    if sin < 1e-6 {
        if cos > 0.0 {
            return Basis::IDENTITY;
        }
        // half a turn about any axis square to `from`
        let side = if f.x.abs() < 0.9 {
            Vector3::RIGHT
        } else {
            Vector3::UP
        };
        return Basis::from_axis_angle(f.cross(side).normalized(), std::f32::consts::PI);
    }
    Basis::from_axis_angle(axis / sin, sin.atan2(cos))
}

/// The left arm's new rotations (upper arm, lower arm, hand: global, in the
/// skeleton's space) that put the palm on `grip`, holding the haft along
/// `haft`; and how far the palm stops short. `upper`, `lower`, `hand` are
/// the arm's global poses now, `palm` the palm's offset from the hand.
pub fn reach(
    upper: Transform3D,
    lower: Transform3D,
    hand: Transform3D,
    palm: Transform3D,
    grip: Vector3,
    haft: Vector3,
) -> (Basis, Basis, Basis, f32) {
    let (hand_b, upper_b, lower_b) = (
        hand.basis.orthonormalized(),
        upper.basis.orthonormalized(),
        lower.basis.orthonormalized(),
    );
    // the palm turned the least to hold the haft along its +y, whichever
    // way along the haft is nearer
    let along = (hand_b * palm.basis).col_b();
    let haft = if along.dot(haft) >= 0.0 { haft } else { -haft };
    let hand_new = arc(along, haft) * hand_b;
    let wrist = grip - hand_new * palm.origin;
    // two bones, shoulder to elbow to wrist
    let (a, b, c) = (upper.origin, lower.origin, hand.origin);
    let (la, lb) = ((b - a).length(), (c - b).length());
    let to = wrist - a;
    let far = to.length();
    let d = far.clamp((la - lb).abs() + 1e-4, la + lb - 1e-4);
    let u = if far > 1e-6 { to / far } else { Vector3::DOWN };
    // the elbow bends the way it bends now
    let bend = (b - a) - u * (b - a).dot(u);
    let v = if bend.length() > 1e-4 {
        bend.normalized()
    } else {
        Vector3::new(0.0, -0.6, -0.8).normalized()
    };
    let cos = ((la * la + d * d - lb * lb) / (2.0 * la * d)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let elbow = a + (u * cos + v * sin) * la;
    let reached = a + u * d;
    let turn_upper = arc(b - a, elbow - a);
    let upper_new = turn_upper * upper_b;
    let turn_lower = arc(turn_upper * (c - b), reached - elbow);
    let lower_new = turn_lower * turn_upper * lower_b;
    (upper_new, lower_new, hand_new, (reached - wrist).length())
}

#[godot_api]
impl ISkeletonModifier3D for OffHand {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let Some(two) = self.two else {
            return;
        };
        let start = Time::singleton().get_ticks_usec();
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let Some(b) = self.bones(&sk) else {
            return;
        };
        self.hold_up(&mut sk, b.right);
        let held = sk.get_bone_global_pose(b.right[2]) * self.right_slot;
        let grip = held * two;
        let haft = (held.basis * two).normalized();
        let parent = sk
            .get_bone_global_pose(b.clavicle_l)
            .basis
            .orthonormalized();
        let (upper, lower, hand, miss) = reach(
            sk.get_bone_global_pose(b.upper_l),
            sk.get_bone_global_pose(b.lower_l),
            sk.get_bone_global_pose(b.hand_l),
            self.left_slot,
            grip,
            haft,
        );
        self.miss = miss;
        sk.set_bone_pose_rotation(b.upper_l, (parent.inverse() * upper).get_quaternion());
        sk.set_bone_pose_rotation(b.lower_l, (upper.inverse() * lower).get_quaternion());
        sk.set_bone_pose_rotation(b.hand_l, (lower.inverse() * hand).get_quaternion());
        let took = Time::singleton().get_ticks_usec().saturating_sub(start);
        self.spent = (self.spent.0 + took, self.spent.1 + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32, z: f32) -> Transform3D {
        Transform3D::new(Basis::IDENTITY, Vector3::new(x, y, z))
    }

    #[test]
    fn the_palm_lands_on_a_grip_within_reach() {
        // an arm hanging down: shoulder, elbow 0.3 below, wrist 0.3 below
        let (upper, lower, hand) = (at(0.2, 1.4, 0.0), at(0.2, 1.1, 0.0), at(0.2, 0.8, 0.0));
        let palm = Transform3D::new(Basis::IDENTITY, Vector3::new(0.0, 0.08, 0.0));
        let grip = Vector3::new(-0.1, 1.2, 0.3);
        let haft = Vector3::UP;
        let (u, l, h, miss) = reach(upper, lower, hand, palm, grip, haft);
        assert!(miss < 1e-3, "{miss}");
        // the bones keep their lengths: the wrist is where the palm meets
        // the grip
        let elbow = upper.origin + u * Vector3::new(0.0, -0.3, 0.0);
        let wrist = elbow + l * Vector3::new(0.0, -0.3, 0.0);
        let palm_at = wrist + h * palm.origin;
        assert!((palm_at - grip).length() < 1e-3, "{palm_at:?}");
        // the palm holds the haft along its +y
        assert!((h * Vector3::UP).dot(haft).abs() > 0.999);
    }

    #[test]
    fn a_grip_out_of_reach_leaves_the_arm_straight_toward_it() {
        let (upper, lower, hand) = (at(0.0, 1.4, 0.0), at(0.0, 1.1, 0.0), at(0.0, 0.8, 0.0));
        let palm = Transform3D::IDENTITY;
        let grip = Vector3::new(1.5, 1.4, 0.0);
        let (u, l, _, miss) = reach(upper, lower, hand, palm, grip, Vector3::RIGHT);
        assert!((miss - (1.5 - 0.6)).abs() < 0.01, "{miss}");
        let dir_u = u * Vector3::new(0.0, -0.3, 0.0);
        let dir_l = l * Vector3::new(0.0, -0.3, 0.0);
        assert!(dir_u.normalized().dot(Vector3::RIGHT) > 0.99);
        assert!(dir_l.normalized().dot(Vector3::RIGHT) > 0.99);
    }
}
