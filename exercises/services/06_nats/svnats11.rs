// notify-service запускают тремя экземплярами, чтобы выдержать нагрузку: каждое событие
// должно быть обработано ровно одним из них. А аудит-журнал должен видеть вообще все
// события, ему делить нечего. Ещё экземпляр иногда выключают при деплое: то, что он уже
// получил, он обязан доработать, а новые события должны уходить остальным.
// Цель: start_worker запускает экземпляр в группе (возвращается, когда экземпляр уже слышит
// события), Worker::stop останавливает его без потерь, start_audit слушает всё без группы.


use async_nats::Client;
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Handled {
    pub worker: usize,
    pub payload: String,
}

pub struct Worker {
    task: JoinHandle<()>,
}

pub async fn start_worker(
    client: Client,
    subject: String,
    group: &str,
    id: usize,
    work: Duration,
    sink: mpsc::UnboundedSender<Handled>,
) -> Result<Worker, String> {
    let _ = (client, subject, group, id, work, sink);
    todo!()
}

impl Worker {
    pub async fn stop(self) {
        self.task.abort();
    }
}

pub async fn start_audit(client: Client, subject: String, sink: mpsc::UnboundedSender<String>) -> Result<JoinHandle<()>, String> {
    let _ = (client, subject, sink);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Proxy, Scope, connect, connect_to};
    use std::collections::{BTreeSet, HashMap};
    use tokio::time::timeout;

    async fn collect_handled(rx: &mut mpsc::UnboundedReceiver<Handled>, n: usize) -> Vec<Handled> {
        let mut got = Vec::new();
        while got.len() < n {
            match timeout(Duration::from_secs(4), rx.recv()).await {
                Ok(Some(item)) => got.push(item),
                _ => panic!("обработано только {} из {n} событий", got.len()),
            }
        }
        got
    }

    async fn publish_numbers(client: &Client, subject: &str, range: std::ops::Range<usize>) {
        for i in range {
            client.publish(subject.to_string(), format!("e{i}").into()).await.unwrap();
        }
        client.flush().await.unwrap();
    }

    #[tokio::test]
    async fn each_event_is_handled_by_exactly_one_instance() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut workers = Vec::new();
        for id in 0..3 {
            workers.push(start_worker(connect().await, subject.clone(), "notify", id, Duration::ZERO, tx.clone()).await.expect("экземпляр"));
        }
        let publisher = connect().await;
        publish_numbers(&publisher, &subject, 0..90).await;
        let got = collect_handled(&mut rx, 90).await;
        let payloads: BTreeSet<&str> = got.iter().map(|h| h.payload.as_str()).collect();
        assert_eq!(payloads.len(), 90, "каждое из 90 событий обработано ровно один раз, без дублей");
        let mut per_worker: HashMap<usize, usize> = HashMap::new();
        for item in &got {
            *per_worker.entry(item.worker).or_default() += 1;
        }
        assert_eq!(per_worker.len(), 3, "работа делится между всеми тремя экземплярами: {per_worker:?}");
        assert!(timeout(Duration::from_millis(300), rx.recv()).await.is_err(), "после 90 событий больше ничего приходить не должно");
        for worker in workers {
            worker.stop().await;
        }
    }

    #[tokio::test]
    async fn the_audit_sees_everything_while_the_workers_split_it() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (audit_tx, mut audit_rx) = mpsc::unbounded_channel();
        let _audit = start_audit(connect().await, subject.clone(), audit_tx).await.expect("аудит");
        let mut workers = Vec::new();
        for id in 0..2 {
            workers.push(start_worker(connect().await, subject.clone(), "notify", id, Duration::ZERO, tx.clone()).await.expect("экземпляр"));
        }
        publish_numbers(&connect().await, &subject, 0..40).await;
        assert_eq!(collect_handled(&mut rx, 40).await.len(), 40, "рабочие обработали по разу");
        let mut audited = BTreeSet::new();
        for _ in 0..40 {
            audited.insert(timeout(Duration::from_secs(3), audit_rx.recv()).await.expect("аудит ждал").expect("канал"));
        }
        assert_eq!(audited.len(), 40, "аудит получил все 40 событий, а не свою долю");
    }

    #[tokio::test]
    async fn two_groups_each_get_their_own_full_copy() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
        let (feed_tx, mut feed_rx) = mpsc::unbounded_channel();
        let _a = start_worker(connect().await, subject.clone(), "notify", 1, Duration::ZERO, notify_tx.clone()).await.unwrap();
        let _b = start_worker(connect().await, subject.clone(), "notify", 2, Duration::ZERO, notify_tx).await.unwrap();
        let _c = start_worker(connect().await, subject.clone(), "feed", 3, Duration::ZERO, feed_tx).await.unwrap();
        publish_numbers(&connect().await, &subject, 0..20).await;
        assert_eq!(collect_handled(&mut notify_rx, 20).await.len(), 20, "группа notify получила 20");
        assert_eq!(collect_handled(&mut feed_rx, 20).await.len(), 20, "группа feed тоже получила все 20, группы друг у друга ничего не отнимают");
    }

    #[tokio::test]
    async fn stopping_an_instance_loses_nothing_and_the_rest_take_over() {
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut workers = Vec::new();
        for id in 0..3 {
            workers.push(start_worker(connect().await, subject.clone(), "notify", id, Duration::from_millis(10), tx.clone()).await.unwrap());
        }
        let publisher = connect().await;
        publish_numbers(&publisher, &subject, 0..90).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        let leaving = workers.remove(0);
        leaving.stop().await;
        publish_numbers(&publisher, &subject, 90..120).await;
        let got = collect_handled(&mut rx, 120).await;
        let payloads: BTreeSet<&str> = got.iter().map(|h| h.payload.as_str()).collect();
        assert_eq!(payloads.len(), 120, "после остановки экземпляра ни одно из 120 событий не потеряно и не обработано дважды");
        let late_for_leaver = got.iter().filter(|h| h.worker == 0 && h.payload[1..].parse::<usize>().unwrap() >= 90).count();
        assert_eq!(late_for_leaver, 0, "остановленный экземпляр не должен получать события, опубликованные после его остановки");
        for worker in workers {
            worker.stop().await;
        }
    }

    #[tokio::test]
    async fn instances_on_a_slow_link_already_hear_events_when_start_returns() {
        let proxy = Proxy::start().await;
        proxy.set_latency(Duration::from_millis(150));
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (audit_tx, mut audit_rx) = mpsc::unbounded_channel();
        let url = proxy.url();
        let slow = || connect_to(&url, async_nats::ConnectOptions::new());
        let worker = start_worker(slow().await, subject.clone(), "notify", 7, Duration::ZERO, tx).await.expect("экземпляр");
        let _audit = start_audit(slow().await, subject.clone(), audit_tx).await.expect("аудит");
        publish_numbers(&connect().await, &subject, 0..1).await;
        assert_eq!(collect_handled(&mut rx, 1).await.len(), 1, "start_worker вернулся, а брокер ещё не знает про подписку экземпляра: нужен круг до брокера");
        let audited = timeout(Duration::from_secs(3), audit_rx.recv()).await;
        assert!(audited.is_ok(), "start_audit вернулся, а брокер ещё не знает про подписку аудита");
        worker.stop().await;
    }
}
