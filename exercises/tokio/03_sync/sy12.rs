// Подключение к хранилищу создаётся лениво, при первом обращении, и должно быть одним на всех,
// даже если в первый момент обращаются двадцать задач разом. Подключаться дорого (20 мс), и
// двадцать подключений вместо одного это двадцать лишних соединений.
// TODO: Допиши `get`: подключение создаётся ровно один раз, остальные ждут и берут готовое.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;

struct Conn {
    id: usize,
}

struct Pool {
    cell: OnceCell<Conn>,
    connects: AtomicUsize,
}

impl Pool {
    fn new() -> Self {
        Pool { cell: OnceCell::new(), connects: AtomicUsize::new(0) }
    }

    async fn connect(&self) -> Conn {
        let id = self.connects.fetch_add(1, Ordering::SeqCst) + 1;
        tokio::time::sleep(Duration::from_millis(20)).await;
        Conn { id }
    }
}

impl Pool {
    async fn get(&self) -> &Conn {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let pool = Pool::new();
    println!("{}", pool.get().await.id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn twenty_callers_one_connection() {
        let pool = Arc::new(Pool::new());
        let started = Instant::now();
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let pool = Arc::clone(&pool);
                tokio::spawn(async move { pool.get().await.id })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.await.unwrap(), 1, "все получают одно и то же подключение");
        }
        assert_eq!(pool.connects.load(Ordering::SeqCst), 1, "подключение должно создаваться один раз");
        assert_eq!(started.elapsed(), Duration::from_millis(20), "остальные ждут единственное подключение, а не делают свои");
    }

    #[tokio::test(start_paused = true)]
    async fn later_calls_are_instant() {
        let pool = Pool::new();
        pool.get().await;
        let started = Instant::now();
        pool.get().await;
        assert_eq!(started.elapsed(), Duration::ZERO, "готовое подключение отдаётся сразу");
    }
}
