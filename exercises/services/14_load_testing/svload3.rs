// Нагрузку создают четыре воркера, и каждый ведёт свою статистику. В итоговом отчёте p99 получается «средним из p99
// воркеров». Отчёт выглядит правдоподобно, но врёт: один воркер, которому достались только медленные запросы, тянет цифру
// вверх, а воркер без запросов тянет вниз. Цель: итог по всем воркерам считается так, как если бы замеры собрал один
// наблюдатель: p50, p99 и максимум по всем успешным замерам, ошибки складываются, воркеры без замеров не искажают цифры.


use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, Default)]
pub struct WorkerStats {
    pub samples: Vec<Duration>,
    pub errors: u64,
}

#[derive(Debug, PartialEq)]
pub struct Merged {
    pub ok: u64,
    pub errors: u64,
    pub p50: Duration,
    pub p99: Duration,
    pub max: Duration,
}


pub fn percentile(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (q.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}


pub fn merge(workers: &[WorkerStats]) -> Merged {
    fn worker_percentile(w: &WorkerStats, q: f64) -> Duration {
        let mut sorted = w.samples.clone();
        sorted.sort();
        percentile(&sorted, q)
    }
    let n = workers.len().max(1) as u32;
    let mean = |q: f64| workers.iter().map(|w| worker_percentile(w, q)).sum::<Duration>() / n;
    Merged {
        ok: workers.iter().map(|w| w.samples.len() as u64).sum(),
        errors: workers.iter().map(|w| w.errors).sum(),
        p50: mean(0.5),
        p99: mean(0.99),
        max: workers.iter().map(|w| worker_percentile(w, 1.0)).sum::<Duration>() / n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn stats(samples: Vec<Duration>) -> WorkerStats {
        WorkerStats { samples, errors: 0 }
    }

    #[test]
    fn the_result_equals_the_percentile_of_all_samples_together() {
        let fast = stats(vec![MS(10); 990]);
        let slow = stats(vec![MS(1000); 10]);
        let merged = merge(&[fast, slow]);
        assert_eq!(merged.ok, 1000);
        assert_eq!(merged.p99, MS(10), "из 1000 замеров всего 10 медленных: 990-й по возрастанию быстрый. Среднее из p99 воркеров (505 мс) соврало бы");
        assert_eq!(merged.max, MS(1000), "максимум это максимум, а не среднее максимумов");
    }

    #[test]
    fn the_order_of_workers_does_not_matter() {
        let a = stats((1..=300).map(MS).collect());
        let b = stats((301..=1000).map(MS).collect());
        let one = merge(&[a.clone(), b.clone()]);
        let two = merge(&[b, a]);
        assert_eq!(one, two);
        assert_eq!((one.p50, one.p99), (MS(500), MS(990)), "как у единой выборки 1..=1000");
    }

    #[test]
    fn workers_without_samples_do_not_drag_the_numbers_down() {
        let busy = stats(vec![MS(100); 50]);
        let idle = WorkerStats { samples: vec![], errors: 3 };
        let merged = merge(&[busy, idle.clone(), idle]);
        assert_eq!((merged.p50, merged.p99, merged.max), (MS(100), MS(100), MS(100)), "молчащий воркер это отсутствие данных, а не нулевая задержка");
        assert_eq!((merged.ok, merged.errors), (50, 6), "ошибки складываются");
    }

    #[test]
    fn nothing_at_all_is_zero_and_not_a_panic() {
        assert_eq!(merge(&[]), Merged { ok: 0, errors: 0, p50: Duration::ZERO, p99: Duration::ZERO, max: Duration::ZERO });
        let only_errors = merge(&[WorkerStats { samples: vec![], errors: 4 }]);
        assert_eq!((only_errors.ok, only_errors.errors, only_errors.p99), (0, 4, Duration::ZERO));
    }

    #[test]
    fn a_skewed_split_is_still_exact() {
        let w1 = stats(vec![MS(5); 9]);
        let w2 = stats(vec![MS(9000)]);
        let merged = merge(&[w1, w2]);
        assert_eq!((merged.p50, merged.p99, merged.max), (MS(5), MS(9000), MS(9000)), "из десяти замеров p99 это десятый, самый медленный");
    }
}
