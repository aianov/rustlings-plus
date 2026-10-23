// Ночная чистка идёт четыре минуты, а аренда живёт тридцать секунд. Экземпляр взял аренду, начал чистить и ни
// разу её не продлил: на тридцать первой секунде запись истекла, второй экземпляр взял «свободную» аренду, и теперь
// двое чистят одно и то же. Цель: `run_exclusive` держит аренду всё время работы (продляет её чаще, чем она
// истекает), а если продление не удалось, немедленно останавливает работу и сообщает об этом ошибкой.

use std::future::Future;
use std::time::Duration;

fn main() {}

use async_nats::jetstream::kv::{CreateErrorKind, Store};

pub struct Leases {
    kv: Store,
}

#[derive(Debug, Clone)]
pub struct Lease {
    pub name: String,
    pub owner: String,
    pub term: u64,
    revision: u64,
}

impl Leases {
    pub fn new(kv: Store) -> Leases {
        Leases { kv }
    }

    pub async fn acquire(&self, name: &str, owner: &str) -> Result<Option<Lease>, String> {
        match self.kv.create(name, owner.to_string().into()).await {
            Ok(revision) => Ok(Some(Lease { name: name.to_string(), owner: owner.to_string(), term: revision, revision })),
            Err(e) if e.kind() == CreateErrorKind::AlreadyExists => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub async fn renew(&self, lease: &mut Lease) -> Result<bool, String> {
        match self.kv.update(&lease.name, lease.owner.clone().into(), lease.revision).await {
            Ok(revision) => {
                lease.revision = revision;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    pub async fn release(&self, lease: &Lease) -> Result<(), String> {
        let _ = self.kv.delete_expect_revision(&lease.name, Some(lease.revision)).await;
        Ok(())
    }
}

pub async fn run_exclusive<F, Fut, T>(leases: &Leases, name: &str, owner: &str, ttl: Duration, work: F) -> Result<Option<T>, String>
where
    F: FnOnce(u64) -> Fut,
    Fut: Future<Output = T>,
{
    let _ = ttl;
    let Some(lease) = leases.acquire(name, owner).await? else { return Ok(None) };
    let output = work(lease.term).await;
    leases.release(&lease).await?;
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::kv::{Config, Store};
    use course_support::nats::{Scope, connect, js};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    const TTL: Duration = Duration::from_millis(1000);

    async fn setup(scope: &Scope) -> (Arc<Leases>, Store) {
        let js = js(&connect().await);
        let bucket = scope.name("leases");
        scope.track_bucket(&bucket);
        let kv = js.create_key_value(Config { bucket, history: 1, max_age: TTL, ..Default::default() }).await.expect("bucket");
        (Arc::new(Leases::new(kv.clone())), kv)
    }

    struct Probe {
        done: Arc<AtomicBool>,
        dropped: Arc<AtomicBool>,
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn work_longer_than_the_ttl_keeps_the_lease() {
        let scope = Scope::new();
        let (leases, _kv) = setup(&scope).await;
        let rival_won = Arc::new(AtomicBool::new(false));
        let rival = {
            let (leases, rival_won) = (Arc::clone(&leases), Arc::clone(&rival_won));
            tokio::spawn(async move {
                for _ in 0..33 {
                    if leases.acquire("cleanup", "auth-2").await.unwrap().is_some() {
                        rival_won.store(true, Ordering::SeqCst);
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
        };
        let result = run_exclusive(&leases, "cleanup", "auth-1", TTL, |_| async {
            tokio::time::sleep(Duration::from_millis(4000)).await;
            42
        })
        .await;
        rival.await.unwrap();
        assert_eq!(result, Ok(Some(42)), "работа дошла до конца");
        assert!(!rival_won.load(Ordering::SeqCst), "второй экземпляр перехватил аренду посреди работы: аренду нужно продлевать, пока работа идёт");
    }

    #[tokio::test]
    async fn the_lease_is_released_when_the_work_ends() {
        let scope = Scope::new();
        let (leases, _kv) = setup(&scope).await;
        run_exclusive(&leases, "cleanup", "auth-1", TTL, |_| async {}).await.expect("run").expect("аренда свободна");
        assert!(leases.acquire("cleanup", "auth-2").await.unwrap().is_some(), "после работы аренда свободна сразу, ждать истечения не нужно");
    }

    #[tokio::test]
    async fn a_busy_lease_means_no_work_at_all() {
        let scope = Scope::new();
        let (leases, _kv) = setup(&scope).await;
        let _held = leases.acquire("cleanup", "auth-2").await.unwrap().unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let result = run_exclusive(&leases, "cleanup", "auth-1", TTL, |_| async move {
            flag.store(true, Ordering::SeqCst);
        })
        .await;
        assert_eq!(result, Ok(None), "аренда занята: работы нет, это не ошибка");
        assert!(!called.load(Ordering::SeqCst), "работа не должна была начаться");
    }

    #[tokio::test]
    async fn a_lost_lease_stops_the_work_promptly() {
        let scope = Scope::new();
        let (leases, kv) = setup(&scope).await;
        let (done, dropped) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let probe = Probe { done: Arc::clone(&done), dropped: Arc::clone(&dropped) };
        let stealer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            kv.delete("cleanup").await.expect("имитация: аренду забрали");
        });
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(4),
            run_exclusive(&leases, "cleanup", "auth-1", TTL, |_| async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                probe.done.store(true, Ordering::SeqCst);
            }),
        )
        .await
        .expect("run_exclusive не должен висеть, когда аренда потеряна");
        stealer.await.unwrap();
        assert!(result.is_err(), "потеря аренды это ошибка, а не тихое завершение: {result:?}");
        assert!(started.elapsed() < Duration::from_secs(3), "остановились за {:?}: слишком долго, продление должно идти чаще срока", started.elapsed());
        assert!(dropped.load(Ordering::SeqCst), "работа должна быть остановлена, а не оставлена доделывать");
        assert!(!done.load(Ordering::SeqCst), "и не дошла до конца");
    }

    #[tokio::test]
    async fn the_work_gets_the_lease_term() {
        let scope = Scope::new();
        let (leases, _kv) = setup(&scope).await;
        let term = run_exclusive(&leases, "cleanup", "auth-1", TTL, |term| async move { term }).await.unwrap().unwrap();
        assert!(term > 0, "работа получает номер аренды, чтобы подписывать им свои записи");
    }
}
