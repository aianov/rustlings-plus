// Сервис получает по тысяче попыток входа в секунду с одного адреса: кто-то перебирает пароли. Каждая попытка стоит нам хеширования
// и ничего не стоит ему. Нужна цена за каждую неудачу, растущая с числом неудач, и отдельно за каждую пару «аккаунт, адрес»,
// и за адрес в целом (чтобы перебор по разным аккаунтам с одного адреса тоже замедлялся).
// Цель: `Throttle`. Задержка после n-й подряд неудачи: до `free` неудач ноль, дальше `base * 2^(n-free-1)`, но не больше `cap`.
// Для пары (аккаунт, адрес): free = 2, base = 1 с, cap = 60 с. Для адреса целиком: free = 9, base = 1 с, cap = 300 с. Успешный вход сбрасывает
// счётчик пары. `check` ничего не меняет, а отвечает, сколько ещё ждать.


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
    let _ = (failures, free, base, cap);
    todo!()
}

impl Throttle {
    pub fn new() -> Throttle {
        Throttle::default()
    }

    /// `Ok`, если можно пробовать, иначе сколько ещё ждать.
    pub fn check(&self, account: &str, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        let _ = (account, ip, now, &self.pairs, &self.ips);
        todo!()
    }

    pub fn record_failure(&mut self, account: &str, ip: IpAddr, now: Instant) {
        let _ = (account, ip, now);
        todo!()
    }

    pub fn record_success(&mut self, account: &str, ip: IpAddr) {
        let _ = (account, ip);
        todo!()
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
