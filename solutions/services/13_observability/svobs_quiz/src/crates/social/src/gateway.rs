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
        let path = req.target.split('?').next().unwrap_or("").to_string();
        if req.method == "GET" && path == "/metrics" {
            return Response { status: 200, body: self.registry.render() };
        }
        let traceparent = req.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case(TRACEPARENT)).map(|(_, value)| value.clone());
        let entered = enter(traceparent.as_deref(), &mut || (self.ids)());
        let span = obs::hop_span("gateway", &entered.ctx, entered.parent);
        let route = match path.as_str() {
            "/posts" => "/posts",
            "/health" => "/health",
            _ => "unmatched",
        };
        let work = async {
            let response = match (req.method.as_str(), route) {
                ("POST", "/posts") => {
                    let mut metadata = Headers::new();
                    metadata.insert(TRACEPARENT.to_string(), entered.ctx.header());
                    match self.posts.create(&metadata, &req.body).await {
                        Ok(id) => Response { status: 201, body: id.to_string() },
                        Err(error) => Response { status: error.status(), body: format!("{error:?}") },
                    }
                }
                ("GET", "/health") => Response { status: 200, body: "ok".to_string() },
                _ => Response { status: 404, body: "not found".to_string() },
            };
            let line = access_line(&Req { method: &req.method, target: &req.target, headers: &req.headers }, response.status);
            info!(line = %line, "запрос обработан");
            (response.status, response)
        };
        self.red.track(route, work).instrument(span).await
    }

    pub fn metrics(&self) -> String {
        self.registry.render()
    }
}
