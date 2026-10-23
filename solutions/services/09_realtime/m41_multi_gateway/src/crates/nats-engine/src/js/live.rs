use std::marker::PhantomData;

use async_nats::header::NATS_SEQUENCE;
use async_nats::{Client, Message, Subscriber};
use futures::StreamExt;
use serde::de::DeserializeOwned;

use crate::bus::{Bus, Envelope, open, round_trip};
use crate::error::{BusError, transport};
use crate::subject::Subject;

#[derive(Debug, Clone, PartialEq)]
pub struct Stored<T> {
    pub sequence: u64,
    pub item: Result<Envelope<T>, BusError>,
}

pub struct Live<T> {
    inner: Subscriber,
    client: Client,
    marker: PhantomData<fn() -> T>,
}

pub(super) fn stored<T: DeserializeOwned>(sequence: u64, message: &Message) -> Stored<T> {
    Stored {
        sequence,
        item: open(message),
    }
}

fn sequence_of(message: &Message) -> Option<u64> {
    let headers = message.headers.as_ref()?;
    headers.get(NATS_SEQUENCE)?.as_str().parse().ok()
}

pub async fn live<T: DeserializeOwned>(bus: &Bus, subject: &Subject) -> Result<Live<T>, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let client = bus.client().clone();
    let inner = client
        .subscribe(subject.as_str().to_string())
        .await
        .map_err(transport)?;
    bus.sync().await?;
    Ok(Live {
        inner,
        client,
        marker: PhantomData,
    })
}

impl<T: DeserializeOwned> Live<T> {
    pub async fn next(&mut self) -> Option<Stored<T>> {
        let message = self.inner.next().await?;
        Some(match sequence_of(&message) {
            Some(sequence) => stored(sequence, &message),
            None => Stored {
                sequence: 0,
                item: Err(BusError::Decode(String::from(
                    "нет Nats-Sequence: сообщение пришло не из stream",
                ))),
            },
        })
    }

    pub async fn stop(&mut self) {
        let _ = self.inner.unsubscribe().await;
        let _ = round_trip(&self.client).await;
    }
}
