use loadgen::{Limits, LogHistogram, Report, Tolerance, compare, run_closed, run_open, run_steps};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const MS: fn(u64) -> Duration = Duration::from_millis;


#[allow(dead_code)]
mod fake {
    use loadgen::Target;
    use futures::future::BoxFuture;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::{Instant, sleep};

    pub type ServiceTime = Box<dyn Fn(u64, Duration) -> Duration + Send + Sync>;
    pub type Failure = Box<dyn Fn(u64) -> bool + Send + Sync>;

    pub struct Fake {
        born: Instant,
        connect_delay: Duration,
        service: ServiceTime,
        fail: Failure,
        connect_failures: u64,
        pub connects: AtomicU64,
        pub requests: AtomicU64,
        pub in_flight: AtomicUsize,
        pub peak: AtomicUsize,
    }

    impl Fake {
        pub fn new(service: ServiceTime) -> Fake {
            Fake { born: Instant::now(), connect_delay: Duration::ZERO, service, fail: Box::new(|_| false), connect_failures: 0, connects: AtomicU64::new(0), requests: AtomicU64::new(0), in_flight: AtomicUsize::new(0), peak: AtomicUsize::new(0) }
        }

        pub fn constant(service: Duration) -> Fake {
            Fake::new(Box::new(move |_, _| service))
        }

        pub fn with_connect_delay(mut self, delay: Duration) -> Fake {
            self.connect_delay = delay;
            self
        }

        pub fn with_failures(mut self, fail: Failure) -> Fake {
            self.fail = fail;
            self
        }

        pub fn with_connect_failures(mut self, first: u64) -> Fake {
            self.connect_failures = first;
            self
        }

        pub fn connects(&self) -> u64 {
            self.connects.load(Ordering::SeqCst)
        }

        pub fn requests(&self) -> u64 {
            self.requests.load(Ordering::SeqCst)
        }

        pub fn peak(&self) -> usize {
            self.peak.load(Ordering::SeqCst)
        }
    }

    impl Target for Fake {
        fn connect(&self) -> BoxFuture<'_, Result<u64, String>> {
            Box::pin(async move {
                sleep(self.connect_delay).await;
                let id = self.connects.fetch_add(1, Ordering::SeqCst);
                if id < self.connect_failures { Err("не подключились".to_string()) } else { Ok(id) }
            })
        }

        fn request(&self, _conn: u64) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                let n = self.requests.fetch_add(1, Ordering::SeqCst);
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.peak.fetch_max(now, Ordering::SeqCst);
                sleep((self.service)(n, self.born.elapsed())).await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                if (self.fail)(n) { Err(format!("запрос {n} упал")) } else { Ok(()) }
            })
        }
    }
}

use fake::Fake;

fn within(real: Duration, estimate: Duration, what: &str) {
    let (r, e) = (real.as_micros() as f64, estimate.as_micros() as f64);
    assert!(e >= r - 1.0 && e <= r * (1.0 + 1.0 / 128.0) + 1.0, "{what}: настоящее {r} мкс, оценка {e} мкс, ошибка больше одной 128-й");
}

#[test]
fn the_histogram_has_bounded_memory_and_bounded_error() {
    let mut h = LogHistogram::new(7);
    let size = h.buckets_len();
    let mut real: Vec<u64> = Vec::new();
    let mut x: u64 = 88172645463325252;
    for _ in 0..40_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let v = 30 + (x % 3_000_000) * (x % 5) / 4;
        real.push(v);
        h.record(Duration::from_micros(v));
    }
    real.sort_unstable();
    assert_eq!(h.buckets_len(), size, "память не растёт с числом замеров");
    for q in [0.5, 0.9, 0.99, 0.999] {
        let exact = real[((q * real.len() as f64).ceil() as usize).max(1) - 1];
        within(Duration::from_micros(exact), h.value_at_quantile(q), &format!("квантиль {q}"));
    }
    assert_eq!((h.min(), h.max()), (Duration::from_micros(real[0]), Duration::from_micros(*real.last().expect("не пусто"))));
}

#[test]
fn merging_two_histograms_equals_recording_into_one() {
    let (mut a, mut b, mut whole) = (LogHistogram::new(7), LogHistogram::new(7), LogHistogram::new(7));
    for i in 1..=4000u64 {
        let v = Duration::from_micros(i * i % 80_000 + 10);
        if i % 2 == 0 { a.record(v) } else { b.record(v) }
        whole.record(v);
    }
    a.merge(&b);
    assert_eq!(a.count(), whole.count());
    for q in [0.0, 0.5, 0.95, 0.99, 1.0] {
        assert_eq!(a.value_at_quantile(q), whole.value_at_quantile(q), "квантиль {q}");
    }
    assert_eq!(LogHistogram::new(7).value_at_quantile(0.99), Duration::ZERO, "пустая гистограмма даёт ноль");
}

#[tokio::test(start_paused = true)]
async fn closed_loop_counts_workers_connections_and_latency() {
    let fake = Arc::new(Fake::constant(MS(20)));
    let report = run_closed(fake.clone(), 4, Duration::from_secs(1)).await;
    assert_eq!((report.ok, report.errors, report.connections), (200, 0, 4), "{report:?}");
    assert_eq!(fake.connects(), 4, "соединение на воркера, а не на запрос");
    assert_eq!(fake.peak(), 4, "воркеры работают одновременно");
    within(MS(20), report.p50, "p50");
    within(MS(20), report.p99, "p99");
    assert_eq!(report.max, MS(20));
    assert_eq!(report.rps, 200.0);
}

#[tokio::test(start_paused = true)]
async fn closed_loop_keeps_failures_out_of_latencies_and_survives_a_dead_connect() {
    let fake = Arc::new(Fake::new(Box::new(|n, _| if n % 5 == 4 { MS(1) } else { MS(20) })).with_failures(Box::new(|n| n % 5 == 4)).with_connect_failures(1));
    let report = run_closed(fake.clone(), 3, Duration::from_secs(1), ).await;
    assert_eq!(report.connections, 2, "один из трёх воркеров не подключился");
    assert!(report.errors > 20, "ошибки: неудачное подключение и каждый пятый запрос: {report:?}");
    within(MS(20), report.p50, "быстрые отказы не должны попасть в p50");
    assert_eq!(report.max, MS(20));
}

#[tokio::test(start_paused = true)]
async fn open_loop_follows_the_schedule_whatever_the_server_does() {
    let fake = Arc::new(Fake::new(Box::new(|_, t| if t >= MS(200) && t < MS(1200) { MS(1200) - t } else { MS(5) })));
    let report = run_open(fake.clone(), 100, Duration::from_secs(2)).await;
    assert_eq!(report.ok, 200, "расписание не подстраивается под сервис: {report:?}");
    assert_eq!(report.max, MS(1000), "самый ранний из замёрзших ждал секунду от назначенного момента");
    assert!(report.p99 >= MS(950), "p99 {:?}: сотня запросов провела в очереди от 10 мс до секунды", report.p99);
    assert!(fake.peak() >= 90, "запросы копились одновременно: {}", fake.peak());
}

#[tokio::test(start_paused = true)]
async fn open_loop_reuses_connections_and_counts_the_connect_in_latency() {
    let fake = Arc::new(Fake::constant(MS(5)).with_connect_delay(MS(50)));
    let report = run_open(fake.clone(), 10, Duration::from_secs(1)).await;
    assert_eq!(report.connections, 1, "ответ быстрее интервала: одно соединение на всех");
    assert_eq!(report.max, MS(55), "первый запрос включает 50 мс подключения");
    within(MS(5), report.p50, "p50");
}

struct Sluggish {
    in_flight: AtomicU64,
}

impl loadgen::Target for Sluggish {
    fn connect(&self) -> futures::future::BoxFuture<'_, Result<u64, String>> {
        Box::pin(async { Ok(0) })
    }

    fn request(&self, _conn: u64) -> futures::future::BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let n = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            tokio::time::sleep(MS(12) + MS(8) * (n as u32 - 1)).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[tokio::test(start_paused = true)]
async fn steps_stop_at_the_first_step_beyond_the_limits() {
    let target = Arc::new(Sluggish { in_flight: AtomicU64::new(0) });
    let limits = Limits { max_p99: MS(40), max_error_share: 0.01 };
    let outcome = run_steps(target, &[10, 25, 50, 100, 200, 400], Duration::from_secs(2), &limits).await;
    let rates: Vec<u32> = outcome.steps.iter().map(|(r, _)| *r).collect();
    assert!(rates.len() < 6, "после первой ступени, не уложившейся в предел, дальше идти незачем: {rates:?}");
    let last = outcome.steps.last().expect("есть хотя бы одна ступень");
    assert!(last.1.p99 > limits.max_p99, "последняя выполненная ступень и есть провалившаяся: {:?}", last.1.p99);
    let good = outcome.sustainable.expect("на малых скоростях всё в порядке");
    assert_eq!(good, rates[rates.len() - 2], "предел это предыдущая ступень: {rates:?}");
    assert!(outcome.steps.iter().take(rates.len() - 1).all(|(_, r)| r.p99 <= limits.max_p99), "все ступени до предела уложились");
}

#[tokio::test(start_paused = true)]
async fn steps_without_any_good_step_have_no_limit() {
    let fake = Arc::new(Fake::constant(MS(300)));
    let limits = Limits { max_p99: MS(100), max_error_share: 0.01 };
    let outcome = run_steps(fake, &[10, 20], Duration::from_secs(1), &limits).await;
    assert_eq!(outcome.sustainable, None);
    assert_eq!(outcome.steps.len(), 1);
}

fn report(ok: u64, errors: u64, rps: f64, p99_ms: u64) -> Report {
    Report { ok, errors, connections: 4, elapsed: Duration::from_secs(10), rps, p50: MS(10), p99: MS(p99_ms), max: MS(p99_ms * 2) }
}

#[test]
fn a_new_run_is_compared_with_the_previous_one() {
    let prev = report(1000, 0, 100.0, 50);
    let tol = Tolerance::default();
    assert!(compare(&prev, &report(1000, 0, 98.0, 55), &tol).is_empty(), "небольшое колебание в пределах допуска");
    let slower = compare(&prev, &report(1000, 0, 100.0, 80), &tol);
    assert_eq!(slower.len(), 1, "p99 вырос на 60%: {slower:?}");
    assert!(slower[0].contains("p99"), "{slower:?}");
    let fewer = compare(&prev, &report(800, 0, 80.0, 50), &tol);
    assert!(fewer.len() == 1 && fewer[0].contains("скорость"), "{fewer:?}");
    let errors = compare(&prev, &report(900, 100, 100.0, 50), &tol);
    assert!(errors.len() == 1 && errors[0].contains("ошибок"), "{errors:?}");
    assert_eq!(compare(&prev, &report(100, 900, 10.0, 500), &tol).len(), 3, "все три регрессии названы отдельно");
    assert!(compare(&prev, &report(2000, 0, 200.0, 20), &tol).is_empty(), "улучшение регрессией не считается");
}

#[test]
fn a_report_survives_a_round_trip_through_a_line() {
    let original = report(1234, 5, 123.4, 77);
    let line = original.to_line();
    assert!(!line.contains('\n'), "отчёт занимает одну строку, чтобы дописывать его в файл истории");
    assert_eq!(Report::from_line(&line), Some(original));
    assert_eq!(Report::from_line("не json"), None);
    assert_eq!(Report::from_line("{\"ok\": 1}"), None, "неполный отчёт не разбирается");
}
