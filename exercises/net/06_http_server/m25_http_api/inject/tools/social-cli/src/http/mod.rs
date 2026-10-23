use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Reply {
    pub status: u16,
    pub body: String,
}

pub async fn request(
    addr: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> io::Result<Reply> {
    let mut stream = TcpStream::connect(addr).await?;
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    if let Some(body) = body {
        head.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    head.push_str("\r\n");
    head.push_str(body.unwrap_or_default());
    stream.write_all(head.as_bytes()).await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    parse(&raw)
}

fn broken(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("кривой ответ HTTP: {what}"),
    )
}

fn parse(raw: &[u8]) -> io::Result<Reply> {
    let end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| broken("нет конца заголовков"))?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| broken("заголовки не UTF-8"))?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| broken("строка статуса"))?;
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok());
    let rest = &raw[end + 4..];
    let body = match length {
        Some(length) => rest
            .get(..length)
            .ok_or_else(|| broken("тело короче Content-Length"))?,
        None => rest,
    };
    Ok(Reply {
        status,
        body: String::from_utf8_lossy(body).into_owned(),
    })
}
