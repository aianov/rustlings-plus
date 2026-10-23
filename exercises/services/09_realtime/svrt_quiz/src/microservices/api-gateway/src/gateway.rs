use crate::Shared;
use bus::{Bus, Delivery, REVOKED, Subscription, typing_subject, user_subject, valid_word};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

const REMEMBERED: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Message { seq: u64, text: String },
    Typing(String),
}

#[derive(Debug, PartialEq)]
pub enum Refused {
    BadUser,
    BadSession,
    Revoked,
}

pub struct Connection {
    pub id: u64,
    pub user: String,
    pub session: String,
    pub inbox: Receiver<Out>,
    closed: Arc<AtomicBool>,
}

impl Connection {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn drain(&self) -> Vec<Out> {
        self.inbox.try_iter().collect()
    }
}

struct Conn {
    user: String,
    session: String,
    tx: Sender<Out>,
    closed: Arc<AtomicBool>,
    last: u64,
    held: Option<Vec<Delivery>>,
}

struct UserSubs {
    refs: usize,
    messages: Subscription,
    typing: Subscription,
}

#[derive(Default)]
struct Inner {
    next: u64,
    conns: HashMap<u64, Conn>,
    subs: HashMap<String, UserSubs>,
    order: VecDeque<String>,
    revoked: HashSet<String>,
}

pub struct Gateway {
    id: String,
    bus: Bus,
    presence: Shared,
    revocations: Subscription,
    inner: Mutex<Inner>,
}

impl Gateway {
    pub fn new(id: &str, bus: Bus, presence: Shared) -> Gateway {
        let revocations = bus.subscribe(REVOKED);
        Gateway { id: id.to_string(), bus, presence, revocations, inner: Mutex::new(Inner::default()) }
    }

    pub fn connect(&self, now: u64, user: &str, session: &str, resume_from: Option<u64>) -> Result<Connection, Refused> {
        todo!()
    }

    pub fn disconnect(&self, connection: &Connection) {
        todo!()
    }

    pub fn tick(&self, now: u64) {
        todo!()
    }

    pub fn pump(&self) {
        todo!()
    }

    pub fn logout(&self, session: &str) {
        todo!()
    }

}
