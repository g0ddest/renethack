//! The parenthesized states doname() appends: "(weapon in right hand)",
//! "(being worn; slippery)", "(unpaid, 15 zorkmids)", "(0:5)", "(lit)",
//! "(attached to your little dog)"... Each English group parses to a
//! [`Status`]; its Russian comes from the lexicon's "status" section,
//! where a pattern may agree with the item ({надет|надета|надето|надеты}:
//! masculine, feminine, neuter, plural) and name a hand ({hand:prep}),
//! a monster ({monster:dat}), a number ({n}), a price ({price}) or a colour
//! ({color:ins}).

use std::sync::Arc;

use crate::grammar::{Case, Gender, Number, Plural, counted_form, plural_category};
use crate::lexicon::{Adjective, Lexicon, Noun};
use crate::phrase::Phrase;

use super::Index;
use super::english::makeplural;
use super::ru::RuName;

/// A hand, claw or tentacle: the body part a weapon is in.
#[derive(Debug, Clone)]
pub struct Hand {
    pub noun: Arc<Noun>,
    /// "hands": both of them.
    pub plural: bool,
    /// "right" or "left", agreeing with the part.
    pub side: Option<Arc<Adjective>>,
}

impl Hand {
    fn render(&self, case: Case) -> String {
        let num = if self.plural {
            Number::Plur
        } else {
            Number::Sing
        };
        let noun = match num {
            Number::Sing => self.noun.singular(case),
            Number::Plur => self.noun.plural(case),
        };
        match &self.side {
            Some(a) => format!("{} {noun}", a.form(self.noun.gender, num, false, case)),
            None => noun.to_string(),
        }
    }
}

/// What a status pattern names besides the item: {hand}, {monster}, {n},
/// {price}, {color}.
#[derive(Debug, Clone, Default)]
pub struct Slots {
    pub hand: Option<Hand>,
    pub monster: Option<Box<RuName>>,
    pub n: Option<String>,
    pub price: Option<(u64, Arc<Noun>)>,
    pub color: Option<Arc<Adjective>>,
}

/// One parenthesized group.
#[derive(Debug, Clone)]
pub enum Status {
    /// A pattern of the lexicon's "status" section and what it names.
    Pattern(String, Slots),
    /// Wand charges as the engine writes them: "1:4".
    Charges(String),
    /// Several parts of one group: "(weapon in hand, brightly lit)".
    Joined(Vec<Status>, &'static str),
    /// A group no pattern knows: kept in English.
    English(String),
}

impl Status {
    /// The group in Russian, agreeing with an item of that gender and
    /// number.
    pub fn render(&self, g: Gender, num: Number) -> String {
        match self {
            Status::Charges(c) => c.clone(),
            Status::English(e) => e.clone(),
            Status::Joined(parts, sep) => parts
                .iter()
                .map(|p| p.render(g, num))
                .collect::<Vec<_>>()
                .join(sep),
            Status::Pattern(ru, slots) => fill(ru, g, num, slots),
        }
    }

    /// Whether the group is translated (no English left).
    pub fn is_russian(&self) -> bool {
        match self {
            Status::English(_) => false,
            Status::Joined(parts, _) => parts.iter().all(Status::is_russian),
            _ => true,
        }
    }
}

/// A pattern with its slots filled: {m|f|n|pl} by the item, {name:case}
/// from `slots`.
fn fill(ru: &str, g: Gender, num: Number, slots: &Slots) -> String {
    let mut out = String::new();
    let mut rest = ru;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        if inner.contains('|') {
            // {m|f|n|pl}: agreement with the item
            let opts: Vec<&str> = inner.split('|').collect();
            let i = match (num, g) {
                (Number::Plur, _) => 3,
                (_, Gender::Masc) => 0,
                (_, Gender::Fem) => 1,
                (_, Gender::Neut) => 2,
            };
            out.push_str(opts.get(i).or(opts.last()).unwrap_or(&""));
            continue;
        }
        let (name, case) = match inner.split_once(':') {
            Some((name, case)) => (name, case.parse::<Case>().unwrap_or(Case::Nom)),
            None => (inner, Case::Nom),
        };
        let value = match name {
            "hand" => slots.hand.as_ref().map(|h| h.render(case)),
            "monster" => slots.monster.as_ref().map(|m| m.form(case)),
            "n" => slots.n.clone(),
            "price" => slots.price.as_ref().map(|(n, noun)| counted(*n, noun)),
            "color" => slots
                .color
                .as_ref()
                .map(|c| c.form(Gender::Neut, Number::Sing, false, case).to_string()),
            _ => Some(format!("{{{inner}}}")),
        };
        out.push_str(&value.unwrap_or_default());
    }
    out.push_str(rest);
    out
}

/// "15 зоркмидов": a number with its noun in the nominative.
pub fn counted(n: u64, noun: &Noun) -> String {
    let (number, case) = counted_form(n, Case::Nom, false);
    let word = match number {
        Number::Sing if case == Case::Gen && plural_category(n) == Plural::Few => noun.few(),
        Number::Sing => noun.singular(case),
        Number::Plur => noun.plural(case),
    };
    format!("{n} {word}")
}

/// The English group (without its parentheses) as a status, or None.
pub(super) fn parse(lex: &Lexicon, index: &Index, group: &str) -> Status {
    if let Some(s) = parse_one(lex, index, group) {
        return s;
    }
    // "being worn; slippery" is a pattern of its own; other groups join
    // parts with "; " or ", " (weapon in hand, brightly lit / for sale, 10
    // zorkmids, 5 aum)
    for sep in ["; ", ", "] {
        if let Some((a, b)) = group.rsplit_once(sep) {
            let first = parse(lex, index, a);
            if first.is_russian()
                && let Some(second) = parse_one(lex, index, b)
            {
                return Status::Joined(vec![first, second], sep);
            }
        }
    }
    Status::English(group.to_string())
}

fn status_ru<'a>(lex: &'a Lexicon, key: &str) -> Option<&'a str> {
    lex.get("status", key)?.fixed()
}

fn parse_one(lex: &Lexicon, index: &Index, group: &str) -> Option<Status> {
    let pattern =
        |key: &str, slots: Slots| Some(Status::Pattern(status_ru(lex, key)?.to_string(), slots));
    if status_ru(lex, group).is_some() {
        return pattern(group, Slots::default());
    }
    // charges: "1:4", "0:-1"
    if let Some((a, b)) = group.split_once(':')
        && a.parse::<i64>().is_ok()
        && b.parse::<i64>().is_ok()
    {
        return Some(Status::Charges(group.to_string()));
    }
    // a hand: "weapon in right hand", "on left claw", "tethered to hands"
    for lead in ["weapon in", "wielded in", "tethered to", "on"] {
        if let Some(rest) = group.strip_prefix(lead).and_then(|r| r.strip_prefix(' '))
            && let Some(hand) = parse_hand(lex, index, rest)
        {
            let slots = Slots {
                hand: Some(hand),
                ..Slots::default()
            };
            return pattern(&format!("{lead} {{hand}}"), slots);
        }
    }
    // the Candelabrum: "3 of 7 candles attached", "1 of 7 candle, lit"
    if let Some((n, rest)) = group.split_once(" of 7 candle")
        && n.parse::<u32>().is_ok()
    {
        let key = match rest.strip_prefix('s').unwrap_or(rest) {
            " attached" => "{n} of 7 candles attached",
            ", lit" => "{n} of 7 candles, lit",
            _ => return None,
        };
        let slots = Slots {
            n: Some(n.to_string()),
            ..Slots::default()
        };
        return pattern(key, slots);
    }
    // a leash: "attached to your little dog"
    if let Some(who) = group.strip_prefix("attached to ")
        && who != "you"
        && let Some(m) = super::monster::parse_monster(lex, index, who, true)
    {
        let slots = Slots {
            monster: Some(Box::new(m.ru(lex))),
            ..Slots::default()
        };
        return pattern("attached to {monster}", slots);
    }
    // prices: "unpaid, 15 zorkmids", "for sale, 3 Altarian Dollars"
    for what in ["unpaid", "contents", "for sale"] {
        if let Some(price) = group.strip_prefix(what).and_then(|r| r.strip_prefix(", "))
            && let Some(price) = parse_price(lex, index, price)
        {
            let slots = Slots {
                price: Some(price),
                ..Slots::default()
            };
            return pattern(&format!("{what}, {{price}}"), slots);
        }
    }
    // wizard mode weight: "5 aum"
    if let Some(n) = group.strip_suffix(" aum")
        && n.parse::<u64>().is_ok()
    {
        let slots = Slots {
            n: Some(n.to_string()),
            ..Slots::default()
        };
        return pattern("{n} aum", slots);
    }
    // Sting and Orcrist: "glimmering light blue"
    for verb in ["quivering", "flickering", "glimmering", "gleaming"] {
        if let Some(color) = group.strip_prefix(verb).and_then(|r| r.strip_prefix(' '))
            && let Some(adj) = lex.get("color", color).and_then(|e| e.adjective())
        {
            let slots = Slots {
                color: Some(adj.clone()),
                ..Slots::default()
            };
            return pattern(&format!("{verb} {{color}}"), slots);
        }
    }
    None
}

/// "right hand", "left claw", "hands", "tentacle".
fn parse_hand(lex: &Lexicon, index: &Index, text: &str) -> Option<Hand> {
    let (side, part) = if let Some(p) = text.strip_prefix("right ") {
        (lex.adjective("right").cloned(), p)
    } else if let Some(p) = text.strip_prefix("left ") {
        (lex.adjective("left").cloned(), p)
    } else {
        (None, text)
    };
    if let Some(noun) = lex.noun("bodypart", part) {
        return Some(Hand {
            noun: noun.clone(),
            plural: false,
            side,
        });
    }
    let key = index.bodypart_plurals.get(part)?;
    Some(Hand {
        noun: lex.noun("bodypart", key)?.clone(),
        plural: true,
        side,
    })
}

/// "15 zorkmids", "1 zorkmid", "3 Altarian Dollars".
fn parse_price(lex: &Lexicon, index: &Index, text: &str) -> Option<(u64, Arc<Noun>)> {
    let (n, currency) = text.split_once(' ')?;
    let amount = n.parse::<u64>().ok()?;
    let key = if lex.get("currency", currency).is_some() {
        currency.to_string()
    } else {
        index.currency_plurals.get(currency)?.clone()
    };
    Some((amount, lex.noun("currency", &key)?.clone()))
}

/// The plurals of body parts and currencies, as makeplural() spells them.
pub(super) fn plurals(lex: &Lexicon, section: &str) -> std::collections::HashMap<String, String> {
    lex.entries(section)
        .map(|(k, _)| (makeplural(k), k.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Index;

    fn render(group: &str, g: Gender, num: Number) -> String {
        let lex = Lexicon::ru();
        let index = Index::new(lex);
        parse(lex, &index, group).render(g, num)
    }

    #[test]
    fn worn_things_agree_with_the_item() {
        assert_eq!(render("being worn", Gender::Masc, Number::Sing), "надет");
        assert_eq!(render("being worn", Gender::Fem, Number::Sing), "надета");
        assert_eq!(render("being worn", Gender::Masc, Number::Plur), "надеты");
        assert_eq!(
            render("being worn; slippery", Gender::Masc, Number::Plur),
            "надеты; скользкие"
        );
    }

    #[test]
    fn hands_claws_and_tentacles() {
        assert_eq!(
            render("weapon in right hand", Gender::Masc, Number::Sing),
            "оружие в правой руке"
        );
        assert_eq!(
            render("weapon in hands", Gender::Masc, Number::Sing),
            "оружие в руках"
        );
        assert_eq!(
            render("weapon in left tentacle", Gender::Masc, Number::Sing),
            "оружие в левом щупальце"
        );
        assert_eq!(
            render("on left hand", Gender::Neut, Number::Sing),
            "на левой руке"
        );
        assert_eq!(
            render("tethered to right paw", Gender::Masc, Number::Sing),
            "привязан к правой лапе"
        );
    }

    #[test]
    fn prices_charges_candles_and_more() {
        assert_eq!(
            render("unpaid, 15 zorkmids", Gender::Masc, Number::Sing),
            "не оплачено, 15 зоркмидов"
        );
        assert_eq!(
            render("for sale, 2 zorkmids", Gender::Masc, Number::Sing),
            "на продажу, 2 зоркмида"
        );
        assert_eq!(render("1:4", Gender::Fem, Number::Sing), "1:4");
        assert_eq!(
            render("3 of 7 candles attached", Gender::Masc, Number::Sing),
            "свечей: 3 из 7"
        );
        assert_eq!(render("lit", Gender::Fem, Number::Plur), "горят");
        assert_eq!(
            render("weapon in hand, brightly lit", Gender::Masc, Number::Sing),
            "оружие в руке, светится"
        );
        assert_eq!(
            render(
                "weapon in hand, glimmering light blue",
                Gender::Masc,
                Number::Sing
            ),
            "оружие в руке, поблёскивает светло-синим"
        );
        assert_eq!(
            render("attached to your little dog", Gender::Masc, Number::Sing),
            "привязан к вашей собачке"
        );
        assert_eq!(
            render("for sale, 10 zorkmids, 5 aum", Gender::Masc, Number::Sing),
            "на продажу, 10 зоркмидов, 5 аум"
        );
    }

    #[test]
    fn an_unknown_group_stays_english() {
        let lex = Lexicon::ru();
        let index = Index::new(lex);
        let s = parse(lex, &index, "humming softly");
        assert!(!s.is_russian());
        assert_eq!(s.render(Gender::Masc, Number::Sing), "humming softly");
    }
}
