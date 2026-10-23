use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub kind: String,
}

impl Event {
    pub fn new(id: u64, kind: &str) -> Event {
        Event { id, kind: kind.to_string() }
    }
}
