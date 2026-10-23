use crate::backend::Backend;
use crate::types::{GwError, Metadata};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::time::Duration;

pub const OPTIONAL_BUDGET: Duration = Duration::from_millis(150);

async fn optional(fut: BoxFuture<'_, Result<Value, GwError>>) -> Option<Value> {
    tokio::time::timeout(OPTIONAL_BUDGET, fut).await.ok().and_then(Result::ok)
}

pub async fn profile(backend: &dyn Backend, id: u64, meta: &Metadata) -> Result<Value, GwError> {
    let (user, posts, stats) = tokio::join!(backend.get_user(id, meta), optional(backend.list_posts(id, meta)), optional(backend.stats(id, meta)));
    let user = user?;
    let mut partial = Vec::new();
    if posts.is_none() {
        partial.push("posts");
    }
    if stats.is_none() {
        partial.push("stats");
    }
    Ok(json!({"user": user, "posts": posts, "stats": stats, "partial": partial}))
}
