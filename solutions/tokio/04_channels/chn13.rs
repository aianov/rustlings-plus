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
    tx.send_modify(|settings| settings.max_len += delta);
}

async fn wait_stopped(mut rx: watch::Receiver<Mode>) -> Result<(), SenderGone> {
    rx.wait_for(|mode| *mode == Mode::Stopped).await.map(|_| ()).map_err(|_| SenderGone)
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
