use std::fmt;
use std::future::Future;
use std::time::Duration;

use nats_engine::js::{Bucket, open_bucket};
use nats_engine::{Bus, BusError};
use tokio::time::{Instant, MissedTickBehavior, interval_at};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub name: String,
    pub owner: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseLost {
    pub name: String,
    pub reason: String,
}

impl fmt::Display for LeaseLost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "аренда {} потеряна: {}", self.name, self.reason)
    }
}

impl std::error::Error for LeaseLost {}

#[derive(Clone)]
pub struct Leases {
    bucket: Bucket,
    ttl: Duration,
}

impl Leases {
    pub async fn open(bus: &Bus, bucket: &str, ttl: Duration) -> Result<Leases, BusError> {
        Ok(Leases {
            bucket: open_bucket(bus, bucket, ttl).await?,
            ttl,
        })
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub async fn acquire(&self, name: &str, owner: &str) -> Result<Option<Lease>, BusError> {
        let claimed = self.bucket.claim(name, owner.as_bytes()).await?;
        Ok(claimed.map(|revision| Lease {
            name: name.to_string(),
            owner: owner.to_string(),
            revision,
        }))
    }

    pub async fn renew(&self, lease: &Lease) -> Result<Option<Lease>, BusError> {
        let renewed = self
            .bucket
            .renew(&lease.name, lease.owner.as_bytes(), lease.revision)
            .await?;
        Ok(renewed.map(|revision| Lease {
            revision,
            ..lease.clone()
        }))
    }

    pub async fn release(&self, lease: &Lease) -> Result<bool, BusError> {
        self.bucket.release(&lease.name, lease.revision).await
    }
}

pub async fn run_exclusive<T, Fut>(leases: &Leases, lease: Lease, work: Fut) -> Result<T, LeaseLost>
where
    Fut: Future<Output = T>,
{
    let every = (leases.ttl / 3).max(Duration::from_millis(10));
    let mut renewal = interval_at(Instant::now() + every, every);
    renewal.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut current = lease;
    tokio::pin!(work);
    let done = loop {
        tokio::select! {
            done = &mut work => break done,
            _ = renewal.tick() => {
                let lost = |reason: String| LeaseLost { name: current.name.clone(), reason };
                match leases.renew(&current).await {
                    Ok(Some(renewed)) => current = renewed,
                    Ok(None) => return Err(lost(String::from("её перехватил другой владелец"))),
                    Err(error) => return Err(lost(error.to_string())),
                }
            }
        }
    };
    let _ = leases.release(&current).await;
    Ok(done)
}
