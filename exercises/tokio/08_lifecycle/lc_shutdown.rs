// Сервис целиком. Тикер раз в 10 мс кладёт задания в очередь, два воркера разбирают очередь (один
// раз воркер падает с паникой на «ядовитом» задании), результаты собирает актор. Токен отмены
// и надзиратель, написанные тобой ранее, лежат ниже вместе с готовыми телами задач.
// TODO: Собери из них `Service`: `start` запускает всё (воркеры под надзирателем, каждая задача
// получает свою копию `probe`), `shutdown(deadline)` останавливает в правильном порядке: тикер,
// затем воркеры (не дольше срока, потом принудительно), и последним актор. Ничего не должно остаться
// живым, а в `Report` попасть число обработанных заданий, перезапусков и остановленных силой воркеров.
use futures::future::select_all;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, sleep, timeout_at};

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

#[allow(dead_code)]
impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Finished { restarts: u32 },
    GaveUp { attempts: u32 },
    Cancelled,
}

async fn supervise<F, Fut>(make: F, token: Token, pause: Duration, max_restarts: u32) -> Outcome
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut restarts = 0;
    loop {
        let mut child = tokio::spawn(make());
        tokio::select! {
            result = &mut child => {
                if result.is_ok() {
                    return Outcome::Finished { restarts };
                }
                if restarts == max_restarts {
                    return Outcome::GaveUp { attempts: restarts + 1 };
                }
            }
            _ = token.cancelled() => {
                child.abort();
                let _ = child.await;
                return Outcome::Cancelled;
            }
        }
        tokio::select! {
            _ = sleep(pause) => {}
            _ = token.cancelled() => return Outcome::Cancelled,
        }
        restarts += 1;
    }
}

#[derive(Clone)]
struct Config {
    stuck_job: Option<u32>,
    emitted: Arc<AtomicU32>,
    finished: Arc<AtomicU32>,
    poisoned: Arc<AtomicBool>,
    probe: Arc<()>,
}

#[derive(Clone)]
struct Results {
    tx: mpsc::Sender<u32>,
}

impl Results {
    async fn add(&self, job: u32) {
        let _ = self.tx.send(job).await;
    }
}

#[derive(Debug, PartialEq)]
struct Stats {
    count: u32,
    sum: u32,
}

#[derive(Debug)]
struct Report {
    processed: u32,
    restarts: u32,
    aborted: usize,
}

async fn results_actor(mut rx: mpsc::Receiver<u32>, _probe: Arc<()>) -> Stats {
    let mut stats = Stats { count: 0, sum: 0 };
    while let Some(job) = rx.recv().await {
        stats.count += 1;
        stats.sum += job;
    }
    stats
}

async fn ticker(token: Token, queue: mpsc::Sender<u32>, emitted: Arc<AtomicU32>, _probe: Arc<()>) {
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            _ = tick.tick() => {
                let job = emitted.fetch_add(1, Ordering::SeqCst);
                if queue.send(job).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn worker(queue: Arc<tokio::sync::Mutex<mpsc::Receiver<u32>>>, results: Results, config: Config) {
    loop {
        let next = queue.lock().await.recv().await;
        let Some(job) = next else { break };
        if job == 3 && !config.poisoned.swap(true, Ordering::SeqCst) {
            panic!("ядовитое задание {job}");
        }
        if Some(job) == config.stuck_job {
            std::future::pending::<()>().await;
        }
        sleep(Duration::from_millis(5)).await;
        results.add(job).await;
        config.finished.fetch_add(1, Ordering::SeqCst);
    }
}

struct Service;

impl Service {
    fn start(config: Config) -> Service {
        todo!()
    }

    async fn shutdown(self, deadline: Duration) -> Report {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let config = Config {
        stuck_job: None,
        emitted: Arc::default(),
        finished: Arc::default(),
        poisoned: Arc::default(),
        probe: Arc::new(()),
    };
    let service = Service::start(config);
    sleep(Duration::from_millis(100)).await;
    println!("{:?}", service.shutdown(Duration::from_secs(1)).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(stuck_job: Option<u32>) -> (Config, Arc<()>) {
        let probe = Arc::new(());
        let config = Config {
            stuck_job,
            emitted: Arc::default(),
            finished: Arc::default(),
            poisoned: Arc::default(),
            probe: probe.clone(),
        };
        (config, probe)
    }

    #[tokio::test(start_paused = true)]
    async fn clean_shutdown_loses_nothing_and_leaves_nothing_alive() {
        let (config, probe) = config(None);
        let (emitted, finished) = (config.emitted.clone(), config.finished.clone());
        let service = Service::start(config);
        sleep(Duration::from_millis(200)).await;
        let begin = Instant::now();
        let report = service.shutdown(Duration::from_secs(1)).await;
        let emitted = emitted.load(Ordering::SeqCst);
        assert!(emitted >= 19, "за 200 мс тикер должен был выдать около двадцати заданий, выдал {emitted}");
        assert_eq!(report.processed, emitted - 1, "обработаны все задания, кроме ядовитого: {emitted} выдано, {} обработано", report.processed);
        assert_eq!(report.processed, finished.load(Ordering::SeqCst), "результаты воркеров и актора расходятся: актор остановили раньше, чем дошли результаты");
        assert_eq!(report.restarts, 1, "один воркер упал на ядовитом задании и был перезапущен");
        assert_eq!(report.aborted, 0, "никого не пришлось останавливать силой");
        assert!(begin.elapsed() < Duration::from_millis(100), "мирная остановка заняла {:?}", begin.elapsed());
        assert_eq!(Arc::strong_count(&probe), 1, "после остановки остались живые задачи, которые держат ресурсы");
    }

    #[tokio::test(start_paused = true)]
    async fn a_stuck_worker_is_stopped_by_force_at_the_deadline() {
        let (config, probe) = config(Some(5));
        let emitted = config.emitted.clone();
        let service = Service::start(config);
        sleep(Duration::from_millis(200)).await;
        let begin = Instant::now();
        let report = service.shutdown(Duration::from_millis(300)).await;
        assert_eq!(begin.elapsed(), Duration::from_millis(300), "остановка ждёт ровно до срока и ни миллисекундой больше");
        assert_eq!(report.aborted, 1, "один воркер завис на задании 5 и был остановлен силой");
        let emitted = emitted.load(Ordering::SeqCst);
        assert_eq!(report.processed, emitted - 2, "потеряны ядовитое и зависшее задания, остальное обработал второй воркер");
        assert_eq!(Arc::strong_count(&probe), 1, "после принудительной остановки не должно остаться живых задач (воркер под надзирателем тоже)");
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_right_after_start_is_fine() {
        let (config, probe) = config(None);
        let service = Service::start(config);
        let report = service.shutdown(Duration::from_secs(1)).await;
        assert_eq!(report.aborted, 0, "ничего не зависло");
        assert_eq!(Arc::strong_count(&probe), 1, "после мгновенной остановки живых задач нет");
    }
}
