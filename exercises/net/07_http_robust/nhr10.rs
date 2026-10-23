// Проверка токена тоже слой, но особый: она иногда отвечает сама и не зовёт обработчик. Она кладёт
// в запрос заголовок X-User с именем владельца токена, и обработчики доверяют этому заголовку.
// А клиент может прислать X-User сам.
// TODO: Напиши require_token: токены лежат в таблице токен -> имя. Без "Authorization: Bearer <токен>"
// (схема и имя заголовка без учёта регистра) или с неизвестным токеном: 401 с "WWW-Authenticate: Bearer"
// без вызова обработчика. Пути из public открыты без токена. X-User от клиента до обработчика не доходит.
use std::collections::HashMap;

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

fn require_token(tokens: HashMap<String, String>, public: Vec<String>) -> Middleware {
    todo!()
}

#[tokio::main]
async fn main() {
    let tokens = HashMap::from([("secret".to_string(), "ada".to_string())]);
    let app = require_token(tokens, vec!["/health".to_string()])(handler(|r| async move {
        Response::new(200, r.header("X-User").unwrap_or("nobody"))
    }));
    let request = Request::new("GET", "/me").with_header("Authorization", "Bearer secret");
    println!("{}", app(request).await.body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tokens() -> HashMap<String, String> {
        HashMap::from([("t-ada".to_string(), "ada".to_string()), ("t-bob".to_string(), "bob".to_string())])
    }

    fn counted() -> (Handler, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let inner = handler(move |r| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Response::new(200, r.header("X-User").unwrap_or("nobody"))
            }
        });
        (require_token(tokens(), vec!["/health".to_string()])(inner), calls)
    }

    #[tokio::test]
    async fn valid_token_passes_and_identifies_the_user() {
        let (app, calls) = counted();
        for (header, expected) in [("Bearer t-ada", "ada"), ("bearer t-bob", "bob"), ("BEARER  t-ada ", "ada")] {
            let response = app(Request::new("GET", "/me").with_header("authorization", header)).await;
            assert_eq!((response.status, response.body.as_str()), (200, expected), "заголовок {header:?}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn rejected_requests_never_reach_the_handler() {
        let (app, calls) = counted();
        let attempts = [
            Request::new("GET", "/me"),
            Request::new("GET", "/me").with_header("Authorization", "Bearer nope"),
            Request::new("GET", "/me").with_header("Authorization", "Basic dDph"),
            Request::new("GET", "/me").with_header("Authorization", "t-ada"),
            Request::new("GET", "/me").with_header("Authorization", "Bearer"),
            Request::new("GET", "/me").with_header("X-User", "ada"),
        ];
        for request in attempts {
            let response = app(request.clone()).await;
            assert_eq!(response.status, 401, "{request:?}");
            assert_eq!(response.header("WWW-Authenticate"), Some("Bearer"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0, "обработчик не должен вызываться, пока токен не проверен");
    }

    #[tokio::test]
    async fn client_cannot_forge_x_user() {
        let (app, _) = counted();
        let forged = Request::new("GET", "/me").with_header("X-User", "admin").with_header("Authorization", "Bearer t-ada");
        assert_eq!(app(forged).await.body, "ada", "заголовок от клиента затирается настоящим");
        let public = Request::new("GET", "/health").with_header("X-User", "admin");
        let response = app(public).await;
        assert_eq!((response.status, response.body.as_str()), (200, "nobody"), "на открытом пути X-User всё равно не доходит до обработчика");
        let lower = Request::new("GET", "/health").with_header("x-user", "admin");
        assert_eq!(app(lower).await.body, "nobody", "имя заголовка без учёта регистра");
    }
}
