use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::url::{Headers, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum RequestError {
    BadMethod,
    BadHeader,
    BadTarget,
}

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

pub fn build_request(method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Vec<u8>, RequestError> {
    if !is_token(method) {
        return Err(RequestError::BadMethod);
    }
    if !url.path.starts_with('/') || url.path.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(RequestError::BadTarget);
    }
    let mut out = format!("{method} {} HTTP/1.1\r\nHost: {}\r\n", url.path, url.authority());
    for (name, value) in headers {
        if !is_token(name) || value.contains(['\r', '\n', '\0']) {
            return Err(RequestError::BadHeader);
        }
        let lower = name.to_ascii_lowercase();
        if matches!(lower.as_str(), "host" | "content-length" | "transfer-encoding") {
            continue;
        }
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    match body {
        Some(b) => out.push_str(&format!("Content-Length: {}\r\n", b.len())),
        None if matches!(method, "POST" | "PUT" | "PATCH") => out.push_str("Content-Length: 0\r\n"),
        None => {}
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    if let Some(b) = body {
        bytes.extend_from_slice(b);
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub minor: u8,
    pub status: u16,
    pub reason: String,
    pub headers: Headers,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

pub const MAX_HEAD: usize = 16 * 1024;

#[allow(dead_code)]
impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    pub fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.all("connection") {
            for token in value.split(',') {
                close |= token.trim().eq_ignore_ascii_case("close");
                keep |= token.trim().eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.minor == 1)
    }
}

pub fn parse_head(buf: &[u8]) -> Result<Option<Head>, HeadError> {
    let _ = buf;
    todo!()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub bytes: Vec<u8>,
    pub reusable: bool,
}

pub async fn fill<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<usize> {
    let mut chunk = [0u8; 4096];
    let n = reader.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n)
}

fn invalid<T>(why: &str) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, why.to_string()))
}

async fn take_line<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<String> {
    loop {
        if let Some(pos) = buf.windows(2).position(|w| w == b"\r\n") {
            let line: Vec<u8> = buf.drain(..pos + 2).take(pos).collect();
            return String::from_utf8(line).or_else(|_| invalid("строка не в utf-8"));
        }
        if buf.len() > 4096 {
            return invalid("слишком длинная строка");
        }
        if fill(reader, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    }
}

pub async fn read_body<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>, head: &Head, max: usize) -> io::Result<Body> {
    let _ = (reader, buf, head, max);
    todo!()
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub reusable: bool,
}

impl RawResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub async fn read_response<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>, method: &str, max: usize) -> io::Result<RawResponse> {
    let _ = (stream, buf, method, max);
    todo!()
}
