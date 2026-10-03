//! Grammar snapshots: every Russian template rendered with words of each
//! gender and number (a masculine, a feminine, a neuter noun, a plural)
//! and numbers that take each plural form, for a person to read.
//!
//! `UPDATE_SNAPSHOTS=1 cargo test -p nh-i18n --test snapshots` writes
//! tests/snapshots/grammar.txt again; review its diff.

mod support;

use std::fmt::Write as _;
use std::path::PathBuf;

use nh_i18n::{Catalog, ConvKind, Gender, Russian, Value, capitalize, convs};

use support::{Name, i18n_dir};

/// One row of arguments: its label, the name for each %s, the number for
/// each %d.
const ROWS: [(&str, &str, u64, i64); 4] = [
    ("m", "newt", 1, 1),
    ("f", "sewer rat", 1, 3),
    ("n", "spear", 1, 5),
    ("pl", "arrow", 3, 21),
];

fn render_all() -> String {
    let catalog_text = std::fs::read_to_string(i18n_dir().join("catalog.en.json")).unwrap();
    let catalog = Catalog::parse(&catalog_text).unwrap();
    let russian = Russian::load_dir(&i18n_dir().join("ru")).unwrap();
    let mut all: Vec<_> = russian.iter().collect();
    all.sort_by(|a, b| (&a.en, &a.id).cmp(&(&b.en, &b.id)));
    let mut out = String::new();
    for tr in all {
        let Some(t) = catalog.by_id(&tr.id) else {
            continue;
        };
        let kinds: Vec<ConvKind> = convs(&t.segments).map(|c| c.kind).collect();
        let _ = writeln!(out, "{}  [{}]", tr.en.replace('\n', "\\n"), tr.id);
        let rows: &[(&str, &str, u64, i64)] = if kinds.is_empty() { &ROWS[..1] } else { &ROWS };
        for &(label, noun, count, number) in rows {
            let values: Vec<Value> = kinds
                .iter()
                .map(|k| match k {
                    ConvKind::Int => Value::Number(number),
                    ConvKind::Str => Value::Phrase(Box::new(Name::many(noun, count))),
                    _ => Value::Text("x".into()),
                })
                .collect();
            let text = capitalize(&tr.template.render(&values, Gender::Masc));
            let label = if kinds.is_empty() { "=" } else { label };
            let _ = writeln!(out, "  {label:<3} {}", text.replace('\n', "\\n"));
        }
    }
    out
}

#[test]
fn grammar_snapshots() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/grammar.txt");
    let now = render_all();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &now).unwrap();
        return;
    }
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    if before != now {
        let first = before
            .lines()
            .zip(now.lines())
            .find(|(a, b)| a != b)
            .map(|(a, b)| format!("was: {a}\nnow: {b}"))
            .unwrap_or_else(|| "the lengths differ".into());
        panic!(
            "the grammar snapshots changed (UPDATE_SNAPSHOTS=1 to accept, then review the diff of {}):\n{first}",
            path.display()
        );
    }
}
