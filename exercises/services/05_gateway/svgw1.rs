// Клиент соцсети говорит со шлюзом по WebSocket кадрами JSON: `{"id": 7, "service": "user", "method":
// "get_profile", "data": {...}}`. Шлюз находит обработчик по паре (service, method) и отвечает кадром с тем же `id`,
// чтобы клиент нашёл свой запрос среди ответов. Цель: `handle` разбирает кадр, вызывает обработчик и
// отвечает. Битый JSON это ошибка `bad_frame` с `id: null`; нет `service` или `method` (или `id`, который не строка и не число)
// это `invalid_argument` (кадр без `id` и JSON, который не объект, тоже), и `id` в ответе берётся из кадра, если он там есть; неизвестная пара это `not_found`.
// Отсутствующее `data` значит `null`. Ошибка обработчика уходит клиенту как есть.

use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct GwError {
    pub code: &'static str,
    pub message: String,
}

impl GwError {
    pub fn new(code: &'static str, message: &str) -> GwError {
        GwError { code, message: message.to_string() }
    }
}

pub fn ok_reply(id: &Value, data: Value) -> Value {
    json!({"id": id, "ok": true, "data": data})
}

pub fn err_reply(id: &Value, error: &GwError) -> Value {
    json!({"id": id, "ok": false, "error": {"code": error.code, "message": error.message}})
}

pub type Handler = Box<dyn Fn(Value) -> Result<Value, GwError>>;

#[derive(Default)]
pub struct Routes {
    table: HashMap<(String, String), Handler>,
}

impl Routes {
    pub fn new() -> Routes {
        Routes::default()
    }

    pub fn add(&mut self, service: &str, method: &str, handler: impl Fn(Value) -> Result<Value, GwError> + 'static) {
        self.table.insert((service.to_string(), method.to_string()), Box::new(handler));
    }
}

pub fn handle(routes: &Routes, text: &str) -> Value {
    todo!()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn routes() -> Routes {
        let mut routes = Routes::new();
        routes.add("user", "echo", Ok);
        routes.add("user", "fail", |_| Err(GwError::new("permission_denied", "нельзя")));
        routes
    }

    #[test]
    fn reply_keeps_the_id_and_its_type() {
        let reply = handle(&routes(), r#"{"id": 7, "service": "user", "method": "echo", "data": {"a": 1}}"#);
        assert_eq!(reply, json!({"id": 7, "ok": true, "data": {"a": 1}}), "числовой id возвращается числом");
        let reply = handle(&routes(), r#"{"id": "req-9", "service": "user", "method": "echo"}"#);
        assert_eq!(reply, json!({"id": "req-9", "ok": true, "data": null}), "строковый id строкой, а data по умолчанию null");
    }

    #[test]
    fn handler_errors_reach_the_client() {
        let reply = handle(&routes(), r#"{"id": 1, "service": "user", "method": "fail"}"#);
        assert_eq!(reply["ok"], false, "ошибка");
        assert_eq!(reply["id"], 1, "с тем же id");
        assert_eq!(reply["error"]["code"], "permission_denied", "код ошибки обработчика");
    }

    #[test]
    fn unknown_method_is_not_found() {
        let reply = handle(&routes(), r#"{"id": 2, "service": "user", "method": "nope"}"#);
        assert_eq!((reply["id"].clone(), reply["error"]["code"].clone()), (json!(2), json!("not_found")), "{reply}");
        let reply = handle(&routes(), r#"{"id": 3, "service": "ghost", "method": "echo"}"#);
        assert_eq!(reply["error"]["code"], "not_found", "неизвестный сервис");
    }

    #[test]
    fn broken_json_has_no_id() {
        for text in ["", "{", "привет", "[1,2"] {
            let reply = handle(&routes(), text);
            assert_eq!(reply["id"], Value::Null, "{text:?}: id взять неоткуда");
            assert_eq!(reply["error"]["code"], "bad_frame", "{text:?}: код bad_frame");
        }
    }

    #[test]
    fn missing_pieces_are_invalid_argument_and_keep_a_known_id() {
        for (text, id) in [
            (r#"{"id": 5, "method": "echo"}"#, json!(5)),
            (r#"{"id": 6, "service": "user"}"#, json!(6)),
            (r#"{"id": 7, "service": 1, "method": "echo"}"#, json!(7)),
            (r#"{"service": "user", "method": "echo"}"#, Value::Null),
            (r#"{"id": [1], "service": "user", "method": "echo"}"#, Value::Null),
            (r#"[1, 2]"#, Value::Null),
        ] {
            let reply = handle(&routes(), text);
            assert_eq!(reply["error"]["code"], "invalid_argument", "{text}: {reply}");
            assert_eq!(reply["id"], id, "{text}: id в ответе");
        }
    }
}
