//! How much of what the game showed the catalog knows and the Russian
//! translates.
//!
//!     cargo run -p nh-i18n --bin i18n-coverage -- [--todo FILE N [--todo-kind KINDS]] [--no-lexicon] [--all] [--dump FILE] CORPUS...
//!
//! A corpus is the soak's dump (`RENETHACK_DUMP_MESSAGES=<file>`: JSON
//! lines `{"kind", "text"}`, with `fmt` and `args` for a message when the
//! engine sent them; a text window's lines joined by newlines) or plain
//! text, one message a line. The report gives, for every kind of text, the
//! share a template matched and the share translated, then the texts no
//! template matched and the templates without a translation, by how often
//! they were shown. `--todo FILE N` writes the N most shown untranslated
//! templates as stubs to translate, of the kinds given (`--todo-kind
//! query,menu,window`; message, query, menu, window). The names in the texts are declined by
//! the lexicon (`client/i18n/lexicon.ru.toml`); `--no-lexicon` leaves them
//! English, to see the templates alone. `--all` gives the report's lists
//! whole, not only their heads. `--dump FILE` writes every distinct text
//! with what it came to, sorted: what a change to the matcher did to a
//! corpus is the diff of two dumps.
//!
//! A text that came out as translated and still holds an English word is
//! counted apart, as mixed: «c - a +1 кинжал.» is no translation. A word
//! in Latin letters is English there unless the template's own Russian
//! has it (an option's or a command's name, a unit), the player typed it
//! (a name after "called", a text in quotes), or it is one letter (an
//! inventory letter) and no article.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nh_i18n::{Arg, Catalog, NameKind, Names, Output, Phrase, Russian, Status, Translator};
use serde::Deserialize;

/// `--no-lexicon`: every name stays English.
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
    /// Came out as translated with an English word left in it.
    mixed: usize,
}

fn main() -> ExitCode {
    let mut todo: Option<(PathBuf, usize)> = None;
    let mut corpora = Vec::new();
    let mut lexicon = true;
    let mut all = false;
    let mut dump: Option<PathBuf> = None;
    let mut kinds: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--no-lexicon" {
            lexicon = false;
        } else if a == "--all" {
            all = true;
        } else if a == "--dump" {
            dump = args.next().map(PathBuf::from);
            if dump.is_none() {
                eprintln!("--dump FILE");
                return ExitCode::FAILURE;
            }
        } else if a == "--todo-kind" {
            kinds.extend(
                args.next()
                    .unwrap_or_default()
                    .split(',')
                    .map(str::to_string),
            );
        } else if a == "--todo" {
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
        eprintln!(
            "usage: i18n-coverage [--todo FILE N] [--no-lexicon] [--all] [--dump FILE] CORPUS..."
        );
        return ExitCode::FAILURE;
    }
    match run(&corpora, todo, &kinds, lexicon, all, dump.as_deref()) {
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

fn run(
    corpora: &[PathBuf],
    todo: Option<(PathBuf, usize)>,
    todo_kinds: &[String],
    lexicon: bool,
    all: bool,
    dump: Option<&Path>,
) -> Result<String, String> {
    let dir = i18n_dir();
    let catalog_text =
        std::fs::read_to_string(dir.join("catalog.en.json")).map_err(|e| e.to_string())?;
    let catalog = Catalog::parse(&catalog_text).map_err(|e| e.to_string())?;
    let russian = Russian::load_dir(&dir.join("ru")).map_err(|e| e.to_string())?;
    let names: Box<dyn Names + Send + Sync> = if lexicon {
        Box::new(nh_i18n::lexicon::Lexicon::ru())
    } else {
        Box::new(NoNames)
    };
    let translator = Translator::new(catalog, russian, names);

    let mut report = Report::default();
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
                    let lines: Vec<&str> = shown
                        .text
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .collect();
                    if whole.template.is_some() || lines.len() < 2 {
                        items.push(("window".into(), shown.text.clone(), whole));
                    } else if translator.is_layout(&shown.text) {
                        // the tombstone, #overview, a list: the client draws
                        // it from data
                        report.layout += 1;
                        report.layout_lines += lines.len();
                    } else {
                        for l in lines {
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
                report.tally(&translator, &kind, &text, out);
            }
        }
    }

    let mut out = String::new();
    let Report {
        layout,
        layout_lines,
        tallies,
        unknown,
        untranslated,
        shown_as,
        partial,
        mixed,
        came_to,
    } = report;
    if let Some(file) = dump {
        let mut lines = String::new();
        for ((kind, text, status, ru), n) in &came_to {
            let flat = |s: &str| s.replace('\n', "\\n");
            let _ = writeln!(
                lines,
                "{n}\t[{kind}] {}\t{status}\t{}",
                flat(text),
                flat(ru)
            );
        }
        std::fs::write(file, lines).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    let mut kinds: Vec<_> = tallies.iter().collect();
    kinds.sort_by_key(|(k, t)| (std::cmp::Reverse(t.shown), (*k).clone()));
    let total = kinds.iter().fold(Tally::default(), |mut a, (_, t)| {
        a.shown += t.shown;
        a.matched += t.matched;
        a.translated += t.translated;
        a.partial += t.partial;
        a.weak += t.weak;
        a.mixed += t.mixed;
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
        "{:<14} {:>7} {:>9} {:>11} {:>9} {:>9} {:>9}",
        "kind", "shown", "matched", "translated", "mixed", "partial", "weak"
    );
    for (k, t) in kinds
        .iter()
        .map(|(k, t)| (k.as_str(), *t))
        .chain([("all", &total)])
    {
        let _ = writeln!(
            out,
            "{:<14} {:>7} {:>8.2}% {:>10.2}% {:>8.2}% {:>8.2}% {:>8.2}%",
            k,
            t.shown,
            pct(t.matched, t.shown),
            pct(t.translated, t.shown),
            pct(t.mixed, t.shown),
            pct(t.partial, t.shown),
            pct(t.weak, t.shown)
        );
    }
    let _ = writeln!(
        out,
        "matched: a template with words of its own (or a name the lexicon reads); \
         mixed: came out as translated with an English word left; \
         partial: a name in it stays English; weak: only a template like \"%s of %s\""
    );
    let _ = writeln!(out, "messages with their format (P7): {with_fmt}");
    let _ = writeln!(
        out,
        "laid out by the client, not counted: {layout} windows of {layout_lines} lines \
         (the tombstone, #overview, the vanquished list)"
    );
    let mut misses: Vec<_> = unknown.into_iter().collect();
    misses.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(out, "\nno template ({} distinct):", misses.len());
    let head = |n: usize| if all { usize::MAX } else { n };
    for (text, n) in misses.iter().take(head(60)) {
        let _ = writeln!(out, "{n:>6} {text}");
    }
    let mut left: Vec<_> = mixed.into_iter().collect();
    left.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(
        out,
        "\nmixed: as translated, with English left ({} distinct, {} shown):",
        left.len(),
        left.iter().map(|(_, n)| n).sum::<usize>()
    );
    for (text, n) in left.iter().take(head(30)) {
        let _ = writeln!(out, "{n:>6} {text}");
    }
    let mut halves: Vec<_> = partial.into_iter().collect();
    halves.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let _ = writeln!(
        out,
        "\ntranslated but for a name the lexicon does not read ({} distinct):",
        halves.len()
    );
    for (text, n) in halves.iter().take(head(30)) {
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
    for (id, n) in todo_list.iter().take(head(40)) {
        let fmt = catalog.by_id(id).map_or("?", |t| t.fmt.as_str());
        let _ = writeln!(out, "{n:>6} [{id}] {fmt:?}");
    }
    if let Some((file, n)) = todo {
        let mut stubs =
            String::from("# The most shown templates without a translation (i18n-coverage).\n\n");
        let wanted = |id: &str| {
            todo_kinds.is_empty()
                || shown_as
                    .get(id)
                    .is_some_and(|ks| ks.iter().any(|k| todo_kinds.contains(k)))
        };
        for (id, count) in todo_list.iter().filter(|(id, _)| wanted(id)).take(n) {
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

/// What the texts of a corpus came to.
#[derive(Default)]
struct Report {
    /// windows laid out as a picture or a table, and their lines
    layout: usize,
    layout_lines: usize,
    tallies: HashMap<String, Tally>,
    /// "[kind] text" no template matched
    unknown: HashMap<String, usize>,
    /// template id -> times shown without a translation
    untranslated: HashMap<String, usize>,
    /// template id -> the kinds of text it was shown as
    shown_as: HashMap<String, BTreeSet<String>>,
    /// "[kind] text" translated but for a name
    partial: HashMap<String, usize>,
    /// "[kind] text -> its Russian  {the English left}"
    mixed: HashMap<String, usize>,
    /// (kind, text, status, its Russian) -> times shown
    came_to: BTreeMap<(String, String, String, String), usize>,
}

impl Report {
    fn tally(&mut self, translator: &Translator, kind: &str, text: &str, out: Output) {
        let key = (
            kind.to_string(),
            text.to_string(),
            format!("{:?}", out.status),
            out.text.clone(),
        );
        *self.came_to.entry(key).or_default() += 1;
        let t = self.tallies.entry(kind.to_string()).or_default();
        t.shown += 1;
        let weak = out
            .template
            .as_deref()
            .and_then(|id| translator.catalog().by_id(id))
            .is_some_and(|tpl| tpl.letters() < 3 && out.status == Status::Untranslated);
        if weak {
            t.weak += 1;
            *self
                .unknown
                .entry(format!("[{kind}] (weak) {text}"))
                .or_default() += 1;
            return;
        }
        match out.status {
            Status::Unknown => *self.unknown.entry(format!("[{kind}] {text}")).or_default() += 1,
            Status::Untranslated => {
                t.matched += 1;
                if let Some(id) = out.template {
                    // "window-line" and "window-title" are windows,
                    // "menu-title" a menu
                    let family = kind.split('-').next().unwrap_or(kind).to_string();
                    self.shown_as.entry(id.clone()).or_default().insert(family);
                    *self.untranslated.entry(id).or_default() += 1;
                }
            }
            Status::Partial => {
                t.matched += 1;
                t.partial += 1;
                *self
                    .partial
                    .entry(format!("[{kind}] {text} -> {}", out.text))
                    .or_default() += 1;
            }
            Status::Translated => {
                t.matched += 1;
                let left = english_left(translator, &out, text);
                if left.is_empty() {
                    t.translated += 1;
                } else {
                    t.mixed += 1;
                    let entry = format!("[{kind}] {text} -> {}  {{{}}}", out.text, left.join(", "));
                    *self.mixed.entry(entry).or_default() += 1;
                }
            }
        }
    }
}

/// English function words: left in a Russian text they are a miss whatever
/// length they have and whoever else wrote them somewhere.
const ENGLISH: [&str; 3] = ["a", "an", "the"];
/// Names that stay in Latin letters on purpose: the soak's hero, what it
/// calls everything it may name, the game.
const KEPT: [&str; 3] = ["Hero", "Elbereth", "NetHack"];

/// The words in Latin letters of a text, with where each starts.
fn latin_words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_ascii_alphabetic(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// What the player typed, in the English of a text: a name after "called"
/// or "named" (to the next comma), a text in double quotes, whose ghost
/// it is (a bones file's hero).
fn typed(english: &str) -> BTreeSet<String> {
    let named = [" called ", " named "]
        .into_iter()
        .flat_map(|by| english.match_indices(by).map(move |(i, _)| i + by.len()))
        .map(|i| english[i..].split(',').next().unwrap_or(""));
    let quoted = english.split('"').skip(1).step_by(2);
    let ghosts = english
        .match_indices("'s ghost")
        .map(|(i, _)| english[..i].rsplit(' ').next().unwrap_or(""));
    named
        .chain(quoted)
        .chain(ghosts)
        .flat_map(latin_words)
        .map(|(_, w)| w.to_lowercase())
        .collect()
}

/// The keys a question lists in brackets, which are no words: inventory
/// letters ("[afgh or ?*]", "[- a or ?*]"), directions ("[ykunjb>]").
fn keys(english: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = english;
    while let Some(open) = rest.find('[') {
        let Some(len) = rest[open..].find(']') else {
            break;
        };
        let inside = &rest[open + 1..open + len];
        let list = inside.strip_suffix(" or ?*").unwrap_or(inside);
        // one run of keys, or a few behind "- " (nothing, as a choice)
        if !list.trim_start_matches("- ").contains(' ') {
            out.extend(latin_words(list).into_iter().map(|(_, w)| w.to_lowercase()));
        }
        rest = &rest[open + len + 1..];
    }
    out
}

/// The English words left in a text that came out as translated.
fn english_left(translator: &Translator, out: &Output, english: &str) -> Vec<String> {
    if !out.text.bytes().any(|b| b.is_ascii_alphabetic()) {
        return Vec::new();
    }
    // what the template's own Russian says in Latin letters is meant
    let own: BTreeSet<String> = out
        .template
        .as_deref()
        .and_then(|id| translator.russian().get(id))
        .map(|tr| {
            latin_words(&tr.ru)
                .into_iter()
                .map(|(_, w)| w.to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    left_in(&out.text, english, &own)
}

/// The words in Latin letters of `ru`, the Russian of `english`, that are
/// English left over: of three letters or more, or an article; not what
/// the template itself says (`own`), what the player typed, a key, a
/// name kept as it is.
fn left_in(ru: &str, english: &str, own: &BTreeSet<String>) -> Vec<String> {
    let mut typed = typed(english);
    let keys = keys(english);
    typed.extend(keys.iter().cloned());
    let mut left = Vec::new();
    for (at, word) in latin_words(ru) {
        let low = word.to_lowercase();
        let (before, after) = (&ru[..at], &ru[at + word.len()..]);
        let long = word.len() >= 3;
        // "a - меч" is the item's letter and "A-G" a range of notes, "a +1
        // кинжал" an article
        let article =
            ENGLISH.contains(&low.as_str()) && !after.starts_with(" - ") && !after.starts_with('-');
        if !(long || article) || KEPT.contains(&word) || typed.contains(&low) {
            continue;
        }
        // a key shown in quotes ('a'), or alone in a list of keys ("[a или ?*]")
        let quoted = before.ends_with('\'') && after.starts_with('\'');
        let key =
            before.ends_with(['[', ' ']) && after.starts_with([' ', ']']) && keys.contains(&low);
        if !long && (quoted || key) {
            continue;
        }
        if own.contains(&low) && !article {
            continue;
        }
        if !left.contains(&word.to_string()) {
            left.push(word.to_string());
        }
    }
    left
}

fn toml_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(english: &str, ru: &str) -> Vec<String> {
        left_in(ru, english, &BTreeSet::new())
    }

    #[test]
    fn english_left_in_a_russian_text() {
        assert_eq!(left("c - a +1 dagger.", "c - a +1 кинжал."), ["a"]);
        assert_eq!(
            left("b - a +0 short sword.", "b - a +0 short меч."),
            ["a", "short"]
        );
        assert_eq!(
            left(
                "What do you want to call this dungeon level?",
                "Как вы хотите назвать this dungeon level?"
            ),
            ["this", "dungeon", "level"]
        );
        assert_eq!(
            left("Contents of the sack:", "Содержимое мешка the:"),
            ["the"]
        );
    }

    #[test]
    fn letters_keys_and_names_are_no_english() {
        for (english, ru) in [
            // the item's letter is a, the thing has no article
            ("a - a +1 dagger.", "a - +1 кинжал."),
            (
                "What do you want to use or apply? [afgh or ?*]",
                "Что вы хотите использовать? [afgh или ?*]",
            ),
            (
                "What do you want to wield? [- a or ?*]",
                "Чем вы хотите вооружиться? [- a или ?*]",
            ),
            (
                "In what direction do you want to dig? [ykunjb>]",
                "В каком направлении копать? [ykunjb>]",
            ),
            (
                "What tune are you playing? [5 notes, A-G]",
                "Какую мелодию вы играете? [5 нот, A-G]",
            ),
            (
                "Reordering spells; swap 'a' with",
                "Перестановка заклинаний: поменять 'a' с",
            ),
            // what the player typed, a bones file's hero, the soak's names
            ("You read: \"ad aquarium\".", "Вы читаете: «ad aquarium»."),
            (
                "Your kitten called tom purrs.",
                "Ваш котёнок по имени tom мурлычет.",
            ),
            ("Mike's ghost touches you!", "Привидение Mike касается вас!"),
            (
                "You swap places with Elbereth.",
                "Вы меняетесь местами с Elbereth.",
            ),
            (
                "Hello Hero, welcome to NetHack!",
                "Привет, Hero, добро пожаловать в NetHack!",
            ),
        ] {
            assert_eq!(left(english, ru), Vec::<String>::new(), "{ru}");
        }
        // what the template itself says in Latin letters is meant
        let own: BTreeSet<String> = ["autopickup".to_string()].into();
        assert!(left_in("Включить autopickup", "Toggle autopickup", &own).is_empty());
        assert_eq!(
            left_in("Включить autopickup", "Toggle autopickup", &BTreeSet::new()),
            ["autopickup"]
        );
    }
}
