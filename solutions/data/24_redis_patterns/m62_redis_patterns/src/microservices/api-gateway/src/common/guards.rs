use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use contracts::UserId;
use service_runtime::{Breaker, Buckets, Bulkhead, RetryBudget, RetryPolicy};

use super::profiles::ProfileCache;
use super::shared::{RedisProfiles, SharedLimits};
use tokio::time::Instant;

pub const PROFILE_FRESH: Duration = Duration::from_secs(30);

pub const IDLE_BUCKET: Duration = Duration::from_secs(300);

pub const USER_RATE_ENV: &str = "GATEWAY_USER_RATE";

pub const USER_BURST_ENV: &str = "GATEWAY_USER_BURST";

pub const IP_RATE_ENV: &str = "GATEWAY_IP_RATE";

pub const IP_BURST_ENV: &str = "GATEWAY_IP_BURST";

pub const SHED_ENV: &str = "GATEWAY_SHED_INFLIGHT";

pub const PROFILE_FRESH_ENV: &str = "GATEWAY_PROFILE_FRESH_MS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimits {
    pub user_rate: u32,
    pub user_burst: u32,
    pub ip_rate: u32,
    pub ip_burst: u32,
    pub shed_inflight: usize,
}

impl Default for RateLimits {
    fn default() -> RateLimits {
        RateLimits {
            user_rate: 100,
            user_burst: 200,
            ip_rate: 2_000,
            ip_burst: 4_000,
            shed_inflight: 4_096,
        }
    }
}

pub struct Guards {
    pub users: Buckets<UserId>,
    pub ips: Buckets<IpAddr>,
    pub profiles: ProfileCache,
    pub user_service: Breaker,
    pub user_seats: Bulkhead,
    pub retries: RetryBudget,
    pub policy: RetryPolicy,
    inflight: AtomicUsize,
    shed: usize,
    pub shared: Option<SharedLimits>,
    pub cached: Option<RedisProfiles>,
}

pub struct InFlight {
    guards: Arc<Guards>,
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.guards.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Guards {
    pub fn new(limits: RateLimits, profile_fresh: Duration) -> Guards {
        Guards {
            users: Buckets::new(f64::from(limits.user_rate), f64::from(limits.user_burst)),
            ips: Buckets::new(f64::from(limits.ip_rate), f64::from(limits.ip_burst)),
            profiles: ProfileCache::new(profile_fresh),
            user_service: Breaker::new(5, Duration::from_secs(5)),
            user_seats: Bulkhead::new(64),
            retries: RetryBudget::default(),
            policy: RetryPolicy::default(),
            inflight: AtomicUsize::new(0),
            shed: limits.shed_inflight,
            shared: None,
            cached: None,
        }
    }

    pub fn with_redis(self, shared: SharedLimits, cached: RedisProfiles) -> Guards {
        Guards {
            shared: Some(shared),
            cached: Some(cached),
            ..self
        }
    }

    pub async fn take_user(&self, user: UserId) -> Result<(), Duration> {
        match &self.shared {
            Some(shared) => shared.take_user(user).await,
            None => self.users.take(&user),
        }
    }

    pub async fn take_ip(&self, ip: IpAddr) -> Result<(), Duration> {
        match &self.shared {
            Some(shared) => shared.take_ip(ip).await,
            None => self.ips.take(&ip),
        }
    }

    pub fn admit(self: &Arc<Guards>) -> Option<InFlight> {
        let now = self.inflight.fetch_add(1, Ordering::AcqRel) + 1;
        let flight = InFlight {
            guards: Arc::clone(self),
        };
        (now <= self.shed).then_some(flight)
    }

    pub fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Acquire)
    }

    pub fn evict(&self, now: Instant) -> usize {
        self.users.evict_idle(IDLE_BUCKET, now) + self.ips.evict_idle(IDLE_BUCKET, now)
    }
}

impl Default for Guards {
    fn default() -> Guards {
        Guards::new(RateLimits::default(), PROFILE_FRESH)
    }
}
