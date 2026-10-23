mod json;
mod parse;
mod router;
mod server;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub use parse::*;
pub use router::Router;
pub use server::serve;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub type Handler = Arc<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>;

pub fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Arc::new(move |request| Box::pin(f(request)))
}
