// Сигнал «пора останавливаться» нужен всем фоновым задачам, и передавать его хочется как обычное
// значение: клонировать, раздавать, ждать в `select!`. Готового токена отмены в нашем разделе нет
// (он живёт в отдельном крейте), поэтому напиши свой.
// TODO: Допиши `Token`: `new`, `cancel`, `is_cancelled` и `cancelled().await`. Клоны смотрят на один
// и тот же сигнал. Тот, кто начал ждать позже отмены, не должен зависнуть.
use std::sync::Arc;
use tokio::sync::watch;

struct Token;

impl Token {
    fn new() -> Token {
        todo!()
    }

    fn cancel(&self) {
        todo!()
    }

    fn is_cancelled(&self) -> bool {
        todo!()
    }

    async fn cancelled(&self) {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let token = Token::new();
    let waiter = token.clone();
    let task = tokio::spawn(async move { waiter.cancelled().await });
    token.cancel();
    task.await.unwrap();
    println!("{}", token.is_cancelled());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    fn assert_shareable<T: Clone + Send + Sync + 'static>() {}

    #[tokio::test]
    async fn cancel_is_visible_through_every_clone() {
        assert_shareable::<Token>();
        let token = Token::new();
        let other = token.clone();
        assert!(!token.is_cancelled() && !other.is_cancelled(), "новый токен не отменён");
        other.cancel();
        assert!(token.is_cancelled(), "отмена через клон видна оригиналу");
        other.cancel();
        assert!(other.is_cancelled(), "повторная отмена безвредна");
    }

    #[tokio::test(start_paused = true)]
    async fn a_waiter_wakes_up_when_cancelled() {
        let token = Token::new();
        let waiter = {
            let token = token.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                tokio::time::Instant::now()
            })
        };
        let start = tokio::time::Instant::now();
        sleep(Duration::from_millis(40)).await;
        assert!(!waiter.is_finished(), "токен не отменён, ждущий должен ждать");
        token.cancel();
        let woke = waiter.await.unwrap();
        assert_eq!(woke - start, Duration::from_millis(40), "ждущий просыпается в момент отмены");
    }

    #[tokio::test]
    async fn late_waiters_do_not_hang() {
        let token = Token::new();
        token.cancel();
        assert!(token.is_cancelled(), "отмена до всяких ожидающих должна запомниться");
        timeout(Duration::from_secs(1), token.cancelled())
            .await
            .expect("токен уже отменён, а cancelled() всё равно ждёт: сигнал потерялся, потому что слушателей не было");
    }

    #[tokio::test]
    async fn all_waiters_wake_and_select_can_use_it() {
        let token = Token::new();
        let mut waiters = Vec::new();
        for _ in 0..5 {
            let token = token.clone();
            waiters.push(tokio::spawn(async move { token.cancelled().await }));
        }
        tokio::task::yield_now().await;
        token.cancel();
        for waiter in waiters {
            timeout(Duration::from_secs(1), waiter).await.expect("ждущий не проснулся").unwrap();
        }
        let verdict = tokio::select! {
            _ = token.cancelled() => "stop",
            _ = sleep(Duration::from_secs(10)) => "timeout",
        };
        assert_eq!(verdict, "stop", "в select! отменённый токен побеждает долгий таймер");
    }
}
