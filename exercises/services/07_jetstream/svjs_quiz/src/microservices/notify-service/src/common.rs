use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Liked,
    Commented,
    Followed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u64,
    pub user: String,
    pub kind: Kind,
    pub actor: String,
    pub post_id: Option<u64>,
    pub request_id: Option<String>,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    pub user: String,
    pub kind: Kind,
    pub actor: String,
    pub post_id: Option<u64>,
    pub request_id: Option<String>,
}

pub type Deliver = Arc<dyn Fn(Notification) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub fn no_delivery() -> Deliver {
    Arc::new(|_| Box::pin(async {}))
}

#[derive(Clone)]
pub struct Config {
    pub url: String,
    pub root: String,
    pub stream: String,
    pub durable: String,
    pub ack_wait: Duration,
    pub batch: usize,
    pub deliver: Deliver,
}

impl Config {
    pub fn new(url: impl Into<String>, root: impl Into<String>) -> Config {
        let root = root.into();
        Config {
            url: url.into(),
            stream: format!("{root}_EVENTS"),
            root,
            durable: "notify-service".to_string(),
            ack_wait: Duration::from_secs(5),
            batch: 10,
            deliver: no_delivery(),
        }
    }

    pub fn dead_subject(&self) -> String {
        format!("{}.dead.notify", self.root)
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub received: AtomicUsize,
    pub ignored: AtomicUsize,
    pub rejected: AtomicUsize,
    pub stored: AtomicUsize,
    pub duplicates: AtomicUsize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsSnapshot {
    pub received: usize,
    pub ignored: usize,
    pub rejected: usize,
    pub stored: usize,
    pub duplicates: usize,
}

impl Stats {
    pub fn bump(counter: &AtomicUsize) {
        counter.fetch_add(1, Ordering::SeqCst);
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        let get = |c: &AtomicUsize| c.load(Ordering::SeqCst);
        StatsSnapshot {
            received: get(&self.received),
            ignored: get(&self.ignored),
            rejected: get(&self.rejected),
            stored: get(&self.stored),
            duplicates: get(&self.duplicates),
        }
    }
}
