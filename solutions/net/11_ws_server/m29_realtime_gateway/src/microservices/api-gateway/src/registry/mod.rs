use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use contracts::{Push, UserId, encode};
use social_core::{Notification, Notifier};
use tokio::sync::{Notify, mpsc};

pub const TOO_SLOW: u16 = 1008;

pub type ConnId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    Text(String),
    Close(u16),
}

struct Inner {
    tx: mpsc::Sender<Out>,
    kicked: AtomicBool,
    kick: Notify,
    tally: Arc<AtomicU64>,
}

#[derive(Clone)]
pub struct Outbox {
    inner: Arc<Inner>,
}

impl Outbox {
    pub fn channel(capacity: usize) -> (Outbox, mpsc::Receiver<Out>) {
        Outbox::counted(capacity, Arc::default())
    }

    fn counted(capacity: usize, tally: Arc<AtomicU64>) -> (Outbox, mpsc::Receiver<Out>) {
        let (tx, rx) = mpsc::channel(capacity.max(1) + 1);
        let inner = Inner {
            tx,
            kicked: AtomicBool::new(false),
            kick: Notify::new(),
            tally,
        };
        (
            Outbox {
                inner: Arc::new(inner),
            },
            rx,
        )
    }

    pub fn send(&self, text: String) -> bool {
        let inner = &self.inner;
        if inner.kicked.load(Ordering::Acquire) || inner.tx.is_closed() {
            return false;
        }
        if inner.tx.capacity() <= 1 {
            self.kick();
            return false;
        }
        inner.tx.try_send(Out::Text(text)).is_ok()
    }

    pub fn push(&self, push: &Push) -> bool {
        match encode(push) {
            Ok(text) => self.send(text),
            Err(..) => false,
        }
    }

    pub fn kick(&self) {
        let inner = &self.inner;
        if !inner.kicked.swap(true, Ordering::AcqRel) {
            let _ = inner.tx.try_send(Out::Close(TOO_SLOW));
            inner.kick.notify_one();
            inner.tally.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn is_kicked(&self) -> bool {
        self.inner.kicked.load(Ordering::Acquire)
    }

    pub async fn kicked(&self) {
        if self.is_kicked() {
            return;
        }
        self.inner.kick.notified().await;
    }
}

type Devices = HashMap<UserId, HashMap<ConnId, Outbox>>;

#[derive(Default)]
pub struct Registry {
    devices: Mutex<Devices>,
    next: AtomicU64,
    kicked: Arc<AtomicU64>,
}

pub struct Registration {
    registry: Arc<Registry>,
    user: UserId,
    conn: ConnId,
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, Devices> {
        self.devices.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn outbox(&self, capacity: usize) -> (Outbox, mpsc::Receiver<Out>) {
        Outbox::counted(capacity, Arc::clone(&self.kicked))
    }

    pub fn register(self: &Arc<Registry>, user: UserId, outbox: Outbox) -> Registration {
        let conn = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.lock().entry(user).or_default().insert(conn, outbox);
        Registration {
            registry: Arc::clone(self),
            user,
            conn,
        }
    }

    pub fn devices(&self, user: UserId) -> usize {
        self.lock().get(&user).map_or(0, HashMap::len)
    }

    pub fn users(&self) -> usize {
        self.lock().len()
    }

    pub fn connections(&self) -> usize {
        self.lock().values().map(HashMap::len).sum()
    }

    pub fn kicked(&self) -> u64 {
        self.kicked.load(Ordering::Relaxed)
    }

    pub fn deliver(&self, user: UserId, push: &Push) -> usize {
        let Ok(text) = encode(push) else {
            return 0;
        };
        let targets: Vec<Outbox> = self
            .lock()
            .get(&user)
            .map(|own| own.values().cloned().collect())
            .unwrap_or_default();
        targets
            .iter()
            .filter(|outbox| outbox.send(text.clone()))
            .count()
    }

    fn leave(&self, user: UserId, conn: ConnId) {
        let mut devices = self.lock();
        if let Some(own) = devices.get_mut(&user) {
            own.remove(&conn);
            if own.is_empty() {
                devices.remove(&user);
            }
        }
    }
}

impl Registration {
    pub fn user(&self) -> UserId {
        self.user
    }

    pub fn conn(&self) -> ConnId {
        self.conn
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.registry.leave(self.user, self.conn);
    }
}

impl Notifier for Registry {
    fn notify(&self, user: UserId, notification: Notification) {
        self.deliver(user, &notification.to_push(user));
    }
}
