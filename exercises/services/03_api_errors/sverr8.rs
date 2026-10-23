// Пентестер прислал отчёт: из ответа сервера он узнал путь к каталогу загрузок, имя пользователя и
// пароль базы из строки подключения, а ещё получил обратно свой собственный пароль в тексте ошибки
// валидации. Ошибки до клиента доходят через один общий `respond`, и он слишком разговорчив.
// Цель: ни один секрет из внутренних ошибок не должен попасть ни в тело ответа, ни в заголовки,
// но понятные клиенту ошибки (нет такого поста, проверь поле) остаются понятными.

use std::path::PathBuf;

pub enum AppError {
    NotFound { what: &'static str, id: u64 },
    Validation(Vec<FieldProblem>),
    Io { path: PathBuf, source: std::io::Error },
    Db { dsn: String, message: String },
}

pub struct FieldProblem {
    pub field: &'static str,
    pub code: &'static str,
    pub value: String,
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
}

pub fn respond(err: &AppError, request_id: &str) -> Response {
    let (status, code, details) = match err {
        AppError::NotFound { .. } => (404, "not_found", Vec::new()),
        AppError::Validation(problems) => (
            422,
            "validation.failed",
            problems
                .iter()
                .map(|p| serde_json::json!({"field": p.field, "code": p.code, "value": p.value}))
                .collect(),
        ),
        AppError::Io { .. } | AppError::Db { .. } => (500, "internal", Vec::new()),
    };
    let message = match err {
        AppError::NotFound { what, id } => format!("{what} {id} не найден"),
        AppError::Validation(_) => "проверь поля".to_string(),
        AppError::Io { path, source } => format!("не удалось прочитать {}: {source}", path.display()),
        AppError::Db { dsn, message } => format!("база {dsn}: {message}"),
    };
    Response {
        status,
        headers: vec![
            ("x-request-id".to_string(), request_id.to_string()),
            ("x-debug-error".to_string(), message.clone()),
        ],
        body: serde_json::json!({"error": {
            "code": code,
            "message": message,
            "request_id": request_id,
            "details": details,
        }}),
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRETS: [&str; 6] = ["/var/lib/uploads", "avatar.png", "app_user", "s3cr3t-pass", "db-7.internal", "hunter2"];

    fn everything_sent(response: &Response) -> String {
        format!("{} {:?} {}", response.status, response.headers, response.body)
    }

    fn io_error() -> AppError {
        AppError::Io {
            path: PathBuf::from("/var/lib/uploads/42/avatar.png"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        }
    }

    fn db_error() -> AppError {
        AppError::Db {
            dsn: "postgres://app_user:s3cr3t-pass@db-7.internal:5432/social".to_string(),
            message: "connection reset".to_string(),
        }
    }

    fn validation_error() -> AppError {
        AppError::Validation(vec![
            FieldProblem { field: "password", code: "too_short", value: "hunter2".to_string() },
            FieldProblem { field: "email", code: "invalid", value: "not-an-email".to_string() },
        ])
    }

    #[test]
    fn internal_errors_reveal_nothing() {
        for err in [io_error(), db_error()] {
            let response = respond(&err, "req-9");
            assert_eq!(response.status, 500, "внутренняя ошибка");
            let sent = everything_sent(&response);
            for secret in SECRETS {
                assert!(!sent.contains(secret), "клиент получил «{secret}»: смотри не только тело, но и заголовки: {sent}");
            }
            assert!(sent.contains("req-9"), "номер запроса остаётся, он нужен поддержке");
        }
    }

    #[test]
    fn validation_does_not_echo_what_the_user_typed() {
        let response = respond(&validation_error(), "req-1");
        let sent = everything_sent(&response);
        assert!(!sent.contains("hunter2"), "пароль вернулся в ответе: {sent}");
        assert!(!sent.contains("not-an-email"), "значение поля эхом возвращать не нужно: {sent}");
        let details = response.body["error"]["details"].as_array().expect("details это список");
        assert_eq!(details.len(), 2, "оба нарушения на месте");
        assert_eq!(details[0]["field"], "password", "поле");
        assert_eq!(details[0]["code"], "too_short", "код нарушения: по нему клиент подсветит поле");
    }

    #[test]
    fn client_errors_stay_helpful() {
        let response = respond(&AppError::NotFound { what: "пост", id: 42 }, "req-2");
        assert_eq!(response.status, 404, "статус");
        let message = response.body["error"]["message"].as_str().unwrap();
        assert!(message.contains("пост") && message.contains("42"), "для 404 понятное сообщение остаётся: {message}");
        let response = respond(&validation_error(), "req-3");
        assert_eq!(response.status, 422, "статус валидации");
        assert_eq!(response.body["error"]["code"], "validation.failed", "код валидации");
    }

    #[test]
    fn internal_errors_look_alike() {
        let a = respond(&io_error(), "r");
        let b = respond(&db_error(), "r");
        assert_eq!(a, b, "по ответу нельзя понять, какая именно внутренняя часть сломалась: это тоже разведданные");
    }
}
