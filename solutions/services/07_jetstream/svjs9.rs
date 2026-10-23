use async_nats::jetstream::Message;
use async_nats::jetstream::message::AckKind;
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Done,
    Retry(Duration),
    Poison,
}

pub async fn settle(message: &Message, outcome: Outcome) -> Result<(), String> {
    let kind = match outcome {
        Outcome::Done => AckKind::Ack,
        Outcome::Retry(after) if after.is_zero() => AckKind::Nak(None),
        Outcome::Retry(after) => AckKind::Nak(Some(after)),
        Outcome::Poison => AckKind::Term,
    };
    message.ack_with(kind).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::consumer::{AckPolicy, Consumer, pull};
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, js};
    use futures::StreamExt;
    use std::time::Instant;

    async fn setup(scope: &Scope) -> (Stream, Consumer<pull::Config>) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        js.publish(scope.subject("ev.x"), "one".into()).await.unwrap().await.unwrap();
        let consumer = stream
            .create_consumer(pull::Config {
                durable_name: Some("reader".to_string()),
                ack_policy: AckPolicy::Explicit,
                ack_wait: Duration::from_millis(500),
                ..Default::default()
            })
            .await
            .unwrap();
        (stream, consumer)
    }

    async fn next(consumer: &Consumer<pull::Config>, wait_ms: u64) -> Option<Message> {
        let mut batch = consumer.fetch().max_messages(1).expires(Duration::from_millis(wait_ms)).messages().await.unwrap();
        batch.next().await.map(|m| m.unwrap())
    }

    #[tokio::test]
    async fn done_means_never_again() {
        let scope = Scope::new();
        let (_stream, consumer) = setup(&scope).await;
        let message = next(&consumer, 500).await.expect("сообщение");
        settle(&message, Outcome::Done).await.unwrap();
        assert!(next(&consumer, 1300).await.is_none(), "подтверждённое сообщение не возвращается даже после ack_wait");
    }

    #[tokio::test]
    async fn retry_comes_back_later_not_at_once() {
        let scope = Scope::new();
        let (_stream, consumer) = setup(&scope).await;
        let message = next(&consumer, 500).await.expect("сообщение");
        let started = Instant::now();
        settle(&message, Outcome::Retry(Duration::from_millis(400))).await.unwrap();
        assert!(next(&consumer, 200).await.is_none(), "через 200 мс его ещё нет: пауза 400 мс");
        let again = next(&consumer, 1500).await.expect("повторная доставка");
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(380), "вернулось слишком рано: {waited:?}");
        assert_eq!(again.info().unwrap().delivered, 2, "это вторая доставка");
        settle(&again, Outcome::Done).await.unwrap();
    }

    #[tokio::test]
    async fn retry_without_a_pause_returns_at_once() {
        let scope = Scope::new();
        let (_stream, consumer) = setup(&scope).await;
        let message = next(&consumer, 500).await.unwrap();
        settle(&message, Outcome::Retry(Duration::ZERO)).await.unwrap();
        let again = next(&consumer, 300).await.expect("сразу же вернулось");
        assert_eq!(again.info().unwrap().delivered, 2, "вторая доставка");
        settle(&again, Outcome::Done).await.unwrap();
    }

    #[tokio::test]
    async fn poison_is_never_delivered_again() {
        let scope = Scope::new();
        let (mut stream, consumer) = setup(&scope).await;
        let message = next(&consumer, 500).await.unwrap();
        settle(&message, Outcome::Poison).await.unwrap();
        assert!(next(&consumer, 1300).await.is_none(), "отвергнутое сообщение не возвращается даже после ack_wait");
        assert_eq!(stream.info().await.unwrap().state.messages, 1, "из stream сообщение не удаляется: оно просто больше не доставляется этому consumer");
    }
}
