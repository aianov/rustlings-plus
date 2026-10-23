// Логирование, замер времени, проверка токена: всё это нужно каждому запросу и не имеет отношения к бизнес-логике.
// Копировать в каждый обработчик нельзя. Слой (middleware) принимает обработчик и возвращает новый, который
// делает что-то до и после вызова старого, как луковица: запрос идёт сквозь слои внутрь, ответ наружу.
// Типы и функции handler и middleware готовы.
// TODO: Напиши apply (первый слой в списке самый внешний), access_log (после ответа пишет в журнал строку
// "МЕТОД ПУТЬ СТАТУС") и timing (добавляет в ответ заголовок X-Elapsed-Ms: целые миллисекунды, пока работал обработчик).

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

fn apply(inner: Handler, layers: &[Middleware]) -> Handler {
    todo!()
}

fn access_log(log: Log) -> Middleware {
    todo!()
}

fn timing() -> Middleware {
    todo!()
}

#[tokio::main]
async fn main() {
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let app = apply(
        handler(|_| async { Response::new(200, "ok") }),
        &[access_log(log.clone()), timing()],
    );
    let response = app(Request::new("GET", "/hello")).await;
    println!("{:?} {:?}", response.header("X-Elapsed-Ms"), log.lock().unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tracer(name: &'static str, trace: Log) -> Middleware {
        middleware(move |next| {
            let trace = trace.clone();
            handler(move |request| {
                let next = next.clone();
                let trace = trace.clone();
                async move {
                    trace.lock().unwrap().push(format!("{name} in"));
                    let response = next(request).await;
                    trace.lock().unwrap().push(format!("{name} out"));
                    response
                }
            })
        })
    }

    fn slow(ms: u64) -> Handler {
        handler(move |_| async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            Response::new(200, "done")
        })
    }

    #[tokio::test]
    async fn first_layer_is_outermost() {
        let trace: Log = Arc::default();
        let inner = {
            let trace = trace.clone();
            handler(move |_| {
                let trace = trace.clone();
                async move {
                    trace.lock().unwrap().push("handler".to_string());
                    Response::new(200, "x")
                }
            })
        };
        let app = apply(inner, &[tracer("a", trace.clone()), tracer("b", trace.clone()), tracer("c", trace.clone())]);
        app(Request::new("GET", "/")).await;
        assert_eq!(*trace.lock().unwrap(), ["a in", "b in", "c in", "handler", "c out", "b out", "a out"]);
    }

    #[tokio::test]
    async fn no_layers_means_the_same_handler() {
        let app = apply(slow(0), &[]);
        assert_eq!(app(Request::new("GET", "/")).await.body, "done");
    }

    #[tokio::test(start_paused = true)]
    async fn log_and_timing() {
        let log: Log = Arc::default();
        let app = apply(slow(50), &[access_log(log.clone()), timing()]);
        for path in ["/a", "/b"] {
            let response = app(Request::new("GET", path)).await;
            assert_eq!(response.header("X-Elapsed-Ms"), Some("50"), "обработчик спал 50 мс");
            assert_eq!(response.status, 200);
        }
        assert_eq!(*log.lock().unwrap(), ["GET /a 200", "GET /b 200"], "слой работает на каждом запросе, а не на первом");
    }

    #[tokio::test(start_paused = true)]
    async fn timing_sees_the_inner_layers_work() {
        let app = apply(slow(10), &[timing(), middleware(|next| {
            handler(move |request| {
                let next = next.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    next(request).await
                }
            })
        })]);
        let response = app(Request::new("GET", "/")).await;
        assert_eq!(response.header("X-Elapsed-Ms"), Some("40"), "timing снаружи и меряет и внутренний слой, и обработчик");
    }
}
