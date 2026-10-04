//! The English of objnam.c the parser needs: `makeplural()`, to know the
//! plural of every name the lexicon has ("potions of healing", "knives",
//! "dwarves"), and the helpers for articles and capitals.

const VOWELS: &str = "aeiouAEIOU";

/// Pairs that follow no rule, as objnam.c's one_off[].
const ONE_OFF: [(&str, &str); 22] = [
    ("child", "children"),
    ("cubus", "cubi"),
    ("culus", "culi"),
    ("Cyclops", "Cyclopes"),
    ("djinni", "djinn"),
    ("erinys", "erinyes"),
    ("foot", "feet"),
    ("fungus", "fungi"),
    ("goose", "geese"),
    ("knife", "knives"),
    ("labrum", "labra"),
    ("louse", "lice"),
    ("mouse", "mice"),
    ("mumak", "mumakil"),
    ("nemesis", "nemeses"),
    ("ovum", "ova"),
    ("ox", "oxen"),
    ("passerby", "passersby"),
    ("rtex", "rtices"),
    ("serum", "sera"),
    ("staff", "staves"),
    ("tooth", "teeth"),
];

/// Words whose plural is the word itself (as_is[]), and the endings
/// makeplural() leaves alone.
const AS_IS: [&str; 33] = [
    "boots",
    "shoes",
    "gloves",
    "lenses",
    "scales",
    "eyes",
    "gauntlets",
    "iron bars",
    "bison",
    "deer",
    "elk",
    "fish",
    "fowl",
    "tuna",
    "yaki",
    "-hai",
    "krill",
    "manes",
    "moose",
    "ninja",
    "sheep",
    "ronin",
    "roshi",
    "shito",
    "tengu",
    "ki-rin",
    "Nazgul",
    "gunyoki",
    "piranha",
    "samurai",
    "shuriken",
    "haggis",
    "Bordeaux",
];

/// The joints after which only the head before them is pluralized.
const COMPOUNDS: [&str; 17] = [
    " of ",
    " labeled ",
    " called ",
    " named ",
    " above",
    " versus ",
    " from ",
    " in ",
    " on ",
    " a la ",
    " with",
    " de ",
    " d'",
    " du ",
    " au ",
    "-in-",
    "-at-",
];

const NO_MEN: [&str; 36] = [
    "albu",
    "antihu",
    "anti",
    "ata",
    "auto",
    "bildungsro",
    "cai",
    "cay",
    "ceru",
    "corner",
    "decu",
    "des",
    "dura",
    "fir",
    "hanu",
    "het",
    "infrahu",
    "inhu",
    "nonhu",
    "otto",
    "out",
    "prehu",
    "protohu",
    "subhu",
    "superhu",
    "talis",
    "unhu",
    "sha",
    "hu",
    "un",
    "le",
    "re",
    "so",
    "to",
    "at",
    "a",
];

const CH_K: [&str; 19] = [
    "monarch",
    "poch",
    "tech",
    "mech",
    "stomach",
    "psych",
    "amphibrach",
    "anarch",
    "atriarch",
    "azedarach",
    "broch",
    "gastrotrich",
    "isopach",
    "loch",
    "oligarch",
    "peritrich",
    "sandarach",
    "sumach",
    "symposiarch",
];

fn ends(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len()
        && s.is_char_boundary(s.len() - suffix.len())
        && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn badman_plural(base: &str) -> bool {
    if base.len() < 4 {
        return false;
    }
    NO_MEN.iter().any(|p| {
        let Some(spot) = base.len().checked_sub(p.len() + 3) else {
            return false;
        };
        base.is_char_boundary(spot)
            && base.is_char_boundary(spot + p.len())
            && base[spot..spot + p.len()].eq_ignore_ascii_case(p)
            && (spot == 0 || base.as_bytes()[spot - 1] == b' ')
    })
}

fn compound_at(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    (0..s.len()).find(|&i| {
        matches!(lower.as_bytes()[i], b' ' | b'-')
            && COMPOUNDS.iter().any(|c| lower[i..].starts_with(c))
    })
}

/// objnam.c makeplural() for the names of things and creatures.
pub fn makeplural(old: &str) -> String {
    let s = old.trim_start_matches(' ');
    if s.is_empty() {
        return "s".to_string();
    }
    if s.len() >= 8 && s[..8].eq_ignore_ascii_case("pair of ") {
        return s.to_string();
    }
    let (s, excess) = match compound_at(s) {
        Some(at) => (&s[..at], &s[at..]),
        None => (s, ""),
    };
    let s = s.trim_end_matches(' ');
    let plural = |stem: &str, add: &str| format!("{stem}{add}{excess}");
    let n = s.len();
    let last = s.chars().last().unwrap_or(' ');
    if s.chars().count() == 1 || !last.is_alphabetic() {
        return plural(s, "'s");
    }
    if AS_IS
        .iter()
        .chain(["ae", "eaux", "matzot"].iter())
        .any(|a| ends(s, a))
        || (n > 5 && ends(s, "craft"))
    {
        return plural(s, "");
    }
    if s.eq_ignore_ascii_case("slice") || s.eq_ignore_ascii_case("mongoose") {
        return plural(s, "s");
    }
    if n > 2 && ends(s, "ox") && !(n > 5 && ends(s, "muskox")) {
        return plural(s, "es");
    }
    if n > 2 && ends(s, "man") && badman_plural(s) {
        return plural(s, "s");
    }
    for (sing, plur) in ONE_OFF {
        if ends(s, plur) {
            return plural(s, "");
        }
        if ends(s, sing) {
            return plural(&format!("{}{plur}", &s[..n - sing.len()]), "");
        }
    }
    if s.eq_ignore_ascii_case("ya") || ends(s, " ya") {
        return plural(s, "");
    }
    if n >= 3 && ends(s, "man") {
        return plural(&s[..n - 2], "en");
    }
    let lower: Vec<char> = s.to_lowercase().chars().collect();
    let len = lower.len();
    let prev = if len >= 2 { lower[len - 2] } else { ' ' };
    if lower[len - 1] == 'f'
        && !(len >= 3 && ends(s, "erf"))
        && ("lr".contains(prev) || VOWELS.contains(prev))
    {
        return plural(&s[..n - 1], "ves");
    }
    if n >= 3 && ends(s, "ium") {
        return plural(&s[..n - 2], "a");
    }
    if (n >= 4 && ends(s, "alga"))
        || (n >= 5 && (ends(s, "hypha") || ends(s, "larva")))
        || (n >= 6 && ends(s, "amoeba"))
        || (n >= 8 && ends(s, "vertebra"))
    {
        return plural(s, "e");
    }
    if n > 3 && ends(s, "us") && !((n >= 5 && ends(s, "lotus")) || (n >= 6 && ends(s, "wumpus"))) {
        return plural(&s[..n - 2], "i");
    }
    if n >= 3 && ends(s, "sis") {
        return plural(&s[..n - 2], "es");
    }
    if n >= 3 && ends(s, "eau") && !ends(s, "bureau") {
        return plural(s, "x");
    }
    if n >= 6 && (ends(s, "matzoh") || ends(s, "matzah")) {
        return plural(&s[..n - 2], "ot");
    }
    if n >= 5 && (ends(s, "matzo") || ends(s, "matza")) {
        return plural(&s[..n - 1], "ot");
    }
    if n >= 5 && (ends(s, "dex") || ends(s, "dix") || ends(s, "tex")) && !ends(s, "index") {
        return plural(&s[..n - 2], "ices");
    }
    let lo = lower[len - 1];
    if "zxs".contains(lo)
        || (len >= 2
            && lo == 'h'
            && "cs".contains(prev)
            && !(len >= 4 && prev == 'c' && CH_K.iter().any(|k| ends(s, k))))
        || (n >= 4 && ends(s, "ato"))
        || (n >= 5 && ends(s, "dingo"))
    {
        return plural(s, "es");
    }
    if lo == 'y' && !VOWELS.contains(prev) {
        return plural(&s[..n - 1], "ies");
    }
    plural(s, "s")
}

/// `text` with its first letter in lower case, as the engine's lower-case
/// form of a word it capitalized at the start of a sentence (The newt, A
/// dagger, It).
pub fn uncapitalized(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// `text` with its first letter in upper case: "The Gnomish Mines" for
/// the "the Gnomish Mines" stairs_description() writes.
pub fn capitalized(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// `text` without the leading article or word of `prefixes`, case
/// insensitive in its first letter: ("the newt", "The newt") -> "newt".
pub fn strip_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word).or_else(|| {
        let mut w = word.chars();
        let first = w.next()?;
        let upper: String = first.to_uppercase().chain(w).collect();
        text.strip_prefix(upper.as_str())
    })?;
    rest.strip_prefix(' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_follow_objnam() {
        for (one, many) in [
            ("long sword", "long swords"),
            ("potion of healing", "potions of healing"),
            ("scroll labeled ZELGO MER", "scrolls labeled ZELGO MER"),
            ("knife", "knives"),
            ("dwarf", "dwarves"),
            ("staff", "staves"),
            ("homunculus", "homunculi"),
            ("mumak", "mumakil"),
            ("watchman", "watchmen"),
            ("shaman", "shamans"),
            ("human", "humans"),
            ("box", "boxes"),
            ("fox", "foxes"),
            ("ya", "ya"),
            ("vortex", "vortices"),
            ("Cyclops", "Cyclopes"),
            ("pair of leather gloves", "pair of leather gloves"),
            ("lichen corpse", "lichen corpses"),
            ("cockatrice", "cockatrices"),
            ("eucalyptus leaf", "eucalyptus leaves"),
            ("clove of garlic", "cloves of garlic"),
            ("ruby", "rubies"),
            ("forehoof", "forehooves"),
            ("tooth", "teeth"),
            ("worm tooth", "worm teeth"),
            ("samurai", "samurai"),
            ("shuriken", "shuriken"),
            ("valkyrie", "valkyries"),
            ("zorkmid", "zorkmids"),
            ("candle", "candles"),
        ] {
            assert_eq!(makeplural(one), many, "{one}");
        }
    }

    #[test]
    fn words_come_off_in_either_case() {
        assert_eq!(strip_word("the newt", "the"), Some("newt"));
        assert_eq!(strip_word("The newt", "the"), Some("newt"));
        assert_eq!(strip_word("theatre", "the"), None);
        assert_eq!(uncapitalized("It"), "it");
    }
}
