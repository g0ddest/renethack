//! renethack engine protocol (v1): JSON Lines between `nh-engine` and a client.
//!
//! Engine -> client: `{"t":<type>,"id"?:N,"fn"?:<name>,"a":{...}}` per line.
//! Client -> engine: `{"id":N,"r":{...}}`, one line per request.

mod catalog;
mod glyph;
mod hash;
mod msg;
mod reply;

pub use catalog::*;
pub use glyph::*;
pub use hash::fnv1a64;
pub use msg::*;
pub use reply::*;

/// Protocol version this crate speaks; must match `hello.protocol`.
pub const PROTOCOL_VERSION: u32 = 1;
