mod chat;
mod ctx;
mod live;
mod posts;
mod users;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Call, Envelope, ErrorBody, Reply, UserView, decode_call};

use crate::controller::protocol::{contract, internal};

pub use ctx::{Ctx, Watching};

pub type Answer = Result<Reply, ErrorBody>;

pub type Pending = Pin<Box<dyn Future<Output = Answer> + Send>>;

type Open = Arc<dyn Fn(Ctx, Envelope) -> Pending + Send + Sync>;

type Authed = Arc<dyn Fn(Ctx, UserView, Envelope) -> Pending + Send + Sync>;

#[derive(Clone)]
enum Route {
    Open(Open),
    Authed(Authed),
}

fn acting(me: &UserView, mut envelope: Envelope) -> Result<Call, ErrorBody> {
    envelope.act_as(me.id).map_err(contract)?;
    decode_call(&envelope).map_err(contract)
}

fn misrouted(method: &str) -> ErrorBody {
    internal(&format!("{method} попал не в свой обработчик"))
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
            .route_authed("posts.edit", posts::edit)
            .route_authed("posts.delete", posts::delete)
            .route_authed("posts.feed", posts::feed)
            .route_authed("chat.send", chat::send)
            .route_authed("chat.history", chat::history)
            .route_authed("posts.watch", live::watch)
            .route_authed("posts.unwatch", live::unwatch)
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
