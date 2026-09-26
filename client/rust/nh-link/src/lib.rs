//! Run `nh-engine` as a child process and drive a protocol session.

mod engine;
mod playground;
mod recording;
mod responder;
mod session;

pub use engine::*;
pub use playground::*;
pub use recording::*;
pub use responder::*;
pub use session::*;

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("cannot start {path}: {source}")]
    Spawn {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("I/O error talking to the engine: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad line from engine ({source}): {line}")]
    Protocol {
        line: String,
        #[source]
        source: nh_protocol::ProtocolError,
    },
    #[error("handshake failed: {0}")]
    Handshake(String),
    #[error("engine sent nothing for {0:?}")]
    Timeout(Duration),
    #[error("more than {0} requests; stopping")]
    TooManyRequests(usize),
    #[error("script: {0}")]
    Script(String),
    #[error("replay diverged at reply #{index}: recorded {expected}, engine asked {actual}")]
    Divergence {
        index: usize,
        expected: String,
        actual: String,
    },
    #[error("recording: {0}")]
    Recording(String),
}
