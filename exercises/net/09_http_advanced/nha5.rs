// Фронтенд-разработчик пишет: «У меня в консоли CORS error, но в логах сервера 200. Где-то ещё иногда 401, и тогда
// тоже CORS». Слой CORS у нас вроде есть и на простых запросах работает. Кроме того, после включения cookie
// перестал работать режим «разрешить всем», а кеш на CDN время от времени отдаёт страницам не те заголовки.
// Это три разных поломки в одном слое.
// TODO: Найди их и почини слой cors. Предзапрос уже обрабатывается правильно, не трогай его.

#![allow(dead_code)]
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type Handler = Arc<dyn Fn(Req) -> BoxFuture<Resp> + Send + Sync>;
type Middleware = Arc<dyn Fn(Handler) -> Handler + Send + Sync>;

fn handler<F, Fut>(f: F) -> Handler
where
    F: Fn(Req) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + 'static,
{
    Arc::new(move |request| Box::pin(f(request)))
}

fn middleware<F>(f: F) -> Middleware
where
    F: Fn(Handler) -> Handler + Send + Sync + 'static,
{
    Arc::new(f)
}

#[derive(Debug, Clone)]
struct Policy {
    origins: Vec<String>,
    methods: Vec<String>,
    headers: Vec<String>,
    credentials: bool,
}

fn preflight(policy: &Policy, origin: &str, method: &str, requested: &str) -> Resp {
    let vary = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers";
    let ok_origin = policy.origins.iter().any(|o| o == "*" || o == origin);
    let ok_method = policy.methods.iter().any(|m| m == method);
    let ok_headers = requested.split(',').map(str::trim).filter(|h| !h.is_empty()).all(|h| policy.headers.iter().any(|a| a.eq_ignore_ascii_case(h)));
    if !(ok_origin && ok_method && ok_headers) {
        return Resp::new(403).with("Vary", vary);
    }
    let mut resp = Resp::new(204)
        .with("Access-Control-Allow-Origin", if policy.credentials { origin } else if policy.origins.iter().any(|o| o == "*") { "*" } else { origin })
        .with("Access-Control-Allow-Methods", &policy.methods.join(", "))
        .with("Access-Control-Allow-Headers", &policy.headers.join(", "))
        .with("Vary", vary);
    if policy.credentials {
        resp = resp.with("Access-Control-Allow-Credentials", "true");
    }
    resp
}

fn cors(policy: Policy) -> Middleware {
    middleware(move |next| {
        let policy = policy.clone();
        handler(move |req| {
            let next = next.clone();
            let policy = policy.clone();
            async move {
                let origin = req.header("Origin").map(str::to_string);
                if req.method == "OPTIONS" {
                    if let (Some(origin), Some(method)) = (origin.as_deref(), req.header("Access-Control-Request-Method")) {
                        return preflight(&policy, origin, method, req.header("Access-Control-Request-Headers").unwrap_or(""));
                    }
                }
                let mut resp = next(req).await;
                if resp.status < 300 {
                    if let Some(origin) = origin {
                        if policy.origins.iter().any(|o| o == "*" || *o == origin) {
                            let shown = if policy.origins.iter().any(|o| o == "*") { "*".to_string() } else { origin };
                            resp.headers.push(("Access-Control-Allow-Origin".to_string(), shown));
                            if policy.credentials {
                                resp.headers.push(("Access-Control-Allow-Credentials".to_string(), "true".to_string()));
                            }
                        }
                    }
                }
                resp
            }
        })
    })
}

#[tokio::main]
async fn main() {
    let policy = Policy { origins: vec!["https://app.example".into()], methods: vec!["GET".into()], headers: vec![], credentials: false };
    let app = cors(policy)(handler(|_| async { Resp::new(200) }));
    println!("{:?}", app(Req::new("GET").with("Origin", "https://app.example")).await);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(origins: &[&str], credentials: bool) -> Policy {
        Policy {
            origins: origins.iter().map(|s| s.to_string()).collect(),
            methods: vec!["GET".into(), "POST".into()],
            headers: vec!["Content-Type".into()],
            credentials,
        }
    }

    fn app(policy: Policy, status: u16) -> Handler {
        cors(policy)(handler(move |_| async move { Resp::new(status).with("Vary", "Accept-Encoding") }))
    }

    fn get(origin: Option<&str>) -> Req {
        let r = Req::new("GET");
        match origin {
            Some(o) => r.with("Origin", o),
            None => r,
        }
    }

    const APP: &str = "https://app.example";

    #[tokio::test]
    async fn the_happy_path_works() {
        let r = app(policy(&[APP], false), 200)(get(Some(APP))).await;
        assert_eq!(r.header("access-control-allow-origin"), Some(APP));
    }

    #[tokio::test]
    async fn error_answers_carry_cors_headers_too() {
        for status in [301, 400, 401, 404, 500, 503] {
            let r = app(policy(&[APP], false), status)(get(Some(APP))).await;
            assert_eq!(
                r.header("access-control-allow-origin"),
                Some(APP),
                "ответ {status} без разрешения: фронт увидит в консоли «CORS error», а не настоящую ошибку {status}"
            );
        }
    }

    #[tokio::test]
    async fn wildcard_without_credentials_is_a_star() {
        let r = app(policy(&["*"], false), 200)(get(Some("https://any.example"))).await;
        assert_eq!(r.header("access-control-allow-origin"), Some("*"));
        assert!(r.header("access-control-allow-credentials").is_none());
    }

    #[tokio::test]
    async fn wildcard_with_credentials_must_echo_the_origin() {
        let r = app(policy(&["*"], true), 200)(get(Some("https://any.example"))).await;
        assert_eq!(
            r.header("access-control-allow-origin"),
            Some("https://any.example"),
            "'*' вместе с Allow-Credentials браузеры отвергают: нужен конкретный Origin"
        );
        assert_eq!(r.header("access-control-allow-credentials"), Some("true"));
    }

    #[tokio::test]
    async fn answers_that_depend_on_origin_say_so() {
        for (label, p, origin) in [
            ("разрешённый", policy(&[APP], false), Some(APP)),
            ("чужой", policy(&[APP], false), Some("https://evil.example")),
            ("без Origin", policy(&[APP], false), None),
            ("шаблон с credentials", policy(&["*"], true), Some("https://x.example")),
        ] {
            let r = app(p, 200)(get(origin)).await;
            assert!(r.tokens("vary").contains(&"origin".to_string()), "{label}: нет Vary: Origin, кеш перепутает ответы для разных сайтов: {:?}", r.headers);
            assert!(r.tokens("vary").contains(&"accept-encoding".to_string()), "{label}: чужой Vary от обработчика пропал: {:?}", r.headers);
        }
    }

    #[tokio::test]
    async fn constant_star_needs_no_vary() {
        let r = app(policy(&["*"], false), 200)(get(Some("https://x.example"))).await;
        assert!(!r.tokens("vary").contains(&"origin".to_string()), "ответ не зависит от Origin, Vary: Origin только ухудшил бы кеш");
    }

    #[tokio::test]
    async fn strangers_get_no_permission() {
        let r = app(policy(&[APP], true), 200)(get(Some("https://evil.example"))).await;
        assert!(r.header("access-control-allow-origin").is_none());
        assert!(r.header("access-control-allow-credentials").is_none());
    }

    #[tokio::test]
    async fn preflight_is_answered_without_the_handler() {
        let inner = handler(|_| async { Resp::new(500) });
        let app = cors(policy(&[APP], false))(inner);
        let req = Req::new("OPTIONS").with("Origin", APP).with("Access-Control-Request-Method", "POST").with("Access-Control-Request-Headers", "content-type");
        let r = app(req).await;
        assert_eq!(r.status, 204);
        assert_eq!(r.header("access-control-allow-origin"), Some(APP));
    }
}
