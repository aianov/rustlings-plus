use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::method::Method;
use crate::parse::decode_component;
use crate::request::{Params, Request};
use crate::response::Response;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub type Handler = Arc<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>;

pub fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Arc::new(move |request| Box::pin(f(request)))
}

pub fn with_state<S, F, Fut>(state: Arc<S>, f: F) -> Handler
where
    S: Send + Sync + 'static,
    F: Fn(Arc<S>, Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Arc::new(move |request| Box::pin(f(Arc::clone(&state), request)))
}

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
        if self.segments.len() != parts.len() {
            return None;
        }
        let mut params = Params::new();
        let mut score = Vec::with_capacity(parts.len());
        for (segment, part) in self.segments.iter().zip(parts) {
            match segment {
                Segment::Static(own) if own == part => score.push(1),
                Segment::Static(..) => return None,
                Segment::Param(..) if part.is_empty() => return None,
                Segment::Param(name) => {
                    params.push(name, part.clone());
                    score.push(0);
                }
            }
        }
        Some((score, params))
    }
}

impl Router {
    pub fn new() -> Router {
        Router::default()
    }

    pub fn route(mut self, method: Method, pattern: &str, handler: Handler) -> Router {
        let segments = split_path(pattern)
            .into_iter()
            .map(|segment| match segment.strip_prefix(':') {
                Some(name) => Segment::Param(name.to_string()),
                None => Segment::Static(segment.to_string()),
            })
            .collect();
        self.routes.push(Route {
            method,
            segments,
            handler,
        });
        self
    }

    pub fn get(self, pattern: &str, handler: Handler) -> Router {
        self.route(Method::Get, pattern, handler)
    }

    pub fn post(self, pattern: &str, handler: Handler) -> Router {
        self.route(Method::Post, pattern, handler)
    }

    pub fn into_handler(self) -> Handler {
        let router = Arc::new(self);
        handler(move |request| {
            let router = Arc::clone(&router);
            async move { router.dispatch(request).await }
        })
    }

    fn best(&self, method: Method, parts: &[String]) -> Option<(&Route, Params)> {
        self.routes
            .iter()
            .filter(|route| route.method == method)
            .filter_map(|route| {
                route
                    .matches(parts)
                    .map(|(score, params)| (score, route, params))
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, route, params)| (route, params))
    }

    pub async fn dispatch(&self, mut request: Request) -> Response {
        let parts: Result<Vec<String>, _> = split_path(request.raw_path())
            .into_iter()
            .map(decode_component)
            .collect();
        let Ok(parts) = parts else {
            return Response::error(400, "кривой percent-encoding в пути");
        };
        let found = match self.best(request.method, &parts) {
            None if request.method == Method::Head => self.best(Method::Get, &parts),
            other => other,
        };
        if let Some((route, params)) = found {
            request.params = params;
            return (route.handler)(request).await;
        }
        let mut allowed: Vec<&str> = self
            .routes
            .iter()
            .filter(|route| route.matches(&parts).is_some())
            .flat_map(|route| match route.method {
                Method::Get => vec!["GET", "HEAD"],
                other => vec![other.as_str()],
            })
            .collect();
        allowed.sort_unstable();
        allowed.dedup();
        if allowed.is_empty() {
            return Response::error(404, "такого адреса нет");
        }
        let mut response = Response::error(405, "метод не подходит к адресу");
        response.headers.append("Allow", &allowed.join(", "));
        response
    }
}
