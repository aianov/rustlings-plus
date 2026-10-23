use async_nats::jetstream::Context;
use async_nats::jetstream::stream::{Config, Stream};

fn main() {}

pub async fn create_events_stream(js: &Context, name: &str, subjects: Vec<String>) -> Result<Stream, String> {
    js.create_stream(Config { name: name.to_string(), subjects, ..Default::default() }).await.map_err(|e| e.to_string())
}

pub async fn publish_event(js: &Context, subject: String, payload: Vec<u8>) -> Result<u64, String> {
    let ack = js.publish(subject, payload.into()).await.map_err(|e| e.to_string())?;
    Ok(ack.await.map_err(|e| e.to_string())?.sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_support::nats::{Scope, connect, js};

    async fn messages_in(stream: &Stream) -> u64 {
        stream.get_info().await.expect("info").state.messages
    }

    #[tokio::test]
    async fn the_stream_remembers_what_was_published() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = create_events_stream(&js, &name, vec![scope.subject("post.>")]).await.expect("stream");
        for i in 1..=3u64 {
            let sequence = publish_event(&js, scope.subject("post.liked"), format!("e{i}").into_bytes()).await.expect("публикация");
            assert_eq!(sequence, i, "номера записей в stream идут подряд с единицы");
            assert_eq!(messages_in(&stream).await, i, "когда publish_event вернулся, запись уже лежит в stream");
        }
    }

    #[tokio::test]
    async fn one_stream_covers_several_subjects_and_nothing_else() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let name = scope.name("EVENTS");
        scope.track(&name);
        let stream = create_events_stream(&js, &name, vec![scope.subject("post.>"), scope.subject("user.followed")]).await.unwrap();
        assert!(publish_event(&js, scope.subject("post.created.alice"), b"a".to_vec()).await.is_ok(), "тема под post.>");
        assert!(publish_event(&js, scope.subject("user.followed"), b"b".to_vec()).await.is_ok(), "вторая тема");
        assert_eq!(stream.get_info().await.unwrap().config.subjects.len(), 2, "в stream записаны обе темы");
        let outside = publish_event(&js, scope.subject("photo.uploaded"), b"c".to_vec()).await;
        assert!(outside.is_err(), "тему не покрывает ни один stream: это ошибка, а не тишина");
        assert_eq!(messages_in(&stream).await, 2, "чужая тема в stream не попала");
    }

    #[tokio::test]
    async fn sequences_are_per_stream() {
        let scope = Scope::new();
        let js = js(&connect().await);
        let (first, second) = (scope.name("A"), scope.name("B"));
        scope.track(&first);
        scope.track(&second);
        create_events_stream(&js, &first, vec![scope.subject("a.>")]).await.unwrap();
        create_events_stream(&js, &second, vec![scope.subject("b.>")]).await.unwrap();
        publish_event(&js, scope.subject("a.x"), b"1".to_vec()).await.unwrap();
        publish_event(&js, scope.subject("a.x"), b"2".to_vec()).await.unwrap();
        let in_second = publish_event(&js, scope.subject("b.x"), b"3".to_vec()).await.unwrap();
        assert_eq!(in_second, 1, "у каждого stream своя нумерация");
    }
}
