use crate::types::{GwError, Metadata, Principal};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::future::Future;

#[derive(Debug, Clone)]
pub struct Ctx {
    pub principal: Option<Principal>,
    pub meta: Metadata,
}

type Raw = Box<dyn Fn(Ctx, Value) -> BoxFuture<'static, Result<Value, GwError>> + Send + Sync>;

pub struct Router {
    table: HashMap<(String, String), Raw>,
}

#[derive(Default)]
pub struct RouterBuilder {
    table: HashMap<(String, String), Raw>,
    duplicates: Vec<String>,
}

fn decode<Req: DeserializeOwned>(data: Value) -> Result<Req, GwError> {
    serde_json::from_value(data).map_err(|_| GwError::new("invalid_argument", "данные запроса не подходят"))
}

fn encode<Resp: Serialize>(resp: Resp) -> Result<Value, GwError> {
    serde_json::to_value(resp).map_err(|_| GwError::new("internal", "ответ не сериализуется"))
}

impl RouterBuilder {
    pub fn new() -> RouterBuilder {
        RouterBuilder::default()
    }

    fn insert(mut self, service: &str, method: &str, raw: Raw) -> RouterBuilder {
        match self.table.entry((service.to_string(), method.to_string())) {
            Entry::Vacant(slot) => {
                slot.insert(raw);
            }
            Entry::Occupied(_) => {
                let name = format!("{service}.{method}");
                if !self.duplicates.contains(&name) {
                    self.duplicates.push(name);
                }
            }
        }
        self
    }

    pub fn route<Req, Resp, F, Fut>(self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        let raw: Raw = Box::new(move |ctx, data| match decode::<Req>(data) {
            Err(error) => async move { Err(error) }.boxed(),
            Ok(req) => {
                let fut = handler(ctx, req);
                async move { encode(fut.await?) }.boxed()
            }
        });
        self.insert(service, method, raw)
    }

    pub fn route_authed<Req, Resp, F, Fut>(self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Principal, Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        let raw: Raw = Box::new(move |ctx, data| {
            let Some(principal) = ctx.principal.clone() else {
                return async { Err(GwError::new("unauthenticated", "нужно войти")) }.boxed();
            };
            match decode::<Req>(data) {
                Err(error) => async move { Err(error) }.boxed(),
                Ok(req) => {
                    let fut = handler(principal, ctx, req);
                    async move { encode(fut.await?) }.boxed()
                }
            }
        });
        self.insert(service, method, raw)
    }

    pub fn build(self) -> Result<Router, Vec<String>> {
        if self.duplicates.is_empty() { Ok(Router { table: self.table }) } else { Err(self.duplicates) }
    }
}

impl Router {
    pub async fn call(&self, service: &str, method: &str, ctx: Ctx, data: Value) -> Result<Value, GwError> {
        match self.table.get(&(service.to_string(), method.to_string())) {
            Some(handler) => handler(ctx, data).await,
            None => Err(GwError::new("not_found", &format!("нет метода {service}.{method}"))),
        }
    }
}
