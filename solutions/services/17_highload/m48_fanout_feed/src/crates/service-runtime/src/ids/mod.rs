use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

pub const EPOCH_MS: u64 = 1_735_689_600_000;

pub const NODE_BITS: u32 = 10;

pub const COUNTER_BITS: u32 = 12;

pub const MAX_NODE: u16 = (1 << NODE_BITS) - 1;

const MAX_COUNTER: u64 = (1 << COUNTER_BITS) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdParts {
    pub millis: u64,
    pub node: u16,
    pub counter: u16,
}

#[derive(Debug)]
struct Last {
    millis: u64,
    counter: u64,
}

#[derive(Debug)]
pub struct SortableIds {
    node: u64,
    last: Mutex<Last>,
}

pub fn wall_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

pub fn compose(parts: IdParts) -> u64 {
    (parts.millis.saturating_sub(EPOCH_MS) << (NODE_BITS + COUNTER_BITS))
        | (u64::from(parts.node & MAX_NODE) << COUNTER_BITS)
        | (u64::from(parts.counter) & MAX_COUNTER)
}

pub fn split(id: u64) -> IdParts {
    IdParts {
        millis: (id >> (NODE_BITS + COUNTER_BITS)) + EPOCH_MS,
        node: u16::try_from((id >> COUNTER_BITS) & u64::from(MAX_NODE)).unwrap_or(0),
        counter: u16::try_from(id & MAX_COUNTER).unwrap_or(0),
    }
}

impl SortableIds {
    pub fn new(node: u16) -> SortableIds {
        SortableIds {
            node: u64::from(node.min(MAX_NODE)),
            last: Mutex::new(Last {
                millis: 0,
                counter: 0,
            }),
        }
    }

    pub fn next(&self) -> u64 {
        self.next_at(wall_millis())
    }

    pub fn next_at(&self, now: u64) -> u64 {
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let now = now.max(EPOCH_MS);
        if now > last.millis {
            last.millis = now;
            last.counter = 0;
        } else if last.counter < MAX_COUNTER {
            last.counter += 1;
        } else {
            last.millis += 1;
            last.counter = 0;
        }
        compose(IdParts {
            millis: last.millis,
            node: u16::try_from(self.node).unwrap_or(0),
            counter: u16::try_from(last.counter).unwrap_or(0),
        })
    }

    pub fn observe(&self, id: u64) {
        let seen = split(id);
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.millis > last.millis {
            last.millis = seen.millis;
            last.counter = u64::from(seen.counter);
        } else if seen.millis == last.millis {
            last.counter = last.counter.max(u64::from(seen.counter));
        }
    }
}
