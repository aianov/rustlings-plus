mod domain;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Envelope, ErrorBody, Reply, UserView};
use social_core::App;

use crate::protocol::contract;

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
    app: App,
}

impl Ctx {
    pub fn new(app: App) -> Ctx {
        Ctx { app }
    }

    pub fn app(&self) -> &App {
        &self.app
    }
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
            .route("users.find", domain::open)
            .route_authed("users.follow", domain::acting)
            .route_authed("users.mute", domain::acting)
            .route_authed("posts.create", domain::acting)
            .route_authed("posts.like", domain::acting)
            .route_authed("posts.delete", domain::acting)
            .route_authed("posts.feed", domain::acting)
            .route_authed("chat.send", domain::acting)
            .route_authed("chat.history", domain::acting)
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
