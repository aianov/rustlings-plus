mod body;
mod head;

use std::fmt;
use std::io;

use tokio::io::AsyncRead;

use crate::url::{Headers, Url};
use body::read_body;
use head::parse_head;

pub use body::fill;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    Method,
    Header,
    Target,
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequestError::Method => f.write_str("кривой метод"),
            RequestError::Header => f.write_str("кривой заголовок запроса"),
            RequestError::Target => f.write_str("кривой путь запроса"),
        }
    }
}

fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

pub fn build_request(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<Vec<u8>, RequestError> {
    if !is_token(method) {
        return Err(RequestError::Method);
    }
    if !url.path.starts_with('/')
        || url
            .path
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(RequestError::Target);
    }
    let mut out = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\n",
        url.path,
        url.authority()
    );
    for (name, value) in headers {
        if !is_token(name) || value.contains(['\r', '\n', '\0']) {
            return Err(RequestError::Header);
        }
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "content-length" | "transfer-encoding"
        ) {
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
pub struct RawResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub reusable: bool,
}

impl RawResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub async fn read_response<S: AsyncRead + Unpin>(
    stream: &mut S,
    buf: &mut Vec<u8>,
    method: &str,
    max: usize,
) -> io::Result<RawResponse> {
    let head = loop {
        if let Some(head) = parse_head(buf).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("плохая голова ответа: {e:?}"),
            )
        })? {
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
    let bodiless =
        method == "HEAD" || head.status == 204 || head.status == 304 || head.status == 101;
    let (bytes, reusable) = if bodiless {
        (Vec::new(), true)
    } else {
        let body = read_body(stream, buf, &head, max).await?;
        (body.bytes, body.reusable)
    };
    Ok(RawResponse {
        status: head.status,
        reusable: reusable && head.keep_alive() && buf.is_empty(),
        headers: head.headers,
        body: bytes,
    })
}
