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
fn an_owner_the_lexicon_reads_without_its_mark() {
    // a god's name (a quest text's "%ds") is a word, not a monster
    let t = with(&[(
        "\"You have prevailed, %s!  %s is surely with you.  Now,\n\
         you must take the Amulet, and sacrifice it on %s altar on\n\
         the Astral Plane.  I suspect that I shall never see you again in this\n\
         life, but I hope to at %s feet.\"",
        r#"ru = "«{1}, {2}: на алтаре {3:gen}, у ног {4:gen}.»""#,
    )]);
    let out = t.window(
        "\"You have prevailed, Hero!  Shan Lai Ching is surely with you.  Now,\n\
         you must take the Amulet, and sacrifice it on Shan Lai Ching's altar on\n\
         the Astral Plane.  I suspect that I shall never see you again in this\n\
         life, but I hope to at Shan Lai Ching's feet.\"",
    );
    assert_eq!(
        (out.text.as_str(), out.status),
        (
            "«Hero, Шань Лай Цин: на алтаре Шань Лай Цин, у ног Шань Лай Цин.»",
            Status::Translated
        )
    );
}

#[test]
fn a_capital_where_the_english_has_one() {
    let t = translator();
    // an inventory letter keeps its case, and a format of conversions
    // alone holds the line rather than an untranslated "%s gold %s."
    let out = t.message(None, &[], "x - 12 gold pieces.");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("x - 12 золотых монет.", Status::Translated)
    );
    assert_eq!(t.text("newt corpse").text, "труп тритона");
}

#[test]
fn a_heading_in_its_padding() {
    let t = with(&[("General", r#"ru = "Общие""#)]);
    let out = t.text(" General                        ");
    assert_eq!(
        (out.text.as_str(), out.status),
        (" Общие                        ", Status::Translated)
    );
}

#[test]
fn a_piece_the_engine_lower_cased() {
    // weapon_insight: lcase(skill_level_name()), "Unskilled" made small
    let t = with(&[
        (
            " You are %s in %s.",
            r#"ru = " Ваш уровень владения {2:ins}: {1}.""#,
        ),
        ("Unskilled", r#"ru = "Неопытный""#),
    ]);
    let out = t.text(" You are unskilled in dagger.");
    assert_eq!(
        (out.text.as_str(), out.status),
        (
            " Ваш уровень владения кинжалом: неопытный.",
            Status::Translated
        )
    );
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
    let t = with(&[
        ("You kill %s!", r#"ru = "Вы убиваете {1:acc}!""#),
        (
            "jumbo shrimp",
            r#"ru = "гигантская креветка"
forms = ["гигантская креветка", "гигантской креветки", "гигантской креветке", "гигантскую креветку", "гигантской креветкой", "гигантской креветке"]
gender = "f""#,
        ),
    ]);
    for en in ["You kill the jumbo shrimp!", "You kill a jumbo shrimp!"] {
        let out = t.message(None, &[], en);
        assert_eq!(
            (out.text.as_str(), out.status),
            ("Вы убиваете гигантскую креветку!", Status::Translated),
            "{en}"
        );
    }
}

/// The built-in catalog and lexicon with these translations only: (the
/// English format, the rest of its entry).
fn with(entries: &[(&str, &str)]) -> Translator {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/catalog.en.json"),
    )
    .unwrap();
    let catalog = Catalog::parse(&text).unwrap();
    let mut toml = String::new();
    for (fmt, rest) in entries {
        let i = catalog.by_fmt(fmt).unwrap_or_else(|| panic!("{fmt}"));
        let en = serde_json::to_string(fmt).unwrap();
        toml += &format!("[{}]\nen = {en}\n{rest}\n\n", catalog.templates()[i].id);
    }
    let russian = Russian::parse(&[("t.toml".into(), toml)]).unwrap();
    Translator::new(catalog, russian, Box::new(Lexicon::ru()))
}
