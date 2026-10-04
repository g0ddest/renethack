//! What the renderer asks of a name: a phrase it can put in any case, with
//! the gender and number the words around it agree with. The name parser
//! makes them from the English names the engine prints.

use crate::grammar::{Case, Gender, Number};

/// A Russian phrase made from an English name the engine printed.
pub trait Phrase {
    /// The whole phrase in `case`, numerals agreed: "3 стрелы", "5 стрел";
    /// in the genitive "3 стрел".
    fn form(&self, case: Case) -> String;
    /// The head noun's gender.
    fn gender(&self) -> Gender;
    /// `Plur` when the count is over one or the noun has no singular.
    fn number(&self) -> Number;
    /// The phrase as counted by `n` (written before it), in `case`:
    /// 1 стрелу, 2 стрелы, 5 стрел; in the genitive 2 стрел
    /// ([`counted_form`](crate::counted_form) says which form). By default
    /// its form in that case.
    fn counted(&self, n: u64, case: Case) -> String {
        let _ = n;
        self.form(case)
    }
    /// The phrase with its "your" as свой, for a sentence whose subject
    /// is the hero ("Вы бьёте своим топором"). By default its form.
    fn own(&self, case: Case) -> String {
        self.form(case)
    }
    /// The phrase agreeing with a noun of that gender and number, in
    /// `case`: a word that is an adjective takes that gender ("lawful":
    /// законопослушная), a role its feminine. By default its form in that
    /// case.
    fn agreeing(&self, gender: Gender, number: Number, case: Case) -> String {
        let _ = (gender, number);
        self.form(case)
    }
}

/// What an argument of a message names, as the catalog knows it from the C
/// expression that printed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameKind {
    /// `doname`, `xname`, `an(xname(obj))`, `yname`...
    Object,
    /// `mon_nam`, `Monnam`, `a_monnam`...
    Monster,
    /// A word of a closed set: a body part, a colour, a surface, terrain...
    Word,
    /// Not known: objects, then monsters, then words.
    Any,
}

/// Parses the English names the engine prints into phrases.
pub trait Names {
    /// `english` as it stood in the message: "the newt", "It", "your little
    /// dog", "a +1 long sword (weapon in hand)".
    fn parse(&self, kind: NameKind, english: &str) -> Option<Box<dyn Phrase>>;
}

/// Text that does not decline: a number, a proper name, words no lexicon
/// knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixed {
    pub text: String,
    pub gender: Gender,
    pub number: Number,
}

impl Fixed {
    /// Masculine singular: the agreement a Russian reader expects of a
    /// word they cannot decline.
    pub fn new(text: impl Into<String>) -> Fixed {
        Fixed {
            text: text.into(),
            gender: Gender::Masc,
            number: Number::Sing,
        }
    }
}

impl Phrase for Fixed {
    fn form(&self, _case: Case) -> String {
        self.text.clone()
    }

    fn gender(&self) -> Gender {
        self.gender
    }

    fn number(&self) -> Number {
        self.number
    }
}

impl<T: Names + ?Sized> Names for &T {
    fn parse(&self, kind: NameKind, english: &str) -> Option<Box<dyn Phrase>> {
        (**self).parse(kind, english)
    }
}
