use std::io;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::http::Web;
use crate::input::{Command, HELP, parse};
use crate::render::{Names, text};

enum Answer {
    Ok(Value),
    Failed(String),
}

pub struct Client<W> {
    web: Web,
    token: Option<String>,
    writer: OwnedWriteHalf,
    lines: Lines<BufReader<OwnedReadHalf>>,
    out: W,
    next_id: u64,
    names: Names,
    unread: Vec<String>,
}

impl<W: AsyncWrite + Unpin> Client<W> {
    pub fn new(stream: TcpStream, http: &str, out: W) -> Client<W> {
        let (read, writer) = stream.into_split();
        Client {
            web: Web::new(http),
            token: None,
            writer,
            lines: BufReader::new(read).lines(),
            out,
            next_id: 0,
            names: Names::default(),
            unread: Vec::new(),
        }
    }

    pub async fn say(&mut self, line: &str) -> io::Result<()> {
        self.out.write_all(line.as_bytes()).await?;
        self.out.write_all(b"\n").await?;
        self.out.flush().await
    }

    pub async fn incoming(&mut self) -> io::Result<Option<String>> {
        self.lines.next_line().await
    }

    pub async fn unsolicited(&mut self, raw: &str) -> io::Result<()> {
        self.show(raw).await?;
        self.catch_up().await
    }

    pub async fn command(&mut self, line: &str) -> io::Result<bool> {
        let command = match parse(line) {
            Ok(command) => command,
            Err(message) => {
                self.say(&message).await?;
                return Ok(true);
            }
        };
        match command {
            Command::Empty => {}
            Command::Quit => return Ok(false),
            Command::Help => self.say(HELP).await?,
            other => self.run(other).await?,
        }
        self.catch_up().await?;
        Ok(true)
    }

    async fn run(&mut self, command: Command) -> io::Result<()> {
        match command {
            Command::Register(name) => {
                let body = json!({ "username": name }).to_string();
                let answer = self.web("POST", "/auth/register", Some(&body)).await?;
                self.user(answer, "зарегистрирован").await
            }
            Command::Hello(name) => {
                let body = json!({ "username": name }).to_string();
                let token = match self.web("POST", "/auth/login", Some(&body)).await? {
                    Answer::Ok(login) => text(&login, "token").unwrap_or_default().to_string(),
                    Answer::Failed(error) => return self.say(&error).await,
                };
                self.token = Some(token.clone());
                let answer = self
                    .call("session.hello", json!({ "token": token }))
                    .await?;
                self.user(answer, "привет,").await
            }
            Command::Me => {
                let answer = self.web("GET", "/users/me", None).await?;
                self.user(answer, "ты").await
            }
            Command::Profile(name) => {
                let answer = self.web("GET", &format!("/users/{name}"), None).await?;
                self.user(answer, "профиль").await
            }
            Command::Show(post) => match self.web("GET", &format!("/posts/{post}"), None).await? {
                Answer::Ok(post) => {
                    let line = self.names.post(&post);
                    self.say(&line).await
                }
                Answer::Failed(error) => self.say(&error).await,
            },
            Command::Bye => {
                let answer = self.call("session.bye", json!({})).await?;
                self.user(answer, "пока,").await
            }
            Command::Find(name) => {
                let answer = self.call("users.find", json!({ "username": name })).await?;
                self.user(answer, "нашёлся").await
            }
            Command::Follow(name) => self.towards(&name, "users.follow").await,
            Command::Mute(name) => self.towards(&name, "users.mute").await,
            Command::Post(body) => {
                let answer = self.call("posts.create", json!({ "text": body })).await?;
                match answer {
                    Answer::Ok(post) => {
                        let line = self.names.post(&post);
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Like(post) => {
                let answer = self.call("posts.like", json!({ "post": post })).await?;
                self.changed(answer).await
            }
            Command::Delete(post) => {
                let answer = self.call("posts.delete", json!({ "post": post })).await?;
                self.changed(answer).await
            }
            Command::Feed { page, size } => {
                let answer = self
                    .call("posts.feed", json!({ "page": page, "size": size }))
                    .await?;
                self.feed(answer).await
            }
            Command::Msg { to, text: body } => {
                let Some(peer) = self.resolve(&to).await? else {
                    return Ok(());
                };
                let answer = self
                    .call("chat.send", json!({ "peer": peer, "text": body }))
                    .await?;
                match answer {
                    Answer::Ok(message) => {
                        let from = text(&message, "from").unwrap_or_default();
                        let said = text(&message, "text").unwrap_or_default();
                        let line = format!(
                            "{} → {}: {said}",
                            self.names.name(from),
                            self.names.name(&peer)
                        );
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::History { with, limit } => {
                let Some(peer) = self.resolve(&with).await? else {
                    return Ok(());
                };
                let answer = self
                    .call("chat.history", json!({ "peer": peer, "limit": limit }))
                    .await?;
                self.history(answer, "переписки пока нет").await
            }
            Command::Empty | Command::Help | Command::Quit => Ok(()),
        }
    }

    async fn towards(&mut self, name: &str, method: &str) -> io::Result<()> {
        let Some(target) = self.resolve(name).await? else {
            return Ok(());
        };
        let answer = self.call(method, json!({ "target": target })).await?;
        self.changed(answer).await
    }

    async fn resolve(&mut self, name: &str) -> io::Result<Option<String>> {
        if let Some(id) = self.names.id_of(name) {
            return Ok(Some(id));
        }
        match self.call("users.find", json!({ "username": name })).await? {
            Answer::Ok(user) => {
                self.names.remember_user(&user);
                Ok(text(&user, "id").map(str::to_string))
            }
            Answer::Failed(error) => {
                self.say(&error).await?;
                Ok(None)
            }
        }
    }

    async fn user(&mut self, answer: Answer, lead: &str) -> io::Result<()> {
        match answer {
            Answer::Ok(user) => {
                self.names.remember_user(&user);
                let id = text(&user, "id").unwrap_or_default();
                let line = format!("{lead} {} ({id})", self.names.name(id));
                self.say(&line).await
            }
            Answer::Failed(error) => self.say(&error).await,
        }
    }

    async fn changed(&mut self, answer: Answer) -> io::Result<()> {
        match answer {
            Answer::Ok(reply) => {
                let changed = reply
                    .get("changed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.say(if changed {
                    "готово"
                } else {
                    "уже было"
                })
                .await
            }
            Answer::Failed(error) => self.say(&error).await,
        }
    }

    async fn feed(&mut self, answer: Answer) -> io::Result<()> {
        let posts = match answer {
            Answer::Ok(reply) => reply
                .get("posts")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            Answer::Failed(error) => return self.say(&error).await,
        };
        if posts.is_empty() {
            return self.say("лента пуста").await;
        }
        for post in &posts {
            let line = self.names.post(post);
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn history(&mut self, answer: Answer, empty: &str) -> io::Result<()> {
        let messages = match answer {
            Answer::Ok(reply) => reply
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            Answer::Failed(error) => return self.say(&error).await,
        };
        if messages.is_empty() {
            return self.say(empty).await;
        }
        for message in &messages {
            let line = self.names.message(message);
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn catch_up(&mut self) -> io::Result<()> {
        while let Some(peer) = self.unread.pop() {
            let answer = self
                .call("chat.history", json!({ "peer": peer, "limit": 1 }))
                .await?;
            if let Answer::Ok(reply) = answer {
                let last = reply
                    .get("messages")
                    .and_then(Value::as_array)
                    .and_then(|all| all.last())
                    .cloned();
                if let Some(message) = last {
                    let line = format!("* {}", self.names.message(&message));
                    self.say(&line).await?;
                }
            }
        }
        Ok(())
    }

    async fn show(&mut self, raw: &str) -> io::Result<()> {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return self
                .say(&format!("непонятная строка от шлюза: {raw}"))
                .await;
        };
        if value.get("push").is_some() {
            if text(&value, "push") == Some("message") {
                if let Some(from) = text(&value, "from") {
                    self.unread.push(from.to_string());
                    return Ok(());
                }
            }
            let line = self.names.push(&value);
            return self.say(&line).await;
        }
        match value.get("error") {
            Some(error) => {
                let line = failure(error);
                self.say(&line).await
            }
            None => self.say(&format!("ответ без запроса: {raw}")).await,
        }
    }

    async fn web(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<Answer> {
        let token = self.token.clone();
        let reply = self
            .web
            .request(method, path, body, token.as_deref())
            .await?;
        let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
        if (200..300).contains(&reply.status) {
            return Ok(Answer::Ok(value));
        }
        match value.get("error") {
            Some(error) if error.is_object() => Ok(Answer::Failed(failure(error))),
            _ => Ok(Answer::Failed(format!(
                "ошибка HTTP {}: {}",
                reply.status, reply.body
            ))),
        }
    }

    async fn call(&mut self, method: &str, data: Value) -> io::Result<Answer> {
        self.next_id += 1;
        let id = self.next_id;
        let service = method.split('.').next().unwrap_or(method);
        let mut line =
            json!({ "id": id, "service": service, "method": method, "data": data }).to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        loop {
            let Some(raw) = self.lines.next_line().await? else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "шлюз закрыл соединение",
                ));
            };
            let Ok(value) = serde_json::from_str::<Value>(&raw) else {
                self.show(&raw).await?;
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                self.show(&raw).await?;
                continue;
            }
            if let Some(error) = value.get("error") {
                return Ok(Answer::Failed(failure(error)));
            }
            return Ok(Answer::Ok(value.get("ok").cloned().unwrap_or(Value::Null)));
        }
    }
}

fn failure(error: &Value) -> String {
    format!(
        "ошибка ({}): {}",
        text(error, "code").unwrap_or("?"),
        text(error, "message").unwrap_or_default()
    )
}
