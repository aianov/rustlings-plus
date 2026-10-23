use crate::Event;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, PartialEq)]
pub struct Appended {
    pub seq: u64,
    pub duplicate: bool,
}

#[derive(Default)]
struct Inner {
    events: Vec<Event>,
    by_id: HashMap<String, u64>,
    failing: u32,
}

#[derive(Default)]
pub struct Journal {
    inner: Mutex<Inner>,
}

impl Journal {
    pub fn new() -> Journal {
        Journal::default()
    }

    pub fn fail_next_appends(&self, count: u32) {
        self.inner.lock().unwrap().failing = count;
    }

    pub fn append(&self, event: &Event) -> Result<Appended, String> {
        let mut inner = self.inner.lock().unwrap();
        if inner.failing > 0 {
            inner.failing -= 1;
            return Err("журнал недоступен".to_string());
        }
        if let Some(seq) = inner.by_id.get(&event.event_id) {
            return Ok(Appended { seq: *seq, duplicate: true });
        }
        inner.events.push(event.clone());
        let seq = inner.events.len() as u64;
        inner.by_id.insert(event.event_id.clone(), seq);
        Ok(Appended { seq, duplicate: false })
    }

    pub fn read_from(&self, seq: u64) -> Vec<(u64, Event)> {
        let inner = self.inner.lock().unwrap();
        inner.events.iter().enumerate().map(|(i, e)| (i as u64 + 1, e.clone())).filter(|(n, _)| *n >= seq).collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
