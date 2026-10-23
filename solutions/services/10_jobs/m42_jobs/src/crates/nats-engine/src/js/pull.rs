use std::marker::PhantomData;
use std::time::Duration;

use async_nats::jetstream::Context;
use async_nats::jetstream::consumer::{AckPolicy, PullConsumer, pull};
use async_nats::jetstream::message::{AckKind, Message};
use futures::StreamExt;
use serde::de::DeserializeOwned;

use super::context;
use super::dead::dead_letter;
use crate::bus::{Bus, Envelope, open};
use crate::error::{BusError, transport};
use crate::subject::{Pattern, Subject};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullSpec {
    pub stream: String,
    pub durable: String,
    pub filter: Pattern,
    pub ack_wait: Duration,
}

pub struct Puller<T> {
    consumer: PullConsumer,
    js: Context,
    marker: PhantomData<fn() -> T>,
}

pub struct Taken<T> {
    message: Message,
    js: Context,
    pub delivered: u64,
    pub published_ms: u64,
    pub item: Result<Envelope<T>, BusError>,
}

pub async fn puller<T: DeserializeOwned>(
    bus: &Bus,
    spec: &PullSpec,
) -> Result<Puller<T>, BusError> {
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
                filter_subject: spec.filter.as_str().to_string(),
                ..Default::default()
            },
        )
        .await
        .map_err(transport)?;
    Ok(Puller {
        consumer,
        js,
        marker: PhantomData,
    })
}

impl<T: DeserializeOwned> Puller<T> {
    pub async fn take(&self, max: usize, wait: Duration) -> Result<Vec<Taken<T>>, BusError> {
        let batch = self
            .consumer
            .fetch()
            .max_messages(max.max(1))
            .expires(wait)
            .messages()
            .await
            .map_err(transport)?;
        let messages: Vec<Message> = batch.filter_map(|item| async { item.ok() }).collect().await;
        Ok(messages
            .into_iter()
            .map(|message| {
                let (delivered, published_ms) = message.info().map_or((1, 0), |info| {
                    let millis = info.published.unix_timestamp_nanos() / 1_000_000;
                    (
                        u64::try_from(info.delivered).unwrap_or(1),
                        u64::try_from(millis).unwrap_or(0),
                    )
                });
                Taken {
                    item: open(&message.message),
                    message,
                    js: self.js.clone(),
                    delivered,
                    published_ms,
                }
            })
            .collect())
    }
}

impl<T> Taken<T> {
    pub async fn ack(&self) -> Result<(), BusError> {
        self.message.double_ack().await.map_err(transport)
    }

    pub async fn nak(&self, delay: Duration) -> Result<(), BusError> {
        let delay = (!delay.is_zero()).then_some(delay);
        self.message
            .ack_with(AckKind::Nak(delay))
            .await
            .map_err(transport)
    }

    pub async fn progress(&self) -> Result<(), BusError> {
        self.message
            .ack_with(AckKind::Progress)
            .await
            .map_err(transport)
    }

    pub async fn bury(&self, dead: &Subject, reason: &str) -> Result<(), BusError> {
        dead_letter(&self.js, &self.message, dead, reason).await?;
        self.message
            .ack_with(AckKind::Term)
            .await
            .map_err(transport)
    }
}
