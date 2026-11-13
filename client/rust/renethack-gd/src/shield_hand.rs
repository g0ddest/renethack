//! The shield hand of a body without finger bones (the generated Valkyrie):
//! a skeleton modifier that, after the animation each frame, holds the left
//! wrist straight on the forearm and draws the open hand in to a fist's
//! length (`ModelSpec::shield_fist`), so that the hand stays behind the
//! boss of the shield strapped there whatever the clip does with the wrist
//! (a blow bent it up past the rim). `equip.rs` makes it with the first
//! shield such a body holds and wakes it only while one is held.

use godot::classes::{ISkeletonModifier3D, SkeletonModifier3D};
use godot::prelude::*;

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct ShieldHand {
    base: Base<SkeletonModifier3D>,
    /// The hand's scale in its own frame.
    fist: Vector3,
    /// The left hand's bone (-1: not looked up yet, or none).
    hand: Option<i32>,
}

impl ShieldHand {
    pub fn set_fist(&mut self, fist: Vector3) {
        self.fist = fist;
    }
}

#[godot_api]
impl ISkeletonModifier3D for ShieldHand {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let hand = *self.hand.get_or_insert_with(|| sk.find_bone("hand_l"));
        if hand < 0 {
            return;
        }
        let straight = sk.get_bone_rest(hand).basis.get_quaternion();
        sk.set_bone_pose_rotation(hand, straight);
        sk.set_bone_pose_scale(hand, self.fist);
    }
}
