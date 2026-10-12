//! What the credits and licences screen shows: who made what the game is
//! made of, and under which terms. The same sources as the package's
//! legal/ folder (`tools/dist/common.sh`, `legal_folder`): the credits
//! files, the fonts' licences, cJSON's and the package's notice embedded
//! when the extension is built; the NGPL as the engine ships it
//! (`data/license`) and Godot's notices as the Godot running tells them;
//! the Rust crates as `build.rs` found them linked. Each is drawn as BBCode
//! for a RichTextLabel; their words stay as written.

use godot::classes::Engine;
use godot::prelude::*;
use serde::Deserialize;

use crate::theme;
use crate::tr;

const ART: &str = include_str!("../../../godot/art/CREDITS.md");
const SOUND: &str = include_str!("../../../godot/audio/CREDITS.md");
const HELP: &str = include_str!("../../../help/CREDITS.md");
const TRANSLATION: &str = include_str!("../../../i18n/CREDITS.md");
const NOTICE: &str = include_str!("../../../../tools/dist/legal/NOTICE.txt");
const CJSON: &str = include_str!("../../../../engine/host/third_party/cjson/LICENSE");
/// Each font family's licence: its name and its OFL.txt.
const FONTS: [(&str, &str); 4] = [
    (
        "Cormorant SC",
        include_str!("../../../godot/fonts/cormorant-sc/OFL.txt"),
    ),
    (
        "Alegreya Sans",
        include_str!("../../../godot/fonts/alegreya-sans/OFL.txt"),
    ),
    (
        "Alegreya Sans SC",
        include_str!("../../../godot/fonts/alegreya-sans-sc/OFL.txt"),
    ),
    (
        "PT Mono",
        include_str!("../../../godot/fonts/pt-mono/OFL.txt"),
    ),
];
const RUST_CRATES: &str = include_str!(concat!(env!("OUT_DIR"), "/rust_crates.json"));

/// The one who made the game (a name, the same in every language).
pub const AUTHOR: &str = "Vitaliy Velikodniy";

/// A document of the screen, in the order of its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Doc {
    Game,
    NetHack,
    Art,
    Sound,
    Help,
    Translation,
    Notice,
    Ngpl,
    Godot,
    GodotParts,
    Rust,
    EngineParts,
    Fonts,
    Steam,
}

impl Doc {
    /// The credits, then the licences (a Steam build's own last).
    pub fn credits() -> &'static [Doc] {
        &[
            Doc::Game,
            Doc::NetHack,
            Doc::Art,
            Doc::Sound,
            Doc::Help,
            Doc::Translation,
        ]
    }

    pub fn licences() -> Vec<Doc> {
        let mut all = vec![
            Doc::Notice,
            Doc::Ngpl,
            Doc::Godot,
            Doc::GodotParts,
            Doc::Rust,
            Doc::EngineParts,
            Doc::Fonts,
        ];
        if cfg!(feature = "steam") {
            all.push(Doc::Steam);
        }
        all
    }

    /// Its name in the list.
    pub fn name(self) -> String {
        match self {
            Doc::Game => tr!("credits-game"),
            Doc::NetHack => tr!("credits-nethack"),
            Doc::Art => tr!("credits-art"),
            Doc::Sound => tr!("credits-sound"),
            Doc::Help => tr!("credits-help"),
            Doc::Translation => tr!("credits-translation"),
            Doc::Notice => tr!("credits-notice"),
            Doc::Ngpl => tr!("credits-ngpl"),
            Doc::Godot => tr!("credits-godot"),
            Doc::GodotParts => tr!("credits-godot-parts"),
            Doc::Rust => tr!("credits-rust"),
            Doc::EngineParts => tr!("credits-engine-parts"),
            Doc::Fonts => tr!("credits-fonts"),
            Doc::Steam => tr!("credits-steam"),
        }
    }
}

/// What the game needs to draw the documents read where it runs: the
/// NGPL and Lua's notice from the engine's data directory.
#[derive(Debug, Clone, Default)]
pub struct Shipped {
    pub ngpl: Option<String>,
    pub lua: Option<String>,
}

/// A document as BBCode.
pub fn bbcode(doc: Doc, shipped: &Shipped) -> String {
    match doc {
        Doc::Game => game(),
        Doc::NetHack => nethack(),
        Doc::Art => markdown(ART),
        Doc::Sound => markdown(SOUND),
        Doc::Help => markdown(HELP),
        Doc::Translation => markdown(TRANSLATION),
        Doc::Notice => plain(&NOTICE.replace("@VERSION@", env!("CARGO_PKG_VERSION"))),
        Doc::Ngpl => match &shipped.ngpl {
            Some(text) => plain(text),
            None => note(&tr!("credits-ngpl-missing")),
        },
        Doc::Godot => plain(&Engine::singleton().get_license_text().to_string()),
        Doc::GodotParts => godot_parts(),
        Doc::Rust => rust_crates(),
        Doc::EngineParts => engine_parts(shipped),
        Doc::Fonts => fonts(),
        Doc::Steam => note(&tr!("credits-steam-text")),
    }
}

/// "#rrggbb" of a colour (without Godot: the tests draw too).
fn hex(c: Color) -> String {
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

/// A heading of a document.
fn heading(text: &str, size: i32) -> String {
    format!(
        "[font_size={size}][color={}][b]{}[/b][/color][/font_size]\n",
        hex(theme::GOLD_BRIGHT),
        theme::bbcode_escape(text)
    )
}

/// Words of the client (Fluent) in a document.
fn note(text: &str) -> String {
    format!("{}\n", theme::bbcode_escape(text))
}

/// A licence's text as it is written, its lines kept.
fn plain(text: &str) -> String {
    theme::bbcode_escape(text.trim())
}

fn game() -> String {
    let mut s = heading("renethack", 28);
    s += &note(&tr!("credits-game-text"));
    s += "\n";
    s += &note(&tr!("credits-game-by", author = AUTHOR));
    s += &note("Copyright 2026 Vitaliy Velikodniy.");
    s
}

fn nethack() -> String {
    let mut s = heading("NetHack 5.0", 28);
    s += &note(&tr!("credits-nethack-text"));
    s += "\n";
    s += &note(
        "NetHack, Copyright 1985-2026 Stichting Mathematisch Centrum, M. Stephenson, \
         the NetHack DevTeam and contributors.",
    );
    s += "\n";
    s += &note(&tr!("credits-nethack-licence"));
    s
}

/// Godot's third-party code: each part's copyright and licence, then the
/// licences' texts.
fn godot_parts() -> String {
    let engine = Engine::singleton();
    let mut s = String::new();
    let version = engine
        .get_version_info()
        .get("string")
        .map(|v| v.to_string());
    s += &note(&tr!(
        "credits-godot-parts-text",
        version = version.unwrap_or_default()
    ));
    s += "\n";
    for info in engine.get_copyright_info().iter_shared() {
        let name = info.get("name").map(|v| v.to_string()).unwrap_or_default();
        s += &format!("[b]{}[/b]\n", theme::bbcode_escape(&name));
        let parts = info
            .get("parts")
            .and_then(|p| p.try_to::<VarArray>().ok())
            .unwrap_or_default();
        for part in parts.iter_shared() {
            let Ok(part) = part.try_to::<VarDictionary>() else {
                continue;
            };
            if let Some(lines) = part
                .get("copyright")
                .and_then(|c| c.try_to::<PackedStringArray>().ok())
            {
                for line in lines.as_slice() {
                    s += &format!("    © {}\n", theme::bbcode_escape(&line.to_string()));
                }
            }
            if let Some(licence) = part.get("license") {
                s += &format!("    {}\n", theme::bbcode_escape(&licence.to_string()));
            }
        }
    }
    s += "\n";
    let texts = engine.get_license_info();
    for (name, text) in texts.iter_shared() {
        s += "\n";
        s += &heading(&name.to_string(), 20);
        s += &plain(&text.to_string());
        s += "\n";
    }
    s
}

#[derive(Debug, Default, Deserialize)]
struct Crates {
    crates: Vec<Crate>,
    texts: Vec<CrateText>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Crate {
    name: String,
    version: String,
    license: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct CrateText {
    text: String,
    users: Vec<String>,
}

fn crates() -> Crates {
    serde_json::from_str(RUST_CRATES).unwrap_or_default()
}

/// The Rust crates: each with its version, licence and source, then the
/// licences' texts and who ships each.
fn rust_crates() -> String {
    let all = crates();
    let mut s = note(&tr!("credits-rust-text"));
    if let Some(e) = all.error.as_deref() {
        s += &note(e);
    }
    s += "\n";
    // a table: the names, the licences and the sources each in a column
    s += "[table=3]";
    for c in &all.crates {
        s += &format!(
            "[cell padding=0,1,24,1][b]{} {}[/b][/cell][cell padding=0,1,24,1]{}[/cell][cell]{}[/cell]",
            theme::bbcode_escape(&c.name),
            theme::bbcode_escape(&c.version),
            theme::bbcode_escape(&c.license),
            link(&c.source)
        );
    }
    s += "[/table]\n";
    for t in &all.texts {
        s += "\n";
        s += &heading(&t.users.join(", "), 18);
        s += &plain(&t.text);
        s += "\n";
    }
    s
}

/// Inside the engine: Lua and cJSON.
fn engine_parts(shipped: &Shipped) -> String {
    let mut s = heading("Lua 5.4", 22);
    match &shipped.lua {
        Some(text) => s += &plain(text),
        None => {
            s += &note("Lua 5.4, Copyright 1994-2025 Lua.org, PUC-Rio: MIT licence.");
            s += &note(&tr!("credits-lua-missing"));
        }
    }
    s += "\n\n";
    s += &heading("cJSON", 22);
    s += &plain(CJSON);
    s
}

/// The fonts: each family's copyright, then the SIL Open Font License once.
fn fonts() -> String {
    let mut s = note(&tr!("credits-fonts-text"));
    s += "\n";
    for (family, text) in FONTS {
        s += &format!("[b]{}[/b]\n", theme::bbcode_escape(family));
        for line in text
            .lines()
            .take_while(|l| !l.starts_with("This Font Software"))
        {
            if !line.trim().is_empty() {
                s += &format!("    {}\n", theme::bbcode_escape(line.trim()));
            }
        }
    }
    s += "\n";
    let licence = FONTS[0]
        .1
        .split_once("-----------------------------------------------------------")
        .map_or(FONTS[0].1, |(_, rest)| rest);
    s += &plain(&format!(
        "-----------------------------------------------------------{licence}"
    ));
    s
}

/// A link the player can follow (the screen opens it in the browser).
fn link(url: &str) -> String {
    format!(
        "[color={}][url={u}]{u}[/url][/color]",
        hex(theme::GOLD),
        u = theme::bbcode_escape(url)
    )
}

// ---- Markdown ----

/// The Markdown of a credits file as BBCode, as a player reads it:
/// headings, paragraphs, bullet lists, and the tables' rows each a block
/// (the work, its author, its link); `code`, **bold**, *italics* and links
/// inline. What concerns only the repository (where a file lies, the
/// tools that fetch or make it, their recipes and locks) stays in the file
/// and out of the screen (`for_players`).
pub fn markdown(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let folders = folders(&lines);
    let mut out = String::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            let text = for_players(&para.join(" "), &folders);
            if !text.is_empty() {
                *out += &inline(&text);
                *out += "\n\n";
            }
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
        } else if let Some((level, text)) = heading_of(trimmed) {
            flush(&mut para, &mut out);
            let size = match level {
                1 => 28,
                2 => 22,
                _ => 19,
            };
            out += &format!(
                "[font_size={size}][color={}][b]{}[/b][/color][/font_size]\n",
                hex(theme::GOLD_BRIGHT),
                inline(text)
            );
            i += 1;
        } else if trimmed.starts_with('|') {
            flush(&mut para, &mut out);
            let start = i;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                i += 1;
            }
            out += &table(&lines[start..i], &folders);
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut para, &mut out);
            let mut text = vec![item.trim()];
            i += 1;
            // the item's lines that follow, indented
            while i < lines.len() && lines[i].starts_with("  ") && !lines[i].trim().is_empty() {
                text.push(lines[i].trim());
                i += 1;
            }
            let item = for_players(&text.join(" "), &folders);
            if !item.is_empty() {
                out += &format!("•  {}\n", inline(&item));
                if lines.get(i).is_none_or(|l| l.trim().is_empty()) {
                    out += "\n";
                }
            }
        } else {
            para.push(trimmed);
            i += 1;
        }
    }
    flush(&mut para, &mut out);
    out.trim_end().to_string()
}

/// Each folder the tables' "Where" column starts with (`cc0/`), and the
/// heading of the first table that has it: "everything under `cc0/`"
/// reads "everything listed under “Art credits”".
fn folders(lines: &[&str]) -> Vec<(String, String)> {
    let mut map: Vec<(String, String)> = Vec::new();
    let mut section = String::new();
    let mut place = None;
    for line in lines {
        let t = line.trim();
        if let Some((_, h)) = heading_of(t) {
            section = h.to_string();
            place = None;
        } else if t.starts_with('|') {
            let row = cells(t);
            match place {
                None => {
                    place = row.iter().position(|c| c.eq_ignore_ascii_case("where"));
                }
                Some(i) => {
                    let Some(cell) = row.get(i) else {
                        continue;
                    };
                    let Some(path) = cell.strip_prefix('`') else {
                        continue;
                    };
                    let Some(folder) = path.split('/').next().filter(|f| !f.is_empty()) else {
                        continue;
                    };
                    let folder = format!("{folder}/");
                    if !map.iter().any(|(f, _)| *f == folder) {
                        map.push((folder, section.clone()));
                    }
                }
            }
        } else if !t.is_empty() {
            place = None;
        }
    }
    map
}

/// A code span names something of the repository: a path, a file, a
/// setting or a word of code (`tools/x.py`, `sources.json`, `gain`), not
/// a key of the game (`?`).
fn internal_code(code: &str) -> bool {
    code.contains(['/', '\\', '=', '_', '.'])
        || (!code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
}

/// Text that speaks of the repository: a tool, a recipe or a lock, a
/// resource path, or a code span that names one.
fn has_internals(text: &str) -> bool {
    if ["tools/", ".json", "res://"]
        .iter()
        .any(|w| text.contains(w))
    {
        return true;
    }
    let mut parts = text.split('`');
    parts.next();
    // every other part is inside a code span
    parts.step_by(2).any(internal_code)
}

/// The indices where `text` splits at `sep` outside parentheses and code.
fn top_level_splits(text: &str, sep: &str) -> Vec<usize> {
    let mut depth = 0i32;
    let mut code = false;
    let mut at = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '`' => code = !code,
            '(' if !code => depth += 1,
            ')' if !code => depth -= 1,
            _ => {}
        }
        if depth == 0 && !code && text[i..].starts_with(sep) {
            at.push(i);
        }
    }
    at
}

/// `text` cut where it splits at `sep` (top level), each piece without
/// the separator.
fn split_top<'a>(text: &'a str, sep: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut from = 0;
    for i in top_level_splits(text, sep) {
        out.push(&text[from..i]);
        from = i + sep.len();
    }
    out.push(&text[from..]);
    out
}

/// The parentheses in `text` that hold something of the repository, gone
/// (with the space before them).
fn without_internal_parens(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        // its closing parenthesis, nested ones counted
        let mut depth = 0;
        let mut close = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            break;
        };
        let inner = &rest[open + 1..close];
        if has_internals(inner) {
            out += rest[..open].trim_end_matches(' ');
        } else {
            out += &rest[..=close];
        }
        rest = &rest[close + 1..];
    }
    out += rest;
    out
}

/// Sentences: a full stop (or ! or ?) and a space before a capital, not
/// after an initial ("J. R. R. Tolkien").
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut from = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (k, &(i, c)) in chars.iter().enumerate() {
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        let next = chars.get(k + 1).map(|&(_, n)| n);
        let after = chars.get(k + 2).map(|&(_, n)| n);
        if next != Some(' ') || !after.is_some_and(|a| a.is_uppercase() || a == '`' || a == '«') {
            continue;
        }
        // an initial: a single capital before the stop
        let word: String = text[from..i]
            .rsplit(' ')
            .next()
            .unwrap_or("")
            .chars()
            .collect();
        if word.chars().count() == 1 && word.chars().all(char::is_uppercase) {
            continue;
        }
        out.push(text[from..=i].trim());
        from = i + 2;
    }
    if from < text.len() {
        out.push(text[from..].trim());
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// A paragraph as a player reads it: the folders named by their section
/// ("listed under “Art credits”"); then the parentheses that hold the
/// repository's things, a "`file`: " before a sentence, the phrases and
/// clauses that speak of them gone; a sentence whose head speaks of them
/// gone whole. Empty when nothing is left.
pub fn for_players(text: &str, folders: &[(String, String)]) -> String {
    let mut text = text.to_string();
    for (folder, section) in folders {
        for word in ["under", "in"] {
            text = text.replace(
                &format!("{word} `{folder}`"),
                &format!("listed under “{section}”"),
            );
        }
    }
    let text = without_internal_parens(&text);
    let mut kept = Vec::new();
    for sentence in sentences(&text) {
        // "`engine/upstream/doc/Guidebook.mn`: the Guidebook..."
        let mut sentence = sentence;
        if sentence.starts_with('`')
            && let Some(end) = sentence[1..].find('`')
            && sentence[end + 2..].starts_with(": ")
            && internal_code(&sentence[1..end + 1])
        {
            sentence = &sentence[end + 4..];
        }
        let (body, stop) = match sentence.char_indices().last() {
            Some((i, '.' | '!' | '?')) => (&sentence[..i], &sentence[i..]),
            _ => (sentence, ""),
        };
        // clauses (; and :), each of phrases (,)
        let mut clauses: Vec<(String, &str)> = Vec::new();
        let mut head_lost = false;
        for (n, (sep, part)) in clause_parts(body).into_iter().enumerate() {
            let phrases = split_top(part, ", ");
            if has_internals(phrases[0]) {
                if n == 0 {
                    head_lost = true;
                    break;
                }
                continue;
            }
            let phrases: Vec<&str> = phrases.into_iter().filter(|p| !has_internals(p)).collect();
            clauses.push((phrases.join(", "), sep));
        }
        if head_lost || clauses.is_empty() {
            continue;
        }
        let mut s = String::new();
        for (k, (clause, sep)) in clauses.iter().enumerate() {
            if k > 0 {
                s += sep;
            }
            s += clause.trim();
        }
        s += if stop.is_empty() { "." } else { stop };
        kept.push(s);
    }
    kept.join(" ")
}

/// A sentence's clauses, each with the separator before it ("; " or
/// ": ").
fn clause_parts(body: &str) -> Vec<(&'static str, &str)> {
    let mut at: Vec<(usize, &'static str)> = top_level_splits(body, "; ")
        .into_iter()
        .map(|i| (i, "; "))
        .chain(top_level_splits(body, ": ").into_iter().map(|i| (i, ": ")))
        .collect();
    at.sort();
    let mut out = Vec::new();
    let mut from = 0;
    let mut sep = "";
    for (i, s) in at {
        out.push((sep, &body[from..i]));
        from = i + s.len();
        sep = s;
    }
    out.push((sep, &body[from..]));
    out
}

/// A heading's level and words.
fn heading_of(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    if level == 0 {
        return None;
    }
    line[level..].strip_prefix(' ').map(|t| (level, t.trim()))
}

/// The cells of a table's row.
fn cells(row: &str) -> Vec<String> {
    let row = row.trim().trim_start_matches('|').trim_end_matches('|');
    row.split('|').map(|c| c.trim().to_string()).collect()
}

/// A table, each row a block: its "What" in bold; "Author", "Source"
/// under it; "Where" small and dim. A table without those columns: its
/// cells in a row.
fn table(rows: &[&str], folders: &[(String, String)]) -> String {
    let Some((head, body)) = rows.split_first() else {
        return String::new();
    };
    let names: Vec<String> = cells(head).iter().map(|c| c.to_lowercase()).collect();
    let col = |name: &str| names.iter().position(|n| n == name);
    let (what, author, source) = (col("what"), col("author"), col("source"));
    let mut out = String::new();
    for row in body {
        let c = cells(row);
        // the separator under the head
        if c.iter().all(|x| x.chars().all(|ch| ch == '-' || ch == ':')) {
            continue;
        }
        // each cell as a player reads it; where the file lies, not at all
        let get = |i: Option<usize>| {
            i.and_then(|i| c.get(i))
                .map(|cell| for_players(cell, folders))
                .map(|t| t.trim_end_matches('.').to_string())
                .filter(|t| !t.is_empty())
        };
        match what {
            Some(_) => {
                out += &format!("[b]{}[/b]\n", inline(&get(what).unwrap_or_default()));
                let by: Vec<String> = [get(author), get(source)]
                    .into_iter()
                    .flatten()
                    .map(|t| inline(&t))
                    .collect();
                if !by.is_empty() {
                    out += &format!("{}\n", by.join("  —  "));
                }
                out += "\n";
            }
            None => {
                let parts: Vec<String> = c
                    .iter()
                    .map(|x| for_players(x, folders))
                    .filter(|t| !t.is_empty())
                    .map(|t| inline(&t))
                    .collect();
                out += &format!("{}\n", parts.join("  —  "));
            }
        }
    }
    out
}

/// Inline Markdown as BBCode: `code`, **bold**, *italics*, <links>,
/// [words](links) and bare links; the rest escaped.
pub fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let rest = |i: usize| chars[i..].iter().collect::<String>();
    while i < chars.len() {
        let c = chars[i];
        // `code`
        if c == '`'
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == '`')
        {
            let code: String = chars[i + 1..i + 1 + end].iter().collect();
            out += &format!("[code]{}[/code]", theme::bbcode_escape(&code));
            i += end + 2;
            continue;
        }
        // **bold**
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            let tail = rest(i + 2);
            if let Some(end) = tail.find("**") {
                out += &format!("[b]{}[/b]", inline(&tail[..end]));
                i += 2 + tail[..end].chars().count() + 2;
                continue;
            }
        }
        // *italics*
        if c == '*'
            && chars
                .get(i + 1)
                .is_some_and(|n| !n.is_whitespace() && *n != '*')
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == '*')
        {
            let words: String = chars[i + 1..i + 1 + end].iter().collect();
            out += &format!("[i]{}[/i]", inline(&words));
            i += end + 2;
            continue;
        }
        // <https://...>
        if c == '<' {
            let tail = rest(i + 1);
            if (tail.starts_with("http://") || tail.starts_with("https://"))
                && let Some(end) = tail.find('>')
            {
                out += &link(&tail[..end]);
                i += 1 + tail[..end].chars().count() + 1;
                continue;
            }
        }
        // [words](link)
        if c == '[' {
            let tail = rest(i + 1);
            if let Some(close) = tail.find("](")
                && let Some(end) = tail[close + 2..].find(')')
            {
                let words = &tail[..close];
                let url = &tail[close + 2..close + 2 + end];
                out += &format!("[url={}]{}[/url]", theme::bbcode_escape(url), inline(words));
                i += 1 + tail[..close + 2 + end].chars().count() + 1;
                continue;
            }
        }
        // a bare link, without what ends a sentence after it
        if c == 'h' {
            let tail = rest(i);
            if tail.starts_with("http://") || tail.starts_with("https://") {
                let mut url: String = tail.chars().take_while(|ch| !ch.is_whitespace()).collect();
                while url.ends_with(['.', ',', ';', ':', ')']) {
                    url.pop();
                }
                out += &link(&url);
                i += url.chars().count();
                continue;
            }
        }
        out += &theme::bbcode_escape(&c.to_string());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_paragraphs_and_lists_are_blocks() {
        let md = "# Sound credits\n\nPress `?` for\nthe help.\n\n- **pymorphy3** — <https://example.org/p>, MIT,\n  with its dictionary.\n";
        let bb = markdown(md);
        assert!(bb.starts_with("[font_size=28]"), "{bb}");
        assert!(bb.contains("Sound credits[/b]"), "{bb}");
        assert!(
            bb.contains("Press [code]?[/code] for the help.\n\n"),
            "{bb}"
        );
        assert!(
            bb.contains("•  [b]pymorphy3[/b] — [color=#b8893b][url=https://example.org/p]https://example.org/p[/url][/color], MIT, with its dictionary."),
            "{bb}"
        );
    }

    #[test]
    fn a_table_row_is_the_work_then_its_author_and_link() {
        let md = "| Where | What | Author | Source |\n|---|---|---|---|\n| `ccby/x.glb` | Elephant (mumakil) | Poly by Google | https://poly.pizza/m/cx0 |\n";
        let bb = markdown(md);
        assert!(bb.contains("[b]Elephant (mumakil)[/b]\n"), "{bb}");
        assert!(
            bb.contains(
                "Poly by Google  —  [color=#b8893b][url=https://poly.pizza/m/cx0]https://poly.pizza/m/cx0[/url][/color]"
            ),
            "{bb}"
        );
        assert!(
            !bb.contains("ccby"),
            "where the file lies is not shown: {bb}"
        );
        assert!(!bb.contains("---"), "the separator row is not drawn: {bb}");
    }

    #[test]
    fn brackets_are_escaped_and_links_lose_the_full_stop_after_them() {
        assert_eq!(inline("a [b] c"), "a [lb]b] c");
        assert_eq!(
            inline("see https://kenney.nl."),
            "see [color=#b8893b][url=https://kenney.nl]https://kenney.nl[/url][/color]."
        );
        assert_eq!(inline("*Dungeons*"), "[i]Dungeons[/i]");
        assert_eq!(
            inline("[site](https://a.b/c)"),
            "[url=https://a.b/c]site[/url]"
        );
    }

    #[test]
    fn every_credits_file_draws_and_keeps_its_attributions() {
        for md in [ART, SOUND, HELP, TRANSLATION] {
            let bb = markdown(md);
            assert!(bb.len() > 200);
            assert!(!bb.contains("|---"), "a table's separator drawn");
        }
        // the CC-BY models and their authors, as the licence asks
        let art = markdown(ART);
        assert!(art.contains("Creative Commons Attribution 3.0"));
        assert!(art.contains("Poly by Google"));
        assert!(markdown(SOUND).contains("Kenney"));
    }

    #[test]
    fn a_player_reads_no_tool_path_recipe_or_lock() {
        for (name, md) in [
            ("art", ART),
            ("sound", SOUND),
            ("help", HELP),
            ("i18n", TRANSLATION),
        ] {
            let bb = markdown(md);
            for word in [
                "tools/", ".json", "res://", "cc0/", "ccby/", ".py", ".toml", ".glb",
            ] {
                assert!(!bb.contains(word), "{name} shows {word:?}:\n{bb}");
            }
        }
        // what a player must read stays
        let art = markdown(ART);
        assert!(
            art.contains("Everything listed under “Art credits” is dedicated to the public domain")
        );
        assert!(art.contains(
            "The models listed under “CC-BY” are under the Creative Commons Attribution 3.0"
        ));
        assert!(
            art.contains("skinned to the skeleton"),
            "the changes a CC-BY model saw"
        );
        assert!(art.contains("SIL Open Font License 1.1"));
        assert!(art.contains("Skins painted: a corpse"), "{art}");
        let help = markdown(HELP);
        assert!(help.contains("A Guide to the Mazes of Menace"));
        assert!(help.contains("Eric S. Raymond"));
        assert!(help.contains("Руководство по Грозным Лабиринтам"));
        assert!(help.contains("Vadim Velikodniy"));
        assert!(help.contains("under the NetHack General Public License"));
        let i18n = markdown(TRANSLATION);
        for w in [
            "Skadga/nethack-ru",
            "pymorphy3",
            "OpenCorpora",
            "J. R. R. Tolkien",
        ] {
            assert!(i18n.contains(w), "{w}: {i18n}");
        }
        let sound = markdown(SOUND);
        assert!(sound.contains("Creative Commons Attribution"));
        assert!(sound.contains("Changes made"));
    }

    #[test]
    fn a_paragraph_loses_its_tool_clauses_and_keeps_its_sentences() {
        let none: [(String, String); 0] = [];
        assert_eq!(
            for_players(
                "Credit is not required; it is given gladly. `tools/x.py` fetches them again.",
                &none
            ),
            "Credit is not required; it is given gladly."
        );
        assert_eq!(
            for_players(
                "`engine/doc/Guide.mn`: the Guide, by J. R. R. Smith.",
                &none
            ),
            "the Guide, by J. R. R. Smith."
        );
        assert_eq!(
            for_players(
                "From the site at commit 576aa83, checked in `help.lock.json`.",
                &none
            ),
            "From the site at commit 576aa83."
        );
        assert_eq!(
            for_players(
                "Recoloured (the recipe's `gain`: a true metal is black).",
                &none
            ),
            "Recoloured."
        );
        let folders = [("cc0/".to_string(), "Art credits".to_string())];
        assert_eq!(
            for_players("Everything under `cc0/` is free.", &folders),
            "Everything listed under “Art credits” is free."
        );
    }

    #[test]
    fn the_fonts_licence_names_every_family_and_its_holders() {
        for (family, text) in FONTS {
            assert!(text.contains("SIL Open Font License"), "{family}");
        }
        assert!(NOTICE.contains("@VERSION@"));
        assert!(CJSON.contains("Dave Gamble"));
    }
}
