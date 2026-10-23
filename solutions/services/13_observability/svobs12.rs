
use std::sync::Mutex;
use std::time::Duration;

fn main() {}


pub struct Histogram {
    name: String,
    help: String,
    bounds: Vec<f64>,
    state: Mutex<State>,
}

struct State {
    counts: Vec<u64>,
    sum: f64,
}

impl Histogram {
    pub fn new(name: &str, help: &str, bounds: &[f64]) -> Histogram {
        assert!(bounds.iter().all(|b| b.is_finite()), "границы бакетов должны быть конечными числами: {bounds:?}");
        assert!(bounds.windows(2).all(|w| w[0] < w[1]), "границы бакетов должны строго возрастать: {bounds:?}");
        Histogram { name: name.to_string(), help: help.to_string(), bounds: bounds.to_vec(), state: Mutex::new(State { counts: vec![0; bounds.len() + 1], sum: 0.0 }) }
    }

    pub fn observe(&self, value: f64) {
        if value.is_nan() {
            return;
        }
        let slot = self.bounds.partition_point(|b| *b < value);
        let mut state = self.state.lock().expect("гистограмма не отравлена");
        state.counts[slot] += 1;
        state.sum += value;
    }

    pub fn count(&self) -> u64 {
        self.state.lock().expect("гистограмма не отравлена").counts.iter().sum()
    }

    pub fn sum(&self) -> f64 {
        self.state.lock().expect("гистограмма не отравлена").sum
    }

    pub fn render(&self) -> String {
        let state = self.state.lock().expect("гистограмма не отравлена");
        let mut out = format!("# HELP {0} {1}\n# TYPE {0} histogram\n", self.name, self.help);
        let mut running = 0;
        for (bound, n) in self.bounds.iter().zip(&state.counts) {
            running += n;
            out.push_str(&format!("{}_bucket{{le=\"{}\"}} {}\n", self.name, bound, running));
        }
        running += state.counts[self.bounds.len()];
        out.push_str(&format!("{0}_bucket{{le=\"+Inf\"}} {1}\n{0}_sum {2}\n{0}_count {1}\n", self.name, running, state.sum));
        out
    }
}


const BOUNDS: [f64; 13] = [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0];

pub struct HttpMetrics {
    latency: Histogram,
}

impl HttpMetrics {
    pub fn new() -> HttpMetrics {
        HttpMetrics { latency: Histogram::new("http_request_duration_seconds", "Время ответа в секундах", &BOUNDS) }
    }

    pub fn record(&self, elapsed: Duration) {
        self.latency.observe(elapsed.as_secs_f64());
    }

    pub fn render(&self) -> String {
        self.latency.render()
    }
}

impl Default for HttpMetrics {
    fn default() -> Self {
        HttpMetrics::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buckets(text: &str) -> Vec<(f64, u64)> {
        text.lines()
            .filter(|l| l.contains("_bucket{le=\""))
            .map(|l| {
                let le = l.split("le=\"").nth(1).and_then(|r| r.split('"').next()).expect("у строки бакета есть le");
                let n = l.rsplit(' ').next().and_then(|n| n.parse().ok()).expect("в конце строки число");
                (if le == "+Inf" { f64::INFINITY } else { le.parse().expect("граница это число") }, n)
            })
            .collect()
    }

    fn quantile(q: f64, buckets: &[(f64, u64)]) -> f64 {
        let total = buckets.last().expect("есть +Inf").1 as f64;
        let rank = q * total;
        let mut prev_bound = 0.0;
        let mut prev_count = 0.0;
        for &(bound, count) in buckets {
            if count as f64 >= rank {
                if bound.is_infinite() {
                    return prev_bound;
                }
                let span = count as f64 - prev_count;
                return if span == 0.0 { bound } else { prev_bound + (bound - prev_bound) * (rank - prev_count) / span };
            }
            prev_bound = bound;
            prev_count = count as f64;
        }
        prev_bound
    }

    fn traffic() -> (HttpMetrics, Vec<f64>) {
        let metrics = HttpMetrics::new();
        let mut truth = Vec::new();
        let mut push = |ms: f64, times: usize| {
            for i in 0..times {
                let jitter = 1.0 + (i % 10) as f64 / 20.0;
                let d = Duration::from_secs_f64(ms * jitter / 1000.0);
                metrics.record(d);
                truth.push(d.as_secs_f64());
            }
        };
        push(0.4, 200);
        push(8.0, 450);
        push(30.0, 250);
        push(300.0, 70);
        push(3000.0, 30);
        truth.sort_by(f64::total_cmp);
        (metrics, truth)
    }

    fn exact(q: f64, sorted: &[f64]) -> f64 {
        sorted[((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1]
    }

    #[test]
    fn the_name_says_seconds() {
        let text = HttpMetrics::new().render();
        assert!(text.contains("# TYPE http_request_duration_seconds histogram"), "метрика времени называется *_seconds и измеряется в секундах: {text}");
    }

    #[test]
    fn the_number_of_buckets_is_modest_but_not_tiny() {
        let n = buckets(&HttpMetrics::new().render()).len();
        assert!((8..=20).contains(&n), "бакетов (с +Inf) {n}: меньше 8 не хватит на четыре порядка, больше 20 слишком дорого по рядам");
    }

    #[test]
    fn percentiles_estimated_from_buckets_are_close_to_the_truth() {
        let (metrics, truth) = traffic();
        let b = buckets(&metrics.render());
        for q in [0.5, 0.95, 0.99] {
            let (est, real) = (quantile(q, &b), exact(q, &truth));
            assert!(est >= real / 2.5 && est <= real * 2.5, "p{}: по бакетам {est:.4} с, на самом деле {real:.4} с. Или бакеты не покрывают такие значения, или значения не в тех единицах", (q * 100.0) as u32);
        }
    }

    #[test]
    fn the_slow_tail_is_not_hidden_in_the_last_bucket() {
        let (metrics, _) = traffic();
        let b = buckets(&metrics.render());
        let below_max = b[b.len() - 2].1;
        let total = b[b.len() - 1].1;
        assert_eq!(total, 1000);
        assert_eq!(below_max, total, "все запросы до трёх секунд должны попасть в конечные бакеты: за последней границей картины нет, перцентиль упирается в неё");
    }

    #[test]
    fn sub_millisecond_requests_are_told_apart_from_zero() {
        let metrics = HttpMetrics::new();
        for _ in 0..100 {
            metrics.record(Duration::from_micros(400));
        }
        let b = buckets(&metrics.render());
        assert_eq!(b[0].1, 100, "первый бакет (до {} с) должен вмещать запросы в 0.4 мс", b[0].0);
        let sum_line = metrics.render().lines().find(|l| l.contains("_sum")).map(str::to_string).expect("есть _sum");
        let sum: f64 = sum_line.rsplit(' ').next().and_then(|s| s.parse().ok()).expect("число");
        assert!((sum - 0.04).abs() < 1e-9, "сумма 100 запросов по 0.4 мс это 0.04 с, а не {sum}: дробные доли теряются при переводе в целые миллисекунды");
    }
}
