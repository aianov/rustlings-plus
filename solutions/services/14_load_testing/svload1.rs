
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
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

pub fn summarize(samples: &[Duration]) -> Summary {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let mean = if sorted.is_empty() { Duration::ZERO } else { sorted.iter().sum::<Duration>() / sorted.len() as u32 };
    Summary {
        count: sorted.len(),
        mean,
        p50: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
        max: sorted.last().copied().unwrap_or(Duration::ZERO),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn nearest_rank_on_a_hundred_values() {
        let samples: Vec<Duration> = (1..=100).map(MS).collect();
        let s = summarize(&samples);
        assert_eq!(s.count, 100);
        assert_eq!((s.p50, s.p95, s.p99, s.max), (MS(50), MS(95), MS(99), MS(100)), "ближайший ранг: p95 из 100 это 95-й по возрастанию");
        assert_eq!(s.mean, Duration::from_micros(50_500));
    }

    #[test]
    fn the_order_of_the_input_does_not_matter_and_the_input_is_not_changed() {
        let samples: Vec<Duration> = vec![MS(40), MS(10), MS(30), MS(20)];
        let copy = samples.clone();
        let s = summarize(&samples);
        assert_eq!(samples, copy, "summarize получила срез и не должна его менять");
        assert_eq!((s.p50, s.max), (MS(20), MS(40)), "p50 из четырёх это второй по возрастанию");
    }

    #[test]
    fn the_mean_looks_fine_while_the_tail_is_awful() {
        let mut samples = vec![MS(10); 980];
        samples.extend(vec![MS(5000); 20]);
        let s = summarize(&samples);
        assert!(s.mean < MS(120), "среднее около 110 мс: {:?}", s.mean);
        assert_eq!(s.p50, MS(10));
        assert_eq!(s.p99, MS(5000), "p99 из тысячи это 990-й по возрастанию, а это уже медленные 20 запросов, которых среднее не показывает");
    }

    #[test]
    fn small_and_empty_samples_do_not_panic() {
        let empty = summarize(&[]);
        assert_eq!((empty.count, empty.mean, empty.p50, empty.p99, empty.max), (0, Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO));
        let one = summarize(&[MS(7)]);
        assert_eq!((one.p50, one.p95, one.p99, one.max, one.mean), (MS(7), MS(7), MS(7), MS(7), MS(7)), "у единственного замера все перцентили равны ему");
    }

    #[test]
    fn percentile_edges_are_clamped() {
        let sorted = [MS(1), MS(2), MS(3)];
        assert_eq!(percentile(&sorted, 0.0), MS(1), "нулевой перцентиль это минимум, а не выход за границу массива");
        assert_eq!(percentile(&sorted, 1.0), MS(3));
        assert_eq!(percentile(&sorted, 7.0), MS(3), "q больше единицы приводится к единице");
        assert_eq!(percentile(&sorted, -1.0), MS(1));
        assert_eq!(percentile(&[], 0.5), Duration::ZERO);
    }
}
