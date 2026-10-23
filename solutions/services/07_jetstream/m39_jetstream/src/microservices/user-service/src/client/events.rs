use contracts::{Event, Origin, Topics};
use nats_engine::js::{StreamSpec, ensure_stream, publish_event};
use nats_engine::{Bus, BusError, Meta, Subject, fresh_id};

use crate::common::NAME;
use crate::repository::BoxFuture;
use crate::service::Events;

pub struct BusEvents {
    bus: Bus,
    topics: Topics,
}

impl BusEvents {
    pub async fn open(bus: Bus, topics: Topics) -> Result<BusEvents, BusError> {
        let spec = StreamSpec::new(&topics.stream(), topics.stream_subjects());
        ensure_stream(&bus, &spec).await?;
        Ok(BusEvents { bus, topics })
    }
}

impl Events for BusEvents {
    fn publish(&self, event: Event, origin: Origin) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let words = self.topics.event(&event);
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let meta = Meta::follow(&origin.request_id, origin.trace_id.as_deref());
            let event_id = fresh_id("evt");
            let sent = match Subject::new(&words) {
                Ok(subject) => publish_event(&self.bus, &subject, &event_id, &event.body, &meta)
                    .await
                    .map(|_| ()),
                Err(error) => Err(BusError::Encode(error.to_string())),
            };
            if let Err(error) = sent {
                eprintln!(
                    "{NAME} [{}]: событие {} ({event_id}) не записано в stream: {error}",
                    origin.request_id,
                    words.join(".")
                );
            }
        })
    }
}
