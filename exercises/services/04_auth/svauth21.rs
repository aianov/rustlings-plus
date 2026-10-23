// После взлома одного аккаунта выяснилось, что токен сброса пароля предсказуем. Генератор засеян текущим временем
// в секундах, а время выдачи письма атакующий знает с точностью до нескольких секунд: он просто
// перебрал секунды вокруг и получил список всех возможных токенов. Дополнительно токен короткий. Цель: токены
// неотличимы от случайных для того, кто знает всё про алгоритм, кроме системного источника случайности:
// не меньше 32 знаков из `[A-Za-z0-9]`, уникальные, не зависят от времени и пользователя.

use rand::distributions::Alphanumeric;
use rand::rngs::OsRng;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct ManualClock(std::sync::atomic::AtomicU64);

impl ManualClock {
    pub fn new(start: u64) -> ManualClock {
        ManualClock(std::sync::atomic::AtomicU64::new(start))
    }

    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

pub fn reset_token(clock: &dyn Clock, user_id: u64) -> String {
    let _ = user_id;
    StdRng::seed_from_u64(clock.now()).sample_iter(&Alphanumeric).take(16).map(char::from).collect()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashSet;

    fn attacker_candidates(approx_now: u64) -> HashSet<String> {
        let mut all = HashSet::new();
        for seed in approx_now.saturating_sub(10)..=approx_now + 10 {
            for len in [8, 12, 16, 20, 24, 32, 40, 64] {
                let token: String = StdRng::seed_from_u64(seed).sample_iter(&Alphanumeric).take(len).map(char::from).collect();
                all.insert(token);
            }
        }
        all
    }

    #[test]
    fn attacker_who_knows_the_time_cannot_guess_the_token() {
        let clock = ManualClock::new(1_700_000_000);
        let victim = reset_token(&clock, 42);
        let candidates = attacker_candidates(1_700_000_003);
        assert!(!candidates.contains(&victim), "токен {victim} найден среди кандидатов, посчитанных из времени: генератор нельзя сеять временем");
    }

    #[test]
    fn tokens_are_long_unique_and_url_safe() {
        let clock = ManualClock::new(5);
        let mut seen = HashSet::new();
        for _ in 0..1_000 {
            let token = reset_token(&clock, 7);
            assert!(token.len() >= 32, "токен короче 32 знаков: {token}");
            assert!(token.chars().all(|c| c.is_ascii_alphanumeric()), "только буквы и цифры: {token}");
            assert!(seen.insert(token), "повторился токен при тех же времени и пользователе: в токене не должно быть ничего детерминированного");
        }
    }

    #[test]
    fn different_users_get_unrelated_tokens() {
        let clock = ManualClock::new(5);
        assert_ne!(reset_token(&clock, 1), reset_token(&clock, 2), "разные пользователи");
        assert_ne!(reset_token(&clock, 1), reset_token(&clock, 1), "один и тот же пользователь дважды");
    }
}
