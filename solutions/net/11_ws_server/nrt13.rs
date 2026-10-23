// Деплой новой версии: старый шлюз должен уйти, а у него тысячи открытых соединений. Просто убить процесс значит оборвать клиентов
// без объяснений, и все они кинутся переподключаться одновременно. Вежливый порядок: каждому отправить Close с кодом 1001
// («сервер уходит»), дать время дописать и уйти самим, а упрямых после срока оборвать. Срок общий на всех, иначе остановка
// с десятью упрямыми клиентами длилась бы десять сроков. TODO: Допиши shutdown и отчёт о том, кто ушёл сам, а кого пришлось добить.

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout_at};

struct ConnHandle {
    close: mpsc::Sender<u16>,
    task: JoinHandle<()>,
}

#[derive(Debug, PartialEq)]
struct Report {
    graceful: usize,
    forced: usize,
}

async fn shutdown(handles: Vec<ConnHandle>, grace: Duration) -> Report {
    for handle in &handles {
        let _ = handle.close.try_send(1001);
    }
    let deadline = Instant::now() + grace;
    let mut report = Report { graceful: 0, forced: 0 };
    for mut handle in handles {
        match timeout_at(deadline, &mut handle.task).await {
            Ok(_) => report.graceful += 1,
            Err(_) => {
                handle.task.abort();
                let _ = handle.task.await;
                report.forced += 1;
            }
        }
    }
    report
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::time::{sleep, timeout};

    type Seen = Arc<Mutex<Vec<(usize, u16, Duration)>>>;

    struct Flag(Arc<AtomicBool>);
    impl Drop for Flag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[derive(Clone, Copy)]
    enum Mood {
        Polite,
        Slow(u64),
        Stubborn,
    }

    fn spawn_conn(index: usize, mood: Mood, seen: Seen, began: Instant, dropped: Arc<AtomicBool>) -> ConnHandle {
        let (close, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(async move {
            let _flag = Flag(dropped);
            loop {
                tokio::select! {
                    Some(code) = rx.recv() => {
                        seen.lock().unwrap().push((index, code, began.elapsed()));
                        match mood {
                            Mood::Polite => return,
                            Mood::Slow(ms) => {
                                sleep(Duration::from_millis(ms)).await;
                                return;
                            }
                            Mood::Stubborn => {}
                        }
                    }
                    _ = sleep(Duration::from_secs(3600)) => {}
                }
            }
        });
        ConnHandle { close, task }
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_gets_close_first_then_the_common_deadline_applies() {
        let began = Instant::now();
        let seen: Seen = Arc::default();
        let stubborn_dropped = Arc::new(AtomicBool::new(false));
        let handles = vec![
            spawn_conn(0, Mood::Stubborn, seen.clone(), began, stubborn_dropped.clone()),
            spawn_conn(1, Mood::Polite, seen.clone(), began, Arc::default()),
            spawn_conn(2, Mood::Slow(2000), seen.clone(), began, Arc::default()),
        ];
        sleep(Duration::from_millis(10)).await;
        let started = Instant::now();
        let report = shutdown(handles, Duration::from_secs(5)).await;
        let took = started.elapsed();
        assert_eq!(report, Report { graceful: 2, forced: 1 }, "двое ушли сами, упрямого пришлось добить");
        assert!(took >= Duration::from_secs(5) && took < Duration::from_millis(5100), "ждать нужно до общего срока (5 с), не дольше: {took:?}");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3, "Close должны получить все трое");
        for (index, code, at) in seen.iter() {
            assert_eq!(*code, 1001, "соединение {index} получило не тот код");
            assert!(*at < Duration::from_millis(11), "соединение {index} получило Close только через {at:?}: сначала надо разослать всем, потом ждать");
        }
        assert!(stubborn_dropped.load(Ordering::SeqCst), "упрямая задача должна быть оборвана (abort) и дождаться этого");
    }

    #[tokio::test(start_paused = true)]
    async fn polite_clients_do_not_make_us_wait_for_the_whole_grace_period() {
        let began = Instant::now();
        let seen: Seen = Arc::default();
        let handles: Vec<_> = (0..20).map(|i| spawn_conn(i, Mood::Polite, seen.clone(), began, Arc::default())).collect();
        let report = shutdown(handles, Duration::from_secs(30)).await;
        assert_eq!(report, Report { graceful: 20, forced: 0 });
        assert!(began.elapsed() < Duration::from_secs(1), "все ушли сразу, а остановка длилась {:?}: срок это потолок, а не пауза", began.elapsed());
    }

    #[tokio::test(start_paused = true)]
    async fn slow_leavers_are_awaited_together_not_one_after_another() {
        let began = Instant::now();
        let seen: Seen = Arc::default();
        let handles: Vec<_> = (0..5).map(|i| spawn_conn(i, Mood::Slow(2000), seen.clone(), began, Arc::default())).collect();
        let report = shutdown(handles, Duration::from_secs(10)).await;
        assert_eq!(report, Report { graceful: 5, forced: 0 });
        let took = began.elapsed();
        assert!(took >= Duration::from_secs(2) && took < Duration::from_millis(2100), "пятеро по 2 с уходят параллельно, всего около 2 с, а не 10: {took:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn already_gone_connections_are_counted_and_do_not_panic() {
        let began = Instant::now();
        let seen: Seen = Arc::default();
        let gone = spawn_conn(0, Mood::Polite, seen.clone(), began, Arc::default());
        gone.close.send(1000).await.unwrap();
        sleep(Duration::from_millis(5)).await;
        assert!(gone.task.is_finished());
        let finished = timeout(Duration::from_secs(1), shutdown(vec![gone], Duration::from_secs(5))).await.expect("shutdown завис на уже завершённом соединении");
        assert_eq!(finished, Report { graceful: 1, forced: 0 });
    }
}
