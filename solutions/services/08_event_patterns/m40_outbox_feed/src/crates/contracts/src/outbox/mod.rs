use std::collections::hash_map::RandomState;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::domain::{DomainEvent, Fact};
use crate::topics::Origin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cause {
    pub origin: Origin,
    pub at: u64,
}

impl Cause {
    pub fn new(origin: &Origin, at: u64) -> Cause {
        Cause {
            origin: origin.clone(),
            at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxRow {
    pub seq: u64,
    pub event: DomainEvent,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct Outbox {
    waiting: VecDeque<OutboxRow>,
    last_seq: u64,
    versions: HashMap<String, u64>,
    published: u64,
}

fn random_half(nanos: u128) -> u64 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(nanos);
    hasher.finish()
}

pub fn fresh_event_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    format!("evt-{:016x}{:016x}", random_half(nanos), random_half(nanos))
}

impl Outbox {
    pub fn new() -> Outbox {
        Outbox::default()
    }

    pub fn prepare(&self, fact: &Fact, cause: &Cause) -> OutboxRow {
        let aggregate = fact.aggregate();
        let version = self.versions.get(&aggregate).copied().unwrap_or(0) + 1;
        OutboxRow {
            seq: self.last_seq + 1,
            event: DomainEvent::new(&fresh_event_id(), fact, version, cause.at),
            request_id: cause.origin.request_id.clone(),
            trace_id: cause.origin.trace_id.clone(),
        }
    }

    pub fn push(&mut self, row: OutboxRow) {
        self.last_seq = self.last_seq.max(row.seq);
        let version = self
            .versions
            .entry(row.event.aggregate_id.clone())
            .or_insert(0);
        *version = (*version).max(row.event.version);
        if row.seq > self.published {
            self.waiting.push_back(row);
        }
    }

    pub fn published_through(&mut self, seq: u64) {
        self.published = self.published.max(seq);
        while self
            .waiting
            .front()
            .is_some_and(|row| row.seq <= self.published)
        {
            self.waiting.pop_front();
        }
    }

    pub fn unpublished(&self, limit: usize) -> Vec<OutboxRow> {
        self.waiting.iter().take(limit).cloned().collect()
    }

    pub fn published(&self) -> u64 {
        self.published
    }

    pub fn pending(&self) -> usize {
        self.waiting.len()
    }

    pub fn version(&self, aggregate: &str) -> u64 {
        self.versions.get(aggregate).copied().unwrap_or(0)
    }
}
