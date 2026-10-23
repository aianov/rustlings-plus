use std::collections::HashMap;
use std::sync::Arc;

use crate::{Handler, Response, handler};

pub type Middleware = Arc<dyn Fn(Handler) -> Handler + Send + Sync>;

pub fn middleware<F>(f: F) -> Middleware
where
    F: Fn(Handler) -> Handler + Send + Sync + 'static,
{
    Arc::new(f)
}

pub fn apply(inner: Handler, layers: &[Middleware]) -> Handler {
    todo!()
}

pub fn access_log(sink: Arc<dyn Fn(String) + Send + Sync>) -> Middleware {
    todo!()
}

pub fn bearer_auth(tokens: HashMap<String, String>, public: Vec<String>) -> Middleware {
    todo!()
}
