
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


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub location: Option<String>,
    pub body_len: usize,
    pub title: Option<String>,
}

/// Сеть. `resolve` спрашивает DNS, `get_by_name` соединяется по имени (клиент сам спросит DNS ещё раз), `get` соединяется с указанным адресом.
pub trait Net {
    fn resolve(&self, host: &str) -> Vec<IpAddr>;
    fn get_by_name(&self, host: &str, port: u16, https: bool, path: &str) -> Response;
    fn get(&self, addr: IpAddr, host: &str, port: u16, https: bool, path: &str) -> Response;
}

struct NetResolver<'a>(&'a dyn Net);

impl Resolver for NetResolver<'_> {
    fn resolve(&self, host: &str) -> Vec<IpAddr> {
        self.0.resolve(host)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreviewError {
    Ssrf(SsrfError),
    TooManyRedirects,
    TooLarge,
    Status(u16),
}

const MAX_BODY: usize = 1024 * 1024;

pub fn fetch_preview(net: &dyn Net, url: &str) -> Result<Option<String>, PreviewError> {
    let resolver = NetResolver(net);
    let mut target = check_url(url, &resolver).map_err(PreviewError::Ssrf)?;
    for _ in 0..=3 {
        let r = net.get(target.addr, &target.host, target.port, target.https, &target.path);
        if let Some(next) = r.location.filter(|_| (300..400).contains(&r.status)) {
            let absolute = if next.starts_with('/') {
                format!("{}://{}{}", if target.https { "https" } else { "http" }, target.host, next)
            } else {
                next
            };
            if next_hop_is_same_host(&absolute, &target) {
                target.path = absolute.splitn(4, '/').nth(3).map_or("/".to_string(), |p| format!("/{p}"));
            } else {
                target = check_url(&absolute, &resolver).map_err(PreviewError::Ssrf)?;
            }
            continue;
        }
        if r.status != 200 {
            return Err(PreviewError::Status(r.status));
        }
        if r.body_len > MAX_BODY {
            return Err(PreviewError::TooLarge);
        }
        return Ok(r.title);
    }
    Err(PreviewError::TooManyRedirects)
}

fn next_hop_is_same_host(absolute: &str, current: &Target) -> bool {
    let prefix = format!("{}://{}/", if current.https { "https" } else { "http" }, current.host);
    absolute.to_ascii_lowercase().starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};

    #[derive(Default)]
    struct Fake {
        dns: RefCell<HashMap<String, VecDeque<Vec<IpAddr>>>>,
        pages: HashMap<(String, String), Response>,
        connections: RefCell<Vec<IpAddr>>,
        resolves: RefCell<HashMap<String, u32>>,
    }

    impl Fake {
        fn dns(self, host: &str, answers: &[&[&str]]) -> Fake {
            self.dns.borrow_mut().insert(host.to_string(), answers.iter().map(|a| a.iter().map(|s| s.parse().unwrap()).collect()).collect());
            self
        }
        fn page(mut self, host: &str, path: &str, r: Response) -> Fake {
            self.pages.insert((host.to_string(), path.to_string()), r);
            self
        }
        fn serve(&self, host: &str, path: &str) -> Response {
            self.pages.get(&(host.to_string(), path.to_string())).cloned().unwrap_or(Response { status: 404, location: None, body_len: 0, title: None })
        }
    }

    impl Net for Fake {
        fn resolve(&self, host: &str) -> Vec<IpAddr> {
            *self.resolves.borrow_mut().entry(host.to_string()).or_default() += 1;
            let mut dns = self.dns.borrow_mut();
            let answers = dns.get_mut(host).expect("неизвестное имя");
            if answers.len() > 1 { answers.pop_front().unwrap() } else { answers[0].clone() }
        }
        fn get_by_name(&self, host: &str, _port: u16, _https: bool, path: &str) -> Response {
            let addr = self.resolve(host)[0];
            self.connections.borrow_mut().push(addr);
            self.serve(host, path)
        }
        fn get(&self, addr: IpAddr, host: &str, _port: u16, _https: bool, path: &str) -> Response {
            self.connections.borrow_mut().push(addr);
            self.serve(host, path)
        }
    }

    fn ok(title: &str) -> Response {
        Response { status: 200, location: None, body_len: 500, title: Some(title.to_string()) }
    }

    fn redirect(to: &str) -> Response {
        Response { status: 302, location: Some(to.to_string()), body_len: 0, title: None }
    }

    fn private(addr: &IpAddr) -> bool {
        !is_public(*addr)
    }

    #[test]
    fn a_normal_page_is_fetched_from_the_checked_address() {
        let net = Fake::default().dns("example.com", &[&["93.184.216.34"]]).page("example.com", "/post", ok("Заголовок"));
        assert_eq!(fetch_preview(&net, "https://example.com/post"), Ok(Some("Заголовок".to_string())));
        assert_eq!(*net.connections.borrow(), vec!["93.184.216.34".parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn internal_targets_are_refused_before_any_connection() {
        let net = Fake::default().dns("localhost", &[&["127.0.0.1"]]).dns("sneaky.example.org", &[&["10.9.9.9"]]);
        for url in ["http://127.0.0.1/", "http://localhost/", "http://sneaky.example.org/", "http://169.254.169.254/", "http://[::1]/", "http://2130706433/"] {
            assert!(matches!(fetch_preview(&net, url), Err(PreviewError::Ssrf(_))), "{url}");
        }
        assert!(net.connections.borrow().is_empty(), "соединение с запрещённой целью было установлено: {:?}", net.connections.borrow());
    }

    #[test]
    fn dns_rebinding_does_not_reach_the_loopback() {
        let net = Fake::default().dns("evil.example.org", &[&["93.184.216.34"], &["127.0.0.1"]]).page("evil.example.org", "/", ok("привет"));
        let r = fetch_preview(&net, "http://evil.example.org/");
        assert!(net.connections.borrow().iter().all(|a| !private(a)), "соединились с внутренним адресом: {:?} (имя при повторном запросе DNS указывает на 127.0.0.1)", net.connections.borrow());
        assert_eq!(r, Ok(Some("привет".to_string())));
        assert_eq!(net.resolves.borrow()["evil.example.org"], 1, "имя должно спрашиваться один раз, а соединение идти по проверенному адресу");
    }

    #[test]
    fn a_redirect_to_the_metadata_service_is_stopped() {
        let net = Fake::default().dns("example.com", &[&["93.184.216.34"]]).page("example.com", "/", redirect("http://169.254.169.254/latest/meta-data/"));
        assert!(matches!(fetch_preview(&net, "http://example.com/"), Err(PreviewError::Ssrf(SsrfError::Forbidden(_)))), "редирект нужно проверять так же, как исходную ссылку");
        assert_eq!(net.connections.borrow().len(), 1, "к служебному адресу не должно быть соединения");
    }

    #[test]
    fn redirects_inside_the_site_and_to_other_public_sites_are_followed() {
        let net = Fake::default()
            .dns("example.com", &[&["93.184.216.34"]])
            .dns("other.example.net", &[&["1.1.1.1"]])
            .page("example.com", "/a", redirect("/b"))
            .page("example.com", "/b", redirect("https://other.example.net/c"))
            .page("other.example.net", "/c", ok("конец"));
        assert_eq!(fetch_preview(&net, "http://example.com/a"), Ok(Some("конец".to_string())));
        assert_eq!(net.connections.borrow().len(), 3);
        assert_eq!(net.resolves.borrow()["other.example.net"], 1);
    }

    #[test]
    fn redirect_loops_end() {
        let net = Fake::default().dns("example.com", &[&["93.184.216.34"]]).page("example.com", "/", redirect("/"));
        assert_eq!(fetch_preview(&net, "http://example.com/"), Err(PreviewError::TooManyRedirects));
        assert!(net.connections.borrow().len() <= 4);
    }

    #[test]
    fn huge_bodies_and_error_statuses_are_refused() {
        let big = Response { status: 200, location: None, body_len: 5 * 1024 * 1024, title: Some("огромная".into()) };
        let net = Fake::default().dns("example.com", &[&["93.184.216.34"]]).page("example.com", "/big", big).page("example.com", "/gone", Response { status: 500, location: None, body_len: 0, title: None });
        assert_eq!(fetch_preview(&net, "http://example.com/big"), Err(PreviewError::TooLarge));
        assert_eq!(fetch_preview(&net, "http://example.com/gone"), Err(PreviewError::Status(500)));
        assert_eq!(fetch_preview(&net, "http://example.com/missing"), Err(PreviewError::Status(404)));
    }
}
