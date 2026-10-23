use crate::bus::{Bus, BusError, Envelope, open};
use crate::meta::Meta;
use crate::subject::{Pattern, Subject};
use async_nats::HeaderMap;
use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::publish::PublishAck;
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
    let js = jetstream::new(bus.client().clone());
    let config = Config { name: spec.name.clone(), subjects: spec.subjects.clone(), duplicate_window: spec.duplicate_window, ..Default::default() };
    let mut stream = js.get_or_create_stream(config.clone()).await.map_err(transport)?;
    let existing = stream.info().await.map_err(transport)?;
    if existing.config.subjects != spec.subjects {
        js.update_stream(config).await.map_err(transport)?;
    }
    Ok(())
}

pub async fn publish_event<T: Serialize>(bus: &Bus, subject: &Subject, event_id: &str, body: &T, meta: &Meta) -> Result<Published, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let js = jetstream::new(bus.client().clone());
    let payload = serde_json::to_vec(body).map_err(|e| BusError::Encode(e.to_string()))?;
    let mut headers: HeaderMap = meta.to_headers();
    headers.insert("Nats-Msg-Id", event_id);
    let mut last = BusError::Timeout;
    for attempt in 1..=PUBLISH_ATTEMPTS {
        let sent = js.publish_with_headers(subject.as_str().to_string(), headers.clone(), payload.clone().into()).await;
        let acked: Result<PublishAck, async_nats::jetstream::context::PublishError> = match sent {
            Ok(future) => future.await,
            Err(error) => Err(error),
        };
        match acked {
            Ok(ack) => return Ok(Published { sequence: ack.sequence, duplicate: ack.duplicate }),
            Err(error) if error.kind() == async_nats::jetstream::context::PublishErrorKind::TimedOut => last = BusError::Timeout,
            Err(error) => return Err(transport(error)),
        }
        if attempt < PUBLISH_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    Err(last)
}

pub(crate) async fn dead_letter(js: &Context, message: &Message, dead_subject: &Subject, reason: &str) -> Result<(), BusError> {
    let delivered = message.info().map(|i| i.delivered).unwrap_or(0);
    let mut headers = HeaderMap::new();
    headers.insert("X-Original-Subject", message.subject.as_str());
    headers.insert("X-Delivery-Count", delivered.to_string());
    headers.insert("X-Error", reason.replace(['\r', '\n'], " "));
    let ack = js.publish_with_headers(dead_subject.as_str().to_string(), headers, message.payload.clone()).await.map_err(transport)?;
    ack.await.map_err(transport)?;
    Ok(())
}

pub async fn open_feed<T>(bus: &Bus, spec: FeedSpec, handler: Handler<T>) -> Result<Feed, BusError>
where
    T: DeserializeOwned + Send + 'static,
{
    let js = jetstream::new(bus.client().clone());
    let stream = js.get_stream(&spec.stream).await.map_err(transport)?;
    let consumer = stream
        .get_or_create_consumer(
            &spec.durable,
            pull::Config {
                durable_name: Some(spec.durable.clone()),
                ack_policy: AckPolicy::Explicit,
                ack_wait: spec.ack_wait,
                max_deliver: MAX_DELIVER,
                filter_subject: spec.filter.as_str().to_string(),
                ..Default::default()
            },
        )
        .await
        .map_err(transport)?;
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        loop {
            if stopped.try_recv().is_ok() {
                return;
            }
            let fetched = consumer.fetch().max_messages(spec.batch).expires(Duration::from_millis(300)).messages().await;
            let Ok(mut batch) = fetched else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            while let Some(Ok(message)) = batch.next().await {
                settle(&js, &spec, &handler, message).await;
            }
        }
    });
    Ok(Feed { stop, task })
}

async fn settle<T>(js: &Context, spec: &FeedSpec, handler: &Handler<T>, message: Message)
where
    T: DeserializeOwned + Send + 'static,
{
    let delivered = message.info().map(|i| i.delivered).unwrap_or(1);
    let envelope = match open::<T>(&message.message) {
        Ok(envelope) => envelope,
        Err(error) => return give_up(js, spec, &message, &error.to_string()).await,
    };
    let work = handler(envelope);
    let mut work = std::pin::pin!(work);
    let mut ticker = tokio::time::interval(spec.ack_wait / 3);
    ticker.tick().await;
    let verdict = loop {
        tokio::select! {
            verdict = &mut work => break verdict,
            _ = ticker.tick() => {
                message.ack_with(AckKind::Progress).await.ok();
            }
        }
    };
    match verdict {
        Verdict::Done => {
            message.ack().await.ok();
        }
        Verdict::Retry if delivered >= MAX_DELIVER => give_up(js, spec, &message, "исчерпаны попытки обработки").await,
        Verdict::Retry => {
            let pauses = backoff();
            let pause = pauses[((delivered - 1).max(0) as usize).min(pauses.len() - 1)];
            message.ack_with(AckKind::Nak(Some(pause))).await.ok();
        }
        Verdict::Poison(reason) => give_up(js, spec, &message, &reason).await,
    }
}

async fn give_up(js: &Context, spec: &FeedSpec, message: &Message, reason: &str) {
    match dead_letter(js, message, &spec.dead_subject, reason).await {
        Ok(()) => {
            message.ack_with(AckKind::Term).await.ok();
        }
        Err(_) => {
            message.ack_with(AckKind::Nak(Some(Duration::from_secs(1)))).await.ok();
        }
    }
}

impl Feed {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}
