use std::sync::Arc;

use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
    Head,
}

#[allow(dead_code)]
impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Request {
    method: Method,
    path: String,
    body: String,
}

impl Request {
    fn new(method: Method, path: &str) -> Request {
        Request { method, path: path.to_string(), body: String::new() }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

#[allow(dead_code)]
impl Response {
    fn text(status: u16, body: &str) -> Response {
        Response { status, headers: Vec::new(), body: body.to_string() }
    }

    fn header(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler = Box<dyn Fn(Request) -> BoxFuture<Response> + Send + Sync>;

fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    Box::new(move |request| Box::pin(f(request)))
}

async fn guarded(handler: Arc<Handler>, request: Request) -> Response {
    match tokio::spawn(async move { handler(request).await }).await {
        Ok(response) => response,
        Err(_) => Response::text(500, "internal error"),
    }
}

#[tokio::main]
async fn main() {
    let h = Arc::new(handler(|_| async { Response::text(200, "fine") }));
    println!("{:?}", guarded(h, Request::new(Method::Get, "/")).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn flaky() -> Arc<Handler> {
        Arc::new(handler(|req| {
            if req.path == "/sync" {
                panic!("secret sync detail");
            }
            async move {
                if req.path == "/async" {
                    panic!("secret async detail");
                }
                Response::text(200, &format!("fine {}", req.path))
            }
        }))
    }

    #[tokio::test]
    async fn healthy_requests_pass_through() {
        let reply = guarded(flaky(), Request::new(Method::Get, "/ok")).await;
        assert_eq!((reply.status, reply.body.as_str()), (200, "fine /ok"));
    }

    #[tokio::test]
    async fn both_kinds_of_panic_become_500() {
        for path in ["/sync", "/async"] {
            let reply = guarded(flaky(), Request::new(Method::Get, path)).await;
            assert_eq!(reply.status, 500, "{path}: паника должна стать ответом 500");
            assert_eq!(reply.body, "internal error");
            assert!(!reply.body.contains("secret"), "текст паники клиенту показывать нельзя");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_same_handler_survives_and_neighbours_are_unharmed() {
        let handler_ = flaky();
        let done = Arc::new(AtomicUsize::new(0));
        let slow: Arc<Handler> = {
            let done = done.clone();
            Arc::new(handler(move |_| {
                let done = done.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    done.fetch_add(1, Ordering::SeqCst);
                    Response::text(200, "slow done")
                }
            }))
        };
        let neighbour = tokio::spawn(guarded(slow, Request::new(Method::Get, "/")));
        for path in ["/async", "/sync", "/async"] {
            assert_eq!(guarded(handler_.clone(), Request::new(Method::Get, path)).await.status, 500);
        }
        assert_eq!(guarded(handler_, Request::new(Method::Get, "/after")).await.body, "fine /after", "после паник обработчик остаётся рабочим");
        let reply = neighbour.await.unwrap();
        assert_eq!((reply.status, done.load(Ordering::SeqCst)), (200, 1), "соседний запрос не должен пострадать");
    }
}
