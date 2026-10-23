use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum ProbeError {
    Url(String),
    Connect(String),
    Io(String),
    Status(u16),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Url(url) => write!(f, "ждали http://хост:порт/путь, а не «{url}»"),
            ProbeError::Connect(why) => write!(f, "не подключиться: {why}"),
            ProbeError::Io(why) => write!(f, "обмен оборвался: {why}"),
            ProbeError::Status(code) => write!(f, "ответ {code}, а не 200"),
        }
    }
}

impl std::error::Error for ProbeError {}

fn split(url: &str) -> Result<(String, String), ProbeError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| ProbeError::Url(url.to_string()))?;
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    if host.is_empty() || !host.contains(':') {
        return Err(ProbeError::Url(url.to_string()));
    }
    Ok((host.to_string(), path.to_string()))
}

pub fn probe(url: &str, limit: Duration) -> Result<u16, ProbeError> {
    let (host, path) = split(url)?;
    let addr = host
        .to_socket_addrs()
        .map_err(|error| ProbeError::Connect(error.to_string()))?
        .next()
        .ok_or_else(|| ProbeError::Connect(format!("{host} не резолвится")))?;
    let mut stream = TcpStream::connect_timeout(&addr, limit)
        .map_err(|error| ProbeError::Connect(error.to_string()))?;
    stream
        .set_read_timeout(Some(limit))
        .and_then(|()| stream.set_write_timeout(Some(limit)))
        .map_err(|error| ProbeError::Io(error.to_string()))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| ProbeError::Io(error.to_string()))?;
    let mut head = [0u8; 64];
    let read = stream
        .read(&mut head)
        .map_err(|error| ProbeError::Io(error.to_string()))?;
    let line = String::from_utf8_lossy(&head[..read]);
    let code = line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| ProbeError::Io(format!("не HTTP: {line}")))?;
    if code == 200 {
        Ok(code)
    } else {
        Err(ProbeError::Status(code))
    }
}
