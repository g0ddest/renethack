//! The hero in the saddle: a skeleton modifier that seats a character on a
//! steed's back whatever clip plays on it. UAL has a chair to sit on and no
//! saddle, and the hero's own clips must go on up there (the idle's breath,
//! a blow, a potion drunk). So after the animation, each frame, the hips are
//! put on the seat, wherever the steed's back is now (a gallop lifts it),
//! the legs astride the back (the thighs out round it and forward, the
//! lower legs hanging, the heels down), and the body from the hips up is
//! left to its clip, the hips turning with it only a little. `riding.rs`
//! makes the modifier when a hero first mounts, fades its influence in and
//! out as they mount and dismount and puts it to sleep on foot.

use godot::classes::{ISkeletonModifier3D, Node3D, Skeleton3D, SkeletonModifier3D};
use godot::prelude::*;

/// The hip joints above what is sat on, in thigh lengths.
const HIPS_UP: f32 = 0.24;
/// How far the bone of a thigh keeps from the steed's side, likewise.
const THIGH_CLEAR: f32 = 0.2;
/// How far a thigh points forward of straight down (radians).
const THIGH_FORWARD: f32 = 0.92;
/// The most of a thigh's length that goes sideways (a back too broad to
/// sit astride leaves the knees this far apart).
const SPLAY_MAX: f32 = 0.82;
/// How much of the clip's turn of the hips the seat lets through.
const HIPS_FOLLOW: f32 = 0.3;
/// The toes turned out and the heels down (radians).
const TOES_OUT: f32 = 0.22;
const HEELS_DOWN: f32 = 0.2;

/// A steed's back: where a rider sits on it.
#[derive(Clone)]
pub struct Mount {
    /// The steed's model: the seat is fixed to it when no bone carries it.
    pub node: Gd<Node3D>,
    /// The bone of the back the seat moves with, on its skeleton.
    pub bone: Option<(Gd<Skeleton3D>, i32)>,
    /// The seat (the top of the saddle) from the bone, or from the node.
    pub seat: Vector3,
    /// Half the back's breadth at the seat, in metres.
    pub half_width: f32,
}

impl Mount {
    /// Where the seat is now, in the world. None once the steed is gone.
    pub fn at(&self) -> Option<Vector3> {
        let live = |n: &Gd<Node3D>| n.is_instance_valid() && n.is_inside_tree();
        match &self.bone {
            Some((sk, bone)) => live(&sk.clone().upcast())
                .then(|| sk.get_global_transform() * (sk.get_bone_global_pose(*bone) * self.seat)),
            None => live(&self.node).then(|| self.node.get_global_transform() * self.seat),
        }
    }
}

/// The bones the seat moves: the hips and the two legs (left, right).
#[derive(Clone, Copy)]
struct Bones {
    pelvis: i32,
    thigh: [i32; 2],
    calf: [i32; 2],
    foot: [i32; 2],
}

/// A character's legs as its skeleton has them, in the space of the hips:
/// where each thigh starts (left, right) and how long it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Legs {
    pub hip: [Vector3; 2],
    pub thigh: f32,
}

/// The seated pose in the body's own frame (x to its left, y up, z the way
/// it faces): the hips, and the turns of each leg's thigh, lower leg and
/// foot (left, right).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Astride {
    pub hips: Transform3D,
    pub thigh: [Basis; 2],
    pub calf: [Basis; 2],
    pub foot: [Basis; 2],
}

/// The hips of the shared skeleton as they stand, and a foot flat on the
/// ground, its toes forward: the bones' own axes in the body's frame (a
/// leg's bone runs along its y, its x to the body's left).
fn hips_square() -> Basis {
    // the hips' bone up and a little forward: (0, 0.968, 0.25)
    Basis::from_axis_angle(Vector3::RIGHT, 0.25f32.atan2(0.968))
}

fn foot_flat() -> Basis {
    // the foot's bone forward and down: (0, -0.51, 0.86)
    Basis::from_axis_angle(Vector3::RIGHT, 0.86f32.atan2(-0.51))
}

/// From `a` towards `b` by `t`, the short way round (worked out here: the
/// engine's own needs the engine).
fn slerp(a: Quaternion, b: Quaternion, t: f32) -> Quaternion {
    let mut d = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let b = if d < 0.0 {
        d = -d;
        Quaternion::new(-b.x, -b.y, -b.z, -b.w)
    } else {
        b
    };
    let (ka, kb) = if d > 0.9995 {
        (1.0 - t, t)
    } else {
        let th = d.clamp(-1.0, 1.0).acos();
        let s = th.sin();
        (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
    };
    let q = Quaternion::new(
        a.x * ka + b.x * kb,
        a.y * ka + b.y * kb,
        a.z * ka + b.z * kb,
        a.w * ka + b.w * kb,
    );
    let n = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
    Quaternion::new(q.x / n, q.y / n, q.z / n, q.w / n)
}

/// Astride a back `half_width` broad at a seat whose top is `seat`: the
/// hips over it, turned as the clip turns them (`hips_now`) by a part
/// only; each thigh forward and out as far as the back is broad, the lower
/// leg hanging from the knee a little back and in, the heel down. All in
/// the body's frame and the skeleton's units.
pub fn astride(seat: Vector3, half_width: f32, hips_now: Basis, legs: &Legs) -> Astride {
    let square = hips_square();
    let turn = slerp(
        square.get_quaternion(),
        hips_now.orthonormalized().get_quaternion(),
        HIPS_FOLLOW,
    );
    let hips = Transform3D::new(
        Basis::from_quaternion(turn),
        seat + Vector3::new(0.0, HIPS_UP * legs.thigh, 0.0),
    );
    let mut out = Astride {
        hips,
        thigh: [Basis::IDENTITY; 2],
        calf: [Basis::IDENTITY; 2],
        foot: [Basis::IDENTITY; 2],
    };
    for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
        let hip = hips * legs.hip[i];
        // the knee clear of the steed's side, never inside of the hip
        let knee_out = half_width + THIGH_CLEAR * legs.thigh;
        let dx = ((knee_out - (hip.x - seat.x).abs()) / legs.thigh).clamp(0.0, SPLAY_MAX);
        let rest = (1.0 - dx * dx).sqrt();
        let thigh = Vector3::new(
            side * dx,
            -rest * THIGH_FORWARD.cos(),
            rest * THIGH_FORWARD.sin(),
        );
        let calf = Vector3::new(-side * 0.06, -0.96, -0.27).normalized();
        // the knee bends about the axis square to both
        let hinge = thigh.cross(calf).normalized();
        out.thigh[i] = Basis::from_cols(hinge, thigh, hinge.cross(thigh));
        out.calf[i] = Basis::from_cols(hinge, calf, hinge.cross(calf));
        out.foot[i] = Basis::from_axis_angle(Vector3::UP, side * TOES_OUT)
            * Basis::from_axis_angle(Vector3::RIGHT, -HEELS_DOWN)
            * foot_flat();
    }
    out
}

#[derive(GodotClass)]
#[class(base = SkeletonModifier3D, init)]
pub struct Seated {
    base: Base<SkeletonModifier3D>,
    /// The model's own node: the body's frame (which way is up for it and
    /// which way it faces).
    body: Option<Gd<Node3D>>,
    mount: Option<Mount>,
    bones: Option<Bones>,
    /// Where it put the hips when it last solved, in the skeleton's space
    /// (self-tests).
    hips: Option<Vector3>,
}

impl Seated {
    /// The model's own node, which the skeleton stands in.
    pub fn set_body(&mut self, body: Gd<Node3D>) {
        self.body = Some(body);
    }

    /// The back sat on, or none.
    pub fn set_mount(&mut self, mount: Option<Mount>) {
        if mount.is_none() {
            self.hips = None;
        }
        self.mount = mount;
    }

    pub fn mount(&self) -> Option<&Mount> {
        self.mount.as_ref()
    }

    /// Where the hips were put when the skeleton was last solved, in its
    /// space (self-tests: the bone is there).
    pub fn hips(&self) -> Option<Vector3> {
        self.hips
    }

    fn bones(&mut self, sk: &Gd<Skeleton3D>) -> Option<Bones> {
        if self.bones.is_none() {
            let find = |n: &str| Some(sk.find_bone(n)).filter(|&i| i >= 0);
            self.bones = Some(Bones {
                pelvis: find("pelvis")?,
                thigh: [find("thigh_l")?, find("thigh_r")?],
                calf: [find("calf_l")?, find("calf_r")?],
                foot: [find("foot_l")?, find("foot_r")?],
            });
        }
        self.bones
    }
}

#[godot_api]
impl ISkeletonModifier3D for Seated {
    fn process_modification_with_delta(&mut self, _delta: f64) {
        let (Some(mount), Some(body)) = (self.mount.clone(), self.body.clone()) else {
            return;
        };
        let Some(mut sk) = self.base().get_skeleton() else {
            return;
        };
        let (Some(seat), Some(b)) = (mount.at(), self.bones(&sk)) else {
            return;
        };
        if !body.is_instance_valid() || !body.is_inside_tree() {
            return;
        }
        // the body's frame in the skeleton's space: the axes, and how many
        // of its units a metre is
        let to_sk = sk.get_global_transform().affine_inverse();
        let axes = to_sk.basis * body.get_global_transform().basis;
        let per_metre = axes.col_b().length();
        let frame = axes.orthonormalized();
        let to_frame = frame.inverse();
        let legs = Legs {
            hip: b.thigh.map(|t| sk.get_bone_pose_position(t)),
            thigh: sk.get_bone_pose_position(b.calf[0]).length(),
        };
        let hips_now = to_frame * sk.get_bone_global_pose(b.pelvis).basis;
        let pose = astride(
            to_frame * (to_sk * seat),
            mount.half_width * per_metre,
            hips_now,
            &legs,
        );
        let hips = Transform3D::new(frame * pose.hips.basis, frame * pose.hips.origin);
        sk.set_bone_global_pose(b.pelvis, hips);
        for i in 0..2 {
            let (thigh, calf, foot) = (
                frame * pose.thigh[i],
                frame * pose.calf[i],
                frame * pose.foot[i],
            );
            sk.set_bone_pose_rotation(b.thigh[i], (hips.basis.inverse() * thigh).get_quaternion());
            sk.set_bone_pose_rotation(b.calf[i], (thigh.inverse() * calf).get_quaternion());
            sk.set_bone_pose_rotation(b.foot[i], (calf.inverse() * foot).get_quaternion());
        }
        self.hips = Some(hips.origin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legs() -> Legs {
        // the shared skeleton's: hips 0.18 apart, thighs 0.4 long
        Legs {
            hip: [
                Vector3::new(0.089, 0.028, 0.046),
                Vector3::new(-0.089, 0.028, 0.046),
            ],
            thigh: 0.4,
        }
    }

    fn knee(p: &Astride, legs: &Legs, i: usize) -> Vector3 {
        p.hips * legs.hip[i] + p.thigh[i].col_b() * legs.thigh
    }

    #[test]
    fn the_hips_rest_over_the_seat_and_the_knees_clear_the_back() {
        let legs = legs();
        let seat = Vector3::new(0.0, 1.5, 0.1);
        for half in [0.12f32, 0.2, 0.3] {
            let p = astride(seat, half, hips_square(), &legs);
            assert!((p.hips.origin - seat - Vector3::new(0.0, 0.096, 0.0)).length() < 1e-5);
            for (i, side) in [1.0f32, -1.0].into_iter().enumerate() {
                let k = knee(&p, &legs, i);
                // out beyond the back's side, below the hips and forward
                assert!(k.x * side >= half + 0.07, "{half}: knee {k:?}");
                assert!(k.y < p.hips.origin.y && k.z > seat.z, "{half}: knee {k:?}");
                // the lower leg hangs, the knee bending backwards only
                let calf = p.calf[i].col_b();
                assert!(calf.y < -0.9, "{calf:?}");
                assert!((p.thigh[i].col_a() - p.calf[i].col_a()).length() < 1e-5);
                assert!(p.thigh[i].col_a().x > 0.0);
                // proper turns, no shear or flip
                for b in [p.thigh[i], p.calf[i], p.foot[i]] {
                    assert!((b.determinant() - 1.0).abs() < 1e-4, "{b:?}");
                }
                // the foot's sole down, its toes forward and out
                let toes = p.foot[i].col_b();
                assert!(toes.z > 0.6 && toes.x * side > 0.0, "{toes:?}");
            }
        }
    }

    #[test]
    fn a_back_too_broad_to_sit_astride_splays_the_legs_no_further() {
        let legs = legs();
        let p = astride(Vector3::ZERO, 2.0, hips_square(), &legs);
        for i in 0..2 {
            let thigh = p.thigh[i].col_b();
            assert!((thigh.x.abs() - SPLAY_MAX).abs() < 1e-5, "{thigh:?}");
            assert!((thigh.length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn the_hips_turn_with_the_clip_by_a_part_only() {
        let legs = legs();
        // a blow turns the hips a quarter round: the seat lets a third through
        let turned = Basis::from_axis_angle(Vector3::UP, 1.2) * hips_square();
        let p = astride(Vector3::ZERO, 0.2, turned, &legs);
        let yaw = p.hips.basis.col_a().z.atan2(p.hips.basis.col_a().x).abs();
        assert!((yaw - 1.2 * HIPS_FOLLOW).abs() < 0.02, "{yaw}");
        // and the knees stay where the back is, whatever the hips do
        let square = astride(Vector3::ZERO, 0.2, hips_square(), &legs);
        for i in 0..2 {
            assert!((p.thigh[i].col_b() - square.thigh[i].col_b()).length() < 0.2);
        }
    }
}
