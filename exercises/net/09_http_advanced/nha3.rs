// Фронт соцсети живёт на https://app.example, а API на https://api.example. Для браузера это разные источники
// (origin), и без разрешения сервера JavaScript не получит ответ, хотя запрос уйдёт и сервер его выполнит. Разрешение
// это заголовки в ответе. Они зависят от Origin запроса, а значит, и общий кеш обязан это знать.
// TODO: Напиши cors_headers: для Origin из списка ответ с Access-Control-Allow-Origin (ровно этого Origin),
// при credentials ещё Allow-Credentials: true, при непустом expose_headers ещё Expose-Headers. Чужому и
// отсутствующему Origin разрешений нет, но Vary: Origin нужен всегда. Сравнение точное: схема, хост и порт.

#![allow(dead_code)]

#[derive(Debug, Clone, Default)]
struct Req {
    method: String,
    headers: Vec<(String, String)>,
}

#[allow(dead_code)]
impl Req {
    fn new(method: &str) -> Req {
        Req { method: method.to_string(), headers: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Req {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[allow(dead_code)]
impl Resp {
    fn new(status: u16) -> Resp {
        Resp { status, headers: Vec::new(), body: Vec::new() }
    }

    fn with(mut self, name: &str, value: &str) -> Resp {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
    }

    fn tokens(&self, name: &str) -> Vec<String> {
        self.all(name).iter().flat_map(|v| v.split(',')).map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()).collect()
    }
}

#[derive(Debug, Clone)]
struct Policy {
    origins: Vec<String>,
    credentials: bool,
    expose_headers: Vec<String>,
}

fn cors_headers(policy: &Policy, origin: Option<&str>) -> Vec<(String, String)> {
    let _ = (policy, origin);
    todo!()
}

fn main() {
    let policy = Policy { origins: vec!["https://app.example".into()], credentials: true, expose_headers: vec![] };
    println!("{:?}", cors_headers(&policy, Some("https://app.example")));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy { origins: vec!["https://app.example".into(), "http://localhost:5173".into()], credentials: false, expose_headers: vec![] }
    }

    fn get(headers: &[(String, String)], name: &str) -> Vec<String> {
        headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()).collect()
    }

    fn has_vary_origin(headers: &[(String, String)]) -> bool {
        get(headers, "vary").iter().flat_map(|v| v.split(',')).any(|t| t.trim().eq_ignore_ascii_case("origin"))
    }

    #[test]
    fn allowed_origin_is_echoed() {
        let h = cors_headers(&policy(), Some("https://app.example"));
        assert_eq!(get(&h, "access-control-allow-origin"), ["https://app.example"], "{h:?}");
        let h = cors_headers(&policy(), Some("http://localhost:5173"));
        assert_eq!(get(&h, "access-control-allow-origin"), ["http://localhost:5173"], "в ответе ровно тот Origin, что прислал браузер, а не весь список");
    }

    #[test]
    fn strangers_get_nothing() {
        for origin in [
            "https://evil.example",
            "https://app.example.evil.com",
            "http://app.example",
            "https://app.example:8443",
            "https://app.example/",
            "https://APP.example",
            "null",
            "",
        ] {
            let h = cors_headers(&policy(), Some(origin));
            assert!(get(&h, "access-control-allow-origin").is_empty(), "origin {origin:?} в список не входит, а ему разрешили");
            assert!(get(&h, "access-control-allow-credentials").is_empty());
        }
    }

    #[test]
    fn vary_origin_is_always_there() {
        for origin in [Some("https://app.example"), Some("https://evil.example"), None] {
            let h = cors_headers(&policy(), origin);
            assert!(has_vary_origin(&h), "ответ зависит от Origin, кеш обязан об этом знать (origin {origin:?}): {h:?}");
        }
    }

    #[test]
    fn no_origin_means_no_cors() {
        let h = cors_headers(&policy(), None);
        assert!(get(&h, "access-control-allow-origin").is_empty(), "запрос без Origin (curl, тот же сайт) в CORS не нуждается");
    }

    #[test]
    fn credentials_only_when_asked() {
        let mut p = policy();
        let h = cors_headers(&p, Some("https://app.example"));
        assert!(get(&h, "access-control-allow-credentials").is_empty());
        p.credentials = true;
        let h = cors_headers(&p, Some("https://app.example"));
        assert_eq!(get(&h, "access-control-allow-credentials"), ["true"]);
        let h = cors_headers(&p, Some("https://evil.example"));
        assert!(get(&h, "access-control-allow-credentials").is_empty(), "чужому разрешение на cookie не даём");
    }

    #[test]
    fn exposed_headers_are_listed_for_allowed_origins_only() {
        let mut p = policy();
        p.expose_headers = vec!["ETag".into(), "X-Request-Id".into()];
        let h = cors_headers(&p, Some("https://app.example"));
        assert_eq!(get(&h, "access-control-expose-headers"), ["ETag, X-Request-Id"], "иначе JavaScript не прочитает эти заголовки ответа");
        let h = cors_headers(&p, Some("https://evil.example"));
        assert!(get(&h, "access-control-expose-headers").is_empty());
    }
}
