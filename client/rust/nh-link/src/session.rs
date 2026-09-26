use std::process::ExitStatus;
use std::time::Duration;

use nh_protocol::{Catalog, EngineMsg, Hello, PROTOCOL_VERSION, Request, WinCall, fnv1a64};
use serde_json::Value;

use crate::{Engine, LinkError, RecordedReply};

/// Decides how to answer each engine request.
pub trait Responder {
    /// Return the `"r"` object, or `Ok(None)` to stop answering and close
    /// the engine's input (the engine then saves and exits).
    fn respond(&mut self, id: u64, req: &Request) -> Result<Option<Value>, LinkError>;
}

#[derive(Debug, Clone)]
pub struct SessionLimits {
    /// Longest silence tolerated from the engine.
    pub step_timeout: Duration,
    /// Stop runaway sessions (e.g. a script that never quits).
    pub max_requests: usize,
}

impl Default for SessionLimits {
    fn default() -> Self {
        SessionLimits {
            step_timeout: Duration::from_secs(10),
            max_requests: 10_000,
        }
    }
}

/// Everything a session produced.
#[derive(Debug, Default)]
pub struct Transcript {
    /// Exact engine output lines, in order.
    pub lines: Vec<String>,
    /// Every reply we sent, in order.
    pub replies: Vec<RecordedReply>,
    pub hello: Option<Hello>,
    pub catalog: Option<Box<Catalog>>,
    /// Messages from the engine's "error" lines (client lost / bad reply).
    pub errors: Vec<String>,
    /// The engine said "bye" (clean exit through atexit).
    pub said_bye: bool,
    pub exit: Option<ExitStatus>,
}

impl Transcript {
    /// Hash of the whole engine output; equal hashes = identical sessions.
    pub fn stream_hash(&self) -> String {
        format!("{:016x}", fnv1a64(self.lines.join("\n").as_bytes()))
    }

    /// Text shown in the message window (for tests and logs).
    pub fn messages(&self, message_win: i32) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| match nh_protocol::parse_line(l) {
                Ok(EngineMsg::Win(WinCall::Putstr { win, text, .. })) if win == message_win => {
                    Some(text)
                }
                _ => None,
            })
            .collect()
    }
}

/// Drive `engine` until it exits, answering requests with `responder`.
pub fn run_session(
    engine: &mut Engine,
    responder: &mut dyn Responder,
    limits: &SessionLimits,
) -> Result<Transcript, LinkError> {
    let mut t = Transcript::default();
    let mut requests = 0usize;
    while let Some(inc) = engine.recv(limits.step_timeout)? {
        t.lines.push(inc.raw);
        match inc.msg {
            EngineMsg::Hello(h) => {
                if t.lines.len() != 1 || t.hello.is_some() {
                    return Err(LinkError::Handshake(
                        "hello is not the first message".into(),
                    ));
                }
                if h.protocol != PROTOCOL_VERSION {
                    return Err(LinkError::Handshake(format!(
                        "engine speaks protocol {}, client {}",
                        h.protocol, PROTOCOL_VERSION
                    )));
                }
                t.hello = Some(h);
            }
            _ if t.hello.is_none() => {
                return Err(LinkError::Handshake("first message is not hello".into()));
            }
            EngineMsg::Catalog(c) => t.catalog = Some(c),
            EngineMsg::Win(WinCall::RawPrint { .. }) => {}
            EngineMsg::Win(_) | EngineMsg::Req { .. } if t.catalog.is_none() => {
                return Err(LinkError::Handshake(
                    "window call before the catalog".into(),
                ));
            }
            EngineMsg::Win(_) => {}
            EngineMsg::Req { id, req } => {
                requests += 1;
                if requests > limits.max_requests {
                    engine.kill();
                    return Err(LinkError::TooManyRequests(limits.max_requests));
                }
                match responder.respond(id, &req)? {
                    Some(r) => {
                        engine.reply(id, &r)?;
                        t.replies.push(RecordedReply {
                            id,
                            func: req.name().to_string(),
                            r,
                        });
                    }
                    None => engine.close_input(),
                }
            }
            EngineMsg::Error { msg } => t.errors.push(msg),
            EngineMsg::Bye => t.said_bye = true,
        }
    }
    t.exit = Some(engine.wait(limits.step_timeout)?);
    Ok(t)
}
