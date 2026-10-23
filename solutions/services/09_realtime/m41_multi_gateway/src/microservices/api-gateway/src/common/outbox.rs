use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use contracts::{Push, encode};
use tokio::sync::{Notify, mpsc};

pub const TOO_SLOW: u16 = 1008;

pub const REVOKED: u16 = 4003;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    Text(String),
    Close(u16),
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Cursor {
    last: u64,
    held: Option<Vec<(u64, String)>>,
}

struct Inner {
    tx: mpsc::Sender<Out>,
    kicked: AtomicBool,
    code: AtomicU16,
    kick: Notify,
    tally: Arc<AtomicU64>,
    cursor: Mutex<Cursor>,
}

#[derive(Clone)]
pub struct Outbox {
    inner: Arc<Inner>,
}

impl Outbox {
    pub fn channel(capacity: usize) -> (Outbox, mpsc::Receiver<Out>) {
        Outbox::counted(capacity, Arc::default())
    }

    pub(super) fn counted(capacity: usize, tally: Arc<AtomicU64>) -> (Outbox, mpsc::Receiver<Out>) {
        let (tx, rx) = mpsc::channel(capacity.max(1) + 1);
        let inner = Inner {
            tx,
            kicked: AtomicBool::new(false),
            code: AtomicU16::new(0),
            kick: Notify::new(),
            tally,
            cursor: Mutex::default(),
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

    pub async fn send_wait(&self, text: String) -> bool {
        if self.is_kicked() {
            return false;
        }
        self.inner.tx.send(Out::Text(text)).await.is_ok()
    }

    pub fn push(&self, push: &Push) -> bool {
        match encode(push) {
            Ok(text) => self.send(text),
            Err(..) => false,
        }
    }

    pub fn hold(&self, after: u64) {
        *lock(&self.inner.cursor) = Cursor {
            last: after,
            held: Some(Vec::new()),
        };
    }

    pub fn stored(&self, seq: u64, text: String) -> bool {
        let mut cursor = lock(&self.inner.cursor);
        if seq <= cursor.last {
            return true;
        }
        if let Some(held) = cursor.held.as_mut() {
            held.push((seq, text));
            return true;
        }
        cursor.last = seq;
        self.send(text)
    }

    pub async fn caught(&self, seq: u64, text: String) -> bool {
        {
            let mut cursor = lock(&self.inner.cursor);
            if seq <= cursor.last {
                return true;
            }
            cursor.last = seq;
        }
        self.send_wait(text).await
    }

    pub async fn release(&self) -> bool {
        loop {
            let ready = {
                let mut cursor = lock(&self.inner.cursor);
                let last = cursor.last;
                let mut ready = match cursor.held.as_mut() {
                    Some(held) if !held.is_empty() => std::mem::take(held),
                    Some(..) | None => {
                        cursor.held = None;
                        return true;
                    }
                };
                ready.sort_by_key(|(seq, _)| *seq);
                ready.dedup_by_key(|(seq, _)| *seq);
                ready.retain(|(seq, _)| *seq > last);
                if let Some((seq, _)) = ready.last() {
                    cursor.last = *seq;
                }
                ready
            };
            for (_, text) in ready {
                if !self.send_wait(text).await {
                    return false;
                }
            }
        }
    }

    pub fn last_seq(&self) -> u64 {
        lock(&self.inner.cursor).last
    }

    fn shut(&self, code: u16) {
        let inner = &self.inner;
        if !inner.kicked.swap(true, Ordering::AcqRel) {
            inner.code.store(code, Ordering::Release);
            let _ = inner.tx.try_send(Out::Close(code));
            inner.kick.notify_one();
            if code == TOO_SLOW {
                inner.tally.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn kick(&self) {
        self.shut(TOO_SLOW);
    }

    pub fn revoke(&self) {
        self.shut(REVOKED);
    }

    pub fn is_kicked(&self) -> bool {
        self.inner.kicked.load(Ordering::Acquire)
    }

    pub fn close_code(&self) -> Option<u16> {
        match self.inner.code.load(Ordering::Acquire) {
            0 => None,
            code => Some(code),
        }
    }

    pub fn is_gone(&self) -> bool {
        self.inner.tx.is_closed()
    }

    pub async fn kicked(&self) {
        if self.is_kicked() {
            return;
        }
        self.inner.kick.notified().await;
    }
}
