// Мобильный клиент различает два случая: «ты прислал не JSON» (баг приложения, показать «обнови
// версию») и «JSON верный, но поля не годятся» (показать ошибки под полями). Сейчас всё это один
// `400 bad request`, а в тексте ошибки serde ещё и торчит присланный пароль. Цель: допиши
// `parse_registration`: битый JSON это 400, неверная форма или значения это 422 со всеми нарушениями
// в `details` сразу, и ни одно присланное значение не возвращается в ответе. Коды: `request.malformed`,
// `request.invalid_body`, `validation.failed`; сообщения для людей придумай сам.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Registration {
    pub username: String,
    pub email: String,
    pub age: u8,
    pub password: String,
}

#[derive(Debug, PartialEq)]
pub struct FieldError {
    pub field: &'static str,
    pub code: &'static str,
}

pub fn validate(reg: &Registration) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if reg.username.chars().count() < 3 {
        errors.push(FieldError { field: "username", code: "too_short" });
    }
    if !reg.email.contains('@') {
        errors.push(FieldError { field: "email", code: "invalid" });
    }
    if reg.age < 13 {
        errors.push(FieldError { field: "age", code: "too_young" });
    }
    if reg.password.chars().count() < 8 {
        errors.push(FieldError { field: "password", code: "too_short" });
    }
    errors
}

#[derive(Debug, PartialEq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
    pub details: Vec<FieldError>,
}

pub fn parse_registration(body: &[u8]) -> Result<Registration, ApiError> {
    let reg: Registration = serde_json::from_slice(body).map_err(|_| ApiError {
        status: 400,
        code: "bad_request",
        message: "плохой запрос".to_string(),
        details: Vec::new(),
    })?;
    let details = validate(&reg);
    if details.is_empty() {
        Ok(reg)
    } else {
        Err(ApiError { status: 400, code: "bad_request", message: "проверь поля".to_string(), details })
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Registration, ApiError> {
        parse_registration(text.as_bytes())
    }

    #[test]
    fn valid_body_passes() {
        let reg = parse(r#"{"username":"alice","email":"a@x.io","age":30,"password":"correct horse"}"#).expect("корректное тело");
        assert_eq!(reg.username, "alice", "поля на месте");
    }

    #[test]
    fn not_json_is_400() {
        for body in ["", "{", "привет", r#"{"username":"a""#, "\u{0}"] {
            let err = parse(body).expect_err("это не JSON");
            assert_eq!(err.status, 400, "не JSON, а {body:?}: клиент прислал мусор, это 400");
            assert_eq!(err.code, "request.malformed", "код для битого тела");
        }
        let err = parse_registration(&[0xff, 0xfe, 0x00]).expect_err("не UTF-8");
        assert_eq!(err.status, 400, "байты, которые не текст, тоже мусор");
    }

    #[test]
    fn wrong_shape_is_422() {
        for body in [r#"{"username":"alice"}"#, r#"{"username":1,"email":"a@x.io","age":30,"password":"correct horse"}"#, "[]", "null"] {
            let err = parse(body).expect_err("форма не подходит");
            assert_eq!(err.status, 422, "JSON корректный, но не та форма: {body}");
            assert_eq!(err.code, "request.invalid_body", "код неверной формы");
        }
    }

    #[test]
    fn values_never_come_back() {
        let body = r#"{"username":"alice","email":"a@x.io","age":"hunter2hunter2","password":"hunter2hunter2"}"#;
        let err = parse(body).expect_err("возраст строкой");
        let shown = format!("{} {} {:?}", err.code, err.message, err.details);
        assert!(!shown.contains("hunter2"), "ошибка serde цитирует присланное значение, а пароль нельзя возвращать: {shown}");
    }

    #[test]
    fn all_violations_come_at_once() {
        let err = parse(r#"{"username":"ab","email":"nope","age":10,"password":"123"}"#).expect_err("всё неверно");
        assert_eq!(err.status, 422, "значения не годятся: 422");
        assert_eq!(err.code, "validation.failed", "код проверки значений");
        let got: Vec<(&str, &str)> = err.details.iter().map(|d| (d.field, d.code)).collect();
        assert_eq!(
            got,
            vec![("username", "too_short"), ("email", "invalid"), ("age", "too_young"), ("password", "too_short")],
            "клиент должен увидеть все ошибки за один раз, в порядке полей"
        );
    }

    #[test]
    fn one_bad_field_is_enough_for_422() {
        let err = parse(r#"{"username":"alice","email":"a@x.io","age":12,"password":"correct horse"}"#).expect_err("только возраст");
        assert_eq!(err.details, vec![FieldError { field: "age", code: "too_young" }], "ровно одно нарушение");
    }
}
