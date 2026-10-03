//! The Russian translation of what the engine prints. The engine stays
//! English; the client looks every shown string up in the catalog of the
//! engine's own formats (`client/i18n/catalog.en.json`, made by
//! `tools/i18n/extract.py`) and renders its Russian template, declining
//! the names in it. No UI code here.
//!
//! ```ignore
//! let catalog = Catalog::parse(&std::fs::read_to_string(CATALOG_PATH)?)?;
//! let russian = Russian::load_dir(Path::new(RU_DIR))?;
//! let mut tr = Translator::new(catalog, russian, Box::new(lexicon));
//! tr.set_hero(Gender::Fem);
//! // a message, with the format and arguments P7 sent when it did
//! let out = tr.message(Some("You hit %s."), &[Arg::Str("the newt".into())], "You hit the newt.");
//! assert_eq!(out.text, "Вы бьёте тритона.");
//! // a question, a menu item, a heading; a text window's lines joined
//! tr.text("What do you want to eat? [fg or ?*]");
//! tr.window("Things that are here:\na newt corpse");
//! ```
//!
//! [`Output::status`] says whether the text is Russian, partly (a name the
//! lexicon does not know), not yet (a template without a translation) or
//! unknown to the catalog; the English text is the fallback. The Russian
//! template syntax is in [`template`](RuTemplate); [`lint`] checks the
//! translations; `src/bin/i18n-coverage.rs` measures a soak's coverage.

mod catalog;
mod format;
mod grammar;
mod lint;
mod phrase;
mod russian;
mod template;
mod translate;

pub use catalog::*;
pub use format::*;
pub use grammar::*;
pub use lint::*;
pub use phrase::*;
pub use russian::*;
pub use template::*;
pub use translate::*;
