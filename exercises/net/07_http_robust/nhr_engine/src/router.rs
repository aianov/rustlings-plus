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
        if self.segments.len() != parts.len() {
            return None;
        }
        let mut params = Params::new();
        let mut score = Vec::with_capacity(parts.len());
        for (segment, part) in self.segments.iter().zip(parts) {
            match segment {
                Segment::Static(s) if s == part => score.push(1),
                Segment::Static(_) => return None,
                Segment::Param(_) if part.is_empty() => return None,
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
        assert!(pattern.starts_with('/'), "шаблон должен начинаться с '/': {pattern}");
        let segments = split_path(pattern)
            .into_iter()
            .map(|s| match s.strip_prefix(':') {
                Some(name) => Segment::Param(name.to_string()),
                None => Segment::Static(s.to_string()),
            })
            .collect();
        self.routes.push(Route { method, segments, handler });
        self
    }

    fn best(&self, method: Method, parts: &[String]) -> Option<(&Route, Params)> {
        self.routes
            .iter()
            .filter(|r| r.method == method)
            .filter_map(|r| r.matches(parts).map(|(score, params)| (score, r, params)))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, r, params)| (r, params))
    }

    pub async fn dispatch(&self, mut request: Request) -> Response {
        let parts: Result<Vec<String>, _> = split_path(request.raw_path()).into_iter().map(decode_component).collect();
        let Ok(parts) = parts else {
            return Response::error(400, "bad percent-encoding in path");
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
            .filter(|r| r.matches(&parts).is_some())
            .flat_map(|r| match r.method {
                Method::Get => vec!["GET", "HEAD"],
                m => vec![m.as_str()],
            })
            .collect();
        allowed.sort_unstable();
        allowed.dedup();
        if allowed.is_empty() {
            return Response::error(404, "not found");
        }
        let mut response = Response::error(405, "method not allowed");
        response.headers.append("Allow", &allowed.join(", "));
        response
    }

    pub fn into_handler(self) -> Handler {
        let router = Arc::new(self);
        handler(move |request| {
            let router = router.clone();
            async move { router.dispatch(request).await }
        })
    }
}
