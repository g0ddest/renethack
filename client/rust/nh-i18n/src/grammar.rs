//! The grammatical categories the lexicon, the name parser and the
//! renderer share: case, gender, number and the Russian plural rule.

use std::fmt;
use std::str::FromStr;

/// The six cases of Russian, in the order of a row of forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Case {
    Nom,
    Gen,
    Dat,
    Acc,
    Ins,
    Prep,
}

impl Case {
    pub const ALL: [Case; 6] = [
        Case::Nom,
        Case::Gen,
        Case::Dat,
        Case::Acc,
        Case::Ins,
        Case::Prep,
    ];

    /// The name templates and the lexicon use: "nom", "gen", "dat", "acc",
    /// "ins", "prep".
    pub fn name(self) -> &'static str {
        match self {
            Case::Nom => "nom",
            Case::Gen => "gen",
            Case::Dat => "dat",
            Case::Acc => "acc",
            Case::Ins => "ins",
            Case::Prep => "prep",
        }
    }

    /// The position in `ALL`: the index into a row of six forms.
    pub fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Case {
    type Err = String;

    fn from_str(s: &str) -> Result<Case, String> {
        Case::ALL
            .into_iter()
            .find(|c| c.name() == s)
            .ok_or_else(|| format!("no case {s:?} (nom, gen, dat, acc, ins, prep)"))
    }
}

/// The gender of a noun (a plural noun keeps its gender: agreement in the
/// plural ignores it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gender {
    Masc,
    Fem,
    Neut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Number {
    Sing,
    Plur,
}

/// The form a Russian noun takes after a number: 1 стрела, 2 стрелы,
/// 5 стрел.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plural {
    /// 1, 21, 31... 101 (not 11)
    One,
    /// 2–4, 22–24... (not 12–14)
    Few,
    /// 0, 5–20, 25–30...
    Many,
}

/// The plural form for `n` things.
pub fn plural_category(n: u64) -> Plural {
    let (n10, n100) = (n % 10, n % 100);
    if n10 == 1 && n100 != 11 {
        Plural::One
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        Plural::Few
    } else {
        Plural::Many
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_categories_follow_the_last_two_digits() {
        let one = [1, 21, 31, 101, 1001, 121];
        let few = [2, 3, 4, 22, 24, 102, 1023];
        let many = [0, 5, 9, 10, 11, 12, 13, 14, 15, 20, 25, 100, 111, 112, 1011];
        assert!(one.iter().all(|&n| plural_category(n) == Plural::One));
        assert!(few.iter().all(|&n| plural_category(n) == Plural::Few));
        assert!(many.iter().all(|&n| plural_category(n) == Plural::Many));
    }

    #[test]
    fn cases_round_trip_through_their_names() {
        for (i, c) in Case::ALL.into_iter().enumerate() {
            assert_eq!(c.index(), i);
            assert_eq!(c.name().parse::<Case>(), Ok(c));
        }
        assert!("вин".parse::<Case>().is_err());
    }
}
