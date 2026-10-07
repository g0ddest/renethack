//! pager.c's line about a map symbol, as `;` and `/` print it, over the
//! built-in catalog, translations and lexicon. The names are whatever the
//! lexicon says: the tests are of where the line is cut.

use std::sync::OnceLock;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Case, NameKind, Names, Status, Translator};

fn translator() -> &'static Translator {
    static T: OnceLock<Translator> = OnceLock::new();
    T.get_or_init(|| Translator::built_in().expect("the built-in translator"))
}

/// The line in Russian, which must be all Russian.
fn look(en: &str) -> String {
    let out = translator().message(None, &[], en);
    assert_eq!(out.status, Status::Translated, "{en} → {}", out.text);
    out.text
}

/// What the lexicon calls `english`: a word of its closed sets (terrain,
/// the classes), or any name.
fn word(english: &str) -> String {
    name(NameKind::Word, english)
}

fn thing(english: &str) -> String {
    name(NameKind::Any, english)
}

fn name(kind: NameKind, english: &str) -> String {
    Lexicon::ru()
        .parse(kind, english)
        .unwrap_or_else(|| panic!("the lexicon does not read {english:?}"))
        .form(Case::Nom)
}

#[test]
fn an_alternative_with_an_or_of_its_own() {
    assert_eq!(
        look("d        a dog or other canine (tame little dog called Rex)"),
        format!(
            "d        {} ({})",
            word("dog or other canine"),
            thing("tame little dog called Rex")
        )
    );
    // "a human or elf" is one alternative, "you" another
    assert_eq!(
        look("@        a human or elf or you (peaceful watchman)"),
        format!(
            "@        {} или {} ({})",
            word("human or elf"),
            thing("you"),
            thing("peaceful watchman")
        )
    );
}

#[test]
fn as_many_alternatives_as_the_symbol_has() {
    assert_eq!(
        look(
            ".        a doorway or the floor of a room or the dark part of a room or ice (doorway)"
        ),
        format!(
            ".        {} или {} или {} или {} ({})",
            word("doorway"),
            word("floor of a room"),
            word("dark part of a room"),
            word("ice"),
            word("doorway")
        )
    );
    assert_eq!(
        look("#        can be many things (corridor)"),
        format!("#        может означать многое ({})", word("corridor"))
    );
    // a symbol the hero typed has no parentheses after it
    assert_eq!(
        look(")        a weapon"),
        format!(")        {}", word("weapon"))
    );
}

#[test]
fn a_word_of_the_map_not_of_the_inventory() {
    // the rock a level is cut in, not a gem seen from afar
    assert_eq!(
        look("         stone (stone)"),
        format!("         {0} ({0})", word("stone"))
    );
    // the line's own word: the lexicon's "trap" is a status
    assert_eq!(
        look("^        a trap (bear trap)"),
        format!("^        ловушка ({})", word("bear trap"))
    );
}

#[test]
fn parentheses_of_an_alternative_s_own() {
    let item = word("useful item (pick-axe, key, lamp...)");
    assert_eq!(
        look("(        a useful item (pick-axe, key, lamp...)"),
        format!("(        {item}")
    );
    assert_eq!(
        look("(        a useful item (pick-axe, key, lamp...) (pick-axe)"),
        format!("(        {item} ({})", thing("pick-axe"))
    );
}

#[test]
fn what_pager_adds_after_a_name() {
    let cat = word("cat or other feline");
    assert_eq!(
        look(
            "f        a cat or other feline (tame kitten called Tom, can't move \
             (paralyzed or sleeping or busy), leashed to you) [seen: normal vision, telepathy]"
        ),
        format!(
            "f        {cat} ({}, не двигается (паралич, сон или занятость), на вашем поводке) \
             [видно: обычное зрение, телепатия]",
            thing("tame kitten called Tom")
        )
    );
    // two clauses the engine wrote as one
    assert_eq!(
        look("p        a piercer (piercer, hiding on the ceiling)"),
        format!(
            "p        {} ({}, прячется на потолке)",
            word("piercer"),
            thing("piercer")
        )
    );
    assert_eq!(
        look(")        a weapon (dagger embedded in a wall)"),
        format!(
            ")        {} ({} (в стене))",
            word("weapon"),
            thing("dagger")
        )
    );
    // what the player wrote stays
    assert_eq!(
        look(".        an engraving (engraving with remembered text: \"ad aquarium\")"),
        format!(
            ".        {0} ({0} с запомненным текстом: «ad aquarium»)",
            word("engraving")
        )
    );
}

#[test]
fn a_text_of_the_catalog_in_the_parentheses() {
    assert_eq!(
        look("-        the interior of a monster or a wall (interior of the purple worm)"),
        format!(
            "-        нутро монстра или {} (нутро {})",
            word("wall"),
            Lexicon::ru()
                .parse(NameKind::Monster, "the purple worm")
                .unwrap()
                .form(Case::Gen)
        )
    );
}

#[test]
fn a_clause_no_template_has_stays() {
    let out = translator().message(
        None,
        &[],
        "m        a mimic (giant mimic, mimicking a fountain)",
    );
    assert_eq!(
        (out.text, out.status),
        (
            format!(
                "m        {} ({}, mimicking a fountain)",
                word("mimic"),
                thing("giant mimic")
            ),
            Status::Partial
        )
    );
}

#[test]
fn eight_spaces_alone_make_no_such_line() {
    let t = translator();
    for en in ["x        ", "You see here a dagger.", "         "] {
        let out = t.message(None, &[], en);
        assert!(!out.text.contains(" или "), "{en} → {}", out.text);
    }
}
