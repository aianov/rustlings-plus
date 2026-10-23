use crate::bus::{Bus, BusError, Envelope, open};
use crate::meta::Meta;
use crate::subject::{Pattern, Subject};
use async_nats::HeaderMap;
use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::stream::Config;
use async_nats::jetstream::{self, Context, Message};
use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub const MAX_DELIVER: i64 = 5;
pub const PUBLISH_ATTEMPTS: u32 = 3;

pub fn backoff() -> Vec<Duration> {
    vec![Duration::from_millis(100), Duration::from_millis(200), Duration::from_millis(400), Duration::from_millis(800)]
}

#[derive(Debug, Clone)]
pub struct StreamSpec {
    pub name: String,
    pub subjects: Vec<String>,
    pub duplicate_window: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub sequence: u64,
    pub duplicate: bool,
}

#[derive(Debug, Clone)]
pub struct FeedSpec {
    pub stream: String,
    pub durable: String,
    pub filter: Pattern,
    pub ack_wait: Duration,
    pub batch: usize,
    pub dead_subject: Subject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Done,
    Retry,
    Poison(String),
}

pub type Handler<T> = Arc<dyn Fn(Envelope<T>) -> Pin<Box<dyn Future<Output = Verdict> + Send>> + Send + Sync>;

pub struct Feed {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

fn transport(error: impl std::fmt::Display) -> BusError {
    BusError::Transport(error.to_string())
}

pub async fn ensure_stream(bus: &Bus, spec: &StreamSpec) -> Result<(), BusError> {
    todo!()
}

pub async fn publish_event<T: Serialize>(bus: &Bus, subject: &Subject, event_id: &str, body: &T, meta: &Meta) -> Result<Published, BusError> {
    todo!()
}

pub(crate) async fn dead_letter(js: &Context, message: &Message, dead_subject: &Subject, reason: &str) -> Result<(), BusError> {
    todo!()
}

pub async fn open_feed<T>(bus: &Bus, spec: FeedSpec, handler: Handler<T>) -> Result<Feed, BusError>
where
    T: DeserializeOwned + Send + 'static,
{
    todo!()
}

impl Feed {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}
