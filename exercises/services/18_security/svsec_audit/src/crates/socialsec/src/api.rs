use crate::net::{Hasher, Net};
use crate::ssrf::Resolver;
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
        let line = format!("level=INFO user=\"{nick}\" ip=\"{}\" msg=\"{} {}\"", req.ip, req.method, req.path);
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
        let state = self.state.lock().unwrap();
        match state.users.get(name) {
            None => Resp::new(401, "Нет такого пользователя"),
            Some(user) if self.hasher.verify(&user.hash, password) => Resp::new(200, &format!("{{\"user\":{}}}", user.id)),
            Some(_) => Resp::new(401, "Неверный пароль"),
        }
    }

    fn create_post(&self, req: &Req) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&req.body) else { return Resp::new(400, "плохой JSON") };
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        let id = state.next_id;
        let post = Post {
            id,
            author: v["author"].as_u64().unwrap_or(user),
            text: v["text"].as_str().unwrap_or("").to_string(),
            tags: v["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default(),
            likes: v["likes"].as_u64().unwrap_or(0),
            pinned: v["pinned"].as_bool().unwrap_or(false),
            draft: false,
        };
        state.posts.insert(id, post);
        Resp::new(201, &format!("{{\"id\":{id}}}"))
    }

    fn post_html(&self, id: u64) -> Resp {
        let state = self.state.lock().unwrap();
        let Some(post) = state.posts.get(&id).filter(|p| !p.draft) else { return Resp::new(404, "не найдено") };
        let nick = state.users.values().find(|u| u.id == post.author).map_or("?", |u| u.nick.as_str());
        let html = format!("<article><h2 title=\"{nick}\">{nick}</h2><p>{}</p></article>", post.text);
        Resp::new(200, &html).with("Content-Type", "text/html; charset=utf-8")
    }

    fn delete_comment(&self, req: &Req, id: u64) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let mut state = self.state.lock().unwrap();
        if state.comments.remove(&id).is_some() { Resp::new(204, "") } else { Resp::new(404, "не найдено") }
    }

    fn get_draft(&self, req: &Req, id: u64) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let state = self.state.lock().unwrap();
        match state.posts.get(&id).filter(|p| p.draft) {
            Some(p) => Resp::new(200, &p.text),
            None => Resp::new(404, "не найдено"),
        }
    }

    fn preview(&self, req: &Req) -> Resp {
        let Some(url) = query_value(&req.path, "url") else { return Resp::new(400, "нужен параметр url") };
        let rest = url.split("://").nth(1).unwrap_or("");
        let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
        let host = authority.split(':').next().unwrap_or("");
        let page = self.net.get_by_name(host, 80, url.starts_with("https"), if path.is_empty() { "/" } else { path });
        let _ = (MAX_PREVIEW_BODY, NetResolver(self.net.as_ref()));
        Resp::new(200, &format!("{{\"title\":{:?}}}", page.title))
    }

    fn ws(&self, req: &Req) -> Resp {
        Resp::new(101, "")
    }

    fn preflight(&self, req: &Req) -> Resp {
        Resp::new(204, "").with("Access-Control-Allow-Methods", "*").with("Access-Control-Allow-Headers", "*")
    }

    fn add_cors(&self, req: &Req, resp: &mut Resp) {
        if let Some(origin) = req.headers.get("origin") {
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
        state.refresh.remove(&token);
        state.next_id += 1;
        let fresh = format!("rt-{}", state.next_id);
        state.refresh.insert(fresh.clone());
        let _ = APP_ORIGIN;
        Resp::new(200, "{}").with("Set-Cookie", &format!("refresh={fresh}; Max-Age=2592000"))
    }

    fn me(&self, req: &Req) -> Resp {
        let Some(user) = req.session else { return Resp::new(401, "нужен вход") };
        let nick = self.nick_of(Some(user));
        Resp::new(200, &format!("{{\"id\":{user}}}")).with("X-Nick", &nick)
    }
}
