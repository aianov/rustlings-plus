// В обработчике входа проверка пароля argon2 вызывается прямо в async-функции. Пока один пользователь
// ждёт свою проверку, весь поток рантайма занят вычислением: `/health`, чужие запросы и таймеры
// замирают. Ещё бы: если прилетит сотня входов разом, сервер уйдёт в своп, потому что каждая проверка
// держит много памяти. Цель: `PasswordService::verify` не блокирует рантайм и позволяет считать
// одновременно не больше `max_parallel` проверок.

use std::sync::Arc;
use tokio::sync::Semaphore;

pub type VerifyFn = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

pub struct PasswordService {
    verify_fn: VerifyFn,
    permits: Arc<Semaphore>,
}

impl PasswordService {
    pub fn new(verify_fn: VerifyFn, max_parallel: usize) -> PasswordService {
        PasswordService { verify_fn, permits: Arc::new(Semaphore::new(max_parallel)) }
    }

    pub async fn verify(&self, password: &str, stored: &str) -> bool {
        (self.verify_fn)(password, stored)
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn slow(ms: u64) -> VerifyFn {
        Arc::new(move |password, stored| {
            std::thread::sleep(Duration::from_millis(ms));
            password == stored
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn result_is_passed_through() {
        let service = PasswordService::new(slow(1), 2);
        assert!(service.verify("a", "a").await, "совпадение");
        assert!(!service.verify("a", "b").await, "несовпадение");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn runtime_keeps_ticking_while_hashes_are_computed() {
        let service = Arc::new(PasswordService::new(slow(120), 4));
        let ticks = Arc::new(AtomicUsize::new(0));
        let ticker = {
            let ticks = ticks.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(5));
                loop {
                    interval.tick().await;
                    ticks.fetch_add(1, Ordering::SeqCst);
                }
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        let before = ticks.load(Ordering::SeqCst);
        let checks: Vec<_> = (0..3)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move { service.verify("x", "x").await })
            })
            .collect();
        for check in checks {
            assert!(check.await.unwrap(), "проверка прошла");
        }
        let during = ticks.load(Ordering::SeqCst) - before;
        ticker.abort();
        assert!(
            during >= 8,
            "пока считались хеши, таймер тикнул всего {during} раз: проверка занимает поток рантайма, вынеси её на пул для блокирующих задач"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn parallelism_is_capped_but_used() {
        let now = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let verify_fn: VerifyFn = {
            let (now, peak) = (now.clone(), peak.clone());
            Arc::new(move |_, _| {
                let current = now.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40));
                now.fetch_sub(1, Ordering::SeqCst);
                true
            })
        };
        let service = Arc::new(PasswordService::new(verify_fn, 2));
        let checks: Vec<_> = (0..8)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move { service.verify("x", "x").await })
            })
            .collect();
        for check in checks {
            check.await.unwrap();
        }
        let peak = peak.load(Ordering::SeqCst);
        assert!(peak <= 2, "одновременно считалось {peak} проверок при лимите 2: разрешение надо держать до конца вычисления");
        assert_eq!(peak, 2, "лимит 2 означает, что две проверки идут параллельно, а не по одной");
    }
}
