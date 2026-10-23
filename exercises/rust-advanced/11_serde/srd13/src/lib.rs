// Запросы и ответы ходят в одинаковом конверте `{"id": .., "data": ..}`, а меняется только то, что в `data`:
// пост, список чисел, ничего. Писать структуру под каждый случай незачем, поэтому конверт обобщённый. Нужны три
// функции: записать, прочитать и прогнать туда-обратно. Типу `T` в каждой нужно разрешить ровно то, что
// функции от него требуют, и не больше. Особенно внимательно с `roundtrip`: она отдаёт `T` наружу, а строка JSON,
// из которой он прочитан, умирает внутри.
// TODO: Допиши функции и подбери границы на `T`.
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub id: u64,
    pub data: T,
}

pub fn encode<T>(id: u64, data: &T) -> String {
    todo!("конверт в строку JSON")
}

pub fn decode<T>(raw: &str) -> Result<Envelope<T>, serde_json::Error> {
    todo!("строка JSON в конверт")
}

pub fn roundtrip<T>(id: u64, data: &T) -> Result<T, serde_json::Error> {
    todo!("записать и тут же прочитать обратно")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Post {
        title: String,
        likes: u32,
    }

    fn post() -> Post {
        Post { title: "hi".into(), likes: 2 }
    }

    #[test]
    fn encodes_different_payloads() {
        assert_eq!(encode(1, &post()), r#"{"id":1,"data":{"title":"hi","likes":2}}"#);
        assert_eq!(encode(2, &[1, 2, 3]), r#"{"id":2,"data":[1,2,3]}"#);
        assert_eq!(encode(3, &()), r#"{"id":3,"data":null}"#, "пустой ответ это null");
        assert_eq!(encode(4, &"text"), r#"{"id":4,"data":"text"}"#);
    }

    #[test]
    fn decodes_into_the_requested_type() {
        let env: Envelope<Post> = decode(r#"{"id":7,"data":{"title":"a","likes":0}}"#).unwrap();
        assert_eq!(env, Envelope { id: 7, data: Post { title: "a".into(), likes: 0 } });
        let list = decode::<Vec<String>>(r#"{"id":1,"data":["x","y"]}"#).unwrap();
        assert_eq!(list.data, vec!["x".to_string(), "y".to_string()]);
        assert_eq!(decode::<()>(r#"{"id":1,"data":null}"#).unwrap(), Envelope { id: 1, data: () });
        assert_eq!(decode::<Option<u8>>(r#"{"id":1,"data":null}"#).unwrap().data, None);
    }

    #[test]
    fn wrong_payload_is_an_error() {
        assert!(decode::<Post>(r#"{"id":1,"data":[1]}"#).is_err());
        assert!(decode::<Post>(r#"{"id":1}"#).is_err(), "без data конверт не годится");
    }

    #[test]
    fn roundtrip_returns_owned_data() {
        assert_eq!(roundtrip(1, &post()).unwrap(), post());
        let names = vec!["a".to_string(), "b".to_string()];
        let back: Vec<String> = roundtrip(9, &names).unwrap();
        assert_eq!(back, names);
    }
}
