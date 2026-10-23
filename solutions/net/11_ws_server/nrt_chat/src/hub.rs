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
        let (tx, rx) = mpsc::channel(capacity + 1);
        let inner = OutboxInner { tx, kicked: AtomicBool::new(false), kick: Notify::new(), seq: Mutex::new(0), stats };
        (Outbox { inner: Arc::new(inner) }, rx)
    }

    fn push(&self, text: String) -> bool {
        let inner = &self.inner;
        if inner.kicked.load(Ordering::Acquire) || inner.tx.is_closed() {
            return false;
        }
        if inner.tx.capacity() <= 1 {
            if !inner.kicked.swap(true, Ordering::AcqRel) {
                let _ = inner.tx.try_send(Out::Close(1008));
                inner.kick.notify_one();
                inner.stats.note_kicked();
            }
            return false;
        }
        inner.tx.try_send(Out::Text(text)).is_ok()
    }

    pub fn reply(&self, text: String) -> bool {
        self.push(text)
    }

    pub fn event(&self, event: &str, data: Value) -> bool {
        let mut seq = self.inner.seq.lock().unwrap_or_else(|p| p.into_inner());
        *seq += 1;
        self.push(render_event(event, *seq, data))
    }

    pub async fn kicked(&self) {
        self.inner.kick.notified().await
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
        let conn = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.lock().users.entry(user.to_string()).or_default().insert(conn, outbox);
        self.stats.connection_opened();
        Guard { hub: self.clone(), user: user.to_string(), conn }
    }

    pub fn join(&self, user: &str, room: &str) -> Vec<String> {
        let mut state = self.lock();
        if !state.users.contains_key(user) {
            return Vec::new();
        }
        let members = state.rooms.entry(room.to_string()).or_default();
        members.insert(user.to_string());
        members.iter().cloned().collect()
    }

    pub fn leave(&self, user: &str, room: &str) -> bool {
        let mut state = self.lock();
        let Some(members) = state.rooms.get_mut(room) else {
            return false;
        };
        let was = members.remove(user);
        if members.is_empty() {
            state.rooms.remove(room);
        }
        was
    }

    pub fn members(&self, room: &str) -> Vec<String> {
        self.lock().rooms.get(room).map(|m| m.iter().cloned().collect()).unwrap_or_default()
    }

    pub fn say(&self, room: &str, from: &str, from_conn: ConnId, text: &str) -> Option<usize> {
        let targets: Vec<Outbox> = {
            let state = self.lock();
            let members = state.rooms.get(room)?;
            if !members.contains(from) {
                return None;
            }
            members
                .iter()
                .filter_map(|user| state.users.get(user))
                .flat_map(|devices| devices.iter())
                .filter(|(id, _)| **id != from_conn)
                .map(|(_, outbox)| outbox.clone())
                .collect()
        };
        let data = json!({ "room": room, "from": from, "text": text });
        Some(targets.iter().filter(|outbox| outbox.event("chat.message", data.clone())).count())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        {
            let mut state = self.hub.lock();
            let empty = match state.users.get_mut(&self.user) {
                Some(devices) => {
                    devices.remove(&self.conn);
                    devices.is_empty()
                }
                None => false,
            };
            if empty {
                state.users.remove(&self.user);
                state.rooms.retain(|_, members| {
                    members.remove(&self.user);
                    !members.is_empty()
                });
            }
        }
        self.hub.stats.connection_closed();
    }
}
