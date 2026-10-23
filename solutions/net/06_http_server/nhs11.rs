use serde::Serialize;

#[derive(Debug)]
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

impl Response {
    fn json<T: Serialize>(status: u16, value: &T) -> Response {
        match serde_json::to_vec(value) {
            Ok(body) => Response { status, headers: vec![("Content-Type".to_string(), "application/json".to_string())], body },
            Err(_) => Response::error(500, "internal error"),
        }
    }

    fn error(status: u16, message: &str) -> Response {
        let body = serde_json::json!({ "error": message }).to_string().into_bytes();
        Response { status, headers: vec![("Content-Type".to_string(), "application/json".to_string())], body }
    }
}

fn main() {
    let ok = Response::json(200, &vec![1, 2, 3]);
    let bad = Response::error(404, "no such user");
    println!("{} {}", ok.status, String::from_utf8_lossy(&ok.body));
    println!("{} {}", bad.status, String::from_utf8_lossy(&bad.body));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn json_response() {
        let reply = Response::json(201, &vec![1, 2]);
        assert_eq!(reply.status, 201, "статус задаёт вызывающий");
        assert_eq!(reply.header("content-type"), Some("application/json"));
        assert_eq!(reply.body, b"[1,2]");
    }

    #[test]
    fn error_is_always_valid_json() {
        for message in ["plain", "say \"hi\"", "back\\slash", "line\nbreak\ttab", "Ошибка: 📦", "</script>", ""] {
            let reply = Response::error(400, message);
            assert_eq!(reply.status, 400);
            assert_eq!(reply.header("content-type"), Some("application/json"));
            let parsed: serde_json::Value = serde_json::from_slice(&reply.body)
                .unwrap_or_else(|e| panic!("для сообщения {message:?} получилось не-JSON ({e}): {:?}", String::from_utf8_lossy(&reply.body)));
            assert_eq!(parsed["error"], message, "сообщение должно дойти до клиента без искажений");
            assert_eq!(parsed.as_object().unwrap().len(), 1, "в теле ошибки одно поле");
        }
    }

    #[test]
    fn unserializable_value_is_a_server_error() {
        let mut broken: HashMap<(u8, u8), u8> = HashMap::new();
        broken.insert((1, 2), 3);
        let reply = Response::json(200, &broken);
        assert_eq!(reply.status, 500, "ключи-кортежи в JSON невозможны: это ошибка сервера, а не 200 с пустым телом");
        assert_eq!(reply.body, br#"{"error":"internal error"}"#);
        assert_eq!(reply.header("content-type"), Some("application/json"));
    }
}
