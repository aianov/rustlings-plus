// Сервис публикует событие «пост лайкнули» и не получает подтверждения: сеть мигнула. Он
// повторяет публикацию, и в stream теперь два одинаковых события, а пользователь получает
// два уведомления. JetStream умеет узнавать повтор: у сообщения есть заголовок `Nats-Msg-Id`,
// и в течение окна дубликатов (duplicate_window stream) второе сообщение с тем же значением
// не записывается, а подтверждается как дубликат с прежним номером. Цель: event_id строит
// стабильное имя события, publish_once публикует с этим именем и сообщает, записано ли
// событие или это дубликат. Имя события: `<вид>:<агрегат>:<версия>`.


use async_nats::HeaderMap;
use async_nats::jetstream::Context;

fn main() {}

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: String,
    pub aggregate: String,
    pub version: u64,
}

#[derive(Debug, PartialEq)]
pub struct Published {
    pub sequence: u64,
    pub duplicate: bool,
}

pub fn event_id(event: &Event) -> String {
    let _ = event;
    todo!()
}

pub async fn publish_once(js: &Context, subject: String, event: &Event, payload: Vec<u8>) -> Result<Published, String> {
    let _ = (js, subject, event, payload, HeaderMap::new());
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::Config;
    use course_support::nats::{Scope, connect, js};
    use std::time::Duration;

    fn event(kind: &str, aggregate: &str, version: u64) -> Event {
        Event { kind: kind.to_string(), aggregate: aggregate.to_string(), version }
    }

    async fn setup(scope: &Scope, window: Duration) -> (Context, async_nats::jetstream::stream::Stream) {
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = js
            .create_stream(Config { name, subjects: vec![scope.subject("ev.>")], duplicate_window: window, ..Default::default() })
            .await
            .unwrap();
        (js, stream)
    }

    #[test]
    fn the_id_names_the_logical_event() {
        assert_eq!(event_id(&event("post.liked", "post-7", 3)), "post.liked:post-7:3", "формат имени события");
        assert_ne!(event_id(&event("post.liked", "post-7", 3)), event_id(&event("post.liked", "post-7", 4)), "другая версия это другое событие");
    }

    #[tokio::test]
    async fn a_repeated_publish_is_recognised_and_stored_once() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Duration::from_secs(60)).await;
        let liked = event("post.liked", "post-7", 1);
        let first = publish_once(&js, scope.subject("ev.liked"), &liked, b"{}".to_vec()).await.unwrap();
        let again = publish_once(&js, scope.subject("ev.liked"), &liked, b"{}".to_vec()).await.unwrap();
        assert_eq!(first, Published { sequence: 1, duplicate: false }, "первая публикация записана");
        assert_eq!(again, Published { sequence: 1, duplicate: true }, "повтор распознан как дубликат с прежним номером");
        assert_eq!(stream.get_info().await.unwrap().state.messages, 1, "в stream одна запись");
    }

    #[tokio::test]
    async fn different_events_are_not_mistaken_for_duplicates() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Duration::from_secs(60)).await;
        for version in 1..=3 {
            let published = publish_once(&js, scope.subject("ev.liked"), &event("post.liked", "post-7", version), b"{}".to_vec()).await.unwrap();
            assert!(!published.duplicate, "версия {version} это новое событие");
        }
        let other = publish_once(&js, scope.subject("ev.liked"), &event("post.commented", "post-7", 1), b"{}".to_vec()).await.unwrap();
        assert!(!other.duplicate, "другой вид с той же версией тоже новое событие");
        assert_eq!(stream.get_info().await.unwrap().state.messages, 4, "четыре записи");
    }

    #[tokio::test]
    async fn the_stored_message_carries_the_id() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Duration::from_secs(60)).await;
        let sequence = publish_once(&js, scope.subject("ev.liked"), &event("post.liked", "p1", 9), b"x".to_vec()).await.unwrap().sequence;
        let raw = stream.get_raw_message(sequence).await.unwrap();
        let id = raw.headers.get(async_nats::header::NATS_MESSAGE_ID).map(|v| v.as_str().to_string());
        assert_eq!(id.as_deref(), Some("post.liked:p1:9"), "идентификатор лежит в заголовке Nats-Msg-Id");
    }

    #[tokio::test]
    async fn the_window_is_finite() {
        let scope = Scope::new();
        let (js, stream) = setup(&scope, Duration::from_secs(1)).await;
        let liked = event("post.liked", "post-7", 1);
        publish_once(&js, scope.subject("ev.liked"), &liked, b"{}".to_vec()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let later = publish_once(&js, scope.subject("ev.liked"), &liked, b"{}".to_vec()).await.unwrap();
        assert!(!later.duplicate, "окно дубликатов закрылось: брокер уже не помнит старое имя");
        assert_eq!(stream.get_info().await.unwrap().state.messages, 2, "это принципиально: защита от дублей конечна, и потребитель всё равно должен быть идемпотентным");
    }
}
