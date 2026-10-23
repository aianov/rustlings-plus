use std::marker::PhantomData;
use std::time::Duration;

use async_nats::{Client, Message, Subscriber};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use tokio::time::timeout;

use super::round_trip;
use crate::error::BusError;
use crate::meta::Meta;
use crate::subject::Subject;

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<T> {
    pub subject: Subject,
    pub meta: Option<Meta>,
    pub body: T,
}

pub struct Subscription<T> {
    inner: Subscriber,
    client: Client,
    marker: PhantomData<fn() -> T>,
}

pub(crate) fn open<T: DeserializeOwned>(message: &Message) -> Result<Envelope<T>, BusError> {
    let subject = Subject::parse(message.subject.as_str())
        .map_err(|error| BusError::Decode(error.to_string()))?;
    let body = serde_json::from_slice(&message.payload)
        .map_err(|error| BusError::Decode(error.to_string()))?;
    Ok(Envelope {
        subject,
        meta: Meta::from_headers(message.headers.as_ref()),
        body,
    })
}

impl<T: DeserializeOwned> Subscription<T> {
    pub(crate) fn new(inner: Subscriber, client: Client) -> Subscription<T> {
        Subscription {
            inner,
            client,
            marker: PhantomData,
        }
    }

    pub async fn next(&mut self) -> Option<Result<Envelope<T>, BusError>> {
        let message = self.inner.next().await?;
        Some(open(&message))
    }

    pub async fn next_within(&mut self, wait: Duration) -> Option<Result<Envelope<T>, BusError>> {
        timeout(wait, self.next()).await.ok().flatten()
    }

    pub async fn stop(&mut self) {
        let _ = self.inner.unsubscribe().await;
        let _ = round_trip(&self.client).await;
    }
}
