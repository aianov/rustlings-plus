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
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() > MAX_HEAD { Err(HeadError::TooLarge) } else { Ok(None) };
    };
    let len = end + 4;
    if len > MAX_HEAD {
        return Err(HeadError::TooLarge);
    }
    let text = std::str::from_utf8(&buf[..end]).map_err(|_| HeadError::Header)?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let (version, rest) = first.split_once(' ').ok_or(HeadError::StatusLine)?;
    let minor = match version {
        "HTTP/1.1" => 1,
        "HTTP/1.0" => 0,
        v if v.starts_with("HTTP/") => return Err(HeadError::Version),
        _ => return Err(HeadError::StatusLine),
    };
    let (code, reason) = rest.split_once(' ').unwrap_or((rest, ""));
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HeadError::Status);
    }
    let status: u16 = code.parse().map_err(|_| HeadError::Status)?;
    if status < 100 {
        return Err(HeadError::Status);
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.starts_with([' ', '\t']) {
            return Err(HeadError::Header);
        }
        let (name, value) = line.split_once(':').ok_or(HeadError::Header)?;
        if !is_token(name) || value.contains(['\r', '\n']) {
            return Err(HeadError::Header);
        }
        headers.push((name.to_string(), value.trim_matches([' ', '\t']).to_string()));
    }
    Ok(Some(Head { minor, status, reason: reason.to_string(), headers, len }))
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
    let encodings = head.all("transfer-encoding");
    let chunked = encodings.last().is_some_and(|v| v.rsplit(',').next().is_some_and(|t| t.trim().eq_ignore_ascii_case("chunked")));
    let mut lengths = Vec::new();
    for value in head.all("content-length") {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return invalid("плохой Content-Length");
            }
            lengths.push(part.parse::<usize>().or_else(|_| invalid("Content-Length слишком велик"))?);
        }
    }
    if lengths.windows(2).any(|w| w[0] != w[1]) {
        return invalid("разные Content-Length");
    }
    if chunked {
        let mut bytes = Vec::new();
        loop {
            let line = take_line(reader, buf).await?;
            let size_text = line.split(';').next().unwrap_or("").trim();
            if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
                return invalid("плохой размер куска");
            }
            let size = usize::from_str_radix(size_text, 16).or_else(|_| invalid("размер куска слишком велик"))?;
            if size == 0 {
                while !take_line(reader, buf).await?.is_empty() {}
                return Ok(Body { bytes, reusable: lengths.is_empty() });
            }
            if size > max - bytes.len().min(max) {
                return invalid("тело больше лимита");
            }
            while buf.len() < size + 2 {
                if fill(reader, buf).await? == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            bytes.extend_from_slice(&buf[..size]);
            if &buf[size..size + 2] != b"\r\n" {
                return invalid("после куска нет CRLF");
            }
            buf.drain(..size + 2);
        }
    }
    if let Some(&n) = lengths.first().filter(|_| encodings.is_empty()) {
        if n > max {
            return invalid("тело больше лимита");
        }
        while buf.len() < n {
            if fill(reader, buf).await? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }
        return Ok(Body { bytes: buf.drain(..n).collect(), reusable: true });
    }
    loop {
        if buf.len() > max {
            return invalid("тело больше лимита");
        }
        if fill(reader, buf).await? == 0 {
            return Ok(Body { bytes: std::mem::take(buf), reusable: false });
        }
    }
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
    let head = loop {
        if let Some(head) = parse_head(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("плохая голова ответа: {e:?}")))? {
            buf.drain(..head.len);
            if (100..200).contains(&head.status) && head.status != 101 {
                continue;
            }
            break head;
        }
        if fill(stream, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    };
    let bodiless = method == "HEAD" || head.status == 204 || head.status == 304 || head.status == 101;
    let (bytes, reusable) = if bodiless {
        (Vec::new(), true)
    } else {
        let body = read_body(stream, buf, &head, max).await?;
        (body.bytes, body.reusable)
    };
    Ok(RawResponse { status: head.status, reusable: reusable && head.keep_alive() && buf.is_empty(), headers: head.headers, body: bytes })
}
