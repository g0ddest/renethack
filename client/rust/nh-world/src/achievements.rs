//! Achievements (spec 2026-10-03-steam-achievements-design.md): the
//! definitions of `client/achievements/achievements.toml`, their conditions
//! over the host's `progress` notice, the unlock policy (a normal game
//! only, each achievement once per player profile) and the local store,
//! `achievements.json` in the user data directory.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;

use nh_protocol::ProgressNotice;
use serde::{Deserialize, Serialize};

/// The definitions built into the client.
pub const BUILT_IN: &str = include_str!("../../../achievements/achievements.toml");

/// The roles, by their file codes (urole.filecode).
pub const ROLES: [&str; 13] = [
    "Arc", "Bar", "Cav", "Hea", "Kni", "Mon", "Pri", "Rog", "Ran", "Sam", "Tou", "Val", "Wiz",
];

/// The `u.uevent` fields a condition may name.
pub const EVENTS: [&str; 15] = [
    "minor_oracle",
    "major_oracle",
    "read_tribute",
    "qcalled",
    "qexpelled",
    "qcompleted",
    "uheard_tune",
    "uopened_dbridge",
    "invoked",
    "gehennom_entered",
    "uhand_of_elbereth",
    "udemigod",
    "uvibrated",
    "ascended",
    "amulet_wish",
];

/// The `u.uconduct` counters a condition may name.
pub const CONDUCTS: [&str; 13] = [
    "unvegetarian",
    "unvegan",
    "food",
    "gnostic",
    "weaphit",
    "killer",
    "literate",
    "polypiles",
    "polyselfs",
    "wishes",
    "wisharti",
    "sokocheat",
    "pets",
];

/// NetHack's own achievements are numbered 1 (the Bell) to 31 (the tune).
const NETHACK_ACHIEVEMENTS: std::ops::RangeInclusive<i32> = 1..=31;

/// One achievement.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Achievement {
    /// What the game and the local store call it.
    pub id: String,
    /// Its API name on Steam.
    pub steam: String,
    pub when: When,
    pub icon: Icon,
    /// Steam hides it until it is earned (and the panel shows "?").
    #[serde(default)]
    pub hidden: bool,
    /// The Fluent keys of its name and description.
    pub name: String,
    pub desc: String,
    /// The English name and description, until the catalogs come.
    pub name_en: String,
    pub desc_en: String,
}

/// A condition over the `progress` notice: every field given must hold.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// NetHack's achievement (ACH_* in you.h) has been attained.
    pub achieved: Option<i32>,
    /// This `u.uevent` field is set.
    pub event: Option<String>,
    /// The deepest level reached is this deep or deeper.
    pub deepest: Option<i32>,
    /// The game is over.
    #[serde(default)]
    pub gameover: bool,
    /// The game ended in an ascension.
    #[serde(default)]
    pub ascended: bool,
    /// The hero's role (its file code).
    pub role: Option<String>,
    /// This `u.uconduct` counter is still 0.
    pub never: Option<String>,
}

impl When {
    /// Whether the condition holds for `p`, the hero being of `role` (its
    /// file code, when known).
    pub fn holds(&self, p: &ProgressNotice, role: Option<&str>) -> bool {
        let ascended = p.gameover && p.how.as_deref() == Some("ascended");
        self.achieved.is_none_or(|n| p.achieved(n))
            && self.event.as_ref().is_none_or(|e| p.event(e))
            && self.deepest.is_none_or(|d| p.deepest >= d)
            && (!self.gameover || p.gameover)
            && (!self.ascended || ascended)
            && self.role.as_ref().is_none_or(|r| role == Some(r.as_str()))
            && self
                .never
                .as_ref()
                .is_none_or(|c| p.conduct.get(c) == Some(&0))
    }

    fn is_empty(&self) -> bool {
        *self == When::default()
    }
}

/// The subject of an achievement's medallion (the achievement-icons bake).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Icon {
    /// An object, by its appearance (what the catalog lists).
    pub object: Option<String>,
    /// A monster, by its name.
    pub creature: Option<String>,
    /// A role's hero, by the role's file code.
    pub role: Option<String>,
    pub numeral: Option<u32>,
    /// A pictogram of the client's icon set (`icons::Glyph`).
    pub glyph: Option<String>,
    /// An emblem drawn for the medallion alone.
    pub emblem: Option<String>,
    /// Struck through: "never did...".
    #[serde(default)]
    pub crossed: bool,
}

/// What a medallion shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject<'a> {
    Object(&'a str),
    Creature(&'a str),
    Role(&'a str),
    Numeral(u32),
    Glyph(&'a str),
    Emblem(&'a str),
}

impl Icon {
    /// Its subject; None unless exactly one is given.
    pub fn subject(&self) -> Option<Subject<'_>> {
        let all = [
            self.object.as_deref().map(Subject::Object),
            self.creature.as_deref().map(Subject::Creature),
            self.role.as_deref().map(Subject::Role),
            self.numeral.map(Subject::Numeral),
            self.glyph.as_deref().map(Subject::Glyph),
            self.emblem.as_deref().map(Subject::Emblem),
        ];
        let mut given = all.into_iter().flatten();
        let one = given.next()?;
        given.next().is_none().then_some(one)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    achievement: Vec<Achievement>,
}

/// The definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct Achievements {
    list: Vec<Achievement>,
}

impl Achievements {
    /// Read and check a definitions file.
    pub fn parse(text: &str) -> Result<Achievements, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let all = Achievements {
            list: file.achievement,
        };
        let errors = all.check();
        if errors.is_empty() {
            Ok(all)
        } else {
            Err(errors.join("\n"))
        }
    }

    /// The definitions built into the client.
    pub fn built_in() -> Achievements {
        Achievements::parse(BUILT_IN).expect("the built-in achievements are valid")
    }

    pub fn all(&self) -> &[Achievement] {
        &self.list
    }

    pub fn get(&self, id: &str) -> Option<&Achievement> {
        self.list.iter().find(|a| a.id == id)
    }

    /// What `progress` earns the hero (of `role`, its file code when
    /// known) that `store` does not hold yet, in the definitions' order:
    /// nothing outside a normal game (explore and debug modes earn
    /// nothing).
    pub fn newly_earned<'a>(
        &'a self,
        progress: &ProgressNotice,
        role: Option<&str>,
        store: &Store,
    ) -> Vec<&'a Achievement> {
        if progress.mode != "normal" {
            return Vec::new();
        }
        self.list
            .iter()
            .filter(|a| !store.has(&a.id) && a.when.holds(progress, role))
            .collect()
    }

    /// Unlock what `progress` newly earns `hero`: recorded in `store` and
    /// returned, for the toast and the backends.
    pub fn unlock<'a>(
        &'a self,
        progress: &ProgressNotice,
        hero: &Hero,
        store: &mut Store,
    ) -> Vec<&'a Achievement> {
        let earned = self.newly_earned(progress, hero.role.as_deref(), store);
        for a in &earned {
            store.unlocked.insert(
                a.id.clone(),
                Unlock {
                    turn: hero.turn,
                    role: hero.role.clone().unwrap_or_default(),
                    character: hero.character.clone(),
                    time: hero.time,
                },
            );
        }
        earned
    }

    /// What is wrong with the definitions.
    fn check(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let (mut ids, mut steam, mut keys) = (HashSet::new(), HashSet::new(), HashSet::new());
        for a in &self.list {
            let id = &a.id;
            if id.is_empty()
                || !id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                errors.push(format!("{id:?}: an id is lower case, digits and _"));
            }
            if !ids.insert(id.as_str()) {
                errors.push(format!("{id}: the id is not unique"));
            }
            let api = a.steam.strip_prefix("ACH_").unwrap_or("");
            if api.is_empty()
                || !api
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                errors.push(format!(
                    "{id}: Steam's name {:?} is not ACH_ and upper case",
                    a.steam
                ));
            }
            if !steam.insert(a.steam.as_str()) {
                errors.push(format!("{id}: Steam's name {} is not unique", a.steam));
            }
            for key in [&a.name, &a.desc] {
                if !fluent_id(key) {
                    errors.push(format!("{id}: {key:?} is not a Fluent message id"));
                }
                if !keys.insert(key.as_str()) {
                    errors.push(format!("{id}: the key {key} is not unique"));
                }
            }
            if a.name_en.trim().is_empty() || a.desc_en.trim().is_empty() {
                errors.push(format!("{id}: no English name or description"));
            }
            errors.extend(
                check_when(&a.when)
                    .into_iter()
                    .map(|e| format!("{id}: {e}")),
            );
            match a.icon.subject() {
                None => errors.push(format!("{id}: the icon needs exactly one subject")),
                Some(Subject::Role(r)) if !ROLES.contains(&r) => {
                    errors.push(format!("{id}: no role {r}"));
                }
                Some(_) => {}
            }
        }
        errors
    }
}

fn check_when(w: &When) -> Vec<String> {
    let mut errors = Vec::new();
    if w.is_empty() {
        errors.push("no condition".to_string());
    }
    if let Some(n) = w.achieved
        && !NETHACK_ACHIEVEMENTS.contains(&n)
    {
        errors.push(format!("no NetHack achievement {n}"));
    }
    if let Some(e) = &w.event
        && !EVENTS.contains(&e.as_str())
    {
        errors.push(format!("no event {e}"));
    }
    if let Some(c) = &w.never
        && !CONDUCTS.contains(&c.as_str())
    {
        errors.push(format!("no conduct {c}"));
    }
    if let Some(r) = &w.role
        && !ROLES.contains(&r.as_str())
    {
        errors.push(format!("no role {r}"));
    }
    if w.deepest.is_some_and(|d| d < 1) {
        errors.push("a depth is 1 or deeper".to_string());
    }
    errors
}

/// A Fluent message id: a letter, then letters, digits, `-` and `_`.
fn fluent_id(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Who earns, and when: what the local store records of an unlock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hero {
    /// The role's file code ("Arc".."Wiz"), when known.
    pub role: Option<String>,
    /// The character's name.
    pub character: String,
    /// The game's turn.
    pub turn: i64,
    /// Unix seconds.
    pub time: i64,
}

/// When and by whom an achievement was earned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unlock {
    /// The game's turn.
    pub turn: i64,
    /// The hero's role (its file code).
    pub role: String,
    /// The character's name.
    pub character: String,
    /// Unix seconds.
    pub time: i64,
}

/// The achievements earned on this computer, for every character of the
/// player profile (`achievements.json` in the user data directory).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store {
    /// By achievement id.
    #[serde(default)]
    pub unlocked: BTreeMap<String, Unlock>,
}

impl Store {
    /// The store at `path`; empty when there is none yet.
    pub fn load(path: &Path) -> io::Result<Store> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Store::default()),
            Err(e) => Err(e),
        }
    }

    /// Write the store to `path` whole or not at all (a new file renamed
    /// over the old one).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, text + "\n")?;
        std::fs::rename(&part, path)
    }

    pub fn has(&self, id: &str) -> bool {
        self.unlocked.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{EngineMsg, WinCall, parse_line};

    /// A recorded new game (nh-protocol's fixture: a Valkyrie, seed 42).
    const SESSION: &str = include_str!("../../nh-protocol/tests/data/eof-session.jsonl");

    /// A notice as the host sends it, from its arguments.
    fn decoded(args: &str) -> ProgressNotice {
        let line = format!(r#"{{"t":"win","fn":"progress","a":{args}}}"#);
        match parse_line(&line).unwrap_or_else(|e| panic!("{e}: {line}")) {
            EngineMsg::Win(WinCall::Progress(p)) => p,
            other => panic!("not a progress notice: {other:?}"),
        }
    }

    /// A normal game at turn 1 on Dlvl 4: a pet, no other conduct broken.
    fn game() -> ProgressNotice {
        let conduct = CONDUCTS
            .iter()
            .map(|c| (c.to_string(), i64::from(*c == "pets")))
            .collect();
        ProgressNotice {
            mode: "normal".into(),
            deepest: 4,
            conduct,
            ..ProgressNotice::default()
        }
    }

    fn hero(role: &str, character: &str, time: i64) -> Hero {
        Hero {
            role: Some(role.into()),
            character: character.into(),
            turn: 1200,
            time,
        }
    }

    fn ids(list: &[&Achievement]) -> Vec<String> {
        list.iter().map(|a| a.id.clone()).collect()
    }

    fn earned(all: &Achievements, p: &ProgressNotice, role: &str) -> Vec<String> {
        ids(&all.newly_earned(p, Some(role), &Store::default()))
    }

    #[test]
    fn the_built_in_file_is_whole() {
        let all = Achievements::built_in();
        assert_eq!(all.all().len(), 69);
        // NetHack's 31 achievements, each once
        let nethack: HashSet<i32> = all.all().iter().filter_map(|a| a.when.achieved).collect();
        assert!(NETHACK_ACHIEVEMENTS.clone().all(|n| nethack.contains(&n)));
        // an ascension for every role and twelve conducts
        for role in ROLES {
            assert!(
                all.all()
                    .iter()
                    .any(|a| a.when.ascended && a.when.role.as_deref() == Some(role)),
                "{role}"
            );
        }
        let conducts = all
            .all()
            .iter()
            .filter(|a| a.when.ascended && a.when.never.is_some())
            .count();
        assert_eq!(conducts, 12);
        // the spoilers and the Amulet's wish stay hidden on Steam
        let hidden: Vec<&str> = all
            .all()
            .iter()
            .filter(|a| a.hidden)
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(
            hidden,
            [
                "mines_prize",
                "sokoban_prize",
                "amulet_wish",
                "sokoban_purist"
            ]
        );
    }

    #[test]
    fn a_bad_file_says_what_is_wrong() {
        let text = r#"
            [[achievement]]
            id = "Bell"
            steam = "BELL"
            when = { achieved = 40, event = "nosuch" }
            icon = { object = "silver bell", creature = "Medusa" }
            name = "1bad"
            desc = "achievement-bell-desc"
            name_en = "Ring My Bell"
            desc_en = ""

            [[achievement]]
            id = "bell2"
            steam = "BELL"
            when = {}
            icon = { role = "Xyz" }
            name = "achievement-bell2-name"
            desc = "achievement-bell-desc"
            name_en = "x"
            desc_en = "y"
        "#;
        let e = Achievements::parse(text).unwrap_err();
        for wanted in [
            "\"Bell\": an id",
            "is not ACH_",
            "BELL is not unique",
            "\"1bad\" is not a Fluent",
            "achievement-bell-desc is not unique",
            "no English",
            "no NetHack achievement 40",
            "no event nosuch",
            "exactly one subject",
            "no condition",
            "no role Xyz",
        ] {
            assert!(e.contains(wanted), "{wanted} not in:\n{e}");
        }
        assert!(Achievements::parse("[[achievement]]\nid = 3").is_err());
    }

    #[test]
    fn the_recorded_fresh_game_earns_nothing() {
        let all = Achievements::built_in();
        let p = SESSION
            .lines()
            .filter_map(|l| match parse_line(l) {
                Ok(EngineMsg::Win(WinCall::Progress(p))) => Some(p),
                _ => None,
            })
            .next()
            .expect("the session's progress notice");
        assert!(
            all.newly_earned(&p, Some("Val"), &Store::default())
                .is_empty()
        );
    }

    #[test]
    fn milestones_ranks_and_depth_unlock_as_they_come() {
        let all = Achievements::built_in();
        let mut store = Store::default();
        // the Mines, a shop, the second and third ranks
        let p = decoded(
            r#"{"mode":"normal","achieved":[15,17,23,24],"events":{"qcalled":false},
            "deepest":4,"conduct":{"pets":1},"roleplay":{"blind":false},
            "gameover":false,"how":null}"#,
        );
        let got = all.unlock(&p, &hero("Val", "Hero", 1_790_000_000), &mut store);
        assert_eq!(ids(&got), ["mines", "shop", "rank_1", "rank_2"]);
        assert_eq!(
            store.unlocked["mines"],
            Unlock {
                turn: 1200,
                role: "Val".into(),
                character: "Hero".into(),
                time: 1_790_000_000
            }
        );
        // later: called to the Quest, Dlvl 12; the earlier ones are not
        // told again
        let mut later = p.clone();
        later.events.insert("qcalled".into(), 1);
        later.deepest = 12;
        let got = all.unlock(&later, &hero("Val", "Hero", 1_790_000_100), &mut store);
        assert_eq!(ids(&got), ["quest_called", "depth_10"]);
        assert!(
            all.unlock(&later, &hero("Val", "Hero", 1_790_000_200), &mut store)
                .is_empty()
        );
    }

    #[test]
    fn explore_and_debug_games_earn_nothing() {
        let all = Achievements::built_in();
        for mode in ["explore", "debug"] {
            let p = ProgressNotice {
                mode: mode.into(),
                achieved: vec![1, 2, 3, 15, 21],
                gameover: true,
                how: Some("ascended".into()),
                ..game()
            };
            assert!(earned(&all, &p, "Wiz").is_empty(), "{mode}");
        }
    }

    #[test]
    fn each_unlocks_once_per_profile_across_characters() {
        let all = Achievements::built_in();
        let mut store = Store::default();
        let p = ProgressNotice {
            achieved: vec![21],
            ..game()
        };
        assert_eq!(
            ids(&all.unlock(&p, &hero("Arc", "Indy", 10), &mut store)),
            ["sokoban"]
        );
        // another character of the same profile
        assert!(
            all.unlock(&p, &hero("Sam", "Musashi", 20), &mut store)
                .is_empty()
        );
        assert_eq!(store.unlocked["sokoban"].character, "Indy");
    }

    #[test]
    fn an_ascension_earns_its_role_and_its_conducts() {
        let all = Achievements::built_in();
        // a vegetarian, atheist Priest who still read and wished
        let mut p = ProgressNotice {
            achieved: (1..=9).collect(),
            gameover: true,
            how: Some("ascended".into()),
            ..game()
        };
        for (c, n) in [
            ("unvegan", 3),
            ("food", 40),
            ("weaphit", 200),
            ("killer", 150),
            ("literate", 9),
            ("wishes", 2),
            ("polypiles", 1),
            ("polyselfs", 1),
        ] {
            p.conduct.insert(c.into(), n);
        }
        let got = earned(&all, &p, "Pri");
        for wanted in [
            "ascension",
            "ascend_pri",
            "ascend_vegetarian",
            "ascend_atheist",
            "ascend_artiwishless",
        ] {
            assert!(got.contains(&wanted.to_string()), "{wanted}: {got:?}");
        }
        for not in [
            "ascend_val",
            "ascend_vegan",
            "ascend_foodless",
            "ascend_weaponless",
            "ascend_pacifist",
            "ascend_illiterate",
            "ascend_wishless",
            "ascend_polypileless",
            "ascend_polyselfless",
            "ascend_petless",
        ] {
            assert!(!got.contains(&not.to_string()), "{not}: {got:?}");
        }
        // the role unknown: no ascension by role, the rest as before
        let blind = ids(&all.newly_earned(&p, None, &Store::default()));
        assert!(!blind.iter().any(|a| a == "ascend_pri") && blind.contains(&"ascension".into()));
        // a death with the same counters earns no ascension
        let dead = ProgressNotice {
            achieved: vec![1, 2, 3],
            how: Some("died".into()),
            ..p
        };
        let got = earned(&all, &dead, "Pri");
        assert!(got.iter().all(|a| !a.starts_with("ascend")), "{got:?}");
    }

    #[test]
    fn the_sokoban_purist_waits_for_the_end() {
        let all = Achievements::built_in();
        // during the game the prize is held back (LL_SPOILER)
        let during = ProgressNotice {
            achieved: vec![21],
            ..game()
        };
        assert!(!earned(&all, &during, "Val").contains(&"sokoban_purist".into()));
        let end = ProgressNotice {
            achieved: vec![21, 11],
            gameover: true,
            how: Some("quit".into()),
            ..game()
        };
        let got = earned(&all, &end, "Val");
        assert!(got.contains(&"sokoban_prize".into()) && got.contains(&"sokoban_purist".into()));
        // a broken rule spoils the purist's
        let mut cheat = end.clone();
        cheat.conduct.insert("sokocheat".into(), 2);
        let got = earned(&all, &cheat, "Val");
        assert!(got.contains(&"sokoban_prize".into()) && !got.contains(&"sokoban_purist".into()));
    }

    #[test]
    fn a_missing_counter_is_not_a_kept_conduct() {
        let all = Achievements::built_in();
        let p = decoded(
            r#"{"mode":"normal","achieved":[9],"events":{},"deepest":50,"conduct":{},
            "roleplay":{},"gameover":true,"how":"ascended"}"#,
        );
        let got = earned(&all, &p, "Tou");
        assert!(got.contains(&"ascend_tou".into()) && got.contains(&"depth_40".into()));
        assert!(!got.iter().any(|a| a == "ascend_vegan"), "{got:?}");
    }

    #[test]
    fn the_store_keeps_its_unlocks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("achievements.json");
        assert_eq!(Store::load(&path).unwrap(), Store::default());
        let mut store = Store::default();
        store.unlocked.insert(
            "medusa".into(),
            Unlock {
                turn: 31337,
                role: "Kni".into(),
                character: "Lancelot".into(),
                time: 1_790_000_000,
            },
        );
        store.save(&path).unwrap();
        assert_eq!(Store::load(&path).unwrap(), store);
        assert!(!path.with_extension("json.part").exists());
        // a damaged file is an error, never an empty store written over it
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(
            Store::load(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
