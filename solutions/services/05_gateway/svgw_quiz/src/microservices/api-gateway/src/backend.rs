use crate::types::{GwError, Metadata};
use futures::future::BoxFuture;
use serde_json::Value;

pub trait Backend: Send + Sync {
    fn get_user(&self, id: u64, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>>;
    fn list_posts(&self, id: u64, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>>;
    fn stats(&self, id: u64, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>>;
    fn create_post(&self, text: &str, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>>;
}
