//! The art library: the manifest (via nh-art), PBR materials, scenes and
//! animation libraries loaded once through the ResourceLoader, and pooled
//! model instances. A model instance is taken for a `ModelLook` and given
//! back when its cell no longer shows it; instances of the same look are
//! reused (a level change reuses the previous level's models), so nodes
//! never pile up. Behind the title the looks a game shows first are built
//! once ahead (`warm`); on the map, a look never built before that is
//! wanted past the frame's budget gets an empty stand-in, built in a later
//! frame (`complete`).
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
    ResourceLoader, Skeleton3D, Skin as GdSkin, StandardMaterial3D, Texture2D,
};
use godot::prelude::*;
use nh_art::{ArtManifest, MaterialSpec, Proc, Resolved, Skin};

#[path = "equip.rs"]
mod equip;
#[path = "outfit.rs"]
mod outfit;

pub use equip::{HELD_NODE, LAMP_LIGHT, THROW_LETS_GO, USE_NODE, Worn};
pub use outfit::{GLOVES_NODE, NECK_NODE};

use crate::meshes::{MeshKey, capsule, cuboid, cylinder, dome, facets, prism, sphere, torus};

/// The manifest built into the client, used when the project's copy cannot
/// be read.
const BUILT_IN_MANIFEST: &str = include_str!("../../../godot/art/manifest.json");
const ART_ROOT: &str = "res://art/";
/// Metals keep some colour of their own under the torch.
const MAX_METALLIC: f32 = 0.45;
/// Pooled instances kept per look; more are freed.
const POOL_MAX: usize = 24;
/// Time per frame the map's new instances may take; a look never seen
/// before that is wanted beyond it is built in a later frame (an empty
/// node stands in meanwhile).
const BUILD_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

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

/// What a look wears on its bones, by the slot that hides it.
type Extras = Vec<(Option<String>, Gd<Node3D>)>;

/// A model instance on the map.
pub struct Model {
    /// Placed by the map; its child holds the model's own transform.
    pub node: Gd<Node3D>,
    /// That child: the model at its look's size, lift and turn (None while
    /// a stand-in).
    inner: Option<Gd<Node3D>>,
    key: PoolKey,
    player: Option<Gd<AnimationPlayer>>,
    /// What it carries (the hero only).
    worn: Option<Box<Worn>>,
    /// What its look wears on its bones (a role's hat), by the slot that
    /// hides it while something is worn there.
    extras: Extras,
    /// The look still to build into `node` (see `Art::complete`).
    pending: Option<ModelLook>,
}

impl Model {
    /// The manifest model it is an instance of (self-tests).
    pub fn model_index(&self) -> usize {
        self.key.model
    }

    pub fn player(&self) -> Option<&Gd<AnimationPlayer>> {
        self.player.as_ref().filter(|p| p.is_instance_valid())
    }

    /// Still an empty stand-in for its look.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
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

/// A model's own transform for a look: its size, its lift off the ground
/// and its turn; a corpse with no death clip on its side.
fn inner_transform(look: &ModelLook, spec: &nh_art::ModelSpec) -> Transform3D {
    let r = look.art;
    let lying = look.pose == Pose::Corpse && spec.anims.death.is_none();
    let mut rot = r.rot;
    let mut lift = r.lift;
    if lying {
        // on its side, in the cell
        rot[2] += 88.0;
        lift += r.height * 0.22;
    }
    let s = r.scale;
    transform([0.0, lift, 0.0], rot, [s, s, s])
}

fn transform(pos: [f32; 3], rot: [f32; 3], scale: [f32; 3]) -> Transform3D {
    let basis = Basis::from_euler(EulerOrder::YXZ, deg(rot))
        * Basis::from_scale(Vector3::new(scale[0], scale[1], scale[2]));
    Transform3D::new(basis, Vector3::new(pos[0], pos[1], pos[2]))
}

/// Something to load before it is first needed.
#[derive(Clone)]
enum Preload {
    Texture(String),
    Library(String, bool),
    Scene(usize),
    /// A base head's meshes (or its arms).
    Head(String, Region),
    /// A scene's meshes shaded smooth.
    Smooth(usize),
    /// A model worn on another's bones, built once (its meshes are made).
    Prop(usize),
    /// An instance built once and pooled: its meshes, materials and
    /// shaders are ready before the look is first seen.
    Warm(ModelLook),
}

/// Time per frame the preloading may take.
const PRELOAD_BUDGET: std::time::Duration = std::time::Duration::from_millis(6);
/// Animations stripped of their root motion in one step.
const STRIP_STEP: usize = 4;

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
    proc_anims: HashMap<(Proc, Option<String>), Gd<AnimationLibrary>>,
    /// The clips built in code on the characters' skeleton (`proc/read`).
    proc_clips: Option<Gd<AnimationLibrary>>,
    /// Meshes shaded smooth, by the source mesh's id.
    smoothed: HashMap<i64, Gd<Mesh>>,
    /// The meshes (and their skins) of each base head, built once.
    heads: HashMap<String, Option<HeadParts>>,
    /// The base characters' bodies cut to a region, by (base, region, cut
    /// in mm).
    bodies: HashMap<(String, u8, i32), CutBody>,
    /// What the worn things put on an outfit, built once.
    outfits: outfit::Outfits,
    pool: HashMap<PoolKey, Vec<Model>>,
    warned: HashSet<String>,
    /// Instances handed out and not given back.
    live: usize,
    /// Instances ever built (self-tests watch the pool work).
    built: usize,
    phase: u32,
    /// What is still to load ahead of need, last first.
    preload: Vec<Preload>,
    /// The files the preloading needs were asked of the loader's threads.
    requested: bool,
    /// Files being read on worker threads (`res://` paths), and the ones
    /// read, held so that a load of them is the cached resource.
    loading: Vec<String>,
    loaded: HashMap<String, Gd<godot::classes::Resource>>,
    /// A library being stripped of its root motion over several steps:
    /// its name, the source, what is stripped so far, the next animation.
    stripping: Option<(String, Gd<AnimationLibrary>, Gd<AnimationLibrary>, usize)>,
    /// Looks built at least once (later instances are cheap).
    warmed: HashSet<PoolKey>,
    /// The hero's model given back with their gear on (a level change):
    /// taken again as it is.
    kept: Option<Model>,
    /// Time the map's new instances took this frame (None: no budget,
    /// everything is built at once).
    spent: Option<std::time::Duration>,
    /// Stand-ins handed out and not yet built.
    pending: usize,
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
            bodies: HashMap::new(),
            outfits: outfit::Outfits::default(),
            pool: HashMap::new(),
            warned: HashSet::new(),
            live: 0,
            built: 0,
            phase: 0,
            preload: Vec::new(),
            requested: false,
            loading: Vec::new(),
            loaded: HashMap::new(),
            stripping: None,
            warmed: HashSet::new(),
            kept: None,
            spent: None,
            pending: 0,
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
        let mut heads: Vec<(String, Region)> = self
            .manifest
            .models()
            .flat_map(|(_, _, m)| {
                let head = m.head.clone().map(|h| (h, Region::Head));
                head.into_iter()
                    .chain(m.bare_arms.clone().map(|a| (a, Region::bare(m))))
            })
            .filter(|(h, _)| self.manifest.head(h).is_some())
            .collect();
        heads.sort_by_key(|(h, r)| (h.clone(), *r as u8));
        heads.dedup();
        q.extend(heads.into_iter().map(|(h, r)| Preload::Head(h, r)));
        q.extend(
            self.manifest
                .models()
                .filter(|(_, _, m)| m.smooth && m.scene.is_some())
                .map(|(i, _, _)| Preload::Smooth(i)),
        );
        q.reverse();
        self.preload = q;
        self
    }

    /// Build each look once ahead of need (and what it needs first), after
    /// what is queued already, or before it with `first`.
    pub fn warm(&mut self, looks: &[ModelLook], first: bool) {
        let mut q = Vec::new();
        for look in looks {
            let spec = self.manifest.model_at(look.art.model).1;
            if spec.proc.is_none() && spec.scene.is_some() {
                let rigs = spec.rig.iter().chain(&spec.extra_rigs);
                q.extend(rigs.map(|r| Preload::Library(r.clone(), spec.strip_root)));
                q.push(Preload::Scene(look.art.model));
                if spec.smooth {
                    q.push(Preload::Smooth(look.art.model));
                }
                q.extend(spec.head.clone().map(|h| Preload::Head(h, Region::Head)));
                q.extend(
                    spec.bare_arms
                        .clone()
                        .map(|a| Preload::Head(a, Region::bare(spec))),
                );
                q.extend(
                    spec.extras
                        .iter()
                        .filter_map(|e| self.manifest.model_index(&e.model))
                        .map(Preload::Prop),
                );
            }
            q.push(Preload::Warm(*look));
        }
        if first {
            self.preload.extend(q.into_iter().rev());
        } else {
            let rest = std::mem::take(&mut self.preload);
            self.preload = q.into_iter().rev().chain(rest).collect();
        }
    }

    /// Nothing is left to load ahead.
    pub fn preloaded(&self) -> bool {
        self.preload.is_empty()
    }

    /// Load ahead for a few milliseconds; false when all is loaded. The
    /// files are read on the loader's threads (an item waits for its
    /// own); what is left for the main thread comes in steps of a few
    /// milliseconds.
    pub fn preload_step(&mut self) -> bool {
        let start = std::time::Instant::now();
        if !self.requested {
            self.requested = true;
            let files: Vec<String> = self
                .preload
                .iter()
                .rev()
                .flat_map(|p| self.files_of(p))
                .collect();
            for f in files {
                self.request(f);
            }
        }
        self.collect_loads();
        // a step is not begun that would end past the budget at the pace
        // of the last one
        let mut last = std::time::Duration::ZERO;
        while start.elapsed() + last < PRELOAD_BUDGET {
            let Some(next) = self.preload.last().cloned() else {
                self.loaded.clear();
                return false;
            };
            let mut waiting = false;
            for f in self.files_of(&next) {
                waiting |= self.request(f);
            }
            if waiting {
                return true;
            }
            let began = std::time::Instant::now();
            self.preload_item(next);
            last = began.elapsed();
        }
        true
    }

    /// Do (a step of) the next item; it leaves the queue once done.
    fn preload_item(&mut self, item: Preload) {
        match item {
            Preload::Texture(t) => {
                self.texture(&t);
            }
            Preload::Library(name, true) if !self.libraries.contains_key(&(name.clone(), true)) => {
                if !self.strip_step(&name) {
                    return;
                }
            }
            Preload::Library(name, strip) => {
                self.library(&name, strip);
            }
            Preload::Scene(i) => {
                self.scene(i);
            }
            Preload::Head(kind, region) => {
                self.head_parts(&kind, region);
            }
            Preload::Smooth(i) => {
                if let Some(scene) = self.scene(i) {
                    let mut inner = scene.instantiate_as::<Node3D>();
                    self.smooth(&inner);
                    inner.queue_free();
                }
            }
            Preload::Prop(i) => {
                if let Some(mut prop) = self.prop(i, Color::WHITE, false) {
                    prop.queue_free();
                }
            }
            Preload::Warm(look) => {
                if !self.warmed.contains(&PoolKey::of(&look)) {
                    // never a stand-in
                    let spent = self.spent.take();
                    let m = self.take(&look);
                    self.give(m);
                    self.spent = spent;
                }
            }
        }
        self.preload.pop();
    }

    /// The files an item reads (`res://` paths).
    fn files_of(&self, item: &Preload) -> Vec<String> {
        let full = |p: &str| format!("{ART_ROOT}{p}");
        let scene = |i: usize| self.manifest.model_at(i).1.scene.as_deref().map(full);
        let library = |n: &str| self.manifest.library(n).map(&full);
        let head = |kind: &str, region: Region| -> Vec<String> {
            let Some(h) = self.manifest.head(kind) else {
                return Vec::new();
            };
            let hair = h.hair.iter().filter(|_| region == Region::Head);
            std::iter::once(&h.base)
                .chain(hair)
                .chain(&h.skin)
                .map(|p| full(p))
                .collect()
        };
        match item {
            Preload::Texture(t) => vec![full(t)],
            Preload::Library(n, _) => library(n).into_iter().collect(),
            Preload::Scene(i) | Preload::Smooth(i) | Preload::Prop(i) => {
                scene(*i).into_iter().collect()
            }
            Preload::Head(kind, region) => head(kind, *region),
            Preload::Warm(look) => {
                let spec = self.manifest.model_at(look.art.model).1;
                let mut out: Vec<String> = scene(look.art.model).into_iter().collect();
                out.extend(
                    spec.rig
                        .iter()
                        .chain(&spec.extra_rigs)
                        .filter_map(|r| library(r)),
                );
                out.extend(spec.head.iter().flat_map(|h| head(h, Region::Head)));
                out.extend(
                    spec.bare_arms
                        .iter()
                        .flat_map(|h| head(h, Region::bare(spec))),
                );
                out.extend(
                    spec.extras
                        .iter()
                        .filter_map(|e| self.manifest.model_index(&e.model))
                        .filter_map(scene),
                );
                out
            }
        }
    }

    /// Ask the loader's threads for a file not read yet; true while it is
    /// being read.
    fn request(&mut self, path: String) -> bool {
        if self.loaded.contains_key(&path) {
            return false;
        }
        if self.loading.contains(&path) {
            return true;
        }
        let mut loader = ResourceLoader::singleton();
        // a missing file is reported where it is used (the threaded calls
        // are not in the bindings: called by name)
        if !loader.exists(&path)
            || loader
                .call("load_threaded_request", &[path.to_variant()])
                .try_to::<i64>()
                .ok()
                != Some(0)
        {
            return false;
        }
        self.loading.push(path);
        true
    }

    /// Take the files the loader's threads have read.
    fn collect_loads(&mut self) {
        // ResourceLoader.ThreadLoadStatus
        const IN_PROGRESS: i64 = 1;
        const LOADED: i64 = 3;
        let mut loader = ResourceLoader::singleton();
        for path in std::mem::take(&mut self.loading) {
            let arg = [path.to_variant()];
            match loader
                .call("load_threaded_get_status", &arg)
                .try_to::<i64>()
            {
                Ok(IN_PROGRESS) => self.loading.push(path),
                Ok(LOADED) => {
                    let r = loader.call("load_threaded_get", &arg);
                    if let Ok(r) = r.try_to::<Gd<godot::classes::Resource>>() {
                        self.loaded.insert(path, r);
                    }
                }
                // a failed one is reported where it is used
                _ => {}
            }
        }
    }

    /// Strip a few more of a library's animations of their root motion;
    /// true once the library is done.
    fn strip_step(&mut self, name: &str) -> bool {
        let (src, out, next) = match self.stripping.take() {
            Some((n, src, out, next)) if n == name => (src, out, next),
            // the source first, its animations in the steps after
            _ => match self.library(name, false) {
                Some(src) => {
                    let out = AnimationLibrary::new_gd();
                    self.stripping = Some((name.to_string(), src, out, 0));
                    return false;
                }
                None => {
                    self.libraries.insert((name.to_string(), true), None);
                    return true;
                }
            },
        };
        let mut out = out;
        let names = src.get_animation_list();
        let end = (next + STRIP_STEP).min(names.len());
        for i in next..end {
            if let Some(a) = names.get(i) {
                strip_into(&mut out, &src, &a);
            }
        }
        if end >= names.len() {
            self.libraries.insert((name.to_string(), true), Some(out));
            true
        } else {
            self.stripping = Some((name.to_string(), src, out, end));
            false
        }
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
        let maps = match (spec.albedo_path(), spec.normal_path(), spec.arm_path()) {
            (Some(a), Some(n), Some(orm)) => Some((a, n, orm)),
            _ => None,
        };
        // an ORM material takes its roughness and metal from the ORM map
        // alone: without one it would be all metal, black where nothing
        // is there to reflect (the paper doll); a plain one keeps the
        // spec's own
        let mut m: Gd<BaseMaterial3D> = if maps.is_some() {
            OrmMaterial3D::new_gd().upcast()
        } else {
            StandardMaterial3D::new_gd().upcast()
        };
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
        let textured = maps.is_some();
        if let Some((a, n, orm)) = maps {
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
            // a pattern on a body that bends is by the mesh's own UVs
            m.set_flag(Flags::UV1_USE_TRIPLANAR, !spec.uv);
            m.set_flag(Flags::UV1_USE_WORLD_TRIPLANAR, world && !spec.uv);
            let s = spec.uv_scale;
            m.set_uv1_scale(Vector3::new(s, s, s));
        }
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

    /// A model instance for this look: one given back before, or a new one
    /// (on the map, past this frame's budget, a stand-in for a look never
    /// built before: see `complete`).
    pub fn take(&mut self, look: &ModelLook) -> Model {
        let key = PoolKey::of(look);
        self.live += 1;
        if let Some(mut m) = self.pool.get_mut(&key).and_then(Vec::pop) {
            m.node.set_visible(true);
            self.start(&mut m, look);
            return m;
        }
        let over = self.spent.is_some_and(|t| t >= BUILD_BUDGET);
        if over && !self.warmed.contains(&key) {
            let mut node = Node3D::new_alloc();
            self.root.add_child(&node);
            node.set_visible(true);
            self.pending += 1;
            return Model {
                node,
                inner: None,
                key,
                player: None,
                worn: None,
                extras: Vec::new(),
                pending: Some(*look),
            };
        }
        let start = std::time::Instant::now();
        self.built += 1;
        let mut m = self.build(look, key);
        self.start(&mut m, look);
        self.warmed.insert(key);
        if let Some(t) = self.spent.as_mut() {
            *t += start.elapsed();
        }
        m
    }

    /// The hero's model: the one given back with their gear on, if it is
    /// of this look, else as `take`.
    pub fn take_hero(&mut self, look: &ModelLook) -> Model {
        match self.kept.take() {
            Some(mut m) if m.key == PoolKey::of(look) => {
                self.live += 1;
                m.node.set_visible(true);
                self.start(&mut m, look);
                m
            }
            other => {
                if let Some(m) = other {
                    self.give_plain(m);
                }
                self.take(look)
            }
        }
    }

    /// Build a stand-in's look into it if this frame's budget allows;
    /// true when it was built.
    pub fn complete(&mut self, m: &mut Model) -> bool {
        let Some(look) = m.pending else {
            return true;
        };
        if self.spent.is_some_and(|t| t >= BUILD_BUDGET) {
            return false;
        }
        let start = std::time::Instant::now();
        self.built += 1;
        let (inner, player, extras) = self.build_into(&m.node, &look);
        m.inner = Some(inner);
        m.player = player;
        m.extras = extras;
        m.worn = None;
        m.pending = None;
        self.pending = self.pending.saturating_sub(1);
        self.start(m, &look);
        self.warmed.insert(m.key);
        if let Some(t) = self.spent.as_mut() {
            *t += start.elapsed();
        }
        true
    }

    /// A new frame of the map: the budget for new instances starts again.
    pub fn begin_frame(&mut self) {
        self.spent = Some(std::time::Duration::ZERO);
    }

    /// Stand-ins not yet built.
    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Hide an instance and keep it for the next look like it; the hero's
    /// keeps their gear on (the next level shows them as they were).
    pub fn give(&mut self, mut m: Model) {
        self.live = self.live.saturating_sub(1);
        if m.pending.is_some() {
            self.pending = self.pending.saturating_sub(1);
            m.node.queue_free();
            return;
        }
        m.node.set_visible(false);
        if let Some(p) = m.player.as_mut() {
            p.pause();
        }
        if m.worn.as_ref().is_some_and(|w| w.has_gear()) {
            self.put_down(&mut m);
            if let Some(old) = self.kept.replace(m) {
                self.give_plain(old);
            }
            return;
        }
        self.give_plain(m);
    }

    /// Into the pool, the gear taken off.
    fn give_plain(&mut self, mut m: Model) {
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
                gait: has(Some("walk")),
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

    /// Start the look: its size, lift and turn (an instance from the pool
    /// was another look's of the same model and tint, a hobbit's body the
    /// shopkeeper's next), then its animation: idle with a random phase; a
    /// corpse lies at the end of its death; a statue stands still.
    fn start(&mut self, m: &mut Model, look: &ModelLook) {
        let spec = self.manifest.model_at(look.art.model).1;
        if let Some(inner) = m.inner.as_mut() {
            inner.set_transform(inner_transform(look, spec));
        }
        let Some(player) = m.player.as_mut() else {
            return;
        };
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
                library_in(&scene).or_else(|| {
                    let mut inst = scene.instantiate()?;
                    let lib =
                        find::<AnimationPlayer>(&inst).and_then(|p| p.get_animation_library(""));
                    inst.queue_free();
                    lib
                })
            })
        };
        if lib.is_none() {
            self.warn_once(format!("animation library {name} does not load"));
        }
        self.libraries.insert(key, lib.clone());
        lib
    }

    fn build(&mut self, look: &ModelLook, key: PoolKey) -> Model {
        let holder = Node3D::new_alloc();
        let (inner, player, extras) = self.build_into(&holder, look);
        self.root.add_child(&holder);
        Model {
            node: holder,
            inner: Some(inner),
            key,
            player,
            worn: None,
            extras,
            pending: None,
        }
    }

    /// The look's model under `holder`: its own node, its animation player
    /// and extras.
    fn build_into(
        &mut self,
        holder: &Gd<Node3D>,
        look: &ModelLook,
    ) -> (Gd<Node3D>, Option<Gd<AnimationPlayer>>, Extras) {
        let r = look.art;
        let spec = self.manifest.model_at(r.model).1.clone();
        let mut holder = holder.clone();
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
                    self.attach_base_head(&inner, kind, Region::bare(&spec));
                }
                let shade = rgb(spec.shade_rgb());
                self.dress(&inner, look, shade);
                (inner, player)
            }
            (None, None) => self.build_proc(Proc::Blob, &spec, look),
        };
        inner.set_transform(inner_transform(look, &spec));
        let extras = if spec.proc.is_none() {
            self.attach_extras(&inner, &spec, look)
        } else {
            Vec::new()
        };
        holder.add_child(&inner);
        (inner, player, extras)
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
        // the head's skin tone, for the bare hands of an outfit
        let skin_tone = spec
            .head
            .as_deref()
            .and_then(|h| self.manifest.head(h))
            .and_then(|h| h.tint.as_deref())
            .and_then(nh_art::hex)
            .map_or(Color::WHITE, rgb);
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
                .find(|(k, _, _)| !head && !k.starts_with('@') && name.ends_with(k.as_str()));
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
                        // an outfit's bare hands are skin: the head's tone,
                        // never the cloth's dye
                        if is_skin(&src) {
                            let tone = mul(look.tint, skin_tone);
                            return self.derive(&src, tone, ghost, None);
                        }
                        // a surface dyed by its material's name ("@Main"):
                        // one mesh of several materials, its coat apart
                        // from its horns and eyes
                        let material = src.get_name().to_string();
                        let own = recolor.iter().find(|(k, _, _)| {
                            !head && k.strip_prefix('@') == Some(material.as_str())
                        });
                        let (tint, flat) = match own {
                            Some((_, c, false)) => (mul(tint, *c), false),
                            Some((_, c, true)) => (mul(look.tint, *c), true),
                            None => (tint, flat),
                        };
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

    /// Shade a scene's meshes smooth: each corner's normal is the mean of
    /// the faces around its place, across the seams that split a low-poly
    /// model's corners (the skin weights stay); once per mesh, shared by
    /// every instance.
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
            let Some(src) = mi.get_mesh().and_then(|m| m.try_cast::<ArrayMesh>().ok()) else {
                continue;
            };
            // the model's up in the mesh's own space
            let mut to_model = Transform3D::IDENTITY;
            let mut at: Gd<Node> = mi.clone().upcast();
            while at != inner.clone().upcast::<Node>() {
                let Ok(n) = at.clone().try_cast::<Node3D>() else {
                    break;
                };
                to_model = n.get_transform() * to_model;
                let Some(parent) = at.get_parent() else {
                    break;
                };
                at = parent;
            }
            let up = (to_model.basis.inverse() * Vector3::UP).normalized();
            let key = src.instance_id().to_i64();
            let mesh = self
                .smoothed
                .entry(key)
                .or_insert_with(|| welded(&src, up).upcast())
                .clone();
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
        let Some(parts) = self.head_parts(kind, region) else {
            return false;
        };
        for (i, (mesh, skin, materials)) in parts.iter().enumerate() {
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_name(&format!("BaseHead{i}"));
            mi.set_mesh(mesh);
            for (s, m) in materials.iter().enumerate() {
                if let Some(m) = m {
                    mi.set_surface_override_material(s as i32, m);
                }
            }
            if let Some(skin) = skin {
                mi.set_skin(skin);
            }
            skeleton.add_child(&mi);
            mi.set_skeleton_path(&NodePath::from(".."));
        }
        true
    }

    /// A base head's meshes, built once.
    fn head_parts(&mut self, kind: &str, region: Region) -> Option<HeadParts> {
        let key = format!("{kind}{region:?}");
        if let Some(p) = self.heads.get(&key) {
            return p.clone();
        }
        let p = self.build_head(kind, region);
        if p.is_none() {
            self.warn_once(format!("head {kind} does not load"));
        }
        self.heads.insert(key, p.clone());
        p
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
                // a material set on the instance (not the mesh) is worn too
                let worn: Vec<Option<Gd<Material>>> = (0..mesh.get_surface_count())
                    .map(|i| mi.get_active_material(i))
                    .collect();
                // the body's own mesh is the tall one; the eyes and the
                // eyebrows come whole
                let body = base && mesh.get_aabb().size.y > 0.5;
                if region != Region::Head && !body {
                    continue;
                }
                let (mesh, worn) = if body {
                    let (cut, kept) = self.body_cut(&path, region, spec.cut, &mesh);
                    let worn = kept
                        .iter()
                        .map(|&i| worn.get(i).cloned().flatten())
                        .collect();
                    (cut, worn)
                } else {
                    (mesh, worn)
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
                let materials = recoloured(&worn, colour, texture);
                out.push((mesh.upcast::<Mesh>(), mi.get_skin(), materials));
            }
            inst.queue_free();
        }
        (!out.is_empty()).then_some(out)
    }

    /// A base character's body cut to `region` (whole triangles; the skin
    /// weights stay), once per base and region: the mesh, and the source
    /// surfaces it keeps.
    fn body_cut(&mut self, base: &str, region: Region, at: f32, mesh: &Gd<ArrayMesh>) -> CutBody {
        let key = (base.to_string(), region as u8, (at * 1000.0).round() as i32);
        if let Some(c) = self.bodies.get(&key) {
            return c.clone();
        }
        let c = match region {
            Region::Head => cut(mesh, |v| v.y >= at),
            // in the rest pose the arms reach out sideways from the
            // shoulders
            Region::Arms => cut(mesh, |v| v.x.abs() > 0.2 && v.y > 1.2),
            // from the waist (under the trousers' top) to the neck
            Region::Torso => cut(mesh, |v| v.y > 1.06 && v.y < at + 0.03),
        };
        self.bodies.insert(key, c.clone());
        c
    }

    /// The things the model's spec wears on its bones (`extras`).
    fn attach_extras(
        &mut self,
        inner: &Gd<Node3D>,
        spec: &nh_art::ModelSpec,
        look: &ModelLook,
    ) -> Extras {
        let mut out = Vec::new();
        if spec.extras.is_empty() {
            return out;
        }
        let Some(mut skeleton) = find::<Skeleton3D>(&inner.clone().upcast()) else {
            return out;
        };
        for e in &spec.extras {
            if let Some((_, holder)) = self.attach_extra(&mut skeleton, e, look.pose == Pose::Ghost)
            {
                out.push((e.slot.clone(), holder));
            }
        }
        out
    }

    /// `extras` on the bones of a model already built, besides its look's
    /// own (what a portrait adds); the attachments, to free before the
    /// model goes back to its pool.
    pub fn attach(&mut self, model: &Model, extras: &[nh_art::Extra]) -> Vec<Gd<Node>> {
        let Some(mut skeleton) = find::<Skeleton3D>(&model.node.clone().upcast()) else {
            return Vec::new();
        };
        extras
            .iter()
            .filter_map(|e| self.attach_extra(&mut skeleton, e, false))
            .map(|(bone, _)| bone.upcast())
            .collect()
    }

    /// One extra on its bone: the attachment and the holder of the model.
    fn attach_extra(
        &mut self,
        skeleton: &mut Gd<Skeleton3D>,
        e: &nh_art::Extra,
        ghost: bool,
    ) -> Option<(Gd<BoneAttachment3D>, Gd<Node3D>)> {
        let model = self.manifest.model_index(&e.model)?;
        let tint = e.tint_rgb().map_or(Color::WHITE, rgb);
        let node = self.prop(model, tint, ghost)?;
        let mut bone = BoneAttachment3D::new_alloc();
        bone.set_bone_name(&e.bone);
        skeleton.add_child(&bone);
        let k = e.size / self.manifest.model_at(model).1.size;
        let mut holder = Node3D::new_alloc();
        holder.set_name(&format!("Extra_{}", e.model));
        holder.set_transform(transform(e.pos, e.rot, [k, k, k]));
        holder.add_child(&node);
        bone.add_child(&holder);
        Some((bone, holder))
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
                    // the shade baked into a smoothed mesh's corners
                    if spec.smooth {
                        m.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
                    }
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

    fn proc_library(&mut self, kind: Proc, shape: Option<&str>) -> Option<Gd<AnimationLibrary>> {
        let key = (kind, shape.map(str::to_string));
        if let Some(l) = self.proc_anims.get(&key) {
            return Some(l.clone());
        }
        let waves = proc_waves(kind);
        if waves.is_empty() {
            return None;
        }
        let mut lib = AnimationLibrary::new_gd();
        let _ = lib.add_animation("idle", &wave_animation(&waves, WAVE_PERIOD));
        // the bodies of creature_kit walk on their legs (the rest hop)
        let walk = creature_kit::walk_waves(kind, shape);
        if !walk.is_empty() {
            let _ = lib.add_animation("walk", &wave_animation(&walk, creature_kit::WALK_PERIOD));
        }
        self.proc_anims.insert(key, lib.clone());
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
        let player = kit
            .art
            .proc_library(kind, spec.shape.as_deref())
            .map(|lib| {
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
        strip_into(&mut out, lib, &name);
    }
    out
}

/// A copy of `lib`'s animation `name` without its position tracks, into
/// `out`.
fn strip_into(out: &mut Gd<AnimationLibrary>, lib: &Gd<AnimationLibrary>, name: &StringName) {
    let Some(anim) = lib.get_animation(name) else {
        return;
    };
    let mut copy = anim.duplicate_resource();
    for t in (0..copy.get_track_count()).rev() {
        if copy.track_get_type(t) == TrackType::POSITION_3D {
            copy.remove_track(t);
        }
    }
    let _ = out.add_animation(name, &copy);
}

/// The animation library of a scene's AnimationPlayer, read from the
/// packed scene without making its nodes.
fn library_in(scene: &Gd<PackedScene>) -> Option<Gd<AnimationLibrary>> {
    let state = scene.get_state()?;
    (0..state.get_node_count())
        .filter(|&n| state.get_node_type(n) == "AnimationPlayer")
        .find_map(|n| {
            (0..state.get_node_property_count(n))
                .find(|&p| state.get_node_property_name(n, p) == "libraries/")
                .and_then(|p| state.get_node_property_value(n, p).try_to().ok())
        })
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

fn wave_animation(waves: &[Wave], period: f32) -> Gd<Animation> {
    let mut a = Animation::new_gd();
    a.set_length(period);
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
            let time = f64::from(f * period);
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
            Wave::Rock("Body", y, 4.0, 0.0),
            Wave::Pulse("Body", 0.02, 0.25),
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
            Proc::Worm => self.worm_body(root),
            Proc::Bug => match self.shape.as_deref() {
                Some("beetle") => self.beetle_body(root),
                Some("grid") => self.ant_body(root, true),
                _ => self.ant_body(root, false),
            },
            Proc::Spider => self.bug(root, 4, true),
            Proc::Bat => self.bat(root),
            Proc::Bird => self.bird(root),
            Proc::Blob => self.blob_body(root),
            Proc::Eye => self.eye_body(root),
            Proc::Light => match self.shape.as_deref() {
                Some("spore") => self.spore_body(root),
                _ => self.light(root),
            },
            Proc::Vortex => self.vortex(root),
            Proc::Fungus => match self.shape.as_deref() {
                Some("lichen") => self.lichen(root),
                Some("mold") => self.mold(root),
                _ => self.fungus(root),
            },
            Proc::Lizard => self.lizard_body(root),
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
            self.cat_body(root);
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

#[path = "object_kit.rs"]
mod object_kit;

#[path = "creature_kit.rs"]
mod creature_kit;

/// A base head's meshes and their skins.
/// A mesh cut from another, and the source surfaces it keeps.
type CutBody = (Gd<ArrayMesh>, Vec<usize>);

/// A base head's meshes, each with its skin and the materials worn on its
/// surfaces.
type HeadParts = Vec<(Gd<Mesh>, Option<Gd<GdSkin>>, Vec<Option<Gd<Material>>>)>;

/// Which part of a base character is worn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Head,
    Arms,
    /// The arms and the chest.
    Torso,
}

impl Region {
    /// What a model shows of its base character's body.
    fn bare(spec: &nh_art::ModelSpec) -> Region {
        if spec.bare_chest {
            Region::Torso
        } else {
            Region::Arms
        }
    }
}

/// A mesh's material that is skin (the outfits' bare hands).
fn is_skin(m: &Gd<Material>) -> bool {
    m.get_name().to_string().starts_with("MI_Regular")
}

/// `mesh` with each corner's normal averaged over every face that meets at
/// its position (corners split by seams included), and a shade baked into
/// its corners (`up` is the model's up in the mesh's space): undersides
/// and low parts (legs, hooves) in their own shadow, and patches across
/// the coat, so a flat-coloured low-poly animal is not plastic. The shade
/// moves with the skin; the materials multiply it in.
fn welded(mesh: &Gd<ArrayMesh>, up: Vector3) -> Gd<ArrayMesh> {
    use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
    let place = |v: Vector3| {
        let q = |f: f32| (f * 10_000.0).round() as i64;
        (q(v.x), q(v.y), q(v.z))
    };
    let bounds = mesh.get_aabb();
    let heights: Vec<f32> = (0..8)
        .map(|i| {
            let pick = |bit: i32, lo: f32, size: f32| if i & bit != 0 { lo + size } else { lo };
            let (p, s) = (bounds.position, bounds.size);
            Vector3::new(pick(1, p.x, s.x), pick(2, p.y, s.y), pick(4, p.z, s.z)).dot(up)
        })
        .collect();
    let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
    let tall = (heights.iter().copied().fold(f32::NEG_INFINITY, f32::max) - low).max(1e-6);
    let mut out = ArrayMesh::new_gd();
    for i in 0..mesh.get_surface_count() {
        let mut arrays = mesh.surface_get_arrays(i);
        let verts: PackedVector3Array = arrays.at(ArrayType::VERTEX.ord() as usize).to();
        let verts = verts.as_slice();
        let index: PackedInt32Array = arrays.at(ArrayType::INDEX.ord() as usize).to();
        let index: Vec<usize> = if index.is_empty() {
            (0..verts.len()).collect()
        } else {
            index.as_slice().iter().map(|&k| k as usize).collect()
        };
        let mut sums: HashMap<(i64, i64, i64), Vector3> = HashMap::new();
        for t in index.chunks(3).filter(|t| t.len() == 3) {
            let (Some(a), Some(b), Some(c)) = (verts.get(t[0]), verts.get(t[1]), verts.get(t[2]))
            else {
                continue;
            };
            // its length is the face's area (twice): big faces weigh more
            let n = (*b - *a).cross(*c - *a);
            for v in [a, b, c] {
                *sums.entry(place(*v)).or_insert(Vector3::ZERO) += n;
            }
        }
        let own: PackedVector3Array = arrays.at(ArrayType::NORMAL.ord() as usize).to();
        let normals: Vec<Vector3> = verts
            .iter()
            .enumerate()
            .map(|(k, v)| {
                let mine = own.get(k).unwrap_or(Vector3::UP);
                let n = sums.get(&place(*v)).copied().unwrap_or(mine);
                if n.length_squared() == 0.0 {
                    return mine;
                }
                // the side the model's own normal faces (whatever the winding)
                let n = n.normalized();
                if n.dot(mine) < 0.0 { -n } else { n }
            })
            .collect();
        let own: PackedColorArray = arrays.at(ArrayType::COLOR.ord() as usize).to();
        let colours: Vec<Color> = verts
            .iter()
            .zip(&normals)
            .enumerate()
            .map(|(k, (v, n))| {
                let h = (v.dot(up) - low) / tall;
                let under = smoothstep(-0.7, 0.4, n.dot(up));
                let ground = smoothstep(0.0, 0.5, h);
                let coat = value_noise(*v * (4.0 / tall));
                let shade = (0.75 + 0.25 * under) * (0.85 + 0.15 * ground) * (0.9 + 0.1 * coat);
                let c = own.get(k).unwrap_or(Color::WHITE);
                Color::from_rgba(c.r * shade, c.g * shade, c.b * shade, c.a)
            })
            .collect();
        arrays.set(
            ArrayType::COLOR.ord() as usize,
            &PackedColorArray::from(colours.as_slice()).to_variant(),
        );
        arrays.set(
            ArrayType::NORMAL.ord() as usize,
            &PackedVector3Array::from(normals.as_slice()).to_variant(),
        );
        // tangents follow the normals; nothing smoothed is normal-mapped
        arrays.set(ArrayType::TANGENT.ord() as usize, &Variant::nil());
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

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth noise in 0..1 with a period of about one unit.
fn value_noise(p: Vector3) -> f32 {
    let hash = |x: i32, y: i32, z: i32| {
        let mut h = (x as u32)
            .wrapping_mul(0x8da6_b343)
            .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
            .wrapping_add((z as u32).wrapping_mul(0xcb1a_b31f));
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    };
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i32, fy as i32, fz as i32);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (tx, ty, tz) = (ease(p.x - fx), ease(p.y - fy), ease(p.z - fz));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |z: i32| {
        let a = lerp(hash(ix, iy, z), hash(ix + 1, iy, z), tx);
        let b = lerp(hash(ix, iy + 1, z), hash(ix + 1, iy + 1, z), tx);
        lerp(a, b, ty)
    };
    lerp(plane(iz), plane(iz + 1), tz)
}

/// The triangles of `mesh` whose corners all `keep` (in the mesh's own
/// rest pose), every vertex array kept as it is.
fn cut(mesh: &Gd<ArrayMesh>, keep: impl Fn(Vector3) -> bool) -> CutBody {
    use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
    let mut out = ArrayMesh::new_gd();
    let mut kept_surfaces = Vec::new();
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
        kept_surfaces.push(i as usize);
    }
    (out, kept_surfaces)
}

/// `materials` multiplied by `colour` and taking `texture` as their albedo
/// (a skin tone, a hair colour): copies, the mesh shared.
fn recoloured(
    materials: &[Option<Gd<Material>>],
    colour: Option<Color>,
    texture: Option<Gd<Texture2D>>,
) -> Vec<Option<Gd<Material>>> {
    materials
        .iter()
        .map(|m| {
            let m = m.as_ref()?;
            if colour.is_none() && texture.is_none() {
                return Some(m.clone());
            }
            let Ok(mut out) = m.duplicate_resource().try_cast::<BaseMaterial3D>() else {
                return Some(m.clone());
            };
            if let Some(c) = colour {
                let albedo = mul(out.get_albedo(), c);
                out.set_albedo(albedo);
            }
            if let Some(t) = &texture {
                out.set_texture(TextureParam::ALBEDO, t);
            }
            Some(out.upcast())
        })
        .collect()
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
