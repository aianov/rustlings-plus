use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{Notify, OnceCell, Semaphore};

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum GateError {
    Connect,
}

pub struct Conn;

pub struct Backend {
    pub connects: AtomicUsize,
    pub fetches: AtomicUsize,
    pub running: AtomicUsize,
    pub peak: AtomicUsize,
    fail_first_connect: bool,
}

impl Backend {
    pub fn new(fail_first_connect: bool) -> Self {
        Backend {
            connects: AtomicUsize::new(0),
            fetches: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            fail_first_connect,
        }
    }

    pub async fn connect(&self) -> Result<Conn, GateError> {
        let attempt = self.connects.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        if self.fail_first_connect && attempt == 0 { Err(GateError::Connect) } else { Ok(Conn) }
    }

    pub async fn fetch(&self, _conn: &Conn, id: u32) -> Profile {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        self.running.fetch_sub(1, Ordering::SeqCst);
        Profile { id, name: format!("user{id}") }
    }
}

pub struct Gateway {
    backend: Arc<Backend>,
    permits: Semaphore,
    cache: RwLock<HashMap<u32, Profile>>,
    conn: OnceCell<Conn>,
    opened: AtomicBool,
    door: Notify,
}

impl Gateway {
    pub fn new(limit: usize, backend: Arc<Backend>) -> Self {
        Gateway {
            backend,
            permits: Semaphore::new(limit),
            cache: RwLock::new(HashMap::new()),
            conn: OnceCell::new(),
            opened: AtomicBool::new(false),
            door: Notify::new(),
        }
    }

    pub fn open(&self) {
        self.opened.store(true, Ordering::SeqCst);
        self.door.notify_waiters();
    }

    async fn wait_open(&self) {
        loop {
            let notified = self.door.notified();
            if self.opened.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    pub async fn profile(&self, id: u32) -> Result<Profile, GateError> {
        self.wait_open().await;
        let cached = self.cache.read().unwrap().get(&id).cloned();
        if let Some(profile) = cached {
            return Ok(profile);
        }
        let conn = self.conn.get_or_try_init(|| self.backend.connect()).await?;
        let profile = {
            let _permit = self.permits.acquire().await.unwrap();
            self.backend.fetch(conn, id).await
        };
        self.cache.write().unwrap().insert(id, profile.clone());
        Ok(profile)
    }
}

#[tokio::main]
async fn main() {
    let gateway = Gateway::new(2, Arc::new(Backend::new(false)));
    gateway.open();
    println!("{:?}", gateway.profile(1).await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, timeout};

    fn gateway(limit: usize, fail_first: bool) -> (Arc<Gateway>, Arc<Backend>) {
        let backend = Arc::new(Backend::new(fail_first));
        (Arc::new(Gateway::new(limit, Arc::clone(&backend))), backend)
    }

    #[tokio::test(start_paused = true)]
    async fn closed_gateway_waits_for_open() {
        let (gateway, _) = gateway(2, false);
        let call = {
            let gateway = Arc::clone(&gateway);
            tokio::spawn(async move { gateway.profile(1).await })
        };
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(!call.is_finished(), "пока шлюз закрыт, profile не отвечает");
        gateway.open();
        let profile = timeout(Duration::from_secs(1), call).await.expect("после open запрос не продолжился").unwrap().unwrap();
        assert_eq!(profile, Profile { id: 1, name: "user1".to_string() }, "профиль пришёл из бэкенда");
    }

    #[tokio::test(start_paused = true)]
    async fn open_wakes_everybody_even_if_called_early() {
        let (gateway, _) = gateway(5, false);
        gateway.open();
        assert!(
            timeout(Duration::from_secs(1), gateway.profile(1)).await.is_ok(),
            "open вызвали до запроса, ожидание не должно было начаться вовсе"
        );
        let (late, _) = self::gateway(5, false);
        let calls: Vec<_> = (0..5)
            .map(|id| {
                let late = Arc::clone(&late);
                tokio::spawn(async move { late.profile(id).await })
            })
            .collect();
        tokio::time::sleep(Duration::from_millis(10)).await;
        late.open();
        for call in calls {
            assert!(timeout(Duration::from_secs(1), call).await.is_ok(), "open должен разбудить всех ждущих, а не одного");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn second_request_comes_from_the_cache() {
        let (gateway, backend) = gateway(2, false);
        gateway.open();
        let first = gateway.profile(7).await.unwrap();
        let started = Instant::now();
        let second = gateway.profile(7).await.unwrap();
        assert_eq!(first, second, "тот же профиль");
        assert_eq!(started.elapsed(), Duration::ZERO, "из кэша ответ мгновенный");
        assert_eq!(backend.fetches.load(Ordering::SeqCst), 1, "в бэкенд сходили один раз");
    }

    #[tokio::test(start_paused = true)]
    async fn connection_is_created_once_and_failures_are_retried() {
        let (gateway, backend) = gateway(20, true);
        gateway.open();
        let calls: Vec<_> = (0..20)
            .map(|id| {
                let gateway = Arc::clone(&gateway);
                tokio::spawn(async move { gateway.profile(id).await })
            })
            .collect();
        let mut errors = 0;
        for call in calls {
            if call.await.unwrap().is_err() {
                errors += 1;
            }
        }
        assert_eq!(errors, 1, "первая попытка подключения упала и достаётся одному вызову, остальные должны дождаться второй попытки");
        assert_eq!(backend.connects.load(Ordering::SeqCst), 2, "две попытки подключения на всех: упавшая и удачная");
    }

    #[tokio::test(start_paused = true)]
    async fn backend_concurrency_is_limited() {
        let (gateway, backend) = gateway(4, false);
        gateway.open();
        let calls: Vec<_> = (0..40)
            .map(|id| {
                let gateway = Arc::clone(&gateway);
                tokio::spawn(async move { gateway.profile(id).await.unwrap() })
            })
            .collect();
        for (id, call) in calls.into_iter().enumerate() {
            assert_eq!(call.await.unwrap().id, id as u32, "каждый получает свой профиль");
        }
        assert_eq!(backend.peak.load(Ordering::SeqCst), 4, "в бэкенд одновременно должно ходить ровно limit запросов");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn stress_on_many_threads() {
        let (gateway, backend) = gateway(3, false);
        gateway.open();
        let calls: Vec<_> = (0..300u32)
            .map(|i| {
                let gateway = Arc::clone(&gateway);
                tokio::spawn(async move { gateway.profile(i % 25).await.unwrap() })
            })
            .collect();
        for (i, call) in calls.into_iter().enumerate() {
            let profile = timeout(Duration::from_secs(10), call).await.expect("запрос завис").unwrap();
            assert_eq!(profile.id, i as u32 % 25, "профиль соответствует запрошенному id");
        }
        assert!(backend.peak.load(Ordering::SeqCst) <= 3, "лимит нарушен под нагрузкой");
        assert_eq!(backend.connects.load(Ordering::SeqCst), 1, "подключение одно на всех");
        let fetches = backend.fetches.load(Ordering::SeqCst);
        assert!((25..=300).contains(&fetches), "число обращений в бэкенд {fetches} выходит за разумные границы");
    }
}
