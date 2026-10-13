//! The codex: `\`'s discoveries (o_init.c's dodiscovered) and #conduct's
//! voluntary challenges (insight.c's show_conduct), parsed from their
//! English. The discoveries are "Discoveries, <order>", then a class's
//! heading ("Scrolls") over its objects: "  scroll of identify (VELOX
//! NEB)", "* spellbook of sleep (silver)" for one known from the start and
//! never come across, a shop's prices after it ("{buy 20 sell 10}"); the
//! relics and the artifacts have headings of their own. The conduct is
//! "Voluntary challenges:" over its lines.
//!
//! No Godot here: the view is in `codex_view`.

/// An object the hero has discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    /// Its name as the line has it ("scroll of identify", "potion" for one
    /// only come across, "Excalibur").
    pub name: String,
    /// What it looks like, when the line says ("VELOX NEB").
    pub appearance: Option<String>,
    /// What else the line says of it: an artifact's "[lawful long sword]".
    pub note: Option<String>,
    /// Known from the start, never come across ("* ").
    pub unseen: bool,
    /// The prices a shop showed ("buy 20 sell 10").
    pub prices: Option<String>,
    /// The line without its marks, as its translation reads it.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The class's heading as the engine wrote it ("Scrolls").
    pub heading: String,
    pub entries: Vec<Discovery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discoveries {
    /// The header as the engine wrote it ("Discoveries, by order of
    /// discovery within each class").
    pub header: String,
    pub sections: Vec<Section>,
}

/// "  scroll of identify (VELOX NEB) {buy 20}".
fn discovery(line: &str) -> Option<Discovery> {
    let (unseen, rest) = match line.strip_prefix("* ") {
        Some(r) => (true, r),
        None => (false, line.strip_prefix("  ")?),
    };
    let mut rest = rest.trim_end();
    let mut prices = None;
    if let Some(open) = rest.rfind(" {")
        && rest.ends_with('}')
    {
        prices = Some(rest[open + 2..rest.len() - 1].to_string());
        rest = &rest[..open];
    }
    let text = rest.to_string();
    let mut note = None;
    let mut appearance = None;
    let mut name = rest;
    if let Some(open) = rest.rfind(" [")
        && rest.ends_with(']')
    {
        note = Some(rest[open + 2..rest.len() - 1].to_string());
        name = &rest[..open];
    } else if let Some(open) = rest.rfind(" (")
        && rest.ends_with(')')
    {
        appearance = Some(rest[open + 2..rest.len() - 1].to_string());
        name = &rest[..open];
    }
    (!name.trim().is_empty()).then(|| Discovery {
        name: name.trim().to_string(),
        appearance,
        note,
        unseen,
        prices,
        text,
    })
}

/// The discoveries in a window's lines (each with whether it is a heading,
/// the window's bold or inverse): None unless they are `\`'s.
pub fn discoveries(lines: &[(String, bool)]) -> Option<Discoveries> {
    let mut texts = lines.iter().filter(|(t, _)| !t.trim().is_empty());
    let (header, _) = texts.next()?;
    if !header.starts_with("Discoveries") {
        return None;
    }
    let mut sections: Vec<Section> = Vec::new();
    for (text, heading) in texts {
        if *heading {
            sections.push(Section {
                heading: text.trim().to_string(),
                entries: Vec::new(),
            });
            continue;
        }
        let d = discovery(text)?;
        match sections.last_mut() {
            Some(s) => s.entries.push(d),
            // sorted across every class: the objects with no heading
            None => sections.push(Section {
                heading: String::new(),
                entries: vec![d],
            }),
        }
    }
    Some(Discoveries {
        header: header.trim().to_string(),
        sections,
    })
}

/// The object class a section's heading names (objnam.c's let_to_name):
/// None for the relics, the artifacts and the objects of every class.
pub fn class_of(heading: &str) -> Option<char> {
    Some(match heading {
        "Weapons" => ')',
        "Armor" => '[',
        "Rings" => '=',
        "Amulets" => '"',
        "Tools" => '(',
        "Comestibles" => '%',
        "Potions" => '!',
        "Scrolls" => '?',
        "Spellbooks" => '+',
        "Wands" => '/',
        "Coins" => '$',
        "Gems/Stones" => '*',
        "Boulders/Statues" => '`',
        "Iron balls" => '0',
        "Chains" => '_',
        "Venoms" => '.',
        _ => return None,
    })
}

/// A card's words from its line as the window's translation says it
/// (`said`): the name, and its looks split off the end when the line has
/// them; the marks and the shop's prices left out.
pub fn card_words(said: &str, d: &Discovery) -> (String, Option<String>) {
    let words = said.trim_start_matches(['*', ' ']).trim_end();
    let words = match words.rfind(" {") {
        Some(i) if d.prices.is_some() => &words[..i],
        _ => words,
    };
    let capital = |t: &str| {
        let mut c = t.chars();
        c.next()
            .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
    };
    match (d.appearance.as_ref(), words.rfind(" (")) {
        (Some(_), Some(i)) if words.ends_with(')') => (
            capital(&words[..i]),
            Some(words[i + 2..words.len() - 1].to_string()),
        ),
        _ => (capital(words), None),
    }
}

/// #conduct's lines, without their indent: None unless the window is it.
pub fn conduct<S: AsRef<str>>(lines: &[S]) -> Option<Vec<String>> {
    let mut texts = lines
        .iter()
        .map(AsRef::as_ref)
        .filter(|l| !l.trim().is_empty());
    if texts.next()?.trim() != "Voluntary challenges:" {
        return None;
    }
    Some(texts.map(|l| l.trim().to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(rows: &[(&str, bool)]) -> Vec<(String, bool)> {
        rows.iter().map(|(t, h)| (t.to_string(), *h)).collect()
    }

    #[test]
    fn a_wizard_s_discoveries_read() {
        let d = discoveries(&lines(&[
            (
                "Discoveries, by order of discovery within each class",
                false,
            ),
            ("", false),
            ("Weapons", true),
            ("  quarterstaff (staff)", false),
            ("Spellbooks", true),
            ("  spellbook of force bolt (glittering)", false),
            ("* spellbook of sleep (silver)", false),
            ("Potions", true),
            ("  potion (golden)", false),
            ("  potion of healing (black) {buy 20 sell 10}", false),
        ]))
        .expect("discoveries");
        assert!(d.header.ends_with("within each class"));
        assert_eq!(d.sections.len(), 3);
        let books = &d.sections[1];
        assert_eq!(books.heading, "Spellbooks");
        assert_eq!(books.entries[0].name, "spellbook of force bolt");
        assert_eq!(books.entries[0].appearance.as_deref(), Some("glittering"));
        assert!(!books.entries[0].unseen && books.entries[1].unseen);
        let potions = &d.sections[2].entries;
        assert_eq!(potions[0].name, "potion");
        assert_eq!(potions[1].prices.as_deref(), Some("buy 20 sell 10"));
        assert_eq!(potions[1].text, "potion of healing (black)");
    }

    #[test]
    fn the_relics_and_the_artifacts_read() {
        let d = discoveries(&lines(&[
            ("Discoveries, alphabetical across all classes", false),
            ("", false),
            ("Unique items or Relics", true),
            ("  Bell of Opening", false),
            ("Artifacts", true),
            ("  Excalibur [lawful long sword]", false),
            ("Discovered items", true),
            ("  dagger", false),
        ]))
        .expect("discoveries");
        let arti = &d.sections[1].entries[0];
        assert_eq!(arti.name, "Excalibur");
        assert_eq!(arti.note.as_deref(), Some("lawful long sword"));
        assert_eq!(d.sections[0].entries[0].appearance, None);
        assert_eq!(d.sections[2].entries[0].name, "dagger");
    }

    #[test]
    fn a_card_s_words_split_off_their_looks() {
        let d = discovery("* spellbook of sleep (silver) {buy 100}").unwrap();
        assert_eq!(
            card_words("* книга сна (серебряная) {buy 100}", &d),
            ("Книга сна".to_string(), Some("серебряная".to_string()))
        );
        let a = discovery("  Excalibur [lawful long sword]").unwrap();
        assert_eq!(
            card_words("  Excalibur [lawful long sword]", &a),
            ("Excalibur [lawful long sword]".to_string(), None)
        );
    }

    #[test]
    fn the_conduct_reads_and_other_windows_do_not() {
        let c = conduct(&[
            "Voluntary challenges:",
            " Character rerolling was not enabled.",
            " You have been an atheist.",
        ])
        .expect("the conduct");
        assert_eq!(
            c,
            [
                "Character rerolling was not enabled.",
                "You have been an atheist."
            ]
        );
        assert_eq!(conduct(&["Logged events:"]), None);
        assert_eq!(
            discoveries(&lines(&[("Voluntary challenges:", false)])),
            None
        );
        // a line that is no object's: no discoveries
        assert_eq!(
            discoveries(&lines(&[
                ("Discoveries, sortloot order", false),
                ("odd", false)
            ])),
            None
        );
    }
}
