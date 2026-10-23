// Читать stream можно с позицией, которую помнит брокер: это consumer. Для сервиса нужен
// durable pull consumer: у него постоянное имя, он помнит, до какого места дочитал, и ждёт
// явного подтверждения (ack) каждого сообщения. Сервис просит у него пачку, обрабатывает
// и подтверждает. Цель: ensure_consumer заводит или находит durable consumer с явными
// подтверждениями и заданным ack_wait; drain_batch берёт до `size` сообщений (ждёт не дольше
// `wait`), подтверждает каждое и возвращает их номера и тексты. Подтверждённое не должно
// приходить второй раз, а повторный вызов ensure_consumer не сбрасывает позицию.


use async_nats::jetstream::Message;
use async_nats::jetstream::consumer::{AckPolicy, Consumer, pull};
use async_nats::jetstream::stream::Stream;
use futures::StreamExt;
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub struct Delivered {
    pub sequence: u64,
    pub payload: String,
}

pub async fn ensure_consumer(stream: &Stream, durable: &str, ack_wait: Duration) -> Result<Consumer<pull::Config>, String> {
    let _ = (stream, durable, ack_wait, AckPolicy::Explicit, pull::Config::default());
    todo!()
}

pub async fn drain_batch(consumer: &Consumer<pull::Config>, size: usize, wait: Duration) -> Result<Vec<Delivered>, String> {
    let _ = (consumer, size, wait);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::Context;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};

    async fn setup(scope: &Scope, n: usize) -> (Context, Stream) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        for i in 1..=n {
            js.publish(scope.subject("ev.x"), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
        (js, stream)
    }

    fn numbers(items: &[Delivered]) -> Vec<u64> {
        items.iter().map(|d| d.sequence).collect()
    }

    #[tokio::test]
    async fn batches_come_in_order_and_each_message_once() {
        let scope = Scope::new();
        let (_js, stream) = setup(&scope, 10).await;
        let consumer = ensure_consumer(&stream, "reader", Duration::from_secs(5)).await.expect("consumer");
        let first = drain_batch(&consumer, 4, Duration::from_millis(300)).await.unwrap();
        let second = drain_batch(&consumer, 4, Duration::from_millis(300)).await.unwrap();
        let third = drain_batch(&consumer, 4, Duration::from_millis(300)).await.unwrap();
        assert_eq!(numbers(&first), vec![1, 2, 3, 4], "первая пачка");
        assert_eq!(numbers(&second), vec![5, 6, 7, 8], "вторая пачка продолжает с того же места");
        assert_eq!(numbers(&third), vec![9, 10], "в третьей остаток, меньше запрошенного");
        assert_eq!(first[0].payload, "e1", "текст сообщения");
    }

    #[tokio::test]
    async fn an_acknowledged_message_never_comes_back() {
        let scope = Scope::new();
        let (_js, stream) = setup(&scope, 6).await;
        let consumer = ensure_consumer(&stream, "reader", Duration::from_millis(500)).await.unwrap();
        assert_eq!(drain_batch(&consumer, 10, Duration::from_millis(300)).await.unwrap().len(), 6, "прочитали всё");
        tokio::time::sleep(Duration::from_millis(1300)).await;
        let again = drain_batch(&consumer, 10, Duration::from_millis(300)).await.unwrap();
        assert!(again.is_empty(), "ack_wait прошёл дважды, а подтверждённое не вернулось: {again:?}. Ты подтверждаешь каждое сообщение?");
    }

    #[tokio::test]
    async fn asking_for_the_consumer_again_keeps_the_position() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, 4).await;
        let before = ensure_consumer(&stream, "reader", Duration::from_secs(5)).await.unwrap();
        assert_eq!(numbers(&drain_batch(&before, 2, Duration::from_millis(300)).await.unwrap()), vec![1, 2], "прочитали две");
        let restarted = ensure_consumer(&stream, "reader", Duration::from_secs(5)).await.expect("тот же durable");
        js.publish(scope.subject("ev.x"), "e5".into()).await.unwrap().await.unwrap();
        let rest = drain_batch(&restarted, 10, Duration::from_millis(300)).await.unwrap();
        assert_eq!(numbers(&rest), vec![3, 4, 5], "после перезапуска чтение идёт с места остановки, а не с начала");
    }

    #[tokio::test]
    async fn two_durables_read_independently() {
        let scope = Scope::new();
        let (_js, stream) = setup(&scope, 3).await;
        let a = ensure_consumer(&stream, "a", Duration::from_secs(5)).await.unwrap();
        let b = ensure_consumer(&stream, "b", Duration::from_secs(5)).await.unwrap();
        assert_eq!(drain_batch(&a, 10, Duration::from_millis(300)).await.unwrap().len(), 3, "a получил все три");
        assert_eq!(drain_batch(&b, 10, Duration::from_millis(300)).await.unwrap().len(), 3, "и b тоже: у каждого consumer своя позиция");
    }
}
