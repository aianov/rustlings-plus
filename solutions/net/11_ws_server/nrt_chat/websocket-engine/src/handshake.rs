use std::future::Future;
use std::time::Duration;

use http_engine::{Method, Request, Response, Version};
use tokio::net::TcpStream;

use crate::base64;
use crate::frame::Role;
use crate::stream::WsStream;

#[derive(Debug, Clone)]
pub struct WsConfig {
    pub allowed_origins: Vec<String>,
    pub allow_missing_origin: bool,
    pub max_message: usize,
    pub close_timeout: Duration,
}

impl Default for WsConfig {
    fn default() -> WsConfig {
        WsConfig { allowed_origins: Vec::new(), allow_missing_origin: true, max_message: 1 << 20, close_timeout: Duration::from_secs(5) }
    }
}

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn accept_key(client_key: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(client_key.as_bytes());
    hasher.update(GUID.as_bytes());
    base64::encode(&hasher.finalize())
}

fn has_token(request: &Request, name: &str, token: &str) -> bool {
    request.headers.get_all(name).iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token))
}

fn reject(status: u16, message: &str) -> Response {
    Response::text(status, message)
}

pub fn check_upgrade(request: &Request) -> Result<String, Response> {
    if request.method != Method::Get {
        let mut response = reject(405, "only GET");
        response.headers.append("Allow", "GET");
        return Err(response);
    }
    if request.version != Version::Http11 {
        return Err(reject(400, "HTTP/1.1 required"));
    }
    if !has_token(request, "upgrade", "websocket") {
        return Err(reject(400, "Upgrade: websocket expected"));
    }
    if !has_token(request, "connection", "upgrade") {
        return Err(reject(400, "Connection: Upgrade expected"));
    }
    let keys = request.headers.get_all("sec-websocket-key");
    let [key] = keys.as_slice() else {
        return Err(reject(400, "exactly one Sec-WebSocket-Key expected"));
    };
    let key = key.trim();
    match base64::decode(key) {
        Ok(raw) if raw.len() == 16 => {}
        _ => return Err(reject(400, "Sec-WebSocket-Key must be 16 bytes in base64")),
    }
    let versions = request.headers.get_all("sec-websocket-version");
    if versions.len() != 1 || versions[0].trim() != "13" {
        let mut response = reject(426, "unsupported websocket version");
        response.headers.append("Sec-WebSocket-Version", "13");
        return Err(response);
    }
    Ok(key.to_string())
}

fn check_origin(config: &WsConfig, request: &Request) -> Result<(), Response> {
    let origins = request.headers.get_all("origin");
    match origins.as_slice() {
        [] if config.allow_missing_origin => Ok(()),
        [] => Err(reject(403, "Origin required")),
        [origin] if !origin.eq_ignore_ascii_case("null") && config.allowed_origins.iter().any(|a| a.eq_ignore_ascii_case(origin)) => Ok(()),
        _ => Err(reject(403, "origin not allowed")),
    }
}

pub fn upgrade<F, Fut>(request: &Request, config: &WsConfig, on_connect: F) -> Response
where
    F: FnOnce(WsStream<TcpStream>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let key = match check_upgrade(request) {
        Ok(key) => key,
        Err(response) => return response,
    };
    if let Err(response) = check_origin(config, request) {
        return response;
    }
    let mut response = Response::new(101);
    response.headers.append("Upgrade", "websocket");
    response.headers.append("Connection", "Upgrade");
    response.headers.append("Sec-WebSocket-Accept", &accept_key(&key));
    let config = config.clone();
    response.on_upgrade(move |upgraded| async move {
        let ws = WsStream::new(upgraded.stream, upgraded.rest, Role::Server, &config);
        on_connect(ws).await;
    })
}
