use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub body: String,
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
        let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", self.addr);
        if let Some(token) = token {
            raw.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if let Some(body) = body {
            raw.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        raw.push_str("\r\n");
        raw.push_str(body.unwrap_or_default());
        let reused = self.conn.is_some();
        match self.exchange(raw.as_bytes()).await {
            Ok(reply) => Ok(reply),
            Err(..) if reused => {
                self.conn = None;
                self.exchange(raw.as_bytes()).await
            }
            Err(error) => Err(error),
        }
    }

    async fn exchange(&mut self, raw: &[u8]) -> io::Result<Reply> {
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
        let (reply, keep) = read_reply(&mut conn).await?;
        if keep {
            self.conn = Some(conn);
        }
        Ok(reply)
    }
}

async fn fill(conn: &mut Conn) -> io::Result<()> {
    let mut chunk = [0u8; 4096];
    let n = conn.stream.read(&mut chunk).await?;
    if n == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    conn.buf.extend_from_slice(&chunk[..n]);
    Ok(())
}

async fn read_reply(conn: &mut Conn) -> io::Result<(Reply, bool)> {
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
        let mut length = 0;
        let mut keep = true;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let (name, value) = (name.trim(), value.trim());
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().map_err(|_| broken("Content-Length"))?;
            }
            if name.eq_ignore_ascii_case("connection") && value.eq_ignore_ascii_case("close") {
                keep = false;
            }
        }
        while conn.buf.len() < length {
            fill(conn).await?;
        }
        let body: Vec<u8> = conn.buf.drain(..length).collect();
        let reply = Reply {
            status,
            body: String::from_utf8_lossy(&body).into_owned(),
        };
        return Ok((reply, keep));
    }
}
