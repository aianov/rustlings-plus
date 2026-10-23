use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Entry {
    failures: u32,
    blocked_until: Option<Instant>,
}

/// Задержка после неудач входа: на пару (аккаунт, адрес) и на адрес в целом.
#[derive(Default)]
pub struct Throttle {
    pairs: HashMap<(String, IpAddr), Entry>,
    ips: HashMap<IpAddr, Entry>,
}

fn delay(failures: u32, free: u32, base: u64, cap: u64) -> Duration {
    if failures <= free {
        return Duration::ZERO;
    }
    let steps = (failures - free - 1).min(20);
    Duration::from_secs(base.saturating_mul(1 << steps).min(cap))
}

impl Throttle {
    pub fn check(&self, account: &str, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        let pair = self.pairs.get(&(account.to_string(), ip)).and_then(|e| e.blocked_until);
        let whole = self.ips.get(&ip).and_then(|e| e.blocked_until);
        match pair.into_iter().chain(whole).max() {
            Some(t) if t > now => Err(t - now),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, account: &str, ip: IpAddr, now: Instant) {
        let pair = self.pairs.entry((account.to_string(), ip)).or_default();
        pair.failures += 1;
        pair.blocked_until = Some(now + delay(pair.failures, 2, 1, 60));
        let whole = self.ips.entry(ip).or_default();
        whole.failures += 1;
        whole.blocked_until = Some(now + delay(whole.failures, 9, 1, 300));
    }

    pub fn record_success(&mut self, account: &str, ip: IpAddr) {
        self.pairs.remove(&(account.to_string(), ip));
    }
}
