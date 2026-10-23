
use std::net::IpAddr;

fn main() {}


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


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Dns(HashMap<&'static str, Vec<IpAddr>>);

    impl Resolver for Dns {
        fn resolve(&self, host: &str) -> Vec<IpAddr> {
            self.0.get(host.to_ascii_lowercase().as_str()).cloned().unwrap_or_default()
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn dns() -> Dns {
        Dns(HashMap::from([
            ("example.com", vec![ip("93.184.216.34")]),
            ("localhost", vec![ip("127.0.0.1")]),
            ("internal.corp", vec![ip("10.1.2.3")]),
            ("mixed.example.com", vec![ip("93.184.216.34"), ip("10.0.0.1")]),
            ("v6.example.com", vec![ip("2606:2800:220:1:248:1893:25c8:1946")]),
        ]))
    }

    #[test]
    fn ordinary_addresses_pass_and_are_pinned() {
        let t = check_url("http://example.com/a?b=1", &dns()).unwrap();
        assert_eq!(t, Target { host: "example.com".into(), addr: ip("93.184.216.34"), port: 80, path: "/a?b=1".into(), https: false });
        let t = check_url("HTTPS://Example.COM:443/x", &dns()).unwrap();
        assert_eq!((t.https, t.port, t.host.as_str(), t.path.as_str()), (true, 443, "example.com", "/x"));
        assert_eq!(check_url("https://example.com", &dns()).unwrap().path, "/", "пустой путь это корень");
        assert!(check_url("https://8.8.8.8/", &dns()).is_ok());
        assert!(check_url("http://[2606:2800:220:1:248:1893:25c8:1946]/", &dns()).is_ok());
        assert!(check_url("http://v6.example.com/", &dns()).is_ok());
        assert!(check_url("http://172.32.0.1/", &dns()).is_ok(), "172.32.0.0 уже не частная сеть: диапазон 172.16..172.31");
        assert!(check_url("http://172.15.255.255/", &dns()).is_ok());
    }

    #[test]
    fn only_http_and_https_are_allowed() {
        for bad in ["ftp://example.com/", "file:///etc/passwd", "gopher://example.com/", "javascript:alert(1)", "//example.com/", "example.com", "dict://example.com:11211/"] {
            assert_eq!(check_url(bad, &dns()), Err(SsrfError::BadScheme), "{bad:?}");
        }
    }

    #[test]
    fn internal_addresses_are_forbidden() {
        for bad in [
            "http://127.0.0.1/",
            "http://localhost/",
            "http://[::1]/",
            "http://10.0.0.5/admin",
            "http://192.168.1.1/",
            "http://172.16.0.1/",
            "http://172.31.255.255/",
            "http://169.254.169.254/latest/meta-data/",
            "http://0.0.0.0/",
            "http://100.64.0.1/",
            "http://[fe80::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://[::ffff:169.254.169.254]/",
            "http://internal.corp/",
            "http://224.0.0.1/",
            "http://255.255.255.255/",
        ] {
            assert!(matches!(check_url(bad, &dns()), Err(SsrfError::Forbidden(_))), "{bad:?} прошёл, а это внутренний адрес");
        }
    }

    #[test]
    fn every_resolved_address_must_be_public() {
        assert!(matches!(check_url("http://mixed.example.com/", &dns()), Err(SsrfError::Forbidden(a)) if a == ip("10.0.0.1")), "имя вернуло и публичный, и частный адрес: нельзя проверять только первый");
        assert_eq!(check_url("http://unknown.example.com/", &dns()), Err(SsrfError::NoAddress));
    }

    #[test]
    fn userinfo_ports_and_disguised_hosts_are_refused() {
        assert_eq!(check_url("http://example.com@127.0.0.1/", &dns()), Err(SsrfError::Userinfo), "под видом example.com скрыт 127.0.0.1");
        assert_eq!(check_url("http://user:pw@example.com/", &dns()), Err(SsrfError::Userinfo));
        for port in ["22", "6379", "8080", "25"] {
            assert_eq!(check_url(&format!("http://example.com:{port}/"), &dns()), Err(SsrfError::BadPort), "порт {port}");
        }
        for bad in ["http://2130706433/", "http://0x7f000001/", "http://127.1/", "http://0177.0.0.1/", "http:///path", "http://exa mple.com/", "http://example..com/"] {
            assert!(matches!(check_url(bad, &dns()), Err(SsrfError::BadUrl)), "{bad:?}: библиотека соединения может превратить это в 127.0.0.1");
        }
    }
}
