// Клиенты шлют HEAD, чтобы узнать размер и тип ресурса, не скачивая его: ответ должен быть ровно таким же,
// как на GET, но без тела. Наш сервер этого не умеет. HEAD /page получает 405, а если бы получил 200, то
// вместе с телом, и лишние байты стали бы началом следующего ответа на соединении. Правила: заголовки как
// у GET (Content-Length считается по настоящему телу GET-ответа), тела нет, в Allow для GET добавляется HEAD.
// TODO: Почини Router::dispatch и to_wire. Явно зарегистрированный HEAD-обработчик важнее запасного пути
// через GET.

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

struct Router {
    routes: Vec<(Method, String, Handler)>,
}

impl Router {
    fn new() -> Router {
        Router { routes: Vec::new() }
    }

    fn route(mut self, method: Method, path: &str, handler: Handler) -> Router {
        self.routes.push((method, path.to_string(), handler));
        self
    }

    async fn dispatch(&self, request: Request) -> Response {
        let mut allowed: Vec<&str> = Vec::new();
        for (method, path, handler) in &self.routes {
            if *path == request.path {
                if *method == request.method {
                    return handler(request).await;
                }
                allowed.push(method.as_str());
            }
        }
        if allowed.is_empty() {
            return Response::text(404, "not found");
        }
        allowed.sort_unstable();
        allowed.dedup();
        Response::text(405, "method not allowed").header("Allow", &allowed.join(", "))
    }
}

fn to_wire(response: &Response, head_only: bool) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {} X\r\n", response.status).into_bytes();
    for (name, value) in &response.headers {
        out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    out.extend_from_slice(format!("Content-Length: {}\r\n\r\n", response.body.len()).as_bytes());
    out.extend_from_slice(response.body.as_bytes());
    out
}

async fn respond(router: &Router, request: Request) -> Vec<u8> {
    let head_only = request.method == Method::Head;
    to_wire(&router.dispatch(request).await, head_only)
}

#[tokio::main]
async fn main() {
    let router = Router::new().route(Method::Get, "/hi", handler(|_| async { Response::text(200, "hello") }));
    let bytes = respond(&router, Request::new(Method::Head, "/hi")).await;
    println!("{}", String::from_utf8_lossy(&bytes));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router {
        Router::new()
            .route(Method::Get, "/page", handler(|_| async { Response::text(200, "привет").header("Content-Type", "text/plain") }))
            .route(Method::Get, "/both", handler(|_| async { Response::text(200, "get-body") }))
            .route(Method::Head, "/both", handler(|_| async { Response::text(200, "").header("X-Custom", "head") }))
            .route(Method::Post, "/form", handler(|_| async { Response::text(201, "made") }))
    }

    fn split(bytes: &[u8]) -> (String, Vec<u8>) {
        let at = bytes.windows(4).position(|w| w == b"\r\n\r\n").expect("нет конца головы") + 4;
        (String::from_utf8(bytes[..at].to_vec()).unwrap(), bytes[at..].to_vec())
    }

    #[tokio::test]
    async fn get_is_untouched() {
        let (head, body) = split(&respond(&router(), Request::new(Method::Get, "/page")).await);
        assert!(head.starts_with("HTTP/1.1 200"));
        assert!(head.contains("Content-Length: 12\r\n"), "{head:?}");
        assert_eq!(body, "привет".as_bytes());
    }

    #[tokio::test]
    async fn head_is_get_without_body() {
        let get = split(&respond(&router(), Request::new(Method::Get, "/page")).await).0;
        let (head, body) = split(&respond(&router(), Request::new(Method::Head, "/page")).await);
        assert!(head.starts_with("HTTP/1.1 200"), "HEAD должен находить GET-обработчик: {head:?}");
        assert_eq!(head, get, "заголовки HEAD совпадают с GET, включая Content-Length настоящего тела");
        assert!(body.is_empty(), "после пустой строки в ответе на HEAD не должно быть ни байта, а там {body:?}");
    }

    #[tokio::test]
    async fn explicit_head_route_wins() {
        let (head, body) = split(&respond(&router(), Request::new(Method::Head, "/both")).await);
        assert!(head.contains("X-Custom: head\r\n"), "зарегистрированный HEAD-обработчик важнее запасного GET: {head:?}");
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn errors_and_allow() {
        let (head, body) = split(&respond(&router(), Request::new(Method::Head, "/missing")).await);
        assert!(head.starts_with("HTTP/1.1 404") && body.is_empty(), "у ответа на HEAD тела нет и при ошибке");
        let (head, _) = split(&respond(&router(), Request::new(Method::Head, "/form")).await);
        assert!(head.starts_with("HTTP/1.1 405"), "у /form нет GET, значит HEAD тоже нельзя: {head:?}");
        assert!(head.contains("Allow: POST\r\n"), "{head:?}");
        let (head, _) = split(&respond(&router(), Request::new(Method::Delete, "/page")).await);
        assert!(head.contains("Allow: GET, HEAD\r\n"), "если разрешён GET, то разрешён и HEAD: {head:?}");
    }
}
