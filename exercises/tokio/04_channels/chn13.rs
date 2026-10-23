// Две мелочи про watch, где легко ошибиться.
// 1. Лимит длины поста правят сразу несколько задач-админов: каждая прибавляет к нему свою
//    дельту. Ни одна правка не должна потеряться, даже при гонке потоков.
// 2. Остановку сервиса ждут через watch-канал состояния. Ждущий должен проснуться, как только
//    состояние стало Stopped, но если оно Stopped уже сейчас, ждать нельзя совсем, а если отправитель
//    ушёл, так и не дойдя до Stopped, это ошибка.
// TODO: Допиши `bump_max_len` и `wait_stopped`.
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq)]
struct Settings {
    max_len: u32,
    posts_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Running,
    Draining,
    Stopped,
}

#[derive(Debug, PartialEq)]
struct SenderGone;

fn bump_max_len(tx: &watch::Sender<Settings>, delta: u32) {
    todo!()
}

async fn wait_stopped(mut rx: watch::Receiver<Mode>) -> Result<(), SenderGone> {
    todo!()
}

#[tokio::main]
async fn main() {
    let (tx, rx) = watch::channel(Mode::Running);
    let waiter = tokio::spawn(wait_stopped(rx));
    tx.send(Mode::Stopped).unwrap();
    println!("{:?}", waiter.await.unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::{Instant, sleep};

    #[test]
    fn concurrent_bumps_are_not_lost() {
        let (tx, rx) = watch::channel(Settings { max_len: 0, posts_enabled: true });
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..5000 {
                        bump_max_len(&tx, 1);
                    }
                });
            }
        });
        let settings = rx.borrow().clone();
        assert_eq!(settings.max_len, 40_000, "часть правок потерялась: чтение и запись должны быть одним действием");
        assert!(settings.posts_enabled, "остальные поля настроек не должны меняться");
        assert!(rx.has_changed().unwrap(), "подписчики должны узнать, что настройки изменились");
    }

    #[tokio::test(start_paused = true)]
    async fn already_stopped_returns_at_once() {
        let (_tx, rx) = watch::channel(Mode::Stopped);
        let result = tokio::time::timeout(Duration::from_secs(1), wait_stopped(rx))
            .await
            .expect("wait_stopped завис: состояние уже Stopped, ждать нечего");
        assert_eq!(result, Ok(()), "уже остановлен");
    }

    #[tokio::test(start_paused = true)]
    async fn wakes_on_the_final_state() {
        let (tx, rx) = watch::channel(Mode::Running);
        let started = Instant::now();
        let waiter = tokio::spawn(wait_stopped(rx));
        sleep(Duration::from_millis(10)).await;
        tx.send(Mode::Draining).unwrap();
        sleep(Duration::from_millis(10)).await;
        tx.send(Mode::Stopped).unwrap();
        assert_eq!(waiter.await.unwrap(), Ok(()), "дождались Stopped");
        assert_eq!(started.elapsed(), Duration::from_millis(20), "промежуточное Draining не должно будить окончательно");
    }

    #[tokio::test(start_paused = true)]
    async fn sender_leaves_before_stop() {
        let (tx, rx) = watch::channel(Mode::Running);
        let waiter = tokio::spawn(wait_stopped(rx));
        sleep(Duration::from_millis(10)).await;
        tx.send(Mode::Draining).unwrap();
        sleep(Duration::from_millis(10)).await;
        drop(tx);
        assert_eq!(waiter.await.unwrap(), Err(SenderGone), "отправитель ушёл на Draining: Stopped уже не наступит");
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_then_sender_gone_is_still_ok() {
        let (tx, rx) = watch::channel(Mode::Running);
        let waiter = tokio::spawn(wait_stopped(rx));
        sleep(Duration::from_millis(10)).await;
        tx.send(Mode::Stopped).unwrap();
        drop(tx);
        assert_eq!(waiter.await.unwrap(), Ok(()), "Stopped наступил до ухода отправителя: это успех, а не SenderGone");
    }
}
