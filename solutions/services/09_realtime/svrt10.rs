use async_nats::jetstream::Context;
use async_nats::{Client, HeaderMap};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Mutex;

fn main() {}

const TYPING_GAP_MS: u64 = 3_000;

pub struct Chat {
    client: Client,
    js: Context,
    prefix: String,
    last_typing: Mutex<HashMap<(String, String), u64>>,
}

fn valid_word(word: &str) -> bool {
    !word.is_empty() && word.len() <= 32 && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Chat {
    pub fn new(client: Client, prefix: &str) -> Chat {
        Chat { js: async_nats::jetstream::new(client.clone()), client, prefix: prefix.to_string(), last_typing: Mutex::new(HashMap::new()) }
    }

    pub fn message_subject(&self, room: &str) -> String {
        format!("{}.room.{room}.msg", self.prefix)
    }

    pub fn typing_subject(&self, room: &str) -> String {
        format!("{}.room.{room}.typing", self.prefix)
    }

    pub async fn send_message(&self, room: &str, user: &str, client_msg_id: &str, text: &str) -> Result<u64, String> {
        if !valid_word(room) || !valid_word(user) || client_msg_id.is_empty() {
            return Err("комната, пользователь и идентификатор сообщения должны быть словами".to_string());
        }
        let mut headers = HeaderMap::new();
        headers.insert(async_nats::header::NATS_MESSAGE_ID, client_msg_id);
        let body = json!({"user": user, "text": text}).to_string();
        let ack = self.js.publish_with_headers(self.message_subject(room), headers, body.into()).await.map_err(|e| e.to_string())?;
        Ok(ack.await.map_err(|e| e.to_string())?.sequence)
    }

    pub async fn typing(&self, now_ms: u64, room: &str, user: &str) -> Result<bool, String> {
        if !valid_word(room) || !valid_word(user) {
            return Err("комната и пользователь должны быть словами".to_string());
        }
        {
            let mut last = self.last_typing.lock().unwrap();
            let key = (room.to_string(), user.to_string());
            if last.get(&key).is_some_and(|before| now_ms < before + TYPING_GAP_MS) {
                return Ok(false);
            }
            last.insert(key, now_ms);
        }
        self.client.publish(self.typing_subject(room), json!({"user": user}).to_string().into()).await.map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_nats::jetstream::stream::{Config, Stream};
    use course_support::nats::{Scope, connect, expect_silence, listen, recv, text};
    use std::time::Duration;

    async fn setup(scope: &Scope) -> (Chat, Stream) {
        let client = connect().await;
        let chat = Chat::new(client.clone(), scope.prefix());
        let name = scope.name("CHAT");
        scope.track(&name);
        let stream = async_nats::jetstream::new(client)
            .create_stream(Config { name, subjects: vec![scope.subject("room.*.msg")], ..Default::default() })
            .await
            .unwrap();
        (chat, stream)
    }

    async fn stored(stream: &mut Stream) -> u64 {
        stream.info().await.unwrap().state.messages
    }

    #[tokio::test]
    async fn a_message_is_stored_with_its_author_and_gets_a_stream_number() {
        let scope = Scope::new();
        let (chat, mut stream) = setup(&scope).await;
        let first = chat.send_message("r1", "alice", "c-1", "привет").await.expect("отправка");
        let second = chat.send_message("r1", "bob", "c-2", "здравствуй").await.unwrap();
        assert!(second > first, "номера растут: {first}, {second}");
        assert_eq!(stored(&mut stream).await, 2, "оба сообщения в stream");
        let raw = stream.get_raw_message(first).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&raw.payload).unwrap();
        assert_eq!((body["user"].as_str(), body["text"].as_str()), (Some("alice"), Some("привет")), "в теле автор и текст");
    }

    #[tokio::test]
    async fn a_retry_with_the_same_client_id_is_one_message() {
        let scope = Scope::new();
        let (chat, mut stream) = setup(&scope).await;
        let first = chat.send_message("r1", "alice", "c-1", "привет").await.unwrap();
        let again = chat.send_message("r1", "alice", "c-1", "привет").await.unwrap();
        assert_eq!(first, again, "повтор с тем же идентификатором возвращает тот же номер");
        assert_eq!(stored(&mut stream).await, 1, "копии нет");
        let twin = chat.send_message("r1", "alice", "c-2", "привет").await.unwrap();
        assert_ne!(twin, first, "тот же текст с новым идентификатором это новое сообщение");
        assert_eq!(stored(&mut stream).await, 2, "два сообщения");
    }

    #[tokio::test]
    async fn typing_is_seen_live_but_never_stored() {
        let scope = Scope::new();
        let (chat, mut stream) = setup(&scope).await;
        let watcher = connect().await;
        let mut live = listen(&watcher, chat.typing_subject("r1")).await;
        assert_eq!(chat.typing(1_000, "r1", "alice").await, Ok(true), "первое «печатает» уходит");
        let seen = recv(&mut live, Duration::from_secs(2)).await;
        assert!(text(&seen).contains("alice"), "слушатель увидел, кто печатает: {}", text(&seen));
        assert_eq!(stored(&mut stream).await, 0, "«печатает» в stream не попадает");
        chat.send_message("r1", "alice", "c-1", "готово").await.unwrap();
        let late = connect().await;
        let mut latecomer = listen(&late, chat.typing_subject("r1")).await;
        expect_silence(&mut latecomer, Duration::from_millis(300)).await;
        assert_eq!(stored(&mut stream).await, 1, "сообщение хранится для тех, кто придёт позже, а «печатает» для них пропало");
    }

    #[tokio::test]
    async fn typing_is_throttled_per_room_and_user() {
        let scope = Scope::new();
        let (chat, _stream) = setup(&scope).await;
        let watcher = connect().await;
        let mut live = listen(&watcher, scope.wildcard()).await;
        let mut published = 0;
        for now in (1_000..3_900).step_by(29) {
            if chat.typing(now, "r1", "alice").await.unwrap() {
                published += 1;
            }
        }
        assert_eq!(published, 1, "сто нажатий клавиш за три секунды дают одно событие");
        assert_eq!(chat.typing(4_000, "r1", "alice").await, Ok(true), "ровно через 3 секунды снова можно");
        assert_eq!(chat.typing(1_500, "r1", "bob").await, Ok(true), "другой пользователь в той же комнате не мешает");
        assert_eq!(chat.typing(1_500, "r2", "alice").await, Ok(true), "та же пользовательница в другой комнате тоже");
        let mut seen = 0;
        while course_support::nats::try_recv(&mut live, Duration::from_millis(300)).await.is_some() {
            seen += 1;
        }
        assert_eq!(seen, 4, "в брокер ушло ровно четыре события");
    }

    #[tokio::test]
    async fn hostile_names_are_refused() {
        let scope = Scope::new();
        let (chat, mut stream) = setup(&scope).await;
        for room in ["", "*", ">", "a.b", "a b"] {
            assert!(chat.send_message(room, "alice", "c-1", "x").await.is_err(), "комната {room:?} не слово");
            assert!(chat.typing(1, room, "alice").await.is_err(), "и для «печатает» комната {room:?} не годится");
        }
        assert!(chat.send_message("r1", "al.ice", "c-1", "x").await.is_err(), "пользователь с точкой не годится");
        assert_eq!(stored(&mut stream).await, 0, "ничего не опубликовано");
    }
}
