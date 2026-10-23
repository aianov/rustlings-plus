// Генератор нагрузки работает, цифры идут, но сервис под ним почему-то «упирается» на жалких 8000 запросов в секунду, а процессор
// сервиса скучает. Профиль показывает, что упёрся сам генератор: каждый воркер на каждый запрос берёт общий замок статистики и
// печатает строку в общий вывод, и в итоге все воркеры стоят в очереди друг за другом. Измеряющий прибор не должен быть самым
// узким местом измеряемой системы. Цель: результаты те же (все замеры на месте), но общее состояние (замок статистики и вывод)
// используется за весь прогон считанные разы, а не по разу на запрос: общих касаний не больше, чем воркеров, и не больше одной
// строки итога.


use futures::future::BoxFuture;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::Instant;

fn main() {}


pub trait Target: Send + Sync + 'static {
    fn connect(&self) -> BoxFuture<'_, Result<u64, String>>;
    fn request(&self, conn: u64) -> BoxFuture<'_, Result<(), String>>;
}


#[derive(Default)]
pub struct Sink {
    samples: Mutex<Vec<Duration>>,
    lines: Mutex<Vec<String>>,
    locks: AtomicUsize,
    printed: AtomicUsize,
}

impl Sink {
    pub fn push(&self, sample: Duration) {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.samples.lock().expect("замеры не отравлены").push(sample);
    }

    pub fn merge(&self, mut local: Vec<Duration>) {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.samples.lock().expect("замеры не отравлены").append(&mut local);
    }

    pub fn say(&self, line: &str) {
        self.printed.fetch_add(1, Ordering::SeqCst);
        self.lines.lock().expect("вывод не отравлен").push(line.to_string());
    }

    pub fn locks(&self) -> usize {
        self.locks.load(Ordering::SeqCst)
    }

    pub fn printed(&self) -> usize {
        self.printed.load(Ordering::SeqCst)
    }

    pub fn samples(&self) -> Vec<Duration> {
        self.samples.lock().expect("замеры не отравлены").clone()
    }
}

pub async fn run_closed(target: Arc<dyn Target>, workers: usize, duration: Duration, sink: Arc<Sink>) -> u64 {
    let deadline = Instant::now() + duration;
    let mut set = JoinSet::new();
    for worker in 0..workers {
        let (target, sink) = (target.clone(), sink.clone());
        set.spawn(async move {
            let Ok(conn) = target.connect().await else { return 0u64 };
            let mut ok = 0u64;
            let mut i = 0u64;
            while Instant::now() < deadline {
                let sent = Instant::now();
                let answer = target.request(conn).await;
                sink.say(&format!("воркер {worker}, запрос {i}: {}", if answer.is_ok() { "ok" } else { "ошибка" }));
                if answer.is_ok() {
                    sink.push(sent.elapsed());
                    ok += 1;
                }
                i += 1;
            }
            ok
        });
    }
    let mut ok = 0;
    while let Some(done) = set.join_next().await {
        ok += done.unwrap_or(0);
    }
    ok
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
    async fn all_samples_are_still_collected() {
        let sink = Arc::new(Sink::default());
        let ok = run_closed(Arc::new(Fake::constant(MS(20))), 4, Duration::from_secs(1), sink.clone()).await;
        assert_eq!(ok, 200);
        let samples = sink.samples();
        assert_eq!(samples.len(), 200, "результаты не должны потеряться при оптимизации");
        assert!(samples.iter().all(|s| *s == MS(20)));
    }

    #[tokio::test(start_paused = true)]
    async fn the_shared_lock_is_taken_a_handful_of_times_not_per_request() {
        let sink = Arc::new(Sink::default());
        let ok = run_closed(Arc::new(Fake::constant(MS(1))), 8, Duration::from_secs(1), sink.clone()).await;
        assert_eq!(ok, 8000);
        assert!(sink.locks() <= 8, "общий замок статистики взят {} раз на 8000 запросов: воркеры должны копить замеры у себя и отдать их один раз", sink.locks());
        assert_eq!(sink.samples().len(), 8000);
    }

    #[tokio::test(start_paused = true)]
    async fn the_output_is_a_summary_not_a_line_per_request() {
        let sink = Arc::new(Sink::default());
        run_closed(Arc::new(Fake::constant(MS(1))), 8, Duration::from_secs(1), sink.clone()).await;
        assert_eq!(sink.printed(), 1, "напечатано {} строк на 8000 запросов: вывод в общий поток это тоже общий замок; нужна одна итоговая строка", sink.printed());
    }

    #[tokio::test(start_paused = true)]
    async fn failures_do_not_get_into_the_samples() {
        let sink = Arc::new(Sink::default());
        let fake = Fake::constant(MS(10)).with_failures(Box::new(|n| n % 4 == 0));
        let ok = run_closed(Arc::new(fake), 2, MS(400), sink.clone()).await;
        assert_eq!(sink.samples().len() as u64, ok, "в замерах только успешные запросы");
        assert!(ok > 0 && ok < 80, "успешных меньше всех запросов: {ok}");
    }
}
