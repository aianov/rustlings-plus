use crate::meta::Meta;
use crate::subject::{Pattern, Subject, token};
use async_nats::client::RequestErrorKind;
use async_nats::{Client, ConnectOptions, Event, Request, Subscriber};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusError {
    Unavailable,
    NoService,
    Timeout,
    Encode(String),
    Decode(String),
    Remote(String),
    Transport(String),
}

impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusError::Unavailable => write!(f, "нет связи с брокером"),
            BusError::NoService => write!(f, "на тему никто не подписан"),
            BusError::Timeout => write!(f, "ответ не пришёл вовремя"),
            BusError::Encode(e) => write!(f, "не удалось собрать JSON: {e}"),
            BusError::Decode(e) => write!(f, "не удалось разобрать JSON: {e}"),
            BusError::Remote(e) => write!(f, "сервис ответил ошибкой: {e}"),
            BusError::Transport(e) => write!(f, "ошибка брокера: {e}"),
        }
    }
}

impl std::error::Error for BusError {}

#[derive(Debug, Clone)]
pub struct BusConfig {
    pub url: String,
    pub subscription_capacity: usize,
}

impl BusConfig {
    pub fn new(url: impl Into<String>) -> BusConfig {
        BusConfig { url: url.into(), subscription_capacity: 4096 }
    }
}

pub struct Bus {
    client: Client,
    slow: Arc<AtomicUsize>,
}

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

pub struct Server {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

#[derive(Serialize, Deserialize)]
enum Reply<A> {
    Ok(A),
    Err(String),
}

fn transport(error: impl std::fmt::Display) -> BusError {
    BusError::Transport(error.to_string())
}

async fn round_trip(client: &Client) -> Result<(), BusError> {
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(transport)?;
    client.publish(inbox, "".into()).await.map_err(transport)?;
    probe.next().await.map(|_| ()).ok_or_else(|| transport("соединение закрылось"))
}

fn open<T: DeserializeOwned>(message: &async_nats::Message) -> Result<Envelope<T>, BusError> {
    let subject = Subject::parse(message.subject.as_str()).map_err(|e| BusError::Decode(e.to_string()))?;
    let body = serde_json::from_slice(&message.payload).map_err(|e| BusError::Decode(e.to_string()))?;
    Ok(Envelope { subject, meta: Meta::from_headers(message.headers.as_ref()), body })
}

impl Bus {
    pub async fn connect(config: BusConfig) -> Result<Bus, BusError> {
        todo!()
    }

    pub fn ready(&self) -> bool {
        todo!()
    }

    pub fn slow_consumer_events(&self) -> usize {
        self.slow.load(Ordering::SeqCst)
    }

    pub async fn sync(&self) -> Result<(), BusError> {
        todo!()
    }

    pub async fn publish<T: Serialize>(&self, subject: &Subject, body: &T, meta: &Meta) -> Result<(), BusError> {
        todo!()
    }

    pub async fn subscribe<T: DeserializeOwned>(&self, pattern: &Pattern) -> Result<Subscription<T>, BusError> {
        todo!()
    }

    pub async fn subscribe_group<T: DeserializeOwned>(&self, pattern: &Pattern, group: &str) -> Result<Subscription<T>, BusError> {
        todo!()
    }

    pub async fn request<Q: Serialize, A: DeserializeOwned>(&self, subject: &Subject, body: &Q, meta: &Meta, deadline: Duration) -> Result<A, BusError> {
        todo!()
    }

    pub async fn serve<Q, A, H>(&self, pattern: &Pattern, group: Option<&str>, handler: H) -> Result<Server, BusError>
    where
        Q: DeserializeOwned + Send + 'static,
        A: Serialize + Send + 'static,
        H: Fn(Envelope<Q>) -> Result<A, String> + Send + Sync + 'static,
    {
        todo!()
    }

    pub async fn close(&self) -> Result<(), BusError> {
        todo!()
    }
}

impl<T: DeserializeOwned> Subscription<T> {
    pub async fn next(&mut self) -> Option<Result<Envelope<T>, BusError>> {
        todo!()
    }

    pub async fn next_within(&mut self, wait: Duration) -> Option<Result<Envelope<T>, BusError>> {
        tokio::time::timeout(wait, self.next()).await.ok().flatten()
    }

    pub async fn stop(&mut self) {
        todo!()
    }
}

impl Server {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}
