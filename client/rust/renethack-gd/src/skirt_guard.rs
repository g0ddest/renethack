//! The coat's guard (a generated hero's coat or robe, `art::simulate_skirt`):
//! a skeleton modifier after its spring bone simulator that keeps every
//! joint of the hem within `SKIRT_SWING` of its rest, whatever the
//! simulation did: no chain of the hem ever stands out flat. (The plank
//! the art loop's iteration 7 saw on a step's start, `c4-steps`, was the
//! skinning: the coat by the floor went with the feet, glb_skirt.py.)

use godot::classes::{ISkeletonModifier3D, SkeletonModifier3D};
use godot::prelude::*;

/// How far a joint of the hem may swing from its rest (degrees): a
/// chain's first joint, at the hip, and each one under it, so that a whole
/// chain swings out at most 70 degrees and never stands out flat.
const SKIRT_SWING: [f32; 3] = [35.0, 20.0, 15.0];

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct SkirtLimit {
    base: Base<SkeletonModifier3D>,
    /// The hem's joints, their rest rotations and how far each may swing
    /// (radians), looked up once.
    joints: Option<Vec<(i32, Quaternion, f32)>>,
}

#[godot_api]
impl ISkeletonModifier3D for SkirtLimit {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let joints = self.joints.get_or_insert_with(|| {
            (0..sk.get_bone_count())
                .filter_map(|b| {
                    // skirt_<chain>_<joint>
                    let name = sk.get_bone_name(b).to_string();
                    let joint: usize = name
                        .strip_prefix("skirt_")?
                        .rsplit('_')
                        .next()?
                        .parse()
                        .ok()?;
                    let swing = SKIRT_SWING[joint.min(SKIRT_SWING.len() - 1)].to_radians();
                    Some((b, sk.get_bone_rest(b).basis.get_quaternion(), swing))
                })
                .collect()
        });
        for &(bone, rest, swing) in joints.iter() {
            let q = sk.get_bone_pose_rotation(bone);
            let mut rel = rest.inverse() * q;
            // (the short way round)
            if rel.w < 0.0 {
                rel = Quaternion::new(-rel.x, -rel.y, -rel.z, -rel.w);
            }
            let angle = rel.get_angle();
            if angle > swing {
                let axis = rel.get_axis();
                sk.set_bone_pose_rotation(bone, rest * Quaternion::from_axis_angle(axis, swing));
            }
        }
    }
}
