// Фоновая задача иногда падает: сеть моргнула, пришло неожиданное сообщение, внутри паника. Бросить её
// насовсем нельзя (сервис без неё не работает), а перезапускать мгновенно и бесконечно тоже:
// вечно падающая задача будет жечь процессор и забивать журнал. Нужен надзиратель.
// TODO: Напиши `supervise`: запускает задачу, при панике ждёт `pause` и запускает заново, но не
// больше `max_restarts` раз; завершение без паники кончает работу. Токен отмены прерывает всё, и ожидание
// паузы тоже, и работающую задачу тоже (её надо остановить, а не бросить).
use futures::future::select_all;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
struct Token {
    own: Arc<watch::Sender<bool>>,
    parents: Vec<Arc<watch::Sender<bool>>>,
}

#[allow(dead_code)]
impl Token {
    fn new() -> Token {
        Token { own: Arc::new(watch::channel(false).0), parents: Vec::new() }
    }

    fn child(&self) -> Token {
        let mut parents = self.parents.clone();
        parents.push(self.own.clone());
        Token { own: Arc::new(watch::channel(false).0), parents }
    }

    fn cancel(&self) {
        self.own.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.own.borrow() || self.parents.iter().any(|flag| *flag.borrow())
    }

    async fn cancelled(&self) {
        let waits: Vec<_> = std::iter::once(&self.own)
            .chain(self.parents.iter())
            .map(|flag| {
                let mut rx = flag.subscribe();
                Box::pin(async move {
                    let _ = rx.wait_for(|cancelled| *cancelled).await;
                })
            })
            .collect();
        select_all(waits).await;
    }
}

use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, PartialEq)]
enum Outcome {
    Finished { restarts: u32 },
    GaveUp { attempts: u32 },
    Cancelled,
}

async fn supervise<F, Fut>(make: F, token: Token, pause: Duration, max_restarts: u32) -> Outcome
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    todo!()
}

#[tokio::main]
async fn main() {
    let outcome = supervise(|| async {}, Token::new(), Duration::from_millis(10), 3).await;
    println!("{outcome:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::time::Instant;

    fn failing(attempts: Arc<AtomicU32>, failures: u32) -> impl Fn() -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
        move || {
            let attempts = attempts.clone();
            Box::pin(async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                sleep(Duration::from_millis(10)).await;
                if n < failures {
                    panic!("задача упала (попытка {n})");
                }
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_flaky_task_is_restarted_until_it_succeeds() {
        let attempts = Arc::new(AtomicU32::new(0));
        let begin = Instant::now();
        let outcome = supervise(failing(attempts.clone(), 2), Token::new(), Duration::from_millis(100), 5).await;
        assert_eq!(outcome, Outcome::Finished { restarts: 2 }, "два падения, третья попытка удалась");
        assert_eq!(attempts.load(Ordering::SeqCst), 3, "всего три попытки");
        assert_eq!(begin.elapsed(), Duration::from_millis(230), "10 мс работы, пауза 100, 10 мс работы, пауза 100, 10 мс работы");
    }

    #[tokio::test(start_paused = true)]
    async fn a_hopeless_task_is_given_up_after_the_limit() {
        let attempts = Arc::new(AtomicU32::new(0));
        let begin = Instant::now();
        let outcome = supervise(failing(attempts.clone(), 99), Token::new(), Duration::from_millis(100), 3).await;
        assert_eq!(outcome, Outcome::GaveUp { attempts: 4 }, "первая попытка и три перезапуска: четыре попытки");
        assert_eq!(attempts.load(Ordering::SeqCst), 4, "пятой попытки быть не должно");
        assert_eq!(begin.elapsed(), Duration::from_millis(340), "после последнего падения пауза уже не нужна");
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_during_the_pause_stops_everything() {
        let attempts = Arc::new(AtomicU32::new(0));
        let token = Token::new();
        let begin = Instant::now();
        let canceller = {
            let token = token.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(50)).await;
                token.cancel();
            })
        };
        let outcome = supervise(failing(attempts.clone(), 99), token, Duration::from_millis(100), 5).await;
        canceller.await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled, "отмена пришла во время паузы");
        assert_eq!(begin.elapsed(), Duration::from_millis(50), "пауза прерывается отменой, а не досиживается");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "после отмены новых попыток быть не должно");
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_during_the_run_stops_the_running_task() {
        struct Note(Arc<Mutex<Vec<&'static str>>>);
        impl Drop for Note {
            fn drop(&mut self) {
                self.0.lock().unwrap().push("задача остановлена");
            }
        }
        let log = Arc::new(Mutex::new(Vec::new()));
        let token = Token::new();
        let make = {
            let log = log.clone();
            move || {
                let log = log.clone();
                async move {
                    let _note = Note(log);
                    std::future::pending::<()>().await;
                }
            }
        };
        let canceller = {
            let token = token.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(50)).await;
                token.cancel();
            })
        };
        let outcome = supervise(make, token, Duration::from_millis(100), 5).await;
        canceller.await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled, "отмена во время работы");
        assert_eq!(*log.lock().unwrap(), vec!["задача остановлена"], "работающую задачу надо остановить и дождаться, а не бросить");
    }

    #[tokio::test(start_paused = true)]
    async fn a_task_that_finishes_is_not_restarted() {
        let attempts = Arc::new(AtomicU32::new(0));
        let outcome = supervise(failing(attempts.clone(), 0), Token::new(), Duration::from_millis(100), 5).await;
        assert_eq!(outcome, Outcome::Finished { restarts: 0 }, "нормальный конец это конец");
        assert_eq!(attempts.load(Ordering::SeqCst), 1, "одна попытка");
    }
}
