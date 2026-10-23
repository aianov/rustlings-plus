// Нужен свой генератор нагрузки: `wrk` и `k6` не знают протокол нашей соцсети. Начнём с самого простого, закрытого цикла: N воркеров,
// каждый держит своё соединение и шлёт запросы один за другим, как только пришёл ответ, пока не кончится время. Цель: `run_closed`
// запускает воркеров параллельно, каждый подключается один раз (keep-alive), замеряет каждый запрос, ошибки считает отдельно и в
// замеры не включает. По итогу собирается отчёт: число успешных, ошибок, соединений, скорость, p50, p99 и максимум. Новые запросы
// начинаются, пока не наступил срок, а начатый запрос дожидается ответа.


use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}


pub trait Target: Send + Sync + 'static {
    fn connect(&self) -> BoxFuture<'_, Result<u64, String>>;
    fn request(&self, conn: u64) -> BoxFuture<'_, Result<(), String>>;
}



#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub ok: u64,
    pub errors: u64,
    pub connections: u64,
    pub elapsed: Duration,
    pub rps: f64,
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
}

impl Report {
    pub fn build(mut samples: Vec<Duration>, errors: u64, connections: u64, elapsed: Duration) -> Report {
        samples.sort();
        let ok = samples.len() as u64;
        let rps = if elapsed.is_zero() { 0.0 } else { ok as f64 / elapsed.as_secs_f64() };
        Report { ok, errors, connections, elapsed, rps, p50: percentile(&samples, 0.5), p99: percentile(&samples, 0.99), max: samples.last().copied().unwrap_or(Duration::ZERO) }
    }
}

fn percentile(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (q.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}


pub async fn run_closed(target: Arc<dyn Target>, workers: usize, duration: Duration) -> Report {
    let _ = (target, workers, duration, Instant::now());
    todo!()
}


#[cfg(test)]
#[allow(dead_code)]
mod fake {
    use super::Target;
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


#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn four_workers_for_a_second_against_a_twenty_ms_service() {
        let fake = Arc::new(Fake::constant(MS(20)));
        let report = run_closed(fake.clone(), 4, Duration::from_secs(1)).await;
        assert_eq!((report.ok, report.errors, report.connections), (200, 0, 4), "четыре воркера по 50 запросов (0, 20, ... 980 мс): {report:?}");
        assert_eq!((report.p50, report.p99, report.max), (MS(20), MS(20), MS(20)), "задержка каждого запроса равна времени сервиса");
        assert_eq!(report.elapsed, Duration::from_secs(1));
        assert_eq!(report.rps, 200.0, "200 запросов за секунду");
    }

    #[tokio::test(start_paused = true)]
    async fn every_worker_connects_once_and_keeps_the_connection() {
        let fake = Arc::new(Fake::constant(MS(20)));
        run_closed(fake.clone(), 4, Duration::from_secs(1)).await;
        assert_eq!(fake.connects(), 4, "соединение на каждый запрос это уже не keep-alive, а совсем другая нагрузка (200 соединений вместо 4)");
        assert_eq!(fake.requests(), 200);
    }

    #[tokio::test(start_paused = true)]
    async fn workers_really_run_in_parallel() {
        let fake = Arc::new(Fake::constant(MS(20)));
        run_closed(fake.clone(), 8, MS(500)).await;
        assert_eq!(fake.peak(), 8, "в один момент в работе должны быть все восемь запросов");
    }

    #[tokio::test(start_paused = true)]
    async fn failed_requests_are_counted_but_do_not_pollute_the_latencies() {
        let fake = Arc::new(Fake::new(Box::new(|n, _| if n % 5 == 4 { MS(1) } else { MS(20) })).with_failures(Box::new(|n| n % 5 == 4)));
        let report = run_closed(fake.clone(), 1, Duration::from_secs(1)).await;
        assert!(report.errors > 0 && report.ok > 0, "{report:?}");
        assert_eq!(report.ok + report.errors, fake.requests(), "каждый запрос учтён один раз: успех или ошибка");
        assert_eq!(report.p50, MS(20), "быстрые отказы (1 мс) не должны делать сервис «быстрее» в отчёте");
        assert_eq!(report.max, MS(20));
    }

    #[tokio::test(start_paused = true)]
    async fn a_worker_that_cannot_connect_does_not_stop_the_others() {
        let fake = Arc::new(Fake::constant(MS(20)).with_connect_failures(1));
        let report = run_closed(fake.clone(), 4, Duration::from_secs(1)).await;
        assert_eq!(report.connections, 3, "один воркер не подключился");
        assert_eq!(report.ok, 150, "остальные трое отработали полностью: {report:?}");
        assert!(report.errors >= 1, "неудачное подключение это ошибка в отчёте");
    }

    #[tokio::test(start_paused = true)]
    async fn zero_duration_sends_nothing_and_does_not_panic() {
        let fake = Arc::new(Fake::constant(MS(20)));
        let report = run_closed(fake.clone(), 3, Duration::ZERO).await;
        assert_eq!((report.ok, report.errors), (0, 0));
        assert_eq!((report.rps, report.p99), (0.0, Duration::ZERO));
    }
}
