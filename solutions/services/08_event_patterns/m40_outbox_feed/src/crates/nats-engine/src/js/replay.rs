use std::marker::PhantomData;

use async_nats::jetstream::consumer::pull::{Ordered, OrderedConfig};
use futures::StreamExt;
use serde::de::DeserializeOwned;

use super::context;
use crate::bus::{Bus, Envelope, open};
use crate::error::{BusError, transport};
use crate::subject::Pattern;

#[derive(Debug, Clone, PartialEq)]
pub struct Replayed<T> {
    pub sequence: u64,
    pub pending: u64,
    pub item: Result<Envelope<T>, BusError>,
}

pub struct Replay<T> {
    messages: Ordered,
    pending: u64,
    marker: PhantomData<fn() -> T>,
}

pub async fn last_sequence(bus: &Bus, stream: &str) -> Result<u64, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let mut stream = context(bus).get_stream(stream).await.map_err(transport)?;
    let info = stream.info().await.map_err(transport)?;
    Ok(info.state.last_sequence)
}

pub async fn replay<T: DeserializeOwned>(
    bus: &Bus,
    stream: &str,
    filter: &Pattern,
) -> Result<Replay<T>, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let stream = context(bus).get_stream(stream).await.map_err(transport)?;
    let consumer = stream
        .create_consumer(OrderedConfig {
            filter_subject: filter.as_str().to_string(),
            ..Default::default()
        })
        .await
        .map_err(transport)?;
    let pending = consumer.cached_info().num_pending;
    let messages = consumer.messages().await.map_err(transport)?;
    Ok(Replay {
        messages,
        pending,
        marker: PhantomData,
    })
}

impl<T: DeserializeOwned> Replay<T> {
    pub fn pending_at_start(&self) -> u64 {
        self.pending
    }

    pub async fn next(&mut self) -> Option<Result<Replayed<T>, BusError>> {
        let message = match self.messages.next().await? {
            Ok(message) => message,
            Err(error) => return Some(Err(transport(error))),
        };
        let (sequence, pending) = match message.info() {
            Ok(info) => (info.stream_sequence, info.pending),
            Err(error) => return Some(Err(transport(error))),
        };
        Some(Ok(Replayed {
            sequence,
            pending,
            item: open(&message.message),
        }))
    }
}
