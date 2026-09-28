//! The hero's inventory, as the engine's `inventory` notice describes it:
//! letters, appearance tiles, worn slots and doname() text. Nothing here
//! knows more than the character: what the text says is all there is.

use nh_protocol::{InvItem, Inventory, Slot};
use serde::{Deserialize, Serialize};

/// The current inventory, replaced wholesale by each notice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pack {
    items: Vec<InvItem>,
    twoweap: bool,
    received: bool,
    changed: bool,
}

impl Pack {
    pub fn new() -> Pack {
        Pack::default()
    }

    /// One `inventory` notice: the whole inventory as it is now.
    pub fn replace(&mut self, inv: &Inventory) {
        self.changed |= !self.received || self.items != inv.items || self.twoweap != inv.twoweap;
        self.items = inv.items.clone();
        self.twoweap = inv.twoweap;
        self.received = true;
    }

    /// In inventory order.
    pub fn items(&self) -> &[InvItem] {
        &self.items
    }

    /// Two-weapon combat: the alternate weapon is in the off hand.
    pub fn twoweap(&self) -> bool {
        self.twoweap
    }

    /// Whether any notice has arrived yet (an empty pack is still news).
    pub fn received(&self) -> bool {
        self.received
    }

    /// Whether the pack changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn by_letter(&self, letter: char) -> Option<&InvItem> {
        self.items.iter().find(|i| i.letter == letter)
    }

    /// The first item with this key: what a UI binding points at now.
    pub fn by_key(&self, key: &ItemKey) -> Option<&InvItem> {
        self.items.iter().find(|i| ItemKey::of(i) == *key)
    }

    pub fn in_slot(&self, slot: &Slot) -> Option<&InvItem> {
        self.items.iter().find(|i| i.slots.contains(slot))
    }

    /// The weapon in the main hand.
    pub fn wielded(&self) -> Option<&InvItem> {
        self.in_slot(&Slot::Weapon)
    }

    /// The shield, or under two-weapon combat the alternate weapon.
    pub fn offhand(&self) -> Option<&InvItem> {
        self.in_slot(&Slot::Shield).or_else(|| {
            if self.twoweap {
                self.in_slot(&Slot::Alternate)
            } else {
                None
            }
        })
    }

    /// Light sources burning now.
    pub fn lit(&self) -> impl Iterator<Item = &InvItem> {
        self.items.iter().filter(|i| i.lit)
    }
}

/// A stable identity for an item in UI bindings: its appearance tile and
/// the name's stem (see [`ItemName::stem`]). It survives a new letter,
/// wielding and wearing, use of charges, a stack growing or shrinking, and
/// learning its curse status or enchantment; it changes when the item gets
/// a new name (identified, called, named), as the item then reads as
/// another one to the player too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemKey {
    pub tile: i32,
    pub stem: String,
}

impl ItemKey {
    pub fn of(item: &InvItem) -> ItemKey {
        ItemKey {
            tile: item.tile,
            stem: parse_item_name(&item.text).stem,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Buc {
    Blessed,
    Uncursed,
    Cursed,
}

/// Wand charges as doname() shows them once known: "(1:4)".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Charges {
    pub recharged: i32,
    pub left: i32,
}

/// What a doname() string says the character knows about an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemName {
    /// The count in front: 1 for "a", "an" and "the", None for "some".
    pub count: Option<i64>,
    pub buc: Option<Buc>,
    /// "+1", "-2": known for weapons, armor and some rings.
    pub enchantment: Option<i32>,
    pub charges: Option<Charges>,
    /// The name without the count or article, without the words that
    /// change over the item's life (curse status, enchantment, erosion,
    /// greased, poisoned, locked, partly eaten...), without the "containing
    /// N items" and parenthesized state suffixes ("(weapon in right hand)",
    /// "(being worn)", "(lit)", "(0:4)", "(unpaid, 5 zorkmids)"...), and in
    /// the singular: "potions of healing" is "potion of healing".
    pub stem: String,
}

/// The words doname() and xname() put before the name for an item's
/// state; "partly" comes with "eaten" or "used".
const STATE_WORDS: [&str; 26] = [
    "empty",
    "blessed",
    "uncursed",
    "cursed",
    "trapped",
    "broken",
    "locked",
    "unlocked",
    "greased",
    "poisoned",
    "very",
    "thoroughly",
    "rusty",
    "burnt",
    "cracked",
    "corroded",
    "rotted",
    "fixed",
    "rustproof",
    "corrodeproof",
    "fireproof",
    "tempered",
    "rotproof",
    "partly",
    "eaten",
    "used",
];

/// Read a doname() string: "2 blessed +0 daggers (in quiver)".
pub fn parse_item_name(text: &str) -> ItemName {
    let mut rest = text.trim();
    let mut charges = None;
    // the parenthesized suffixes, last first
    while let Some(open) = rest.strip_suffix(')').and_then(|r| r.rfind(" (")) {
        let group = &rest[open + 2..rest.len() - 1];
        if charges.is_none() {
            charges = parse_charges(group);
        }
        rest = rest[..open].trim_end();
    }
    if let Some(at) = rest.rfind(" containing ")
        && rest[at + 12..]
            .split(' ')
            .next()
            .is_some_and(|n| n.parse::<u32>().is_ok())
    {
        rest = &rest[..at];
    }

    let mut words = rest.split(' ').peekable();
    let count = match words.peek().copied() {
        Some("a" | "an" | "the") => {
            words.next();
            Some(1)
        }
        Some("some") => {
            words.next();
            None
        }
        Some(w) => match w.parse::<i64>() {
            Ok(n) => {
                words.next();
                Some(n)
            }
            Err(_) => Some(1),
        },
        None => Some(1),
    };
    let plural = count != Some(1);

    let (mut buc, mut enchantment) = (None, None);
    while let Some(&w) = words.peek() {
        match w {
            "blessed" => buc = Some(Buc::Blessed),
            "uncursed" => buc = Some(Buc::Uncursed),
            "cursed" => buc = Some(Buc::Cursed),
            _ if STATE_WORDS.contains(&w) => {}
            _ => match parse_enchantment(w) {
                Some(e) => enchantment = Some(e),
                None => break,
            },
        }
        words.next();
    }
    let name = words.collect::<Vec<_>>().join(" ");
    let stem = if plural { singular(&name) } else { name };
    ItemName {
        count,
        buc,
        enchantment,
        charges,
        stem,
    }
}

/// "+1", "-2", "+0"; the sign is always there.
fn parse_enchantment(word: &str) -> Option<i32> {
    let digits = word.strip_prefix('+').or_else(|| word.strip_prefix('-'))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// "1:4", "0:-1".
fn parse_charges(group: &str) -> Option<Charges> {
    let (a, b) = group.split_once(':')?;
    Some(Charges {
        recharged: a.parse().ok()?,
        left: b.parse().ok()?,
    })
}

/// The words after which makeplural() leaves the rest of a name alone:
/// "potions of healing", "scrolls labeled FOO".
const COMPOUNDS: [&str; 4] = [" of ", " labeled ", " called ", " named "];

/// Undo makeplural() for the names inventory items have: only the head
/// noun changes ("cloves of garlic", "knives", "eucalyptus leaves").
fn singular(name: &str) -> String {
    let split = COMPOUNDS
        .iter()
        .filter_map(|c| name.find(c))
        .min()
        .unwrap_or(name.len());
    let (head, tail) = name.split_at(split);
    let (lead, last) = match head.rfind(' ') {
        Some(i) => head.split_at(i + 1),
        None => ("", head),
    };
    format!("{lead}{}{tail}", singular_word(last))
}

fn singular_word(w: &str) -> String {
    const IRREGULAR: [(&str, &str); 4] = [
        ("knives", "knife"),
        ("staves", "staff"),
        ("teeth", "tooth"),
        ("feet", "foot"),
    ];
    if let Some((_, one)) = IRREGULAR.iter().find(|(many, _)| *many == w) {
        return one.to_string();
    }
    if let Some(stem) = w.strip_suffix("ies")
        && !matches!(w, "cookies" | "pies" | "zombies")
    {
        return format!("{stem}y");
    }
    if let Some(stem) = w.strip_suffix("ves")
        && stem.ends_with(['l', 'r', 'a', 'e', 'i', 'o', 'u'])
        && w != "cloves"
    {
        return format!("{stem}f");
    }
    for es in ["ches", "shes", "sses", "xes", "zes", "oes"] {
        if w.ends_with(es) {
            return w[..w.len() - 2].to_string();
        }
    }
    if w.ends_with("us") || w.ends_with("ss") {
        return w.to_string();
    }
    if let Some(stem) = w.strip_suffix("men") {
        return format!("{stem}man");
    }
    w.strip_suffix('s').unwrap_or(w).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(letter: char, tile: i32, slots: &[Slot], lit: bool, text: &str) -> InvItem {
        InvItem {
            letter,
            class: ')',
            tile,
            quan: 1,
            slots: slots.to_vec(),
            lit,
            text: text.into(),
        }
    }

    /// The Valkyrie's starting inventory, as the engine sends it.
    fn valkyrie() -> Inventory {
        Inventory {
            items: vec![
                item(
                    'a',
                    816,
                    &[Slot::Weapon],
                    false,
                    "a +1 spear (weapon in right hand)",
                ),
                item(
                    'b',
                    823,
                    &[Slot::Alternate],
                    false,
                    "a +0 dagger (alternate weapon; not wielded)",
                ),
                item(
                    'c',
                    941,
                    &[Slot::Shield],
                    false,
                    "a blessed +3 small shield (being worn)",
                ),
                item('d', 1084, &[], false, "an uncursed food ration"),
                item('e', 1018, &[], true, "an uncursed oil lamp (lit)"),
            ],
            twoweap: false,
        }
    }

    #[test]
    fn a_pack_finds_items_by_letter_slot_and_hand() {
        let mut pack = Pack::new();
        assert!(!pack.received());
        pack.replace(&valkyrie());
        assert!(pack.received());
        assert_eq!(pack.items().len(), 5);
        assert!(pack.by_letter('d').unwrap().text.contains("food ration"));
        assert!(pack.by_letter('z').is_none());
        assert_eq!(pack.wielded().unwrap().letter, 'a');
        assert_eq!(pack.offhand().unwrap().letter, 'c');
        assert_eq!(pack.in_slot(&Slot::Alternate).unwrap().letter, 'b');
        assert!(pack.in_slot(&Slot::Cloak).is_none());
        let lit: Vec<char> = pack.lit().map(|i| i.letter).collect();
        assert_eq!(lit, vec!['e']);
    }

    #[test]
    fn under_twoweap_the_alternate_weapon_is_the_offhand() {
        let mut inv = valkyrie();
        inv.items.remove(2); // the shield comes off first
        inv.items[1].text = "a +0 dagger (wielded in left hand)".into();
        inv.twoweap = true;
        let mut pack = Pack::new();
        pack.replace(&inv);
        assert_eq!(pack.offhand().unwrap().letter, 'b');
        inv.twoweap = false;
        pack.replace(&inv);
        assert!(pack.offhand().is_none());
    }

    #[test]
    fn a_notice_replaces_the_pack_and_flags_only_real_changes() {
        let mut pack = Pack::new();
        pack.replace(&Inventory::default());
        assert!(pack.take_changed(), "the first notice is news, even empty");
        pack.replace(&valkyrie());
        assert!(pack.take_changed());
        assert!(!pack.take_changed());
        pack.replace(&valkyrie());
        assert!(!pack.take_changed(), "the same inventory again");
        let mut less = valkyrie();
        less.items.truncate(2);
        pack.replace(&less);
        assert!(pack.take_changed());
        assert!(pack.by_letter('c').is_none());
    }

    #[test]
    fn a_key_survives_letters_wielding_and_counts() {
        let spear = item(
            'a',
            816,
            &[Slot::Weapon],
            false,
            "a +1 spear (weapon in right hand)",
        );
        let moved = item('f', 816, &[], false, "an uncursed +1 spear");
        assert_eq!(ItemKey::of(&spear), ItemKey::of(&moved));
        assert_eq!(ItemKey::of(&spear).stem, "spear");
        // same name, another look: another item
        let other = item('g', 817, &[], false, "a +1 spear");
        assert_ne!(ItemKey::of(&spear), ItemKey::of(&other));
        let two = item('q', 850, &[], false, "2 uncursed potions of healing");
        let one = item('q', 850, &[], false, "an uncursed potion of healing");
        assert_eq!(ItemKey::of(&two), ItemKey::of(&one));
        let mut pack = Pack::new();
        let mut inv = valkyrie();
        inv.items.push(moved);
        pack.replace(&inv);
        assert_eq!(pack.by_key(&ItemKey::of(&spear)).unwrap().letter, 'a');
    }

    #[test]
    fn names_tell_count_curse_status_enchantment_and_charges() {
        let n = parse_item_name("a blessed +3 small shield (being worn)");
        assert_eq!(n.count, Some(1));
        assert_eq!(n.buc, Some(Buc::Blessed));
        assert_eq!(n.enchantment, Some(3));
        assert_eq!(n.charges, None);
        assert_eq!(n.stem, "small shield");

        let n = parse_item_name("a wand of striking (0:4)");
        assert_eq!(n.buc, None);
        assert_eq!(n.enchantment, None);
        assert_eq!(
            n.charges,
            Some(Charges {
                recharged: 0,
                left: 4
            })
        );
        assert_eq!(n.stem, "wand of striking");

        let n = parse_item_name("a cursed wand of digging (1:-1)");
        assert_eq!(n.buc, Some(Buc::Cursed));
        assert_eq!(n.charges.unwrap().left, -1);

        let n = parse_item_name("12 uncursed -1 darts (in quiver)");
        assert_eq!(n.count, Some(12));
        assert_eq!(n.buc, Some(Buc::Uncursed));
        assert_eq!(n.enchantment, Some(-1));
        assert_eq!(n.stem, "dart");

        let n = parse_item_name("an uncursed +2 ring of protection (on left hand)");
        assert_eq!(n.enchantment, Some(2));
        assert_eq!(n.stem, "ring of protection");

        // nothing known: no curse status, no enchantment
        let n = parse_item_name("a scroll labeled ZELGO MER");
        assert_eq!((n.buc, n.enchantment), (None, None));
        assert_eq!(n.stem, "scroll labeled ZELGO MER");
        assert_eq!(parse_item_name("some gold pieces").count, None);
    }

    #[test]
    fn stems_drop_state_words_and_suffixes() {
        for (text, stem) in [
            ("a +0 dagger (alternate weapon; not wielded)", "dagger"),
            ("a +0 dagger (wielded in left hand)", "dagger"),
            (
                "an uncursed +0 two-handed sword (weapon in hands)",
                "two-handed sword",
            ),
            (
                "an uncursed very rusty +0 long sword (weapon in right hand)",
                "long sword",
            ),
            (
                "a blessed rustproof +2 pair of speed boots (being worn)",
                "pair of speed boots",
            ),
            (
                "an uncursed greased +0 leather armor (being worn; slippery)",
                "leather armor",
            ),
            ("an uncursed oil lamp (lit)", "oil lamp"),
            ("a partly used wax candle (lit)", "wax candle"),
            ("an uncursed partly eaten food ration", "food ration"),
            ("a locked large box", "large box"),
            ("an uncursed bag containing 3 items", "bag"),
            ("a bag of holding (unpaid, 100 zorkmids)", "bag of holding"),
            (
                "an uncursed candelabrum (3 of 7 candles attached)",
                "candelabrum",
            ),
            ("a heavy iron ball (chained to you)", "heavy iron ball"),
            ("a jade ring (on left hand)", "jade ring"),
            ("a blindfold (being worn)", "blindfold"),
            (
                "a +0 bow named Longbow (weapon in left hand)",
                "bow named Longbow",
            ),
            (
                "an elven mithril-coat named Bob",
                "elven mithril-coat named Bob",
            ),
            ("the uncursed Amulet of Yendor", "Amulet of Yendor"),
            ("13 gold pieces", "gold piece"),
            ("2 cloves of garlic", "clove of garlic"),
            ("3 uncursed eucalyptus leaves", "eucalyptus leaf"),
            ("4 knives", "knife"),
            ("2 fortune cookies", "fortune cookie"),
            ("2 lichen corpses", "lichen corpse"),
            (
                "3 uncursed poisoned +0 orcish arrows (in quiver)",
                "orcish arrow",
            ),
            ("2 scrolls labeled FOO", "scroll labeled FOO"),
            (
                "5 worthless pieces of blue glass",
                "worthless piece of blue glass",
            ),
            ("2 uncursed tins of kobold meat", "tin of kobold meat"),
            ("6 shuriken (at the ready)", "shuriken"),
        ] {
            assert_eq!(parse_item_name(text).stem, stem, "{text}");
        }
    }
}
