//! The client's own words in the player's language: Project Fluent
//! catalogs built in (`client/i18n/en.ftl` and `ru.ftl`; English is the
//! source and the fallback). `tr!("key")` and `tr!("key", n = 3)` give a
//! string; `bind` keeps a widget's static label in the language, also
//! after a switch. The engine's own words (messages, names, menus, text
//! windows) are not here: they reach the player through `engine`, where
//! the engine's translator (nh-i18n) plugs in.
//!
//! The pseudo-language (`--lang=qps`, for self-tests) marks every word
//! that comes through here: ⟦the client's⟧, ⟪the engine's⟫; a word on
//! screen outside the marks is one the code shows as it is.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};

pub use fluent_bundle::FluentArgs;
use fluent_bundle::{FluentBundle, FluentResource, FluentValue};
use godot::prelude::*;
use nh_world::{FmtArg, Message};

/// A language of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Lang {
    #[default]
    En,
    Ru,
    /// English with the marks (self-tests; never offered).
    Pseudo,
}

impl Lang {
    /// The languages a player picks from.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];

    /// Its code in the profile and the character's state ("en", "ru").
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
            Lang::Pseudo => "qps",
        }
    }

    /// A code or a locale ("ru", "ru_RU", "en-GB"); None for another
    /// language.
    pub fn from_code(code: &str) -> Option<Lang> {
        let code = code.trim().to_ascii_lowercase();
        match code.split(['_', '-', '.']).next()? {
            "en" => Some(Lang::En),
            "ru" => Some(Lang::Ru),
            "qps" => Some(Lang::Pseudo),
            _ => None,
        }
    }

    /// Its own name, as the language picker shows it.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::Pseudo => "Pseudo",
        }
    }

    /// Its bundle among `ALL`'s (the pseudo-language's words are
    /// English).
    fn index(self) -> usize {
        match self {
            Lang::Pseudo => Lang::En as usize,
            other => other as usize,
        }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En | Lang::Pseudo => EN_FTL,
            Lang::Ru => RU_FTL,
        }
    }
}

const EN_FTL: &str = include_str!("../../../i18n/en.ftl");
const RU_FTL: &str = include_str!("../../../i18n/ru.ftl");

/// Each language's bundle, made on first use.
struct Catalog {
    bundles: Vec<FluentBundle<FluentResource>>,
}

impl Catalog {
    fn new() -> Catalog {
        let bundles = Lang::ALL
            .iter()
            .map(|&lang| {
                let id = lang.code().parse().expect("a language code is a locale");
                let mut bundle = FluentBundle::new(vec![id]);
                // no bidi marks around arguments: the interface is
                // left-to-right, and labels would show them
                bundle.set_use_isolating(false);
                let resource = match FluentResource::try_new(lang.source().to_string()) {
                    Ok(r) => r,
                    Err((r, errors)) => {
                        godot_error!("i18n: {}.ftl does not parse: {errors:?}", lang.code());
                        r
                    }
                };
                if let Err(errors) = bundle.add_resource(resource) {
                    godot_error!("i18n: {}.ftl: {errors:?}", lang.code());
                }
                bundle
            })
            .collect();
        Catalog { bundles }
    }

    fn format(&self, lang: Lang, id: &str, args: Option<&FluentArgs>) -> Option<String> {
        let bundle = &self.bundles[lang.index()];
        let pattern = bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        let text = bundle.format_pattern(pattern, args, &mut errors);
        if !errors.is_empty() {
            godot_warn!("i18n: {}: {id}: {errors:?}", lang.code());
        }
        Some(text.into_owned())
    }
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::En) };
    static CATALOG: Catalog = Catalog::new();
    static BOUND: RefCell<Vec<Bound>> = const { RefCell::new(Vec::new()) };
    static ENGINE: RefCell<Option<Box<dyn EngineText>>> = const { RefCell::new(None) };
}

/// The interface's language now.
pub fn lang() -> Lang {
    LANG.with(Cell::get)
}

/// Switch the interface's language: bound labels change at once; the
/// views that draw their own words are drawn again by the game
/// (`relang`).
pub fn set_lang(lang: Lang) {
    if LANG.with(|l| l.replace(lang)) == lang {
        return;
    }
    retranslate();
}

/// The string of `id` in the language now, else in English, else the id
/// itself (a missing key shows, and is reported).
pub fn tr(id: &str) -> String {
    tr_with(id, None)
}

/// `tr` with Fluent arguments (see the `tr!` macro).
pub fn tr_with(id: &str, args: Option<&FluentArgs>) -> String {
    let lang = lang();
    let text = CATALOG.with(|c| {
        c.format(lang, id, args)
            .or_else(|| {
                (lang != Lang::En)
                    .then(|| c.format(Lang::En, id, args))
                    .flatten()
            })
            .unwrap_or_else(|| {
                godot_warn!("i18n: no string for {id}");
                id.to_string()
            })
    });
    match lang {
        Lang::Pseudo => format!("⟦{text}⟧"),
        _ => text,
    }
}

/// The string of `id` in `lang` alone, without the pseudo-language's
/// marks (files made for others: Steam's achievements).
pub fn tr_in(lang: Lang, id: &str) -> Option<String> {
    CATALOG.with(|c| c.format(lang, id, None))
}

/// The catalogs have a string for `id`.
pub fn has(id: &str) -> bool {
    CATALOG.with(|c| c.bundles[Lang::En.index()].has_message(id))
}

/// A key that comes as data ("item.put_on_left", from nh-world): its
/// Fluent id has dashes ("item-put-on-left").
pub fn fluent_id(key: &str) -> String {
    key.replace(['.', '_'], "-")
}

/// A hint of " · "-separated parts ("Enter: OK · Esc: cancel") wraps
/// between its parts, never inside one: the spaces in a part become
/// no-break spaces.
pub fn whole_parts(hint: &str) -> String {
    hint.split(" · ")
        .map(|part| part.replace(' ', "\u{a0}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A value for a Fluent argument: numbers stay numbers (plural forms
/// choose by them), the rest is text.
pub trait Arg {
    fn fluent(self) -> FluentValue<'static>;
}

macro_rules! number_arg {
    ($($t:ty),*) => {$(
        impl Arg for $t {
            fn fluent(self) -> FluentValue<'static> {
                FluentValue::from(self as f64)
            }
        }
    )*};
}
number_arg!(i32, i64, u32, u64, usize, isize, u8, u16);

impl Arg for &str {
    fn fluent(self) -> FluentValue<'static> {
        FluentValue::from(self.to_string())
    }
}

impl Arg for String {
    fn fluent(self) -> FluentValue<'static> {
        FluentValue::from(self)
    }
}

impl Arg for &String {
    fn fluent(self) -> FluentValue<'static> {
        FluentValue::from(self.clone())
    }
}

impl Arg for char {
    fn fluent(self) -> FluentValue<'static> {
        FluentValue::from(self.to_string())
    }
}

/// The client's own string for `key` in the language now, with Fluent
/// arguments: `tr!("hud-gold", amount = 12)`.
#[macro_export]
macro_rules! tr {
    ($id:literal) => {
        $crate::i18n::tr($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::i18n::FluentArgs::new();
        $( args.set(stringify!($name), $crate::i18n::Arg::fluent($value)); )+
        $crate::i18n::tr_with($id, Some(&args))
    }};
}

/// A widget's property kept in the language: set now, and again on each
/// switch while the widget lives.
struct Bound {
    id: InstanceId,
    property: &'static str,
    key: &'static str,
}

/// Set `property` ("text", "tooltip_text", "placeholder_text") of `node`
/// to the string of `key`, now and after every switch of the language.
pub fn bind<T: Inherits<Object>>(node: &Gd<T>, property: &'static str, key: &'static str) {
    let mut object = node.clone().upcast::<Object>();
    object.set(property, &tr(key).to_variant());
    let id = object.instance_id();
    BOUND.with(|b| {
        let mut bound = b.borrow_mut();
        bound.retain(|x| !(x.id == id && x.property == property));
        bound.push(Bound { id, property, key });
    });
}

/// `bind` of the widget's text.
pub fn text<T: Inherits<Object>>(node: &Gd<T>, key: &'static str) {
    bind(node, "text", key);
}

/// `bind` of the widget's tooltip.
pub fn tip<T: Inherits<Object>>(node: &Gd<T>, key: &'static str) {
    bind(node, "tooltip_text", key);
}

/// The meta of a widget whose text is the same in every language.
pub const VERBATIM: &str = "i18n_verbatim";

/// Mark a widget whose text is the same in every language (a language's
/// own name, a controller's button, a command as the player types it):
/// the pseudo-language's check passes it by.
pub fn verbatim<T: Inherits<Object>>(node: &Gd<T>) {
    node.clone()
        .upcast::<Object>()
        .set_meta(VERBATIM, &true.to_variant());
}

/// The bound labels in the language now (those whose widget is gone are
/// forgotten).
fn retranslate() {
    let bound = BOUND.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let mut kept = Vec::with_capacity(bound.len());
    for b in bound {
        if let Ok(mut object) = Gd::<Object>::try_from_instance_id(b.id) {
            object.set(b.property, &tr(b.key).to_variant());
            kept.push(b);
        }
    }
    BOUND.with(|b| b.borrow_mut().extend(kept));
}

/// Forget every bound widget (the game is shutting down).
pub fn clear() {
    BOUND.with(|b| b.borrow_mut().clear());
    ENGINE.with(|e| e.borrow_mut().take());
}

/// What kind of the engine's words a string is, for its translator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// A message of the log (pline).
    Message,
    /// A question (yn, getlin, getobj's prompt, getpos's).
    Prompt,
    /// A menu's title or entry.
    Menu,
    /// A line of a text window.
    Window,
    /// A name: a role, race, gender, alignment, monster, object.
    Name,
    /// A word of the status line (conditions, hunger, rank).
    Status,
}

/// The engine's translator (nh-i18n): the English the engine wrote, in
/// another language, or None to show the English.
pub trait EngineText {
    fn translate(&self, lang: Lang, kind: EngineKind, english: &str) -> Option<String>;

    /// A message of the log with the printf format the engine made it
    /// from and the format's arguments, the surer key of its translation
    /// (None, no arguments: history, or text the host could not pair with
    /// a format); by default its text alone.
    fn message(
        &self,
        lang: Lang,
        fmt: Option<&str>,
        args: &[FmtArg],
        english: &str,
    ) -> Option<String> {
        let _ = (fmt, args);
        self.translate(lang, EngineKind::Message, english)
    }

    /// A whole text window (a quest's text, an oracle, a help), which a
    /// translation may say in another number of lines; by default line by
    /// line.
    fn window(&self, lang: Lang, lines: &[&str]) -> Option<Vec<String>> {
        let shown: Vec<Option<String>> = lines
            .iter()
            .map(|l| self.translate(lang, EngineKind::Window, l))
            .collect();
        shown.iter().any(Option::is_some).then(|| {
            shown
                .into_iter()
                .zip(lines)
                .map(|(s, l)| s.unwrap_or_else(|| l.to_string()))
                .collect()
        })
    }

    /// The hero is female (or not): the words that agree with the hero.
    fn set_hero_female(&self, female: bool) {
        let _ = female;
    }

    /// A word that describes the hero (an alignment: Нейтральная for a
    /// heroine), agreeing with the hero; by default as a status word.
    fn hero_word(&self, lang: Lang, english: &str) -> Option<String> {
        self.translate(lang, EngineKind::Status, english)
    }
}

/// Plug the engine's translator in (None: the English shows).
pub fn set_engine_text(translator: Option<Box<dyn EngineText>>) {
    ENGINE.with(|e| *e.borrow_mut() = translator);
}

/// The hero's gender, for the translator's agreement.
pub fn set_hero_female(female: bool) {
    ENGINE.with(|e| {
        if let Some(t) = e.borrow().as_ref() {
            t.set_hero_female(female);
        }
    });
}

/// A word of the engine's that describes the hero, as the player reads it
/// (agreeing with the hero where the language wants it).
pub fn engine_hero_word(english: &str) -> Cow<'_, str> {
    let lang = lang();
    match lang {
        Lang::En => return Cow::Borrowed(english),
        Lang::Pseudo => return Cow::Owned(mark_columns(english)),
        Lang::Ru => {}
    }
    ENGINE.with(|e| {
        e.borrow()
            .as_ref()
            .and_then(|t| t.hero_word(lang, english))
            .map_or(Cow::Borrowed(english), Cow::Owned)
    })
}

/// A text window's lines as the player reads them (None: as written; the
/// pseudo-language marks each).
pub fn engine_window(lines: &[&str]) -> Option<Vec<String>> {
    let lang = lang();
    match lang {
        Lang::En => None,
        Lang::Pseudo => Some(lines.iter().map(|l| mark_columns(l)).collect()),
        Lang::Ru => ENGINE.with(|e| e.borrow().as_ref().and_then(|t| t.window(lang, lines))),
    }
}

/// A message of the log as the player reads it (see `engine`): its format
/// and arguments go to the translator with it.
pub fn engine_message(m: &Message) -> Cow<'_, str> {
    let lang = lang();
    match lang {
        Lang::En => return Cow::Borrowed(&m.text),
        Lang::Pseudo => return Cow::Owned(mark_columns(&m.text)),
        Lang::Ru => {}
    }
    ENGINE.with(|e| {
        e.borrow()
            .as_ref()
            .and_then(|t| t.message(lang, m.fmt.as_deref(), &m.args, &m.text))
            .map_or(Cow::Borrowed(m.text.as_str()), Cow::Owned)
    })
}

/// What the engine wrote, as the player reads it: the English kept as is
/// (views keep the originals and draw them again on a switch).
pub fn engine(kind: EngineKind, english: &str) -> Cow<'_, str> {
    let lang = lang();
    match lang {
        Lang::En => return Cow::Borrowed(english),
        Lang::Pseudo => return Cow::Owned(mark_columns(english)),
        Lang::Ru => {}
    }
    ENGINE.with(|e| {
        e.borrow()
            .as_ref()
            .and_then(|t| t.translate(lang, kind, english))
            .map_or(Cow::Borrowed(english), Cow::Owned)
    })
}

/// The engine's text marked for the pseudo-language, each of its columns
/// (two spaces or more part them, as menus line them up) alone: views cut
/// a text into its columns.
fn mark_columns(english: &str) -> String {
    let mut out = String::with_capacity(english.len() + 8);
    let mut field = String::new();
    let mut gap = String::new();
    let close = |out: &mut String, field: &mut String| {
        if !field.is_empty() {
            out.push('⟪');
            out.push_str(field);
            out.push('⟫');
            field.clear();
        }
    };
    for c in english.chars() {
        if c == ' ' {
            gap.push(c);
            continue;
        }
        if !gap.is_empty() {
            if gap.len() >= 2 || field.is_empty() {
                close(&mut out, &mut field);
                out.push_str(&gap);
            } else {
                field.push_str(&gap);
            }
            gap.clear();
        }
        field.push(c);
    }
    close(&mut out, &mut field);
    out.push_str(&gap);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_syntax::ast;

    /// The messages of a catalog, with the variables each uses.
    fn messages(source: &str) -> Vec<(String, Vec<String>)> {
        let resource = match fluent_syntax::parser::parse(source) {
            Ok(r) => r,
            Err((_, errors)) => panic!("the catalog does not parse: {errors:?}"),
        };
        resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                ast::Entry::Message(m) => {
                    let mut vars = Vec::new();
                    if let Some(p) = &m.value {
                        pattern_vars(p, &mut vars);
                    }
                    vars.sort();
                    vars.dedup();
                    Some((m.id.name.to_string(), vars))
                }
                _ => None,
            })
            .collect()
    }

    fn pattern_vars(p: &ast::Pattern<&str>, out: &mut Vec<String>) {
        for e in &p.elements {
            if let ast::PatternElement::Placeable { expression } = e {
                expression_vars(expression, out);
            }
        }
    }

    fn expression_vars(e: &ast::Expression<&str>, out: &mut Vec<String>) {
        match e {
            ast::Expression::Inline(i) => inline_vars(i, out),
            ast::Expression::Select { selector, variants } => {
                inline_vars(selector, out);
                for v in variants {
                    pattern_vars(&v.value, out);
                }
            }
        }
    }

    fn inline_vars(i: &ast::InlineExpression<&str>, out: &mut Vec<String>) {
        match i {
            ast::InlineExpression::VariableReference { id } => out.push(id.name.to_string()),
            ast::InlineExpression::Placeable { expression } => expression_vars(expression, out),
            ast::InlineExpression::FunctionReference { arguments, .. } => {
                for a in &arguments.positional {
                    inline_vars(a, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn every_key_is_in_both_languages_with_the_same_arguments() {
        let en = messages(EN_FTL);
        let ru = messages(RU_FTL);
        assert!(en.len() > 100, "the English catalog has {} keys", en.len());
        let en_keys: Vec<&String> = en.iter().map(|(k, _)| k).collect();
        let ru_keys: Vec<&String> = ru.iter().map(|(k, _)| k).collect();
        let missing: Vec<_> = en_keys.iter().filter(|k| !ru_keys.contains(k)).collect();
        let extra: Vec<_> = ru_keys.iter().filter(|k| !en_keys.contains(k)).collect();
        assert!(missing.is_empty(), "not in ru.ftl: {missing:?}");
        assert!(extra.is_empty(), "only in ru.ftl: {extra:?}");
        let mut seen = std::collections::HashSet::new();
        for (k, _) in &en {
            assert!(seen.insert(k), "{k} twice in en.ftl");
        }
        for (k, vars) in &en {
            let theirs = &ru.iter().find(|(r, _)| r == k).expect("in both").1;
            assert_eq!(vars, theirs, "{k}: the arguments differ between en and ru");
        }
    }

    #[test]
    fn every_message_formats_cleanly_in_both_languages() {
        let catalog = Catalog::new();
        for lang in Lang::ALL {
            for (key, vars) in messages(lang.source()) {
                let mut args = FluentArgs::new();
                for v in &vars {
                    // a number for every argument: plural selectors need
                    // one, and a number formats as text anywhere
                    args.set(v.as_str(), FluentValue::from(2.0));
                }
                let bundle = &catalog.bundles[lang.index()];
                let pattern = bundle
                    .get_message(&key)
                    .and_then(|m| m.value())
                    .unwrap_or_else(|| panic!("{key} has no value in {}", lang.code()));
                let mut errors = Vec::new();
                let text = bundle.format_pattern(pattern, Some(&args), &mut errors);
                assert!(errors.is_empty(), "{}: {key}: {errors:?}", lang.code());
                assert!(
                    !text.trim().is_empty() || key.ends_with("-empty"),
                    "{key} is empty"
                );
            }
        }
    }

    /// The string literals of the Rust files in `dir` (tests left out:
    /// they sit at the end of a file).
    fn literals(dir: &str) -> Vec<(String, String)> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{dir}: {e}"))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .collect();
        paths.sort();
        let mut out = Vec::new();
        for path in paths {
            let text = std::fs::read_to_string(&path).expect("a source file");
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            out.extend(string_literals(code).into_iter().map(|l| (file.clone(), l)));
        }
        out
    }

    /// The contents of the string literals of Rust code, comments and
    /// character literals passed by.
    fn string_literals(code: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut chars = code.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '/' if code[i..].starts_with("//") => {
                    for (_, c) in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                '/' if code[i..].starts_with("/*") => {
                    let end = code[i + 2..]
                        .find("*/")
                        .map_or(code.len(), |e| i + 2 + e + 2);
                    while chars.peek().is_some_and(|&(j, _)| j < end) {
                        chars.next();
                    }
                }
                // a character literal ('"', '\'', 'é'), else a lifetime
                '\'' => {
                    let rest = &code[i + 1..];
                    let len = if rest.starts_with('\\') {
                        rest[2..].find('\'').map(|e| e + 3)
                    } else {
                        let c = rest.chars().next().map_or(0, char::len_utf8);
                        rest[c..].starts_with('\'').then_some(c + 1)
                    };
                    if let Some(len) = len {
                        let end = i + 1 + len;
                        while chars.peek().is_some_and(|&(j, _)| j < end) {
                            chars.next();
                        }
                    }
                }
                '"' => {
                    let mut s = String::new();
                    while let Some((_, c)) = chars.next() {
                        match c {
                            '\\' => {
                                if let Some((_, e)) = chars.next() {
                                    s.push('\\');
                                    s.push(e);
                                }
                            }
                            '"' => break,
                            c => s.push(c),
                        }
                    }
                    out.push(s);
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn the_source_reader_finds_string_literals() {
        let code = r##"let a = tr!("hud-gold"); // "not this"
            let b = '"'; let c = '\''; /* "nor this" */ fn f<'a>(x: &'a str) {}
            let d = "say \"hi\"";"##;
        assert_eq!(string_literals(code), ["hud-gold", "say \\\"hi\\\""]);
    }

    /// Every key the code names is in the catalogs, and every key of the
    /// catalogs is named somewhere: the client's literals that look like
    /// a key of a group the catalogs have ("hud-gold"), the label keys of
    /// nh-world's actions and tabs ("item.wield", "inv.weapons"), and its
    /// bar commands' ("cmd.search").
    #[test]
    fn the_catalogs_have_every_key_the_code_names_and_no_other() {
        let keys: Vec<String> = messages(EN_FTL).into_iter().map(|(k, _)| k).collect();
        let groups: std::collections::HashSet<&str> =
            keys.iter().filter_map(|k| k.split('-').next()).collect();
        let looks_like_a_key = |s: &str| {
            let mut parts = s.split('-');
            let first = parts.next().unwrap_or("");
            s.contains('-')
                && groups.contains(first)
                && s.split('-').all(|p| {
                    !p.is_empty()
                        && p.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                })
        };
        let manifest = env!("CARGO_MANIFEST_DIR");
        // the self-tests' names of scenarios and screens ("item-use",
        // "hud-start") are not keys
        let mut named: Vec<(String, String)> = literals(&format!("{manifest}/src"))
            .into_iter()
            .filter(|(f, l)| !f.starts_with("selftest") && looks_like_a_key(l))
            .collect();
        named.extend(
            literals(&format!("{manifest}/../nh-world/src"))
                .into_iter()
                .filter(|(_, l)| {
                    let mut p = l.splitn(2, '.');
                    matches!(p.next(), Some("item" | "inv" | "cmd"))
                        && p.next().is_some_and(|r| {
                            !r.is_empty() && r.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                        })
                })
                .map(|(f, l)| (format!("nh-world/{f}"), fluent_id(&l))),
        );
        named.extend(
            nh_world::BarCommand::ALL
                .iter()
                .map(|c| ("BarCommand".to_string(), fluent_id(&c.label_key()))),
        );
        let missing: Vec<String> = named
            .iter()
            .filter(|(_, k)| !keys.contains(k))
            .map(|(f, k)| format!("{k} ({f})"))
            .collect();
        assert!(missing.is_empty(), "not in en.ftl: {missing:?}");
        // the achievements' names and descriptions are keys their
        // definitions name
        let achievements: Vec<String> = nh_world::achievements::Achievements::built_in()
            .all()
            .iter()
            .flat_map(|a| [a.name.clone(), a.desc.clone()])
            .collect();
        let missing: Vec<&String> = achievements.iter().filter(|k| !keys.contains(k)).collect();
        assert!(
            missing.is_empty(),
            "achievements not in en.ftl: {missing:?}"
        );
        let unused: Vec<&String> = keys
            .iter()
            .filter(|k| !named.iter().any(|(_, n)| n == *k) && !achievements.contains(k))
            .collect();
        assert!(unused.is_empty(), "keys nothing names: {unused:?}");
    }

    #[test]
    fn russian_numbers_take_their_plural_forms() {
        let catalog = Catalog::new();
        let format = |n: f64| {
            let mut args = FluentArgs::new();
            args.set("n", FluentValue::from(n));
            catalog
                .format(Lang::Ru, "log-count", Some(&args))
                .expect("log-count")
        };
        assert!(format(1.0).contains("сообщение"), "{}", format(1.0));
        assert!(format(3.0).contains("сообщения"), "{}", format(3.0));
        assert!(format(5.0).contains("сообщений"), "{}", format(5.0));
        assert!(format(21.0).contains("сообщение"), "{}", format(21.0));
    }

    #[test]
    fn the_pseudo_language_marks_each_column_of_the_engine() {
        assert_eq!(mark_columns("a long sword"), "⟪a long sword⟫");
        assert_eq!(
            mark_columns("pickup_types   all  (for autopickup)"),
            "⟪pickup_types⟫   ⟪all⟫  ⟪(for autopickup)⟫"
        );
        assert_eq!(mark_columns("    (ignored) "), "    ⟪(ignored)⟫ ");
        assert_eq!(mark_columns(""), "");
    }

    #[test]
    fn the_engines_translator_has_a_message_with_its_format() {
        struct Marks;
        impl EngineText for Marks {
            fn translate(&self, _: Lang, kind: EngineKind, english: &str) -> Option<String> {
                (kind == EngineKind::Name).then(|| format!("<{english}>"))
            }
            fn message(
                &self,
                _: Lang,
                fmt: Option<&str>,
                args: &[FmtArg],
                _: &str,
            ) -> Option<String> {
                Some(format!("{} with {}", fmt?, args.len()))
            }
        }
        let m = Message {
            seq: 1,
            text: "You hit the newt.".into(),
            attr: 0,
            turn: Some(3),
            urgent: false,
            from_history: false,
            fmt: Some("You hit %s.".into()),
            args: vec![FmtArg::Str("the newt".into())],
        };
        set_engine_text(Some(Box::new(Marks)));
        assert_eq!(engine_message(&m), "You hit the newt.", "English stays");
        LANG.with(|l| l.set(Lang::Ru));
        assert_eq!(engine_message(&m), "You hit %s. with 1");
        assert_eq!(engine(EngineKind::Name, "newt"), "<newt>");
        assert_eq!(engine(EngineKind::Message, "Hello."), "Hello.");
        // a window: by default line by line, the English where nothing is
        // translated; nothing translated, as written
        assert_eq!(engine_window(&["Hello.", "newt"]), None);
        struct Lines;
        impl EngineText for Lines {
            fn translate(&self, _: Lang, kind: EngineKind, english: &str) -> Option<String> {
                (kind == EngineKind::Window && english == "a").then(|| "А".to_string())
            }
        }
        set_engine_text(Some(Box::new(Lines)));
        assert_eq!(
            engine_window(&["a", "b"]),
            Some(vec!["А".to_string(), "b".to_string()])
        );
        set_engine_text(None);
        assert_eq!(engine_message(&m), "You hit the newt.");
        LANG.with(|l| l.set(Lang::En));
    }

    #[test]
    fn codes_and_locales_name_a_language() {
        assert_eq!(Lang::from_code("ru"), Some(Lang::Ru));
        assert_eq!(Lang::from_code("ru_RU"), Some(Lang::Ru));
        assert_eq!(Lang::from_code("en-GB"), Some(Lang::En));
        assert_eq!(Lang::from_code("de"), None);
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(fluent_id("item.put_on_left"), "item-put-on-left");
    }
}
