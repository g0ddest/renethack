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

/// The Markdown of a credits file as BBCode: headings, paragraphs, bullet
/// lists, and the tables' rows each a block (what, by whom and from where,
/// where in the game's files under them); `code`, **bold**, *italics* and
/// links inline.
pub fn markdown(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = String::new();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            *out += &inline(&para.join(" "));
            *out += "\n\n";
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
            out += &table(&lines[start..i]);
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
            out += &format!("•  {}\n", inline(&text.join(" ")));
            if lines.get(i).is_none_or(|l| l.trim().is_empty()) {
                out += "\n";
            }
        } else {
            para.push(trimmed);
            i += 1;
        }
    }
    flush(&mut para, &mut out);
    out.trim_end().to_string()
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
fn table(rows: &[&str]) -> String {
    let Some((head, body)) = rows.split_first() else {
        return String::new();
    };
    let names: Vec<String> = cells(head).iter().map(|c| c.to_lowercase()).collect();
    let col = |name: &str| names.iter().position(|n| n == name);
    let (what, author, source, place) = (col("what"), col("author"), col("source"), col("where"));
    let mut out = String::new();
    for row in body {
        let c = cells(row);
        // the separator under the head
        if c.iter().all(|x| x.chars().all(|ch| ch == '-' || ch == ':')) {
            continue;
        }
        let get = |i: Option<usize>| i.and_then(|i| c.get(i)).map(String::as_str);
        match what {
            Some(_) => {
                out += &format!("[b]{}[/b]\n", inline(get(what).unwrap_or("")));
                let by: Vec<String> = [get(author), get(source)]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .map(inline)
                    .collect();
                if !by.is_empty() {
                    out += &format!("{}\n", by.join("  —  "));
                }
                if let Some(p) = get(place).filter(|p| !p.is_empty()) {
                    out += &format!(
                        "[font_size=14][color={}]{}[/color][/font_size]\n",
                        hex(theme::TEXT_DIM),
                        inline(p)
                    );
                }
                out += "\n";
            }
            None => {
                let parts: Vec<String> = c.iter().map(|x| inline(x)).collect();
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
        let md = "# Sound credits\n\nEverything under `cc0/` is\nfree.\n\n- **pymorphy3** — <https://example.org/p>, MIT,\n  with its dictionary.\n";
        let bb = markdown(md);
        assert!(bb.starts_with("[font_size=28]"), "{bb}");
        assert!(bb.contains("Sound credits[/b]"), "{bb}");
        assert!(
            bb.contains("Everything under [code]cc0/[/code] is free.\n\n"),
            "{bb}"
        );
        assert!(
            bb.contains("•  [b]pymorphy3[/b] — [color=#b8893b][url=https://example.org/p]https://example.org/p[/url][/color], MIT, with its dictionary."),
            "{bb}"
        );
    }

    #[test]
    fn a_table_row_is_what_then_by_whom_then_where() {
        let md = "| Where | What | Author | Source |\n|---|---|---|---|\n| `ccby/x.glb` | Elephant (mumakil) | Poly by Google | https://poly.pizza/m/cx0 |\n";
        let bb = markdown(md);
        assert!(bb.contains("[b]Elephant (mumakil)[/b]\n"), "{bb}");
        assert!(
            bb.contains(
                "Poly by Google  —  [color=#b8893b][url=https://poly.pizza/m/cx0]https://poly.pizza/m/cx0[/url][/color]\n"
            ),
            "{bb}"
        );
        assert!(bb.contains("[code]ccby/x.glb[/code]"), "{bb}");
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
    fn the_fonts_licence_names_every_family_and_its_holders() {
        for (family, text) in FONTS {
            assert!(text.contains("SIL Open Font License"), "{family}");
        }
        assert!(NOTICE.contains("@VERSION@"));
        assert!(CJSON.contains("Dave Gamble"));
    }
}
