use socialsec::net::{Hasher, Net, Response};
use socialsec::{App, Req};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ATTACKER: &str = "198.51.100.66";
const OWNER: &str = "203.0.113.7";
const APP: &str = "https://app.example.com";

struct SlowHasher {
    verifies: AtomicU32,
}

impl Hasher for SlowHasher {
    fn hash(&self, password: &str) -> String {
        format!("slow${password}")
    }
    fn verify(&self, hash: &str, password: &str) -> bool {
        self.verifies.fetch_add(1, Ordering::SeqCst);
        hash == format!("slow${password}")
    }
}

#[derive(Default)]
struct FakeNet {
    dns: Mutex<HashMap<String, VecDeque<Vec<IpAddr>>>>,
    pages: Mutex<HashMap<(String, String), Response>>,
    connections: Mutex<Vec<IpAddr>>,
}

impl FakeNet {
    fn answers(&self, host: &str) -> Vec<IpAddr> {
        let mut dns = self.dns.lock().unwrap();
        match dns.get_mut(host) {
            Some(q) if q.len() > 1 => q.pop_front().unwrap(),
            Some(q) => q[0].clone(),
            None => Vec::new(),
        }
    }
    fn page(&self, host: &str, path: &str) -> Response {
        self.pages.lock().unwrap().get(&(host.to_string(), path.to_string())).cloned().unwrap_or(Response { status: 404, body_len: 0, title: None })
    }
}

impl Net for FakeNet {
    fn resolve(&self, host: &str) -> Vec<IpAddr> {
        self.answers(host)
    }
    fn get_by_name(&self, host: &str, _port: u16, _https: bool, path: &str) -> Response {
        if let Ok(ip) = host.parse::<IpAddr>() {
            self.connections.lock().unwrap().push(ip);
        } else if let Some(ip) = self.answers(host).first() {
            self.connections.lock().unwrap().push(*ip);
        }
        self.page(host, path)
    }
    fn get(&self, addr: IpAddr, host: &str, _port: u16, _https: bool, path: &str) -> Response {
        self.connections.lock().unwrap().push(addr);
        self.page(host, path)
    }
}

struct World {
    app: App,
    hasher: Arc<SlowHasher>,
    net: Arc<FakeNet>,
    t0: Instant,
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn world() -> World {
    let hasher = Arc::new(SlowHasher { verifies: AtomicU32::new(0) });
    let net = Arc::new(FakeNet::default());
    net.dns.lock().unwrap().insert("example.com".into(), VecDeque::from([vec![ip("93.184.216.34")]]));
    net.dns.lock().unwrap().insert("localhost".into(), VecDeque::from([vec![ip("127.0.0.1")]]));
    net.dns.lock().unwrap().insert("evil.example.org".into(), VecDeque::from([vec![ip("93.184.216.35")], vec![ip("127.0.0.1")]]));
    net.pages.lock().unwrap().insert(("example.com".into(), "/post".into()), Response { status: 200, body_len: 900, title: Some("Заголовок".into()) });
    let app = App::new(hasher.clone(), net.clone(), &[APP, "http://localhost:3000"]);
    app.add_user(1, "anna", "Аня", "secret");
    app.add_user(2, "boris", "Боря", "pa55");
    app.add_user(3, "mallory", "x\r\nSet-Cookie: session=evil", "pw");
    app.add_post(10, 1, "публичный пост Ани", false);
    app.add_post(11, 1, "черновик Ани", true);
    app.add_comment(100, 10, 2);
    app.add_comment(101, 10, 2);
    app.add_comment(102, 10, 2);
    app.add_refresh_token("good-token");
    World { app, hasher, net, t0: Instant::now() }
}

impl World {
    fn req(&self, method: &str, path: &str) -> Req {
        Req::new(method, path, ip(OWNER), self.t0)
    }
    fn login(&self, from: &str, name: &str, password: &str, after: Duration) -> socialsec::Resp {
        let req = Req::new("POST", "/login", ip(from), self.t0 + after).body(&format!("name={name}&password={password}"));
        self.app.handle(&req)
    }
}

#[test]
fn normal_flows_work() {
    let w = world();
    assert_eq!(w.login(OWNER, "anna", "secret", Duration::ZERO).status, 200);
    let created = w.app.handle(&w.req("POST", "/posts").session(1).body(r#"{"text":"привет","tags":["a","b"]}"#));
    assert_eq!(created.status, 201);
    let html = w.app.handle(&w.req("GET", "/posts/10/html"));
    assert!(html.status == 200 && html.body.contains("публичный пост Ани") && html.body.contains("Аня"));
    assert_eq!(w.app.handle(&w.req("DELETE", "/comments/100").session(2)).status, 204);
    assert_eq!(w.app.handle(&w.req("GET", "/drafts/11").session(1)).status, 200);
    let preview = w.app.handle(&w.req("GET", "/preview?url=http://example.com/post"));
    assert_eq!((preview.status, preview.body.contains("Заголовок")), (200, true));
    assert_eq!(w.app.handle(&w.req("GET", "/ws").header("origin", APP).cookie("session", "s")).status, 101);
    let me = w.app.handle(&w.req("GET", "/me").session(1));
    assert_eq!((me.status, me.header("x-nick")), (200, Some("Аня")));
}

#[test]
fn login_does_not_reveal_which_accounts_exist() {
    let w = world();
    let ghost = w.login(OWNER, "ghost", "x", Duration::ZERO);
    let wrong = w.login(OWNER, "anna", "x", Duration::ZERO);
    assert_eq!((ghost.status, &ghost.body), (wrong.status, &wrong.body), "ответы на несуществующее имя и неверный пароль различаются: {:?} и {:?}", ghost.body, wrong.body);
}

#[test]
fn login_does_the_same_work_for_unknown_names() {
    let w = world();
    let spent = |name: &str| {
        let before = w.hasher.verifies.load(Ordering::SeqCst);
        w.login(OWNER, name, "x", Duration::ZERO);
        w.hasher.verifies.load(Ordering::SeqCst) - before
    };
    assert_eq!(spent("ghost"), spent("anna"), "по времени (числу проверок пароля) видно, есть ли такое имя");
    assert_eq!(spent("ghost"), 1);
}

#[test]
fn guessing_passwords_gets_slower_but_does_not_lock_the_owner_out() {
    let w = world();
    for _ in 0..5 {
        w.login(ATTACKER, "anna", "guess", Duration::ZERO);
    }
    let blocked = w.login(ATTACKER, "anna", "secret", Duration::ZERO);
    assert_eq!(blocked.status, 429, "после серии неудач даже верный пароль с того же адреса должен ждать");
    assert!(blocked.header("retry-after").and_then(|v| v.parse::<u64>().ok()).is_some_and(|s| s >= 1), "429 без Retry-After");
    assert_eq!(w.login(OWNER, "anna", "secret", Duration::ZERO).status, 200, "хозяин с другого адреса входит без задержки: блокировка не должна быть оружием");
    assert_eq!(w.login(ATTACKER, "anna", "secret", Duration::from_secs(120)).status, 200, "через две минуты ограничение снимается");
}

#[test]
fn clients_cannot_set_fields_that_belong_to_the_server() {
    let w = world();
    let r = w.app.handle(&w.req("POST", "/posts").session(2).body(r#"{"text":"взлом","author":1,"likes":1000000,"pinned":true,"draft":true,"id":10}"#));
    assert_eq!(r.status, 201);
    let id: u64 = r.body.trim_start_matches("{\"id\":").trim_end_matches('}').parse().unwrap();
    let post = w.app.post(id).unwrap();
    assert_eq!((post.author, post.likes, post.pinned, post.draft), (2, 0, false, false), "клиент проставил поля, которые должен ставить только сервер: {post:?}");
    assert_eq!(w.app.post(10).unwrap().text, "публичный пост Ани", "чужой пост не должен быть перезаписан");
}

#[test]
fn request_sizes_are_limited() {
    let w = world();
    let big = format!(r#"{{"text":"{}"}}"#, "я".repeat(200_000));
    assert_eq!(w.app.handle(&w.req("POST", "/posts").session(1).body(&big)).status, 413);
    let tags = vec!["\"t\""; 1000].join(",");
    let many = w.app.handle(&w.req("POST", "/posts").session(1).body(&format!(r#"{{"text":"x","tags":[{tags}]}}"#)));
    assert!(many.status == 413 || many.status == 422, "тысяча тегов принята: {}", many.status);
    let long_tag = format!(r#"{{"text":"x","tags":["{}"]}}"#, "t".repeat(100));
    assert_eq!(w.app.handle(&w.req("POST", "/posts").session(1).body(&long_tag)).status, 422);
    assert_eq!(w.app.handle(&w.req("POST", "/posts").body("{}")).status, 401);
}

#[test]
fn user_text_is_escaped_in_html() {
    let w = world();
    w.app.add_post(20, 1, "<script>alert(document.cookie)</script> & <img src=x onerror=alert(1)>", false);
    w.app.add_user(4, "xss", "\" onmouseover=\"alert(1)", "pw");
    w.app.add_post(21, 4, "текст", false);
    let r = w.app.handle(&w.req("GET", "/posts/20/html"));
    assert!(!r.body.contains("<script") && !r.body.contains("<img"), "текст поста вставлен в HTML как есть: {}", r.body);
    assert!(r.body.contains("&lt;script&gt;") && r.body.contains("&amp;"), "угловые скобки и амперсанд должны быть экранированы: {}", r.body);
    assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
    let attr = w.app.handle(&w.req("GET", "/posts/21/html"));
    assert!(!attr.body.contains("\" onmouseover=\""), "кавычка в нике вырвалась из атрибута: {}", attr.body);
    assert!(attr.body.contains("&quot;"));
}

#[test]
fn comments_and_drafts_belong_to_their_owners() {
    let w = world();
    assert_eq!(w.app.handle(&w.req("DELETE", "/comments/100").session(3)).status, 403, "посторонний удалил чужой комментарий");
    assert!(w.app.comment_exists(100));
    assert_eq!(w.app.handle(&w.req("DELETE", "/comments/101").session(1)).status, 204, "автор поста убирает комментарии под своим постом");
    assert_eq!(w.app.handle(&w.req("DELETE", "/comments/102").session(2)).status, 204, "автор комментария");
    assert_eq!(w.app.handle(&w.req("GET", "/drafts/11").session(2)).status, 404, "чужой черновик должен выглядеть несуществующим");
    assert_eq!(w.app.handle(&w.req("GET", "/drafts/11").session(2)).body, w.app.handle(&w.req("GET", "/drafts/999").session(2)).body);
    assert_eq!(w.app.handle(&w.req("GET", "/drafts/11")).status, 401);
}

#[test]
fn preview_cannot_be_turned_against_the_inside() {
    let w = world();
    for url in ["http://127.0.0.1/", "http://localhost/", "http://169.254.169.254/latest/meta-data/", "http://[::1]/", "http://example.com@127.0.0.1/", "http://2130706433/", "ftp://example.com/", "http://10.0.0.5:6379/"] {
        let r = w.app.handle(&w.req("GET", &format!("/preview?url={url}")));
        assert_eq!(r.status, 400, "{url} не должен проходить");
    }
    assert!(w.net.connections.lock().unwrap().is_empty(), "было соединение с запрещённой целью: {:?}", w.net.connections.lock().unwrap());
    let rebinding = w.app.handle(&w.req("GET", "/preview?url=http://evil.example.org/"));
    let touched = w.net.connections.lock().unwrap().iter().any(|a| a.is_loopback());
    assert!(!touched, "DNS rebinding привёл к соединению с loopback");
    assert_ne!(rebinding.status, 200);
}

#[test]
fn websocket_checks_the_origin() {
    let w = world();
    let with = |origin: Option<&str>| {
        let mut r = w.req("GET", "/ws").cookie("session", "s");
        if let Some(o) = origin {
            r = r.header("origin", o);
        }
        w.app.handle(&r).status
    };
    assert_eq!(with(Some(APP)), 101);
    for evil in ["https://evil.com", "https://app.example.com.evil.com", "null", "http://app.example.com"] {
        assert_eq!(with(Some(evil)), 403, "{evil}");
    }
    assert_eq!(with(None), 403, "куки без Origin");
}

#[test]
fn cors_allows_only_our_frontend() {
    let w = world();
    let get = |origin: &str| w.app.handle(&w.req("GET", "/posts/10/html").header("origin", origin));
    let ok = get(APP);
    assert_eq!((ok.header("access-control-allow-origin"), ok.header("access-control-allow-credentials")), (Some(APP), Some("true")));
    assert!(ok.header("vary").is_some_and(|v| v.contains("Origin")));
    let evil = get("https://evil.com");
    assert!(evil.header("access-control-allow-origin").is_none() && evil.header("access-control-allow-credentials").is_none(), "чужому сайту разрешено читать ответ");
    let pre = |origin: &str, method: &str, headers: &str| {
        w.app.handle(&w.req("OPTIONS", "/posts").header("origin", origin).header("access-control-request-method", method).header("access-control-request-headers", headers))
    };
    let allowed = pre(APP, "POST", "content-type");
    assert_eq!(allowed.status, 204);
    let methods = allowed.header("access-control-allow-methods").unwrap();
    assert!(!methods.contains('*') && !methods.contains("PUT"));
    assert_eq!(pre("https://evil.com", "POST", "content-type").status, 403);
    assert_eq!(pre(APP, "PUT", "content-type").status, 403);
    assert_eq!(pre(APP, "GET", "x-admin").status, 403);
}

#[test]
fn refresh_is_protected_from_cross_site_requests() {
    let w = world();
    let forged = w.req("POST", "/auth/refresh").cookie("refresh", "good-token").header("origin", "https://evil.example").header("sec-fetch-site", "cross-site");
    assert_eq!(w.app.handle(&forged).status, 403, "форма с чужого сайта обновила сессию");
    assert_eq!(w.app.handle(&w.req("POST", "/auth/refresh").cookie("refresh", "good-token")).status, 403, "без нашего заголовка");
    assert_eq!(w.app.handle(&w.req("GET", "/auth/refresh").cookie("refresh", "good-token").header("x-requested-with", "fetch")).status, 404, "GET не обновляет");
    let legit = w.req("POST", "/auth/refresh").cookie("refresh", "good-token").header("x-requested-with", "fetch").header("origin", APP).header("sec-fetch-site", "same-origin");
    let ok = w.app.handle(&legit);
    assert_eq!(ok.status, 200);
    let cookie = ok.header("set-cookie").unwrap().to_ascii_lowercase();
    for needed in ["httponly", "secure", "samesite=strict", "path=/auth/refresh"] {
        assert!(cookie.contains(needed), "в куки нет {needed}: {cookie}");
    }
    let again = w.req("POST", "/auth/refresh").cookie("refresh", "good-token").header("x-requested-with", "fetch");
    assert_eq!(w.app.handle(&again).status, 401, "старый токен после обновления недействителен");
}

#[test]
fn a_nickname_cannot_split_the_response() {
    let w = world();
    let r = w.app.handle(&w.req("GET", "/me").session(3));
    assert!(r.headers.iter().all(|(n, v)| !v.contains(['\r', '\n', '\0']) && !n.eq_ignore_ascii_case("set-cookie")), "ник внедрил заголовок: {:?}", r.headers);
}

#[test]
fn the_log_cannot_be_forged() {
    let w = world();
    w.app.handle(&w.req("GET", "/me\nlevel=INFO user=\"admin\" ip=\"127.0.0.1\" msg=\"пароль изменён\"").session(3));
    let lines = w.app.log_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines.iter().all(|l| !l.contains(['\n', '\r'])), "в журнал попали переводы строк: {lines:?}");
    assert_eq!(lines[0].matches("user=\"admin\"").count(), 0, "подделанная запись выглядит как настоящее поле: {}", lines[0]);
}
