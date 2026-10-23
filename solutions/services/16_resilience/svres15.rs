
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, mpsc, oneshot};

fn main() {}

/// Отказ из-за перегрузки.
#[derive(Debug, PartialEq, Eq)]
pub struct Overloaded;

struct Job {
    reply: oneshot::Sender<()>,
    _place: OwnedSemaphorePermit,
}

pub struct Service {
    tx: mpsc::UnboundedSender<Job>,
    places: Arc<Semaphore>,
}

impl Service {
    /// `workers` воркеров, каждый обрабатывает один запрос за `work`.
    pub fn start(workers: usize, work: Duration) -> Service {
        let (tx, rx) = mpsc::unbounded_channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..workers {
            let rx = rx.clone();
            tokio::spawn(async move {
                loop {
                    let job = rx.lock().await.recv().await;
                    let Some(job) = job else { break };
                    tokio::time::sleep(work).await;
                    let _ = job.reply.send(());
                }
            });
        }
        // Принятый запрос должен успеть до того, как клиент потеряет терпение (300 мс): при 4 воркерах по 20 мс
        // это не больше 60 запросов в системе, берём с запасом.
        Service { tx, places: Arc::new(Semaphore::new(workers * 10)) }
    }

    pub async fn handle(&self) -> Result<(), Overloaded> {
        let (reply, answer) = oneshot::channel();
        let place = self.places.clone().try_acquire_owned().map_err(|_| Overloaded)?;
        self.tx.send(Job { reply, _place: place }).map_err(|_| Overloaded)?;
        answer.await.map_err(|_| Overloaded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::task::JoinSet;
    use tokio::time::{Instant, sleep, timeout};

    const CLIENT_PATIENCE: Duration = Duration::from_millis(300);

    #[derive(Debug, Default)]
    struct Outcome {
        in_time: u32,
        rejected: u32,
        timed_out: u32,
        slow_rejections: u32,
    }

    async fn drive(svc: Arc<Service>, rps: u64, seconds: u64) -> Outcome {
        let gap = Duration::from_micros(1_000_000 / rps);
        let mut set = JoinSet::new();
        for _ in 0..rps * seconds {
            let svc = svc.clone();
            set.spawn(async move {
                let started = Instant::now();
                let r = timeout(CLIENT_PATIENCE, svc.handle()).await;
                (r, started.elapsed())
            });
            sleep(gap).await;
        }
        let mut out = Outcome::default();
        while let Some(done) = set.join_next().await {
            match done.unwrap() {
                (Ok(Ok(())), _) => out.in_time += 1,
                (Ok(Err(Overloaded)), took) => {
                    out.rejected += 1;
                    if took > Duration::from_millis(5) {
                        out.slow_rejections += 1;
                    }
                }
                (Err(_), _) => out.timed_out += 1,
            }
        }
        out
    }

    #[tokio::test(start_paused = true)]
    async fn normal_load_is_served_in_full() {
        let svc = Arc::new(Service::start(4, Duration::from_millis(20)));
        let out = drive(svc, 100, 3).await;
        assert_eq!(out.rejected, 0, "нагрузка вдвое ниже возможностей, а отказов {}: предел поставлен слишком низко", out.rejected);
        assert_eq!(out.in_time, 300);
    }

    #[tokio::test(start_paused = true)]
    async fn double_load_still_yields_useful_work() {
        let svc = Arc::new(Service::start(4, Duration::from_millis(20)));
        let out = drive(svc, 400, 3).await;
        assert!(out.in_time >= 480, "вовремя обслужено только {} из 1200 (возможности около 600): очередь забита мёртвыми запросами или лимит слишком строгий", out.in_time);
        assert!(out.timed_out <= 24, "по таймауту клиентов ушло {}: принятый запрос должен успевать, а то, что не успеет, надо отвергать на входе", out.timed_out);
        assert!(out.rejected >= 300, "отказано всего {}: лишнюю нагрузку должны были отсечь", out.rejected);
        assert_eq!(out.slow_rejections, 0, "отказ должен быть мгновенным, а не после ожидания в очереди");
        assert_eq!(out.in_time + out.rejected + out.timed_out, 1200);
    }

    #[tokio::test(start_paused = true)]
    async fn the_service_recovers_after_the_spike() {
        let svc = Arc::new(Service::start(4, Duration::from_millis(20)));
        let spike = drive(svc.clone(), 600, 2).await;
        assert!(spike.in_time > 0);
        sleep(Duration::from_secs(1)).await;
        let calm = drive(svc, 100, 2).await;
        assert_eq!(calm.in_time, 200, "после всплеска обычные запросы должны проходить без отказов, а прошло {} из 200", calm.in_time);
    }
}
