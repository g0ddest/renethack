//! What the player can do with an inventory item: the rows of its context
//! menu and detail panel, the double-click action. Only what the item
//! shows decides it (its class, its doname() text, where it is worn),
//! never what it truly is.

use nh_protocol::{InvItem, Slot};
use serde::{Deserialize, Serialize};

use crate::{Pack, parse_item_name};

/// An action on an item; each one is a macro (see `Macro::item`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemActionKind {
    Wield,
    /// `w -`: bare hands.
    Unwield,
    /// Make it the alternate weapon: `w`, then `x` (two actions).
    SetAlternate,
    /// `x`.
    SwapWeapons,
    /// `Q`: ready it in the quiver.
    Quiver,
    /// `Q -`.
    EmptyQuiver,
    /// `f`: fire what the quiver holds.
    Fire,
    Throw,
    Apply,
    Wear,
    TakeOff,
    PutOn,
    PutOnLeft,
    PutOnRight,
    Remove,
    Eat,
    Quaff,
    Read,
    Zap,
    /// `E`: engrave with it.
    Engrave,
    /// Apply a wand: the engine asks whether to break it.
    Break,
    Drop,
    /// Drop part of a stack: a count first.
    DropSome,
    /// `#adjust`: another letter (the UI asks which first).
    Adjust,
    /// `#adjust` with a count: split the stack (the UI asks how many and
    /// the letter first).
    Split,
    /// `#name`: this very item.
    Name,
    /// `#name`: its type ("call").
    Call,
    /// `#dip` into another item (the UI asks which first).
    Dip,
    TwoWeapon,
    /// `#force` a lock with the wielded weapon.
    Force,
    /// `#rub` (gray stones).
    Rub,
    /// `#tip` a container out.
    Tip,
}

impl ItemActionKind {
    /// The key of the label in the text tables: "item.wield".
    pub fn label_key(self) -> &'static str {
        use ItemActionKind::*;
        match self {
            Wield => "item.wield",
            Unwield => "item.unwield",
            SetAlternate => "item.set_alternate",
            SwapWeapons => "item.swap_weapons",
            Quiver => "item.quiver",
            EmptyQuiver => "item.empty_quiver",
            Fire => "item.fire",
            Throw => "item.throw",
            Apply => "item.apply",
            Wear => "item.wear",
            TakeOff => "item.take_off",
            PutOn => "item.put_on",
            PutOnLeft => "item.put_on_left",
            PutOnRight => "item.put_on_right",
            Remove => "item.remove",
            Eat => "item.eat",
            Quaff => "item.quaff",
            Read => "item.read",
            Zap => "item.zap",
            Engrave => "item.engrave",
            Break => "item.break",
            Drop => "item.drop",
            DropSome => "item.drop_some",
            Adjust => "item.adjust",
            Split => "item.split",
            Name => "item.name",
            Call => "item.call",
            Dip => "item.dip",
            TwoWeapon => "item.two_weapon",
            Force => "item.force",
            Rub => "item.rub",
            Tip => "item.tip",
        }
    }

    /// The NetHack keys it stands for, as a hint: "w", "w -", "#adjust".
    pub fn keys(self) -> &'static str {
        use ItemActionKind::*;
        match self {
            Wield => "w",
            Unwield => "w -",
            SetAlternate => "w x",
            SwapWeapons => "x",
            Quiver => "Q",
            EmptyQuiver => "Q -",
            Fire => "f",
            Throw => "t",
            Apply | Break => "a",
            Wear => "W",
            TakeOff => "T",
            PutOn | PutOnLeft | PutOnRight => "P",
            Remove => "R",
            Eat => "e",
            Quaff => "q",
            Read => "r",
            Zap => "z",
            Engrave => "E",
            Drop | DropSome => "d",
            Adjust | Split => "#adjust",
            Name | Call => "#name",
            Dip => "#dip",
            TwoWeapon => "#twoweapon",
            Force => "#force",
            Rub => "#rub",
            Tip => "#tip",
        }
    }

    /// The UI must choose something first: another item (dip), a letter
    /// (adjust), a count (split, drop some).
    pub fn needs_choice(self) -> bool {
        use ItemActionKind::*;
        matches!(self, Dip | Adjust | Split | DropSome)
    }
}

/// One row of an item's menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemAction {
    pub kind: ItemActionKind,
    pub label_key: &'static str,
    pub keys: &'static str,
    /// The double-click action (the first row, bold).
    pub default: bool,
    /// It takes two engine commands (shown as "(2 actions)").
    pub two_actions: bool,
}

/// Armor slots: worn armor is taken off with `T`.
const ARMOR_SLOTS: [Slot; 7] = [
    Slot::Body,
    Slot::Cloak,
    Slot::Helmet,
    Slot::Shield,
    Slot::Gloves,
    Slot::Boots,
    Slot::Shirt,
];

/// Worn as armor (`T` takes it off).
pub fn worn_armor(item: &InvItem) -> bool {
    item.slots.iter().any(|s| ARMOR_SLOTS.contains(s))
}

/// Worn as an accessory: ring, amulet, blindfold (`R` removes it).
pub fn worn_accessory(item: &InvItem) -> bool {
    item.slots.iter().any(|s| {
        matches!(
            s,
            Slot::LeftRing | Slot::RightRing | Slot::Amulet | Slot::Eyes
        )
    })
}

/// The name as the character knows the look, without "named X" and
/// "called Y": what the appearance tables below match.
fn look(item: &InvItem) -> String {
    let stem = parse_item_name(&item.text).stem;
    let end = [" named ", " called "]
        .iter()
        .filter_map(|s| stem.find(s))
        .min()
        .unwrap_or(stem.len());
    stem[..end].to_string()
}

/// Weapons applied rather than swung: polearms and lances (hit at a
/// distance), the bullwhip, the mattock (dig). Unidentified names too.
const APPLIED_WEAPONS: [&str; 20] = [
    "partisan",
    "ranseur",
    "spetum",
    "glaive",
    "halberd",
    "bardiche",
    "voulge",
    "fauchard",
    "guisarme",
    "bill-guisarme",
    "lucern hammer",
    "bec de corbin",
    "pole cleaver",
    "pole sickle",
    "pruning hook",
    "lance",
    "bullwhip",
    "dwarvish mattock",
    "broad pick",
    "grappling hook",
];

fn applied_weapon(look: &str) -> bool {
    APPLIED_WEAPONS.contains(&look) || look.ends_with(" polearm") || look.ends_with(" poleaxe")
}

/// Ammunition and missiles: readied in the quiver, fired with `f`.
fn missile(look: &str) -> bool {
    let last = look.rsplit(' ').next().unwrap_or(look);
    matches!(
        last,
        "arrow" | "bolt" | "dart" | "shuriken" | "star" | "boomerang" | "ya"
    )
}

/// Tools that are wielded: they dig, heal or hook from the hand.
fn wielded_tool(look: &str) -> bool {
    matches!(
        look,
        "pick-axe" | "unicorn horn" | "grappling hook" | "iron hook"
    )
}

/// Tools worn over the eyes.
fn eyewear(look: &str) -> bool {
    matches!(look, "blindfold" | "towel" | "lenses" | "pair of lenses")
}

fn container(look: &str) -> bool {
    let last = look.rsplit(' ').next().unwrap_or(look);
    matches!(last, "bag" | "sack" | "box" | "chest") || look.starts_with("bag of ")
}

/// Gray stones: rubbed on (touchstones) or rubbed with.
fn gray_stone(look: &str) -> bool {
    matches!(
        look,
        "gray stone" | "touchstone" | "luckstone" | "loadstone" | "flint stone"
    )
}

/// The rows of an item's menu, the double-click one first when it has
/// one; the rows every item has come last.
pub fn actions_for(item: &InvItem, pack: &Pack) -> Vec<ItemAction> {
    use ItemActionKind::*;
    let look = look(item);
    let has = |s: Slot| item.slots.contains(&s);
    let wielded = has(Slot::Weapon);
    let alternate = has(Slot::Alternate);
    let quivered = has(Slot::Quiver);
    let many = item.quan > 1;
    let mut rows: Vec<(ItemActionKind, bool)> = Vec::new();
    let mut add = |k: ItemActionKind, default: bool| rows.push((k, default));
    match item.class {
        '$' => {
            add(DropSome, false);
            add(Throw, false);
        }
        ')' => {
            if wielded {
                add(Unwield, true);
            } else if quivered {
                add(Fire, true);
            } else if missile(&look) {
                add(Quiver, true);
            } else {
                add(Wield, true);
            }
            if !wielded && missile(&look) {
                add(Wield, false);
            }
            if !wielded && !alternate {
                add(SetAlternate, false);
            }
            if wielded || alternate {
                add(SwapWeapons, false);
            }
            if !quivered && !missile(&look) {
                add(Quiver, false);
            }
            if applied_weapon(&look) {
                add(Apply, false);
            }
            add(Engrave, false);
            if (wielded || alternate)
                && pack.wielded().is_some()
                && pack.in_slot(&Slot::Alternate).is_some()
            {
                add(TwoWeapon, false);
            }
            if wielded {
                add(Force, false);
            }
        }
        '[' => add(if worn_armor(item) { TakeOff } else { Wear }, true),
        '=' => {
            if worn_accessory(item) {
                add(Remove, true);
            } else {
                add(PutOn, true);
                add(PutOnLeft, false);
                add(PutOnRight, false);
            }
        }
        '"' => add(if worn_accessory(item) { Remove } else { PutOn }, true),
        '(' => {
            if eyewear(&look) {
                add(if worn_accessory(item) { Remove } else { PutOn }, true);
                add(Apply, false);
            } else {
                add(Apply, true);
            }
            if wielded_tool(&look) {
                add(if wielded { Unwield } else { Wield }, false);
            }
            if container(&look) {
                add(Tip, false);
            }
        }
        '%' => add(Eat, true),
        '!' => {
            add(Quaff, true);
            add(Dip, false);
        }
        '?' | '+' => add(Read, true),
        '/' => {
            add(Zap, true);
            add(Engrave, false);
            add(Break, false);
        }
        '*' => {
            if quivered {
                add(Fire, false);
            } else {
                add(Quiver, false);
            }
            if gray_stone(&look) {
                add(Rub, false);
            }
        }
        _ => {}
    }
    if item.class != '$' {
        add(Drop, false);
        if many {
            add(DropSome, false);
        }
        add(Throw, false);
        add(Adjust, false);
        if many {
            add(Split, false);
        }
        add(Name, false);
        add(Call, false);
        add(Dip, false);
    }
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (kind, default) in rows {
        if seen.contains(&kind) {
            continue;
        }
        seen.push(kind);
        let two_actions = match kind {
            SetAlternate => true,
            Drop | DropSome => worn_armor(item) || worn_accessory(item),
            _ => false,
        };
        out.push(ItemAction {
            kind,
            label_key: kind.label_key(),
            keys: kind.keys(),
            default,
            two_actions,
        });
    }
    out
}

/// The double-click action, if the item has one.
pub fn default_action(item: &InvItem, pack: &Pack) -> Option<ItemActionKind> {
    actions_for(item, pack)
        .into_iter()
        .find(|a| a.default)
        .map(|a| a.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ItemActionKind::*;
    use nh_protocol::Inventory;

    fn item(letter: char, class: char, quan: i64, slots: &[Slot], text: &str) -> InvItem {
        InvItem {
            letter,
            class,
            tile: 100,
            quan,
            slots: slots.to_vec(),
            lit: false,
            text: text.into(),
        }
    }

    fn pack_of(items: &[InvItem]) -> Pack {
        let mut p = Pack::new();
        p.replace(&Inventory {
            items: items.to_vec(),
            twoweap: false,
        });
        p
    }

    fn kinds(item: &InvItem, pack: &Pack) -> Vec<ItemActionKind> {
        actions_for(item, pack).iter().map(|a| a.kind).collect()
    }

    fn valkyrie() -> Vec<InvItem> {
        vec![
            item(
                'a',
                ')',
                1,
                &[Slot::Weapon],
                "a +1 spear (weapon in right hand)",
            ),
            item(
                'b',
                ')',
                1,
                &[Slot::Alternate],
                "a +0 dagger (alternate weapon; not wielded)",
            ),
            item(
                'c',
                '[',
                1,
                &[Slot::Shield],
                "a blessed +3 small shield (being worn)",
            ),
            item('d', '%', 1, &[], "an uncursed food ration"),
            item('e', '(', 1, &[], "an uncursed oil lamp"),
        ]
    }

    #[test]
    fn a_wielded_weapon_unwields_and_the_alternate_wields() {
        let inv = valkyrie();
        let pack = pack_of(&inv);
        let spear = kinds(&inv[0], &pack);
        assert_eq!(spear[0], Unwield);
        for k in [
            SwapWeapons,
            Quiver,
            Engrave,
            TwoWeapon,
            Force,
            Drop,
            Throw,
            Name,
        ] {
            assert!(spear.contains(&k), "{k:?} in {spear:?}");
        }
        assert!(!spear.contains(&Wield) && !spear.contains(&SetAlternate));
        assert!(!spear.contains(&DropSome), "a single spear");
        let dagger = kinds(&inv[1], &pack);
        assert_eq!(dagger[0], Wield);
        assert!(dagger.contains(&SwapWeapons) && !dagger.contains(&Force));
        let rows = actions_for(&inv[0], &pack);
        assert!(rows[0].default && rows.iter().skip(1).all(|r| !r.default));
        assert_eq!(rows[0].keys, "w -");
        assert_eq!(rows[0].label_key, "item.unwield");
        assert_eq!(default_action(&inv[1], &pack), Some(Wield));
    }

    #[test]
    fn a_loose_weapon_can_become_the_alternate_in_two_actions() {
        let axe = item('f', ')', 1, &[], "an axe");
        let pack = pack_of(std::slice::from_ref(&axe));
        let rows = actions_for(&axe, &pack);
        assert_eq!(rows[0].kind, Wield);
        let alt = rows.iter().find(|r| r.kind == SetAlternate).unwrap();
        assert!(alt.two_actions);
        assert!(
            !kinds(&axe, &pack).contains(&TwoWeapon),
            "no main and alternate"
        );
        assert!(!kinds(&axe, &pack).contains(&Apply));
    }

    #[test]
    fn missiles_go_to_the_quiver_and_fire_from_it() {
        let arrows = item('c', ')', 12, &[], "12 +0 arrows");
        let pack = pack_of(std::slice::from_ref(&arrows));
        let k = kinds(&arrows, &pack);
        assert_eq!(k[0], Quiver);
        assert!(k.contains(&Wield) && k.contains(&DropSome) && k.contains(&Split));
        let quivered = item('c', ')', 12, &[Slot::Quiver], "12 +0 arrows (in quiver)");
        let k = kinds(&quivered, &pack);
        assert_eq!(k[0], Fire);
        assert!(!k.contains(&Quiver));
        for text in [
            "6 shuriken",
            "4 throwing stars",
            "10 crossbow bolts",
            "3 darts",
            "20 bamboo arrows",
            "a boomerang",
            "7 ya",
        ] {
            let m = item('d', ')', 2, &[], text);
            assert_eq!(kinds(&m, &pack)[0], Quiver, "{text}");
        }
    }

    #[test]
    fn polearms_and_whips_are_applied_by_their_look() {
        for text in [
            "a vulgar polearm",
            "an angled poleaxe",
            "a pole cleaver",
            "a +0 halberd",
            "a lance",
            "a bullwhip",
            "a broad pick",
            "a hooked polearm named Hook",
        ] {
            let w = item('a', ')', 1, &[], text);
            assert!(kinds(&w, &pack_of(&[])).contains(&Apply), "{text}");
        }
        let sword = item('a', ')', 1, &[], "a long sword");
        assert!(!kinds(&sword, &pack_of(&[])).contains(&Apply));
    }

    #[test]
    fn worn_things_come_off_and_others_go_on() {
        let pack = pack_of(&[]);
        let shield = item('c', '[', 1, &[Slot::Shield], "a small shield (being worn)");
        assert_eq!(kinds(&shield, &pack)[0], TakeOff);
        let rows = actions_for(&shield, &pack);
        assert!(rows.iter().find(|r| r.kind == Drop).unwrap().two_actions);
        let cloak = item('d', '[', 1, &[], "a faded pall");
        assert_eq!(kinds(&cloak, &pack)[0], Wear);
        let ring = item('f', '=', 1, &[], "a jade ring");
        assert_eq!(kinds(&ring, &pack)[..3], [PutOn, PutOnLeft, PutOnRight]);
        let on = item('f', '=', 1, &[Slot::LeftRing], "a jade ring (on left hand)");
        assert_eq!(kinds(&on, &pack)[0], Remove);
        assert!(!kinds(&on, &pack).contains(&PutOnLeft));
        let amulet = item('g', '"', 1, &[], "a circular amulet");
        assert_eq!(kinds(&amulet, &pack)[0], PutOn);
        let worn = item(
            'g',
            '"',
            1,
            &[Slot::Amulet],
            "a circular amulet (being worn)",
        );
        assert_eq!(kinds(&worn, &pack)[0], Remove);
        let blind = item('h', '(', 1, &[], "a blindfold");
        assert_eq!(kinds(&blind, &pack)[0], PutOn);
        let on = item('h', '(', 1, &[Slot::Eyes], "a blindfold (being worn)");
        assert_eq!(kinds(&on, &pack)[0], Remove);
    }

    #[test]
    fn each_class_has_its_use() {
        let pack = pack_of(&[]);
        let first = |class, text: &str| kinds(&item('a', class, 1, &[], text), &pack)[0];
        assert_eq!(first('%', "an uncursed food ration"), Eat);
        assert_eq!(first('!', "a bubbly potion"), Quaff);
        assert_eq!(first('?', "a scroll labeled ZELGO MER"), Read);
        assert_eq!(first('+', "a dog-eared spellbook"), Read);
        assert_eq!(first('/', "an oak wand"), Zap);
        assert_eq!(first('(', "an uncursed oil lamp"), Apply);
        let wand = kinds(&item('a', '/', 1, &[], "an oak wand"), &pack);
        assert!(wand.contains(&Engrave) && wand.contains(&Break));
        let potion = kinds(&item('a', '!', 2, &[], "2 bubbly potions"), &pack);
        assert_eq!(
            potion.iter().filter(|k| **k == Dip).count(),
            1,
            "no doubles"
        );
        // gems and gold have no double-click
        let gem = item('a', '*', 1, &[], "a gray stone");
        assert_eq!(default_action(&gem, &pack), None);
        assert!(kinds(&gem, &pack).contains(&Rub));
        let glass = item('a', '*', 1, &[], "a white gem");
        assert!(!kinds(&glass, &pack).contains(&Rub));
        let gold = item('$', '$', 40, &[], "40 gold pieces");
        assert_eq!(kinds(&gold, &pack), [DropSome, Throw]);
        assert_eq!(default_action(&gold, &pack), None);
        let boulder = item('a', '`', 1, &[], "a boulder");
        assert_eq!(default_action(&boulder, &pack), None);
        assert!(kinds(&boulder, &pack).contains(&Drop));
    }

    #[test]
    fn tools_wield_tip_and_apply_by_their_look() {
        let pack = pack_of(&[]);
        let pick = kinds(&item('a', '(', 1, &[], "a +0 pick-axe"), &pack);
        assert_eq!(pick[0], Apply);
        assert!(pick.contains(&Wield));
        let held = kinds(
            &item(
                'a',
                '(',
                1,
                &[Slot::Weapon],
                "a +0 pick-axe (weapon in hand)",
            ),
            &pack,
        );
        assert!(held.contains(&Unwield) && !held.contains(&Wield));
        let bag = kinds(&item('b', '(', 1, &[], "a bag"), &pack);
        assert!(bag.contains(&Tip));
        let boxed = kinds(&item('b', '(', 1, &[], "a locked large box"), &pack);
        assert!(boxed.contains(&Tip));
        let lamp = kinds(&item('e', '(', 1, &[], "an uncursed oil lamp (lit)"), &pack);
        assert!(!lamp.contains(&Tip) && !lamp.contains(&Wield));
    }

    #[test]
    fn nothing_depends_on_the_true_identity() {
        // two stacks of the same look act the same whatever they are
        let pack = pack_of(&[]);
        let a = item('a', '!', 1, &[], "a bubbly potion");
        let b = item('b', '!', 1, &[], "a bubbly potion called healing");
        assert_eq!(kinds(&a, &pack), kinds(&b, &pack));
    }

    #[test]
    fn labels_and_keys_cover_every_kind() {
        for k in [
            Wield,
            Unwield,
            SetAlternate,
            SwapWeapons,
            Quiver,
            EmptyQuiver,
            Fire,
            Throw,
            Apply,
            Wear,
            TakeOff,
            PutOn,
            PutOnLeft,
            PutOnRight,
            Remove,
            Eat,
            Quaff,
            Read,
            Zap,
            Engrave,
            Break,
            Drop,
            DropSome,
            Adjust,
            Split,
            Name,
            Call,
            Dip,
            TwoWeapon,
            Force,
            Rub,
            Tip,
        ] {
            assert!(k.label_key().starts_with("item."));
            assert!(!k.keys().is_empty());
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<ItemActionKind>(&json).unwrap(), k);
        }
        assert_eq!(serde_json::to_string(&TakeOff).unwrap(), "\"take_off\"");
        assert!(Dip.needs_choice() && Adjust.needs_choice() && !Wield.needs_choice());
    }
}
