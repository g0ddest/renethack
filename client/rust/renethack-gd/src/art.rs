//! The art library: the manifest (via nh-art), PBR materials, scenes and
//! animation libraries loaded once through the ResourceLoader, and pooled
//! model instances. A model instance is taken for a `ModelLook` and given
//! back when its cell no longer shows it; instances of the same look are
//! reused (a level change reuses the previous level's models), so nodes
//! never pile up.
//!
//! A scene file that is missing is reported once and drawn as a procedural
//! blob instead. Procedural bodies (serpents, bugs, bats, rings, wands...)
//! are built here from primitive meshes with the project's materials.

use std::collections::{HashMap, HashSet};

use godot::classes::animation::{LoopMode, TrackType};
use godot::classes::base_material_3d::{
    CullMode, Feature, Flags, ShadingMode, TextureParam, Transparency,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{
    Animation, AnimationLibrary, AnimationPlayer, ArrayMesh, BaseMaterial3D, BoneAttachment3D,
    FileAccess, Material, Mesh, MeshInstance3D, Node, Node3D, OrmMaterial3D, PackedScene,
    ResourceLoader, Skeleton3D, Skin as GdSkin, StandardMaterial3D, SurfaceTool, Texture2D,
};
use godot::prelude::*;
use nh_art::{ArtManifest, MaterialSpec, Proc, Resolved, Skin};

#[path = "equip.rs"]
mod equip;

pub use equip::{HELD_NODE, LAMP_LIGHT, THROW_LETS_GO, USE_NODE, Worn};

use crate::meshes::{MeshKey, capsule, cuboid, cylinder, dome, facets, prism, sphere, torus};

/// The manifest built into the client, used when the project's copy cannot
/// be read.
const BUILT_IN_MANIFEST: &str = include_str!("../../../godot/art/manifest.json");
const ART_ROOT: &str = "res://art/";
/// Metals keep some colour of their own under the torch.
const MAX_METALLIC: f32 = 0.45;
/// Pooled instances kept per look; more are freed.
const POOL_MAX: usize = 24;

/// How a thing is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pose {
    Alive,
    /// Lying dead (a death animation's last frame, or on its side).
    Corpse,
    /// Frozen still, in the statue's stone (the skin says which).
    Statue,
    /// See-through: sensed, or seen invisible.
    Ghost,
}

/// A model on a cell, as plain data (see `map_view::Look`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelLook {
    pub art: Resolved,
    /// Multiplies the model's colours (white: as it is).
    pub tint: Color,
    pub pose: Pose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PoolKey {
    model: usize,
    skin: Skin,
    tint: u32,
    pose: Pose,
}

impl PoolKey {
    fn of(look: &ModelLook) -> PoolKey {
        PoolKey {
            model: look.art.model,
            skin: look.art.skin,
            tint: color_key(look.tint),
            pose: look.pose,
        }
    }
}

/// A model instance on the map.
pub struct Model {
    /// Placed by the map; its child holds the model's own transform.
    pub node: Gd<Node3D>,
    key: PoolKey,
    player: Option<Gd<AnimationPlayer>>,
    /// What it carries (the hero only).
    worn: Option<Box<Worn>>,
    /// What its look wears on its bones (a role's hat), by the slot that
    /// hides it while something is worn there.
    extras: Vec<(Option<String>, Gd<Node3D>)>,
}

impl Model {
    /// The manifest model it is an instance of (self-tests).
    pub fn model_index(&self) -> usize {
        self.key.model
    }

    pub fn player(&self) -> Option<&Gd<AnimationPlayer>> {
        self.player.as_ref().filter(|p| p.is_instance_valid())
    }
}

/// A model's animation player and the clips it has that the map plays
/// (each one checked to be in the player).
#[derive(Clone, Default)]
pub struct Clips {
    pub player: Option<Gd<AnimationPlayer>>,
    pub idle: Option<String>,
    /// The walk, or the run in a hurry (None: the map sways the model).
    pub gait: Option<String>,
    pub attack: Option<String>,
}

pub fn color_key(c: Color) -> u32 {
    c.to_u32(godot::builtin::ColorChannelOrder::RGBA)
}

fn rgb(c: [f32; 3]) -> Color {
    Color::from_rgb(c[0], c[1], c[2])
}

fn mul(a: Color, b: Color) -> Color {
    Color::from_rgba(a.r * b.r, a.g * b.g, a.b * b.b, a.a * b.a)
}

fn deg(v: [f32; 3]) -> Vector3 {
    Vector3::new(v[0], v[1], v[2]) * (std::f32::consts::PI / 180.0)
}

fn transform(pos: [f32; 3], rot: [f32; 3], scale: [f32; 3]) -> Transform3D {
    let basis = Basis::from_euler(EulerOrder::YXZ, deg(rot))
        * Basis::from_scale(Vector3::new(scale[0], scale[1], scale[2]));
    Transform3D::new(basis, Vector3::new(pos[0], pos[1], pos[2]))
}

/// Something to load before it is first needed.
enum Preload {
    Texture(String),
    Library(String, bool),
    Scene(usize),
}

/// Time per frame the preloading may take.
const PRELOAD_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);

/// A material key: (manifest material, brightness %, world UVs, tint).
type SurfaceKey = (usize, u8, bool, u32);

pub struct Art {
    manifest: ArtManifest,
    root: Gd<Node3D>,
    meshes: HashMap<MeshKey, Gd<Mesh>>,
    textures: HashMap<String, Option<Gd<Texture2D>>>,
    surfaces: HashMap<SurfaceKey, Gd<Material>>,
    flat: HashMap<(u32, u8), Gd<Material>>,
    scenes: HashMap<usize, Option<Gd<PackedScene>>>,
    libraries: HashMap<(String, bool), Option<Gd<AnimationLibrary>>>,
    derived: HashMap<(i64, u32, bool, Option<usize>), Gd<Material>>,
    proc_anims: HashMap<Proc, Gd<AnimationLibrary>>,
    /// The clips built in code on the characters' skeleton (`proc/read`).
    proc_clips: Option<Gd<AnimationLibrary>>,
    /// Meshes shaded smooth, by the source mesh's id.
    smoothed: HashMap<i64, Gd<Mesh>>,
    /// The meshes (and their skins) of each base head, built once.
    heads: HashMap<String, Option<HeadParts>>,
    pool: HashMap<PoolKey, Vec<Model>>,
    warned: HashSet<String>,
    /// Instances handed out and not given back.
    live: usize,
    /// Instances ever built (self-tests watch the pool work).
    built: usize,
    phase: u32,
    /// What is still to load ahead of need, last first.
    preload: Vec<Preload>,
}

impl Art {
    /// `root`: where model instances live (the map's cells node).
    pub fn new(root: Gd<Node3D>) -> Art {
        let text = FileAccess::get_file_as_string(&format!("{ART_ROOT}manifest.json"));
        let mut manifest = match ArtManifest::parse(&text.to_string()) {
            Ok(m) => m,
            Err(e) => {
                godot_warn!("renethack: {e}; using the art manifest built into the client");
                ArtManifest::parse(BUILT_IN_MANIFEST).expect("the built-in art manifest is valid")
            }
        };
        // a scene Godot cannot load is drawn as a tinted blob instead
        let mut loader = ResourceLoader::singleton();
        let missing: Vec<(usize, String)> = manifest
            .models()
            .filter_map(|(i, _, m)| Some((i, m.scene.clone()?)))
            .filter(|(_, s)| !loader.exists(&format!("{ART_ROOT}{s}")))
            .collect();
        for (i, scene) in missing {
            godot_warn!("renethack: art {scene} is missing (not imported?); drawn procedurally");
            manifest.replace_with_proc(i, Proc::Blob);
        }
        Art {
            manifest,
            root,
            meshes: HashMap::new(),
            textures: HashMap::new(),
            surfaces: HashMap::new(),
            flat: HashMap::new(),
            scenes: HashMap::new(),
            libraries: HashMap::new(),
            derived: HashMap::new(),
            proc_anims: HashMap::new(),
            proc_clips: None,
            smoothed: HashMap::new(),
            heads: HashMap::new(),
            pool: HashMap::new(),
            warned: HashSet::new(),
            live: 0,
            built: 0,
            phase: 0,
            preload: Vec::new(),
        }
        .with_preload()
    }

    /// Queue every texture, animation library and scene the manifest names:
    /// loaded a few per frame from the title screen on, a level's first
    /// sight does not wait for the disk.
    fn with_preload(mut self) -> Art {
        let mut q = Vec::new();
        for (_, _, m) in self.manifest.materials() {
            q.extend(m.albedo_path().map(Preload::Texture));
            q.extend(m.normal_path().map(Preload::Texture));
            q.extend(m.arm_path().map(Preload::Texture));
        }
        let mut rigs: Vec<(String, bool)> = self
            .manifest
            .models()
            .flat_map(|(_, _, m)| {
                let rigs = m.rig.iter().chain(&m.extra_rigs);
                rigs.map(|r| (r.clone(), m.strip_root)).collect::<Vec<_>>()
            })
            .collect();
        rigs.sort();
        rigs.dedup();
        q.extend(rigs.into_iter().map(|(r, s)| Preload::Library(r, s)));
        q.extend(
            self.manifest
                .models()
                .filter(|(_, _, m)| m.scene.is_some())
                .map(|(i, _, _)| Preload::Scene(i)),
        );
        q.reverse();
        self.preload = q;
        self
    }

    /// Load ahead for a few milliseconds; false when all is loaded.
    pub fn preload_step(&mut self) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < PRELOAD_BUDGET {
            match self.preload.pop() {
                Some(Preload::Texture(t)) => {
                    self.texture(&t);
                }
                Some(Preload::Library(name, strip)) => {
                    self.library(&name, strip);
                }
                Some(Preload::Scene(i)) => {
                    self.scene(i);
                }
                None => return false,
            }
        }
        true
    }

    pub fn manifest(&self) -> &ArtManifest {
        &self.manifest
    }

    /// (instances in use, instances ever built).
    pub fn counts(&self) -> (usize, usize) {
        (self.live, self.built)
    }

    pub fn mesh(&mut self, key: MeshKey) -> Gd<Mesh> {
        self.meshes
            .entry(key)
            .or_insert_with(|| key.build())
            .clone()
    }

    fn warn_once(&mut self, what: String) {
        if self.warned.insert(what.clone()) {
            godot_warn!("renethack: {what}");
        }
    }

    fn texture(&mut self, path: &str) -> Option<Gd<Texture2D>> {
        if let Some(t) = self.textures.get(path) {
            return t.clone();
        }
        let full = format!("{ART_ROOT}{path}");
        let t = if ResourceLoader::singleton().exists(&full) {
            godot::tools::try_load::<Texture2D>(&full).ok()
        } else {
            None
        };
        if t.is_none() {
            self.warn_once(format!("texture {path} is missing"));
        }
        self.textures.insert(path.to_string(), t.clone());
        t
    }

    fn build_surface(
        &mut self,
        spec: &MaterialSpec,
        shade: u8,
        world: bool,
        tint: Color,
    ) -> Gd<Material> {
        let mut m = OrmMaterial3D::new_gd();
        let base = mul(rgb(spec.albedo()), tint);
        let k = f32::from(shade) / 100.0;
        let mut albedo = Color::from_rgba(base.r * k, base.g * k, base.b * k, tint.a);
        if let Some(a) = spec.alpha {
            albedo.a *= a;
        }
        if albedo.a < 0.999 {
            m.set_transparency(Transparency::ALPHA);
        }
        m.set_albedo(albedo);
        let textured = match (spec.albedo_path(), spec.normal_path(), spec.arm_path()) {
            (Some(a), Some(n), Some(orm)) => {
                let (a, n, orm) = (self.texture(&a), self.texture(&n), self.texture(&orm));
                if let Some(a) = a {
                    m.set_texture(TextureParam::ALBEDO, &a);
                }
                if let Some(n) = n {
                    m.set_feature(Feature::NORMAL_MAPPING, true);
                    m.set_texture(TextureParam::NORMAL, &n);
                    m.set_normal_scale(spec.normal_scale);
                }
                if let Some(orm) = orm {
                    m.set_texture(TextureParam::ORM, &orm);
                }
                m.set_flag(Flags::UV1_USE_TRIPLANAR, true);
                m.set_flag(Flags::UV1_USE_WORLD_TRIPLANAR, world);
                let s = spec.uv_scale;
                m.set_uv1_scale(Vector3::new(s, s, s));
                true
            }
            _ => false,
        };
        m.set_roughness(spec.roughness.unwrap_or(if textured { 1.0 } else { 0.8 }));
        m.set_metallic(spec.metallic.unwrap_or(0.0));
        if !textured {
            m.set_specular(0.5);
        }
        if let Some(e) = spec.emission_color() {
            m.set_feature(Feature::EMISSION, true);
            m.set_emission(rgb(e));
            m.set_emission_energy_multiplier(spec.emission_energy);
        }
        m.upcast()
    }

    /// A manifest material: `shade` % of its brightness, UVs in world space
    /// (map geometry: the pattern runs on across cells) or in the model's
    /// own space (a model that moves keeps its pattern), multiplied by `tint`.
    pub fn surface(
        &mut self,
        material: usize,
        shade: u8,
        world: bool,
        tint: Color,
    ) -> Gd<Material> {
        let key = (material, shade, world, color_key(tint));
        if let Some(m) = self.surfaces.get(&key) {
            return m.clone();
        }
        let spec = self.manifest.material_at(material).1.clone();
        let m = self.build_surface(&spec, shade, world, tint);
        self.surfaces.insert(key, m.clone());
        m
    }

    /// A plain colour with a finish (markers, glows, eyes).
    pub fn flat(&mut self, color: Color, finish: Finish) -> Gd<Material> {
        let key = (color_key(color), finish as u8);
        if let Some(m) = self.flat.get(&key) {
            return m.clone();
        }
        let m = build_flat(color, finish);
        self.flat.insert(key, m.clone());
        m
    }

    /// A model instance for this look: one given back before, or a new one.
    pub fn take(&mut self, look: &ModelLook) -> Model {
        let key = PoolKey::of(look);
        self.live += 1;
        if let Some(mut m) = self.pool.get_mut(&key).and_then(Vec::pop) {
            m.node.set_visible(true);
            self.start(&mut m, look);
            return m;
        }
        self.built += 1;
        let mut m = self.build(look, key);
        self.start(&mut m, look);
        m
    }

    /// Hide an instance and keep it for the next look like it.
    pub fn give(&mut self, mut m: Model) {
        self.live = self.live.saturating_sub(1);
        self.unequip(&mut m);
        m.node.set_visible(false);
        if let Some(p) = m.player.as_mut() {
            p.pause();
        }
        let pool = self.pool.entry(m.key).or_default();
        if pool.len() < POOL_MAX {
            pool.push(m);
        } else {
            m.node.queue_free();
        }
    }

    /// The clips the map plays on a model as it moves and fights; a
    /// procedural body only idles.
    pub fn clips(&self, m: &Model, hurry: bool) -> Clips {
        let Some(player) = m.player.clone() else {
            return Clips::default();
        };
        let spec = self.manifest.model_at(m.key.model).1;
        let has = |n: Option<&str>| n.filter(|n| player.has_animation(*n)).map(str::to_string);
        if spec.proc.is_some() {
            return Clips {
                idle: has(Some("idle")),
                player: Some(player),
                ..Clips::default()
            };
        }
        // the gear's idle and attack (a sword, a shield, a lamp, fists)
        let gear = m.worn.as_ref().map(|w| w.gear());
        let idle = gear.and_then(|g| has(g.idle.as_deref()));
        let attack = gear.and_then(|g| has(g.attack.as_deref()));
        Clips {
            idle: idle.or_else(|| has(spec.anims.idle.as_deref())),
            gait: has(spec.anims.gait(hurry)),
            attack: attack.or_else(|| has(spec.anims.attack.as_deref())),
            player: Some(player),
        }
    }

    /// A model struck reels: its hit clip, then back to its idle (the
    /// gear's for the hero). False when it has no hit clip.
    pub fn flinch(&self, m: &Model) -> bool {
        let Some(mut p) = m.player.clone().filter(|p| p.is_instance_valid()) else {
            return false;
        };
        let spec = self.manifest.model_at(m.key.model).1;
        let Some(hit) = spec.anims.hit.as_deref().filter(|h| p.has_animation(*h)) else {
            return false;
        };
        let idle = self.clips(m, false).idle;
        p.play_ex().name(hit).custom_blend(0.06).done();
        if let Some(idle) = idle {
            p.queue(idle.as_str());
        }
        true
    }

    /// Start the look's animation: idle with a random phase; a corpse lies
    /// at the end of its death; a statue stands still.
    fn start(&mut self, m: &mut Model, look: &ModelLook) {
        let Some(player) = m.player.as_mut() else {
            return;
        };
        let spec = self.manifest.model_at(look.art.model).1;
        let proc = spec.proc.is_some();
        let (idle, death) = if proc {
            (Some("idle".to_string()), None)
        } else {
            (spec.anims.idle.clone(), spec.anims.death.clone())
        };
        self.phase = self.phase.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let phase = f64::from(self.phase >> 16 & 0x7fff) / 32768.0;
        let pick = match look.pose {
            Pose::Corpse => death.or(idle),
            _ => idle,
        };
        let Some(name) = pick.filter(|n| player.has_animation(n.as_str())) else {
            return;
        };
        player.play_ex().name(name.as_str()).done();
        let len = player
            .get_animation(name.as_str())
            .map_or(0.0, |a| f64::from(a.get_length()));
        match look.pose {
            Pose::Corpse if spec.anims.death.is_some() => {
                player.seek_ex(len).update(true).done();
                player.pause();
            }
            Pose::Corpse | Pose::Statue => {
                player.seek_ex(len * 0.2).update(true).done();
                player.pause();
            }
            _ => {
                player.seek_ex(len * phase).update(true).done();
            }
        }
    }

    fn scene(&mut self, model: usize) -> Option<Gd<PackedScene>> {
        if let Some(s) = self.scenes.get(&model) {
            return s.clone();
        }
        let path = self.manifest.model_at(model).1.scene.clone()?;
        let s = godot::tools::try_load::<PackedScene>(&format!("{ART_ROOT}{path}")).ok();
        if s.is_none() {
            self.warn_once(format!("art {path} does not load"));
        }
        self.scenes.insert(model, s.clone());
        s
    }

    /// An animation library (its position tracks dropped with `strip`).
    fn library(&mut self, name: &str, strip: bool) -> Option<Gd<AnimationLibrary>> {
        let key = (name.to_string(), strip);
        if let Some(l) = self.libraries.get(&key) {
            return l.clone();
        }
        let lib = if strip {
            self.library(name, false).map(|l| stripped(&l))
        } else {
            self.manifest.library(name).and_then(|path| {
                let scene =
                    godot::tools::try_load::<PackedScene>(&format!("{ART_ROOT}{path}")).ok()?;
                let mut inst = scene.instantiate()?;
                let lib = find::<AnimationPlayer>(&inst).and_then(|p| p.get_animation_library(""));
                inst.queue_free();
                lib
            })
        };
        if lib.is_none() {
            self.warn_once(format!("animation library {name} does not load"));
        }
        self.libraries.insert(key, lib.clone());
        lib
    }

    fn build(&mut self, look: &ModelLook, key: PoolKey) -> Model {
        let r = look.art;
        let spec = self.manifest.model_at(r.model).1.clone();
        let mut holder = Node3D::new_alloc();
        let (mut inner, player) = match (spec.proc, self.scene(r.model)) {
            (Some(kind), _) => self.build_proc(kind, &spec, look),
            (None, Some(scene)) => {
                let inner = scene.instantiate_as::<Node3D>();
                if spec.smooth {
                    self.smooth(&inner);
                }
                let player = self.animate_scene(&inner, &spec);
                if let Some(kind) = spec.head.as_deref() {
                    // a base character's head, else the procedural one
                    if !self.attach_base_head(&inner, kind, Region::Head) {
                        self.attach_head(&inner, kind);
                    }
                }
                if let Some(kind) = spec.bare_arms.as_deref() {
                    self.attach_base_head(&inner, kind, Region::Arms);
                }
                let shade = rgb(spec.shade_rgb());
                self.dress(&inner, look, shade);
                (inner, player)
            }
            (None, None) => self.build_proc(Proc::Blob, &spec, look),
        };
        let lying = look.pose == Pose::Corpse && spec.anims.death.is_none();
        let mut rot = r.rot;
        let mut lift = r.lift;
        if lying {
            // on its side, in the cell
            rot[2] += 88.0;
            lift += r.height * 0.22;
        }
        let s = r.scale;
        inner.set_transform(transform([0.0, lift, 0.0], rot, [s, s, s]));
        holder.add_child(&inner);
        self.root.add_child(&holder);
        let extras = if spec.proc.is_none() {
            self.attach_extras(&inner, &spec, look)
        } else {
            Vec::new()
        };
        Model {
            node: holder,
            key,
            player,
            worn: None,
            extras,
        }
    }

    /// The scene's own AnimationPlayer, or a new one with the rig's
    /// library (the tracks name `Armature/Skeleton3D:<bone>` from the
    /// scene root). Idle, walk and run loop.
    fn animate_scene(
        &mut self,
        inner: &Gd<Node3D>,
        spec: &nh_art::ModelSpec,
    ) -> Option<Gd<AnimationPlayer>> {
        let mut player = match find::<AnimationPlayer>(&inner.clone().upcast()) {
            Some(p) => p,
            None => {
                let rig = spec.rig.as_deref()?;
                let lib = self.library(rig, spec.strip_root)?;
                let mut p = AnimationPlayer::new_alloc();
                p.set_name("AnimationPlayer");
                let _ = p.add_animation_library("", &lib);
                for extra in &spec.extra_rigs {
                    if let Some(lib) = self.library(extra, spec.strip_root) {
                        let _ = p.add_animation_library(extra.as_str(), &lib);
                    }
                }
                let mut inner = inner.clone();
                inner.add_child(&p);
                p
            }
        };
        let loops = [&spec.anims.idle, &spec.anims.walk, &spec.anims.run];
        for name in loops.into_iter().flatten() {
            if let Some(mut a) = player.get_animation(name.as_str()) {
                a.set_loop_mode(LoopMode::LINEAR);
            } else {
                self.warn_once(format!("no animation {name} for {:?}", spec.scene));
            }
        }
        player.set_active(true);
        Some(player)
    }

    /// Tint, skin and see-through: every surface of the scene gets a
    /// material derived from its own (shared by instances of the look).
    fn dress(&mut self, inner: &Gd<Node3D>, look: &ModelLook, shade: Color) {
        let r = look.art;
        let ghost = look.pose == Pose::Ghost || r.skin == Skin::Translucent;
        let tint = mul(look.tint, shade);
        let spec = self.manifest.model_at(r.model).1;
        for name in spec.hide.clone() {
            if let Some(mut n) = inner
                .find_child_ex(&name)
                .owned(false)
                .done()
                .and_then(|n| n.try_cast::<Node3D>().ok())
            {
                n.set_visible(false);
            }
        }
        // the scene's own metal and glow (a steel blade, a runed one)
        let finish = spec.refinishes().then_some(r.model);
        // (mesh name suffix, colour, dyed outright)
        let recolor: Vec<(String, Color, bool)> = spec
            .recolor
            .iter()
            .filter_map(|(k, v)| {
                let flat = v.starts_with('=');
                Some((
                    k.clone(),
                    rgb(nh_art::hex(v.trim_start_matches('='))?),
                    flat,
                ))
            })
            .collect();
        let plain = tint == Color::WHITE
            && !ghost
            && r.skin == Skin::Own
            && finish.is_none()
            && recolor.is_empty();
        for node in inner
            .find_children_ex("*")
            .type_("MeshInstance3D")
            .owned(false)
            .done()
            .iter_shared()
        {
            let Ok(mut mi) = node.try_cast::<MeshInstance3D>() else {
                continue;
            };
            if plain {
                continue;
            }
            let name = mi.get_name().to_string();
            // a base head keeps its own skin and hair (only a ghost's fades)
            let head = name.starts_with("BaseHead");
            if head && !ghost {
                continue;
            }
            let dye = recolor
                .iter()
                .find(|(k, _, _)| !head && name.ends_with(k.as_str()));
            let tint = match dye {
                Some((_, c, false)) => mul(tint, *c),
                Some((_, c, true)) => mul(look.tint, *c),
                None if head => Color::WHITE,
                None => tint,
            };
            let flat = dye.is_some_and(|d| d.2);
            let n = mi.get_mesh().map_or(0, |m| m.get_surface_count());
            for i in 0..n {
                let mat = match r.skin {
                    Skin::Material(m) if !head => Some(self.surface(m, 100, false, look.tint)),
                    _ => mi.get_active_material(i).map(|src| {
                        let src = if flat { self.undyed(&src) } else { src };
                        self.derive(&src, tint, ghost, finish)
                    }),
                };
                if let Some(mat) = mat {
                    mi.set_surface_override_material(i, &mat);
                }
            }
        }
    }

    /// Shade a scene's meshes smooth: each surface's normals are made
    /// again across its shared corners (the skin weights stay); once per
    /// mesh, shared by every instance.
    fn smooth(&mut self, inner: &Gd<Node3D>) {
        for node in inner
            .find_children_ex("*")
            .type_("MeshInstance3D")
            .owned(false)
            .done()
            .iter_shared()
        {
            let Ok(mut mi) = node.try_cast::<MeshInstance3D>() else {
                continue;
            };
            let Some(src) = mi.get_mesh() else {
                continue;
            };
            let key = src.instance_id().to_i64();
            let mesh = match self.smoothed.get(&key) {
                Some(m) => m.clone(),
                None => {
                    let out = ArrayMesh::new_gd();
                    for i in 0..src.get_surface_count() {
                        let mut st = SurfaceTool::new_gd();
                        st.create_from(&src, i);
                        st.generate_normals();
                        if let Some(mat) = src.surface_get_material(i) {
                            st.set_material(&mat);
                        }
                        st.commit_ex().existing(&out).done();
                    }
                    let out: Gd<Mesh> = out.upcast();
                    self.smoothed.insert(key, out.clone());
                    out
                }
            };
            mi.set_mesh(&mesh);
        }
    }

    /// A base character's head (see `nh_art::HeadSpec`) worn on the
    /// rig's skeleton: each of its meshes skinned to the rig's bones (the
    /// names are the same: one skeleton). False when there is no such head
    /// (the procedural one is drawn instead).
    fn attach_base_head(&mut self, inner: &Gd<Node3D>, kind: &str, region: Region) -> bool {
        if self.manifest.head(kind).is_none() {
            return false;
        }
        let Some(mut skeleton) = find::<Skeleton3D>(&inner.clone().upcast()) else {
            return false;
        };
        let key = format!("{kind}{region:?}");
        let parts = match self.heads.get(&key) {
            Some(p) => p.clone(),
            None => {
                let p = self.build_head(kind, region);
                if p.is_none() {
                    self.warn_once(format!("head {kind} does not load"));
                }
                self.heads.insert(key, p.clone());
                p
            }
        };
        let Some(parts) = parts else {
            return false;
        };
        for (i, (mesh, skin)) in parts.iter().enumerate() {
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_name(&format!("BaseHead{i}"));
            mi.set_mesh(mesh);
            if let Some(skin) = skin {
                mi.set_skin(skin);
            }
            skeleton.add_child(&mi);
            mi.set_skeleton_path(&NodePath::from(".."));
        }
        true
    }

    /// The meshes of a base head: the face cut from the base character at
    /// the neck (whole triangles above the cut; the skin weights stay), its
    /// eyes and eyebrows, and the hair; skin and hair re-coloured.
    fn build_head(&mut self, kind: &str, region: Region) -> Option<HeadParts> {
        let spec = self.manifest.head(kind)?.clone();
        let tint = spec.tint.as_deref().and_then(nh_art::hex).map(rgb);
        let hair_tint = spec.hair_color.as_deref().and_then(nh_art::hex).map(rgb);
        let skin_tex = spec.skin.as_deref().and_then(|t| self.texture(t));
        let mut out = Vec::new();
        // arms come from the base alone, without its eyes or hair
        let hair = if region == Region::Head {
            spec.hair.clone()
        } else {
            Vec::new()
        };
        let scenes =
            std::iter::once((spec.base.clone(), true)).chain(hair.into_iter().map(|h| (h, false)));
        for (path, base) in scenes {
            let scene = godot::tools::try_load::<PackedScene>(&format!("{ART_ROOT}{path}")).ok()?;
            let mut inst = scene.instantiate()?;
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
                let Some(mesh) = mi.get_mesh().and_then(|m| m.try_cast::<ArrayMesh>().ok()) else {
                    continue;
                };
                // a material set on the instance (not the mesh) goes with
                // the mesh, or the copy would wear the default one
                let mesh = with_active_materials(&mi, mesh);
                // the body's own mesh is the tall one; the eyes and the
                // eyebrows come whole
                let body = base && mesh.get_aabb().size.y > 0.5;
                if region == Region::Arms && !body {
                    continue;
                }
                let mesh = match (body, region) {
                    (true, Region::Head) => cut(&mesh, |v| v.y >= spec.cut),
                    // in the rest pose the arms reach out sideways from
                    // the shoulders
                    (true, Region::Arms) => cut(&mesh, |v| v.x.abs() > 0.2 && v.y > 1.2),
                    _ => mesh,
                };
                // the base's own eyebrows take the hair's colour; its eyes
                // keep theirs
                let brows = mi.get_name().to_string().contains("Eyebrow");
                let colour = match (body, base) {
                    (true, _) => tint,
                    (false, true) if brows => hair_tint,
                    (false, true) => None,
                    (false, false) => hair_tint,
                };
                let texture = if body { skin_tex.clone() } else { None };
                let mesh = recolour(&mesh, colour, texture);
                out.push((mesh.upcast::<Mesh>(), mi.get_skin()));
            }
            inst.queue_free();
        }
        (!out.is_empty()).then_some(out)
    }

    /// The things the model's spec wears on its bones (`extras`).
    fn attach_extras(
        &mut self,
        inner: &Gd<Node3D>,
        spec: &nh_art::ModelSpec,
        look: &ModelLook,
    ) -> Vec<(Option<String>, Gd<Node3D>)> {
        let mut out = Vec::new();
        if spec.extras.is_empty() {
            return out;
        }
        let Some(mut skeleton) = find::<Skeleton3D>(&inner.clone().upcast()) else {
            return out;
        };
        for e in &spec.extras {
            let Some(model) = self.manifest.model_index(&e.model) else {
                continue;
            };
            let tint = e.tint_rgb().map_or(Color::WHITE, rgb);
            let Some(node) = self.prop(model, tint, look.pose == Pose::Ghost) else {
                continue;
            };
            let mut bone = BoneAttachment3D::new_alloc();
            bone.set_bone_name(&e.bone);
            skeleton.add_child(&bone);
            let k = e.size / self.manifest.model_at(model).1.size;
            let mut holder = Node3D::new_alloc();
            holder.set_name(&format!("Extra_{}", e.model));
            holder.set_transform(transform(e.pos, e.rot, [k, k, k]));
            holder.add_child(&node);
            bone.add_child(&holder);
            out.push((e.slot.clone(), holder));
        }
        out
    }

    /// A model on its own, dressed in `tint` (a prop worn or carried).
    fn prop(&mut self, model: usize, tint: Color, ghost: bool) -> Option<Gd<Node3D>> {
        let spec = self.manifest.model_at(model).1.clone();
        let look = ModelLook {
            art: Resolved {
                model,
                scale: 1.0,
                lift: 0.0,
                rot: [0.0; 3],
                height: 1.0,
                tint: nh_art::Tint::None,
                skin: Skin::Own,
                level: nh_art::Level::Generic,
            },
            tint,
            pose: if ghost { Pose::Ghost } else { Pose::Alive },
        };
        match (spec.proc, self.scene(model)) {
            (Some(kind), _) => {
                let (root, player) = self.build_proc(kind, &spec, &look);
                if let Some(mut p) = player {
                    p.queue_free();
                }
                Some(root)
            }
            (None, Some(scene)) => {
                let inner = scene.instantiate_as::<Node3D>();
                let shade = rgb(spec.shade_rgb());
                self.dress(&inner, &look, shade);
                Some(inner)
            }
            (None, None) => None,
        }
    }

    /// A head on the rig's `Head` bone (the outfits come without one).
    fn attach_head(&mut self, inner: &Gd<Node3D>, kind: &str) {
        let Some(mut skeleton) = find::<Skeleton3D>(&inner.clone().upcast()) else {
            return;
        };
        let mut bone = BoneAttachment3D::new_alloc();
        bone.set_bone_name("Head");
        skeleton.add_child(&bone);
        let mut head = Node3D::new_alloc();
        head.set_name("ProcHead");
        head.set_scale(Vector3::new(0.88, 0.88, 0.88));
        bone.add_child(&head);
        let skin = lit_skin(Color::from_rgb(0.56, 0.4, 0.31), 0.0);
        let mut kit = Kit {
            art: self,
            skin,
            tint: Color::WHITE,
            shape: None,
        };
        kit.head(kind, &mut head);
    }

    /// A material without its albedo texture (to be dyed a flat colour);
    /// one per source material.
    fn undyed(&mut self, src: &Gd<Material>) -> Gd<Material> {
        let key = (src.instance_id().to_i64(), 0, true, Some(usize::MAX));
        if let Some(m) = self.derived.get(&key) {
            return m.clone();
        }
        let out = match src.duplicate_resource().try_cast::<BaseMaterial3D>() {
            Ok(mut m) => {
                m.set_texture(TextureParam::ALBEDO, Gd::null_arg());
                m.set_albedo(Color::WHITE);
                m.upcast::<Material>()
            }
            Err(m) => m,
        };
        self.derived.insert(key, out.clone());
        out
    }

    /// `finish`: the model whose metal, roughness and glow replace the
    /// source's own.
    fn derive(
        &mut self,
        src: &Gd<Material>,
        tint: Color,
        ghost: bool,
        finish: Option<usize>,
    ) -> Gd<Material> {
        let key = (src.instance_id().to_i64(), color_key(tint), ghost, finish);
        if let Some(m) = self.derived.get(&key) {
            return m.clone();
        }
        let out = match src.duplicate_resource().try_cast::<BaseMaterial3D>().ok() {
            Some(mut m) => {
                let mut albedo = mul(m.get_albedo(), tint);
                // nothing in a dungeon to reflect: full metal would be black
                if m.get_metallic() > MAX_METALLIC {
                    m.set_metallic(MAX_METALLIC);
                    let rough = m.get_roughness().max(0.35);
                    m.set_roughness(rough);
                }
                if let Some(model) = finish {
                    let spec = self.manifest.model_at(model).1;
                    if let Some(v) = spec.metallic {
                        m.set_metallic(v);
                    }
                    if let Some(v) = spec.roughness {
                        m.set_roughness(v);
                    }
                    if let Some(g) = spec.glow_rgb() {
                        // brightest where the albedo is: the blade, not the grip
                        m.set_feature(Feature::EMISSION, true);
                        m.set_emission(rgb(g));
                        m.set_emission_energy_multiplier(spec.glow_energy);
                        if let Some(t) = m.get_texture(TextureParam::ALBEDO) {
                            m.set_texture(TextureParam::EMISSION, &t);
                        }
                    }
                }
                if ghost {
                    albedo.a = 0.4;
                    m.set_transparency(Transparency::ALPHA);
                }
                m.set_albedo(albedo);
                m.upcast::<Material>()
            }
            None => src.clone(),
        };
        self.derived.insert(key, out.clone());
        out
    }

    fn proc_library(&mut self, kind: Proc) -> Option<Gd<AnimationLibrary>> {
        if let Some(l) = self.proc_anims.get(&kind) {
            return Some(l.clone());
        }
        let waves = proc_waves(kind);
        if waves.is_empty() {
            return None;
        }
        let mut lib = AnimationLibrary::new_gd();
        let _ = lib.add_animation("idle", &wave_animation(&waves));
        self.proc_anims.insert(kind, lib.clone());
        Some(lib)
    }

    fn build_proc(
        &mut self,
        kind: Proc,
        spec: &nh_art::ModelSpec,
        look: &ModelLook,
    ) -> (Gd<Node3D>, Option<Gd<AnimationPlayer>>) {
        let ghost = look.pose == Pose::Ghost || look.art.skin == Skin::Translucent;
        let mut tint = look.tint;
        if ghost {
            tint.a = 0.45;
        }
        let skin = match look.art.skin {
            Skin::Material(m) => self.surface(m, 100, false, tint),
            _ => match spec
                .material
                .as_deref()
                .and_then(|n| self.manifest.material_index(n))
            {
                Some(m) => self.surface(m, 100, false, tint),
                None => self.flat(tint, Finish::Matte),
            },
        };
        let mut kit = Kit {
            art: self,
            skin,
            tint,
            shape: spec.shape.clone(),
        };
        let mut root = Node3D::new_alloc();
        kit.body(kind, &mut root);
        let player = kit.art.proc_library(kind).map(|lib| {
            let mut p = AnimationPlayer::new_alloc();
            p.set_name("AnimationPlayer");
            let _ = p.add_animation_library("", &lib);
            root.add_child(&p);
            p
        });
        (root, player)
    }
}

fn find<T: GodotClass + Inherits<Node>>(root: &Gd<Node>) -> Option<Gd<T>> {
    root.find_children_ex("*")
        .type_(&T::class_id().to_string())
        .owned(false)
        .done()
        .iter_shared()
        .next()
        .and_then(|n| n.try_cast::<T>().ok())
}

/// A copy of a library without position tracks (the pelvis height of a
/// taller rig would lift a shorter one off the ground).
fn stripped(lib: &Gd<AnimationLibrary>) -> Gd<AnimationLibrary> {
    let mut out = AnimationLibrary::new_gd();
    for name in lib.get_animation_list().iter_shared() {
        let Some(anim) = lib.get_animation(&name) else {
            continue;
        };
        let mut copy = anim.duplicate_resource();
        for t in (0..copy.get_track_count()).rev() {
            if copy.track_get_type(t) == TrackType::POSITION_3D {
                copy.remove_track(t);
            }
        }
        let _ = out.add_animation(&name, &copy);
    }
    out
}

/// How a plain surface is lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    Matte,
    /// Wet, polished: eyes, claws.
    Glossy,
    /// Lit from inside.
    Ember,
    /// Full colour, no shading (beams).
    Glow,
    /// See-through (clouds, air, the unseen).
    Ghost,
    /// Markers: unshaded, alpha from the colour.
    Flat,
    /// Clear glass: polished, alpha from the colour, both faces drawn.
    Glass,
    /// A cut stone or a potion's liquid: polished, lit a little from inside.
    Gem,
}

pub fn build_flat(color: Color, finish: Finish) -> Gd<Material> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_albedo(color);
    m.set_roughness(0.9);
    match finish {
        Finish::Matte => {}
        Finish::Glossy => {
            m.set_roughness(0.15);
            m.set_specular(0.9);
        }
        Finish::Ember => {
            m.set_feature(Feature::EMISSION, true);
            m.set_emission(color);
            m.set_emission_energy_multiplier(2.0);
        }
        Finish::Glow => m.set_shading_mode(ShadingMode::UNSHADED),
        Finish::Ghost => {
            m.set_transparency(Transparency::ALPHA);
            m.set_albedo(color.with_alpha(0.4));
            m.set_roughness(0.5);
        }
        Finish::Flat => {
            m.set_shading_mode(ShadingMode::UNSHADED);
            m.set_transparency(Transparency::ALPHA);
            m.set_cull_mode(CullMode::DISABLED);
        }
        Finish::Glass => {
            m.set_transparency(Transparency::ALPHA);
            m.set_cull_mode(CullMode::DISABLED);
            m.set_roughness(0.04);
            m.set_specular(1.0);
            m.set_feature(Feature::RIM, true);
            m.set_rim(0.6);
            m.set_rim_tint(0.2);
        }
        Finish::Gem => {
            m.set_roughness(0.06);
            m.set_specular(1.0);
            m.set_metallic(0.15);
            m.set_feature(Feature::EMISSION, true);
            m.set_emission(color);
            m.set_emission_energy_multiplier(0.35);
            m.set_feature(Feature::RIM, true);
            m.set_rim(0.5);
            m.set_rim_tint(0.8);
        }
    }
    m.upcast()
}

/// A repeating motion of a named part of a procedural body.
#[derive(Clone, Copy)]
enum Wave {
    /// Rock about an axis: node, axis, amplitude in degrees, phase (turns).
    Rock(&'static str, Vector3, f32, f32),
    /// Bob up and down: node, rest position, amplitude, phase.
    Bob(&'static str, Vector3, f32, f32),
    /// Breathe: node, amplitude (fraction of its size), phase.
    Pulse(&'static str, f32, f32),
    /// Turn around y once per cycle.
    Spin(&'static str),
}

/// Seconds per cycle of every procedural idle.
const WAVE_PERIOD: f32 = 1.6;
const WAVE_KEYS: i32 = 16;

fn wave_animation(waves: &[Wave]) -> Gd<Animation> {
    let mut a = Animation::new_gd();
    a.set_length(WAVE_PERIOD);
    a.set_loop_mode(LoopMode::LINEAR);
    let tau = std::f32::consts::TAU;
    for w in waves {
        let (kind, path) = match w {
            Wave::Rock(p, ..) | Wave::Spin(p) => (TrackType::ROTATION_3D, *p),
            Wave::Bob(p, ..) => (TrackType::POSITION_3D, *p),
            Wave::Pulse(p, ..) => (TrackType::SCALE_3D, *p),
        };
        let t = a.add_track(kind);
        a.track_set_path(t, &NodePath::from(path));
        for k in 0..=WAVE_KEYS {
            let f = k as f32 / WAVE_KEYS as f32;
            let time = f64::from(f * WAVE_PERIOD);
            match *w {
                Wave::Rock(_, axis, amp, ph) => {
                    let angle = (amp * (tau * (f + ph)).sin()).to_radians();
                    a.rotation_track_insert_key(t, time, Quaternion::from_axis_angle(axis, angle));
                }
                Wave::Spin(_) => {
                    let q = Quaternion::from_axis_angle(Vector3::UP, -tau * f);
                    a.rotation_track_insert_key(t, time, q);
                }
                Wave::Bob(_, rest, amp, ph) => {
                    let y = amp * (tau * (f + ph)).sin();
                    a.position_track_insert_key(t, time, rest + Vector3::new(0.0, y, 0.0));
                }
                Wave::Pulse(_, amp, ph) => {
                    let s = 1.0 + amp * (tau * (f + ph)).sin();
                    let flat = 1.0 - amp * 0.5 * (tau * (f + ph)).sin();
                    a.scale_track_insert_key(t, time, Vector3::new(flat, s, flat));
                }
            }
        }
    }
    a
}

/// The idle motions of each procedural body (node names as `Kit` builds them).
fn proc_waves(kind: Proc) -> Vec<Wave> {
    let x = Vector3::RIGHT;
    let y = Vector3::UP;
    let z = Vector3::BACK;
    match kind {
        Proc::Serpent => vec![
            Wave::Rock("Neck", y, 14.0, 0.0),
            Wave::Rock("Neck", x, 0.0, 0.0),
        ],
        Proc::Worm => vec![
            Wave::Rock("Front", y, 10.0, 0.0),
            Wave::Rock("Back", y, 8.0, 0.5),
        ],
        Proc::Bug | Proc::Spider => vec![
            Wave::Rock("LegsL", z, 6.0, 0.0),
            Wave::Rock("LegsR", z, 6.0, 0.5),
            Wave::Bob("Body", Vector3::ZERO, 0.02, 0.0),
        ],
        Proc::Bat => vec![
            Wave::Rock("WingL", z, 38.0, 0.0),
            Wave::Rock("WingR", z, -38.0, 0.0),
            Wave::Bob("Body", Vector3::ZERO, 0.08, 0.25),
        ],
        Proc::Dragon => vec![
            Wave::Rock("WingL", z, 12.0, 0.0),
            Wave::Rock("WingR", z, -12.0, 0.0),
            Wave::Rock("Neck", x, 5.0, 0.3),
        ],
        Proc::Bird => vec![Wave::Rock("Head", x, 10.0, 0.0)],
        Proc::Blob => vec![Wave::Pulse("Body", 0.06, 0.0)],
        Proc::Eye | Proc::Light => vec![Wave::Bob("Body", Vector3::ZERO, 0.06, 0.0)],
        Proc::Vortex => vec![Wave::Spin("Body")],
        Proc::Fungus => vec![Wave::Pulse("Body", 0.03, 0.0)],
        Proc::Lizard | Proc::Beast => vec![
            Wave::Rock("Tail", y, 12.0, 0.0),
            Wave::Rock("Head", y, 6.0, 0.4),
        ],
        Proc::Fish => vec![
            Wave::Rock("Tail", y, 20.0, 0.0),
            Wave::Bob("Body", Vector3::ZERO, 0.04, 0.0),
        ],
        _ => Vec::new(),
    }
}

/// Builds a procedural body: parts from primitive meshes, the skin
/// material (tinted), and named pivots the idle animation moves. Every body
/// is one unit tall, stands on y = 0 and faces +z (the camera).
struct Kit<'a> {
    art: &'a mut Art,
    skin: Gd<Material>,
    tint: Color,
    /// The model's variant (`ModelSpec::shape`).
    shape: Option<String>,
}

impl Kit<'_> {
    fn part(
        &mut self,
        parent: &mut Gd<Node3D>,
        shape: MeshKey,
        mat: &Gd<Material>,
        pos: [f32; 3],
        rot: [f32; 3],
        scale: [f32; 3],
    ) {
        let mesh = self.art.mesh(shape);
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&mesh);
        mi.set_material_override(mat);
        mi.set_transform(transform(pos, rot, scale));
        parent.add_child(&mi);
    }

    fn skin(
        &mut self,
        parent: &mut Gd<Node3D>,
        shape: MeshKey,
        pos: [f32; 3],
        rot: [f32; 3],
        scale: [f32; 3],
    ) {
        let skin = self.skin.clone();
        self.part(parent, shape, &skin, pos, rot, scale);
    }

    fn pivot(&mut self, parent: &mut Gd<Node3D>, name: &str, pos: [f32; 3]) -> Gd<Node3D> {
        let mut n = Node3D::new_alloc();
        n.set_name(name);
        n.set_position(Vector3::new(pos[0], pos[1], pos[2]));
        parent.add_child(&n);
        n
    }

    fn dark(&mut self) -> Gd<Material> {
        let c = Color::from_rgba(
            self.tint.r * 0.35,
            self.tint.g * 0.33,
            self.tint.b * 0.32,
            self.tint.a,
        );
        self.art.flat(c, Finish::Matte)
    }

    fn eye(&mut self) -> Gd<Material> {
        self.art
            .flat(Color::from_rgb(0.02, 0.02, 0.02), Finish::Glossy)
    }

    fn glint(&mut self) -> Gd<Material> {
        self.art
            .flat(Color::from_rgb(0.95, 0.75, 0.2), Finish::Ember)
    }

    fn bone(&mut self) -> Gd<Material> {
        self.art
            .flat(Color::from_rgb(0.78, 0.72, 0.6), Finish::Matte)
    }

    fn named(&mut self, name: &str, fallback: Color) -> Gd<Material> {
        match self.art.manifest.material_index(name) {
            Some(m) => self.art.surface(m, 100, false, Color::WHITE),
            None => self.art.flat(fallback, Finish::Matte),
        }
    }

    fn eyes(&mut self, parent: &mut Gd<Node3D>, at: [f32; 3], apart: f32, r: f32, glow: bool) {
        let mat = if glow { self.glint() } else { self.eye() };
        for s in [-1.0, 1.0] {
            self.part(
                parent,
                sphere(r),
                &mat,
                [at[0] + s * apart, at[1], at[2]],
                [0.0; 3],
                [1.0; 3],
            );
        }
    }

    /// Jointed legs from a body: `n` per side between `z.0` and `z.1`,
    /// each up and out to a knee, then down to the ground.
    fn legs(&mut self, body: &mut Gd<Node3D>, n: usize, y: f32, z: (f32, f32), reach: f32) {
        let dark = self.dark();
        for (side, name) in [(-1.0f32, "LegsL"), (1.0, "LegsR")] {
            let mut group = self.pivot(body, name, [0.0, y, 0.0]);
            for i in 0..n {
                let z = if n == 1 {
                    z.0
                } else {
                    z.0 + (z.1 - z.0) * i as f32 / (n - 1) as f32
                };
                let yaw = (z / reach.max(0.1)) * 25.0 * side;
                let kx = side * reach * 0.55;
                let upper = cylinder(0.018, 0.022, reach * 0.62);
                let at_knee = [kx * 0.5, 0.12, z];
                self.part(
                    &mut group,
                    upper,
                    &dark,
                    at_knee,
                    [yaw, 0.0, side * -58.0],
                    [1.0; 3],
                );
                let lower = cylinder(0.01, 0.018, y + 0.14);
                let at_foot = [kx + side * reach * 0.18, 0.08 - y * 0.45, z];
                self.part(
                    &mut group,
                    lower,
                    &dark,
                    at_foot,
                    [yaw, 0.0, side * 18.0],
                    [1.0; 3],
                );
            }
        }
    }

    /// A human head in the Head bone's space (y up the neck, z forward),
    /// in the outfit's units (a body 1.6 tall).
    fn head(&mut self, kind: &str, head: &mut Gd<Node3D>) {
        let one = [1.0f32; 3];
        let flat = [0.0f32; 3];
        let hair = self
            .art
            .flat(Color::from_rgb(0.1, 0.07, 0.05), Finish::Matte);
        let hood = kind == "hood";
        // in a hood's shadow the face keeps a little light of its own, so
        // it reads as a face and not as a hole
        let face = if hood {
            lit_skin(Color::from_rgb(0.46, 0.33, 0.26), 0.45)
        } else {
            self.skin.clone()
        };
        self.part(
            head,
            sphere(0.1),
            &face,
            [0.0, 0.1, 0.02],
            flat,
            [0.9, 1.12, 1.0],
        );
        self.part(
            head,
            sphere(0.065),
            &face,
            [0.0, 0.035, 0.05],
            flat,
            [1.0, 0.8, 1.0],
        );
        self.part(
            head,
            sphere(0.02),
            &face,
            [0.0, 0.083, 0.118],
            flat,
            [0.85, 1.05, 1.35],
        );
        // cheeks with a little colour, so the face has a shape and is not
        // a mask
        let cheek = self
            .art
            .flat(Color::from_rgb(0.6, 0.36, 0.3), Finish::Matte);
        for s in [-1.0f32, 1.0] {
            self.part(
                head,
                sphere(0.02),
                &cheek,
                [s * 0.05, 0.075, 0.095],
                flat,
                [1.0, 0.7, 0.45],
            );
        }
        // eyes (whites, iris, pupil), brows and a mouth: a face, not a
        // mannequin's blank
        let white = self
            .art
            .flat(Color::from_rgb(0.86, 0.84, 0.8), Finish::Glossy);
        let iris = self
            .art
            .flat(Color::from_rgb(0.24, 0.16, 0.1), Finish::Glossy);
        let eye = self.eye();
        let lips = self
            .art
            .flat(Color::from_rgb(0.42, 0.2, 0.17), Finish::Matte);
        self.part(
            head,
            cuboid(0.034, 0.007, 0.01),
            &lips,
            [0.0, 0.052, 0.108],
            flat,
            one,
        );
        for s in [-1.0f32, 1.0] {
            self.part(
                head,
                sphere(0.02),
                &white,
                [s * 0.034, 0.112, 0.1],
                flat,
                [0.85, 0.6, 0.5],
            );
            self.part(
                head,
                sphere(0.01),
                &iris,
                [s * 0.034, 0.112, 0.108],
                flat,
                [0.9, 0.9, 0.45],
            );
            self.part(
                head,
                // meshes come in whole centimetres: a smaller one is scaled
                sphere(0.01),
                &eye,
                [s * 0.034, 0.112, 0.113],
                flat,
                [0.45, 0.45, 0.3],
            );
            self.part(
                head,
                cuboid(0.03, 0.006, 0.012),
                &hair,
                [s * 0.036, 0.132, 0.106],
                [0.0, 0.0, s * -10.0],
                one,
            );
            if !hood {
                self.part(
                    head,
                    sphere(0.024),
                    &face,
                    [s * 0.092, 0.095, 0.0],
                    flat,
                    [0.45, 1.0, 0.8],
                );
            }
        }
        if hood {
            return;
        }
        // hair: a cap over the crown and down the back
        self.part(
            head,
            sphere(0.106),
            &hair,
            [0.0, 0.13, -0.008],
            [-12.0, 0.0, 0.0],
            [0.98, 0.78, 1.04],
        );
        self.part(
            head,
            sphere(0.098),
            &hair,
            [0.0, 0.085, -0.04],
            flat,
            [0.96, 1.0, 0.82],
        );
        match kind {
            "female" => {
                self.part(
                    head,
                    capsule(0.085, 0.34),
                    &hair,
                    [0.0, -0.02, -0.07],
                    [8.0, 0.0, 0.0],
                    [1.15, 1.0, 0.62],
                );
            }
            "bearded" => {
                self.part(
                    head,
                    sphere(0.07),
                    &hair,
                    [0.0, 0.0, 0.07],
                    [20.0, 0.0, 0.0],
                    [1.05, 1.35, 0.75],
                );
                self.part(
                    head,
                    sphere(0.05),
                    &hair,
                    [0.0, -0.07, 0.07],
                    [30.0, 0.0, 0.0],
                    [0.9, 1.3, 0.7],
                );
            }
            _ => {}
        }
    }

    fn body(&mut self, kind: Proc, root: &mut Gd<Node3D>) {
        match kind {
            Proc::Serpent => self.serpent(root),
            Proc::Worm => self.worm(root),
            Proc::Bug => self.bug(root, 3, false),
            Proc::Spider => self.bug(root, 4, true),
            Proc::Bat => self.bat(root),
            Proc::Bird => self.bird(root),
            Proc::Blob => self.blob(root),
            Proc::Eye => self.floating_eye(root),
            Proc::Light => self.light(root),
            Proc::Vortex => self.vortex(root),
            Proc::Fungus => self.fungus(root),
            Proc::Lizard => self.lizard(root, false),
            Proc::Dragon => self.lizard(root, true),
            Proc::Fish => self.fish(root),
            Proc::Piercer => self.piercer(root),
            Proc::Beast => self.beast(root),
            _ => self.object(kind, root),
        }
    }

    fn serpent(&mut self, root: &mut Gd<Node3D>) {
        // three coils narrowing upwards, a raised neck and a wedge head
        for (i, (r, y)) in [(0.46f32, 0.1f32), (0.37, 0.27), (0.27, 0.42)]
            .into_iter()
            .enumerate()
        {
            let tube = 0.11 - i as f32 * 0.015;
            self.skin(
                root,
                torus(r - tube, r + tube),
                [0.0, y, 0.0],
                [0.0, i as f32 * 40.0, 0.0],
                [1.0, 1.4, 1.0],
            );
        }
        let mut neck = self.pivot(root, "Neck", [0.0, 0.45, 0.12]);
        self.skin(
            &mut neck,
            capsule(0.075, 0.5),
            [0.0, 0.2, 0.06],
            [-22.0, 0.0, 0.0],
            [1.0; 3],
        );
        self.skin(
            &mut neck,
            sphere(0.1),
            [0.0, 0.46, 0.2],
            [0.0; 3],
            [1.0, 0.7, 1.6],
        );
        self.eyes(&mut neck, [0.0, 0.5, 0.28], 0.065, 0.022, true);
        let red = self
            .art
            .flat(Color::from_rgb(0.6, 0.05, 0.05), Finish::Matte);
        self.part(
            &mut neck,
            cuboid(0.01, 0.005, 0.12),
            &red,
            [0.0, 0.42, 0.4],
            [0.0; 3],
            [1.0; 3],
        );
    }

    fn worm(&mut self, root: &mut Gd<Node3D>) {
        // segments along z, thick in the middle, a round maw in front
        let mut front = self.pivot(root, "Front", [0.0, 0.0, 0.3]);
        let mut back = self.pivot(root, "Back", [0.0, 0.0, -0.3]);
        let segs = [
            (-1.6f32, 0.28f32),
            (-1.15, 0.36),
            (-0.7, 0.44),
            (-0.25, 0.5),
            (0.2, 0.5),
            (0.65, 0.46),
            (1.05, 0.42),
        ];
        for (i, (z, r)) in segs.into_iter().enumerate() {
            let x = (i as f32 * 1.3).sin() * 0.12;
            let (node, dz) = if z < 0.0 {
                (&mut back, 0.3)
            } else {
                (&mut front, -0.3)
            };
            let mesh = sphere(r);
            self.skin(node, mesh, [x, r, z + dz], [0.0; 3], [1.0, 1.0, 0.9]);
        }
        let maw = self.dark();
        self.part(
            &mut front,
            cylinder(0.26, 0.3, 0.06),
            &maw,
            [0.0, 0.42, 1.12],
            [90.0, 0.0, 0.0],
            [1.0; 3],
        );
        let teeth = self.bone();
        for a in 0..6 {
            let t = a as f32 / 6.0 * std::f32::consts::TAU;
            self.part(
                &mut front,
                cylinder(0.0, 0.03, 0.08),
                &teeth,
                [t.cos() * 0.22, 0.42 + t.sin() * 0.22, 1.16],
                [90.0, 0.0, 0.0],
                [1.0; 3],
            );
        }
    }

    fn bug(&mut self, root: &mut Gd<Node3D>, pairs: usize, spider: bool) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        let y = if spider { 0.42 } else { 0.5 };
        if spider {
            self.skin(
                &mut body,
                sphere(0.36),
                [0.0, y + 0.08, -0.42],
                [0.0; 3],
                [1.0, 0.85, 1.1],
            );
            self.skin(
                &mut body,
                sphere(0.2),
                [0.0, y, 0.05],
                [0.0; 3],
                [1.0, 0.8, 1.1],
            );
            self.eyes(&mut body, [0.0, y + 0.08, 0.22], 0.06, 0.03, true);
            self.eyes(&mut body, [0.0, y + 0.13, 0.19], 0.1, 0.02, true);
            self.legs(&mut body, pairs, y, (0.2, -0.15), 0.95);
        } else {
            self.skin(
                &mut body,
                sphere(0.3),
                [0.0, y + 0.05, -0.45],
                [0.0; 3],
                [0.85, 0.75, 1.3],
            );
            self.skin(
                &mut body,
                sphere(0.16),
                [0.0, y, 0.0],
                [0.0; 3],
                [1.0, 0.9, 1.1],
            );
            self.skin(
                &mut body,
                sphere(0.15),
                [0.0, y + 0.02, 0.25],
                [0.0; 3],
                [1.0, 0.9, 1.0],
            );
            self.eyes(&mut body, [0.0, y + 0.06, 0.35], 0.09, 0.045, false);
            let dark = self.dark();
            for s in [-1.0, 1.0] {
                self.part(
                    &mut body,
                    cylinder(0.006, 0.01, 0.4),
                    &dark,
                    [s * 0.08, y + 0.25, 0.42],
                    [-35.0, 0.0, s * -25.0],
                    [1.0; 3],
                );
                // mandibles
                self.part(
                    &mut body,
                    cylinder(0.0, 0.025, 0.12),
                    &dark,
                    [s * 0.05, y - 0.08, 0.4],
                    [80.0, 0.0, s * 25.0],
                    [1.0; 3],
                );
            }
            self.legs(&mut body, pairs, y, (0.15, -0.15), 0.7);
        }
    }

    fn bat(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        self.skin(
            &mut body,
            capsule(0.14, 0.42),
            [0.0, 0.45, 0.0],
            [20.0, 0.0, 0.0],
            [1.0; 3],
        );
        self.skin(
            &mut body,
            sphere(0.12),
            [0.0, 0.68, 0.1],
            [0.0; 3],
            [1.0; 3],
        );
        for s in [-1.0f32, 1.0] {
            self.skin(
                &mut body,
                cylinder(0.0, 0.045, 0.16),
                [s * 0.07, 0.83, 0.08],
                [0.0, 0.0, s * -15.0],
                [1.0; 3],
            );
        }
        self.eyes(&mut body, [0.0, 0.7, 0.2], 0.05, 0.02, true);
        let membrane = self.dark();
        for (s, name) in [(-1.0f32, "WingL"), (1.0, "WingR")] {
            let mut wing = self.pivot(&mut body, name, [s * 0.1, 0.55, 0.0]);
            // two scalloped panels and a bony edge
            self.part(
                &mut wing,
                prism(0.7, 0.45, 0.02),
                &membrane,
                [s * 0.38, -0.02, 0.0],
                [0.0, 0.0, s * -100.0],
                [1.0; 3],
            );
            self.part(
                &mut wing,
                prism(0.5, 0.35, 0.02),
                &membrane,
                [s * 0.7, 0.02, -0.02],
                [0.0, 0.0, s * -80.0],
                [1.0; 3],
            );
            let bone = self.dark();
            self.part(
                &mut wing,
                cylinder(0.012, 0.018, 0.95),
                &bone,
                [s * 0.48, 0.14, 0.0],
                [0.0, 0.0, s * 82.0],
                [1.0; 3],
            );
        }
    }

    fn bird(&mut self, root: &mut Gd<Node3D>) {
        self.skin(
            root,
            sphere(0.26),
            [0.0, 0.5, 0.0],
            [-15.0, 0.0, 0.0],
            [0.9, 0.85, 1.25],
        );
        let mut head = self.pivot(root, "Head", [0.0, 0.72, 0.2]);
        self.skin(
            &mut head,
            capsule(0.07, 0.22),
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [1.0; 3],
        );
        self.skin(
            &mut head,
            sphere(0.11),
            [0.0, 0.12, 0.05],
            [0.0; 3],
            [1.0; 3],
        );
        let beak = self
            .art
            .flat(Color::from_rgb(0.75, 0.55, 0.15), Finish::Glossy);
        self.part(
            &mut head,
            cylinder(0.0, 0.045, 0.14),
            &beak,
            [0.0, 0.1, 0.19],
            [90.0, 0.0, 0.0],
            [1.0; 3],
        );
        let comb = self
            .art
            .flat(Color::from_rgb(0.65, 0.08, 0.06), Finish::Matte);
        self.part(
            &mut head,
            prism(0.02, 0.1, 0.14),
            &comb,
            [0.0, 0.24, 0.03],
            [0.0; 3],
            [1.0; 3],
        );
        self.eyes(&mut head, [0.0, 0.15, 0.12], 0.065, 0.02, false);
        let dark = self.dark();
        for s in [-1.0f32, 1.0] {
            self.skin(
                root,
                sphere(0.2),
                [s * 0.2, 0.52, -0.02],
                [0.0; 3],
                [0.35, 0.7, 1.1],
            );
            self.part(
                root,
                cylinder(0.015, 0.02, 0.3),
                &beak,
                [s * 0.09, 0.15, 0.0],
                [0.0; 3],
                [1.0; 3],
            );
            self.part(
                root,
                cuboid(0.08, 0.015, 0.12),
                &beak,
                [s * 0.09, 0.01, 0.04],
                [0.0; 3],
                [1.0; 3],
            );
        }
        self.part(
            root,
            prism(0.22, 0.3, 0.04),
            &dark,
            [0.0, 0.62, -0.32],
            [-50.0, 0.0, 0.0],
            [1.0; 3],
        );
    }

    fn blob(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        self.skin(
            &mut body,
            sphere(0.5),
            [0.0, 0.42, 0.0],
            [0.0; 3],
            [1.3, 0.85, 1.2],
        );
        for (x, y, z, r) in [
            (0.4f32, 0.22f32, 0.3f32, 0.2f32),
            (-0.45, 0.2, 0.1, 0.24),
            (0.1, 0.15, -0.5, 0.22),
            (-0.2, 0.75, 0.2, 0.14),
        ] {
            self.skin(&mut body, sphere(r), [x, y, z], [0.0; 3], [1.0, 0.8, 1.0]);
        }
        let inner = self.dark();
        self.part(
            &mut body,
            sphere(0.18),
            &inner,
            [0.1, 0.4, 0.1],
            [0.0; 3],
            [1.0; 3],
        );
    }

    fn floating_eye(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        let white = self
            .art
            .flat(Color::from_rgb(0.85, 0.82, 0.78), Finish::Glossy);
        self.part(
            &mut body,
            sphere(0.5),
            &white,
            [0.0, 0.5, 0.0],
            [0.0; 3],
            [1.0; 3],
        );
        let skin = self.skin.clone();
        self.part(
            &mut body,
            cylinder(0.26, 0.26, 0.06),
            &skin,
            [0.0, 0.5, 0.46],
            [90.0, 0.0, 0.0],
            [1.0; 3],
        );
        let pupil = self.eye();
        self.part(
            &mut body,
            cylinder(0.12, 0.12, 0.07),
            &pupil,
            [0.0, 0.5, 0.48],
            [90.0, 0.0, 0.0],
            [1.0; 3],
        );
        let vein = self
            .art
            .flat(Color::from_rgb(0.5, 0.08, 0.06), Finish::Matte);
        for a in [30.0f32, 150.0, 260.0] {
            let t = a.to_radians();
            self.part(
                &mut body,
                cylinder(0.008, 0.008, 0.4),
                &vein,
                [t.cos() * 0.35, 0.5 + t.sin() * 0.35, 0.28],
                [0.0, 0.0, a],
                [1.0; 3],
            );
        }
    }

    fn light(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        let core = self.art.flat(
            Color::from_rgba(self.tint.r, self.tint.g, self.tint.b, 1.0),
            Finish::Ember,
        );
        self.part(
            &mut body,
            sphere(0.28),
            &core,
            [0.0, 0.5, 0.0],
            [0.0; 3],
            [1.0; 3],
        );
        let halo = self.art.flat(
            Color::from_rgba(self.tint.r, self.tint.g, self.tint.b, 0.25),
            Finish::Ghost,
        );
        self.part(
            &mut body,
            sphere(0.5),
            &halo,
            [0.0, 0.5, 0.0],
            [0.0; 3],
            [1.0; 3],
        );
    }

    fn vortex(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        let mist = self.art.flat(
            Color::from_rgba(self.tint.r, self.tint.g, self.tint.b, 0.35),
            Finish::Ghost,
        );
        for i in 0..6 {
            let f = i as f32 / 5.0;
            let r = 0.08 + f * 0.42;
            self.part(
                &mut body,
                torus(r * 0.75, r),
                &mist,
                [f * 0.05, 0.08 + f * 0.85, 0.0],
                [8.0, i as f32 * 50.0, 0.0],
                [1.0, 2.2, 1.0],
            );
        }
        let debris = self.dark();
        for i in 0..5 {
            let t = i as f32 * 1.3;
            self.part(
                &mut body,
                facets(0.04, 4),
                &debris,
                [t.cos() * 0.35, 0.3 + i as f32 * 0.12, t.sin() * 0.35],
                [t * 30.0, t * 40.0, 0.0],
                [1.0; 3],
            );
        }
    }

    fn fungus(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        let stem = self.bone();
        for (x, z, h, r) in [
            (0.0f32, 0.0f32, 0.7f32, 0.34f32),
            (0.32, 0.22, 0.42, 0.22),
            (-0.28, 0.18, 0.32, 0.18),
            (0.12, -0.3, 0.5, 0.24),
        ] {
            self.part(
                &mut body,
                cylinder(r * 0.28, r * 0.38, h),
                &stem,
                [x, h / 2.0, z],
                [0.0; 3],
                [1.0; 3],
            );
            self.skin(
                &mut body,
                dome(r),
                [x, h - 0.02, z],
                [0.0; 3],
                [1.0, 0.8, 1.0],
            );
            let gills = self.dark();
            self.part(
                &mut body,
                cylinder(r * 0.95, r * 0.95, 0.01),
                &gills,
                [x, h - 0.025, z],
                [0.0; 3],
                [1.0; 3],
            );
        }
    }

    /// A low four-legged reptile with a long tail, or with wings a dragon.
    fn lizard(&mut self, root: &mut Gd<Node3D>, dragon: bool) {
        let y = if dragon { 0.45 } else { 0.3 };
        self.skin(
            root,
            sphere(0.3),
            [0.0, y, 0.0],
            [0.0; 3],
            [0.95, 0.72, 1.7],
        );
        let mut neck = self.pivot(root, "Neck", [0.0, y + 0.05, 0.42]);
        let mut head = if dragon {
            self.skin(
                &mut neck,
                capsule(0.1, 0.55),
                [0.0, 0.2, 0.12],
                [-45.0, 0.0, 0.0],
                [1.0; 3],
            );
            self.pivot(&mut neck, "Head", [0.0, 0.42, 0.3])
        } else {
            self.pivot(&mut neck, "Head", [0.0, 0.0, 0.1])
        };
        self.skin(
            &mut head,
            sphere(0.14),
            [0.0, 0.0, 0.08],
            [0.0; 3],
            [0.9, 0.7, 1.6],
        );
        self.eyes(&mut head, [0.0, 0.07, 0.12], 0.08, 0.025, dragon);
        if dragon {
            let horn = self.bone();
            for s in [-1.0f32, 1.0] {
                self.part(
                    &mut head,
                    cylinder(0.0, 0.03, 0.2),
                    &horn,
                    [s * 0.07, 0.12, -0.05],
                    [-60.0, 0.0, s * -15.0],
                    [1.0; 3],
                );
            }
        }
        let mut tail = self.pivot(root, "Tail", [0.0, y, -0.45]);
        self.skin(
            &mut tail,
            cylinder(0.14, 0.02, 0.9),
            [0.0, -y * 0.45, -0.4],
            [-78.0, 0.0, 0.0],
            [1.0; 3],
        );
        // legs: a thigh out from the flank, a shin down to a splayed foot
        let claw = self.dark();
        for (x, z) in [(-1.0f32, 0.3f32), (1.0, 0.3), (-1.0, -0.28), (1.0, -0.28)] {
            let r = if dragon { 0.075 } else { 0.06 };
            self.skin(
                root,
                capsule(r * 1.2, 0.24),
                [x * 0.2, y * 0.78, z],
                [0.0, 0.0, x * -50.0],
                [1.0; 3],
            );
            self.skin(
                root,
                capsule(r, y * 0.75),
                [x * 0.28, y * 0.38, z],
                [0.0, 0.0, x * -10.0],
                [1.0; 3],
            );
            self.part(
                root,
                sphere(r * 1.3),
                &claw,
                [x * 0.3, r * 0.5, z + 0.05],
                [0.0; 3],
                [1.0, 0.45, 1.6],
            );
        }
        if dragon {
            let membrane = self.dark();
            for (s, name) in [(-1.0f32, "WingL"), (1.0, "WingR")] {
                let mut wing = self.pivot(root, name, [s * 0.18, y + 0.15, 0.1]);
                // an arm up and out, fingers back, skin between
                self.skin(
                    &mut wing,
                    cylinder(0.02, 0.04, 0.7),
                    [s * 0.28, 0.28, 0.0],
                    [0.0, 0.0, s * -50.0],
                    [1.0; 3],
                );
                for (i, back) in [20.0f32, 45.0, 70.0].into_iter().enumerate() {
                    let len = 0.75 - i as f32 * 0.12;
                    self.part(
                        &mut wing,
                        cylinder(0.008, 0.015, len),
                        &membrane,
                        [s * 0.52, 0.5 - len * 0.35, -len * 0.3],
                        [back, 0.0, s * -12.0],
                        [1.0; 3],
                    );
                }
                self.part(
                    &mut wing,
                    prism(0.55, 0.62, 0.015),
                    &membrane,
                    [s * 0.42, 0.26, -0.22],
                    [-78.0, s * 12.0, s * -30.0],
                    [1.0; 3],
                );
            }
        }
    }

    fn fish(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0, 0.0, 0.0]);
        self.skin(
            &mut body,
            sphere(0.5),
            [0.0, 0.5, 0.0],
            [0.0; 3],
            [0.5, 0.9, 1.4],
        );
        let mut tail = self.pivot(&mut body, "Tail", [0.0, 0.5, -0.65]);
        let fin = self.dark();
        self.part(
            &mut tail,
            prism(0.04, 0.7, 0.4),
            &fin,
            [0.0, 0.0, -0.15],
            [90.0, 0.0, 0.0],
            [1.0; 3],
        );
        self.part(
            &mut body,
            prism(0.04, 0.35, 0.5),
            &fin,
            [0.0, 0.95, -0.05],
            [0.0; 3],
            [1.0; 3],
        );
        self.eyes(&mut body, [0.0, 0.62, 0.48], 0.2, 0.05, false);
        let teeth = self.bone();
        self.part(
            &mut body,
            cuboid(0.3, 0.03, 0.05),
            &teeth,
            [0.0, 0.42, 0.66],
            [0.0; 3],
            [1.0; 3],
        );
    }

    fn piercer(&mut self, root: &mut Gd<Node3D>) {
        self.skin(
            root,
            cylinder(0.0, 0.32, 1.0),
            [0.0, 0.5, 0.0],
            [0.0; 3],
            [1.0; 3],
        );
        self.skin(
            root,
            facets(0.3, 6),
            [0.1, 0.12, 0.05],
            [0.0, 20.0, 0.0],
            [1.2, 0.5, 1.1],
        );
        self.eyes(root, [0.0, 0.45, 0.17], 0.06, 0.025, true);
    }

    fn beast(&mut self, root: &mut Gd<Node3D>) {
        if self.shape.as_deref() == Some("cat") {
            self.cat(root);
            return;
        }
        self.skin(
            root,
            sphere(0.34),
            [0.0, 0.55, 0.0],
            [0.0; 3],
            [0.8, 0.75, 1.4],
        );
        let mut head = self.pivot(root, "Head", [0.0, 0.72, 0.45]);
        self.skin(
            &mut head,
            sphere(0.18),
            [0.0, 0.0, 0.08],
            [0.0; 3],
            [0.9, 0.85, 1.2],
        );
        self.skin(
            &mut head,
            sphere(0.09),
            [0.0, -0.05, 0.25],
            [0.0; 3],
            [1.0; 3],
        );
        self.eyes(&mut head, [0.0, 0.06, 0.2], 0.08, 0.025, true);
        for s in [-1.0f32, 1.0] {
            self.skin(
                &mut head,
                cylinder(0.0, 0.05, 0.12),
                [s * 0.1, 0.17, 0.02],
                [0.0, 0.0, s * -20.0],
                [1.0; 3],
            );
        }
        let mut tail = self.pivot(root, "Tail", [0.0, 0.62, -0.45]);
        self.skin(
            &mut tail,
            cylinder(0.02, 0.05, 0.45),
            [0.0, -0.1, -0.18],
            [-60.0, 0.0, 0.0],
            [1.0; 3],
        );
        for (x, z) in [(-1.0f32, 0.28f32), (1.0, 0.28), (-1.0, -0.28), (1.0, -0.28)] {
            self.skin(
                root,
                capsule(0.07, 0.5),
                [x * 0.17, 0.25, z],
                [0.0; 3],
                [1.0; 3],
            );
        }
    }
}

impl Kit<'_> {
    /// A cat: a long slender body, a round head with pointed ears and eyes
    /// that catch the light, thin legs and a long tail carried high.
    fn cat(&mut self, root: &mut Gd<Node3D>) {
        let one = [1.0f32; 3];
        self.skin(
            root,
            sphere(0.26),
            [0.0, 0.5, -0.02],
            [0.0; 3],
            [0.72, 0.68, 1.55],
        );
        self.skin(
            root,
            sphere(0.2),
            [0.0, 0.56, 0.24],
            [0.0; 3],
            [0.85, 0.85, 1.0],
        );
        let mut head = self.pivot(root, "Head", [0.0, 0.74, 0.42]);
        self.skin(
            &mut head,
            sphere(0.15),
            [0.0, 0.0, 0.04],
            [0.0; 3],
            [1.0, 0.88, 0.92],
        );
        self.skin(
            &mut head,
            sphere(0.065),
            [0.0, -0.05, 0.16],
            [0.0; 3],
            [1.1, 0.8, 0.9],
        );
        let nose = self
            .art
            .flat(Color::from_rgb(0.55, 0.3, 0.3), Finish::Matte);
        self.part(
            &mut head,
            sphere(0.018),
            &nose,
            [0.0, -0.025, 0.215],
            [0.0; 3],
            one,
        );
        self.eyes(&mut head, [0.0, 0.03, 0.155], 0.058, 0.024, true);
        for s in [-1.0f32, 1.0] {
            self.skin(
                &mut head,
                cylinder(0.0, 0.055, 0.12),
                [s * 0.085, 0.15, 0.0],
                [-10.0, 0.0, s * -18.0],
                one,
            );
        }
        let mut tail = self.pivot(root, "Tail", [0.0, 0.58, -0.42]);
        self.skin(
            &mut tail,
            cylinder(0.022, 0.034, 0.34),
            [0.0, 0.08, -0.14],
            [-35.0, 0.0, 0.0],
            one,
        );
        self.skin(
            &mut tail,
            cylinder(0.014, 0.022, 0.28),
            [0.0, 0.33, -0.25],
            [-8.0, 0.0, 0.0],
            one,
        );
        for (x, z) in [(-1.0f32, 0.3f32), (1.0, 0.3), (-1.0, -0.3), (1.0, -0.3)] {
            self.skin(
                root,
                capsule(0.045, 0.52),
                [x * 0.11, 0.25, z],
                [0.0; 3],
                one,
            );
            self.skin(
                root,
                sphere(0.05),
                [x * 0.11, 0.03, z + 0.03],
                [0.0; 3],
                [1.0, 0.6, 1.3],
            );
        }
    }
}

#[path = "object_kit.rs"]
mod object_kit;

/// A base head's meshes and their skins.
type HeadParts = Vec<(Gd<Mesh>, Option<Gd<GdSkin>>)>;

/// `mesh` with each surface's material as `mi` shows it (an override on
/// the instance included).
fn with_active_materials(mi: &Gd<MeshInstance3D>, mesh: Gd<ArrayMesh>) -> Gd<ArrayMesh> {
    let n = mesh.get_surface_count();
    let missing = (0..n).any(|i| {
        mesh.surface_get_material(i).is_none() || mi.get_surface_override_material(i).is_some()
    });
    if !missing {
        return mesh;
    }
    let mut out = mesh.duplicate_resource();
    for i in 0..n {
        if let Some(m) = mi.get_active_material(i) {
            out.surface_set_material(i, &m);
        }
    }
    out
}

/// Which part of a base character is worn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Head,
    Arms,
}

/// The triangles of `mesh` whose corners all `keep` (in the mesh's own
/// rest pose), every vertex array kept as it is.
fn cut(mesh: &Gd<ArrayMesh>, keep: impl Fn(Vector3) -> bool) -> Gd<ArrayMesh> {
    use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
    let mut out = ArrayMesh::new_gd();
    for i in 0..mesh.get_surface_count() {
        let mut arrays = mesh.surface_get_arrays(i);
        let verts: PackedVector3Array = arrays.at(ArrayType::VERTEX.ord() as usize).to();
        let index: PackedInt32Array = arrays.at(ArrayType::INDEX.ord() as usize).to();
        let above = |k: i32| verts.get(k as usize).is_some_and(&keep);
        let kept: Vec<i32> = index
            .as_slice()
            .chunks(3)
            .filter(|t| t.len() == 3 && t.iter().all(|&k| above(k)))
            .flatten()
            .copied()
            .collect();
        if kept.is_empty() {
            continue;
        }
        arrays.set(
            ArrayType::INDEX.ord() as usize,
            &PackedInt32Array::from(kept.as_slice()).to_variant(),
        );
        // extra UV and colour sets come as custom channels whose formats
        // would have to be passed too; nothing here uses them
        for custom in [
            ArrayType::CUSTOM0,
            ArrayType::CUSTOM1,
            ArrayType::CUSTOM2,
            ArrayType::CUSTOM3,
        ] {
            arrays.set(custom.ord() as usize, &Variant::nil());
        }
        let eight = mesh.surface_get_format(i).ord() & ArrayFormat::FLAG_USE_8_BONE_WEIGHTS.ord();
        out.add_surface_from_arrays_ex(PrimitiveType::TRIANGLES, &arrays)
            .flags(ArrayFormat::from_ord(eight))
            .done();
        if let Some(m) = mesh.surface_get_material(i) {
            let n = out.get_surface_count() - 1;
            out.surface_set_material(n, &m);
        }
    }
    out
}

/// A copy of `mesh` whose materials are multiplied by `colour` and take
/// `texture` as their albedo (a skin tone, a hair colour).
fn recolour(
    mesh: &Gd<ArrayMesh>,
    colour: Option<Color>,
    texture: Option<Gd<Texture2D>>,
) -> Gd<ArrayMesh> {
    if colour.is_none() && texture.is_none() {
        return mesh.clone();
    }
    let mut out = mesh.duplicate_resource();
    for i in 0..out.get_surface_count() {
        let Some(mut m) = out
            .surface_get_material(i)
            .and_then(|m| m.duplicate_resource().try_cast::<BaseMaterial3D>().ok())
        else {
            continue;
        };
        if let Some(c) = colour {
            let albedo = mul(m.get_albedo(), c);
            m.set_albedo(albedo);
        }
        if let Some(t) = &texture {
            m.set_texture(TextureParam::ALBEDO, t);
        }
        out.surface_set_material(i, &m);
    }
    out
}

/// Skin: light scattered under it softens the shading (a face, not a
/// painted mask), a faint rim holds its outline, and `glow` lifts it a
/// little from inside (a face in a hood's shadow).
fn lit_skin(color: Color, glow: f32) -> Gd<Material> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_albedo(color);
    m.set_roughness(0.55);
    m.set_feature(Feature::SUBSURFACE_SCATTERING, true);
    m.set_subsurface_scattering_strength(0.45);
    m.set_feature(Feature::RIM, true);
    m.set_rim(0.25);
    m.set_rim_tint(0.6);
    if glow > 0.0 {
        m.set_feature(Feature::EMISSION, true);
        m.set_emission(color);
        m.set_emission_energy_multiplier(glow);
    }
    m.upcast()
}

/// Cast shadows off for flat ground geometry (map_view).
pub fn no_shadow(mi: &mut Gd<MeshInstance3D>) {
    mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
}
