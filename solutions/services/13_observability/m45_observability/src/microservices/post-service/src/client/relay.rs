use std::sync::Arc;
use std::time::Duration;

use contracts::{OutboxRow, Topics};
use nats_engine::js::{StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusError, Meta, Subject};
use tokio::time::{sleep, timeout};

use crate::common::NAME;
use crate::repository::OutboxRepository;

pub const RELAY_BATCH: usize = 64;

const IDLE: Duration = Duration::from_secs(1);

const PAUSE: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub struct Relay {
    bus: Bus,
    topics: Topics,
    outbox: Arc<dyn OutboxRepository>,
}

impl Relay {
    pub async fn open(
        bus: Bus,
        topics: Topics,
        outbox: Arc<dyn OutboxRepository>,
    ) -> Result<Relay, BusError> {
        let spec = StreamSpec::new(&topics.stream(), topics.stream_subjects());
        ensure_stream(&bus, &spec).await?;
        Ok(Relay {
            bus,
            topics,
            outbox,
        })
    }

    pub async fn pump(&self) -> Result<usize, BusError> {
        let rows = self.outbox.unpublished(RELAY_BATCH).await;
        let mut sent = 0;
        let mut through = None;
        let mut failure = None;
        for row in &rows {
            match self.publish(row).await {
                Ok(()) => {
                    through = Some(row.seq);
                    sent += 1;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if let Some(through) = through {
            let marked = self.outbox.mark_published(through).await;
            marked.map_err(|error| BusError::Transport(error.to_string()))?;
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(sent),
        }
    }

    async fn publish(&self, row: &OutboxRow) -> Result<(), BusError> {
        let words = self.topics.event(&row.event);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let subject = Subject::new(&words).map_err(|error| BusError::Encode(error.to_string()))?;
        let meta = Meta::follow(&row.request_id, row.trace_id.as_deref());
        publish_event(&self.bus, &subject, &row.event.event_id, &row.event, &meta)
            .await
            .map(|_| ())
    }

    pub async fn run(self) {
        let signal = self.outbox.signal();
        let mut failing = false;
        loop {
            match self.pump().await {
                Ok(0) => {
                    let _ = timeout(IDLE, signal.notified()).await;
                }
                Ok(_) => {}
                Err(error) => {
                    if !failing {
                        tracing::warn!(
                            "{NAME}: релей outbox встал: {error}; события ждут в outbox"
                        );
                    }
                    failing = true;
                    sleep(PAUSE).await;
                    continue;
                }
            }
            if failing {
                tracing::warn!("{NAME}: релей outbox снова публикует");
                failing = false;
            }
        }
    }
}
