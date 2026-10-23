// Событие ленты бывает одним из нескольких видов (лайк, комментарий или заметка), а ещё несёт произвольные метки. В схеме для
// первого есть oneof, для второго map. Эти конструкции сложнее обычных полей: у oneof нет своего номера, а у map нет порядка.
// Но кэш и подписи по байтам требуют, чтобы одно и то же событие всегда кодировалось одинаково. Схема:
//     message Like { uint64 post = 1; }   message Comment { uint64 post = 1; string text = 2; }
//     message Event { uint64 at = 1; oneof payload { Like like = 2; Comment comment = 3; string note = 4; } map<string, string> labels = 5; }
// TODO: Опиши Like, Comment и Event (вместе с модулем event для oneof) так, чтобы тесты прошли.

#![allow(dead_code)]
use prost::Message;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Message)]
struct Like {
    #[prost(uint64, tag = "1")]
    post: u64,
}

#[derive(Clone, PartialEq, Message)]
struct Comment {
    #[prost(uint64, tag = "1")]
    post: u64,
    #[prost(string, tag = "2")]
    text: String,
}

#[derive(Clone, PartialEq, Message)]
struct Event {
    #[prost(uint64, tag = "1")]
    at: u64,
    #[prost(oneof = "event::Payload", tags = "2, 3, 4")]
    payload: Option<event::Payload>,
    #[prost(btree_map = "string, string", tag = "5")]
    labels: BTreeMap<String, String>,
}

mod event {
    use super::{Comment, Like};

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag = "2")]
        Like(Like),
        #[prost(message, tag = "3")]
        Comment(Comment),
        #[prost(string, tag = "4")]
        Note(String),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use event::Payload;

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    const GOLDEN: [u8; 26] = [
        0x08, 0x05, 0x1a, 0x06, 0x08, 0x07, 0x12, 0x02, b'o', b'k', 0x2a, 0x06, 0x0a, 0x01, b'a', 0x12, 0x01, b'1', 0x2a, 0x06, 0x0a, 0x01, b'b', 0x12, 0x01, b'2',
    ];

    fn golden_event() -> Event {
        Event { at: 5, payload: Some(Payload::Comment(Comment { post: 7, text: "ok".into() })), labels: labels(&[("a", "1"), ("b", "2")]) }
    }

    #[test]
    fn golden_bytes() {
        assert_eq!(golden_event().encode_to_vec(), GOLDEN.to_vec(), "oneof пишется как обычное поле своего варианта, map как повторяющиеся записи ключ-значение");
        assert_eq!(Event::decode(&GOLDEN[..]).unwrap(), golden_event());
    }

    #[test]
    fn empty_event_is_empty() {
        assert!(Event::default().encode_to_vec().is_empty());
        assert_eq!(Event::decode(&[][..]).unwrap().payload, None, "вариант не задан, значит None");
    }

    #[test]
    fn last_variant_of_a_oneof_wins() {
        let bytes = [0x12, 0x02, 0x08, 0x09, 0x22, 0x03, b'a', b'b', b'c'];
        assert_eq!(Event::decode(&bytes[..]).unwrap().payload, Some(Payload::Note("abc".into())), "если на проводе два варианта oneof, остаётся последний");
        let bytes = [0x22, 0x01, b'z', 0x12, 0x02, 0x08, 0x01];
        assert_eq!(Event::decode(&bytes[..]).unwrap().payload, Some(Payload::Like(Like { post: 1 })));
    }

    #[test]
    fn duplicate_map_keys_last_wins() {
        let bytes = [0x2a, 0x06, 0x0a, 0x01, b'a', 0x12, 0x01, b'1', 0x2a, 0x06, 0x0a, 0x01, b'a', 0x12, 0x01, b'9'];
        assert_eq!(Event::decode(&bytes[..]).unwrap().labels, labels(&[("a", "9")]));
    }

    #[test]
    fn same_event_always_gives_the_same_bytes() {
        let pairs = [("env", "prod"), ("region", "eu"), ("build", "42"), ("zone", "a"), ("owner", "x"), ("tier", "1"), ("cell", "7"), ("app", "web")];
        let reference = {
            let e = Event { labels: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Event::default() };
            e.encode_to_vec()
        };
        for shift in 0..pairs.len() {
            let mut rotated = pairs.to_vec();
            rotated.rotate_left(shift);
            let e = Event { labels: rotated.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Event::default() };
            assert_eq!(e.encode_to_vec(), reference, "порядок записей map в байтах не должен зависеть от порядка вставки и от случайного хэша");
        }
        for _ in 0..30 {
            let e = Event { labels: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Event::default() };
            assert_eq!(e.encode_to_vec(), reference, "повторное кодирование равных событий должно давать равные байты");
        }
    }

    #[test]
    fn all_variants_roundtrip() {
        for payload in [Payload::Like(Like { post: 3 }), Payload::Comment(Comment { post: 3, text: "привет".into() }), Payload::Note("заметка".into()), Payload::Note(String::new())] {
            let e = Event { at: 99, payload: Some(payload), labels: labels(&[("k", "v")]) };
            assert_eq!(Event::decode(&e.encode_to_vec()[..]).unwrap(), e);
        }
    }

    #[test]
    fn oneof_with_a_default_value_is_still_present() {
        let e = Event { payload: Some(Payload::Note(String::new())), ..Event::default() };
        assert_eq!(e.encode_to_vec(), vec![0x22, 0x00], "в oneof пустая строка это заданное значение, и её нужно писать");
    }
}
