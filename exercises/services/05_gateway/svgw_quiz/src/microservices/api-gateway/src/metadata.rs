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

impl RequestIds {
    pub fn new() -> RequestIds {
        RequestIds::default()
    }

    pub fn context(&self, frame: &Value, principal: Option<Principal>) -> RequestCtx {
        todo!()
    }
}

pub fn forward_metadata(client_meta: &Map<String, Value>, ctx: &RequestCtx) -> Metadata {
    todo!()
}
