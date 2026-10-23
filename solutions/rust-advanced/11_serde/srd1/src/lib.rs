use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub verified: bool,
    pub tags: Vec<String>,
}

pub fn to_json(user: &User) -> String {
    serde_json::to_string(user).expect("User состоит из чисел и строк и всегда сериализуется")
}

pub fn to_bytes(user: &User) -> Vec<u8> {
    serde_json::to_vec(user).expect("User состоит из чисел и строк и всегда сериализуется")
}

pub fn from_json(raw: &str) -> Result<User, serde_json::Error> {
    serde_json::from_str(raw)
}

pub fn from_bytes(raw: &[u8]) -> Result<User, serde_json::Error> {
    serde_json::from_slice(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ira() -> User {
        User { id: 7, name: "ira".into(), verified: true, tags: vec!["rust".into(), "jazz".into()] }
    }

    #[test]
    fn serializes_in_field_order() {
        assert_eq!(to_json(&ira()), r#"{"id":7,"name":"ira","verified":true,"tags":["rust","jazz"]}"#);
    }

    #[test]
    fn bytes_are_the_same_json() {
        assert_eq!(to_bytes(&ira()), to_json(&ira()).into_bytes());
    }

    #[test]
    fn roundtrip() {
        let user = ira();
        assert_eq!(from_json(&to_json(&user)).unwrap(), user);
        assert_eq!(from_bytes(&to_bytes(&user)).unwrap(), user, "байты разбираются так же, как строка");
    }

    #[test]
    fn extra_keys_are_ignored_and_order_does_not_matter() {
        let user = from_json(r#"{"tags":[],"unknown":{"a":1},"name":"o","verified":false,"id":1}"#).unwrap();
        assert_eq!(user, User { id: 1, name: "o".into(), verified: false, tags: vec![] });
    }

    #[test]
    fn broken_input_is_an_error_with_a_reason() {
        let err = from_json(r#"{"id":1,"name":"o","verified":false}"#).unwrap_err();
        assert!(err.to_string().contains("missing field `tags`"), "ждали жалобу на пропавшее поле tags: {err}");
        let err = from_json(r#"{"id":"one","name":"o","verified":false,"tags":[]}"#).unwrap_err();
        assert!(err.to_string().contains("invalid type"), "ждали жалобу на тип: {err}");
        assert!(from_bytes(&[0xff, 0xfe]).is_err(), "не-UTF-8 это тоже ошибка, а не паника");
    }
}
