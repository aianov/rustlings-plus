use std::sync::{Arc, Mutex};

use api_gateway::Config;
use contracts::UserView;
use grpc_kit::LogSink;
use service_runtime::{JsonLayer, TraceContext};
use social_cli::Web;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use crate::support::{Gateway, ok, within};

type Lines = Arc<Mutex<Vec<String>>>;

fn capture() -> (tracing::subscriber::DefaultGuard, Lines) {
    let lines: Lines = Arc::default();
    let sink = Arc::clone(&lines);
    let layer = JsonLayer::new("stack", move |_, line| {
        sink.lock().expect("строки").push(line.to_string());
    });
    let guard = tracing::subscriber::set_default(Registry::default().with(layer));
    (guard, lines)
}

fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("\"{name}\":\"");
    let start = line.find(&key)? + key.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

#[tokio::test]
async fn one_trace_runs_through_the_gateway_and_the_services() {
    let (_guard, lines) = capture();
    let stack = Gateway::observed(
        Config {
            access_log: true,
            ..Config::default()
        },
        LogSink::stderr(),
    )
    .await;
    stack.register("alice").await;
    let login = stack.login("alice").await;
    for _ in 0..2 {
        let _ = within(
            "прогрев: точки событий регистрируются при уже поставленном подписчике",
            Web::new(&stack.http.to_string()).request("GET", "/users/alice", None, None),
        )
        .await;
    }
    let parent = TraceContext::fresh();
    let reply = within(
        "профиль с traceparent",
        Web::new(&stack.http.to_string()).send(
            "GET",
            "/users/alice",
            None,
            &[
                ("traceparent", &parent.header()),
                ("Authorization", &format!("Bearer {}", login.token)),
            ],
        ),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(reply.status, 200, "{reply:?}");
    let echoed = reply
        .header("traceparent")
        .and_then(TraceContext::parse)
        .expect("шлюз возвращает свой traceparent");
    assert_eq!(echoed.trace_id, parent.trace_id, "трейс клиента продолжен");
    let rid = reply
        .header("x-request-id")
        .expect("request-id в ответе")
        .to_string();
    let mut peer = stack.peer_with(&login.token).await;
    let ws_parent = TraceContext::fresh();
    peer.raw(&format!(
        r#"{{"id": 1, "service": "users", "method": "users.find", "data": {{"username": "alice"}}, "meta": {{"traceparent": "{}"}}}}"#,
        ws_parent.header()
    ))
    .await;
    let found = peer.any_reply().await;
    assert!(found.contains("\"ok\""), "{found}");
    let _: UserView = ok!(peer, "users.find", r#"{"username": "alice"}"#.to_string() => UserView);
    let admin = stack.admin.expect("у стенда есть admin-порт");
    let metrics = within(
        "метрики шлюза",
        Web::new(&admin.to_string()).request("GET", "/metrics", None, None),
    )
    .await
    .expect("admin отвечает");
    assert!(
        metrics.body.contains(
            "http_requests_total{method=\"GET\",route=\"/users/:username\",status=\"200\"}"
        ),
        "RED по шаблону маршрута: {}",
        metrics.body
    );
    assert!(metrics.body.contains("http_request_duration_seconds_bucket{method=\"GET\",route=\"/users/:username\",le=\"+Inf\"}"));
    assert!(
        metrics
            .body
            .contains("ws_requests_total{method=\"users.find\",outcome=\"ok\"}")
    );
    assert!(metrics.body.contains("grpc_server_requests_total{code=\"0\",method=\"/social.v1.Users/FindProfile\",service=\"user-service\"}"), "{}", metrics.body);
    stack.stop().await;
    let lines = lines.lock().expect("строки").clone();
    let http_trace: Vec<&String> = lines
        .iter()
        .filter(|line| field(line, "trace_id") == Some(parent.trace_id.as_str()))
        .collect();
    assert!(
        http_trace
            .iter()
            .any(|line| field(line, "target") == Some("access") && field(line, "server").is_none()),
        "строка доступа шлюза несёт trace_id клиента: {http_trace:#?}"
    );
    assert!(
        http_trace
            .iter()
            .any(|line| field(line, "server") == Some("user-service")),
        "и строка user-service того же запроса: {http_trace:#?}"
    );
    assert!(
        http_trace
            .iter()
            .any(|line| field(line, "request_id") == Some(rid.as_str())),
        "по request-id из ответа находится тот же путь: {http_trace:#?}"
    );
    assert!(
        lines.iter().any(
            |line| field(line, "trace_id") == Some(ws_parent.trace_id.as_str())
                && field(line, "server") == Some("user-service")
        ),
        "traceparent из meta конверта WebSocket доходит до user-service"
    );
    let all = lines.join("\n");
    assert!(
        !all.contains(&login.token),
        "токен доступа не попадает в логи"
    );
    assert!(!all.contains("correct-horse"), "пароль не попадает в логи");
}
