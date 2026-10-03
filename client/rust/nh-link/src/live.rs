use std::collections::VecDeque;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use nh_protocol::{Catalog, EngineMsg, Hello, Reply, Request, WinCall, fnv1a64};
use serde_json::Value;

use crate::handshake::Handshake;
use crate::{Engine, EngineConfig, Incoming, LinkError, Polled, RecordedReply};

/// Engine lines kept for bug reports.
const RECENT_LINES: usize = 5000;
/// Lines of engine.stderr an `Ending` carries.
const STDERR_TAIL_LINES: usize = 40;
/// How long `Drop` lets a hung-up engine save before killing it.
const DROP_GRACE: Duration = Duration::from_secs(5);
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// What a `LiveSession` saw, in the engine's order.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Hello(Hello),
    Catalog(Box<Catalog>),
    Win(WinCall),
    /// Only while the session is open; after `hang_up` requests are
    /// answered as hangups internally.
    Request {
        id: u64,
        req: Request,
    },
    /// The engine's "error" line.
    EngineError(String),
    Bye,
    /// Protocol or I/O failure; the session kills the engine and then
    /// reports `Exited`.
    Failed(String),
    /// Exactly once, last.
    Exited(Ending),
}

/// How the engine process ended.
#[derive(Debug, Clone, PartialEq)]
pub struct Ending {
    /// Exit code; `None` when killed by a signal.
    pub code: Option<i32>,
    pub said_bye: bool,
    /// The engine's "error" line, if any.
    pub engine_error: Option<String>,
    /// We called `hang_up` (or `Drop` did).
    pub hung_up_by_client: bool,
    /// The `SessionEvent::Failed` text, if any.
    pub failed: Option<String>,
    pub stderr_tail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AnswerError {
    /// Harmless: a stale UI event or a double click.
    #[error("no request {0} is waiting")]
    NotPending(u64),
    /// Writing the reply failed: fatal for the session.
    #[error(transparent)]
    Link(#[from] LinkError),
}

/// A session driven by an interactive client: never blocks, hands out
/// events as they come and takes answers whenever the player decides.
pub struct LiveSession {
    engine: Engine,
    handshake: Handshake,
    pending: Option<(u64, Request)>,
    /// FNV-1a state over all lines joined by '\n'.
    hash: u64,
    line_count: usize,
    replies: Vec<RecordedReply>,
    recent: VecDeque<String>,
    /// Last line from the engine or last thing we sent it.
    last_activity: Instant,
    hung_up: bool,
    /// We killed the engine: requests still in the pipe cannot be answered.
    killed: bool,
    output_closed: bool,
    exited: bool,
    said_bye: bool,
    engine_error: Option<String>,
    failed: Option<String>,
}

impl LiveSession {
    pub fn start(cfg: &EngineConfig) -> Result<LiveSession, LinkError> {
        Ok(LiveSession {
            engine: Engine::spawn(cfg)?,
            handshake: Handshake::new(),
            pending: None,
            hash: fnv1a64(b""),
            line_count: 0,
            replies: Vec::new(),
            recent: VecDeque::new(),
            last_activity: Instant::now(),
            hung_up: false,
            killed: false,
            output_closed: false,
            exited: false,
            said_bye: false,
            engine_error: None,
            failed: None,
        })
    }

    /// Decoded events until `deadline` or until nothing is ready; never
    /// blocks. Stops early after a Request, after a "delay_output" call
    /// (a frame boundary for animations) and after Exited.
    pub fn poll(&mut self, deadline: Instant) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        while !self.exited {
            if self.output_closed {
                match self.engine.try_wait() {
                    Ok(Some(status)) => self.finish(Some(status), &mut events),
                    Ok(None) => {}
                    Err(e) => self.fail(e, &mut events),
                }
                break;
            }
            let stop = match self.engine.try_recv() {
                Ok(Polled::Line(inc)) => self.take(*inc, &mut events),
                Ok(Polled::Empty) => break,
                Ok(Polled::Closed) => {
                    self.output_closed = true;
                    continue;
                }
                Err(e) => {
                    self.fail(e, &mut events);
                    true
                }
            };
            if stop || Instant::now() >= deadline {
                break;
            }
        }
        events
    }

    pub fn pending(&self) -> Option<(u64, &Request)> {
        self.pending.as_ref().map(|(id, req)| (*id, req))
    }

    /// Answer the pending request. `NotPending` is harmless; `Link` means
    /// the engine is gone.
    pub fn answer(&mut self, id: u64, reply: &Reply) -> Result<(), AnswerError> {
        let func = match &self.pending {
            Some((waiting, req)) if *waiting == id => req.name(),
            _ => return Err(AnswerError::NotPending(id)),
        };
        let r = reply.to_value();
        self.pending = None;
        self.last_activity = Instant::now();
        self.engine.reply(id, &r)?;
        self.replies.push(RecordedReply {
            id,
            func: func.to_string(),
            r,
        });
        Ok(())
    }

    /// The engine owes output (no request pending, not exited) and has been
    /// silent that long.
    pub fn silent_for(&self) -> Option<Duration> {
        if self.exited || self.pending.is_some() {
            None
        } else {
            Some(self.last_activity.elapsed())
        }
    }

    /// Close the engine's input: it saves and exits. A pending request is
    /// recorded as a hangup (r: null); requests that still arrive are
    /// recorded the same way, never exposed.
    pub fn hang_up(&mut self) {
        if self.hung_up || self.exited {
            return;
        }
        self.hung_up = true;
        if let Some((id, req)) = self.pending.take() {
            self.record_hangup(id, &req);
        }
        self.engine.close_input();
        self.last_activity = Instant::now();
    }

    /// Kill the engine; `poll` then reports the lines it had already sent
    /// (requests among them are dropped) and `Exited`.
    pub fn kill(&mut self) {
        self.killed = true;
        self.pending = None;
        self.engine.kill();
    }

    pub fn has_exited(&self) -> bool {
        self.exited
    }

    /// FNV-1a of all lines joined by '\n' (equal to
    /// `Transcript::stream_hash`), computed as the lines arrive.
    pub fn stream_hash(&self) -> String {
        format!("{:016x}", self.hash)
    }

    pub fn replies(&self) -> &[RecordedReply] {
        &self.replies
    }

    /// The last engine lines (up to 5000) for bug reports.
    pub fn recent_lines(&self) -> impl Iterator<Item = &str> {
        self.recent.iter().map(String::as_str)
    }

    /// Handle one line; true when `poll` should stop after it.
    fn take(&mut self, inc: Incoming, events: &mut Vec<SessionEvent>) -> bool {
        let Incoming { raw, msg } = inc;
        self.last_activity = Instant::now();
        if self.line_count > 0 {
            self.hash = fnv_feed(self.hash, b"\n");
        }
        self.hash = fnv_feed(self.hash, raw.as_bytes());
        self.line_count += 1;
        self.remember(raw);
        if let Err(e) = self.handshake.check(&msg) {
            self.fail(e, events);
            return true;
        }
        match msg {
            EngineMsg::Hello(h) => events.push(SessionEvent::Hello(h)),
            EngineMsg::Catalog(c) => events.push(SessionEvent::Catalog(c)),
            EngineMsg::Win(w) => {
                let frame = matches!(w, WinCall::DelayOutput);
                events.push(SessionEvent::Win(w));
                return frame;
            }
            EngineMsg::Req { id, req } if self.hung_up => self.record_hangup(id, &req),
            EngineMsg::Req { .. } if self.killed => {}
            EngineMsg::Req { id, req } => {
                self.pending = Some((id, req.clone()));
                events.push(SessionEvent::Request { id, req });
                return true;
            }
            EngineMsg::Error { msg } => {
                self.engine_error.get_or_insert_with(|| msg.clone());
                events.push(SessionEvent::EngineError(msg));
            }
            EngineMsg::Bye => {
                self.said_bye = true;
                events.push(SessionEvent::Bye);
            }
        }
        false
    }

    fn remember(&mut self, line: String) {
        if self.recent.len() == RECENT_LINES {
            self.recent.pop_front();
        }
        self.recent.push_back(line);
    }

    fn record_hangup(&mut self, id: u64, req: &Request) {
        // run_session records hangups the same way, so the session replays
        self.replies.push(RecordedReply {
            id,
            func: req.name().to_string(),
            r: Value::Null,
        });
    }

    fn fail(&mut self, e: LinkError, events: &mut Vec<SessionEvent>) {
        if let LinkError::Protocol { line, .. } = &e {
            self.remember(line.clone());
        }
        let text = e.to_string();
        self.failed = Some(text.clone());
        events.push(SessionEvent::Failed(text));
        self.pending = None;
        self.engine.kill();
        let status = self.engine.try_wait().ok().flatten();
        self.finish(status, events);
    }

    fn finish(&mut self, status: Option<ExitStatus>, events: &mut Vec<SessionEvent>) {
        self.exited = true;
        self.pending = None;
        events.push(SessionEvent::Exited(Ending {
            code: status.and_then(|s| s.code()),
            said_bye: self.said_bye,
            engine_error: self.engine_error.clone(),
            hung_up_by_client: self.hung_up,
            failed: self.failed.clone(),
            stderr_tail: self.engine.stderr_tail(STDERR_TAIL_LINES),
        }));
    }
}

impl Drop for LiveSession {
    /// Never kill a live game outright: hang up so the engine saves, and
    /// kill only if it does not exit in time.
    fn drop(&mut self) {
        if self.exited {
            return;
        }
        self.hang_up();
        let _ = self.engine.wait(DROP_GRACE);
    }
}

fn fnv_feed(mut h: u64, data: &[u8]) -> u64 {
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_hash_equals_the_hash_of_joined_lines() {
        let lines = ["{\"t\":\"hello\"}", "", "second line", "ünïcode"];
        let mut h = fnv1a64(b"");
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                h = fnv_feed(h, b"\n");
            }
            h = fnv_feed(h, l.as_bytes());
        }
        assert_eq!(h, fnv1a64(lines.join("\n").as_bytes()));
    }
}
