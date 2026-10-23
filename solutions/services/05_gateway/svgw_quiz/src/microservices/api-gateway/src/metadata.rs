use crate::types::{Metadata, Principal};
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub struct RequestCtx {
    pub request_id: String,
    pub principal: Option<Principal>,
}

#[derive(Default)]
pub struct RequestIds {
    next: AtomicU64,
}

const ALLOWED: [&str; 2] = ["x-trace-id", "accept-language"];

impl RequestIds {
    pub fn new() -> RequestIds {
        RequestIds::default()
    }

    pub fn context(&self, frame: &Value, principal: Option<Principal>) -> RequestCtx {
        let offered = frame
            .get("meta")
            .and_then(|meta| meta.get("request_id"))
            .and_then(Value::as_str)
            .filter(|id| (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)));
        let request_id = match offered {
            Some(id) => id.to_string(),
            None => format!("gw-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1),
        };
        RequestCtx { request_id, principal }
    }
}

pub fn forward_metadata(client_meta: &Map<String, Value>, ctx: &RequestCtx) -> Metadata {
    let mut metadata = vec![("x-request-id".to_string(), ctx.request_id.clone())];
    if let Some(principal) = &ctx.principal {
        metadata.push(("x-user-id".to_string(), principal.user_id.clone()));
        metadata.push(("x-user-role".to_string(), principal.role.clone()));
    }
    for (key, value) in client_meta {
        let key = key.to_ascii_lowercase();
        let Some(value) = value.as_str() else { continue };
        if ALLOWED.contains(&key.as_str()) && value.len() <= 128 && !value.chars().any(char::is_control) {
            metadata.push((key, value.to_string()));
        }
    }
    metadata
}
