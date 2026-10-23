
use std::error::Error;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Error,
}

pub trait Sink {
    fn log(&self, level: Level, line: String);
}

#[derive(Default)]
pub struct MemorySink {
    pub lines: Mutex<Vec<(Level, String)>>,
}

impl Sink for MemorySink {
    fn log(&self, level: Level, line: String) {
        self.lines.lock().unwrap().push((level, line));
    }
}

pub enum ApiError {
    Client { status: u16, code: &'static str, message: String },
    Internal(Box<dyn Error + Send + Sync>),
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: serde_json::Value,
}

pub fn respond(err: &ApiError, request_id: &str, sink: &dyn Sink) -> Response {
    match err {
        ApiError::Client { status, code, message } => {
            sink.log(Level::Info, format!("request_id={request_id} status={status} code={code}"));
            Response {
                status: *status,
                body: serde_json::json!({"error": {"code": code, "message": message, "request_id": request_id}}),
            }
        }
        ApiError::Internal(cause) => {
            let mut line = format!("request_id={request_id} code=internal error={cause}");
            let mut source = cause.source();
            while let Some(next) = source {
                line.push_str(&format!("; caused by: {next}"));
                source = next.source();
            }
            sink.log(Level::Error, line);
            Response {
                status: 500,
                body: serde_json::json!({"error": {
                    "code": "internal",
                    "message": "внутренняя ошибка, назови поддержке номер запроса",
                    "request_id": request_id,
                }}),
            }
        }
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Debug)]
    struct Layer {
        text: &'static str,
        source: Option<Box<dyn Error + Send + Sync>>,
    }

    impl fmt::Display for Layer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.text)
        }
    }

    impl Error for Layer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.source.as_ref().map(|s| s.as_ref() as &(dyn Error + 'static))
        }
    }

    fn deep_error() -> ApiError {
        let socket = Layer { text: "connection refused (os error 111)", source: None };
        let pool = Layer { text: "pool exhausted for db-7.internal:5432", source: Some(Box::new(socket)) };
        let top = Layer { text: "не удалось списать платёж", source: Some(Box::new(pool)) };
        ApiError::Internal(Box::new(top))
    }

    #[test]
    fn log_gets_the_whole_chain_and_the_request_id() {
        let sink = MemorySink::default();
        respond(&deep_error(), "req-77", &sink);
        let lines = sink.lines.lock().unwrap();
        assert_eq!(lines.len(), 1, "одна ошибка это одна запись в логе: {lines:?}");
        let (level, line) = &lines[0];
        assert_eq!(*level, Level::Error, "внутренняя ошибка пишется уровнем Error");
        for needle in ["req-77", "не удалось списать платёж", "pool exhausted for db-7.internal:5432", "connection refused"] {
            assert!(line.contains(needle), "в логе нет «{needle}», а вся цепочка причин нужна, иначе корень проблемы не найти: {line}");
        }
    }

    #[test]
    fn user_gets_a_generic_message_and_a_request_id() {
        let sink = MemorySink::default();
        let response = respond(&deep_error(), "req-77", &sink);
        assert_eq!(response.status, 500, "внутренняя ошибка это 500");
        assert_eq!(response.body["error"]["code"], "internal", "код");
        assert_eq!(response.body["error"]["request_id"], "req-77", "номер запроса пользователь передаёт поддержке");
        let text = response.body.to_string();
        for secret in ["db-7", "5432", "os error", "pool", "платёж"] {
            assert!(!text.contains(secret), "в ответе клиенту не должно быть «{secret}»: {text}");
        }
    }

    #[test]
    fn client_errors_are_logged_quietly_and_shown_as_is() {
        let sink = MemorySink::default();
        let err = ApiError::Client { status: 404, code: "post.not_found", message: "пост 5 не найден".to_string() };
        let response = respond(&err, "req-1", &sink);
        assert_eq!(response.status, 404, "статус клиентской ошибки сохраняется");
        assert_eq!(response.body["error"]["message"], "пост 5 не найден", "человеческое сообщение показываем");
        assert_eq!(response.body["error"]["code"], "post.not_found", "и код");
        let lines = sink.lines.lock().unwrap();
        assert_eq!(lines.len(), 1, "запись всё же нужна: {lines:?}");
        assert_eq!(lines[0].0, Level::Info, "404 это не авария, уровень Info: иначе алерты шумят из-за чужих опечаток");
        assert!(lines[0].1.contains("req-1") && lines[0].1.contains("post.not_found"), "в записи номер запроса и код: {}", lines[0].1);
    }

    #[test]
    fn error_without_a_chain_still_works() {
        let sink = MemorySink::default();
        let err = ApiError::Internal(Box::new(Layer { text: "одинокая ошибка", source: None }));
        respond(&err, "req-2", &sink);
        let lines = sink.lines.lock().unwrap();
        assert!(lines[0].1.contains("одинокая ошибка"), "причина без цепочки: {}", lines[0].1);
    }
}
