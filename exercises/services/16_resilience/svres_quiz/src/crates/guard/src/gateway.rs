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
        let _ = (dep, req, idempotent, budget);
        todo!()
    }

    /// Создание поста. Повторять «создать» можно только с ключом идемпотентности: шлюз сам придумывает ключ,
    /// один на запрос (он общий у всех попыток этого запроса и разный у разных запросов), и отправляет его в зависимость `post`.
    pub async fn create_post(&self, user: u64, text: &str, budget: Duration) -> Result<Response, CallError> {
        let _ = (user, text, budget);
        todo!()
    }
}
