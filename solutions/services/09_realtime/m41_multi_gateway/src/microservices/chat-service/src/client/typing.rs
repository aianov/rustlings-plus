use contracts::{Origin, Topics, TypingNote};
use nats_engine::{Bus, Meta, Subject};

use crate::common::NAME;
use crate::repository::BoxFuture;
use crate::service::{Typist, Undelivered};

pub struct BusTypist {
    bus: Bus,
    topics: Topics,
}

impl BusTypist {
    pub fn new(bus: Bus, topics: Topics) -> BusTypist {
        BusTypist { bus, topics }
    }
}

impl Typist for BusTypist {
    fn typing(&self, note: TypingNote, origin: Origin) -> BoxFuture<'_, Result<(), Undelivered>> {
        Box::pin(async move {
            let words = self.topics.typing(note.to);
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let subject = Subject::new(&words).map_err(|_| Undelivered)?;
            let meta = Meta::follow(&origin.request_id, origin.trace_id.as_deref());
            self.bus
                .publish(&subject, &note, &meta)
                .await
                .map_err(|error| {
                    eprintln!(
                        "{NAME} [{}]: «печатает» для {} не ушло: {error}",
                        origin.request_id, note.to
                    );
                    Undelivered
                })
        })
    }
}
