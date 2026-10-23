mod server;
mod subscription;

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_nats::client::RequestErrorKind;
use async_nats::connection::State;
use async_nats::{Client, ConnectOptions, Event, Request};
use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::BusConfig;
use crate::error::{BusError, transport};
use crate::meta::Meta;
use crate::subject::{Pattern, Subject, token};

pub use server::Server;
pub use subscription::{Envelope, Subscription};

pub(crate) use subscription::open;

use server::Reply;

const RECONNECT_STEP: u64 = 50;

const RECONNECT_MAX: u64 = 200;

#[derive(Clone)]
pub struct Bus {
    client: Client,
    slow: Arc<AtomicUsize>,
}

pub(crate) async fn round_trip(client: &Client) -> Result<(), BusError> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(transport)?;
    client
        .publish(inbox, bytes::Bytes::new())
        .await
        .map_err(transport)?;
    match probe.next().await {
        Some(_) => Ok(()),
        None => Err(transport("соединение закрылось")),
    }
}

fn group_name(group: &str) -> Result<String, BusError> {
    token(group)
        .map(str::to_string)
        .map_err(|error| BusError::Transport(format!("группа {group:?}: {error}")))
}

impl Bus {
    pub async fn connect(config: BusConfig) -> Result<Bus, BusError> {
        let slow = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&slow);
        let mut options = ConnectOptions::new()
            .subscription_capacity(config.subscription_capacity)
            .connection_timeout(config.connect_timeout)
            .reconnect_delay_callback(|attempt| {
                Duration::from_millis((RECONNECT_STEP * attempt as u64).min(RECONNECT_MAX))
            })
            .event_callback(move |event| {
                let counter = Arc::clone(&counter);
                async move {
                    if let Event::SlowConsumer(_) = event {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        if let Some(name) = &config.name {
            options = options.name(name);
        }
        let client = options
            .connect(config.url.as_str())
            .await
            .map_err(|error| BusError::Transport(format!("{}: {error}", config.url)))?;
        Ok(Bus { client, slow })
    }

    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    pub fn ready(&self) -> bool {
        self.client.connection_state() == State::Connected
    }

    pub fn slow_consumer_events(&self) -> usize {
        self.slow.load(Ordering::SeqCst)
    }

    pub async fn sync(&self) -> Result<(), BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        round_trip(&self.client).await
    }

    pub async fn publish<T: Serialize>(
        &self,
        subject: &Subject,
        body: &T,
        meta: &Meta,
    ) -> Result<(), BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let payload =
            serde_json::to_vec(body).map_err(|error| BusError::Encode(error.to_string()))?;
        self.client
            .publish_with_headers(
                subject.as_str().to_string(),
                meta.to_headers(),
                payload.into(),
            )
            .await
            .map_err(transport)
    }

    pub async fn subscribe<T: DeserializeOwned>(
        &self,
        pattern: &Pattern,
    ) -> Result<Subscription<T>, BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let inner = self
            .client
            .subscribe(pattern.as_str().to_string())
            .await
            .map_err(transport)?;
        self.sync().await?;
        Ok(Subscription::new(inner, self.client.clone()))
    }

    pub async fn subscribe_group<T: DeserializeOwned>(
        &self,
        pattern: &Pattern,
        group: &str,
    ) -> Result<Subscription<T>, BusError> {
        let group = group_name(group)?;
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let inner = self
            .client
            .queue_subscribe(pattern.as_str().to_string(), group)
            .await
            .map_err(transport)?;
        self.sync().await?;
        Ok(Subscription::new(inner, self.client.clone()))
    }

    pub async fn request<Q: Serialize, A: DeserializeOwned>(
        &self,
        subject: &Subject,
        body: &Q,
        meta: &Meta,
        deadline: Duration,
    ) -> Result<A, BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let payload =
            serde_json::to_vec(body).map_err(|error| BusError::Encode(error.to_string()))?;
        let request = Request::new()
            .payload(payload.into())
            .headers(meta.to_headers())
            .timeout(Some(deadline));
        let reply = self
            .client
            .send_request(subject.as_str().to_string(), request)
            .await
            .map_err(|error| match error.kind() {
                RequestErrorKind::NoResponders => BusError::NoService,
                RequestErrorKind::TimedOut => BusError::Timeout,
                RequestErrorKind::Other => transport(error),
            })?;
        match serde_json::from_slice::<Reply<A>>(&reply.payload) {
            Ok(Reply::Ok(answer)) => Ok(answer),
            Ok(Reply::Err(text)) => Err(BusError::Remote(text)),
            Err(error) => Err(BusError::Decode(error.to_string())),
        }
    }

    pub async fn serve<Q, A, H, F>(
        &self,
        pattern: &Pattern,
        group: Option<&str>,
        handler: H,
    ) -> Result<Server, BusError>
    where
        Q: DeserializeOwned + Send + 'static,
        A: Serialize + Send + 'static,
        H: Fn(Envelope<Q>) -> F + Send + Sync + 'static,
        F: Future<Output = Result<A, String>> + Send + 'static,
    {
        let group = group.map(group_name).transpose()?;
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let requests = match group {
            Some(group) => self
                .client
                .queue_subscribe(pattern.as_str().to_string(), group)
                .await
                .map_err(transport)?,
            None => self
                .client
                .subscribe(pattern.as_str().to_string())
                .await
                .map_err(transport)?,
        };
        self.sync().await?;
        Ok(Server::start(self.client.clone(), requests, handler))
    }

    pub async fn close(&self) -> Result<(), BusError> {
        self.client.flush().await.map_err(transport)
    }
}
