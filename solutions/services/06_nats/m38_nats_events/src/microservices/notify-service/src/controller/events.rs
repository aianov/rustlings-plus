use std::sync::Arc;

use contracts::{EventBody, Topics};
use nats_engine::{Bus, BusError, Subscription};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use super::pattern;
use crate::common::{Config, Incoming, NAME, Stats};
use crate::dto::{Decoded, decode};
use crate::service::NotifyService;

type Waiting = Arc<Mutex<mpsc::Receiver<Incoming>>>;

pub struct Intake {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
    workers: Workers,
}

pub struct Workers(Vec<JoinHandle<()>>);

pub async fn start(
    bus: &Bus,
    config: &Config,
    service: Arc<NotifyService>,
    stats: Arc<Stats>,
) -> Result<Intake, BusError> {
    let events = pattern(&config.topics.all_events())?;
    let events = bus
        .subscribe_group::<EventBody>(&events, &config.group)
        .await?;
    let (queue, waiting) = mpsc::channel(config.queue);
    let waiting: Waiting = Arc::new(Mutex::new(waiting));
    let workers = (0..config.workers)
        .map(|_| {
            let (waiting, service, stats) = (
                Arc::clone(&waiting),
                Arc::clone(&service),
                Arc::clone(&stats),
            );
            tokio::spawn(work(waiting, service, stats))
        })
        .collect();
    let (stop, stopped) = oneshot::channel();
    let topics = config.topics.clone();
    let task = tokio::spawn(receive(events, queue, topics, stats, stopped));
    Ok(Intake {
        stop,
        task,
        workers: Workers(workers),
    })
}

async fn receive(
    mut events: Subscription<EventBody>,
    queue: mpsc::Sender<Incoming>,
    topics: Topics,
    stats: Arc<Stats>,
    mut stopped: oneshot::Receiver<()>,
) {
    let mut closing = false;
    loop {
        let item = if closing {
            events.next().await
        } else {
            tokio::select! {
                item = events.next() => item,
                _ = &mut stopped => {
                    events.stop().await;
                    closing = true;
                    continue;
                }
            }
        };
        let Some(item) = item else { break };
        Stats::bump(&stats.received);
        let envelope = match item {
            Ok(envelope) => envelope,
            Err(error) => {
                Stats::bump(&stats.rejected);
                eprintln!("{NAME}: событие отвергнуто: {error}");
                continue;
            }
        };
        match decode(&topics, &envelope) {
            Decoded::Ignore => Stats::bump(&stats.ignored),
            Decoded::Reject(reason) => {
                Stats::bump(&stats.rejected);
                eprintln!("{NAME}: событие {} отвергнуто: {reason}", envelope.subject);
            }
            Decoded::Notify(incoming) => match queue.try_send(incoming) {
                Ok(()) => Stats::bump(&stats.queued),
                Err(..) => Stats::bump(&stats.dropped),
            },
        }
    }
}

async fn work(waiting: Waiting, service: Arc<NotifyService>, stats: Arc<Stats>) {
    loop {
        let next = waiting.lock().await.recv().await;
        let Some(incoming) = next else { return };
        let request_id = incoming
            .request_id
            .clone()
            .unwrap_or_else(|| "-".to_string());
        match service.handle(incoming).await {
            Ok(handled) => {
                Stats::add(&stats.stored, handled.stored.len());
                Stats::add(&stats.duplicates, handled.duplicates);
            }
            Err(error) => {
                Stats::bump(&stats.failed);
                eprintln!("{NAME} [{request_id}]: событие не обработано: {error}");
            }
        }
    }
}

impl Intake {
    pub async fn close(self) -> Workers {
        let _ = self.stop.send(());
        let _ = self.task.await;
        self.workers
    }
}

impl Workers {
    pub async fn finish(self) {
        for worker in self.0 {
            let _ = worker.await;
        }
    }
}
