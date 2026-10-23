// Сервис замер на секунду посреди прогона (пауза сборщика мусора, блокировка в базе), а отчёт генератора такой: «p99 = 5 мс,
// всё прекрасно». Обман в самом генераторе. Он шлёт запрос, ждёт ответа и только потом спит до следующего. Пока сервис стоял,
// генератор тоже стоял в ожидании одного запроса и не отправил те сто, что должен был: замер у него один, а не сто.
// Это называется координированным упущением (coordinated omission). Цель: `run_paced` держит расписание «`rate` запросов
// в секунду на одном соединении» независимо от скорости сервиса, а задержка запроса считается от момента, на который он назначен
// по расписанию, а не от момента, когда генератор до него наконец дошёл.


use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;

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


pub async fn run_paced(target: Arc<dyn Target>, rate: u32, duration: Duration) -> Report {
    let started = Instant::now();
    let Ok(conn) = target.connect().await else {
        return Report::build(Vec::new(), 1, 0, started.elapsed());
    };
    let mut samples = Vec::new();
    let mut errors = 0;
    let interval = Duration::from_nanos(1_000_000_000 / u64::from(rate.max(1)));
    while started.elapsed() < duration {
        let sent = Instant::now();
        match target.request(conn).await {
            Ok(()) => samples.push(sent.elapsed()),
            Err(_) => errors += 1,
        }
        sleep(interval).await;
    }
    Report::build(samples, errors, 1, started.elapsed())
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
    async fn a_healthy_service_gets_exactly_the_scheduled_requests() {
        let fake = Arc::new(Fake::constant(MS(5)));
        let report = run_paced(fake.clone(), 100, Duration::from_secs(1)).await;
        assert_eq!(report.ok, 100, "сто запросов в секунду за секунду, а не меньше из-за пауз между запросами: {report:?}");
        assert_eq!(report.connections, 1);
        assert_eq!((report.p50, report.max), (MS(5), MS(5)), "никаких задержек, кроме времени сервиса");
    }

    #[tokio::test(start_paused = true)]
    async fn a_freeze_shows_up_in_the_tail() {
        let fake = Arc::new(Fake::new(Box::new(|_, t| if t >= Duration::from_secs(2) && t < Duration::from_secs(3) { Duration::from_secs(3) - t } else { MS(5) })));
        let report = run_paced(fake.clone(), 100, Duration::from_secs(5)).await;
        assert_eq!(report.ok, 500, "расписание не сдвигается из-за простоя сервиса: все 500 запросов отправлены");
        assert_eq!(report.max, MS(1000), "запрос, назначенный на 2.00, дождался ответа в 3.00");
        assert!(report.p99 >= MS(950), "p99 {:?}: около двухсот запросов отстали из-за простоя, и их задержку считали от назначенного времени", report.p99);
        assert!(report.p50 <= MS(10), "большинство запросов до и после простоя быстрые: p50 {:?}", report.p50);
    }

    #[tokio::test(start_paused = true)]
    async fn a_service_slower_than_the_schedule_makes_latency_grow() {
        let fake = Arc::new(Fake::constant(MS(20)));
        let report = run_paced(fake, 100, Duration::from_secs(1)).await;
        assert_eq!(report.ok, 100);
        assert_eq!(report.max, MS(1010), "сервис отвечает по 20 мс при расписании по 10: очередь растёт, последний (назначен на 990) ответил в 2000");
        assert!(report.p50 > MS(400), "медиана тоже выросла: {:?}", report.p50);
    }

    #[tokio::test(start_paused = true)]
    async fn failures_are_errors_and_stay_out_of_the_latencies() {
        let fake = Arc::new(Fake::constant(MS(5)).with_failures(Box::new(|n| n % 10 == 0)));
        let report = run_paced(fake, 100, Duration::from_secs(1)).await;
        assert_eq!((report.ok, report.errors), (90, 10));
    }
}
