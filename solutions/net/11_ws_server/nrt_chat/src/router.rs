use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use crate::hub::{ConnId, Hub, Outbox};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Debug, PartialEq)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError { code, message: message.into() }
    }
}

pub type Reply = Result<Value, ApiError>;

#[derive(Clone)]
pub struct Ctx {
    pub user: String,
    pub conn: ConnId,
    pub hub: Hub,
    pub outbox: Outbox,
}

type Handler = Arc<dyn Fn(Ctx, Value) -> BoxFuture<Reply> + Send + Sync>;

#[derive(Default)]
pub struct Router {
    routes: HashMap<String, Handler>,
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    pub fn route<F, Fut>(mut self, name: &str, handler: F) -> Router
    where
        F: Fn(Ctx, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Reply> + Send + 'static,
    {
        let handler: Handler = Arc::new(move |ctx, data| Box::pin(handler(ctx, data)));
        if self.routes.insert(name.to_string(), handler).is_some() {
            panic!("метод {name} зарегистрирован дважды");
        }
        self
    }

    pub fn dispatch(&self, ctx: Ctx, service: &str, method: &str, data: Value) -> BoxFuture<Reply> {
        let key = format!("{service}.{method}");
        match self.routes.get(&key) {
            Some(handler) => handler(ctx, data),
            None => Box::pin(async move { Err(ApiError::new("not_found", format!("нет метода {key}"))) }),
        }
    }
}
