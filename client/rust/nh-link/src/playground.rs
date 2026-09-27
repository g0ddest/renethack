use std::fs;
use std::io;
use std::path::Path;

/// The lock file that makes one client the playground's only user.
pub const PLAYGROUND_LOCK: &str = "renethack.lock";

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
        // a copy beside it, then a rename: an engine that has the old file
        // open keeps reading it whole
        let tmp = dir.join(format!("{name}.new"));
        fs::copy(&from, &tmp)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", from.display())))?;
        fs::rename(&tmp, dir.join(name))?;
    }
    for name in EMPTY_FILES {
        let path = dir.join(name);
        if !path.exists() {
            fs::File::create(path)?;
        }
    }
    Ok(())
}

/// An exclusive hold on a playground: while it lives no other client may
/// recover, list or start games there (`recover` would take a running
/// game's level files away from it).
#[derive(Debug)]
pub struct PlaygroundLock {
    _file: fs::File,
}

/// Take the playground (creating the directory); `None` if another process
/// holds it. The lock goes with the process, so a crash never leaves it.
pub fn lock_playground(dir: &Path) -> io::Result<Option<PlaygroundLock>> {
    fs::create_dir_all(dir)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(PLAYGROUND_LOCK))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(PlaygroundLock { _file: file })),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playground_has_one_holder_at_a_time() {
        let pg = tempfile::tempdir().unwrap();
        let dir = pg.path().join("playground");
        let first = lock_playground(&dir).unwrap();
        assert!(first.is_some());
        assert!(lock_playground(&dir).unwrap().is_none());
        drop(first);
        assert!(lock_playground(&dir).unwrap().is_some());
    }

    #[test]
    fn data_files_are_replaced_not_rewritten() {
        let pg = tempfile::tempdir().unwrap();
        let data = pg.path().join("data");
        let dir = pg.path().join("playground");
        fs::create_dir_all(&data).unwrap();
        for name in DATA_FILES {
            fs::write(data.join(name), "new").unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("nhdat"), "old").unwrap();
        let open = fs::File::open(dir.join("nhdat")).unwrap();
        create_playground(&data, &dir).unwrap();
        // the reader of the old file still sees the old contents
        assert_eq!(io::read_to_string(open).unwrap(), "old");
        assert_eq!(fs::read_to_string(dir.join("nhdat")).unwrap(), "new");
        assert!(!dir.join("nhdat.new").exists());
    }
}
