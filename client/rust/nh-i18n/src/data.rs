//! The catalog and the Russian translations built into the crate: the
//! client needs no files beside its extension.

use crate::catalog::Catalog;
use crate::russian::Russian;
use crate::translate::Translator;

/// `client/i18n/catalog.en.json`.
pub const CATALOG_JSON: &str = include_str!("../../../i18n/catalog.en.json");

/// `client/i18n/ru/*.toml`, by name: every file of the directory (build.rs
/// lists them).
pub const RU_FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/ru_files.rs"));

impl Translator {
    /// The built-in catalog and translations, with the built-in lexicon.
    /// About 0.3 s unoptimized: a client makes it on a thread.
    pub fn built_in() -> Result<Translator, String> {
        let catalog = Catalog::parse(CATALOG_JSON).map_err(|e| e.to_string())?;
        let files: Vec<(String, String)> = RU_FILES
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect();
        let russian = Russian::parse(&files).map_err(|e| e.to_string())?;
        Ok(Translator::new(
            catalog,
            russian,
            Box::new(crate::lexicon::Lexicon::ru()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_translation_file_is_built_in() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n/ru");
        let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".toml"))
            .collect();
        on_disk.sort();
        let mut built_in: Vec<String> = RU_FILES.iter().map(|(n, _)| n.to_string()).collect();
        built_in.sort();
        assert_eq!(
            on_disk, built_in,
            "build.rs lists every client/i18n/ru file"
        );
        let t = Translator::built_in().unwrap();
        assert_eq!(
            t.message(None, &[], "You are already here.").text,
            "Вы уже здесь."
        );
    }
}
