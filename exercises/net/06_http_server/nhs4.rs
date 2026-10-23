// Обработчик приветствует посетителя и считает визиты. Компилятор в ужасе: выдаёт длинные ошибки про Send
// и про время жизни, и в них легко потеряться. Читай их сверху вниз: каждая называет значение, из-за
// которого будущее не пролезает в tokio::spawn.
// TODO: Почини greeter, чтобы Handler можно было вызывать из любых задач на любых потоках, а счётчик
// не врал, даже если его дёргают одновременно. Приветствие должно жить дольше строки, из которой оно взято.

use std::cell::Cell;
use std::rc::Rc;

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

fn greeter(greeting: &str) -> Handler {
    let seen = Rc::new(Cell::new(0u32));
    handler(move |req| {
        let seen = seen.clone();
        async move {
            seen.set(seen.get() + 1);
            Response::text(200, &format!("{greeting}, {} (визит {})", req.path, seen.get()))
        }
    })
}

#[tokio::main]
async fn main() {
    let greeting = String::from("Привет");
    let h = greeter(&greeting);
    println!("{}", h(Request::new(Method::Get, "/a")).await.body);
    println!("{}", h(Request::new(Method::Get, "/b")).await.body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn visit(body: &str) -> u32 {
        let start = body.rfind("(визит ").expect("в ответе нет '(визит N)'") + "(визит ".len();
        body[start..body.len() - 1].parse().expect("N должно быть числом")
    }

    #[tokio::test]
    async fn greeting_outlives_its_source() {
        let h = {
            let temporary = String::from("Здравствуй");
            greeter(&temporary)
        };
        let reply = h(Request::new(Method::Get, "/x")).await;
        assert_eq!(reply.body, "Здравствуй, /x (визит 1)");
        assert_eq!(h(Request::new(Method::Get, "/y")).await.body, "Здравствуй, /y (визит 2)", "счётчик общий для вызовов одного обработчика");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn counts_visits_from_many_threads() {
        let h = Arc::new(greeter("Hi"));
        let mut tasks = Vec::new();
        for _ in 0..200 {
            let h = h.clone();
            tasks.push(tokio::spawn(async move {
                tokio::task::yield_now().await;
                visit(&h(Request::new(Method::Get, "/")).await.body)
            }));
        }
        let mut seen = Vec::new();
        for task in tasks {
            seen.push(task.await.unwrap());
        }
        seen.sort_unstable();
        assert_eq!(seen, (1..=200).collect::<Vec<u32>>(), "каждый визит должен получить свой номер, без повторов и пропусков");
    }
}
