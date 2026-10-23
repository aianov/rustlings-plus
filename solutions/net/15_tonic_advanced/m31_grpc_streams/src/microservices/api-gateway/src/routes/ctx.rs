use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use contracts::{ErrorBody, Push, UserId};
use grpc_kit::Stamp;
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::chat_client::ChatClient;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use tokio::task::JoinHandle;
use tonic::Status;

use crate::registry::{Outbox, Registry};
use crate::upstream::{Stamped, Upstreams, failure};

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
    stamp: Stamp,
    conn: Option<Conn>,
}

impl Ctx {
    pub fn new(upstreams: Upstreams, registry: Arc<Registry>, budget: Duration) -> Ctx {
        Ctx {
            upstreams,
            registry,
            budget,
            stamp: Stamp::new("-"),
            conn: None,
        }
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

    pub fn for_request(&self, user: UserId) -> Ctx {
        Ctx {
            stamp: Stamp::fresh().user(user.0).within(self.budget),
            ..self.clone()
        }
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

    pub fn endless_posts(&self) -> PostsClient<Stamped> {
        self.upstreams.posts(&self.stamp.clone().endless())
    }

    pub fn push(&self, to: UserId, push: &Push) -> usize {
        self.registry.deliver(to, push)
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
