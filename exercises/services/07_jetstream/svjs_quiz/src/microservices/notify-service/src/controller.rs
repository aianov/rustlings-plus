use crate::common::{Config, Stats};
use crate::dto::{Decoded, EventBody, decode};
use crate::service::{NotifyService, Recorded};
use nats_engine::js::{Feed, FeedSpec, Handler, Verdict, open_feed};
use nats_engine::{Bus, BusError, Pattern, Seg, Server, Subject};
use std::sync::Arc;

pub struct Controller {
    feed: Feed,
    servers: Vec<Server>,
}

pub async fn start(bus: Arc<Bus>, config: &Config, service: Arc<NotifyService>, stats: Arc<Stats>) -> Result<Controller, BusError> {
    let root = config.root.as_str();
    let plain = |error: nats_engine::SubjectError| BusError::Transport(error.to_string());
    let filter = Pattern::build(&[Seg::Word(root), Seg::Any, Seg::Any, Seg::Any]).map_err(plain)?;
    let spec = FeedSpec {
        stream: config.stream.clone(),
        durable: config.durable.clone(),
        filter,
        ack_wait: config.ack_wait,
        batch: config.batch,
        dead_subject: Subject::parse(&config.dead_subject()).map_err(plain)?,
    };
    let handler: Handler<EventBody> = {
        let (root, service, stats, deliver) = (config.root.clone(), service.clone(), stats.clone(), config.deliver.clone());
        Arc::new(move |envelope| {
            let (root, service, stats, deliver) = (root.clone(), service.clone(), stats.clone(), deliver.clone());
            Box::pin(async move {
                Stats::bump(&stats.received);
                match decode(&root, &envelope) {
                    Decoded::Ignore => {
                        Stats::bump(&stats.ignored);
                        Verdict::Done
                    }
                    Decoded::Reject(reason) => {
                        Stats::bump(&stats.rejected);
                        Verdict::Poison(reason)
                    }
                    Decoded::Notify(incoming) => match service.record(incoming) {
                        Recorded::Stored(notification) => {
                            Stats::bump(&stats.stored);
                            deliver(notification).await;
                            Verdict::Done
                        }
                        Recorded::Duplicate => {
                            Stats::bump(&stats.duplicates);
                            Verdict::Done
                        }
                    },
                }
            })
        })
    };
    let feed = open_feed(&bus, spec, handler).await?;

    let mut servers = Vec::new();
    let unread = Pattern::build(&[Seg::Word(root), Seg::Word("unread"), Seg::Any]).map_err(plain)?;
    let reader = service.clone();
    servers.push(bus.serve::<serde_json::Value, u32, _>(&unread, Some(&config.durable), move |request| Ok(reader.unread(request.subject.last_token()))).await?);
    let read = Pattern::build(&[Seg::Word(root), Seg::Word("read"), Seg::Any]).map_err(plain)?;
    let marker = service.clone();
    servers.push(bus.serve::<serde_json::Value, u32, _>(&read, Some(&config.durable), move |request| Ok(marker.mark_read(request.subject.last_token()))).await?);
    Ok(Controller { feed, servers })
}

impl Controller {
    pub async fn stop(self) {
        self.feed.stop().await;
        for server in self.servers {
            server.stop().await;
        }
    }
}
