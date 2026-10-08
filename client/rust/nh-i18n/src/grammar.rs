//! The grammatical categories the lexicon, the name parser and the
//! renderer share: case, gender, number and the Russian plural rule.

use std::fmt;
use std::str::FromStr;

/// The six cases of Russian, in the order of a row of forms, and the
/// locative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Case {
    Nom,
    Gen,
    Dat,
    Acc,
    Ins,
    Prep,
    /// The second prepositional, after в and на of a place: на полу, во
    /// льду, на мосту. It has no cell of its own: a noun without one (на
    /// столе), a plural and an adjective take the prepositional.
    Loc,
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
    /// "ins", "prep", "loc".
    pub fn name(self) -> &'static str {
        match self {
            Case::Nom => "nom",
            Case::Gen => "gen",
            Case::Dat => "dat",
            Case::Acc => "acc",
            Case::Ins => "ins",
            Case::Prep => "prep",
            Case::Loc => "loc",
        }
    }

    /// The position in `ALL`: the index into a row of six forms. The
    /// locative's is the prepositional's.
    pub fn index(self) -> usize {
        match self {
            Case::Loc => Case::Prep as usize,
            c => c as usize,
        }
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
            .chain([Case::Loc])
            .find(|c| c.name() == s)
            .ok_or_else(|| format!("no case {s:?} (nom, gen, dat, acc, ins, prep, loc)"))
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

/// Which form of a noun follows the number `n` written in digits, in
/// `case`: (number, case) of the paradigm's cell. In the nominative and
/// the inanimate accusative 1 takes the singular, 2–4 the genitive
/// singular, 5–20 the genitive plural (1 стрела, 2 стрелы, 5 стрел); an
/// animate accusative takes the genitive plural after 2 and more (2
/// тритонов); the other cases the plural of that case (2 стрелам). After
/// 2–4 in the nominative a phrase with adjectives is none of the cells:
/// the noun is in the genitive singular, its adjectives in the genitive
/// plural (2 гигантских муравья) — the lexicon's `few` form.
pub fn counted_form(n: u64, case: Case, animate: bool) -> (Number, Case) {
    let p = plural_category(n);
    if p == Plural::One {
        return (Number::Sing, case);
    }
    match case {
        Case::Nom => match p {
            Plural::Few => (Number::Sing, Case::Gen),
            _ => (Number::Plur, Case::Gen),
        },
        Case::Acc if animate => (Number::Plur, Case::Gen),
        Case::Acc => match p {
            Plural::Few => (Number::Sing, Case::Gen),
            _ => (Number::Plur, Case::Gen),
        },
        other => (Number::Plur, other),
    }
}

/// A preposition of one consonant takes a vowel before a word it would
/// stick to: в → во (во льду, во флаконе), с → со (со свитком, со мной),
/// к → ко (ко мне). A template writes "в {1:loc}" and cannot know the
/// word that comes.
pub fn euphony(text: &str) -> std::borrow::Cow<'_, str> {
    let mut out: Option<String> = None;
    let mut last = 0;
    let mut before: Option<char> = None;
    for (i, c) in text.char_indices() {
        let lone = !before.is_some_and(char::is_alphabetic);
        before = Some(c);
        if !lone || !"вВсСкК".contains(c) {
            continue;
        }
        let after = i + c.len_utf8();
        let Some(rest) = text[after..].strip_prefix(' ') else {
            continue;
        };
        let word: String = rest
            .chars()
            .take_while(|l| l.is_alphabetic())
            .flat_map(char::to_lowercase)
            .collect();
        let takes = match c {
            'в' | 'В' => vo(&word),
            'с' | 'С' => so(&word),
            _ => ko(&word),
        };
        if takes {
            let out = out.get_or_insert_with(String::new);
            out.push_str(&text[last..after]);
            out.push('о');
            last = after;
        }
    }
    match out {
        Some(mut out) => {
            out.push_str(&text[last..]);
            out.into()
        }
        None => text.into(),
    }
}

/// Does a word start with one of `first` and then a consonant?
fn cluster(word: &str, first: &str) -> bool {
    let mut letters = word.chars();
    letters.next().is_some_and(|c| first.contains(c))
        && letters
            .next()
            .is_some_and(|c| "бвгджзйклмнпрстфхцчшщ".contains(c))
}

fn starts(word: &str, stems: &[&str]) -> bool {
    stems.iter().any(|s| word.starts_with(s))
}

/// во власти, во флаконе; во льду, во рту, во сне, во мне, во дворе.
fn vo(word: &str) -> bool {
    cluster(word, "вф")
        || starts(
            word,
            &["льд", "льв", "мгл", "мрак", "мног", "множ", "тьм", "двор"],
        )
        || [
            "рту",
            "лбу",
            "рву",
            "ржи",
            "сне",
            "мне",
            "весь",
            "что",
            "имя",
            "благо",
        ]
        .contains(&word)
}

/// со свитком, со змеёй, со шлемом, со щитом; со мной, со льдом, со дна.
fn so(word: &str) -> bool {
    cluster(word, "сзшж")
        || starts(word, &["щ", "льд", "льв", "мно", "двор", "вс", "лб"])
        || ["рта", "ртом", "дна", "рвом", "лжи"].contains(&word)
}

/// ко мне, ко всем, ко льду, ко дну.
fn ko(word: &str) -> bool {
    starts(word, &["мно", "вс", "двор", "льд", "льв", "втор"])
        || ["мне", "рту", "лбу", "дну", "сну", "рву"].contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_govern_their_noun() {
        let f = |n, c, a| counted_form(n, c, a);
        assert_eq!(f(1, Case::Nom, false), (Number::Sing, Case::Nom));
        assert_eq!(f(21, Case::Acc, true), (Number::Sing, Case::Acc));
        assert_eq!(f(3, Case::Nom, false), (Number::Sing, Case::Gen));
        assert_eq!(f(22, Case::Acc, false), (Number::Sing, Case::Gen));
        assert_eq!(f(5, Case::Acc, false), (Number::Plur, Case::Gen));
        assert_eq!(f(3, Case::Acc, true), (Number::Plur, Case::Gen));
        assert_eq!(f(12, Case::Ins, false), (Number::Plur, Case::Ins));
    }

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

    #[test]
    fn the_locative_reads_the_prepositional_cell() {
        assert_eq!("loc".parse::<Case>(), Ok(Case::Loc));
        assert_eq!(Case::Loc.index(), Case::Prep.index());
        assert!(!Case::ALL.contains(&Case::Loc));
        assert_eq!(counted_form(5, Case::Loc, false), (Number::Plur, Case::Loc));
    }

    #[test]
    fn a_preposition_before_a_word_it_sticks_to() {
        for (written, said) in [
            ("Вы стоите в льду.", "Вы стоите во льду."),
            (
                "В флаконе пусто, в рту сухо.",
                "Во флаконе пусто, во рту сухо.",
            ),
            ("Вы бьёте с свитком в руке", "Вы бьёте со свитком в руке"),
            (
                "с змеёй, с шлемом, с щитом, с мной",
                "со змеёй, со шлемом, со щитом, со мной",
            ),
            ("к мне (к всем)", "ко мне (ко всем)"),
            // nothing to change
            (
                "в полу, в воде, с зомби, с мечом, к двери",
                "в полу, в воде, с зомби, с мечом, к двери",
            ),
            (
                "в рот, в ртути, с ртутью, в двери",
                "в рот, в ртути, с ртутью, в двери",
            ),
            // a letter of a word, an inventory letter
            ("лев с львицей: в - свиток", "лев со львицей: в - свиток"),
            ("x - свиток (в сумке)", "x - свиток (в сумке)"),
        ] {
            assert_eq!(euphony(written), said);
        }
        assert!(matches!(euphony("в полу"), std::borrow::Cow::Borrowed(_)));
    }
}
