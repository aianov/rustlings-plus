use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, PartialEq)]
pub enum BodyError {
    Malformed,
    Invalid(Vec<(String, String)>),
}

fn object(body: &[u8]) -> Result<serde_json::Map<String, Value>, BodyError> {
    match serde_json::from_slice(body) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(BodyError::Malformed),
    }
}

fn text<'a>(map: &'a serde_json::Map<String, Value>, field: &str) -> Option<&'a str> {
    map.get(field).and_then(Value::as_str)
}

fn push(errors: &mut Vec<(String, String)>, field: &str, code: &str) {
    errors.push((field.to_string(), code.to_string()));
}

pub fn parse_register(body: &[u8]) -> Result<NewUser, BodyError> {
    let map = object(body)?;
    let mut errors = Vec::new();
    let username = text(&map, "username").map(|s| s.trim().to_lowercase());
    match username.as_deref() {
        None => push(&mut errors, "username", "required"),
        Some(name) => {
            let len = name.chars().count();
            if len < 3 {
                push(&mut errors, "username", "too_short");
            } else if len > 20 {
                push(&mut errors, "username", "too_long");
            } else if !name.starts_with(|c: char| c.is_ascii_lowercase()) || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                push(&mut errors, "username", "invalid");
            }
        }
    }
    let email = text(&map, "email").map(|s| s.trim().to_lowercase());
    match email.as_deref() {
        None => push(&mut errors, "email", "required"),
        Some(address) => {
            let valid = match address.split_once('@') {
                Some((local, domain)) => {
                    !local.is_empty()
                        && !domain.contains('@')
                        && !address.contains(char::is_whitespace)
                        && domain.split('.').count() >= 2
                        && domain.split('.').all(|label| !label.is_empty())
                }
                None => false,
            };
            if !valid {
                push(&mut errors, "email", "invalid");
            }
        }
    }
    let password = text(&map, "password");
    match password {
        None => push(&mut errors, "password", "required"),
        Some(pw) => {
            let len = pw.chars().count();
            if len < 8 {
                push(&mut errors, "password", "too_short");
            } else if len > 128 {
                push(&mut errors, "password", "too_long");
            }
        }
    }
    if !errors.is_empty() {
        return Err(BodyError::Invalid(errors));
    }
    Ok(NewUser { username: username.unwrap_or_default(), email: email.unwrap_or_default(), password: password.unwrap_or_default().to_string() })
}

pub fn parse_login(body: &[u8]) -> Result<Credentials, BodyError> {
    let map = object(body)?;
    let mut errors = Vec::new();
    let username = text(&map, "username");
    let password = text(&map, "password");
    if username.is_none() {
        push(&mut errors, "username", "required");
    }
    if password.is_none() {
        push(&mut errors, "password", "required");
    }
    match (username, password) {
        (Some(username), Some(password)) => Ok(Credentials { username: username.trim().to_lowercase(), password: password.to_string() }),
        _ => Err(BodyError::Invalid(errors)),
    }
}

pub fn parse_refresh(body: &[u8]) -> Result<String, BodyError> {
    let map = object(body)?;
    match text(&map, "refresh") {
        Some(token) if !token.is_empty() => Ok(token.to_string()),
        _ => Err(BodyError::Invalid(vec![("refresh".to_string(), "required".to_string())])),
    }
}
