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
    methods: Vec<String>,
    headers: Vec<String>,
    credentials: bool,
    max_age: u32,
}

fn preflight(policy: &Policy, req: &Req) -> Option<Resp> {
    let origin = req.header("origin")?;
    let wanted_method = req.header("access-control-request-method")?;
    if req.method != "OPTIONS" {
        return None;
    }
    let vary = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers";
    let requested: Vec<&str> = req.header("access-control-request-headers").unwrap_or("").split(',').map(str::trim).filter(|h| !h.is_empty()).collect();
    let allowed = policy.origins.iter().any(|o| o == origin)
        && policy.methods.iter().any(|m| m == wanted_method)
        && requested.iter().all(|h| policy.headers.iter().any(|a| a.eq_ignore_ascii_case(h)));
    if !allowed {
        return Some(Resp::new(403).with("Vary", vary));
    }
    let mut resp = Resp::new(204)
        .with("Access-Control-Allow-Origin", origin)
        .with("Access-Control-Allow-Methods", &policy.methods.join(", "))
        .with("Access-Control-Allow-Headers", &policy.headers.join(", "))
        .with("Access-Control-Max-Age", &policy.max_age.to_string())
        .with("Vary", vary);
    if policy.credentials {
        resp = resp.with("Access-Control-Allow-Credentials", "true");
    }
    Some(resp)
}

fn main() {
    let policy = Policy { origins: vec!["https://app.example".into()], methods: vec!["GET".into()], headers: vec![], credentials: false, max_age: 600 };
    let req = Req::new("OPTIONS").with("Origin", "https://app.example").with("Access-Control-Request-Method", "GET");
    println!("{:?}", preflight(&policy, &req));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy {
            origins: vec!["https://app.example".into()],
            methods: vec!["GET".into(), "POST".into(), "DELETE".into()],
            headers: vec!["Content-Type".into(), "Authorization".into()],
            credentials: false,
            max_age: 600,
        }
    }

    fn pre(method: &str, headers: Option<&str>) -> Req {
        let mut r = Req::new("OPTIONS").with("Origin", "https://app.example").with("Access-Control-Request-Method", method);
        if let Some(h) = headers {
            r = r.with("Access-Control-Request-Headers", h);
        }
        r
    }

    #[test]
    fn good_preflight_gets_a_204_with_permissions() {
        let r = preflight(&policy(), &pre("POST", Some("content-type, authorization"))).expect("это предзапрос");
        assert_eq!(r.status, 204);
        assert!(r.body.is_empty());
        assert_eq!(r.header("access-control-allow-origin"), Some("https://app.example"));
        let methods = r.tokens("access-control-allow-methods");
        assert!(methods.contains(&"post".to_string()) && methods.contains(&"get".to_string()) && methods.contains(&"delete".to_string()), "{methods:?}");
        let headers = r.tokens("access-control-allow-headers");
        assert!(headers.contains(&"content-type".to_string()) && headers.contains(&"authorization".to_string()), "{headers:?}");
        assert_eq!(r.header("access-control-max-age"), Some("600"), "браузер запомнит разрешение на 10 минут и не будет спрашивать на каждый запрос");
        assert!(r.header("access-control-allow-credentials").is_none());
    }

    #[test]
    fn preflight_varies_on_everything_it_reads() {
        let r = preflight(&policy(), &pre("GET", None)).unwrap();
        let vary = r.tokens("vary");
        for needed in ["origin", "access-control-request-method", "access-control-request-headers"] {
            assert!(vary.contains(&needed.to_string()), "ответ зависит от заголовка {needed}, а Vary его не называет: {vary:?}");
        }
    }

    #[test]
    fn credentials_are_announced_when_enabled() {
        let mut p = policy();
        p.credentials = true;
        let r = preflight(&p, &pre("GET", None)).unwrap();
        assert_eq!(r.header("access-control-allow-credentials"), Some("true"));
    }

    #[test]
    fn requested_headers_are_compared_without_case_and_with_spaces() {
        let r = preflight(&policy(), &pre("GET", Some("  AUTHORIZATION ,Content-Type "))).unwrap();
        assert_eq!(r.status, 204);
        let r = preflight(&policy(), &pre("GET", Some(""))).unwrap();
        assert_eq!(r.status, 204, "пустой список запрошенных заголовков это просто нет заголовков");
    }

    #[test]
    fn what_is_not_allowed_is_refused() {
        for (label, req) in [
            ("метод", pre("PUT", None)),
            ("метод в нижнем регистре", pre("get", None)),
            ("заголовок", pre("GET", Some("content-type, x-evil"))),
            ("origin", Req::new("OPTIONS").with("Origin", "https://evil.example").with("Access-Control-Request-Method", "GET")),
        ] {
            let r = preflight(&policy(), &req).expect("это всё ещё предзапрос, просто неудачный");
            assert_eq!(r.status, 403, "{label}: предзапрос должен быть отвергнут");
            assert!(r.header("access-control-allow-origin").is_none(), "{label}: отказ без разрешений");
            assert!(r.header("access-control-allow-methods").is_none());
        }
    }

    #[test]
    fn plain_options_is_not_a_preflight() {
        assert!(preflight(&policy(), &Req::new("OPTIONS")).is_none(), "OPTIONS без Origin это обычный запрос возможностей");
        assert!(preflight(&policy(), &Req::new("OPTIONS").with("Origin", "https://app.example")).is_none(), "без Access-Control-Request-Method это не предзапрос");
        assert!(preflight(&policy(), &Req::new("GET").with("Origin", "https://app.example").with("Access-Control-Request-Method", "GET")).is_none(), "предзапрос всегда OPTIONS");
    }
}
