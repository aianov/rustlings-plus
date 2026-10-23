use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub bytes: Vec<u8>,
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(own, _)| own.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Body<'a> {
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
}

pub struct Web {
    addr: String,
    conn: Option<Conn>,
    opened: usize,
}

fn broken(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("кривой ответ HTTP: {what}"),
    )
}

impl Web {
    pub fn new(addr: &str) -> Web {
        Web {
            addr: addr.to_string(),
            conn: None,
            opened: 0,
        }
    }

    pub fn opened(&self) -> usize {
        self.opened
    }

    pub async fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&str>,
        token: Option<&str>,
    ) -> io::Result<Reply> {
        let body = body.map(|text| Body {
            content_type: "application/json",
            bytes: text.as_bytes(),
        });
        let bearer = token.map(|token| format!("Bearer {token}"));
        let headers: Vec<(&str, &str)> = bearer
            .as_deref()
            .map(|value| ("Authorization", value))
            .into_iter()
            .collect();
        self.send(method, path, body, &headers).await
    }

    pub async fn send(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Body<'_>>,
        headers: &[(&str, &str)],
    ) -> io::Result<Reply> {
        let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", self.addr);
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        if let Some(body) = body {
            head.push_str(&format!(
                "Content-Type: {}\r\nContent-Length: {}\r\n",
                body.content_type,
                body.bytes.len()
            ));
        }
        head.push_str("\r\n");
        let mut raw = head.into_bytes();
        if let Some(body) = body {
            raw.extend_from_slice(body.bytes);
        }
        let bodiless = method == "HEAD";
        let reused = self.conn.is_some();
        match self.exchange(&raw, bodiless).await {
            Ok(reply) => Ok(reply),
            Err(..) if reused => {
                self.conn = None;
                self.exchange(&raw, bodiless).await
            }
            Err(error) => Err(error),
        }
    }

    async fn exchange(&mut self, raw: &[u8], bodiless: bool) -> io::Result<Reply> {
        let mut conn = match self.conn.take() {
            Some(conn) => conn,
            None => {
                let stream = TcpStream::connect(&self.addr).await?;
                self.opened += 1;
                Conn {
                    stream,
                    buf: Vec::new(),
                }
            }
        };
        conn.stream.write_all(raw).await?;
        let (reply, keep) = read_reply(&mut conn, bodiless).await?;
        if keep {
            self.conn = Some(conn);
        }
        Ok(reply)
    }
}

async fn fill(conn: &mut Conn) -> io::Result<()> {
    let mut chunk = [0u8; 8192];
    let n = conn.stream.read(&mut chunk).await?;
    if n == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    conn.buf.extend_from_slice(&chunk[..n]);
    Ok(())
}

async fn read_reply(conn: &mut Conn, bodiless: bool) -> io::Result<(Reply, bool)> {
    loop {
        let end = loop {
            match conn.buf.windows(4).position(|window| window == b"\r\n\r\n") {
                Some(end) => break end,
                None => fill(conn).await?,
            }
        };
        let head = String::from_utf8(conn.buf.drain(..end + 4).collect())
            .map_err(|_| broken("заголовки не UTF-8"))?;
        let mut lines = head.split("\r\n");
        let status: u16 = lines
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| broken("строка статуса"))?;
        if (100..200).contains(&status) {
            continue;
        }
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
            .collect();
        let find = |name: &str| {
            headers
                .iter()
                .find(|(own, _)| own.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        };
        let keep = !find("connection").is_some_and(|value| value.eq_ignore_ascii_case("close"));
        let length = match find("content-length") {
            Some(value) => value.parse().map_err(|_| broken("Content-Length"))?,
            None => 0,
        };
        let length = if bodiless || status == 204 || status == 304 {
            0
        } else {
            length
        };
        while conn.buf.len() < length {
            fill(conn).await?;
        }
        let bytes: Vec<u8> = conn.buf.drain(..length).collect();
        let body = String::from_utf8_lossy(&bytes).into_owned();
        let reply = Reply {
            status,
            headers,
            bytes,
            body,
        };
        return Ok((reply, keep));
    }
}
