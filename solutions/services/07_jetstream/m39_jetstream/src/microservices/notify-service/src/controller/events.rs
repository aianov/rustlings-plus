use std::sync::Arc;

use contracts::{EventBody, Topics};
use nats_engine::js::{Feed, FeedSpec, Handler, Verdict, open_feed};
use nats_engine::{Bus, BusError, Envelope, Subject};

use super::pattern;
use crate::common::{Config, NAME, Stats};
use crate::dto::{Decoded, decode};
use crate::service::{AudienceError, NotifyService};

pub fn spec(config: &Config) -> Result<FeedSpec, BusError> {
    let topics = &config.topics;
    let dead = topics.dead(&config.group);
    let dead: Vec<&str> = dead.iter().map(String::as_str).collect();
    let dead = Subject::new(&dead).map_err(|error| BusError::Transport(error.to_string()))?;
    let filter = pattern(&topics.all_events())?;
    Ok(FeedSpec {
        ack_wait: config.ack_wait,
        batch: config.batch,
        parallel: config.workers,
        ..FeedSpec::new(&topics.stream(), &config.group, filter, dead)
    })
}

pub async fn start(
    bus: &Bus,
    config: &Config,
    service: Arc<NotifyService>,
    stats: Arc<Stats>,
) -> Result<Feed, BusError> {
    let topics = config.topics.clone();
    let handler: Handler<EventBody> = Arc::new(move |envelope| {
        let (topics, service, stats) = (topics.clone(), Arc::clone(&service), Arc::clone(&stats));
        Box::pin(async move { judge(&topics, &service, &stats, envelope).await })
    });
    open_feed(bus, spec(config)?, handler).await
}

async fn judge(
    topics: &Topics,
    service: &NotifyService,
    stats: &Stats,
    envelope: Envelope<EventBody>,
) -> Verdict {
    Stats::bump(&stats.received);
    let incoming = match decode(topics, &envelope) {
        Decoded::Ignore => {
            Stats::bump(&stats.ignored);
            return Verdict::Done;
        }
        Decoded::Reject(reason) => {
            Stats::bump(&stats.rejected);
            return Verdict::Poison(reason);
        }
        Decoded::Notify(incoming) => incoming,
    };
    let request_id = incoming
        .request_id
        .clone()
        .unwrap_or_else(|| "-".to_string());
    match service.handle(incoming).await {
        Ok(handled) => {
            Stats::bump(&stats.handled);
            Stats::add(&stats.stored, handled.stored.len());
            Stats::add(&stats.duplicates, handled.duplicates);
            Verdict::Done
        }
        Err(AudienceError::Unavailable) => {
            Stats::bump(&stats.retried);
            eprintln!("{NAME} [{request_id}]: читатели недоступны, событие вернётся позже");
            Verdict::Retry
        }
        Err(AudienceError::Failed(reason)) => {
            Stats::bump(&stats.rejected);
            Verdict::Poison(reason)
        }
    }
}
