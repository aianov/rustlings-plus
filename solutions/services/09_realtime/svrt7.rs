use async_nats::jetstream::Context;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, pull};
use futures::StreamExt;
use std::time::Duration;

fn main() {}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub seq: u64,
    pub text: String,
}

pub async fn history_after(js: &Context, stream: &str, subject: &str, last_seq: u64, limit: usize) -> Result<Vec<Item>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let stream = js.get_stream(stream).await.map_err(|e| e.to_string())?;
    let consumer = stream
        .create_consumer(pull::Config {
            filter_subject: subject.to_string(),
            deliver_policy: DeliverPolicy::ByStartSequence { start_sequence: last_seq.saturating_add(1) },
            ack_policy: AckPolicy::None,
            inactive_threshold: Duration::from_secs(5),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    let name = consumer.cached_info().name.clone();
    let result = read_page(&consumer, limit).await;
    stream.delete_consumer(&name).await.ok();
    result
}

async fn read_page(consumer: &async_nats::jetstream::consumer::PullConsumer, limit: usize) -> Result<Vec<Item>, String> {
    let mut batch = consumer.fetch().max_messages(limit).expires(Duration::from_millis(300)).messages().await.map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    while let Some(message) = batch.next().await {
        let message = message.map_err(|e| e.to_string())?;
        let seq = message.info().map_err(|e| e.to_string())?.stream_sequence;
        items.push(Item { seq, text: String::from_utf8_lossy(&message.payload).into_owned() });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, js};

    async fn setup(scope: &Scope) -> (Context, Stream, String) {
        let js = js(&connect().await);
        let name = scope.name("CHAT");
        scope.track(&name);
        let stream = js.create_stream(Config { name: name.clone(), subjects: vec![scope.subject("chat.*")], ..Default::default() }).await.unwrap();
        (js, stream, name)
    }

    async fn say(js: &Context, scope: &Scope, user: &str, text: &str) -> u64 {
        js.publish(scope.subject(&format!("chat.{user}")), text.to_string().into()).await.unwrap().await.unwrap().sequence
    }

    #[tokio::test]
    async fn only_this_users_messages_after_the_given_number_come_back_with_stream_numbers() {
        let scope = Scope::new();
        let (js, _stream, name) = setup(&scope).await;
        let subject = scope.subject("chat.alice");
        let a1 = say(&js, &scope, "alice", "a1").await;
        say(&js, &scope, "bob", "b1").await;
        let a2 = say(&js, &scope, "alice", "a2").await;
        say(&js, &scope, "bob", "b2").await;
        let a3 = say(&js, &scope, "alice", "a3").await;
        let all = history_after(&js, &name, &subject, 0, 10).await.expect("история");
        assert_eq!(all, vec![Item { seq: a1, text: "a1".into() }, Item { seq: a2, text: "a2".into() }, Item { seq: a3, text: "a3".into() }], "только сообщения alice, с номерами stream (1, 3, 5), а не 0, 1, 2");
        let tail = history_after(&js, &name, &subject, a1, 10).await.unwrap();
        assert_eq!(tail.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), vec!["a2", "a3"], "после первого сообщения остаются два");
        let after_bobs = history_after(&js, &name, &subject, a3 - 1, 10).await.unwrap();
        assert_eq!(after_bobs.len(), 1, "номер между сообщениями (чужое посередине) тоже годится как точка отсчёта");
    }

    #[tokio::test]
    async fn nothing_new_means_an_empty_answer() {
        let scope = Scope::new();
        let (js, _stream, name) = setup(&scope).await;
        let subject = scope.subject("chat.alice");
        let last = say(&js, &scope, "alice", "a1").await;
        assert!(history_after(&js, &name, &subject, last, 10).await.unwrap().is_empty(), "клиент уже всё видел");
        assert!(history_after(&js, &name, &subject, last + 100, 10).await.unwrap().is_empty(), "номер из будущего это тоже пусто, а не ошибка");
        assert!(history_after(&js, &name, &scope.subject("chat.carol"), 0, 10).await.unwrap().is_empty(), "у carol сообщений не было");
        assert!(history_after(&js, &name, &subject, 0, 0).await.unwrap().is_empty(), "limit 0 это пустой ответ");
    }

    #[tokio::test]
    async fn the_limit_makes_pages_and_the_next_page_continues_from_the_last_number() {
        let scope = Scope::new();
        let (js, _stream, name) = setup(&scope).await;
        let subject = scope.subject("chat.alice");
        for i in 0..10 {
            say(&js, &scope, "alice", &format!("m{i}")).await;
            say(&js, &scope, "bob", "шум").await;
        }
        let mut seen = Vec::new();
        let mut last = 0;
        let mut pages = 0;
        loop {
            let page = history_after(&js, &name, &subject, last, 4).await.unwrap();
            assert!(page.len() <= 4, "страница не больше limit: {}", page.len());
            let Some(end) = page.last() else { break };
            last = end.seq;
            seen.extend(page.into_iter().map(|i| i.text));
            pages += 1;
            assert!(pages < 10, "постраничное чтение не заканчивается");
        }
        let expected: Vec<String> = (0..10).map(|i| format!("m{i}")).collect();
        assert_eq!(seen, expected, "все десять по порядку, без дыр и повторов между страницами");
        assert_eq!(pages, 3, "4 + 4 + 2");
    }

    #[tokio::test]
    async fn no_temporary_readers_are_left_in_the_broker() {
        let scope = Scope::new();
        let (js, mut stream, name) = setup(&scope).await;
        let subject = scope.subject("chat.alice");
        say(&js, &scope, "alice", "a1").await;
        for _ in 0..5 {
            history_after(&js, &name, &subject, 0, 10).await.unwrap();
        }
        let info = stream.info().await.unwrap();
        assert_eq!(info.state.consumer_count, 0, "каждый запрос истории оставил в stream читателя: за неделю их набегут тысячи");
    }

    #[tokio::test]
    async fn an_unknown_stream_is_an_error() {
        let js = js(&connect().await);
        assert!(history_after(&js, "NO_SUCH_STREAM_FOR_HISTORY", "x.y", 0, 5).await.is_err(), "нет такого stream: ошибка, а не пустой ответ");
    }
}
