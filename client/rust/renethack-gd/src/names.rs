//! The name a new hero gets when the player leaves the field empty, or
//! rolls the dice beside it: one of a pool in the player's language (the
//! client's catalogs, `creation-names-female` and `creation-names-male`),
//! a heroine's name for a heroine. A gender left to chance is chosen here
//! first, among those the role and race allow, so that the name agrees
//! with the hero the game starts with; a name a saved game has is passed
//! over (starting would continue that game).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use nh_protocol::Catalog;

use crate::i18n::{self, Lang};
use crate::ui_events::CharacterChoice;

/// What the creation form sends for a choice left to chance.
const RANDOM: &str = "random";

/// The most bytes NetHack keeps of a name (PL_NSIZ, less its NUL): a
/// Cyrillic letter takes two.
pub const MAX_BYTES: usize = 31;

/// A name the engine takes (`nh_link::EngineConfig::character_options`):
/// letters, digits, spaces and ' _ ., at most 31 bytes, no space at an end.
fn takes(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_BYTES
        && name.trim() == name
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '_' | '.'))
}

/// The names of a list ("Aldric, Bran, …"), those the engine takes.
fn parse(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|n| takes(n))
        .map(str::to_string)
        .collect()
}

/// As much of `name` as NetHack keeps: its first 31 bytes, cut between
/// letters.
pub fn clip(name: &str) -> &str {
    let mut end = name.len().min(MAX_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// The pool for a heroine (`female`) or a hero, in the language now (the
/// pseudo-language's is English, unmarked: names are no words to mark).
pub fn pool(female: bool) -> Vec<String> {
    let id = if female {
        "creation-names-female"
    } else {
        "creation-names-male"
    };
    let list = i18n::tr_in(i18n::lang(), id)
        .or_else(|| i18n::tr_in(Lang::En, id))
        .unwrap_or_default();
    parse(&list)
}

/// A roll of the dice: the clock's nanoseconds and a count of the rolls,
/// mixed (splitmix64), so that two rolls in a row differ.
pub fn roll() -> u64 {
    static ROLLS: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut z = now
        ^ ROLLS
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A name of `pool` where `roll` falls, or the next one no saved game has
/// (`taken`); every one taken, the first free of them numbered ("Bran 2").
/// None for an empty pool.
pub fn pick(pool: &[String], roll: u64, taken: impl Fn(&str) -> bool) -> Option<String> {
    let n = pool.len();
    if n == 0 {
        return None;
    }
    let from = (roll % n as u64) as usize;
    let turn = || (0..n).map(|i| &pool[(from + i) % n]);
    if let Some(name) = turn().find(|name| !taken(name)) {
        return Some(name.clone());
    }
    (2..1000)
        .flat_map(|k| turn().map(move |name| format!("{name} {k}")))
        .find(|name| takes(name) && !taken(name))
}

/// The catalog index of what `code` names among `codes` (a role's,
/// race's or gender's code or name, any case); None for "random".
fn index_of<'a>(code: &str, codes: impl Iterator<Item = (i32, &'a str, &'a str)>) -> Option<i32> {
    let code = code.to_lowercase();
    codes
        .into_iter()
        .find(|(_, c, name)| c.to_lowercase() == code || name.to_lowercase() == code)
        .map(|(idx, _, _)| idx)
}

/// The genders a hero of the choice's role and race may be, by index.
fn genders(cat: &Catalog, choice: &CharacterChoice) -> Vec<i32> {
    let role = index_of(
        &choice.role,
        cat.roles
            .iter()
            .map(|r| (r.idx, r.code.as_str(), r.name.as_str())),
    );
    let race = index_of(
        &choice.race,
        cat.races
            .iter()
            .map(|r| (r.idx, r.code.as_str(), r.noun.as_str())),
    );
    let mut v: Vec<i32> = cat
        .roles
        .iter()
        .filter(|r| role.is_none_or(|x| r.idx == x))
        .flat_map(|r| r.combos.iter())
        .filter(|c| race.is_none_or(|x| c.race == x))
        .flat_map(|c| c.genders.iter().copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Whether the gender `code` names is the female one.
pub fn is_female(cat: &Catalog, code: &str) -> bool {
    let idx = index_of(
        code,
        cat.genders
            .iter()
            .map(|g| (g.idx, g.code.as_str(), g.adj.as_str())),
    );
    idx.and_then(|i| cat.genders.get(i as usize))
        .is_some_and(|g| g.adj == "female")
}

/// The choice with its gender chosen, when it was left to chance (by
/// `roll`, among those its role and race allow), and a name from the pool
/// that agrees with it: when it has none, or always (`again`: the dice).
/// A name no saved game has (`taken`); with no pool, the choice as it was.
pub fn named(
    cat: &Catalog,
    mut choice: CharacterChoice,
    roll: u64,
    again: bool,
    taken: impl Fn(&str) -> bool,
) -> CharacterChoice {
    if !again && !choice.name.trim().is_empty() {
        return choice;
    }
    if choice.gender.eq_ignore_ascii_case(RANDOM) {
        let allowed = genders(cat, &choice);
        if !allowed.is_empty() {
            // the high bits: the low ones pick the name
            let idx = allowed[((roll >> 32) % allowed.len() as u64) as usize];
            if let Some(g) = cat.genders.get(idx as usize) {
                choice.gender = g.code.clone();
            }
        }
    }
    let female = is_female(cat, &choice.gender);
    if let Some(name) = pick(&pool(female), roll, taken) {
        choice.name = name;
    }
    choice
}

/// The choice with the name the dice gave it (a heroine's when `female`)
/// kept in agreement with its gender: a gender left to chance becomes the
/// name's, when the role and race allow it; else, or when the other
/// gender is chosen, the dice roll again (`roll`, `taken` as `named`).
pub fn agreed(
    cat: &Catalog,
    mut choice: CharacterChoice,
    female: bool,
    roll: u64,
    taken: impl Fn(&str) -> bool,
) -> CharacterChoice {
    if choice.gender.eq_ignore_ascii_case(RANDOM) {
        let its = genders(cat, &choice)
            .into_iter()
            .filter_map(|i| cat.genders.get(i as usize))
            .find(|g| (g.adj == "female") == female);
        if let Some(g) = its {
            choice.gender = g.code.clone();
            return choice;
        }
    } else if is_female(cat, &choice.gender) == female {
        return choice;
    }
    named(cat, choice, roll, true, taken)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_protocol::{EngineMsg, parse_line};
    use nh_world::KeyProfile;

    fn catalog() -> Catalog {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    fn choice(role: &str, gender: &str) -> CharacterChoice {
        CharacterChoice {
            name: String::new(),
            role: role.into(),
            race: RANDOM.into(),
            gender: gender.into(),
            align: RANDOM.into(),
            profile: KeyProfile::Modern,
        }
    }

    #[test]
    fn the_pools_hold_names_the_engine_takes() {
        for lang in [Lang::En, Lang::Ru] {
            for id in ["creation-names-female", "creation-names-male"] {
                let list = i18n::tr_in(lang, id).unwrap_or_default();
                let all: Vec<&str> = list.split(',').map(str::trim).collect();
                let names = parse(&list);
                assert!(names.len() >= 20, "{lang:?} {id}: {} names", names.len());
                assert_eq!(names.len(), all.len(), "{lang:?} {id}: {all:?}");
                let mut unique = names.clone();
                unique.sort();
                unique.dedup();
                assert_eq!(unique.len(), names.len(), "{lang:?} {id}: a name twice");
            }
        }
    }

    #[test]
    fn a_name_passes_over_the_saved_games() {
        let pool: Vec<String> = ["Ann", "Bea", "Cy"].map(String::from).to_vec();
        assert_eq!(pick(&pool, 4, |_| false).as_deref(), Some("Bea"));
        assert_eq!(pick(&pool, 4, |n| n == "Bea").as_deref(), Some("Cy"));
        let all = |n: &str| ["Ann", "Bea", "Cy", "Bea 2"].contains(&n);
        assert_eq!(pick(&pool, 4, all).as_deref(), Some("Cy 2"));
        assert_eq!(pick(&[], 4, |_| false), None);
        assert!(!takes(" Ann") && !takes("Ann!") && takes("Ann 2") && takes("Злата"));
    }

    #[test]
    fn a_name_agrees_with_the_gender_chosen_first() {
        let cat = catalog();
        let female = pool(true);
        let male = pool(false);
        // a Valkyrie is a woman: her gender is chosen so, her name a woman's
        for roll in 0..20u64 {
            let c = named(
                &cat,
                choice("valkyrie", RANDOM),
                roll << 33 | roll,
                false,
                |_| false,
            );
            assert_eq!(c.gender, "Fem");
            assert!(female.contains(&c.name), "{c:?}");
        }
        let c = named(&cat, choice("samurai", "male"), 3, false, |_| false);
        assert_eq!(c.gender, "male");
        assert!(male.contains(&c.name), "{c:?}");
        // either gender comes up for a role that takes both
        let genders: Vec<String> = (0..64u64)
            .map(|r| named(&cat, choice("wizard", RANDOM), r << 32, false, |_| false).gender)
            .collect();
        assert!(genders.iter().any(|g| g == "Fem") && genders.iter().any(|g| g == "Mal"));
        for r in 0..64u64 {
            let c = named(&cat, choice(RANDOM, RANDOM), r << 32 | r, false, |_| false);
            let pool = if c.gender == "Fem" { &female } else { &male };
            assert!(pool.contains(&c.name), "{c:?}");
        }
    }

    #[test]
    fn a_rolled_name_follows_the_gender() {
        let cat = catalog();
        let female = pool(true);
        let male = pool(false);
        let mut c = choice("wizard", RANDOM);
        c.name = male[0].clone();
        // left to chance, the gender becomes the name's
        let kept = agreed(&cat, c.clone(), false, 5, |_| false);
        assert_eq!(
            (kept.name.as_str(), kept.gender.as_str()),
            (male[0].as_str(), "Mal")
        );
        // a heroine chosen: a heroine's name
        c.gender = "Fem".into();
        let rolled = agreed(&cat, c.clone(), false, 5, |_| false);
        assert_eq!(rolled.gender, "Fem");
        assert!(female.contains(&rolled.name), "{rolled:?}");
        // a Valkyrie is a woman: a man's name rolls again, hers agrees
        let mut v = choice("valkyrie", RANDOM);
        v.name = male[1].clone();
        let rolled = agreed(&cat, v, false, 5, |_| false);
        assert_eq!(rolled.gender, "Fem");
        assert!(female.contains(&rolled.name), "{rolled:?}");
    }

    #[test]
    fn a_name_is_clipped_to_what_nethack_keeps() {
        assert_eq!(clip("Aldric"), "Aldric");
        let long = "Ярославаярославаярославаярослава";
        let kept = clip(long);
        assert!(kept.len() <= MAX_BYTES && long.starts_with(kept));
        assert_eq!(kept.chars().count(), 15);
        assert_eq!(clip(&"a".repeat(40)).len(), 31);
    }

    #[test]
    fn a_name_typed_is_kept_unless_the_dice_roll() {
        let cat = catalog();
        let mut c = choice("valkyrie", RANDOM);
        c.name = "Brunhild".into();
        let kept = named(&cat, c.clone(), 7, false, |_| false);
        assert_eq!(
            (kept.name.as_str(), kept.gender.as_str()),
            ("Brunhild", "random")
        );
        let rolled = named(&cat, c, 7, true, |_| false);
        assert_ne!(rolled.name, "Brunhild");
        assert_eq!(rolled.gender, "Fem");
    }
}
