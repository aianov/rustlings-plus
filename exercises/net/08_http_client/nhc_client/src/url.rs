pub type Headers = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    Scheme,
    UserInfo,
    Host,
    Port,
    BadChar,
}

impl Url {
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == 80 { host } else { format!("{host}:{}", self.port) }
    }
}

pub fn parse_url(input: &str) -> Result<Url, UrlError> {
    if input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UrlError::BadChar);
    }
    let (scheme, rest) = input.split_once("://").ok_or(UrlError::Scheme)?;
    if !scheme.eq_ignore_ascii_case("http") {
        return Err(UrlError::Scheme);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.contains('@') {
        return Err(UrlError::UserInfo);
    }
    let (host, port) = match authority.strip_prefix('[') {
        Some(inner) => {
            let (host, after) = inner.split_once(']').ok_or(UrlError::Host)?;
            if host.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(UrlError::Host);
            }
            match after {
                "" => (host, None),
                _ => (host, Some(after.strip_prefix(':').ok_or(UrlError::Host)?)),
            }
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    if host.is_empty() || (!authority.starts_with('[') && host.contains(':')) {
        return Err(UrlError::Host);
    }
    let port = match port {
        None => 80,
        Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => match p.parse::<u16>() {
            Ok(0) | Err(_) => return Err(UrlError::Port),
            Ok(n) => n,
        },
        Some(_) => return Err(UrlError::Port),
    };
    let tail = tail.split('#').next().unwrap_or("");
    let path = match tail {
        "" => "/".to_string(),
        t if t.starts_with('?') => format!("/{t}"),
        t => t.to_string(),
    };
    Ok(Url { host: host.to_ascii_lowercase(), port, path })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectError {
    BadLocation,
    Scheme,
}

pub fn resolve(base: &Url, location: &str) -> Result<Url, RedirectError> {
    let location = location.split('#').next().unwrap_or("");
    let absolute = if location.contains("://") {
        location.to_string()
    } else if let Some(rest) = location.strip_prefix("//") {
        format!("http://{rest}")
    } else {
        let (base_path, _) = base.path.split_once('?').unwrap_or((base.path.as_str(), ""));
        let path = if location.starts_with('/') {
            location.to_string()
        } else if location.starts_with('?') {
            format!("{base_path}{location}")
        } else {
            let dir = &base_path[..base_path.rfind('/').map_or(0, |i| i + 1)];
            format!("{dir}{location}")
        };
        format!("http://{}{}", base.authority(), path)
    };
    let mut url = parse_url(&absolute).map_err(|e| if e == UrlError::Scheme { RedirectError::Scheme } else { RedirectError::BadLocation })?;
    let (path, query) = match url.path.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (url.path.clone(), None),
    };
    let segments: Vec<&str> = path.split('/').skip(1).collect();
    let mut kept: Vec<&str> = Vec::new();
    for (i, segment) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        match *segment {
            "." => {
                if last {
                    kept.push("");
                }
            }
            ".." => {
                kept.pop();
                if last {
                    kept.push("");
                }
            }
            s => kept.push(s),
        }
    }
    url.path = format!("/{}", kept.join("/"));
    if let Some(q) = query {
        url.path = format!("{}?{q}", url.path);
    }
    Ok(url)
}
