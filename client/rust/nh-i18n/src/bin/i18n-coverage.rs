//! How much of what the game showed the catalog knows and the Russian
//! translates.
//!
//!     cargo run -p nh-i18n --bin i18n-coverage -- [--todo FILE N] CORPUS...
//!
//! A corpus is the soak's dump (`RENETHACK_DUMP_MESSAGES=<file>`: JSON
//! lines `{"kind", "text"}`, with `fmt` and `args` for a message when the
//! engine sent them; a text window's lines joined by newlines) or plain
//! text, one message a line. The report gives, for every kind of text, the
//! share a template matched and the share translated, then the texts no
//! template matched and the templates without a translation, by how often
//! they were shown. `--todo FILE N` writes the N most shown untranslated
//! templates as stubs to translate.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nh_i18n::{Arg, Catalog, NameKind, Names, Output, Phrase, Russian, Status, Translator};
use serde::Deserialize;

/// No lexicon yet: every name stays English.
struct NoNames;

impl Names for NoNames {
    fn parse(&self, _kind: NameKind, _english: &str) -> Option<Box<dyn Phrase>> {
        None
    }
}

#[derive(Deserialize)]
struct Shown {
    #[serde(default = "message")]
    kind: String,
    text: String,
    #[serde(default)]
    fmt: Option<String>,
    #[serde(default)]
    args: Vec<serde_json::Value>,
}

fn message() -> String {
    "message".into()
}

fn arg(v: &serde_json::Value) -> Arg {
    match v {
        serde_json::Value::String(s) => Arg::Str(s.clone()),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Arg::Int(i),
            None => Arg::Num(n.as_f64().unwrap_or(0.0)),
        },
        _ => Arg::Null,
    }
}

#[derive(Default)]
struct Tally {
    shown: usize,
    matched: usize,
    translated: usize,
    partial: usize,
    /// Matched only by a template of almost no words of its own ("%s of
    /// %s"): a name the lexicon should read.
    weak: usize,
}

fn main() -> ExitCode {
    let mut todo: Option<(PathBuf, usize)> = None;
    let mut corpora = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--todo" {
            let file = args.next().map(PathBuf::from);
            let n = args.next().and_then(|n| n.parse().ok());
            match (file, n) {
                (Some(f), Some(n)) => todo = Some((f, n)),
                _ => {
                    eprintln!("--todo FILE N");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            corpora.push(PathBuf::from(a));
        }
    }
    if corpora.is_empty() {
        eprintln!("usage: i18n-coverage [--todo FILE N] CORPUS...");
        return ExitCode::FAILURE;
    }
    match run(&corpora, todo) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("i18n-coverage: {e}");
            ExitCode::FAILURE
        }
    }
}

fn i18n_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n")
}

fn run(corpora: &[PathBuf], todo: Option<(PathBuf, usize)>) -> Result<String, String> {
    let dir = i18n_dir();
    let catalog_text =
        std::fs::read_to_string(dir.join("catalog.en.json")).map_err(|e| e.to_string())?;
    let catalog = Catalog::parse(&catalog_text).map_err(|e| e.to_string())?;
    let russian = Russian::load_dir(&dir.join("ru")).map_err(|e| e.to_string())?;
    let translator = Translator::new(catalog, russian, Box::new(NoNames));

    let mut tallies: HashMap<String, Tally> = HashMap::new();
    let mut unknown: HashMap<String, usize> = HashMap::new();
    let mut untranslated: HashMap<String, usize> = HashMap::new();
    let mut with_fmt = 0;
    for path in corpora {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let shown = if line.starts_with('{') {
                serde_json::from_str::<Shown>(line)
                    .map_err(|e| format!("{}: {e}: {line}", path.display()))?
            } else {
                Shown {
                    kind: message(),
                    text: line.to_string(),
                    fmt: None,
                    args: Vec::new(),
                }
            };
            // a text window that is no text of the catalog as a whole is
            // counted line by line, as the translator takes it
            let mut items: Vec<(String, String, Output)> = Vec::new();
            match shown.kind.as_str() {
                "message" => {
                    with_fmt += usize::from(shown.fmt.is_some());
                    let args: Vec<Arg> = shown.args.iter().map(arg).collect();
                    let out = translator.message(shown.fmt.as_deref(), &args, &shown.text);
                    items.push((shown.kind.clone(), shown.text.clone(), out));
                }
                "window" => {
                    let whole = translator.window(&shown.text);
                    if whole.template.is_some() || !shown.text.contains('\n') {
                        items.push(("window".into(), shown.text.clone(), whole));
                    } else {
                        for l in shown.text.lines().filter(|l| !l.trim().is_empty()) {
                            items.push(("window-line".into(), l.to_string(), translator.text(l)));
                        }
                    }
                }
                _ => {
                    let out = translator.text(&shown.text);
                    items.push((shown.kind.clone(), shown.text.clone(), out));
                }
            }
            for (kind, text, out) in items {
                tally(
                    &translator,
                    &mut tallies,
                    &mut unknown,
                    &mut untranslated,
                    &kind,
                    &text,
                    out,
                );
            }
        }
    }

    let mut out = String::new();
    let mut kinds: Vec<_> = tallies.iter().collect();
    kinds.sort_by_key(|(k, t)| (std::cmp::Reverse(t.shown), (*k).clone()));
    let total = kinds.iter().fold(Tally::default(), |mut a, (_, t)| {
        a.shown += t.shown;
        a.matched += t.matched;
        a.translated += t.translated;
        a.partial += t.partial;
        a.weak += t.weak;
        a
    });
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    let _ = writeln!(
        out,
        "{:<14} {:>7} {:>9} {:>11} {:>9} {:>9}",
        "kind", "shown", "matched", "translated", "partial", "weak"
    );
    for (k, t) in kinds
        .iter()
        .map(|(k, t)| (k.as_str(), *t))
        .chain([("all", &total)])
    {
        let _ = writeln!(
            out,
            "{:<14} {:>7} {:>8.2}% {:>10.2}% {:>8.2}% {:>8.2}%",
            k,
            t.shown,
            pct(t.matched, t.shown),
            pct(t.translated, t.shown),
            pct(t.partial, t.shown),
            pct(t.weak, t.shown)
        );
    }
    let _ = writeln!(
        out,
        "matched: a template with words of its own; partial: its names stay English \
         (no lexicon); weak: only a template like \"%s of %s\" (a name to read)"
    );
    let _ = writeln!(out, "messages with their format (P7): {with_fmt}");
    let mut misses: Vec<_> = unknown.into_iter().collect();
    misses.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(out, "\nno template ({} distinct):", misses.len());
    for (text, n) in misses.iter().take(60) {
        let _ = writeln!(out, "{n:>6} {text}");
    }
    let mut todo_list: Vec<_> = untranslated.into_iter().collect();
    todo_list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(
        out,
        "\nnot translated yet ({} templates), the most shown:",
        todo_list.len()
    );
    let catalog = translator.catalog();
    for (id, n) in todo_list.iter().take(40) {
        let fmt = catalog.by_id(id).map_or("?", |t| t.fmt.as_str());
        let _ = writeln!(out, "{n:>6} [{id}] {fmt:?}");
    }
    if let Some((file, n)) = todo {
        let mut stubs =
            String::from("# The most shown templates without a translation (i18n-coverage).\n\n");
        for (id, count) in todo_list.iter().take(n) {
            let Some(t) = catalog.by_id(id) else {
                continue;
            };
            let _ = writeln!(
                stubs,
                "# shown {count} times; {}",
                t.sites.first().map_or("", String::as_str)
            );
            let _ = writeln!(stubs, "[{id}]\nen = {}\nru = \"\"\n", toml_string(&t.fmt));
        }
        std::fs::write(&file, stubs).map_err(|e| format!("{}: {e}", file.display()))?;
        let _ = writeln!(
            out,
            "\n{} stubs in {}",
            n.min(todo_list.len()),
            file.display()
        );
    }
    Ok(out)
}

fn tally(
    translator: &Translator,
    tallies: &mut HashMap<String, Tally>,
    unknown: &mut HashMap<String, usize>,
    untranslated: &mut HashMap<String, usize>,
    kind: &str,
    text: &str,
    out: Output,
) {
    let t = tallies.entry(kind.to_string()).or_default();
    t.shown += 1;
    let weak = out
        .template
        .as_deref()
        .and_then(|id| translator.catalog().by_id(id))
        .is_some_and(|tpl| tpl.letters() < 3 && out.status == Status::Untranslated);
    if weak {
        t.weak += 1;
        *unknown
            .entry(format!("[{kind}] (weak) {text}"))
            .or_default() += 1;
        return;
    }
    match out.status {
        Status::Unknown => *unknown.entry(format!("[{kind}] {text}")).or_default() += 1,
        Status::Untranslated => {
            t.matched += 1;
            if let Some(id) = out.template {
                *untranslated.entry(id).or_default() += 1;
            }
        }
        Status::Partial => {
            t.matched += 1;
            t.partial += 1;
        }
        Status::Translated => {
            t.matched += 1;
            t.translated += 1;
        }
    }
}

fn toml_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}
