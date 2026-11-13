//! A file's stamp: a number its bytes make. build.rs stamps every file a
//! debug library is built from; the game, run from a checkout, stamps them
//! again at start-up (freshness.rs). One function serves both (build.rs
//! includes this file), so a stamp differs only where a file does.

use std::path::Path;
use std::time::UNIX_EPOCH;

/// When the file at `path` was last written, in nanoseconds (0 when it
/// cannot be told): a file whose time is the one stamped is not read again.
pub fn written(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

/// The stamp of the file at `path`; one that cannot be read has its own,
/// unlike any content's.
pub fn file_stamp(path: &Path) -> u64 {
    let Ok(bytes) = std::fs::read(path) else {
        return u64::MAX;
    };
    let mut mix = Mix(0xcbf2_9ce4_8422_2325);
    mix.word(bytes.len() as u64);
    let mut words = bytes.chunks_exact(8);
    for w in &mut words {
        mix.word(u64::from_le_bytes([
            w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7],
        ]));
    }
    let rest = words.remainder();
    let mut last = [0u8; 8];
    last[..rest.len()].copy_from_slice(rest);
    mix.word(u64::from_le_bytes(last));
    mix.0
}

/// A word at a time (a debug build stamps some 7 MB at start-up): not a
/// cryptographic hash, only one that no edit matches by chance.
struct Mix(u64);

impl Mix {
    fn word(&mut self, w: u64) {
        self.0 = (self.0 ^ w).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.0 ^= self.0 >> 29;
    }
}
