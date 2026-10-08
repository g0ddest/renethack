//! What the things worn do to the hero's outfit (art direction §4; the
//! manifest's `worn` section, `nh_art::WornRules`): metal body armour puts
//! its material on the outfit's torso, gloves cover the hands in leather,
//! and a thing carried round the neck (a stethoscope) hangs there, skinned
//! to the skeleton as the outfit is. All of it comes off again when the
//! gear changes or the model goes back to the pool.

use std::collections::HashMap;

use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, Material, Mesh, MeshInstance3D, Node, Skeleton3D, Skin};
use godot::prelude::*;
use nh_art::{Gear, Look};

use super::{Art, cut, is_skin, mul, rgb};

/// The names of the meshes put on the skeleton, each followed by a number
/// (self-tests look for them).
pub const NECK_NODE: &str = "WornNeck";
pub const GLOVES_NODE: &str = "WornGloves";

/// How far up the forearm a glove's cuff reaches from the wrist, metres
/// (and where a sleeve ends that leaves the hand bare).
pub(super) const CUFF: f32 = 0.035;
/// How thick a glove is over the hand it covers.
const GLOVE: f32 = 0.003;
/// A mesh with a hand reaches at least this far past the wrist.
const FINGERS: f32 = 0.06;

/// What a model's outfit has on, to take off again.
#[derive(Default)]
pub struct Dressed {
    neck: Option<usize>,
    armour: Option<Look>,
    gloves: Option<Look>,
    /// The meshes put on the skeleton: the neck's, the gloves'.
    hung: Vec<Gd<MeshInstance3D>>,
    gloved: Vec<Gd<MeshInstance3D>>,
    /// Surfaces in the armour's material: (mesh, surface, what it had).
    plated: Vec<(Gd<MeshInstance3D>, i32, Option<Gd<Material>>)>,
}

/// A skinned mesh, its skin and the materials on its surfaces.
type Part = (Gd<Mesh>, Gd<Skin>, Vec<Option<Gd<Material>>>);

/// A model made on one body: its skinned meshes, and where the bones of
/// that body's skeleton lay at rest (by the skin).
#[derive(Clone)]
struct Fitted {
    parts: Vec<Part>,
    bones: Vec<(StringName, Vector3)>,
}

/// Built once and shared by every model.
#[derive(Default)]
pub struct Outfits {
    /// The fitted models, by manifest model.
    fitted: HashMap<usize, Option<Fitted>>,
    /// The gloves over a mesh's hands, by the mesh's id (None: no hands).
    gloves: HashMap<i64, Option<Gd<ArrayMesh>>>,
}

/// The skinned meshes of the scene a skeleton is in.
fn meshes(skeleton: &Gd<Skeleton3D>) -> Vec<Gd<MeshInstance3D>> {
    let scene: Gd<Node> = match skeleton.get_parent() {
        Some(p) => p,
        None => skeleton.clone().upcast(),
    };
    scene
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<MeshInstance3D>().ok())
        .filter(|mi| !mi.is_queued_for_deletion())
        .collect()
}

/// Off the skeleton at once, freed when the frame ends: what is put on
/// next in the same frame (other gloves on a model kept dressed) takes
/// the same name, and Godot would give it another while the old is there.
fn free(meshes: &mut Vec<Gd<MeshInstance3D>>) {
    for mut mi in meshes.drain(..) {
        if !mi.is_instance_valid() {
            continue;
        }
        if let Some(mut parent) = mi.get_parent() {
            parent.remove_child(&mi);
        }
        mi.queue_free();
    }
}

/// A mesh on the skeleton with `skin`, lit and shadowed as `like` is.
fn put_on(
    skeleton: &mut Gd<Skeleton3D>,
    name: String,
    mesh: &Gd<Mesh>,
    skin: &Gd<Skin>,
    like: Option<&Gd<MeshInstance3D>>,
) -> Gd<MeshInstance3D> {
    let mut mi = MeshInstance3D::new_alloc();
    mi.set_name(&name);
    mi.set_mesh(mesh);
    mi.set_skin(skin);
    if let Some(like) = like {
        mi.set_layer_mask(like.get_layer_mask());
        mi.set_cast_shadows_setting(like.get_cast_shadows_setting());
    }
    skeleton.add_child(&mi);
    mi.set_skeleton_path(&NodePath::from(".."));
    mi
}

/// How far from the body's middle the wrists of a mesh skinned by `skin`
/// lie, its arms out at rest: where the skin binds the left hand.
pub(super) fn wrist_of(skin: &Gd<Skin>) -> Option<f32> {
    let hand = StringName::from("hand_l");
    (0..skin.get_bind_count())
        .find(|&i| skin.get_bind_name(i) == hand)
        .map(|i| skin.get_bind_pose(i).affine_inverse().origin.x.abs())
}

/// Whether `mesh` reaches past the wrists as far as fingers do: it has
/// the hands (a bracer or a short sleeve does not).
pub(super) fn has_hands(mesh: &Gd<ArrayMesh>, wrist: f32) -> bool {
    let bounds = mesh.get_aabb();
    bounds.position.x.abs().max(bounds.end().x.abs()) >= wrist + FINGERS
}

/// The gloves over the hands of `mesh` (a body's or a sleeve's, skinned by
/// `skin`): its faces from a cuff's length above the wrists outward, a
/// glove's thickness out along their normals. None when it has no hand.
fn gloves_over(mesh: &Gd<ArrayMesh>, skin: &Gd<Skin>) -> Option<Gd<ArrayMesh>> {
    let wrist = wrist_of(skin).filter(|w| has_hands(mesh, *w))?;
    let (hands, _) = cut(mesh, |v| v.x.abs() > wrist - CUFF);
    let out = swollen(&hands, GLOVE)?;
    (out.get_surface_count() > 0).then_some(out)
}

/// `mesh` with every vertex moved `by` out along its normal (a layer
/// over the same body); its surfaces keep their materials.
pub(super) fn swollen(mesh: &Gd<ArrayMesh>, by: f32) -> Option<Gd<ArrayMesh>> {
    let hands = mesh;
    let mut out = ArrayMesh::new_gd();
    for i in 0..hands.get_surface_count() {
        let mut arrays = hands.surface_get_arrays(i);
        let verts: PackedVector3Array = arrays.at(ArrayType::VERTEX.ord() as usize).to();
        let normals: PackedVector3Array = arrays.at(ArrayType::NORMAL.ord() as usize).to();
        let out_by: Vec<Vector3> = verts
            .as_slice()
            .iter()
            .zip(normals.as_slice())
            .map(|(v, n)| *v + *n * by)
            .collect();
        if out_by.len() != verts.len() {
            return None;
        }
        arrays.set(
            ArrayType::VERTEX.ord() as usize,
            &PackedVector3Array::from(out_by.as_slice()).to_variant(),
        );
        let eight = hands.surface_get_format(i).ord() & ArrayFormat::FLAG_USE_8_BONE_WEIGHTS.ord();
        out.add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays)
            .flags(ArrayFormat::from_ord(eight))
            .done();
        if let Some(m) = hands.surface_get_material(i) {
            out.surface_set_material(i, &m);
        }
    }
    Some(out)
}

impl Art {
    /// Put on what `gear` wears and take off what it no longer does: the
    /// armour's material, the gloves (shaded as the outfit is), the thing
    /// round the neck.
    pub(super) fn wear(
        &mut self,
        on: &mut Dressed,
        skeleton: &Gd<Skeleton3D>,
        gear: &Gear,
        shade: Color,
    ) {
        if on.armour != gear.armour {
            self.plate(on, skeleton, gear.armour);
        }
        if on.gloves != gear.gloves {
            self.glove(on, skeleton, gear.gloves, shade);
        }
        if on.neck != gear.neck {
            self.hang(on, skeleton, gear.neck);
        }
    }

    /// Everything off: the outfit as it was built.
    pub(super) fn take_off(&mut self, on: &mut Dressed) {
        free(&mut on.hung);
        free(&mut on.gloved);
        unplate(on);
        *on = Dressed::default();
    }

    /// The armour's material on the outfit's parts its rule names; the
    /// skin a part shows (a sleeve's hand) stays skin. Metal keeps its own
    /// brightness (the outfit's shade mutes cloth).
    fn plate(&mut self, on: &mut Dressed, skeleton: &Gd<Skeleton3D>, look: Option<Look>) {
        unplate(on);
        on.armour = look;
        let Some(look) = look else {
            return;
        };
        let rule = self.manifest.worn_rules().armour[look.rule].clone();
        let Some(material) = rule
            .material
            .as_deref()
            .and_then(|m| self.manifest.material_index(m))
        else {
            return;
        };
        let material = self.surface(material, 100, false, rgb(look.tint));
        for mut mi in meshes(skeleton) {
            let name = mi.get_name().to_string();
            if !rule.parts.iter().any(|p| name.ends_with(p.as_str())) {
                continue;
            }
            let Some(mesh) = mi.get_mesh() else {
                continue;
            };
            for s in 0..mesh.get_surface_count() {
                if mesh.surface_get_material(s).is_some_and(|m| is_skin(&m)) {
                    continue;
                }
                let had = mi.get_surface_override_material(s);
                mi.set_surface_override_material(s, &material);
                on.plated.push((mi.clone(), s, had));
            }
        }
    }

    /// Gloves of the look's leather over every hand the model shows: the
    /// outfit's own sleeves', or its bare arms'.
    fn glove(
        &mut self,
        on: &mut Dressed,
        skeleton: &Gd<Skeleton3D>,
        look: Option<Look>,
        shade: Color,
    ) {
        free(&mut on.gloved);
        on.gloves = look;
        let Some(look) = look else {
            return;
        };
        let rule = &self.manifest.worn_rules().gloves[look.rule];
        let Some(material) = rule
            .material
            .as_deref()
            .and_then(|m| self.manifest.material_index(m))
        else {
            return;
        };
        let material = self.surface(material, 100, false, mul(rgb(look.tint), shade));
        let mut skeleton = skeleton.clone();
        for mi in meshes(&skeleton) {
            if !mi.is_visible() || mi.get_name().to_string().starts_with(NECK_NODE) {
                continue;
            }
            let Some(mesh) = mi.get_mesh().and_then(|m| m.try_cast::<ArrayMesh>().ok()) else {
                continue;
            };
            let Some(skin) = mi.get_skin() else {
                continue;
            };
            let key = mesh.instance_id().to_i64();
            let gloves = self
                .outfits
                .gloves
                .entry(key)
                .or_insert_with(|| gloves_over(&mesh, &skin))
                .clone();
            let Some(gloves) = gloves else {
                continue;
            };
            let name = format!("{GLOVES_NODE}{}", on.gloved.len());
            let mut g = put_on(
                &mut skeleton,
                name,
                &gloves.upcast::<Mesh>(),
                &skin,
                Some(&mi),
            );
            g.set_material_override(&material);
            on.gloved.push(g);
        }
    }

    /// The thing round the neck: of its rule's models, the one fitted to
    /// the skeleton nearest this one.
    fn hang(&mut self, on: &mut Dressed, skeleton: &Gd<Skeleton3D>, rule: Option<usize>) {
        free(&mut on.hung);
        on.neck = rule;
        let Some(rule) = rule else {
            return;
        };
        let models: Vec<usize> = self.manifest.worn_rules().neck[rule]
            .models
            .iter()
            .filter_map(|m| self.manifest.model_index(m))
            .collect();
        let mut best: Option<(f32, Fitted)> = None;
        for model in models {
            let Some(fitted) = self.fitted(model) else {
                continue;
            };
            // how far its body's bones lie from this skeleton's, on average
            let apart: Vec<f32> = fitted
                .bones
                .iter()
                .filter_map(|(name, at)| {
                    let bone = skeleton.find_bone(name.to_string().as_str());
                    (bone >= 0).then(|| skeleton.get_bone_global_rest(bone).origin.distance_to(*at))
                })
                .collect();
            if apart.is_empty() {
                continue;
            }
            let apart = apart.iter().sum::<f32>() / apart.len() as f32;
            if best.as_ref().is_none_or(|(b, _)| apart < *b) {
                best = Some((apart, fitted));
            }
        }
        let Some((_, fitted)) = best else {
            return;
        };
        let mut skeleton = skeleton.clone();
        let like = meshes(&skeleton).into_iter().find(|m| m.is_visible());
        for (mesh, skin, materials) in &fitted.parts {
            let name = format!("{NECK_NODE}{}", on.hung.len());
            let mut mi = put_on(&mut skeleton, name, mesh, skin, like.as_ref());
            for (s, m) in materials.iter().enumerate() {
                if let Some(m) = m {
                    mi.set_surface_override_material(s as i32, m);
                }
            }
            on.hung.push(mi);
        }
    }

    /// A fitted model's parts, read from its scene once.
    fn fitted(&mut self, model: usize) -> Option<Fitted> {
        if let Some(f) = self.outfits.fitted.get(&model) {
            return f.clone();
        }
        let fitted = self.scene(model).and_then(|scene| {
            let mut inst = scene.instantiate()?;
            let mut parts = Vec::new();
            let mut bones = Vec::new();
            for node in inst
                .find_children_ex("*")
                .type_("MeshInstance3D")
                .owned(false)
                .done()
                .iter_shared()
            {
                let Ok(mi) = node.try_cast::<MeshInstance3D>() else {
                    continue;
                };
                let (Some(mesh), Some(skin)) = (mi.get_mesh(), mi.get_skin()) else {
                    continue;
                };
                if bones.is_empty() {
                    bones = (0..skin.get_bind_count())
                        .map(|i| {
                            let at = skin.get_bind_pose(i).affine_inverse().origin;
                            (skin.get_bind_name(i), at)
                        })
                        .collect();
                }
                let materials = (0..mesh.get_surface_count())
                    .map(|s| mi.get_active_material(s))
                    .collect();
                parts.push((mesh, skin, materials));
            }
            inst.queue_free();
            (!parts.is_empty()).then_some(Fitted { parts, bones })
        });
        if fitted.is_none() {
            let name = self.manifest.model_at(model).0.to_string();
            self.warn_once(format!("worn model {name} has no skinned mesh"));
        }
        self.outfits.fitted.insert(model, fitted.clone());
        fitted
    }
}

/// The armour's material off: every surface as it was.
fn unplate(on: &mut Dressed) {
    for (mut mi, s, had) in on.plated.drain(..) {
        if !mi.is_instance_valid() {
            continue;
        }
        match had {
            Some(m) => mi.set_surface_override_material(s, &m),
            None => mi.set_surface_override_material(s, Gd::null_arg()),
        }
    }
}
