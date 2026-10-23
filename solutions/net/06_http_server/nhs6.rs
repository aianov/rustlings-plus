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

#[tokio::main]
async fn main() {
    let router = Router::new().route(Method::Get, "/hello", handler(|_| async { Response::text(200, "hi") }));
    println!("{:?}", router.dispatch(Request::new(Method::Get, "/hello")).await);
    println!("{:?}", router.dispatch(Request::new(Method::Post, "/hello")).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router {
        Router::new()
            .route(Method::Post, "/items", handler(|_| async { Response::text(201, "created") }))
            .route(Method::Get, "/items", handler(|_| async { Response::text(200, "list") }))
            .route(Method::Delete, "/items", handler(|_| async { Response::text(204, "") }))
            .route(Method::Get, "/items", handler(|_| async { Response::text(200, "second") }))
            .route(Method::Get, "/health", handler(|_| async { Response::text(200, "ok") }))
    }

    #[tokio::test]
    async fn finds_by_method_and_path() {
        let r = router();
        assert_eq!(r.dispatch(Request::new(Method::Get, "/items")).await.body, "list", "первая регистрация побеждает");
        assert_eq!(r.dispatch(Request::new(Method::Post, "/items")).await.status, 201);
        assert_eq!(r.dispatch(Request::new(Method::Get, "/health")).await.body, "ok");
    }

    #[tokio::test]
    async fn unknown_path_is_404() {
        let r = router();
        for path in ["/nope", "/items/", "/Items", "", "/health/x"] {
            assert_eq!(r.dispatch(Request::new(Method::Get, path)).await.status, 404, "{path:?}: такого пути нет");
        }
    }

    #[tokio::test]
    async fn wrong_method_is_405_with_allow() {
        let r = router();
        let reply = r.dispatch(Request::new(Method::Put, "/items")).await;
        assert_eq!(reply.status, 405, "путь /items есть, метода PUT нет: это 405, а не 404");
        assert_eq!(reply.get_header("Allow"), Some("DELETE, GET, POST"), "по алфавиту, без повторов (GET зарегистрирован дважды)");
        let reply = r.dispatch(Request::new(Method::Post, "/health")).await;
        assert_eq!((reply.status, reply.get_header("Allow")), (405, Some("GET")));
    }
}
