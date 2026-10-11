//! The states a stethoscope or a wand of probing lists after a creature's
//! level, hit points and armour class (insight.c mstatusline() and
//! ustatusline(), pager.c mhidden_description()): ", tame, fast, hiding
//! under a chest". Each is a pattern of the lexicon's "mstate" section
//! agreeing with the creature ({ручной|ручная|ручное|ручные}), some with
//! another creature or a thing in it ({monster:ins}).

use crate::grammar::{Case, Gender, Number};
use crate::lexicon::Lexicon;
use crate::phrase::Phrase;

use super::Index;
use super::ru::RuName;
use super::status::{Slots, Status};

/// The states of one creature, each after ", ".
#[derive(Debug, Clone)]
pub struct States(Vec<Status>);

/// The states whose last words name a creature or a thing.
const WITH_A_NAME: [&str; 8] = [
    "mimicking",
    "masquerading as",
    "hiding under",
    "hiding on the",
    "being digested by",
    "engulfed by",
    "held by",
    "holding",
];

/// The states of `text` (", tame, fast"), or None when one is no state
/// the lexicon knows.
pub(super) fn parse(lex: &Lexicon, index: &Index, text: &str) -> Option<States> {
    let rest = text.strip_prefix(", ")?;
    rest.split(", ")
        .map(|item| state(lex, index, item))
        .collect::<Option<Vec<_>>>()
        .map(States)
}

fn pattern(lex: &Lexicon, key: &str, slots: Slots) -> Option<Status> {
    let ru = lex.get("mstate", key)?.fixed()?;
    Some(Status::Pattern(ru.to_string(), slots))
}

fn state(lex: &Lexicon, index: &Index, item: &str) -> Option<Status> {
    if let Some(s) = pattern(lex, item, Slots::default()) {
        return Some(s);
    }
    // a long worm: "2nd of 5 segments"
    if let Some((nth, total)) = item
        .strip_suffix(" segments")
        .and_then(|r| r.split_once(" of "))
    {
        let n = nth.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if digits(n) && digits(total) {
            let ru = lex.get("mstate", "{n} of {m} segments")?.fixed()?;
            return Some(Status::Text(ru.replace("{n}", n).replace("{m}", total)));
        }
    }
    // a body part, agreeing with the part: "injured legs" is ранены ноги
    for lead in ["injured", "slippery"] {
        if let Some(part) = item.strip_prefix(lead).and_then(|r| r.strip_prefix(' '))
            && let Some(p) = lex.word(part)
        {
            let ru = lex.get("mstate", &format!("{lead} {{part}}"))?.fixed()?;
            let ru = ru.replace("{part}", &p.form(Case::Nom));
            let text = Status::Pattern(ru, Slots::default()).render(p.gender(), p.number());
            return Some(Status::Text(text));
        }
    }
    // a creature or a thing: "held by the owlbear", "mimicking a boulder"
    for lead in WITH_A_NAME {
        if let Some(name) = item.strip_prefix(lead).and_then(|r| r.strip_prefix(' '))
            && let Some(named) = name_ru(lex, index, name)
        {
            let slots = Slots {
                monster: Some(Box::new(named)),
                ..Slots::default()
            };
            return pattern(lex, &format!("{lead} {{monster}}"), slots);
        }
    }
    None
}

/// What a state names: an object as a mimic shows it, a creature, the
/// ceiling or a floor.
fn name_ru(lex: &Lexicon, index: &Index, text: &str) -> Option<RuName> {
    super::object::parse_object(lex, index, text)
        .map(|o| o.ru(lex))
        .or_else(|| super::monster::parse_monster(lex, index, text, false).map(|m| m.ru(lex)))
        .or_else(|| lex.word(text))
}

impl States {
    fn render(&self, gender: Gender, number: Number) -> String {
        self.0
            .iter()
            .map(|s| format!(", {}", s.render(gender, number)))
            .collect()
    }
}

impl Phrase for States {
    fn form(&self, _case: Case) -> String {
        self.render(Gender::Masc, Number::Sing)
    }

    fn gender(&self) -> Gender {
        Gender::Masc
    }

    fn number(&self) -> Number {
        Number::Sing
    }

    /// The states of a creature of that gender and number: ", ручная".
    fn agreeing(&self, gender: Gender, number: Number, _case: Case) -> String {
        self.render(gender, number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Index;

    fn render(text: &str, gender: Gender, number: Number) -> Option<String> {
        let lex = Lexicon::ru();
        let index = Index::new(lex);
        parse(lex, &index, text).map(|s| s.agreeing(gender, number, Case::Nom))
    }

    #[test]
    fn a_creature_s_states_agree_with_it() {
        assert_eq!(
            render(", tame", Gender::Fem, Number::Sing).as_deref(),
            Some(", ручная")
        );
        assert_eq!(
            render(", peaceful, fast, invisible", Gender::Masc, Number::Sing).as_deref(),
            Some(", мирный, быстр, невидим")
        );
        assert_eq!(
            render(", 2nd of 5 segments", Gender::Masc, Number::Sing).as_deref(),
            Some(", сегмент 2 из 5")
        );
        // a part agrees with itself, not the creature
        assert_eq!(
            render(
                ", carrying you, injured rear legs",
                Gender::Masc,
                Number::Sing
            )
            .as_deref(),
            Some(", несёт вас, ранены задние ноги")
        );
        // another creature or a thing
        assert_eq!(
            render(", mimicking a boulder", Gender::Masc, Number::Sing).as_deref(),
            Some(", прикидывается валуном")
        );
        assert_eq!(
            render(", hiding under something", Gender::Fem, Number::Sing).as_deref(),
            Some(", прячется под чем-то")
        );
        assert_eq!(
            render(", engulfed by a fog cloud", Gender::Fem, Number::Sing).as_deref(),
            Some(", поглощена туманным облаком")
        );
        // a state no pattern knows: the whole list stays English
        assert!(render(", tame (10", Gender::Masc, Number::Sing).is_none());
        assert!(render("tame", Gender::Masc, Number::Sing).is_none());
    }
}
