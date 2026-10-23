use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::base64;
use crate::frame::{MaskGen, Role};
use crate::handshake::{WsConfig, accept_key};
use crate::stream::{WsError, WsStream};

pub fn new_key(keys: &mut MaskGen) -> String {
    let mut raw = Vec::with_capacity(16);
    for _ in 0..4 {
        raw.extend_from_slice(&keys.next_key());
    }
    base64::encode(&raw)
}

pub fn client_request(host: &str, path: &str, key: &str, origin: Option<&str>) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let mut out = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
    );
    if let Some(origin) = origin {
        out.push_str(&format!("Origin: {origin}\r\n"));
    }
    out.push_str("\r\n");
    out
}

pub fn verify_response(head: &str, key: &str) -> Result<(), String> {
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or("");
    let mut parts = status.splitn(3, ' ');
    if parts.next() != Some("HTTP/1.1") || parts.next() != Some("101") {
        return Err(format!("ожидали 101 Switching Protocols, пришло {status:?}"));
    }
    let headers: Vec<(String, String)> = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let all = |name: &str| -> Vec<&str> { headers.iter().filter(|(n, _)| n == name).map(|(_, v)| v.as_str()).collect() };
    let has_token = |name: &str, token: &str| all(name).iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case(token));
    if !has_token("upgrade", "websocket") {
        return Err("нет Upgrade: websocket".to_string());
    }
    if !has_token("connection", "upgrade") {
        return Err("нет Connection: Upgrade".to_string());
    }
    let accepts = all("sec-websocket-accept");
    if accepts.len() != 1 || accepts[0] != accept_key(key) {
        return Err("Sec-WebSocket-Accept не совпал с ожидаемым".to_string());
    }
    if !all("sec-websocket-extensions").is_empty() {
        return Err("сервер включил расширение, о котором мы не просили".to_string());
    }
    if !all("sec-websocket-protocol").is_empty() {
        return Err("сервер выбрал подпротокол, о котором мы не просили".to_string());
    }
    Ok(())
}

async fn read_head(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Result<usize, WsError> {
    let mut chunk = [0u8; 2048];
    loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            return Ok(pos + 4);
        }
        if buf.len() > 16 * 1024 {
            return Err(WsError::Handshake("слишком длинный ответ сервера".to_string()));
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(WsError::Handshake("сервер закрыл соединение до ответа".to_string()));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

pub async fn connect(addr: SocketAddr, host: &str, path: &str, origin: Option<&str>, config: &WsConfig) -> Result<WsStream<TcpStream>, WsError> {
    let mut stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;
    let key = new_key(&mut MaskGen::new());
    stream.write_all(client_request(host, path, &key, origin).as_bytes()).await?;
    let mut buf = Vec::new();
    let end = timeout(Duration::from_secs(5), read_head(&mut stream, &mut buf))
        .await
        .map_err(|_| WsError::Handshake("сервер молчит".to_string()))??;
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    verify_response(&head, &key).map_err(WsError::Handshake)?;
    let rest = buf.split_off(end);
    Ok(WsStream::new(stream, rest, Role::Client, config))
}
