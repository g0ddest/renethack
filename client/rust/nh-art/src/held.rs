//! What the hero holds and wears, drawn on the rig's bones (art direction
//! §4): the manifest's `held` section picks a held model by the item's
//! appearance (appearance rule, else its class), never by what it is. The
//! same chain gives a used item's model in the hand (a potion drunk, a
//! scroll read).

use std::collections::BTreeMap;

use nh_protocol::{Catalog, InvItem, ObjectTile, Slot};
use nh_world::Pack;
use serde::Deserialize;

use crate::{ArtManifest, Level, Look, Skin, Tint, appearance_matches, has_words, hex};

/// A transform in a parent's space: metres, degrees (Euler YXZ).
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grip {
    #[serde(default)]
    pub pos: [f32; 3],
    #[serde(default)]
    pub rot: [f32; 3],
}

/// Where a slot's things hang: a bone and the grip from the bone's space
/// to the held frame (handle at the origin, the item along +y).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotSpec {
    pub bone: String,
    #[serde(default)]
    pub pos: [f32; 3],
    #[serde(default)]
    pub rot: [f32; 3],
}

impl SlotSpec {
    pub fn grip(&self) -> Grip {
        Grip {
            pos: self.pos,
            rot: self.rot,
        }
    }
}

/// The light a burning thing gives.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightSpec {
    pub color: String,
    pub range: f32,
    pub energy: f32,
    /// The flame, in the model's own units.
    #[serde(default)]
    pub at: [f32; 3],
    /// How much the energy wavers (0..1).
    #[serde(default)]
    pub flicker: f32,
}

impl LightSpec {
    pub fn rgb(&self) -> [f32; 3] {
        hex(&self.color).unwrap_or([1.0, 0.7, 0.42])
    }
}

/// A model as held.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldSpec {
    /// A `models` entry; none: the object's own model on the floor.
    #[serde(default)]
    pub model: Option<String>,
    /// Its length (or height) in the hand, metres.
    pub size: f32,
    /// From the model's own space to the held frame.
    #[serde(default)]
    pub grip: Grip,
    /// The grip on the back, where it is not the hand's (a pole carried
    /// head up, while in the hand it stands butt down).
    #[serde(default)]
    pub back: Option<Grip>,
    /// Meshes of the scene not shown (a dagger's scabbard).
    #[serde(default)]
    pub hide: Vec<String>,
    /// The hero's idle and attack clips while it is wielded.
    #[serde(default)]
    pub idle: Option<String>,
    #[serde(default)]
    pub attack: Option<String>,
    /// A lit one's light.
    #[serde(default)]
    pub light: Option<LightSpec>,
}

/// A held model by appearance: the class (any when absent) and an exact
/// appearance or whole words in it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldRule {
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub exact: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
    pub held: String,
}

/// A part of an outfit (a mesh whose name ends so) shown only while
/// something is worn in `worn`, or hidden while `hidden_by` is worn.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartRule {
    pub suffix: String,
    #[serde(default)]
    pub worn: Option<String>,
    #[serde(default)]
    pub hidden_by: Option<String>,
}

impl PartRule {
    /// Whether the part shows with these slots worn.
    pub fn shown(&self, worn: &[&str]) -> bool {
        self.worn.as_deref().is_none_or(|w| worn.contains(&w))
            && !self.hidden_by.as_deref().is_some_and(|h| worn.contains(&h))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldRules {
    /// hand_r, hand_l, arm_l, back, quiver, head.
    #[serde(default)]
    pub slots: BTreeMap<String, SlotSpec>,
    #[serde(default)]
    pub models: BTreeMap<String, HeldSpec>,
    #[serde(default)]
    pub appearances: Vec<HeldRule>,
    /// A held model per class symbol.
    #[serde(default)]
    pub classes: BTreeMap<String, String>,
    /// The held model on the back for what is quivered.
    #[serde(default)]
    pub quiver: Option<String>,
    /// What the quiver holds: words of the appearance of what is quivered
    /// ("arrow": arrows and ya, not darts); none: anything.
    #[serde(default)]
    pub quivers: Vec<String>,
    /// Colours of appearances ("ruby": "#9b1b30"): a drink's sparkle.
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    #[serde(default)]
    pub parts: Vec<PartRule>,
    /// The hero's idle clip with a lit light ("light"), a shield
    /// ("shield"), empty hands ("bare").
    #[serde(default)]
    pub idle: BTreeMap<String, String>,
    /// The attack clip bare-handed ("bare").
    #[serde(default)]
    pub attack: BTreeMap<String, String>,
    /// The clip of each use (`UseKind` in snake case: "quaff", "zap"...).
    #[serde(default)]
    pub uses: BTreeMap<String, String>,
}

impl HeldRules {
    /// Whether the quiver shows for a quivered thing of this appearance.
    fn quivered(&self, appearance: &str) -> bool {
        let a = appearance.to_lowercase();
        self.quivers.is_empty() || self.quivers.iter().any(|w| has_words(&a, w))
    }
}

/// The "library" of the clips built in code (`proc/read`).
pub const PROC_CLIPS: &str = "proc";

/// Slots every manifest's `held.slots` has.
pub const SLOTS: [&str; 6] = ["hand_r", "hand_l", "arm_l", "back", "quiver", "head"];

impl HeldRules {
    pub(crate) fn check(
        &self,
        model: impl Fn(&str) -> bool,
        library: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if self.models.is_empty() {
            return errors;
        }
        for s in SLOTS {
            if !self.slots.contains_key(s) {
                errors.push(format!("held: no slot {s}"));
            }
        }
        for (name, h) in &self.models {
            if let Some(m) = &h.model
                && !model(m)
            {
                errors.push(format!("held {name}: no model {m}"));
            }
            if h.size <= 0.0 {
                errors.push(format!("held {name}: size {}", h.size));
            }
            if let Some(l) = &h.light
                && hex(&l.color).is_none()
            {
                errors.push(format!("held {name}: light colour {}", l.color));
            }
        }
        let known = |h: &String| self.models.contains_key(h);
        for (i, r) in self.appearances.iter().enumerate() {
            if !known(&r.held) {
                errors.push(format!("held rule {i}: no held model {}", r.held));
            }
        }
        for h in self.classes.values().chain(&self.quiver) {
            if !known(h) {
                errors.push(format!("held class: no held model {h}"));
            }
        }
        for (n, c) in &self.colors {
            if hex(c).is_none() {
                errors.push(format!("held colour {n}: {c}"));
            }
        }
        let clips = self
            .models
            .values()
            .flat_map(|h| h.idle.iter().chain(&h.attack));
        for clip in clips
            .chain(self.idle.values())
            .chain(self.attack.values())
            .chain(self.uses.values())
        {
            if let Some((lib, _)) = clip.split_once('/')
                && lib != PROC_CLIPS
                && !library(lib)
            {
                errors.push(format!("held clip {clip}: no library {lib}"));
            }
        }
        errors
    }

    /// Every clip named: (library or None for the rig's own, clip).
    pub fn clips(&self) -> Vec<(Option<&str>, &str)> {
        let mut out: Vec<(Option<&str>, &str)> = self
            .models
            .values()
            .flat_map(|h| h.idle.iter().chain(&h.attack))
            .chain(self.idle.values())
            .chain(self.attack.values())
            .chain(self.uses.values())
            .map(|c| match c.split_once('/') {
                Some((l, n)) => (Some(l), n),
                None => (None, c.as_str()),
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// A held model resolved for one item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeldArt {
    /// Index into `ArtManifest::held_at`.
    pub held: usize,
    /// Index into `ArtManifest::model_at`.
    pub model: usize,
    /// From the model's units to metres.
    pub scale: f32,
    pub tint: Tint,
    pub skin: Skin,
    pub level: Level,
    /// A light source burning now (it has a light).
    pub lit: bool,
}

/// What the hero shows of their equipment, per slot of `SLOTS`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gear {
    pub hand_r: Option<HeldArt>,
    pub hand_l: Option<HeldArt>,
    pub arm_l: Option<HeldArt>,
    pub back: Option<HeldArt>,
    pub quiver: Option<HeldArt>,
    pub head: Option<HeldArt>,
    /// Armour slots worn: "body", "cloak", "helmet", "gloves", "boots",
    /// "shirt".
    pub worn: Vec<&'static str>,
    /// What is worn round the neck (a rule of `worn.neck`), and how the body
    /// armour and the gloves look (see `WornRules`).
    pub neck: Option<usize>,
    pub armour: Option<Look>,
    pub gloves: Option<Look>,
    pub idle: Option<String>,
    pub attack: Option<String>,
}

impl Gear {
    /// (slot name, what is there) for every slot.
    pub fn slots(&self) -> [(&'static str, Option<HeldArt>); 6] {
        [
            ("hand_r", self.hand_r),
            ("hand_l", self.hand_l),
            ("arm_l", self.arm_l),
            ("back", self.back),
            ("quiver", self.quiver),
            ("head", self.head),
        ]
    }

    /// A lit light is carried in hand.
    pub fn lit(&self) -> bool {
        self.hand_l.is_some_and(|h| h.lit)
    }
}

impl ArtManifest {
    pub fn held_at(&self, i: usize) -> (&str, &HeldSpec) {
        let (n, h) = &self.held_models[i];
        (n, h)
    }

    pub fn held_index(&self, name: &str) -> Option<usize> {
        self.held_models.iter().position(|(n, _)| n == name)
    }

    pub fn held_rules(&self) -> &HeldRules {
        &self.held
    }

    pub fn held_slot(&self, slot: &str) -> Option<&SlotSpec> {
        self.held.slots.get(slot)
    }

    fn held_named(&self, name: &str, tile: &ObjectTile, level: Level) -> Option<HeldArt> {
        let i = self.held_index(name)?;
        let spec = &self.held_models[i].1;
        let (model, tint, skin) = match &spec.model {
            Some(m) => (self.model_index(m)?, Tint::None, Skin::Own),
            None => {
                let r = self.object(tile);
                (r.model, r.tint, r.skin)
            }
        };
        Some(HeldArt {
            held: i,
            model,
            scale: spec.size / self.models[model].1.size,
            tint,
            skin,
            level,
            lit: false,
        })
    }

    /// The held model of an object by its appearance, else its class; None
    /// when things of its class are not held (body armour).
    pub fn held(&self, tile: &ObjectTile) -> Option<HeldArt> {
        let rules = &self.held;
        if let Some(r) = rules
            .appearances
            .iter()
            .find(|r| appearance_matches(r.class.as_deref(), &r.exact, &r.words, tile))
        {
            return self.held_named(&r.held, tile, Level::Appearance);
        }
        let name = rules.classes.get(&tile.class)?;
        self.held_named(name, tile, Level::Class)
    }

    /// The colour an appearance names ("ruby", "dark green"), if any.
    pub fn appearance_color(&self, tile: &ObjectTile) -> Option<[f32; 3]> {
        let a = tile.appearance.to_lowercase();
        let mut names: Vec<(&String, &String)> = self.held.colors.iter().collect();
        // "dark green" before "green"
        names.sort_by_key(|(n, _)| std::cmp::Reverse(n.split(' ').count()));
        names
            .into_iter()
            .find(|(n, _)| has_words(&a, n))
            .and_then(|(_, c)| hex(c))
    }

    /// What the hero shows of the pack: weapon, shield, a lit light, the
    /// alternate weapon and the quiver on the back, a helmet, and the
    /// armour slots worn and how they look; the idle and attack clips they
    /// call for.
    pub fn gear(&self, pack: &Pack, catalog: &Catalog) -> Gear {
        let tile = |i: &InvItem| catalog.object_tiles.iter().find(|t| t.tile == i.tile);
        let held = |i: Option<&InvItem>| i.and_then(|i| self.held(tile(i)?));
        let mut g = Gear {
            hand_r: held(pack.wielded()),
            arm_l: held(pack.in_slot(&Slot::Shield)),
            head: held(pack.in_slot(&Slot::Helmet)),
            ..Gear::default()
        };
        let alternate = pack.in_slot(&Slot::Alternate);
        if pack.twoweap() {
            g.hand_l = held(alternate);
        } else {
            g.back = held(alternate);
            // the first light burning that is held with a light
            g.hand_l = pack.lit().find_map(|i| {
                let mut h = self.held(tile(i)?)?;
                self.held_models[h.held].1.light.as_ref()?;
                h.lit = true;
                Some(h)
            });
        }
        if let (Some(q), Some(name)) = (pack.in_slot(&Slot::Quiver), &self.held.quiver)
            && q.class == ')'
            && let Some(t) = tile(q)
            && self.held.quivered(&t.appearance)
        {
            g.quiver = self.held_named(name, t, Level::Class);
        }
        (g.neck, g.armour, g.gloves) = self.worn_looks(pack, catalog);
        for (slot, name) in [
            (Slot::Body, "body"),
            (Slot::Cloak, "cloak"),
            (Slot::Helmet, "helmet"),
            (Slot::Gloves, "gloves"),
            (Slot::Boots, "boots"),
            (Slot::Shirt, "shirt"),
        ] {
            if pack.in_slot(&slot).is_some() {
                g.worn.push(name);
            }
        }
        let spec = |h: Option<HeldArt>| h.map(|h| &self.held_models[h.held].1);
        let rules = &self.held;
        g.idle = if g.lit() {
            rules.idle.get("light").cloned()
        } else if g.arm_l.is_some() {
            rules.idle.get("shield").cloned()
        } else {
            match spec(g.hand_r) {
                Some(s) => s.idle.clone(),
                None => rules.idle.get("bare").cloned(),
            }
        };
        g.attack = match spec(g.hand_r) {
            Some(s) => s.attack.clone(),
            None => rules.attack.get("bare").cloned(),
        };
        g
    }
}
