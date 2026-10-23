use contracts::{Event, Origin, Topics};
use nats_engine::{Bus, BusError, Meta, Subject};

use crate::common::NAME;
use crate::repository::BoxFuture;
use crate::service::Events;

pub struct BusEvents {
    bus: Bus,
    topics: Topics,
}

impl BusEvents {
    pub fn new(bus: Bus, topics: Topics) -> BusEvents {
        BusEvents { bus, topics }
    }
}

impl Events for BusEvents {
    fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let words = self.topics.event(&event);
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let meta = Meta::follow(&origin.request_id, origin.trace_id.as_deref());
            let sent = match Subject::new(&words) {
                Ok(subject) => self.bus.publish(&subject, &event.body, &meta).await,
                Err(error) => Err(BusError::Encode(error.to_string())),
            };
            if let Err(error) = sent {
                eprintln!(
                    "{NAME} [{}]: событие {} не ушло: {error}",
                    origin.request_id,
                    words.join(".")
                );
            }
        })
    }
}
