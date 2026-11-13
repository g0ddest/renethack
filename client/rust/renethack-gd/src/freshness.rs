//! Whether a debug library run from a checkout is what the checkout builds
//! now, and whether Godot's import holds the art on disk: on 2026-11-12 the
//! main checkout's library was three weeks older than its sources and its
//! import behind 359 files, and the game it ran showed none of that work.
//!
//! build.rs lists what the library is built from with each file's stamp
//! (source_stamp.rs); at start-up the game stamps the same files again, and
//! looks for art whose import is older than the file. One line says it
//! (`renethack: build: …`). Stale, a self-test fails at its first step
//! (`RENETHACK_ALLOW_STALE=1` lets it run) and a game shows a red line over
//! everything. Only in a debug library, and only where the sources are
//! beside the project: a release build (every package) has none of this.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime};

use godot::classes::{FileAccess, ProjectSettings};
use godot::prelude::*;

use crate::source_stamp::{file_stamp, written};

/// What build.rs stamped: "<stamp in hex> <path>" a line, the paths
/// relative to client/rust.
const BUILT_FROM: &str = include_str!(concat!(env!("OUT_DIR"), "/sources.txt"));

/// The files Godot imports: their importer's settings sit beside them.
const IMPORTABLE: [&str; 20] = [
    "png", "jpg", "jpeg", "webp", "svg", "tga", "bmp", "exr", "hdr", "glb", "gltf", "obj", "fbx",
    "ogg", "wav", "mp3", "ttf", "otf", "woff", "woff2",
];

/// The project's folders `make import` imports (the Makefile's
/// IMPORT_INPUTS).
const IMPORTED_DIRS: [&str; 4] = ["art", "audio", "fonts", "ui"];

/// The names shown of a stale kind at most (the count says the rest).
const NAMED: usize = 3;

#[derive(Debug, Default, PartialEq)]
pub struct Freshness {
    /// The sources changed since the library was built (relative to
    /// client/rust).
    pub sources: Vec<String>,
    /// The art changed since Godot imported it, or never imported (relative
    /// to the project).
    pub import: Vec<String>,
}

impl Freshness {
    /// What is stale and how to mend it (None when nothing is).
    pub fn reason(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.sources.is_empty() {
            parts.push(format!(
                "the library is older than its sources ({}): make client",
                some(&self.sources)
            ));
        }
        if !self.import.is_empty() {
            parts.push(format!(
                "the import is older than the art ({}): make import",
                some(&self.import)
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// A few of `names`, and how many more.
fn some(names: &[String]) -> String {
    let shown = names
        .iter()
        .take(NAMED)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match names.len().saturating_sub(NAMED) {
        0 => shown,
        more => format!("{shown} and {more} more"),
    }
}

/// The sources listed in `built` (build.rs's lines: stamp, time, path) whose
/// bytes differ now under `rust` (client/rust). Only a file written since is
/// read again (a debug build stamps 7 MB slowly); touched, not changed, it
/// is current.
pub fn changed_sources(rust: &Path, built: &str) -> Vec<String> {
    let mut changed = Vec::new();
    for line in built.lines() {
        let mut words = line.splitn(3, ' ');
        let (Some(stamp), Some(time), Some(path)) = (words.next(), words.next(), words.next())
        else {
            continue;
        };
        let file = rust.join(path);
        if u128::from_str_radix(time, 16).ok() == Some(written(&file)) {
            continue;
        }
        if u64::from_str_radix(stamp, 16).ok() != Some(file_stamp(&file)) {
            changed.push(path.to_string());
        }
    }
    changed
}

/// The art under `project` whose import is older than the file: never
/// imported (no importer's settings, or its result gone), or changed since
/// both the last `make import` and the import's own record, its bytes no
/// longer the ones imported (`md5`: Godot's own sum of a file, absolute
/// path). A file touched but not changed (a checkout) is current.
pub fn stale_imports(project: &Path, md5: &dyn Fn(&Path) -> String) -> Vec<String> {
    let stamp = modified(&project.join(".godot/renethack-import.stamp"));
    let mut stale = Vec::new();
    for dir in IMPORTED_DIRS {
        let mut files = Vec::new();
        assets(&project.join(dir), &mut files);
        for file in files {
            if import_stale(project, &file, stamp, md5) {
                let rel = file.strip_prefix(project).unwrap_or(&file);
                stale.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    stale.sort();
    stale
}

fn import_stale(
    project: &Path,
    file: &Path,
    stamp: Option<SystemTime>,
    md5: &dyn Fn(&Path) -> String,
) -> bool {
    let settings = PathBuf::from(format!("{}.import", file.display()));
    let Ok(text) = std::fs::read_to_string(&settings) else {
        return true;
    };
    let changed = modified(file);
    if changed.is_some() && changed <= stamp {
        return false;
    }
    // kept as it is (no result), or a kind with no import record
    let Some(record) = record_of(project, &text) else {
        return false;
    };
    let Ok(sums) = std::fs::read_to_string(&record) else {
        return true;
    };
    if changed.is_some() && changed <= modified(&record) {
        return false;
    }
    let imported = sums
        .lines()
        .find_map(|l| l.strip_prefix("source_md5=\""))
        .map(|s| s.trim_end_matches('"'));
    imported != Some(md5(file).as_str())
}

/// The import's record of a file (`.godot/imported/<name>-<hash>.md5`), from
/// its importer's settings: the first of its results, named for the same
/// file and hash.
fn record_of(project: &Path, settings: &str) -> Option<PathBuf> {
    let line = settings
        .lines()
        .find_map(|l| l.strip_prefix("dest_files=[\""))?;
    let dest = line.split('"').next()?;
    let name = dest.strip_prefix("res://")?;
    let dash = name.rfind('-')?;
    let hash = name.get(dash + 1..dash + 33)?;
    Some(project.join(format!("{}-{hash}.md5", &name[..dash])))
}

/// The files Godot imports under `dir`, not under a folder it ignores.
fn assets(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join(".gdignore").exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            assets(&path, out);
        } else if path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| IMPORTABLE.contains(&x.to_ascii_lowercase().as_str()))
        {
            out.push(path);
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

static FOUND: OnceLock<Option<Freshness>> = OnceLock::new();

/// At start-up: stamps the sources and looks over the import, once, and
/// says so in a line. None when the project is not in a checkout (its
/// sources not beside it).
pub fn at_start() -> Option<&'static Freshness> {
    FOUND
        .get_or_init(|| {
            let t0 = Instant::now();
            let project = PathBuf::from(
                ProjectSettings::singleton()
                    .globalize_path("res://")
                    .to_string(),
            );
            let rust = project.join("../rust");
            if !rust.join("Cargo.lock").is_file() || BUILT_FROM.is_empty() {
                godot_print!(
                    "renethack: build: a debug library, its sources not beside the project"
                );
                return None;
            }
            let found = Freshness {
                sources: changed_sources(&rust, BUILT_FROM),
                import: stale_imports(&project, &|f| {
                    FileAccess::get_md5(f.to_string_lossy().as_ref()).to_string()
                }),
            };
            let ms = t0.elapsed().as_millis();
            match found.reason() {
                None => godot_print!(
                    "renethack: build: the library and the import current ({} sources, {ms} ms)",
                    BUILT_FROM.lines().count()
                ),
                Some(why) => godot_print!("renethack: build: STALE ({ms} ms): {why}"),
            }
            Some(found)
        })
        .as_ref()
}

/// A self-test's first step: Err while the library or the import is stale
/// (unless RENETHACK_ALLOW_STALE=1).
pub fn verdict() -> Result<(), String> {
    if std::env::var("RENETHACK_ALLOW_STALE").as_deref() == Ok("1") {
        return Ok(());
    }
    match at_start().and_then(Freshness::reason) {
        Some(why) => Err(format!("stale build: {why}")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    /// A folder of its own for a test, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("renethack-freshness-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    fn line_of<'a>(path: &str) -> &'a str {
        BUILT_FROM
            .lines()
            .find(|l| l.splitn(3, ' ').nth(2) == Some(path))
            .unwrap_or_else(|| panic!("{path} is not listed"))
    }

    #[test]
    fn the_list_holds_what_the_library_is_built_from() {
        for path in [
            "Cargo.lock",
            "Cargo.toml",
            "renethack-gd/Cargo.toml",
            "renethack-gd/build.rs",
            "renethack-gd/src/lib.rs",
            "renethack-gd/src/freshness.rs",
            "nh-world/src/lib.rs",
            "nh-protocol/src/lib.rs",
            "nh-i18n/build.rs",
            "../godot/art/variants.txt",
            "../i18n/ru.ftl",
        ] {
            line_of(path);
        }
        assert!(
            BUILT_FROM
                .lines()
                .any(|l| l.contains(" ../i18n/ru/") && l.ends_with(".toml")),
            "nh-i18n's translation files are listed"
        );
        // nh-cli is the workspace's, not the library's
        assert!(!BUILT_FROM.contains(" nh-cli/"));
        // and build.rs stamped them as the game does: all current now
        assert_eq!(
            changed_sources(&workspace(), BUILT_FROM),
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_edited_source_makes_the_library_stale() {
        let path = "renethack-gd/src/freshness.rs";
        let line = line_of(path);
        let rust = scratch("sources").join("rust");
        let copy = rust.join(path);
        fs::create_dir_all(copy.parent().unwrap()).unwrap();
        fs::copy(workspace().join(path), &copy).unwrap();
        assert!(
            changed_sources(&rust, line).is_empty(),
            "the copy is what was built"
        );
        // touched, not changed (a checkout): still current
        let later = SystemTime::now() + Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&copy)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(changed_sources(&rust, line).is_empty());
        // one byte more: stale, and named
        let mut text = fs::read_to_string(&copy).unwrap();
        text.push('\n');
        fs::write(&copy, text).unwrap();
        assert_eq!(changed_sources(&rust, line), vec![path.to_string()]);
        let found = Freshness {
            sources: changed_sources(&rust, line),
            import: Vec::new(),
        };
        let why = found.reason().expect("stale");
        assert!(why.contains("make client") && why.contains(path), "{why}");
        // gone: stale too
        fs::remove_file(&copy).unwrap();
        assert_eq!(changed_sources(&rust, line), vec![path.to_string()]);
        let _ = fs::remove_dir_all(rust.parent().unwrap());
    }

    /// A project with one imported picture, its import's record made
    /// `record_age` seconds after the picture.
    fn project_with_picture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let project = scratch(name);
        let hash = "0123456789abcdef0123456789abcdef";
        let art = project.join("art/icons");
        fs::create_dir_all(&art).unwrap();
        fs::create_dir_all(project.join(".godot/imported")).unwrap();
        let picture = art.join("a.png");
        fs::write(&picture, b"picture").unwrap();
        fs::write(
            art.join("a.png.import"),
            format!(
                "[remap]\n\nimporter=\"texture\"\n\n[deps]\n\nsource_file=\"res://art/icons/a.png\"\n\
                 dest_files=[\"res://.godot/imported/a.png-{hash}.ctex\"]\n"
            ),
        )
        .unwrap();
        let record = project.join(format!(".godot/imported/a.png-{hash}.md5"));
        fs::write(&record, "source_md5=\"sum of picture\"\ndest_md5=\"x\"\n").unwrap();
        (project, picture, record)
    }

    fn set_age(path: &Path, secs_ago: u64) {
        let t = SystemTime::now() - Duration::from_secs(secs_ago);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    /// Godot's sum, as the test's pictures have it.
    fn sum(path: &Path) -> String {
        format!("sum of {}", fs::read_to_string(path).unwrap())
    }

    #[test]
    fn art_changed_since_its_import_is_stale() {
        let (project, picture, record) = project_with_picture("import");
        set_age(&picture, 100);
        set_age(&record, 50);
        assert!(
            stale_imports(&project, &sum).is_empty(),
            "imported after it changed"
        );
        // changed after the import
        fs::write(&picture, b"another picture").unwrap();
        assert_eq!(
            stale_imports(&project, &sum),
            vec!["art/icons/a.png".to_string()]
        );
        // ...but `make import` ran since: current
        fs::write(project.join(".godot/renethack-import.stamp"), b"").unwrap();
        set_age(&picture, 10);
        assert!(stale_imports(&project, &sum).is_empty());
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn art_touched_but_the_same_is_current() {
        let (project, picture, record) = project_with_picture("touched");
        set_age(&record, 100);
        set_age(&picture, 10);
        assert!(
            stale_imports(&project, &sum).is_empty(),
            "the same bytes as imported"
        );
        let _ = fs::remove_dir_all(&project);
    }

    #[test]
    fn art_never_imported_is_stale_and_ignored_folders_are_not() {
        let (project, _, record) = project_with_picture("new");
        fs::write(project.join("art/icons/b.ogg"), b"sound").unwrap();
        fs::remove_file(&record).unwrap();
        assert_eq!(
            stale_imports(&project, &sum),
            vec!["art/icons/a.png".to_string(), "art/icons/b.ogg".to_string()]
        );
        // a folder Godot ignores, and files it does not import (manifest.json)
        let ignored = project.join("art/sources");
        fs::create_dir_all(&ignored).unwrap();
        fs::write(ignored.join(".gdignore"), b"").unwrap();
        fs::write(ignored.join("c.svg"), b"<svg/>").unwrap();
        fs::write(project.join("art/manifest.json"), b"{}").unwrap();
        assert_eq!(stale_imports(&project, &sum).len(), 2);
        let _ = fs::remove_dir_all(&project);
    }
}
