use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::LinkError;

/// First line of a recording file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordingHeader {
    /// Recording format version.
    pub format: u32,
    pub seed: u64,
    pub fixed_time: i64,
    pub options: String,
    pub engine: String,
    pub patchset: String,
    /// `Transcript::stream_hash()` of the recorded session.
    pub stream_hash: String,
}

/// One reply the client sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedReply {
    pub id: u64,
    #[serde(rename = "fn")]
    pub func: String,
    pub r: Value,
}

/// A reproducible session: the header plus every reply, one JSON per line.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub header: RecordingHeader,
    pub replies: Vec<RecordedReply>,
}

pub const RECORDING_FORMAT: u32 = 1;

impl Recording {
    pub fn save(&self, path: &Path) -> Result<(), LinkError> {
        let mut out = fs::File::create(path)?;
        let header =
            serde_json::to_string(&self.header).map_err(|e| LinkError::Recording(e.to_string()))?;
        writeln!(out, "{header}")?;
        for r in &self.replies {
            let line = serde_json::to_string(r).map_err(|e| LinkError::Recording(e.to_string()))?;
            writeln!(out, "{line}")?;
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Recording, LinkError> {
        let text = fs::read_to_string(path)?;
        let mut lines = text.lines();
        let bad = |what: &str, e: serde_json::Error| {
            LinkError::Recording(format!("{}: {what}: {e}", path.display()))
        };
        let header: RecordingHeader = serde_json::from_str(
            lines
                .next()
                .ok_or_else(|| LinkError::Recording("empty file".into()))?,
        )
        .map_err(|e| bad("header", e))?;
        if header.format != RECORDING_FORMAT {
            return Err(LinkError::Recording(format!(
                "unsupported format {}",
                header.format
            )));
        }
        let replies = lines
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| bad("reply", e)))
            .collect::<Result<_, _>>()?;
        Ok(Recording { header, replies })
    }
}
