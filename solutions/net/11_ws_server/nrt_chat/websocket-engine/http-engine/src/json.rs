use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Request, Response};

impl Response {
    pub fn json<T: Serialize>(status: u16, value: &T) -> Response {
        match serde_json::to_vec(value) {
            Ok(body) => {
                let mut response = Response::new(status);
                response.headers.append("Content-Type", "application/json");
                response.body = body;
                response
            }
            Err(_) => Response::error(500, "internal error"),
        }
    }

    pub fn error(status: u16, message: &str) -> Response {
        let mut response = Response::new(status);
        response.headers.append("Content-Type", "application/json");
        response.body = serde_json::json!({ "error": message }).to_string().into_bytes();
        response
    }
}

impl Request {
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, Response> {
        let unsupported = || Response::error(415, "Content-Type must be application/json (UTF-8)");
        let content_type = self.headers.get("content-type").ok_or_else(unsupported)?;
        let mut parts = content_type.split(';');
        if !parts.next().unwrap_or("").trim().eq_ignore_ascii_case("application/json") {
            return Err(unsupported());
        }
        for param in parts {
            let Some((name, value)) = param.split_once('=') else { continue };
            if name.trim().eq_ignore_ascii_case("charset") && !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8") {
                return Err(unsupported());
            }
        }
        serde_json::from_slice(&self.body).map_err(|e| Response::error(400, &format!("invalid JSON: {e}")))
    }
}
