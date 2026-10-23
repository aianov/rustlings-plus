use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::stream::Stream;
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

fn main() {}

pub struct Reader {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub async fn start_reader(stream: &Stream, service: &str, sink: mpsc::UnboundedSender<u64>) -> Result<Reader, String> {
    let consumer = stream
        .get_or_create_consumer(
            service,
            pull::Config { durable_name: Some(service.to_string()), ack_policy: AckPolicy::Explicit, ..Default::default() },
        )
        .await
        .map_err(|e| e.to_string())?;
    let (stop, mut stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        loop {
            if stopped.try_recv().is_ok() {
                return;
            }
            let Ok(mut batch) = consumer.fetch().max_messages(10).expires(Duration::from_millis(200)).messages().await else { continue };
            while let Some(Ok(message)) = batch.next().await {
                if let Ok(info) = message.info() {
                    sink.send(info.stream_sequence).ok();
                }
                message.ack().await.ok();
            }
        }
    });
    Ok(Reader { stop, task })
}

impl Reader {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.task.await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use tokio::time::timeout;

    async fn read(rx: &mut mpsc::UnboundedReceiver<u64>, count: usize) -> Vec<u64> {
        let mut got = Vec::new();
        while got.len() < count {
            match timeout(Duration::from_secs(4), rx.recv()).await {
                Ok(Some(item)) => got.push(item),
                _ => panic!("получено {} из {count}", got.len()),
            }
        }
        got
    }

    #[tokio::test]
    async fn a_restart_does_not_replay_history() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        for i in 1..=5 {
            js.publish(scope.subject("ev.x"), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        let first = start_reader(&stream, "notify", tx.clone()).await.expect("первый запуск");
        assert_eq!(read(&mut rx, 5).await, vec![1, 2, 3, 4, 5], "первый запуск читает историю");
        first.stop().await;
        for i in 6..=8 {
            js.publish(scope.subject("ev.x"), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
        let second = start_reader(&stream, "notify", tx).await.expect("рестарт");
        assert_eq!(read(&mut rx, 3).await, vec![6, 7, 8], "после рестарта пришли только новые записи");
        assert!(timeout(Duration::from_millis(600), rx.recv()).await.is_err(), "старые уведомления повторно не приходят");
        second.stop().await;
    }

    #[tokio::test]
    async fn many_restarts_leave_one_consumer() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let mut stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        for _ in 0..4 {
            let reader = start_reader(&stream, "notify", tx.clone()).await.unwrap();
            reader.stop().await;
        }
        let info = stream.info().await.unwrap();
        assert_eq!(info.state.consumer_count, 1, "четыре запуска, а consumer должен быть один, у сервиса одно постоянное имя");
        let other = start_reader(&stream, "feed", tx).await.unwrap();
        other.stop().await;
        assert_eq!(stream.info().await.unwrap().state.consumer_count, 2, "другому сервису свой consumer");
    }
}
