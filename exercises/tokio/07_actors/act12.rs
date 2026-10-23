// Соединения с базой дорогие, поэтому их делают заранее и одалживают: пул из `size` штук. Свободного
// нет: ждём, пока кто-то вернёт. Ждать вечно нельзя: у `get_timeout` есть срок. Одолженное возвращается
// само, когда `Lease` выбрасывают, как бы это ни случилось.
// TODO: Допиши `Pool` и `Lease`. Соединение (`Conn`) помнит, сколько раз его использовали, и это
// число переживает возврат в пул.
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

#[derive(Debug)]
struct Conn {
    id: u32,
    uses: u32,
}

struct Pool;

struct Lease;

impl Pool {
    fn new(size: usize) -> Pool {
        todo!()
    }

    async fn get(&self) -> Lease {
        todo!()
    }

    async fn get_timeout(&self, wait: Duration) -> Option<Lease> {
        todo!()
    }

    fn idle(&self) -> usize {
        todo!()
    }
}

impl Deref for Lease {
    type Target = Conn;

    fn deref(&self) -> &Conn {
        todo!()
    }
}

impl DerefMut for Lease {
    fn deref_mut(&mut self) -> &mut Conn {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let pool = Pool::new(2);
    let mut lease = pool.get().await;
    lease.uses += 1;
    println!("{:?}, свободных {}", *lease, pool.idle());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Instant, sleep};

    #[tokio::test(start_paused = true)]
    async fn connections_are_reused_with_their_state() {
        let pool = Pool::new(2);
        assert_eq!(pool.idle(), 2, "новый пул: оба соединения свободны");
        for _ in 0..10 {
            let mut lease = pool.get().await;
            assert!(lease.id < 2, "в пуле всего два соединения, id {} откуда-то взялся", lease.id);
            lease.uses += 1;
        }
        assert_eq!(pool.idle(), 2, "всё возвращено, свободны оба");
        let (a, b) = (pool.get().await, pool.get().await);
        assert_eq!(a.uses + b.uses, 10, "счётчик использований должен переживать возврат в пул");
        assert_eq!(pool.idle(), 0, "оба соединения на руках");
    }

    #[tokio::test(start_paused = true)]
    async fn a_waiter_wakes_when_a_lease_is_returned() {
        let pool = Arc::new(Pool::new(2));
        let first = pool.get().await;
        let second = pool.get().await;
        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move {
                let start = Instant::now();
                let lease = pool.get().await;
                (lease.id, start.elapsed())
            })
        };
        sleep(Duration::from_millis(30)).await;
        let freed = first.id;
        drop(first);
        let (id, waited) = waiter.await.unwrap();
        assert_eq!(id, freed, "ждавший должен получить именно освободившееся соединение");
        assert_eq!(waited, Duration::from_millis(30), "ждавший просыпается в момент возврата, не раньше и не позже");
        drop(second);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_gives_up_at_the_deadline() {
        let pool = Pool::new(1);
        let held = pool.get().await;
        let start = Instant::now();
        assert!(pool.get_timeout(Duration::from_millis(50)).await.is_none(), "соединений нет, через 50 мс ожидается None");
        assert_eq!(start.elapsed(), Duration::from_millis(50), "отказ ровно по сроку");
        drop(held);
        assert!(pool.get_timeout(Duration::from_millis(50)).await.is_some(), "соединение вернули, теперь оно доступно");
    }

    #[tokio::test(start_paused = true)]
    async fn abandoned_waiters_do_not_shrink_the_pool() {
        let pool = Pool::new(2);
        let a = pool.get().await;
        let b = pool.get().await;
        for _ in 0..10 {
            assert!(pool.get_timeout(Duration::from_millis(5)).await.is_none(), "пул пуст, ожидающий должен отказаться");
        }
        drop(a);
        drop(b);
        assert_eq!(pool.idle(), 2, "после возврата свободны оба соединения: отказавшиеся ожидающие ничего не унесли");
        let (_x, _y) = (pool.get().await, pool.get().await);
        assert!(pool.get_timeout(Duration::from_millis(5)).await.is_none(), "третьего соединения быть не должно");
    }
}
