use crate::rules::{CharLen, Matches, Rule};
use crate::validator::FieldError;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Username(String);

#[derive(Debug, Clone, PartialEq)]
pub struct Email(String);

fn failure(code: &'static str, message: String) -> FieldError {
    FieldError { path: String::new(), code, message }
}

impl Username {
    pub fn parse(raw: &str) -> Result<Username, FieldError> {
        let name = raw.trim().to_lowercase();
        if let Some((code, message)) = (CharLen { min: 3, max: 20 }).check(name.as_str()) {
            return Err(failure(code, message));
        }
        let pattern = Matches::new("[a-z][a-z0-9_]*").expect("шаблон ника верен");
        if let Some((code, message)) = pattern.check(name.as_str()) {
            return Err(failure(code, message));
        }
        Ok(Username(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Email {
    pub fn parse(raw: &str) -> Result<Email, FieldError> {
        let email = raw.trim().to_lowercase();
        let ok = match email.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && local.chars().count() <= 64
                    && !email.chars().any(char::is_whitespace)
                    && !domain.contains('@')
                    && domain.split('.').count() >= 2
                    && domain.split('.').all(|label| !label.is_empty())
            }
            None => false,
        };
        if ok { Ok(Email(email)) } else { Err(failure("invalid_format", "это не похоже на адрес почты".to_string())) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
