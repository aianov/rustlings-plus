use crate::net::{Hasher, Net};
use crate::ssrf::{Resolver, check_url};
use crate::throttle::Throttle;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct Req {
    pub method: String,
    /// Путь вместе с запросом: `/preview?url=http://example.com/`.
    pub path: String,
    /// Имена заголовков в нижнем регистре.
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub body: String,
    pub ip: IpAddr,
    /// Пользователь, которого шлюз уже опознал по токену.
    pub session: Option<u64>,
    pub now: Instant,
}

impl Req {
    pub fn new(method: &str, path: &str, ip: IpAddr, now: Instant) -> Req {
        Req { method: method.into(), path: path.into(), headers: HashMap::new(), cookies: HashMap::new(), body: String::new(), ip, session: None, now }
    }

    pub fn header(mut self, name: &str, value: &str) -> Req {
        self.headers.insert(name.to_ascii_lowercase(), value.to_string());
        self
    }

    pub fn cookie(mut self, name: &str, value: &str) -> Req {
        self.cookies.insert(name.to_string(), value.to_string());
        self
    }

    pub fn body(mut self, body: &str) -> Req {
        self.body = body.to_string();
        self
    }

    pub fn session(mut self, user: u64) -> Req {
        self.session = Some(user);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Resp {
    fn new(status: u16, body: &str) -> Resp {
        Resp { status, headers: Vec::new(), body: body.to_string() }
    }

    fn with(mut self, name: &str, value: &str) -> Resp {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub text: String,
    pub tags: Vec<String>,
    pub likes: u64,
    pub pinned: bool,
    pub draft: bool,
}

struct User {
    id: u64,
    nick: String,
    hash: String,
}

#[derive(Default)]
struct State {
    users: HashMap<String, User>,
    posts: HashMap<u64, Post>,
    comments: HashMap<u64, (u64, u64)>,
    refresh: HashSet<String>,
    next_id: u64,
}

pub struct App {
    hasher: Arc<dyn Hasher>,
    net: Arc<dyn Net>,
    allowed_origins: Vec<String>,
    state: Mutex<State>,
    throttle: Mutex<Throttle>,
    log: Mutex<Vec<String>>,
    dummy: String,
}

struct NetResolver<'a>(&'a dyn Net);

impl Resolver for NetResolver<'_> {
    fn resolve(&self, host: &str) -> Vec<IpAddr> {
        self.0.resolve(host)
    }
}

const APP_ORIGIN: &str = "https://app.example.com";
const MAX_BODY: usize = 64 * 1024;
const MAX_PREVIEW_BODY: usize = 1024 * 1024;

fn form_value<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    body.split('&').find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| v))
}

fn query_value<'a>(path: &'a str, name: &str) -> Option<&'a str> {
    let (_, query) = path.split_once('?')?;
    form_value(query, name)
}

fn path_id(path: &str, prefix: &str, suffix: &str) -> Option<u64> {
    path.strip_prefix(prefix)?.strip_suffix(suffix)?.parse().ok()
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

fn escape_log(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl App {
    pub fn new(hasher: Arc<dyn Hasher>, net: Arc<dyn Net>, allowed_origins: &[&str]) -> App {
        let dummy = hasher.hash("пароль, которого нет ни у кого: 8d41c0e57b");
        App {
            hasher,
            net,
            allowed_origins: allowed_origins.iter().map(|s| s.to_string()).collect(),
            state: Mutex::new(State { next_id: 1000, ..State::default() }),
            throttle: Mutex::new(Throttle::default()),
            log: Mutex::new(Vec::new()),
            dummy,
        }
    }

    pub fn add_user(&self, id: u64, name: &str, nick: &str, password: &str) {
        let hash = self.hasher.hash(password);
        self.state.lock().unwrap().users.insert(name.to_string(), User { id, nick: nick.to_string(), hash });
    }

    pub fn add_post(&self, id: u64, author: u64, text: &str, draft: bool) {
        let post = Post { id, author, text: text.to_string(), tags: Vec::new(), likes: 0, pinned: false, draft };
        self.state.lock().unwrap().posts.insert(id, post);
    }

    pub fn add_comment(&self, id: u64, post: u64, author: u64) {
        self.state.lock().unwrap().comments.insert(id, (post, author));
    }

    pub fn add_refresh_token(&self, token: &str) {
        self.state.lock().unwrap().refresh.insert(token.to_string());
    }

    pub fn post(&self, id: u64) -> Option<Post> {
        self.state.lock().unwrap().posts.get(&id).cloned()
    }

    pub fn comment_exists(&self, id: u64) -> bool {
        self.state.lock().unwrap().comments.contains_key(&id)
    }

    pub fn log_lines(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn handle(&self, req: &Req) -> Resp {
        self.log_request(req);
        let mut resp = if req.method == "OPTIONS" { self.preflight(req) } else { self.route(req) };
        self.add_cors(req, &mut resp);
        resp
    }

    fn nick_of(&self, user: Option<u64>) -> String {
        let state = self.state.lock().unwrap();
        user.and_then(|id| state.users.values().find(|u| u.id == id)).map_or("-".to_string(), |u| u.nick.clone())
    }

    fn log_request(&self, req: &Req) {
        let nick = self.nick_of(req.session);
        let line = format!("level=INFO user=\"{}\" ip=\"{}\" msg=\"{} {}\"", escape_log(&nick), req.ip, escape_log(&req.method), escape_log(&req.path));
        self.log.lock().unwrap().push(line);
    }

    fn route(&self, req: &Req) -> Resp {
        let path = req.path.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("POST", "/login") => self.login(req),
            ("POST", "/posts") => self.create_post(req),
            ("GET", "/preview") => self.preview(req),
            ("GET", "/ws") => self.ws(req),
            ("POST", "/auth/refresh") => self.refresh(req),
            ("GET", "/me") => self.me(req),
            ("GET", p) if path_id(p, "/posts/", "/html").is_some() => self.post_html(path_id(p, "/posts/", "/html").unwrap_or(0)),
            ("DELETE", p) if path_id(p, "/comments/", "").is_some() => self.delete_comment(req, path_id(p, "/comments/", "").unwrap_or(0)),
            ("GET", p) if path_id(p, "/drafts/", "").is_some() => self.get_draft(req, path_id(p, "/drafts/", "").unwrap_or(0)),
            _ => Resp::new(404, "не найдено"),
        }
    }

    fn login(&self, req: &Req) -> Resp {
        let name = form_value(&req.body, "name").unwrap_or("");
        let password = form_value(&req.body, "password").unwrap_or("");
        if let Err(wait) = self.throttle.lock().unwrap().check(name, req.ip, req.now) {
            let secs = wait.as_secs_f64().ceil().max(1.0) as u64;
            return Resp::new(429, "Слишком много попыток входа").with("Retry-After", &secs.to_string());
        }
        let found = {
            let state = self.state.lock().unwrap();
            state.users.get(name).map(|u| (u.id, u.hash.clone()))
        };
        let hash = found.as_ref().map_or(self.dummy.as_str(), |(_, h)| h.as_str());
        let verified = self.hasher.verify(hash, password);
        match found {
            Some((id, _)) if verified => {
                self.throttle.lock().unwrap().record_success(name, req.ip);
                Resp::new(200, &format!("{{\"user\":{id}}}"))
            }
            _ => {
                self.throttle.lock().unwrap().record_failure(name, req.ip, req.now);
                Resp::new(401, "Неверное имя или пароль")
            }
        }
    }

    fn create_post(&self, req: &Req) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        #[derive(Deserialize)]
        struct NewPost {
            text: String,
            #[serde(default)]
            tags: Vec<String>,
        }
        if req.body.len() > MAX_BODY {
            return Resp::new(413, "слишком большое тело");
        }
        let Ok(new) = serde_json::from_str::<NewPost>(&req.body) else { return Resp::new(400, "плохой JSON") };
        if new.tags.len() > 20 || new.tags.iter().any(|t| t.chars().count() > 32) {
            return Resp::new(422, "слишком много или слишком длинные теги");
        }
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        let id = state.next_id;
        state.posts.insert(id, Post { id, author: user, text: new.text, tags: new.tags, likes: 0, pinned: false, draft: false });
        Resp::new(201, &format!("{{\"id\":{id}}}"))
    }

    fn post_html(&self, id: u64) -> Resp {
        let state = self.state.lock().unwrap();
        let Some(post) = state.posts.get(&id).filter(|p| !p.draft) else { return Resp::new(404, "не найдено") };
        let nick = state.users.values().find(|u| u.id == post.author).map_or("?", |u| u.nick.as_str());
        let html = format!("<article><h2 title=\"{}\">{}</h2><p>{}</p></article>", escape_html(nick), escape_html(nick), escape_html(&post.text));
        Resp::new(200, &html).with("Content-Type", "text/html; charset=utf-8").with("X-Content-Type-Options", "nosniff")
    }

    fn delete_comment(&self, req: &Req, id: u64) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let mut state = self.state.lock().unwrap();
        let Some(&(post, author)) = state.comments.get(&id) else { return Resp::new(404, "не найдено") };
        let post_author = state.posts.get(&post).map(|p| p.author);
        if author != user && post_author != Some(user) {
            return Resp::new(403, "нельзя");
        }
        state.comments.remove(&id);
        Resp::new(204, "")
    }

    fn get_draft(&self, req: &Req, id: u64) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let state = self.state.lock().unwrap();
        match state.posts.get(&id) {
            Some(p) if p.draft && p.author == user => Resp::new(200, &p.text),
            _ => Resp::new(404, "не найдено"),
        }
    }

    fn preview(&self, req: &Req) -> Resp {
        let Some(url) = query_value(&req.path, "url") else { return Resp::new(400, "нужен параметр url") };
        let Ok(target) = check_url(url, &NetResolver(self.net.as_ref())) else { return Resp::new(400, "этот адрес нельзя") };
        let page = self.net.get(target.addr, &target.host, target.port, target.https, &target.path);
        if page.status != 200 || page.body_len > MAX_PREVIEW_BODY {
            return Resp::new(502, "страница недоступна");
        }
        Resp::new(200, &format!("{{\"title\":{:?}}}", page.title))
    }

    fn ws(&self, req: &Req) -> Resp {
        let cookie_auth = req.cookies.contains_key("session");
        match req.headers.get("origin") {
            None if cookie_auth => Resp::new(403, "нужен Origin"),
            None => Resp::new(101, ""),
            Some(origin) if self.origin_allowed(origin) => Resp::new(101, ""),
            Some(_) => Resp::new(403, "чужой Origin"),
        }
    }

    fn origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.iter().any(|a| a == origin)
    }

    fn preflight(&self, req: &Req) -> Resp {
        let allowed_origin = req.headers.get("origin").is_some_and(|o| self.origin_allowed(o));
        let method_ok = req.headers.get("access-control-request-method").is_some_and(|m| ["GET", "POST", "DELETE"].contains(&m.as_str()));
        let asked = req.headers.get("access-control-request-headers").cloned().unwrap_or_default();
        let headers_ok = asked.split(',').map(|h| h.trim().to_ascii_lowercase()).filter(|h| !h.is_empty()).all(|h| ["content-type", "authorization", "x-requested-with"].contains(&h.as_str()));
        if !(allowed_origin && method_ok && headers_ok) {
            return Resp::new(403, "CORS запрещён");
        }
        Resp::new(204, "")
            .with("Access-Control-Allow-Methods", "GET, POST, DELETE")
            .with("Access-Control-Allow-Headers", asked.trim())
            .with("Access-Control-Max-Age", "600")
    }

    fn add_cors(&self, req: &Req, resp: &mut Resp) {
        resp.headers.push(("Vary".into(), "Origin".into()));
        if let Some(origin) = req.headers.get("origin").filter(|o| self.origin_allowed(o)) {
            resp.headers.push(("Access-Control-Allow-Origin".into(), origin.clone()));
            resp.headers.push(("Access-Control-Allow-Credentials".into(), "true".into()));
        }
    }

    fn refresh(&self, req: &Req) -> Resp {
        if req.method != "POST" {
            return Resp::new(405, "только POST");
        }
        let mut state = self.state.lock().unwrap();
        let Some(token) = req.cookies.get("refresh").filter(|t| state.refresh.contains(*t)).cloned() else { return Resp::new(401, "нет сессии") };
        let same_site = req.headers.get("sec-fetch-site").is_none_or(|s| s == "same-origin" || s == "same-site");
        let own_origin = req.headers.get("origin").is_none_or(|o| o == APP_ORIGIN);
        if req.headers.get("x-requested-with").map(String::as_str) != Some("fetch") || !same_site || !own_origin {
            return Resp::new(403, "подозрение на CSRF");
        }
        state.refresh.remove(&token);
        state.next_id += 1;
        let fresh = format!("rt-{}", state.next_id);
        state.refresh.insert(fresh.clone());
        Resp::new(200, "{}").with("Set-Cookie", &format!("refresh={fresh}; Max-Age=2592000; Path=/auth/refresh; HttpOnly; Secure; SameSite=Strict"))
    }

    fn me(&self, req: &Req) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let nick = self.nick_of(Some(user));
        let resp = Resp::new(200, &format!("{{\"id\":{user}}}"));
        if nick.chars().all(|c| c == '\t' || !c.is_control()) { resp.with("X-Nick", &nick) } else { resp }
    }
}
