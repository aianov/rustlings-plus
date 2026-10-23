use obs::metrics::Registry;
use obs::red::Red;
use obs::redact::{Req, access_line};
use obs::trace::{TraceContext, enter, incoming, parse};
use std::sync::Arc;
use std::time::Duration;

const GOOD: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

fn counter(start: u64) -> impl FnMut() -> u64 {
    let mut n = start;
    move || {
        n += 1;
        n
    }
}

#[test]
fn traceparent_is_parsed_strictly() {
    let ctx = parse(GOOD).expect("правильный заголовок");
    assert_eq!((ctx.trace_id, ctx.span_id, ctx.sampled), (0x4bf92f3577b34da6a3ce929d0e0e4736, 0x00f067aa0ba902b7, true));
    for bad in [
        "",
        "мусор",
        "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-x",
        "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
    ] {
        assert_eq!(parse(bad), None, "заголовок {bad:?} должен отвергаться");
    }
    assert_eq!(parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-future"), parse(GOOD), "версию выше 00 читаем по известному префиксу");
}

#[test]
fn headers_round_trip_with_fixed_width() {
    let ctx = TraceContext { trace_id: 0xabc, span_id: 0x12, sampled: false };
    assert_eq!(ctx.header(), "00-00000000000000000000000000000abc-0000000000000012-00");
    assert_eq!(parse(&ctx.header()), Some(ctx));
}

#[test]
fn children_keep_the_trace_and_change_the_span() {
    let parent = parse(GOOD).expect("заголовок");
    let mut script = vec![0u64, 0x00f067aa0ba902b7, 9].into_iter();
    let child = parent.child(&mut || script.next().expect("хватает чисел"));
    assert_eq!((child.trace_id, child.span_id, child.sampled), (parent.trace_id, 9, true), "нулевой и родительский span_id пропускаются");
}

#[test]
fn incoming_continues_valid_headers_and_starts_roots_otherwise() {
    let continued = incoming(Some(GOOD), &mut counter(50));
    assert_eq!((continued.trace_id, continued.span_id), (0x4bf92f3577b34da6a3ce929d0e0e4736, 51));
    for header in [None, Some("мусор")] {
        let root = incoming(header, &mut counter(50));
        assert_ne!(root.trace_id, 0);
        assert!(root.sampled);
    }
}

#[test]
fn enter_reports_the_parent_span() {
    let entered = enter(Some(GOOD), &mut counter(7));
    assert_eq!(entered.parent, Some(0x00f067aa0ba902b7));
    assert_eq!(entered.ctx.span_id, 8);
    assert_eq!(enter(None, &mut counter(7)).parent, None);
    assert_eq!(enter(Some("мусор"), &mut counter(7)).parent, None, "мусорный заголовок это нет родителя");
}

#[test]
fn counters_render_with_sorted_labels_and_escaping() {
    let reg = Registry::new();
    reg.inc("hits_total", "Обращения", &[("status", "200"), ("route", "/a")]);
    reg.inc("hits_total", "Обращения", &[("route", "/a"), ("status", "200")]);
    reg.inc("hits_total", "Обращения", &[("route", "/b\"x\\\n"), ("status", "500")]);
    assert_eq!(
        reg.render(),
        "# HELP hits_total Обращения\n# TYPE hits_total counter\n\
         hits_total{route=\"/a\",status=\"200\"} 2\n\
         hits_total{route=\"/b\\\"x\\\\\\n\",status=\"500\"} 1\n",
        "метки отсортированы по имени, порядок при вызове не важен, кавычка, обратная черта и перевод строки в значении экранируются"
    );
}

#[test]
fn gauges_and_counters_without_labels_have_no_braces() {
    let reg = Registry::new();
    reg.gauge_add("open", "Открыто", 3);
    reg.gauge_add("open", "Открыто", -5);
    reg.inc("plain_total", "Без меток", &[]);
    let text = reg.render();
    assert!(text.contains("open -2\n"), "{text}");
    assert!(text.contains("plain_total 1\n"), "{text}");
    assert!(text.find("# HELP open").expect("есть open") < text.find("# HELP plain_total").expect("есть plain_total"), "метрики по алфавиту");
}

#[test]
fn histograms_are_cumulative_per_label_set() {
    let reg = Registry::new();
    for v in [0.05, 0.25, 0.25, 2.0] {
        reg.observe("lat_seconds", "Время", &[0.1, 0.5], &[("route", "/a")], v);
    }
    reg.observe("lat_seconds", "Время", &[0.1, 0.5], &[("route", "/b")], 0.5);
    reg.observe("lat_seconds", "Время", &[0.1, 0.5], &[("route", "/b")], f64::NAN);
    let text = reg.render();
    for line in [
        "lat_seconds_bucket{route=\"/a\",le=\"0.1\"} 1",
        "lat_seconds_bucket{route=\"/a\",le=\"0.5\"} 3",
        "lat_seconds_bucket{route=\"/a\",le=\"+Inf\"} 4",
        "lat_seconds_sum{route=\"/a\"} 2.55",
        "lat_seconds_count{route=\"/a\"} 4",
        "lat_seconds_bucket{route=\"/b\",le=\"0.5\"} 1",
        "lat_seconds_count{route=\"/b\"} 1",
    ] {
        assert!(text.lines().any(|l| l == line), "нет строки «{line}» в выводе:\n{text}");
    }
    assert!(text.contains("# TYPE lat_seconds histogram\n"));
}

#[test]
fn registry_is_safe_to_share_between_threads() {
    let reg = Arc::new(Registry::new());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let reg = reg.clone();
            std::thread::spawn(move || {
                for _ in 0..2_000 {
                    reg.inc("n_total", "n", &[("k", "v")]);
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("поток не упал");
    }
    assert!(reg.render().contains("n_total{k=\"v\"} 16000\n"));
}

#[tokio::test(start_paused = true)]
async fn red_counts_each_finished_request_with_status_and_duration() {
    let reg = Arc::new(Registry::new());
    let red = Red::new(reg.clone());
    let out = red
        .track("/feed", async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            (200, "готово")
        })
        .await;
    assert_eq!(out, "готово", "обёртка возвращает то, что вернула работа");
    red.track("/feed", async { (503, ()) }).await;
    let text = reg.render();
    assert!(text.contains("http_requests_total{route=\"/feed\",status=\"200\"} 1\n"), "{text}");
    assert!(text.contains("http_requests_total{route=\"/feed\",status=\"503\"} 1\n"), "{text}");
    assert!(text.contains("http_request_duration_seconds_count{route=\"/feed\"} 2\n"), "{text}");
    assert!(text.contains("http_request_duration_seconds_sum{route=\"/feed\"} 0.02\n"), "длительность в секундах, 20 мс это 0.02: {text}");
    assert!(text.contains("http_requests_in_flight 0\n"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn red_in_flight_drops_back_when_the_caller_gives_up() {
    let reg = Arc::new(Registry::new());
    let red = Red::new(reg.clone());
    let slow = red.track("/slow", async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        (200, ())
    });
    assert!(tokio::time::timeout(Duration::from_millis(10), slow).await.is_err());
    let text = reg.render();
    assert!(text.contains("http_requests_in_flight 0\n"), "бросили на полпути, а «в работе» застряло: {text}");
    assert!(!text.contains("status=\"200\""), "брошенный запрос в завершённые не попадает: {text}");
}

fn headers(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
}

#[test]
fn access_lines_hide_secrets_and_cannot_be_forged() {
    let h = headers(&[("User-Agent", "curl/8.0"), ("Authorization", "Bearer SECRET1"), ("Cookie", "s=SECRET2")]);
    let line = access_line(&Req { method: "GET", target: "/export?page=2&token=SECRET3&%73ig=SECRET4", headers: &h }, 200);
    for secret in ["SECRET1", "SECRET2", "SECRET3", "SECRET4"] {
        assert!(!line.contains(secret), "{secret} попал в лог: {line}");
    }
    assert!(line.contains("page=2") && line.contains("token=[redacted]") && line.contains("user-agent=\"curl/8.0\"") && line.contains("authorization=\"[redacted]\""), "{line}");
    let h = headers(&[("User-Agent", "x\r\nINFO forged")]);
    let line = access_line(&Req { method: "GET", target: "/a\nb", headers: &h }, 200);
    assert!(line.chars().all(|c| !c.is_control()), "данные клиента разорвали строку: {line:?}");
}
