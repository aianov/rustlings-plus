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

#[tokio::main]
async fn main() {
    let h = handler(|req| async move { Response::text(200, &format!("hello {}", req.path)) });
    println!("{}", h(Request::new(Method::Get, "/world")).await.body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    async fn hello(_req: Request) -> Response {
        Response::text(200, "hello")
    }

    async fn delayed(req: Request) -> Response {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Response::text(200, &req.path)
    }

    #[tokio::test]
    async fn different_futures_in_one_table() {
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let prefix = String::from("n=");
        let mut table: HashMap<&str, Handler> = HashMap::new();
        table.insert("hello", handler(hello));
        table.insert("delayed", handler(delayed));
        table.insert(
            "count",
            handler(move |_req| {
                let counter = counter.clone();
                let prefix = prefix.clone();
                async move { Response::text(200, &format!("{prefix}{}", counter.fetch_add(1, Ordering::SeqCst) + 1)) }
            }),
        );
        assert_eq!(table["hello"](Request::new(Method::Get, "/")).await.body, "hello");
        assert_eq!(table["delayed"](Request::new(Method::Get, "/slow")).await.body, "/slow");
        assert_eq!(table["count"](Request::new(Method::Get, "/")).await.body, "n=1");
        assert_eq!(table["count"](Request::new(Method::Get, "/")).await.body, "n=2", "замыкание должно вызываться много раз, а не один");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn handler_moves_between_tasks() {
        let shared = Arc::new(handler(delayed));
        let mut tasks = Vec::new();
        for i in 0..8 {
            let shared = shared.clone();
            tasks.push(tokio::spawn(async move { shared(Request::new(Method::Get, &format!("/{i}"))).await.body }));
        }
        for (i, task) in tasks.into_iter().enumerate() {
            assert_eq!(task.await.unwrap(), format!("/{i}"));
        }
    }
}
