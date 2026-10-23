use std::sync::Arc;

use tokio::sync::broadcast::{self, error::RecvError};

use super::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Event(Arc<Event>),
    Missed(u64),
    Closed,
}

pub struct EventStream {
    events: broadcast::Receiver<Arc<Event>>,
    missed: u64,
}

impl EventStream {
    pub fn new(events: broadcast::Receiver<Arc<Event>>) -> EventStream {
        EventStream { events, missed: 0 }
    }

    pub async fn next(&mut self) -> Next {
        match self.events.recv().await {
            Ok(event) => Next::Event(event),
            Err(RecvError::Lagged(missed)) => {
                self.missed += missed;
                Next::Missed(missed)
            }
            Err(RecvError::Closed) => Next::Closed,
        }
    }

    pub async fn next_event(&mut self) -> Option<Arc<Event>> {
        loop {
            match self.next().await {
                Next::Event(event) => return Some(event),
                Next::Missed(_) => continue,
                Next::Closed => return None,
            }
        }
    }

    pub fn missed(&self) -> u64 {
        self.missed
    }
}
