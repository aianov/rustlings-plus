// Защиту от перебора сделали «по аккаунту»: после пяти неудач вход в аккаунт блокируется на 15 минут для всех. Выглядит надёжно,
// пока кто-то не понял, что можно просто посылать неверные пароли на чужой аккаунт: хозяин не может войти, а атакующему ничего не стоит.
// Так блокировка превращается в оружие против жертвы. Правильная цена неудач ложится на того, кто их делает: на пару «аккаунт, адрес»
// и на адрес в целом, а не на всех обладателей аккаунта.
// Цель: интерфейс `Throttle` прежний, но пятьдесят неудач с адреса атакующего не мешают хозяину входить с его адреса, адрес атакующего при этом
// замедляется, а перебор разных аккаунтов с одного адреса тоже тормозит. Успех сбрасывает счёт пары.


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
    accounts: HashMap<String, Entry>,
}

impl Throttle {
    pub fn new() -> Throttle {
        Throttle::default()
    }

    pub fn check(&self, account: &str, ip: IpAddr, now: Instant) -> Result<(), Duration> {
        let _ = ip;
        match self.accounts.get(account).and_then(|e| e.blocked_until) {
            Some(until) if until > now => Err(until - now),
            _ => Ok(()),
        }
    }

    pub fn record_failure(&mut self, account: &str, ip: IpAddr, now: Instant) {
        let _ = ip;
        let e = self.accounts.entry(account.to_string()).or_default();
        e.failures += 1;
        if e.failures >= 5 {
            e.blocked_until = Some(now + Duration::from_secs(15 * 60));
        }
    }

    pub fn record_success(&mut self, account: &str, ip: IpAddr) {
        let _ = ip;
        self.accounts.remove(account);
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
