use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use e2e::{Stack, StackConfig};
use loadgen::{
    Closed, Histogram, Hit, Mode, Open, Ramp, Report, Sample, Scenario, Step, Stop, compare,
    parse_args, ramp, run_closed, run_open, stack_target,
};

#[test]
fn histogram_quantiles_stay_within_one_percent() {
    let mut histogram = Histogram::new();
    for value in 1..=100_000u64 {
        histogram.record(value);
    }
    for (q, exact) in [(0.5, 50_000.0), (0.9, 90_000.0), (0.99, 99_000.0)] {
        let got = histogram.quantile(q) as f64;
        assert!(
            (got - exact).abs() / exact < 0.01,
            "q{q}: {got} против {exact}"
        );
    }
    assert_eq!(histogram.quantile(1.0), 100_000);
    assert_eq!(
        (histogram.min(), histogram.max(), histogram.count()),
        (1, 100_000, 100_000)
    );
    assert!((histogram.mean() - 50_000.5).abs() < 0.001);
    let mut small = Histogram::new();
    for value in [0, 1, 2, 127, 128] {
        small.record(value);
    }
    assert_eq!(small.quantile(0.2), 0, "маленькие значения точные");
    assert_eq!(small.quantile(0.8), 127);
}

#[test]
fn histograms_merge_like_one() {
    let mut left = Histogram::new();
    let mut right = Histogram::new();
    let mut both = Histogram::new();
    for value in 0..5_000u64 {
        let value = value * value % 1_000_003;
        if value % 2 == 0 {
            left.record(value);
        } else {
            right.record(value);
        }
        both.record(value);
    }
    left.merge(&right);
    assert_eq!(left, both, "слияние воркеров не теряет и не искажает");
    let empty = Histogram::new();
    assert_eq!(empty.quantile(0.99), 0);
}

fn fake_hit(base_ms: u64, slow_every: u64) -> Hit {
    let calls = Arc::new(AtomicU64::new(0));
    Arc::new(move |_worker| {
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            let n = calls.fetch_add(1, Ordering::Relaxed) + 1;
            let ms = if slow_every > 0 && n % slow_every == 0 {
                base_ms * 10
            } else {
                base_ms
            };
            tokio::time::sleep(Duration::from_millis(ms)).await;
            vec![loadgen_sample(
                "fake",
                Duration::from_millis(ms),
                n % 50 != 0,
            )]
        })
    })
}

fn loadgen_sample(step: &'static str, took: Duration, ok: bool) -> Sample {
    Sample { step, took, ok }
}

#[tokio::test(start_paused = true)]
async fn closed_model_keeps_users_busy() {
    let stats = run_closed(
        Closed {
            users: 4,
            duration: Duration::from_secs(1),
        },
        fake_hit(10, 0),
    )
    .await;
    let report = stats.report("закрытая");
    assert!(
        (380..=420).contains(&report.requests),
        "4 пользователя по 10 мс за секунду: {report:?}"
    );
    assert_eq!(report.errors, 8, "каждый пятидесятый с ошибкой");
    assert!((report.p99_ms - 10.0).abs() < 0.2, "{report:?}");
}

#[tokio::test(start_paused = true)]
async fn open_model_keeps_the_schedule_and_counts_queueing() {
    let stats = run_open(
        Open {
            rate: 100.0,
            duration: Duration::from_secs(2),
            workers: 1,
        },
        fake_hit(20, 0),
    )
    .await;
    let report = stats.report("открытая");
    assert_eq!(
        report.requests, 200,
        "прибытия по расписанию, а не по готовности"
    );
    assert!(
        report.p99_ms > 1000.0,
        "один исполнитель на 20 мс не держит 100 в секунду: очередь видна в задержке, {report:?}"
    );
}

#[tokio::test]
async fn ramp_stops_at_the_first_step_over_the_limit() {
    let plan = Ramp {
        start: 10.0,
        factor: 2.0,
        steps: 10,
        p99: Duration::from_millis(100),
        errors: 0.05,
    };
    let outcome = ramp(plan, |rate| async move {
        Report {
            label: format!("{rate}"),
            seconds: 1.0,
            requests: rate as u64,
            errors: 0,
            error_rate: 0.0,
            rps: rate,
            p50_ms: rate / 4.0,
            p90_ms: rate / 2.0,
            p99_ms: rate,
            max_ms: rate,
            steps: Vec::new(),
        }
    })
    .await;
    assert_eq!(outcome.reports.len(), 5, "10, 20, 40, 80, 160");
    assert_eq!(outcome.best.map(|(rate, _)| rate), Some(80.0));
    assert_eq!(
        outcome.stop,
        Stop::Latency {
            rate: 160.0,
            p99_ms: 160.0
        }
    );
    let errors = ramp(plan, |rate| async move {
        let mut report = Report::build("ошибки", 1.0, &Histogram::new(), 0, Vec::new());
        report.error_rate = if rate > 15.0 { 0.2 } else { 0.0 };
        report
    })
    .await;
    assert!(matches!(errors.stop, Stop::Errors { rate, .. } if rate == 20.0));
}

#[test]
fn reports_compare_with_the_previous_run() {
    let mut total = Histogram::new();
    for value in [1_000u64, 2_000, 3_000, 100_000] {
        total.record(value);
    }
    let before = Report::build("вчера", 2.0, &total, 1, Vec::new());
    assert_eq!(before.requests, 5);
    assert!((before.rps - 2.5).abs() < 1e-9);
    assert!((before.error_rate - 0.2).abs() < 1e-9);
    let restored = Report::from_json(&before.to_json()).expect("отчёт читается обратно");
    assert_eq!(restored, before);
    let mut after = before.clone();
    after.label = String::from("сегодня");
    after.p99_ms = before.p99_ms * 2.0;
    let text = compare(&before, &after);
    assert!(text.contains("p99 +100.0%"), "{text}");
    assert!(text.contains("хуже прошлого прогона"), "{text}");
    assert!(before.render().contains("p99"));
}

#[test]
fn arguments_parse_into_options() {
    let args: Vec<String> = [
        "--model",
        "open",
        "--rate",
        "200",
        "--users",
        "8",
        "--scenario",
        "login,feed",
        "--duration",
        "3",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let options = parse_args(&args, Some(String::from("127.0.0.1:17880"))).expect("флаги верны");
    assert_eq!(options.mode, Mode::Open);
    assert_eq!(options.gateway, "127.0.0.1:17880");
    assert_eq!(options.scenario.steps, [Step::Login, Step::Feed]);
    assert_eq!(options.duration, Duration::from_secs(3));
    assert!(parse_args(&[String::from("--ramp")], None).is_ok_and(|o| o.mode == Mode::Ramp));
    assert!(parse_args(&[String::from("--users"), String::from("0")], None).is_err());
    assert!(Scenario::parse("login,dance").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_short_load_through_the_stack_stays_fast_and_clean() {
    let stack = Stack::start_with(StackConfig {
        gateway: api_gateway::Config {
            limits: api_gateway::RateLimits {
                user_rate: 1_000_000,
                user_burst: 1_000_000,
                ip_rate: 1_000_000,
                ip_burst: 1_000_000,
                ..api_gateway::RateLimits::default()
            },
            ..api_gateway::Config::default()
        },
        ..StackConfig::default()
    })
    .await;
    let gateway = stack.http.to_string();
    let scenario = Scenario::parse("login,post,feed,message").expect("сценарий");
    let hit = stack_target(&gateway, 4, scenario)
        .await
        .expect("пользователи нагрузки готовы");
    let report = run_closed(
        Closed {
            users: 4,
            duration: Duration::from_secs(2),
        },
        hit,
    )
    .await
    .report("стек в процессе");
    assert_eq!(
        report.errors,
        0,
        "под нагрузкой нет ошибок: {}",
        report.render()
    );
    assert!(report.requests >= 40, "нагрузка шла: {}", report.render());
    assert!(
        report.p99_ms < 1_000.0,
        "p99 в пределах секунды: {}",
        report.render()
    );
    for step in ["login", "post", "feed", "message"] {
        assert!(
            report
                .steps
                .iter()
                .any(|own| own.name == step && own.requests > 0),
            "шаг {step} в отчёте: {}",
            report.render()
        );
    }
    stack.stop().await;
}
