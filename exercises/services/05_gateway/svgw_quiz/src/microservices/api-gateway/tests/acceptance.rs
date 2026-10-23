use api_gateway::backend::Backend;
use api_gateway::gateway::{Config, Gateway};
use api_gateway::proxy::{ProxyConfig, ProxyRequest, ProxyResponse, Upstream, UpstreamError};
use api_gateway::router::{Ctx, RouterBuilder};
use api_gateway::types::{CheckError, GwError, ManualClock, Metadata, Principal, TokenCheck};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Default)]
struct Spy {
    calls: Mutex<Vec<(String, Metadata)>>,
    posts_mode: Mutex<&'static str>,
}

impl Backend for Spy {
    fn get_user(&self, id: u64, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>> {
        self.calls.lock().unwrap().push(("get_user".to_string(), meta.clone()));
        async move {
            match id {
                13 => panic!("backend сломался"),
                99 => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok(json!({"id": 99, "name": "медленный"}))
                }
                id => Ok(json!({"id": id, "name": format!("user-{id}")})),
            }
        }
        .boxed()
    }

    fn list_posts(&self, _: u64, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>> {
        self.calls.lock().unwrap().push(("list_posts".to_string(), meta.clone()));
        let mode = *self.posts_mode.lock().unwrap();
        async move {
            match mode {
                "hang" => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    Ok(json!([]))
                }
                "fail" => Err(GwError::new("unavailable", "posts down")),
                _ => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    Ok(json!([1, 2]))
                }
            }
        }
        .boxed()
    }

    fn stats(&self, _: u64, _: &Metadata) -> BoxFuture<'_, Result<Value, GwError>> {
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(json!({"friends": 3}))
        }
        .boxed()
    }

    fn create_post(&self, text: &str, meta: &Metadata) -> BoxFuture<'_, Result<Value, GwError>> {
        self.calls.lock().unwrap().push(("create_post".to_string(), meta.clone()));
        let text = text.to_string();
        async move { Ok(json!({"text": text})) }.boxed()
    }
}

struct Checker;

impl TokenCheck for Checker {
    fn validate(&self, token: &str) -> Result<(Principal, u64), CheckError> {
        match token {
            "alice" => Ok((Principal { user_id: "u-1".to_string(), role: "user".to_string() }, 1_000)),
            "old" => Err(CheckError::Expired),
            _ => Err(CheckError::Invalid),
        }
    }
}

struct EchoUpstream {
    seen: Mutex<Vec<ProxyRequest>>,
    mode: Mutex<&'static str>,
}

impl Upstream for EchoUpstream {
    fn send(&self, request: ProxyRequest) -> BoxFuture<'_, Result<ProxyResponse, UpstreamError>> {
        self.seen.lock().unwrap().push(request);
        let mode = *self.mode.lock().unwrap();
        async move {
            match mode {
                "down" => Err(UpstreamError::Connect),
                "slow" => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Ok(ProxyResponse { status: 200, headers: vec![], body: vec![] })
                }
                _ => Ok(ProxyResponse {
                    status: 200,
                    headers: vec![("Content-Type".to_string(), "application/json".to_string()), ("Connection".to_string(), "close".to_string())],
                    body: b"{}".to_vec(),
                }),
            }
        }
        .boxed()
    }
}

struct World {
    gateway: Arc<Gateway>,
    spy: Arc<Spy>,
    upstream: Arc<EchoUpstream>,
    clock: Arc<ManualClock>,
}

fn world(max_in_flight: usize) -> World {
    let spy = Arc::new(Spy::default());
    let upstream = Arc::new(EchoUpstream { seen: Mutex::new(Vec::new()), mode: Mutex::new("ok") });
    let clock = Arc::new(ManualClock::new(100));
    let gateway = Gateway::new(Config {
        backend: spy.clone(),
        checker: Arc::new(Checker),
        clock: clock.clone(),
        upstream: upstream.clone(),
        max_in_flight,
        proxy: ProxyConfig { max_body: 1024, timeout: Duration::from_millis(500) },
    })
    .unwrap_or_else(|dups| panic!("в готовом наборе маршрутов дубликаты: {dups:?}"));
    World { gateway: Arc::new(gateway), spy, upstream, clock }
}

fn bearer(token: &str) -> Vec<(String, String)> {
    vec![("Authorization".to_string(), format!("Bearer {token}"))]
}

async fn run(world: &World, headers: Vec<(String, String)>, frames: Vec<Value>) -> Vec<Value> {
    let session = world.gateway.open(&headers, "").expect("подключение");
    let (tx_in, rx_in) = mpsc::channel(100);
    let (tx_out, mut rx_out) = mpsc::channel(100);
    let server = tokio::spawn(world.gateway.clone().serve(session, rx_in, tx_out));
    for frame in frames {
        tx_in.send(frame.to_string()).await.unwrap();
    }
    drop(tx_in);
    let mut replies = Vec::new();
    while let Some(reply) = rx_out.recv().await {
        replies.push(reply);
    }
    server.await.expect("serve не паникует");
    replies
}

fn frame(id: u64, service: &str, method: &str, data: Value) -> Value {
    json!({"id": id, "service": service, "method": method, "data": data})
}

fn meta_value(meta: &Metadata, key: &str) -> Vec<String> {
    meta.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect()
}

#[tokio::test(start_paused = true)]
async fn connect_checks_the_token_once_and_refuses_bad_ones() {
    let w = world(8);
    assert!(w.gateway.open(&bearer("alice"), "").is_ok(), "годный токен");
    assert!(w.gateway.open(&[], "").is_ok(), "аноним");
    assert!(w.gateway.open(&[], "token=alice").is_ok(), "токен в запросе");
    assert_eq!(w.gateway.open(&bearer("old"), "").unwrap_err().code, "token_expired", "просрочен");
    assert_eq!(w.gateway.open(&bearer("junk"), "").unwrap_err().code, "unauthenticated", "мусор");
}

#[tokio::test(start_paused = true)]
async fn replies_carry_the_frame_id_and_a_request_id() {
    let w = world(8);
    let replies = run(&w, vec![], vec![frame(7, "user", "get", json!({"id": 5}))]).await;
    assert_eq!(replies.len(), 1, "один ответ");
    assert_eq!(replies[0]["id"], 7, "id кадра");
    assert_eq!(replies[0]["data"]["name"], "user-5", "данные из backend");
    assert!(replies[0]["request_id"].as_str().is_some_and(|s| !s.is_empty()), "request_id для поддержки: {}", replies[0]);
}

#[tokio::test(start_paused = true)]
async fn unknown_method_and_bad_data_are_client_errors() {
    let w = world(8);
    let replies = run(&w, vec![], vec![frame(1, "user", "nope", Value::Null), frame(2, "user", "get", json!({"id": "x"}))]).await;
    let by_id = |id: u64| replies.iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(by_id(1)["error"]["code"], "not_found", "нет метода");
    assert_eq!(by_id(2)["error"]["code"], "invalid_argument", "плохие данные");
}

#[tokio::test(start_paused = true)]
async fn protected_methods_need_a_login_and_get_the_trusted_user() {
    let w = world(8);
    let anon = run(&w, vec![], vec![frame(1, "post", "create", json!({"text": "привет"}))]).await;
    assert_eq!(anon[0]["error"]["code"], "unauthenticated", "аноним не публикует");
    assert!(w.spy.calls.lock().unwrap().iter().all(|(name, _)| name != "create_post"), "backend не вызывался");
    let user = run(&w, bearer("alice"), vec![frame(2, "post", "create", json!({"text": "привет"}))]).await;
    assert_eq!(user[0]["ok"], true, "вошедший публикует: {}", user[0]);
    let calls = w.spy.calls.lock().unwrap();
    let meta = &calls.iter().find(|(name, _)| name == "create_post").unwrap().1;
    assert_eq!(meta_value(meta, "x-user-id"), vec!["u-1"], "backend получил проверенного пользователя");
}

#[tokio::test(start_paused = true)]
async fn clients_cannot_forge_internal_metadata() {
    let w = world(8);
    let forged = json!({"id": 1, "service": "post", "method": "create", "data": {"text": "x"},
        "meta": {"x-user-id": "u-999", "x-user-role": "admin", "x-internal-skip": "1", "x-trace-id": "t-1", "request_id": "app-5"}});
    run(&w, bearer("alice"), vec![forged]).await;
    let calls = w.spy.calls.lock().unwrap();
    let meta = &calls[0].1;
    assert_eq!(meta_value(meta, "x-user-id"), vec!["u-1"], "user-id только из токена");
    assert_eq!(meta_value(meta, "x-user-role"), vec!["user"], "роль только из токена");
    assert!(meta_value(meta, "x-internal-skip").is_empty(), "внутренние ключи не пропускаются");
    assert_eq!(meta_value(meta, "x-trace-id"), vec!["t-1"], "разрешённое проходит");
    assert_eq!(meta_value(meta, "x-request-id"), vec!["app-5"], "безопасный номер клиента принят");
}

#[tokio::test(start_paused = true)]
async fn requests_of_one_connection_run_in_parallel_and_replies_come_out_of_order() {
    let w = world(8);
    let started = Instant::now();
    let replies = run(&w, vec![], vec![frame(1, "user", "get", json!({"id": 99})), frame(2, "user", "get", json!({"id": 5}))]).await;
    let ids: Vec<u64> = replies.iter().map(|r| r["id"].as_u64().unwrap()).collect();
    assert_eq!(ids, vec![2, 1], "быстрый ответ первым: {replies:?}");
    assert_eq!(started.elapsed(), Duration::from_millis(100), "вместе 100 мс");
}

#[tokio::test(start_paused = true)]
async fn too_many_in_flight_are_refused_at_once() {
    let w = world(2);
    let frames: Vec<Value> = (0..6).map(|i| frame(i, "user", "get", json!({"id": 99}))).collect();
    let replies = run(&w, vec![], frames).await;
    let refused = replies.iter().filter(|r| r["error"]["code"] == "resource_exhausted").count();
    let served = replies.iter().filter(|r| r["ok"] == true).count();
    assert_eq!((served, refused), (2, 4), "два в работе, остальные отказ: {replies:?}");
}

#[tokio::test(start_paused = true)]
async fn a_panic_in_a_backend_is_an_internal_error_for_that_request_only() {
    let w = world(8);
    let replies = run(&w, vec![], vec![frame(1, "user", "get", json!({"id": 13})), frame(2, "user", "get", json!({"id": 5}))]).await;
    let by_id = |id: u64| replies.iter().find(|r| r["id"] == id).unwrap_or_else(|| panic!("нет ответа на {id}: {replies:?}")).clone();
    assert_eq!(by_id(1)["error"]["code"], "internal", "паника это ответ internal с id запроса");
    assert!(!by_id(1).to_string().contains("сломался"), "текст паники не уходит клиенту");
    assert_eq!(by_id(2)["ok"], true, "сосед жив");
}

#[tokio::test(start_paused = true)]
async fn the_token_expiry_closes_the_connection() {
    let w = world(8);
    let session = w.gateway.open(&bearer("alice"), "").unwrap();
    let (tx_in, rx_in) = mpsc::channel(10);
    let (tx_out, mut rx_out) = mpsc::channel(10);
    let server = tokio::spawn(w.gateway.clone().serve(session, rx_in, tx_out));
    tx_in.send(frame(1, "user", "get", json!({"id": 5})).to_string()).await.unwrap();
    assert_eq!(rx_out.recv().await.unwrap()["ok"], true, "до срока работает");
    w.clock.set(1_000);
    tx_in.send(frame(2, "user", "get", json!({"id": 5})).to_string()).await.unwrap();
    let reply = rx_out.recv().await.unwrap();
    assert_eq!((reply["id"].clone(), reply["error"]["code"].clone()), (json!(2), json!("token_expired")), "{reply}");
    assert!(rx_out.recv().await.is_none(), "после этого шлюз закрывает соединение");
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn profile_is_assembled_in_parallel_with_partial_results() {
    let w = world(8);
    let started = Instant::now();
    let replies = run(&w, bearer("alice"), vec![frame(1, "profile", "full", json!({"id": 5}))]).await;
    assert_eq!(started.elapsed(), Duration::from_millis(100), "три запроса параллельно");
    assert_eq!(replies[0]["data"]["partial"], json!([]), "всё получено");
    *w.spy.posts_mode.lock().unwrap() = "hang";
    let started = Instant::now();
    let replies = run(&w, bearer("alice"), vec![frame(2, "profile", "full", json!({"id": 5}))]).await;
    assert_eq!(started.elapsed(), Duration::from_millis(150), "зависшие посты стоят ровно свой бюджет");
    assert_eq!(replies[0]["data"]["partial"], json!(["posts"]), "частичный ответ: {}", replies[0]);
    assert_eq!(replies[0]["data"]["user"]["id"], 5, "пользователь на месте");
    let anon = run(&w, vec![], vec![frame(3, "profile", "full", json!({"id": 5}))]).await;
    assert_eq!(anon[0]["error"]["code"], "unauthenticated", "профиль только для вошедших");
}

#[test]
fn duplicate_routes_are_a_startup_error() {
    #[derive(Deserialize)]
    struct Nothing {}
    let result = RouterBuilder::new()
        .route("a", "b", |_: Ctx, _: Nothing| async { Ok(json!(1)) })
        .route_authed("a", "b", |_: Principal, _: Ctx, _: Nothing| async { Ok(json!(2)) })
        .route("c", "d", |_: Ctx, _: Nothing| async { Ok(json!(3)) })
        .route("c", "d", |_: Ctx, _: Nothing| async { Ok(json!(4)) })
        .build();
    assert_eq!(result.err(), Some(vec!["a.b".to_string(), "c.d".to_string()]), "дубликаты названы, тип регистрации не важен");
}

fn proxy_request(path: &str, headers: &[(&str, &str)], body: usize) -> ProxyRequest {
    ProxyRequest {
        method: "POST".to_string(),
        path: path.to_string(),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: vec![b'x'; body],
        peer_ip: "203.0.113.9".to_string(),
    }
}

#[tokio::test(start_paused = true)]
async fn auth_proxy_passes_login_and_hides_the_connection_details() {
    let w = world(8);
    let headers = [("Connection", "keep-alive, X-Hop"), ("X-Hop", "1"), ("X-Forwarded-For", "6.6.6.6"), ("X-Real-IP", "6.6.6.6"), ("Content-Type", "application/json")];
    let response = w.gateway.proxy_auth(proxy_request("/auth/login", &headers, 10)).await;
    assert_eq!(response.status, 200, "ответ апстрима");
    assert!(response.headers.iter().all(|(k, _)| !k.eq_ignore_ascii_case("connection")), "hop-by-hop в ответе нет");
    let seen = w.upstream.seen.lock().unwrap();
    let sent = &seen[0];
    assert_eq!(sent.path, "/login", "префикс /auth убран");
    let get = |name: &str| sent.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect::<Vec<_>>();
    assert_eq!(get("x-forwarded-for"), vec!["203.0.113.9"], "адрес клиента наш, не подделка");
    assert_eq!(get("x-real-ip"), vec!["203.0.113.9"], "x-real-ip тоже");
    assert!(get("connection").is_empty() && get("x-hop").is_empty(), "заголовки соединения не передаются");
    assert_eq!(get("content-type"), vec!["application/json"], "обычные остаются");
}

#[tokio::test(start_paused = true)]
async fn auth_proxy_protects_itself() {
    let w = world(8);
    assert_eq!(w.gateway.proxy_auth(proxy_request("/auth/register", &[], 1025)).await.status, 413, "большое тело");
    assert_eq!(w.gateway.proxy_auth(proxy_request("/users/1", &[], 0)).await.status, 404, "чужой путь");
    *w.upstream.mode.lock().unwrap() = "down";
    assert_eq!(w.gateway.proxy_auth(proxy_request("/auth/login", &[], 0)).await.status, 502, "апстрим недоступен");
    *w.upstream.mode.lock().unwrap() = "slow";
    assert_eq!(w.gateway.proxy_auth(proxy_request("/auth/login", &[], 0)).await.status, 504, "апстрим медленный");
    assert_eq!(w.upstream.seen.lock().unwrap().len(), 2, "до апстрима дошли только два вызова");
}
