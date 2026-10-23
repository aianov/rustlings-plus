use crate::{Headers, Ids, PostService};
use obs::TRACEPARENT;
use obs::metrics::Registry;
use obs::red::Red;
use obs::redact::{Req, access_line};
use obs::trace::enter;
use std::sync::Arc;
use tracing::{Instrument, info};

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn new(method: &str, target: &str) -> Request {
        Request { method: method.to_string(), target: target.to_string(), headers: Vec::new(), body: String::new() }
    }

    pub fn header(mut self, name: &str, value: &str) -> Request {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: &str) -> Request {
        self.body = body.to_string();
        self
    }
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub struct Gateway {
    registry: Arc<Registry>,
    red: Red,
    posts: Arc<PostService>,
    ids: Ids,
}

impl Gateway {
    pub fn new(posts: Arc<PostService>, ids: Ids) -> Gateway {
        let registry = Arc::new(Registry::new());
        Gateway { red: Red::new(registry.clone()), registry, posts, ids }
    }

    pub async fn handle(&self, req: Request) -> Response {
        let _ = (req,);
        todo!()
    }

    pub fn metrics(&self) -> String {
        self.registry.render()
    }
}
