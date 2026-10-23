use std::future::Future;
use std::time::Duration;

use nats_engine::BusError;

use crate::lease::Leases;

pub fn spread(instance: &str, every: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in instance.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash % every.max(1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Periodic {
    every: u64,
    next: u64,
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}

impl Periodic {
    pub fn new(instance: &str, every: Duration, now: u64) -> Periodic {
        let every = millis(every);
        let mut next = now - now % every + spread(instance, every);
        if next <= now {
            next += every;
        }
        Periodic { every, next }
    }

    pub fn every(&self) -> u64 {
        self.every
    }

    pub fn next(&self) -> u64 {
        self.next
    }

    pub fn due(&mut self, now: u64) -> bool {
        if now < self.next {
            return false;
        }
        let missed = (now - self.next) / self.every + 1;
        self.next = self.next.saturating_add(missed.saturating_mul(self.every));
        true
    }
}

pub struct Leader {
    periodic: Periodic,
    leases: Leases,
    name: String,
    owner: String,
}

impl Leader {
    pub fn new(name: &str, owner: &str, every: Duration, leases: Leases, now: u64) -> Leader {
        Leader {
            periodic: Periodic::new(owner, every, now),
            leases,
            name: name.to_string(),
            owner: owner.to_string(),
        }
    }

    pub fn periodic(&self) -> &Periodic {
        &self.periodic
    }

    pub async fn tick<F, Fut>(&mut self, now: u64, run: F) -> Result<bool, BusError>
    where
        F: FnOnce(u64) -> Fut,
        Fut: Future<Output = ()>,
    {
        if !self.periodic.due(now) {
            return Ok(false);
        }
        let period = now / self.periodic.every;
        let name = format!("{}.{period}", self.name);
        match self.leases.acquire(&name, &self.owner).await? {
            Some(..) => {
                run(period).await;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
