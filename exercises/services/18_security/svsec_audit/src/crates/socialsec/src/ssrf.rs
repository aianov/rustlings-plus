use std::net::IpAddr;

#[derive(Debug, PartialEq, Eq)]
pub enum SsrfError {
    BadUrl,
    BadScheme,
    BadPort,
    Userinfo,
    NoAddress,
    Forbidden(IpAddr),
}

pub trait Resolver {
    fn resolve(&self, host: &str) -> Vec<IpAddr>;
}

/// Проверенная цель: к ней и только к ней надо подключаться.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub addr: IpAddr,
    pub port: u16,
    pub path: String,
    pub https: bool,
}

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || (o[0] == 100 && (64..128).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                || o[0] >= 240
                || o[0] == 0)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || (s[0] == 0x0064 && s[1] == 0xff9b)
                || (s[..6] == [0, 0, 0, 0, 0, 0]))
        }
    }
}

fn valid_host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_'
}

pub fn check_url(url: &str, resolver: &dyn Resolver) -> Result<Target, SsrfError> {
    let (scheme, rest) = url.split_once("://").ok_or(SsrfError::BadScheme)?;
    let https = match scheme.to_ascii_lowercase().as_str() {
        "http" => false,
        "https" => true,
        _ => return Err(SsrfError::BadScheme),
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err(SsrfError::Userinfo);
    }
    let path = if tail.is_empty() || !tail.starts_with('/') { format!("/{}", tail.trim_start_matches('/')) } else { tail.to_string() };
    let (host, port_text) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']').ok_or(SsrfError::BadUrl)?;
        (host, after.strip_prefix(':'))
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let port = match port_text {
        None => {
            if https {
                443
            } else {
                80
            }
        }
        Some(text) => text.parse::<u16>().map_err(|_| SsrfError::BadUrl)?,
    };
    if port != 80 && port != 443 {
        return Err(SsrfError::BadPort);
    }
    if host.is_empty() {
        return Err(SsrfError::BadUrl);
    }
    let addrs: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        if !host.chars().all(valid_host_char) || host.starts_with('.') || host.contains("..") {
            return Err(SsrfError::BadUrl);
        }
        let last = host.trim_end_matches('.').rsplit('.').next().unwrap_or("");
        if !last.chars().any(|c| c.is_ascii_alphabetic()) || last.starts_with("0x") {
            return Err(SsrfError::BadUrl);
        }
        resolver.resolve(host)
    };
    let first = *addrs.first().ok_or(SsrfError::NoAddress)?;
    if let Some(bad) = addrs.iter().find(|a| !is_public(**a)) {
        return Err(SsrfError::Forbidden(*bad));
    }
    Ok(Target { host: host.to_ascii_lowercase(), addr: first, port, path, https })
}
