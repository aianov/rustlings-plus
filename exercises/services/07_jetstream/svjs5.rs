// Публикация события ушла на брокер и записалась, но подтверждение потерялось по дороге.
// Сервис видит таймаут и честно повторяет, а в stream теперь два одинаковых события. Дедупликация
// по Nats-Msg-Id в stream включена, но повторы её не используют. Цель: publish_with_retry
// делает до `attempts` попыток при таймауте (между попытками пауза 50 мс); все попытки
// одной публикации распознаются брокером как одно и то же событие; отказ брокера (не
// таймаут) повторять бессмысленно и возвращается сразу. Две разные публикации с одинаковым
// телом это два события.


use async_nats::jetstream::publish::PublishAck;
use rand::Rng;
use std::future::Future;
use std::time::Duration;

fn main() {}

#[derive(Debug, PartialEq)]
pub enum SendError {
    Timeout,
    Rejected(String),
}

#[derive(Debug, Clone)]
pub struct Attempt {
    pub number: u32,
    pub id: String,
    pub subject: String,
    pub payload: Vec<u8>,
}

pub async fn publish_with_retry<F, Fut>(send: F, subject: &str, payload: &[u8], attempts: u32) -> Result<PublishAck, SendError>
where
    F: Fn(Attempt) -> Fut,
    Fut: Future<Output = Result<PublishAck, SendError>>,
{
    let mut last = SendError::Timeout;
    for number in 1..=attempts {
        let id = format!("{:016x}", rand::thread_rng().r#gen::<u64>());
        let attempt = Attempt { number, id, subject: subject.to_string(), payload: payload.to_vec() };
        match send(attempt).await {
            Ok(ack) => return Ok(ack),
            Err(SendError::Timeout) => last = SendError::Timeout,
            Err(rejected) => return Err(rejected),
        }
        if number < attempts {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::HeaderMap;
    use async_nats::jetstream::Context;
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, js};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    async fn setup(scope: &Scope) -> (Context, Stream) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js.create_stream(Config { name, subjects: vec![scope.subject("ev.>")], ..Default::default() }).await.unwrap();
        (js, stream)
    }

    async fn store(js: &Context, attempt: Attempt) -> Result<PublishAck, SendError> {
        let mut headers = HeaderMap::new();
        headers.insert("Nats-Msg-Id", attempt.id.as_str());
        let future = js.publish_with_headers(attempt.subject, headers, attempt.payload.into()).await.map_err(|e| SendError::Rejected(e.to_string()))?;
        future.await.map_err(|e| SendError::Rejected(e.to_string()))
    }

    async fn messages(stream: &Stream) -> u64 {
        stream.get_info().await.unwrap().state.messages
    }

    #[tokio::test]
    async fn a_lost_ack_does_not_create_a_second_event() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        let calls = Arc::new(AtomicU32::new(0));
        let counter = calls.clone();
        let send = |attempt: Attempt| {
            let js = js.clone();
            let counter = counter.clone();
            async move {
                let stored = store(&js, attempt).await;
                if counter.fetch_add(1, Ordering::SeqCst) == 0 { Err(SendError::Timeout) } else { stored }
            }
        };
        let ack = publish_with_retry(send, &scope.subject("ev.liked"), b"like", 3).await.expect("вторая попытка проходит");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "две попытки: первая потеряла подтверждение");
        assert!(ack.duplicate, "брокер распознал вторую попытку как дубликат первой");
        assert_eq!(messages(&stream).await, 1, "событие записано один раз");
    }

    #[tokio::test]
    async fn two_separate_publications_with_the_same_body_are_two_events() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope).await;
        for _ in 0..2 {
            let send = |attempt: Attempt| {
                let js = js.clone();
                async move { store(&js, attempt).await }
            };
            publish_with_retry(send, &scope.subject("ev.liked"), b"same body", 3).await.unwrap();
        }
        assert_eq!(messages(&stream).await, 2, "одинаковое тело ещё не повод считать публикации одним событием");
    }

    #[tokio::test]
    async fn a_rejection_is_not_retried_and_timeouts_give_up_after_the_limit() {
        let calls = Arc::new(AtomicU32::new(0));
        let counter = calls.clone();
        let send = move |_attempt: Attempt| {
            counter.fetch_add(1, Ordering::SeqCst);
            async { Err::<PublishAck, _>(SendError::Rejected("stream not found".to_string())) }
        };
        let result = publish_with_retry(send, "x", b"y", 5).await;
        assert_eq!(result, Err(SendError::Rejected("stream not found".to_string())), "отказ возвращается как есть");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "отказ повторять бессмысленно");
        let calls = Arc::new(AtomicU32::new(0));
        let counter = calls.clone();
        let send = move |_attempt: Attempt| {
            counter.fetch_add(1, Ordering::SeqCst);
            async { Err::<PublishAck, _>(SendError::Timeout) }
        };
        let started = std::time::Instant::now();
        assert_eq!(publish_with_retry(send, "x", b"y", 4).await, Err(SendError::Timeout), "все попытки истекли");
        assert_eq!(calls.load(Ordering::SeqCst), 4, "ровно четыре попытки");
        assert!(started.elapsed() >= Duration::from_millis(150), "между попытками пауза 50 мс");
    }

    #[tokio::test]
    async fn every_attempt_of_one_publication_has_the_same_id_and_different_publications_differ() {
        let ids = Arc::new(std::sync::Mutex::new(Vec::new()));
        for _ in 0..2 {
            let ids = ids.clone();
            let send = move |attempt: Attempt| {
                ids.lock().unwrap().push((attempt.number, attempt.id.clone()));
                async { Err::<PublishAck, _>(SendError::Timeout) }
            };
            let _ = publish_with_retry(send, "x", b"y", 3).await;
        }
        let ids = ids.lock().unwrap();
        assert_eq!(ids.len(), 6, "по три попытки, дважды");
        assert!(ids[0].1 == ids[1].1 && ids[1].1 == ids[2].1, "в пределах одной публикации идентификатор один: {ids:?}");
        assert_ne!(ids[0].1, ids[3].1, "у разных публикаций идентификаторы разные");
    }
}
