mod request;

use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::conn::Conn;
use crate::error::Error;
use crate::guard::pin;
use crate::pool::Pool;
use crate::url::{Headers, Url, UrlError, parse_url, resolve};
use crate::wire::{RawResponse, build_request, fill, read_response};

pub use request::{Config, Request, Response};

#[derive(Debug)]
enum ExchangeError {
    Stale(io::Error),
    Failed(io::Error),
}

fn reset_like(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    wire: &[u8],
    method: &str,
    max: usize,
) -> Result<RawResponse, ExchangeError> {
    if let Err(e) = stream.write_all(wire).await {
        return Err(if reset_like(e.kind()) {
            ExchangeError::Stale(e)
        } else {
            ExchangeError::Failed(e)
        });
    }
    let mut buf = Vec::new();
    match fill(stream, &mut buf).await {
        Ok(0) => return Err(ExchangeError::Stale(io::ErrorKind::UnexpectedEof.into())),
        Ok(_) => {}
        Err(e) if reset_like(e.kind()) => return Err(ExchangeError::Stale(e)),
        Err(e) => return Err(ExchangeError::Failed(e)),
    }
    read_response(stream, &mut buf, method, max)
        .await
        .map_err(ExchangeError::Failed)
}

fn io_error(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::InvalidData {
        Error::Protocol(e.to_string())
    } else {
        Error::Io(e)
    }
}

fn is_idempotent(method: &str) -> bool {
    matches!(
        method,
        "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS" | "TRACE"
    )
}

fn without(headers: &mut Headers, names: &[&str]) {
    headers.retain(|(n, _)| !names.contains(&n.to_ascii_lowercase().as_str()));
}

pub struct Client {
    config: Config,
    pool: Mutex<Pool<Conn>>,
}

impl Client {
    pub fn new(config: Config) -> Client {
        let pool = Pool::new(config.max_idle_per_host, config.idle_ttl);
        Client {
            config,
            pool: Mutex::new(pool),
        }
    }

    fn pool(&self) -> MutexGuard<'_, Pool<Conn>> {
        self.pool.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn idle_connections(&self) -> usize {
        self.pool().idle()
    }

    async fn connect(&self, url: &Url) -> Result<Conn, Error> {
        let limit = self.config.connect_timeout;
        let pinned = match &self.config.guard {
            Some(guard) => Some(pin(guard, url, limit).await?),
            None => None,
        };
        let dial = async {
            match pinned {
                Some(addr) => TcpStream::connect(addr).await,
                None => TcpStream::connect((url.host.as_str(), url.port)).await,
            }
        };
        let tcp = match timeout(limit, dial).await {
            Err(_) => return Err(Error::ConnectTimeout),
            Ok(Err(e)) => return Err(Error::Connect(e)),
            Ok(Ok(stream)) => stream,
        };
        let _ = tcp.set_nodelay(true);
        if !url.secure {
            return Ok(Conn::Plain(tcp));
        }
        let config = self
            .config
            .tls
            .clone()
            .ok_or(Error::Url(UrlError::Scheme))?;
        let name = self.config.tls_name.as_deref().unwrap_or(&url.host);
        match timeout(limit, Conn::secure(tcp, config, name)).await {
            Err(_) => Err(Error::ConnectTimeout),
            Ok(Err(e)) => Err(Error::Tls(e)),
            Ok(Ok(conn)) => Ok(conn),
        }
    }

    fn usable(&self, url: &str) -> Result<Url, Error> {
        let url = parse_url(url).map_err(Error::Url)?;
        if url.secure && self.config.tls.is_none() {
            return Err(Error::Url(UrlError::Scheme));
        }
        Ok(url)
    }

    pub async fn relay(&self, request: Request) -> Result<Response, Error> {
        let url = self.usable(&request.url)?;
        let once = self.send_once(
            &request.method,
            &url,
            &request.headers,
            request.body.as_deref(),
        );
        let raw = timeout(self.config.total_timeout, once)
            .await
            .unwrap_or(Err(Error::TotalTimeout))?;
        Ok(Response {
            status: raw.status,
            headers: raw.headers,
            body: raw.body,
            url,
            redirects: 0,
        })
    }

    pub async fn get(&self, url: &str) -> Result<Response, Error> {
        self.send(Request::get(url)).await
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let response = self
            .send(Request::get(url).header("Accept", "application/json"))
            .await?;
        decode(&response)
    }

    pub async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T, Error> {
        let bytes = serde_json::to_vec(body).map_err(|e| Error::Decode(e.to_string()))?;
        let request = Request::new("POST", url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(bytes);
        let response = self.send(request).await?;
        decode(&response)
    }

    pub async fn send(&self, request: Request) -> Result<Response, Error> {
        timeout(self.config.total_timeout, self.follow(request))
            .await
            .unwrap_or(Err(Error::TotalTimeout))
    }

    async fn follow(&self, request: Request) -> Result<Response, Error> {
        let mut method = request.method;
        let mut url = self.usable(&request.url)?;
        let mut headers = request.headers;
        let mut body = request.body;
        let mut hops = 0;
        loop {
            let raw = self
                .send_once(&method, &url, &headers, body.as_deref())
                .await?;
            let redirect = matches!(raw.status, 301 | 302 | 303 | 307 | 308);
            let location = raw.header("location").map(str::to_string);
            let Some(location) = location.filter(|_| redirect) else {
                return Ok(Response {
                    status: raw.status,
                    headers: raw.headers,
                    body: raw.body,
                    url,
                    redirects: hops,
                });
            };
            if hops >= self.config.max_redirects {
                return Err(Error::TooManyRedirects);
            }
            hops += 1;
            let next = resolve(&url, &location).map_err(Error::BadRedirect)?;
            if (raw.status == 303 && method != "HEAD")
                || (matches!(raw.status, 301 | 302) && method == "POST")
            {
                method = "GET".to_string();
                body = None;
                without(
                    &mut headers,
                    &[
                        "content-type",
                        "content-length",
                        "content-encoding",
                        "transfer-encoding",
                    ],
                );
            }
            if (next.host.as_str(), next.port) != (url.host.as_str(), url.port) {
                without(
                    &mut headers,
                    &["authorization", "cookie", "proxy-authorization"],
                );
            }
            url = next;
        }
    }

    async fn send_once(
        &self,
        method: &str,
        url: &Url,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<RawResponse, Error> {
        let wire = build_request(method, url, headers, body).map_err(Error::Request)?;
        let key = (url.secure, url.host.clone(), url.port);
        let pooled = self.pool().take(&key);
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
                    self.pool().put(key, stream);
                }
                Ok(raw)
            }
            Err(ExchangeError::Stale(e) | ExchangeError::Failed(e)) => Err(io_error(e)),
        }
    }
}

fn decode<T: DeserializeOwned>(response: &Response) -> Result<T, Error> {
    if !(200..300).contains(&response.status) {
        return Err(Error::Status {
            code: response.status,
            body: response.text(),
        });
    }
    serde_json::from_slice(&response.body).map_err(|e| Error::Decode(e.to_string()))
}
