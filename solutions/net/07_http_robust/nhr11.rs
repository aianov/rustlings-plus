use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use tokio::time::Instant;

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

#[allow(dead_code)]
impl Request {
    fn new(method: &str, path: &str) -> Request {
        Request { method: method.to_string(), path: path.to_string(), headers: Vec::new() }
    }

    fn with_header(mut self, name: &str, value: &str) -> Request {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn set_header(&mut self, name: &str, value: &str) {
        self.remove_header(name);
        self.headers.push((name.to_string(), value.to_string()));
    }

    fn remove_header(&mut self, name: &str) {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    }
}

#[derive(Debug, Clone)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

#[allow(dead_code)]
impl Response {
    fn new(status: u16, body: &str) -> Response {
        Response { status, headers: Vec::new(), body: body.to_string() }
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler = Arc<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>;
type Middleware = Arc<dyn Fn(Handler) -> Handler + Send + Sync>;

fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Arc::new(move |request| Box::pin(f(request)))
}

fn middleware<F>(f: F) -> Middleware
where
    F: Fn(Handler) -> Handler + Send + Sync + 'static,
{
    Arc::new(f)
}

#[allow(dead_code)]
type Log = Arc<Mutex<Vec<String>>>;
use std::sync::atomic::AtomicUsize;

fn apply(inner: Handler, layers: &[Middleware]) -> Handler {
    layers.iter().rev().fold(inner, |next, layer| layer(next))
}

fn recover() -> Middleware {
    middleware(|next| {
        handler(move |request| {
            let next = next.clone();
            async move {
                match tokio::spawn(async move { next(request).await }).await {
                    Ok(response) => response,
                    Err(_) => Response::new(500, "internal error"),
                }
            }
        })
    })
}

fn access_log(log: Log) -> Middleware {
    middleware(move |next| {
        let log = log.clone();
        handler(move |request| {
            let next = next.clone();
            let log = log.clone();
            let line = format!("{} {}", request.method, request.path);
            async move {
                let response = next(request).await;
                log.lock().unwrap().push(format!("{line} {}", response.status));
                response
            }
        })
    })
}

fn timing() -> Middleware {
    middleware(|next| {
        handler(move |request| {
            let next = next.clone();
            async move {
                let started = Instant::now();
                let mut response = next(request).await;
                response.headers.push(("X-Elapsed-Ms".to_string(), started.elapsed().as_millis().to_string()));
                response
            }
        })
    })
}

fn auth(token: &'static str) -> Middleware {
    middleware(move |next| {
        handler(move |request| {
            let next = next.clone();
            async move {
                if request.header("Authorization") == Some(token) {
                    next(request).await
                } else {
                    Response::new(401, "unauthorized")
                }
            }
        })
    })
}

fn routes(hits: Arc<std::sync::atomic::AtomicUsize>) -> Handler {
    handler(move |request| {
        let hits = hits.clone();
        async move {
            hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if request.path == "/boom" {
                panic!("handler bug");
            }
            Response::new(200, "secret data")
        }
    })
}

fn build_app(log: Log, hits: Arc<AtomicUsize>) -> Handler {
    apply(routes(hits), &[timing(), access_log(log), recover(), auth("Bearer ok")])
}

#[tokio::main]
async fn main() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let app = build_app(log.clone(), Arc::default());
    let response = app(Request::new("GET", "/private")).await;
    println!("{} {:?}", response.status, log.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn app() -> (Handler, Log, Arc<AtomicUsize>) {
        let log: Log = Arc::default();
        let hits = Arc::new(AtomicUsize::new(0));
        (build_app(log.clone(), hits.clone()), log, hits)
    }

    #[tokio::test]
    async fn rejected_requests_are_logged_and_timed() {
        let (app, log, hits) = app();
        let response = app(Request::new("GET", "/private")).await;
        assert_eq!(response.status, 401);
        assert!(response.header("X-Elapsed-Ms").is_some(), "замер должен охватывать и проверку токена: у ответа 401 нет X-Elapsed-Ms");
        assert_eq!(*log.lock().unwrap(), ["GET /private 401"], "журнал должен видеть отказы");
        assert_eq!(hits.load(Ordering::SeqCst), 0, "без токена обработчик не вызывается");
    }

    #[tokio::test]
    async fn panic_is_a_logged_500() {
        let (app, log, hits) = app();
        let response = app(Request::new("GET", "/boom").with_header("Authorization", "Bearer ok")).await;
        assert_eq!(response.status, 500);
        assert_eq!(response.body, "internal error");
        assert!(response.header("X-Elapsed-Ms").is_some());
        assert_eq!(*log.lock().unwrap(), ["GET /boom 500"], "паника должна превратиться в 500 раньше, чем её увидит журнал, и попасть в него");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn happy_path_still_works() {
        let (app, log, _) = app();
        let response = app(Request::new("GET", "/data").with_header("Authorization", "Bearer ok")).await;
        assert_eq!((response.status, response.body.as_str()), (200, "secret data"));
        assert!(response.header("X-Elapsed-Ms").is_some());
        assert_eq!(*log.lock().unwrap(), ["GET /data 200"]);
    }
}
