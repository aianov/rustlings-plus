use serde::Deserialize;
use serde_json::Value;

fn main() {}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Followed { event_id: String, follower: String, followee: String },
    PostSeen { event_id: String, post_id: String, author: String },
    Ignored,
}

#[derive(Deserialize)]
struct Raw {
    event_id: String,
    #[serde(rename = "type")]
    kind: String,
    payload: Value,
}

#[derive(Deserialize)]
struct Followed {
    follower_id: String,
    followee_id: String,
}

#[derive(Deserialize)]
struct Created {
    post_id: String,
    author_id: String,
}

pub fn handle(bytes: &[u8]) -> Result<Outcome, String> {
    let raw: Raw = serde_json::from_slice(bytes).map_err(|e| format!("битое событие: {e}"))?;
    match raw.kind.as_str() {
        "user.followed" => {
            let payload: Followed = serde_json::from_value(raw.payload).map_err(|e| format!("битый payload: {e}"))?;
            Ok(Outcome::Followed { event_id: raw.event_id, follower: payload.follower_id, followee: payload.followee_id })
        }
        "post.created" => {
            let payload: Created = serde_json::from_value(raw.payload).map_err(|e| format!("битый payload: {e}"))?;
            Ok(Outcome::PostSeen { event_id: raw.event_id, post_id: payload.post_id, author: payload.author_id })
        }
        _ => Ok(Outcome::Ignored),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(kind: &str, payload: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({"event_id": "e1", "type": kind, "schema": 1, "version": 1, "occurred_at": 5, "aggregate_id": "x", "payload": payload})).unwrap()
    }

    #[test]
    fn plain_events_are_read() {
        let followed = handle(&envelope("user.followed", json!({"follower_id": "bob", "followee_id": "alice"}))).expect("подписка");
        assert_eq!(followed, Outcome::Followed { event_id: "e1".into(), follower: "bob".into(), followee: "alice".into() }, "подписка читается");
        let created = handle(&envelope("post.created", json!({"post_id": "p1", "author_id": "alice", "text": "hi"}))).expect("пост");
        assert_eq!(created, Outcome::PostSeen { event_id: "e1".into(), post_id: "p1".into(), author: "alice".into() }, "пост читается");
    }

    #[test]
    fn a_new_field_in_the_payload_does_not_break_the_reader() {
        let bytes = envelope("user.followed", json!({"follower_id": "bob", "followee_id": "alice", "source": "profile_page"}));
        assert!(matches!(handle(&bytes), Ok(Outcome::Followed { .. })), "поле `source` потребителю не нужно, но и мешать не должно");
    }

    #[test]
    fn a_new_field_in_the_envelope_does_not_break_the_reader() {
        let mut value: Value = serde_json::from_slice(&envelope("user.followed", json!({"follower_id": "bob", "followee_id": "alice"}))).unwrap();
        value["trace_id"] = json!("abc");
        value["producer"] = json!("user-service");
        assert!(matches!(handle(&serde_json::to_vec(&value).unwrap()), Ok(Outcome::Followed { .. })), "лишние поля конверта тоже игнорируются");
    }

    #[test]
    fn an_unknown_event_type_is_skipped_whatever_its_payload() {
        for payload in [json!({"anything": 1}), json!(null), json!("строка")] {
            assert_eq!(handle(&envelope("user.blocked", payload)), Ok(Outcome::Ignored), "незнакомый тип пропускается, его payload мы не разбираем");
        }
    }

    #[test]
    fn required_fields_stay_required() {
        assert!(handle(&envelope("user.followed", json!({"follower_id": "bob"}))).is_err(), "нет followee_id: молча подставлять пустое нельзя");
        assert!(handle(&envelope("post.created", json!({"post_id": 5, "author_id": "a"}))).is_err(), "post_id не строка");
        let no_id = br#"{"type":"user.followed","payload":{"follower_id":"a","followee_id":"b"}}"#;
        assert!(handle(no_id).is_err(), "у события нет event_id");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(handle(b"").is_err(), "пусто");
        assert!(handle(b"[1,2]").is_err(), "не объект");
        assert!(handle(&[0xff, 0xfe]).is_err(), "не UTF-8");
    }
}
