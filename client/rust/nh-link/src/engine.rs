use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use nh_protocol::{EngineMsg, parse_line};
use serde_json::Value;

use crate::LinkError;

/// Options every session needs: turn counter for the timeline, no intro
/// text, no tutorial prompt, no autopickup surprises.
pub const BASE_OPTIONS: &str = "time,!legacy,!tutorial,!autopickup";

/// Options the interactive client adds to character/restore options: the
/// status line carries experience points only with "showexp", and with
/// "pushweapon" wielding a new weapon makes the old one the alternate, so
/// an equip by drag and drop is predictable. The key profile adds its
/// number_pad (nh-world's `KeyProfile::engine_option`); NetHack does not
/// keep either option in the save, so a restore needs them again.
pub const CLIENT_EXTRA_OPTIONS: &str = "showexp,pushweapon";

/// How to start one engine process.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Path to the `nh-engine` executable.
    pub engine: PathBuf,
    /// Prepared playground; the engine runs with it as working directory.
    pub playground: PathBuf,
    /// NETHACKOPTIONS value (see `BASE_OPTIONS`).
    pub options: String,
    /// RENETHACK_SEED: fixed RNG seed for reproducible runs.
    pub seed: Option<u64>,
    /// RENETHACK_FIXED_TIME: fixed clock (unix seconds) for reproducible runs.
    pub fixed_time: Option<i64>,
}

impl EngineConfig {
    /// Options for a fixed character on top of `BASE_OPTIONS`.
    ///
    /// Everything lands in NETHACKOPTIONS as is, so whatever NetHack would
    /// read as syntax is refused: a ',' starts another option (the name
    /// "Hero,playmode:debug" would switch on debug mode) and a '-' makes
    /// NetHack read the rest of a name as a role or race suffix.
    pub fn character_options(
        name: &str,
        role: &str,
        race: &str,
        gender: &str,
        align: &str,
    ) -> Result<String, LinkError> {
        check_name(name)?;
        for (what, word) in [
            ("role", role),
            ("race", race),
            ("gender", gender),
            ("align", align),
        ] {
            if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphabetic()) {
                return Err(LinkError::Character(format!(
                    "{what} {word:?} is not a plain word"
                )));
            }
        }
        Ok(format!(
            "{BASE_OPTIONS},name:{name},role:{role},race:{race},gender:{gender},align:{align}"
        ))
    }

    /// `BASE_OPTIONS` plus the name only: NetHack restores the save for this
    /// name (the regularized form, "Olaf_the_Bold", finds it too).
    pub fn restore_options(name: &str) -> Result<String, LinkError> {
        check_name(name)?;
        Ok(format!("{BASE_OPTIONS},name:{name}"))
    }
}

/// NetHack keeps a name in PL_NSIZ = 32 bytes, including the terminating NUL.
const MAX_NAME_BYTES: usize = 31;

fn check_name(name: &str) -> Result<(), LinkError> {
    let bad = |why: &str| Err(LinkError::Character(format!("name {name:?} {why}")));
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return bad("must be 1 to 31 bytes long");
    }
    if name.trim() != name {
        return bad("starts or ends with a space");
    }
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '_' | '.');
    if !name.chars().all(allowed) {
        return bad("may only hold letters, digits, spaces and ' _ .");
    }
    Ok(())
}

/// One decoded line plus its exact text (hashes and replays need the text).
#[derive(Debug, Clone)]
pub struct Incoming {
    pub raw: String,
    pub msg: EngineMsg,
}

/// Result of a non-blocking read.
#[derive(Debug)]
pub enum Polled {
    Line(Incoming),
    /// Nothing ready yet.
    Empty,
    /// The engine closed its output and every line has been taken.
    Closed,
}

/// A running engine process.
pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Result<Incoming, LinkError>>,
    stderr: PathBuf,
}

impl Engine {
    pub fn spawn(cfg: &EngineConfig) -> Result<Engine, LinkError> {
        let stderr = cfg.playground.join("engine.stderr");
        let mut cmd = Command::new(&cfg.engine);
        cmd.current_dir(&cfg.playground)
            .env("NETHACKOPTIONS", &cfg.options)
            .env_remove("RENETHACK_SEED")
            .env_remove("RENETHACK_FIXED_TIME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(std::fs::File::create(&stderr)?));
        if let Some(seed) = cfg.seed {
            cmd.env("RENETHACK_SEED", seed.to_string());
        }
        if let Some(t) = cfg.fixed_time {
            cmd.env("RENETHACK_FIXED_TIME", t.to_string());
        }
        let mut child = cmd.spawn().map_err(|source| LinkError::Spawn {
            path: cfg.engine.display().to_string(),
            source,
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, rx) = mpsc::channel();
        // parse off the caller's thread: a level change is ~1700 lines
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let item = match line {
                    Ok(raw) => match parse_line(&raw) {
                        Ok(msg) => Ok(Incoming { raw, msg }),
                        Err(source) => Err(LinkError::Protocol { line: raw, source }),
                    },
                    Err(e) => Err(LinkError::Io(e)),
                };
                if tx.send(item).is_err() {
                    break;
                }
            }
        });
        Ok(Engine {
            child,
            stdin,
            lines: rx,
            stderr,
        })
    }

    /// Next message, or `None` once the engine closed its output.
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Incoming>, LinkError> {
        match self.lines.recv_timeout(timeout) {
            Ok(item) => item.map(Some),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
            Err(RecvTimeoutError::Timeout) => Err(LinkError::Timeout(timeout)),
        }
    }

    /// Lines are parsed in the reader thread; this only moves a ready value.
    /// Never blocks. A bad line comes back as `Err(LinkError::Protocol)` in
    /// its turn, after the lines before it.
    pub fn try_recv(&mut self) -> Result<Polled, LinkError> {
        match self.lines.try_recv() {
            Ok(item) => item.map(Polled::Line),
            Err(TryRecvError::Empty) => Ok(Polled::Empty),
            Err(TryRecvError::Disconnected) => Ok(Polled::Closed),
        }
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, LinkError> {
        Ok(self.child.try_wait()?)
    }

    /// Last lines of `<playground>/engine.stderr` (for error screens).
    pub fn stderr_tail(&self, max_lines: usize) -> String {
        read_tail(&self.stderr, max_lines).unwrap_or_default()
    }

    /// Answer request `id` with the `"r"` object.
    pub fn reply(&mut self, id: u64, r: &Value) -> Result<(), LinkError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| LinkError::Handshake("input already closed".into()))?;
        writeln!(stdin, "{}", nh_protocol::encode_reply(id, r))?;
        stdin.flush()?;
        Ok(())
    }

    /// Close the engine's input, as if the client had died.
    pub fn close_input(&mut self) {
        self.stdin = None;
    }

    /// Wait for the process to exit; kills it after `timeout`.
    pub fn wait(&mut self, timeout: Duration) -> Result<ExitStatus, LinkError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                self.kill();
                return Err(LinkError::Timeout(timeout));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.kill();
        }
    }
}

/// How much of a log file is read for its tail.
const TAIL_BYTES: u64 = 64 * 1024;

fn read_tail(path: &Path, max_lines: usize) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    Ok(lines[lines.len().saturating_sub(max_lines)..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(name: &str) -> Result<String, LinkError> {
        EngineConfig::character_options(name, "valkyrie", "human", "female", "neutral")
    }

    #[test]
    fn a_plain_name_becomes_the_options_string() {
        assert_eq!(
            options("Hero").unwrap(),
            "time,!legacy,!tutorial,!autopickup,name:Hero,role:valkyrie,race:human,gender:female,align:neutral"
        );
        assert!(options("Сигурд").is_ok());
        assert!(options("Olaf the Bold").is_ok());
    }

    #[test]
    fn names_cannot_smuggle_options_or_suffixes() {
        assert!(options("Hero,playmode:debug").is_err()); // another option
        assert!(options("Conan, the Barbarian").is_err()); // same, by accident
        assert!(options("Jean-Luc").is_err()); // NetHack reads "-Luc" as a role
        assert!(options("name:x").is_err());
        assert!(options("").is_err());
        assert!(options(" Hero").is_err());
        assert!(options(&"x".repeat(32)).is_err()); // PL_NSIZ is 32 with the NUL
        assert!(options("Hero\n").is_err());
    }

    #[test]
    fn role_and_friends_are_plain_words() {
        let bad = EngineConfig::character_options(
            "Hero",
            "valkyrie,playmode:debug",
            "human",
            "female",
            "neutral",
        );
        assert!(bad.is_err());
    }

    #[test]
    fn restoring_names_only_the_hero() {
        assert_eq!(
            EngineConfig::restore_options("Olaf_the_Bold").unwrap(),
            "time,!legacy,!tutorial,!autopickup,name:Olaf_the_Bold"
        );
        assert!(EngineConfig::restore_options("Hero,playmode:debug").is_err());
    }

    #[test]
    fn the_stderr_tail_is_the_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("engine.stderr");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(read_tail(&path, 2).unwrap(), "two\nthree");
        assert_eq!(read_tail(&path, 10).unwrap(), "one\ntwo\nthree");
        assert!(read_tail(&dir.path().join("missing"), 2).is_err());
    }
}
