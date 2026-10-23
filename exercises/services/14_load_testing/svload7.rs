// Закрытый цикл шлёт следующий запрос, только когда получил ответ на прошлый, и когда сервис замедляется, генератор тоже
// замедляется и шлёт меньше. Живые пользователи так не делают: они приходят в своё время, не считаясь с тем, что у сервиса
// затор. Цель: открытая модель. `run_open` отправляет ровно `rate` запросов в секунду по расписанию (запрос номер `i` назначен
// на момент `i/rate` от старта), не дожидаясь ответов. Для запроса берётся свободное соединение из набора, а если свободных
// нет, открывается новое. Задержка считается от назначенного момента до ответа, подключение входит в неё.


use futures::future::BoxFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::{Instant, sleep_until};

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


pub async fn run_open(target: Arc<dyn Target>, rate: u32, duration: Duration) -> Report {
    let _ = (target, rate, duration, Instant::now(), sleep_until(Instant::now()), Mutex::new(0), JoinSet::<()>::new());
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
    async fn a_fast_service_gets_exactly_the_planned_requests_over_one_connection() {
        let fake = Arc::new(Fake::constant(MS(5)));
        let report = run_open(fake.clone(), 100, Duration::from_secs(1)).await;
        assert_eq!((report.ok, report.errors), (100, 0), "сто запросов в секунду за секунду: {report:?}");
        assert_eq!(report.connections, 1, "ответ приходит раньше следующего запроса, одного соединения хватает");
        assert_eq!((report.p50, report.max), (MS(5), MS(5)));
        assert!(report.elapsed >= Duration::from_secs(1) - MS(10), "прогон идёт по расписанию, а не заканчивается раньше: {:?}", report.elapsed);
    }

    #[tokio::test(start_paused = true)]
    async fn a_slower_service_needs_more_connections_not_fewer_requests() {
        let fake = Arc::new(Fake::constant(MS(25)));
        let report = run_open(fake.clone(), 100, Duration::from_secs(1)).await;
        assert_eq!(report.ok, 100, "число запросов не зависит от скорости сервиса");
        assert_eq!(report.connections, 3, "запросы идут каждые 10 мс, а длятся по 25: одновременно живут три");
        assert_eq!(fake.peak(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn when_the_server_freezes_the_requests_keep_coming_and_their_latency_shows_it() {
        let fake = Arc::new(Fake::new(Box::new(|_, t| if t >= MS(200) && t < MS(1200) { MS(1200) - t } else { MS(5) })));
        let report = run_open(fake.clone(), 100, Duration::from_secs(2)).await;
        assert_eq!(report.ok, 200, "расписание не подстраивается под сервис: за две секунды ровно 200 запросов");
        assert_eq!(report.max, MS(1000), "самый ранний из замёрзших ждал секунду: он назначен на 200 мс, а ответ в 1200");
        assert!(report.p99 >= MS(950), "p99 {:?}: сотня запросов провела в очереди от 10 мс до секунды, это одна десятая прогона", report.p99);
        assert!(fake.peak() >= 90, "во время простоя запросы копятся одновременно: {}", fake.peak());
    }

    #[tokio::test(start_paused = true)]
    async fn the_time_to_connect_is_part_of_the_latency() {
        let fake = Arc::new(Fake::constant(MS(5)).with_connect_delay(MS(50)));
        let report = run_open(fake.clone(), 10, Duration::from_secs(1)).await;
        assert_eq!(report.max, MS(55), "первый запрос открывал соединение: 50 мс подключения и 5 мс ответа");
        assert_eq!(report.p50, MS(5), "остальные пользуются готовым соединением");
        assert_eq!(report.connections, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failures_are_counted_and_latencies_are_of_successes_only() {
        let fake = Arc::new(Fake::new(Box::new(|n, _| if n % 4 == 3 { MS(1) } else { MS(8) })).with_failures(Box::new(|n| n % 4 == 3)));
        let report = run_open(fake, 100, Duration::from_secs(1)).await;
        assert_eq!(report.ok + report.errors, 100, "каждому запросу по расписанию соответствует исход");
        assert_eq!(report.errors, 25);
        assert_eq!((report.p50, report.max), (MS(8), MS(8)), "быстрые отказы не попадают в задержки");
    }

    #[tokio::test(start_paused = true)]
    async fn nothing_is_planned_for_zero_time() {
        let report = run_open(Arc::new(Fake::constant(MS(5))), 100, Duration::ZERO).await;
        assert_eq!((report.ok, report.errors, report.connections), (0, 0, 0));
    }
}
