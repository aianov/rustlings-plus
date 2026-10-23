use contracts::Topics;
use nats_engine::{Bus, BusError, Meta, Subject};

use crate::common::{NAME, Notice};
use crate::repository::BoxFuture;
use crate::service::Courier;

pub struct BusCourier {
    bus: Bus,
    topics: Topics,
}

impl BusCourier {
    pub fn new(bus: Bus, topics: Topics) -> BusCourier {
        BusCourier { bus, topics }
    }
}

impl Courier for BusCourier {
    fn deliver(&self, notice: Notice, request_id: Option<String>) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let words = self.topics.notice(notice.to);
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let meta = match &request_id {
                Some(request_id) => Meta::follow(request_id, None),
                None => Meta::fresh(),
            };
            let sent = match Subject::new(&words) {
                Ok(subject) => self.bus.publish(&subject, &notice.view(), &meta).await,
                Err(error) => Err(BusError::Encode(error.to_string())),
            };
            if let Err(error) = sent {
                tracing::warn!(
                    "{NAME} [{}]: уведомление {} для {} не ушло: {error}",
                    meta.request_id,
                    notice.id,
                    notice.to
                );
            }
        })
    }
}
