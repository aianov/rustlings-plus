// Шлюз ждёт ответ две секунды и отвечает клиенту «таймаут». Выглядит правильно. Но сервисы ниже о таймауте шлюза ничего не знают:
// post-service честно отрабатывает все свои шаги ещё три секунды, user-service тоже, и всё это впустую, потому что ответ
// читать некому. Под нагрузкой такие «призраки» занимают воркеры и базу, пока живые клиенты ждут. Здесь «сеть» между сервисами
// устроена как настоящая: сервер работает в своей задаче и не замечает, что вызывающий ушёл.
// Цель: срок запроса (`Deadline` из `svres1`, он уже есть) путешествует по цепочке; после срока ни один шаг не стартует, а
// начатый обрывается в срок. Результат успевшего запроса не меняется.


use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

fn main() {}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    at: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeadlineExceeded;

impl Deadline {
    pub fn after(budget: Duration) -> Deadline {
        Deadline { at: Instant::now() + budget }
    }

    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }
}

pub async fn run_with<F: Future>(deadline: Deadline, work: F) -> Result<F::Output, DeadlineExceeded> {
    if deadline.expired() {
        return Err(DeadlineExceeded);
    }
    tokio::time::timeout_at(deadline.at, work).await.map_err(|_| DeadlineExceeded)
}


#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    DeadlineExceeded,
}

pub struct Request {
    pub user: u64,
    pub deadline: Option<Deadline>,
}

#[derive(Default)]
pub struct Stats {
    started: AtomicU32,
    finished: AtomicU32,
    aborted: AtomicU32,
    last_event: Mutex<Option<Instant>>,
}

impl Stats {
    pub fn started(&self) -> u32 {
        self.started.load(Ordering::SeqCst)
    }
    pub fn finished(&self) -> u32 {
        self.finished.load(Ordering::SeqCst)
    }
    pub fn aborted(&self) -> u32 {
        self.aborted.load(Ordering::SeqCst)
    }
    /// Момент, когда любой шаг работы закончился или был оборван.
    pub fn last_event(&self) -> Option<Instant> {
        *self.last_event.lock().unwrap()
    }
    fn touch(&self) {
        *self.last_event.lock().unwrap() = Some(Instant::now());
    }
}

struct StepGuard {
    stats: Arc<Stats>,
    done: bool,
}

impl Drop for StepGuard {
    fn drop(&mut self) {
        if !self.done {
            self.stats.aborted.fetch_add(1, Ordering::SeqCst);
            self.stats.touch();
        }
    }
}

/// Один шаг настоящей работы сервиса: запрос в базу, отрисовка, что угодно, что стоит времени.
async fn step(stats: &Arc<Stats>, cost: Duration) {
    stats.started.fetch_add(1, Ordering::SeqCst);
    let mut guard = StepGuard { stats: stats.clone(), done: false };
    tokio::time::sleep(cost).await;
    guard.done = true;
    stats.finished.fetch_add(1, Ordering::SeqCst);
    stats.touch();
}

/// «Сеть»: сервер крутится в своей задаче. Если вызывающий бросил ожидание, сервер об этом не узнает.
async fn call<T: Send + 'static>(server: impl Future<Output = T> + Send + 'static) -> T {
    tokio::spawn(server).await.expect("сервер не должен паниковать")
}

const STEP: Duration = Duration::from_millis(1500);

async fn user_service(stats: Arc<Stats>, req: Request) -> Result<String, Error> {
    step(&stats, STEP).await;
    Ok(format!("user-{}", req.user))
}

pub async fn post_service(stats: Arc<Stats>, req: Request) -> Result<String, Error> {
    let name = call(user_service(stats.clone(), Request { user: req.user, deadline: None })).await?;
    step(&stats, STEP).await;
    step(&stats, STEP).await;
    Ok(format!("{name}: 2 поста"))
}

pub async fn gateway(stats: &Arc<Stats>, user: u64, budget: Duration) -> Result<String, Error> {
    let req = Request { user, deadline: None };
    match tokio::time::timeout(budget, call(post_service(stats.clone(), req))).await {
        Ok(result) => result,
        Err(_) => Err(Error::DeadlineExceeded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[tokio::test(start_paused = true)]
    async fn the_client_gets_a_timeout_at_the_budget() {
        let stats = Arc::new(Stats::default());
        let start = Instant::now();
        let r = gateway(&stats, 1, MS(2000)).await;
        assert_eq!(r, Err(Error::DeadlineExceeded));
        assert_eq!(start.elapsed(), MS(2000), "клиент должен получить отказ ровно в срок");
    }

    #[tokio::test(start_paused = true)]
    async fn downstream_stops_working_when_the_client_is_gone() {
        let stats = Arc::new(Stats::default());
        let start = Instant::now();
        let _ = gateway(&stats, 1, MS(2000)).await;
        tokio::time::sleep(Duration::from_secs(20)).await;
        let last = stats.last_event().expect("хоть один шаг должен был начаться");
        assert!(
            last <= start + MS(2000),
            "сервисы продолжали работать ещё {:?} после ухода клиента: срок до них не дошёл или не остановил работу",
            last - (start + MS(2000))
        );
        assert_eq!(stats.finished(), 1, "в срок успевает один шаг (user-service), остальная работа не нужна");
        assert_eq!(stats.started(), 2, "второй шаг стартует в 1500 мс и обрывается в 2000, третий не стартует вообще");
        assert_eq!(stats.aborted(), 1, "начатый шаг должен быть оборван, а не дожит до конца");
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_in_time_works_as_before() {
        let stats = Arc::new(Stats::default());
        let r = gateway(&stats, 7, Duration::from_secs(10)).await;
        assert_eq!(r.as_deref(), Ok("user-7: 2 поста"));
        assert_eq!((stats.started(), stats.finished(), stats.aborted()), (3, 3, 0), "при достаточном бюджете работа идёт полностью");
    }

    #[tokio::test(start_paused = true)]
    async fn a_service_refuses_a_request_whose_deadline_has_passed() {
        let stats = Arc::new(Stats::default());
        let deadline = Deadline::after(MS(100));
        tokio::time::advance(MS(200)).await;
        let r = post_service(stats.clone(), Request { user: 3, deadline: Some(deadline) }).await;
        assert_eq!(r, Err(Error::DeadlineExceeded));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(stats.started(), 0, "запрос пришёл с уже вышедшим сроком, а сервис всё равно начал работу");
    }

    #[tokio::test(start_paused = true)]
    async fn the_deadline_reaches_the_second_level() {
        let stats = Arc::new(Stats::default());
        let deadline = Deadline::after(MS(1000));
        let r = post_service(stats.clone(), Request { user: 3, deadline: Some(deadline) }).await;
        assert_eq!(r, Err(Error::DeadlineExceeded));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(stats.finished(), 0, "user-service получил бюджет в секунду, а шаг стоит 1500 мс: он не должен дойти до конца");
        assert_eq!(stats.started(), 1, "после обрыва первого шага другие не начинаются");
    }
}
