use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

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
