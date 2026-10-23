// Предохранитель стоит перед user-service, и в целом работает: открывается, ждёт, пускает пробу. Но когда сервис оживает после
// аварии, он тут же падает снова. Разбор: в полуоткрытом состоянии breaker пускает не один пробный вызов, а всех подряд, пока не
// придёт первый ответ. Только что поднявшийся сервис получает толпу и ложится. Есть и вторая беда, менее заметная: запоздавший
// ответ старого вызова может закрыть открытый breaker, хотя ничего не доказал.
// Цель: в полуоткрытом состоянии идёт ровно одна проба, остальные получают отказ; результат считается только от нужного вызова.


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
            State::HalfOpen => Ok(Permit { breaker: self, probe: false, done: false }),
        }
    }

    fn record(&self, _probe: bool, ok: bool) {
        let mut inner = self.inner.lock().unwrap();
        match inner.state {
            State::Open | State::HalfOpen => {
                if ok {
                    inner.state = State::Closed;
                    inner.window.clear();
                } else {
                    inner.state = State::Open;
                    inner.until = Instant::now() + self.cfg.open_for;
                }
                return;
            }
            State::Closed => {}
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

    fn trip(b: &Breaker) {
        for _ in 0..5 {
            fail(b);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn half_open_lets_only_one_probe_through() {
        let b = Breaker::new(cfg());
        trip(&b);
        tokio::time::advance(MS(1000)).await;
        let probe = b.allow().expect("первый вызов после паузы идёт как проба");
        let admitted = (0..20).filter(|_| b.allow().map(|p| p.success()).is_ok()).count();
        assert_eq!(admitted, 0, "пока проба в пути, пропущено ещё {admitted} вызовов: восстанавливающийся сервис снова завалят толпой");
        probe.success();
        assert_eq!(b.state(), State::Closed);
        assert!((0..20).all(|_| b.allow().map(|p| p.success()).is_ok()), "после успешной пробы пускаем всех");
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_probe_closes_the_door_again() {
        let b = Breaker::new(cfg());
        trip(&b);
        tokio::time::advance(MS(1000)).await;
        let probe = b.allow().unwrap();
        probe.failure();
        assert_eq!(b.state(), State::Open);
        assert!(b.allow().is_err());
        tokio::time::advance(MS(1000)).await;
        assert!(b.allow().is_ok(), "после новой паузы новая проба");
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_probe_does_not_wedge_the_breaker() {
        let b = Breaker::new(cfg());
        trip(&b);
        tokio::time::advance(MS(1000)).await;
        drop(b.allow().unwrap());
        assert_eq!(b.state(), State::Open, "проба оборвана без ответа: считаем неудачей");
        tokio::time::advance(MS(1000)).await;
        assert!(b.allow().is_ok(), "после паузы снова можно пробовать: breaker не должен застрять в полуоткрытом состоянии навсегда");
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_answer_from_before_the_trip_changes_nothing() {
        let b = Breaker::new(cfg());
        let slow = b.allow().unwrap();
        trip(&b);
        assert_eq!(b.state(), State::Open);
        slow.success();
        assert_eq!(b.state(), State::Open, "запоздавший успех старого вызова не доказывает, что зависимость выздоровела");
        assert!(b.allow().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_failure_does_not_extend_the_pause() {
        let b = Breaker::new(cfg());
        let slow = b.allow().unwrap();
        trip(&b);
        tokio::time::advance(MS(600)).await;
        slow.failure();
        tokio::time::advance(MS(400)).await;
        assert_eq!(b.state(), State::HalfOpen, "пауза идёт от открытия, а не от запоздавших ответов");
    }
}
