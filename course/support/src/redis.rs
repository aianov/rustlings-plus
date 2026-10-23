//! Помощники для тестов с Redis (фича `redis`).
//!
//! Redis курса один на все тесты, и они идут параллельно. Каждый тест берёт свой `Scope`: уникальный
//! префикс ключей и маленький собственный клиент на голом tokio (без крейта `redis`). Когда `Scope`
//! уничтожается (в том числе после паники в `assert!`), все ключи с этим префиксом удаляются.
//!
//! API:
//!
//! - `url()`, `DEFAULT_URL`: адрес из `RUSTLINGS_REDIS_URL` или значение по умолчанию.
//!   `addr()` даёт `host:port` из этого адреса, чтобы подключать к нему клиент ученика через
//!   `TcpStream::connect`.
//! - `Scope::new().await`: уникальный префикс теста вида `t_<pid>_<время>_<n>` и подключённый клиент,
//!   паника с понятным текстом, если сервер не отвечает. `scope.key("views")` даёт ключ
//!   `<prefix>:views`, `scope.prefix()` сам префикс (без двоеточия), `scope.client()` копию клиента,
//!   `scope.cmd(&["GET", &key]).await` сокращение для команды, `scope.addr()` и `scope.url()` адрес сервера.
//!   Любой ключ вне префикса тест создаёт на свой риск: он не удалится.
//! - `Client`: клиент курса с тем же API, что получится у тебя в `resp-client`. `Client::connect().await`
//!   (паника, если сервера нет), `Client::connect_to(addr).await -> Result<Client, Error>` для адреса
//!   вида `host:port` или `redis://host:port`. Клиент можно клонировать: копии делят одно соединение,
//!   команды разных задач идут по очереди, по одной (между командами одной задачи могут вклиниться
//!   чужие, как и у настоящего общего соединения). `client.cmd(&["SET", "k", "v"]).await ->
//!   Result<Value, Error>`: ответ-ошибка сервера приходит как `Err(Error::Server(текст))`.
//!   `client.pipeline(&[&["INCR", "a"], &["GET", "a"]]).await -> Result<Vec<Value>, Error>` пишет
//!   все команды разом; ошибки сервера внутри пачки лежат в ответах как `Value::Error`.
//!   `client.round_trips()` считает обмены «записали и прочитали» у этого клиента и всех его копий
//!   (`cmd` это один обмен, `pipeline` тоже один): так тест проверяет, что `MGET` заменил десять `GET`.
//!   Блокирующие команды (`BLPOP`, `XREAD BLOCK`) занимают соединение целиком: для них бери отдельный
//!   `Client::connect_to(&scope.addr())`.
//! - `Value`: ответ RESP2: `Simple`, `Error`, `Int`, `Bulk(Vec<u8>)`, `Nil`, `Array`. Удобные методы:
//!   `text() -> Option<String>` (простая строка или bulk как текст), `int() -> Option<i64>`
//!   (число или bulk, в котором число), `is_nil()`, `list() -> Vec<Value>` (элементы массива, для
//!   остальных пусто), `texts() -> Vec<String>` (тексты элементов массива, nil пропускаются).
//! - `Error`: `Server(String)`, `Io(io::Error)`, `Protocol(String)`.
//! - `FakeServer::start(|conn, args| vec![Step::...]).await`: свой маленький RESP-сервер на
//!   `127.0.0.1:0` для сбоев, которых у настоящего Redis не добиться. Обработчик получает номер
//!   соединения (с нуля) и команду как список строк, а возвращает шаги: `Step::Write(байты)`,
//!   `Step::Sleep(срок)`, `Step::Close`. Пустой список значит «промолчать». Команды из одной пачки
//!   (pipeline) обрабатываются по очереди. `server.addr()`, `server.url()`, `server.connections()` (сколько
//!   соединений принято), `server.open_connections()` (сколько ещё открыто), `server.commands()` (все полученные команды по порядку). Сервер останавливается
//!   при `drop`.

use std::fmt;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};

pub const DEFAULT_URL: &str = "redis://127.0.0.1:56379";

pub fn url() -> String {
    crate::env_or("RUSTLINGS_REDIS_URL", DEFAULT_URL)
}

pub fn addr() -> String {
    strip_scheme(&url())
}

fn strip_scheme(u: &str) -> String {
    let rest = u.strip_prefix("redis://").unwrap_or(u);
    rest.split('/').next().unwrap_or(rest).to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Vec<u8>),
    Nil,
    Array(Vec<Value>),
}

impl Value {
    pub fn text(&self) -> Option<String> {
        match self {
            Value::Simple(s) => Some(s.clone()),
            Value::Bulk(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Bulk(b) => String::from_utf8_lossy(b).parse().ok(),
            _ => None,
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    pub fn list(&self) -> Vec<Value> {
        match self {
            Value::Array(v) => v.clone(),
            _ => Vec::new(),
        }
    }

    pub fn texts(&self) -> Vec<String> {
        self.list().iter().filter_map(Value::text).collect()
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Simple(s) => write!(f, "+{s}"),
            Value::Error(s) => write!(f, "-{s}"),
            Value::Int(n) => write!(f, ":{n}"),
            Value::Bulk(b) => write!(f, "{:?}", String::from_utf8_lossy(b)),
            Value::Nil => write!(f, "nil"),
            Value::Array(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Server(String),
    Io(io::Error),
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Server(s) => write!(f, "ошибка сервера: {s}"),
            Error::Io(e) => write!(f, "ввод-вывод: {e}"),
            Error::Protocol(s) => write!(f, "протокол: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

fn encode(args: &[&str]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for a in args {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn line(buf: &[u8], from: usize) -> Option<(&[u8], usize)> {
    let rel = buf.get(from..)?.windows(2).position(|w| w == b"\r\n")?;
    Some((&buf[from..from + rel], from + rel + 2))
}

fn parse_at(buf: &[u8], pos: usize) -> Result<Option<(Value, usize)>, String> {
    if pos >= buf.len() {
        return Ok(None);
    }
    let Some((l, next)) = line(buf, pos + 1) else {
        return Ok(None);
    };
    let s = String::from_utf8_lossy(l).into_owned();
    let num = |s: &str| s.parse::<i64>().map_err(|_| format!("не число: {s:?}"));
    match buf[pos] {
        b'+' => Ok(Some((Value::Simple(s), next))),
        b'-' => Ok(Some((Value::Error(s), next))),
        b':' => Ok(Some((Value::Int(num(&s)?), next))),
        b'$' => {
            let n = num(&s)?;
            if n < 0 {
                return Ok(Some((Value::Nil, next)));
            }
            let n = n as usize;
            if buf.len() < next + n + 2 {
                return Ok(None);
            }
            Ok(Some((Value::Bulk(buf[next..next + n].to_vec()), next + n + 2)))
        }
        b'*' => {
            let n = num(&s)?;
            if n < 0 {
                return Ok(Some((Value::Nil, next)));
            }
            let mut items = Vec::new();
            let mut p = next;
            for _ in 0..n {
                match parse_at(buf, p)? {
                    None => return Ok(None),
                    Some((v, np)) => {
                        items.push(v);
                        p = np;
                    }
                }
            }
            Ok(Some((Value::Array(items), p)))
        }
        other => Err(format!("неизвестный тип ответа {:?}", other as char)),
    }
}

struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Conn {
    async fn read_value(&mut self) -> Result<Value, Error> {
        loop {
            match parse_at(&self.buf, 0).map_err(Error::Protocol)? {
                Some((v, used)) => {
                    self.buf.drain(..used);
                    return Ok(v);
                }
                None => {
                    let mut chunk = [0u8; 4096];
                    let n = self.stream.read(&mut chunk).await?;
                    if n == 0 {
                        return Err(Error::Io(io::Error::new(io::ErrorKind::UnexpectedEof, "сервер закрыл соединение")));
                    }
                    self.buf.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Client {
    conn: Arc<Mutex<Conn>>,
    trips: Arc<AtomicUsize>,
}

impl Client {
    pub async fn connect() -> Client {
        let a = addr();
        match Client::connect_to(&a).await {
            Ok(c) => c,
            Err(e) => panic!("Redis курса не отвечает на {a} ({e}). Запусти `docker compose up -d` в папке курса"),
        }
    }

    pub async fn connect_to(address: &str) -> Result<Client, Error> {
        let stream = TcpStream::connect(strip_scheme(address)).await?;
        stream.set_nodelay(true)?;
        Ok(Client { conn: Arc::new(Mutex::new(Conn { stream, buf: Vec::new() })), trips: Arc::new(AtomicUsize::new(0)) })
    }

    pub fn round_trips(&self) -> usize {
        self.trips.load(Ordering::SeqCst)
    }

    pub async fn cmd(&self, args: &[&str]) -> Result<Value, Error> {
        match self.pipeline(&[args]).await?.pop() {
            Some(Value::Error(e)) => Err(Error::Server(e)),
            Some(v) => Ok(v),
            None => Err(Error::Protocol("пустой ответ".into())),
        }
    }

    pub async fn pipeline(&self, cmds: &[&[&str]]) -> Result<Vec<Value>, Error> {
        let mut bytes = Vec::new();
        for c in cmds {
            bytes.extend(encode(c));
        }
        let mut conn = self.conn.lock().await;
        self.trips.fetch_add(1, Ordering::SeqCst);
        conn.stream.write_all(&bytes).await?;
        let mut out = Vec::with_capacity(cmds.len());
        for _ in cmds {
            out.push(conn.read_value().await?);
        }
        Ok(out)
    }
}

pub struct Scope {
    prefix: String,
    client: Client,
}

impl Scope {
    pub async fn new() -> Scope {
        Scope { prefix: crate::unique_name("t"), client: Client::connect().await }
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn key(&self, name: &str) -> String {
        format!("{}:{name}", self.prefix)
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    pub async fn cmd(&self, args: &[&str]) -> Result<Value, Error> {
        self.client.cmd(args).await
    }

    pub fn addr(&self) -> String {
        addr()
    }

    pub fn url(&self) -> String {
        url()
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let pattern = format!("{}:*", self.prefix);
        let target = addr();
        let _ = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                return;
            };
            rt.block_on(async {
                let Ok(c) = Client::connect_to(&target).await else {
                    return;
                };
                let mut cursor = "0".to_string();
                loop {
                    let Ok(r) = c.cmd(&["SCAN", &cursor, "MATCH", &pattern, "COUNT", "500"]).await else {
                        return;
                    };
                    let parts = r.list();
                    let keys = parts.get(1).map(Value::texts).unwrap_or_default();
                    if !keys.is_empty() {
                        let mut args = vec!["UNLINK"];
                        args.extend(keys.iter().map(String::as_str));
                        let _ = c.cmd(&args).await;
                    }
                    cursor = parts.first().and_then(Value::text).unwrap_or_else(|| "0".into());
                    if cursor == "0" {
                        return;
                    }
                }
            })
        })
        .join();
    }
}

pub enum Step {
    Write(Vec<u8>),
    Sleep(Duration),
    Close,
}

struct FakeState {
    connections: AtomicUsize,
    open: AtomicUsize,
    log: StdMutex<Vec<Vec<String>>>,
}

pub struct FakeServer {
    addr: String,
    state: Arc<FakeState>,
    task: JoinHandle<()>,
}

impl FakeServer {
    pub async fn start<F>(handler: F) -> FakeServer
    where
        F: Fn(usize, Vec<String>) -> Vec<Step> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("свободный порт для FakeServer");
        let addr = listener.local_addr().expect("адрес FakeServer").to_string();
        let state = Arc::new(FakeState { connections: AtomicUsize::new(0), open: AtomicUsize::new(0), log: StdMutex::new(Vec::new()) });
        let handler = Arc::new(handler);
        let st = state.clone();
        let task = tokio::spawn(async move {
            let mut set = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                let index = st.connections.fetch_add(1, Ordering::SeqCst);
                st.open.fetch_add(1, Ordering::SeqCst);
                set.spawn(serve(stream, index, handler.clone(), st.clone()));
            }
        });
        FakeServer { addr, state, task }
    }

    pub fn addr(&self) -> String {
        self.addr.clone()
    }

    pub fn url(&self) -> String {
        format!("redis://{}", self.addr)
    }

    pub fn connections(&self) -> usize {
        self.state.connections.load(Ordering::SeqCst)
    }

    pub fn open_connections(&self) -> usize {
        self.state.open.load(Ordering::SeqCst)
    }

    pub fn commands(&self) -> Vec<Vec<String>> {
        self.state.log.lock().expect("журнал команд").clone()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve<F>(stream: TcpStream, index: usize, handler: Arc<F>, state: Arc<FakeState>)
where
    F: Fn(usize, Vec<String>) -> Vec<Step> + Send + Sync + 'static,
{
    serve_loop(stream, index, handler, &state).await;
    state.open.fetch_sub(1, Ordering::SeqCst);
}

async fn serve_loop<F>(mut stream: TcpStream, index: usize, handler: Arc<F>, state: &FakeState)
where
    F: Fn(usize, Vec<String>) -> Vec<Step> + Send + Sync + 'static,
{
    let mut buf: Vec<u8> = Vec::new();
    loop {
        while let Ok(Some((v, used))) = parse_at(&buf, 0) {
            buf.drain(..used);
            let args: Vec<String> = match v {
                Value::Array(items) => items.iter().filter_map(Value::text).collect(),
                _ => Vec::new(),
            };
            state.log.lock().expect("журнал команд").push(args.clone());
            for step in handler(index, args) {
                match step {
                    Step::Write(b) => {
                        if stream.write_all(&b).await.is_err() {
                            return;
                        }
                    }
                    Step::Sleep(d) => tokio::time::sleep(d).await,
                    Step::Close => return,
                }
            }
        }
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}
