use std::future::Future;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::headers::Headers;
use crate::parse::{ParseError, is_token};
use crate::status::StatusCode;
use crate::upgrade::{UpgradeHook, Upgraded};

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub stream: Option<mpsc::Receiver<Vec<u8>>>,
    pub upgrade: Option<UpgradeHook>,
}

impl Response {
    pub fn new(status: u16) -> Response {
        Response {
            status: StatusCode::new(status).unwrap_or(StatusCode::INTERNAL),
            headers: Headers::new(),
            body: Vec::new(),
            stream: None,
            upgrade: None,
        }
    }

    pub fn on_upgrade<F, Fut>(mut self, f: F) -> Response
    where
        F: FnOnce(Upgraded) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.upgrade = Some(UpgradeHook::new(f));
        self
    }

    pub fn streaming(status: u16, content_type: &str, chunks: mpsc::Receiver<Vec<u8>>) -> Response {
        let mut response = Response::new(status);
        response.headers.append("Content-Type", content_type);
        response.stream = Some(chunks);
        response
    }

    pub fn text(status: u16, body: &str) -> Response {
        let mut response = Response::new(status);
        response
            .headers
            .append("Content-Type", "text/plain; charset=utf-8");
        response.body = body.as_bytes().to_vec();
        response
    }

    pub fn json<T: Serialize>(status: u16, value: &T) -> Response {
        match serde_json::to_vec(value) {
            Ok(body) => {
                let mut response = Response::new(status);
                response.headers.append("Content-Type", "application/json");
                response.body = body;
                response
            }
            Err(_) => Response::error(500, "ответ не сериализовался"),
        }
    }

    pub fn error(status: u16, message: &str) -> Response {
        let mut response = Response::json(status, &serde_json::json!({ "error": message }));
        if !response.status.allows_body() {
            response.body.clear();
        }
        response
    }

    pub fn header(&mut self, name: &str, value: &str) -> Result<(), ParseError> {
        if name.is_empty() || !name.chars().all(is_token) {
            return Err(ParseError::BadHeader);
        }
        if value.chars().any(|c| c.is_control() && c != '\t') {
            return Err(ParseError::BadHeader);
        }
        self.headers.append(name, value);
        Ok(())
    }

    pub fn to_bytes(&self, head_only: bool) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {}\r\n", self.status).into_bytes();
        for (name, value) in self.headers.iter() {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        let allows = self.status.allows_body();
        let framed = self.stream.is_some()
            || self.headers.contains("content-length")
            || self.headers.contains("transfer-encoding");
        if allows && !framed {
            out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if allows && !head_only {
            out.extend_from_slice(&self.body);
        }
        out
    }
}
