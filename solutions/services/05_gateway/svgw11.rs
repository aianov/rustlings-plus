
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Principal {
    pub user_id: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestCtx {
    pub request_id: String,
    pub principal: Option<Principal>,
}

#[derive(Default)]
pub struct RequestIds {
    next: AtomicU64,
}

impl RequestIds {
    pub fn new() -> RequestIds {
        RequestIds::default()
    }

    pub fn context(&self, frame: &Value, principal: Option<Principal>) -> RequestCtx {
        let offered = frame
            .get("meta")
            .and_then(|meta| meta.get("request_id"))
            .and_then(Value::as_str)
            .filter(|id| (1..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)));
        let request_id = match offered {
            Some(id) => id.to_string(),
            None => format!("gw-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1),
        };
        RequestCtx { request_id, principal }
    }
}

pub fn outgoing_metadata(ctx: &RequestCtx) -> Vec<(String, String)> {
    let mut metadata = vec![("x-request-id".to_string(), ctx.request_id.clone())];
    if let Some(principal) = &ctx.principal {
        metadata.push(("x-user-id".to_string(), principal.user_id.clone()));
        metadata.push(("x-user-role".to_string(), principal.role.clone()));
    }
    metadata
}

pub fn finish(mut reply: Value, ctx: &RequestCtx) -> Value {
    reply["request_id"] = json!(ctx.request_id);
    reply
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Option<Principal> {
        Some(Principal { user_id: "u-1".to_string(), role: "user".to_string() })
    }

    fn kv(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn gateway_issues_sequential_ids_when_the_client_sent_none() {
        let ids = RequestIds::new();
        let first = ids.context(&json!({"id": 1}), None);
        let second = ids.context(&json!({"id": 2}), None);
        assert_eq!((first.request_id.as_str(), second.request_id.as_str()), ("gw-1", "gw-2"), "свои номера по порядку");
    }

    #[test]
    fn safe_client_id_is_kept() {
        let ids = RequestIds::new();
        let ctx = ids.context(&json!({"meta": {"request_id": "app-7f3.a_b-9"}}), None);
        assert_eq!(ctx.request_id, "app-7f3.a_b-9", "безопасный номер клиента");
    }

    #[test]
    fn unsafe_client_ids_are_replaced() {
        let ids = RequestIds::new();
        for bad in ["", "с пробелом и кириллицей", "a b", "a\nb", "x\r\nSet-Cookie: 1", &"a".repeat(65), "a/b", "a;b"] {
            let ctx = ids.context(&json!({"meta": {"request_id": bad}}), None);
            assert!(ctx.request_id.starts_with("gw-"), "{bad:?}: небезопасный номер заменён своим, а было {}", ctx.request_id);
        }
        let ctx = ids.context(&json!({"meta": {"request_id": 5}}), None);
        assert!(ctx.request_id.starts_with("gw-"), "не строка");
        let long = "a".repeat(64);
        assert_eq!(ids.context(&json!({"meta": {"request_id": long}}), None).request_id.len(), 64, "ровно 64 символа допустимо");
    }

    #[test]
    fn metadata_carries_request_and_user() {
        let ids = RequestIds::new();
        let ctx = ids.context(&json!({}), alice());
        assert_eq!(
            outgoing_metadata(&ctx),
            vec![kv("x-request-id", "gw-1"), kv("x-user-id", "u-1"), kv("x-user-role", "user")],
            "metadata для вошедшего"
        );
        let anonymous = ids.context(&json!({}), None);
        assert_eq!(outgoing_metadata(&anonymous), vec![kv("x-request-id", "gw-2")], "аноним: только номер запроса, а user-id нет вообще");
    }

    #[test]
    fn reply_names_the_request_id() {
        let ids = RequestIds::new();
        let ctx = ids.context(&json!({}), None);
        let reply = finish(json!({"id": 9, "ok": true, "data": 1}), &ctx);
        assert_eq!(reply, json!({"id": 9, "ok": true, "data": 1, "request_id": "gw-1"}), "остальные поля ответа не тронуты");
    }
}
