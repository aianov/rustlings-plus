use std::{
    collections::{HashMap, VecDeque},
    fmt::Write,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use rand::RngCore;
use serde_json::{Value, json};
use tokio::{
    sync::Notify,
    task::JoinHandle,
    time::{sleep, timeout},
};

use crate::{
    config::Config,
    dto::{FieldError, SubscribeRequest, is_event_type, is_idempotency_key},
    error::ApiError,
    sign::{hex_sha256, signature},
};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Outgoing {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

pub trait Transport: Send + Sync + 'static {
    fn send(&self, request: Outgoing) -> BoxFuture<Result<u16, TransportError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub id: String,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    pub event_id: String,
    pub subscription: String,
    pub attempts: u32,
    pub last_status: Option<u16>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub published: u64,
    pub delivered: u64,
    pub failed_attempts: u64,
    pub dead: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    pub drained: bool,
    pub undelivered: usize,
}

struct Event {
    id: String,
    kind: String,
    body: Vec<u8>,
}

struct Sub {
    id: String,
    tenant: String,
    url: String,
    events: Vec<String>,
    queue: Mutex<VecDeque<Arc<Event>>>,
    wake: Notify,
}

impl Sub {
    fn wants(&self, kind: &str) -> bool {
        self.events.iter().any(|e| e == "*" || e == kind)
    }
}

#[derive(Default)]
struct State {
    keys: HashMap<String, String>,
    subs: Vec<Arc<Sub>>,
    next_sub: u64,
    next_event: u64,
    seen: HashMap<(String, String), (String, String)>,
    dead: Vec<(String, DeadLetter)>,
    stats: Stats,
}

struct Inner {
    config: Config,
    transport: Arc<dyn Transport>,
    state: Mutex<State>,
    pending: AtomicUsize,
    idle: Notify,
    closing: AtomicBool,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn workers(&self) -> MutexGuard<'_, Vec<JoinHandle<()>>> {
        self.workers.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn tenant(&self, key: &str) -> Result<String, ApiError> {
        self.state()
            .keys
            .get(&hex_sha256(key.as_bytes()))
            .cloned()
            .ok_or(ApiError::Unauthorized)
    }

    fn open(&self) -> Result<(), ApiError> {
        if self.closing.load(Ordering::SeqCst) {
            Err(ApiError::ShuttingDown)
        } else {
            Ok(())
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Hookd {
    inner: Arc<Inner>,
}

impl Hookd {
    pub fn new(config: Config, transport: Arc<dyn Transport>) -> Hookd {
        Hookd {
            inner: Arc::new(Inner {
                config,
                transport,
                state: Mutex::new(State::default()),
                pending: AtomicUsize::new(0),
                idle: Notify::new(),
                closing: AtomicBool::new(false),
                workers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn create_tenant(&self, name: &str) -> String {
        let mut raw = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut raw);
        let mut key = String::from("hk_");
        for byte in raw {
            let _ = write!(key, "{byte:02x}");
        }
        self.inner
            .state()
            .keys
            .insert(hex_sha256(key.as_bytes()), name.to_owned());
        key
    }

    pub fn subscribe(&self, key: &str, request: SubscribeRequest) -> Result<String, ApiError> {
        let tenant = self.inner.tenant(key)?;
        self.inner.open()?;
        request.validate().map_err(ApiError::Validation)?;
        let sub = {
            let mut state = self.inner.state();
            state.next_sub += 1;
            let sub = Arc::new(Sub {
                id: format!("sub_{}", state.next_sub),
                tenant,
                url: request.url,
                events: request.events,
                queue: Mutex::new(VecDeque::new()),
                wake: Notify::new(),
            });
            state.subs.push(Arc::clone(&sub));
            sub
        };
        let worker = tokio::spawn(run(Arc::clone(&self.inner), Arc::clone(&sub)));
        self.inner.workers().push(worker);
        Ok(sub.id.clone())
    }

    pub fn publish(
        &self,
        key: &str,
        idempotency_key: &str,
        kind: &str,
        data: Value,
    ) -> Result<Published, ApiError> {
        let tenant = self.inner.tenant(key)?;
        self.inner.open()?;
        let mut errors = Vec::new();
        if !is_idempotency_key(idempotency_key) {
            errors.push(FieldError::new("idempotency_key", "format"));
        }
        if !is_event_type(kind) {
            errors.push(FieldError::new("type", "format"));
        }
        if !errors.is_empty() {
            return Err(ApiError::Validation(errors));
        }
        let fingerprint = hex_sha256(format!("{kind}\n{data}").as_bytes());
        let mut state = self.inner.state();
        let seen_key = (tenant.clone(), idempotency_key.to_owned());
        if let Some((id, seen)) = state.seen.get(&seen_key) {
            return if *seen == fingerprint {
                Ok(Published {
                    id: id.clone(),
                    duplicate: true,
                })
            } else {
                Err(ApiError::IdempotencyConflict)
            };
        }
        state.next_event += 1;
        let id = format!("evt_{}", state.next_event);
        let body = json!({ "id": id, "type": kind, "data": data }).to_string();
        let event = Arc::new(Event {
            id: id.clone(),
            kind: kind.to_owned(),
            body: body.into_bytes(),
        });
        state.seen.insert(seen_key, (id.clone(), fingerprint));
        state.stats.published += 1;
        for sub in state
            .subs
            .iter()
            .filter(|s| s.tenant == tenant && s.wants(kind))
        {
            self.inner.pending.fetch_add(1, Ordering::SeqCst);
            lock(&sub.queue).push_back(Arc::clone(&event));
            sub.wake.notify_one();
        }
        Ok(Published {
            id,
            duplicate: false,
        })
    }

    pub fn dead_letters(&self, key: &str) -> Result<Vec<DeadLetter>, ApiError> {
        let tenant = self.inner.tenant(key)?;
        Ok(self
            .inner
            .state()
            .dead
            .iter()
            .filter(|(owner, _)| *owner == tenant)
            .map(|(_, letter)| letter.clone())
            .collect())
    }

    pub fn stats(&self) -> Stats {
        self.inner.state().stats
    }

    pub async fn shutdown(&self, grace: Duration) -> ShutdownReport {
        self.inner.closing.store(true, Ordering::SeqCst);
        let inner = &self.inner;
        let drained = timeout(grace, async {
            loop {
                let idle = inner.idle.notified();
                tokio::pin!(idle);
                idle.as_mut().enable();
                if inner.pending.load(Ordering::SeqCst) == 0 {
                    break;
                }
                idle.await;
            }
        })
        .await
        .is_ok();
        for worker in inner.workers().drain(..) {
            worker.abort();
        }
        ShutdownReport {
            drained,
            undelivered: inner.pending.load(Ordering::SeqCst),
        }
    }
}

impl Drop for Hookd {
    fn drop(&mut self) {
        for worker in self.inner.workers().drain(..) {
            worker.abort();
        }
    }
}

async fn run(inner: Arc<Inner>, sub: Arc<Sub>) {
    loop {
        let next = lock(&sub.queue).front().cloned();
        let Some(event) = next else {
            sub.wake.notified().await;
            continue;
        };
        deliver(&inner, &sub, &event).await;
        lock(&sub.queue).pop_front();
        if inner.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            inner.idle.notify_waiters();
        }
    }
}

fn backoff(config: &Config, retry: u32) -> Duration {
    let factor = 1u32 << retry.saturating_sub(1).min(20);
    config.backoff.saturating_mul(factor).min(config.max_backoff)
}

async fn deliver(inner: &Inner, sub: &Sub, event: &Event) {
    let config = &inner.config;
    let mut attempts = 0;
    let mut last_status = None;
    while attempts < config.max_attempts {
        if attempts > 0 {
            sleep(backoff(config, attempts)).await;
        }
        attempts += 1;
        let request = Outgoing {
            url: sub.url.clone(),
            headers: vec![
                ("content-type".to_owned(), "application/json".to_owned()),
                ("x-hookd-id".to_owned(), event.id.clone()),
                ("x-hookd-event".to_owned(), event.kind.clone()),
                ("x-hookd-attempt".to_owned(), attempts.to_string()),
                (
                    "x-hookd-signature".to_owned(),
                    signature(&config.signing_key, &event.body),
                ),
            ],
            body: event.body.clone(),
        };
        let status = match timeout(config.attempt_timeout, inner.transport.send(request)).await {
            Ok(Ok(status)) => Some(status),
            Ok(Err(_)) | Err(_) => None,
        };
        if status.is_some_and(|s| (200..300).contains(&s)) {
            inner.state().stats.delivered += 1;
            return;
        }
        inner.state().stats.failed_attempts += 1;
        last_status = status;
        if status.is_some_and(|s| s != 429 && s < 500) {
            break;
        }
    }
    let mut state = inner.state();
    state.stats.dead += 1;
    state.dead.push((
        sub.tenant.clone(),
        DeadLetter {
            event_id: event.id.clone(),
            subscription: sub.id.clone(),
            attempts,
            last_status,
        },
    ));
}
