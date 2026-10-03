//! The name parser over every name the game can print
//! (tests/data/names.en.txt, made by tools/i18n/lexicon_corpus.py from the
//! engine's own tables), and the Russian of a sample of them for a person
//! to read (tests/snapshots/names.txt).
//!
//! `UPDATE_SNAPSHOTS=1 cargo test -p nh-i18n --test names` writes the
//! snapshot again; review its diff.

use std::fmt::Write as _;
use std::path::PathBuf;

use nh_i18n::lexicon::Lexicon;
use nh_i18n::names::{MonsterKind, ObjectName};
use nh_i18n::{Case, NameKind, Names};

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

/// (kind, name) for every line of the corpus.
fn corpus() -> Vec<(char, String)> {
    let text = std::fs::read_to_string(data("data/names.en.txt")).expect("tests/data/names.en.txt");
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (kind, name) = l.split_once('\t').expect("kind<TAB>name");
            (kind.chars().next().unwrap(), name.to_string())
        })
        .collect()
}

/// Why an object's name is not fully Russian, if it is not.
fn object_problem(lex: &Lexicon, name: &str) -> Option<String> {
    let Some(parsed) = lex.parse_object(name) else {
        return Some("does not parse".into());
    };
    let ru = parsed.ru(lex);
    if let Some(s) = ru.statuses.iter().find(|s| !s.is_russian()) {
        return Some(format!("status stays English: {s:?}"));
    }
    if parsed.states.iter().any(|s| lex.adjective(s).is_none()) {
        return Some(format!("a state the lexicon lacks: {:?}", parsed.states));
    }
    None
}

/// Why a monster's name is not understood, if it is not: a name the
/// lexicon does not know must at least look like a personal name.
fn monster_problem(lex: &Lexicon, name: &str) -> Option<String> {
    if lex.parse_monster(name, true).is_some() {
        return None;
    }
    match lex.parse_monster(name, false) {
        Some(m) => match &m.kind {
            MonsterKind::Name(n) if n.starts_with(char::is_uppercase) => None,
            other => Some(format!("parsed as {other:?}")),
        },
        None => Some("does not parse".into()),
    }
}

#[test]
fn every_name_the_game_prints_parses() {
    let lex = Lexicon::ru();
    let mut problems = Vec::new();
    let all = corpus();
    for (kind, name) in &all {
        let problem = match kind {
            'o' => object_problem(lex, name),
            _ => monster_problem(lex, name),
        };
        if let Some(p) = problem {
            problems.push(format!("{kind} {name:?}: {p}"));
        }
    }
    assert!(
        problems.is_empty(),
        "{} of {} names fail:\n{}",
        problems.len(),
        all.len(),
        problems
            .iter()
            .take(80)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The names the snapshot shows: one of each kind of thing the parser
/// knows, and every 97th line of the corpus.
fn sample() -> Vec<(char, String)> {
    let mut out: Vec<(char, String)> = [
        ('o', "2 uncursed +1 elven daggers (weapon in hand)"),
        ('o', "a scroll labeled ELBERETH"),
        ('o', "a potion called healing"),
        ('o', "an uncursed figurine of a newt named Bob"),
        ('o', "the Amulet of Yendor"),
        ('o', "a lichen corpse"),
        ('o', "Medusa's partly eaten corpse"),
        ('o', "a tin of newt meat"),
        ('o', "the blessed rustproof +5 Excalibur (weapon in hands)"),
        ('o', "2 pair of speed boots"),
        ('o', "a blessed +2 pair of speed boots (being worn)"),
        ('o', "5 cursed -1 darts (in quiver)"),
        ('o', "21 uncursed arrows"),
        ('o', "your 3 food rations"),
        ('o', "a wand of digging (0:5)"),
        ('o', "an uncursed bag of holding containing 7 items"),
        ('o', "an oil lamp (lit)"),
        ('o', "a +0 ring of protection (on left hand)"),
        ('o', "a set of red dragon scales (embedded in your skin)"),
        ('o', "13 gold pieces"),
        ('m', "the newt"),
        ('m', "It"),
        ('m', "your little dog"),
        ('m', "a saddled pony"),
        ('m', "Medusa"),
        ('m', "the Oracle"),
        ('m', "Asidonhopo the invisible shopkeeper"),
        ('m', "the high priestess of Moloch"),
        ('m', "dog called Fido"),
        ('m', "Bob's ghost"),
    ]
    .iter()
    .map(|(k, n)| (*k, n.to_string()))
    .collect();
    out.extend(corpus().into_iter().step_by(97));
    out
}

fn render_sample() -> String {
    let lex = Lexicon::ru();
    let mut out = String::new();
    for (kind, name) in sample() {
        let k = if kind == 'o' {
            NameKind::Object
        } else {
            NameKind::Monster
        };
        let _ = writeln!(out, "{name}");
        match lex.parse(k, &name) {
            Some(p) => {
                let forms: Vec<String> = Case::ALL.iter().map(|&c| p.form(c)).collect();
                let _ = writeln!(out, "  {} | {} | {}", forms[0], forms[1], forms[2]);
                let _ = writeln!(out, "  {} | {} | {}", forms[3], forms[4], forms[5]);
            }
            None => {
                let _ = writeln!(out, "  (not parsed)");
            }
        }
    }
    out
}

#[test]
fn a_sample_in_every_case_matches_its_snapshot() {
    let path = data("snapshots/names.txt");
    let text = render_sample();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        old == text,
        "tests/snapshots/names.txt is out of date: run UPDATE_SNAPSHOTS=1 cargo test -p nh-i18n --test names and review the diff"
    );
}

#[test]
fn parsed_names_say_what_they_are() {
    let lex = Lexicon::ru();
    let ObjectName {
        count, statuses, ..
    } = lex
        .parse_object("2 uncursed +1 elven daggers (weapon in hand)")
        .unwrap();
    assert_eq!(count, nh_i18n::names::Count::Exactly(2));
    assert_eq!(statuses, ["weapon in hand"]);
}
