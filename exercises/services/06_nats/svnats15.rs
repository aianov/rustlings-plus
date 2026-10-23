// notify-service получает события в цикле и тут же, прямо в нём, отправляет уведомления
// (это 20 мс на каждое). Пока цикл занят обработкой, буфер подписки копится, и во время
// всплеска хвост событий молча отбрасывается. Цель: цикл приёма только принимает и передаёт
// работу дальше, обработка идёт не больше чем в `workers` одновременных задач, а очередь
// между ними ограничена `queue`. Когда очередь полна, цикл приёма не встаёт: он выбрасывает
// событие и считает это в `dropped`, чтобы перегрузка была видна, а не молчалива.


use async_nats::Client;
use futures::StreamExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::task::JoinHandle;

fn main() {}

pub type Handler = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub struct Intake {
    pub dropped: Arc<AtomicUsize>,
    pub task: JoinHandle<()>,
}

pub async fn start(client: Client, subject: String, workers: usize, queue: usize, handler: Handler) -> Result<Intake, String> {
    let mut subscription = client.subscribe(subject).await.map_err(|e| e.to_string())?;
    let inbox = client.new_inbox();
    let mut probe = client.subscribe(inbox.clone()).await.map_err(|e| e.to_string())?;
    client.publish(inbox, "".into()).await.map_err(|e| e.to_string())?;
    probe.next().await;
    let dropped = Arc::new(AtomicUsize::new(0));
    let _ = (workers, queue);
    let task = tokio::spawn(async move {
        let _connection = client;
        while let Some(message) = subscription.next().await {
            handler(String::from_utf8_lossy(&message.payload).into_owned()).await;
        }
    });
    Ok(Intake { dropped, task })
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, sync};
    use std::time::Duration;
    use tokio::sync::Semaphore;
    use tokio::time::{Instant, timeout};

    #[derive(Default)]
    struct Probe {
        entered: AtomicUsize,
        running: AtomicUsize,
        peak: AtomicUsize,
        done: AtomicUsize,
    }

    fn slow_handler(probe: Arc<Probe>, work: Duration) -> Handler {
        Arc::new(move |_payload| {
            let probe = probe.clone();
            Box::pin(async move {
                probe.entered.fetch_add(1, Ordering::SeqCst);
                let now = probe.running.fetch_add(1, Ordering::SeqCst) + 1;
                probe.peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(work).await;
                probe.running.fetch_sub(1, Ordering::SeqCst);
                probe.done.fetch_add(1, Ordering::SeqCst);
            })
        })
    }

    fn gated_handler(probe: Arc<Probe>, gate: Arc<Semaphore>) -> Handler {
        Arc::new(move |_payload| {
            let (probe, gate) = (probe.clone(), gate.clone());
            Box::pin(async move {
                probe.entered.fetch_add(1, Ordering::SeqCst);
                let permit = gate.acquire().await.expect("шлагбаум");
                permit.forget();
                probe.done.fetch_add(1, Ordering::SeqCst);
            })
        })
    }

    async fn publish(client: &Client, subject: &str, n: usize) {
        for i in 0..n {
            client.publish(subject.to_string(), format!("e{i}").into()).await.unwrap();
        }
        sync(client).await;
    }

    async fn until(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !ready() {
            assert!(Instant::now() < deadline, "не дождались: {what}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn a_burst_is_handled_in_parallel_without_loss() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let probe = Arc::new(Probe::default());
        let intake = start(connect().await, subject.clone(), 8, 256, slow_handler(probe.clone(), Duration::from_millis(20))).await.expect("запуск");
        let started = Instant::now();
        publish(&connect().await, &subject, 200).await;
        timeout(Duration::from_secs(2), until("все 200 событий обработаны", || probe.done.load(Ordering::SeqCst) == 200))
            .await
            .expect("200 событий по 20 мс при 8 работниках должны уложиться в пару секунд; последовательная обработка занимает 4");
        assert!(started.elapsed() < Duration::from_millis(1800), "параллельная обработка должна быть заметно быстрее последовательной: {:?}", started.elapsed());
        let peak = probe.peak.load(Ordering::SeqCst);
        assert!(peak > 1, "обработка шла по одному событию за раз");
        assert!(peak <= 8, "одновременно работало {peak} обработчиков при лимите 8");
        assert_eq!(intake.dropped.load(Ordering::SeqCst), 0, "очередь на 256 вмещает всплеск в 200, ничего выбрасывать не надо");
    }

    #[tokio::test]
    async fn overload_is_dropped_and_counted_while_the_receiver_keeps_up() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let probe = Arc::new(Probe::default());
        let gate = Arc::new(Semaphore::new(0));
        let intake = start(connect().await, subject.clone(), 2, 8, gated_handler(probe.clone(), gate.clone())).await.expect("запуск");
        let publisher = connect().await;
        publish(&publisher, &subject, 2).await;
        until("оба работника заняты", || probe.entered.load(Ordering::SeqCst) == 2).await;
        publish(&publisher, &subject, 100).await;
        until("цикл приёма разобрал всё и выбросил лишнее", || intake.dropped.load(Ordering::SeqCst) >= 92).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(intake.dropped.load(Ordering::SeqCst), 92, "работники заняты, в очереди 8 мест: из 100 событий принимаются 8, остальные 92 выбрасываются и считаются");
        gate.add_permits(1000);
        until("принятые обработаны", || probe.done.load(Ordering::SeqCst) == 10).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(probe.done.load(Ordering::SeqCst), 10, "обработано ровно то, что принято: 2 в работе и 8 в очереди");
    }
}
