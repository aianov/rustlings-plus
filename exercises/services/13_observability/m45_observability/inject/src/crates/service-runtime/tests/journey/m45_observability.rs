use std::sync::{Arc, Mutex};

use serde_json::Value;
use service_runtime::{
    HIDDEN, JsonLayer, Metrics, TraceContext, current_trace, escape_json, with_trace,
};
use tracing::Level;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

type Lines = Arc<Mutex<Vec<(Level, String)>>>;

fn capture() -> (impl tracing::Subscriber + Send + Sync, Lines) {
    let lines: Lines = Arc::default();
    let sink = Arc::clone(&lines);
    let layer = JsonLayer::new("test-service", move |level, line| {
        sink.lock().expect("строки").push((level, line.to_string()));
    });
    (Registry::default().with(layer), lines)
}

fn parsed(lines: &Lines) -> Vec<Value> {
    lines
        .lock()
        .expect("строки")
        .iter()
        .map(|(_, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("строка лога это JSON ({e}): {line}"))
        })
        .collect()
}

#[test]
fn events_are_json_lines_with_fields_and_span_context() {
    let (subscriber, lines) = capture();
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("http", request_id = "rid-1", trace_id = "abc");
        span.in_scope(|| {
            tracing::info!(user_id = 7, post_id = 42u64, ok = true, "пост опубликован");
            tracing::warn!(reason = "empty", "пост отклонён");
        });
        tracing::error!("вне запроса");
    });
    let lines_seen = lines.lock().expect("строки").clone();
    assert_eq!(lines_seen.len(), 3);
    assert_eq!(lines_seen[1].0, Level::WARN);
    let events = parsed(&lines);
    let first = &events[0];
    assert_eq!(first["level"], "INFO");
    assert_eq!(first["service"], "test-service");
    assert_eq!(first["message"], "пост опубликован");
    assert_eq!(first["user_id"], 7);
    assert_eq!(first["post_id"], 42);
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["request_id"], "rid-1",
        "поля span'а идут в каждое событие внутри"
    );
    assert_eq!(first["trace_id"], "abc");
    assert_eq!(first["spans"], "http");
    assert!(first["ts"].is_u64());
    assert_eq!(events[1]["reason"], "empty");
    assert!(
        events[2].get("request_id").is_none(),
        "вне span'а его полей нет"
    );
}

#[test]
fn secrets_never_reach_the_log() {
    let (subscriber, lines) = capture();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            authorization = "Bearer abc.def.ghi",
            password = "correct-horse",
            refresh_token = "r-123",
            header = "Bearer zzz-secret",
            "вход"
        );
        tracing::info!("пришёл заголовок Authorization: Bearer leaked-token-777");
    });
    let raw: String = lines
        .lock()
        .expect("строки")
        .iter()
        .map(|(_, line)| line.clone())
        .collect();
    for secret in [
        "abc.def.ghi",
        "correct-horse",
        "r-123",
        "zzz-secret",
        "leaked-token-777",
    ] {
        assert!(!raw.contains(secret), "{secret} попал в лог: {raw}");
    }
    let events = parsed(&lines);
    assert_eq!(events[0]["authorization"], HIDDEN);
    assert_eq!(events[0]["refresh_token"], HIDDEN);
    assert!(
        events[1]["message"]
            .as_str()
            .is_some_and(|m| m.ends_with(HIDDEN))
    );
}

#[test]
fn escaping_keeps_one_event_on_one_line() {
    let (subscriber, lines) = capture();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            text = "кавычка \" и \\ и\nперевод\tтаб\u{1}",
            "многострочное\nсообщение"
        );
    });
    let seen = lines.lock().expect("строки").clone();
    assert_eq!(seen.len(), 1);
    assert!(
        !seen[0].1.contains('\n'),
        "одно событие это одна строка: {}",
        seen[0].1
    );
    let event = &parsed(&lines)[0];
    assert_eq!(event["text"], "кавычка \" и \\ и\nперевод\tтаб\u{1}");
    assert_eq!(escape_json("a\u{2028}b"), "a\\u2028b");
}

#[test]
fn metrics_render_in_prometheus_text_format() {
    let metrics = Metrics::new();
    metrics.inc(
        "requests_total",
        "запросы",
        &[("route", "/users/:name"), ("status", "200")],
        1,
    );
    metrics.inc(
        "requests_total",
        "запросы",
        &[("status", "200"), ("route", "/users/:name")],
        2,
    );
    metrics.inc(
        "requests_total",
        "запросы",
        &[("route", "/a\"b"), ("status", "500")],
        1,
    );
    metrics.set("connections", "открытые соединения", &[], 5);
    metrics.add("connections", "открытые соединения", &[], -2);
    for seconds in [0.003, 0.02, 0.2, 7.0, 30.0] {
        metrics.observe(
            "latency_seconds",
            "время ответа",
            &[("route", "/x")],
            seconds,
        );
    }
    let text = metrics.render();
    assert_eq!(text.matches("# TYPE requests_total counter").count(), 1);
    assert!(
        text.contains("requests_total{route=\"/users/:name\",status=\"200\"} 3"),
        "{text}"
    );
    assert!(
        text.contains("requests_total{route=\"/a\\\"b\",status=\"500\"} 1"),
        "{text}"
    );
    assert!(
        text.contains("# TYPE connections gauge\nconnections 3"),
        "{text}"
    );
    assert!(text.contains("# TYPE latency_seconds histogram"), "{text}");
    assert!(
        text.contains("latency_seconds_bucket{route=\"/x\",le=\"0.005\"} 1"),
        "{text}"
    );
    assert!(
        text.contains("latency_seconds_bucket{route=\"/x\",le=\"0.25\"} 3"),
        "накопительно: {text}"
    );
    assert!(
        text.contains("latency_seconds_bucket{route=\"/x\",le=\"10\"} 4"),
        "{text}"
    );
    assert!(
        text.contains("latency_seconds_bucket{route=\"/x\",le=\"+Inf\"} 5"),
        "{text}"
    );
    assert!(
        text.contains("latency_seconds_count{route=\"/x\"} 5"),
        "{text}"
    );
    assert!(
        text.contains("latency_seconds_sum{route=\"/x\"} 37.223"),
        "{text}"
    );
}

#[test]
fn traceparent_is_parsed_strictly() {
    let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let parsed = TraceContext::parse(header).expect("пример из W3C");
    assert_eq!(parsed.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(parsed.span_id, "00f067aa0ba902b7");
    assert!(parsed.sampled);
    assert_eq!(parsed.header(), header);
    for bad in [
        "",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
        "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
        "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
    ] {
        assert_eq!(TraceContext::parse(bad), None, "{bad}");
    }
    let child = parsed.child();
    assert_eq!(child.trace_id, parsed.trace_id, "ребёнок в том же трейсе");
    assert_ne!(child.span_id, parsed.span_id, "но со своим span");
    let fresh = TraceContext::fresh();
    assert_eq!(TraceContext::parse(&fresh.header()), Some(fresh.clone()));
    assert_eq!(
        TraceContext::continue_from(Some(header)).trace_id,
        parsed.trace_id
    );
    assert_ne!(
        TraceContext::continue_from(Some("мусор")).trace_id,
        parsed.trace_id
    );
}

#[tokio::test]
async fn the_current_trace_follows_the_task() {
    assert_eq!(current_trace(), None);
    let trace = TraceContext::fresh();
    let seen = with_trace(trace.clone(), async { current_trace() }).await;
    assert_eq!(seen, Some(trace));
    assert_eq!(current_trace(), None, "вне scope трейса нет");
}
