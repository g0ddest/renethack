//! The command menu's sections: `#`, the HUD's Commands button and a
//! pad's "All commands" list every extended command NetHack offers, each
//! in the one section a player looks in first. What the hero knows or
//! has hides nothing and greys nothing out: only the order and the
//! sections are the menu's, every command stays as NetHack has it.
//!
//! The sections, top down:
//! - Common: the verbs of every few turns, the ones a player looks for by
//!   name (pray, chat, search, a door, a box, food and drink, a scroll,
//!   armour, a weapon, a tool), short enough to be read in one glance;
//! - Items: what else is done with things carried, and what is worn;
//! - Actions: what the hero does in the world, at an altar, a trap, a
//!   steed, the stairs;
//! - Information: the hero, the dungeon, the discoveries, a look around;
//! - Game: saving, quitting, options, help, versions;
//! - Moves and prefixes: a step, a rush or a run one way, and the
//!   prefixes for a key to come (folded at first; the stick does them
//!   under a pad, where they are left out).

use crate::dialogs::PaletteCmd;

/// A section of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Common,
    Items,
    Actions,
    Info,
    Game,
    Moves,
}

impl Section {
    /// Top down.
    pub const ALL: [Section; 6] = [
        Section::Common,
        Section::Items,
        Section::Actions,
        Section::Info,
        Section::Game,
        Section::Moves,
    ];

    /// Its heading's string id.
    pub fn title_id(self) -> &'static str {
        match self {
            Section::Common => "cmd-section-common",
            Section::Items => "cmd-section-items",
            Section::Actions => "cmd-section-actions",
            Section::Info => "cmd-section-info",
            Section::Game => "cmd-section-game",
            Section::Moves => "cmd-section-moves",
        }
    }

    /// Folded when the menu is first opened: the moves, which a key or
    /// the mouse does better than a menu.
    pub fn folded_at_first(self) -> bool {
        self == Section::Moves
    }

    /// Its commands by name, in the order it shows them (the moves'
    /// section holds every step, rush, run and prefix besides).
    fn names(self) -> &'static [&'static str] {
        match self {
            Section::Common => &[
                "pray", "chat", "pay", "search", "wait", "open", "close", "kick", "loot", "eat",
                "quaff", "read", "wear", "takeoff", "wield", "apply",
            ],
            Section::Items => &[
                "throw",
                "fire",
                "quiver",
                "zap",
                "puton",
                "remove",
                "takeoffall",
                "swap",
                "twoweapon",
                "invoke",
                "rub",
                "dip",
                "tip",
                "pickup",
                "drop",
                "droptype",
                "inventory",
                "inventtype",
                "adjust",
                "call",
                "name",
                "seeall",
                "seeweapon",
                "seearmor",
                "seerings",
                "seeamulet",
                "seetools",
                "showgold",
            ],
            Section::Actions => &[
                "engrave", "offer", "cast", "sit", "force", "untrap", "ride", "jump", "turn",
                "monster", "wipe", "up", "down", "travel", "retravel", "teleport",
            ],
            Section::Info => &[
                "attributes",
                "enhance",
                "showspells",
                "overview",
                "conduct",
                "known",
                "knownclass",
                "terrain",
                "annotate",
                "chronicle",
                "vanquished",
                "genocided",
                "look",
                "lookaround",
                "glance",
                "whatis",
                "whatdoes",
                "showtrap",
                "herecmdmenu",
                "therecmdmenu",
                "prevmsg",
            ],
            Section::Game => &[
                "save",
                "quit",
                "options",
                "optionsfull",
                "saveoptions",
                "autopickup",
                "toggle",
                "perminv",
                "redraw",
                "repeat",
                "help",
                "?",
                "history",
                "version",
                "versionshort",
                "bugreport",
                "exploremode",
                "suspend",
            ],
            Section::Moves => &["fight", "reqmenu", "run", "rush"],
        }
    }
}

/// A step, a rush or a run in a direction ("movewest", "runnorth"): the
/// moves' section's.
fn is_move(name: &str) -> bool {
    ["move", "rush", "run"]
        .iter()
        .any(|p| name.starts_with(p) && name.len() > p.len())
}

/// The section `c` is in: the one whose list names it; a step, a rush, a
/// run or a prefix, the moves'; a command no list names (one a later
/// NetHack adds) goes with the game's.
pub fn section_of(c: &PaletteCmd) -> Section {
    Section::ALL
        .into_iter()
        .find(|s| s.names().contains(&c.name.as_str()))
        .unwrap_or(if c.prefix || is_move(&c.name) {
            Section::Moves
        } else {
            Section::Game
        })
}

/// Left out under a pad: a step or a run in a direction, which the stick
/// does, or a prefix for a key to come.
pub fn pad_leaves_out(c: &PaletteCmd) -> bool {
    section_of(c) == Section::Moves
}

/// Each section with its commands (indexes into `cmds`) in the order
/// shown: its list's order, a command it holds but its list does not name
/// after those in the catalog's order. While goods are not paid for
/// (`owes`), #pay first; under a pad (`pad`) no moves nor prefixes. Every
/// section is there, an empty one too (a pad's moves).
pub fn sections(cmds: &[PaletteCmd], owes: bool, pad: bool) -> Vec<(Section, Vec<usize>)> {
    Section::ALL
        .into_iter()
        .map(|s| {
            let mut idx: Vec<usize> = (0..cmds.len())
                .filter(|&i| section_of(&cmds[i]) == s)
                .filter(|&i| !(pad && pad_leaves_out(&cmds[i])))
                .collect();
            let rank = |i: usize| {
                let pinned = owes && cmds[i].name == "pay";
                let at = s.names().iter().position(|n| *n == cmds[i].name);
                (!pinned, at.unwrap_or(usize::MAX), i)
            };
            idx.sort_by_key(|&i| rank(i));
            (s, idx)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The extended commands of NetHack 5.0's catalog as the engine sends
    /// it (nh-world's fixture), as the palette makes them.
    fn catalog_cmds() -> Vec<PaletteCmd> {
        let data = include_str!("../../nh-world/tests/data/catalog.jsonl");
        let line = data
            .lines()
            .find(|l| l.contains("\"extcmds\""))
            .expect("a catalog line");
        let v: serde_json::Value = serde_json::from_str(line).expect("JSON");
        let ext = v["a"]["extcmds"].as_array().expect("extcmds");
        ext.iter()
            .map(|e| {
                let name = e["name"].as_str().unwrap_or_default().to_string();
                let desc = e["desc"].as_str().unwrap_or_default().to_string();
                PaletteCmd {
                    prefix: desc.starts_with("prefix:"),
                    name,
                    desc,
                    key: String::new(),
                    also: Vec::new(),
                }
            })
            .filter(|c| !c.name.is_empty() && c.name != "#")
            .collect()
    }

    #[test]
    fn every_command_is_in_exactly_one_section() {
        let cmds = catalog_cmds();
        assert!(cmds.len() > 100, "{} commands", cmds.len());
        for (owes, pad) in [(false, false), (true, false), (false, true), (true, true)] {
            let laid = sections(&cmds, owes, pad);
            assert_eq!(
                laid.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                Section::ALL
            );
            let mut seen = vec![0; cmds.len()];
            for (_, idx) in &laid {
                for &i in idx {
                    seen[i] += 1;
                }
            }
            for (c, n) in cmds.iter().zip(&seen) {
                let want = usize::from(!(pad && pad_leaves_out(c)));
                assert_eq!(*n, want, "#{} listed {n} times (pad {pad})", c.name);
            }
        }
        // each named by a section's list, or a move: none falls to the
        // game's by default
        for c in &cmds {
            let named = Section::ALL
                .iter()
                .any(|s| s.names().contains(&c.name.as_str()));
            assert!(named || is_move(&c.name), "#{} named by no section", c.name);
        }
        // no list names a command twice, nor one in two sections
        let mut all: Vec<&str> = Section::ALL
            .iter()
            .flat_map(|s| s.names())
            .copied()
            .collect();
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn pay_is_first_while_goods_are_owed() {
        let cmds = catalog_cmds();
        for pad in [false, true] {
            let laid = sections(&cmds, true, pad);
            let (s, common) = &laid[0];
            assert_eq!(*s, Section::Common);
            assert_eq!(cmds[common[0]].name, "pay");
            let laid = sections(&cmds, false, pad);
            assert_eq!(cmds[laid[0].1[0]].name, "pray");
        }
    }

    #[test]
    fn a_pad_s_menu_has_no_moves_nor_prefixes() {
        let cmds = catalog_cmds();
        let laid = sections(&cmds, false, true);
        let moves = &laid.iter().find(|(s, _)| *s == Section::Moves).unwrap().1;
        assert!(moves.is_empty());
        for (_, idx) in &laid {
            for &i in idx {
                let n = &cmds[i].name;
                assert!(!is_move(n) && !cmds[i].prefix, "#{n} under a pad");
            }
        }
        // the keys' menu has them all, the prefixes first
        let laid = sections(&cmds, false, false);
        let moves = &laid.iter().find(|(s, _)| *s == Section::Moves).unwrap().1;
        assert_eq!(moves.len(), 28);
        assert_eq!(cmds[moves[0]].name, "fight");
        assert!(Section::Moves.folded_at_first());
        assert!(!Section::Common.folded_at_first());
    }

    #[test]
    fn a_command_no_list_names_still_has_a_section() {
        let c = |name: &str, prefix: bool| PaletteCmd {
            name: name.into(),
            desc: String::new(),
            key: String::new(),
            also: Vec::new(),
            prefix,
        };
        assert_eq!(section_of(&c("pray", false)), Section::Common);
        assert_eq!(section_of(&c("newthing", false)), Section::Game);
        assert_eq!(section_of(&c("newprefix", true)), Section::Moves);
        assert_eq!(section_of(&c("runwest", false)), Section::Moves);
        // "run" and "rush" alone are prefixes, named by the moves' list
        assert_eq!(section_of(&c("run", true)), Section::Moves);
    }
}
