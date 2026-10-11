//! Riding, the art's side: a steed's back measured for the seat, the
//! saddle on it, and the hero seated there. No model says where its back
//! is, so it is measured once a model as the model rests: the top of the
//! body over the bone the manifest names for its seat (or over its middle)
//! and how broad it is there. The saddle is fitted to that and hangs on
//! the bone, so it rides the back as the steed walks and gallops; the hero
//! is held on it by `seated.rs`, which follows the same bone.

use godot::classes::mesh::ArrayType;
use godot::classes::{
    BoneAttachment3D, MeshInstance3D, Node, Node3D, Skeleton3D, SkeletonModifier3D,
};
use godot::prelude::*;

use super::{Art, Finish, Model, Pose, find, within};
use crate::meshes::torus;
use crate::seated::{Mount, Seated};

/// The seat's solver under the skeleton, and the saddle on a back
/// (self-tests look for them).
pub const SEATED: &str = "Seated";
pub const SADDLE_NODE: &str = "Saddle";
/// Seconds the hero takes to settle in the saddle, or to leave it.
const SEAT_SECS: f32 = 0.3;
/// The saddle model, and how long it is on a back as broad as a horse's
/// (`HORSE_HALF`, metres): a broader back takes a bigger one, within
/// bounds (a mumak's is no howdah).
const SADDLE_MODEL: &str = "saddle";
const SADDLE_LENGTH: f32 = 0.36;
const HORSE_HALF: f32 = 0.16;
const SADDLE_SIZES: (f32, f32) = (0.6, 1.5);
/// How high the saddle's seat is over the back, in its lengths.
const SADDLE_RISE: f32 = 0.11;
/// One vertex in so many is enough to find a back.
const EVERY: usize = 3;

/// A model's back where a rider sits, as the model rests, in its own
/// space (the scene's units, before the look's size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Back {
    /// The top of the back at the seat.
    pub top: Vector3,
    /// Half the back's breadth there.
    pub half_width: f32,
    /// How low the belly hangs under it (a girth goes round).
    pub belly: f32,
}

/// The back among the `points` of a body: at `at` along it (None: its
/// middle), a slice a seventh of its length thick. The top is the highest
/// point of the slice, the belly its lowest near the middle, the breadth
/// that of what lies above the legs' height.
pub fn back_of(points: &[Vector3], at: Option<f32>) -> Option<Back> {
    let (lo, hi) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.z), hi.max(p.z))
    });
    if lo > hi {
        return None;
    }
    let z = at.unwrap_or((lo + hi) / 2.0);
    let slab = ((hi - lo) / 14.0).max(1e-4);
    let here: Vec<Vector3> = points
        .iter()
        .copied()
        .filter(|p| (p.z - z).abs() <= slab)
        .collect();
    let reach = here.iter().map(|p| p.x.abs()).fold(0.0, f32::max);
    let top = here.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let belly = here
        .iter()
        .filter(|p| p.x.abs() <= reach * 0.3)
        .map(|p| p.y)
        .fold(f32::MAX, f32::min);
    if here.is_empty() || belly > top {
        return None;
    }
    let half_width = here
        .iter()
        .filter(|p| p.y >= belly + (top - belly) * 0.3)
        .map(|p| p.x.abs())
        .fold(0.0, f32::max);
    Some(Back {
        top: Vector3::new(0.0, top, z),
        half_width,
        belly,
    })
}

/// How big the saddle on a back `half_width` (metres) broad is: its
/// length.
fn saddle_length(half_width: f32) -> f32 {
    SADDLE_LENGTH * (half_width / HORSE_HALF).clamp(SADDLE_SIZES.0, SADDLE_SIZES.1)
}

/// Under something worn on a bone (a horn, a saddle): no part of the body.
fn is_worn(inner: &Gd<Node3D>, node: &Gd<Node>) -> bool {
    let mut at = node.get_parent();
    while let Some(n) = at {
        if n.instance_id() == inner.instance_id() {
            return false;
        }
        if n.clone().try_cast::<BoneAttachment3D>().is_ok() {
            return true;
        }
        at = n.get_parent();
    }
    false
}

/// Points of the body's meshes under `inner`, in its space, as the model
/// rests: a skinned mesh's vertices where its bones' rest poses carry
/// them, another mesh's bounds.
fn rest_points(inner: &Gd<Node3D>) -> Vec<Vector3> {
    let mut points = Vec::new();
    for node in inner
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if is_worn(inner, &node) {
            continue;
        }
        let Ok(mi) = node.try_cast::<MeshInstance3D>() else {
            continue;
        };
        let Some(mesh) = mi.get_mesh().filter(|_| mi.is_visible()) else {
            continue;
        };
        let skeleton = mi
            .get_node_or_null(&mi.get_skeleton_path())
            .and_then(|n| n.try_cast::<Skeleton3D>().ok());
        let skinned = mi
            .get_skin()
            .filter(|s| s.get_bind_count() > 0)
            .zip(skeleton);
        let Some((skin, skeleton)) = skinned else {
            // the bounds, their middles too: a slice across finds them
            let to_inner = within(inner, &mi.clone().upcast());
            let b = mesh.get_aabb();
            for i in 0..27 {
                let part = |n: i32| (i / n % 3) as f32 / 2.0;
                let at = Vector3::new(part(1), part(3), part(9));
                points.push(to_inner * (b.position + b.size * at));
            }
            continue;
        };
        let to_inner = within(inner, &skeleton.clone().upcast());
        let binds: Vec<Transform3D> = (0..skin.get_bind_count())
            .map(|b| {
                let name = skin.get_bind_name(b).to_string();
                let bone = if name.is_empty() {
                    skin.get_bind_bone(b)
                } else {
                    skeleton.find_bone(name.as_str())
                };
                to_inner * skeleton.get_bone_global_rest(bone.max(0)) * skin.get_bind_pose(b)
            })
            .collect();
        for surface in 0..mesh.get_surface_count() {
            let arrays = mesh.surface_get_arrays(surface);
            let get = |a: ArrayType| arrays.at(a.ord() as usize);
            let verts: PackedVector3Array = get(ArrayType::VERTEX).to();
            let bones: PackedInt32Array = get(ArrayType::BONES).try_to().unwrap_or_default();
            let weights: PackedFloat32Array = get(ArrayType::WEIGHTS).try_to().unwrap_or_default();
            let per = bones.len().checked_div(verts.len()).unwrap_or(0);
            if per == 0 || weights.len() != bones.len() {
                continue;
            }
            let (bones, weights) = (bones.as_slice(), weights.as_slice());
            for (v, p) in verts.as_slice().iter().enumerate().step_by(EVERY) {
                let mut at = Vector3::ZERO;
                for k in v * per..(v + 1) * per {
                    if let Some(bind) = binds.get(bones[k] as usize).filter(|_| weights[k] > 0.0) {
                        at += (*bind * *p) * weights[k];
                    }
                }
                points.push(at);
            }
        }
    }
    points
}

impl Art {
    /// The seat's bone on a model's skeleton, as the manifest names it.
    fn seat_bone(&self, m: &Model) -> Option<(Gd<Skeleton3D>, i32)> {
        let name = self.manifest.model_at(m.key.model).1.seat.as_ref()?;
        let skeleton = find::<Skeleton3D>(&m.inner.clone()?.upcast())?;
        let bone = skeleton.find_bone(name.bone.as_deref()?);
        (bone >= 0).then_some((skeleton, bone))
    }

    /// The back of a model for a rider: measured when a model is first
    /// sat on or saddled, with what the manifest says of its seat. None
    /// for a stand-in still to be built, or a body with nothing to
    /// measure.
    pub fn back(&mut self, m: &Model) -> Option<Back> {
        if let Some(b) = self.backs.get(&m.key.model) {
            return *b;
        }
        let inner = m.inner.clone()?;
        let seat = self.manifest.model_at(m.key.model).1.seat.clone();
        let seat = seat.unwrap_or_default();
        // along the body from the seat's bone, where it rests
        let over = self.seat_bone(m).map(|(sk, bone)| {
            let to_inner = within(&inner, &sk.clone().upcast());
            (to_inner * sk.get_bone_global_rest(bone).origin).z
        });
        let points = rest_points(&inner);
        let mid = back_of(&points, None).map(|b| b.top.z);
        let at = over.or(mid).map(|z| z + seat.along);
        let back = back_of(&points, at).map(|b| Back {
            top: Vector3::new(b.top.x, seat.height.unwrap_or(b.top.y), b.top.z),
            half_width: seat.half_width.unwrap_or(b.half_width),
            ..b
        });
        if back.is_none() {
            let name = self.manifest.model_at(m.key.model).0.to_string();
            self.warn_once(format!("no back to sit on found on {name}"));
        }
        self.backs.insert(m.key.model, back);
        back
    }

    /// Half the breadth in metres of a back on a model as its look sizes
    /// it, and that look's own scale.
    fn breadth(inner: &Gd<Node3D>, back: &Back) -> (f32, Vector3) {
        let scale = inner.get_scale();
        (back.half_width * scale.x, scale)
    }

    /// Where a rider sits on this steed: the top of its saddle, moving
    /// with the bone of its back.
    pub fn mount(&mut self, steed: &Model) -> Option<Mount> {
        let back = self.back(steed)?;
        let inner = steed.inner.clone()?;
        let (half_width, scale) = Self::breadth(&inner, &back);
        let rise = SADDLE_RISE * saddle_length(half_width) / scale.y.max(1e-6);
        let top = back.top + Vector3::new(0.0, rise, 0.0);
        let bone = self.seat_bone(steed);
        let seat = match &bone {
            Some((sk, bone)) => {
                let to_inner = within(&inner, &sk.clone().upcast());
                (to_inner * sk.get_bone_global_rest(*bone)).affine_inverse() * top
            }
            None => top,
        };
        Some(Mount {
            node: inner,
            bone,
            seat,
            half_width,
        })
    }

    /// A saddle on the model's back, or none: made when first called for
    /// and kept on the model, hidden while it wears none.
    pub fn saddle(&mut self, m: &mut Model, on: bool) {
        let fits = m.inner.as_ref().map(|i| i.get_scale());
        // one fitted to another look's size (a pooled body) is made anew
        if let Some((mut node, fit)) = m.saddle.take() {
            if node.is_instance_valid() && Some(fit) == fits {
                node.set_visible(on);
                m.saddle = Some((node, fit));
                return;
            }
            if node.is_instance_valid() {
                node.queue_free();
            }
        }
        if !on {
            return;
        }
        let (Some(back), Some(mut inner)) = (self.back(m), m.inner.clone()) else {
            return;
        };
        let Some(model) = self.manifest.model_index(SADDLE_MODEL) else {
            return;
        };
        let ghost = m.key.pose == Pose::Ghost;
        let Some(prop) = self.prop(model, Color::WHITE, ghost) else {
            return;
        };
        let (half_width, scale) = Self::breadth(&inner, &back);
        let length = saddle_length(half_width);
        let k = length / self.manifest.model_at(model).1.size;
        // upright and its own shape on a back the look sizes, and squeezes
        // into its cell: the look's scale undone
        let basis = inner.get_transform().basis.inverse() * Basis::from_scale(Vector3::ONE * k);
        let fitted = Transform3D::new(basis, back.top);
        let mut root = Node3D::new_alloc();
        root.set_name(SADDLE_NODE);
        root.add_child(&prop);
        // the girth: a strap round the barrel, in the saddle's own units
        let deep = (back.top.y - back.belly) * scale.y / 2.0;
        if deep > 0.0 {
            let leather = self.flat(Color::from_rgb(0.16, 0.1, 0.07), Finish::Matte);
            let mut girth = MeshInstance3D::new_alloc();
            girth.set_name("Girth");
            girth.set_mesh(&self.mesh(torus(0.94, 1.0)));
            girth.set_material_override(&leather);
            // the ring lies flat: stood up across the back, its plane
            // square to the body
            let stood = Basis::from_axis_angle(Vector3::RIGHT, std::f32::consts::FRAC_PI_2);
            let size = Vector3::new((half_width + 0.004) / k, (deep + 0.004) / k, 1.4);
            girth.set_transform(Transform3D::new(
                Basis::from_scale(size) * stood,
                Vector3::new(0.0, -deep / k, 0.12),
            ));
            root.add_child(&girth);
        }
        match self.seat_bone(m) {
            Some((mut sk, bone)) => {
                let to_inner = within(&inner, &sk.clone().upcast());
                let rest = to_inner * sk.get_bone_global_rest(bone);
                let mut at = BoneAttachment3D::new_alloc();
                at.set_name(&format!("{SADDLE_NODE}On"));
                at.set_bone_idx(bone);
                root.set_transform(rest.affine_inverse() * fitted);
                at.add_child(&root);
                sk.add_child(&at);
            }
            None => {
                root.set_transform(fitted);
                inner.add_child(&root);
            }
        }
        m.saddle = Some((root, scale));
    }

    /// The hero's model in the saddle on `mount`, or on foot (None): the
    /// seat's solver is made when a character first mounts, takes
    /// `SEAT_SECS` to settle them and to let them go, and sleeps on foot.
    pub fn sit(&mut self, m: &mut Model, mount: Option<&Mount>, delta: f32) {
        if m.seated.is_none() {
            let skeleton = mount.and_then(|_| find::<Skeleton3D>(&m.node.clone().upcast()));
            // a body with legs to put astride
            let Some(mut skeleton) = skeleton.filter(|s| s.find_bone("thigh_l") >= 0) else {
                return;
            };
            let mut seated = Seated::new_alloc();
            seated.set_name(SEATED);
            seated.bind_mut().set_body(m.node.clone());
            let mut modifier: Gd<SkeletonModifier3D> = seated.clone().upcast();
            modifier.set_influence(0.0);
            modifier.set_active(false);
            skeleton.add_child(&modifier);
            m.seated = Some(seated);
        }
        let Some(seated) = m.seated.as_mut() else {
            return;
        };
        let mut modifier: Gd<SkeletonModifier3D> = seated.clone().upcast();
        let now = modifier.get_influence();
        let to = if mount.is_some() { 1.0 } else { 0.0 };
        let step = (delta / SEAT_SECS).min(1.0);
        let next = if to > now {
            (now + step).min(to)
        } else {
            (now - step).max(to)
        };
        // leaving the saddle, the hips come off the back it was on
        if let Some(mount) = mount {
            seated.bind_mut().set_mount(Some(mount.clone()));
        } else if next <= 0.0 && seated.bind().mount().is_some() {
            seated.bind_mut().set_mount(None);
        }
        if next != now {
            modifier.set_influence(next);
            modifier.set_active(next > 0.0);
        }
    }

    /// On foot at once, and no saddle shown (the model is put away).
    pub(super) fn unseat(&mut self, m: &mut Model) {
        if let Some(seated) = m.seated.as_mut() {
            let mut modifier: Gd<SkeletonModifier3D> = seated.clone().upcast();
            modifier.set_influence(0.0);
            modifier.set_active(false);
            seated.bind_mut().set_mount(None);
        }
        if let Some((node, _)) = m.saddle.as_mut()
            && node.is_instance_valid()
        {
            node.set_visible(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A barrel on four legs: a body 2 long, 0.5 broad and 0.6 deep, its
    /// back at 1.5, legs at its ends down to the ground.
    fn beast() -> Vec<Vector3> {
        let mut points = Vec::new();
        for i in 0..=40 {
            let z = -1.0 + i as f32 * 0.05;
            for k in 0..24 {
                let a = k as f32 * std::f32::consts::TAU / 24.0;
                points.push(Vector3::new(0.25 * a.cos(), 1.2 + 0.3 * a.sin(), z));
            }
        }
        for (x, z) in [(0.2, 0.9), (-0.2, 0.9), (0.2, -0.9), (-0.2, -0.9)] {
            for i in 0..=18 {
                points.push(Vector3::new(x, i as f32 * 0.05, z));
            }
        }
        points
    }

    #[test]
    fn a_back_is_the_top_of_the_body_between_the_legs() {
        let b = back_of(&beast(), None).unwrap();
        assert!(
            (b.top - Vector3::new(0.0, 1.5, 0.0)).length() < 1e-3,
            "{b:?}"
        );
        assert!((b.half_width - 0.25).abs() < 1e-3, "{b:?}");
        assert!((b.belly - 0.9).abs() < 1e-3, "{b:?}");
        // over the shoulders the legs do not count for the breadth
        let b = back_of(&beast(), Some(0.9)).unwrap();
        assert!((b.top.y - 1.5).abs() < 1e-3 && (b.top.z - 0.9).abs() < 1e-3);
        assert!((b.half_width - 0.25).abs() < 1e-3, "{b:?}");
        // nothing to sit on
        assert_eq!(back_of(&[], None), None);
        assert_eq!(back_of(&beast(), Some(5.0)), None);
    }

    #[test]
    fn a_broader_back_takes_a_bigger_saddle_within_bounds() {
        assert_eq!(saddle_length(HORSE_HALF), SADDLE_LENGTH);
        assert!(saddle_length(0.1) < SADDLE_LENGTH);
        assert_eq!(saddle_length(0.01), SADDLE_LENGTH * SADDLE_SIZES.0);
        assert_eq!(saddle_length(2.0), SADDLE_LENGTH * SADDLE_SIZES.1);
    }
}
