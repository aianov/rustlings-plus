use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpReply {
    pub status: u16,
    pub body: String,
}

impl HttpReply {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

pub fn ok(status: u16, body: serde_json::Value) -> HttpReply {
    HttpReply { status, body: body.to_string() }
}

pub fn failure(status: u16, code: &str, message: &str, details: &[(String, String)]) -> HttpReply {
    let mut error = json!({"code": code, "message": message});
    if !details.is_empty() {
        error["details"] = details.iter().map(|(field, code)| json!({"field": field, "code": code})).collect();
    }
    HttpReply { status, body: json!({"error": error}).to_string() }
}
