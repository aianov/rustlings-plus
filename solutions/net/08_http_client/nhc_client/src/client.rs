use std::io;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::pool::Pool;
use crate::url::{Headers, RedirectError, Url, UrlError, parse_url, resolve};
use crate::wire::{RawResponse, RequestError, build_request, fill, read_response};

#[derive(Debug, Clone)]
pub struct Config {
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_redirects: usize,
    pub max_idle_per_host: usize,
    pub idle_ttl: Duration,
    pub max_body: usize,
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
        Request { method: method.to_string(), url: url.to_string(), headers: Vec::new(), body: None }
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

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub url: Url,
    pub redirects: usize,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[derive(Debug)]
pub enum Error {
    Url(UrlError),
    Request(RequestError),
    ConnectTimeout,
    TotalTimeout,
    Connect(io::Error),
    Io(io::Error),
    Protocol(String),
    TooManyRedirects,
    BadRedirect(RedirectError),
    Status { code: u16, body: String },
    Decode(String),
}

#[derive(Debug)]
enum ExchangeError {
    Stale(io::Error),
    Failed(io::Error),
}

fn reset_like(kind: io::ErrorKind) -> bool {
    matches!(kind, io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted)
}

async fn exchange(stream: &mut TcpStream, wire: &[u8], method: &str, max: usize) -> Result<RawResponse, ExchangeError> {
    if let Err(e) = stream.write_all(wire).await {
        return Err(if reset_like(e.kind()) { ExchangeError::Stale(e) } else { ExchangeError::Failed(e) });
    }
    let mut buf = Vec::new();
    match fill(stream, &mut buf).await {
        Ok(0) => return Err(ExchangeError::Stale(io::ErrorKind::UnexpectedEof.into())),
        Ok(_) => {}
        Err(e) if reset_like(e.kind()) => return Err(ExchangeError::Stale(e)),
        Err(e) => return Err(ExchangeError::Failed(e)),
    }
    read_response(stream, &mut buf, method, max).await.map_err(ExchangeError::Failed)
}

fn io_error(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::InvalidData { Error::Protocol(e.to_string()) } else { Error::Io(e) }
}

fn is_idempotent(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS" | "TRACE")
}

fn without(headers: &mut Headers, names: &[&str]) {
    headers.retain(|(n, _)| !names.contains(&n.to_ascii_lowercase().as_str()));
}

pub struct Client {
    config: Config,
    pool: Mutex<Pool<TcpStream>>,
}

impl Client {
    pub fn new(config: Config) -> Client {
        let pool = Pool::new(config.max_idle_per_host, config.idle_ttl);
        Client { config, pool: Mutex::new(pool) }
    }

    pub fn idle_connections(&self) -> usize {
        self.pool.lock().unwrap().idle()
    }

    async fn connect(&self, url: &Url) -> Result<TcpStream, Error> {
        match timeout(self.config.connect_timeout, TcpStream::connect((url.host.as_str(), url.port))).await {
            Err(_) => Err(Error::ConnectTimeout),
            Ok(Err(e)) => Err(Error::Connect(e)),
            Ok(Ok(stream)) => Ok(stream),
        }
    }

    pub async fn get(&self, url: &str) -> Result<Response, Error> {
        self.send(Request::get(url)).await
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let response = self.send(Request::get(url).header("Accept", "application/json")).await?;
        decode(&response)
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(&self, url: &str, body: &B) -> Result<T, Error> {
        let bytes = serde_json::to_vec(body).map_err(|e| Error::Decode(e.to_string()))?;
        let request = Request::new("POST", url).header("Content-Type", "application/json").header("Accept", "application/json").body(bytes);
        let response = self.send(request).await?;
        decode(&response)
    }

    pub async fn send(&self, request: Request) -> Result<Response, Error> {
        timeout(self.config.total_timeout, self.follow(request)).await.unwrap_or(Err(Error::TotalTimeout))
    }

    async fn follow(&self, request: Request) -> Result<Response, Error> {
        let mut method = request.method;
        let mut url = parse_url(&request.url).map_err(Error::Url)?;
        let mut headers = request.headers;
        let mut body = request.body;
        let mut hops = 0;
        loop {
            let raw = self.send_once(&method, &url, &headers, body.as_deref()).await?;
            let redirect = matches!(raw.status, 301 | 302 | 303 | 307 | 308);
            let location = raw.header("location").map(str::to_string);
            let Some(location) = location.filter(|_| redirect) else {
                return Ok(Response { status: raw.status, headers: raw.headers, body: raw.body, url, redirects: hops });
            };
            if hops >= self.config.max_redirects {
                return Err(Error::TooManyRedirects);
            }
            hops += 1;
            let next = resolve(&url, &location).map_err(Error::BadRedirect)?;
            if (raw.status == 303 && method != "HEAD") || (matches!(raw.status, 301 | 302) && method == "POST") {
                method = "GET".to_string();
                body = None;
                without(&mut headers, &["content-type", "content-length", "content-encoding", "transfer-encoding"]);
            }
            if (next.host.as_str(), next.port) != (url.host.as_str(), url.port) {
                without(&mut headers, &["authorization", "cookie", "proxy-authorization"]);
            }
            url = next;
        }
    }

    async fn send_once(&self, method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>) -> Result<RawResponse, Error> {
        let wire = build_request(method, url, headers, body).map_err(Error::Request)?;
        let key = (url.host.clone(), url.port);
        let pooled = self.pool.lock().unwrap().take(&key);
        let (mut stream, reused) = match pooled {
            Some(stream) => (stream, true),
            None => (self.connect(url).await?, false),
        };
        let max = self.config.max_body;
        let outcome = match exchange(&mut stream, &wire, method, max).await {
            Err(ExchangeError::Stale(_)) if reused && is_idempotent(method) => {
                stream = self.connect(url).await?;
                exchange(&mut stream, &wire, method, max).await
            }
            other => other,
        };
        match outcome {
            Ok(raw) => {
                if raw.reusable {
                    self.pool.lock().unwrap().put(key, stream);
                }
                Ok(raw)
            }
            Err(ExchangeError::Stale(e) | ExchangeError::Failed(e)) => Err(io_error(e)),
        }
    }
}

fn decode<T: DeserializeOwned>(response: &Response) -> Result<T, Error> {
    if !(200..300).contains(&response.status) {
        return Err(Error::Status { code: response.status, body: response.text() });
    }
    serde_json::from_slice(&response.body).map_err(|e| Error::Decode(e.to_string()))
}
