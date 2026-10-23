use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::{Context, Message};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use service_runtime::{TraceContext, with_trace};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep};
use tracing::Instrument;

use super::context;
use super::dead::dead_letter;
use crate::bus::{Bus, Envelope, open};
use crate::error::{BusError, transport};
use crate::meta::Meta;
use crate::subject::{Pattern, Subject};

const FETCH_WAIT: Duration = Duration::from_millis(300);

const IDLE: Duration = Duration::from_millis(100);

const DEAD_RETRY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Done,
    Retry,
    Poison(String),
}

pub type Handler<T> =
    Arc<dyn Fn(Envelope<T>) -> Pin<Box<dyn Future<Output = Verdict> + Send>> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct FeedSpec {
    pub stream: String,
    pub durable: String,
    pub filter: Pattern,
    pub dead: Subject,
    pub ack_wait: Duration,
    pub batch: usize,
    pub parallel: usize,
    pub max_deliver: i64,
    pub backoff: Vec<Duration>,
}

impl FeedSpec {
    pub fn new(stream: &str, durable: &str, filter: Pattern, dead: Subject) -> FeedSpec {
        FeedSpec {
            stream: stream.to_string(),
            durable: durable.to_string(),
            filter,
            dead,
            ack_wait: Duration::from_secs(5),
            batch: 32,
            parallel: 4,
            max_deliver: 5,
            backoff: vec![
                Duration::from_millis(100),
                Duration::from_millis(200),
                Duration::from_millis(400),
                Duration::from_millis(800),
            ],
        }
    }

    pub fn pause(&self, delivered: i64) -> Duration {
        let step = usize::try_from(delivered.saturating_sub(1)).unwrap_or(0);
        match self.backoff.get(step).or(self.backoff.last()) {
            Some(pause) => *pause,
            None => Duration::ZERO,
        }
    }
}

pub struct Feed {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub async fn open_feed<T>(bus: &Bus, spec: FeedSpec, handler: Handler<T>) -> Result<Feed, BusError>
where
    T: DeserializeOwned + Send + 'static,
{
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let js = context(bus);
    let stream = js.get_stream(&spec.stream).await.map_err(transport)?;
    let consumer = stream
        .get_or_create_consumer(
            &spec.durable,
            pull::Config {
                durable_name: Some(spec.durable.clone()),
                ack_policy: AckPolicy::Explicit,
                ack_wait: spec.ack_wait,
                max_deliver: spec.max_deliver,
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
            let fetched = consumer
                .fetch()
                .max_messages(spec.batch.max(1))
                .expires(FETCH_WAIT)
                .messages()
                .await;
            let Ok(batch) = fetched else {
                sleep(IDLE).await;
                continue;
            };
            let messages: Vec<Message> =
                batch.filter_map(|item| async { item.ok() }).collect().await;
            futures::stream::iter(messages)
                .for_each_concurrent(spec.parallel.max(1), |message| {
                    settle(&js, &spec, &handler, message)
                })
                .await;
        }
    });
    Ok(Feed { stop, task })
}

async fn settle<T>(js: &Context, spec: &FeedSpec, handler: &Handler<T>, message: Message)
where
    T: DeserializeOwned + Send + 'static,
{
    let delivered = message.info().map(|info| info.delivered).unwrap_or(1);
    let envelope = match open::<T>(&message.message) {
        Ok(envelope) => envelope,
        Err(error) => return give_up(js, spec, &message, &error.to_string()).await,
    };
    let trace = envelope
        .meta
        .as_ref()
        .map_or_else(TraceContext::fresh, Meta::trace);
    let span = tracing::info_span!(
        "event",
        subject = %envelope.subject.as_str(),
        trace_id = %trace.trace_id,
        delivered,
    );
    let mut work = Box::pin(with_trace(trace, handler(envelope)).instrument(span));
    let beat = (spec.ack_wait / 3).max(Duration::from_millis(10));
    let mut progress = interval_at(Instant::now() + beat, beat);
    progress.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let verdict = loop {
        tokio::select! {
            verdict = &mut work => break verdict,
            _ = progress.tick() => {
                let _ = message.ack_with(AckKind::Progress).await;
            }
        }
    };
    match verdict {
        Verdict::Done => {
            let _ = message.ack().await;
        }
        Verdict::Retry if delivered >= spec.max_deliver => {
            give_up(js, spec, &message, "исчерпаны попытки обработки").await;
        }
        Verdict::Retry => {
            let _ = message
                .ack_with(AckKind::Nak(Some(spec.pause(delivered))))
                .await;
        }
        Verdict::Poison(reason) => give_up(js, spec, &message, &reason).await,
    }
}

async fn give_up(js: &Context, spec: &FeedSpec, message: &Message, reason: &str) {
    match dead_letter(js, message, &spec.dead, reason).await {
        Ok(()) => {
            let _ = message.ack_with(AckKind::Term).await;
        }
        Err(..) => {
            let _ = message.ack_with(AckKind::Nak(Some(DEAD_RETRY))).await;
        }
    }
}

impl Feed {
    pub async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
    }
}
