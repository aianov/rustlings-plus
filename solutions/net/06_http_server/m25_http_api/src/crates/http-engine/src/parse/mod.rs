mod chunked;
mod text;

use std::error::Error;
use std::fmt;

use crate::headers::Headers;
use crate::request::{Params, Request, Version};
use chunked::decode_chunked;
use text::{parse_header_line, parse_request_line};

pub(crate) use text::{is_token, percent_decode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    BadRequestLine,
    BadMethod,
    BadVersion,
    BadTarget,
    BadEscape,
    BadHeader,
    BadContentLength,
    Smuggling,
    MissingHost,
    DuplicateHost,
    HeadTooLarge,
    TooManyHeaders,
    BodyTooLarge,
    BadChunk,
}

impl ParseError {
    pub fn status(self) -> u16 {
        match self {
            ParseError::HeadTooLarge | ParseError::TooManyHeaders => 431,
            ParseError::BodyTooLarge => 413,
            ParseError::BadMethod => 501,
            ParseError::BadVersion => 505,
            ParseError::BadRequestLine
            | ParseError::BadTarget
            | ParseError::BadEscape
            | ParseError::BadHeader
            | ParseError::BadContentLength
            | ParseError::Smuggling
            | ParseError::MissingHost
            | ParseError::DuplicateHost
            | ParseError::BadChunk => 400,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ParseError::BadRequestLine => "кривая строка запроса",
            ParseError::BadMethod => "неизвестный метод",
            ParseError::BadVersion => "версия HTTP не поддерживается",
            ParseError::BadTarget => "цель запроса должна начинаться с /",
            ParseError::BadEscape => "кривой percent-encoding",
            ParseError::BadHeader => "кривой заголовок",
            ParseError::BadContentLength => "кривой Content-Length",
            ParseError::Smuggling => "противоречивая длина тела",
            ParseError::MissingHost => "в HTTP/1.1 нужен Host",
            ParseError::DuplicateHost => "два заголовка Host",
            ParseError::HeadTooLarge => "заголовки слишком длинные",
            ParseError::TooManyHeaders => "слишком много заголовков",
            ParseError::BodyTooLarge => "тело слишком большое",
            ParseError::BadChunk => "кривой chunked",
        };
        f.write_str(text)
    }
}

impl Error for ParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_head_bytes: usize,
    pub max_headers: usize,
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_head_bytes: 8192,
            max_headers: 64,
            max_body: 1 << 20,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Partial,
    Complete { request: Request, consumed: usize },
}

pub fn decode_component(raw: &str) -> Result<String, ParseError> {
    percent_decode(raw, false)
}

struct Head {
    request: Request,
    len: usize,
    body: Option<usize>,
    chunked: bool,
}

fn content_length(headers: &Headers) -> Result<Option<usize>, ParseError> {
    let mut found: Option<usize> = None;
    for value in headers.get_all("content-length") {
        for part in value.split(',') {
            let part = part.trim_matches(|c| c == ' ' || c == '\t');
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(ParseError::BadContentLength);
            }
            let n: usize = part.parse().map_err(|_| ParseError::BadContentLength)?;
            match found {
                Some(seen) if seen != n => return Err(ParseError::Smuggling),
                _ => found = Some(n),
            }
        }
    }
    Ok(found)
}

fn parse_head(buf: &[u8], limits: &Limits) -> Result<Option<Head>, ParseError> {
    let Some(end) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
        if buf.len() >= limits.max_head_bytes {
            return Err(ParseError::HeadTooLarge);
        }
        return Ok(None);
    };
    let len = end + 4;
    if len > limits.max_head_bytes {
        return Err(ParseError::HeadTooLarge);
    }
    let lines = buf[..end]
        .windows(2)
        .filter(|window| window == b"\r\n")
        .count();
    if lines > limits.max_headers {
        return Err(ParseError::TooManyHeaders);
    }
    let head = std::str::from_utf8(&buf[..end]).map_err(|_| ParseError::BadHeader)?;
    let mut lines = head.split("\r\n");
    let (method, target, version) = parse_request_line(lines.next().unwrap_or_default())?;
    let mut headers = Headers::new();
    for line in lines {
        let (name, value) = parse_header_line(line)?;
        headers.append(&name, &value);
    }
    if version == Version::Http11 {
        match headers.get_all("host").as_slice() {
            [] | [""] => return Err(ParseError::MissingHost),
            [_] => {}
            _ => return Err(ParseError::DuplicateHost),
        }
    }
    let body = content_length(&headers)?;
    let chunked = match headers.get_all("transfer-encoding").as_slice() {
        [] => false,
        [one] if one.trim().eq_ignore_ascii_case("chunked") => true,
        _ => return Err(ParseError::BadHeader),
    };
    if chunked && body.is_some() {
        return Err(ParseError::Smuggling);
    }
    if body.is_some_and(|n| n > limits.max_body) {
        return Err(ParseError::BodyTooLarge);
    }
    let request = Request {
        method,
        target,
        version,
        headers,
        body: Vec::new(),
        params: Params::new(),
    };
    Ok(Some(Head {
        request,
        len,
        body,
        chunked,
    }))
}

pub fn parse_request(buf: &[u8], limits: &Limits) -> Result<Parsed, ParseError> {
    let Some(head) = parse_head(buf, limits)? else {
        return Ok(Parsed::Partial);
    };
    let rest = &buf[head.len..];
    let (body, used) = if head.chunked {
        match decode_chunked(rest, limits.max_body)? {
            Some(done) => done,
            None => return Ok(Parsed::Partial),
        }
    } else {
        let n = head.body.unwrap_or(0);
        if rest.len() < n {
            return Ok(Parsed::Partial);
        }
        (rest[..n].to_vec(), n)
    };
    let mut request = head.request;
    request.body = body;
    Ok(Parsed::Complete {
        request,
        consumed: head.len + used,
    })
}
