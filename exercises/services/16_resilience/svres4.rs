// Сервис упал на секунду, и тысяча клиентов разом получила ошибку. Если все они подождут по 100 мс и повторят, то в момент 100 мс
// на только что поднявшийся сервис придёт тысяча запросов разом, потом, после 200 мс, снова вся тысяча, и так до бесконечности.
// Повторы должны быть растянуты по времени: пауза растёт с номером попытки и случайна внутри окна.
// Цель: `backoff` возвращает случайную паузу от нуля до потолка, потолок удваивается с каждой попыткой и не превышает `cap`,
// а случайность приходит снаружи через типаж `Jitter`, чтобы тесты были точными.


use std::time::Duration;

fn main() {}

/// Источник случайности: число из [0, 1).
pub trait Jitter {
    fn unit(&mut self) -> f64;
}

/// Верхняя граница паузы для попытки `attempt` (с нуля): `base`, `2*base`, `4*base`, и так до `cap`.
/// Не должна паниковать и переполняться при любом номере попытки.
pub fn ceiling(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let _ = (attempt, base, cap);
    todo!()
}

/// Случайная пауза перед повтором: от нуля до `ceiling` включительно слева, равномерно.
pub fn backoff(attempt: u32, base: Duration, cap: Duration, rng: &mut impl Jitter) -> Duration {
    let _ = (attempt, base, cap, rng);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    struct Fixed(f64);
    impl Jitter for Fixed {
        fn unit(&mut self) -> f64 {
            self.0
        }
    }

    struct Lcg(u64);
    impl Jitter for Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn the_ceiling_doubles_up_to_the_cap() {
        let got: Vec<Duration> = (0..6).map(|n| ceiling(n, MS(100), MS(1500))).collect();
        assert_eq!(got, vec![MS(100), MS(200), MS(400), MS(800), MS(1500), MS(1500)], "потолок удваивается и упирается в cap");
    }

    #[test]
    fn huge_attempt_numbers_do_not_overflow() {
        for attempt in [31, 32, 63, 64, 1000, u32::MAX] {
            assert_eq!(ceiling(attempt, MS(100), Duration::from_secs(30)), Duration::from_secs(30), "попытка {attempt}: потолок должен быть cap, без паники");
        }
        assert_eq!(ceiling(70, Duration::from_secs(u64::MAX / 4), Duration::from_secs(5)), Duration::from_secs(5));
    }

    #[test]
    fn the_pause_is_a_fraction_of_the_ceiling() {
        assert_eq!(backoff(3, MS(100), MS(10_000), &mut Fixed(0.0)), MS(0), "полный джиттер допускает нулевую паузу");
        assert_eq!(backoff(3, MS(100), MS(10_000), &mut Fixed(0.5)), MS(400), "половина потолка 800 мс");
        let almost = backoff(3, MS(100), MS(10_000), &mut Fixed(0.999999));
        assert!(almost < MS(800) && almost > MS(799), "почти потолок, но не больше: {almost:?}");
    }

    #[test]
    fn full_jitter_spreads_over_the_whole_window() {
        let mut rng = Lcg(42);
        let samples: Vec<Duration> = (0..2000).map(|_| backoff(3, MS(100), MS(10_000), &mut rng)).collect();
        let mean = samples.iter().map(|d| d.as_secs_f64()).sum::<f64>() / samples.len() as f64;
        assert!((0.36..0.44).contains(&mean), "среднее при полном джиттере половина потолка (0.4 с), получилось {mean:.3} с");
        let min = samples.iter().min().unwrap();
        let max = samples.iter().max().unwrap();
        assert!(*min < MS(100), "при полном джиттере встречаются очень короткие паузы, а минимум {min:?}: похоже на «половина плюс случайное»");
        assert!(*max > MS(700), "паузы должны добираться почти до потолка 800 мс, максимум {max:?}");
    }

    #[test]
    fn a_crowd_does_not_retry_in_lockstep() {
        let mut rng = Lcg(7);
        let delays: HashSet<Duration> = (0..100).map(|_| backoff(2, MS(100), MS(10_000), &mut rng)).collect();
        assert!(delays.len() >= 95, "сто клиентов упали одновременно, а разных пауз всего {}: они придут снова толпой", delays.len());
    }
}
