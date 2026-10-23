use std::time::Duration;

use contracts::{ErrorBody, ErrorCode, NoticeAsk, NotifyMethod, Topics};
use nats_engine::{Bus, BusError, Meta, Subject};
use serde::de::DeserializeOwned;

#[derive(Clone)]
pub struct Notify {
    bus: Bus,
    topics: Topics,
    limit: Duration,
}

fn refused(error: BusError, request_id: &str) -> ErrorBody {
    let body = match error {
        BusError::Unavailable | BusError::NoService => ErrorBody::new(
            ErrorCode::Unavailable,
            "уведомления сейчас недоступны, попробуй позже",
        ),
        BusError::Timeout => {
            ErrorBody::new(ErrorCode::Timeout, "notify-service не ответил вовремя")
        }
        other => {
            eprintln!("api-gateway [{request_id}]: notify-service: {other}");
            ErrorBody::public(ErrorCode::Internal)
        }
    };
    body.with_request_id(request_id)
}

impl Notify {
    pub fn new(bus: Bus, topics: Topics, limit: Duration) -> Notify {
        Notify { bus, topics, limit }
    }

    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    pub async fn ask<A: DeserializeOwned>(
        &self,
        method: NotifyMethod,
        ask: &NoticeAsk,
        meta: &Meta,
    ) -> Result<A, ErrorBody> {
        let words = self.topics.notify(method);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let subject = Subject::new(&words)
            .map_err(|error| refused(BusError::Encode(error.to_string()), &meta.request_id))?;
        self.bus
            .request(&subject, ask, meta, self.limit)
            .await
            .map_err(|error| refused(error, &meta.request_id))
    }
}
