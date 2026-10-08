//! Monster names as x_monnam() and its family print them: "the newt",
//! "It", "your little dog", "a saddled pony", "Fido", "dog called Fido",
//! "Asidonhopo the invisible shopkeeper", "the high priest of Moloch",
//! "Bob's ghost", "the poor newt", "newts".

use std::sync::Arc;

use crate::lexicon::{Lexicon, Noun};

use super::Index;
use super::english::{strip_word, uncapitalized};
use super::ru::{Count, RuName, Tail};

/// The adjectives x_monnam() and its callers put before a monster's name.
const ADJECTIVES: [&str; 17] = [
    "invisible",
    "saddled",
    "tame",
    "peaceful",
    "poor",
    "angry",
    "falling",
    "sleeping",
    "bite-covered",
    "plain",
    "immobile",
    "blinded",
    "blind",
    "beautiful",
    "renegade",
    "grand",
    "high",
];

/// The pronouns the engine names a monster (or the hero) with.
const PRONOUNS: [&str; 8] = [
    "it",
    "someone",
    "something",
    "you",
    "himself",
    "herself",
    "itself",
    "themselves",
];

/// What a monster's name says it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonsterKind {
    /// A monster of the lexicon: "newt".
    Species(String),
    /// "dog called Fido".
    Called { species: String, name: String },
    /// "Asidonhopo the invisible shopkeeper", "Bob the stripling": the
    /// title's section ("monster", "rank", "role") and key.
    Titled {
        name: String,
        section: &'static str,
        title: String,
    },
    /// "priest of Mitra", "guardian Angel of Thoth": the god's name as the
    /// engine printed it.
    Of { title: String, of: String },
    /// "Bob's ghost".
    Ghost(String),
    /// "it", "you", "itself".
    Pronoun(String),
    /// A rank or a role on its own: a player monster ("the stripling").
    Rank { section: &'static str, key: String },
    /// A personal name: a pet, a shopkeeper, a name no lexicon knows.
    Name(String),
}

/// A monster's name, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonsterName {
    /// "your little dog".
    pub your: bool,
    /// `Some` for a plural name ("newts").
    pub count: Count,
    /// English keys of the "adjective" section, in order.
    pub adjectives: Vec<String>,
    pub kind: MonsterKind,
}

/// `text` as a monster's name. A text no lexicon knows is a personal name
/// when `lenient`, else None.
pub(super) fn parse_monster(
    lex: &Lexicon,
    index: &Index,
    text: &str,
    lenient: bool,
) -> Option<MonsterName> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // s_suffix(): "Asidonhopo's", "the gnome's"; the template puts the
    // owner in the genitive after what is his
    if !text.ends_with("'s ghost")
        && let Some(owner) = text
            .strip_suffix("'s")
            .or_else(|| text.strip_suffix("s'").map(|_| &text[..text.len() - 1]))
    {
        return parse_monster(lex, index, owner, lenient);
    }
    let low = uncapitalized(text);
    if PRONOUNS.contains(&low.as_str()) {
        return Some(MonsterName {
            your: false,
            count: Count::One,
            adjectives: Vec::new(),
            kind: MonsterKind::Pronoun(low),
        });
    }
    let mut rest = text;
    let mut your = false;
    if let Some(r) = strip_word(rest, "your") {
        your = true;
        rest = r;
    } else if let Some(r) = ["the", "a", "an"].iter().find_map(|a| strip_word(rest, a)) {
        rest = r;
    }
    let mut adjectives = Vec::new();
    loop {
        // a whole name first: "high priest", "plain"... are no adjectives there
        if index.monsters.contains_key(rest) {
            break;
        }
        match ADJECTIVES.iter().find_map(|a| {
            rest.strip_prefix(a)
                .and_then(|r| r.strip_prefix(' '))
                .map(|r| (a, r))
        }) {
            Some((a, r)) => {
                adjectives.push(a.to_string());
                rest = r;
            }
            None => break,
        }
    }
    let (kind, count) = kind(lex, index, rest, lenient, &mut adjectives)?;
    Some(MonsterName {
        your,
        count,
        adjectives,
        kind,
    })
}

/// What `rest` names; the adjectives of a title ("the invisible
/// shopkeeper") go to `adjectives`.
fn kind(
    lex: &Lexicon,
    index: &Index,
    rest: &str,
    lenient: bool,
    adjectives: &mut Vec<String>,
) -> Option<(MonsterKind, Count)> {
    if let Some((key, plural)) = species(index, rest) {
        let count = if plural { Count::Some } else { Count::One };
        return Some((MonsterKind::Species(key), count));
    }
    if let Some((s, name)) = rest.split_once(" called ")
        && let Some((key, false)) = species(index, s)
    {
        return Some((
            MonsterKind::Called {
                species: key,
                name: name.to_string(),
            },
            Count::One,
        ));
    }
    if let Some((title, of)) = rest.split_once(" of ")
        && let Some((key, false)) = species(index, title)
        && (lex.get("god", of).is_some() || lenient)
    {
        return Some((
            MonsterKind::Of {
                title: key,
                of: of.to_string(),
            },
            Count::One,
        ));
    }
    if let Some(name) = rest
        .strip_suffix("'s ghost")
        .or_else(|| rest.strip_suffix("' ghost"))
    {
        return Some((MonsterKind::Ghost(name.to_string()), Count::One));
    }
    // "Bob the stripling": a name the engine capitalized, not words that
    // happen to stand before a "the" ("interior of the purple worm")
    if let Some((name, title)) = rest.split_once(" the ")
        && !name.starts_with(char::is_lowercase)
    {
        let mut t = title;
        let mut title_adjectives = Vec::new();
        while let Some((a, r)) = ADJECTIVES.iter().find_map(|a| {
            t.strip_prefix(a)
                .and_then(|r| r.strip_prefix(' '))
                .map(|r| (a, r))
        }) {
            title_adjectives.push(a.to_string());
            t = r;
        }
        if let Some((section, key)) = index.titles.get(&t.to_lowercase()) {
            adjectives.extend(title_adjectives);
            return Some((
                MonsterKind::Titled {
                    name: name.to_string(),
                    section,
                    title: key.clone(),
                },
                Count::One,
            ));
        }
    }
    if let Some((section, key)) = index.titles.get(&rest.to_lowercase()) {
        return Some((
            MonsterKind::Rank {
                section,
                key: key.clone(),
            },
            Count::One,
        ));
    }
    // a name the engine gives (a default pet, a shopkeeper), or any name
    // where one is expected
    if lex.noun("name", rest).is_some() || (lenient && !rest.is_empty()) {
        return Some((MonsterKind::Name(rest.to_string()), Count::One));
    }
    None
}

/// A monster of the lexicon, singular or plural: (key, plural).
fn species(index: &Index, text: &str) -> Option<(String, bool)> {
    index
        .monsters
        .get(text)
        .or_else(|| index.monsters.get(&uncapitalized(text)))
        .cloned()
}

impl MonsterName {
    /// The name in Russian.
    pub fn ru(&self, lex: &Lexicon) -> RuName {
        let noun = |section: &str, key: &str| -> Arc<Noun> {
            lex.noun(section, key)
                .cloned()
                .unwrap_or_else(|| Arc::new(Noun::fixed(key, crate::grammar::Gender::Masc)))
        };
        let mut name = match &self.kind {
            MonsterKind::Species(k) => {
                // "human" is a race's adjective as well (человеческий), and a
                // player monster ("wizard") a role with its feminine
                let mut n = RuName::new(noun("monster", k));
                n.as_adjective = lex.adjective_reading(k);
                n.female = lex
                    .noun("female", &super::english::capitalized(k))
                    .map(|f| Arc::new(f.uncapitalized()));
                n
            }
            MonsterKind::Pronoun(k) => {
                // what an owner with no name is said with: чья-то шляпа
                let mut n = RuName::new(noun("monster", k));
                n.whose = match k.as_str() {
                    "it" | "someone" => lex.adjective("someone's").cloned(),
                    "you" => lex.adjective("your").cloned(),
                    _ => None,
                };
                n
            }
            MonsterKind::Called { species, name } => {
                let mut n = RuName::new(noun("monster", species));
                n.tails
                    .push(Tail::Text(format!("{} {name}", link(lex, "named"))));
                n
            }
            MonsterKind::Titled {
                name,
                section,
                title,
            } => {
                let mut n = RuName::new(noun(section, title));
                // the engine's own names are Russian (Асидонхопо, Ицхак)
                n.tails.push(match lex.noun("name", name) {
                    Some(who) => Tail::Same(Box::new(RuName::new(who.clone()))),
                    None => Tail::Text(name.clone()),
                });
                n
            }
            MonsterKind::Of { title, of } => {
                let mut n = RuName::new(noun("monster", title));
                n.tails.push(match lex.noun("god", of) {
                    Some(god) => Tail::Genitive(Box::new(RuName::new(god.clone()))),
                    None => Tail::Text(of.clone()),
                });
                n
            }
            MonsterKind::Ghost(who) => {
                let mut n = RuName::new(noun("monster", "ghost"));
                n.tails.push(Tail::Text(who.clone()));
                n
            }
            MonsterKind::Rank { section, key } => {
                // "Wizard" is a role and a rank: either has the role's feminine
                let mut n = RuName::new(Arc::new(noun(section, key).uncapitalized()));
                n.female = lex.noun("female", key).map(|f| Arc::new(f.uncapitalized()));
                n
            }
            MonsterKind::Name(who) => match lex.noun("name", who) {
                Some(n) => RuName::new(n.clone()),
                None => RuName::fixed(who),
            },
        };
        let renamed = self.adjectives.iter().map(String::as_str);
        name.adjectives = renamed.filter_map(|a| lex.adjective(a).cloned()).collect();
        if self.your {
            name.possessive = lex.adjective("your").cloned();
            name.own = lex.adjective("own").cloned();
        }
        name.count = self.count;
        name
    }
}

/// A word the renderer joins names with ("по имени").
pub(super) fn link<'a>(lex: &'a Lexicon, key: &'a str) -> &'a str {
    lex.get("part", key).and_then(|e| e.fixed()).unwrap_or(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::Case;
    use crate::phrase::Phrase;

    fn parse(text: &str) -> MonsterName {
        Lexicon::ru()
            .parse_monster(text, false)
            .unwrap_or_else(|| panic!("{text:?}"))
    }

    fn ru(text: &str, case: Case) -> String {
        parse(text).ru(Lexicon::ru()).form(case)
    }

    #[test]
    fn articles_adjectives_and_possessives() {
        assert_eq!(parse("the invisible newt").adjectives, ["invisible"]);
        assert_eq!(ru("the invisible newt", Case::Acc), "невидимого тритона");
        assert_eq!(ru("Your little dog", Case::Ins), "вашей собачкой");
        assert_eq!(ru("a saddled pony", Case::Dat), "осёдланному пони");
        assert_eq!(ru("The poor newt", Case::Nom), "бедный тритон");
        assert_eq!(ru("newts", Case::Gen), "тритонов");
    }

    #[test]
    fn uniques_and_proper_names() {
        assert_eq!(ru("Medusa", Case::Gen), "Медузы");
        assert_eq!(ru("the Oracle", Case::Dat), "Оракулу");
        assert_eq!(ru("Vlad the Impaler", Case::Ins), "Владом Цепешем");
        assert_eq!(ru("the Wizard of Yendor", Case::Acc), "Волшебника Йендора");
    }

    #[test]
    fn names_titles_priests_ghosts_and_pronouns() {
        let lex = Lexicon::ru();
        assert_eq!(
            lex.parse_monster("Fido", false).unwrap().kind,
            MonsterKind::Name("Fido".into())
        );
        assert!(lex.parse_monster("Fido", true).is_none());
        let pet = lex.parse_monster("Slasher", true).unwrap();
        assert_eq!(pet.kind, MonsterKind::Name("Slasher".into()));
        let zlaw = lex.parse_monster("Zlaw", true).unwrap().ru(lex);
        assert_eq!(zlaw.gender(), crate::grammar::Gender::Fem);
        assert_eq!(ru("dog called Fido", Case::Gen), "собаки по имени Fido");
        assert_eq!(
            ru("Asidonhopo the invisible shopkeeper", Case::Dat),
            "невидимому лавочнику Асидонхопо"
        );
        assert_eq!(ru("Izchak the shopkeeper", Case::Dat), "лавочнику Ицхаку");
        assert_eq!(
            ru("the high priestess of Moloch", Case::Nom),
            "верховная жрица Молоха"
        );
        assert_eq!(
            ru("the renegade Angel of Thoth", Case::Acc),
            "мятежного ангела Тота"
        );
        assert_eq!(ru("Bob's ghost", Case::Ins), "привидением Bob");
        assert_eq!(ru("Asidonhopo's", Case::Gen), "Асидонхопо");
        assert_eq!(ru("the gnome's", Case::Gen), "гнома");
        assert_eq!(ru("the gnome lords'", Case::Gen), "лордов гномов");
        assert_eq!(ru("It", Case::Dat), "кому-то");
        assert_eq!(ru("you", Case::Acc), "вас");
        assert_eq!(ru("the stripling", Case::Gen), "новобранца");
        // the welcome's words, read as monsters: a heroine's race and role
        let her = |w: &str| {
            parse(w).ru(lex).agreeing(
                crate::grammar::Gender::Fem,
                crate::grammar::Number::Sing,
                Case::Nom,
            )
        };
        assert_eq!(her("human"), "человеческая");
        assert_eq!(her("Wizard"), "волшебница");
        assert_eq!(her("Archeologist"), "женщина-археолог");
    }
}
