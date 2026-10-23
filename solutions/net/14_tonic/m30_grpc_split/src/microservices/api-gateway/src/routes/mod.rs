mod chat;
mod posts;
mod users;

pub(crate) use posts::view as post_view;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Call, Envelope, ErrorBody, Push, Reply, UserId, UserView, decode_call};
use proto::social::v1::auth_client::AuthClient;
use proto::social::v1::posts_client::PostsClient;
use proto::social::v1::users_client::UsersClient;
use proto::{new_request_id, traced};
use tonic::transport::Channel;
use tonic::{Request, Status};

use crate::chat::ChatStore;
use crate::protocol::{contract, internal};
use crate::registry::Registry;
use crate::upstream::{Upstreams, failure};

pub type Answer = Result<Reply, ErrorBody>;

pub type Pending = Pin<Box<dyn Future<Output = Answer> + Send>>;

type Open = Arc<dyn Fn(Ctx, Envelope) -> Pending + Send + Sync>;

type Authed = Arc<dyn Fn(Ctx, UserView, Envelope) -> Pending + Send + Sync>;

#[derive(Clone)]
enum Route {
    Open(Open),
    Authed(Authed),
}

#[derive(Clone)]
pub struct Ctx {
    upstreams: Upstreams,
    registry: Arc<Registry>,
    chats: Arc<ChatStore>,
    request_id: String,
}

impl Ctx {
    pub fn new(upstreams: Upstreams, registry: Arc<Registry>, chats: Arc<ChatStore>) -> Ctx {
        Ctx {
            upstreams,
            registry,
            chats,
            request_id: String::from("-"),
        }
    }

    pub fn for_request(&self) -> Ctx {
        Ctx {
            request_id: new_request_id(),
            ..self.clone()
        }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn traced<T>(&self, message: T) -> Request<T> {
        traced(message, &self.request_id)
    }

    pub fn auth(&self) -> AuthClient<Channel> {
        self.upstreams.auth()
    }

    pub fn users(&self) -> UsersClient<Channel> {
        self.upstreams.users()
    }

    pub fn posts(&self) -> PostsClient<Channel> {
        self.upstreams.posts()
    }

    pub fn chats(&self) -> &ChatStore {
        &self.chats
    }

    pub fn push(&self, to: UserId, push: &Push) -> usize {
        self.registry.deliver(to, push)
    }

    pub fn failed<'a>(&'a self, service: &'a str) -> impl Fn(Status) -> ErrorBody + 'a {
        move |status| failure(&status, service, &self.request_id)
    }
}

fn acting(me: &UserView, mut envelope: Envelope) -> Result<Call, ErrorBody> {
    envelope.act_as(me.id).map_err(contract)?;
    decode_call(&envelope).map_err(contract)
}

fn misrouted(call: &Call) -> ErrorBody {
    internal(&format!("{} попал не в свой обработчик", call.method()))
}

#[derive(Clone, Default)]
pub struct Routes {
    table: HashMap<String, Route>,
}

fn ready(answer: Answer) -> Pending {
    Box::pin(async move { answer })
}

impl Routes {
    pub fn new() -> Routes {
        Routes::default()
    }

    pub fn social() -> Routes {
        Routes::new()
            .route("users.find", users::find)
            .route_authed("users.follow", users::follow)
            .route_authed("users.mute", users::mute)
            .route_authed("posts.create", posts::create)
            .route_authed("posts.like", posts::like)
            .route_authed("posts.delete", posts::delete)
            .route_authed("posts.feed", posts::feed)
            .route_authed("chat.send", chat::send)
            .route_authed("chat.history", chat::history)
    }

    pub fn route<F, Fut>(self, name: &str, handler: F) -> Routes
    where
        F: Fn(Ctx, Envelope) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Answer> + Send + 'static,
    {
        let handler: Open = Arc::new(move |ctx, envelope| Box::pin(handler(ctx, envelope)));
        self.insert(name, Route::Open(handler))
    }

    pub fn route_authed<F, Fut>(self, name: &str, handler: F) -> Routes
    where
        F: Fn(Ctx, UserView, Envelope) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Answer> + Send + 'static,
    {
        let handler: Authed =
            Arc::new(move |ctx, user, envelope| Box::pin(handler(ctx, user, envelope)));
        self.insert(name, Route::Authed(handler))
    }

    fn insert(mut self, name: &str, route: Route) -> Routes {
        if self.table.insert(name.to_string(), route).is_some() {
            panic!("метод {name} зарегистрирован дважды");
        }
        self
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.table.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn is_authed(&self, name: &str) -> Option<bool> {
        self.table
            .get(name)
            .map(|route| matches!(route, Route::Authed(..)))
    }

    pub fn dispatch(&self, ctx: Ctx, user: &UserView, envelope: Envelope) -> Pending {
        let method = envelope.method.as_str();
        let service = method
            .split_once('.')
            .map_or(method, |(service, _)| service);
        if service != envelope.service {
            return ready(Err(contract(contracts::Error::WrongService {
                service: envelope.service.clone(),
                method: method.to_string(),
            })));
        }
        match self.table.get(method) {
            Some(Route::Open(handler)) => handler(ctx, envelope),
            Some(Route::Authed(handler)) => handler(ctx, user.clone(), envelope),
            None => ready(Err(contract(contracts::Error::UnknownMethod(
                method.to_string(),
            )))),
        }
    }
}
