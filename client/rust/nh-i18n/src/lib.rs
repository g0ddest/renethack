//! The Russian translation of what the engine prints. The engine stays
//! English; the client looks every shown string up in the catalog of the
//! engine's own formats (`client/i18n/catalog.en.json`, made by
//! `tools/i18n/extract.py`) and renders its Russian template, declining
//! the names in it. No UI code here.

mod grammar;
mod phrase;

pub use grammar::*;
pub use phrase::*;
