use async_nats::Client;
use async_nats::jetstream::consumer::{AckPolicy, pull};
use async_nats::jetstream::{self, Context, stream};
use course_support::nats::Scope;
use futures::StreamExt;
use std::time::Duration;

fn main() {}

pub struct Env {
    js: Context,
    stream: String,
    subject: String,
    reader: String,
}

impl Env {
    pub async fn new(client: &Client, scope: &Scope) -> Env {
        let js = jetstream::new(client.clone());
        let stream = scope.name("EVENTS");
        let subject = scope.subject("events");
        scope.track(&stream);
        js.get_or_create_stream(stream::Config { name: stream.clone(), subjects: vec![format!("{subject}.>")], storage: stream::StorageType::Memory, ..Default::default() })
            .await
            .expect("stream");
        Env { js, stream, subject, reader: scope.name("reader") }
    }

    pub fn stream_name(&self) -> &str {
        &self.stream
    }

    pub async fn publish(&self, kind: &str, payload: String) {
        self.js.publish(format!("{}.{kind}", self.subject), payload.into_bytes().into()).await.expect("публикация").await.expect("подтверждение записи");
    }

    pub async fn read_all(&self) -> Vec<String> {
        let stream = self.js.get_stream(&self.stream).await.expect("stream есть");
        let consumer = stream
            .get_or_create_consumer(&self.reader, pull::Config { durable_name: Some(self.reader.clone()), ack_policy: AckPolicy::Explicit, ..Default::default() })
            .await
            .expect("читатель");
        let mut batch = consumer.fetch().max_messages(100).expires(Duration::from_millis(500)).messages().await.expect("пачка");
        let mut got = Vec::new();
        while let Some(message) = batch.next().await {
            let message = message.expect("сообщение");
            got.push(String::from_utf8_lossy(&message.payload).to_string());
            message.ack().await.expect("ack");
        }
        got
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{connect, wait_until};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    async fn scenario(env: Env, tag: &'static str, barrier: Arc<Barrier>) -> Vec<String> {
        for i in 0..5 {
            env.publish("created", format!("{tag}-{i}")).await;
        }
        barrier.wait().await;
        let mut got = env.read_all().await;
        got.sort();
        got
    }

    #[tokio::test]
    async fn two_tests_on_one_broker_do_not_hear_each_other() {
        let client = connect().await;
        let (scope_a, scope_b) = (Scope::new(), Scope::new());
        let (env_a, env_b) = (Env::new(&client, &scope_a).await, Env::new(&client, &scope_b).await);
        let barrier = Arc::new(Barrier::new(2));
        let (a, b) = tokio::join!(scenario(env_a, "a", barrier.clone()), scenario(env_b, "b", barrier));
        assert_eq!(a, (0..5).map(|i| format!("a-{i}")).collect::<Vec<_>>(), "первый тест должен прочитать ровно свои пять событий, а не чужие и не меньше");
        assert_eq!(b, (0..5).map(|i| format!("b-{i}")).collect::<Vec<_>>(), "второй тоже ровно свои");
    }

    #[tokio::test]
    async fn every_name_comes_from_the_scope() {
        let client = connect().await;
        let scope = Scope::new();
        let env = Env::new(&client, &scope).await;
        assert!(env.stream_name().starts_with(scope.prefix()), "имя stream {:?} должно начинаться с префикса теста {}", env.stream_name(), scope.prefix());
        assert!(env.subject.starts_with(scope.prefix()), "тема {:?} тоже", env.subject);
        assert!(env.reader.starts_with(scope.prefix()), "и читатель {:?}", env.reader);
    }

    #[tokio::test]
    async fn the_stream_is_removed_when_the_test_is_over() {
        let client = connect().await;
        let js = jetstream::new(client.clone());
        let name = {
            let scope = Scope::new();
            let env = Env::new(&client, &scope).await;
            env.publish("created", "x".to_string()).await;
            env.stream_name().to_string()
        };
        wait_until("stream убран с брокера после теста", Duration::from_secs(5), || async { js.get_stream(&name).await.is_err() }).await;
    }

    #[tokio::test]
    async fn a_reader_sees_events_in_the_order_they_were_published() {
        let client = connect().await;
        let scope = Scope::new();
        let env = Env::new(&client, &scope).await;
        for i in 0..3 {
            env.publish("created", format!("n{i}")).await;
        }
        assert_eq!(env.read_all().await, vec!["n0".to_string(), "n1".to_string(), "n2".to_string()], "своё окружение отдаёт всё своё по порядку");
        assert!(env.read_all().await.is_empty(), "прочитанное подтверждено и второй раз не приходит");
    }
}
