use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub alg: String,
    pub kid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub sub: u64,
    pub name: String,
    pub role: String,
    pub iss: String,
    pub aud: String,
    pub iat: u64,
    pub nbf: u64,
    pub exp: u64,
    pub jti: String,
    #[serde(default)]
    pub sid: String,
}

pub fn to_json<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

pub fn header_from(bytes: &[u8]) -> Option<Header> {
    serde_json::from_slice(bytes).ok()
}

pub fn claims_from(bytes: &[u8]) -> Option<Claims> {
    serde_json::from_slice(bytes).ok()
}
