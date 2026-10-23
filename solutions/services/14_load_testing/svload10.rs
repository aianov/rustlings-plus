
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Ok,
    Error,
    Timeout,
}

#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub kind: Kind,
    pub latency: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub total: u64,
    pub ok: u64,
    pub errors: u64,
    pub error_share: f64,
    pub rps: f64,
    pub p50: Duration,
    pub p99: Duration,
}


pub fn percentile(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (q.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}


pub fn build_report(outcomes: &[Outcome], elapsed: Duration) -> Report {
    let mut latencies: Vec<Duration> = outcomes.iter().filter(|o| o.kind == Kind::Ok).map(|o| o.latency).collect();
    latencies.sort();
    let total = outcomes.len() as u64;
    let ok = latencies.len() as u64;
    Report {
        total,
        ok,
        errors: total - ok,
        error_share: if total == 0 { 0.0 } else { (total - ok) as f64 / total as f64 },
        rps: if elapsed.is_zero() { 0.0 } else { ok as f64 / elapsed.as_secs_f64() },
        p50: percentile(&latencies, 0.5),
        p99: percentile(&latencies, 0.99),
    }
}

#[derive(Debug, Clone)]
pub struct Step {
    pub offered_rps: u32,
    pub report: Report,
}

pub fn max_sustainable(steps: &[Step], max_p99: Duration, max_error_share: f64) -> Option<u32> {
    let mut best = None;
    for step in steps {
        if step.report.p99 > max_p99 || step.report.error_share > max_error_share {
            break;
        }
        best = Some(step.offered_rps);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn outcomes(ok: usize, ok_latency: Duration, bad: usize, bad_latency: Duration, kind: Kind) -> Vec<Outcome> {
        let mut v = vec![Outcome { kind: Kind::Ok, latency: ok_latency }; ok];
        v.extend(vec![Outcome { kind, latency: bad_latency }; bad]);
        v
    }

    #[test]
    fn fast_failures_do_not_make_the_service_look_fast_or_busy() {
        let report = build_report(&outcomes(500, MS(20), 500, MS(1), Kind::Error), Duration::from_secs(1));
        assert_eq!((report.total, report.ok, report.errors), (1000, 500, 500));
        assert_eq!(report.rps, 500.0, "скорость это успешные ответы в секунду, а не все подряд");
        assert_eq!(report.p50, MS(20), "быстрые отказы не должны тянуть медиану вниз");
        assert_eq!(report.error_share, 0.5);
    }

    #[test]
    fn timeouts_are_errors_with_their_own_latency_left_out_of_the_percentiles() {
        let report = build_report(&outcomes(990, MS(20), 10, Duration::from_secs(5), Kind::Timeout), Duration::from_secs(1));
        assert_eq!(report.errors, 10);
        assert_eq!(report.p99, MS(20), "по успешным ответам p99 равен 20 мс; таймауты видны в доле ошибок");
        assert!(report.error_share > 0.0099 && report.error_share < 0.0101);
    }

    #[test]
    fn an_empty_run_is_zeros() {
        let report = build_report(&[], Duration::from_secs(1));
        assert_eq!((report.total, report.rps, report.p99, report.error_share), (0, 0.0, Duration::ZERO, 0.0));
        let broken = build_report(&outcomes(0, MS(1), 5, MS(1), Kind::Error), Duration::from_secs(1));
        assert_eq!((broken.ok, broken.rps, broken.p99, broken.error_share), (0, 0.0, Duration::ZERO, 1.0), "только ошибки: успешных нет, скорость нулевая");
        assert_eq!(build_report(&outcomes(5, MS(1), 0, MS(1), Kind::Ok), Duration::ZERO).rps, 0.0, "нулевое время не делит на ноль");
    }

    fn step(offered: u32, p99: u64, errors_per_100: usize) -> Step {
        let ok = 100 - errors_per_100;
        Step { offered_rps: offered, report: build_report(&outcomes(ok, MS(p99), errors_per_100, MS(1), Kind::Error), Duration::from_secs(1)) }
    }

    #[test]
    fn the_limit_is_the_last_step_before_the_first_failure() {
        let steps = [step(100, 20, 0), step(200, 25, 0), step(400, 80, 0), step(800, 400, 20), step(1600, 30, 0)];
        assert_eq!(max_sustainable(&steps, MS(100), 0.01), Some(400), "на 800 ошибки и медленно: дальше не идём, даже если 1600 случайно выглядит хорошо");
    }

    #[test]
    fn the_error_share_alone_can_break_a_step() {
        let steps = [step(100, 20, 0), step(200, 20, 5)];
        assert_eq!(max_sustainable(&steps, MS(100), 0.01), Some(100), "задержки в норме, но 5% ошибок");
        assert_eq!(max_sustainable(&steps, MS(100), 0.05), Some(200), "предел включительно: ровно 5% ещё допустимо");
    }

    #[test]
    fn nothing_sustainable_and_everything_sustainable() {
        assert_eq!(max_sustainable(&[], MS(100), 0.01), None);
        assert_eq!(max_sustainable(&[step(100, 500, 0)], MS(100), 0.01), None, "уже первая ступень медленная");
        assert_eq!(max_sustainable(&[step(100, 20, 0), step(200, 30, 0)], MS(100), 0.01), Some(200), "все ступени в норме: берём последнюю");
    }
}
