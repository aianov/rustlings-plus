use serde::de::DeserializeOwned;

use crate::headers::Headers;
use crate::method::Method;
use crate::parse::{ParseError, percent_decode};
use crate::response::Response;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Http10,
    Http11,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    items: Vec<(String, String)>,
}

impl Params {
    pub fn new() -> Params {
        Params::default()
    }

    pub fn push(&mut self, name: &str, value: String) {
        self.items.push((name.to_string(), value));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|(own, _)| own == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub target: String,
    pub version: Version,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub params: Params,
}

impl Request {
    pub fn new(method: Method, target: &str) -> Request {
        Request {
            method,
            target: target.to_string(),
            version: Version::Http11,
            headers: Headers::new(),
            body: Vec::new(),
            params: Params::new(),
        }
    }

    pub fn raw_path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn path(&self) -> Result<String, ParseError> {
        percent_decode(self.raw_path(), false)
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name)
    }

    pub fn query(&self) -> Result<Vec<(String, String)>, ParseError> {
        let Some((_, query)) = self.target.split_once('?') else {
            return Ok(Vec::new());
        };
        let mut pairs = Vec::new();
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            pairs.push((percent_decode(key, true)?, percent_decode(value, true)?));
        }
        Ok(pairs)
    }

    pub fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.headers.get_all("connection") {
            for token in value.split(',').map(str::trim) {
                close |= token.eq_ignore_ascii_case("close");
                keep |= token.eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.version == Version::Http11)
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, Response> {
        let unsupported = || Response::error(415, "тело должно быть application/json в UTF-8");
        let content_type = self.headers.get("content-type").ok_or_else(unsupported)?;
        let mut parts = content_type.split(';');
        let media = parts.next().unwrap_or_default().trim();
        if !media.eq_ignore_ascii_case("application/json") {
            return Err(unsupported());
        }
        for param in parts {
            let Some((name, value)) = param.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            if name.trim().eq_ignore_ascii_case("charset") && !value.eq_ignore_ascii_case("utf-8") {
                return Err(unsupported());
            }
        }
        serde_json::from_slice(&self.body)
            .map_err(|error| Response::error(400, &format!("тело не разобралось: {error}")))
    }
}
