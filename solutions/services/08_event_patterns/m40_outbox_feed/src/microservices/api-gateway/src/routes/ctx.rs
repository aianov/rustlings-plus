use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use contracts::{ErrorBody, ErrorCode, Map, NoticeAsk, NotifyMethod, Push, UserId, Value};
use grpc_kit::Stamp;
use nats_engine::Meta;
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::feeds_client::FeedsClient;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;
use tonic::Status;

use crate::client::{Notify, Stamped, Upstreams, failure};
use crate::common::{Outbox, PART_BUDGET, Registry, Visitor};

#[derive(Default)]
pub struct Watching {
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Watching {
    fn lock(&self) -> MutexGuard<'_, Option<JoinHandle<()>>> {
        self.task.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn start(&self, task: JoinHandle<()>) -> bool {
        let old = self.lock().replace(task);
        old.map(|old| old.abort()).is_some()
    }

    pub fn stop(&self) -> bool {
        let old = self.lock().take();
        old.map(|old| old.abort()).is_some()
    }

    pub fn is_watching(&self) -> bool {
        self.lock().as_ref().is_some_and(|task| !task.is_finished())
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct Conn {
    outbox: Outbox,
    watching: Arc<Watching>,
}

#[derive(Clone)]
pub struct Ctx {
    upstreams: Upstreams,
    registry: Arc<Registry>,
    budget: Duration,
    parts: Duration,
    stamp: Stamp,
    conn: Option<Conn>,
    notify: Option<Notify>,
}

impl Ctx {
    pub fn new(upstreams: Upstreams, registry: Arc<Registry>, budget: Duration) -> Ctx {
        Ctx {
            upstreams,
            registry,
            budget,
            parts: PART_BUDGET,
            stamp: Stamp::new("-"),
            conn: None,
            notify: None,
        }
    }

    pub fn with_notify(self, notify: Option<Notify>) -> Ctx {
        Ctx { notify, ..self }
    }

    pub fn parts(self, parts: Duration) -> Ctx {
        Ctx { parts, ..self }
    }

    pub fn on(self, outbox: Outbox) -> Ctx {
        Ctx {
            conn: Some(Conn {
                outbox,
                watching: Arc::default(),
            }),
            ..self
        }
    }

    pub fn for_request(&self, visitor: &Visitor, meta: &Map<String, Value>) -> Ctx {
        let mut stamp = Stamp::fresh()
            .within(self.budget)
            .role(visitor.role().as_str());
        if let Some(user) = visitor.user() {
            stamp = stamp.user(user.0);
        }
        for (key, value) in meta {
            if let Value::String(value) = value {
                stamp = stamp.pass(key, value);
            }
        }
        Ctx {
            stamp,
            ..self.clone()
        }
    }

    pub fn part_budget(&self) -> Duration {
        self.parts
    }

    pub fn stamp(&self) -> &Stamp {
        &self.stamp
    }

    pub fn request_id(&self) -> &str {
        self.stamp.request_id()
    }

    pub fn auth(&self) -> AuthClient<Stamped> {
        self.upstreams.auth(&self.stamp)
    }

    pub fn users(&self) -> UsersClient<Stamped> {
        self.upstreams.users(&self.stamp)
    }

    pub fn posts(&self) -> PostsClient<Stamped> {
        self.upstreams.posts(&self.stamp)
    }

    pub fn chat(&self) -> ChatClient<Stamped> {
        self.upstreams.chat(&self.stamp)
    }

    pub fn feed(&self) -> FeedsClient<Stamped> {
        self.upstreams.feed(&self.stamp)
    }

    pub fn endless_posts(&self) -> PostsClient<Stamped> {
        self.upstreams.posts(&self.stamp.clone().endless())
    }

    pub fn push(&self, to: UserId, push: &Push) -> usize {
        self.registry.deliver(to, push)
    }

    pub async fn notify<A: DeserializeOwned>(
        &self,
        method: NotifyMethod,
        ask: &NoticeAsk,
    ) -> Result<A, ErrorBody> {
        let Some(notify) = &self.notify else {
            let reason = "уведомления выключены: шлюз запущен без NATS_URL";
            return Err(
                ErrorBody::new(ErrorCode::Unavailable, reason).with_request_id(self.request_id())
            );
        };
        let meta = Meta::follow(self.request_id(), self.stamp.trace_id());
        notify.ask(method, ask, &meta).await
    }

    pub fn outbox(&self) -> Option<&Outbox> {
        self.conn.as_ref().map(|conn| &conn.outbox)
    }

    pub fn watching(&self) -> Option<&Watching> {
        self.conn.as_ref().map(|conn| conn.watching.as_ref())
    }

    pub fn failed<'a>(&'a self, service: &'a str) -> impl Fn(Status) -> ErrorBody + 'a {
        move |status| failure(&status, service, self.stamp.request_id())
    }
}
