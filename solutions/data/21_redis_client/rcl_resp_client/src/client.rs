use std::collections::VecDeque;
use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, timeout};

use crate::codec::{Limits, decode, encode_command};
use crate::{Error, Options, Result, Value};

type Reply = oneshot::Sender<Result<Value>>;

struct Request {
    args: Vec<Vec<u8>>,
    reply: Reply,
}

struct Target {
    addr: String,
    username: Option<String>,
    password: Option<String>,
    db: u32,
}

enum Outcome {
    Closed,
    Broken,
}

fn server_error(text: &str) -> Error {
    let (kind, message) = text.split_once(' ').unwrap_or((text, ""));
    Error::Server { kind: kind.to_string(), message: message.trim_start().to_string() }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(text.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn parse_url(url: &str) -> Result<Target> {
    let bad = |reason: &str| Error::InvalidUrl(format!("{reason}: {url:?}"));
    let rest = url.strip_prefix("redis://").ok_or_else(|| bad("нужна схема redis://"))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (credentials, hostport) = match authority.rsplit_once('@') {
        Some((credentials, hostport)) => (Some(credentials), hostport),
        None => (None, authority),
    };
    let (mut username, mut password) = (None, None);
    if let Some(credentials) = credentials {
        let (user, pass) = credentials.split_once(':').ok_or_else(|| bad("в учётных данных нет двоеточия"))?;
        if !user.is_empty() {
            username = Some(percent_decode(user).ok_or_else(|| bad("плохой %-код"))?);
        }
        password = Some(percent_decode(pass).ok_or_else(|| bad("плохой %-код"))?);
    }
    let (host, port) = match hostport.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().map_err(|_| bad("плохой порт"))?),
        None => (hostport, 6379),
    };
    if host.is_empty() {
        return Err(bad("нет хоста"));
    }
    let db = if path.is_empty() { 0 } else { path.parse::<u32>().map_err(|_| bad("номер базы не число"))? };
    Ok(Target { addr: format!("{host}:{port}"), username, password, db })
}

async fn read_one(stream: &mut TcpStream, buf: &mut BytesMut, limits: &Limits) -> Result<Value> {
    loop {
        if let Some(value) = decode(buf, limits).map_err(Error::Protocol)? {
            return Ok(value);
        }
        if stream.read_buf(buf).await? == 0 {
            return Err(Error::Disconnected);
        }
    }
}

async fn open(target: &Target, options: &Options, limits: &Limits) -> Result<(TcpStream, BytesMut)> {
    let attempt = async {
        let mut stream = TcpStream::connect(&target.addr).await?;
        let mut buf = BytesMut::new();
        let mut steps: Vec<Vec<Vec<u8>>> = Vec::new();
        if let Some(password) = &target.password {
            let mut auth = vec![b"AUTH".to_vec()];
            if let Some(user) = &target.username {
                auth.push(user.clone().into_bytes());
            }
            auth.push(password.clone().into_bytes());
            steps.push(auth);
        }
        if target.db != 0 {
            steps.push(vec![b"SELECT".to_vec(), target.db.to_string().into_bytes()]);
        }
        steps.push(vec![b"PING".to_vec()]);
        for step in steps {
            stream.write_all(&encode_command(step.as_slice())).await?;
            if let Value::Error(text) = read_one(&mut stream, &mut buf, limits).await? {
                return Err(server_error(&text));
            }
        }
        Ok::<_, Error>((stream, buf))
    };
    timeout(options.connect_timeout, attempt).await.map_err(|_| Error::Timeout)?
}

fn fail_all(waiting: &mut VecDeque<Reply>, make: impl Fn() -> Error) {
    for reply in waiting.drain(..) {
        let _ = reply.send(Err(make()));
    }
}

async fn serve(mut stream: TcpStream, mut buf: BytesMut, rx: &mut mpsc::Receiver<Request>, options: &Options, limits: &Limits) -> Outcome {
    let mut waiting: VecDeque<Reply> = VecDeque::new();
    let mut open = true;
    while open || !waiting.is_empty() {
        tokio::select! {
            req = rx.recv(), if open && waiting.len() < options.max_in_flight => match req {
                Some(req) => {
                    if stream.write_all(&encode_command(req.args.as_slice())).await.is_err() {
                        let _ = req.reply.send(Err(Error::Disconnected));
                        fail_all(&mut waiting, || Error::Disconnected);
                        return Outcome::Broken;
                    }
                    waiting.push_back(req.reply);
                }
                None => open = false,
            },
            read = stream.read_buf(&mut buf) => {
                if !matches!(read, Ok(n) if n > 0) {
                    fail_all(&mut waiting, || Error::Disconnected);
                    return Outcome::Broken;
                }
                loop {
                    match decode(&mut buf, limits) {
                        Ok(Some(value)) => {
                            if let Some(reply) = waiting.pop_front() {
                                let _ = reply.send(Ok(value));
                            }
                        }
                        Ok(None) => break,
                        Err(reason) => {
                            fail_all(&mut waiting, || Error::Protocol(reason.clone()));
                            return Outcome::Broken;
                        }
                    }
                }
            }
        }
    }
    Outcome::Closed
}

#[derive(Clone)]
pub struct Client {
    tx: mpsc::Sender<Request>,
    url: String,
    options: Options,
}

impl Client {
    pub async fn connect(url: &str) -> Result<Client> {
        Client::connect_with(url, Options::default()).await
    }

    pub async fn connect_with(url: &str, options: Options) -> Result<Client> {
        let target = parse_url(url)?;
        let limits = Limits { max_bulk: options.max_bulk, max_array: 1 << 20, max_depth: 16 };
        let first = open(&target, &options, &limits).await?;
        let (tx, mut rx) = mpsc::channel::<Request>(256);
        let task_options = options.clone();
        tokio::spawn(async move {
            let mut conn = first;
            while let Outcome::Broken = serve(conn.0, conn.1, &mut rx, &task_options, &limits).await {
                conn = loop {
                    if rx.is_closed() {
                        return;
                    }
                    sleep(task_options.reconnect_pause).await;
                    if let Ok(fresh) = open(&target, &task_options, &limits).await {
                        break fresh;
                    }
                };
            }
        });
        Ok(Client { tx, url: url.to_string(), options })
    }

    async fn raw(&self, args: Vec<Vec<u8>>) -> Result<Value> {
        let (reply, answer) = oneshot::channel();
        let wait = async {
            self.tx.send(Request { args, reply }).await.map_err(|_| Error::Closed)?;
            answer.await.map_err(|_| Error::Closed)?
        };
        timeout(self.options.command_timeout, wait).await.map_err(|_| Error::Timeout)?
    }

    pub async fn call<S: AsRef<[u8]>>(&self, args: &[S]) -> Result<Value> {
        match self.raw(args.iter().map(|a| a.as_ref().to_vec()).collect()).await? {
            Value::Error(text) => Err(server_error(&text)),
            value => Ok(value),
        }
    }

    pub async fn pipeline(&self, cmds: &[Vec<String>]) -> Result<Vec<Value>> {
        let wait = async {
            let mut answers = Vec::with_capacity(cmds.len());
            for cmd in cmds {
                let (reply, answer) = oneshot::channel();
                let args = cmd.iter().map(|s| s.clone().into_bytes()).collect();
                self.tx.send(Request { args, reply }).await.map_err(|_| Error::Closed)?;
                answers.push(answer);
            }
            let mut out = Vec::with_capacity(cmds.len());
            for answer in answers {
                out.push(answer.await.map_err(|_| Error::Closed)??);
            }
            Ok::<_, Error>(out)
        };
        timeout(self.options.command_timeout, wait).await.map_err(|_| Error::Timeout)?
    }

    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match self.call(&["GET", key]).await? {
            Value::Bulk(bytes) => Ok(Some(bytes)),
            Value::Null => Ok(None),
            other => Err(Error::Unexpected(other)),
        }
    }

    async fn expect_ok(&self, args: &[&[u8]]) -> Result<()> {
        match self.call(args).await? {
            Value::Simple(status) if status == "OK" => Ok(()),
            other => Err(Error::Unexpected(other)),
        }
    }

    pub async fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        self.expect_ok(&[b"SET", key.as_bytes(), value]).await
    }

    pub async fn set_ex(&self, key: &str, value: &[u8], seconds: u64) -> Result<()> {
        let seconds = seconds.to_string();
        self.expect_ok(&[b"SET", key.as_bytes(), value, b"EX", seconds.as_bytes()]).await
    }

    async fn int(&self, args: &[&str]) -> Result<i64> {
        match self.call(args).await? {
            Value::Int(n) => Ok(n),
            other => Err(Error::Unexpected(other)),
        }
    }

    pub async fn del(&self, key: &str) -> Result<bool> {
        Ok(self.int(&["DEL", key]).await? > 0)
    }

    pub async fn incr(&self, key: &str) -> Result<i64> {
        self.int(&["INCR", key]).await
    }

    pub async fn expire(&self, key: &str, seconds: u64) -> Result<bool> {
        let seconds = seconds.to_string();
        Ok(self.int(&["EXPIRE", key, seconds.as_str()]).await? == 1)
    }

    pub async fn ttl(&self, key: &str) -> Result<i64> {
        self.int(&["TTL", key]).await
    }

    pub async fn blpop(&self, key: &str, timeout_secs: u32) -> Result<Option<(String, Vec<u8>)>> {
        let mut options = self.options.clone();
        options.command_timeout += Duration::from_secs(u64::from(timeout_secs));
        let dedicated = Client::connect_with(&self.url, options).await?;
        let seconds = timeout_secs.to_string();
        match dedicated.call(&["BLPOP", key, seconds.as_str()]).await? {
            Value::Null => Ok(None),
            Value::Array(items) => match <[Value; 2]>::try_from(items) {
                Ok([Value::Bulk(list), Value::Bulk(value)]) => Ok(Some((String::from_utf8_lossy(&list).into_owned(), value))),
                Ok(other) => Err(Error::Unexpected(Value::Array(other.to_vec()))),
                Err(items) => Err(Error::Unexpected(Value::Array(items))),
            },
            other => Err(Error::Unexpected(other)),
        }
    }
}
