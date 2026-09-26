use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One reply the client sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedReply {
    pub id: u64,
    #[serde(rename = "fn")]
    pub func: String,
    pub r: Value,
}
