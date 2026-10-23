use crate::backend::Backend;
use crate::metadata::{RequestIds, forward_metadata};
use crate::profile::profile;
use crate::proxy::{ProxyConfig, ProxyRequest, ProxyResponse, Upstream, proxy};
use crate::router::{Ctx, Router, RouterBuilder};
use crate::session::Session;
use crate::types::{Clock, GwError, TokenCheck, err_reply, ok_reply};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

pub struct Config {
    pub backend: Arc<dyn Backend>,
    pub checker: Arc<dyn TokenCheck>,
    pub clock: Arc<dyn Clock>,
    pub upstream: Arc<dyn Upstream>,
    pub max_in_flight: usize,
    pub proxy: ProxyConfig,
}

pub struct Gateway {
    router: Router,
    ids: RequestIds,
    config: Config,
}

#[derive(Deserialize)]
struct ById {
    id: u64,
}

#[derive(Deserialize)]
struct NewPost {
    text: String,
}

fn frame_id(frame: &Value) -> Value {
    frame.get("id").cloned().filter(|id| id.is_string() || id.is_number()).unwrap_or(Value::Null)
}

impl Gateway {
    pub fn new(config: Config) -> Result<Gateway, Vec<String>> {
        let users = config.backend.clone();
        let posts = config.backend.clone();
        let full = config.backend.clone();
        let router = RouterBuilder::new()
            .route("user", "get", move |ctx: Ctx, req: ById| {
                let backend = users.clone();
                async move { backend.get_user(req.id, &ctx.meta).await }
            })
            .route_authed("post", "create", move |_who, ctx: Ctx, req: NewPost| {
                let backend = posts.clone();
                async move { backend.create_post(&req.text, &ctx.meta).await }
            })
            .route_authed("profile", "full", move |_who, ctx: Ctx, req: ById| {
                let backend = full.clone();
                async move { profile(backend.as_ref(), req.id, &ctx.meta).await }
            })
            .build()?;
        Ok(Gateway { router, ids: RequestIds::new(), config })
    }

    pub fn open(&self, headers: &[(String, String)], query: &str) -> Result<Session, GwError> {
        Session::open(headers, query, self.config.checker.as_ref())
    }

    pub async fn proxy_auth(&self, request: ProxyRequest) -> ProxyResponse {
        proxy(request, self.config.upstream.as_ref(), &self.config.proxy).await
    }

    async fn handle(&self, frame: Value, session_principal: Option<crate::types::Principal>) -> Value {
        let id = frame_id(&frame);
        let (Some(service), Some(method)) = (frame.get("service").and_then(Value::as_str), frame.get("method").and_then(Value::as_str)) else {
            return err_reply(&id, &GwError::new("invalid_argument", "нужны service и method"));
        };
        let request_ctx = self.ids.context(&frame, session_principal.clone());
        let client_meta = frame.get("meta").and_then(Value::as_object).cloned().unwrap_or_else(Map::new);
        let ctx = Ctx { principal: session_principal, meta: forward_metadata(&client_meta, &request_ctx) };
        let mut reply = match self.router.call(service, method, ctx, frame.get("data").cloned().unwrap_or(Value::Null)).await {
            Ok(data) => ok_reply(&id, data),
            Err(error) => err_reply(&id, &error),
        };
        reply["request_id"] = Value::String(request_ctx.request_id);
        reply
    }

    pub async fn serve(self: Arc<Self>, mut session: Session, mut incoming: mpsc::Receiver<String>, outgoing: mpsc::Sender<Value>) {
        let permits = Arc::new(Semaphore::new(self.config.max_in_flight));
        let mut tasks: JoinSet<Value> = JoinSet::new();
        let mut ids: HashMap<tokio::task::Id, Value> = HashMap::new();
        loop {
            tokio::select! {
                text = incoming.recv() => {
                    let Some(text) = text else { break };
                    let Ok(frame) = serde_json::from_str::<Value>(&text) else {
                        let _ = outgoing.send(err_reply(&Value::Null, &GwError::new("bad_frame", "кадр не JSON"))).await;
                        continue;
                    };
                    let id = frame_id(&frame);
                    if let Err(error) = session.ensure_alive(self.config.clock.now()) {
                        let _ = outgoing.send(err_reply(&id, &error)).await;
                        break;
                    }
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        let busy = err_reply(&id, &GwError::new("resource_exhausted", "слишком много запросов в работе"));
                        let _ = outgoing.send(busy).await;
                        continue;
                    };
                    let gateway = self.clone();
                    let principal = session.principal.clone();
                    let handle = tasks.spawn(async move {
                        let _permit = permit;
                        gateway.handle(frame, principal).await
                    });
                    ids.insert(handle.id(), id);
                }
                Some(done) = tasks.join_next_with_id(), if !tasks.is_empty() => {
                    Self::deliver(&outgoing, &mut ids, done).await;
                }
            }
        }
        while let Some(done) = tasks.join_next_with_id().await {
            Self::deliver(&outgoing, &mut ids, done).await;
        }
    }

    async fn deliver(outgoing: &mpsc::Sender<Value>, ids: &mut HashMap<tokio::task::Id, Value>, done: Result<(tokio::task::Id, Value), tokio::task::JoinError>) {
        let reply = match done {
            Ok((task, reply)) => {
                ids.remove(&task);
                reply
            }
            Err(failed) => {
                let id = ids.remove(&failed.id()).unwrap_or(Value::Null);
                err_reply(&id, &GwError::new("internal", "обработчик упал"))
            }
        };
        let _ = outgoing.send(reply).await;
    }
}
