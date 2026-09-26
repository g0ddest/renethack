use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use nh_protocol::{EngineMsg, parse_line};
use serde_json::Value;

use crate::LinkError;

/// Options every session needs: turn counter for the timeline, no intro
/// text, no tutorial prompt, no autopickup surprises.
pub const BASE_OPTIONS: &str = "time,!legacy,!tutorial,!autopickup";

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
    pub fn character_options(
        name: &str,
        role: &str,
        race: &str,
        gender: &str,
        align: &str,
    ) -> String {
        format!("{BASE_OPTIONS},name:{name},role:{role},race:{race},gender:{gender},align:{align}")
    }
}

/// One decoded line plus its exact text (hashes and replays need the text).
#[derive(Debug, Clone)]
pub struct Incoming {
    pub raw: String,
    pub msg: EngineMsg,
}

/// A running engine process.
pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<std::io::Result<String>>,
}

impl Engine {
    pub fn spawn(cfg: &EngineConfig) -> Result<Engine, LinkError> {
        let mut cmd = Command::new(&cfg.engine);
        cmd.current_dir(&cfg.playground)
            .env("NETHACKOPTIONS", &cfg.options)
            .env_remove("RENETHACK_SEED")
            .env_remove("RENETHACK_FIXED_TIME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(std::fs::File::create(
                cfg.playground.join("engine.stderr"),
            )?));
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
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Engine {
            child,
            stdin,
            lines: rx,
        })
    }

    /// Next message, or `None` once the engine closed its output.
    pub fn recv(&mut self, timeout: Duration) -> Result<Option<Incoming>, LinkError> {
        match self.lines.recv_timeout(timeout) {
            Ok(Ok(raw)) => {
                let msg = parse_line(&raw).map_err(|source| LinkError::Protocol {
                    line: raw.clone(),
                    source,
                })?;
                Ok(Some(Incoming { raw, msg }))
            }
            Ok(Err(e)) => Err(LinkError::Io(e)),
            Err(RecvTimeoutError::Disconnected) => Ok(None),
            Err(RecvTimeoutError::Timeout) => Err(LinkError::Timeout(timeout)),
        }
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
