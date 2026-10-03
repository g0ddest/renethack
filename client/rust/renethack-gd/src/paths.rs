//! Where the engine and the game files live.

use std::path::{Path, PathBuf};

use godot::classes::ProjectSettings;
use godot::prelude::*;

/// Project setting with the engine build directory.
const ENGINE_DIR_SETTING: &str = "renethack/engine_dir";
const DEFAULT_ENGINE_DIR: &str = "res://../../engine/build";
const DEFAULT_PLAYGROUND: &str = "user://playground";

#[derive(Debug, Clone)]
pub struct Paths {
    /// nh-engine, recover and data/.
    pub engine_dir: PathBuf,
    /// Saves, lock files, scores: the engine's working directory.
    pub playground: PathBuf,
}

impl Paths {
    /// RENETHACK_ENGINE_DIR, else the project setting; the playground from
    /// `--playground=`, else user://playground.
    pub fn resolve(playground: Option<&str>) -> Paths {
        let settings = ProjectSettings::singleton();
        let setting = settings
            .has_setting(ENGINE_DIR_SETTING)
            .then(|| settings.get_setting(ENGINE_DIR_SETTING).to_string())
            .filter(|s| !s.is_empty());
        let env = std::env::var_os("RENETHACK_ENGINE_DIR").map(PathBuf::from);
        let engine_dir = env.unwrap_or_else(|| {
            let res = setting.unwrap_or_else(|| DEFAULT_ENGINE_DIR.to_string());
            globalize(&res)
        });
        let playground = match playground {
            Some(p) => PathBuf::from(p),
            None => globalize(DEFAULT_PLAYGROUND),
        };
        Paths {
            engine_dir,
            playground,
        }
    }

    pub fn engine(&self) -> PathBuf {
        self.engine_dir.join("nh-engine")
    }

    pub fn data(&self) -> PathBuf {
        self.engine_dir.join("data")
    }

    pub fn recover(&self) -> PathBuf {
        self.engine_dir.join("recover")
    }

    /// The local achievements store, beside the playground: in the user
    /// data directory for a player, in a self-test's own directory for a
    /// test.
    pub fn achievements(&self) -> PathBuf {
        match self.playground.parent() {
            Some(dir) => dir.join("achievements.json"),
            None => self.playground.join("achievements.json"),
        }
    }

    /// Why the engine cannot run, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let missing = |p: &Path| format!("{} is missing", p.display());
        for p in [self.engine(), self.data().join("nhdat")] {
            if !p.is_file() {
                return Err(missing(&p));
            }
        }
        Ok(())
    }
}

fn globalize(path: &str) -> PathBuf {
    let global = ProjectSettings::singleton()
        .globalize_path(path)
        .to_string();
    normalize(Path::new(&global))
}

/// Drop "." and fold ".." lexically: res://../../x globalizes to <project>/../../x.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_fold_parent_components() {
        assert_eq!(
            normalize(Path::new("/a/client/godot/../../engine/build")),
            PathBuf::from("/a/engine/build")
        );
        assert_eq!(normalize(Path::new("/a/./b")), PathBuf::from("/a/b"));
    }
}
