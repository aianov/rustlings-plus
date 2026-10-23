// В соцсети три вида данных с разной судьбой. Журнал событий хранит последние записи, пока их
// не станет слишком много, и ему нужен потолок. Копия события для сервиса, который может быть
// выключен, нужна, пока кто-нибудь её не обработал, а если никто не подписан, хранить нечего.
// Очередь заданий хранит задание до первого подтверждения и отдаёт его одному исполнителю.
// Цель: stream_config для каждого вида даёт настройки stream, при которых брокер ведёт себя
// именно так. Журнал хранит не больше `max_messages` записей и не дольше 7 суток.


use async_nats::jetstream::stream::{Config, RetentionPolicy, StorageType};
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Journal { max_messages: i64 },
    Interest,
    WorkQueue,
}

pub fn stream_config(kind: Kind, name: &str, subjects: Vec<String>) -> Config {
    let _ = (kind, name, subjects, RetentionPolicy::Limits, StorageType::File, Duration::ZERO);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::Context;
    use async_nats::jetstream::consumer::{AckPolicy, pull};
    use async_nats::jetstream::stream::Stream;
    use course_support::nats::{Scope, connect, js, wait_until};
    use futures::StreamExt;

    async fn setup(scope: &Scope, kind: Kind) -> (Context, Stream) {
        let js = js(&connect().await);
        let name = scope.name("S");
        scope.track(&name);
        let stream = js.create_stream(stream_config(kind, &name, vec![scope.subject("ev.>")])).await.expect("stream создался");
        (js, stream)
    }

    async fn publish(js: &Context, scope: &Scope, n: usize) {
        for i in 0..n {
            js.publish(scope.subject("ev.x"), format!("e{i}").into()).await.unwrap().await.unwrap();
        }
    }

    async fn messages(stream: &Stream) -> u64 {
        stream.get_info().await.unwrap().state.messages
    }

    async fn consume_all(stream: &Stream, durable: &str, filter: &str) {
        let consumer = stream
            .create_consumer(pull::Config {
                durable_name: Some(durable.to_string()),
                ack_policy: AckPolicy::Explicit,
                filter_subject: filter.to_string(),
                ..Default::default()
            })
            .await
            .expect("consumer");
        let mut batch = consumer.fetch().max_messages(50).expires(std::time::Duration::from_millis(300)).messages().await.unwrap();
        while let Some(message) = batch.next().await {
            message.unwrap().ack().await.unwrap();
        }
    }

    #[test]
    fn the_journal_has_a_ceiling_in_count_and_time() {
        let config = stream_config(Kind::Journal { max_messages: 100 }, "J", vec!["j.>".to_string()]);
        assert_eq!(config.max_messages, 100, "потолок по числу записей");
        assert_eq!(config.max_age, Duration::from_secs(7 * 24 * 3600), "и по возрасту: 7 суток");
        assert_eq!(config.storage, StorageType::File, "хранится на диске");
        let interest = stream_config(Kind::Interest, "I", vec!["i.>".to_string()]);
        assert_eq!(interest.retention, RetentionPolicy::Interest, "интерес");
        let queue = stream_config(Kind::WorkQueue, "W", vec![]);
        assert_eq!(queue.retention, RetentionPolicy::WorkQueue, "очередь");
        for (name, config) in [("интерес", &interest), ("очередь", &queue)] {
            assert_eq!(config.storage, StorageType::File, "{name}: хранится на диске");
            assert_eq!(config.max_age, Duration::ZERO, "{name}: потолка по возрасту нет, запись живёт, пока её не прочитают");
        }
    }

    #[tokio::test]
    async fn the_journal_forgets_the_oldest_when_full() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Kind::Journal { max_messages: 5 }).await;
        publish(&js, &scope, 8).await;
        let state = stream.get_info().await.unwrap().state;
        assert_eq!((state.messages, state.first_sequence, state.last_sequence), (5, 4, 8), "осталось 5 последних: записи с 4 по 8");
    }

    #[tokio::test]
    async fn interest_keeps_a_message_only_while_somebody_still_has_to_read_it() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Kind::Interest).await;
        publish(&js, &scope, 3).await;
        assert_eq!(messages(&stream).await, 0, "подписчиков нет: хранить нечего");
        let consumer = stream
            .create_consumer(pull::Config { durable_name: Some("reader".to_string()), ack_policy: AckPolicy::Explicit, ..Default::default() })
            .await
            .unwrap();
        publish(&js, &scope, 3).await;
        assert_eq!(messages(&stream).await, 3, "появился читатель: записи ждут его подтверждения");
        let mut batch = consumer.fetch().max_messages(10).expires(Duration::from_millis(300)).messages().await.unwrap();
        while let Some(message) = batch.next().await {
            message.unwrap().ack().await.unwrap();
        }
        wait_until("все подтверждены и удалены", Duration::from_secs(3), || async { messages(&stream).await == 0 }).await;
    }

    #[tokio::test]
    async fn a_work_queue_drops_a_job_after_its_ack_and_gives_it_to_one_worker() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Kind::WorkQueue).await;
        publish(&js, &scope, 3).await;
        assert_eq!(messages(&stream).await, 3, "задания ждут исполнителя");
        consume_all(&stream, "worker", "").await;
        wait_until("подтверждённые задания удалены", Duration::from_secs(3), || async { messages(&stream).await == 0 }).await;
        let second = stream
            .create_consumer(pull::Config { durable_name: Some("second".to_string()), ack_policy: AckPolicy::Explicit, ..Default::default() })
            .await;
        assert!(second.is_err(), "очередь заданий не отдаёт одно задание двум консьюмерам: второй консьюмер на те же темы должен быть отвергнут");
    }
}
