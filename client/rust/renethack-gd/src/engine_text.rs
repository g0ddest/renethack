//! The engine's texts in Russian: nh-i18n's translator behind the
//! interface's hook (`i18n::EngineText`). The catalog, the translations and
//! the lexicon are built into the extension; they load on a thread when
//! the game starts, and the English shows until they are ready. A text
//! the translator does not know, or knows without a translation yet,
//! stays English.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::thread::JoinHandle;

use godot::prelude::*;
use nh_i18n::lexicon::Lexicon;
use nh_i18n::{Arg, Case, Gender, Number, Output, Status, Translator};
use nh_world::FmtArg;

use crate::i18n::{EngineKind, EngineText, Lang};

/// Translations kept before the cache starts again (a long game shows
/// some thousands of different texts).
const CACHE_SIZE: usize = 20_000;

/// A text the translator was asked for: (kind, English, format).
type Asked = (u8, String, Option<String>);

pub struct EngineTranslator {
    loading: RefCell<Option<JoinHandle<Result<Translator, String>>>>,
    translator: RefCell<Option<Translator>>,
    /// what shows for each text asked
    cache: RefCell<HashMap<Asked, Option<String>>>,
    hero: Cell<Gender>,
}

impl EngineTranslator {
    /// Starts loading the built-in translator on a thread.
    pub fn new() -> EngineTranslator {
        let loading = std::thread::Builder::new()
            .name("nh-i18n".into())
            .spawn(Translator::built_in)
            .map_err(|e| godot_warn!("the engine's translator does not load: {e}"))
            .ok();
        EngineTranslator {
            loading: RefCell::new(loading),
            translator: RefCell::new(None),
            cache: RefCell::new(HashMap::new()),
            hero: Cell::new(Gender::Masc),
        }
    }

    /// The hero's gender, for the words of a message that agree with it.
    pub fn set_hero_female(&self, female: bool) {
        let gender = if female { Gender::Fem } else { Gender::Masc };
        if self.hero.replace(gender) != gender {
            if let Some(t) = self.translator.borrow_mut().as_mut() {
                t.set_hero(gender);
            }
            self.cache.borrow_mut().clear();
        }
    }

    /// The translator once it is loaded (a failed load is reported once,
    /// and the English shows from then on).
    fn ready(&self) -> bool {
        if self.translator.borrow().is_some() {
            return true;
        }
        let finished = self
            .loading
            .borrow()
            .as_ref()
            .is_some_and(JoinHandle::is_finished);
        if !finished {
            return false;
        }
        let Some(handle) = self.loading.borrow_mut().take() else {
            return false;
        };
        match handle.join() {
            Ok(Ok(mut t)) => {
                t.set_hero(self.hero.get());
                *self.translator.borrow_mut() = Some(t);
                true
            }
            Ok(Err(e)) => {
                godot_warn!("the engine's translator does not load: {e}");
                false
            }
            Err(_) => {
                godot_warn!("the engine's translator panicked while loading");
                false
            }
        }
    }

    /// What shows for `english`, through the cache.
    fn cached(
        &self,
        kind: EngineKind,
        english: &str,
        fmt: Option<&str>,
        translate: impl FnOnce(&Translator) -> Output,
    ) -> Option<String> {
        if !self.ready() {
            return None;
        }
        let key = (kind as u8, english.to_string(), fmt.map(str::to_string));
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let shown = self
            .translator
            .borrow()
            .as_ref()
            .and_then(|t| shown(translate(t)));
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= CACHE_SIZE {
            cache.clear();
        }
        cache.insert(key, shown.clone());
        shown
    }
}

impl Default for EngineTranslator {
    fn default() -> EngineTranslator {
        EngineTranslator::new()
    }
}

/// The Russian when there is some: a template translated (a name in it
/// may stay English: a pet's, an engraving's text).
fn shown(out: Output) -> Option<String> {
    match out.status {
        Status::Translated | Status::Partial => Some(out.text),
        Status::Untranslated | Status::Unknown => None,
    }
}

fn arg(a: &FmtArg) -> Arg {
    match a {
        FmtArg::Str(s) => Arg::Str(s.clone()),
        FmtArg::Int(n) => Arg::Int(*n),
        FmtArg::Num(x) => Arg::Num(*x),
        FmtArg::Null => Arg::Null,
    }
}

impl EngineText for EngineTranslator {
    fn translate(&self, lang: Lang, kind: EngineKind, english: &str) -> Option<String> {
        if lang != Lang::Ru || english.trim().is_empty() {
            return None;
        }
        self.cached(kind, english, None, |t| match kind {
            EngineKind::Message => t.message(None, &[], english),
            EngineKind::Window => t.window(english),
            EngineKind::Prompt | EngineKind::Menu => t.text(english),
            EngineKind::Name | EngineKind::Status => t.name(english),
        })
    }

    fn message(
        &self,
        lang: Lang,
        fmt: Option<&str>,
        args: &[FmtArg],
        english: &str,
    ) -> Option<String> {
        if lang != Lang::Ru || english.trim().is_empty() {
            return None;
        }
        self.cached(EngineKind::Message, english, fmt, |t| {
            let args: Vec<Arg> = args.iter().map(arg).collect();
            t.message(fmt, &args, english)
        })
    }

    fn window(&self, lang: Lang, lines: &[&str]) -> Option<Vec<String>> {
        let whole = lines.join("\n");
        if lang != Lang::Ru || whole.trim().is_empty() {
            return None;
        }
        // the translator leaves out the blank lines around a text: they
        // stay, so the lines keep their places (and attributes)
        let lead = lines.iter().take_while(|l| l.trim().is_empty()).count();
        let trail = lines[lead..]
            .iter()
            .rev()
            .take_while(|l| l.trim().is_empty())
            .count();
        self.cached(EngineKind::Window, &whole, None, |t| t.window(&whole))
            .map(|text| {
                let mut out = vec![String::new(); lead];
                out.extend(text.split('\n').map(str::to_string));
                out.extend(std::iter::repeat_n(String::new(), trail));
                out
            })
    }

    fn set_hero_female(&self, female: bool) {
        EngineTranslator::set_hero_female(self, female);
    }

    fn hero_word(&self, lang: Lang, english: &str) -> Option<String> {
        if lang != Lang::Ru {
            return None;
        }
        // an alignment (or another adjective) in the hero's gender
        let key = english.trim().to_lowercase();
        let lex = Lexicon::ru();
        let adjective = lex
            .get("alignment", &key)
            .and_then(|e| e.adjective())
            .or_else(|| lex.adjective(&key));
        match adjective {
            Some(a) => {
                let word = a.form(self.hero.get(), Number::Sing, true, Case::Nom);
                let mut c = word.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect::<String>())
            }
            None => self.translate(lang, EngineKind::Status, english),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> EngineTranslator {
        let t = EngineTranslator::new();
        let start = std::time::Instant::now();
        while !t.ready() {
            assert!(
                start.elapsed().as_secs() < 30,
                "the translator never loaded"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        t
    }

    #[test]
    fn an_alignment_agrees_with_the_hero() {
        let t = loaded();
        t.set_hero_female(true);
        assert_eq!(
            t.hero_word(Lang::Ru, "Neutral").as_deref(),
            Some("Нейтральная")
        );
        t.set_hero_female(false);
        assert_eq!(
            t.hero_word(Lang::Ru, "Lawful").as_deref(),
            Some("Законопослушный")
        );
        assert_eq!(t.hero_word(Lang::En, "Chaotic"), None);
    }

    #[test]
    fn russian_through_the_hook_english_otherwise() {
        let t = loaded();
        let ru = |kind, text: &str| t.translate(Lang::Ru, kind, text);
        assert_eq!(
            ru(EngineKind::Message, "You are already here."),
            Some("Вы уже здесь.".into())
        );
        let args = [FmtArg::Str("the newt".into())];
        let hit = t.message(Lang::Ru, Some("You hit %s."), &args, "You hit the newt.");
        assert_eq!(hit.as_deref(), Some("Вы бьёте тритона."));
        // a template without a translation yet, a text the catalog does not
        // know, another language: the English shows
        let untranslated = {
            let inner = t.translator.borrow();
            let tr = inner.as_ref().expect("the translator");
            tr.catalog()
                .templates()
                .iter()
                .filter(|x| x.arity() == 0 && tr.russian().get(&x.id).is_none())
                .find(|x| tr.text(&x.fmt).template.as_deref() == Some(x.id.as_str()))
                .map(|x| x.fmt.clone())
                .expect("a template without Russian")
        };
        assert_eq!(ru(EngineKind::Message, &untranslated), None);
        assert_eq!(ru(EngineKind::Window, "Xyzzy plugh"), None);
        assert_eq!(
            t.translate(Lang::En, EngineKind::Message, "Never mind."),
            None
        );
        // a name alone
        assert_eq!(ru(EngineKind::Name, "newt"), Some("тритон".into()));
        // twice: from the cache
        assert_eq!(
            ru(EngineKind::Message, "You are already here."),
            Some("Вы уже здесь.".into())
        );
    }
}
