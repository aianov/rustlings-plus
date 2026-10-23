use serde::{Deserialize, Serialize};
use serde_json::Value;

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "one")]
    pub schema: u32,
    pub version: u64,
    pub occurred_at: u64,
    pub aggregate_id: String,
    pub payload: Value,
}

impl Event {
    pub fn new(event_id: String, kind: &str, schema: u32, version: u64, occurred_at: u64, aggregate_id: &str, payload: Value) -> Event {
        Event { event_id, kind: kind.to_string(), schema, version, occurred_at, aggregate_id: aggregate_id.to_string(), payload }
    }
}
