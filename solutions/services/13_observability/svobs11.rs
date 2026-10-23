
use std::sync::Mutex;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn latency() -> Histogram {
        Histogram::new("request_seconds", "Время ответа", &[0.1, 0.5, 1.0])
    }

    #[test]
    fn buckets_are_cumulative_and_end_with_inf() {
        let h = latency();
        for v in [0.05, 0.25, 0.25, 0.75, 2.0] {
            h.observe(v);
        }
        assert_eq!(
            h.render(),
            "# HELP request_seconds Время ответа\n# TYPE request_seconds histogram\n\
             request_seconds_bucket{le=\"0.1\"} 1\nrequest_seconds_bucket{le=\"0.5\"} 3\nrequest_seconds_bucket{le=\"1\"} 4\n\
             request_seconds_bucket{le=\"+Inf\"} 5\nrequest_seconds_sum 3.3\nrequest_seconds_count 5\n",
            "каждая строка le считает ВСЕ значения не больше границы, включая попавшие в меньшие бакеты"
        );
    }

    #[test]
    fn a_value_equal_to_a_bound_belongs_to_that_bucket() {
        let h = latency();
        h.observe(0.5);
        let text = h.render();
        assert!(text.contains("request_seconds_bucket{le=\"0.1\"} 0\n"), "{text}");
        assert!(text.contains("request_seconds_bucket{le=\"0.5\"} 1\n"), "le значит «меньше или равно»: значение 0.5 уже в бакете 0.5: {text}");
    }

    #[test]
    fn an_empty_histogram_renders_zeros() {
        let text = latency().render();
        assert!(text.contains("request_seconds_bucket{le=\"+Inf\"} 0\n"), "{text}");
        assert!(text.contains("request_seconds_sum 0\n") && text.contains("request_seconds_count 0\n"), "{text}");
    }

    #[test]
    fn count_and_sum_agree_with_the_inf_bucket() {
        let h = latency();
        for v in [0.5, 0.25, 4.0, 0.125] {
            h.observe(v);
        }
        assert_eq!(h.count(), 4);
        assert_eq!(h.sum(), 4.875);
        let text = h.render();
        assert!(text.contains("_bucket{le=\"+Inf\"} 4\n") && text.contains("_count 4\n"), "+Inf и count всегда равны: {text}");
    }

    #[test]
    fn nan_is_ignored_and_negative_values_land_in_the_first_bucket() {
        let h = latency();
        h.observe(f64::NAN);
        h.observe(-1.0);
        assert_eq!(h.count(), 1, "NaN не наблюдение: он испортил бы сумму навсегда");
        assert_eq!(h.sum(), -1.0);
        assert!(h.render().contains("_bucket{le=\"0.1\"} 1\n"), "отрицательное меньше первой границы и считается в ней");
    }

    #[test]
    fn huge_values_only_reach_inf() {
        let h = latency();
        h.observe(1e9);
        let text = h.render();
        assert!(text.contains("_bucket{le=\"1\"} 0\n") && text.contains("_bucket{le=\"+Inf\"} 1\n"), "{text}");
    }

    #[test]
    fn bounds_must_be_sorted_and_finite() {
        for bad in [&[1.0, 0.5][..], &[0.5, 0.5], &[f64::NAN], &[0.1, f64::INFINITY]] {
            let result = std::panic::catch_unwind(|| Histogram::new("x", "x", bad));
            assert!(result.is_err(), "границы {bad:?} недопустимы: бакеты должны строго возрастать, а +Inf добавляется сам");
        }
    }

    #[test]
    fn parallel_observations_are_all_counted() {
        let h = Arc::new(latency());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let h = h.clone();
                std::thread::spawn(move || {
                    for _ in 0..5_000 {
                        h.observe(0.25);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("поток не упал");
        }
        assert_eq!(h.count(), 40_000);
        assert_eq!(h.sum(), 10_000.0);
        assert!(h.render().contains("_bucket{le=\"0.5\"} 40000\n"));
    }
}
