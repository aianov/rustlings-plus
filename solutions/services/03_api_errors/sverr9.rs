
use futures::FutureExt;
use futures::future::BoxFuture;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};

pub struct Request {
    pub id: String,
    pub path: String,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

pub type Handler = Arc<dyn Fn(Request) -> BoxFuture<'static, Response> + Send + Sync>;

pub struct Server {
    handler: Handler,
    log: Arc<Mutex<Vec<String>>>,
}

impl Server {
    pub fn new(handler: Handler, log: Arc<Mutex<Vec<String>>>) -> Server {
        Server { handler, log }
    }

    pub async fn handle(&self, request: Request) -> Response {
        let id = request.id.clone();
        let handler = self.handler.clone();
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(move || handler(request)));
        let outcome = match outcome {
            Ok(future) => AssertUnwindSafe(future).catch_unwind().await,
            Err(payload) => Err(payload),
        };
        match outcome {
            Ok(response) => response,
            Err(payload) => {
                let text = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "паника без текста".to_string());
                self.log.lock().unwrap_or_else(|p| p.into_inner()).push(format!("request_id={id} panic: {text}"));
                Response {
                    status: 500,
                    body: format!(r#"{{"error":{{"code":"internal","message":"внутренняя ошибка","request_id":"{id}"}}}}"#),
                }
            }
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn server(handler: Handler) -> (Arc<Server>, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        (Arc::new(Server::new(handler, log.clone())), log)
    }

    fn request(id: &str, path: &str) -> Request {
        Request { id: id.to_string(), path: path.to_string() }
    }

    fn app() -> Handler {
        Arc::new(|req: Request| -> BoxFuture<'static, Response> {
            match req.path.as_str() {
                "/ok" => async { Response { status: 200, body: "ok".to_string() } }.boxed(),
                "/panic-in-future" => async move {
                    let items: Vec<u32> = Vec::new();
                    let _first = items[0];
                    Response { status: 200, body: "unreachable".to_string() }
                }
                .boxed(),
                "/panic-before-future" => panic!("боевой секрет: пароль базы hunter2"),
                "/slow" => async {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    Response { status: 200, body: "slow".to_string() }
                }
                .boxed(),
                _ => async { Response { status: 404, body: "no".to_string() } }.boxed(),
            }
        })
    }

    async fn run(server: &Arc<Server>, req: Request) -> Response {
        let server = server.clone();
        tokio::time::timeout(Duration::from_secs(5), tokio::spawn(async move { server.handle(req).await }))
            .await
            .expect("ответ не пришёл вовремя")
            .expect("задача соединения не должна умирать из-за паники в обработчике")
    }

    #[tokio::test(start_paused = true)]
    async fn normal_requests_are_untouched() {
        let (server, log) = server(app());
        let response = run(&server, request("r1", "/ok")).await;
        assert_eq!(response, Response { status: 200, body: "ok".to_string() }, "обычный ответ проходит как есть");
        assert!(log.lock().unwrap().is_empty(), "без паники в логе пусто");
    }

    #[tokio::test(start_paused = true)]
    async fn panic_inside_the_future_becomes_500() {
        let (server, log) = server(app());
        let response = run(&server, request("req-5", "/panic-in-future")).await;
        assert_eq!(response.status, 500, "паника в обработчике это 500");
        assert!(response.body.contains("req-5") && response.body.contains("internal"), "в теле код и номер запроса: {}", response.body);
        let log = log.lock().unwrap();
        assert!(log.iter().any(|l| l.contains("req-5") && l.contains("index out of bounds")), "текст паники и номер запроса в логе: {log:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn panic_before_the_future_is_caught_too() {
        let (server, log) = server(app());
        let response = run(&server, request("req-6", "/panic-before-future")).await;
        assert_eq!(response.status, 500, "обработчик упал, не успев вернуть future");
        assert!(!response.body.contains("hunter2"), "текст паники нельзя отдавать клиенту: {}", response.body);
        assert!(log.lock().unwrap().iter().any(|l| l.contains("hunter2")), "а в лог он должен попасть целиком");
    }

    #[tokio::test(start_paused = true)]
    async fn server_keeps_serving_after_a_panic() {
        let (server, _log) = server(app());
        for _ in 0..3 {
            assert_eq!(run(&server, request("r", "/panic-in-future")).await.status, 500, "паника");
            assert_eq!(run(&server, request("r", "/ok")).await.status, 200, "после паники сервер жив");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn neighbours_are_not_affected() {
        let (server, _log) = server(app());
        let slow = tokio::spawn({
            let server = server.clone();
            async move { server.handle(request("slow", "/slow")).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(run(&server, request("boom", "/panic-in-future")).await.status, 500, "сосед упал");
        let response = slow.await.expect("медленный запрос не пострадал");
        assert_eq!(response.status, 200, "медленный запрос заканчивается нормально, хотя рядом была паника");
    }
}
