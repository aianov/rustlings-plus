// Каждый сервис соцсети публикует события, и без общего конверта потребителям пришлось бы гадать, что где лежит.
// Конверт: event_id (уникальный), type (факт в прошедшем времени, «post.created»), schema (версия формата payload,
// пока 1), version (номер изменения агрегата: у поста 1 при создании), occurred_at (миллисекунды, когда это случилось),
// aggregate_id и payload. Цель: post_created собирает такое событие, encode и decode переводят его в JSON и обратно,
// причём decode не пропускает событие без обязательных частей. Лишние поля в JSON его не смущают.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn main() {}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "one")]
    pub schema: u32,
    pub version: u64,
    pub occurred_at: u64,
    pub aggregate_id: String,
    pub payload: Value,
}

pub trait Ids {
    fn next_id(&self) -> String;
}

#[derive(Debug)]
pub enum DecodeError {
    Malformed(String),
    Invalid(&'static str),
}

pub fn post_created(ids: &dyn Ids, now: u64, post_id: &str, author_id: &str, text: &str) -> Event {
    let _ = (ids, now, post_id, author_id, text);
    todo!()
}

pub fn encode(event: &Event) -> Vec<u8> {
    let _ = event;
    todo!()
}

pub fn decode(bytes: &[u8]) -> Result<Event, DecodeError> {
    let _ = bytes;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Counter(AtomicU64);

    impl Ids for Counter {
        fn next_id(&self) -> String {
            format!("evt-{}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    fn sample() -> Event {
        post_created(&Counter(AtomicU64::new(0)), 1_700_000_000_000, "p1", "alice", "привет")
    }

    #[test]
    fn the_envelope_carries_everything_a_consumer_needs() {
        let event = sample();
        assert_eq!(event.kind, "post.created", "тип это факт в прошедшем времени");
        assert_eq!(event.aggregate_id, "p1", "агрегат события это пост, а не автор");
        assert_eq!((event.schema, event.version), (1, 1), "первая схема, первое изменение поста");
        assert_eq!(event.occurred_at, 1_700_000_000_000, "время берётся из аргумента, а не из часов");
        assert_eq!(event.payload["author_id"], "alice", "в payload автор");
        assert_eq!(event.payload["text"], "привет", "и текст");
        assert_eq!(event.payload["post_id"], "p1", "и идентификатор поста");
    }

    #[test]
    fn every_event_gets_its_own_id() {
        let ids = Counter(AtomicU64::new(0));
        let a = post_created(&ids, 1, "p1", "alice", "a");
        let b = post_created(&ids, 1, "p1", "alice", "a");
        assert_ne!(a.event_id, b.event_id, "два события, даже одинаковых по содержанию, различаются идентификатором");
    }

    #[test]
    fn json_has_exactly_the_agreed_keys_and_type_is_not_kind() {
        let value: Value = serde_json::from_slice(&encode(&sample())).expect("encode даёт JSON");
        let keys: BTreeSet<&str> = value.as_object().expect("объект").keys().map(String::as_str).collect();
        let want: BTreeSet<&str> = ["aggregate_id", "event_id", "occurred_at", "payload", "schema", "type", "version"].into();
        assert_eq!(keys, want, "в JSON ровно семь согласованных ключей, поле называется `type`");
        assert!(value["occurred_at"].is_u64() && value["version"].is_u64(), "числа остаются числами, а не строками");
    }

    #[test]
    fn decode_returns_what_encode_wrote() {
        let event = sample();
        assert_eq!(decode(&encode(&event)).expect("свой JSON читается"), event, "круг encode и decode ничего не теряет");
    }

    #[test]
    fn a_missing_schema_means_the_first_one_and_unknown_fields_are_ignored() {
        let raw = br#"{"event_id":"e1","type":"post.created","version":1,"occurred_at":5,"aggregate_id":"p1","payload":{},"trace_id":"abc"}"#;
        let event = decode(raw).expect("старый производитель не знал про schema, новый добавил trace_id");
        assert_eq!(event.schema, 1, "нет поля schema значит первая схема");
    }

    #[test]
    fn required_parts_cannot_be_empty() {
        let base = |event_id: &str, kind: &str, aggregate: &str, version: u64, schema: u32| {
            serde_json::to_vec(&json!({"event_id": event_id, "type": kind, "schema": schema, "version": version, "occurred_at": 1, "aggregate_id": aggregate, "payload": {}})).unwrap()
        };
        let cases = [
            (base("", "post.created", "p1", 1, 1), "event_id"),
            (base("e1", " ", "p1", 1, 1), "type"),
            (base("e1", "post.created", "", 1, 1), "aggregate_id"),
            (base("e1", "post.created", "p1", 0, 1), "version"),
            (base("e1", "post.created", "p1", 1, 0), "schema"),
        ];
        for (bytes, field) in cases {
            match decode(&bytes) {
                Err(DecodeError::Invalid(name)) => assert_eq!(name, field, "ошибка должна называть поле"),
                other => panic!("событие с пустым полем {field} принято или отвергнуто не так: {other:?}"),
            }
        }
    }

    #[test]
    fn broken_json_and_missing_fields_are_malformed() {
        assert!(matches!(decode(b"{oops"), Err(DecodeError::Malformed(_))), "не JSON");
        assert!(matches!(decode(br#"{"event_id":"e1"}"#), Err(DecodeError::Malformed(_))), "нет остальных обязательных частей");
        assert!(matches!(decode(b""), Err(DecodeError::Malformed(_))), "пустое тело");
    }
}
