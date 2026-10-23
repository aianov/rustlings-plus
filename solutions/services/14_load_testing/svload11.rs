
use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tokio::task::JoinSet;
use tokio::time::timeout;

fn main() {}

pub trait Receiver: Send + Sync + 'static {
    fn send(&self, message: &str) -> BoxFuture<'_, Result<(), String>>;
}

#[derive(Debug, PartialEq)]
pub struct BroadcastReport {
    pub delivered: usize,
    pub timed_out: Vec<usize>,
    pub failed: Vec<usize>,
    pub last_delivery: Duration,
}

pub async fn broadcast(receivers: &[Arc<dyn Receiver>], message: &str, per_receiver: Duration) -> BroadcastReport {
    let started = Instant::now();
    let mut set = JoinSet::new();
    for (i, receiver) in receivers.iter().enumerate() {
        let (receiver, message) = (receiver.clone(), message.to_string());
        set.spawn(async move {
            let outcome = timeout(per_receiver, receiver.send(&message)).await;
            (i, outcome, started.elapsed())
        });
    }
    let mut report = BroadcastReport { delivered: 0, timed_out: Vec::new(), failed: Vec::new(), last_delivery: Duration::ZERO };
    while let Some(done) = set.join_next().await {
        let Ok((i, outcome, at)) = done else { continue };
        match outcome {
            Ok(Ok(())) => {
                report.delivered += 1;
                report.last_delivery = report.last_delivery.max(at);
            }
            Ok(Err(_)) => report.failed.push(i),
            Err(_) => report.timed_out.push(i),
        }
    }
    report.timed_out.sort_unstable();
    report.failed.sort_unstable();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    struct Peer {
        delay: Duration,
        fail: bool,
    }

    impl Receiver for Peer {
        fn send(&self, _message: &str) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                sleep(self.delay).await;
                if self.fail { Err("соединение закрыто".to_string()) } else { Ok(()) }
            })
        }
    }

    fn peers(n: usize, delay: Duration) -> Vec<Arc<dyn Receiver>> {
        (0..n).map(|_| Arc::new(Peer { delay, fail: false }) as Arc<dyn Receiver>).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn a_thousand_healthy_receivers_get_it_at_the_same_time() {
        let started = Instant::now();
        let report = broadcast(&peers(1000, MS(1)), "привет", MS(200)).await;
        assert_eq!(report.delivered, 1000);
        assert!(report.timed_out.is_empty() && report.failed.is_empty());
        assert_eq!(report.last_delivery, MS(1), "тысяча получателей по 1 мс одновременно: до последнего тоже 1 мс, а не секунда по очереди");
        assert_eq!(started.elapsed(), MS(1));
    }

    #[tokio::test(start_paused = true)]
    async fn one_frozen_receiver_does_not_hold_the_others() {
        let mut all = peers(1000, MS(1));
        all[500] = Arc::new(Peer { delay: Duration::from_secs(10), fail: false });
        let started = Instant::now();
        let report = broadcast(&all, "привет", MS(200)).await;
        assert_eq!(report.delivered, 999);
        assert_eq!(report.timed_out, [500], "зависший получатель отмечен, а не потерян");
        assert_eq!(report.last_delivery, MS(1), "до последнего из успешных дошло за 1 мс; зависший на это число не влияет");
        assert_eq!(started.elapsed(), MS(200), "вся рассылка занимает не больше срока на одного получателя");
    }

    #[tokio::test(start_paused = true)]
    async fn failures_and_timeouts_are_told_apart_and_listed_in_order() {
        let mut all = peers(6, MS(5));
        all[4] = Arc::new(Peer { delay: MS(1), fail: true });
        all[1] = Arc::new(Peer { delay: MS(1), fail: true });
        all[2] = Arc::new(Peer { delay: Duration::from_secs(3), fail: false });
        all[0] = Arc::new(Peer { delay: Duration::from_secs(3), fail: false });
        let report = broadcast(&all, "x", MS(100)).await;
        assert_eq!(report.delivered, 2);
        assert_eq!(report.failed, [1, 4]);
        assert_eq!(report.timed_out, [0, 2]);
        assert_eq!(report.last_delivery, MS(5));
    }

    #[tokio::test(start_paused = true)]
    async fn nobody_to_send_to() {
        let report = broadcast(&[], "x", MS(100)).await;
        assert_eq!(report, BroadcastReport { delivered: 0, timed_out: vec![], failed: vec![], last_delivery: Duration::ZERO });
    }

    #[tokio::test(start_paused = true)]
    async fn a_receiver_exactly_at_the_limit_is_still_a_timeout_or_a_delivery_but_never_lost() {
        let report = broadcast(&peers(3, MS(100)), "x", MS(100)).await;
        assert_eq!(report.delivered as usize + report.timed_out.len(), 3, "каждый получатель учтён ровно один раз: {report:?}");
    }
}
