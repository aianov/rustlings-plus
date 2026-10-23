use obs::logs::JsonLines;
use serde_json::Value;
use social::{Request, Stack};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;

const CLIENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
const CLIENT_TRACE: &str = "0af7651916cd43dd8448eb211c80319c";
const CLIENT_SPAN: &str = "b7ad6b7169203331";

fn stack() -> Stack {
    let n = Arc::new(AtomicU64::new(1000));
    Stack::new(Arc::new(move || n.fetch_add(1, Ordering::SeqCst)))
}

fn logs() -> (JsonLines, tracing::subscriber::DefaultGuard) {
    let sink = JsonLines::default();
    let guard = tracing::subscriber::set_default(tracing_subscriber::registry().with(sink.clone()));
    (sink, guard)
}

#[derive(Debug, Clone, PartialEq)]
struct Hop {
    service: String,
    trace_id: String,
    span_id: String,
    parent_id: String,
}

fn hops(sink: &JsonLines) -> Vec<Hop> {
    let mut found: Vec<Hop> = Vec::new();
    for line in sink.lines() {
        let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("строка лога не JSON ({e}): {line}"));
        for span in v["spans"].as_array().expect("spans это массив") {
            if span["name"] != "hop" {
                continue;
            }
            let f = &span["fields"];
            let text = |k: &str| f[k].as_str().unwrap_or_else(|| panic!("в span hop нет строки {k}: {line}")).to_string();
            let hop = Hop { service: text("service"), trace_id: text("trace_id"), span_id: text("span_id"), parent_id: text("parent_id") };
            if !found.iter().any(|h| h.span_id == hop.span_id) {
                found.push(hop);
            }
        }
    }
    found
}

fn post(body: &str) -> Request {
    Request::new("POST", "/posts").body(body)
}

#[tokio::test]
async fn one_request_is_one_trace_visible_from_the_logs_alone() {
    let (sink, _guard) = logs();
    let stack = stack();
    let response = stack.gateway.handle(post("привет").header("traceparent", CLIENT)).await;
    assert_eq!(response.status, 201);
    assert_eq!(stack.notify.run_once().await, 1, "уведомление обработано");
    let hops = hops(&sink);
    let services: Vec<&str> = hops.iter().map(|h| h.service.as_str()).collect();
    assert_eq!(services, ["gateway", "posts", "notify"], "в логах должны быть события внутри span каждого из трёх сервисов: {hops:?}");
    assert!(hops.iter().all(|h| h.trace_id == CLIENT_TRACE), "трейс клиента должен дойти до последнего сервиса: {hops:?}");
    assert_eq!(hops[0].parent_id, CLIENT_SPAN, "родитель шлюза это span клиента");
    assert_eq!(hops[1].parent_id, hops[0].span_id, "родитель posts это шлюз");
    assert_eq!(hops[2].parent_id, hops[1].span_id, "родитель notify это posts: трейс должен пройти через шину");
    let mut spans: Vec<&String> = hops.iter().map(|h| &h.span_id).collect();
    spans.sort();
    spans.dedup();
    assert_eq!(spans.len(), 3, "у каждого перехода свой span_id");
}

#[tokio::test]
async fn a_request_without_a_trace_starts_one_that_reaches_the_end() {
    let (sink, _guard) = logs();
    let stack = stack();
    stack.gateway.handle(post("привет")).await;
    stack.notify.run_once().await;
    let hops = hops(&sink);
    assert_eq!(hops.len(), 3, "{hops:?}");
    assert_eq!(hops[0].parent_id, "", "у начала трейса родителя нет");
    assert!(hops.iter().all(|h| h.trace_id == hops[0].trace_id), "{hops:?}");
    assert_eq!(hops[2].parent_id, hops[1].span_id);
}

#[tokio::test]
async fn two_requests_do_not_mix_their_traces() {
    let (sink, _guard) = logs();
    let stack = stack();
    stack.gateway.handle(post("первый")).await;
    stack.gateway.handle(post("второй")).await;
    assert_eq!(stack.notify.run_once().await, 2);
    let hops = hops(&sink);
    assert_eq!(hops.len(), 6, "{hops:?}");
    let mut traces: Vec<&String> = hops.iter().map(|h| &h.trace_id).collect();
    traces.sort();
    traces.dedup();
    assert_eq!(traces.len(), 2, "два запроса это два трейса по три перехода: {hops:?}");
    for trace in traces {
        assert_eq!(hops.iter().filter(|h| &h.trace_id == trace).count(), 3);
    }
}

#[tokio::test]
async fn a_broken_traceparent_starts_a_new_trace_and_does_not_fail_the_request() {
    let (sink, _guard) = logs();
    let stack = stack();
    let response = stack.gateway.handle(post("привет").header("traceparent", "это не заголовок")).await;
    assert_eq!(response.status, 201, "кривой заголовок трассировки не повод отказывать пользователю");
    stack.notify.run_once().await;
    let hops = hops(&sink);
    assert_eq!(hops[0].parent_id, "");
    assert_eq!(hops.len(), 3);
}

#[tokio::test]
async fn secrets_never_reach_the_logs_but_the_access_line_is_there() {
    let (sink, _guard) = logs();
    let stack = stack();
    let req = Request::new("GET", "/health?page=1&token=SECRETTOKEN&API_KEY=SECRETKEY").header("Authorization", "Bearer SECRETBEARER").header("Cookie", "sid=SECRETCOOKIE").header("User-Agent", "curl/8.0");
    assert_eq!(stack.gateway.handle(req).await.status, 200);
    let all = sink.lines().join("\n");
    for secret in ["SECRETTOKEN", "SECRETKEY", "SECRETBEARER", "SECRETCOOKIE"] {
        assert!(!all.contains(secret), "секрет {secret} попал в логи:\n{all}");
    }
    assert!(all.contains("token=[redacted]") && all.contains("page=1") && all.contains("curl/8.0"), "строка доступа должна быть в логе, с пометками вместо секретов:\n{all}");
}

#[tokio::test]
async fn every_log_line_is_one_json_object_even_with_nasty_input() {
    let (sink, _guard) = logs();
    let stack = stack();
    stack.gateway.handle(Request::new("GET", "/a\"b\nINFO forged").header("User-Agent", "x\r\ny")).await;
    stack.gateway.handle(post("привет \"мир\"\nс переводом")).await;
    let lines = sink.lines();
    assert!(!lines.is_empty());
    for line in lines {
        assert!(!line.contains('\n'), "перевод строки внутри записи: {line:?}");
        serde_json::from_str::<Value>(&line).unwrap_or_else(|e| panic!("не JSON ({e}): {line}"));
    }
}

#[tokio::test]
async fn the_gateway_exposes_red_metrics_per_route() {
    let (_sink, _guard) = logs();
    let stack = stack();
    for _ in 0..3 {
        assert_eq!(stack.gateway.handle(post("ok")).await.status, 201);
    }
    assert_eq!(stack.gateway.handle(post("")).await.status, 422);
    assert_eq!(stack.gateway.handle(post("boom")).await.status, 500);
    for i in 0..4 {
        assert_eq!(stack.gateway.handle(Request::new("GET", &format!("/wp-admin/{i}"))).await.status, 404);
    }
    let scrape = stack.gateway.handle(Request::new("GET", "/metrics")).await;
    assert_eq!(scrape.status, 200);
    let text = scrape.body;
    for line in [
        "http_requests_total{route=\"/posts\",status=\"201\"} 3",
        "http_requests_total{route=\"/posts\",status=\"422\"} 1",
        "http_requests_total{route=\"/posts\",status=\"500\"} 1",
        "http_requests_total{route=\"unmatched\",status=\"404\"} 4",
        "http_request_duration_seconds_count{route=\"/posts\"} 5",
        "http_requests_in_flight 0",
    ] {
        assert!(text.lines().any(|l| l == line), "нет строки «{line}» на /metrics:\n{text}");
    }
    assert!(!text.contains("wp-admin"), "пути клиентов не должны попадать в метки:\n{text}");
    assert!(!text.contains("route=\"/metrics\""), "сам опрос метрик в RED не считается:\n{text}");
    assert!(text.contains("# TYPE http_request_duration_seconds histogram"), "{text}");
}

#[tokio::test]
async fn posts_and_notify_have_their_own_metrics() {
    let (_sink, _guard) = logs();
    let stack = stack();
    stack.gateway.handle(post("привет")).await;
    stack.gateway.handle(post("boom")).await;
    stack.notify.run_once().await;
    let posts = stack.posts.metrics();
    assert!(posts.contains("http_requests_total{route=\"create\",status=\"201\"} 1\n"), "{posts}");
    assert!(posts.contains("http_requests_total{route=\"create\",status=\"500\"} 1\n"), "{posts}");
    let notify = stack.notify.metrics();
    assert!(notify.contains("http_requests_total{route=\"post.created\",status=\"200\"} 1\n"), "{notify}");
}

#[tokio::test(start_paused = true)]
async fn durations_are_in_seconds_and_a_dropped_request_leaves_no_trace_in_the_gauges() {
    let (_sink, _guard) = logs();
    let stack = stack();
    assert_eq!(stack.gateway.handle(post("slow")).await.status, 201);
    let text = stack.gateway.metrics();
    assert!(text.contains("http_request_duration_seconds_sum{route=\"/posts\"} 5\n"), "медленный запрос шёл ровно 5 секунд: {text}");
    assert!(text.contains("http_request_duration_seconds_bucket{route=\"/posts\",le=\"5\"} 1\n") && text.contains("http_request_duration_seconds_bucket{route=\"/posts\",le=\"2.5\"} 0\n"), "{text}");

    let gave_up = tokio::time::timeout(Duration::from_secs(1), stack.gateway.handle(post("slow"))).await;
    assert!(gave_up.is_err(), "клиент не дождался");
    for (who, text) in [("шлюз", stack.gateway.metrics()), ("posts", stack.posts.metrics())] {
        assert!(text.contains("http_requests_in_flight 0\n"), "{who}: после отказа клиента запрос остался «в работе»: {text}");
    }
    assert_eq!(stack.bus.len(), 1, "брошенный запрос не успел опубликовать событие, в шине только первое");
}
