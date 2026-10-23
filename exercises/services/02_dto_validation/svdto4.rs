// Пользователь отправляет форму регистрации, получает «имя слишком короткое», исправляет, отправляет
// снова и получает «пароль слишком короткий», исправляет, и так пять раз. Валидатор возвращается
// на первой же ошибке. Цель: `validate` отдаёт ВСЕ ошибки сразу, в порядке полей формы, у каждой
// есть поле, машинный код и сообщение. Правила: username 3..=20 символов, email содержит `@`,
// password от 8 символов, age (если прислан) 13..=120.

#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub code: &'static str,
    pub message: String,
}

fn error(field: &str, code: &'static str, message: &str) -> FieldError {
    FieldError { field: field.to_string(), code, message: message.to_string() }
}

pub trait Validate {
    fn validate(&self) -> Result<(), Vec<FieldError>>;
}

pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub age: Option<u8>,
}

impl Validate for RegisterForm {
    fn validate(&self) -> Result<(), Vec<FieldError>> {
        if self.username.chars().count() < 3 || self.username.chars().count() > 20 {
            return Err(vec![error("username", "bad_length", "от 3 до 20 символов")]);
        }
        if !self.email.contains('@') {
            return Err(vec![error("email", "invalid_format", "нужен адрес с @")]);
        }
        if self.password.chars().count() < 8 {
            return Err(vec![error("password", "too_short", "минимум 8 символов")]);
        }
        if let Some(age) = self.age {
            if !(13..=120).contains(&age) {
                return Err(vec![error("age", "out_of_range", "от 13 до 120")]);
            }
        }
        Ok(())
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> RegisterForm {
        RegisterForm {
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "correct horse".to_string(),
            age: Some(30),
        }
    }

    #[test]
    fn valid_form_passes() {
        assert_eq!(good().validate(), Ok(()), "правильная форма не должна давать ошибок");
    }

    #[test]
    fn all_problems_come_at_once_in_field_order() {
        let form = RegisterForm {
            username: "ab".to_string(),
            email: "no-at-sign".to_string(),
            password: "short".to_string(),
            age: Some(5),
        };
        let errors = form.validate().unwrap_err();
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(
            fields,
            vec!["username", "email", "password", "age"],
            "должны прийти все четыре ошибки сразу, в порядке полей формы"
        );
    }

    #[test]
    fn errors_carry_stable_codes() {
        let form = RegisterForm { password: "x".to_string(), age: Some(200), ..good() };
        let errors = form.validate().unwrap_err();
        let codes: Vec<(&str, &str)> = errors.iter().map(|e| (e.field.as_str(), e.code)).collect();
        assert_eq!(codes, vec![("password", "too_short"), ("age", "out_of_range")], "поле и машинный код");
        assert!(errors.iter().all(|e| !e.message.is_empty()), "у каждой ошибки есть человеческое сообщение");
    }

    #[test]
    fn missing_optional_age_is_fine() {
        let form = RegisterForm { age: None, ..good() };
        assert_eq!(form.validate(), Ok(()), "возраст необязателен");
    }

    #[test]
    fn boundaries() {
        let form = RegisterForm { username: "abc".to_string(), password: "12345678".to_string(), age: Some(13), ..good() };
        assert_eq!(form.validate(), Ok(()), "границы 3, 8 и 13 допустимы");
        let form = RegisterForm { username: "a".repeat(21), age: Some(121), ..good() };
        assert_eq!(form.validate().unwrap_err().len(), 2, "21 символ и возраст 121 это две ошибки");
    }
}
