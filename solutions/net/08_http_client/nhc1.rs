#[derive(Debug, Clone, PartialEq)]
struct Url {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UrlError {
    Scheme,
    UserInfo,
    Host,
    Port,
    BadChar,
}

#[allow(dead_code)]
impl Url {
    fn authority(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == 80 { host } else { format!("{host}:{}", self.port) }
    }
}

fn parse_url(input: &str) -> Result<Url, UrlError> {
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

fn main() {
    println!("{:?}", parse_url("http://example.com:8080/a?b=1"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(host: &str, port: u16, path: &str) -> Url {
        Url { host: host.to_string(), port, path: path.to_string() }
    }

    #[test]
    fn simple_address() {
        assert_eq!(parse_url("http://example.com/a/b"), Ok(url("example.com", 80, "/a/b")));
        assert_eq!(parse_url("http://example.com:8080/x"), Ok(url("example.com", 8080, "/x")));
    }

    #[test]
    fn empty_path_is_a_slash() {
        assert_eq!(parse_url("http://example.com"), Ok(url("example.com", 80, "/")), "у адреса без пути запрос идёт на '/'");
        assert_eq!(parse_url("http://example.com:81"), Ok(url("example.com", 81, "/")));
    }

    #[test]
    fn query_stays_and_fragment_goes() {
        assert_eq!(parse_url("http://h.io/search?q=a&n=2"), Ok(url("h.io", 80, "/search?q=a&n=2")));
        assert_eq!(parse_url("http://h.io/page#top"), Ok(url("h.io", 80, "/page")), "фрагмент остаётся у клиента и на сервер не уходит");
        assert_eq!(parse_url("http://h.io?x=1"), Ok(url("h.io", 80, "/?x=1")), "запрос без пути: путь это '/', запрос сохраняется");
        assert_eq!(parse_url("http://h.io#only"), Ok(url("h.io", 80, "/")));
        assert_eq!(parse_url("http://h.io/a?b=c#d?e"), Ok(url("h.io", 80, "/a?b=c")));
    }

    #[test]
    fn scheme_and_host_ignore_case() {
        assert_eq!(parse_url("HTTP://ExAmple.COM/Path"), Ok(url("example.com", 80, "/Path")), "хост приводится к нижнему регистру, путь нет");
    }

    #[test]
    fn ipv6_lives_in_brackets() {
        assert_eq!(parse_url("http://[::1]:8080/x"), Ok(url("::1", 8080, "/x")), "в Url хост хранится без скобок");
        assert_eq!(parse_url("http://[::1]/x"), Ok(url("::1", 80, "/x")), "двоеточия внутри скобок не порт");
        assert_eq!(parse_url("http://[2001:db8::7]:9/"), Ok(url("2001:db8::7", 9, "/")));
        assert_eq!(parse_url("http://::1/x"), Err(UrlError::Host), "IPv6 без скобок однозначно не разобрать");
        assert_eq!(parse_url("http://[::1/x"), Err(UrlError::Host));
        assert_eq!(parse_url("http://[not-an-ip]/"), Err(UrlError::Host));
        assert_eq!(parse_url("http://[::1]x/"), Err(UrlError::Host));
    }

    #[test]
    fn bad_ports() {
        for bad in ["http://h:/", "http://h:0/", "http://h:65536/", "http://h:+80/", "http://h:8a/", "http://h:-1/"] {
            assert_eq!(parse_url(bad), Err(UrlError::Port), "порт в {bad:?} недопустим");
        }
        assert_eq!(parse_url("http://h:65535/"), Ok(url("h", 65535, "/")));
    }

    #[test]
    fn other_schemes_and_junk() {
        assert_eq!(parse_url("https://h/"), Err(UrlError::Scheme), "TLS у нас появится в последней главе раздела");
        assert_eq!(parse_url("ftp://h/"), Err(UrlError::Scheme));
        assert_eq!(parse_url("h/path"), Err(UrlError::Scheme));
        assert_eq!(parse_url("http:///path"), Err(UrlError::Host));
        assert_eq!(parse_url("http://user:pw@h/"), Err(UrlError::UserInfo), "логин в адресе не поддерживаем: он утекает в журналы");
    }

    #[test]
    fn whitespace_cannot_reach_the_request_line() {
        assert_eq!(parse_url("http://h/a b"), Err(UrlError::BadChar));
        assert_eq!(parse_url("http://h/a\r\nHost: evil"), Err(UrlError::BadChar), "перевод строки в адресе это впрыск заголовков");
        assert_eq!(parse_url("http://h\n/"), Err(UrlError::BadChar));
    }
}
