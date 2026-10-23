use async_nats::jetstream::Message;
use async_nats::jetstream::consumer::{Consumer, pull};
use futures::StreamExt;
use std::time::Duration;
use async_nats::jetstream::message::AckKind;

fn main() {}

async fn take(consumer: &Consumer<pull::Config>, count: usize, wait: Duration) -> Result<Vec<Message>, String> {
    let mut batch = consumer.fetch().max_messages(count).expires(wait).messages().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(message) = batch.next().await {
        out.push(message.map_err(|e| e.to_string())?);
    }
    Ok(out)
}
pub async fn handle_batch(
    consumer: &Consumer<pull::Config>,
    size: usize,
    wait: Duration,
    process: impl Fn(&str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut done = 0;
    for message in take(consumer, size, wait).await? {
        let payload = String::from_utf8_lossy(&message.payload).into_owned();
        match process(&payload) {
            Ok(()) => {
                message.ack().await.map_err(|e| e.to_string())?;
                done += 1;
            }
            Err(_) => message.ack_with(AckKind::Nak(Some(Duration::from_millis(100)))).await.map_err(|e| e.to_string())?,
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::AckPolicy;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use std::collections::HashMap;
    use std::sync::Mutex;

    async fn setup(scope: &Scope, payloads: &[&str]) -> Consumer<pull::Config> {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        for payload in payloads {
            js.publish(scope.subject("ev.x"), payload.to_string().into()).await.unwrap().await.unwrap();
        }
        stream
            .create_consumer(pull::Config { durable_name: Some("reader".to_string()), ack_policy: AckPolicy::Explicit, ack_wait: Duration::from_secs(10), ..Default::default() })
            .await
            .unwrap()
    }

    #[derive(Default)]
    struct Journal {
        attempts: Mutex<HashMap<String, u32>>,
        applied: Mutex<Vec<String>>,
    }

    impl Journal {
        fn process(&self, payload: &str, fail_first: &[&str]) -> Result<(), String> {
            let mut attempts = self.attempts.lock().unwrap();
            let n = attempts.entry(payload.to_string()).or_default();
            *n += 1;
            if *n == 1 && fail_first.contains(&payload) {
                return Err("база недоступна".to_string());
            }
            self.applied.lock().unwrap().push(payload.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_failure_after_the_event_arrived_does_not_lose_it() {
        let scope = Scope::new();
        let consumer = setup(&scope, &["e1", "e2", "e3"]).await;
        let journal = Journal::default();
        let started = std::time::Instant::now();
        while journal.applied.lock().unwrap().len() < 3 && started.elapsed() < Duration::from_secs(5) {
            handle_batch(&consumer, 10, Duration::from_millis(250), |p| journal.process(p, &["e2"])).await.expect("пачка");
        }
        let mut applied = journal.applied.lock().unwrap().clone();
        applied.sort();
        assert_eq!(applied, vec!["e1", "e2", "e3"], "все три события обработаны, в том числе то, на котором база споткнулась");
        assert_eq!(journal.attempts.lock().unwrap()["e2"], 2, "e2 пришло дважды: первая попытка упала, вторая прошла");
    }

    #[tokio::test]
    async fn the_count_is_of_acknowledged_events() {
        let scope = Scope::new();
        let consumer = setup(&scope, &["a", "b", "c"]).await;
        let journal = Journal::default();
        let first = handle_batch(&consumer, 10, Duration::from_millis(250), |p| journal.process(p, &["b"])).await.unwrap();
        assert_eq!(first, 2, "из трёх подтверждены два, b вернётся позже");
        let second = handle_batch(&consumer, 10, Duration::from_millis(600), |p| journal.process(p, &["b"])).await.unwrap();
        assert_eq!(second, 1, "b вернулся через паузу и прошёл");
    }

    #[tokio::test]
    async fn a_failed_event_waits_before_coming_back() {
        let scope = Scope::new();
        let consumer = setup(&scope, &["only"]).await;
        let journal = Journal::default();
        let started = std::time::Instant::now();
        handle_batch(&consumer, 1, Duration::from_millis(250), |p| journal.process(p, &["only"])).await.unwrap();
        let mut second = 0;
        while second == 0 && started.elapsed() < Duration::from_secs(3) {
            second = handle_batch(&consumer, 1, Duration::from_millis(250), |p| journal.process(p, &["only"])).await.unwrap();
        }
        assert_eq!(second, 1, "вторая попытка прошла");
        assert!(started.elapsed() >= Duration::from_millis(95), "событие вернулось раньше положенной паузы в 100 мс: {:?}", started.elapsed());
    }
}
