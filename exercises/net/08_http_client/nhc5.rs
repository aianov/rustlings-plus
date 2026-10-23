// Клиент работает: GET с Content-Length, GET с chunked и даже ответ до закрытия читаются без проблем. Но на
// некоторых ответах он замирает навсегда, а на других отдаёт не тот ответ, который ждёшь. Сервер при этом ведёт
// себя по правилам, и соединение у него открыто.
// TODO: Найди, на каких ответах клиент ошибается, и почини read_response. Тесты ниже покажут, где он зависает.

#![allow(dead_code)]
use std::io;
#[allow(unused_imports)]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

type Headers = Vec<(String, String)>;

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

#[derive(Debug, Clone, PartialEq)]
struct Head {
    minor: u8,
    status: u16,
    reason: String,
    headers: Headers,
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadError {
    StatusLine,
    Version,
    Status,
    Header,
    TooLarge,
}

const MAX_HEAD: usize = 16 * 1024;

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn keep_alive(&self) -> bool {
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

fn parse_head(buf: &[u8]) -> Result<Option<Head>, HeadError> {
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
struct Body {
    bytes: Vec<u8>,
    reusable: bool,
}

async fn fill<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>) -> io::Result<usize> {
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

async fn read_body<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut Vec<u8>, head: &Head, max: usize) -> io::Result<Body> {
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
struct Response {
    status: u16,
    headers: Headers,
    body: Vec<u8>,
    reusable: bool,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn head_error(e: HeadError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("плохая голова ответа: {e:?}"))
}


async fn read_response<S: AsyncRead + Unpin>(stream: &mut S, buf: &mut Vec<u8>, method: &str, max: usize) -> io::Result<Response> {
    let _ = method;
    let head = loop {
        if let Some(head) = parse_head(buf).map_err(head_error)? {
            buf.drain(..head.len);
            break head;
        }
        if fill(stream, buf).await? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
    };
    let body = read_body(stream, buf, &head, max).await?;
    Ok(Response { status: head.status, reusable: body.reusable && head.keep_alive() && buf.is_empty(), headers: head.headers, body: body.bytes })
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    async fn run(method: &str, wire: &[u8], close_after: bool) -> io::Result<Response> {
        let (mut client, mut server) = tokio::io::duplex(65536);
        server.write_all(wire).await.unwrap();
        let _keep = if close_after {
            drop(server);
            None
        } else {
            Some(server)
        };
        let mut buf = Vec::new();
        timeout(Duration::from_secs(1), read_response(&mut client, &mut buf, method, 1 << 20))
            .await
            .expect("клиент завис: он ждёт тело, которого по правилам протокола быть не может (или заглянул не в тот ответ)")
    }

    #[tokio::test]
    async fn head_has_no_body_even_with_a_length() {
        let r = run("HEAD", b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n", false).await.unwrap();
        assert_eq!(r.status, 200);
        assert!(r.body.is_empty());
        assert!(r.reusable, "ответ на HEAD без тела прочитан целиком, соединение свободно");
        let r = run("HEAD", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", false).await.unwrap();
        assert!(r.body.is_empty() && r.reusable, "chunked в ответе на HEAD тоже не означает, что куски придут");
        let r = run("HEAD", b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n", false).await.unwrap();
        assert!(r.body.is_empty(), "на HEAD нет тела и тогда, когда нет длины: не читаем до закрытия");
    }

    #[tokio::test]
    async fn no_content_and_not_modified_have_no_body() {
        let r = run("GET", b"HTTP/1.1 204 No Content\r\n\r\n", false).await.unwrap();
        assert_eq!((r.status, r.body.len()), (204, 0), "после 204 тела нет, а соединение открыто: ждать нечего");
        assert!(r.reusable);
        let r = run("GET", b"HTTP/1.1 204 No Content\r\nContent-Length: 5\r\n\r\n", false).await.unwrap();
        assert_eq!(r.body.len(), 0, "даже если сервер написал длину, у 204 тела нет");
        let r = run("GET", b"HTTP/1.1 304 Not Modified\r\nETag: \"x\"\r\nContent-Length: 100\r\n\r\n", false).await.unwrap();
        assert_eq!((r.status, r.body.len()), (304, 0), "304 повторяет заголовки 200, включая длину, но тела не несёт");
        assert!(r.reusable);
    }

    #[tokio::test]
    async fn interim_responses_are_skipped() {
        let r = run("POST", b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\nok", false).await.unwrap();
        assert_eq!(r.status, 201, "100 Continue это не ответ, настоящий идёт следом");
        assert_eq!(r.text(), "ok");
        let r = run("GET", b"HTTP/1.1 103 Early Hints\r\nLink: </a.css>\r\n\r\nHTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n", false).await.unwrap();
        assert_eq!(r.status, 204, "промежуточных ответов может быть несколько");
    }

    #[tokio::test]
    async fn real_bodies_are_still_read() {
        let r = run("GET", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", false).await.unwrap();
        assert_eq!(r.text(), "hello");
        assert!(r.reusable);
        let r = run("GET", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n", false).await.unwrap();
        assert_eq!(r.text(), "abc");
        let r = run("GET", b"HTTP/1.1 200 OK\r\n\r\nuntil the end", true).await.unwrap();
        assert_eq!(r.text(), "until the end");
        assert!(!r.reusable, "тело до закрытия: соединение уже закрыто");
        let r = run("DELETE", b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\ngone", false).await.unwrap();
        assert_eq!((r.status, r.text()), (404, "gone".to_string()));
    }

    #[tokio::test]
    async fn connection_close_and_old_versions_are_not_reusable() {
        let r = run("GET", b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 1\r\n\r\nx", false).await.unwrap();
        assert!(!r.reusable);
        let r = run("GET", b"HTTP/1.0 200 OK\r\nContent-Length: 1\r\n\r\nx", false).await.unwrap();
        assert!(!r.reusable, "HTTP/1.0 без keep-alive закрывается после ответа");
        let r = run("HEAD", b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 9\r\n\r\n", false).await.unwrap();
        assert!(!r.reusable, "ответ на HEAD тоже может сказать close");
    }
}
