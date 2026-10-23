// Подключение теперь может не получиться: первые две попытки падают. Ошибку кэшировать нельзя,
// следующий вызов должен попробовать снова. А успешное подключение должно быть одно на всех,
// и попыток на всё не больше, чем нужно: две неудачные и одна удачная, сколько бы задач ни
// ждало одновременно. Сейчас `get` проверяет и подключается без согласования, и параллельные
// вызовы подключаются каждый сам.
// TODO: Почини get.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;

#[derive(Debug, PartialEq)]
struct ConnectError(usize);

struct Conn;

struct Pool {
    cell: OnceCell<Conn>,
    attempts: AtomicUsize,
}

impl Pool {
    fn new() -> Self {
        Pool { cell: OnceCell::new(), attempts: AtomicUsize::new(0) }
    }

    async fn connect(&self) -> Result<Conn, ConnectError> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        tokio::time::sleep(Duration::from_millis(20)).await;
        if attempt <= 2 { Err(ConnectError(attempt)) } else { Ok(Conn) }
    }
}

impl Pool {
    async fn get(&self) -> Result<&Conn, ConnectError> {
        if let Some(conn) = self.cell.get() {
            return Ok(conn);
        }
        let conn = self.connect().await?;
        Ok(self.cell.get_or_init(|| async { conn }).await)
    }
}

#[tokio::main]
async fn main() {
    let pool = Pool::new();
    for _ in 0..3 {
        println!("{:?}", pool.get().await.map(|_| "подключено"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test(start_paused = true)]
    async fn failures_are_not_cached() {
        let pool = Pool::new();
        assert_eq!(pool.get().await.err(), Some(ConnectError(1)), "первая попытка падает");
        assert_eq!(pool.get().await.err(), Some(ConnectError(2)), "вторая тоже: ошибку кэшировать нельзя");
        assert!(pool.get().await.is_ok(), "третья удалась");
        assert!(pool.get().await.is_ok(), "готовое подключение отдаётся из кэша");
        assert_eq!(pool.attempts.load(Ordering::SeqCst), 3, "всего три попытки");
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_callers_share_attempts() {
        let pool = Arc::new(Pool::new());
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let pool = Arc::clone(&pool);
                tokio::spawn(async move { pool.get().await.is_ok() })
            })
            .collect();
        let mut ok = 0;
        for handle in handles {
            if handle.await.unwrap() {
                ok += 1;
            }
        }
        assert_eq!(
            pool.attempts.load(Ordering::SeqCst),
            3,
            "десять задач сделали не три попытки подключения: вызовы не согласованы между собой"
        );
        assert_eq!(ok, 8, "две задачи получают ошибки неудачных попыток, остальные восемь подключение");
    }
}
