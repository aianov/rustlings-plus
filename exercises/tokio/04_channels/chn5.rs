// Метрики летят из обработчика запросов, и обработчик не имеет права ждать: лучше потерять
// точку метрики, чем затормозить пользователя. Очередь к отправщику ограничена.
// TODO: Допиши `report` и `dropped`. `report` никогда не ждёт: кладёт метрику в очередь и говорит,
// что получилось (Queued), или что очередь полна и метрика выброшена (Dropped), или что
// отправщик ушёл насовсем (Closed). `dropped` показывает, сколько метрик выброшено из-за
// переполнения; закрытый канал в это число не входит.
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc::{self, Receiver, Sender, error::TrySendError};

#[derive(Debug, PartialEq)]
enum Outcome {
    Queued,
    Dropped,
    Closed,
}

struct Reporter {
    tx: Sender<String>,
    dropped: AtomicUsize,
}

impl Reporter {
    fn new(capacity: usize) -> (Reporter, Receiver<String>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Reporter { tx, dropped: AtomicUsize::new(0) }, rx)
    }

    fn report(&self, metric: &str) -> Outcome {
        todo!()
    }

    fn dropped(&self) -> usize {
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let (reporter, mut rx) = Reporter::new(1);
    println!("{:?} {:?}", reporter.report("a"), reporter.report("b"));
    println!("{:?}", rx.recv().await);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_queue_drops_new_metrics() {
        let (reporter, _rx) = Reporter::new(2);
        assert_eq!(reporter.report("a"), Outcome::Queued, "в пустую очередь кладём");
        assert_eq!(reporter.report("b"), Outcome::Queued, "второй тоже помещается");
        assert_eq!(reporter.report("c"), Outcome::Dropped, "третья не помещается: выбрасываем, не ждём");
        assert_eq!(reporter.dropped(), 1, "одна метрика выброшена");
    }

    #[tokio::test]
    async fn space_frees_up_after_receive() {
        let (reporter, mut rx) = Reporter::new(1);
        assert_eq!(reporter.report("a"), Outcome::Queued);
        assert_eq!(reporter.report("b"), Outcome::Dropped);
        assert_eq!(rx.recv().await.as_deref(), Some("a"), "в очереди осталась первая метрика, а не вторая");
        assert_eq!(reporter.report("c"), Outcome::Queued, "после чтения место снова есть");
        assert_eq!(reporter.dropped(), 1);
    }

    #[tokio::test]
    async fn closed_receiver_is_not_overflow() {
        let (reporter, rx) = Reporter::new(2);
        drop(rx);
        assert_eq!(reporter.report("a"), Outcome::Closed, "получатель ушёл: это Closed, а не Dropped");
        assert_eq!(reporter.dropped(), 0, "закрытие не считается переполнением");
    }
}
