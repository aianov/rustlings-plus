// Клиент уже знает, куда идти. Теперь из Url, метода, заголовков и тела нужно получить байты запроса. Сервер
// верит каждой строке: если вызывающий подсунул свой Host или Content-Length, или в значение заголовка попал
// перевод строки, запрос получится не тем, что задумывалось.
// TODO: Напиши build_request. Строка запроса с путём как есть, потом Host (с портом, если он не 80, IPv6 в скобках),
// потом заголовки вызывающего в его порядке, потом длина тела, пустая строка и тело. Host, Content-Length и
// Transfer-Encoding вызывающего игнорируются: за них отвечает клиент. Опасные символы в методе, заголовках и
// пути это ошибка, а не санитайзинг.

#![allow(dead_code)]

type Headers = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
struct Url {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum RequestError {
    BadMethod,
    BadHeader,
    BadTarget,
}

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn build_request(method: &str, url: &Url, headers: &[(String, String)], body: Option<&[u8]>) -> Result<Vec<u8>, RequestError> {
    todo!()
}

fn main() {
    let url = parse_url("http://example.com/hello").unwrap();
    let wire = build_request("GET", &url, &[], None).unwrap();
    print!("{}", String::from_utf8_lossy(&wire));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    fn text(wire: Vec<u8>) -> String {
        String::from_utf8(wire).unwrap()
    }

    #[test]
    fn plain_get() {
        let url = parse_url("http://example.com/films?genre=drama").unwrap();
        let wire = text(build_request("GET", &url, &[h("Accept", "application/json")], None).unwrap());
        assert_eq!(wire, "GET /films?genre=drama HTTP/1.1\r\nHost: example.com\r\nAccept: application/json\r\n\r\n", "{wire:?}");
    }

    #[test]
    fn host_carries_the_port_when_it_is_not_80() {
        let url = parse_url("http://localhost:3000/").unwrap();
        let wire = text(build_request("GET", &url, &[], None).unwrap());
        assert!(wire.contains("\r\nHost: localhost:3000\r\n"), "для нестандартного порта Host пишется с портом: {wire:?}");
        let url = parse_url("http://[::1]:8080/").unwrap();
        let wire = text(build_request("GET", &url, &[], None).unwrap());
        assert!(wire.contains("\r\nHost: [::1]:8080\r\n"), "IPv6 в Host возвращается в скобки: {wire:?}");
        let url = parse_url("http://[::1]/").unwrap();
        let wire = text(build_request("GET", &url, &[], None).unwrap());
        assert!(wire.contains("\r\nHost: [::1]\r\n"), "{wire:?}");
    }

    #[test]
    fn body_gets_a_length() {
        let url = parse_url("http://h/items").unwrap();
        let wire = build_request("POST", &url, &[h("Content-Type", "text/plain")], Some("привет".as_bytes())).unwrap();
        let text = String::from_utf8(wire).unwrap();
        assert!(text.contains("Content-Length: 12\r\n"), "длина считается в байтах, а не в символах: {text:?}");
        assert!(text.ends_with("\r\n\r\nпривет"), "после пустой строки идёт тело: {text:?}");
    }

    #[test]
    fn empty_post_still_says_zero() {
        let url = parse_url("http://h/items").unwrap();
        for method in ["POST", "PUT", "PATCH"] {
            let wire = text(build_request(method, &url, &[], None).unwrap());
            assert!(wire.contains("Content-Length: 0\r\n"), "{method} без тела должен сказать Content-Length: 0, иначе сервер подождёт тело: {wire:?}");
        }
        let wire = text(build_request("GET", &url, &[], None).unwrap());
        assert!(!wire.to_ascii_lowercase().contains("content-length"), "у GET без тела длины нет: {wire:?}");
        let wire = text(build_request("DELETE", &url, &[], None).unwrap());
        assert!(!wire.to_ascii_lowercase().contains("content-length"), "{wire:?}");
    }

    #[test]
    fn caller_cannot_break_framing() {
        let url = parse_url("http://real.example/").unwrap();
        let wire = text(
            build_request("POST", &url, &[h("host", "evil.example"), h("Content-Length", "999"), h("Transfer-Encoding", "chunked"), h("X-A", "1")], Some(b"abc"))
                .unwrap(),
        );
        assert_eq!(wire.matches("Host:").count() + wire.matches("host:").count(), 1, "Host ровно один: {wire:?}");
        assert!(wire.contains("Host: real.example"), "Host берётся из адреса, а не из заголовков вызывающего: {wire:?}");
        assert_eq!(wire.to_ascii_lowercase().matches("content-length").count(), 1, "длина тела считается самим клиентом: {wire:?}");
        assert!(wire.contains("Content-Length: 3\r\n"), "{wire:?}");
        assert!(!wire.to_ascii_lowercase().contains("transfer-encoding"), "клиент не умеет отправлять chunked, а значит и не заявляет его: {wire:?}");
        assert!(wire.contains("X-A: 1\r\n"), "{wire:?}");
    }

    #[test]
    fn header_order_is_kept() {
        let url = parse_url("http://h/").unwrap();
        let wire = text(build_request("GET", &url, &[h("B", "1"), h("A", "2"), h("B", "3")], None).unwrap());
        let b1 = wire.find("B: 1").unwrap();
        let a = wire.find("A: 2").unwrap();
        let b3 = wire.find("B: 3").unwrap();
        assert!(b1 < a && a < b3, "порядок и повторы заголовков сохраняются: {wire:?}");
    }

    #[test]
    fn injection_is_refused() {
        let url = parse_url("http://h/").unwrap();
        assert_eq!(build_request("GET", &url, &[h("X", "a\r\nHost: evil")], None), Err(RequestError::BadHeader), "перевод строки в значении заголовка");
        assert_eq!(build_request("GET", &url, &[h("X\r\nY", "a")], None), Err(RequestError::BadHeader));
        assert_eq!(build_request("GET", &url, &[h("", "a")], None), Err(RequestError::BadHeader));
        assert_eq!(build_request("GET / HTTP/1.1\r\nX: y\r\n\r\nGET", &url, &[], None), Err(RequestError::BadMethod));
        assert_eq!(build_request("", &url, &[], None), Err(RequestError::BadMethod));
        let bad = Url { host: "h".into(), port: 80, path: "/a b".into() };
        assert_eq!(build_request("GET", &bad, &[], None), Err(RequestError::BadTarget));
        let bad = Url { host: "h".into(), port: 80, path: "no-slash".into() };
        assert_eq!(build_request("GET", &bad, &[], None), Err(RequestError::BadTarget));
    }
}
