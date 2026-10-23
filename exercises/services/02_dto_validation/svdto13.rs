// «Parse, don't validate»: вместо `String`, про который нигде не написано, проверен ли он, у нас
// типы `Username` и `Email`. Получить их можно только через `TryFrom<&str>`, и значит, любой код,
// принявший `Username`, точно получил нормализованное и допустимое значение. Конструктора без
// проверки быть не должно (поле приватное). `Registration::try_from(dto)` собирает оба типа
// и возвращает ВСЕ ошибки сразу.
// Правила: ник нормализуется (trim, нижний регистр) и проверяется (3..=20, a-z0-9_, с буквы);
// почта нормализуется и проверяется (один `@`, непустая локальная часть, домен с точкой).

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub field: &'static str,
    pub code: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Username(String);

#[derive(Debug, Clone, PartialEq)]
pub struct Email(String);

impl Username {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Email {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for Username {
    type Error = FieldError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<&str> for Email {
    type Error = FieldError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

pub struct RegisterDto {
    pub username: String,
    pub email: String,
}

#[derive(Debug, PartialEq)]
pub struct Registration {
    pub username: Username,
    pub email: Email,
}

impl TryFrom<RegisterDto> for Registration {
    type Error = Vec<FieldError>;

    fn try_from(dto: RegisterDto) -> Result<Self, Self::Error> {
        todo!()
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_is_normalized_and_checked() {
        assert_eq!(Username::try_from("  Bob_7 ").unwrap().as_str(), "bob_7", "ник нормализуется");
        assert_eq!(Username::try_from("ab").unwrap_err().code, "too_short", "короткий");
        assert_eq!(Username::try_from(&*"a".repeat(21)).unwrap_err().code, "too_long", "длинный");
        assert_eq!(Username::try_from("7bob").unwrap_err().code, "invalid_format", "начинается с цифры");
        assert_eq!(Username::try_from("bo b").unwrap_err().code, "invalid_format", "пробел внутри");
    }

    #[test]
    fn email_is_normalized_and_checked() {
        assert_eq!(Email::try_from(" Bob@Mail.EXAMPLE ").unwrap().as_str(), "bob@mail.example", "почта нормализуется");
        for bad in ["bob", "bob@", "@x.y", "bob@x", "a@b@c.d", "b ob@x.y", "bob@x..y"] {
            assert_eq!(Email::try_from(bad).unwrap_err().field, "email", "{bad:?} не адрес");
        }
    }

    #[test]
    fn newtypes_display_their_value() {
        assert_eq!(Username::try_from("alice").unwrap().to_string(), "alice", "Display отдаёт значение");
    }

    #[test]
    fn registration_collects_every_error() {
        let bad = RegisterDto { username: "x".to_string(), email: "nope".to_string() };
        assert_eq!(
            Registration::try_from(bad).unwrap_err(),
            vec![
                FieldError { field: "username", code: "too_short" },
                FieldError { field: "email", code: "invalid_format" },
            ],
            "обе ошибки сразу, в порядке полей"
        );
        let one_bad = RegisterDto { username: "alice".to_string(), email: "nope".to_string() };
        assert_eq!(Registration::try_from(one_bad).unwrap_err().len(), 1, "если плох один, ошибка одна");
    }

    #[test]
    fn registration_holds_only_valid_values() {
        let ok = Registration::try_from(RegisterDto { username: " Alice ".to_string(), email: "A@B.IO".to_string() }).unwrap();
        assert_eq!((ok.username.as_str(), ok.email.as_str()), ("alice", "a@b.io"), "внутри уже нормальные значения");
    }
}
