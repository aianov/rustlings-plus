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

impl RouterBuilder {
    pub fn new() -> RouterBuilder {
        RouterBuilder::default()
    }

    pub fn route<Req, Resp, F, Fut>(self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        todo!()
    }

    pub fn route_authed<Req, Resp, F, Fut>(self, service: &str, method: &str, handler: F) -> RouterBuilder
    where
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + 'static,
        F: Fn(Principal, Ctx, Req) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Resp, GwError>> + Send + 'static,
    {
        todo!()
    }

    pub fn build(self) -> Result<Router, Vec<String>> {
        todo!()
    }
}

impl Router {
    pub async fn call(&self, service: &str, method: &str, ctx: Ctx, data: Value) -> Result<Value, GwError> {
        todo!()
    }
}
