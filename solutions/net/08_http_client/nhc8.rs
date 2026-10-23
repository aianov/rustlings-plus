#![allow(dead_code)]
use std::future::Future;

type Headers = Vec<(String, String)>;

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

#[derive(Debug, Clone, PartialEq)]
struct Req {
    method: String,
    url: Url,
    headers: Headers,
    body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Resp {
    status: u16,
    headers: Headers,
    body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RedirectError {
    TooMany,
    BadLocation,
    Scheme,
}

fn header<'a>(headers: &'a Headers, name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn resolve(base: &Url, location: &str) -> Result<Url, RedirectError> {
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

async fn follow<F, Fut>(first: Req, max_redirects: usize, mut send: F) -> Result<Resp, RedirectError>
where
    F: FnMut(Req) -> Fut,
    Fut: Future<Output = Resp>,
{
    let mut request = first;
    let mut hops = 0;
    loop {
        let response = send(request.clone()).await;
        if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let Some(location) = header(&response.headers, "location") else {
            return Ok(response);
        };
        if hops >= max_redirects {
            return Err(RedirectError::TooMany);
        }
        hops += 1;
        let url = resolve(&request.url, location)?;
        let mut next = Req { method: request.method.clone(), url, headers: request.headers.clone(), body: request.body.clone() };
        let to_get = (response.status == 303 && request.method != "HEAD") || (matches!(response.status, 301 | 302) && request.method == "POST");
        if to_get {
            next.method = "GET".to_string();
            next.body = None;
            next.headers.retain(|(n, _)| !["content-type", "content-length", "content-encoding", "transfer-encoding"].contains(&n.to_ascii_lowercase().as_str()));
        }
        if (next.url.host.as_str(), next.url.port) != (request.url.host.as_str(), request.url.port) {
            next.headers.retain(|(n, _)| !["authorization", "cookie", "proxy-authorization"].contains(&n.to_ascii_lowercase().as_str()));
        }
        request = next;
    }
}

#[tokio::main]
async fn main() {
    let base = parse_url("http://example.com/a/b").unwrap();
    println!("{:?}", resolve(&base, "../c"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn u(s: &str) -> Url {
        parse_url(s).unwrap()
    }

    fn pair(n: &str, v: &str) -> (String, String) {
        (n.to_string(), v.to_string())
    }

    fn resp(status: u16, location: Option<&str>) -> Resp {
        Resp { status, headers: location.map(|l| vec![pair("Location", l)]).unwrap_or_default(), body: b"x".to_vec() }
    }

    fn req(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> Req {
        Req { method: method.to_string(), url: u(url), headers: headers.iter().map(|(n, v)| pair(n, v)).collect(), body: body.map(|b| b.to_vec()) }
    }

    type Log = Rc<RefCell<Vec<Req>>>;

    async fn run(first: Req, max: usize, script: &[(&str, Resp)]) -> (Result<Resp, RedirectError>, Vec<Req>) {
        let log: Log = Rc::default();
        let table: HashMap<String, Resp> = script.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        let l = log.clone();
        let result = follow(first, max, move |r: Req| {
            let key = format!("{}{}", r.url.authority(), r.url.path);
            l.borrow_mut().push(r);
            let answer = table.get(&key).cloned().unwrap_or_else(|| Resp { status: 404, headers: vec![], body: key.into_bytes() });
            async move { answer }
        })
        .await;
        let calls = log.borrow().clone();
        (result, calls)
    }

    #[test]
    fn locations_in_all_shapes() {
        let base = u("http://a.test:8080/dir/page?x=1");
        let cases = [
            ("http://b.test/z", "http://b.test/z"),
            ("//c.test:9/q?r=2", "http://c.test:9/q?r=2"),
            ("/root?k=v", "http://a.test:8080/root?k=v"),
            ("sibling", "http://a.test:8080/dir/sibling"),
            ("sibling?y=2", "http://a.test:8080/dir/sibling?y=2"),
            ("../up", "http://a.test:8080/up"),
            ("./same", "http://a.test:8080/dir/same"),
            ("a/../b/./c", "http://a.test:8080/dir/b/c"),
            ("?only=query", "http://a.test:8080/dir/page?only=query"),
            ("/path#frag", "http://a.test:8080/path"),
            ("../../../way/up", "http://a.test:8080/way/up"),
            ("sub/", "http://a.test:8080/dir/sub/"),
            ("..", "http://a.test:8080/"),
        ];
        for (location, expected) in cases {
            assert_eq!(resolve(&base, location), Ok(u(expected)), "Location {location:?} относительно {:?}", base.path);
        }
    }

    #[test]
    fn bad_locations() {
        let base = u("http://a.test/x");
        assert_eq!(resolve(&base, "https://a.test/secure"), Err(RedirectError::Scheme), "https пока не умеем");
        assert_eq!(resolve(&base, "http://[::1/x"), Err(RedirectError::BadLocation));
        assert_eq!(resolve(&base, "/a b"), Err(RedirectError::BadLocation), "пробел в адресе");
        assert_eq!(resolve(&base, "http://h:99999/"), Err(RedirectError::BadLocation));
    }

    #[tokio::test]
    async fn no_redirect_returns_the_response() {
        let (result, calls) = run(req("GET", "http://a.test/", &[], None), 5, &[("a.test/", resp(200, None))]).await;
        assert_eq!(result.unwrap().status, 200);
        assert_eq!(calls.len(), 1);
        let (result, calls) = run(req("GET", "http://a.test/", &[], None), 5, &[("a.test/", resp(304, Some("/ignored")))]).await;
        assert_eq!(result.unwrap().status, 304, "304 не переход, даже если в нём есть Location");
        assert_eq!(calls.len(), 1);
        let (result, calls) = run(req("GET", "http://a.test/", &[], None), 5, &[("a.test/", resp(302, None))]).await;
        assert_eq!(result.unwrap().status, 302, "переход без Location вернуть как есть: вести некуда");
        assert_eq!(calls.len(), 1);
    }

    #[tokio::test]
    async fn chain_is_followed() {
        let script = [("a.test/old", resp(301, Some("/mid"))), ("a.test/mid", resp(302, Some("new"))), ("a.test/new", resp(200, None))];
        let (result, calls) = run(req("GET", "http://a.test/old", &[("Accept", "x")], None), 5, &script).await;
        assert_eq!(result.unwrap().status, 200);
        let paths: Vec<&str> = calls.iter().map(|c| c.url.path.as_str()).collect();
        assert_eq!(paths, ["/old", "/mid", "/new"]);
        assert!(calls.iter().all(|c| c.headers.contains(&pair("Accept", "x"))), "обычные заголовки сохраняются на всём пути");
    }

    #[tokio::test]
    async fn post_becomes_get_on_301_302_303() {
        for status in [301, 302, 303] {
            let script = [("a.test/form", resp(status, Some("/done"))), ("a.test/done", resp(200, None))];
            let first = req("POST", "http://a.test/form", &[("Content-Type", "text/plain"), ("Content-Length", "3"), ("X-Keep", "1")], Some(b"abc"));
            let (result, calls) = run(first, 5, &script).await;
            assert_eq!(result.unwrap().status, 200);
            assert_eq!(calls[1].method, "GET", "после {status} POST превращается в GET");
            assert_eq!(calls[1].body, None, "у GET тела нет");
            let names: Vec<String> = calls[1].headers.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
            assert_eq!(names, ["x-keep"], "заголовки про тело ({status}) переходят к GET бессмысленно и вредно");
        }
    }

    #[tokio::test]
    async fn get_stays_get_and_303_keeps_head() {
        let script = [("a.test/", resp(303, Some("/next"))), ("a.test/next", resp(200, None))];
        let (_, calls) = run(req("HEAD", "http://a.test/", &[], None), 5, &script).await;
        assert_eq!(calls[1].method, "HEAD", "HEAD после 303 остаётся HEAD");
        let (_, calls) = run(req("PUT", "http://a.test/", &[], Some(b"v")), 5, &script).await;
        assert_eq!(calls[1].method, "GET", "303 это «иди посмотри результат»: для любого метода, кроме HEAD, это GET");
        let script = [("a.test/", resp(302, Some("/next"))), ("a.test/next", resp(200, None))];
        let (_, calls) = run(req("PUT", "http://a.test/", &[], Some(b"v")), 5, &script).await;
        assert_eq!(calls[1].method, "PUT", "302 меняет на GET только POST, остальные методы остаются");
        assert_eq!(calls[1].body.as_deref(), Some(b"v".as_slice()));
    }

    #[tokio::test]
    async fn temporary_and_permanent_307_308_keep_method_and_body() {
        for status in [307, 308] {
            let script = [("a.test/upload", resp(status, Some("http://a.test/upload2"))), ("a.test/upload2", resp(201, None))];
            let first = req("POST", "http://a.test/upload", &[("Content-Type", "application/json"), ("Content-Length", "2")], Some(b"{}"));
            let (result, calls) = run(first, 5, &script).await;
            assert_eq!(result.unwrap().status, 201);
            assert_eq!(calls[1].method, "POST", "{status} сохраняет метод");
            assert_eq!(calls[1].body.as_deref(), Some(b"{}".as_slice()), "{status} сохраняет тело");
            assert!(calls[1].headers.contains(&pair("Content-Type", "application/json")));
        }
    }

    #[tokio::test]
    async fn credentials_do_not_follow_to_another_host() {
        let script = [
            ("a.test/start", resp(302, Some("/same"))),
            ("a.test/same", resp(302, Some("http://b.test/other"))),
            ("b.test/other", resp(302, Some("http://a.test/back"))),
            ("a.test/back", resp(200, None)),
        ];
        let first = req("GET", "http://a.test/start", &[("Authorization", "Bearer s3cret"), ("cookie", "sid=1"), ("Accept", "*/*")], None);
        let (result, calls) = run(first, 9, &script).await;
        assert_eq!(result.unwrap().status, 200);
        let has_auth = |c: &Req| c.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("authorization") || n.eq_ignore_ascii_case("cookie"));
        assert!(has_auth(&calls[0]) && has_auth(&calls[1]), "на тот же хост учётные данные идут");
        assert!(!has_auth(&calls[2]), "на чужой хост Authorization и Cookie уходить не должны: {:?}", calls[2].headers);
        assert!(!has_auth(&calls[3]), "вернувшись на исходный хост, их уже не вернуть: клиент про них забыл");
        assert!(calls[3].headers.contains(&pair("Accept", "*/*")));
    }

    #[tokio::test]
    async fn different_port_is_a_different_host() {
        let script = [("a.test/x", resp(302, Some("http://a.test:81/y"))), ("a.test:81/y", resp(200, None))];
        let (_, calls) = run(req("GET", "http://a.test/x", &[("Authorization", "t")], None), 5, &script).await;
        assert!(calls[1].headers.is_empty(), "другой порт это другой источник, Authorization туда не идёт");
    }

    #[tokio::test]
    async fn loops_end_with_an_error() {
        let script = [("a.test/a", resp(302, Some("/b"))), ("a.test/b", resp(302, Some("/a")))];
        let (result, calls) = run(req("GET", "http://a.test/a", &[], None), 5, &script).await;
        assert_eq!(result.unwrap_err(), RedirectError::TooMany);
        assert_eq!(calls.len(), 6, "первый запрос и ровно 5 переходов, потом отказ");
        let (result, calls) = run(req("GET", "http://a.test/a", &[], None), 0, &script).await;
        assert_eq!(result.unwrap_err(), RedirectError::TooMany, "max_redirects = 0 значит «не переходить»");
        assert_eq!(calls.len(), 1);
        let script = [("a.test/a", resp(302, Some("/b"))), ("a.test/b", resp(200, None))];
        let (result, _) = run(req("GET", "http://a.test/a", &[], None), 1, &script).await;
        assert_eq!(result.unwrap().status, 200, "один переход при лимите 1 допустим");
    }

    #[tokio::test]
    async fn bad_location_surfaces() {
        let script = [("a.test/a", resp(302, Some("https://a.test/secure")))];
        let (result, _) = run(req("GET", "http://a.test/a", &[], None), 5, &script).await;
        assert_eq!(result.unwrap_err(), RedirectError::Scheme);
    }
}
