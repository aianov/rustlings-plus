// Запрос профиля идёт в десять сервисов параллельно и ждёт самого медленного. Каждый из них быстрый: «99% ответов
// в пределах 200 мс». Но пользователь жалуется, что профиль медленный примерно раз в десять открытий. Тут нет ошибки,
// есть арифметика хвоста: чем больше сервисов в запросе, тем чаще хотя бы один из них попадёт в свой хвост. Цель:
// посчитать вероятность этого, понять, насколько быстрыми должны быть сервисы, и найти типичную задержку составного запроса
// по выборке одного сервиса (сервисы считаются независимыми и одинаковыми).


use std::time::Duration;

fn main() {}


pub fn percentile(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (q.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}


/// Вероятность, что хотя бы один из `calls` независимых вызовов окажется медленным, если один вызов
/// медленный с вероятностью `p_slow`.
pub fn p_any_slow(p_slow: f64, calls: u32) -> f64 {
    let _ = (p_slow, calls);
    todo!()
}

/// Какую долю быстрых ответов должен давать каждый из `calls` сервисов, чтобы составной запрос
/// укладывался в срок с долей `good_share`.
pub fn quantile_needed(good_share: f64, calls: u32) -> f64 {
    let _ = (good_share, calls);
    todo!()
}

/// Квантиль `q` задержки составного запроса, если один сервис описан отсортированной выборкой `sorted`.
pub fn fanout_quantile(sorted: &[Duration], q: f64, calls: u32) -> Duration {
    let _ = (sorted, q, calls);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ten_services_at_p99_make_every_tenth_request_slow() {
        let p = p_any_slow(0.01, 10);
        assert!(close(p, 0.0956179), "вероятность хотя бы одного медленного из десяти: {p}");
        assert!(close(p_any_slow(0.01, 1), 0.01), "один вызов: вероятность та же");
        assert_eq!(p_any_slow(0.0, 100), 0.0);
        assert!(p_any_slow(0.01, 100) > 0.63, "сто вызовов: большинство запросов задето хвостом");
    }

    #[test]
    fn each_service_must_be_much_better_than_the_whole() {
        let q = quantile_needed(0.99, 10);
        assert!(close(q, 0.998995), "для p99 составного запроса каждому из десяти нужен p99.9: {q}");
        assert!(close(quantile_needed(0.5, 1), 0.5), "для одного вызова доли совпадают");
        assert!(quantile_needed(0.99, 100) > 0.9998, "для ста вызовов ещё строже");
    }

    #[test]
    fn the_median_of_a_wide_request_is_a_high_percentile_of_one_service() {
        let sorted: Vec<Duration> = (1..=1000).map(Duration::from_millis).collect();
        let whole = fanout_quantile(&sorted, 0.5, 10);
        assert_eq!(whole, Duration::from_millis(934), "медиана запроса к десяти сервисам это p93.3 одного: типичный запрос ждёт хвоста");
        assert_eq!(fanout_quantile(&sorted, 0.99, 1), Duration::from_millis(990), "один вызов: обычный p99");
        assert_eq!(fanout_quantile(&sorted, 0.99, 10), Duration::from_millis(999), "p99 запроса к десяти это почти максимум одного сервиса");
    }

    #[test]
    fn an_empty_sample_gives_zero() {
        assert_eq!(fanout_quantile(&[], 0.9, 5), Duration::ZERO);
    }
}
