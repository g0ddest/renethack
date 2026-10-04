//! A parsed name in Russian: a noun phrase that can be put in any case,
//! counted, owned, with the adjectives before it agreeing with it and the
//! words after it (a genitive, a name, a parenthesized state) staying as
//! they are.

use std::sync::Arc;

use crate::grammar::{Case, Gender, Number, Plural, counted_form, plural_category};
use crate::lexicon::{Adjective, Noun};
use crate::phrase::Phrase;

use super::status::Status;

/// How many things a name counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Count {
    /// One thing: "a", "an", "the", or no count.
    One,
    /// The number the engine printed: "2 daggers".
    Exactly(u64),
    /// "some" (a number not known), or a plural name with no count:
    /// the plural.
    Some,
}

/// What follows the head noun.
#[derive(Debug, Clone)]
pub enum Tail {
    /// Words as they are: "по имени Fido", "с надписью ZELGO MER".
    Text(String),
    /// Another name in the genitive: труп *тритона*.
    Genitive(Box<RuName>),
}

/// A name in Russian.
#[derive(Debug, Clone)]
pub struct RuName {
    /// "your": ваш, agreeing with the name.
    pub possessive: Option<Arc<Adjective>>,
    /// "the 2nd arrow": 2-я стрела.
    pub ordinal: Option<u32>,
    pub count: Count,
    /// Write no number for `count`: the template writes it.
    pub count_hidden: bool,
    /// Before the head, in order, agreeing with it.
    pub adjectives: Vec<Arc<Adjective>>,
    /// "+1", between the adjectives and the head.
    pub enchantment: Option<i32>,
    pub head: Arc<Noun>,
    /// The thing a count of the head is made of (пара, набор).
    pub unit: Option<Arc<Noun>>,
    pub tails: Vec<Tail>,
    /// Parenthesized states: "(в руке)".
    pub statuses: Vec<Status>,
    /// A price quote: " {покупка 10-20}".
    pub quote: Option<String>,
}

impl RuName {
    pub fn new(head: Arc<Noun>) -> RuName {
        RuName {
            possessive: None,
            ordinal: None,
            count: Count::One,
            count_hidden: false,
            adjectives: Vec::new(),
            enchantment: None,
            head,
            unit: None,
            tails: Vec::new(),
            statuses: Vec::new(),
            quote: None,
        }
    }

    /// A name that never changes: masculine singular.
    pub fn fixed(text: &str) -> RuName {
        RuName::new(Arc::new(Noun::fixed(text, Gender::Masc)))
    }

    /// The gender, number and animacy the words around the name agree
    /// with, and the noun that sets them (the unit of a counted pair).
    fn agreement(&self) -> (Gender, Number) {
        let head_number = if self.head.plural_only {
            Number::Plur
        } else {
            Number::Sing
        };
        match self.count {
            Count::One => (self.head.gender, head_number),
            Count::Some => (self.head.gender, Number::Plur),
            Count::Exactly(n) => {
                let gender = match &self.unit {
                    Some(u) => u.gender,
                    None => self.head.gender,
                };
                if plural_category(n) == Plural::One
                    && !(self.unit.is_none() && self.head.plural_only)
                {
                    (gender, Number::Sing)
                } else {
                    (gender, Number::Plur)
                }
            }
        }
    }

    /// The head and the words that agree with it, in `case`.
    fn core(&self, case: Case) -> Vec<String> {
        let mut words: Vec<String> = Vec::new();
        let anim = self.head.animate;
        let g = self.head.gender;
        match self.count {
            Count::Exactly(n) if self.unit.is_some() && n != 1 => {
                // 2 пары кожаных перчаток: the number counts the unit; the
                // thing itself is in the genitive of its own number
                let unit = self.unit.as_ref().expect("unit");
                if let Some(p) = &self.possessive {
                    words.push(possessive_counted(p, unit.gender, n, case, false).to_string());
                }
                if !self.count_hidden {
                    words.push(n.to_string());
                }
                let (num, c) = counted_form(n, case, false);
                words.push(match num {
                    Number::Sing if c == Case::Gen && plural_category(n) == Plural::Few => {
                        unit.few().to_string()
                    }
                    Number::Sing => unit.singular(c).to_string(),
                    Number::Plur => unit.plural(c).to_string(),
                });
                let own = if self.head.plural_only {
                    Number::Plur
                } else {
                    Number::Sing
                };
                self.push_inner(&mut words, own, Case::Gen);
            }
            Count::Exactly(n) => {
                if let Some(p) = &self.possessive {
                    words.push(possessive_counted(p, g, n, case, anim).to_string());
                }
                if !self.count_hidden {
                    words.push(n.to_string());
                }
                let cat = plural_category(n);
                let (num, c) = counted_form(n, case, anim);
                if cat == Plural::One {
                    self.push_inner(&mut words, Number::Sing, case);
                } else if num == Number::Sing && c == Case::Gen {
                    // after 2, 3, 4 in the nominative: 2 благословенных
                    // длинных меча, 2 благословенные стрелы
                    let adj_case = if g == Gender::Fem {
                        Case::Nom
                    } else {
                        Case::Gen
                    };
                    self.push_ordinal(&mut words, Number::Plur, adj_case, false);
                    for a in &self.adjectives {
                        words.push(a.form(g, Number::Plur, false, adj_case).to_string());
                    }
                    self.push_enchantment(&mut words);
                    words.push(if self.head.has_plural() {
                        self.head.few().to_string()
                    } else {
                        self.head.singular(Case::Gen).to_string()
                    });
                } else {
                    self.push_inner(&mut words, Number::Plur, c);
                }
            }
            Count::Some => {
                if let Some(p) = &self.possessive {
                    words.push(p.form(g, Number::Plur, anim, case).to_string());
                }
                self.push_inner(&mut words, Number::Plur, case);
            }
            Count::One => {
                let num = if self.head.plural_only {
                    Number::Plur
                } else {
                    Number::Sing
                };
                if let Some(p) = &self.possessive {
                    words.push(p.form(g, num, anim, case).to_string());
                }
                self.push_inner(&mut words, num, case);
            }
        }
        words
    }

    /// Ordinal, adjectives, enchantment and head, agreeing in `num` and
    /// `case`.
    fn push_inner(&self, words: &mut Vec<String>, num: Number, case: Case) {
        let (g, anim) = (self.head.gender, self.head.animate);
        self.push_ordinal(words, num, case, anim);
        for a in &self.adjectives {
            words.push(a.form(g, num, anim, case).to_string());
        }
        self.push_enchantment(words);
        words.push(match num {
            Number::Sing => self.head.singular(case).to_string(),
            Number::Plur => self.head.plural(case).to_string(),
        });
    }

    fn push_enchantment(&self, words: &mut Vec<String>) {
        if let Some(e) = self.enchantment {
            words.push(format!("{e:+}"));
        }
    }

    fn push_ordinal(&self, words: &mut Vec<String>, num: Number, case: Case, anim: bool) {
        if let Some(n) = self.ordinal {
            words.push(format!(
                "{n}-{}",
                ordinal_ending(self.head.gender, num, anim, case)
            ));
        }
    }

    fn tail_text(&self) -> String {
        let mut out = String::new();
        for t in &self.tails {
            out.push(' ');
            match t {
                Tail::Text(s) => out.push_str(s),
                Tail::Genitive(n) => out.push_str(&n.form(Case::Gen)),
            }
        }
        let (g, num) = self.agreement();
        for s in &self.statuses {
            out.push_str(" (");
            out.push_str(&s.render(g, num));
            out.push(')');
        }
        if let Some(q) = &self.quote {
            out.push_str(" {");
            out.push_str(q);
            out.push('}');
        }
        out
    }
}

/// "ваш" before a counted name: the plural of the case (ваши 2 меча, ваших
/// 2 тритонов), the singular after 1, 21... (ваш 21 меч).
fn possessive_counted(p: &Adjective, g: Gender, n: u64, case: Case, anim: bool) -> &str {
    if plural_category(n) == Plural::One {
        p.form(g, Number::Sing, anim, case)
    } else {
        p.form(g, Number::Plur, anim, case)
    }
}

/// The ending of an ordinal written in digits: 2-я стрела, 2-й кинжал,
/// 2-го кинжала, 2-ю стрелу.
fn ordinal_ending(g: Gender, num: Number, anim: bool, case: Case) -> &'static str {
    match (num, g, case) {
        (Number::Plur, _, Case::Nom) => "е",
        (Number::Plur, _, Case::Acc) if !anim => "е",
        (Number::Plur, _, Case::Ins) => "ми",
        (Number::Plur, _, Case::Dat) => "м",
        (Number::Plur, _, _) => "х",
        (Number::Sing, Gender::Fem, Case::Nom) => "я",
        (Number::Sing, Gender::Fem, Case::Acc) => "ю",
        (Number::Sing, Gender::Fem, _) => "й",
        (Number::Sing, Gender::Masc, Case::Nom) => "й",
        (Number::Sing, Gender::Neut, Case::Nom) => "е",
        (Number::Sing, Gender::Masc, Case::Acc) if !anim => "й",
        (Number::Sing, Gender::Neut, Case::Acc) => "е",
        (Number::Sing, _, Case::Gen | Case::Acc) => "го",
        (Number::Sing, _, Case::Dat) => "му",
        (Number::Sing, _, Case::Ins | Case::Prep | Case::Loc) => "м",
    }
}

impl Phrase for RuName {
    fn form(&self, case: Case) -> String {
        let mut s = self.core(case).join(" ");
        s.push_str(&self.tail_text());
        s
    }

    fn gender(&self) -> Gender {
        self.agreement().0
    }

    fn number(&self) -> Number {
        self.agreement().1
    }

    fn counted(&self, n: u64, case: Case) -> String {
        let mut counted = self.clone();
        counted.count = Count::Exactly(n);
        counted.count_hidden = true;
        counted.form(case)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexicon::Lexicon;

    fn noun(section: &str, key: &str) -> Arc<Noun> {
        Lexicon::ru().noun(section, key).unwrap().clone()
    }

    fn adj(key: &str) -> Arc<Adjective> {
        Lexicon::ru().adjective(key).unwrap().clone()
    }

    fn all(n: &RuName) -> Vec<String> {
        Case::ALL.iter().map(|&c| n.form(c)).collect()
    }

    #[test]
    fn one_thing_declines_with_its_adjectives() {
        let mut n = RuName::new(noun("object", "long sword"));
        n.adjectives = vec![adj("blessed"), adj("rusty")];
        n.enchantment = Some(2);
        assert_eq!(
            all(&n),
            [
                "благословенный ржавый +2 длинный меч",
                "благословенного ржавого +2 длинного меча",
                "благословенному ржавому +2 длинному мечу",
                "благословенный ржавый +2 длинный меч",
                "благословенным ржавым +2 длинным мечом",
                "благословенном ржавом +2 длинном мече",
            ]
        );
        assert_eq!((n.gender(), n.number()), (Gender::Masc, Number::Sing));
    }

    #[test]
    fn numbers_govern_the_name() {
        let mut n = RuName::new(noun("object", "dagger"));
        n.adjectives = vec![adj("uncursed")];
        n.count = Count::Exactly(2);
        assert_eq!(n.form(Case::Nom), "2 непроклятых кинжала");
        assert_eq!(n.form(Case::Gen), "2 непроклятых кинжалов");
        assert_eq!(n.form(Case::Ins), "2 непроклятыми кинжалами");
        n.count = Count::Exactly(5);
        assert_eq!(n.form(Case::Acc), "5 непроклятых кинжалов");
        n.count = Count::Exactly(21);
        assert_eq!(n.form(Case::Nom), "21 непроклятый кинжал");
        assert_eq!(n.number(), Number::Sing);
        let mut arrows = RuName::new(noun("object", "arrow"));
        arrows.adjectives = vec![adj("blessed")];
        arrows.count = Count::Exactly(3);
        assert_eq!(arrows.form(Case::Nom), "3 благословенные стрелы");
        assert_eq!(arrows.form(Case::Dat), "3 благословенным стрелам");
        assert_eq!(arrows.number(), Number::Plur);
    }

    #[test]
    fn animate_things_counted_take_the_genitive() {
        let mut n = RuName::new(noun("monster", "newt"));
        n.count = Count::Exactly(2);
        assert_eq!(n.form(Case::Nom), "2 тритона");
        assert_eq!(n.form(Case::Acc), "2 тритонов");
        n.count = Count::One;
        assert_eq!(n.form(Case::Acc), "тритона");
    }

    #[test]
    fn pairs_are_counted_in_pairs() {
        let mut n = RuName::new(noun("object", "pair of speed boots"));
        n.unit = Some(noun("part", "pair"));
        n.adjectives = vec![adj("blessed")];
        assert_eq!(n.form(Case::Nom), "благословенные сапоги-скороходы");
        assert_eq!(n.number(), Number::Plur);
        n.count = Count::Exactly(2);
        assert_eq!(n.form(Case::Nom), "2 пары благословенных сапог-скороходов");
        assert_eq!(n.form(Case::Dat), "2 парам благословенных сапог-скороходов");
        n.count = Count::Exactly(5);
        assert_eq!(n.form(Case::Nom), "5 пар благословенных сапог-скороходов");
        assert_eq!(n.gender(), Gender::Fem);
    }

    #[test]
    fn your_agrees_with_the_name() {
        let mut n = RuName::new(noun("monster", "little dog"));
        n.possessive = Some(adj("your"));
        assert_eq!(n.form(Case::Nom), "ваша собачка");
        assert_eq!(n.form(Case::Dat), "вашей собачке");
        let mut d = RuName::new(noun("object", "dagger"));
        d.possessive = Some(adj("your"));
        d.count = Count::Exactly(2);
        assert_eq!(d.form(Case::Nom), "ваши 2 кинжала");
        assert_eq!(d.form(Case::Gen), "ваших 2 кинжалов");
    }

    #[test]
    fn tails_stay_and_ordinals_agree() {
        let mut corpse = RuName::new(noun("object", "corpse"));
        corpse.adjectives = vec![adj("partly eaten")];
        corpse.tails.push(Tail::Genitive(Box::new(RuName::new(noun(
            "monster", "newt",
        )))));
        assert_eq!(corpse.form(Case::Ins), "надкусанным трупом тритона");
        let mut arrow = RuName::new(noun("object", "arrow"));
        arrow.ordinal = Some(2);
        assert_eq!(arrow.form(Case::Nom), "2-я стрела");
        assert_eq!(arrow.form(Case::Acc), "2-ю стрелу");
    }

    #[test]
    fn counted_by_the_template_writes_no_number() {
        let n = RuName::new(noun("object", "arrow"));
        assert_eq!(n.counted(5, Case::Nom), "стрел");
        assert_eq!(n.counted(2, Case::Nom), "стрелы");
        assert_eq!(n.counted(1, Case::Acc), "стрелу");
    }
}
