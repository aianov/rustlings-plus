use std::future::pending;
use std::io;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::sleep;

use crate::connect;
use crate::http::{Body, Reply, Web};
use crate::input::{Command, HELP, parse};
use crate::render::{Names, text};
use crate::ws::Ws;

const SIGN_IN: &str = "сначала войди: login <ник>";

enum Answer {
    Ok(Value),
    Failed(String),
}

pub struct Client<W> {
    addr: String,
    web: Web,
    auth_addr: String,
    auth: Web,
    token: Option<String>,
    ws: Option<Ws>,
    out: W,
    next_id: u64,
    names: Names,
    unread: Vec<String>,
}

impl<W: AsyncWrite + Unpin> Client<W> {
    pub fn new(http: &str, auth: &str, out: W) -> Client<W> {
        Client {
            addr: http.to_string(),
            web: Web::new(http),
            auth_addr: auth.to_string(),
            auth: Web::new(auth),
            token: None,
            ws: None,
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

    pub async fn wait_ready(&mut self, attempts: u32) -> io::Result<()> {
        let mut gateway = false;
        let mut auth = false;
        for _ in 0..attempts.max(1) {
            gateway = gateway || self.web.request("GET", "/health", None, None).await.is_ok();
            auth = auth
                || self
                    .auth
                    .request("GET", "/health", None, None)
                    .await
                    .is_ok();
            if gateway && auth {
                let line = format!(
                    "шлюз http://{} и вход http://{} на связи: register и login <ник>, help покажет команды",
                    self.addr, self.auth_addr
                );
                return self.say(&line).await;
            }
            sleep(Duration::from_millis(100)).await;
        }
        if !gateway {
            let line = format!("шлюз http://{} не отвечает", self.addr);
            self.say(&line).await?;
        }
        if !auth {
            let line = format!("auth-service http://{} не отвечает", self.auth_addr);
            self.say(&line).await?;
        }
        Ok(())
    }

    pub async fn incoming(&mut self) -> io::Result<Option<String>> {
        match self.ws.as_mut() {
            Some(ws) => ws.recv().await,
            None => pending().await,
        }
    }

    pub async fn hang_up(&mut self) -> io::Result<()> {
        match self.ws.take() {
            Some(mut ws) => ws.close(1000).await,
            None => Ok(()),
        }
    }

    pub async fn lost(&mut self) -> io::Result<()> {
        let code = self.ws.take().and_then(|ws| ws.close_code());
        let line = match code {
            Some(code) => format!("шлюз закрыл соединение, код {code}: войди снова, login <ник>"),
            None => String::from("соединение со шлюзом оборвалось: войди снова, login <ник>"),
        };
        self.say(&line).await
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
                let answer = self.signed("POST", "/auth/register", Some(&body)).await?;
                self.user(answer, "зарегистрирован").await
            }
            Command::Hello(name) => {
                let body = json!({ "username": name }).to_string();
                let login = match self.signed("POST", "/auth/login", Some(&body)).await? {
                    Answer::Ok(login) => login,
                    Answer::Failed(error) => return self.say(&error).await,
                };
                let token = text(&login, "token").unwrap_or_default().to_string();
                let user = login.get("user").cloned().unwrap_or(Value::Null);
                self.hang_up().await?;
                match connect(&self.addr, &token, 1).await {
                    Ok(ws) => self.ws = Some(ws),
                    Err(error) => {
                        let line = format!("WebSocket не открылся: {error}");
                        return self.say(&line).await;
                    }
                }
                self.token = Some(token);
                self.user(Answer::Ok(user), "привет,").await
            }
            Command::Me => {
                let answer = self.web("GET", "/users/me", None).await?;
                self.user(answer, "ты").await
            }
            Command::Logout => {
                let answer = self.signed("POST", "/auth/logout", None).await?;
                self.token = None;
                self.hang_up().await?;
                match answer {
                    Answer::Ok(..) => self.say("до встречи: токен отозван").await,
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::AvatarSet(file) => self.avatar_set(&file).await,
            Command::AvatarGet { nick, file } => {
                let file = file.unwrap_or_else(|| format!("{nick}.avatar"));
                self.avatar_get(&nick, &file).await
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

    fn bearer(&self) -> Option<String> {
        self.token.as_ref().map(|token| format!("Bearer {token}"))
    }

    async fn avatar_set(&mut self, file: &str) -> io::Result<()> {
        let kind = match file
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
        {
            Some(ext) if ext == "png" => "image/png",
            Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
            Some(ext) if ext == "gif" => "image/gif",
            Some(ext) if ext == "webp" => "image/webp",
            _ => {
                return self
                    .say("аватар это картинка: png, jpg, gif или webp")
                    .await;
            }
        };
        let bytes = match tokio::fs::read(file).await {
            Ok(bytes) => bytes,
            Err(error) => return self.say(&format!("не прочитать {file}: {error}")).await,
        };
        let bearer = self.bearer();
        let headers: Vec<(&str, &str)> = bearer
            .as_deref()
            .map(|value| ("Authorization", value))
            .into_iter()
            .collect();
        let body = Body {
            content_type: kind,
            bytes: &bytes,
        };
        let reply = self
            .web
            .send("PUT", "/users/me/avatar", Some(body), &headers)
            .await?;
        match reply.status {
            201 | 204 => {
                let line = format!("аватар загружен: {} байт", bytes.len());
                self.say(&line).await
            }
            status => {
                let line = format!("аватар не загружен, HTTP {status}: {}", reply.body);
                self.say(&line).await
            }
        }
    }

    async fn avatar_get(&mut self, nick: &str, file: &str) -> io::Result<()> {
        let profile = self
            .web
            .request("GET", &format!("/users/{nick}"), None, None)
            .await?;
        let user: Value = serde_json::from_str(&profile.body).unwrap_or(Value::Null);
        let Some(id) = text(&user, "id").map(str::to_string) else {
            return self.say(&format!("пользователя @{nick} нет")).await;
        };
        let have = tokio::fs::metadata(file)
            .await
            .map(|meta| meta.len())
            .unwrap_or(0);
        let range = format!("bytes={have}-");
        let headers: Vec<(&str, &str)> = if have > 0 {
            vec![("Range", range.as_str())]
        } else {
            Vec::new()
        };
        let reply = self
            .web
            .send("GET", &format!("/avatars/{id}"), None, &headers)
            .await?;
        match reply.status {
            200 => {
                tokio::fs::write(file, &reply.bytes).await?;
                let line = format!("скачано {} байт в {file}", reply.bytes.len());
                self.say(&line).await
            }
            206 => {
                let mut target = tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(file)
                    .await?;
                tokio::io::AsyncWriteExt::write_all(&mut target, &reply.bytes).await?;
                let line = format!(
                    "докачано {} байт с позиции {have} в {file}",
                    reply.bytes.len()
                );
                self.say(&line).await
            }
            416 => self.say(&format!("{file} уже скачан целиком")).await,
            status => {
                let line = format!("аватар не скачан, HTTP {status}: {}", reply.body);
                self.say(&line).await
            }
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
        if let Some(event) = text(&value, "event") {
            let data = value.get("data").cloned().unwrap_or(Value::Null);
            if event == "message" {
                if let Some(from) = text(&data, "from") {
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
        Ok(answer(reply))
    }

    async fn signed(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<Answer> {
        let token = self.token.clone();
        let reply = self
            .auth
            .request(method, path, body, token.as_deref())
            .await?;
        Ok(answer(reply))
    }

    async fn call(&mut self, method: &str, data: Value) -> io::Result<Answer> {
        self.next_id += 1;
        let id = self.next_id;
        let service = method.split('.').next().unwrap_or(method);
        let line =
            json!({ "id": id, "service": service, "method": method, "data": data }).to_string();
        let Some(ws) = self.ws.as_mut() else {
            return Ok(Answer::Failed(SIGN_IN.to_string()));
        };
        if ws.send_text(&line).await.is_err() {
            self.lost().await?;
            return Ok(Answer::Failed(SIGN_IN.to_string()));
        }
        loop {
            let received = match self.ws.as_mut() {
                Some(ws) => ws.recv().await,
                None => Ok(None),
            };
            let Ok(Some(raw)) = received else {
                self.lost().await?;
                return Ok(Answer::Failed(SIGN_IN.to_string()));
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

fn answer(reply: Reply) -> Answer {
    let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    if (200..300).contains(&reply.status) {
        return Answer::Ok(value);
    }
    match value.get("error") {
        Some(error) if error.is_object() => Answer::Failed(failure(error)),
        _ => Answer::Failed(format!("ошибка HTTP {}: {}", reply.status, reply.body)),
    }
}

fn failure(error: &Value) -> String {
    format!(
        "ошибка ({}): {}",
        text(error, "code").unwrap_or("?"),
        text(error, "message").unwrap_or_default()
    )
}
