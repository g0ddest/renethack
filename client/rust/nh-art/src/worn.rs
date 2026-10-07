//! How the things the hero wears change their outfit (art direction §4),
//! by the appearance alone, as the held ones do: body armour of metal is
//! its material over the outfit's torso (chain mail, plate), gloves are
//! leather of their appearance's colour over the hands, and a thing carried
//! round the neck (a stethoscope) is a model skinned to the skeleton, made
//! in variants fitted to the bodies (a man's, a woman's).

use nh_protocol::{Catalog, InvItem, ObjectTile, Slot};
use nh_world::Pack;
use serde::Deserialize;

use crate::{ArtManifest, appearance_matches, hex};

/// A thing worn round the neck while it is carried, by its appearance (the
/// class, any when absent; an exact appearance or whole words in it): the
/// models it is worn as, each fitted to one body. The client wears the one
/// fitted to the skeleton nearest the hero's.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeckRule {
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub exact: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
    pub models: Vec<String>,
}

/// How a worn thing looks on the outfit, by its appearance (exactly, or
/// whole words in it; neither: any): body armour is a manifest material
/// over the outfit's meshes whose names end in one of `parts`, gloves the
/// material over the hands, multiplied by `tint`. No material: the
/// outfit's own stays (leather).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WornLook {
    #[serde(default)]
    pub exact: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub tint: Option<String>,
    #[serde(default)]
    pub parts: Vec<String>,
}

impl WornLook {
    fn matches(&self, tile: &ObjectTile) -> bool {
        (self.exact.is_empty() && self.words.is_empty())
            || appearance_matches(None, &self.exact, &self.words, tile)
    }
}

/// A worn look resolved for one item: its rule (of `WornRules::armour` or
/// `gloves`) and its tint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub rule: usize,
    pub tint: [f32; 3],
}

/// The manifest's `worn` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WornRules {
    #[serde(default)]
    pub neck: Vec<NeckRule>,
    /// Body armour, the first rule that matches.
    #[serde(default)]
    pub armour: Vec<WornLook>,
    #[serde(default)]
    pub gloves: Vec<WornLook>,
}

impl WornRules {
    pub(crate) fn check(
        &self,
        model: impl Fn(&str) -> bool,
        material: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for (i, r) in self.neck.iter().enumerate() {
            if r.models.is_empty() {
                errors.push(format!("worn neck rule {i}: no models"));
            }
            for m in r.models.iter().filter(|m| !model(m)) {
                errors.push(format!("worn neck rule {i}: no model {m}"));
            }
        }
        for (what, looks) in [("armour", &self.armour), ("gloves", &self.gloves)] {
            for (i, l) in looks.iter().enumerate() {
                if let Some(m) = &l.material
                    && !material(m)
                {
                    errors.push(format!("worn {what} rule {i}: no material {m}"));
                }
                if let Some(t) = &l.tint
                    && hex(t).is_none()
                {
                    errors.push(format!("worn {what} rule {i}: tint {t}"));
                }
            }
        }
        errors
    }

    /// The look of a worn thing of this appearance: the first of `looks`
    /// that matches it, when that one has a material.
    fn look(looks: &[WornLook], tile: &ObjectTile) -> Option<Look> {
        let rule = looks.iter().position(|l| l.matches(tile))?;
        let l = &looks[rule];
        l.material.as_ref()?;
        let tint = l.tint.as_deref().and_then(hex).unwrap_or([1.0; 3]);
        Some(Look { rule, tint })
    }
}

impl ArtManifest {
    pub fn worn_rules(&self) -> &WornRules {
        &self.worn
    }

    /// What the things worn show (see `Gear`): the rule of the first thing
    /// carried that is worn round the neck, and how the body armour and the
    /// gloves look.
    pub(crate) fn worn_looks(
        &self,
        pack: &Pack,
        catalog: &Catalog,
    ) -> (Option<usize>, Option<Look>, Option<Look>) {
        let tile = |i: &InvItem| catalog.object_tiles.iter().find(|t| t.tile == i.tile);
        let neck = pack.items().iter().find_map(|i| {
            let t = tile(i)?;
            self.worn
                .neck
                .iter()
                .position(|r| appearance_matches(r.class.as_deref(), &r.exact, &r.words, t))
        });
        let look = |slot: Slot, looks: &[WornLook]| {
            pack.in_slot(&slot)
                .and_then(|i| WornRules::look(looks, tile(i)?))
        };
        (
            neck,
            look(Slot::Body, &self.worn.armour),
            look(Slot::Gloves, &self.worn.gloves),
        )
    }
}
