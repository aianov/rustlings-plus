use std::sync::Arc;

use crate::{Handler, Method, Params, Request, Response, decode_component, handler};

enum Segment {
    Static(String),
    Param(String),
}

struct Route {
    method: Method,
    segments: Vec<Segment>,
    handler: Handler,
}

#[derive(Default)]
pub struct Router {
    routes: Vec<Route>,
}

fn split_path(path: &str) -> Vec<&str> {
    path.strip_prefix('/').unwrap_or(path).split('/').collect()
}

impl Route {
    fn matches(&self, parts: &[String]) -> Option<(Vec<u8>, Params)> {
        todo!()
    }
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    pub fn route(mut self, method: Method, pattern: &str, handler: Handler) -> Router {
        todo!()
    }

    pub async fn dispatch(&self, mut request: Request) -> Response {
        todo!()
    }

    pub fn into_handler(self) -> Handler {
        let router = Arc::new(self);
        handler(move |request| {
            let router = router.clone();
            async move { router.dispatch(request).await }
        })
    }
}
