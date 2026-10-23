use contracts::{SessionRevoked, Topics, UserId};
use nats_engine::{Bus, Meta, Subject};

use crate::common::NAME;
use crate::repository::BoxFuture;
use crate::service::Revoker;

pub struct BusRevoker {
    bus: Bus,
    topics: Topics,
}

impl BusRevoker {
    pub fn new(bus: Bus, topics: Topics) -> BusRevoker {
        BusRevoker { bus, topics }
    }
}

impl Revoker for BusRevoker {
    fn revoked(&self, user_id: u64, sessions: Vec<String>) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let words = self.topics.revoked();
            let words: Vec<&str> = words.iter().map(String::as_str).collect();
            let Ok(subject) = Subject::new(&words) else {
                return;
            };
            let note = SessionRevoked {
                user: UserId(user_id),
                sessions,
            };
            if let Err(error) = self.bus.publish(&subject, &note, &Meta::fresh()).await {
                eprintln!(
                    "{NAME}: отзыв сессий {:?} не ушёл шлюзам: {error}; они закроют соединения по сроку токена",
                    note.sessions
                );
            }
        })
    }
}
