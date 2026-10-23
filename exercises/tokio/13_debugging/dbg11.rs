// Фоновая задача запущена через `tokio::spawn` и забыта. Если она паникует, это видно только в stderr
// самого процесса, а сервис продолжает считать себя здоровым. Нужна обёртка `spawn_watched`: она
// запускает задачу под именем и присылает в канал отчёт о том, чем кончилась: нормально, паникой
// (с текстом паники) или отменой. Вернуть она должна ручку, которой задачу можно прервать.
// TODO: Напиши `spawn_watched`.
use std::future::Future;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

#[derive(Debug, PartialEq)]
enum End {
    Done,
    Panicked(String),
    Cancelled,
}

type Reports = mpsc::UnboundedSender<(&'static str, End)>;

fn spawn_watched<F>(name: &'static str, job: F, reports: Reports) -> AbortHandle
where
    F: Future<Output = ()> + Send + 'static,
{
    let _ = (name, reports);
    tokio::spawn(job).abort_handle()
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    spawn_watched("hello", async {}, tx);
    println!("{:?}", rx.recv().await);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    async fn next(rx: &mut mpsc::UnboundedReceiver<(&'static str, End)>) -> (&'static str, End) {
        timeout(Duration::from_secs(1), rx.recv()).await.expect("отчёта нет: задача закончилась, а о ней никто не сообщил").unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn a_normal_end_is_reported() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_watched("mail", async { sleep(Duration::from_millis(10)).await }, tx);
        assert_eq!(next(&mut rx).await, ("mail", End::Done), "обычное завершение");
    }

    #[tokio::test(start_paused = true)]
    async fn a_panic_is_reported_with_its_text() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_watched("sync", async { panic!("диск переполнен") }, tx.clone());
        spawn_watched("fmt", async { panic!("код {}", 42) }, tx.clone());
        spawn_watched("odd", async { std::panic::panic_any(7u8) }, tx);
        let mut got = vec![next(&mut rx).await, next(&mut rx).await, next(&mut rx).await];
        got.sort_by_key(|(name, _)| *name);
        assert_eq!(
            got,
            vec![
                ("fmt", End::Panicked("код 42".to_string())),
                ("odd", End::Panicked("неизвестная паника".to_string())),
                ("sync", End::Panicked("диск переполнен".to_string())),
            ],
            "паника должна приходить с текстом (строка литералом, строка из format!, и ничего понятного)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_abort_is_reported_as_cancellation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let abort = spawn_watched("long", async { sleep(Duration::from_secs(3600)).await }, tx);
        sleep(Duration::from_millis(10)).await;
        abort.abort();
        assert_eq!(next(&mut rx).await, ("long", End::Cancelled), "прерванная задача сообщает об отмене");
    }

    #[tokio::test(start_paused = true)]
    async fn one_panic_does_not_stop_the_others() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_watched("bad", async { panic!("упала") }, tx.clone());
        spawn_watched("good", async { sleep(Duration::from_millis(5)).await }, tx);
        let first = next(&mut rx).await;
        let second = next(&mut rx).await;
        assert_eq!(first, ("bad", End::Panicked("упала".to_string())));
        assert_eq!(second, ("good", End::Done), "соседняя задача отработала нормально");
    }
}
