//! The settings' Gameplay section: the NetHack options a new player trips
//! on (the rest stay on the engine's own menu). NetHack keeps them in the
//! save, so the profile's reach a new game only, in the options it starts
//! with (`Gameplay::engine_options`); a game in play has its own, which the
//! host tells in its options notice (`Gameplay::from_notice`) and which
//! change through the engine's full options menu (`#optionsfull`) as a
//! player would (`OptionsRun`): the entries that differ picked, their
//! questions answered.

use std::collections::{BTreeMap, VecDeque};

use nh_protocol::{MenuItem, OptionsNotice, Reply};
use serde::{Deserialize, Serialize};

use crate::{Prompt, step_through};

/// The full options menu's title.
pub const OPTIONS_MENU: &str = "Set what options?";
/// The questions of the compound options the section has.
const TYPES_MENU: &str = "Autopickup what?";
const TYPES_LINE: &str = "New pickup_types:";
const BURDEN_MENU: &str = "Select encumbrance level:";
const PARANOID_MENU: &str = "Actions requiring extra confirmation:";

/// The most a pickup lifts without asking (`pickup_burden`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Burden {
    Unencumbered,
    Burdened,
    #[default]
    Stressed,
    Strained,
    Overtaxed,
    Overloaded,
}

impl Burden {
    pub const ALL: [Burden; 6] = [
        Burden::Unencumbered,
        Burden::Burdened,
        Burden::Stressed,
        Burden::Strained,
        Burden::Overtaxed,
        Burden::Overloaded,
    ];

    /// The engine's word for it, as its menus show it.
    pub fn word(self) -> &'static str {
        match self {
            Burden::Unencumbered => "unencumbered",
            Burden::Burdened => "burdened",
            Burden::Stressed => "stressed",
            Burden::Strained => "strained",
            Burden::Overtaxed => "overtaxed",
            Burden::Overloaded => "overloaded",
        }
    }

    /// The letter its menu and its option take (the option reads a word's
    /// first letter: "strained" would be stressed).
    pub fn letter(self) -> char {
        match self {
            Burden::Unencumbered => 'u',
            Burden::Burdened => 'b',
            Burden::Stressed => 's',
            Burden::Strained => 'n',
            Burden::Overtaxed => 't',
            Burden::Overloaded => 'l',
        }
    }

    fn of_word(w: &str) -> Option<Burden> {
        Burden::ALL.into_iter().find(|b| b.word() == w)
    }
}

/// A switch of `paranoid_confirmation` the section has (the others stay as
/// the game has them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paranoid {
    /// "yes", not y, to quit without saving.
    Quit,
    /// "yes", not y, to attack a peaceful.
    Attack,
    /// A question before praying.
    Pray,
    /// R and T ask which, even with one thing on.
    Remove,
}

impl Paranoid {
    pub const ALL: [Paranoid; 4] = [
        Paranoid::Quit,
        Paranoid::Attack,
        Paranoid::Pray,
        Paranoid::Remove,
    ];

    /// Its word in the option's value.
    pub fn word(self) -> &'static str {
        match self {
            Paranoid::Quit => "quit",
            Paranoid::Attack => "attack",
            Paranoid::Pray => "pray",
            Paranoid::Remove => "Remove",
        }
    }

    /// Its letter in the option's menu.
    fn letter(self) -> char {
        match self {
            Paranoid::Quit => 'q',
            Paranoid::Attack => 'a',
            Paranoid::Pray => 'p',
            Paranoid::Remove => 'R',
        }
    }
}

/// What autopickup takes, the choices a step goes through: everything,
/// coins, coins and the small valuables (amulets, scrolls, spellbooks,
/// potions, rings, wands), as the engine lists their classes.
pub const PICKUP_PRESETS: [&str; 3] = ["", "$", "$\"?+!=/"];

/// The object classes' symbols, as the engine lists them.
const CLASSES: &str = "$\")[%?+!=/(*`0_";

/// A row of the section: an option, or a switch of paranoid_confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameOption {
    Autopickup,
    PickupTypes,
    PickupBurden,
    Autoopen,
    Autoquiver,
    SafePet,
    Confirm,
    Paranoid(Paranoid),
    Verbose,
}

impl GameOption {
    /// The section's rows, top to bottom.
    pub const ALL: [GameOption; 12] = [
        GameOption::Autopickup,
        GameOption::PickupTypes,
        GameOption::PickupBurden,
        GameOption::Autoopen,
        GameOption::Autoquiver,
        GameOption::SafePet,
        GameOption::Confirm,
        GameOption::Paranoid(Paranoid::Attack),
        GameOption::Paranoid(Paranoid::Pray),
        GameOption::Paranoid(Paranoid::Quit),
        GameOption::Paranoid(Paranoid::Remove),
        GameOption::Verbose,
    ];

    /// The option's name in the engine's menus.
    pub fn name(self) -> &'static str {
        match self {
            GameOption::Autopickup => "autopickup",
            GameOption::PickupTypes => "pickup_types",
            GameOption::PickupBurden => "pickup_burden",
            GameOption::Autoopen => "autoopen",
            GameOption::Autoquiver => "autoquiver",
            GameOption::SafePet => "safe_pet",
            GameOption::Confirm => "confirm",
            GameOption::Paranoid(_) => "paranoid_confirmation",
            GameOption::Verbose => "verbose",
        }
    }

    /// The row whose switch this one needs: the kinds picked up need
    /// autopickup, a "yes" to attack needs the question.
    pub fn needs(self) -> Option<GameOption> {
        match self {
            GameOption::PickupTypes => Some(GameOption::Autopickup),
            GameOption::Paranoid(Paranoid::Attack) => Some(GameOption::Confirm),
            _ => None,
        }
    }
}

/// The section's options, as a new game starts with them. The defaults are
/// the engine's, but autopickup's: off, as the client has always started
/// the engine (`BASE_OPTIONS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gameplay {
    pub autopickup: bool,
    /// The classes autopickup takes, their symbols ("$?!"); empty: all.
    pub pickup_types: String,
    pub pickup_burden: Burden,
    pub autoopen: bool,
    pub autoquiver: bool,
    pub safe_pet: bool,
    pub confirm: bool,
    pub verbose: bool,
    pub paranoid_quit: bool,
    pub paranoid_attack: bool,
    pub paranoid_pray: bool,
    pub paranoid_remove: bool,
}

impl Default for Gameplay {
    fn default() -> Gameplay {
        Gameplay {
            autopickup: false,
            pickup_types: String::new(),
            pickup_burden: Burden::Stressed,
            autoopen: true,
            autoquiver: false,
            safe_pet: true,
            confirm: true,
            verbose: true,
            paranoid_quit: false,
            paranoid_attack: false,
            paranoid_pray: true,
            paranoid_remove: false,
        }
    }
}

/// The same classes, whatever their order.
fn same_classes(a: &str, b: &str) -> bool {
    let set = |s: &str| {
        let mut v: Vec<char> = s.chars().collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    set(a) == set(b)
}

impl Gameplay {
    /// Nothing changed from the defaults (the profile leaves it out).
    pub fn is_default(&self) -> bool {
        *self == Gameplay::default()
    }

    /// A switch's value (None for the compound options).
    pub fn is_on(&self, o: GameOption) -> Option<bool> {
        Some(match o {
            GameOption::Autopickup => self.autopickup,
            GameOption::Autoopen => self.autoopen,
            GameOption::Autoquiver => self.autoquiver,
            GameOption::SafePet => self.safe_pet,
            GameOption::Confirm => self.confirm,
            GameOption::Verbose => self.verbose,
            GameOption::Paranoid(p) => self.paranoid(p),
            GameOption::PickupTypes | GameOption::PickupBurden => return None,
        })
    }

    pub fn paranoid(&self, p: Paranoid) -> bool {
        match p {
            Paranoid::Quit => self.paranoid_quit,
            Paranoid::Attack => self.paranoid_attack,
            Paranoid::Pray => self.paranoid_pray,
            Paranoid::Remove => self.paranoid_remove,
        }
    }

    fn switch_mut(&mut self, o: GameOption) -> Option<&mut bool> {
        Some(match o {
            GameOption::Autopickup => &mut self.autopickup,
            GameOption::Autoopen => &mut self.autoopen,
            GameOption::Autoquiver => &mut self.autoquiver,
            GameOption::SafePet => &mut self.safe_pet,
            GameOption::Confirm => &mut self.confirm,
            GameOption::Verbose => &mut self.verbose,
            GameOption::Paranoid(Paranoid::Quit) => &mut self.paranoid_quit,
            GameOption::Paranoid(Paranoid::Attack) => &mut self.paranoid_attack,
            GameOption::Paranoid(Paranoid::Pray) => &mut self.paranoid_pray,
            GameOption::Paranoid(Paranoid::Remove) => &mut self.paranoid_remove,
            GameOption::PickupTypes | GameOption::PickupBurden => return None,
        })
    }

    /// Which of `PICKUP_PRESETS` the kinds picked up are, if one.
    pub fn pickup_preset(&self) -> Option<usize> {
        PICKUP_PRESETS
            .iter()
            .position(|p| same_classes(p, &self.pickup_types))
    }

    /// `o` a step on (`dir` 1) or back (-1); `wrap`: past the last value
    /// to the first. A switch flips; the kinds picked up step through the
    /// presets (from the first when they are none of them).
    pub fn step(&self, o: GameOption, dir: i32, wrap: bool) -> Gameplay {
        let mut g = self.clone();
        match o {
            GameOption::PickupTypes => {
                let all: Vec<usize> = (0..PICKUP_PRESETS.len()).collect();
                let i = match self.pickup_preset() {
                    Some(i) => step_through(&all, i, dir, wrap),
                    None => 0,
                };
                g.pickup_types = PICKUP_PRESETS[i].to_string();
            }
            GameOption::PickupBurden => {
                g.pickup_burden = step_through(&Burden::ALL, self.pickup_burden, dir, wrap);
            }
            _ => {
                if let Some(b) = g.switch_mut(o) {
                    *b = step_through(&[false, true], *b, dir, wrap);
                }
            }
        }
        g
    }

    /// `o`'s value taken from `from`, the rest kept.
    pub fn take(&mut self, o: GameOption, from: &Gameplay) {
        match o {
            GameOption::PickupTypes => self.pickup_types = from.pickup_types.clone(),
            GameOption::PickupBurden => self.pickup_burden = from.pickup_burden,
            _ => {
                if let (Some(b), Some(v)) = (self.switch_mut(o), from.is_on(o)) {
                    *b = v;
                }
            }
        }
    }

    /// The options a new game starts with these, for NETHACKOPTIONS. It
    /// reads its list right to left, so the first of two values wins: they
    /// go before the client's others (`BASE_OPTIONS` has `!autopickup`).
    pub fn engine_options(&self) -> String {
        let flag = |on: bool, name: &str| {
            if on {
                name.to_string()
            } else {
                format!("!{name}")
            }
        };
        // the classes the option takes (a hand-edited profile's others
        // would stop the engine with an error at its start)
        let mut types: String = CLASSES
            .chars()
            .filter(|&c| self.pickup_types.contains(c))
            .collect();
        if types.is_empty() {
            types = "all".to_string();
        }
        // the switches on after '+', the others after '!' (all after '-'
        // when none is on); paranoid_confirmation's others are left alone
        let (on, off): (Vec<Paranoid>, Vec<Paranoid>) =
            Paranoid::ALL.into_iter().partition(|&p| self.paranoid(p));
        let paranoid = if on.is_empty() {
            let words: Vec<&str> = off.iter().map(|p| p.word()).collect();
            format!("-{}", words.join(" "))
        } else {
            let words: Vec<String> = on
                .iter()
                .map(|p| p.word().to_string())
                .chain(off.iter().map(|p| format!("!{}", p.word())))
                .collect();
            format!("+{}", words.join(" "))
        };
        [
            flag(self.autopickup, "autopickup"),
            format!("pickup_types:{types}"),
            format!("pickup_burden:{}", self.pickup_burden.letter()),
            flag(self.autoopen, "autoopen"),
            flag(self.autoquiver, "autoquiver"),
            flag(self.safe_pet, "safe_pet"),
            flag(self.confirm, "confirm"),
            flag(self.verbose, "verbose"),
            format!("paranoid_confirmation:{paranoid}"),
        ]
        .join(",")
    }

    /// The values an options notice tells (a burden it does not know: the
    /// default's).
    pub fn from_notice(n: &OptionsNotice) -> Gameplay {
        Gameplay {
            autopickup: n.autopickup,
            pickup_types: n.pickup_types.clone(),
            pickup_burden: Burden::of_word(&n.pickup_burden).unwrap_or_default(),
            autoopen: n.autoopen,
            autoquiver: n.autoquiver,
            safe_pet: n.safe_pet,
            confirm: n.confirm,
            verbose: n.verbose,
            paranoid_quit: n.paranoid_quit,
            paranoid_attack: n.paranoid_attack,
            paranoid_pray: n.paranoid_pray,
            paranoid_remove: n.paranoid_remove,
        }
    }

    /// The values the full options menu shows (None when one is missing).
    pub fn from_menu(items: &[MenuItem]) -> Option<Gameplay> {
        let shown = shown_options(items);
        let value = |name: &str| shown.get(name).map(|(_, v)| v.as_str());
        let switch = |name: &str| value(name).map(|v| matches!(v, "true" | "on"));
        let paranoid = value("paranoid_confirmation")?;
        let has = |p: Paranoid| paranoid.split_whitespace().any(|w| w == p.word());
        let types = value("pickup_types")?;
        Some(Gameplay {
            autopickup: switch("autopickup")?,
            pickup_types: if types == "all" {
                String::new()
            } else {
                types.to_string()
            },
            pickup_burden: Burden::of_word(value("pickup_burden")?)?,
            autoopen: switch("autoopen")?,
            autoquiver: switch("autoquiver")?,
            safe_pet: switch("safe_pet")?,
            confirm: switch("confirm")?,
            verbose: switch("verbose")?,
            paranoid_quit: has(Paranoid::Quit),
            paranoid_attack: has(Paranoid::Attack),
            paranoid_pray: has(Paranoid::Pray),
            paranoid_remove: has(Paranoid::Remove),
        })
    }

    /// The options whose values differ from `other`'s, by their names.
    fn differing(&self, other: &Gameplay) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = GameOption::ALL
            .into_iter()
            .filter(|&o| match o {
                GameOption::PickupTypes => !same_classes(&self.pickup_types, &other.pickup_types),
                GameOption::PickupBurden => self.pickup_burden != other.pickup_burden,
                _ => self.is_on(o) != other.is_on(o),
            })
            .map(GameOption::name)
            .collect();
        names.dedup();
        names
    }

    /// The same values (the kinds picked up in any order).
    pub fn same_as(&self, other: &Gameplay) -> bool {
        self.differing(other).is_empty()
    }
}

/// An entry of the full options menu: its option's name and value
/// ("autopickup              [false]", "pickup_types\t[$?!]").
fn menu_entry(text: &str) -> Option<(&str, &str)> {
    let t = text.trim_start();
    let open = t
        .char_indices()
        .find(|&(i, c)| c == '[' && t[..i].ends_with([' ', '\t']))?
        .0;
    let value = t[open + 1..].strip_suffix(']')?;
    let name = t[..open].trim_end();
    (!name.is_empty()).then_some((name, value))
}

/// The options the full options menu lets the player change: each one's
/// entry and value.
fn shown_options(items: &[MenuItem]) -> BTreeMap<String, (u32, String)> {
    items
        .iter()
        .filter(|i| i.selectable)
        .filter_map(|i| {
            let (name, value) = menu_entry(i.str.as_deref()?)?;
            Some((
                name.to_string(),
                (u32::try_from(i.idx).ok()?, value.to_string()),
            ))
        })
        .collect()
}

/// A message an option's change says in the full menu ("'autopickup'
/// option toggled on."): the page shows the change, the log keeps the game.
pub fn is_toggle_note(text: &str) -> bool {
    text.starts_with('\'')
        && (text.ends_with("' option toggled on.") || text.ends_with("' option toggled off."))
}

#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// `#` sent: the extended command's prompt next.
    Hash,
    /// `optionsfull` sent: its menu next.
    Menu,
    /// Entries picked: the questions of the compound ones still to come.
    Questions(VecDeque<&'static str>),
    /// The menu closed with nothing picked, or a request given up: the
    /// command prompt ends it.
    Closing,
}

/// What the run does with a request.
#[derive(Debug, Clone, PartialEq)]
pub enum RunStep {
    /// Answer it so; the run goes on.
    Reply(Reply),
    /// The run is over at this command prompt, the player's again (the
    /// options notice before it tells what the game took).
    Done,
}

/// The game's options changed through the engine's full options menu, a
/// request at a time, as a player would: it begins at a command prompt and
/// ends at the next one. The menu then is the command ^A repeats, as after
/// a player's own change.
#[derive(Debug, Clone)]
pub struct OptionsRun {
    /// The values asked for.
    goal: Gameplay,
    stage: Stage,
}

impl OptionsRun {
    /// Give the game `goal`; the reply to the command prompt waiting.
    pub fn change(goal: Gameplay) -> (OptionsRun, Reply) {
        let run = OptionsRun {
            goal,
            stage: Stage::Hash,
        };
        (run, Reply::Key('#' as i32))
    }

    /// The values the run gives.
    pub fn goal(&self) -> &Gameplay {
        &self.goal
    }

    /// The next request, as the world sees it.
    pub fn on_prompt(&mut self, prompt: &Prompt) -> RunStep {
        let step = match prompt {
            Prompt::ExtCmd if self.stage == Stage::Hash => {
                self.stage = Stage::Menu;
                Some(Reply::ExtCmd(Some("optionsfull".to_string())))
            }
            Prompt::Menu { title, items, .. }
                if self.stage == Stage::Menu && title.as_deref() == Some(OPTIONS_MENU) =>
            {
                Some(self.on_menu(items))
            }
            Prompt::Command => return RunStep::Done,
            // a look at the map or a text window on the way: seen
            Prompt::MapPause | Prompt::AutoAck | Prompt::Show { .. } => Some(Reply::Ack),
            _ => match &mut self.stage {
                Stage::Questions(asked) => question(asked, &self.goal, prompt),
                _ => None,
            },
        };
        // a request the run did not expect: given up, the run ends at
        // the command prompt it leads to
        if step.is_none() {
            self.stage = Stage::Closing;
        }
        RunStep::Reply(step.unwrap_or_else(|| prompt.escape_reply()))
    }

    /// The full options menu: the entries whose values differ from the
    /// goal picked (as the menu lists them), or nothing.
    fn on_menu(&mut self, items: &[MenuItem]) -> Reply {
        let Some(now) = Gameplay::from_menu(items) else {
            self.stage = Stage::Closing;
            return Reply::Cancel;
        };
        let names = self.goal.differing(&now);
        let shown = shown_options(items);
        let mut picks: Vec<(u32, &'static str)> = names
            .iter()
            .filter_map(|&n| shown.get(n).map(|(idx, _)| (*idx, n)))
            .collect();
        if picks.is_empty() {
            self.stage = Stage::Closing;
            return Reply::Cancel;
        }
        picks.sort_unstable();
        let compound = ["pickup_types", "pickup_burden", "paranoid_confirmation"];
        self.stage = Stage::Questions(
            picks
                .iter()
                .map(|&(_, n)| n)
                .filter(|n| compound.contains(n))
                .collect(),
        );
        Reply::Menu(picks.iter().map(|&(idx, _)| (idx, -1)).collect())
    }
}

/// The answer to the question of a compound option picked, `goal` its
/// value; the option is taken off `asked`. None: not one of theirs.
fn question(asked: &mut VecDeque<&'static str>, goal: &Gameplay, prompt: &Prompt) -> Option<Reply> {
    let (name, reply) = match prompt {
        Prompt::Menu { title, items, .. } => {
            let title = title.as_deref()?;
            let idx = |i: &MenuItem| u32::try_from(i.idx).ok().map(|n| (n, -1));
            let picks: Vec<(u32, i64)> = match title {
                TYPES_MENU if goal.pickup_types.is_empty() => items
                    .iter()
                    .filter(|i| i.selectable && i.ch == 'A' as i32)
                    .filter_map(idx)
                    .collect(),
                TYPES_MENU => items
                    .iter()
                    .filter(|i| {
                        i.selectable
                            && i.gch > 0
                            && char::from_u32(i.gch as u32)
                                .is_some_and(|c| goal.pickup_types.contains(c))
                    })
                    .filter_map(idx)
                    .collect(),
                BURDEN_MENU => {
                    let want = goal.pickup_burden.letter() as i32;
                    items
                        .iter()
                        .filter(|i| i.selectable && i.ch == want)
                        .filter_map(idx)
                        .collect()
                }
                // the switches the section has as asked, the others as
                // the game has them
                PARANOID_MENU => items
                    .iter()
                    .filter(|i| {
                        let mine = Paranoid::ALL
                            .into_iter()
                            .find(|p| p.letter() as i32 == i.ch);
                        i.selectable && mine.map_or(i.preselected, |p| goal.paranoid(p))
                    })
                    .filter_map(idx)
                    .collect(),
                _ => return None,
            };
            let name = match title {
                TYPES_MENU => "pickup_types",
                BURDEN_MENU => "pickup_burden",
                _ => "paranoid_confirmation",
            };
            (name, Reply::Menu(picks))
        }
        // menustyle traditional asks for the classes on a line
        Prompt::Text { query, .. } if query.starts_with(TYPES_LINE) => {
            let types = if goal.pickup_types.is_empty() {
                "a".to_string()
            } else {
                goal.pickup_types.clone()
            };
            ("pickup_types", Reply::Text(types))
        }
        _ => return None,
    };
    let at = asked.iter().position(|&n| n == name)?;
    asked.remove(at);
    Some(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::PickHow;

    fn item(idx: i32, ch: char, gch: char, text: &str, pre: bool) -> MenuItem {
        MenuItem {
            win: 5,
            idx,
            glyph: None,
            selectable: true,
            ch: ch as i32,
            gch: gch as i32,
            attr: 0,
            clr: 0,
            str: Some(text.to_string()),
            preselected: pre,
            skipinvert: false,
        }
    }

    /// The full options menu as the engine shows it (its entries for the
    /// section, a few others, and what cannot be picked).
    fn options_menu(g: &Gameplay) -> Prompt {
        let tf = |b: bool| if b { "true" } else { "false" };
        let mut paranoid: Vec<&str> = Vec::new();
        for (p, w) in [
            (g.paranoid_quit, "quit"),
            (g.paranoid_attack, "attack"),
            (g.paranoid_pray, "pray"),
            (true, "trap"),
            (true, "swim"),
            (g.paranoid_remove, "Remove"),
        ] {
            if p {
                paranoid.push(w);
            }
        }
        let types = if g.pickup_types.is_empty() {
            "all"
        } else {
            &g.pickup_types
        };
        let mut items = vec![
            MenuItem {
                selectable: false,
                ..item(
                    0,
                    '\0',
                    '\0',
                    "Booleans (selecting will toggle value):",
                    false,
                )
            },
            MenuItem {
                selectable: false,
                ..item(1, '\0', '\0', "    bones                   [true]", false)
            },
        ];
        let lines = [
            (25, "autodig                 [false]".to_string()),
            (26, format!("autoopen                [{}]", tf(g.autoopen))),
            (
                27,
                format!("autopickup              [{}]", tf(g.autopickup)),
            ),
            (
                28,
                format!("autoquiver              [{}]", tf(g.autoquiver)),
            ),
            (33, format!("confirm                 [{}]", tf(g.confirm))),
            (68, format!("safe_pet                [{}]", tf(g.safe_pet))),
            (87, format!("verbose                 [{}]", tf(g.verbose))),
            (
                120,
                "packorder               [$\")[%?+!=/(*`0_]".to_string(),
            ),
            (
                121,
                format!("paranoid_confirmation   [{}]", paranoid.join(" ")),
            ),
            (
                122,
                format!("pickup_burden           [{}]", g.pickup_burden.word()),
            ),
            (123, format!("pickup_types            [{types}]")),
            (
                141,
                "autopickup exceptions   [(0 currently set)]".to_string(),
            ),
        ];
        for (idx, text) in lines {
            items.push(item(idx, '\0', '\0', &text, false));
        }
        Prompt::Menu {
            win: 5,
            how: PickHow::Any,
            title: Some(OPTIONS_MENU.into()),
            items,
        }
    }

    fn types_menu(now: &str) -> Prompt {
        let classes = [
            ('$', "pile of coins"),
            ('"', "amulet"),
            (')', "weapon"),
            ('[', "suit or piece of armor"),
            ('%', "piece of food"),
            ('?', "scroll"),
            ('+', "spellbook"),
            ('!', "potion"),
            ('=', "ring"),
            ('/', "wand"),
            ('(', "useful item (pick-axe, key, lamp...)"),
            ('*', "gem or rock"),
        ];
        let mut items: Vec<MenuItem> = classes
            .iter()
            .enumerate()
            .map(|(i, &(c, what))| {
                let letter = (b'a' + i as u8) as char;
                item(
                    i as i32,
                    letter,
                    c,
                    &format!("{c}  {what}"),
                    now.contains(c),
                )
            })
            .collect();
        items.push(item(15, 'A', '\0', "   All classes of objects", false));
        Prompt::Menu {
            win: 6,
            how: PickHow::Any,
            title: Some(TYPES_MENU.into()),
            items,
        }
    }

    fn burden_menu() -> Prompt {
        Prompt::Menu {
            win: 6,
            how: PickHow::One,
            title: Some(BURDEN_MENU.into()),
            items: Burden::ALL
                .iter()
                .enumerate()
                .map(|(i, b)| item(i as i32, b.letter(), '\0', b.word(), false))
                .collect(),
        }
    }

    fn paranoid_menu(g: &Gameplay) -> Prompt {
        let entries = [
            (
                'C',
                "for \"yes\" confirmations, require \"no\" to reject",
                false,
            ),
            (
                'q',
                "yes vs y to quit or to enter explore mode",
                g.paranoid_quit,
            ),
            ('d', "yes vs y to die (explore mode or debug mode)", false),
            (
                'a',
                "yes vs y to attack a peaceful monster",
                g.paranoid_attack,
            ),
            (
                'p',
                "y required to pray (supersedes old \"prayconfirm\" option)",
                g.paranoid_pray,
            ),
            (
                't',
                "y required to enter known trap unless considered harmless",
                true,
            ),
            (
                's',
                "'m' prefix necessary to deliberately walk into lava or water",
                true,
            ),
            (
                'R',
                "always pick from inventory for Remove and Takeoff",
                g.paranoid_remove,
            ),
        ];
        Prompt::Menu {
            win: 6,
            how: PickHow::Any,
            title: Some(PARANOID_MENU.into()),
            items: entries
                .iter()
                .enumerate()
                .map(|(i, &(c, text, pre))| item(i as i32, c, '\0', text, pre))
                .collect(),
        }
    }

    #[test]
    fn a_new_game_starts_with_every_option_of_the_section() {
        assert_eq!(
            Gameplay::default().engine_options(),
            "!autopickup,pickup_types:all,pickup_burden:s,autoopen,!autoquiver,safe_pet,\
             confirm,verbose,paranoid_confirmation:+pray !quit !attack !Remove"
        );
        let g = Gameplay {
            autopickup: true,
            pickup_types: "$?!".into(),
            pickup_burden: Burden::Strained,
            autoopen: false,
            paranoid_pray: false,
            ..Gameplay::default()
        };
        let o = g.engine_options();
        assert!(
            o.starts_with("autopickup,pickup_types:$?!,pickup_burden:n,!autoopen,"),
            "{o}"
        );
        assert!(
            o.ends_with("paranoid_confirmation:-quit attack pray Remove"),
            "{o}"
        );
        // only the classes there are, in the engine's order
        let odd = Gameplay {
            pickup_types: "x!$,".into(),
            ..Gameplay::default()
        };
        assert!(odd.engine_options().contains(",pickup_types:$!,"));
    }

    #[test]
    fn the_profile_keeps_only_a_changed_section() {
        let read: Gameplay = serde_json::from_str("{}").unwrap();
        assert_eq!(read, Gameplay::default());
        assert!(read.is_default());
        let g = Gameplay {
            autopickup: true,
            pickup_types: "$".into(),
            ..Gameplay::default()
        };
        let text = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<Gameplay>(&text).unwrap(), g);
    }

    #[test]
    fn the_values_are_read_from_the_full_options_menu() {
        let g = Gameplay {
            autopickup: true,
            pickup_types: "$?".into(),
            pickup_burden: Burden::Overtaxed,
            safe_pet: false,
            paranoid_quit: true,
            paranoid_pray: false,
            ..Gameplay::default()
        };
        let Prompt::Menu { items, .. } = options_menu(&g) else {
            unreachable!()
        };
        assert_eq!(Gameplay::from_menu(&items), Some(g));
        assert_eq!(
            menu_entry("autopickup exceptions   [(0 currently set)]"),
            Some(("autopickup exceptions", "(0 currently set)"))
        );
        assert_eq!(
            menu_entry("pickup_types\t[$?]"),
            Some(("pickup_types", "$?"))
        );
        assert_eq!(
            menu_entry("packorder               [$\")[%?+!=/(*`0_]"),
            Some(("packorder", "$\")[%?+!=/(*`0_"))
        );
        // a menu without the section's options reads as nothing
        assert_eq!(Gameplay::from_menu(&items[..4]), None);
    }

    #[test]
    fn the_notice_tells_the_game_s_values() {
        let n = OptionsNotice {
            autopickup: true,
            pickup_types: "$?".into(),
            pickup_burden: "overtaxed".into(),
            autoopen: false,
            autoquiver: true,
            safe_pet: true,
            confirm: false,
            verbose: true,
            paranoid_quit: true,
            paranoid_attack: false,
            paranoid_pray: false,
            paranoid_remove: true,
        };
        let g = Gameplay::from_notice(&n);
        assert_eq!(
            g,
            Gameplay {
                autopickup: true,
                pickup_types: "$?".into(),
                pickup_burden: Burden::Overtaxed,
                autoopen: false,
                autoquiver: true,
                confirm: false,
                paranoid_quit: true,
                paranoid_pray: false,
                paranoid_remove: true,
                ..Gameplay::default()
            }
        );
        // a burden it does not know: the default's
        let odd = OptionsNotice {
            pickup_burden: "heavy".into(),
            ..n
        };
        assert_eq!(Gameplay::from_notice(&odd).pickup_burden, Burden::Stressed);
    }

    #[test]
    fn a_change_picks_what_differs_and_answers_its_questions() {
        let now = Gameplay::default();
        let goal = Gameplay {
            autopickup: true,
            pickup_types: "$?!".into(),
            pickup_burden: Burden::Burdened,
            paranoid_attack: true,
            paranoid_pray: false,
            ..Gameplay::default()
        };
        let (mut run, first) = OptionsRun::change(goal.clone());
        assert_eq!((first, run.goal()), (Reply::Key('#' as i32), &goal));
        assert_eq!(
            run.on_prompt(&Prompt::ExtCmd),
            RunStep::Reply(Reply::ExtCmd(Some("optionsfull".into())))
        );
        // the entries in the menu's order: the switch, then the compounds
        assert_eq!(
            run.on_prompt(&options_menu(&now)),
            RunStep::Reply(Reply::Menu(vec![(27, -1), (121, -1), (122, -1), (123, -1)]))
        );
        // pray off, attack on; trap and swim as the game has them
        assert_eq!(
            run.on_prompt(&paranoid_menu(&now)),
            RunStep::Reply(Reply::Menu(vec![(3, -1), (5, -1), (6, -1)]))
        );
        assert_eq!(
            run.on_prompt(&burden_menu()),
            RunStep::Reply(Reply::Menu(vec![(1, -1)]))
        );
        assert_eq!(
            run.on_prompt(&types_menu("")),
            RunStep::Reply(Reply::Menu(vec![(0, -1), (5, -1), (7, -1)]))
        );
        // the toggle's note is the page's, not the log's
        assert!(is_toggle_note("'autopickup' option toggled on."));
        assert!(!is_toggle_note("You see here a scroll."));
        // the command prompt: the player's again
        assert_eq!(run.on_prompt(&Prompt::Command), RunStep::Done);
    }

    #[test]
    fn every_kind_picked_up_is_all_classes_and_a_line_asks_for_symbols() {
        let now = Gameplay {
            autopickup: true,
            pickup_types: "$".into(),
            ..Gameplay::default()
        };
        let all = Gameplay {
            pickup_types: String::new(),
            ..now.clone()
        };
        let (mut run, _) = OptionsRun::change(all);
        run.on_prompt(&Prompt::ExtCmd);
        assert_eq!(
            run.on_prompt(&options_menu(&now)),
            RunStep::Reply(Reply::Menu(vec![(123, -1)]))
        );
        assert_eq!(
            run.on_prompt(&types_menu("$")),
            RunStep::Reply(Reply::Menu(vec![(15, -1)]))
        );
        // menustyle traditional: the classes typed
        let some = Gameplay {
            pickup_types: "$?".into(),
            ..now.clone()
        };
        let (mut run, _) = OptionsRun::change(some);
        run.on_prompt(&Prompt::ExtCmd);
        run.on_prompt(&options_menu(&now));
        let line = Prompt::Text {
            query: "New pickup_types: [$\"?+!=/ am] ($)".into(),
            name: false,
        };
        assert_eq!(
            run.on_prompt(&line),
            RunStep::Reply(Reply::Text("$?".into()))
        );
    }

    #[test]
    fn nothing_to_change_closes_the_menu_and_a_stranger_is_given_up() {
        let g = Gameplay::default();
        let (mut run, _) = OptionsRun::change(g.clone());
        run.on_prompt(&Prompt::ExtCmd);
        assert_eq!(
            run.on_prompt(&options_menu(&g)),
            RunStep::Reply(Reply::Cancel)
        );
        assert_eq!(run.on_prompt(&Prompt::Command), RunStep::Done);
        // the extended command refused: an Esc, then the command prompt
        let (mut run, _) = OptionsRun::change(g.clone());
        let odd = Prompt::Choice {
            query: "Really?".into(),
            visible: vec!['y', 'n'],
            allowed: vec!['y', 'n'],
            default: None,
        };
        assert_eq!(run.on_prompt(&odd), RunStep::Reply(Reply::Char(27)));
        assert_eq!(run.on_prompt(&Prompt::Command), RunStep::Done);
    }

    #[test]
    fn a_step_flips_a_switch_and_goes_through_the_kinds_and_the_loads() {
        let g = Gameplay::default();
        assert!(g.step(GameOption::Autopickup, 1, false).autopickup);
        assert!(
            !g.step(GameOption::Paranoid(Paranoid::Pray), -1, false)
                .paranoid_pray
        );
        let coins = g.step(GameOption::PickupTypes, 1, false);
        assert_eq!(coins.pickup_types, "$");
        let small = coins.step(GameOption::PickupTypes, 1, false);
        assert_eq!(small.pickup_preset(), Some(2));
        assert_eq!(
            small.step(GameOption::PickupTypes, 1, true).pickup_types,
            ""
        );
        // the classes in another order are the same preset; others step
        // from the first
        let mixed = Gameplay {
            pickup_types: "/=!+?\"$".into(),
            ..g.clone()
        };
        assert_eq!(mixed.pickup_preset(), Some(2));
        let odd = Gameplay {
            pickup_types: "$*".into(),
            ..g.clone()
        };
        assert_eq!(
            (
                odd.pickup_preset(),
                odd.step(GameOption::PickupTypes, 1, false)
                    .pickup_types
                    .as_str()
            ),
            (None, "")
        );
        assert_eq!(
            g.step(GameOption::PickupBurden, 1, false).pickup_burden,
            Burden::Strained
        );
        let most = Gameplay {
            pickup_burden: Burden::Overloaded,
            ..g.clone()
        };
        assert_eq!(
            most.step(GameOption::PickupBurden, 1, false).pickup_burden,
            Burden::Overloaded
        );
        // one option taken from another set, the rest kept
        let mut mine = Gameplay::default();
        mine.take(GameOption::PickupTypes, &odd);
        mine.take(
            GameOption::Confirm,
            &Gameplay {
                confirm: false,
                ..odd.clone()
            },
        );
        assert_eq!(
            (mine.pickup_types.as_str(), mine.confirm, mine.autoopen),
            ("$*", false, true)
        );
    }
}
