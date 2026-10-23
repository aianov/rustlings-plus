// Фронтенд решает, что показать, по тексту ошибки: `if message.contains("занят")`. Копирайтер
// поправил формулировку, и экран регистрации сломался. Сервер должен отдавать ошибку в одном
// конверте: стабильный `code` для программ, `message` для людей, `request_id` для поддержки и
// `details` для полей. Цель: собрать каталог кодов и конверт так, чтобы JSON выглядел как в тестах,
// а клиент переживал коды, которых он ещё не знает (сервер обновили раньше приложения). Пустых `details`
// в JSON быть не должно, serde умеет пропускать поля, а при чтении подставлять умолчание.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    UserNotFound,
    UsernameTaken,
    ValidationFailed,
    RateLimited,
    Internal,
    Unknown,
}

impl Code {
    pub const KNOWN: [Code; 5] =
        [Code::UserNotFound, Code::UsernameTaken, Code::ValidationFailed, Code::RateLimited, Code::Internal];

    pub fn as_str(self) -> &'static str {
        todo!()
    }

    pub fn from_wire(text: &str) -> Code {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    pub field: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub code: String,
    pub message: String,
    pub request_id: String,
    pub details: Vec<Detail>,
}

impl Envelope {
    pub fn new(code: Code, message: &str, request_id: &str) -> Envelope {
        todo!()
    }

    pub fn with_detail(mut self, field: &str, code: &str) -> Envelope {
        todo!()
    }
}

pub fn to_json(envelope: &Envelope) -> String {
    todo!()
}

pub fn from_json(text: &str) -> Result<Envelope, String> {
    todo!()
}

fn main() {
    let envelope = Envelope::new(Code::UsernameTaken, "ник уже занят", "req-1").with_detail("username", "taken");
    println!("{}", to_json(&envelope));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(envelope: &Envelope) -> serde_json::Value {
        serde_json::from_str(&to_json(envelope)).expect("to_json отдаёт корректный JSON")
    }

    #[test]
    fn wire_names_are_frozen() {
        let expected = [
            (Code::UserNotFound, "user.not_found"),
            (Code::UsernameTaken, "user.username_taken"),
            (Code::ValidationFailed, "validation.failed"),
            (Code::RateLimited, "rate_limited"),
            (Code::Internal, "internal"),
        ];
        for (code, wire) in expected {
            assert_eq!(code.as_str(), wire, "строка кода уходит клиентам и не должна меняться: {code:?}");
            assert_eq!(Code::from_wire(wire), code, "из строки должен получаться тот же код: {wire}");
        }
    }

    #[test]
    fn unknown_wire_name_is_not_an_error() {
        assert_eq!(Code::from_wire("post.archived"), Code::Unknown, "новый код сервера клиент обязан пережить");
        assert_eq!(Code::from_wire(""), Code::Unknown, "пустая строка тоже неизвестный код");
        assert_eq!(Code::from_wire("USER.NOT_FOUND"), Code::Unknown, "регистр важен");
    }

    #[test]
    fn envelope_shape_without_details() {
        let envelope = Envelope::new(Code::UserNotFound, "пользователь не найден", "req-7");
        assert_eq!(
            parsed(&envelope),
            json!({"error": {"code": "user.not_found", "message": "пользователь не найден", "request_id": "req-7"}}),
            "ошибка лежит под ключом error, пустых details в JSON нет"
        );
    }

    #[test]
    fn envelope_shape_with_details() {
        let envelope = Envelope::new(Code::ValidationFailed, "проверь поля", "req-8")
            .with_detail("username", "too_short")
            .with_detail("email", "invalid");
        assert_eq!(
            parsed(&envelope)["error"]["details"],
            json!([{"field": "username", "code": "too_short"}, {"field": "email", "code": "invalid"}]),
            "details идут в порядке добавления"
        );
    }

    #[test]
    fn client_reads_what_server_wrote() {
        let envelope = Envelope::new(Code::RateLimited, "слишком часто", "req-9").with_detail("ip", "limit");
        let back = from_json(&to_json(&envelope)).expect("свой же конверт читается");
        assert_eq!(back, envelope, "после круга туда и обратно конверт не меняется");
    }

    #[test]
    fn client_survives_a_code_from_the_future() {
        let text = r#"{"error":{"code":"post.archived","message":"пост в архиве","request_id":"r1"}}"#;
        let envelope = from_json(text).expect("неизвестный код не должен ломать разбор");
        assert_eq!(envelope.code, "post.archived", "сам текст кода сохраняется: его можно показать в отчёте");
        assert_eq!(Code::from_wire(&envelope.code), Code::Unknown, "а решать по нему клиент пока не умеет");
        assert!(envelope.details.is_empty(), "details без поля в JSON это пустой список");
    }

    #[test]
    fn broken_body_is_an_error_not_a_panic() {
        assert!(from_json("не json").is_err(), "мусор вместо JSON");
        assert!(from_json(r#"{"code":"x"}"#).is_err(), "нет обёртки error");
    }
}
