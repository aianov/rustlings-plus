// Мобильный клиент шлёт регистрацию в camelCase: `displayName`, `birthYear`, `marketingOptIn`.
// А в Rust поля принято писать в snake_case, и менять имена в коде ради клиента незачем.
// Цель: `RegisterDto` читается из такого JSON; `displayName` и `birthYear` можно не присылать,
// `marketingOptIn` без указания означает «нет». Обязательные поля без значения это ошибка разбора.
// TODO: Допиши атрибуты serde на `RegisterDto`, чтобы тесты проходили.

use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
pub struct RegisterDto {
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub birth_year: Option<u16>,
    pub marketing_opt_in: bool,
}

pub fn parse_register(json: &str) -> Result<RegisterDto, serde_json::Error> {
    serde_json::from_str(json)
}

fn main() {
    println!("{:?}", parse_register(r#"{"username":"alice","email":"a@b.c"}"#));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_body_in_camel_case() {
        let dto = parse_register(
            r#"{"username":"alice","email":"alice@example.com","displayName":"Alice","birthYear":1999,"marketingOptIn":true}"#,
        )
        .expect("camelCase поля должны читаться");
        assert_eq!(dto.display_name.as_deref(), Some("Alice"), "displayName попадает в display_name");
        assert_eq!(dto.birth_year, Some(1999), "birthYear попадает в birth_year");
        assert!(dto.marketing_opt_in, "marketingOptIn попадает в marketing_opt_in");
    }

    #[test]
    fn optional_fields_may_be_absent() {
        let dto = parse_register(r#"{"username":"bob","email":"bob@example.com"}"#)
            .expect("необязательные поля можно не присылать");
        assert_eq!(dto.display_name, None, "displayName не пришло: None");
        assert_eq!(dto.birth_year, None, "birthYear не пришло: None");
        assert!(!dto.marketing_opt_in, "без marketingOptIn подразумевается false");
    }

    #[test]
    fn required_fields_are_required() {
        assert!(parse_register(r#"{"email":"a@b.c"}"#).is_err(), "без username разбор должен провалиться");
        assert!(parse_register(r#"{"username":"a"}"#).is_err(), "без email разбор должен провалиться");
    }

    #[test]
    fn wrong_types_are_rejected() {
        assert!(
            parse_register(r#"{"username":"a","email":"b","birthYear":"1999"}"#).is_err(),
            "год строкой вместо числа: ошибка разбора, а не молчаливое преобразование"
        );
        assert!(
            parse_register(r#"{"username":"a","email":"b","birthYear":-5}"#).is_err(),
            "отрицательный год не помещается в u16"
        );
    }

    #[test]
    fn snake_case_names_are_not_accepted_from_clients() {
        let dto = parse_register(r#"{"username":"a","email":"b","display_name":"X"}"#)
            .expect("лишние поля пока допустимы");
        assert_eq!(dto.display_name, None, "клиентский формат это displayName, а display_name лишнее поле");
    }
}
