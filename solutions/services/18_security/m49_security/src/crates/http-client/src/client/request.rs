use std::sync::Arc;
use std::time::Duration;

use tokio_rustls::rustls::ClientConfig;

use crate::guard::AddrGuard;
use crate::url::{Headers, Url};

#[derive(Debug, Clone)]
pub struct Config {
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_redirects: usize,
    pub max_idle_per_host: usize,
    pub idle_ttl: Duration,
    pub max_body: usize,
    pub tls: Option<Arc<ClientConfig>>,
    pub tls_name: Option<String>,
    pub guard: Option<AddrGuard>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            connect_timeout: Duration::from_secs(2),
            total_timeout: Duration::from_secs(30),
            max_redirects: 5,
            max_idle_per_host: 4,
            idle_ttl: Duration::from_secs(30),
            max_body: 8 << 20,
            tls: None,
            tls_name: None,
            guard: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Headers,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn new(method: &str, url: &str) -> Request {
        Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn get(url: &str) -> Request {
        Request::new("GET", url)
    }

    pub fn header(mut self, name: &str, value: &str) -> Request {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Request {
        self.body = Some(body.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub url: Url,
    pub redirects: usize,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}
