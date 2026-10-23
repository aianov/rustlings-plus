use std::collections::VecDeque;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use api_gateway::{Config, run};
use contracts::{Changed, ErrorBody, Login, Outcome, Push, Response, UserView, decode};
use service_runtime::CancellationToken;
use social_cli::Ws;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub const DEADLINE: Duration = Duration::from_secs(10);

pub async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what}: не уложилось в {DEADLINE:?}, похоже на зависание"))
}

pub fn id_of(raw: &str) -> Option<u64> {
    let rest = raw.strip_prefix(r#"{"id":"#)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub enum Line {
    Push(Push),
    Reply(String),
}

macro_rules! ok {
    ($peer:expr, $method:expr, $data:expr => $ty:ty) => {{
        let method = $method;
        let data = $data;
        let raw = $peer.call(method, &data).await;
        match contracts::decode::<contracts::Response<$ty>>(&raw) {
            Ok(contracts::Response {
                outcome: contracts::Outcome::Ok(value),
                ..
            }) => value,
            Ok(contracts::Response {
                outcome: contracts::Outcome::Error(body),
                ..
            }) => panic!("{method} {data}: ждали успех, а пришло {body:?}"),
            Err(error) => panic!("ответ на {method} не разобрался ({error}): {raw}"),
        }
    }};
}

pub(crate) use ok;

pub struct Gateway {
    pub http: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<api_gateway::Result<()>>,
}

impl Gateway {
    pub async fn start(config: Config) -> Gateway {
        let http = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("порт 0 на 127.0.0.1 свободен всегда");
        let addr = http.local_addr().expect("у слушателя есть адрес");
        let stop = CancellationToken::new();
        let task = tokio::spawn(run(http, config, stop.clone()));
        Gateway {
            http: addr,
            stop,
            task,
        }
    }

    pub async fn stop(self) {
        self.stop.cancel();
        let finished = within("остановка шлюза", self.task)
            .await
            .expect("задача шлюза не падает");
        if let Err(error) = finished {
            panic!("run завершился ошибкой: {error:?}");
        }
    }

    pub async fn http(&self, method: &str, path: &str, body: Option<&str>) -> Http {
        http(self.http, method, path, body).await
    }

    pub async fn register(&self, name: &str) -> UserView {
        let body = format!(r#"{{"username": "{name}"}}"#);
        let answer = self.http("POST", "/auth/register", Some(&body)).await;
        assert_eq!(answer.status, 201, "регистрация {name}: {answer:?}");
        decode(&answer.body)
            .unwrap_or_else(|e| panic!("ответ регистрации разбирается ({e}): {answer:?}"))
    }

    pub async fn login(&self, name: &str) -> Login {
        let body = format!(r#"{{"username": "{name}"}}"#);
        let answer = self.http("POST", "/auth/login", Some(&body)).await;
        assert_eq!(answer.status, 200, "вход {name}: {answer:?}");
        decode(&answer.body).unwrap_or_else(|e| panic!("ответ входа разбирается ({e}): {answer:?}"))
    }

    pub async fn peer(&self) -> Peer {
        Peer::connect(self.http, "/ws").await
    }

    pub async fn peer_with(&self, token: &str) -> Peer {
        Peer::connect(self.http, &format!("/ws?token={token}")).await
    }

    pub async fn member(&self, name: &str) -> (UserView, Peer) {
        let user = self.register(name).await;
        let login = self.login(name).await;
        let mut peer = self.peer().await;
        peer.hello(&login.token).await;
        (user, peer)
    }
}

#[derive(Debug)]
pub struct Http {
    pub status: u16,
    pub head: String,
    pub body: String,
}

impl Http {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.split("\r\n").skip(1).find_map(|line| {
            let (own, value) = line.split_once(':')?;
            own.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

pub async fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> Http {
    let mut stream = within("подключение к HTTP", TcpStream::connect(addr))
        .await
        .expect("HTTP-порт шлюза принимает соединения");
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    if let Some(body) = body {
        raw.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
    } else {
        raw.push_str("\r\n");
    }
    stream
        .write_all(raw.as_bytes())
        .await
        .expect("запрос уходит");
    let mut answer = Vec::new();
    within("ответ HTTP", stream.read_to_end(&mut answer))
        .await
        .expect("ответ читается");
    let text = String::from_utf8(answer).expect("ответ в UTF-8");
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("кривая строка статуса: {head:?}"));
    Http {
        status,
        head: head.to_string(),
        body: body.to_string(),
    }
}

pub struct Peer {
    ws: Ws,
    next: u64,
    pushes: VecDeque<Push>,
}

impl Peer {
    pub async fn connect(addr: SocketAddr, path: &str) -> Peer {
        let ws = within(
            "подключение WebSocket",
            Ws::connect(&addr.to_string(), path),
        )
        .await
        .unwrap_or_else(|e| panic!("шлюз открывает WebSocket на {path}: {e}"));
        Peer {
            ws,
            next: 0,
            pushes: VecDeque::new(),
        }
    }

    pub async fn raw(&mut self, line: &str) {
        self.ws
            .send_text(line)
            .await
            .expect("текстовый фрейм уходит в шлюз");
    }

    pub async fn binary(&mut self, bytes: &[u8]) {
        self.ws
            .send_binary(bytes)
            .await
            .expect("двоичный фрейм уходит в шлюз");
    }

    pub fn close_code(&self) -> Option<u16> {
        self.ws.close_code()
    }

    pub async fn line(&mut self) -> Option<Line> {
        let raw = within("сообщение от шлюза", self.ws.recv())
            .await
            .ok()
            .flatten()?;
        match decode::<Push>(&raw) {
            Ok(push) => Some(Line::Push(push)),
            Err(_) => Some(Line::Reply(raw)),
        }
    }

    pub async fn send(&mut self, method: &str, data: &str) -> u64 {
        self.next += 1;
        let service = method.split('.').next().unwrap_or(method);
        let line = format!(
            r#"{{"id": {}, "service": "{service}", "method": "{method}", "data": {data}}}"#,
            self.next
        );
        self.raw(&line).await;
        self.next
    }

    pub async fn reply(&mut self, id: u64) -> String {
        loop {
            match self.line().await {
                Some(Line::Push(push)) => self.pushes.push_back(push),
                Some(Line::Reply(raw)) if id_of(&raw) == Some(id) => return raw,
                Some(Line::Reply(raw)) => panic!("ждали ответ на запрос {id}, а пришло {raw}"),
                None => panic!("шлюз закрыл соединение, не ответив на запрос {id}"),
            }
        }
    }

    pub async fn call(&mut self, method: &str, data: &str) -> String {
        let id = self.send(method, data).await;
        self.reply(id).await
    }

    pub async fn fails(&mut self, method: &str, data: &str) -> ErrorBody {
        let raw = self.call(method, data).await;
        match decode::<Response<Changed>>(&raw) {
            Ok(Response {
                outcome: Outcome::Error(body),
                ..
            }) => body,
            _ => panic!("{method} {data}: ждали ошибку, а пришло {raw}"),
        }
    }

    pub async fn hello(&mut self, token: &str) -> UserView {
        ok!(self, "session.hello", format!(r#"{{"token": "{token}"}}"#) => UserView)
    }

    pub async fn push(&mut self) -> Push {
        if let Some(push) = self.pushes.pop_front() {
            return push;
        }
        match self.line().await {
            Some(Line::Push(push)) => push,
            Some(Line::Reply(raw)) => panic!("ждали push, а пришёл ответ {raw}"),
            None => panic!("шлюз закрыл соединение, а push так и не пришёл"),
        }
    }

    pub async fn closed(&mut self) -> bool {
        loop {
            match timeout(DEADLINE, self.ws.recv()).await {
                Ok(Ok(Some(..))) => continue,
                Ok(Ok(None) | Err(..)) => return true,
                Err(..) => return false,
            }
        }
    }
}
