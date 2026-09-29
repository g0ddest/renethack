//! What the hero carries, on the bones of their model (art direction §4):
//! the manifest's `held` section says which model each slot shows and how
//! it sits in the hand; a lit lamp brings its flickering light; armour
//! toggles the outfit's parts; the idle and attack clips follow the gear. An
//! item used is shown in the hand for a moment. Held models are built when
//! the gear changes and freed when it changes again (or the model goes back
//! to the pool), so nothing piles up.

use std::collections::HashMap;

use godot::classes::animation::{LoopMode, TrackType};
use godot::classes::light_3d::Param;
use godot::classes::{
    Animation, AnimationLibrary, AnimationPlayer, BoneAttachment3D, MeshInstance3D, Node, Node3D,
    OmniLight3D, Skeleton3D,
};
use godot::prelude::*;
use nh_art::{Gear, HeldArt, LightSpec, PROC_CLIPS, Resolved, Tint};

use super::{Art, Finish, Model, ModelLook, Pose, find, transform};
use crate::meshes::sphere;

/// The node names under a bone attachment (self-tests look for them).
pub const HELD_NODE: &str = "Held";
pub const USE_NODE: &str = "InUse";
pub const LAMP_LIGHT: &str = "LampLight";
/// Visual layers of held things: the map's and the hero's rim light's.
const HELD_LAYERS: u32 = 1 | 1 << 1;
/// Seconds of one cycle of a lamp's flicker.
const FLICKER_SECS: f32 = 2.4;
const FLICKER_KEYS: i32 = 48;

/// The gear on one model's bones.
#[derive(Default)]
pub struct Worn {
    skeleton: Option<Gd<Skeleton3D>>,
    bones: HashMap<String, Gd<BoneAttachment3D>>,
    gear: Gear,
    /// The node on each slot now.
    held: Vec<(&'static str, Gd<Node3D>)>,
    /// The outfit's meshes some part rule governs, and the rule.
    parts: Option<Vec<(Gd<MeshInstance3D>, usize)>>,
    /// An item used, in the right hand for a few seconds more.
    in_use: Option<(Gd<Node3D>, f32)>,
}

impl Worn {
    pub fn gear(&self) -> &Gear {
        &self.gear
    }
}

fn tint_of(t: Tint) -> Color {
    match t {
        Tint::Rgb(c, s) => {
            let c = Color::from_rgb(c[0], c[1], c[2]);
            Color::WHITE.lerp(c, f64::from(s))
        }
        _ => Color::WHITE,
    }
}

fn set_layers(node: &Gd<Node>, mask: u32) {
    for n in node
        .find_children_ex("*")
        .type_("VisualInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if let Ok(mut v) = n.try_cast::<godot::classes::VisualInstance3D>() {
            v.set_layer_mask(mask);
        }
    }
}

/// A lamp's flicker: its energy wavers and the flame sways a little, as
/// the wall torches do.
fn flicker_animation(energy: f32, amount: f32, at: Vector3) -> Gd<Animation> {
    let mut a = Animation::new_gd();
    a.set_length(FLICKER_SECS);
    a.set_loop_mode(LoopMode::LINEAR);
    let e = a.add_track(TrackType::VALUE);
    a.track_set_path(e, &NodePath::from(".:light_energy"));
    let p = a.add_track(TrackType::VALUE);
    a.track_set_path(p, &NodePath::from(".:position"));
    let tau = std::f32::consts::TAU;
    for k in 0..=FLICKER_KEYS {
        let f = k as f32 / FLICKER_KEYS as f32;
        let t = f * FLICKER_SECS;
        // whole cycles of each wave in the loop
        let w = 0.55 * (tau * 5.0 * f).sin()
            + 0.3 * (tau * 11.0 * f + 1.3).sin()
            + 0.15 * (tau * 23.0 * f + 0.4).sin();
        a.track_insert_key(e, f64::from(t), &(energy * (1.0 + amount * w)).to_variant());
        let sway = Vector3::new(
            0.012 * (tau * 7.0 * f).sin(),
            0.0,
            0.012 * (tau * 9.0 * f + 0.7).sin(),
        );
        a.track_insert_key(p, f64::from(t), &(at + sway).to_variant());
    }
    a
}

impl Art {
    /// Show `gear` on the model (a character's bones; other bodies show
    /// nothing); an item in use leaves the hand after its time. Cheap when
    /// nothing changed. True when a lit light is in hand.
    pub fn equip(&mut self, m: &mut Model, gear: &Gear, delta: f32) -> bool {
        if m.worn.is_none() {
            let skeleton = find::<Skeleton3D>(&m.node.clone().upcast());
            m.worn = Some(Box::new(Worn {
                skeleton,
                ..Worn::default()
            }));
        }
        let Some(worn) = m.worn.as_mut() else {
            return false;
        };
        if worn.skeleton.is_none() {
            return false;
        }
        if let Some((node, left)) = worn.in_use.as_mut() {
            *left -= delta;
            if *left <= 0.0 {
                let mut node = node.clone();
                if node.is_instance_valid() {
                    node.queue_free();
                }
                worn.in_use = None;
                show_slot(worn, "hand_r", true);
            }
        }
        if worn.gear == *gear {
            return gear.lit();
        }
        let old = std::mem::take(&mut worn.gear);
        let mut worn = m.worn.take().expect("worn");
        for (slot, want) in gear.slots() {
            let had = old
                .slots()
                .into_iter()
                .find(|(s, _)| *s == slot)
                .and_then(|(_, h)| h);
            if had == want {
                continue;
            }
            if let Some(i) = worn.held.iter().position(|(s, _)| *s == slot) {
                let (_, mut node) = worn.held.swap_remove(i);
                if node.is_instance_valid() {
                    node.queue_free();
                }
            }
            if let Some(h) = want
                && let Some(node) = self.hold(&mut worn, slot, h, HELD_NODE)
            {
                worn.held.push((slot, node));
            }
        }
        if worn.in_use.is_some() {
            show_slot(&mut worn, "hand_r", false);
        }
        self.show_parts(&mut worn, &gear.worn);
        worn.gear = gear.clone();
        m.worn = Some(worn);
        // the idle the gear calls for, unless the model is busy
        let spec = self.manifest.model_at(m.key.model).1;
        let old_idle = old.idle.clone().or(spec.anims.idle.clone());
        let new_idle = gear.idle.clone().or(spec.anims.idle.clone());
        if let (Some(p), Some(idle)) = (m.player.as_mut(), new_idle)
            && old_idle.as_deref() != Some(idle.as_str())
            && p.has_animation(idle.as_str())
        {
            let now = p.get_current_animation().to_string();
            if !p.is_playing() || Some(now.as_str()) == old_idle.as_deref() {
                if let Some(mut a) = p.get_animation(idle.as_str()) {
                    a.set_loop_mode(LoopMode::LINEAR);
                }
                p.play_ex().name(idle.as_str()).custom_blend(0.25).done();
            }
        }
        gear.lit()
    }

    /// Take the gear off (the model goes back to the pool).
    pub(super) fn unequip(&mut self, m: &mut Model) {
        let Some(worn) = m.worn.as_mut() else {
            return;
        };
        for (_, mut node) in worn.held.drain(..) {
            if node.is_instance_valid() {
                node.queue_free();
            }
        }
        if let Some((mut node, _)) = worn.in_use.take()
            && node.is_instance_valid()
        {
            node.queue_free();
        }
        if let Some(parts) = &worn.parts {
            for (mi, _) in parts {
                let mut mi = mi.clone();
                mi.set_visible(true);
            }
        }
        worn.gear = Gear::default();
    }

    /// Show an item in the right hand for `secs` (a potion drunk, a wand
    /// zapped); the weapon there steps aside meanwhile.
    pub fn hold_for(&mut self, m: &mut Model, h: HeldArt, secs: f32) -> bool {
        let Some(mut worn) = m.worn.take() else {
            return false;
        };
        if let Some((mut old, _)) = worn.in_use.take()
            && old.is_instance_valid()
        {
            old.queue_free();
        }
        let node = self.hold(&mut worn, "hand_r", h, USE_NODE);
        let shown = node.is_some();
        if let Some(node) = node {
            worn.in_use = Some((node, secs));
            show_slot(&mut worn, "hand_r", false);
        }
        m.worn = Some(worn);
        shown
    }

    /// A held thing on its own (a thrown one in flight), `scale` times
    /// its size in the hand.
    pub fn held_prop(&mut self, h: HeldArt, scale: f32) -> Option<Gd<Node3D>> {
        let spec = self.manifest.held_at(h.held).1.clone();
        let model = self.held_model(h)?;
        let k = h.scale * scale;
        let mut holder = Node3D::new_alloc();
        holder.set_transform(transform(spec.grip.pos, spec.grip.rot, [k, k, k]));
        holder.add_child(&model);
        Some(holder)
    }

    fn bone(&mut self, worn: &mut Worn, bone: &str) -> Option<Gd<BoneAttachment3D>> {
        if let Some(b) = worn.bones.get(bone).filter(|b| b.is_instance_valid()) {
            return Some(b.clone());
        }
        let mut skeleton = worn.skeleton.clone()?;
        skeleton.find_bone(bone).ge(&0).then_some(())?;
        let mut b = BoneAttachment3D::new_alloc();
        b.set_name(&format!("Gear_{bone}"));
        b.set_bone_name(bone);
        skeleton.add_child(&b);
        worn.bones.insert(bone.to_string(), b.clone());
        Some(b)
    }

    /// A held model on its slot's bone: slot grip, the model's grip and
    /// size, its light when lit.
    fn hold(&mut self, worn: &mut Worn, slot: &str, h: HeldArt, name: &str) -> Option<Gd<Node3D>> {
        let slot_spec = self.manifest.held_slot(slot)?.clone();
        let spec = self.manifest.held_at(h.held).1.clone();
        let mut bone = self.bone(worn, &slot_spec.bone)?;
        let model = self.held_model(h)?;
        let mut root = Node3D::new_alloc();
        root.set_name(&format!("{name}_{}", self.manifest.held_at(h.held).0));
        root.set_transform(transform(slot_spec.pos, slot_spec.rot, [1.0; 3]));
        // sizes are the body's own (a character 1.6 tall): the bone scales
        // them with it
        let k = h.scale;
        let grip = transform(spec.grip.pos, spec.grip.rot, [k, k, k]);
        let mut holder = Node3D::new_alloc();
        holder.set_transform(grip);
        holder.add_child(&model);
        root.add_child(&holder);
        if h.lit
            && let Some(light) = &spec.light
        {
            let at = Vector3::new(light.at[0], light.at[1], light.at[2]);
            self.light_up(&mut root, light, grip * at);
        }
        set_layers(&root.clone().upcast(), HELD_LAYERS);
        bone.add_child(&root);
        Some(root)
    }

    /// The model of a held thing, dressed as it looks.
    fn held_model(&mut self, h: HeldArt) -> Option<Gd<Node3D>> {
        let spec = self.manifest.model_at(h.model).1.clone();
        let held = self.manifest.held_at(h.held).1.clone();
        let look = ModelLook {
            art: Resolved {
                model: h.model,
                scale: 1.0,
                lift: 0.0,
                rot: [0.0; 3],
                height: 1.0,
                tint: h.tint,
                skin: h.skin,
                level: h.level,
            },
            tint: tint_of(h.tint),
            pose: Pose::Alive,
        };
        let inner = match (spec.proc, self.scene(h.model)) {
            (Some(kind), _) => {
                let (root, player) = self.build_proc(kind, &spec, &look);
                if let Some(mut p) = player {
                    p.queue_free();
                }
                root
            }
            (None, Some(scene)) => {
                let inner = scene.instantiate_as::<Node3D>();
                for hide in &held.hide {
                    if let Some(mut n) = inner
                        .find_child(hide)
                        .and_then(|n| n.try_cast::<Node3D>().ok())
                    {
                        n.set_visible(false);
                    }
                }
                let shade = super::rgb(spec.shade_rgb());
                self.dress(&inner, &look, shade);
                inner
            }
            (None, None) => return None,
        };
        Some(inner)
    }

    /// A burning light's glow and light at `at` (the slot's space).
    fn light_up(&mut self, root: &mut Gd<Node3D>, spec: &LightSpec, at: Vector3) {
        let c = spec.rgb();
        let color = Color::from_rgb(c[0], c[1], c[2]);
        let mut light = OmniLight3D::new_alloc();
        light.set_name(LAMP_LIGHT);
        light.set_color(color);
        light.set_param(Param::ENERGY, spec.energy);
        light.set_param(Param::RANGE, spec.range);
        light.set_param(Param::ATTENUATION, 1.0);
        light.set_param(Param::SHADOW_BIAS, 0.03);
        light.set_param(Param::SHADOW_NORMAL_BIAS, 1.2);
        light.set_param(Param::SHADOW_BLUR, 1.5);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.0);
        light.set_shadow(true);
        light.set_position(at);
        // the flame itself glows (the light never lights its own inside)
        let flame = self.flat(Color::from_rgb(1.0, 0.62, 0.28), Finish::Ember);
        let mut core = MeshInstance3D::new_alloc();
        core.set_mesh(&self.mesh(sphere(0.018)));
        core.set_material_override(&flame);
        crate::art::no_shadow(&mut core);
        light.add_child(&core);
        if spec.flicker > 0.0 {
            let mut lib = AnimationLibrary::new_gd();
            let _ = lib.add_animation("flicker", &flicker_animation(spec.energy, spec.flicker, at));
            let mut p = AnimationPlayer::new_alloc();
            let _ = p.add_animation_library("", &lib);
            light.add_child(&p);
            p.set_autoplay("flicker");
        }
        root.add_child(&light);
    }

    fn show_parts(&mut self, worn: &mut Worn, slots: &[&str]) {
        let rules = self.manifest.held_rules().parts.clone();
        if worn.parts.is_none() {
            let mut parts = Vec::new();
            if let Some(skeleton) = &worn.skeleton
                && let Some(scene) = skeleton.get_parent()
            {
                for n in scene
                    .find_children_ex("*")
                    .type_("MeshInstance3D")
                    .owned(false)
                    .done()
                    .iter_shared()
                {
                    let name = n.get_name().to_string();
                    if let Some(i) = rules.iter().position(|r| name.ends_with(&r.suffix))
                        && let Ok(mi) = n.try_cast::<MeshInstance3D>()
                    {
                        parts.push((mi, i));
                    }
                }
            }
            worn.parts = Some(parts);
        }
        for (mi, i) in worn.parts.iter().flatten() {
            let mut mi = mi.clone();
            if mi.is_instance_valid() {
                mi.set_visible(rules[*i].shown(slots));
            }
        }
    }

    /// The clip a use plays on this model, if it has it: from the
    /// manifest's `held.uses` ("proc/read" is built here).
    pub fn use_clip(&mut self, m: &Model, kind: &str) -> Option<String> {
        let name = self.manifest.held_rules().uses.get(kind)?.clone();
        let mut player = m.player.clone()?;
        if name.starts_with(&format!("{PROC_CLIPS}/")) && !player.has_animation(name.as_str()) {
            self.add_proc_clips(&mut player);
        }
        player.has_animation(name.as_str()).then_some(name)
    }

    /// The clips built in code on the shared skeleton: `read` is the idle
    /// with both forearms raised to hold a page and the head bowed to it.
    fn add_proc_clips(&mut self, player: &mut Gd<AnimationPlayer>) {
        if player.has_animation_library(PROC_CLIPS) {
            return;
        }
        let lib = match &self.proc_clips {
            Some(l) => l.clone(),
            None => {
                let Some(idle) = player.get_animation("Idle") else {
                    return;
                };
                let mut lib = AnimationLibrary::new_gd();
                let _ = lib.add_animation("read", &read_animation(&idle));
                self.proc_clips = Some(lib.clone());
                lib
            }
        };
        let _ = player.add_animation_library(PROC_CLIPS, &lib);
    }
}

/// Bone turns (bone, local axis, degrees) of the read pose: in the idle
/// both arms hang with their x to the hero's right, so a turn about x
/// swings them forward; the head bows about its own x.
const READ_POSE: [(&str, Vector3, f32); 6] = [
    ("upperarm_l", Vector3::new(1.0, 0.0, 0.0), 28.0),
    ("upperarm_r", Vector3::new(1.0, 0.0, 0.0), 28.0),
    ("lowerarm_l", Vector3::new(1.0, 0.0, 0.0), 70.0),
    ("lowerarm_r", Vector3::new(1.0, 0.0, 0.0), 70.0),
    ("neck_01", Vector3::new(1.0, 0.0, 0.0), 10.0),
    ("Head", Vector3::new(1.0, 0.0, 0.0), 18.0),
];

fn read_animation(idle: &Gd<Animation>) -> Gd<Animation> {
    let mut a = idle.duplicate_resource();
    a.set_loop_mode(LoopMode::NONE);
    let len = f64::from(a.get_length());
    for t in 0..a.get_track_count() {
        if a.track_get_type(t) != TrackType::ROTATION_3D {
            continue;
        }
        let path = a.track_get_path(t).to_string();
        let Some((_, axis, deg)) = READ_POSE
            .iter()
            .find(|(b, _, _)| path.ends_with(&format!(":{b}")))
        else {
            continue;
        };
        let turn = Quaternion::from_axis_angle(*axis, deg.to_radians());
        for k in 0..a.track_get_key_count(t) {
            let time = a.track_get_key_time(t, k);
            // in and out of the pose over the first and last fifth
            let w = ((time / len).min(1.0 - time / len) * 5.0).clamp(0.0, 1.0) as f32;
            let q: Quaternion = a.track_get_key_value(t, k).to();
            let q = q * Quaternion::IDENTITY.slerp(turn, w);
            a.track_set_key_value(t, k, &q.to_variant());
        }
    }
    a
}

fn show_slot(worn: &mut Worn, slot: &str, on: bool) {
    for (s, node) in &worn.held {
        if *s == slot && node.is_instance_valid() {
            let mut node = node.clone();
            node.set_visible(on);
        }
    }
}
