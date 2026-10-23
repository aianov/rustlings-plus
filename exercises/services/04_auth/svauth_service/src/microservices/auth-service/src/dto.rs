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
    todo!()
}

pub fn parse_login(body: &[u8]) -> Result<Credentials, BodyError> {
    todo!()
}

pub fn parse_refresh(body: &[u8]) -> Result<String, BodyError> {
    todo!()
}
