use async_nats::jetstream::consumer::{Consumer, pull};
use async_nats::jetstream::message::AckKind;
use futures::StreamExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

fn main() {}

pub type JobFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
pub type Handler = Arc<dyn Fn(String) -> JobFuture + Send + Sync>;

pub struct Worker {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

pub fn start_worker(consumer: Consumer<pull::Config>, handler: Handler) -> Worker {
    let (stop, mut stopped) = watch::channel(false);
    let task = tokio::spawn(async move {
        loop {
            if *stopped.borrow_and_update() {
                return;
            }
            let Ok(mut batch) = consumer.fetch().max_messages(5).expires(Duration::from_millis(300)).messages().await else { continue };
            while let Some(Ok(message)) = batch.next().await {
                if *stopped.borrow_and_update() {
                    message.ack_with(AckKind::Nak(None)).await.ok();
                    continue;
                }
                let work = handler(String::from_utf8_lossy(&message.payload).into_owned());
                tokio::pin!(work);
                let verdict = tokio::select! {
                    result = &mut work => if result.is_ok() { AckKind::Ack } else { AckKind::Nak(None) },
                    _ = stopped.changed() => AckKind::Nak(None),
                };
                message.ack_with(verdict).await.ok();
            }
        }
    });
    Worker { stop, task }
}

impl Worker {
    pub async fn stop(self) {
        let _ = self.stop.send(true);
        let _ = self.task.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::AckPolicy;
    use async_nats::jetstream::stream::{Config, RetentionPolicy, Stream};
    use course_support::nats::{Scope, connect, js, wait_until};
    use std::sync::Mutex;

    async fn setup(scope: &Scope, bodies: &[&str]) -> (async_nats::jetstream::Context, String, Stream, Consumer<pull::Config>) {
        let js = js(&connect().await);
        let name = scope.name("JOBS");
        scope.track(&name);
        let subject = scope.subject("jobs.run");
        let stream = js
            .create_stream(Config { name, subjects: vec![subject.clone()], retention: RetentionPolicy::WorkQueue, ..Default::default() })
            .await
            .expect("stream");
        for body in bodies {
            js.publish(subject.clone(), body.to_string().into()).await.unwrap().await.unwrap();
        }
        let consumer = stream
            .create_consumer(pull::Config {
                durable_name: Some("workers".to_string()),
                ack_policy: AckPolicy::Explicit,
                ack_wait: Duration::from_secs(30),
                ..Default::default()
            })
            .await
            .expect("consumer");
        (js, subject, stream, consumer)
    }

    fn handler(log: Arc<Mutex<Vec<String>>>) -> Handler {
        Arc::new(move |body: String| -> JobFuture {
            let log = Arc::clone(&log);
            Box::pin(async move {
                log.lock().unwrap().push(body.clone());
                if body == "slow" {
                    tokio::time::sleep(Duration::from_secs(20)).await;
                }
                Ok(())
            })
        })
    }

    async fn take_all(consumer: &Consumer<pull::Config>, within: Duration) -> Vec<(String, i64)> {
        let mut got = Vec::new();
        let mut batch = consumer.fetch().max_messages(10).expires(within).messages().await.expect("fetch");
        while let Some(Ok(message)) = batch.next().await {
            got.push((String::from_utf8_lossy(&message.payload).into_owned(), message.info().unwrap().delivered));
            message.ack().await.ok();
            if got.len() == 3 {
                break;
            }
        }
        got.sort();
        got
    }

    #[tokio::test]
    async fn stopping_returns_the_running_job_and_the_unstarted_ones_at_once() {
        let scope = Scope::new();
        let (_js, _subject, _stream, consumer) = setup(&scope, &["slow", "b", "c"]).await;
        let log = Arc::new(Mutex::new(Vec::new()));
        let worker = start_worker(consumer.clone(), handler(Arc::clone(&log)));
        wait_until("воркер начал медленную задачу", Duration::from_secs(5), || async { !log.lock().unwrap().is_empty() }).await;
        let started = std::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(3), worker.stop()).await.expect("stop не должен ждать окончания 20-секундной задачи");
        assert!(started.elapsed() < Duration::from_secs(2), "остановка заняла {:?}", started.elapsed());
        let got = take_all(&consumer, Duration::from_secs(2)).await;
        let names: Vec<&str> = got.iter().map(|(b, _)| b.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "slow"], "все три задачи должны быть доступны другому воркеру сразу, а не через 30 секунд ack_wait: вернулись {names:?}");
        assert_eq!(*log.lock().unwrap(), vec!["slow".to_string()], "b и c останавливающийся воркер даже не начинал");
    }

    #[tokio::test]
    async fn a_quiet_worker_stops_cleanly_and_takes_nothing_afterwards() {
        let scope = Scope::new();
        let (js, subject, stream, consumer) = setup(&scope, &["a", "b", "d"]).await;
        let log = Arc::new(Mutex::new(Vec::new()));
        let worker = start_worker(consumer.clone(), handler(Arc::clone(&log)));
        wait_until("три задачи выполнены", Duration::from_secs(5), || async { log.lock().unwrap().len() == 3 }).await;
        tokio::time::timeout(Duration::from_secs(3), worker.stop()).await.expect("stop");
        wait_until("выполненное подтверждено и удалено", Duration::from_secs(3), || async { stream.get_info().await.unwrap().state.messages == 0 }).await;
        js.publish(subject, "late".into()).await.unwrap().await.unwrap();
        tokio::time::sleep(Duration::from_millis(700)).await;
        let got = take_all(&consumer, Duration::from_secs(1)).await;
        assert_eq!(got, vec![("late".to_string(), 1)], "после stop воркер новых задач не берёт: поздняя задача должна быть нетронутой (первая доставка), а вышло {got:?}");
    }
}
