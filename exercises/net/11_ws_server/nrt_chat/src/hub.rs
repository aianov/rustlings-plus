use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

use crate::Stats;
use crate::envelope::render_event;

pub type ConnId = u64;

#[derive(Debug, PartialEq)]
pub enum Out {
    Text(String),
    Close(u16),
}

struct OutboxInner {
    tx: mpsc::Sender<Out>,
    kicked: AtomicBool,
    kick: Notify,
    seq: Mutex<u64>,
    stats: Arc<Stats>,
}

#[derive(Clone)]
pub struct Outbox {
    inner: Arc<OutboxInner>,
}

impl Outbox {
    pub fn channel(capacity: usize, stats: Arc<Stats>) -> (Outbox, mpsc::Receiver<Out>) {
        todo!()
    }

    fn push(&self, text: String) -> bool {
        todo!()
    }

    pub fn reply(&self, text: String) -> bool {
        todo!()
    }

    pub fn event(&self, event: &str, data: Value) -> bool {
        todo!()
    }

    pub async fn kicked(&self) {
        todo!()
    }
}

#[derive(Default)]
struct State {
    users: HashMap<String, HashMap<ConnId, Outbox>>,
    rooms: HashMap<String, BTreeSet<String>>,
}

#[derive(Clone)]
pub struct Hub {
    state: Arc<Mutex<State>>,
    next: Arc<AtomicU64>,
    stats: Arc<Stats>,
}

pub struct Guard {
    hub: Hub,
    user: String,
    conn: ConnId,
}

impl Guard {
    pub fn id(&self) -> ConnId {
        self.conn
    }
}

impl Hub {
    pub fn new(stats: Arc<Stats>) -> Hub {
        Hub { state: Arc::default(), next: Arc::default(), stats }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn connect(&self, user: &str, outbox: Outbox) -> Guard {
        todo!()
    }

    pub fn join(&self, user: &str, room: &str) -> Vec<String> {
        todo!()
    }

    pub fn leave(&self, user: &str, room: &str) -> bool {
        todo!()
    }

    pub fn members(&self, room: &str) -> Vec<String> {
        todo!()
    }

    pub fn say(&self, room: &str, from: &str, from_conn: ConnId, text: &str) -> Option<usize> {
        todo!()
    }
}

