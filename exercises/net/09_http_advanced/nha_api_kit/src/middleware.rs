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
    layers.iter().rev().fold(inner, |next, layer| layer(next))
}

pub fn access_log(sink: Arc<dyn Fn(String) + Send + Sync>) -> Middleware {
    middleware(move |next| {
        let sink = sink.clone();
        handler(move |request| {
            let next = next.clone();
            let sink = sink.clone();
            let line = format!("{} {}", request.method.as_str(), request.raw_path());
            async move {
                let response = next(request).await;
                sink(format!("{line} {}", response.status.code()));
                response
            }
        })
    })
}

pub fn bearer_auth(tokens: HashMap<String, String>, public: Vec<String>) -> Middleware {
    let tokens = Arc::new(tokens);
    let public = Arc::new(public);
    middleware(move |next| {
        let tokens = tokens.clone();
        let public = public.clone();
        handler(move |mut request| {
            let next = next.clone();
            let tokens = tokens.clone();
            let public = public.clone();
            async move {
                request.headers.remove("x-user");
                if public.iter().any(|p| p == request.raw_path()) {
                    return next(request).await;
                }
                let user = request.headers.get("authorization").and_then(|value| {
                    let (scheme, token) = value.split_once(' ')?;
                    if !scheme.eq_ignore_ascii_case("bearer") {
                        return None;
                    }
                    tokens.get(token.trim()).cloned()
                });
                match user {
                    Some(user) => {
                        request.headers.set("X-User", &user);
                        next(request).await
                    }
                    None => {
                        let mut response = Response::error(401, "unauthorized");
                        response.headers.set("WWW-Authenticate", "Bearer");
                        response
                    }
                }
            }
        })
    })
}
