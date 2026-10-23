use std::collections::HashSet;
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    NotFound,
    Unavailable,
}

pub trait UserApi: Send + Sync {
    fn get(&self, id: u64) -> impl Future<Output = Result<String, ApiError>> + Send;
}

#[derive(Default)]
pub struct Real {
    pub hits: AtomicUsize,
}

impl UserApi for Real {
    async fn get(&self, id: u64) -> Result<String, ApiError> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        if id == 0 { Err(ApiError::NotFound) } else { Ok(format!("user{id}")) }
    }
}

pub async fn fetch_with_retry<A: UserApi>(api: &A, id: u64, attempts: usize) -> Result<String, ApiError> {
    let mut last = ApiError::Unavailable;
    for _ in 0..attempts {
        match api.get(id).await {
            Ok(user) => return Ok(user),
            Err(ApiError::NotFound) => return Err(ApiError::NotFound),
            Err(e) => last = e,
        }
    }
    Err(last)
}

pub struct Flaky<T> {
    inner: T,
    calls: AtomicUsize,
    doomed: Mutex<HashSet<usize>>,
    down: AtomicBool,
    latency: Mutex<Duration>,
}

impl<T: UserApi> Flaky<T> {
    pub fn new(inner: T) -> Flaky<T> {
        Flaky { inner, calls: AtomicUsize::new(0), doomed: Mutex::new(HashSet::new()), down: AtomicBool::new(false), latency: Mutex::new(Duration::ZERO) }
    }

    pub fn fail_on_call(&self, n: usize) {
        self.doomed.lock().unwrap().insert(n);
    }

    pub fn fail_all(&self) {
        self.down.store(true, Ordering::SeqCst);
    }

    pub fn heal(&self) {
        self.down.store(false, Ordering::SeqCst);
    }

    pub fn set_latency(&self, latency: Duration) {
        *self.latency.lock().unwrap() = latency;
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl<T: UserApi> UserApi for Flaky<T> {
    async fn get(&self, id: u64) -> Result<String, ApiError> {
        let number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let latency = *self.latency.lock().unwrap();
        if !latency.is_zero() {
            tokio::time::sleep(latency).await;
        }
        if self.down.load(Ordering::SeqCst) || self.doomed.lock().unwrap().remove(&number) {
            return Err(ApiError::Unavailable);
        }
        self.inner.get(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test]
    async fn without_faults_it_is_transparent() {
        let api = Flaky::new(Real::default());
        assert_eq!(api.get(5).await, Ok("user5".to_string()), "обёртка отдаёт то, что отвечает настоящий сервис");
        assert_eq!(api.get(0).await, Err(ApiError::NotFound), "настоящие ошибки проходят как есть");
        assert_eq!(api.calls(), 2, "оба вызова посчитаны");
    }

    #[tokio::test]
    async fn exactly_the_chosen_call_fails_once() {
        let api = Flaky::new(Real::default());
        api.fail_on_call(3);
        let mut outcomes = Vec::new();
        for _ in 0..5 {
            outcomes.push(api.get(1).await.is_ok());
        }
        assert_eq!(outcomes, vec![true, true, false, true, true], "падает третий вызов (счёт с единицы), остальные нет");
        assert_eq!(api.calls(), 5, "упавший вызов тоже в счёте");
        assert_eq!(api.inner.hits.load(Ordering::SeqCst), 4, "до настоящего сервиса упавший вызов не дошёл");
    }

    #[tokio::test]
    async fn several_planned_failures_are_independent() {
        let api = Flaky::new(Real::default());
        api.fail_on_call(1);
        api.fail_on_call(4);
        let mut outcomes = Vec::new();
        for _ in 0..5 {
            outcomes.push(api.get(1).await.is_ok());
        }
        assert_eq!(outcomes, vec![false, true, true, false, true], "первый и четвёртый");
    }

    #[tokio::test]
    async fn fail_all_lasts_until_heal() {
        let api = Flaky::new(Real::default());
        api.fail_all();
        assert_eq!(api.get(1).await, Err(ApiError::Unavailable), "сервис лежит");
        assert_eq!(api.get(1).await, Err(ApiError::Unavailable), "и продолжает лежать");
        api.heal();
        assert_eq!(api.get(1).await, Ok("user1".to_string()), "после heal снова работает");
        assert_eq!(api.calls(), 3, "счёт идёт и во время поломки");
    }

    #[tokio::test(start_paused = true)]
    async fn latency_slows_every_call_even_failing_ones() {
        let api = Flaky::new(Real::default());
        api.set_latency(Duration::from_millis(200));
        let began = Instant::now();
        api.get(1).await.unwrap();
        assert_eq!(began.elapsed(), Duration::from_millis(200), "вызов длится столько, сколько задано");
        api.fail_on_call(2);
        let before = Instant::now();
        assert!(api.get(1).await.is_err(), "второй вызов падает");
        assert_eq!(before.elapsed(), Duration::from_millis(200), "ошибка приходит с той же задержкой: настоящая поломка тоже обнаруживается не сразу");
        api.set_latency(Duration::ZERO);
        let again = Instant::now();
        api.get(1).await.unwrap();
        assert_eq!(again.elapsed(), Duration::ZERO, "задержку можно снять");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn under_concurrency_exactly_one_call_fails() {
        let api = Arc::new(Flaky::new(Real::default()));
        api.fail_on_call(5);
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let api = api.clone();
                tokio::spawn(async move { api.get(1).await.is_err() })
            })
            .collect();
        let mut failed = 0;
        for task in tasks {
            if task.await.unwrap() {
                failed += 1;
            }
        }
        assert_eq!(failed, 1, "из восьми одновременных вызовов падает ровно пятый по счёту, а не ни один и не несколько");
        assert_eq!(api.calls(), 8, "каждый вызов получил свой номер");
    }

    #[tokio::test]
    async fn the_retrying_client_survives_two_failures_but_not_three() {
        let api = Flaky::new(Real::default());
        api.fail_on_call(1);
        api.fail_on_call(2);
        assert_eq!(fetch_with_retry(&api, 9, 3).await, Ok("user9".to_string()), "две неудачи подряд переживаются тремя попытками");
        assert_eq!(api.calls(), 3, "потрачено три вызова");
        let api = Flaky::new(Real::default());
        api.fail_all();
        assert_eq!(fetch_with_retry(&api, 9, 3).await, Err(ApiError::Unavailable), "сервис лежит: после трёх попыток сдаёмся");
        assert_eq!(api.calls(), 3, "ровно три, не больше");
        let api = Flaky::new(Real::default());
        assert_eq!(fetch_with_retry(&api, 0, 5).await, Err(ApiError::NotFound), "«нет такого» повторять бессмысленно");
        assert_eq!(api.calls(), 1, "и клиент не повторяет");
    }
}
