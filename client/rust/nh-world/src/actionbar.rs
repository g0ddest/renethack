//! The action bar: ten slots on the top-row digits, each bound to an item
//! and an action, a spell, or a command; kept per character in
//! `<save>.rhui.json` with the key profile.

use std::cmp::Reverse;

use nh_protocol::InvItem;
use serde::{Deserialize, Serialize};

use crate::{
    ItemActionKind, ItemKey, KeyProfile, Macro, Order, Pack, default_action, parse_item_name,
};

/// Slots on the bar: `1`–`9`, `0`.
pub const BAR_SLOTS: usize = 10;

/// A command a slot can hold (the Commands palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarCommand {
    Search,
    /// The client's rest order (F5): search until HP and Pw are full.
    Rest,
    Wait,
    Kick,
    PickUp,
    LookHere,
    Farlook,
    Travel,
    Pray,
    Offer,
    Chat,
    Loot,
    Force,
    Sit,
    TurnUndead,
    Jump,
    Ride,
    Untrap,
    Open,
    Close,
    Pay,
    Fire,
    Swap,
    TwoWeapon,
    Enhance,
    Terrain,
    Overview,
    Attributes,
    Discoveries,
    /// `Z`: the spell menu.
    Cast,
    Throw,
    Engrave,
    Up,
    Down,
}

/// How a command reaches the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKeys {
    /// A key, the same with and without number_pad.
    Key(char),
    /// An extended command by name (user key options cannot break it).
    Ext(&'static str),
    /// A client order.
    Rest,
}

impl BarCommand {
    /// The palette, in its order.
    pub const ALL: [BarCommand; 34] = {
        use BarCommand::*;
        [
            Search,
            Rest,
            Wait,
            Kick,
            PickUp,
            LookHere,
            Farlook,
            Travel,
            Pray,
            Offer,
            Chat,
            Loot,
            Force,
            Sit,
            TurnUndead,
            Jump,
            Ride,
            Untrap,
            Open,
            Close,
            Pay,
            Fire,
            Swap,
            TwoWeapon,
            Enhance,
            Terrain,
            Overview,
            Attributes,
            Discoveries,
            Cast,
            Throw,
            Engrave,
            Up,
            Down,
        ]
    };

    pub fn keys(self) -> CommandKeys {
        use BarCommand::*;
        use CommandKeys::{Ext, Key};
        match self {
            Search => Key('s'),
            Rest => CommandKeys::Rest,
            Wait => Key('.'),
            Kick => Key('\u{4}'),
            PickUp => Key(','),
            LookHere => Key(':'),
            Farlook => Key(';'),
            Travel => Key('_'),
            Pray => Ext("pray"),
            Offer => Ext("offer"),
            Chat => Ext("chat"),
            Loot => Ext("loot"),
            Force => Ext("force"),
            Sit => Ext("sit"),
            TurnUndead => Ext("turn"),
            Jump => Ext("jump"),
            Ride => Ext("ride"),
            Untrap => Ext("untrap"),
            Open => Key('o'),
            Close => Key('c'),
            Pay => Key('p'),
            Fire => Key('f'),
            Swap => Key('x'),
            TwoWeapon => Ext("twoweapon"),
            Enhance => Ext("enhance"),
            Terrain => Ext("terrain"),
            Overview => Ext("overview"),
            Attributes => Ext("attributes"),
            Discoveries => Ext("known"),
            Cast => Key('Z'),
            Throw => Key('t'),
            Engrave => Key('E'),
            Up => Key('<'),
            Down => Key('>'),
        }
    }

    /// The key of the label in the text tables: "cmd.search".
    pub fn label_key(self) -> String {
        let name = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        format!("cmd.{name}")
    }

    /// The key hint: "s", "^D", "#pray", "F5".
    pub fn hint(self) -> String {
        match self.keys() {
            CommandKeys::Key('\u{4}') => "^D".into(),
            CommandKeys::Key(c) => c.to_string(),
            CommandKeys::Ext(name) => format!("#{name}"),
            CommandKeys::Rest => "F5".into(),
        }
    }
}

/// What a slot holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlotBinding {
    /// An item by its identity, and what to do with it. `letter` and
    /// `text` are where and how it was last seen, to tell apart items
    /// that look the same.
    Item {
        key: ItemKey,
        action: ItemActionKind,
        letter: char,
        #[serde(default)]
        text: String,
    },
    /// A spell by its name (letters in the spell menu change).
    Spell {
        name: String,
    },
    Command {
        cmd: BarCommand,
    },
}

impl SlotBinding {
    /// An item with an action (its double-click action, or another row of
    /// its menu).
    pub fn item(item: &InvItem, action: ItemActionKind) -> SlotBinding {
        SlotBinding::Item {
            key: ItemKey::of(item),
            action,
            letter: item.letter,
            text: item.text.clone(),
        }
    }
}

/// How a slot looks now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Ready,
    /// The item is not in the pack: grey, slashed; the binding stays and
    /// picks the item up again when it comes back.
    Gone,
}

/// What the bar view draws for a slot.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotView {
    pub state: SlotState,
    /// The item's appearance tile (the icon), also while it is gone.
    pub tile: Option<i32>,
    /// The item's letter now.
    pub letter: Option<char>,
    /// Stack size, when more than one.
    pub count: Option<i64>,
    /// Wand charges left, when known ("(0:5)" → 5).
    pub charges: Option<i32>,
    /// Tooltip text: the item's doname, or the spell's name.
    pub text: Option<String>,
    /// The action's or the command's label key.
    pub label_key: Option<String>,
    /// NetHack's keys for it, as a hint.
    pub hint: Option<String>,
}

impl SlotView {
    fn empty() -> SlotView {
        SlotView {
            state: SlotState::Empty,
            tile: None,
            letter: None,
            count: None,
            charges: None,
            text: None,
            label_key: None,
            hint: None,
        }
    }
}

/// What pressing a slot does.
#[derive(Debug, Clone, PartialEq)]
pub enum SlotUse {
    /// Run this macro (at the command prompt, after stopping any order).
    Macro(Macro),
    /// Start this client order (rest, a counted search).
    Order(Order),
    /// Nothing: an empty slot, or the item is gone.
    Nothing,
}

/// The ten slots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionBar {
    slots: Vec<Option<SlotBinding>>,
}

impl Default for ActionBar {
    fn default() -> ActionBar {
        ActionBar {
            slots: vec![None; BAR_SLOTS],
        }
    }
}

impl ActionBar {
    pub fn new() -> ActionBar {
        ActionBar::default()
    }

    pub fn get(&self, slot: usize) -> Option<&SlotBinding> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    pub fn set(&mut self, slot: usize, binding: Option<SlotBinding>) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = binding;
        }
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a < self.slots.len() && b < self.slots.len() {
            self.slots.swap(a, b);
        }
    }

    /// The item a slot points at now, as [`rebind`] would find it.
    ///
    /// [`rebind`]: ActionBar::rebind
    pub fn item<'p>(&self, slot: usize, pack: &'p Pack) -> Option<&'p InvItem> {
        match self.get(slot)? {
            SlotBinding::Item {
                key, letter, text, ..
            } => find_item(pack, key, *letter, text),
            _ => None,
        }
    }

    /// Follow the items after an inventory change: a new letter, a new
    /// name (identified, named), a merged or split stack. A slot whose
    /// item is gone keeps its binding. True when a binding changed.
    pub fn rebind(&mut self, pack: &Pack) -> bool {
        let mut changed = false;
        for slot in self.slots.iter_mut() {
            let Some(SlotBinding::Item {
                key, letter, text, ..
            }) = slot
            else {
                continue;
            };
            let Some(found) = find_item(pack, key, *letter, text) else {
                continue;
            };
            let new_key = ItemKey::of(found);
            if *key != new_key || *letter != found.letter || *text != found.text {
                *key = new_key;
                *letter = found.letter;
                *text = found.text.clone();
                changed = true;
            }
        }
        changed
    }

    /// What the view shows for a slot.
    pub fn view(&self, slot: usize, pack: &Pack) -> SlotView {
        let Some(binding) = self.get(slot) else {
            return SlotView::empty();
        };
        let mut v = SlotView::empty();
        v.state = SlotState::Ready;
        match binding {
            SlotBinding::Item {
                key, action, text, ..
            } => {
                v.tile = Some(key.tile);
                v.label_key = Some(action.label_key().to_string());
                v.hint = Some(action.keys().to_string());
                match self.item(slot, pack) {
                    Some(item) => {
                        v.letter = Some(item.letter);
                        v.count = (item.quan > 1).then_some(item.quan);
                        v.charges = parse_item_name(&item.text).charges.map(|c| c.left);
                        v.text = Some(item.text.clone());
                    }
                    None => {
                        v.state = SlotState::Gone;
                        v.text = Some(text.clone());
                    }
                }
            }
            SlotBinding::Spell { name } => {
                v.text = Some(name.clone());
                v.label_key = Some("cmd.cast".into());
                v.hint = Some("Z".into());
            }
            SlotBinding::Command { cmd } => {
                v.label_key = Some(cmd.label_key());
                v.hint = Some(cmd.hint());
            }
        }
        v
    }

    /// What pressing a slot does, with a count typed before it
    /// (`n20` then slot 3 = Search: a Repeat order of 20).
    pub fn activate(
        &self,
        slot: usize,
        pack: &Pack,
        count: Option<u32>,
        profile: KeyProfile,
    ) -> SlotUse {
        match self.get(slot) {
            None => SlotUse::Nothing,
            Some(SlotBinding::Item { action, .. }) => {
                let Some(item) = self.item(slot, pack) else {
                    return SlotUse::Nothing;
                };
                let action = match *action {
                    ItemActionKind::DropSome => ItemActionKind::Drop,
                    a => a,
                };
                Macro::item(action, item, count).map_or(SlotUse::Nothing, SlotUse::Macro)
            }
            Some(SlotBinding::Spell { name }) => SlotUse::Macro(Macro::cast(name)),
            Some(SlotBinding::Command { cmd }) => match (cmd.keys(), count) {
                (CommandKeys::Rest, _) => SlotUse::Order(Order::Rest),
                (CommandKeys::Key(key @ ('s' | '.')), Some(left)) => {
                    SlotUse::Order(Order::Repeat { key, left })
                }
                (CommandKeys::Key(key), count) => {
                    SlotUse::Macro(Macro::key(key as i32, count, profile.number_pad()))
                }
                (CommandKeys::Ext(name), _) => SlotUse::Macro(Macro::ext(name)),
            },
        }
    }

    /// The bar a new character starts with (ui-design §4.4), from the
    /// role's name ("Valkyrie", "val") and the first inventory.
    pub fn default_for(role: &str, pack: &Pack) -> ActionBar {
        use BarCommand::*;
        let cmd = |cmd| Some(SlotBinding::Command { cmd });
        let spell = |name: &str| Some(SlotBinding::Spell { name: name.into() });
        let with = |pred: &dyn Fn(&InvItem, &str) -> bool, action| {
            pack.items()
                .iter()
                .find(|i| pred(i, &parse_item_name(&i.text).stem))
                .map(|i| SlotBinding::item(i, action))
        };
        let named = |name: &'static str, action| {
            with(
                &move |_: &InvItem, stem: &str| stem.starts_with(name),
                action,
            )
        };
        let first_spell = || {
            pack.items().iter().find_map(|i| {
                let stem = parse_item_name(&i.text).stem;
                (i.class == '+')
                    .then(|| stem.strip_prefix("spellbook of ").map(str::to_string))
                    .flatten()
                    .map(|name| SlotBinding::Spell { name })
            })
        };
        let role = role.to_lowercase();
        let role = role.get(..3).unwrap_or(&role);
        let (a, b) = match role {
            "arc" => (
                named("pick-axe", ItemActionKind::Apply),
                named("tinning kit", ItemActionKind::Apply),
            ),
            "bar" => (
                cmd(Fire),
                with(
                    &|i: &InvItem, _: &str| i.class == ')' && i.slots.is_empty(),
                    ItemActionKind::Wield,
                )
                .or_else(|| {
                    with(
                        &|i: &InvItem, _: &str| {
                            i.class == ')' && i.slots.contains(&nh_protocol::Slot::Alternate)
                        },
                        ItemActionKind::Wield,
                    )
                }),
            ),
            "cav" => (cmd(Fire), cmd(Throw)),
            "hea" => (
                spell("healing"),
                named("stethoscope", ItemActionKind::Apply),
            ),
            "kni" => (named("lance", ItemActionKind::Apply), cmd(Ride)),
            "mon" => (first_spell(), cmd(Fire)),
            "pri" => (
                first_spell(),
                with(
                    &|i: &InvItem, stem: &str| i.class == '!' && stem.contains("holy water"),
                    ItemActionKind::Throw,
                ),
            ),
            "ran" | "rog" | "sam" => (cmd(Fire), cmd(Swap)),
            "tou" => (
                named("expensive camera", ItemActionKind::Apply),
                named("scroll of magic mapping", ItemActionKind::Read),
            ),
            "val" => (cmd(Swap), cmd(Fire)),
            "wiz" => (
                spell("force bolt"),
                with(&|i: &InvItem, _: &str| i.class == '/', ItemActionKind::Zap).or(cmd(Swap)),
            ),
            _ => (None, None),
        };
        let food = pack
            .items()
            .iter()
            .find(|i| i.class == '%' && default_action(i, pack) == Some(ItemActionKind::Eat))
            .map(|i| SlotBinding::item(i, ItemActionKind::Eat));
        ActionBar {
            slots: vec![
                a,
                b,
                cmd(Search),
                cmd(Rest),
                cmd(Kick),
                cmd(PickUp),
                cmd(LookHere),
                cmd(Pray),
                cmd(Enhance),
                food,
            ],
        }
    }

    fn normalize(&mut self) {
        self.slots.resize(BAR_SLOTS, None);
    }
}

/// Identity match, first rule that finds something:
/// 1. items with the same key (look and name): the one whose curse status
///    and enchantment agree with the last text, then the one at the
///    letter, then the first in the pack;
/// 2. the item at the letter, when it has the same look (it was
///    identified or named in place);
/// 3. the only item with the same look (identified and moved).
fn find_item<'p>(pack: &'p Pack, key: &ItemKey, letter: char, text: &str) -> Option<&'p InvItem> {
    let was = parse_item_name(text);
    let score = |i: &InvItem| {
        let now = parse_item_name(&i.text);
        let tokens = usize::from(was.buc.is_some() && was.buc == now.buc)
            + usize::from(was.enchantment.is_some() && was.enchantment == now.enchantment);
        (tokens, i.letter == letter)
    };
    let best = pack
        .items()
        .iter()
        .enumerate()
        .filter(|(_, i)| ItemKey::of(i) == *key)
        .max_by_key(|&(n, i)| (score(i), Reverse(n)));
    if let Some((_, i)) = best {
        return Some(i);
    }
    if let Some(i) = pack.by_letter(letter)
        && i.tile == key.tile
    {
        return Some(i);
    }
    let mut same_look = pack.items().iter().filter(|i| i.tile == key.tile);
    match (same_look.next(), same_look.next()) {
        (Some(i), None) => Some(i),
        _ => None,
    }
}

/// What `<save>.rhui.json` keeps for a character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    pub version: u32,
    /// Chosen at creation; a restore passes its number_pad again.
    #[serde(default)]
    pub profile: KeyProfile,
    #[serde(default)]
    pub bar: ActionBar,
    /// The interface's language this character plays in ("en", "ru");
    /// None: the client's own choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl UiState {
    pub const VERSION: u32 = 1;

    pub fn new(profile: KeyProfile) -> UiState {
        UiState {
            version: UiState::VERSION,
            profile,
            bar: ActionBar::new(),
            lang: None,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<UiState, serde_json::Error> {
        let mut s: UiState = serde_json::from_str(text)?;
        s.bar.normalize();
        Ok(s)
    }
}

/// The client's own settings, the same for every character
/// (`<playground>/profile.json`): the last language chosen, the hero of
/// the last game played (the title scene shows them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// The last hero's role, as the engine's code ("Val").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_role: Option<String>,
    /// The last hero is a woman.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_female: Option<bool>,
}

impl Profile {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A broken or missing file is an empty profile.
    pub fn from_json(text: &str) -> Profile {
        serde_json::from_str(text).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{Inventory, Slot};

    fn inv(letter: char, class: char, tile: i32, quan: i64, text: &str) -> InvItem {
        InvItem {
            letter,
            class,
            tile,
            quan,
            slots: vec![],
            lit: false,
            text: text.into(),
        }
    }

    fn pack_of(items: Vec<InvItem>) -> Pack {
        let mut p = Pack::new();
        p.replace(&Inventory {
            items,
            twoweap: false,
        });
        p
    }

    fn valkyrie() -> Vec<InvItem> {
        vec![
            InvItem {
                slots: vec![Slot::Weapon],
                ..inv('a', ')', 816, 1, "a +1 spear (weapon in right hand)")
            },
            InvItem {
                slots: vec![Slot::Alternate],
                ..inv(
                    'b',
                    ')',
                    823,
                    1,
                    "a +0 dagger (alternate weapon; not wielded)",
                )
            },
            InvItem {
                slots: vec![Slot::Shield],
                ..inv('c', '[', 941, 1, "a blessed +3 small shield (being worn)")
            },
            inv('d', '%', 1084, 1, "an uncursed food ration"),
            inv('e', '(', 1018, 1, "an uncursed oil lamp"),
        ]
    }

    fn letter(bar: &ActionBar, slot: usize, pack: &Pack) -> Option<char> {
        bar.item(slot, pack).map(|i| i.letter)
    }

    #[test]
    fn a_bound_item_survives_a_new_letter_and_identification() {
        let potion = inv('f', '!', 900, 2, "2 bubbly potions");
        let mut bar = ActionBar::new();
        bar.set(0, Some(SlotBinding::item(&potion, ItemActionKind::Quaff)));
        let mut items = valkyrie();
        items.push(potion);
        let mut pack = pack_of(items.clone());
        assert_eq!(letter(&bar, 0, &pack), Some('f'));
        // #adjust f -> q
        items[5].letter = 'q';
        pack.replace(&Inventory {
            items: items.clone(),
            twoweap: false,
        });
        assert!(bar.rebind(&pack));
        assert_eq!(letter(&bar, 0, &pack), Some('q'));
        // quaffed one: identified, still at q
        items[5].text = "a potion of healing".into();
        items[5].quan = 1;
        pack.replace(&Inventory {
            items: items.clone(),
            twoweap: false,
        });
        assert!(bar.rebind(&pack));
        assert_eq!(letter(&bar, 0, &pack), Some('q'));
        let Some(SlotBinding::Item { key, .. }) = bar.get(0) else {
            panic!()
        };
        assert_eq!(key.stem, "potion of healing");
        assert!(!bar.rebind(&pack), "nothing new");
        // dropped: gone, the binding stays; picked up at another letter
        let dropped: Vec<InvItem> = items[..5].to_vec();
        pack.replace(&Inventory {
            items: dropped,
            twoweap: false,
        });
        assert!(!bar.rebind(&pack));
        assert_eq!(bar.view(0, &pack).state, SlotState::Gone);
        assert_eq!(bar.view(0, &pack).tile, Some(900));
        assert_eq!(
            bar.activate(0, &pack, None, KeyProfile::Modern),
            SlotUse::Nothing
        );
        items[5].letter = 'g';
        pack.replace(&Inventory {
            items: items.clone(),
            twoweap: false,
        });
        bar.rebind(&pack);
        assert_eq!(letter(&bar, 0, &pack), Some('g'));
        assert_eq!(bar.view(0, &pack).state, SlotState::Ready);
    }

    #[test]
    fn stacks_of_one_look_stay_apart_by_what_the_name_tells() {
        let blessed = inv('f', '!', 900, 2, "2 blessed bubbly potions");
        let cursed = inv('g', '!', 900, 1, "a cursed bubbly potion");
        let mut bar = ActionBar::new();
        bar.set(0, Some(SlotBinding::item(&blessed, ItemActionKind::Quaff)));
        bar.set(1, Some(SlotBinding::item(&cursed, ItemActionKind::Throw)));
        let pack = pack_of(vec![blessed.clone(), cursed.clone()]);
        assert_eq!(letter(&bar, 0, &pack), Some('f'));
        assert_eq!(letter(&bar, 1, &pack), Some('g'));
        // letters swapped: each slot follows its stack
        let swapped = pack_of(vec![
            InvItem {
                letter: 'g',
                ..blessed.clone()
            },
            InvItem {
                letter: 'f',
                ..cursed.clone()
            },
        ]);
        bar.rebind(&swapped);
        assert_eq!(letter(&bar, 0, &swapped), Some('g'));
        assert_eq!(letter(&bar, 1, &swapped), Some('f'));
        // unknown curse status: the one at the letter, else the lowest
        let a = inv('h', '!', 901, 1, "a murky potion");
        let b = inv('i', '!', 901, 1, "a murky potion");
        let mut bar = ActionBar::new();
        bar.set(0, Some(SlotBinding::item(&b, ItemActionKind::Quaff)));
        let pack = pack_of(vec![a.clone(), b.clone()]);
        assert_eq!(letter(&bar, 0, &pack), Some('i'));
        let moved = pack_of(vec![a.clone(), InvItem { letter: 'j', ..b }]);
        assert_eq!(letter(&bar, 0, &moved), Some('h'));
    }

    #[test]
    fn merges_and_lost_armor() {
        let one = inv('m', ')', 830, 5, "5 +0 daggers");
        let mut bar = ActionBar::new();
        bar.set(4, Some(SlotBinding::item(&one, ItemActionKind::Throw)));
        // picked up more: the stack grows
        let pack = pack_of(vec![inv('m', ')', 830, 8, "8 +0 daggers")]);
        assert_eq!(letter(&bar, 4, &pack), Some('m'));
        assert_eq!(bar.view(4, &pack).count, Some(8));
        // polymorph broke the armor: nothing of that look is left
        let mail = inv('c', '[', 950, 1, "an uncursed +0 ring mail (being worn)");
        let mut bar = ActionBar::new();
        bar.set(2, Some(SlotBinding::item(&mail, ItemActionKind::TakeOff)));
        let pack = pack_of(vec![inv('c', '%', 1084, 1, "a food ration")]);
        assert_eq!(bar.item(2, &pack), None);
        assert_eq!(bar.view(2, &pack).state, SlotState::Gone);
    }

    #[test]
    fn slots_show_counts_charges_and_labels() {
        let wand = inv('l', '/', 1200, 1, "a wand of striking (0:5)");
        let mut bar = ActionBar::new();
        bar.set(1, Some(SlotBinding::item(&wand, ItemActionKind::Zap)));
        bar.set(
            2,
            Some(SlotBinding::Command {
                cmd: BarCommand::Kick,
            }),
        );
        bar.set(
            3,
            Some(SlotBinding::Spell {
                name: "force bolt".into(),
            }),
        );
        let pack = pack_of(vec![wand]);
        let v = bar.view(1, &pack);
        assert_eq!(v.charges, Some(5));
        assert_eq!(v.letter, Some('l'));
        assert_eq!(v.count, None);
        assert_eq!(v.label_key.as_deref(), Some("item.zap"));
        let k = bar.view(2, &pack);
        assert_eq!(k.hint.as_deref(), Some("^D"));
        assert_eq!(k.label_key.as_deref(), Some("cmd.kick"));
        assert_eq!(bar.view(3, &pack).text.as_deref(), Some("force bolt"));
        assert_eq!(bar.view(0, &pack).state, SlotState::Empty);
        assert_eq!(bar.view(99, &pack).state, SlotState::Empty);
        bar.swap(1, 0);
        assert_eq!(bar.view(0, &pack).letter, Some('l'));
    }

    #[test]
    fn pressing_a_slot_runs_its_keys() {
        use crate::{MacroRunner, MacroStep, Prompt, World};
        use nh_protocol::Reply;
        let pack = pack_of(valkyrie());
        let bar = ActionBar::default_for("Valkyrie", &pack);
        let w = World::new();
        // slot 3: search; with a count, a Repeat order
        assert_eq!(
            bar.activate(2, &pack, Some(20), KeyProfile::Modern),
            SlotUse::Order(Order::Repeat { key: 's', left: 20 })
        );
        let SlotUse::Macro(m) = bar.activate(2, &pack, None, KeyProfile::Modern) else {
            panic!()
        };
        let mut r = MacroRunner::new();
        assert_eq!(r.start(m), Some(Reply::Key('s' as i32)));
        assert_eq!(
            bar.activate(3, &pack, None, KeyProfile::Modern),
            SlotUse::Order(Order::Rest)
        );
        // slot 0: the food ration, eaten
        let SlotUse::Macro(m) = bar.activate(9, &pack, None, KeyProfile::Modern) else {
            panic!()
        };
        assert_eq!(m.action, Some((ItemActionKind::Eat, 'd')));
        // pray by name
        let SlotUse::Macro(m) = bar.activate(7, &pack, None, KeyProfile::Modern) else {
            panic!()
        };
        let mut r = MacroRunner::new();
        assert_eq!(r.start(m), Some(Reply::Key('#' as i32)));
        assert_eq!(
            r.on_prompt(&Prompt::ExtCmd, &w),
            MacroStep::Reply(Reply::ExtCmd(Some("pray".into())))
        );
        // a spell: Z, then its entry
        let mut bar = ActionBar::new();
        bar.set(
            0,
            Some(SlotBinding::Spell {
                name: "force bolt".into(),
            }),
        );
        let SlotUse::Macro(m) = bar.activate(0, &pack, None, KeyProfile::Modern) else {
            panic!()
        };
        assert_eq!(m, Macro::cast("force bolt"));
        // a counted open: the engine's count
        bar.set(
            1,
            Some(SlotBinding::Command {
                cmd: BarCommand::Open,
            }),
        );
        let SlotUse::Macro(m) = bar.activate(1, &pack, Some(3), KeyProfile::Classic) else {
            panic!()
        };
        assert_eq!(m, Macro::key('o' as i32, Some(3), false));
        assert_eq!(
            bar.activate(5, &pack, None, KeyProfile::Modern),
            SlotUse::Nothing
        );
    }

    #[test]
    fn each_role_starts_with_its_loadout() {
        let pack = pack_of(valkyrie());
        let bar = ActionBar::default_for("Valkyrie", &pack);
        let cmd = |c| Some(SlotBinding::Command { cmd: c });
        assert_eq!(bar.get(0).cloned(), cmd(BarCommand::Swap));
        assert_eq!(bar.get(1).cloned(), cmd(BarCommand::Fire));
        assert_eq!(bar.get(2).cloned(), cmd(BarCommand::Search));
        assert_eq!(bar.get(3).cloned(), cmd(BarCommand::Rest));
        assert_eq!(bar.get(4).cloned(), cmd(BarCommand::Kick));
        assert_eq!(bar.get(5).cloned(), cmd(BarCommand::PickUp));
        assert_eq!(bar.get(6).cloned(), cmd(BarCommand::LookHere));
        assert_eq!(bar.get(7).cloned(), cmd(BarCommand::Pray));
        assert_eq!(bar.get(8).cloned(), cmd(BarCommand::Enhance));
        assert_eq!(letter(&bar, 9, &pack), Some('d'));

        let wizard = pack_of(vec![
            InvItem {
                slots: vec![Slot::Weapon],
                ..inv(
                    'a',
                    ')',
                    870,
                    1,
                    "a blessed +1 quarterstaff (weapon in hands)",
                )
            },
            inv(
                'b',
                '[',
                960,
                1,
                "an uncursed +0 cloak of magic resistance (being worn)",
            ),
            inv('c', '/', 1210, 1, "an uncursed wand of sleep (0:7)"),
            inv('f', '?', 1100, 1, "an uncursed scroll of light"),
            inv('h', '+', 1150, 1, "a blessed spellbook of force bolt"),
        ]);
        let bar = ActionBar::default_for("wizard", &wizard);
        assert_eq!(
            bar.get(0).cloned(),
            Some(SlotBinding::Spell {
                name: "force bolt".into()
            })
        );
        assert_eq!(letter(&bar, 1, &wizard), Some('c'));
        assert_eq!(bar.get(9), None, "no food");

        let priest = pack_of(vec![
            inv('a', ')', 880, 1, "a blessed +1 mace (weapon in right hand)"),
            inv('d', '!', 990, 4, "4 potions of holy water"),
            inv('g', '+', 1160, 1, "a blessed spellbook of remove curse"),
            inv('e', '%', 1090, 1, "a clove of garlic"),
        ]);
        let bar = ActionBar::default_for("Pri", &priest);
        assert_eq!(
            bar.get(0).cloned(),
            Some(SlotBinding::Spell {
                name: "remove curse".into()
            })
        );
        assert_eq!(letter(&bar, 1, &priest), Some('d'));
        assert_eq!(letter(&bar, 9, &priest), Some('e'));

        let barbarian = pack_of(vec![
            InvItem {
                slots: vec![Slot::Weapon],
                ..inv('a', ')', 840, 1, "a +0 two-handed sword (weapon in hands)")
            },
            InvItem {
                slots: vec![Slot::Alternate],
                ..inv('b', ')', 841, 1, "a +0 axe (alternate weapon; not wielded)")
            },
        ]);
        let bar = ActionBar::default_for("Barbarian", &barbarian);
        assert_eq!(letter(&bar, 1, &barbarian), Some('b'));
        let bar = ActionBar::default_for(
            "Archeologist",
            &pack_of(vec![
                inv('a', '(', 1000, 1, "a +2 pick-axe"),
                inv('b', '(', 1001, 1, "a tinning kit (0:30)"),
            ]),
        );
        assert!(matches!(
            bar.get(0),
            Some(SlotBinding::Item {
                action: ItemActionKind::Apply,
                letter: 'a',
                ..
            })
        ));
        assert!(matches!(
            bar.get(1),
            Some(SlotBinding::Item { letter: 'b', .. })
        ));
        let bar = ActionBar::default_for(
            "Knight",
            &pack_of(vec![inv('b', ')', 850, 1, "a +1 lance")]),
        );
        assert_eq!(bar.get(1).cloned(), cmd(BarCommand::Ride));
        assert!(
            ActionBar::default_for("nobody", &Pack::new())
                .get(0)
                .is_none()
        );
    }

    #[test]
    fn the_state_file_round_trips() {
        let pack = pack_of(valkyrie());
        let mut state = UiState::new(KeyProfile::Classic);
        state.bar = ActionBar::default_for("valkyrie", &pack);
        state.bar.set(
            0,
            Some(SlotBinding::item(&pack.items()[1], ItemActionKind::Throw)),
        );
        let json = state.to_json();
        assert!(json.contains("\"profile\": \"classic\""), "{json}");
        assert!(json.contains("\"kind\": \"item\""), "{json}");
        assert!(json.contains("\"action\": \"throw\""), "{json}");
        let back = UiState::from_json(&json).unwrap();
        assert_eq!(back, state);
        // an older or shorter file: the missing slots are empty
        let old = UiState::from_json(
            r#"{"version":1,"bar":{"slots":[{"kind":"command","cmd":"search"}]}}"#,
        )
        .unwrap();
        assert_eq!(old.profile, KeyProfile::Modern);
        assert_eq!(
            old.bar.get(0).cloned(),
            Some(SlotBinding::Command {
                cmd: BarCommand::Search
            })
        );
        assert_eq!(old.bar.view(9, &pack).state, SlotState::Empty);
        assert!(UiState::from_json("not json").is_err());
    }

    #[test]
    fn the_language_is_kept_with_the_character_and_in_the_profile() {
        let mut state = UiState::new(KeyProfile::Modern);
        assert!(
            !state.to_json().contains("lang"),
            "no language: none written"
        );
        state.lang = Some("ru".into());
        let back = UiState::from_json(&state.to_json()).unwrap();
        assert_eq!(back.lang.as_deref(), Some("ru"));
        let profile = Profile {
            lang: Some("ru".into()),
            ..Profile::default()
        };
        assert_eq!(Profile::from_json(&profile.to_json()), profile);
        assert_eq!(Profile::from_json("broken"), Profile::default());
        // the last hero: kept beside the language, an older file reads
        let hero = Profile {
            last_role: Some("Val".into()),
            last_female: Some(true),
            ..profile.clone()
        };
        assert_eq!(Profile::from_json(&hero.to_json()), hero);
        assert_eq!(Profile::from_json(r#"{"lang": "ru"}"#), profile);
    }

    #[test]
    fn every_command_has_keys_and_a_label() {
        for c in BarCommand::ALL {
            assert!(c.label_key().starts_with("cmd.") && c.label_key().len() > 4);
            assert!(!c.hint().is_empty());
        }
        assert_eq!(BarCommand::Pray.hint(), "#pray");
        assert_eq!(BarCommand::TurnUndead.label_key(), "cmd.turn_undead");
    }
}
