use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub struct Request {
    pub id: u64,
    pub service: String,
    pub method: String,
    pub data: Value,
}

#[derive(Debug, PartialEq)]
pub struct Failure {
    pub id: Option<u64>,
    pub code: &'static str,
    pub message: String,
}

pub fn parse_request(text: &str) -> Result<Request, Failure> {
    let value: Value = serde_json::from_str(text).map_err(|e| Failure { id: None, code: "bad_json", message: e.to_string() })?;
    let Some(object) = value.as_object() else {
        return Err(Failure { id: None, code: "bad_request", message: "ожидался JSON-объект".to_string() });
    };
    let Some(id) = object.get("id").and_then(Value::as_u64) else {
        return Err(Failure { id: None, code: "bad_request", message: "id должен быть неотрицательным целым".to_string() });
    };
    let name = |field: &str| -> Result<String, Failure> {
        match object.get(field).and_then(Value::as_str) {
            Some(s) if !s.is_empty() && !s.contains('.') => Ok(s.to_string()),
            _ => Err(Failure { id: Some(id), code: "bad_request", message: format!("поле {field} должно быть непустой строкой без точки") }),
        }
    };
    let service = name("service")?;
    let method = name("method")?;
    let data = object.get("data").cloned().unwrap_or(Value::Null);
    Ok(Request { id, service, method, data })
}

pub fn render_ok(id: u64, data: Value) -> String {
    json!({ "id": id, "ok": true, "data": data }).to_string()
}

pub fn render_err(failure: &Failure) -> String {
    json!({ "id": failure.id, "ok": false, "error": { "code": failure.code, "message": failure.message } }).to_string()
}

pub fn render_event(event: &str, seq: u64, data: Value) -> String {
    json!({ "event": event, "data": data, "seq": seq }).to_string()
}
