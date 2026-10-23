use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{Instant, timeout_at};

pub const PROXY_LINE_MAX: usize = 107;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyLine {
    Incomplete,
    Broken,
    Unknown { consumed: usize },
    Peer { addr: SocketAddr, consumed: usize },
}

pub fn proxy_line(client: SocketAddr, local: SocketAddr) -> String {
    let family = match (client, local) {
        (SocketAddr::V4(..), SocketAddr::V4(..)) => "TCP4",
        (SocketAddr::V6(..), SocketAddr::V6(..)) => "TCP6",
        _ => return String::from("PROXY UNKNOWN\r\n"),
    };
    format!(
        "PROXY {family} {} {} {} {}\r\n",
        client.ip(),
        local.ip(),
        client.port(),
        local.port()
    )
}

fn address(family: &str, ip: &str) -> Option<IpAddr> {
    match family {
        "TCP4" => ip.parse::<Ipv4Addr>().ok().map(IpAddr::V4),
        "TCP6" => ip.parse::<Ipv6Addr>().ok().map(IpAddr::V6),
        _ => None,
    }
}

pub fn parse_proxy_line(buf: &[u8]) -> ProxyLine {
    let prefix = b"PROXY ";
    let seen = buf.len().min(prefix.len());
    if buf[..seen] != prefix[..seen] {
        return ProxyLine::Broken;
    }
    let window = &buf[..buf.len().min(PROXY_LINE_MAX)];
    let Some(end) = window.windows(2).position(|pair| pair == b"\r\n") else {
        return if buf.len() >= PROXY_LINE_MAX {
            ProxyLine::Broken
        } else {
            ProxyLine::Incomplete
        };
    };
    let consumed = end + 2;
    let Ok(line) = std::str::from_utf8(&buf[prefix.len()..end]) else {
        return ProxyLine::Broken;
    };
    let parts: Vec<&str> = line.split(' ').collect();
    match parts.as_slice() {
        ["UNKNOWN", ..] => ProxyLine::Unknown { consumed },
        [family, source, target, source_port, target_port] => {
            let ports = (source_port.parse::<u16>(), target_port.parse::<u16>());
            let ips = (address(family, source), address(family, target));
            match (ips, ports) {
                ((Some(ip), Some(..)), (Ok(port), Ok(..))) => ProxyLine::Peer {
                    addr: SocketAddr::new(ip, port),
                    consumed,
                },
                _ => ProxyLine::Broken,
            }
        }
        _ => ProxyLine::Broken,
    }
}

pub(crate) async fn announced(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    wait: Duration,
) -> Option<Option<SocketAddr>> {
    let deadline = Instant::now() + wait;
    let mut chunk = [0u8; 128];
    loop {
        match parse_proxy_line(buf) {
            ProxyLine::Peer { addr, consumed } => {
                buf.drain(..consumed);
                return Some(Some(addr));
            }
            ProxyLine::Unknown { consumed } => {
                buf.drain(..consumed);
                return Some(None);
            }
            ProxyLine::Broken => return None,
            ProxyLine::Incomplete => {}
        }
        match timeout_at(deadline, stream.read(&mut chunk)).await {
            Ok(Ok(0) | Err(..)) | Err(..) => return None,
            Ok(Ok(n)) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}
