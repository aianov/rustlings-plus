use super::ApiError;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpReply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub fn http_reply(err: &ApiError, request_id: &str) -> HttpReply {
    todo!()
}
