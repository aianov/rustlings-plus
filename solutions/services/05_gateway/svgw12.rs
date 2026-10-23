
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Principal {
    pub user_id: String,
    pub role: String,
}

pub struct Trusted {
    pub request_id: String,
    pub principal: Option<Principal>,
}

const ALLOWED: [&str; 2] = ["x-trace-id", "accept-language"];

pub fn forward_metadata(client_meta: &Map<String, Value>, trusted: &Trusted) -> Vec<(String, String)> {
    let mut metadata = vec![("x-request-id".to_string(), trusted.request_id.clone())];
    if let Some(principal) = &trusted.principal {
        metadata.push(("x-user-id".to_string(), principal.user_id.clone()));
        metadata.push(("x-user-role".to_string(), principal.role.clone()));
    }
    for (key, value) in client_meta {
        let key = key.to_ascii_lowercase();
        let Some(value) = value.as_str() else { continue };
        if ALLOWED.contains(&key.as_str()) && value.len() <= 128 && !value.chars().any(char::is_control) {
            metadata.push((key, value.to_string()));
        }
    }
    metadata
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    fn trusted(user: Option<&str>) -> Trusted {
        Trusted {
            request_id: "gw-1".to_string(),
            principal: user.map(|id| Principal { user_id: id.to_string(), role: "user".to_string() }),
        }
    }

    fn values<'a>(metadata: &'a [(String, String)], key: &str) -> Vec<&'a str> {
        metadata.iter().filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str()).collect()
    }

    #[test]
    fn anonymous_client_cannot_pose_as_a_user() {
        let sent = meta(json!({"x-user-id": "u-1", "x-user-role": "admin"}));
        let metadata = forward_metadata(&sent, &trusted(None));
        assert!(values(&metadata, "x-user-id").is_empty(), "у анонима нет user-id, что бы он ни прислал: {metadata:?}");
        assert!(values(&metadata, "x-user-role").is_empty(), "и роли нет: {metadata:?}");
    }

    #[test]
    fn logged_in_user_cannot_pose_as_another() {
        let sent = meta(json!({"x-user-id": "u-999", "X-User-Role": "admin", "x-request-id": "forged"}));
        let metadata = forward_metadata(&sent, &trusted(Some("u-1")));
        assert_eq!(values(&metadata, "x-user-id"), vec!["u-1"], "ровно один user-id и он настоящий");
        assert_eq!(values(&metadata, "x-user-role"), vec!["user"], "роль только из проверенного контекста, в любом регистре ключа");
        assert_eq!(values(&metadata, "x-request-id"), vec!["gw-1"], "номер запроса шлюзовый, подделать нельзя");
    }

    #[test]
    fn internal_prefix_is_reserved() {
        let sent = meta(json!({"x-internal-skip-auth": "1", "X-Internal-Debug": "on"}));
        let metadata = forward_metadata(&sent, &trusted(Some("u-1")));
        assert!(metadata.iter().all(|(k, _)| !k.to_ascii_lowercase().starts_with("x-internal-")), "внутренние ключи клиент не ставит: {metadata:?}");
    }

    #[test]
    fn only_allow_listed_client_keys_pass() {
        let sent = meta(json!({"x-trace-id": "t-5", "Accept-Language": "ru", "x-anything-else": "1", "cookie": "s=1"}));
        let metadata = forward_metadata(&sent, &trusted(None));
        assert_eq!(values(&metadata, "x-trace-id"), vec!["t-5"], "trace-id пропущен");
        assert_eq!(values(&metadata, "accept-language"), vec!["ru"], "ключ приведён к нижнему регистру");
        assert!(metadata.iter().all(|(k, _)| k != "x-anything-else" && k != "cookie"), "прочее отброшено: {metadata:?}");
        assert!(metadata.iter().all(|(k, _)| *k == k.to_ascii_lowercase()), "все ключи в нижнем регистре: {metadata:?}");
    }

    #[test]
    fn dangerous_values_are_dropped() {
        let sent = meta(json!({"x-trace-id": "a\r\nx-user-id: u-1"}));
        assert!(values(&forward_metadata(&sent, &trusted(None)), "x-trace-id").is_empty(), "перевод строки в значении это попытка вставить заголовок");
        let long = "a".repeat(129);
        assert!(values(&forward_metadata(&meta(json!({"x-trace-id": long})), &trusted(None)), "x-trace-id").is_empty(), "слишком длинное значение");
        let edge = "a".repeat(128);
        assert_eq!(values(&forward_metadata(&meta(json!({"x-trace-id": edge.clone()})), &trusted(None)), "x-trace-id"), vec![edge.as_str()], "ровно 128 байт можно");
        assert!(values(&forward_metadata(&meta(json!({"x-trace-id": 5})), &trusted(None)), "x-trace-id").is_empty(), "не строка");
    }
}
