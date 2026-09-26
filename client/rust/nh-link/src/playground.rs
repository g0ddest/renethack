use std::fs;
use std::io;
use std::path::Path;

/// Files the engine reads from its playground (copied from the data dir).
pub const DATA_FILES: [&str; 4] = ["nhdat", "license", "symbols", "sysconf"];
/// Files the engine appends to; they must exist.
const EMPTY_FILES: [&str; 5] = ["perm", "record", "logfile", "xlogfile", "livelog"];

/// Prepare `dir` as a NetHack playground: data files, empty score/log
/// files and a `save/` directory. Existing contents are kept.
pub fn create_playground(data_dir: &Path, dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir.join("save"))?;
    for name in DATA_FILES {
        let from = data_dir.join(name);
        fs::copy(&from, dir.join(name))
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", from.display())))?;
    }
    for name in EMPTY_FILES {
        let path = dir.join(name);
        if !path.exists() {
            fs::File::create(path)?;
        }
    }
    Ok(())
}
