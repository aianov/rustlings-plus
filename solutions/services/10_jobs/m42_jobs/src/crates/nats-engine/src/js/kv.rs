use std::time::Duration;

use async_nats::jetstream::consumer::pull::Config as ReaderConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, PullConsumer};
use async_nats::jetstream::kv::{Config, CreateErrorKind, Operation, Store, UpdateErrorKind};
use async_nats::jetstream::stream::Stream;
use futures::StreamExt;

use super::context;
use crate::bus::Bus;
use crate::error::{BusError, transport};

const OPERATION: &str = "KV-Operation";

const FETCH_WAIT: Duration = Duration::from_secs(1);

const IDLE: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Bucket {
    store: Store,
    stream: Stream,
    name: String,
}

pub async fn open_bucket(bus: &Bus, name: &str, ttl: Duration) -> Result<Bucket, BusError> {
    if !bus.ready() {
        return Err(BusError::Unavailable);
    }
    let js = context(bus);
    let store = match js.get_key_value(name).await {
        Ok(store) => store,
        Err(..) => js
            .create_key_value(Config {
                bucket: name.to_string(),
                history: 1,
                max_age: ttl,
                ..Default::default()
            })
            .await
            .map_err(transport)?,
    };
    let stream = js
        .get_stream(format!("KV_{name}"))
        .await
        .map_err(transport)?;
    Ok(Bucket {
        store,
        stream,
        name: name.to_string(),
    })
}

impl Bucket {
    pub async fn touch(&self, key: &str) -> Result<(), BusError> {
        self.store
            .put(key, bytes::Bytes::new())
            .await
            .map(|_| ())
            .map_err(transport)
    }

    pub async fn forget(&self, key: &str) -> Result<(), BusError> {
        self.store.delete(key).await.map_err(transport)
    }

    pub async fn alive(&self, prefix: &str) -> Result<usize, BusError> {
        let consumer: PullConsumer = self
            .stream
            .create_consumer(ReaderConfig {
                filter_subject: format!("$KV.{}.{prefix}.*", self.name),
                deliver_policy: DeliverPolicy::LastPerSubject,
                ack_policy: AckPolicy::None,
                headers_only: true,
                inactive_threshold: IDLE,
                ..Default::default()
            })
            .await
            .map_err(transport)?;
        let name = consumer.cached_info().name.clone();
        let counted = count(&consumer).await;
        let _ = self.stream.delete_consumer(&name).await;
        counted
    }
}

async fn count(consumer: &PullConsumer) -> Result<usize, BusError> {
    let pending = usize::try_from(consumer.cached_info().num_pending).unwrap_or(usize::MAX);
    if pending == 0 {
        return Ok(0);
    }
    let mut batch = consumer
        .fetch()
        .max_messages(pending)
        .expires(FETCH_WAIT)
        .messages()
        .await
        .map_err(transport)?;
    let mut alive = 0;
    while let Some(message) = batch.next().await {
        let message = message.map_err(transport)?;
        let operation = message
            .headers
            .as_ref()
            .and_then(|headers| headers.get(OPERATION))
            .map(|value| value.as_str().to_string());
        if operation.is_none_or(|operation| operation == "PUT") {
            alive += 1;
        }
    }
    Ok(alive)
}

impl Bucket {
    pub async fn put(&self, key: &str, value: &[u8]) -> Result<u64, BusError> {
        self.store
            .put(key, bytes::Bytes::copy_from_slice(value))
            .await
            .map_err(transport)
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, BusError> {
        match self.store.entry(key).await.map_err(transport)? {
            Some(entry) if entry.operation == Operation::Put => Ok(Some(entry.value.to_vec())),
            Some(..) | None => Ok(None),
        }
    }

    pub async fn claim(&self, key: &str, value: &[u8]) -> Result<Option<u64>, BusError> {
        match self
            .store
            .create(key, bytes::Bytes::copy_from_slice(value))
            .await
        {
            Ok(revision) => Ok(Some(revision)),
            Err(error) if error.kind() == CreateErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(transport(error)),
        }
    }

    pub async fn renew(
        &self,
        key: &str,
        value: &[u8],
        revision: u64,
    ) -> Result<Option<u64>, BusError> {
        match self
            .store
            .update(key, bytes::Bytes::copy_from_slice(value), revision)
            .await
        {
            Ok(revision) => Ok(Some(revision)),
            Err(error) if error.kind() == UpdateErrorKind::WrongLastRevision => Ok(None),
            Err(error) => Err(transport(error)),
        }
    }

    pub async fn release(&self, key: &str, revision: u64) -> Result<bool, BusError> {
        match self.store.delete_expect_revision(key, Some(revision)).await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == UpdateErrorKind::WrongLastRevision => Ok(false),
            Err(error) => Err(transport(error)),
        }
    }
}
