//! The inventory grid: NetHack's pack order (classes by `packorder`, then
//! letters) and the panel's filters.

use nh_protocol::{InvItem, Slot};
use serde::{Deserialize, Serialize};

use crate::{ItemQuestion, Pack};

/// NetHack's default `packorder`, and venom last.
pub const PACK_ORDER: &str = "$\")[%?+!=/(*`0_.";

/// Where a class goes in the pack order (unknown classes last).
pub fn class_rank(class: char) -> usize {
    PACK_ORDER.find(class).unwrap_or(PACK_ORDER.len())
}

/// Letters in inventory order: a–z, A–Z, then '#' (the overflow).
fn letter_rank(c: char) -> u32 {
    match c {
        '$' => 0,
        'a'..='z' => 1 + (c as u32 - 'a' as u32),
        'A'..='Z' => 27 + (c as u32 - 'A' as u32),
        _ => 60,
    }
}

/// A filter tab of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvFilter {
    #[default]
    All,
    /// Selection mode only: what the question suggests.
    Suggested,
    Weapons,
    Armor,
    /// Rings and amulets.
    Accessories,
    Tools,
    Food,
    Potions,
    ScrollsAndBooks,
    Wands,
    /// Gems, rocks, boulders, statues, balls, chains, venom.
    GemsAndOther,
    /// Worn, wielded, quivered, lit or in use.
    Equipped,
}

impl InvFilter {
    /// The tabs in order (Suggested comes first in selection mode).
    pub const TABS: [InvFilter; 11] = [
        InvFilter::All,
        InvFilter::Weapons,
        InvFilter::Armor,
        InvFilter::Accessories,
        InvFilter::Tools,
        InvFilter::Food,
        InvFilter::Potions,
        InvFilter::ScrollsAndBooks,
        InvFilter::Wands,
        InvFilter::GemsAndOther,
        InvFilter::Equipped,
    ];

    /// The class symbols of the tab's tooltip.
    pub fn classes(self) -> &'static str {
        match self {
            InvFilter::Weapons => ")",
            InvFilter::Armor => "[",
            InvFilter::Accessories => "=\"",
            InvFilter::Tools => "(",
            InvFilter::Food => "%",
            InvFilter::Potions => "!",
            InvFilter::ScrollsAndBooks => "?+",
            InvFilter::Wands => "/",
            InvFilter::GemsAndOther => "*`0_.",
            InvFilter::All | InvFilter::Suggested | InvFilter::Equipped => "",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            InvFilter::All => "inv.all",
            InvFilter::Suggested => "inv.suggested",
            InvFilter::Weapons => "inv.weapons",
            InvFilter::Armor => "inv.armor",
            InvFilter::Accessories => "inv.accessories",
            InvFilter::Tools => "inv.tools",
            InvFilter::Food => "inv.food",
            InvFilter::Potions => "inv.potions",
            InvFilter::ScrollsAndBooks => "inv.scrolls_and_books",
            InvFilter::Wands => "inv.wands",
            InvFilter::GemsAndOther => "inv.gems_and_other",
            InvFilter::Equipped => "inv.equipped",
        }
    }

    /// Whether the item shows under this tab; `question` is the getobj
    /// question of selection mode (Suggested shows everything without one,
    /// and with "[*]").
    pub fn shows(self, item: &InvItem, question: Option<&ItemQuestion>) -> bool {
        match self {
            InvFilter::All => true,
            InvFilter::Suggested => question.is_none_or(|q| q.all || q.suggests(item.letter)),
            InvFilter::Equipped => equipped(item),
            other => other.classes().contains(item.class),
        }
    }
}

/// Worn, wielded, quivered, the alternate weapon, lit, a leash in use.
pub fn equipped(item: &InvItem) -> bool {
    item.slots.iter().any(|s| !matches!(s, Slot::Other(_)))
        || item.lit
        || item.text.ends_with("(in use)")
}

/// A cell of the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridCell<'p> {
    /// The '-' pseudo-item of selection mode (bare hands, fingers,
    /// nothing), first when the question offers it.
    Hands,
    Item {
        item: &'p InvItem,
        /// The first cell of its class: it shows the class glyph.
        class_start: bool,
    },
}

/// The cells under a filter, packed in NetHack's order: classes by
/// [`PACK_ORDER`], letters within a class; `search` keeps the items whose
/// name contains it (any case).
pub fn grid<'p>(
    pack: &'p Pack,
    filter: InvFilter,
    question: Option<&ItemQuestion>,
    search: &str,
) -> Vec<GridCell<'p>> {
    let search = search.trim().to_lowercase();
    let mut items: Vec<&InvItem> = pack
        .items()
        .iter()
        .filter(|i| filter.shows(i, question))
        .filter(|i| search.is_empty() || i.text.to_lowercase().contains(&search))
        .collect();
    items.sort_by_key(|i| (class_rank(i.class), letter_rank(i.letter)));
    let mut cells = Vec::with_capacity(items.len() + 1);
    if question.is_some_and(|q| q.hands) && matches!(filter, InvFilter::All | InvFilter::Suggested)
    {
        cells.push(GridCell::Hands);
    }
    let mut last = None;
    for item in items {
        cells.push(GridCell::Item {
            item,
            class_start: last != Some(item.class),
        });
        last = Some(item.class);
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_item_question;
    use nh_protocol::Inventory;

    fn inv(letter: char, class: char, slots: &[Slot], text: &str) -> InvItem {
        InvItem {
            letter,
            class,
            tile: 1,
            quan: 1,
            slots: slots.to_vec(),
            lit: false,
            text: text.into(),
        }
    }

    fn pack() -> Pack {
        let mut p = Pack::new();
        p.replace(&Inventory {
            items: vec![
                inv(
                    'a',
                    ')',
                    &[Slot::Weapon],
                    "a +1 spear (weapon in right hand)",
                ),
                inv(
                    'b',
                    ')',
                    &[Slot::Alternate],
                    "a +0 dagger (alternate weapon; not wielded)",
                ),
                inv(
                    'c',
                    '[',
                    &[Slot::Shield],
                    "a blessed +3 small shield (being worn)",
                ),
                inv('d', '%', &[], "an uncursed food ration"),
                InvItem {
                    lit: true,
                    ..inv('e', '(', &[], "an uncursed oil lamp (lit)")
                },
                inv('A', '!', &[], "a bubbly potion"),
                inv('f', '!', &[], "a murky potion"),
                inv('$', '$', &[], "13 gold pieces"),
                inv('g', '"', &[], "a circular amulet"),
            ],
            twoweap: false,
        });
        p
    }

    fn letters(cells: &[GridCell]) -> String {
        cells
            .iter()
            .map(|c| match c {
                GridCell::Hands => '-',
                GridCell::Item { item, .. } => item.letter,
            })
            .collect()
    }

    #[test]
    fn the_grid_follows_packorder_then_letters() {
        let p = pack();
        let cells = grid(&p, InvFilter::All, None, "");
        assert_eq!(letters(&cells), "$gabcdfAe");
        let starts: String = cells
            .iter()
            .filter_map(|c| match c {
                GridCell::Item {
                    item,
                    class_start: true,
                } => Some(item.class),
                _ => None,
            })
            .collect();
        assert_eq!(starts, "$\")[%!(");
        assert!(class_rank('?') < class_rank('!'));
        assert_eq!(class_rank('X'), PACK_ORDER.len());
    }

    #[test]
    fn filters_repack_without_holes() {
        let p = pack();
        let f = |filter| letters(&grid(&p, filter, None, ""));
        assert_eq!(f(InvFilter::Weapons), "ab");
        assert_eq!(f(InvFilter::Potions), "fA");
        assert_eq!(f(InvFilter::Accessories), "g");
        assert_eq!(f(InvFilter::Equipped), "abce");
        assert_eq!(f(InvFilter::Wands), "");
        assert_eq!(f(InvFilter::GemsAndOther), "");
        assert_eq!(letters(&grid(&p, InvFilter::All, None, "POTION")), "fA");
        assert_eq!(letters(&grid(&p, InvFilter::Weapons, None, "dag")), "b");
        for tab in InvFilter::TABS {
            assert!(tab.label_key().starts_with("inv."));
        }
    }

    #[test]
    fn selection_mode_suggests_and_offers_hands() {
        let p = pack();
        let q = parse_item_question("What do you want to wield? [- ab or ?*]").unwrap();
        assert_eq!(
            letters(&grid(&p, InvFilter::Suggested, Some(&q), "")),
            "-ab"
        );
        assert_eq!(
            letters(&grid(&p, InvFilter::All, Some(&q), "")),
            "-$gabcdfAe"
        );
        assert_eq!(letters(&grid(&p, InvFilter::Food, Some(&q), "")), "d");
        let any = parse_item_question("What do you want to eat? [*]").unwrap();
        assert_eq!(
            letters(&grid(&p, InvFilter::Suggested, Some(&any), "")),
            "$gabcdfAe"
        );
    }
}
