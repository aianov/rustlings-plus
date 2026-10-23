// Производитель нумерует сообщения подряд (0, 1, 2, ...) и шлёт их в очередь, пока не придёт сигнал
// остановки. Получатель иногда тормозит, очередь заполняется, и сигнал приходит как раз тогда, когда
// производитель ждёт свободное место. После остановки в нумерации дыра: номер выдан, а сообщение
// не доставлено. Выданные номера и доставленные сообщения должны сходиться.
// TODO: Почини `produce`.
use tokio::sync::{mpsc, oneshot};

struct Ids {
    next: u32,
}

impl Ids {
    fn new() -> Ids {
        Ids { next: 0 }
    }

    fn take(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

async fn produce(out: mpsc::Sender<u32>, mut ids: Ids, mut stop: oneshot::Receiver<()>) -> Ids {
    loop {
        tokio::select! {
            _ = out.send(ids.take()) => {}
            _ = &mut stop => break,
        }
    }
    ids
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(2);
    let (stop_tx, stop) = oneshot::channel();
    let producer = tokio::spawn(produce(tx, Ids::new(), stop));
    println!("{:?}", rx.recv().await);
    stop_tx.send(()).unwrap();
    let ids = producer.await.unwrap();
    println!("выдано номеров: {}", ids.next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test(start_paused = true)]
    async fn full_queue_at_stop_loses_nothing() {
        let (tx, mut rx) = mpsc::channel(3);
        let (stop_tx, stop) = oneshot::channel();
        let producer = tokio::spawn(produce(tx, Ids::new(), stop));
        sleep(Duration::from_millis(10)).await;
        stop_tx.send(()).unwrap();
        let ids = producer.await.unwrap();
        let mut received = Vec::new();
        while let Some(id) = rx.recv().await {
            received.push(id);
        }
        assert_eq!(received, vec![0, 1, 2], "очередь на три места: в ней должны лежать номера 0, 1, 2");
        assert_eq!(ids.next, 3, "выдано {} номеров, а доставлено 3: номер взят, но сообщение не отправлено", ids.next);
    }

    #[tokio::test(start_paused = true)]
    async fn slow_consumer_sees_a_gapless_stream() {
        let (tx, mut rx) = mpsc::channel(2);
        let (stop_tx, stop) = oneshot::channel();
        let producer = tokio::spawn(produce(tx, Ids::new(), stop));
        let consumer = tokio::spawn(async move {
            let mut received = Vec::new();
            while let Some(id) = rx.recv().await {
                received.push(id);
                sleep(Duration::from_millis(50)).await;
            }
            received
        });
        sleep(Duration::from_millis(1000)).await;
        stop_tx.send(()).unwrap();
        let ids = producer.await.unwrap();
        let received = consumer.await.unwrap();
        let expected: Vec<u32> = (0..ids.next).collect();
        assert_eq!(received, expected, "получатель должен увидеть все выданные номера подряд, без дыр");
        assert!(received.len() > 10, "производитель работал целую секунду, а доставил всего {}", received.len());
    }
}
