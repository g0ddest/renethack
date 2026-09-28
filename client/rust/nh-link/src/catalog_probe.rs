use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use nh_protocol::{Catalog, EngineMsg, Hello, Reply, Request};

use crate::handshake::Handshake;
use crate::{BASE_OPTIONS, Engine, EngineConfig, LinkError, create_playground};

/// How long the probe waits for the engine before killing it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Engine with `BASE_OPTIONS` only (no name): read hello and catalog, answer
/// the askname request with ESC, wait for bye and exit (kill only after a
/// 10 s timeout). Runs in a temporary playground made from `data_dir` (the
/// data dir or any playground: only its data files are read).
pub fn fetch_catalog(engine: &Path, data_dir: &Path) -> Result<(Hello, Catalog), LinkError> {
    let playground = tempfile::tempdir()?;
    create_probe_playground(data_dir, playground.path())?;
    probe(engine, playground.path())
}

/// A playground where the engine asks for a name before anything else.
/// Without a name option NetHack's whoami() takes $USER, $LOGNAME or
/// getlogin(), then takes a lock slot and restores that user's save;
/// GENERICUSERS=* makes plnamesuffix() drop that name and call askname.
fn create_probe_playground(data_dir: &Path, dir: &Path) -> io::Result<()> {
    create_playground(data_dir, dir)?;
    let mut sysconf = OpenOptions::new().append(true).open(dir.join("sysconf"))?;
    sysconf.write_all(b"\nGENERICUSERS=*\n")
}

/// Answering askname with ESC ends the game before a lock slot or a level
/// exists, so the playground is left as it was (apart from engine.stderr).
fn probe(engine: &Path, playground: &Path) -> Result<(Hello, Catalog), LinkError> {
    let mut proc = Engine::spawn(&EngineConfig {
        engine: engine.to_path_buf(),
        playground: playground.to_path_buf(),
        options: BASE_OPTIONS.to_string(),
        seed: None,
        fixed_time: None,
    })?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let mut handshake = Handshake::new();
    let (mut hello, mut catalog, mut said_bye) = (None, None, false);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(inc) = proc.recv(left).inspect_err(|_| proc.kill())? else {
            break;
        };
        handshake.check(&inc.msg)?;
        match inc.msg {
            EngineMsg::Hello(h) => hello = Some(h),
            EngineMsg::Catalog(c) => catalog = Some(*c),
            EngineMsg::Req {
                id,
                req: Request::Askname,
            } => proc.reply(id, &Reply::Text("\u{1b}".into()).to_value())?,
            EngineMsg::Req { req, .. } => {
                return Err(LinkError::Handshake(format!(
                    "engine asked {} before a name",
                    req.name()
                )));
            }
            EngineMsg::Error { msg } => return Err(LinkError::Handshake(msg)),
            EngineMsg::Bye => said_bye = true,
            EngineMsg::Win(_) => {}
        }
    }
    let status = proc.wait(deadline.saturating_duration_since(Instant::now()))?;
    match (hello, catalog) {
        (Some(h), Some(c)) if said_bye && status.success() => Ok((h, c)),
        (Some(_), Some(_)) => Err(LinkError::Handshake(format!(
            "engine did not exit cleanly after the catalog ({status})"
        ))),
        _ => Err(LinkError::Handshake(
            "engine exited before sending the catalog".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn engine_dir() -> PathBuf {
        std::env::var_os("RENETHACK_ENGINE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/build"))
    }

    #[test]
    fn the_probe_leaves_no_game_behind() {
        let dir = engine_dir();
        let pg = tempfile::tempdir().unwrap();
        create_probe_playground(&dir.join("data"), pg.path()).unwrap();
        let sysconf = fs::read_to_string(pg.path().join("sysconf")).unwrap();
        assert!(sysconf.ends_with("\nGENERICUSERS=*\n"), "{sysconf}");
        // Ok means askname came first and the engine exited by itself with code 0
        let (_, catalog) = probe(&dir.join("nh-engine"), pg.path()).unwrap();
        assert_eq!(catalog.roles.len(), 13);
        let mut left: Vec<String> = fs::read_dir(pg.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|f| f.contains("lock") || f.ends_with(".0"))
            .collect();
        left.extend(
            fs::read_dir(pg.path().join("save"))
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap()),
        );
        assert!(left.is_empty(), "{left:?}");
    }
}
