// Чистку сессий должен делать один экземпляр auth-service из трёх, иначе три одинаковых запроса бьют в базу.
// Аренда это ключ в KV: кто создал, тот и работает. Бакет создан с `max_age` в секунду, поэтому запись исчезает
// сама, если владелец замолчал. Цель: `acquire` берёт аренду, только если она свободна, и возвращает её номер
// (он растёт при каждой новой выдаче); `renew` продлевает аренду владельцу и сообщает, что аренда потеряна,
// если её уже перехватили; `release` отдаёт аренду, но не трогает чужую.

use async_nats::jetstream::kv::{CreateErrorKind, Store};

fn main() {}

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
        let _ = (name, owner, &self.kv, CreateErrorKind::AlreadyExists);
        todo!()
    }

    pub async fn renew(&self, lease: &mut Lease) -> Result<bool, String> {
        let _ = (&lease.name, &lease.owner, lease.revision);
        todo!()
    }

    pub async fn release(&self, lease: &Lease) -> Result<(), String> {
        let _ = lease;
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::kv::Config;
    use course_support::nats::{Scope, connect, js, wait_until};
    use std::sync::Arc;
    use std::time::Duration;

    const TTL: Duration = Duration::from_millis(1000);

    async fn setup(scope: &Scope) -> Arc<Leases> {
        let js = js(&connect().await);
        let bucket = scope.name("leases");
        scope.track_bucket(&bucket);
        let kv = js.create_key_value(Config { bucket, history: 1, max_age: TTL, ..Default::default() }).await.expect("bucket");
        Arc::new(Leases::new(kv))
    }

    #[tokio::test]
    async fn only_one_owner_holds_the_lease_at_a_time() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let a = leases.acquire("cleanup", "auth-1").await.expect("acquire").expect("свободная аренда достаётся первому");
        assert!(a.term > 0, "у аренды есть номер");
        assert!(leases.acquire("cleanup", "auth-2").await.expect("acquire").is_none(), "пока аренда у auth-1, второй её не получит");
        assert!(leases.acquire("digest", "auth-2").await.expect("acquire").is_some(), "другая работа это другая аренда");
    }

    #[tokio::test]
    async fn renewing_keeps_the_lease_past_the_ttl() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let mut a = leases.acquire("cleanup", "auth-1").await.unwrap().unwrap();
        for round in 1..=8 {
            tokio::time::sleep(Duration::from_millis(400)).await;
            assert!(leases.renew(&mut a).await.expect("renew"), "круг {round}: владелец живой, продление должно удаваться");
            assert!(leases.acquire("cleanup", "auth-2").await.unwrap().is_none(), "круг {round}: аренда продлена, перехватить её нельзя");
        }
    }

    #[tokio::test]
    async fn a_silent_owner_loses_the_lease_and_the_new_term_is_higher() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let mut a = leases.acquire("cleanup", "auth-1").await.unwrap().unwrap();
        let taken = Arc::new(std::sync::Mutex::new(None));
        wait_until("аренда истекла и её взял второй экземпляр", Duration::from_secs(6), || {
            let (leases, taken) = (Arc::clone(&leases), Arc::clone(&taken));
            async move {
                let got = leases.acquire("cleanup", "auth-2").await.unwrap();
                let some = got.is_some();
                if some {
                    *taken.lock().unwrap() = got;
                }
                some
            }
        })
        .await;
        let b = taken.lock().unwrap().clone().expect("аренда второго");
        assert!(b.term > a.term, "номер новой выдачи ({}) больше прежнего ({})", b.term, a.term);
        assert!(!leases.renew(&mut a).await.expect("renew"), "проснувшийся прежний владелец не должен продлевать чужую аренду");
        assert!(leases.acquire("cleanup", "auth-3").await.unwrap().is_none(), "и аренда второго осталась на месте");
    }

    #[tokio::test]
    async fn released_lease_is_free_at_once() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let a = leases.acquire("cleanup", "auth-1").await.unwrap().unwrap();
        leases.release(&a).await.expect("release");
        let b = leases.acquire("cleanup", "auth-2").await.expect("acquire").expect("после release аренда свободна сразу, ждать срок не нужно");
        assert!(b.term > a.term, "новый номер больше");
    }

    #[tokio::test]
    async fn a_late_release_does_not_take_the_lease_from_the_new_owner() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let a = leases.acquire("cleanup", "auth-1").await.unwrap().unwrap();
        tokio::time::sleep(Duration::from_millis(3200)).await;
        let b = leases.acquire("cleanup", "auth-2").await.unwrap().expect("аренда истекла, второй её взял");
        leases.release(&a).await.expect("release прежнего");
        assert!(leases.acquire("cleanup", "auth-3").await.unwrap().is_none(), "прежний владелец отдал не свою аренду: аренда второго (номер {}) должна остаться", b.term);
    }

    #[tokio::test]
    async fn of_eight_simultaneous_candidates_exactly_one_wins() {
        let scope = Scope::new();
        let leases = setup(&scope).await;
        let mut tasks = Vec::new();
        for i in 0..8 {
            let leases = Arc::clone(&leases);
            tasks.push(tokio::spawn(async move { leases.acquire("cleanup", &format!("auth-{i}")).await.unwrap().is_some() }));
        }
        let mut winners = 0;
        for task in tasks {
            winners += usize::from(task.await.unwrap());
        }
        assert_eq!(winners, 1, "победителей {winners}: проверка «свободно ли» и занятие должны быть одной атомарной операцией");
    }
}
