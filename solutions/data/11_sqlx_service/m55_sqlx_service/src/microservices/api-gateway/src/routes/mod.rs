mod chat;
mod ctx;
mod live;
mod notify;
mod pins;
mod posts;
mod profiles;
mod social;
mod threads;
mod users;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use contracts::{Envelope, ErrorBody, ErrorCode, Value, read_data, to_data};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::common::{Duplicates, Principal, Visitor};
use crate::controller::protocol::{contract, internal};

pub use ctx::{Ctx, Watching};

pub type Answer = Result<Value, ErrorBody>;

pub type Pending = Pin<Box<dyn Future<Output = Answer> + Send>>;

type Raw = Arc<dyn Fn(Ctx, &Visitor, Envelope) -> Pending + Send + Sync>;

#[derive(Clone)]
struct Route {
    authed: bool,
    raw: Raw,
}

fn ready(answer: Answer) -> Pending {
    Box::pin(async move { answer })
}

fn answer<Resp, Fut>(work: Fut) -> Pending
where
    Resp: Serialize,
    Fut: Future<Output = Result<Resp, ErrorBody>> + Send + 'static,
{
    Box::pin(async move {
        let done = work.await?;
        to_data(&done).map_err(|error| internal(&format!("ответ не собрался: {error}")))
    })
}

fn guest(method: &str) -> ErrorBody {
    ErrorBody::new(
        ErrorCode::Unauthorized,
        format!("{method} только для вошедших: открой сокет с токеном"),
    )
}

#[derive(Default)]
pub struct Routes {
    table: HashMap<String, Route>,
    twice: Vec<String>,
}

impl Routes {
    pub fn new() -> Routes {
        Routes::default()
    }

    pub fn social() -> Routes {
        Routes::new()
            .route("users.find", users::find)
            .route("users.profile", users::profile)
            .route("users.followers", social::followers)
            .route("users.following", social::following)
            .route("users.friends", social::friends)
            .route_authed("posts.daily", social::daily)
            .route("posts.thread", threads::thread)
            .route("posts.search", threads::search)
            .route_authed("posts.pin", pins::pin)
            .route_authed("posts.unpin", pins::unpin)
            .route("posts.pinned", pins::pinned)
            .route_authed("posts.like_comment", pins::like_comment)
            .route("posts.top", threads::top)
            .route_authed("users.suggest", threads::suggest)
            .route_authed("users.follow", users::follow)
            .route_authed("users.mute", users::mute)
            .route_authed("posts.create", posts::create)
            .route_authed("posts.like", posts::like)
            .route_authed("posts.edit", posts::edit)
            .route_authed("posts.delete", posts::delete)
            .route_authed("posts.feed", posts::feed)
            .route_authed("posts.comment", posts::comment)
            .route_authed("chat.send", chat::send)
            .route_authed("chat.history", chat::history)
            .route_authed("chat.typing", chat::typing)
            .route_authed("users.online", users::online)
            .route_authed("posts.watch", live::watch)
            .route_authed("posts.unwatch", live::unwatch)
            .route_authed("notify.unread", notify::unread)
            .route_authed("notify.list", notify::list)
            .route_authed("notify.read", notify::read)
    }

    pub fn route<Req, Resp, F, Fut>(self, name: &str, handler: F) -> Routes
    where
        Req: DeserializeOwned + 'static,
        Resp: Serialize + 'static,
        F: Fn(Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ErrorBody>> + Send + 'static,
    {
        let raw: Raw = Arc::new(move |ctx, _, envelope: Envelope| {
            match read_data::<Req>(&envelope.method, envelope.data) {
                Ok(data) => answer(handler(ctx, data)),
                Err(body) => ready(Err(body)),
            }
        });
        self.insert(name, false, raw)
    }

    pub fn route_authed<Req, Resp, F, Fut>(self, name: &str, handler: F) -> Routes
    where
        Req: DeserializeOwned + 'static,
        Resp: Serialize + 'static,
        F: Fn(Ctx, Principal, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, ErrorBody>> + Send + 'static,
    {
        let raw: Raw = Arc::new(move |ctx, visitor, mut envelope: Envelope| {
            let Visitor::Member(me) = visitor else {
                return ready(Err(guest(&envelope.method)));
            };
            if let Err(error) = envelope.act_as(me.id) {
                return ready(Err(contract(error)));
            }
            match read_data::<Req>(&envelope.method, envelope.data) {
                Ok(data) => answer(handler(ctx, me.clone(), data)),
                Err(body) => ready(Err(body)),
            }
        });
        self.insert(name, true, raw)
    }

    fn insert(mut self, name: &str, authed: bool, raw: Raw) -> Routes {
        if self.table.contains_key(name) {
            if !self.twice.iter().any(|seen| seen == name) {
                self.twice.push(name.to_string());
            }
            return self;
        }
        self.table.insert(name.to_string(), Route { authed, raw });
        self
    }

    pub fn build(self) -> Result<Router, Duplicates> {
        if !self.twice.is_empty() {
            return Err(Duplicates(self.twice));
        }
        Ok(Router {
            table: Arc::new(self.table),
        })
    }
}

#[derive(Clone)]
pub struct Router {
    table: Arc<HashMap<String, Route>>,
}

impl Router {
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.table.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn is_authed(&self, name: &str) -> Option<bool> {
        self.table.get(name).map(|route| route.authed)
    }

    pub fn dispatch(&self, ctx: Ctx, visitor: &Visitor, envelope: Envelope) -> Pending {
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
            Some(route) => (route.raw)(ctx, visitor, envelope),
            None => ready(Err(contract(contracts::Error::UnknownMethod(
                method.to_string(),
            )))),
        }
    }
}
