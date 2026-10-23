
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

    /// `Ok`, если можно пробовать, иначе сколько ещё ждать.
    pub fn check(&self, account: &str, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        let pair = self.pairs.get(&(account.to_string(), ip)).and_then(|e| e.blocked_until);
        let whole = self.ips.get(&ip).and_then(|e| e.blocked_until);
        let until = pair.into_iter().chain(whole).max();
        match until {
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
        IpAddr::from([203, 0, 113, n])
    }

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn the_first_two_failures_are_free_then_the_delay_grows() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        t.record_failure("anna", ip(1), t0);
        t.record_failure("anna", ip(1), t0);
        assert_eq!(t.check("anna", ip(1), t0), Ok(()), "две неудачи ещё не повод ждать");
        t.record_failure("anna", ip(1), t0);
        assert_eq!(t.check("anna", ip(1), t0), Err(S(1)), "третья неудача: ждать 1 секунду");
        assert_eq!(t.check("anna", ip(1), t0 + Duration::from_millis(400)), Err(Duration::from_millis(600)), "остаток ожидания уменьшается");
        assert_eq!(t.check("anna", ip(1), t0 + S(1)), Ok(()));
        t.record_failure("anna", ip(1), t0 + S(1));
        assert_eq!(t.check("anna", ip(1), t0 + S(1)), Err(S(2)), "четвёртая: 2 секунды");
        t.record_failure("anna", ip(1), t0 + S(3));
        assert_eq!(t.check("anna", ip(1), t0 + S(3)), Err(S(4)), "пятая: 4 секунды");
    }

    #[test]
    fn the_delay_is_capped() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..9 {
            t.record_failure("anna", ip(1), t0);
        }
        let wait = t.check("anna", ip(1), t0).unwrap_err();
        assert_eq!(wait, S(60), "пара (аккаунт, адрес) ждёт не больше минуты (после 9 неудач 2^6 = 64 с, обрезано до 60), а не {wait:?}");
    }

    #[test]
    fn success_resets_the_pair() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..4 {
            t.record_failure("anna", ip(1), t0);
        }
        t.record_success("anna", ip(1));
        assert_eq!(t.check("anna", ip(1), t0), Ok(()));
        t.record_failure("anna", ip(1), t0);
        t.record_failure("anna", ip(1), t0);
        assert_eq!(t.check("anna", ip(1), t0), Ok(()), "после успеха счёт неудач начался заново");
    }

    #[test]
    fn pairs_are_independent() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..5 {
            t.record_failure("anna", ip(1), t0);
        }
        assert!(t.check("anna", ip(1), t0).is_err());
        assert_eq!(t.check("anna", ip(2), t0), Ok(()), "с другого адреса тот же аккаунт доступен");
        assert_eq!(t.check("boris", ip(1), t0), Ok(()), "с того же адреса другой аккаунт доступен, пока адрес в целом не злоупотребляет");
    }

    #[test]
    fn one_address_cannot_sweep_many_accounts() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for i in 0..12 {
            t.record_failure(&format!("user{i}"), ip(9), t0);
        }
        let wait = t.check("совсем-новый-аккаунт", ip(9), t0).expect_err("12 неудач с одного адреса на разные аккаунты должны тормозить сам адрес");
        assert_eq!(wait, S(4), "12-я неудача адреса: 2^(12-9-1) = 4 секунды");
        assert_eq!(t.check("user1", ip(10), t0), Ok(()));
    }

    #[test]
    fn check_is_read_only() {
        let t0 = Instant::now();
        let mut t = Throttle::new();
        for _ in 0..3 {
            t.record_failure("anna", ip(1), t0);
        }
        for _ in 0..100 {
            let _ = t.check("anna", ip(1), t0);
        }
        assert_eq!(t.check("anna", ip(1), t0), Err(S(1)), "проверка не должна накручивать счётчик");
    }
}
