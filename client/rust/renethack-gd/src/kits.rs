//! The starting kits NetHack gives each role (u_init.c), as far as they
//! show on the hero: what is wielded, the alternate weapon and the quiver,
//! the shield, the armour worn, and what shows though only carried (a
//! Healer's stethoscope, round the neck). The title scene dresses its hero
//! in them, and the `kits` self-test holds the game's hero to them, so the
//! two cannot drift apart.

use nh_protocol::{Catalog, InvItem, Inventory, Slot};

/// Where a thing of a kit goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wear {
    Wielded,
    /// The alternate weapon (`x`), on the back.
    Alternate,
    Quiver,
    Shield,
    Helmet,
    Body,
    Cloak,
    Gloves,
    Shirt,
    /// In the pack only, yet shown.
    Carried,
}

impl Wear {
    /// The slot it is in; none for a thing only carried.
    pub fn slot(self) -> Option<Slot> {
        Some(match self {
            Wear::Wielded => Slot::Weapon,
            Wear::Alternate => Slot::Alternate,
            Wear::Quiver => Slot::Quiver,
            Wear::Shield => Slot::Shield,
            Wear::Helmet => Slot::Helmet,
            Wear::Body => Slot::Body,
            Wear::Cloak => Slot::Cloak,
            Wear::Gloves => Slot::Gloves,
            Wear::Shirt => Slot::Shirt,
            Wear::Carried => return None,
        })
    }
}

/// A thing of a kit: where it goes, and its appearances unidentified as
/// the catalog names them (the first the catalog has is taken: a helmet
/// or a pair of gloves looks like any of several).
pub type KitItem = (Wear, &'static [&'static str]);

const HELMET: &[&str] = &[
    "plumed helmet",
    "etched helmet",
    "crested helmet",
    "visored helmet",
];
const GLOVES: &[&str] = &[
    "old gloves",
    "padded gloves",
    "riding gloves",
    "fencing gloves",
];
const CLOAK: &[&str] = &[
    "tattered cape",
    "opera cloak",
    "ornamental cope",
    "piece of cloth",
];
const SMALL_SHIELD: &[&str] = &["small shield", "wooden shield"];

use Wear::*;

const ARCHEOLOGIST: &[KitItem] = &[
    (Wielded, &["bullwhip"]),
    (Alternate, &["pick-axe"]),
    (Body, &["leather jacket"]),
    (Helmet, &["fedora"]),
];
const BARBARIAN_0: &[KitItem] = &[
    (Wielded, &["two-handed sword"]),
    (Alternate, &["axe"]),
    (Body, &["ring mail"]),
];
const BARBARIAN_1: &[KitItem] = &[
    (Wielded, &["double-headed axe"]),
    (Alternate, &["short sword"]),
    (Body, &["ring mail"]),
];
const CAVEMAN: &[KitItem] = &[
    (Wielded, &["club"]),
    (Alternate, &["sling"]),
    (Body, &["leather armor"]),
];
const HEALER: &[KitItem] = &[
    (Wielded, &["scalpel"]),
    (Gloves, GLOVES),
    (Carried, &["stethoscope"]),
];
const KNIGHT: &[KitItem] = &[
    (Wielded, &["long sword"]),
    (Alternate, &["lance"]),
    (Body, &["ring mail"]),
    (Helmet, HELMET),
    (Shield, SMALL_SHIELD),
    (Gloves, GLOVES),
];
const MONK: &[KitItem] = &[(Gloves, GLOVES), (Cloak, &["robe"])];
const PRIEST: &[KitItem] = &[
    (Wielded, &["mace"]),
    (Cloak, &["robe"]),
    (Shield, SMALL_SHIELD),
];
const RANGER: &[KitItem] = &[
    (Wielded, &["dagger"]),
    (Alternate, &["bow"]),
    (Quiver, &["arrow"]),
    (Cloak, CLOAK),
];
const ROGUE: &[KitItem] = &[
    (Wielded, &["short sword"]),
    (Alternate, &["dagger"]),
    (Body, &["leather armor"]),
];
const SAMURAI: &[KitItem] = &[
    (Wielded, &["samurai sword"]),
    (Alternate, &["short sword"]),
    (Quiver, &["bamboo arrow"]),
    (Body, &["splint mail"]),
];
const TOURIST: &[KitItem] = &[(Quiver, &["dart"]), (Shirt, &["Hawaiian shirt"])];
const VALKYRIE: &[KitItem] = &[
    (Wielded, &["spear"]),
    (Alternate, &["dagger"]),
    (Shield, SMALL_SHIELD),
];
const WIZARD: &[KitItem] = &[(Wielded, &["staff"]), (Cloak, CLOAK)];

/// The roles, by the engine's name for each.
pub const ROLES: [&str; 13] = [
    "archeologist",
    "barbarian",
    "caveman",
    "healer",
    "knight",
    "monk",
    "priest",
    "ranger",
    "rogue",
    "samurai",
    "tourist",
    "valkyrie",
    "wizard",
];

/// The kits a role starts with ("Valkyrie", "priestess"...): one, or the
/// Barbarian's two (a game gives him the one or the other; the title
/// dresses him in the first). None for a role NetHack does not have.
pub fn kits(role: &str) -> &'static [&'static [KitItem]] {
    match role.to_lowercase().as_str() {
        "archeologist" => &[ARCHEOLOGIST],
        "barbarian" => &[BARBARIAN_0, BARBARIAN_1],
        "caveman" | "cavewoman" => &[CAVEMAN],
        "healer" => &[HEALER],
        "knight" => &[KNIGHT],
        "monk" => &[MONK],
        "priest" | "priestess" => &[PRIEST],
        "ranger" => &[RANGER],
        "rogue" => &[ROGUE],
        "samurai" => &[SAMURAI],
        "tourist" => &[TOURIST],
        "valkyrie" => &[VALKYRIE],
        "wizard" => &[WIZARD],
        _ => &[],
    }
}

/// A kit as a pack: each thing the catalog has, in its slot.
pub fn inventory(catalog: &Catalog, kit: &[KitItem]) -> Inventory {
    let mut letters = 'a'..='z';
    let items = kit
        .iter()
        .filter_map(|(wear, names)| {
            let (name, t) = names.iter().find_map(|name| {
                catalog
                    .object_tiles
                    .iter()
                    .find(|t| t.appearance == *name)
                    .map(|t| (name, t))
            })?;
            Some(InvItem {
                letter: letters.next()?,
                class: t.class.chars().next().unwrap_or(')'),
                tile: t.tile,
                quan: 1,
                slots: wear.slot().into_iter().collect(),
                lit: false,
                text: format!("a {name}"),
            })
        })
        .collect();
    Inventory {
        items,
        twoweap: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{EngineMsg, parse_line};

    fn catalog() -> Catalog {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_thing_of_every_kit_is_in_the_catalog() {
        let cat = catalog();
        for role in ROLES {
            let all = kits(role);
            assert!(!all.is_empty(), "{role}");
            for kit in all {
                let inv = inventory(&cat, kit);
                assert_eq!(inv.items.len(), kit.len(), "{role}: {kit:?}");
            }
        }
    }

    #[test]
    fn the_healer_has_the_stethoscope_round_the_neck() {
        let cat = catalog();
        let art = nh_art::ArtManifest::parse(include_str!("../../../godot/art/manifest.json"));
        let art = art.unwrap();
        let neck = |role: &str| {
            let mut pack = nh_world::Pack::new();
            pack.replace(&inventory(&cat, kits(role)[0]));
            art.gear(&pack, &cat).neck
        };
        assert!(neck("healer").is_some());
        assert_eq!(neck("priest"), None);
    }

    #[test]
    fn a_kit_is_found_by_any_name_of_its_role() {
        assert_eq!(kits("Valkyrie"), kits("valkyrie"));
        assert_eq!(kits("Priestess"), kits("priest"));
        assert_eq!(kits("cavewoman"), kits("Caveman"));
        assert_eq!(kits("barbarian").len(), 2);
        assert!(kits("knave").is_empty());
    }
}
