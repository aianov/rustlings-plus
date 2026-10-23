mod dead;
mod feed;
mod history;
mod kv;
mod live;
mod replay;

use std::time::Duration;

use async_nats::jetstream::context::PublishErrorKind;
use async_nats::jetstream::stream::{Config, Republish as Echo};
use async_nats::jetstream::{self, Context};
use serde::Serialize;

use crate::bus::Bus;
use crate::error::{BusError, transport};
use crate::meta::Meta;
use crate::subject::Subject;

pub use dead::{DELIVERY_COUNT, ERROR, ORIGINAL_SUBJECT};
pub use feed::{Feed, FeedSpec, Handler, Verdict, open_feed};
pub use history::history;
pub use kv::{Bucket, open_bucket};
pub use live::{Live, Stored, live};
pub use replay::{Replay, Replayed, last_sequence, replay};

pub const MSG_ID: &str = "Nats-Msg-Id";

pub const PUBLISH_ATTEMPTS: u32 = 3;

pub const ACK_TIMEOUT: Duration = Duration::from_secs(1);

const PUBLISH_PAUSE: Duration = Duration::from_millis(50);

const DUPLICATE_WINDOW: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Republish {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSpec {
    pub name: String,
    pub subjects: Vec<String>,
    pub duplicate_window: Duration,
    pub max_age: Duration,
    pub republish: Option<Republish>,
}

impl StreamSpec {
    pub fn new(name: &str, subjects: Vec<String>) -> StreamSpec {
        StreamSpec {
            name: name.to_string(),
            subjects,
            duplicate_window: DUPLICATE_WINDOW,
            max_age: Duration::ZERO,
            republish: None,
        }
    }

    pub fn republish(self, from: &str, to: &str) -> StreamSpec {
        StreamSpec {
            republish: Some(Republish {
                from: from.to_string(),
                to: to.to_string(),
            }),
            ..self
        }
    }
}

fn echo(spec: &StreamSpec) -> Option<Echo> {
    spec.republish.as_ref().map(|republish| Echo {
        source: republish.from.clone(),
        destination: republish.to.clone(),
        headers_only: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Published {
    pub sequence: u64,
    pub duplicate: bool,
}

pub(crate) fn context(bus: &Bus) -> Context {
    let mut context = jetstream::new(bus.client().clone());
    context.set_timeout(ACK_TIMEOUT);
    context
}

pub async fn ensure_stream(bus: &Bus, spec: &StreamSpec) -> Result<(), BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let js = context(bus);
    let config = Config {
        name: spec.name.clone(),
        subjects: spec.subjects.clone(),
        duplicate_window: spec.duplicate_window,
        max_age: spec.max_age,
        republish: echo(spec),
        ..Default::default()
    };
    let mut stream = js
        .get_or_create_stream(config.clone())
        .await
        .map_err(transport)?;
    let existing = stream.info().await.map_err(transport)?;
    let mut have = existing.config.subjects.clone();
    let mut want = spec.subjects.clone();
    have.sort();
    want.sort();
    if have != want || existing.config.republish != config.republish {
        js.update_stream(config).await.map_err(transport)?;
    }
    Ok(())
}

pub async fn publish_event<T: Serialize>(
    bus: &Bus,
    subject: &Subject,
    event_id: &str,
    body: &T,
    meta: &Meta,
) -> Result<Published, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let js = context(bus);
    let payload = serde_json::to_vec(body).map_err(|error| BusError::Encode(error.to_string()))?;
    let mut headers = meta.to_headers();
    headers.insert(async_nats::header::NATS_MESSAGE_ID, event_id);
    let mut last = BusError::Timeout;
    for attempt in 1..=PUBLISH_ATTEMPTS {
        let sent = js
            .publish_with_headers(
                subject.as_str().to_string(),
                headers.clone(),
                payload.clone().into(),
            )
            .await;
        let acked = match sent {
            Ok(ack) => ack.await,
            Err(error) => Err(error),
        };
        match acked {
            Ok(ack) => {
                return Ok(Published {
                    sequence: ack.sequence,
                    duplicate: ack.duplicate,
                });
            }
            Err(error) if error.kind() == PublishErrorKind::TimedOut => last = BusError::Timeout,
            Err(..) if !bus.ready() => last = BusError::Unavailable,
            Err(error) => return Err(transport(error)),
        }
        if attempt < PUBLISH_ATTEMPTS {
            tokio::time::sleep(PUBLISH_PAUSE).await;
        }
    }
    Err(last)
}
