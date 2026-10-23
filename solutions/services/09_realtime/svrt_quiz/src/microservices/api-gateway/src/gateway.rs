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

impl Conn {
    fn pass(&mut self, delivery: &Delivery) {
        if delivery.seq > self.last {
            self.last = delivery.seq;
            self.tx.send(Out::Message { seq: delivery.seq, text: delivery.text.clone() }).ok();
        }
    }
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

impl Inner {
    fn device(gateway: &str, conn: u64) -> String {
        format!("{gateway}:{conn}")
    }

    fn close(&mut self, gateway: &str, presence: &Shared, id: u64) {
        let Some(conn) = self.conns.remove(&id) else {
            return;
        };
        conn.closed.store(true, Ordering::SeqCst);
        presence.lock().unwrap().leave(&conn.user, &Inner::device(gateway, id));
        if let Some(subs) = self.subs.get_mut(&conn.user) {
            subs.refs -= 1;
            if subs.refs == 0 {
                self.subs.remove(&conn.user);
            }
        }
    }

    fn revoke(&mut self, gateway: &str, presence: &Shared, session: &str) {
        let ids: Vec<u64> = self.conns.iter().filter(|(_, conn)| conn.session == session).map(|(id, _)| *id).collect();
        for id in ids {
            self.close(gateway, presence, id);
        }
        if self.revoked.insert(session.to_string()) {
            self.order.push_back(session.to_string());
            if self.order.len() > REMEMBERED
                && let Some(oldest) = self.order.pop_front()
            {
                self.revoked.remove(&oldest);
            }
        }
    }
}

impl Gateway {
    pub fn new(id: &str, bus: Bus, presence: Shared) -> Gateway {
        let revocations = bus.subscribe(REVOKED);
        Gateway { id: id.to_string(), bus, presence, revocations, inner: Mutex::new(Inner::default()) }
    }

    pub fn connect(&self, now: u64, user: &str, session: &str, resume_from: Option<u64>) -> Result<Connection, Refused> {
        if !valid_word(user) {
            return Err(Refused::BadUser);
        }
        if session.is_empty() || session.len() > 64 {
            return Err(Refused::BadSession);
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.revoked.contains(session) {
            return Err(Refused::Revoked);
        }
        inner.next += 1;
        let id = inner.next;
        let (tx, inbox) = channel();
        let closed = Arc::new(AtomicBool::new(false));
        let bus = self.bus.clone();
        let subs = inner.subs.entry(user.to_string()).or_insert_with(|| UserSubs {
            refs: 0,
            messages: bus.subscribe(&user_subject(user)),
            typing: bus.subscribe(&typing_subject(user)),
        });
        subs.refs += 1;
        let conn = Conn {
            user: user.to_string(),
            session: session.to_string(),
            tx,
            closed: closed.clone(),
            last: resume_from.unwrap_or(0),
            held: resume_from.map(|_| Vec::new()),
        };
        inner.conns.insert(id, conn);
        self.presence.lock().unwrap().heartbeat(user, &Inner::device(&self.id, id), now);
        if let Some(last_seen) = resume_from {
            let history = self.bus.history_after(&user_subject(user), last_seen);
            if let Some(conn) = inner.conns.get_mut(&id) {
                for delivery in &history {
                    conn.pass(delivery);
                }
            }
            self.drain(&mut inner);
            if let Some(conn) = inner.conns.get_mut(&id) {
                let mut held = conn.held.take().unwrap_or_default();
                held.sort_by_key(|d| d.seq);
                for delivery in &held {
                    conn.pass(delivery);
                }
            }
        }
        Ok(Connection { id, user: user.to_string(), session: session.to_string(), inbox, closed })
    }

    pub fn disconnect(&self, connection: &Connection) {
        let mut inner = self.inner.lock().unwrap();
        inner.close(&self.id, &self.presence, connection.id);
    }

    pub fn tick(&self, now: u64) {
        let inner = self.inner.lock().unwrap();
        let mut presence = self.presence.lock().unwrap();
        for (id, conn) in &inner.conns {
            presence.heartbeat(&conn.user, &Inner::device(&self.id, *id), now);
        }
    }

    pub fn pump(&self) {
        let mut inner = self.inner.lock().unwrap();
        self.drain(&mut inner);
    }

    pub fn logout(&self, session: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.revoke(&self.id, &self.presence, session);
        drop(inner);
        self.bus.publish(REVOKED, session);
    }

    fn drain(&self, inner: &mut Inner) {
        while let Some(revocation) = self.revocations.try_next() {
            inner.revoke(&self.id, &self.presence, &revocation.text);
        }
        let mut messages: Vec<(String, Delivery)> = Vec::new();
        let mut typing: Vec<(String, Delivery)> = Vec::new();
        for (user, subs) in &inner.subs {
            while let Some(delivery) = subs.messages.try_next() {
                messages.push((user.clone(), delivery));
            }
            while let Some(delivery) = subs.typing.try_next() {
                typing.push((user.clone(), delivery));
            }
        }
        for (user, delivery) in messages {
            for conn in inner.conns.values_mut().filter(|conn| conn.user == user) {
                match conn.held.as_mut() {
                    Some(held) => held.push(delivery.clone()),
                    None => conn.pass(&delivery),
                }
            }
        }
        for (user, delivery) in typing {
            for conn in inner.conns.values().filter(|conn| conn.user == user) {
                conn.tx.send(Out::Typing(delivery.text.clone())).ok();
            }
        }
    }
}
