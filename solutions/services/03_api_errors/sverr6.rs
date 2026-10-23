
use serde_json::json;

#[derive(Debug)]
pub enum DomainError {
    Unauthenticated,
    Forbidden { action: &'static str },
    NotFound { what: &'static str, id: u64 },
    RateLimited { retry_after_secs: u64 },
    Internal,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

pub fn to_response(err: &DomainError, request_id: &str) -> Response {
    let (status, code, message) = match err {
        DomainError::Unauthenticated => (401, "auth.unauthenticated", "нужно войти".to_string()),
        DomainError::Forbidden { action } => (403, "auth.forbidden", format!("нет прав: {action}")),
        DomainError::NotFound { what, id } => (404, "not_found", format!("{what} {id} не найден")),
        DomainError::RateLimited { .. } => (429, "rate_limited", "слишком много запросов".to_string()),
        DomainError::Internal => (500, "internal", "внутренняя ошибка".to_string()),
    };
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    match err {
        DomainError::Unauthenticated => headers.push(("www-authenticate".to_string(), "Bearer".to_string())),
        DomainError::RateLimited { retry_after_secs } => headers.push(("retry-after".to_string(), retry_after_secs.to_string())),
        _ => {}
    }
    let body = json!({"error": {"code": code, "message": message, "request_id": request_id}}).to_string();
    Response { status, headers, body }
}

pub fn client_should_refresh_token(response: &Response) -> bool {
    response.status == 401
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(response: &Response) -> serde_json::Value {
        serde_json::from_str(&response.body).expect("тело ответа должно быть JSON")
    }

    #[test]
    fn statuses() {
        let cases = [
            (DomainError::Unauthenticated, 401),
            (DomainError::Forbidden { action: "delete" }, 403),
            (DomainError::NotFound { what: "пост", id: 3 }, 404),
            (DomainError::RateLimited { retry_after_secs: 5 }, 429),
            (DomainError::Internal, 500),
        ];
        for (err, status) in cases {
            assert_eq!(to_response(&err, "r").status, status, "{err:?}");
        }
    }

    #[test]
    fn forbidden_does_not_send_the_client_into_a_login_loop() {
        let mut refreshes = 0;
        let mut last = to_response(&DomainError::Forbidden { action: "delete" }, "r");
        for _ in 0..5 {
            if !client_should_refresh_token(&last) {
                break;
            }
            refreshes += 1;
            last = to_response(&DomainError::Forbidden { action: "delete" }, "r");
        }
        assert_eq!(refreshes, 0, "клиент уже вошёл, новый токен прав не добавит: отдавай 403, а не 401");
    }

    #[test]
    fn unauthenticated_says_how_to_authenticate() {
        let response = to_response(&DomainError::Unauthenticated, "r");
        assert_eq!(response.header("www-authenticate"), Some("Bearer"), "для 401 нужен заголовок www-authenticate");
    }

    #[test]
    fn rate_limit_tells_when_to_come_back() {
        let response = to_response(&DomainError::RateLimited { retry_after_secs: 30 }, "r");
        assert_eq!(response.header("retry-after"), Some("30"), "заголовок retry-after в секундах");
        assert!(to_response(&DomainError::Internal, "r").header("retry-after").is_none(), "лишний retry-after у 500 не нужен");
    }

    #[test]
    fn body_is_the_envelope() {
        let response = to_response(&DomainError::NotFound { what: "пост", id: 42 }, "req-5");
        assert_eq!(response.header("content-type"), Some("application/json"), "тип содержимого");
        let value = body(&response);
        assert_eq!(value["error"]["code"], "not_found", "стабильный код");
        assert_eq!(value["error"]["request_id"], "req-5", "номер запроса для поддержки");
        let message = value["error"]["message"].as_str().expect("message это строка");
        assert!(message.contains("пост") && message.contains("42"), "сообщение называет, что не найдено: {message}");
    }

    #[test]
    fn auth_errors_have_different_codes() {
        let a = body(&to_response(&DomainError::Unauthenticated, "r"));
        let f = body(&to_response(&DomainError::Forbidden { action: "delete" }, "r"));
        assert_ne!(a["error"]["code"], f["error"]["code"], "клиент по коду должен отличать «войди» от «нельзя»");
    }

    #[test]
    fn debug_output_never_reaches_the_client() {
        let response = to_response(&DomainError::Forbidden { action: "delete" }, "r");
        assert!(!response.body.contains("Forbidden {"), "в теле не должно быть Debug-вида внутреннего enum: {}", response.body);
    }
}
