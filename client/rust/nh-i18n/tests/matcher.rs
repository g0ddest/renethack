//! The matcher on the lines it once got wrong, over the built-in catalog,
//! translations and lexicon: the names the game wrote side by side, the
//! hero in a monster's place, a %s before a %c, a hallucinated name, an
//! engraving a level's Lua builds.

use std::sync::OnceLock;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Catalog, Russian, Status, Translator};

fn translator() -> &'static Translator {
    static T: OnceLock<Translator> = OnceLock::new();
    T.get_or_init(|| Translator::built_in().expect("the built-in translator"))
}

/// The Russian of a message the engine sent without its format, which
/// must be all Russian and hold `want`.
fn check(en: &str, want: &str) {
    let out = translator().message(None, &[], en);
    assert_eq!(out.status, Status::Translated, "{en} → {}", out.text);
    assert!(
        !out.text.chars().any(|c| c.is_ascii_alphabetic()),
        "{en} → {}",
        out.text
    );
    assert!(out.text.contains(want), "{en} → {} (no {want:?})", out.text);
}

#[test]
fn an_owner_goes_with_its_name() {
    // "%s %s falls to the %s.": the shortest first capture was "The"
    check("The pony's saddle falls to the floor.", "Седло пони падает");
    check(
        "Your pony's saddle falls to the floor.",
        "Седло вашего пони",
    );
    check("The newt's cloak gets wet.", "Плащ тритона намокает");
    check(
        "The giant beetle's cloak gets wet.",
        "Плащ гигантского жука",
    );
    // "%s welds itself to %s %s!": "the" | "gnome's hand" before
    check(
        "The crossbow welds itself to the gnome's hand!",
        "к руке гнома",
    );
}

#[test]
fn a_corpse_is_a_name_the_lexicon_reads() {
    // not "%s corpse" nor "One of %s" taking it whole, untranslated
    check("The newt corpse disappears.", "Труп тритона исчезает");
    check("Your newt corpse disappears!", "Ваш труп тритона исчезает");
    check(
        "One of your newt corpses disappears!",
        "Один из ваших трупов тритона",
    );
}

#[test]
fn the_hero_fills_a_monster_s_place() {
    check("The rock hits you but doesn't hurt.", "попадает в вас");
    // "your " of a buffer the format glues to a word
    check("Your shirt is obscured by your mail.", "Вашу рубашку");
}

#[test]
fn a_text_before_a_character() {
    // "This %s tastes %s%c": the %s ends where the "." begins
    check("This newt corpse tastes okay.", "Этот труп тритона на вкус");
}

#[test]
fn an_engraving_of_a_lua_local() {
    // themerms.lua: "Dig" .. dig, dig a string.format() of steps
    check(
        "You read: \"Dig 3 east 2 south\".",
        "«Копай: 3 шага на восток, 2 шага на юг»",
    );
    check("You read: \"Dig 1 west\".", "«Копай: 1 шаг на запад»");
    check("You read: \"Dig here\".", "«Копай здесь»");
}

#[test]
fn a_hallucinated_name_behind_an_article() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/catalog.en.json"),
    )
    .unwrap();
    let catalog = Catalog::parse(&text).unwrap();
    let id = |fmt: &str| {
        let i = catalog.by_fmt(fmt).unwrap_or_else(|| panic!("{fmt}"));
        catalog.templates()[i].id.clone()
    };
    let russian = Russian::parse(&[(
        "t.toml".into(),
        format!(
            r#"
[{}]
en = "You kill %s!"
ru = "Вы убиваете {{1:acc}}!"

[{}]
en = "jumbo shrimp"
ru = "гигантская креветка"
forms = ["гигантская креветка", "гигантской креветки", "гигантской креветке", "гигантскую креветку", "гигантской креветкой", "гигантской креветке"]
gender = "f"
"#,
            id("You kill %s!"),
            id("jumbo shrimp")
        ),
    )])
    .unwrap();
    let t = Translator::new(catalog, russian, Box::new(Lexicon::ru()));
    for (en, ru) in [
        (
            "You kill the jumbo shrimp!",
            "Вы убиваете гигантскую креветку!",
        ),
        (
            "You kill a jumbo shrimp!",
            "Вы убиваете гигантскую креветку!",
        ),
    ] {
        let out = t.message(None, &[], en);
        assert_eq!(
            (out.text.as_str(), out.status),
            (ru, Status::Translated),
            "{en}"
        );
    }
}
