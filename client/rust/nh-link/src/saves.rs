use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::LinkError;

/// Index of typed names in the playground: regularized name -> as typed.
const NAMES_FILE: &str = "renethack-names.json";
/// Suffixes NetHack's COMPRESS may add: gzip on Linux, compress on macOS.
const COMPRESSED: [&str; 4] = [".gz", ".Z", ".bz2", ".xz"];

/// A game saved in `<playground>/save`.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedGame {
    /// Pass to `EngineConfig::restore_options` (the regularized form).
    pub name: String,
    /// As typed (from the name index), else `name`.
    pub display_name: String,
    pub file: PathBuf,
    pub modified: SystemTime,
}

/// Newest first. Strips .gz/.Z/.bz2/.xz, skips panic saves (.e before or
/// after the suffix); the uid prefix is the file owner's uid, and "0Hero"
/// and "0Hero.gz" are one game.
pub fn list_saves(playground: &Path) -> io::Result<Vec<SavedGame>> {
    let entries = match fs::read_dir(playground.join("save")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let names = load_names(playground);
    let mut games: Vec<SavedGame> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let file_name = entry.file_name();
        let Some(name) = file_name
            .to_str()
            .filter(|_| meta.is_file())
            .and_then(|f| save_name(f, owner_uid(&meta)))
        else {
            continue;
        };
        let modified = meta.modified()?;
        if let Some(game) = games.iter_mut().find(|g| g.name == name) {
            if modified > game.modified {
                game.file = entry.path();
                game.modified = modified;
            }
            continue;
        }
        games.push(SavedGame {
            display_name: names.get(&name).cloned().unwrap_or_else(|| name.clone()),
            name,
            file: entry.path(),
            modified,
        });
    }
    games.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    Ok(games)
}

/// Remember the typed name, so the save list can show it as typed
/// ("Olaf the Bold" rather than "Olaf_the_Bold").
pub fn remember_name(playground: &Path, name: &str) -> io::Result<()> {
    let mut names = load_names(playground);
    names.insert(regularize(name), name.to_string());
    let text = serde_json::to_string_pretty(&names).map_err(io::Error::other)?;
    let tmp = playground.join(format!("{NAMES_FILE}.tmp"));
    fs::write(&tmp, text)?;
    fs::rename(tmp, playground.join(NAMES_FILE))
}

/// Would a new game with this name restore an old one? NetHack finds the
/// save by the regularized name.
pub fn save_exists(playground: &Path, name: &str) -> bool {
    let name = regularize(name);
    list_saves(playground).is_ok_and(|games| games.iter().any(|g| g.name == name))
}

/// Lock slots of interrupted games: bases like "alock" (files `<base>.0`),
/// plus any "<uid><name>.0" in case MAXPLAYERS is ever dropped. None if
/// the playground does not exist yet.
pub fn interrupted_games(playground: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(playground) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut bases = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        let file_name = entry.file_name();
        let Some(base) = file_name
            .to_str()
            .filter(|_| meta.is_file())
            .and_then(|f| f.strip_suffix(".0"))
        else {
            continue;
        };
        if is_lock_slot(base) || save_name(base, owner_uid(&meta)).is_some() {
            bases.push(base.to_string());
        }
    }
    bases.sort();
    Ok(bases)
}

/// What `recover_game` made of an interrupted game.
#[derive(Debug, Clone, PartialEq)]
pub enum Recovered {
    /// Saved under this name (for `EngineConfig::restore_options`).
    Saved(String),
    Lost,
}

/// Run `recover -d <playground> <base>`. The name comes from recover's
/// 'recovered "alock" to save/0Hero' or from the one new file in save/.
/// On failure the slot's files `<base>.*` (and any partial save) are
/// removed, so the slot is free again.
pub fn recover_game(recover: &Path, playground: &Path, base: &str) -> Result<Recovered, LinkError> {
    let before = save_files(playground);
    let out = Command::new(recover)
        .arg("-d")
        .arg(playground)
        .arg(base)
        .stdin(Stdio::null())
        .output()
        .map_err(|source| LinkError::Spawn {
            path: recover.display().to_string(),
            source,
        })?;
    let report = String::from_utf8_lossy(&out.stderr);
    let new_files: Vec<String> = save_files(playground)
        .difference(&before)
        .cloned()
        .collect();
    let prefix = format!("recovered \"{base}\" to ");
    let file = match report.lines().find_map(|l| l.strip_prefix(prefix.as_str())) {
        Some(path) => Some(playground.join(path)),
        // another recover's wording: trust a lone new file, if it said nothing else
        None if report.trim().is_empty() && new_files.len() == 1 => {
            Some(playground.join("save").join(&new_files[0]))
        }
        None => None,
    };
    let name = file.and_then(|f| {
        let meta = fs::metadata(&f).ok()?;
        save_name(f.file_name()?.to_str()?, owner_uid(&meta))
    });
    match name {
        Some(name) if out.status.success() => Ok(Recovered::Saved(name)),
        _ => {
            remove_slot(playground, base)?;
            for f in new_files {
                fs::remove_file(playground.join("save").join(f))?;
            }
            Ok(Recovered::Lost)
        }
    }
}

/// NetHack's regularize() on Unix: '.', '/' and ' ' become '_'.
fn regularize(name: &str) -> String {
    name.chars()
        .map(|c| if matches!(c, '.' | '/' | ' ') { '_' } else { c })
        .collect()
}

/// The game name in a save file name ("0Hero.gz" -> "Hero"), or `None` for
/// panic saves and other files. `uid` is the file owner's.
fn save_name(file_name: &str, uid: Option<u32>) -> Option<String> {
    let base = COMPRESSED
        .iter()
        .find_map(|s| file_name.strip_suffix(s))
        .unwrap_or(file_name);
    // regularized names hold no '.': "0Hero.e" is a panic save
    if base.contains('.') {
        return None;
    }
    let name = match uid {
        Some(uid) => base.strip_prefix(uid.to_string().as_str())?,
        None => base.trim_start_matches(|c: char| c.is_ascii_digit()),
    };
    (!name.is_empty()).then(|| name.to_string())
}

/// "alock" ... "jlock" and beyond: NetHack's slots when MAXPLAYERS is set.
fn is_lock_slot(base: &str) -> bool {
    base.len() == 5 && base.ends_with("lock") && base.as_bytes()[0].is_ascii_lowercase()
}

fn remove_slot(playground: &Path, base: &str) -> io::Result<()> {
    let prefix = format!("{base}.");
    for entry in fs::read_dir(playground)? {
        let entry = entry?;
        let level = entry
            .file_name()
            .to_str()
            .and_then(|f| f.strip_prefix(prefix.as_str()))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if level {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn save_files(playground: &Path) -> BTreeSet<String> {
    fs::read_dir(playground.join("save"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn load_names(playground: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(playground.join(NAMES_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(unix)]
fn owner_uid(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.uid())
}

#[cfg(not(unix))]
fn owner_uid(_meta: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_file_names_parse() {
        let name = |f: &str| save_name(f, Some(0));
        assert_eq!(name("0Hero").as_deref(), Some("Hero"));
        assert_eq!(name("0Hero.gz").as_deref(), Some("Hero"));
        assert_eq!(name("0Hero.Z").as_deref(), Some("Hero"));
        assert_eq!(name("0Hero.e"), None);
        assert_eq!(name("0Hero.e.gz"), None);
        assert_eq!(name("0Olaf_the_Bold.gz").as_deref(), Some("Olaf_the_Bold"));
        assert_eq!(name("0Сигурд.gz").as_deref(), Some("Сигурд"));
        // the uid is the owner's, so a name may start with digits
        assert_eq!(save_name("1000007.bz2", Some(1000)).as_deref(), Some("007"));
        assert_eq!(save_name("0Hero", Some(1000)), None);
        assert_eq!(name("0"), None);
        assert_eq!(save_name("1000Hero.xz", None).as_deref(), Some("Hero"));
    }

    #[test]
    fn regularize_is_nethacks() {
        assert_eq!(regularize("Olaf the.Bold/x"), "Olaf_the_Bold_x");
        assert_eq!(regularize("Сигурд"), "Сигурд");
    }

    #[test]
    fn lock_slots_are_letters_before_lock() {
        assert!(is_lock_slot("alock"));
        assert!(is_lock_slot("jlock"));
        assert!(!is_lock_slot("Alock"));
        assert!(!is_lock_slot("lock"));
        assert!(!is_lock_slot("perm_lock"));
    }

    #[cfg(unix)]
    fn my_uid(dir: &Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(dir).unwrap().uid()
    }

    #[cfg(unix)]
    #[test]
    fn compressed_copies_are_one_game_and_typed_names_come_back() {
        let pg = tempfile::tempdir().unwrap();
        let save = pg.path().join("save");
        fs::create_dir(&save).unwrap();
        let uid = my_uid(pg.path());
        for f in ["Hero", "Hero.gz", "Hero.e.gz", "Olaf_the_Bold.gz"] {
            fs::write(save.join(format!("{uid}{f}")), "x").unwrap();
        }
        fs::write(save.join("README"), "x").unwrap();
        remember_name(pg.path(), "Olaf the Bold").unwrap();
        let mut games = list_saves(pg.path()).unwrap();
        games.sort_by(|a, b| a.name.cmp(&b.name));
        let shown: Vec<(&str, &str)> = games
            .iter()
            .map(|g| (g.name.as_str(), g.display_name.as_str()))
            .collect();
        assert_eq!(
            shown,
            [("Hero", "Hero"), ("Olaf_the_Bold", "Olaf the Bold")]
        );
        assert!(save_exists(pg.path(), "Olaf the Bold"));
        assert!(save_exists(pg.path(), "Olaf.the.Bold"));
        assert!(!save_exists(pg.path(), "Olaf"));
        assert!(list_saves(&pg.path().join("nowhere")).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_games_are_slots_with_a_level_0() {
        let pg = tempfile::tempdir().unwrap();
        let uid = my_uid(pg.path());
        for f in [
            "alock.0".to_string(),
            "alock.1".to_string(),
            "block.1".to_string(),
            "clock.0".to_string(),
            format!("{uid}Hero.0"),
            "nhdat".to_string(),
        ] {
            fs::write(pg.path().join(f), "x").unwrap();
        }
        let mut expected = [
            "alock".to_string(),
            "clock".to_string(),
            format!("{uid}Hero"),
        ];
        expected.sort();
        assert_eq!(interrupted_games(pg.path()).unwrap(), expected);
        assert!(
            interrupted_games(&pg.path().join("nowhere"))
                .unwrap()
                .is_empty()
        );
        remove_slot(pg.path(), "alock").unwrap();
        assert!(!pg.path().join("alock.0").exists());
        assert!(!pg.path().join("alock.1").exists());
        assert!(pg.path().join("block.1").exists());
    }
}
