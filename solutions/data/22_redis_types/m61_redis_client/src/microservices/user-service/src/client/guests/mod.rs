use contracts::UserId;
use resp_client::{Client, cmd, key};

use crate::service::{BoxGuests, GUESTS_KEPT, Guests};

pub const GUESTS_TTL: u64 = 7 * 24 * 60 * 60;

pub struct RedisGuests {
    redis: Client,
}

pub fn guests_key(owner: UserId) -> String {
    key("users", "guests", owner.0)
}

impl RedisGuests {
    pub fn new(redis: Client) -> RedisGuests {
        RedisGuests { redis }
    }
}

impl Guests for RedisGuests {
    fn visited(&self, owner: UserId, guest: UserId) -> BoxGuests<'_, ()> {
        Box::pin(async move {
            let key = guests_key(owner);
            let keep = i64::try_from(GUESTS_KEPT).unwrap_or(i64::MAX) - 1;
            let done = self
                .redis
                .pipeline(vec![
                    cmd("LREM").arg(&key).arg(0).arg(guest.0),
                    cmd("LPUSH").arg(&key).arg(guest.0),
                    cmd("LTRIM").arg(&key).arg(0).arg(keep),
                    cmd("EXPIRE").arg(&key).arg(GUESTS_TTL),
                ])
                .await;
            if let Err(error) = done {
                tracing::warn!("user-service: гость не записан: {error}");
            }
        })
    }

    fn recent(&self, owner: UserId, limit: usize) -> BoxGuests<'_, Vec<UserId>> {
        Box::pin(async move {
            let stop = i64::try_from(limit).unwrap_or(i64::MAX) - 1;
            match self.redis.lrange(&guests_key(owner), 0, stop).await {
                Ok(ids) => ids
                    .iter()
                    .filter_map(|id| id.parse().ok())
                    .map(UserId)
                    .collect(),
                Err(error) => {
                    tracing::warn!("user-service: гости не прочитаны: {error}");
                    Vec::new()
                }
            }
        })
    }
}
