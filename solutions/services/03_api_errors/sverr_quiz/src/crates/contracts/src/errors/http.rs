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
    let status = err.code.http_status();
    let message = match (&err.message, err.code.is_client_fault()) {
        (Some(text), true) => text.as_str(),
        _ => err.code.public_message(),
    };
    let mut error = json!({"code": err.code.as_str(), "message": message, "request_id": request_id});
    if err.code.is_client_fault() && !err.details.is_empty() {
        error["details"] = err.details.iter().map(|d| json!({"field": d.field, "code": d.code})).collect();
    }
    let mut headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        ("x-request-id".to_string(), request_id.to_string()),
    ];
    if status == 401 {
        headers.push(("www-authenticate".to_string(), "Bearer".to_string()));
    }
    if let (429 | 503, Some(secs)) = (status, err.retry_after_secs) {
        headers.push(("retry-after".to_string(), secs.to_string()));
    }
    HttpReply { status, headers, body: json!({"error": error}).to_string() }
}
