//! The art manifest (`client/godot/art/manifest.json`) and the fallback
//! chain of the spec (section 6.2) that picks what to draw:
//!
//! - a monster: its name, else its class letter, else its body (the
//!   catalog's flags: serpent, flyer, blob, ghost, humanoid...), else a
//!   generic creature; the height follows the catalog size;
//! - an object: its appearance, else its class symbol, else a generic
//!   object. Only `ObjectTile` data (tile, class, appearance) goes in: the
//!   client never knows an object's glyph, and its look never tells more
//!   than its appearance;
//! - a map feature: its `Terrain`, else the generic entry.
//!
//! Every answer is plain data: a model (a scene file or a procedural body),
//! its scale and lift, a tint, a skin and the fallback level used (for the
//! coverage report and a developer's hover). No Godot here.

use std::collections::BTreeMap;
use std::path::Path;

use nh_protocol::{MonsterInfo, ObjectTile, mg};
use nh_world::Terrain;
use serde::Deserialize;

mod held;

pub use held::*;

/// Where the manifest lives, relative to the Godot project.
pub const MANIFEST_PATH: &str = "art/manifest.json";

#[derive(Debug, thiserror::Error)]
pub enum ArtError {
    #[error("cannot read the art manifest: {0}")]
    Io(#[from] std::io::Error),
    #[error("the art manifest is not valid JSON for its schema: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the art manifest is inconsistent: {0}")]
    Invalid(String),
}

/// A body built in code from primitives with the project's materials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proc {
    // creatures
    Serpent,
    Worm,
    Bug,
    Spider,
    Bat,
    Bird,
    Blob,
    Eye,
    Light,
    Vortex,
    Fungus,
    Lizard,
    Dragon,
    Fish,
    Piercer,
    Beast,
    // objects
    Ring,
    Amulet,
    Wand,
    Gem,
    Rock,
    Ball,
    Helm,
    Boots,
    Gloves,
    Garment,
    Cuirass,
    Pole,
    Bow,
    Arrows,
    Fruit,
    Egg,
    Tin,
    Lump,
    Splash,
    Horn,
    Orb,
    Mirror,
    /// Small things worn or carried on the body, by shape: a camera on
    /// its strap, a seer's crystal.
    Carried,
    Heap,
    /// A mace, morning star, flail or club (by its shape).
    Mace,
    /// A sword by its shape: long, broad, great, short, curved, katana.
    Sword,
    /// A wrapped, tied ration.
    Ration,
    Potion,
}

/// Animation names of a model (in its own scene or in its rig's library).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anims {
    #[serde(default)]
    pub idle: Option<String>,
    #[serde(default)]
    pub walk: Option<String>,
    /// A faster gait (running, hurrying on); `walk` when there is none.
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub attack: Option<String>,
    #[serde(default)]
    pub hit: Option<String>,
    #[serde(default)]
    pub death: Option<String>,
}

impl Anims {
    /// The loop to play while moving from cell to cell: `run` when in a
    /// hurry and the model has one, else `walk`; None when the model has
    /// no gait (the map sways it instead).
    pub fn gait(&self, hurry: bool) -> Option<&str> {
        let walk = self.walk.as_deref();
        if hurry {
            self.run.as_deref().or(walk)
        } else {
            walk
        }
    }

    /// Every clip named, for checks.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        [
            &self.idle,
            &self.walk,
            &self.run,
            &self.attack,
            &self.hit,
            &self.death,
        ]
        .into_iter()
        .filter_map(|n| n.as_deref())
    }
}

fn one() -> f32 {
    1.0
}

/// A model: a scene under `client/godot/art` or a procedural body.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSpec {
    /// Scene file relative to `client/godot/art` (glTF, GLB, FBX).
    #[serde(default)]
    pub scene: Option<String>,
    #[serde(default)]
    pub proc: Option<Proc>,
    /// The model's own reference size (height of a creature standing,
    /// length of a thing lying) in its units: scale = target / size.
    pub size: f32,
    /// Raise the model by this much of its own units so it rests on the
    /// ground (feet at 0).
    #[serde(default)]
    pub lift: f32,
    /// Euler angles in degrees applied to the model before the cell's own
    /// turn (a sword lies down, a fallen trunk stands up).
    #[serde(default)]
    pub rot: [f32; 3],
    /// The animation library (see `libraries`) that drives this rig.
    #[serde(default)]
    pub rig: Option<String>,
    /// More libraries on the same skeleton, their clips named
    /// `<library>/<clip>` (the hero's use and gear clips).
    #[serde(default)]
    pub extra_rigs: Vec<String>,
    /// Drop the library's position tracks (a shorter rig keeps its height).
    #[serde(default)]
    pub strip_root: bool,
    #[serde(default)]
    pub anims: Anims,
    /// Monsters: the catalog size's height times this.
    #[serde(default = "one")]
    pub size_factor: f32,
    /// Objects and features: the size in metres when no rule gives one.
    #[serde(default)]
    pub target: Option<f32>,
    /// How much the glyph colour tints it when no rule says (0 = never).
    #[serde(default)]
    pub tint_strength: f32,
    /// The material of a procedural body's skin.
    #[serde(default)]
    pub material: Option<String>,
    /// A head built in code on the rig's `Head` bone, for bodies that come
    /// without one: "male", "female", "bearded" or "hood" (a face in a hood).
    #[serde(default)]
    pub head: Option<String>,
    /// Multiplies the model's own colours ("#rrggbb"): mutes a pack's
    /// bright palette to the project's dark one.
    #[serde(default)]
    pub shade: Option<String>,
    /// A procedural body's variant, by the appearance the rule matched: a
    /// potion's bottle, an amulet's shape, a ring's setting...
    #[serde(default)]
    pub shape: Option<String>,
    /// Meshes of the scene not shown (a dagger's scabbard, an unlit
    /// lamp's flame), on the floor, in the hand and in icons.
    #[serde(default)]
    pub hide: Vec<String>,
    /// The scene's own metal, and how rough it is (they multiply its
    /// textures: a blade reads as steel, its leather grip stays leather).
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default)]
    pub roughness: Option<f32>,
    /// A faint glow ("#rrggbb") over the scene's own colours (runes).
    #[serde(default)]
    pub glow: Option<String>,
    #[serde(default)]
    pub glow_energy: f32,
    /// Shade the scene's meshes smooth (a low-poly pack's flat facets).
    #[serde(default)]
    pub smooth: bool,
    /// Multiplies the meshes whose names end so ("_Arms": "#c89070"):
    /// sleeves the colour of bare skin; "=#rrggbb" dyes them that colour
    /// outright (a white robe: a product only darkens). A key "@Name" takes
    /// the surfaces whose material is so named instead: one mesh of several
    /// materials (a bull's coat, not its horns and eyes).
    #[serde(default)]
    pub recolor: BTreeMap<String, String>,
    /// Things worn on the rig's bones: a hat, a winged helm, a cape.
    #[serde(default)]
    pub extras: Vec<Extra>,
    /// Bare arms: those of this head's base character (its skin), worn
    /// instead of the outfit's sleeves (hide those).
    #[serde(default)]
    pub bare_arms: Option<String>,
    /// With `bare_arms`, the chest bare too (hide the outfit's body).
    #[serde(default)]
    pub bare_chest: bool,
}

/// A model worn on a bone of a character (a role's hat or cape), hidden
/// while something is worn in its `slot` ("helmet", "cloak", "shield").
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extra {
    pub model: String,
    pub bone: String,
    /// Metres and degrees in the bone's space, and the model's size.
    #[serde(default)]
    pub pos: [f32; 3],
    #[serde(default)]
    pub rot: [f32; 3],
    pub size: f32,
    #[serde(default)]
    pub tint: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
}

impl Extra {
    pub fn tint_rgb(&self) -> Option<[f32; 3]> {
        self.tint.as_deref().and_then(hex)
    }
}

/// How a monster sits for its portrait (an achievement's medallion): which
/// of its variants, and what it wears or carries there besides its own
/// look (a role's hat, a weapon over the shoulder), so that each reads at
/// 64 px. The map never draws these.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortraitSpec {
    #[serde(default)]
    pub female: bool,
    #[serde(default)]
    pub extras: Vec<Extra>,
    /// Other medallions of the same monster, by name: their extras
    /// instead.
    #[serde(default)]
    pub variants: BTreeMap<String, Vec<Extra>>,
}

impl PortraitSpec {
    /// The extras of `variant` (None: the portrait's own).
    pub fn extras_of(&self, variant: Option<&str>) -> Option<&[Extra]> {
        match variant {
            None => Some(&self.extras),
            Some(v) => self.variants.get(v).map(Vec::as_slice),
        }
    }
}

impl ModelSpec {
    /// Whether the scene's own materials are changed beyond a tint.
    pub fn refinishes(&self) -> bool {
        self.metallic.is_some() || self.roughness.is_some() || self.glow.is_some()
    }

    pub fn glow_rgb(&self) -> Option<[f32; 3]> {
        self.glow.as_deref().and_then(hex)
    }

    pub fn shade_rgb(&self) -> [f32; 3] {
        self.shade
            .as_deref()
            .and_then(hex)
            .unwrap_or([1.0, 1.0, 1.0])
    }
}

/// A head cut from a base character at the neck and worn on a rig's
/// skeleton: its face (texture, normal and roughness maps), eyes and
/// eyebrows, hair and beard on the Head bone.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadSpec {
    /// The base character's scene (relative to `client/godot/art`).
    pub base: String,
    /// Keep what lies above this height of the base's own rest pose.
    pub cut: f32,
    /// Another albedo texture for the skin (a lighter or darker one).
    #[serde(default)]
    pub skin: Option<String>,
    /// Multiplies the skin ("#rrggbb"): an elf's pallor, an orc's green.
    #[serde(default)]
    pub tint: Option<String>,
    /// Hair, beard and eyebrows: scenes skinned to the Head bone.
    #[serde(default)]
    pub hair: Vec<String>,
    /// Multiplies the hair ("#rrggbb").
    #[serde(default)]
    pub hair_color: Option<String>,
}

/// A PBR material: `<textures>_albedo.jpg`, `_normal.jpg` and `_arm.jpg`
/// (ambient occlusion, roughness, metallic in R, G, B), or a plain colour.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialSpec {
    #[serde(default)]
    pub textures: Option<String>,
    /// Albedo colour ("#rrggbb"), multiplied with the texture.
    #[serde(default)]
    pub color: Option<String>,
    /// World-space texture repeats per metre (triplanar).
    #[serde(default = "one")]
    pub uv_scale: f32,
    #[serde(default)]
    pub roughness: Option<f32>,
    #[serde(default)]
    pub metallic: Option<f32>,
    #[serde(default = "one")]
    pub normal_scale: f32,
    #[serde(default)]
    pub emission: Option<String>,
    #[serde(default)]
    pub emission_energy: f32,
    /// Below 1: see-through.
    #[serde(default)]
    pub alpha: Option<f32>,
}

impl MaterialSpec {
    pub fn albedo_path(&self) -> Option<String> {
        self.textures.as_ref().map(|t| format!("{t}_albedo.jpg"))
    }

    pub fn normal_path(&self) -> Option<String> {
        self.textures.as_ref().map(|t| format!("{t}_normal.jpg"))
    }

    pub fn arm_path(&self) -> Option<String> {
        self.textures.as_ref().map(|t| format!("{t}_arm.jpg"))
    }

    pub fn albedo(&self) -> [f32; 3] {
        self.color
            .as_deref()
            .and_then(hex)
            .unwrap_or([1.0, 1.0, 1.0])
    }

    pub fn emission_color(&self) -> Option<[f32; 3]> {
        self.emission.as_deref().and_then(hex)
    }
}

/// How a monster or object rule looks; missing fields come from the next,
/// more general rule of the chain.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtRule {
    #[serde(default)]
    pub model: Option<String>,
    /// Monsters: the model for a female (MG_FEMALE).
    #[serde(default)]
    pub female: Option<String>,
    /// Objects: alternatives picked by the appearance (stable).
    #[serde(default)]
    pub variants: Vec<String>,
    /// Target size in metres (monsters: overrides the catalog size).
    #[serde(default)]
    pub size: Option<f32>,
    /// Multiplies the target size.
    #[serde(default)]
    pub scale: Option<f32>,
    /// A fixed tint ("#rrggbb"); without it the glyph colour tints.
    #[serde(default)]
    pub tint: Option<String>,
    #[serde(default)]
    pub tint_strength: Option<f32>,
    /// "translucent", or a material name that replaces the model's own.
    #[serde(default)]
    pub skin: Option<String>,
}

/// A monster rule by body flags: every flag of `all`, none of `none`, and
/// a size among `sizes` (any when empty).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyRule {
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub none: Vec<String>,
    #[serde(default)]
    pub sizes: Vec<String>,
    pub art: ArtRule,
}

impl BodyRule {
    fn matches(&self, m: &MonsterInfo) -> bool {
        let has = |f: &String| m.body.iter().any(|b| b == f);
        self.all.iter().all(has)
            && !self.none.iter().any(has)
            && (self.sizes.is_empty() || self.sizes.contains(&m.size))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterRules {
    /// Height in metres of a creature of each catalog size.
    pub sizes: BTreeMap<String, f32>,
    #[serde(default)]
    pub names: BTreeMap<String, ArtRule>,
    #[serde(default)]
    pub classes: BTreeMap<String, ArtRule>,
    #[serde(default)]
    pub bodies: Vec<BodyRule>,
    pub generic: ArtRule,
}

/// An object rule by appearance: the class (any when absent) and either an
/// exact appearance or whole words in it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceRule {
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub exact: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
    pub art: ArtRule,
}

impl AppearanceRule {
    fn matches(&self, t: &ObjectTile) -> bool {
        appearance_matches(self.class.as_deref(), &self.exact, &self.words, t)
    }
}

/// The class (any when None) and an exact appearance or whole words in it.
fn appearance_matches(
    class: Option<&str>,
    exact: &[String],
    words: &[String],
    t: &ObjectTile,
) -> bool {
    if class.is_some_and(|c| c != t.class) {
        return false;
    }
    let a = t.appearance.to_lowercase();
    exact.iter().any(|e| a == e.to_lowercase())
        || words.iter().any(|w| has_words(&a, &w.to_lowercase()))
}

/// `phrase` (one or more words) occurs in `text` on word boundaries.
fn has_words<'a>(text: &'a str, phrase: &'a str) -> bool {
    let split = |s: &'a str| -> Vec<&'a str> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (words, want) = (split(text), split(phrase));
    !want.is_empty() && words.windows(want.len()).any(|w| w == want.as_slice())
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRules {
    #[serde(default)]
    pub appearances: Vec<AppearanceRule>,
    #[serde(default)]
    pub classes: BTreeMap<String, ArtRule>,
    pub generic: ArtRule,
}

/// A map feature: its surface material, a second one for trim (door
/// bands), and a model standing on it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSpec {
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub trim: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatueSpec {
    /// The stone of a statue made from a monster's model.
    pub material: String,
    /// A statue of nothing known.
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    version: u32,
    #[serde(default)]
    libraries: BTreeMap<String, String>,
    materials: BTreeMap<String, MaterialSpec>,
    models: BTreeMap<String, ModelSpec>,
    monsters: MonsterRules,
    objects: ObjectRules,
    terrain: BTreeMap<String, TerrainSpec>,
    statue: StatueSpec,
    #[serde(default)]
    held: HeldRules,
    #[serde(default)]
    heads: BTreeMap<String, HeadSpec>,
    /// By monster name.
    #[serde(default)]
    portraits: BTreeMap<String, PortraitSpec>,
}

/// How specific the art found is (most specific first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// A monster's own name.
    Name,
    /// An object's appearance.
    Appearance,
    /// A feature's own terrain.
    Terrain,
    /// The class letter or symbol.
    Class,
    /// The monster's body flags.
    Body,
    Generic,
}

/// How the model is coloured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tint {
    None,
    /// By the glyph's colour, this strongly (0..1).
    Glyph(f32),
    /// By a fixed colour.
    Rgb([f32; 3], f32),
}

/// What covers the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Skin {
    /// Its own materials (tinted).
    Own,
    /// See-through (ghosts, wraiths).
    Translucent,
    /// This material everywhere (statues, golems).
    Material(usize),
}

/// The art chosen for one thing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved {
    /// Index into `ArtManifest::model_at`.
    pub model: usize,
    /// Uniform scale of the model.
    pub scale: f32,
    /// Metres to raise it so it rests on the ground.
    pub lift: f32,
    /// The model's own turn (degrees).
    pub rot: [f32; 3],
    /// Its size in metres once scaled (height of a creature).
    pub height: f32,
    pub tint: Tint,
    pub skin: Skin,
    pub level: Level,
}

/// A map feature's art.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainArt {
    pub material: Option<usize>,
    pub trim: Option<usize>,
    pub model: Option<Resolved>,
    pub level: Level,
}

/// The parsed, checked manifest.
#[derive(Debug, Clone)]
pub struct ArtManifest {
    libraries: BTreeMap<String, String>,
    materials: Vec<(String, MaterialSpec)>,
    models: Vec<(String, ModelSpec)>,
    monsters: MonsterRules,
    objects: ObjectRules,
    terrain: BTreeMap<String, TerrainSpec>,
    statue_material: usize,
    statue_model: usize,
    held: HeldRules,
    held_models: Vec<(String, HeldSpec)>,
    heads: BTreeMap<String, HeadSpec>,
    portraits: BTreeMap<String, PortraitSpec>,
}

/// "#rrggbb" as linear-ish 0..1 components (the client treats them as sRGB).
pub fn hex(s: &str) -> Option<[f32; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let c = |i: usize| {
        u8::from_str_radix(s.get(i..i + 2)?, 16)
            .ok()
            .map(|v| f32::from(v) / 255.0)
    };
    Some([c(0)?, c(2)?, c(4)?])
}

/// FNV-1a: a stable pick among variants.
fn stable_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl ArtManifest {
    pub fn parse(json: &str) -> Result<ArtManifest, ArtError> {
        let raw: RawManifest = serde_json::from_str(json)?;
        if raw.version != 1 {
            return Err(ArtError::Invalid(format!("version {}", raw.version)));
        }
        let materials: Vec<_> = raw.materials.into_iter().collect();
        let models: Vec<_> = raw.models.into_iter().collect();
        let mat = |name: &str| materials.iter().position(|(n, _)| n == name);
        let model = |name: &str| models.iter().position(|(n, _)| n == name);
        let mut errors = Vec::new();
        for (name, m) in &models {
            if m.scene.is_some() == m.proc.is_some() {
                errors.push(format!("model {name}: exactly one of scene and proc"));
            }
            if m.size <= 0.0 {
                errors.push(format!("model {name}: size {}", m.size));
            }
            if let Some(rig) = &m.rig
                && !raw.libraries.contains_key(rig)
            {
                errors.push(format!("model {name}: no library {rig}"));
            }
            if let Some(mm) = &m.material
                && mat(mm).is_none()
            {
                errors.push(format!("model {name}: no material {mm}"));
            }
            if let Some(h) = &m.head
                && !matches!(h.as_str(), "male" | "female" | "bearded" | "hood")
                && !raw.heads.contains_key(h)
            {
                errors.push(format!("model {name}: head {h}"));
            }
            if let Some(sh) = &m.shade
                && hex(sh).is_none()
            {
                errors.push(format!("model {name}: shade {sh}"));
            }
            if let Some(g) = &m.glow
                && hex(g).is_none()
            {
                errors.push(format!("model {name}: glow {g}"));
            }
            for c in m.recolor.values() {
                if hex(c.trim_start_matches('=')).is_none() {
                    errors.push(format!("model {name}: recolour {c}"));
                }
            }
            if let Some(h) = &m.bare_arms
                && !raw.heads.contains_key(h)
            {
                errors.push(format!("model {name}: bare arms of no head {h}"));
            }
            if m.bare_chest && m.bare_arms.is_none() {
                errors.push(format!("model {name}: a bare chest without bare arms"));
            }
            for e in &m.extras {
                if model(&e.model).is_none() {
                    errors.push(format!("model {name}: no extra model {}", e.model));
                }
                if let Some(t) = &e.tint
                    && hex(t).is_none()
                {
                    errors.push(format!("model {name}: extra tint {t}"));
                }
            }
        }
        let rule_errors = |what: &str, r: &ArtRule| {
            let mut errors = Vec::new();
            for m in r.model.iter().chain(&r.female).chain(&r.variants) {
                if model(m).is_none() {
                    errors.push(format!("{what}: no model {m}"));
                }
            }
            if let Some(s) = &r.skin
                && s != "translucent"
                && mat(s).is_none()
            {
                errors.push(format!("{what}: no skin material {s}"));
            }
            if let Some(t) = &r.tint
                && hex(t).is_none()
            {
                errors.push(format!("{what}: tint {t}"));
            }
            errors
        };
        let mut check_rule = |what: &str, r: &ArtRule| errors.extend(rule_errors(what, r));
        for (n, r) in &raw.monsters.names {
            check_rule(&format!("monster {n}"), r);
        }
        for (n, r) in &raw.monsters.classes {
            check_rule(&format!("class {n}"), r);
        }
        for (i, b) in raw.monsters.bodies.iter().enumerate() {
            check_rule(&format!("body rule {i}"), &b.art);
        }
        check_rule("generic monster", &raw.monsters.generic);
        for (i, a) in raw.objects.appearances.iter().enumerate() {
            check_rule(&format!("appearance rule {i}"), &a.art);
        }
        for (n, r) in &raw.objects.classes {
            check_rule(&format!("object class {n}"), r);
        }
        check_rule("generic object", &raw.objects.generic);
        if raw.monsters.generic.model.is_none() {
            errors.push("the generic monster has no model".into());
        }
        if raw.objects.generic.model.is_none() && raw.objects.generic.variants.is_empty() {
            errors.push("the generic object has no model".into());
        }
        for size in ["tiny", "small", "medium", "large", "huge", "gigantic"] {
            if !raw.monsters.sizes.contains_key(size) {
                errors.push(format!("no height for size {size}"));
            }
        }
        for (n, t) in &raw.terrain {
            let known = n == "generic" || Terrain::ALL.iter().any(|t| format!("{t:?}") == *n);
            if !known {
                errors.push(format!("terrain {n}: no such terrain"));
            }
            for m in t.material.iter().chain(&t.trim) {
                if mat(m).is_none() {
                    errors.push(format!("terrain {n}: no material {m}"));
                }
            }
            if let Some(m) = &t.model
                && model(m).is_none()
            {
                errors.push(format!("terrain {n}: no model {m}"));
            }
        }
        if !raw.terrain.contains_key("generic") {
            errors.push("no generic terrain".into());
        }
        let statue_material = mat(&raw.statue.material);
        let statue_model = model(&raw.statue.model);
        if statue_material.is_none() || statue_model.is_none() {
            errors.push("the statue's material or model is missing".into());
        }
        for (name, h) in &raw.heads {
            for c in h.tint.iter().chain(&h.hair_color) {
                if hex(c).is_none() {
                    errors.push(format!("head {name}: colour {c}"));
                }
            }
        }
        errors.extend(
            raw.held
                .check(|m| model(m).is_some(), |l| raw.libraries.contains_key(l)),
        );
        for (name, p) in &raw.portraits {
            for e in p.extras.iter().chain(p.variants.values().flatten()) {
                if model(&e.model).is_none() {
                    errors.push(format!("portrait {name}: no extra model {}", e.model));
                }
                if let Some(t) = &e.tint
                    && hex(t).is_none()
                {
                    errors.push(format!("portrait {name}: extra tint {t}"));
                }
            }
        }
        for (name, m) in &models {
            for rig in &m.extra_rigs {
                if !raw.libraries.contains_key(rig) {
                    errors.push(format!("model {name}: no library {rig}"));
                }
            }
        }
        if !errors.is_empty() {
            return Err(ArtError::Invalid(errors.join("; ")));
        }
        Ok(ArtManifest {
            libraries: raw.libraries,
            materials,
            models,
            monsters: raw.monsters,
            objects: raw.objects,
            terrain: raw.terrain,
            statue_material: statue_material.unwrap_or_default(),
            statue_model: statue_model.unwrap_or_default(),
            held_models: raw.held.models.clone().into_iter().collect(),
            heads: raw.heads,
            held: raw.held,
            portraits: raw.portraits,
        })
    }

    pub fn load(path: &Path) -> Result<ArtManifest, ArtError> {
        ArtManifest::parse(&std::fs::read_to_string(path)?)
    }

    pub fn model_at(&self, i: usize) -> (&str, &ModelSpec) {
        let (n, m) = &self.models[i];
        (n, m)
    }

    pub fn models(&self) -> impl Iterator<Item = (usize, &str, &ModelSpec)> {
        self.models
            .iter()
            .enumerate()
            .map(|(i, (n, m))| (i, n.as_str(), m))
    }

    pub fn model_index(&self, name: &str) -> Option<usize> {
        self.models.iter().position(|(n, _)| n == name)
    }

    pub fn material_at(&self, i: usize) -> (&str, &MaterialSpec) {
        let (n, m) = &self.materials[i];
        (n, m)
    }

    pub fn material_index(&self, name: &str) -> Option<usize> {
        self.materials.iter().position(|(n, _)| n == name)
    }

    pub fn materials(&self) -> impl Iterator<Item = (usize, &str, &MaterialSpec)> {
        self.materials
            .iter()
            .enumerate()
            .map(|(i, (n, m))| (i, n.as_str(), m))
    }

    /// The scene file of an animation library.
    pub fn library(&self, name: &str) -> Option<&str> {
        self.libraries.get(name).map(String::as_str)
    }

    /// Every file the manifest names, relative to `client/godot/art`.
    pub fn files(&self) -> Vec<String> {
        let mut out: Vec<String> = self.libraries.values().cloned().collect();
        out.extend(self.models.iter().filter_map(|(_, m)| m.scene.clone()));
        for h in self.heads.values() {
            out.push(h.base.clone());
            out.extend(h.skin.clone());
            out.extend(h.hair.iter().cloned());
        }
        for (_, m) in &self.materials {
            out.extend(m.albedo_path());
            out.extend(m.normal_path());
            out.extend(m.arm_path());
        }
        out.sort();
        out.dedup();
        out
    }

    fn placed(&self, model: usize, target: f32, level: Level) -> Resolved {
        let spec = &self.models[model].1;
        let scale = target / spec.size;
        Resolved {
            model,
            scale,
            lift: spec.lift * scale,
            rot: spec.rot,
            height: target,
            tint: if spec.tint_strength > 0.0 {
                Tint::Glyph(spec.tint_strength)
            } else {
                Tint::None
            },
            skin: Skin::Own,
            level,
        }
    }

    /// Tint and skin from the most specific rules that set them.
    fn apply(&self, chain: &[&ArtRule], mut r: Resolved) -> Resolved {
        let first = |f: &dyn Fn(&ArtRule) -> Option<f32>| chain.iter().find_map(|r| f(r));
        if let Some(t) = chain.iter().find_map(|r| r.tint.as_deref()).and_then(hex) {
            let strength = first(&|r| r.tint_strength).unwrap_or(0.5);
            r.tint = Tint::Rgb(t, strength);
        } else if let Some(s) = first(&|r| r.tint_strength) {
            r.tint = if s > 0.0 { Tint::Glyph(s) } else { Tint::None };
        }
        if let Some(skin) = chain.iter().find_map(|r| r.skin.as_deref()) {
            r.skin = match self.material_index(skin) {
                Some(i) => Skin::Material(i),
                None => Skin::Translucent,
            };
        }
        r
    }

    /// The rules of a monster from the most specific, with their levels.
    fn monster_chain<'a>(&'a self, m: &MonsterInfo) -> Vec<(&'a ArtRule, Level)> {
        let rules = &self.monsters;
        let mut chain = Vec::new();
        if let Some(r) = rules.names.get(&m.name) {
            chain.push((r, Level::Name));
        }
        if let Some(r) = rules.classes.get(&m.class) {
            chain.push((r, Level::Class));
        }
        if let Some(b) = rules.bodies.iter().find(|b| b.matches(m)) {
            chain.push((&b.art, Level::Body));
        }
        chain.push((&rules.generic, Level::Generic));
        chain
    }

    /// A monster's art; `flags` are the glyph's (MG_FEMALE picks a
    /// female model where a rule has one).
    pub fn monster(&self, m: &MonsterInfo, flags: u32) -> Resolved {
        let chain = self.monster_chain(m);
        let level = chain[0].1;
        let rules: Vec<&ArtRule> = chain.iter().map(|(r, _)| *r).collect();
        let female = flags & mg::FEMALE != 0;
        // the model of the most specific rule with one (its female
        // variant for a female); every other field from the most specific
        // rule of the whole chain that sets it
        let name = rules.iter().find_map(|r| {
            r.model.as_ref()?;
            if female { r.female.as_ref() } else { None }.or(r.model.as_ref())
        });
        let model = name.and_then(|n| self.model_index(n)).unwrap_or_default();
        let spec = &self.models[model].1;
        let height = match rules.iter().find_map(|r| r.size) {
            Some(h) => h,
            None => {
                let base = self.monsters.sizes.get(&m.size).copied().unwrap_or(1.0);
                base * spec.size_factor
            }
        };
        let height = height * rules.iter().find_map(|r| r.scale).unwrap_or(1.0);
        let placed = self.placed(model, height, level);
        self.apply(&rules, placed)
    }

    /// A statue: the monster's model in stone, or the statue model for a
    /// monster not known.
    pub fn statue(&self, m: Option<&MonsterInfo>, flags: u32) -> Resolved {
        match m {
            Some(m) => Resolved {
                skin: Skin::Material(self.statue_material),
                tint: Tint::None,
                ..self.monster(m, flags)
            },
            None => {
                let spec = &self.models[self.statue_model].1;
                self.placed(
                    self.statue_model,
                    spec.target.unwrap_or(1.0),
                    Level::Generic,
                )
            }
        }
    }

    /// An object's art from its appearance tile. It takes no glyph on
    /// purpose: the look never reveals more than the appearance.
    pub fn object(&self, tile: &ObjectTile) -> Resolved {
        let rules = &self.objects;
        let mut chain: Vec<(&ArtRule, Level)> = Vec::new();
        if let Some(a) = rules.appearances.iter().find(|a| a.matches(tile)) {
            chain.push((&a.art, Level::Appearance));
        }
        if let Some(r) = rules.classes.get(&tile.class) {
            chain.push((r, Level::Class));
        }
        chain.push((&rules.generic, Level::Generic));
        let level = chain[0].1;
        let has_model = |r: &ArtRule| r.model.is_some() || !r.variants.is_empty();
        let rules: Vec<&ArtRule> = chain.iter().map(|(r, _)| *r).collect();
        let pick = rules.iter().find(|r| has_model(r)).map(|r| {
            if r.variants.is_empty() {
                r.model.clone().unwrap_or_default()
            } else {
                let i = stable_hash(&tile.appearance) as usize % r.variants.len();
                r.variants[i].clone()
            }
        });
        let model = pick.and_then(|n| self.model_index(&n)).unwrap_or_default();
        let spec = &self.models[model].1;
        let size = rules
            .iter()
            .find_map(|r| r.size)
            .or(spec.target)
            .unwrap_or(0.3);
        let size = size * rules.iter().find_map(|r| r.scale).unwrap_or(1.0);
        let placed = self.placed(model, size, level);
        self.apply(&rules, placed)
    }

    /// A map feature's art: its own entry, else the generic one.
    pub fn terrain(&self, t: Terrain) -> TerrainArt {
        let (spec, level) = match self.terrain.get(&format!("{t:?}")) {
            Some(s) => (s, Level::Terrain),
            None => (&self.terrain["generic"], Level::Generic),
        };
        let model = spec.model.as_deref().and_then(|n| self.model_index(n));
        TerrainArt {
            material: spec
                .material
                .as_deref()
                .and_then(|m| self.material_index(m)),
            trim: spec.trim.as_deref().and_then(|m| self.material_index(m)),
            model: model.map(|i| {
                let size = spec.size.or(self.models[i].1.target).unwrap_or(1.0);
                self.placed(i, size, level)
            }),
            level,
        }
    }

    /// Draw a model as a procedural body from now on (its scene file is
    /// missing or does not load); its size in the world stays the same.
    pub fn replace_with_proc(&mut self, model: usize, proc: Proc) {
        let skin = self.material_index("skin").map(|_| "skin".to_string());
        let spec = &mut self.models[model].1;
        *spec = ModelSpec {
            scene: None,
            proc: Some(proc),
            size: 1.0,
            lift: 0.0,
            rot: [0.0; 3],
            rig: None,
            extra_rigs: Vec::new(),
            strip_root: false,
            anims: Anims::default(),
            size_factor: spec.size_factor,
            target: spec.target,
            tint_strength: spec.tint_strength.max(0.5),
            material: skin,
            head: None,
            shade: None,
            shape: None,
            hide: Vec::new(),
            metallic: None,
            roughness: None,
            glow: None,
            glow_energy: 0.0,
            smooth: false,
            recolor: BTreeMap::new(),
            extras: Vec::new(),
            bare_arms: None,
            bare_chest: false,
        };
    }

    /// A head built from a base character's (see `HeadSpec`), by name.
    pub fn head(&self, name: &str) -> Option<&HeadSpec> {
        self.heads.get(name)
    }

    /// How a monster sits for its portrait, by its name.
    pub fn portrait(&self, monster: &str) -> Option<&PortraitSpec> {
        self.portraits.get(monster)
    }

    pub fn portraits(&self) -> impl Iterator<Item = (&str, &PortraitSpec)> {
        self.portraits.iter().map(|(n, p)| (n.as_str(), p))
    }

    /// A material by name (the renderer's own surfaces: water, lava...).
    pub fn material(&self, name: &str) -> Option<usize> {
        self.material_index(name)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use nh_protocol::{Catalog, EngineMsg, parse_line};

    use super::*;

    fn art_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot/art")
    }

    fn manifest() -> ArtManifest {
        ArtManifest::load(&art_dir().join("manifest.json")).unwrap()
    }

    fn catalog() -> Catalog {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    fn exists(art: &ArtManifest, r: &Resolved) -> bool {
        let (_, spec) = art.model_at(r.model);
        match &spec.scene {
            Some(s) => art_dir().join(s).is_file(),
            None => spec.proc.is_some(),
        }
    }

    #[test]
    fn every_file_the_manifest_names_exists() {
        let art = manifest();
        let missing: Vec<String> = art
            .files()
            .into_iter()
            .filter(|f| !art_dir().join(f).is_file())
            .collect();
        assert!(missing.is_empty(), "missing: {missing:?}");
    }

    #[test]
    fn every_monster_resolves_to_a_model() {
        let (art, cat) = (manifest(), catalog());
        for m in &cat.monsters {
            for flags in [0, mg::MALE, mg::FEMALE] {
                let r = art.monster(m, flags);
                assert!(exists(&art, &r), "{}", m.name);
                assert!(r.scale > 0.0 && r.scale.is_finite(), "{}", m.name);
                assert!(
                    (0.08..=3.0).contains(&r.height),
                    "{} is {} m tall",
                    m.name,
                    r.height
                );
            }
        }
    }

    #[test]
    fn every_object_tile_resolves_to_a_model() {
        let (art, cat) = (manifest(), catalog());
        for t in &cat.object_tiles {
            let r = art.object(t);
            assert!(exists(&art, &r), "{t:?}");
            assert!((0.02..=2.0).contains(&r.height), "{t:?}: {}", r.height);
        }
    }

    #[test]
    fn every_terrain_has_art() {
        let art = manifest();
        for t in Terrain::ALL {
            let a = art.terrain(t);
            if let Some(m) = a.model {
                assert!(exists(&art, &m), "{t:?}");
            }
            if !matches!(t, Terrain::Stone | Terrain::Effect | Terrain::Unknown) {
                assert_eq!(a.level, Level::Terrain, "{t:?} has its own entry");
            }
        }
    }

    /// The object function sees only the appearance tile: its signature
    /// takes no glyph (a glyph number would reveal the true type).
    #[test]
    fn the_object_function_takes_no_glyph() {
        let f: fn(&ArtManifest, &ObjectTile) -> Resolved = ArtManifest::object;
        let (art, cat) = (manifest(), catalog());
        let t = &cat.object_tiles[0];
        assert_eq!(f(&art, t), art.object(t));
        // the same appearance in the same class looks the same
        let twin = ObjectTile {
            tile: t.tile + 1000,
            ..t.clone()
        };
        assert_eq!(art.object(&twin), art.object(t));
    }

    /// The catalog gives each appearance the colour the map shows it in
    /// (the icon bake tints by it).
    #[test]
    fn every_object_tile_has_its_colour() {
        let cat = catalog();
        let ruby = |class: &str, a: &str| {
            let t = cat
                .object_tiles
                .iter()
                .find(|t| t.class == class && t.appearance == a);
            t.and_then(|t| t.color)
        };
        assert!(cat.object_tiles.iter().all(|t| t.color.is_some()));
        assert_eq!(ruby("!", "ruby"), Some(1));
        assert_eq!(ruby("*", "blue"), Some(4));
    }

    #[test]
    fn statues_are_stone_and_corpses_keep_the_monster() {
        let (art, cat) = (manifest(), catalog());
        let dog = cat.monsters.iter().find(|m| m.name == "dog").unwrap();
        let s = art.statue(Some(dog), 0);
        assert!(matches!(s.skin, Skin::Material(_)));
        assert_eq!(s.model, art.monster(dog, 0).model);
        let unknown = art.statue(None, 0);
        assert!(exists(&art, &unknown));
    }

    #[test]
    fn species_of_a_class_are_told_apart() {
        let (art, cat) = (manifest(), catalog());
        let m = |n: &str| art.monster(cat.monsters.iter().find(|m| m.name == n).unwrap(), 0);
        // the same model, but bigger and coloured differently
        let (jackal, wolf) = (m("jackal"), m("wolf"));
        assert_eq!(jackal.model, wolf.model);
        assert!(wolf.height > jackal.height);
        assert_ne!(jackal.tint, wolf.tint);
        // a female of a class with women gets a woman's model
        let human = cat.monsters.iter().find(|m| m.name == "human").unwrap();
        assert_ne!(
            art.monster(human, mg::FEMALE).model,
            art.monster(human, 0).model
        );
        // words match whole: a monkey is not a key
        assert!(has_words("skeleton key", "key") && !has_words("monkey", "key"));
        assert!(has_words("large box", "large box") && !has_words("box", "large box"));
    }

    /// How many monsters and object tiles resolve at each level of the
    /// fallback chain (printed; run with --nocapture).
    /// The animation names a scene file holds, as Godot names them: glTF
    /// clips lose a `_Loop` suffix on import; FBX clips are searched for as
    /// they are (None: the file is not glTF, only a byte search is possible).
    fn clips_in(path: &std::path::Path) -> Option<Vec<String>> {
        let bytes = std::fs::read(path).unwrap();
        let json: serde_json::Value = match path.extension()?.to_str()? {
            "glb" => {
                let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
                serde_json::from_slice(&bytes[20..20 + len]).unwrap()
            }
            "gltf" => serde_json::from_slice(&bytes).unwrap(),
            _ => return None,
        };
        let names = json["animations"].as_array().cloned().unwrap_or_default();
        Some(
            names
                .iter()
                .filter_map(|a| a["name"].as_str())
                .map(|n| n.strip_suffix("_Loop").unwrap_or(n).to_string())
                .collect(),
        )
    }

    #[test]
    fn every_clip_the_manifest_names_is_in_its_file() {
        let art = manifest();
        for (_, name, spec) in art.models() {
            let file = match (&spec.rig, &spec.scene) {
                (Some(rig), _) => art.library(rig).unwrap().to_string(),
                (None, Some(scene)) => scene.clone(),
                (None, None) => continue,
            };
            let path = art_dir().join(&file);
            let clips = clips_in(&path);
            let bytes = std::fs::read(&path).unwrap();
            for clip in spec.anims.names() {
                let found = match &clips {
                    Some(c) => c.iter().any(|c| c == clip),
                    None => bytes.windows(clip.len()).any(|w| w == clip.as_bytes()),
                };
                assert!(found, "model {name}: no clip {clip} in {file}");
            }
        }
    }

    #[test]
    fn walkers_walk_and_some_run() {
        let art = manifest();
        let anims = |n: &str| {
            let (_, _, spec) = art.models().find(|(_, name, _)| *name == n).unwrap();
            spec.anims.clone()
        };
        let human = anims("human_male");
        assert_eq!(human.gait(false), Some("Walk"));
        assert_eq!(human.gait(true), Some("Jog_Fwd"));
        assert_eq!(anims("imp").gait(true), Some("Jog_Fwd"));
        assert_eq!(anims("horse").gait(true), Some("Gallop"));
        assert_eq!(anims("horse").gait(false), Some("Walk"));
        // no gait: swayed by the map
        assert_eq!(anims("dog").gait(true), None);
        assert_eq!(anims("rat").gait(false), None);
        assert_eq!(anims("blob").gait(false), None);
        // a walk without a run walks in a hurry too
        let only_walk = Anims {
            walk: Some("Walk".into()),
            ..Anims::default()
        };
        assert_eq!(only_walk.gait(true), Some("Walk"));
        // every rigged character (people, imps, puglins) has a walk
        for (_, name, spec) in art.models() {
            if spec.rig.is_some() {
                assert!(spec.anims.walk.is_some(), "{name}");
            }
        }
    }

    #[test]
    fn coverage_report() {
        let (art, cat) = (manifest(), catalog());
        let mut monsters: BTreeMap<Level, Vec<&str>> = BTreeMap::new();
        for m in &cat.monsters {
            monsters
                .entry(art.monster(m, 0).level)
                .or_default()
                .push(&m.name);
        }
        let mut objects: BTreeMap<Level, usize> = BTreeMap::new();
        for t in &cat.object_tiles {
            *objects.entry(art.object(t).level).or_default() += 1;
        }
        let mut models: BTreeMap<&str, usize> = BTreeMap::new();
        for m in &cat.monsters {
            *models
                .entry(art.model_at(art.monster(m, 0).model).0)
                .or_default() += 1;
        }
        println!("art coverage: {} monsters", cat.monsters.len());
        for (level, names) in &monsters {
            println!("  {level:?}: {}", names.len());
            if *level >= Level::Body {
                println!("    {}", names.join(", "));
            }
        }
        println!("  by model: {models:?}");
        println!("art coverage: {} object tiles", cat.object_tiles.len());
        for (level, n) in &objects {
            println!("  {level:?}: {n}");
        }
        assert!(!monsters.contains_key(&Level::Generic), "{monsters:?}");
    }

    #[test]
    fn a_missing_scene_becomes_a_procedural_body_of_the_same_size() {
        let (mut art, cat) = (manifest(), catalog());
        let dog = cat.monsters.iter().find(|m| m.name == "jackal").unwrap();
        let before = art.monster(dog, 0);
        art.replace_with_proc(before.model, Proc::Blob);
        let after = art.monster(dog, 0);
        assert_eq!(after.height, before.height);
        assert_eq!(art.model_at(after.model).1.proc, Some(Proc::Blob));
    }

    fn tile<'a>(cat: &'a Catalog, class: &str, appearance: &str) -> &'a ObjectTile {
        cat.object_tiles
            .iter()
            .find(|t| t.class == class && t.appearance == appearance)
            .unwrap_or_else(|| panic!("no {class} {appearance}"))
    }

    fn held_name(art: &ArtManifest, h: Option<HeldArt>) -> Option<&str> {
        h.map(|h| art.held_at(h.held).0)
    }

    /// Every weapon, every shield and every light source of the catalog
    /// is held as something, by its appearance: and each resolves to a
    /// model that exists.
    #[test]
    fn every_weapon_shield_and_light_is_held_as_something() {
        let (art, cat) = (manifest(), catalog());
        let mut generic = Vec::new();
        for t in &cat.object_tiles {
            let wanted = match t.class.as_str() {
                ")" => true,
                "[" => has_words(&t.appearance, "shield"),
                "(" => ["lamp", "lantern", "candle", "candelabrum"]
                    .iter()
                    .any(|w| has_words(&t.appearance, w)),
                _ => false,
            };
            if !wanted {
                continue;
            }
            let h = art
                .held(t)
                .unwrap_or_else(|| panic!("{t:?} is not held as anything"));
            let r = Resolved {
                model: h.model,
                ..art.object(t)
            };
            assert!(exists(&art, &r), "{t:?}");
            assert!(h.scale > 0.0 && h.scale.is_finite(), "{t:?}");
            if h.level == Level::Class {
                generic.push(t.appearance.as_str());
            }
        }
        // only the unnamed class glyph falls back to the class's model
        assert_eq!(generic, ["weapon"], "held by class only");
        let lamp = art.held(tile(&cat, "(", "lamp")).unwrap();
        assert!(art.held_at(lamp.held).1.light.is_some());
        // body armour is worn, not held
        assert_eq!(art.held(tile(&cat, "[", "plate mail")), None);
    }

    #[test]
    fn a_held_model_follows_the_head_noun_of_the_appearance() {
        let (art, cat) = (manifest(), catalog());
        let h = |class: &str, a: &str| held_name(&art, art.held(tile(&cat, class, a)));
        assert_eq!(h(")", "runed dagger"), Some("dagger"));
        assert_eq!(h(")", "crude dagger"), Some("dagger"));
        assert_eq!(h(")", "long sword"), Some("long_blade"));
        assert_eq!(h(")", "runed broadsword"), Some("long_blade"));
        assert_eq!(h(")", "two-handed sword"), Some("great_blade"));
        assert_eq!(h(")", "crude short sword"), Some("short_blade"));
        assert_eq!(h(")", "spear"), Some("spear"));
        assert_eq!(h(")", "hilted polearm"), Some("polearm"));
        assert_eq!(h(")", "double-headed axe"), Some("great_axe"));
        assert_eq!(h(")", "axe"), Some("axe"));
        assert_eq!(h(")", "crossbow"), Some("bow"));
        assert_eq!(h("[", "large round shield"), Some("round_shield"));
        assert_eq!(h("[", "polished silver shield"), Some("kite_shield"));
        assert_eq!(h("(", "brass lantern"), Some("lantern"));
        assert_eq!(h("(", "candle"), Some("candle"));
        // a potion in hand is its own model on the floor, tinted the same
        let p = tile(&cat, "!", "ruby");
        let held = art.held(p).unwrap();
        assert_eq!(held.model, art.object(p).model);
        assert_eq!(held.tint, art.object(p).tint);
        // its colour is the appearance's, never the potion's identity
        assert!(art.appearance_color(p).is_some_and(|c| c[0] > c[1]));
        assert!(
            art.appearance_color(tile(&cat, "!", "dark green"))
                .is_some_and(|c| c[1] > c[0])
        );
        assert_eq!(art.appearance_color(tile(&cat, "?", "ZELGO MER")), None);
    }

    /// Like `ArtManifest::object`, `held` sees only the appearance tile.
    #[test]
    fn the_held_function_takes_no_glyph() {
        let f: fn(&ArtManifest, &ObjectTile) -> Option<HeldArt> = ArtManifest::held;
        let (art, cat) = (manifest(), catalog());
        let t = tile(&cat, ")", "runed dagger");
        let twin = ObjectTile {
            tile: t.tile + 1000,
            ..t.clone()
        };
        assert_eq!(f(&art, &twin), art.held(t));
    }

    fn pack(items: Vec<nh_protocol::InvItem>, twoweap: bool) -> nh_world::Pack {
        let mut p = nh_world::Pack::new();
        p.replace(&nh_protocol::Inventory { items, twoweap });
        p
    }

    fn inv(
        cat: &Catalog,
        letter: char,
        class: &str,
        a: &str,
        slots: Vec<nh_protocol::Slot>,
        lit: bool,
    ) -> nh_protocol::InvItem {
        nh_protocol::InvItem {
            letter,
            class: class.chars().next().unwrap(),
            tile: tile(cat, class, a).tile,
            quan: 1,
            slots,
            lit,
            text: String::new(),
        }
    }

    #[test]
    fn a_valkyrie_shows_her_spear_and_shield() {
        use nh_protocol::Slot;
        let (art, cat) = (manifest(), catalog());
        let items = vec![
            inv(&cat, 'a', ")", "spear", vec![Slot::Weapon], false),
            inv(&cat, 'b', ")", "dagger", vec![Slot::Alternate], false),
            inv(&cat, 'c', "[", "wooden shield", vec![Slot::Shield], false),
            inv(&cat, 'd', "(", "lamp", vec![], false),
        ];
        let g = art.gear(&pack(items.clone(), false), &cat);
        assert_eq!(held_name(&art, g.hand_r), Some("spear"));
        assert_eq!(held_name(&art, g.arm_l), Some("round_shield"));
        assert_eq!(held_name(&art, g.back), Some("dagger"));
        assert_eq!(g.hand_l, None, "an unlit lamp stays in the pack");
        assert_eq!(g.idle.as_deref(), Some("ual2/Idle_Shield"));
        assert_eq!(g.attack.as_deref(), Some("Sword_Attack"));
        assert!(!g.lit());
        // the lamp lit: in the left hand, and the idle holds it up
        let mut lit = items.clone();
        lit[3].lit = true;
        let g = art.gear(&pack(lit, false), &cat);
        assert_eq!(held_name(&art, g.hand_l), Some("oil_lamp"));
        assert!(g.lit());
        assert_eq!(g.idle.as_deref(), Some("Idle_Torch"));
        // two weapons: the dagger in the left hand, nothing on the back
        let mut two = items.clone();
        two.remove(2);
        let g = art.gear(&pack(two, true), &cat);
        assert_eq!(held_name(&art, g.hand_l), Some("dagger"));
        assert_eq!(g.back, None);
        // bare hands punch
        let g = art.gear(&pack(vec![items[3].clone()], false), &cat);
        assert_eq!((g.hand_r, g.attack.as_deref()), (None, Some("Punch_Jab")));
        assert_eq!(g.idle.as_deref(), Some("Idle"));
    }

    #[test]
    fn worn_armour_shows_on_the_outfit_parts() {
        use nh_protocol::Slot;
        let (art, cat) = (manifest(), catalog());
        let items = vec![
            inv(&cat, 'a', "[", "leather armor", vec![Slot::Body], false),
            inv(&cat, 'b', "[", "visored helmet", vec![Slot::Helmet], false),
        ];
        let g = art.gear(&pack(items, false), &cat);
        assert_eq!(g.worn, ["body", "helmet"]);
        assert_eq!(held_name(&art, g.head), Some("helm"));
        let parts = &art.held_rules().parts;
        let rule = |suffix: &str| parts.iter().find(|p| p.suffix == suffix).unwrap();
        assert!(rule("_Acc_Pauldron").shown(&g.worn));
        assert!(!rule("_Arms_Bracer").shown(&g.worn));
        assert!(!rule("_Head_Hood").shown(&g.worn));
        assert!(rule("_Head_Hood").shown(&[]));
    }

    #[test]
    fn every_held_clip_is_in_its_library() {
        let art = manifest();
        for (lib, clip) in art.held_rules().clips() {
            if lib == Some(PROC_CLIPS) {
                continue;
            }
            let file = art.library(lib.unwrap_or("ual")).unwrap();
            let clips = clips_in(&art_dir().join(file)).unwrap();
            assert!(clips.iter().any(|c| c == clip), "no clip {clip} in {file}");
        }
    }

    #[test]
    fn every_portrait_is_of_a_known_monster() {
        let (art, cat) = (manifest(), catalog());
        for (name, _) in art.portraits() {
            assert!(cat.monsters.iter().any(|m| m.name == name), "{name}");
        }
    }

    #[test]
    fn a_bad_manifest_is_refused() {
        let json = std::fs::read_to_string(art_dir().join("manifest.json")).unwrap();
        let broken = json.replacen("\"model\": \"human_male\"", "\"model\": \"nobody\"", 1);
        assert_ne!(broken, json, "the test edits a real rule");
        assert!(matches!(
            ArtManifest::parse(&broken),
            Err(ArtError::Invalid(_))
        ));
    }
}
