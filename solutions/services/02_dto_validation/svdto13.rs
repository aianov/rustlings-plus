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
        let name = raw.trim().to_lowercase();
        let len = name.chars().count();
        if len < 3 {
            return Err(FieldError { field: "username", code: "too_short" });
        }
        if len > 20 {
            return Err(FieldError { field: "username", code: "too_long" });
        }
        let starts_with_letter = name.chars().next().is_some_and(|c| c.is_ascii_lowercase());
        let allowed = name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !starts_with_letter || !allowed {
            return Err(FieldError { field: "username", code: "invalid_format" });
        }
        Ok(Username(name))
    }
}

impl TryFrom<&str> for Email {
    type Error = FieldError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        let email = raw.trim().to_lowercase();
        let ok = match email.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && !domain.contains('@')
                    && !email.chars().any(char::is_whitespace)
                    && domain.split('.').count() >= 2
                    && domain.split('.').all(|label| !label.is_empty())
            }
            None => false,
        };
        if ok { Ok(Email(email)) } else { Err(FieldError { field: "email", code: "invalid_format" }) }
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
        let username = Username::try_from(dto.username.as_str());
        let email = Email::try_from(dto.email.as_str());
        match (username, email) {
            (Ok(username), Ok(email)) => Ok(Registration { username, email }),
            (username, email) => Err([username.err(), email.err()].into_iter().flatten().collect()),
        }
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
