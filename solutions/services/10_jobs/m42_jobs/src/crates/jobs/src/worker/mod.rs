use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use nats_engine::BusError;
use nats_engine::js::{PullSpec, Taken, puller};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep};

use crate::job::{Job, Outcome, now_ms};
use crate::policy::{Decision, Policy};
use crate::queue::Queue;

const FETCH_WAIT: Duration = Duration::from_millis(300);

const IDLE: Duration = Duration::from_millis(100);

pub type Handler = Arc<dyn Fn(Job) -> Pin<Box<dyn Future<Output = Outcome> + Send>> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSpec {
    pub durable: String,
    pub kind: Option<String>,
    pub batch: usize,
    pub ack_wait: Duration,
    pub policy: Policy,
}

impl WorkerSpec {
    pub fn new(durable: &str) -> WorkerSpec {
        WorkerSpec {
            durable: durable.to_string(),
            kind: None,
            batch: 8,
            ack_wait: Duration::from_secs(30),
            policy: Policy::default(),
        }
    }
}

pub struct Worker {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

enum Settled {
    Finished,
    Interrupted,
}

async fn settle(
    queue: &Queue,
    spec: &WorkerSpec,
    handler: &Handler,
    taken: &Taken<Job>,
    stop: &mut watch::Receiver<bool>,
) -> Settled {
    let job = match &taken.item {
        Ok(envelope) => envelope.body.clone(),
        Err(error) => {
            if let Ok(dead) = queue.dead("unreadable") {
                let _ = taken
                    .bury(&dead, &format!("конверт задачи не читается: {error}"))
                    .await;
            }
            return Settled::Finished;
        }
    };
    let now = now_ms();
    if job.run_at > now {
        let _ = taken.nak(Duration::from_millis(job.run_at - now)).await;
        return Settled::Finished;
    }
    let early = u64::from(job.run_at > taken.published_ms);
    let attempt = taken.delivered.saturating_sub(early).max(1);
    let kind = job.kind.clone();
    let mut work = handler(job);
    let beat = (spec.ack_wait / 3).max(Duration::from_millis(10));
    let mut progress = interval_at(Instant::now() + beat, beat);
    progress.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let outcome = loop {
        tokio::select! {
            outcome = &mut work => break outcome,
            _ = progress.tick() => {
                let _ = taken.progress().await;
            }
            _ = stop.changed() => {
                let _ = taken.nak(Duration::ZERO).await;
                return Settled::Interrupted;
            }
        }
    };
    match spec.policy.decide(&outcome, attempt) {
        Decision::Done => {
            let _ = taken.ack().await;
        }
        Decision::Retry(pause) => {
            let _ = taken.nak(pause).await;
        }
        Decision::Dead(reason) => match queue.dead(&kind) {
            Ok(dead) => {
                if taken.bury(&dead, &reason).await.is_err() {
                    let _ = taken.nak(spec.policy.first).await;
                }
            }
            Err(..) => {
                let _ = taken.nak(spec.policy.first).await;
            }
        },
    }
    Settled::Finished
}

async fn hand_back(waiting: &mut VecDeque<Taken<Job>>) {
    for taken in waiting.drain(..) {
        let _ = taken.nak(Duration::ZERO).await;
    }
}

impl Worker {
    pub async fn start(
        queue: &Queue,
        spec: WorkerSpec,
        handler: Handler,
    ) -> Result<Worker, BusError> {
        let pull = PullSpec {
            stream: queue.spec().stream.clone(),
            durable: spec.durable.clone(),
            filter: queue.filter(spec.kind.as_deref())?,
            ack_wait: spec.ack_wait,
        };
        let jobs = puller::<Job>(queue.bus(), &pull).await?;
        let queue = queue.clone();
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            loop {
                if *stopped.borrow() {
                    return;
                }
                let mut waiting: VecDeque<Taken<Job>> =
                    match jobs.take(spec.batch, FETCH_WAIT).await {
                        Ok(taken) => taken.into(),
                        Err(..) => {
                            sleep(IDLE).await;
                            continue;
                        }
                    };
                while let Some(taken) = waiting.pop_front() {
                    if *stopped.borrow() {
                        let _ = taken.nak(Duration::ZERO).await;
                        hand_back(&mut waiting).await;
                        return;
                    }
                    if let Settled::Interrupted =
                        settle(&queue, &spec, &handler, &taken, &mut stopped).await
                    {
                        hand_back(&mut waiting).await;
                        return;
                    }
                }
            }
        });
        Ok(Worker { stop, task })
    }

    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.task.await;
    }
}
