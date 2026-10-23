use api_gateway::common::RequestCtx;
use api_gateway::pb::users_server::{Users, UsersServer};
use api_gateway::pb::{GetUserRequest, User};
use api_gateway::user_client::UserGateway;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

#[derive(Default)]
struct Seen {
    calls: AtomicUsize,
    connections: AtomicUsize,
    metadata: Mutex<Vec<(Option<String>, Option<String>)>>,
}

struct Fake {
    seen: Arc<Seen>,
}

#[tonic::async_trait]
impl Users for Fake {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        self.seen.calls.fetch_add(1, Ordering::SeqCst);
        let meta = |key: &str| request.metadata().get(key).and_then(|v| v.to_str().ok()).map(str::to_string);
        self.seen.metadata.lock().unwrap().push((meta("x-request-id"), meta("x-user-id")));
        match request.get_ref().id {
            404 => Err(Status::not_found("user 404 not found in db-7.internal")),
            403 => Err(Status::permission_denied("service token expired")),
            500 => Err(Status::unavailable("pool exhausted")),
            9000 => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(Response::new(User { id: 9000, name: "slow".to_string() }))
            }
            id => Ok(Response::new(User { id, name: format!("user-{id}") })),
        }
    }
}

async fn start() -> (String, Arc<Seen>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let seen = Arc::new(Seen::default());
    let counter = seen.clone();
    let incoming = TcpListenerStream::new(listener).map(move |connection| {
        counter.connections.fetch_add(1, Ordering::SeqCst);
        connection
    });
    let service = UsersServer::new(Fake { seen: seen.clone() });
    tokio::spawn(async move { tonic::transport::Server::builder().add_service(service).serve_with_incoming(incoming).await });
    (addr, seen)
}

fn ctx() -> RequestCtx {
    RequestCtx { request_id: "gw-7".to_string(), user_id: Some("u-1".to_string()) }
}

#[tokio::test]
async fn json_in_json_out() {
    let (addr, _) = start().await;
    let gateway = UserGateway::new(&addr);
    let reply = gateway.get_user(json!({"id": 7}), &ctx()).await.expect("вызов");
    assert_eq!(reply, json!({"id": 7, "name": "user-7"}), "JSON превращается в proto и обратно");
}

#[tokio::test]
async fn one_connection_serves_many_calls_even_concurrent_ones() {
    let (addr, seen) = start().await;
    let gateway = Arc::new(UserGateway::new(&addr));
    for id in 1..=5 {
        gateway.get_user(json!({"id": id}), &ctx()).await.unwrap();
    }
    let calls: Vec<_> = (10..30)
        .map(|id| {
            let gateway = gateway.clone();
            tokio::spawn(async move { gateway.get_user(json!({"id": id}), &ctx()).await.unwrap() })
        })
        .collect();
    for call in calls {
        call.await.unwrap();
    }
    assert_eq!(seen.calls.load(Ordering::SeqCst), 25, "все вызовы дошли");
    assert_eq!(seen.connections.load(Ordering::SeqCst), 1, "25 вызовов шли по одному соединению: канал создаётся один раз и клонируется, а не открывается на каждый вызов");
}

#[tokio::test]
async fn metadata_carries_the_request_and_the_user() {
    let (addr, seen) = start().await;
    let gateway = UserGateway::new(&addr);
    gateway.get_user(json!({"id": 1}), &ctx()).await.unwrap();
    gateway.get_user(json!({"id": 2}), &RequestCtx { request_id: "gw-8".to_string(), user_id: None }).await.unwrap();
    let recorded = seen.metadata.lock().unwrap().clone();
    assert_eq!(recorded[0], (Some("gw-7".to_string()), Some("u-1".to_string())), "номер запроса и пользователь доехали");
    assert_eq!(recorded[1], (Some("gw-8".to_string()), None), "для анонима x-user-id нет вообще");
}

#[tokio::test]
async fn slow_service_is_cut_by_the_deadline() {
    let (addr, _) = start().await;
    let gateway = UserGateway::new(&addr);
    let started = Instant::now();
    let error = gateway.get_user(json!({"id": 9000}), &ctx()).await.unwrap_err();
    assert_eq!(error.code, "deadline_exceeded", "сервис не ответил в срок");
    assert!(started.elapsed() < Duration::from_secs(2), "ждали {:?}, а срок вызова 300 мс: дедлайн должен быть задан на самом вызове", started.elapsed());
}

#[tokio::test]
async fn statuses_become_gateway_errors_without_leaking() {
    let (addr, _) = start().await;
    let gateway = UserGateway::new(&addr);
    let not_found = gateway.get_user(json!({"id": 404}), &ctx()).await.unwrap_err();
    assert_eq!(not_found.code, "not_found", "NotFound");
    assert!(!not_found.message.contains("db-7"), "текст соседа клиенту не уходит: {}", not_found.message);
    let down = gateway.get_user(json!({"id": 500}), &ctx()).await.unwrap_err();
    assert_eq!(down.code, "unavailable", "Unavailable");
    let denied = gateway.get_user(json!({"id": 403}), &ctx()).await.unwrap_err();
    assert_eq!(denied.code, "internal", "PermissionDenied при служебном вызове это наша поломка, а не 403 клиенту");
    assert!(!denied.message.contains("token"), "и без подробностей: {}", denied.message);
}

#[tokio::test]
async fn bad_data_does_not_reach_the_service() {
    let (addr, seen) = start().await;
    let gateway = UserGateway::new(&addr);
    for data in [json!({}), json!({"id": "7"}), json!({"id": -1}), Value::Null] {
        let error = gateway.get_user(data.clone(), &ctx()).await.unwrap_err();
        assert_eq!(error.code, "invalid_argument", "{data}");
    }
    assert_eq!(seen.calls.load(Ordering::SeqCst), 0, "сервис не побеспокоили");
}

#[tokio::test]
async fn unreachable_service_is_unavailable_and_quick() {
    let free = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().to_string()
    };
    let gateway = UserGateway::new(&free);
    let started = Instant::now();
    let error = gateway.get_user(json!({"id": 1}), &ctx()).await.unwrap_err();
    assert_eq!(error.code, "unavailable", "никто не слушает порт");
    assert!(started.elapsed() < Duration::from_secs(3), "отказ приходит быстро: {:?}", started.elapsed());
}
