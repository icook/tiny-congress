use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimal sigchain link representation used for validation and reducers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigchainLink {
    pub seqno: i64,
    pub prev_hash: Option<String>,
    pub ctime: DateTime<Utc>,
    pub link_type: String,
    pub body: Value,
}
