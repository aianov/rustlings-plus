
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Сколько последних вызовов помнит breaker.
    pub window: usize,
    /// Пока вызовов в окне меньше этого числа, решение не принимается.
    pub min_calls: usize,
    /// Доля неудач в окне, с которой breaker открывается.
    pub failure_ratio: f64,
    /// Сколько breaker остаётся открытым, прежде чем пустить пробный вызов.
    pub open_for: Duration,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Open {
    pub retry_after: Duration,
}

struct Inner {
    state: State,
    window: VecDeque<bool>,
    until: Instant,
    probe_in_flight: bool,
}

pub struct Breaker {
    cfg: Config,
    inner: Mutex<Inner>,
}

pub struct Permit<'a> {
    breaker: &'a Breaker,
    probe: bool,
    done: bool,
}

impl Breaker {
    pub fn new(cfg: Config) -> Breaker {
        let inner = Inner { state: State::Closed, window: VecDeque::new(), until: Instant::now(), probe_in_flight: false };
        Breaker { cfg, inner: Mutex::new(inner) }
    }

    fn refresh(inner: &mut Inner) {
        if inner.state == State::Open && Instant::now() >= inner.until {
            inner.state = State::HalfOpen;
            inner.probe_in_flight = false;
        }
    }

    pub fn state(&self) -> State {
        let mut inner = self.inner.lock().unwrap();
        Breaker::refresh(&mut inner);
        inner.state
    }

    pub fn allow(&self) -> Result<Permit<'_>, Open> {
        let mut inner = self.inner.lock().unwrap();
        Breaker::refresh(&mut inner);
        match inner.state {
            State::Closed => Ok(Permit { breaker: self, probe: false, done: false }),
            State::Open => Err(Open { retry_after: inner.until.saturating_duration_since(Instant::now()) }),
            State::HalfOpen if inner.probe_in_flight => Err(Open { retry_after: Duration::ZERO }),
            State::HalfOpen => {
                inner.probe_in_flight = true;
                Ok(Permit { breaker: self, probe: true, done: false })
            }
        }
    }

    fn record(&self, probe: bool, ok: bool) {
        let mut inner = self.inner.lock().unwrap();
        if probe {
            inner.probe_in_flight = false;
            if ok {
                inner.state = State::Closed;
                inner.window.clear();
            } else {
                inner.state = State::Open;
                inner.until = Instant::now() + self.cfg.open_for;
            }
            return;
        }
        if inner.state != State::Closed {
            return;
        }
        inner.window.push_back(ok);
        while inner.window.len() > self.cfg.window {
            inner.window.pop_front();
        }
        let failures = inner.window.iter().filter(|ok| !**ok).count();
        if inner.window.len() >= self.cfg.min_calls && failures as f64 / inner.window.len() as f64 >= self.cfg.failure_ratio {
            inner.state = State::Open;
            inner.until = Instant::now() + self.cfg.open_for;
            inner.window.clear();
        }
    }
}

impl Permit<'_> {
    pub fn success(mut self) {
        self.done = true;
        self.breaker.record(self.probe, true);
    }

    pub fn failure(mut self) {
        self.done = true;
        self.breaker.record(self.probe, false);
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.breaker.record(self.probe, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn cfg() -> Config {
        Config { window: 10, min_calls: 5, failure_ratio: 0.5, open_for: MS(1000) }
    }

    fn fail(b: &Breaker) {
        b.allow().expect("breaker должен пускать").failure();
    }

    fn ok(b: &Breaker) {
        b.allow().expect("breaker должен пускать").success();
    }

    #[tokio::test(start_paused = true)]
    async fn a_healthy_dependency_keeps_it_closed() {
        let b = Breaker::new(cfg());
        for _ in 0..100 {
            ok(&b);
        }
        assert_eq!(b.state(), State::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn it_waits_for_enough_calls_before_deciding() {
        let b = Breaker::new(cfg());
        for _ in 0..4 {
            fail(&b);
        }
        assert_eq!(b.state(), State::Closed, "четыре вызова меньше min_calls = 5: решать рано, одна случайная серия не повод");
        fail(&b);
        assert_eq!(b.state(), State::Open, "пять вызовов, все неудачные: доля выше порога");
    }

    #[tokio::test(start_paused = true)]
    async fn the_threshold_is_a_ratio_not_a_streak() {
        let b = Breaker::new(cfg());
        ok(&b);
        ok(&b);
        fail(&b);
        fail(&b);
        assert_eq!(b.state(), State::Closed);
        fail(&b);
        assert_eq!(b.state(), State::Open, "3 неудачи из 5 это 60%, порог 50%");
    }

    #[tokio::test(start_paused = true)]
    async fn old_results_slide_out_of_the_window() {
        let cfg = Config { window: 10, min_calls: 10, failure_ratio: 0.5, open_for: MS(1000) };
        let b = Breaker::new(cfg);
        for _ in 0..4 {
            fail(&b);
        }
        for _ in 0..10 {
            ok(&b);
        }
        assert_eq!(b.state(), State::Closed, "четыре давние неудачи уже вытеснены десятью успехами");
        for _ in 0..4 {
            fail(&b);
        }
        assert_eq!(b.state(), State::Closed, "в окне 6 успехов и 4 неудачи, это 40%");
        fail(&b);
        assert_eq!(b.state(), State::Open, "в окне 5 успехов и 5 неудач, это 50%");
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_breaker_rejects_and_says_when_to_come_back() {
        let b = Breaker::new(cfg());
        for _ in 0..5 {
            fail(&b);
        }
        tokio::time::advance(MS(300)).await;
        let err = b.allow().err().expect("открытый breaker не пускает");
        assert_eq!(err.retry_after, MS(700), "осталось 700 мс из 1000");
        assert_eq!(b.state(), State::Open);
    }

    #[tokio::test(start_paused = true)]
    async fn after_the_pause_one_probe_decides() {
        let b = Breaker::new(cfg());
        for _ in 0..5 {
            fail(&b);
        }
        tokio::time::advance(MS(1000)).await;
        assert_eq!(b.state(), State::HalfOpen, "пауза прошла: можно пробовать");
        ok(&b);
        assert_eq!(b.state(), State::Closed, "успешная проба закрывает breaker");
        fail(&b);
        assert_eq!(b.state(), State::Closed, "окно очищено, одна неудача (меньше min_calls) его не открывает");
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_probe_opens_it_again_with_a_fresh_timer() {
        let b = Breaker::new(cfg());
        for _ in 0..5 {
            fail(&b);
        }
        tokio::time::advance(MS(1000)).await;
        fail(&b);
        assert_eq!(b.state(), State::Open, "проба не удалась");
        tokio::time::advance(MS(999)).await;
        assert_eq!(b.state(), State::Open, "пауза отсчитывается заново от неудачной пробы");
        tokio::time::advance(MS(1)).await;
        assert_eq!(b.state(), State::HalfOpen);
    }

    #[tokio::test(start_paused = true)]
    async fn a_permit_dropped_without_an_answer_counts_as_a_failure() {
        let b = Breaker::new(cfg());
        for _ in 0..5 {
            let permit = b.allow().unwrap();
            drop(permit);
        }
        assert_eq!(b.state(), State::Open, "вызов оборван без ответа (отмена, таймаут): для breaker это неудача");
    }
}
