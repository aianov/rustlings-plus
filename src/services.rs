use std::{
    env,
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use crate::i18n::{fill, tr};

pub struct Service {
    pub name: &'static str,
    pub env: &'static str,
    pub default: &'static str,
}

pub const SERVICES: [Service; 4] = [
    Service {
        name: "postgres",
        env: "RUSTLINGS_PG_URL",
        default: "postgres://rustlings:rustlings@127.0.0.1:55432/rustlings",
    },
    Service {
        name: "scylla",
        env: "RUSTLINGS_SCYLLA_URI",
        default: "127.0.0.1:59042",
    },
    Service {
        name: "nats",
        env: "RUSTLINGS_NATS_URL",
        default: "nats://127.0.0.1:54222",
    },
    Service {
        name: "redis",
        env: "RUSTLINGS_REDIS_URL",
        default: "redis://127.0.0.1:56379",
    },
];

pub fn host_port(url: &str) -> Option<(String, u16)> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let rest = rest.split(['/', '?', ',']).next()?;
    let (host, port) = rest.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some((host.to_owned(), port.parse().ok()?))
}

pub fn address(name: &str) -> Option<(String, u16)> {
    let service = SERVICES.iter().find(|s| s.name == name)?;
    let url = env::var(service.env)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| service.default.to_owned());
    host_port(&url)
}

pub fn reachable(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok())
}

pub fn down_message(required: &[String]) -> Option<String> {
    let mut messages = Vec::new();
    for name in required {
        let Some((host, port)) = address(name) else {
            continue;
        };
        if !reachable(&host, port) {
            let addr = format!("{host}:{port}");
            messages.push(fill(tr().service_down, &[name, &addr, name]));
        }
    }
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_addresses() {
        let hp = |s| host_port(s).map(|(h, p)| format!("{h}:{p}"));
        assert_eq!(
            hp("postgres://rustlings:rustlings@127.0.0.1:55432/rustlings").as_deref(),
            Some("127.0.0.1:55432")
        );
        assert_eq!(hp("127.0.0.1:59042").as_deref(), Some("127.0.0.1:59042"));
        assert_eq!(
            hp("nats://127.0.0.1:54222").as_deref(),
            Some("127.0.0.1:54222")
        );
        assert_eq!(hp("redis://localhost:1/0").as_deref(), Some("localhost:1"));
        assert_eq!(hp("nohost"), None);
    }

    #[test]
    fn closed_port_is_down() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(reachable("127.0.0.1", port));
        assert!(!reachable("127.0.0.1", 1));
    }
}
