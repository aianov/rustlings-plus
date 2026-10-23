
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
    let Ok(frame) = serde_json::from_str::<Value>(text) else {
        return err_reply(&Value::Null, &GwError::new("bad_frame", "кадр не JSON"));
    };
    let id = frame.get("id").cloned().filter(|id| id.is_string() || id.is_number());
    let service = frame.get("service").and_then(Value::as_str);
    let method = frame.get("method").and_then(Value::as_str);
    let (Some(id), Some(service), Some(method)) = (id.clone(), service, method) else {
        return err_reply(&id.unwrap_or(Value::Null), &GwError::new("invalid_argument", "нужны id, service и method"));
    };
    let Some(handler) = routes.table.get(&(service.to_string(), method.to_string())) else {
        return err_reply(&id, &GwError::new("not_found", &format!("нет метода {service}.{method}")));
    };
    match handler(frame.get("data").cloned().unwrap_or(Value::Null)) {
        Ok(data) => ok_reply(&id, data),
        Err(error) => err_reply(&id, &error),
    }
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
