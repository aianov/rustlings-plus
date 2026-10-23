use crate::backend::{Backend, Request, Response};
use crate::breaker::{self, Breaker};
use crate::deadline::{Deadline, DeadlineExceeded, run_with};
use crate::retry::{Attempt, Lcg, RetryBudget, Status, backoff, should_retry};
use crate::shed::Shedder;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Debug, PartialEq, Eq)]
pub enum CallError {
    /// Шлюз перегружен: запрос отвергнут на входе.
    Overloaded,
    /// У зависимости нет настроек.
    UnknownDep,
    /// У зависимости заняты все места (переборка).
    Full,
    /// Предохранитель зависимости открыт.
    CircuitOpen { retry_after: Duration },
    /// Общий срок запроса вышел.
    DeadlineExceeded,
    /// Зависимость ответила ошибкой, повторять нельзя или нечем.
    Failed(Status),
}

pub struct Config {
    /// Зависимость и сколько одновременных вызовов к ней разрешено.
    pub deps: Vec<(&'static str, usize)>,
    /// Сколько запросов шлюз ведёт одновременно (всего).
    pub max_in_flight: usize,
    /// Попыток на запрос, включая первую.
    pub attempts: u32,
    pub base: Duration,
    pub cap: Duration,
    pub breaker: breaker::Config,
    pub retry_ratio: f64,
    pub retry_reserve: u32,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            deps: vec![("user", 4), ("post", 4)],
            max_in_flight: 8,
            attempts: 3,
            base: Duration::from_millis(50),
            cap: Duration::from_secs(1),
            breaker: breaker::Config { window: 10, min_calls: 5, failure_ratio: 0.5, open_for: Duration::from_secs(5) },
            retry_ratio: 0.1,
            retry_reserve: 10,
            seed: 1,
        }
    }
}

pub struct Gateway<B> {
    backend: B,
    cfg: Config,
    shedder: Shedder,
    bulkheads: HashMap<&'static str, Semaphore>,
    breakers: HashMap<&'static str, Breaker>,
    budget: Mutex<RetryBudget>,
    rng: Mutex<Lcg>,
    keys: AtomicU64,
}

impl<B: Backend> Gateway<B> {
    pub fn new(backend: B, cfg: Config) -> Gateway<B> {
        let shedder = Shedder::new(cfg.max_in_flight, Duration::from_secs(1));
        let bulkheads = cfg.deps.iter().map(|(dep, n)| (*dep, Semaphore::new(*n))).collect();
        let breakers = cfg.deps.iter().map(|(dep, _)| (*dep, Breaker::new(cfg.breaker.clone()))).collect();
        let budget = Mutex::new(RetryBudget::new(cfg.retry_ratio, cfg.retry_reserve));
        let rng = Mutex::new(Lcg(cfg.seed));
        Gateway { backend, cfg, shedder, bulkheads, breakers, budget, rng, keys: AtomicU64::new(0) }
    }

    /// Вызов зависимости `dep` с защитой. Порядок защиты (сверху вниз): вход шлюза, переборка зависимости, предохранитель,
    /// срок, повторы. `budget` это общий срок запроса со всеми повторами. `idempotent` говорит, можно ли повторять
    /// после неоднозначных ошибок.
    pub async fn call(&self, dep: &'static str, req: Request, idempotent: bool, budget: Duration) -> Result<Response, CallError> {
        let _admit = self.shedder.try_admit().map_err(|_| CallError::Overloaded)?;
        let bulkhead = self.bulkheads.get(dep).ok_or(CallError::UnknownDep)?;
        let breaker = self.breakers.get(dep).ok_or(CallError::UnknownDep)?;
        let _place = bulkhead.try_acquire().map_err(|_| CallError::Full)?;
        let deadline = Deadline::after(budget);
        self.budget.lock().unwrap().on_request();
        let mut tried = 0u32;
        loop {
            let permit = breaker.allow().map_err(|open| CallError::CircuitOpen { retry_after: open.retry_after })?;
            let status = match run_with(deadline, self.backend.call(dep, req.clone(), deadline)).await {
                Ok(Ok(response)) => {
                    permit.success();
                    return Ok(response);
                }
                Ok(Err(status)) => status,
                Err(DeadlineExceeded) => {
                    permit.failure();
                    return Err(CallError::DeadlineExceeded);
                }
            };
            if status.0.blames_dependency() {
                permit.failure();
            } else {
                permit.success();
            }
            let pause = backoff(tried, self.cfg.base, self.cfg.cap, &mut *self.rng.lock().unwrap());
            tried += 1;
            let again = Attempt { code: status.0, idempotent, budget_left: deadline.remaining(), pause };
            if tried >= self.cfg.attempts || !should_retry(&again) || !self.budget.lock().unwrap().try_retry() {
                return Err(CallError::Failed(status));
            }
            tokio::time::sleep(pause).await;
        }
    }

    /// Создание поста. Повторять «создать» можно только с ключом идемпотентности: шлюз сам придумывает ключ,
    /// один на запрос (он общий у всех попыток этого запроса и разный у разных запросов), и отправляет его в зависимость `post`.
    pub async fn create_post(&self, user: u64, text: &str, budget: Duration) -> Result<Response, CallError> {
        let n = self.keys.fetch_add(1, Ordering::SeqCst);
        let req = Request { body: format!("{user}:{text}"), idempotency_key: Some(format!("gw-{user}-{n}")) };
        self.call("post", req, true, budget).await
    }
}
