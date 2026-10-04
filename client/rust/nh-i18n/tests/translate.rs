//! The translator over the real catalog and the seed translations, with a
//! tiny lexicon.

mod support;

use std::sync::OnceLock;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Arg, Catalog, Gender, Glossary, Russian, Status, Translator, lint};

use support::{TestNames, i18n_dir};

fn catalog() -> Catalog {
    let text = std::fs::read_to_string(i18n_dir().join("catalog.en.json")).expect("the catalog");
    Catalog::parse(&text).expect("a valid catalog")
}

fn russian() -> Russian {
    Russian::load_dir(&i18n_dir().join("ru")).expect("the Russian translations")
}

fn translator() -> &'static Translator {
    static T: OnceLock<Translator> = OnceLock::new();
    T.get_or_init(|| Translator::new(catalog(), russian(), Box::new(TestNames)))
}

fn say(text: &str) -> (String, Status) {
    let out = translator().message(None, &[], text);
    (out.text, out.status)
}

#[test]
fn the_catalog_and_the_translations_agree() {
    let catalog = catalog();
    assert!(catalog.len() > 20_000, "{} templates", catalog.len());
    let russian = russian();
    assert!(russian.len() >= 50, "{} translations", russian.len());
    let read = |name: &str| std::fs::read_to_string(i18n_dir().join(name)).expect(name);
    let glossary =
        Glossary::from_toml(&read("glossary.ru.toml"), Lexicon::ru()).expect("the glossary");
    assert!(
        glossary.terms.len() > 1000,
        "{} terms",
        glossary.terms.len()
    );
    let elbereth = glossary.terms.iter().find(|t| t.en == "Elbereth");
    assert_eq!(elbereth.map(|t| t.ru[0].as_str()), Some("Elbereth"));
    let problems = lint(&catalog, &russian, &glossary);
    assert!(
        problems.is_empty(),
        "{}",
        problems
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn messages_by_their_text() {
    let cases = [
        ("The newt bites!", "Тритон кусает!"),
        (
            "You swap places with your kitten.",
            "Вы меняетесь местами с вашим котёнком.",
        ),
        ("You see here a dart.", "Здесь лежит дротик."),
        ("You see here 3 arrows.", "Здесь лежат 3 стрелы."),
        ("You kill the sewer rat!", "Вы убиваете крысу!"),
        ("The sewer rat is killed!", "Крыса убита!"),
        ("You miss the newt.", "Вы промахиваетесь по тритону."),
        (
            "Slasher picks up a gold piece.",
            "Slasher подбирает золотую монету.",
        ),
        ("You stop digging.", "Вы перестаёте копать."),
        (
            "You begin bashing monsters with your pick-axe.",
            "Вы начинаете колотить монстров вашей киркой.",
        ),
        ("You are already here.", "Вы уже здесь."),
        ("You don't have anything to eat.", "Вам нечего есть."),
        ("You shoot 2 arrows.", "Вы выпускаете 2 стрелы."),
        ("You shoot 5 arrows.", "Вы выпускаете 5 стрел."),
    ];
    for (en, ru) in cases {
        assert_eq!(say(en), (ru.to_string(), Status::Translated), "{en}");
    }
}

#[test]
fn messages_with_their_format() {
    let t = translator();
    // P7: hitmsg's generic format; the derived "%s bites!" says more, its
    // conversion lined up with P7's first argument
    let args = [
        Arg::Str("The newt".into()),
        Arg::Str("bites".into()),
        Arg::Str(String::new()),
        Arg::Str("!".into()),
    ];
    let out = t.message(Some("%s %s%s%s"), &args, "The newt bites!");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("Тритон кусает!", Status::Translated)
    );
    let out = t.message(
        Some("You hit %s."),
        &[Arg::Str("the newt".into())],
        "You hit the newt.",
    );
    assert_eq!(out.text, "Вы бьёте тритона.");
    let out = t.message(
        Some("There are %ld gold pieces here; eat them?"),
        &[Arg::Int(22)],
        "There are 22 gold pieces here; eat them?",
    );
    assert_eq!(out.text, "Здесь лежат 22 золотые монеты; съесть их?");
    // "%s" says nothing: the text decides
    let out = t.message(
        Some("%s"),
        &[Arg::Str("You are already here.".into())],
        "You are already here.",
    );
    assert_eq!(out.text, "Вы уже здесь.");
}

#[test]
fn what_cannot_be_translated_stays_english() {
    // a template without Russian yet: one the translator takes as itself
    let catalog = catalog();
    let russian = russian();
    let untranslated = catalog
        .templates()
        .iter()
        .filter(|t| t.arity() == 0 && russian.get(&t.id).is_none())
        .find(|t| translator().text(&t.fmt).template.as_deref() == Some(t.id.as_str()))
        .expect("a template without Russian");
    let (text, status) = say(&untranslated.fmt);
    assert_eq!(
        (text.as_str(), status),
        (untranslated.fmt.as_str(), Status::Untranslated)
    );
    // no template at all
    let (text, status) = say("Xyzzy plugh");
    assert_eq!((text.as_str(), status), ("Xyzzy plugh", Status::Unknown));
    // a name the lexicon does not know: the template is Russian, the name
    // not (but its article goes: Russian has none)
    let (text, status) = say("You kill the flaming sphere!");
    assert_eq!(
        (text.as_str(), status),
        ("Вы убиваете flaming sphere!", Status::Partial)
    );
}

#[test]
fn the_hero_gender_is_the_translators() {
    let mut t = Translator::new(catalog(), Russian::default(), Box::new(TestNames));
    t.set_hero(Gender::Fem);
    let out = t.text("You are already here.");
    assert_eq!(out.status, Status::Untranslated);
}

#[test]
fn a_window_of_paragraphs() {
    // the Oracle's words under its heading: each paragraph a text of the
    // catalog
    let t = translator();
    let passage = "Though the shopkeepers be wary, thieves have nevertheless stolen much by using\n\
                   their digging wands to hasten exits through the pavement.";
    let out = t.window(&format!(
        "The Oracle meditates for a moment and then intones:\n\n{passage}\n"
    ));
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    let (head, body) = out.text.split_once("\n\n").unwrap();
    assert!(head.starts_with("Оракул"), "{head}");
    assert!(body.starts_with("Хоть лавочники"), "{body}");
}

#[test]
fn windows_headings_and_names() {
    let russian = Russian::parse(&[(
        "t.toml".into(),
        r#"
[54d1bffd3629]
en = "The Gnomish Mines"
ru = "Гномьи копи"
"#
        .into(),
    )])
    .unwrap();
    let t = Translator::new(catalog(), russian, Box::new(TestNames));
    // a heading the catalog knows without its colon
    let out = t.text("The Gnomish Mines:");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("Гномьи копи:", Status::Translated)
    );
    // a name, not "%s of %s"
    let out = t.text("a dart");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("дротик", Status::Translated)
    );
    // a window of lines the catalog knows one by one
    let out = translator().window("You are already here.\n\nNever mind.");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("Вы уже здесь.\n\nНеважно.", Status::Translated)
    );
    let out = translator().window("You are already here.\nXyzzy plugh");
    assert_eq!(out.status, Status::Unknown);
    assert_eq!(out.text, "Вы уже здесь.\nXyzzy plugh");
}

#[test]
fn names_alone() {
    let t = translator();
    assert_eq!(t.name("dart").text, "дротик");
    assert_eq!(t.name("Your kitten").text, "Ваш котёнок");
    // not a name: a text of the catalog
    assert_eq!(t.name("You are already here.").text, "Вы уже здесь.");
}
