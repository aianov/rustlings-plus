// Что делать с полями, которых в DTO нет? Для входа в систему лишнее поле подозрительно: клиент
// шлёт то, чего сервер не понимает, и вместо молчаливого игнора лучше сказать об этом. А для
// поиска клиенты (веб, мобильные, аналитика) спокойно добавляют свои параметры, и падать на них
// нельзя. Тесты описывают оба поведения; ошибка для строгого DTO называет лишнее поле.
// TODO: Настрой `LoginDto` и `SearchDto` так, чтобы тесты стали зелёными.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LoginDto {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchDto {
    pub query: String,
    pub limit: Option<u32>,
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_accepts_exactly_its_fields() {
        let dto: LoginDto = serde_json::from_str(r#"{"username":"a","password":"b"}"#).unwrap();
        assert_eq!((dto.username.as_str(), dto.password.as_str()), ("a", "b"), "обычный вход разбирается");
    }

    #[test]
    fn login_rejects_unknown_fields_and_names_them() {
        let err = serde_json::from_str::<LoginDto>(r#"{"username":"a","password":"b","isAdmin":true}"#)
            .expect_err("лишнее поле isAdmin в строгом DTO должно быть ошибкой");
        assert!(err.to_string().contains("isAdmin"), "ошибка должна называть лишнее поле, а сказала: {err}");
    }

    #[test]
    fn search_ignores_extra_parameters() {
        let dto: SearchDto = serde_json::from_str(r#"{"query":"rust","utm_source":"mail","limit":5}"#)
            .expect("лишние параметры в поиске должны игнорироваться");
        assert_eq!(dto.limit, Some(5), "известные поля при этом читаются");
    }
}
