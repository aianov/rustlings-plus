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

pub(crate) fn open<T: DeserializeOwned>(message: &async_nats::Message) -> Result<Envelope<T>, BusError> {
    let subject = Subject::parse(message.subject.as_str()).map_err(|e| BusError::Decode(e.to_string()))?;
    let body = serde_json::from_slice(&message.payload).map_err(|e| BusError::Decode(e.to_string()))?;
    Ok(Envelope { subject, meta: Meta::from_headers(message.headers.as_ref()), body })
}

impl Bus {
    pub async fn connect(config: BusConfig) -> Result<Bus, BusError> {
        let slow = Arc::new(AtomicUsize::new(0));
        let counter = slow.clone();
        let client = ConnectOptions::new()
            .subscription_capacity(config.subscription_capacity)
            .reconnect_delay_callback(|attempt| Duration::from_millis((50 * attempt as u64).min(500)))
            .event_callback(move |event| {
                let counter = counter.clone();
                async move {
                    if let Event::SlowConsumer(_) = event {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
            .connect(config.url.as_str())
            .await
            .map_err(transport)?;
        Ok(Bus { client, slow })
    }

    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    pub fn ready(&self) -> bool {
        self.client.connection_state() == async_nats::connection::State::Connected
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

    pub async fn publish<T: Serialize>(&self, subject: &Subject, body: &T, meta: &Meta) -> Result<(), BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let payload = serde_json::to_vec(body).map_err(|e| BusError::Encode(e.to_string()))?;
        self.client
            .publish_with_headers(subject.as_str().to_string(), meta.to_headers(), payload.into())
            .await
            .map_err(transport)
    }

    pub async fn subscribe<T: DeserializeOwned>(&self, pattern: &Pattern) -> Result<Subscription<T>, BusError> {
        let inner = self.client.subscribe(pattern.as_str().to_string()).await.map_err(transport)?;
        self.sync().await?;
        Ok(Subscription { inner, client: self.client.clone(), marker: PhantomData })
    }

    pub async fn subscribe_group<T: DeserializeOwned>(&self, pattern: &Pattern, group: &str) -> Result<Subscription<T>, BusError> {
        let group = token(group).map_err(|e| BusError::Transport(e.to_string()))?;
        let inner = self.client.queue_subscribe(pattern.as_str().to_string(), group.to_string()).await.map_err(transport)?;
        self.sync().await?;
        Ok(Subscription { inner, client: self.client.clone(), marker: PhantomData })
    }

    pub async fn request<Q: Serialize, A: DeserializeOwned>(&self, subject: &Subject, body: &Q, meta: &Meta, deadline: Duration) -> Result<A, BusError> {
        if !self.ready() {
            return Err(BusError::Unavailable);
        }
        let payload = serde_json::to_vec(body).map_err(|e| BusError::Encode(e.to_string()))?;
        let request = Request::new().payload(payload.into()).headers(meta.to_headers()).timeout(Some(deadline));
        let reply = self.client.send_request(subject.as_str().to_string(), request).await.map_err(|error| match error.kind() {
            RequestErrorKind::NoResponders => BusError::NoService,
            RequestErrorKind::TimedOut => BusError::Timeout,
            RequestErrorKind::Other => transport(error),
        })?;
        match serde_json::from_slice::<Reply<A>>(&reply.payload) {
            Ok(Reply::Ok(answer)) => Ok(answer),
            Ok(Reply::Err(text)) => Err(BusError::Remote(text)),
            Err(e) => Err(BusError::Decode(e.to_string())),
        }
    }

    pub async fn serve<Q, A, H>(&self, pattern: &Pattern, group: Option<&str>, handler: H) -> Result<Server, BusError>
    where
        Q: DeserializeOwned + Send + 'static,
        A: Serialize + Send + 'static,
        H: Fn(Envelope<Q>) -> Result<A, String> + Send + Sync + 'static,
    {
        let mut requests = match group {
            Some(group) => {
                let group = token(group).map_err(|e| BusError::Transport(e.to_string()))?;
                self.client.queue_subscribe(pattern.as_str().to_string(), group.to_string()).await.map_err(transport)?
            }
            None => self.client.subscribe(pattern.as_str().to_string()).await.map_err(transport)?,
        };
        self.sync().await?;
        let client = self.client.clone();
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                let message = tokio::select! {
                    message = requests.next() => message,
                    _ = &mut stopped => None,
                };
                let Some(message) = message else { break };
                let Some(reply_to) = message.reply.clone() else { continue };
                let reply: Reply<A> = match open::<Q>(&message) {
                    Ok(envelope) => match handler(envelope) {
                        Ok(answer) => Reply::Ok(answer),
                        Err(text) => Reply::Err(text),
                    },
                    Err(error) => Reply::Err(error.to_string()),
                };
                let Ok(payload) = serde_json::to_vec(&reply) else { continue };
                let meta = Meta::from_headers(message.headers.as_ref());
                let sent = match meta {
                    Some(meta) => client.publish_with_headers(reply_to, meta.to_headers(), payload.into()).await,
                    None => client.publish(reply_to, payload.into()).await,
                };
                sent.ok();
            }
            requests.unsubscribe().await.ok();
            round_trip(&client).await.ok();
        });
        Ok(Server { stop, task })
    }

    pub async fn close(&self) -> Result<(), BusError> {
        self.client.flush().await.map_err(transport)
    }
}

impl<T: DeserializeOwned> Subscription<T> {
    pub async fn next(&mut self) -> Option<Result<Envelope<T>, BusError>> {
        let message = self.inner.next().await?;
        Some(open(&message))
    }

    pub async fn next_within(&mut self, wait: Duration) -> Option<Result<Envelope<T>, BusError>> {
        tokio::time::timeout(wait, self.next()).await.ok().flatten()
    }

    pub async fn stop(&mut self) {
        self.inner.unsubscribe().await.ok();
        round_trip(&self.client).await.ok();
    }
}

impl Server {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}
