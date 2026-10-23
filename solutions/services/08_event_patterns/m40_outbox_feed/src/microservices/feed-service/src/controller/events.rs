use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use contracts::{DomainEvent, Topics};
use nats_engine::js::replay;
use nats_engine::{Bus, BusError, Pattern};

use crate::common::NAME;
use crate::service::FeedService;

#[derive(Debug, Default)]
pub struct Progress {
    applied: AtomicU64,
    caught_up: AtomicBool,
}

impl Progress {
    pub fn applied(&self) -> u64 {
        self.applied.load(Ordering::SeqCst)
    }

    pub fn caught_up(&self) -> bool {
        self.caught_up.load(Ordering::SeqCst)
    }

    fn catch_up(&self) {
        self.caught_up.store(true, Ordering::SeqCst);
    }
}

pub async fn follow(
    bus: Bus,
    topics: Topics,
    service: Arc<FeedService>,
    progress: Arc<Progress>,
) -> Result<(), BusError> {
    let filter = Pattern::parse(&topics.all_events().join("."))
        .map_err(|error| BusError::Transport(error.to_string()))?;
    let mut events = replay::<DomainEvent>(&bus, &topics.stream(), &filter).await?;
    if events.pending_at_start() == 0 {
        progress.catch_up();
    }
    while let Some(next) = events.next().await {
        let replayed = next?;
        match replayed.item {
            Ok(envelope) => {
                if let Err(error) = service.consume(&envelope.body).await {
                    eprintln!(
                        "{NAME}: событие {} пропущено без эффекта: {error}",
                        envelope.body.event_id
                    );
                }
            }
            Err(error) => eprintln!("{NAME}: запись {} не разобрана: {error}", replayed.sequence),
        }
        progress.applied.store(replayed.sequence, Ordering::SeqCst);
        if replayed.pending == 0 {
            progress.catch_up();
        }
    }
    Err(BusError::Transport(String::from("поток stream закончился")))
}
