use std::str::FromStr;

use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Patch,
    Options,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Http10,
    Http11,
}

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
            _ => 400,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_head_bytes: usize,
    pub max_headers: usize,
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_head_bytes: 8192, max_headers: 64, max_body: 1 << 20 }
    }
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
            Method::Options => "OPTIONS",
        }
    }
}

impl FromStr for Method {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Method, ParseError> {
        Ok(match s {
            "GET" => Method::Get,
            "HEAD" => Method::Head,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "DELETE" => Method::Delete,
            "PATCH" => Method::Patch,
            "OPTIONS" => Method::Options,
            _ => return Err(ParseError::BadMethod),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Headers {
    items: Vec<(String, String)>,
}

impl Headers {
    pub fn new() -> Headers {
        Headers::default()
    }

    pub fn append(&mut self, name: &str, value: &str) {
        self.items.push((name.to_string(), value.to_string()));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.items.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.items.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.remove(name);
        self.append(name, value);
    }

    pub fn remove(&mut self, name: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        before - self.items.len()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().map(|(n, v)| (n.as_str(), v.as_str()))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    items: Vec<(String, String)>,
}

impl Params {
    pub fn new() -> Params {
        Params::default()
    }

    pub fn push(&mut self, name: &str, value: String) {
        self.items.push((name.to_string(), value));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.items.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub target: String,
    pub version: Version,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub params: Params,
}

impl Request {
    pub fn raw_path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    pub fn path(&self) -> Result<String, ParseError> {
        percent_decode(self.raw_path(), false)
    }

    pub fn query(&self) -> Result<Vec<(String, String)>, ParseError> {
        let Some((_, query)) = self.target.split_once('?') else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            out.push((percent_decode(key, true)?, percent_decode(value, true)?));
        }
        Ok(out)
    }

    pub fn keep_alive(&self) -> bool {
        let mut close = false;
        let mut keep = false;
        for value in self.headers.get_all("connection") {
            for token in value.split(',') {
                let token = token.trim();
                close |= token.eq_ignore_ascii_case("close");
                keep |= token.eq_ignore_ascii_case("keep-alive");
            }
        }
        !close && (keep || self.version == Version::Http11)
    }
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    Partial,
    Complete { request: Request, consumed: usize },
}

pub fn decode_component(s: &str) -> Result<String, ParseError> {
    percent_decode(s, false)
}

fn percent_decode(s: &str, plus_as_space: bool) -> Result<String, ParseError> {
    let bytes = s.as_bytes();
    let hex = |b: Option<&u8>| b.and_then(|b| (*b as char).to_digit(16)).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => match (hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) {
                (Some(hi), Some(lo)) => {
                    out.push(hi << 4 | lo);
                    i += 3;
                }
                _ => return Err(ParseError::BadEscape),
            },
            b'+' if plus_as_space => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ParseError::BadEscape)
}

fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

fn parse_header_line(line: &str) -> Result<(String, String), ParseError> {
    let (name, value) = line.split_once(':').ok_or(ParseError::BadHeader)?;
    if name.is_empty() || !name.chars().all(is_token) {
        return Err(ParseError::BadHeader);
    }
    let value = value.trim_matches(|c| c == ' ' || c == '\t');
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ParseError::BadHeader);
    }
    Ok((name.to_string(), value.to_string()))
}

fn parse_request_line(line: &str) -> Result<(Method, String, Version), ParseError> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(ParseError::BadRequestLine);
    }
    let method = parts[0].parse()?;
    let version = match parts[2] {
        "HTTP/1.0" => Version::Http10,
        "HTTP/1.1" => Version::Http11,
        _ => return Err(ParseError::BadVersion),
    };
    let target = parts[1];
    if !target.starts_with('/') || target.chars().any(|c| c == '#' || c.is_control()) {
        return Err(ParseError::BadTarget);
    }
    Ok((method, target.to_string(), version))
}

fn content_length(headers: &Headers) -> Result<Option<usize>, ParseError> {
    let mut found: Option<usize> = None;
    for value in headers.get_all("content-length") {
        for part in value.split(',') {
            let part = part.trim_matches(|c| c == ' ' || c == '\t');
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::BadContentLength);
            }
            let n: usize = part.parse().map_err(|_| ParseError::BadContentLength)?;
            match found {
                Some(prev) if prev != n => return Err(ParseError::Smuggling),
                _ => found = Some(n),
            }
        }
    }
    Ok(found)
}

fn crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn decode_chunked(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ParseError> {
    let mut pos = 0;
    let mut body = Vec::new();
    loop {
        let rest = &buf[pos..];
        let Some(end) = crlf(rest) else {
            return if rest.len() > 64 { Err(ParseError::BadChunk) } else { Ok(None) };
        };
        let line = std::str::from_utf8(&rest[..end]).map_err(|_| ParseError::BadChunk)?;
        let digits = line.split(';').next().unwrap_or("");
        if digits.is_empty() || digits.len() > 16 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ParseError::BadChunk);
        }
        let size = usize::from_str_radix(digits, 16).map_err(|_| ParseError::BadChunk)?;
        pos += end + 2;
        if size == 0 {
            loop {
                let Some(end) = crlf(&buf[pos..]) else { return Ok(None) };
                pos += end + 2;
                if end == 0 {
                    return Ok(Some((body, pos)));
                }
            }
        }
        if body.len().saturating_add(size) > max_body {
            return Err(ParseError::BodyTooLarge);
        }
        let available = buf.len() - pos;
        if available < size.saturating_add(2) {
            if available > size && buf[pos + size] != b'\r' {
                return Err(ParseError::BadChunk);
            }
            return Ok(None);
        }
        if &buf[pos + size..pos + size + 2] != b"\r\n" {
            return Err(ParseError::BadChunk);
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        pos += size + 2;
    }
}

#[derive(Debug)]
pub struct Head {
    pub request: Request,
    pub head_len: usize,
    pub length: Option<usize>,
    pub chunked: bool,
}

impl Head {
    pub fn expects_continue(&self) -> bool {
        self.request.version == Version::Http11
            && self.request.headers.get_all("expect").iter().any(|v| v.trim().eq_ignore_ascii_case("100-continue"))
    }

    pub fn has_unknown_expectation(&self) -> bool {
        self.request.headers.get_all("expect").iter().any(|v| !v.trim().eq_ignore_ascii_case("100-continue"))
    }
}

pub fn parse_head(buf: &[u8], limits: &Limits) -> Result<Option<Head>, ParseError> {
    let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() >= limits.max_head_bytes { Err(ParseError::HeadTooLarge) } else { Ok(None) };
    };
    let head_len = pos + 4;
    if head_len > limits.max_head_bytes {
        return Err(ParseError::HeadTooLarge);
    }
    if buf[..pos].windows(2).filter(|w| w == b"\r\n").count() > limits.max_headers {
        return Err(ParseError::TooManyHeaders);
    }
    let head = std::str::from_utf8(&buf[..pos]).map_err(|_| ParseError::BadHeader)?;
    let mut lines = head.split("\r\n");
    let (method, target, version) = parse_request_line(lines.next().unwrap_or(""))?;
    let mut headers = Headers::new();
    for line in lines {
        let (name, value) = parse_header_line(line)?;
        headers.append(&name, &value);
    }
    let hosts = headers.get_all("host");
    if version == Version::Http11 {
        match hosts.as_slice() {
            [] => return Err(ParseError::MissingHost),
            [""] => return Err(ParseError::MissingHost),
            [_] => {}
            _ => return Err(ParseError::DuplicateHost),
        }
    }
    let length = content_length(&headers)?;
    let chunked = match headers.get_all("transfer-encoding").as_slice() {
        [] => false,
        [one] if one.trim().eq_ignore_ascii_case("chunked") => true,
        _ => return Err(ParseError::BadHeader),
    };
    if chunked && length.is_some() {
        return Err(ParseError::Smuggling);
    }
    if length.is_some_and(|n| n > limits.max_body) {
        return Err(ParseError::BodyTooLarge);
    }
    let request = Request { method, target, version, headers, body: Vec::new(), params: Params::new() };
    Ok(Some(Head { request, head_len, length, chunked }))
}

pub fn parse_request(buf: &[u8], limits: &Limits) -> Result<Parsed, ParseError> {
    let Some(head) = parse_head(buf, limits)? else {
        return Ok(Parsed::Partial);
    };
    let rest = &buf[head.head_len..];
    let (body, body_len) = if head.chunked {
        match decode_chunked(rest, limits.max_body)? {
            Some(done) => done,
            None => return Ok(Parsed::Partial),
        }
    } else {
        let n = head.length.unwrap_or(0);
        if rest.len() < n {
            return Ok(Parsed::Partial);
        }
        (rest[..n].to_vec(), n)
    };
    let mut request = head.request;
    request.body = body;
    Ok(Parsed::Complete { request, consumed: head.head_len + body_len })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(u16);

impl StatusCode {
    pub fn new(code: u16) -> Option<StatusCode> {
        (100..=599).contains(&code).then_some(StatusCode(code))
    }

    pub fn code(self) -> u16 {
        self.0
    }

    pub fn reason(self) -> &'static str {
        match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            206 => "Partial Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            413 => "Payload Too Large",
            414 => "URI Too Long",
            415 => "Unsupported Media Type",
            417 => "Expectation Failed",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            431 => "Request Header Fields Too Large",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            505 => "HTTP Version Not Supported",
            _ => "",
        }
    }

    pub fn allows_body(self) -> bool {
        self.0 >= 200 && self.0 != 204 && self.0 != 304
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: StatusCode,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub stream: Option<mpsc::Receiver<Vec<u8>>>,
}

impl Response {
    pub fn new(status: u16) -> Response {
        let status = StatusCode::new(status).unwrap_or_else(|| StatusCode::new(500).expect("500 допустим"));
        Response { status, headers: Headers::new(), body: Vec::new(), stream: None }
    }

    pub fn text(status: u16, body: &str) -> Response {
        let mut response = Response::new(status);
        response.headers.append("Content-Type", "text/plain; charset=utf-8");
        response.body = body.as_bytes().to_vec();
        response
    }

    pub fn streaming(status: u16, content_type: &str, chunks: mpsc::Receiver<Vec<u8>>) -> Response {
        let mut response = Response::new(status);
        response.headers.append("Content-Type", content_type);
        response.stream = Some(chunks);
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
        let mut out = format!("HTTP/1.1 {} {}\r\n", self.status.code(), self.status.reason()).into_bytes();
        for (name, value) in self.headers.iter() {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        let allows = self.status.allows_body();
        if allows && self.headers.get("content-length").is_none() && self.headers.get("transfer-encoding").is_none() {
            out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        if allows && !head_only {
            out.extend_from_slice(&self.body);
        }
        out
    }
}
