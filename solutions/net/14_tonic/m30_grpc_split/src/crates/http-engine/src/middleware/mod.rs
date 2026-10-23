use std::sync::Arc;

use tokio::time::Instant;

use crate::request::Request;
use crate::response::Response;
use crate::router::{BoxFuture, Handler, handler};

pub type Middleware = Arc<dyn Fn(Handler) -> Handler + Send + Sync>;

pub fn middleware<F>(f: F) -> Middleware
where
    F: Fn(Handler) -> Handler + Send + Sync + 'static,
{
    Arc::new(f)
}

pub fn apply(inner: Handler, layers: &[Middleware]) -> Handler {
    layers.iter().rev().fold(inner, |next, layer| layer(next))
}

pub fn access_log(sink: Arc<dyn Fn(String) + Send + Sync>) -> Middleware {
    middleware(move |next| {
        let sink = Arc::clone(&sink);
        handler(move |request| {
            let next = Arc::clone(&next);
            let sink = Arc::clone(&sink);
            let line = format!("{} {}", request.method, request.raw_path());
            async move {
                let started = Instant::now();
                let response = next(request).await;
                let spent = started.elapsed().as_millis();
                sink(format!("{line} {} {spent}ms", response.status.code()));
                response
            }
        })
    })
}

fn unauthorized() -> Response {
    let mut response = Response::error(401, "нужен заголовок Authorization: Bearer <токен>");
    response.headers.set("WWW-Authenticate", "Bearer");
    response
}

fn bearer(request: &Request) -> Option<&str> {
    let value = request.headers.get("authorization")?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

pub fn bearer_auth<V, G>(verify: V, guard: G) -> Middleware
where
    V: Fn(&str) -> Option<String> + Send + Sync + 'static,
    G: Fn(&Request) -> bool + Send + Sync + 'static,
{
    let verify = Arc::new(verify);
    let guard = Arc::new(guard);
    middleware(move |next| {
        let verify = Arc::clone(&verify);
        let guard = Arc::clone(&guard);
        handler(move |mut request: Request| {
            let next = Arc::clone(&next);
            request.headers.remove("x-user");
            let user = bearer(&request).and_then(|token| verify(token));
            let open = !guard(&request);
            async move {
                match user {
                    Some(user) => {
                        request.headers.set("X-User", &user);
                        next(request).await
                    }
                    None if open => next(request).await,
                    None => unauthorized(),
                }
            }
        })
    })
}

pub fn bearer_auth_async<V, G>(verify: V, guard: G) -> Middleware
where
    V: Fn(String) -> BoxFuture<Result<Option<String>, Response>> + Send + Sync + 'static,
    G: Fn(&Request) -> bool + Send + Sync + 'static,
{
    let verify = Arc::new(verify);
    let guard = Arc::new(guard);
    middleware(move |next| {
        let verify = Arc::clone(&verify);
        let guard = Arc::clone(&guard);
        handler(move |mut request: Request| {
            let next = Arc::clone(&next);
            request.headers.remove("x-user");
            let checking =
                guard(&request).then(|| bearer(&request).map(|token| verify(token.to_string())));
            async move {
                match checking {
                    None => next(request).await,
                    Some(None) => unauthorized(),
                    Some(Some(checking)) => match checking.await {
                        Ok(Some(user)) => {
                            request.headers.set("X-User", &user);
                            next(request).await
                        }
                        Ok(None) => unauthorized(),
                        Err(response) => response,
                    },
                }
            }
        })
    })
}
