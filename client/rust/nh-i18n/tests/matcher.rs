//! The matcher on the lines it once got wrong, over the built-in catalog,
//! translations and lexicon: the names the game wrote side by side, the
//! hero in a monster's place, a %s before a %c, a hallucinated name, an
//! engraving a level's Lua builds.

use std::sync::OnceLock;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Arg, Catalog, Gender, Russian, Status, Translator};

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
fn a_question_the_catalog_knows_as_a_piece() {
    // doengrave: a Sprintf into a member that getlin shows is a piece to
    // the catalog, and without its mark the text is a "%s into %s" too
    let t = with(&[(
        "What do you want to burn into the %s here?",
        r#"ru = "Что вы хотите выжечь на {1:loc}?""#,
    )]);
    let out = t.text("What do you want to burn into the floor here?");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("Что вы хотите выжечь на полу?", Status::Translated)
    );
}

#[test]
fn the_death_text_in_the_hero_s_gender() {
    // topten.c formatkiller(), as the tombstone and #overview's graves show
    // it (the client sends it whole)
    let lines = [
        ("killed by a jackal", "убит шакалом", "убита шакалом"),
        (
            "choked on a lichen corpse",
            "подавился трупом лишайника",
            "подавилась трупом лишайника",
        ),
        (
            "petrified by touching a cockatrice corpse",
            "окаменел от прикосновения к трупу василиска",
            "окаменела от прикосновения к трупу василиска",
        ),
        (
            "killed by a fox, while fainted from lack of food",
            "убит лисой, в голодном обмороке",
            "убита лисой, в голодном обмороке",
        ),
        (
            "killed by a gas spore's explosion",
            "убит взрывом газовой споры",
            "убита взрывом газовой споры",
        ),
        ("died of starvation", "умер от голода", "умерла от голода"),
        ("quit", "сдался", "сдалась"),
        // #overview's grave of the hero, "his" made "your"
        (
            "killed by your own player",
            "убит собственным игроком",
            "убита собственным игроком",
        ),
        (
            "killed yourself with your bullwhip",
            "убил себя своим кнутом",
            "убила себя своим кнутом",
        ),
    ];
    for (gender, pick) in [(Gender::Masc, 0), (Gender::Fem, 1)] {
        let mut t = Translator::built_in().expect("the built-in translator");
        t.set_hero(gender);
        for (en, m, f) in lines {
            let out = t.window(en);
            assert_eq!(
                (out.text.as_str(), out.status),
                ([m, f][pick], Status::Translated),
                "{en}"
            );
        }
    }
}

#[test]
fn what_a_helper_writes() {
    // ^X: trap_predicament() writes the trap, piousness() returns the
    // piety, lcase(skill_level_name()) the skill's level
    check(" You are trapped in a pit.", "Вы застряли в яме");
    check(
        " You are piously aligned.",
        "Вы благочестиво преданы своему мировоззрению",
    );
    check(
        " You are unskilled in dagger.",
        "Ваш уровень владения кинжалом: неопытный",
    );
    // shk.c: append_honorific() adds to "For you, "
    check(
        "\"For you, esteemed sir; only 789 zorkmids for this large box and its contents.\"",
        "«Для вас, уважаемый господин, всего 789 зоркмидов",
    );
    check(
        "\"For you, scum; 133 zorkmids for this lamp.\"",
        "«Для тебя, негодяй, 133 зоркмида за лампу.»",
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

#[test]
fn a_hallucinated_name_in_the_plural_and_after_your() {
    let t = with(&[
        ("You kill %s!", r#"ru = "Вы убиваете {1:acc}!""#),
        ("You hit %s.", r#"ru = "Вы бьёте {1:acc}.""#),
        ("%s bites!", r#"ru = "{1} {1:num|кусает|кусают}!""#),
        (
            "jumbo shrimp",
            r#"ru = "гигантская креветка"
forms = ["гигантская креветка", "гигантской креветки", "гигантской креветке", "гигантскую креветку", "гигантской креветкой", "гигантской креветке"]
plural = ["гигантские креветки", "гигантских креветок", "гигантским креветкам", "гигантских креветок", "гигантскими креветками", "гигантских креветках"]
gender = "f""#,
        ),
        (
            "gnu",
            r#"ru = "гну"
forms = ["гну", "гну", "гну", "гну", "гну", "гну"]
gender = "m""#,
        ),
    ]);
    for (en, ru) in [
        (
            "You kill the jumbo shrimps!",
            "Вы убиваете гигантских креветок!",
        ),
        (
            "Your jumbo shrimp bites!",
            "Ваша гигантская креветка кусает!",
        ),
        (
            "You hit your jumbo shrimp.",
            "Вы бьёте вашу гигантскую креветку.",
        ),
        ("You hit your gnu.", "Вы бьёте вашего гну."),
    ] {
        let out = t.message(None, &[], en);
        assert_eq!(
            (out.text.as_str(), out.status),
            (ru, Status::Translated),
            "{en}"
        );
    }
}

#[test]
fn a_row_of_the_extended_commands_list() {
    let t = with(&[
        (
            " apply          %4s %s",
            r#"ru = " apply          {1} {2}""#,
        ),
        (
            "apply (use) a tool (pick-axe, key, lamp...)",
            r#"ru = "применить инструмент (кирку, ключ, лампу...)""#,
        ),
        (
            " annotate       %4s %s",
            r#"ru = " annotate       {1} {2}""#,
        ),
        ("name current level", r#"ru = "дать имя текущему уровню""#),
        (
            " ?              %4s list all extended commands",
            r#"ru = " ?              {1} список всех расширенных команд""#,
        ),
    ]);
    // doextlist's " %-14s %4s %s"
    let row = |name: &str, flags: &str, desc: &str| format!(" {name:<14} {flags:>4} {desc}");
    for (en, ru) in [
        (
            row(
                "apply",
                "[A]",
                "apply (use) a tool (pick-axe, key, lamp...)",
            ),
            row(
                "apply",
                "[A]",
                "применить инструмент (кирку, ключ, лампу...)",
            ),
        ),
        (
            row(
                "apply",
                "[mA]",
                "apply (use) a tool (pick-axe, key, lamp...)",
            ),
            row(
                "apply",
                "[mA]",
                "применить инструмент (кирку, ключ, лампу...)",
            ),
        ),
        (
            row("annotate", "", "name current level"),
            row("annotate", "", "дать имя текущему уровню"),
        ),
        (
            row("?", "[A]", "list all extended commands"),
            row("?", "[A]", "список всех расширенных команд"),
        ),
    ] {
        let out = t.text(&en);
        assert_eq!(
            (out.text.as_str(), out.status),
            (ru.as_str(), Status::Translated),
            "{en}"
        );
    }
}

#[test]
fn a_text_with_spaces_of_its_own_at_its_ends() {
    // sellobj: a head that ends " for your ", then the thing
    let t = with(&[
        ("%s%s.  Sell it?", r#"ru = "{1}{2:acc}.  Продать его?""#),
        (
            "%s offers %ld gold pieces for your ",
            r#"ru = "{1} предлагает {2} {2:plural|золотую монету|золотые монеты|золотых монет} за ""#,
        ),
        // weapon_insight: one of three heads, then a tail after its verb
        (
            " Your skill in %s is%s.",
            r#"ru = " Ваше владение {1:ins}{2}.""#,
        ),
        (
            " also limited by being %s with two weapons",
            r#"ru = " также ограничено навыком боя с двух рук ({1})""#,
        ),
        ("Unskilled", r#"ru = "Неопытный""#),
    ]);
    let out = t.text("Asidonhopo offers 12 gold pieces for your bullwhip.  Sell it?");
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    assert!(
        out.text
            .ends_with(" предлагает 12 золотых монет за кнут.  Продать его?"),
        "{}",
        out.text
    );
    let out = t.text(" Your skill in whip is also limited by being unskilled with two weapons.");
    assert_eq!(
        (out.text.as_str(), out.status),
        (
            " Ваше владение кнутом также ограничено навыком боя с двух рук (неопытный).",
            Status::Translated
        )
    );
}

#[test]
fn a_text_in_a_text_by_any_template_that_holds_it() {
    // prinv: "You ready: " and an inventory line, which "%s of %s" matches
    // first and "%c - %s." holds
    let t = with(&[("You ready: %s", r#"ru = "Наготове: {1}""#)]);
    let out = t.message(None, &[], "You ready: b - an uncursed tin of newt meat.");
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    assert!(out.text.starts_with("Наготове: b - "), "{}", out.text);
    assert!(!out.text.contains("tin"), "{}", out.text);
}

#[test]
fn the_derived_template_the_arguments_line_up_with() {
    // m_slips_free: You("%s %s %s %s!", …, "greased", xname(obj)), the
    // cloak not yet known as oilskin
    let t = with(&[
        (
            "You grab, but cannot hold onto %s greased %s!",
            r#"ru = "Вы хватаете, но не можете удержать {2:acc} в жире ({1:gen})!""#,
        ),
        (
            "You grab, but cannot hold onto %s slippery cloak!",
            r#"ru = "Вы хватаете, но не можете удержать скользкий плащ {1:gen}!""#,
        ),
    ]);
    let args: Vec<Arg> = [
        "grab, but cannot hold onto",
        "the gnome's",
        "greased",
        "slippery cloak",
    ]
    .iter()
    .map(|a| Arg::Str(a.to_string()))
    .collect();
    let out = t.message(
        Some("You %s %s %s %s!"),
        &args,
        "You grab, but cannot hold onto the gnome's greased slippery cloak!",
    );
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    assert!(out.text.contains(" в жире (гнома)!"), "{}", out.text);
}

#[test]
fn an_owner_the_engine_printed_apart() {
    // doquiver_core: pline("%s%s remains as secondary weapon.", …)
    let t = with(&[(
        "%s%s remains as secondary weapon.",
        r#"ru = "{1:like2}{2:cap} остаётся запасным оружием.""#,
    )]);
    let text = "Asidonhopo's mace remains as secondary weapon.";
    let by_text = t.message(None, &[], text);
    let args = [
        Arg::Str("Asidonhopo's ".to_string()),
        Arg::Str("mace".to_string()),
    ];
    let by_args = t.message(Some("%s%s remains as secondary weapon."), &args, text);
    assert_eq!(by_args, by_text);
    assert_eq!(by_args.status, Status::Translated, "{}", by_args.text);
}

#[test]
fn what_the_russian_leaves_out_may_stay_english() {
    let t = with(&[("You hit %s.", r#"ru = "Вы попадаете.{1:skip}""#)]);
    let out = t.message(None, &[], "You hit the frobnicating zzyzx.");
    assert_eq!(
        (out.text.as_str(), out.status),
        ("Вы попадаете.", Status::Translated)
    );
}

#[test]
fn a_god_speaks_to_a_heroine() {
    // godvoice(): what is said is a piece put into the voice's line
    let mut t = with(&[
        (
            "The voice of %s booms: \"%s\"",
            r#"ru = "Голос {1:gen} гремит: «{2}»""#,
        ),
        (
            "Thou hast angered me.",
            r#"ru = "Ты {hero:gender|прогневал|прогневала} меня.""#,
        ),
    ]);
    let line = "The voice of Mitra booms: \"Thou hast angered me.\"";
    assert!(
        t.message(None, &[], line)
            .text
            .ends_with("«Ты прогневал меня.»"),
        "{}",
        t.message(None, &[], line).text
    );
    t.set_hero(Gender::Fem);
    assert!(
        t.message(None, &[], line)
            .text
            .ends_with("«Ты прогневала меня.»"),
        "{}",
        t.message(None, &[], line).text
    );
}

#[test]
fn a_plural_spelled_like_its_singular() {
    // vtense() says the number in the verb, the name does not
    let t = with(&[
        (
            "%s slip from your %s.",
            r#"ru = "{1:pl:cap} выскальзывают у вас из рук.{2:skip}""#,
        ),
        (
            "%s slips from your %s.",
            r#"ru = "{1:cap} выскальзывает у вас из рук.{2:skip}""#,
        ),
    ]);
    let many = t.message(None, &[], "Your shuriken slip from your hands.");
    let one = t.message(None, &[], "Your shuriken slips from your hands.");
    assert_eq!(many.status, Status::Translated, "{}", many.text);
    assert!(
        one.text.ends_with(" выскальзывает у вас из рук."),
        "{}",
        one.text
    );
    assert!(
        many.text.ends_with(" выскальзывают у вас из рук."),
        "{}",
        many.text
    );
    // «Ваши сюрикены», not «Ваш сюрикен»
    assert_ne!(
        many.text.trim_end_matches(" выскальзывают у вас из рук."),
        one.text.trim_end_matches(" выскальзывает у вас из рук."),
        "{} / {}",
        many.text,
        one.text
    );
}

#[test]
fn a_row_of_the_spell_menu() {
    // dospellmenu: "%-20s  %2d   %-12s %3d%% %9s", the school in its column
    let out = translator().text("force bolt             1   attack         0%  91%-100%");
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    assert!(
        !out.text.chars().any(|c| c.is_ascii_alphabetic()),
        "{}",
        out.text
    );
    // five cells two spaces apart, as the client parts them
    let cells = out.text.split("  ").filter(|c| !c.trim().is_empty());
    assert_eq!(cells.count(), 5, "{}", out.text);
}

#[test]
fn a_thing_as_its_owner_s() {
    // "%s %s was poisoned!": the owner and the thing are two arguments
    let t = with(&[(
        "%s %s was poisoned!",
        r#"ru = "{2:of1:cap} {2:gender|был отравлен|была отравлена|было отравлено|были отравлены}!""#,
    )]);
    let out = t.message(None, &[], "The newt's dagger was poisoned!");
    assert_eq!(out.status, Status::Translated, "{}", out.text);
    assert!(
        out.text.starts_with("Кинжал ") && out.text.ends_with(" был отравлен!"),
        "{}",
        out.text
    );
    assert!(
        !out.text.chars().any(|c| c.is_ascii_alphabetic()),
        "{}",
        out.text
    );
}

#[test]
fn an_enchantment_is_no_price() {
    // xprname(): "%c - %s." of a thing picked up; with a price it is
    // "%c - %-45.*s%c%6ld %.50s", which took "a" for the name's end and
    // "+1" for the price
    let t = translator();
    for (en, thing) in [
        ("c - a +1 dagger.", "a +1 dagger"),
        ("a - a blessed +2 bullwhip.", "a blessed +2 bullwhip"),
        (
            "b - a +2 sling named Elbereth.",
            "a +2 sling named Elbereth",
        ),
        ("b - a +0 axe.", "a +0 axe"),
        ("d - 3 +0 daggers.", "3 +0 daggers"),
    ] {
        let name = t.name(thing);
        assert_eq!(name.status, Status::Translated, "{thing}");
        let out = t.message(None, &[], en);
        assert_eq!(
            (out.text.as_str(), out.status),
            (
                format!("{} - {}.", &en[..1], name.text).as_str(),
                Status::Translated
            ),
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
