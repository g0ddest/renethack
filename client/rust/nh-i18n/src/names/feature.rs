//! The features of a spot as dfeature_at(), stairs_description(),
//! ice_descr() and waterbody_name() describe them: "staircase up to level
//! 3", "branch staircase down to the Gnomish Mines", "high altar to Moloch
//! (unaligned)", "thin ice", "pool of yoghurt", "frozen water"; and the
//! trap a hallucinating hero reads its own role into.

use crate::grammar::{Case, Gender, Number};
use crate::lexicon::Lexicon;
use crate::phrase::Phrase;

use super::monster::link;
use super::ru::{Count, RuName, Tail};
use super::status::Status;

/// What the ice is like (ice_descr()) and what the liquid is like
/// (waterbody_name()).
const WATER_ADJECTIVES: [&str; 9] = [
    "solid", "sturdy", "steady", "unsteady", "thin", "slushy", "molten", "frozen", "deep",
];

/// Where stairs that are not ordinary lead.
const ENDS: [&str; 3] = [
    "out of the dungeon",
    "to the Elemental Planes",
    "to the end game",
];

pub(super) fn parse(lex: &Lexicon, text: &str) -> Option<RuName> {
    stairs(lex, text)
        .or_else(|| altar(lex, text))
        .or_else(|| water(lex, text))
        .or_else(|| interior(lex, text))
        .or_else(|| role_trap(lex, text))
}

/// The last of the traps a hallucinating hero reads (trap.c trapname()):
/// the hero's own role or rank in lower case before " trap", after
/// "tourist trap". "valkyrie trap" is ловушка для валькирий, in lower
/// case as the engine's.
fn role_trap(lex: &Lexicon, text: &str) -> Option<RuName> {
    let who = text.strip_suffix(" trap").filter(|w| !w.is_empty())?;
    let role = ["role", "rank"].iter().find_map(|section| {
        lex.entries(section)
            .find(|(key, _)| key.to_lowercase() == who)
            .and_then(|(_, entry)| entry.noun().cloned())
    })?;
    let mut many = RuName::new(role);
    many.count = Count::Some;
    let mut name = RuName::new(lex.noun("terrain", "trap")?.clone());
    name.tails.push(Tail::Text(format!(
        "{} {}",
        link(lex, "for"),
        many.form(Case::Gen).to_lowercase()
    )));
    Some(name)
}

/// What farlook calls the spots around a swallowed hero (pager.c):
/// "interior of the purple worm".
fn interior(lex: &Lexicon, text: &str) -> Option<RuName> {
    let monster = lex.parse_monster(text.strip_prefix("interior of ")?, true)?;
    let mut name = RuName::new(lex.noun("bodypart", "interior")?.clone());
    name.tails.push(Tail::Genitive(Box::new(monster.ru(lex))));
    Some(name)
}

/// "staircase up", "ladder down to level 4", "branch staircase down to the
/// Gnomish Mines", "staircase up out of the dungeon".
fn stairs(lex: &Lexicon, text: &str) -> Option<RuName> {
    let (branch, rest) = match text.strip_prefix("branch ") {
        Some(r) => ("branch ", r),
        None => ("", text),
    };
    let (kind, rest) = ["staircase", "stairs", "ladder"]
        .iter()
        .find_map(|k| rest.strip_prefix(k)?.strip_prefix(' ').map(|r| (*k, r)))?;
    let (dir, rest) = ["up", "down"]
        .iter()
        .find_map(|d| rest.strip_prefix(d).map(|r| (*d, r)))?;
    // "stairs" are a staircase when no entry says otherwise
    let head = [
        format!("{branch}{kind} {dir}"),
        format!("{branch}staircase {dir}"),
    ]
    .iter()
    .find_map(|k| lex.noun("terrain", k).cloned())?;
    let mut name = RuName::new(head);
    if rest.is_empty() {
        return Some(name);
    }
    let rest = rest.strip_prefix(' ')?;
    let tail = if let Some(n) = rest.strip_prefix("to level ") {
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        format!("{} {n}", link(lex, "to level"))
    } else if let Some(end) = ENDS.iter().find(|e| **e == rest) {
        link(lex, end).to_string()
    } else {
        // the branch's dungeon: в Гномьи копи
        let place = lex.word(rest.strip_prefix("to ")?)?;
        format!("{} {}", link(lex, "to"), place.form(Case::Acc))
    };
    name.tails.push(Tail::Text(tail));
    Some(name)
}

/// "altar to Tyr (lawful)", "high altar to Moloch (unaligned)".
fn altar(lex: &Lexicon, text: &str) -> Option<RuName> {
    let (key, rest) = match text.strip_prefix("high altar to ") {
        Some(r) => ("high altar", r),
        None => ("altar", text.strip_prefix("altar to ")?),
    };
    let (god, alignment) = rest.strip_suffix(')')?.rsplit_once(" (")?;
    let alignment = lex.get("alignment", alignment)?.adjective()?;
    let mut name = RuName::new(lex.noun("terrain", key)?.clone());
    name.tails.push(match lex.noun("god", god) {
        Some(g) => Tail::Genitive(Box::new(RuName::new(g.clone()))),
        None => Tail::Text(god.to_string()),
    });
    name.statuses.push(Status::Text(
        alignment
            .form(Gender::Masc, Number::Sing, false, Case::Nom)
            .to_string(),
    ));
    Some(name)
}

/// A body of water or ice: "thin ice", "frozen water", "molten lava",
/// "pool of yoghurt", "wall of water", "sturdy frozen ink".
fn water(lex: &Lexicon, text: &str) -> Option<RuName> {
    let mut adjectives = Vec::new();
    let mut rest = text;
    while let Some((a, r)) = WATER_ADJECTIVES
        .iter()
        .find_map(|a| rest.strip_prefix(a)?.strip_prefix(' ').map(|r| (*a, r)))
    {
        adjectives.push(lex.adjective(a)?.clone());
        rest = r;
    }
    let found = ["terrain", "surface", "liquid"]
        .iter()
        .find_map(|s| lex.noun(s, rest).cloned());
    let mut name = match found {
        Some(n) => RuName::new(n),
        None => {
            // a pool or a wall of a liquid: водоём йогурта
            let (head, liquid) = ["pool", "wall"]
                .iter()
                .find_map(|h| Some((*h, rest.strip_prefix(h)?.strip_prefix(" of ")?)))?;
            let liquid = lex.noun("liquid", liquid)?;
            let mut n = RuName::new(lex.noun("terrain", head)?.clone());
            n.tails
                .push(Tail::Genitive(Box::new(RuName::new(liquid.clone()))));
            n
        }
    };
    name.adjectives = adjectives;
    Some(name)
}

#[cfg(test)]
mod tests {
    use crate::grammar::Case;
    use crate::lexicon::Lexicon;
    use crate::phrase::Phrase;

    fn word(text: &str, case: Case) -> String {
        Lexicon::ru()
            .word(text)
            .unwrap_or_else(|| panic!("{text:?}"))
            .form(case)
    }

    #[test]
    fn stairs_say_where_they_lead() {
        assert_eq!(word("a staircase up", Case::Nom), "лестница вверх");
        assert_eq!(
            word("staircase up to level 1", Case::Nom),
            "лестница вверх на уровень 1"
        );
        assert_eq!(
            word("a ladder down to level 12", Case::Prep),
            "приставной лестнице вниз на уровень 12"
        );
        assert_eq!(
            word("branch staircase down to the Gnomish Mines", Case::Nom),
            "боковая лестница вниз в Гномьи копи"
        );
        assert_eq!(
            word("staircase up out of the dungeon", Case::Nom),
            "лестница вверх из подземелья"
        );
        assert_eq!(word("stairs down", Case::Nom), "лестница вниз");
    }

    #[test]
    fn altars_name_their_god_and_alignment() {
        assert_eq!(
            word("an altar to Tyr (lawful)", Case::Nom),
            "алтарь Тюра (законопослушный)"
        );
        assert_eq!(
            word("high altar to Moloch (unaligned)", Case::Prep),
            "главном алтаре Молоха (безверный)"
        );
    }

    #[test]
    fn the_interior_of_whoever_swallowed_the_hero() {
        assert_eq!(
            word("interior of the purple worm", Case::Nom),
            "нутро пурпурного червя"
        );
        assert_eq!(word("interior of it", Case::Loc), "нутре кого-то");
    }

    #[test]
    fn a_trap_for_the_hero_s_own_kind() {
        assert_eq!(word("valkyrie trap", Case::Nom), "ловушка для валькирий");
        assert_eq!(
            word("the student of stones trap", Case::Gen),
            "ловушки для учеников камня"
        );
        assert_eq!(
            word("a cavewoman trap", Case::Acc),
            "ловушку для пещерных женщин"
        );
        // the tourist's own is the trap everyone falls into
        assert_eq!(word("tourist trap", Case::Nom), "ловушка для туристов");
        // a trap that is one, and a trap for nobody
        assert_eq!(word("bear trap", Case::Nom), "медвежий капкан");
        assert!(Lexicon::ru().word("newt trap").is_none());
    }

    #[test]
    fn ice_and_water() {
        assert_eq!(word("thin ice", Case::Loc), "тонком льду");
        assert_eq!(word("solid ice", Case::Nom), "прочный лёд");
        assert_eq!(word("frozen yoghurt", Case::Nom), "замёрзший йогурт");
        assert_eq!(word("pool of water", Case::Nom), "водоём");
        assert_eq!(word("a pool of ink", Case::Gen), "водоёма чернил");
        assert_eq!(word("wall of lava", Case::Ins), "стеной лавы");
        assert_eq!(word("moat", Case::Loc), "рву");
    }
}
