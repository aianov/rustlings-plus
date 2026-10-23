use std::sync::Arc;
use std::time::Duration;

use contracts::{UserId, Value};
use jobs::{
    Checkpoints, Failure, Handler, Job, Leader, Leases, Queue, QueueSpec, Worker, WorkerSpec,
    now_ms, run_checkpointed,
};
use nats_engine::{Bus, BusError, fresh_id};

use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::common::{Config, NAME};
use crate::service::NotifyService;

pub const DIGEST_KIND: &str = "digest";

const TICK: Duration = Duration::from_millis(500);

pub struct Chores {
    worker: Worker,
    stop: watch::Sender<bool>,
    leader: JoinHandle<()>,
}

fn digests(service: Arc<NotifyService>, checkpoints: Checkpoints) -> Handler {
    Arc::new(move |job: Job| {
        let (service, checkpoints) = (Arc::clone(&service), checkpoints.clone());
        Box::pin(async move {
            let targets = service.digest_targets().await;
            let users: Vec<u64> = targets.iter().map(|(user, _)| user.0).collect();
            let sent = run_checkpointed(&job.id, &users, &checkpoints, |key, user| {
                let service = Arc::clone(&service);
                let unread = targets
                    .iter()
                    .find(|(target, _)| target.0 == user)
                    .map_or(0, |(_, unread)| *unread);
                async move {
                    service.send_digest(UserId(user), unread, &key).await;
                    Ok::<(), Failure>(())
                }
            })
            .await?;
            eprintln!("{NAME}: сводка {} разослана {sent} пользователям", job.id);
            Ok(())
        })
    })
}

async fn lead(mut leader: Leader, queue: Queue, mut stop: watch::Receiver<bool>) {
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            _ = stop.changed() => return,
            _ = ticker.tick() => {}
        }
        let queue = queue.clone();
        let ran = leader
            .tick(now_ms(), |period| async move {
                let job = Job::new(&format!("digest.{period}"), DIGEST_KIND, Value::Null);
                if let Err(error) = queue.enqueue(&job).await {
                    eprintln!("{NAME}: сводка {} не поставлена: {error}", job.id);
                }
            })
            .await;
        if let Err(error) = ran {
            eprintln!("{NAME}: аренда сводки недоступна: {error}");
        }
    }
}

pub async fn start(
    bus: &Bus,
    config: &Config,
    service: Arc<NotifyService>,
) -> Result<Chores, BusError> {
    let topics = &config.topics;
    let spec = QueueSpec {
        stream: topics.jobs_stream(),
        prefix: topics.jobs(),
        dead_stream: topics.jobs_dead_stream(),
        dead_prefix: topics.jobs_dead(),
    };
    let queue = Queue::open(bus.clone(), spec).await?;
    let checkpoints = Checkpoints::open(bus, &topics.checkpoints()).await?;
    let every = config.digest_every;
    let leases = Leases::open(bus, &format!("{}_DIGEST", topics.leases()), every * 2).await?;
    let worker_spec = WorkerSpec {
        kind: Some(DIGEST_KIND.to_string()),
        ..WorkerSpec::new("notify-digest")
    };
    let worker = Worker::start(&queue, worker_spec, digests(service, checkpoints)).await?;
    let leader = Leader::new("digest", &fresh_id(NAME), every, leases, now_ms());
    let (stop, stopped) = watch::channel(false);
    let leader = tokio::spawn(lead(leader, queue, stopped));
    Ok(Chores {
        worker,
        stop,
        leader,
    })
}

impl Chores {
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.leader.await;
        self.worker.stop().await;
    }
}
