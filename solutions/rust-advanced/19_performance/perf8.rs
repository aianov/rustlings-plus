use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub min: Duration,
    pub median: Duration,
    pub p99: Duration,
    pub max: Duration,
    pub samples: usize,
}

pub fn percentile(sorted: &[Duration], p: u32) -> Duration {
    assert!(!sorted.is_empty(), "нет замеров");
    assert!((1..=100).contains(&p), "процентиль задают числом от 1 до 100");
    let rank = (sorted.len() * p as usize).div_ceil(100);
    sorted[rank - 1]
}

pub fn median(sorted: &[Duration]) -> Duration {
    assert!(!sorted.is_empty(), "нет замеров");
    let n = sorted.len();
    if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2 }
}

pub fn bench(warmup: usize, runs: usize, mut f: impl FnMut()) -> Stats {
    assert!(runs > 0, "нужен хотя бы один замер (runs)");
    for _ in 0..warmup {
        f();
    }
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        f();
        times.push(start.elapsed());
    }
    times.sort();
    Stats { min: times[0], median: median(&times), p99: percentile(&times, 99), max: times[runs - 1], samples: runs }
}

fn main() {
    let stats = bench(3, 20, || {
        std::hint::black_box((0..10_000u64).sum::<u64>());
    });
    println!("{stats:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
        fn code_only() -> String {
        SRC.split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }
    const SRC: &str = include_str!("perf8.rs");

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let hundred: Vec<Duration> = (1..=100).map(ms).collect();
        assert_eq!(percentile(&hundred, 99), ms(99), "p99 от ста замеров это 99-й по порядку");
        assert_eq!(percentile(&hundred, 50), ms(50));
        assert_eq!(percentile(&hundred, 100), ms(100));
        assert_eq!(percentile(&hundred, 1), ms(1));
        let five = [ms(10), ms(20), ms(30), ms(40), ms(50)];
        assert_eq!(percentile(&five, 99), ms(50), "ceil(0.99 * 5) = 5: хвост на малой выборке это максимум");
        assert_eq!(percentile(&five, 20), ms(10));
        assert_eq!(percentile(&five, 21), ms(20), "ceil(0.21 * 5) = 2");
        assert_eq!(percentile(&[ms(7)], 99), ms(7));
    }

    #[test]
    fn median_of_odd_and_even() {
        assert_eq!(median(&[ms(1), ms(5), ms(9)]), ms(5));
        assert_eq!(median(&[ms(1), ms(5), ms(9), ms(20)]), ms(7), "для чётного числа берётся среднее двух средних");
        assert_eq!(median(&[ms(4)]), ms(4));
    }

    #[test]
    #[should_panic(expected = "runs")]
    fn zero_runs_is_a_mistake() {
        bench(1, 0, || {});
    }

    #[test]
    fn warmup_is_called_but_not_recorded() {
        let calls = Cell::new(0usize);
        let stats = bench(3, 7, || calls.set(calls.get() + 1));
        assert_eq!(calls.get(), 10, "функция вызывается warmup + runs раз");
        assert_eq!(stats.samples, 7, "в статистику идут только runs замеров");
    }

    #[test]
    fn slow_warmup_does_not_leak_into_numbers() {
        let calls = Cell::new(0usize);
        let stats = bench(3, 9, || {
            calls.set(calls.get() + 1);
            std::thread::sleep(if calls.get() <= 3 { ms(60) } else { ms(2) });
        });
        assert!(stats.max < ms(40), "прогревочные 60 мс попали в замеры: max = {:?}", stats.max);
        assert!(stats.median >= ms(2), "медиана не может быть меньше самого сна: {:?}", stats.median);
    }

    #[test]
    fn median_resists_one_outlier_while_max_shows_it() {
        let calls = Cell::new(0usize);
        let stats = bench(1, 21, || {
            calls.set(calls.get() + 1);
            std::thread::sleep(if calls.get() == 12 { ms(80) } else { ms(2) });
        });
        assert!(stats.max >= ms(80), "выброс должен быть виден в max");
        assert!(stats.median < ms(30), "один выброс не должен двигать медиану: {:?}", stats.median);
        assert_eq!(stats.p99, stats.max, "на 21 замере p99 это самый большой");
        assert!(stats.min <= stats.median && stats.median <= stats.p99 && stats.p99 <= stats.max, "порядок min <= median <= p99 <= max");
    }

    #[test]
    fn monotonic_clock_only() {
        let code = code_only();
        assert!(code.contains("Instant::now"), "замер нужно делать через Instant");
        assert!(!code.contains("SystemTime"), "SystemTime может прыгнуть назад (синхронизация часов), для замеров он не годится");
    }
}
