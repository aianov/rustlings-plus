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

impl Gateway {
    pub fn new(config: Config) -> Result<Gateway, Vec<String>> {
        todo!()
    }

    pub fn open(&self, headers: &[(String, String)], query: &str) -> Result<Session, GwError> {
        todo!()
    }

    pub async fn proxy_auth(&self, request: ProxyRequest) -> ProxyResponse {
        todo!()
    }

    pub async fn serve(self: Arc<Self>, mut session: Session, mut incoming: mpsc::Receiver<String>, outgoing: mpsc::Sender<Value>) {
        todo!()
    }
}
