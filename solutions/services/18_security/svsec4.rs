
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

fn main() {}

#[derive(Default)]
struct Entry {
    failures: u32,
    blocked_until: Option<Instant>,
}

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
    pub fn new() -> Throttle {
        Throttle::default()
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([198, 51, 100, n])
    }

    #[test]
    fn an_attacker_cannot_lock_the_owner_out() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..50 {
            t.record_failure("victim", ip(66), t0);
        }
        assert_eq!(t.check("victim", ip(7), t0), Ok(()), "хозяин с обычного адреса не может войти из-за чужих неудач: блокировка стала оружием");
        assert!(t.check("victim", ip(66), t0).is_err(), "а сам атакующий должен ждать");
    }

    #[test]
    fn the_attacker_is_slowed_on_every_account_from_his_address() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for i in 0..40 {
            t.record_failure(&format!("user{i}"), ip(66), t0);
        }
        assert!(t.check("anna", ip(66), t0).is_err(), "перебор разных аккаунтов с одного адреса должен тормозить весь адрес");
        assert_eq!(t.check("anna", ip(8), t0), Ok(()), "чужой адрес не страдает");
    }

    #[test]
    fn distributed_guesses_do_not_lock_the_account_for_everyone() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for n in 100..140 {
            t.record_failure("victim", ip(n), t0);
        }
        assert_eq!(t.check("victim", ip(7), t0), Ok(()), "сорок разных адресов по одной неудаче не повод запирать аккаунт для хозяина");
    }

    #[test]
    fn success_resets_the_pair_and_delays_grow() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..6 {
            t.record_failure("anna", ip(1), t0);
        }
        let wait = t.check("anna", ip(1), t0).unwrap_err();
        assert!(wait >= Duration::from_secs(8) && wait <= Duration::from_secs(60), "после шести неудач задержка выросла, но не стала вечной: {wait:?}");
        t.record_success("anna", ip(1));
        assert_eq!(t.check("anna", ip(1), t0), Ok(()));
    }
}
