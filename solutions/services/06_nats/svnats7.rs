use async_nats::{Client, Subscriber};
use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

fn main() {}

pub async fn publish_json<T: Serialize>(client: &Client, subject: String, value: &T) -> Result<(), String> {
    let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    client.publish(subject, body.into()).await.map_err(|e| e.to_string())
}

pub async fn next_json<T: DeserializeOwned>(subscriber: &mut Subscriber, wait: Duration) -> Option<T> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let message = tokio::time::timeout_at(deadline, subscriber.next()).await.ok()??;
        if let Ok(value) = serde_json::from_slice(&message.payload) {
            return Some(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect};
    use serde::Deserialize;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Liked {
        post_id: u64,
        by: String,
    }

    fn liked(post_id: u64, by: &str) -> Liked {
        Liked { post_id, by: by.to_string() }
    }

    const WAIT: Duration = Duration::from_secs(2);

    #[tokio::test]
    async fn event_goes_through_the_broker_and_comes_back_equal() {
        let client = connect().await;
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let mut sub = client.subscribe(subject.clone()).await.unwrap();
        publish_json(&client, subject, &liked(7, "bob")).await.expect("публикация");
        let got: Option<Liked> = next_json(&mut sub, WAIT).await;
        assert_eq!(got, Some(liked(7, "bob")), "событие должно вернуться таким же");
    }

    #[tokio::test]
    async fn wildcard_subscription_keeps_the_order_and_ignores_other_subjects() {
        let client = connect().await;
        let scope = Scope::new();
        let mut sub = client.subscribe(scope.subject("post.>")).await.unwrap();
        publish_json(&client, scope.subject("post.liked"), &liked(1, "a")).await.unwrap();
        publish_json(&client, scope.subject("user.followed"), &liked(99, "stranger")).await.unwrap();
        publish_json(&client, scope.subject("post.commented"), &liked(2, "b")).await.unwrap();
        let first: Option<Liked> = next_json(&mut sub, WAIT).await;
        let second: Option<Liked> = next_json(&mut sub, WAIT).await;
        let third: Option<Liked> = next_json(&mut sub, Duration::from_millis(200)).await;
        assert_eq!((first, second), (Some(liked(1, "a")), Some(liked(2, "b"))), "приходят события post.*, по порядку");
        assert_eq!(third, None, "событие чужой темы не должно попасть в подписку, а тишина это None");
    }

    #[tokio::test]
    async fn garbage_in_the_stream_is_skipped() {
        let client = connect().await;
        let scope = Scope::new();
        let subject = scope.subject("post.liked");
        let mut sub = client.subscribe(subject.clone()).await.unwrap();
        client.publish(subject.clone(), "это не JSON".into()).await.unwrap();
        client.publish(subject.clone(), r#"{"post_id":"не число","by":"x"}"#.into()).await.unwrap();
        publish_json(&client, subject, &liked(5, "carol")).await.unwrap();
        let got: Option<Liked> = next_json(&mut sub, WAIT).await;
        assert_eq!(got, Some(liked(5, "carol")), "после мусора должно прийти нормальное событие, а не None");
    }

    #[tokio::test]
    async fn silence_is_none_after_about_the_wait() {
        let client = connect().await;
        let scope = Scope::new();
        let mut sub = client.subscribe(scope.subject("quiet")).await.unwrap();
        let started = std::time::Instant::now();
        let got: Option<Liked> = next_json(&mut sub, Duration::from_millis(300)).await;
        assert_eq!(got, None, "тишина это None");
        assert!(started.elapsed() < Duration::from_millis(1500), "next_json должен вернуться примерно через wait, а не висеть");
    }

    #[tokio::test]
    async fn wait_is_for_the_whole_call_not_for_each_garbage_message() {
        let client = connect().await;
        let scope = Scope::new();
        let subject = scope.subject("noise");
        let mut sub = client.subscribe(subject.clone()).await.unwrap();
        let noise = tokio::spawn({
            let client = client.clone();
            async move {
                for _ in 0..40 {
                    client.publish(subject.clone(), "мусор".into()).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        });
        let started = std::time::Instant::now();
        let got: Option<Liked> = next_json(&mut sub, Duration::from_millis(300)).await;
        assert_eq!(got, None, "подходящего события не было");
        assert!(started.elapsed() < Duration::from_millis(900), "мусор приходит каждые 25 мс, но срок ожидания общий на вызов: {:?}", started.elapsed());
        noise.abort();
    }

    #[tokio::test]
    async fn fifty_events_keep_their_order() {
        let client = connect().await;
        let scope = Scope::new();
        let subject = scope.subject("count");
        let mut sub = client.subscribe(subject.clone()).await.unwrap();
        for i in 0..50 {
            publish_json(&client, subject.clone(), &liked(i, "x")).await.unwrap();
        }
        for i in 0..50 {
            let got: Option<Liked> = next_json(&mut sub, WAIT).await;
            assert_eq!(got, Some(liked(i, "x")), "событие номер {i}");
        }
    }
}
