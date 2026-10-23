// Два разных сервиса (notify и feed) читают один stream, и каждому нужны все события.
// Но у notify два экземпляра, и их работа должна делиться, а не дублироваться. У consumer
// есть имя: два сервиса со своими именами получают по полной копии, а два экземпляра одного
// сервиса, назвавшие один и тот же consumer, делят его поток. Цель: start_instance запускает
// экземпляр сервиса: он читает пачками, шлёт в `sink` пару (номер экземпляра, номер записи в
// stream) и подтверждает каждую. Останавливается экземпляр без потерь: начатая пачка
// дорабатывается, а позицию сервиса новый экземпляр потом подхватывает с того же места.


use async_nats::jetstream::consumer::{AckPolicy, Consumer, pull};
use async_nats::jetstream::stream::Stream;
use async_nats::jetstream::Message;
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

fn main() {}

pub struct Instance {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub async fn start_instance(stream: &Stream, service: &str, instance: usize, sink: mpsc::UnboundedSender<(usize, u64)>) -> Result<Instance, String> {
    let _ = (stream, service, instance, sink, AckPolicy::Explicit, pull::Config::default());
    todo!()
}

impl Instance {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::Context;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use std::collections::{BTreeMap, BTreeSet};
    use tokio::time::timeout;

    async fn setup(scope: &Scope, n: usize) -> (Context, Stream) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        publish(&js, scope, 1, n).await;
        (js, stream)
    }

    async fn publish(js: &Context, scope: &Scope, from: usize, to: usize) {
        for i in from..=to {
            js.publish(scope.subject("ev.x"), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
    }

    async fn gather(rx: &mut mpsc::UnboundedReceiver<(usize, u64)>, count: usize) -> Vec<(usize, u64)> {
        let mut got = Vec::new();
        while got.len() < count {
            match timeout(Duration::from_secs(4), rx.recv()).await {
                Ok(Some(item)) => got.push(item),
                _ => panic!("получено {} из {count}", got.len()),
            }
        }
        got
    }

    async fn quiet(rx: &mut mpsc::UnboundedReceiver<(usize, u64)>) -> bool {
        timeout(Duration::from_millis(600), rx.recv()).await.is_err()
    }

    #[tokio::test]
    async fn services_with_their_own_names_each_get_everything() {
        let scope = Scope::new();
        let (_js, stream) = setup(&scope, 20).await;
        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel();
        let (feed_tx, mut feed_rx) = mpsc::unbounded_channel();
        let notify = start_instance(&stream, "notify", 1, notify_tx).await.expect("notify");
        let feed = start_instance(&stream, "feed", 1, feed_tx).await.expect("feed");
        let a: BTreeSet<u64> = gather(&mut notify_rx, 20).await.into_iter().map(|(_, s)| s).collect();
        let b: BTreeSet<u64> = gather(&mut feed_rx, 20).await.into_iter().map(|(_, s)| s).collect();
        assert_eq!((a.len(), b.len()), (20, 20), "каждый сервис получил все 20 записей и ни одной дважды");
        notify.stop().await;
        feed.stop().await;
    }

    #[tokio::test]
    async fn instances_sharing_a_name_share_the_work() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, 0).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let first = start_instance(&stream, "notify", 1, tx.clone()).await.unwrap();
        let second = start_instance(&stream, "notify", 2, tx).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        publish(&js, &scope, 1, 60).await;
        let got = gather(&mut rx, 60).await;
        let unique: BTreeSet<u64> = got.iter().map(|(_, s)| *s).collect();
        assert_eq!(unique.len(), 60, "все 60 записей получены ровно по одному разу на два экземпляра");
        assert!(quiet(&mut rx).await, "после 60 записей сообщений больше нет: дублей не было");
        let mut by_instance: BTreeMap<usize, usize> = BTreeMap::new();
        for (instance, _) in &got {
            *by_instance.entry(*instance).or_default() += 1;
        }
        assert_eq!(by_instance.values().sum::<usize>(), 60, "{by_instance:?}");
        assert_eq!(by_instance.len(), 2, "работу делили оба экземпляра, а не один: {by_instance:?}");
        first.stop().await;
        second.stop().await;
    }

    #[tokio::test]
    async fn a_new_instance_continues_where_the_service_stopped() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, 5).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let old = start_instance(&stream, "notify", 1, tx.clone()).await.unwrap();
        gather(&mut rx, 5).await;
        old.stop().await;
        publish(&js, &scope, 6, 8).await;
        let fresh = start_instance(&stream, "notify", 2, tx).await.unwrap();
        let got: Vec<u64> = gather(&mut rx, 3).await.into_iter().map(|(_, s)| s).collect();
        assert_eq!(got, vec![6, 7, 8], "после рестарта читаются только новые записи, историю заново не поднимают");
        assert!(quiet(&mut rx).await, "и больше ничего");
        fresh.stop().await;
    }
}
