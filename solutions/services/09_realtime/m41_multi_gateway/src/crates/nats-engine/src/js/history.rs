use std::time::Duration;

use async_nats::jetstream::consumer::pull::Config;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, PullConsumer};
use futures::StreamExt;
use serde::de::DeserializeOwned;

use super::context;
use super::live::{Stored, stored};
use crate::bus::Bus;
use crate::error::{BusError, transport};
use crate::subject::Subject;

const FETCH_WAIT: Duration = Duration::from_secs(1);

const IDLE: Duration = Duration::from_secs(30);

pub async fn history<T: DeserializeOwned>(
    bus: &Bus,
    stream: &str,
    subject: &Subject,
    after: u64,
    limit: usize,
) -> Result<Vec<Stored<T>>, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let stream = context(bus).get_stream(stream).await.map_err(transport)?;
    let consumer: PullConsumer = stream
        .create_consumer(Config {
            filter_subject: subject.as_str().to_string(),
            deliver_policy: DeliverPolicy::ByStartSequence {
                start_sequence: after.saturating_add(1),
            },
            ack_policy: AckPolicy::None,
            inactive_threshold: IDLE,
            ..Default::default()
        })
        .await
        .map_err(transport)?;
    let name = consumer.cached_info().name.clone();
    let read = read(&consumer, limit).await;
    let _ = stream.delete_consumer(&name).await;
    read
}

async fn read<T: DeserializeOwned>(
    consumer: &PullConsumer,
    limit: usize,
) -> Result<Vec<Stored<T>>, BusError> {
    let pending = usize::try_from(consumer.cached_info().num_pending).unwrap_or(usize::MAX);
    let wanted = pending.min(limit);
    let mut found = Vec::with_capacity(wanted);
    if wanted == 0 {
        return Ok(found);
    }
    let mut batch = consumer
        .fetch()
        .max_messages(wanted)
        .expires(FETCH_WAIT)
        .messages()
        .await
        .map_err(transport)?;
    while let Some(message) = batch.next().await {
        let message = message.map_err(transport)?;
        let info = message.info().map_err(transport)?;
        found.push(stored(info.stream_sequence, &message.message));
    }
    Ok(found)
}
