use std::collections::VecDeque;
use std::future::pending;
use std::io;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::sleep;

mod session;

use crate::connect_with;
use crate::http::{Body, Reply, Web};
use crate::input::{Command, HELP, parse};
use crate::net::Dialer;
use crate::render::{Names, text};
use crate::ws::Ws;

const SIGN_IN: &str = "сначала войди: login <ник> <пароль>";

const EXPIRED: u16 = 4001;

const REVOKED: u16 = 4003;

enum Answer {
    Ok(Value),
    Failed(String),
}

pub struct Client<W> {
    addr: String,
    dialer: Dialer,
    web: Web,
    token: Option<String>,
    refresh: Option<String>,
    ws: Option<Ws>,
    out: W,
    next_id: u64,
    names: Names,
    last_seq: Option<u64>,
    tracing: bool,
    last_rid: Option<String>,
    feed_next: Option<String>,
}

impl<W: AsyncWrite + Unpin> Client<W> {
    pub fn new(http: &str, dialer: Dialer, out: W) -> Client<W> {
        Client {
            addr: http.to_string(),
            web: Web::with(http, dialer.clone()),
            dialer,
            token: None,
            refresh: None,
            ws: None,
            out,
            next_id: 0,
            names: Names::default(),
            last_seq: None,
            tracing: false,
            last_rid: None,
            feed_next: None,
        }
    }

    pub async fn say(&mut self, line: &str) -> io::Result<()> {
        self.out.write_all(line.as_bytes()).await?;
        self.out.write_all(b"\n").await?;
        self.out.flush().await
    }

    pub async fn wait_ready(&mut self, attempts: u32) -> io::Result<()> {
        let scheme = self.dialer.http_scheme();
        for _ in 0..attempts.max(1) {
            if self.web.request("GET", "/health", None, None).await.is_ok() {
                let line = format!(
                    "шлюз {scheme}://{} на связи, вход и регистрация через него же: register и login <ник> <пароль>, help покажет команды",
                    self.addr
                );
                return self.say(&line).await;
            }
            sleep(Duration::from_millis(100)).await;
        }
        let line = format!("шлюз {scheme}://{} не отвечает", self.addr);
        self.say(&line).await
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

    pub async fn closed(&mut self) -> io::Result<()> {
        let expired = self.ws.as_ref().and_then(Ws::close_code) == Some(EXPIRED);
        if expired && self.refresh.is_some() {
            self.ws = None;
            if self.renew().await? {
                return Ok(());
            }
        }
        self.lost().await
    }

    pub async fn lost(&mut self) -> io::Result<()> {
        let code = self.ws.take().and_then(|ws| ws.close_code());
        let line = match code {
            Some(REVOKED) => String::from(
                "сессия отозвана: из неё вышли на другом устройстве, войди снова, login <ник> <пароль>",
            ),
            Some(code) => {
                format!("шлюз закрыл соединение, код {code}: войди снова, login <ник> <пароль>")
            }
            None => {
                String::from("соединение со шлюзом оборвалось: войди снова, login <ник> <пароль>")
            }
        };
        self.say(&line).await
    }

    pub async fn unsolicited(&mut self, raw: &str) -> io::Result<()> {
        self.show(raw).await
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
            Command::Trace(on) => {
                self.tracing = on;
                let line = if on {
                    "request-id ответов показываю"
                } else {
                    "request-id ответов больше не показываю"
                };
                self.say(line).await?;
            }
            other => {
                self.last_rid = None;
                if let Err(error) = Box::pin(self.run(other)).await {
                    let line = self.trouble(&error);
                    self.say(&line).await?;
                }
                if self.tracing {
                    if let Some(rid) = self.last_rid.take() {
                        self.say(&format!("  · request-id {rid}")).await?;
                    }
                }
            }
        }
        Ok(true)
    }

    fn trouble(&self, error: &io::Error) -> String {
        let garbled = matches!(
            error.kind(),
            io::ErrorKind::UnexpectedEof
                | io::ErrorKind::InvalidData
                | io::ErrorKind::ConnectionReset
        );
        if garbled && !self.dialer.is_tls() {
            format!("нет связи ({error}): похоже, сервер ждёт TLS, запусти с --ca certs/ca.pem")
        } else {
            format!("нет связи: {error}")
        }
    }

    async fn run(&mut self, command: Command) -> io::Result<()> {
        match command {
            Command::Register {
                name,
                password,
                email,
            } => {
                let body = match email {
                    Some(email) => {
                        json!({ "username": name, "password": password, "email": email })
                    }
                    None => json!({ "username": name, "password": password }),
                }
                .to_string();
                let answer = self.signed("POST", "/auth/register", Some(&body)).await?;
                self.user(answer, "зарегистрирован").await
            }
            Command::Hello { name, password } => {
                let body = json!({ "username": name, "password": password }).to_string();
                let login = match self.signed("POST", "/auth/login", Some(&body)).await? {
                    Answer::Ok(login) => login,
                    Answer::Failed(error) => return self.say(&error).await,
                };
                let user = login.get("user").cloned().unwrap_or(Value::Null);
                if !self.adopt(&login).await? {
                    return Ok(());
                }
                self.user(Answer::Ok(user), "привет,").await
            }
            Command::LogoutAll => {
                let answer = self.signed("POST", "/auth/logout-all", None).await?;
                self.forget().await?;
                match answer {
                    Answer::Ok(..) => self.say("вышли на всех устройствах").await,
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::DeleteAccount => {
                let answer = self.signed("POST", "/auth/delete-account", None).await?;
                match answer {
                    Answer::Ok(..) => {
                        self.forget().await?;
                        self.say("аккаунт удаляется: посты, профиль и вход исчезнут через несколько секунд")
                            .await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Password { old, new } => {
                let body = json!({ "old_password": old, "new_password": new }).to_string();
                match self.signed("POST", "/auth/password", Some(&body)).await? {
                    Answer::Ok(..) => self.say("пароль сменён, другие устройства вышли").await,
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Confirm => match self.signed("POST", "/auth/email/request", None).await? {
                Answer::Ok(..) => {
                    self.say("письмо отправлено (пока в лог auth-service)")
                        .await
                }
                Answer::Failed(error) => self.say(&error).await,
            },
            Command::Forgot(name) => {
                let body = json!({ "username": name }).to_string();
                match self
                    .signed("POST", "/auth/password/forgot", Some(&body))
                    .await?
                {
                    Answer::Ok(..) => self.say("если почта есть, письмо со ссылкой ушло").await,
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Me => {
                let answer = self.web("GET", "/users/me", None).await?;
                self.user(answer, "ты").await
            }
            Command::Set { field, value } => {
                let body = json!({ field.as_str(): value }).to_string();
                let answer = self.web("PATCH", "/users/me", Some(&body)).await?;
                self.user(answer, "профиль обновлён:").await
            }
            Command::Clear(field) => {
                let body = json!({ field.as_str(): null }).to_string();
                let answer = self.web("PATCH", "/users/me", Some(&body)).await?;
                self.user(answer, "профиль обновлён:").await
            }
            Command::Logout => {
                let answer = self.signed("POST", "/auth/logout", None).await?;
                self.forget().await?;
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
            Command::Show(post) => match self
                .web("GET", &format!("/posts/{}", self.names.real(&post)), None)
                .await?
            {
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
            Command::Card(name) => {
                let Some(target) = self.resolve(&name).await? else {
                    return Ok(());
                };
                let answer = self
                    .call("users.profile", json!({ "target": target }))
                    .await?;
                self.card(answer).await
            }
            Command::Follow(name) => self.towards(&name, "users.follow").await,
            Command::TopAuthors => {
                let answer = self.call("posts.top", json!({})).await?;
                self.top(answer).await
            }
            Command::Mute(name) => self.towards(&name, "users.mute").await,
            Command::Post(body) => {
                let mut data = draft(&body);
                data["idempotency_key"] = Value::String(fresh_key());
                let mut answer = self.call("posts.create", data.clone()).await?;
                if matches!(&answer, Answer::Failed(error) if error == SIGN_IN)
                    && self.token.is_some()
                {
                    answer = self.call("posts.create", data).await?;
                }
                match answer {
                    Answer::Ok(post) => {
                        let line = self.names.post(&post);
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Like(post) => {
                let post = self.names.real(&post);
                let answer = self.call("posts.like", json!({ "post": post })).await?;
                self.changed(answer).await
            }
            Command::Comment { post, text } => {
                let post = self.names.real(&post);
                let data = json!({ "post": post, "text": text });
                let answer = self.call("posts.comment", data).await?;
                self.commented(answer).await
            }
            Command::Bell => {
                let answer = self.call("notify.list", json!({ "limit": 10 })).await?;
                self.bell(answer).await
            }
            Command::ReadAll => {
                let answer = self.call("notify.read", json!({})).await?;
                self.marked(answer).await
            }
            Command::Delete(post) => {
                let post = self.names.real(&post);
                let answer = self.call("posts.delete", json!({ "post": post })).await?;
                self.changed(answer).await
            }
            Command::Feed { page, size } => {
                let answer = self
                    .call("posts.feed", json!({ "page": page, "size": size }))
                    .await?;
                self.feed(answer).await
            }
            Command::More => {
                let Some(cursor) = self.feed_next.clone() else {
                    return self.say("дальше ленты нет: начни с feed").await;
                };
                let answer = self
                    .call("posts.feed", json!({ "cursor": cursor, "size": 10 }))
                    .await?;
                self.feed(answer).await
            }
            Command::Watch { recent } => {
                let answer = self
                    .call("posts.watch", json!({ "recent": recent }))
                    .await?;
                match answer {
                    Answer::Ok(..) => {
                        self.say("живая лента включена: новые посты появятся сами")
                            .await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Unwatch => match self.call("posts.unwatch", json!({})).await? {
                Answer::Ok(reply) if reply.get("changed") == Some(&Value::Bool(true)) => {
                    self.say("живая лента выключена").await
                }
                Answer::Ok(..) => self.say("живая лента и так выключена").await,
                Answer::Failed(error) => self.say(&error).await,
            },
            Command::Msg {
                to,
                text: body,
                ttl,
            } => {
                let Some(peer) = self.resolve(&to).await? else {
                    return Ok(());
                };
                let mut data = json!({ "peer": peer, "text": body });
                if let Some(ttl) = ttl {
                    data["ttl"] = Value::from(ttl);
                }
                let answer = self.call("chat.send", data).await?;
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
            Command::Typing(name) => {
                let Some(peer) = self.resolve(&name).await? else {
                    return Ok(());
                };
                match self.call("chat.typing", json!({ "peer": peer })).await? {
                    Answer::Ok(reply) if reply.get("changed") == Some(&Value::Bool(true)) => {
                        let line = format!("{} видит «печатает…»", self.names.name(&peer));
                        self.say(&line).await
                    }
                    Answer::Ok(..) => self.say("«печатает» уже ушло недавно").await,
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Online(name) => {
                let Some(target) = self.resolve(&name).await? else {
                    return Ok(());
                };
                match self
                    .call("users.online", json!({ "target": target }))
                    .await?
                {
                    Answer::Ok(reply) => {
                        let state = match reply.get("online") == Some(&Value::Bool(true)) {
                            true => "в сети",
                            false => "не в сети",
                        };
                        let line = format!("{} {state}", self.names.name(&target));
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Empty | Command::Help | Command::Quit | Command::Trace(..) => Ok(()),
            data => Box::pin(self.run_data(data)).await,
        }
    }

    async fn run_data(&mut self, command: Command) -> io::Result<()> {
        match command {
            Command::Related { method, nick } => {
                let (nick, after) = match nick.split_once(' ') {
                    Some((nick, after)) => (nick.to_string(), Some(after.to_string())),
                    None => (nick, None),
                };
                let Some(target) = self.resolve(&nick).await? else {
                    return Ok(());
                };
                let mut data = json!({ "target": target });
                if let Some(after) = after {
                    let after = after.strip_prefix("u_").unwrap_or(&after).to_string();
                    data["after"] = Value::String(format!("u_{after}"));
                }
                let answer = self.call(method, data).await?;
                self.people(&nick, method, answer).await
            }
            Command::Thread(post) => {
                let post = self.names.real(&post);
                let answer = self.call("posts.thread", json!({ "post": post })).await?;
                self.thread(answer).await
            }
            Command::Reply { post, parent, text } => {
                let post = self.names.real(&post);
                let data = json!({ "post": post, "parent": parent, "text": text });
                let answer = self.call("posts.comment", data).await?;
                self.commented(answer).await
            }
            Command::Pin(post, on) => {
                let post = self.names.real(&post);
                let method = if on { "posts.pin" } else { "posts.unpin" };
                let answer = self.call(method, json!({ "post": post })).await?;
                self.changed(answer).await
            }
            Command::Pinned(nick) => {
                let Some(target) = self.resolve(&nick).await? else {
                    return Ok(());
                };
                let answer = self
                    .call("posts.pinned", json!({ "target": target }))
                    .await?;
                match answer {
                    Answer::Ok(page) => {
                        let posts = page.get("posts").and_then(Value::as_array).cloned();
                        if posts.as_ref().is_none_or(Vec::is_empty) {
                            return self.say("закреплённых постов нет").await;
                        }
                        for post in posts.iter().flatten() {
                            let line = format!("📌 {}", self.names.post(post));
                            self.say(&line).await?;
                        }
                        Ok(())
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::LikeComment(comment) => {
                let answer = self
                    .call("posts.like_comment", json!({ "comment": comment }))
                    .await?;
                match answer {
                    Answer::Ok(reply) => {
                        let likes = reply.get("likes").and_then(Value::as_u64).unwrap_or(0);
                        let line = format!("у комментария {comment} лайков: {likes}");
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Search(filters, text) => {
                let mut data = json!({ "text": text });
                for (flag, value) in filters {
                    if flag == "author" {
                        let Some(target) = self.resolve(&value).await? else {
                            return Ok(());
                        };
                        data["author"] = Value::String(target);
                    } else {
                        data[flag.as_str()] = Value::String(value);
                    }
                }
                match self.call("posts.search", data).await? {
                    Answer::Ok(page) => {
                        let posts = page.get("posts").and_then(Value::as_array).cloned();
                        if posts.as_ref().is_none_or(Vec::is_empty) {
                            return self.say("ничего не нашлось").await;
                        }
                        for post in posts.iter().flatten() {
                            let line = self.names.post(post);
                            self.say(&line).await?;
                        }
                        Ok(())
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Export(file) => {
                let token = self.token.clone();
                let reply = self
                    .web
                    .request("GET", "/users/me/export", None, token.as_deref())
                    .await?;
                if reply.status != 200 {
                    let line = format!("выгрузка не пришла, HTTP {}: {}", reply.status, reply.body);
                    return self.say(&line).await;
                }
                let count = reply.body.lines().filter(|line| !line.is_empty()).count();
                tokio::fs::write(&file, &reply.bytes).await?;
                self.say(&format!("выгружено постов: {count}, файл {file}"))
                    .await
            }
            Command::AuthorPosts(nick, cursor) => {
                let Some(target) = self.resolve(&nick).await? else {
                    return Ok(());
                };
                let mut data = json!({ "target": target });
                if let Some(cursor) = cursor {
                    data["cursor"] = Value::String(cursor);
                }
                match self.call("posts.by_author", data).await? {
                    Answer::Ok(page) => {
                        let posts = page.get("posts").and_then(Value::as_array).cloned();
                        if posts.as_ref().is_none_or(Vec::is_empty) {
                            return self.say("постов больше нет").await;
                        }
                        for post in posts.iter().flatten() {
                            let line = self.names.post(post);
                            self.say(&line).await?;
                        }
                        match text(&page, "next") {
                            Some(next) => self.say(&format!("  дальше: posts {nick} {next}")).await,
                            None => Ok(()),
                        }
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::FullText(query) => {
                let query = query.trim_matches('"').to_string();
                match self.call("posts.find", json!({ "query": query })).await? {
                    Answer::Ok(found) => {
                        let hits = found.get("hits").and_then(Value::as_array).cloned();
                        if hits.as_ref().is_none_or(Vec::is_empty) {
                            return self.say("ничего не нашлось").await;
                        }
                        for hit in hits.iter().flatten() {
                            let post = hit.get("post").cloned().unwrap_or(Value::Null);
                            let line = self.names.post(&post);
                            self.say(&line).await?;
                            let headline = text(hit, "headline").unwrap_or_default();
                            self.say(&format!("    … {headline} …")).await?;
                        }
                        Ok(())
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::People(nick) => {
                let answer = self.call("users.people", json!({ "query": nick })).await?;
                self.people(&nick, "users.people", answer).await
            }
            Command::Settings(change) => {
                let answer = match change {
                    None => self.call("users.settings", json!({})).await?,
                    Some((field, value)) => {
                        let value = match value.as_str() {
                            "on" => Value::Bool(true),
                            "off" => Value::Bool(false),
                            _ => Value::String(value),
                        };
                        let mut patch = json!({});
                        patch[field.as_str()] = value;
                        self.call("users.set_settings", patch).await?
                    }
                };
                match answer {
                    Answer::Ok(settings) => {
                        let line = format!("настройки: {settings}");
                        self.say(&line).await
                    }
                    Answer::Failed(error) => self.say(&error).await,
                }
            }
            Command::Suggest => {
                let answer = self.call("users.suggest", json!({})).await?;
                self.people("тебя", "users.suggest", answer).await
            }
            Command::Daily(days) => {
                let answer = self.call("posts.daily", json!({ "days": days })).await?;
                self.daily(answer).await
            }
            Command::History {
                with,
                limit,
                cursor,
            } => {
                let Some(peer) = self.resolve(&with).await? else {
                    return Ok(());
                };
                let mut data = json!({ "peer": peer, "limit": limit });
                if let Some(cursor) = cursor {
                    data["cursor"] = Value::String(cursor);
                }
                let answer = self.call("chat.history", data).await?;
                let next = match &answer {
                    Answer::Ok(reply) => text(reply, "next").map(str::to_string),
                    Answer::Failed(..) => None,
                };
                self.history(answer, "переписки пока нет").await?;
                match next {
                    Some(next) => {
                        let line = format!("  дальше: history {with} {limit} {next}");
                        self.say(&line).await
                    }
                    None => Ok(()),
                }
            }
            Command::Edit { to, id, text: body } => {
                let Some(peer) = self.resolve(&to).await? else {
                    return Ok(());
                };
                let data = json!({ "peer": peer, "id": id, "text": body });
                let answer = self.call("chat.edit", data).await?;
                self.changed(answer).await
            }
            Command::Seen(to) => {
                let Some(peer) = self.resolve(&to).await? else {
                    return Ok(());
                };
                let answer = self.call("chat.seen", json!({ "peer": peer })).await?;
                self.changed(answer).await
            }
            Command::Chats => match self.call("chat.list", json!({})).await? {
                Answer::Ok(list) => {
                    let chats = list.get("chats").and_then(Value::as_array).cloned();
                    if chats.as_ref().is_none_or(Vec::is_empty) {
                        return self.say("чатов пока нет").await;
                    }
                    for chat in chats.iter().flatten() {
                        let peer = text(chat, "peer").unwrap_or_default().to_string();
                        let unread = match chat.get("unread").and_then(Value::as_u64) {
                            Some(0) | None => String::new(),
                            Some(count) if count >= 100 => String::from(" [99+]"),
                            Some(count) => format!(" [{count}]"),
                        };
                        let line = format!(
                            "{}{unread}: {}",
                            self.names.name(&peer),
                            text(chat, "preview").unwrap_or_default()
                        );
                        self.say(&line).await?;
                    }
                    Ok(())
                }
                Answer::Failed(error) => self.say(&error).await,
            },
            _ => Ok(()),
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
            Some(ext) if ext == "webp" => "image/webp",
            _ => {
                return self.say("аватар это картинка: png, jpg или webp").await;
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
                let line = match reply.header("location") {
                    Some(link) => format!("{line}, ссылка {link}"),
                    None => line,
                };
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
        if text(&user, "id").is_none() {
            return self.say(&format!("пользователя @{nick} нет")).await;
        }
        let Some(link) = text(&user, "avatar").map(str::to_string) else {
            return self.say(&format!("у @{nick} нет аватара")).await;
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
        let reply = self.web.send("GET", &link, None, &headers).await?;
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
                target.write_all(&reply.bytes).await?;
                target.flush().await?;
                let line = format!(
                    "докачано {} байт с позиции {have} в {file}",
                    reply.bytes.len()
                );
                self.say(&line).await
            }
            416 => self.say(&format!("{file} уже скачан целиком")).await,
            410 => {
                self.say("ссылка на аватар просрочена, запроси профиль заново")
                    .await
            }
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
                let mut line = format!("{lead} {} ({id})", self.names.name(id));
                if let Some(name) = text(&user, "display_name") {
                    line.push_str(&format!(", имя «{name}»"));
                }
                if let Some(bio) = text(&user, "bio") {
                    line.push_str(&format!(", о себе: {bio}"));
                }
                self.say(&line).await
            }
            Answer::Failed(error) => self.say(&error).await,
        }
    }

    async fn people(&mut self, nick: &str, method: &str, answer: Answer) -> io::Result<()> {
        let list = match answer {
            Answer::Ok(list) => list,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let users = list.get("users").and_then(Value::as_array).cloned();
        let mut names = Vec::new();
        for user in users.iter().flatten() {
            self.names.remember_user(user);
            names.push(text(user, "username").unwrap_or_default().to_string());
        }
        let what = match method {
            "users.followers" => "подписчики",
            "users.following" => "подписки",
            "users.suggest" => "возможно, знакомы с",
            "users.people" => "похожие на",
            _ => "друзья",
        };
        let line = match names.is_empty() {
            true => format!("{what} {nick}: никого"),
            false => format!("{what} {nick}: {}", names.join(", ")),
        };
        self.say(&line).await?;
        match text(&list, "next") {
            Some(next) => {
                self.say(&format!(
                    "  дальше: {} {nick} {next}",
                    method.trim_start_matches("users.")
                ))
                .await
            }
            None => Ok(()),
        }
    }

    async fn thread(&mut self, answer: Answer) -> io::Result<()> {
        let tree = match answer {
            Answer::Ok(tree) => tree,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let nodes = tree.get("nodes").and_then(Value::as_array).cloned();
        if nodes.as_ref().is_none_or(Vec::is_empty) {
            return self.say("комментариев пока нет").await;
        }
        for node in nodes.iter().flatten() {
            let depth = node.get("depth").and_then(Value::as_u64).unwrap_or(1);
            let pad = "  ".repeat(usize::try_from(depth).unwrap_or(1));
            let id = text(node, "id").unwrap_or_default();
            let replies = node.get("replies").and_then(Value::as_u64).unwrap_or(0);
            let body = match node.get("deleted").and_then(Value::as_bool) {
                Some(true) => String::from("[комментарий удалён]"),
                _ => format!(
                    "{}: {}",
                    self.names.name(text(node, "author").unwrap_or_default()),
                    text(node, "text").unwrap_or_default()
                ),
            };
            let line = format!("{pad}{id} {body} (ответов: {replies})");
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn top(&mut self, answer: Answer) -> io::Result<()> {
        let top = match answer {
            Answer::Ok(top) => top,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let authors = top.get("authors").and_then(Value::as_array).cloned();
        if authors.as_ref().is_none_or(Vec::is_empty) {
            return self.say("за неделю лайков не было").await;
        }
        for rank in authors.iter().flatten() {
            let number = |key: &str| rank.get(key).and_then(Value::as_i64).unwrap_or(0);
            let shift = match number("shift") {
                0 => String::new(),
                up if up > 0 => format!(", вверх на {up}"),
                down => format!(", вниз на {}", -down),
            };
            let line = format!(
                "{}. {}: лайков {}{shift}",
                number("place"),
                text(rank, "author_name").unwrap_or_default(),
                number("likes")
            );
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn daily(&mut self, answer: Answer) -> io::Result<()> {
        let stats = match answer {
            Answer::Ok(stats) => stats,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let days = stats.get("days").and_then(Value::as_array).cloned();
        for day in days.iter().flatten() {
            let count = |key: &str| day.get(key).and_then(Value::as_u64).unwrap_or(0);
            let line = format!(
                "{}: постов {}, лайков {}",
                text(day, "day").unwrap_or_default(),
                count("posts"),
                count("likes")
            );
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn card(&mut self, answer: Answer) -> io::Result<()> {
        let card = match answer {
            Answer::Ok(card) => card,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let user = card.get("user").cloned().unwrap_or(Value::Null);
        self.names.remember_user(&user);
        let id = text(&user, "id").unwrap_or_default();
        let mut line = format!("карточка {} ({id})", self.names.name(id));
        if let Some(stats) = card.get("stats").filter(|stats| stats.is_object()) {
            let count = |key: &str| stats.get(key).and_then(Value::as_u64).unwrap_or(0);
            line.push_str(&format!(
                ": подписчиков {}, подписок {}, постов {}, лайков {}",
                count("followers"),
                count("following"),
                count("posts"),
                count("likes")
            ));
        }
        self.say(&line).await?;
        let posts = card.get("posts").and_then(Value::as_array).cloned();
        for post in posts.iter().flatten() {
            let line = format!("  {}", self.names.post(post));
            self.say(&line).await?;
        }
        if posts.as_ref().is_some_and(Vec::is_empty) {
            self.say("  постов пока нет").await?;
        }
        let partial: Vec<&str> = card
            .get("partial")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !partial.is_empty() {
            let line = format!("  не успели прийти: {}", partial.join(", "));
            self.say(&line).await?;
        }
        Ok(())
    }

    async fn commented(&mut self, answer: Answer) -> io::Result<()> {
        match answer {
            Answer::Ok(reply) => {
                let post = self.names.alias(text(&reply, "post").unwrap_or_default());
                let id = text(&reply, "id").unwrap_or_default();
                self.say(&format!("комментарий {id} к посту [{post}]"))
                    .await
            }
            Answer::Failed(error) => self.say(&error).await,
        }
    }

    async fn bell(&mut self, answer: Answer) -> io::Result<()> {
        let reply = match answer {
            Answer::Ok(reply) => reply,
            Answer::Failed(error) => return self.say(&error).await,
        };
        let unread = reply.get("unread").and_then(Value::as_u64).unwrap_or(0);
        let notices = reply
            .get("notices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if notices.is_empty() {
            return self.say("колокольчик пуст").await;
        }
        let mut aside = Vec::new();
        for notice in &notices {
            if let Some(actor) = text(notice, "actor") {
                self.learn(actor, &mut aside).await?;
            }
        }
        self.say(&format!("колокольчик: непрочитанных {unread}"))
            .await?;
        for notice in &notices {
            let line = self.names.notice(notice);
            self.say(&line).await?;
        }
        for raw in aside {
            self.show(&raw).await?;
        }
        Ok(())
    }

    async fn marked(&mut self, answer: Answer) -> io::Result<()> {
        match answer {
            Answer::Ok(reply) => {
                let marked = reply.get("marked").and_then(Value::as_u64).unwrap_or(0);
                self.say(&format!("прочитано уведомлений: {marked}")).await
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
        let (posts, stale) = match answer {
            Answer::Ok(reply) => {
                self.feed_next = text(&reply, "next").map(str::to_string);
                (
                    reply
                        .get("posts")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                    reply.get("stale").and_then(Value::as_bool).unwrap_or(false),
                )
            }
            Answer::Failed(error) => return self.say(&error).await,
        };
        if stale {
            self.say("(профили могут быть устаревшими: сервис профилей не ответил)")
                .await?;
        }
        if posts.is_empty() {
            return self.say("лента пуста").await;
        }
        for post in &posts {
            let line = self.names.post(post);
            self.say(&line).await?;
        }
        if self.feed_next.is_some() {
            self.say("  … дальше: more").await?;
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

    async fn show(&mut self, raw: &str) -> io::Result<()> {
        let mut waiting = VecDeque::from([raw.to_string()]);
        while let Some(raw) = waiting.pop_front() {
            let mut aside = Vec::new();
            self.render(&raw, &mut aside).await?;
            waiting.extend(aside);
        }
        Ok(())
    }

    async fn learn(&mut self, id: &str, aside: &mut Vec<String>) -> io::Result<()> {
        if id.is_empty() || self.names.knows(id) {
            return Ok(());
        }
        let ask = json!({ "target": id });
        if let Answer::Ok(card) = self.exchange_aside("users.profile", &ask, aside).await? {
            if let Some(user) = card.get("user") {
                self.names.remember_user(user);
            }
        }
        Ok(())
    }

    async fn render(&mut self, raw: &str, aside: &mut Vec<String>) -> io::Result<()> {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return self
                .say(&format!("непонятная строка от шлюза: {raw}"))
                .await;
        };
        if let Some(event) = text(&value, "event") {
            let data = value.get("data").cloned().unwrap_or(Value::Null);
            if event == "message" {
                if let Some(seq) = data.get("seq").and_then(Value::as_u64) {
                    self.last_seq = Some(self.last_seq.map_or(seq, |last| last.max(seq)));
                }
            }
            for field in ["by", "author", "from"] {
                if let Some(id) = text(&data, field) {
                    self.learn(id, aside).await?;
                }
            }
            let line = self.names.push(&value);
            return self.say(&line).await;
        }
        match value.get("error") {
            Some(error)
                if value.get("id") == Some(&Value::Null)
                    && text(error, "code") == Some("expired") =>
            {
                self.say("срок входа вышел").await
            }
            Some(error) => {
                let line = failure(error, None);
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
        self.last_rid = reply.header("x-request-id").map(str::to_string);
        if reply.status != 401 || token.is_none() {
            return Ok(answer(reply));
        }
        if !self.renew().await? {
            return Ok(Answer::Failed(String::from("сессия закрыта: войди заново")));
        }
        let token = self.token.clone();
        let again = self
            .web
            .request(method, path, body, token.as_deref())
            .await?;
        Ok(answer(again))
    }

    async fn signed(&mut self, method: &str, path: &str, body: Option<&str>) -> io::Result<Answer> {
        let token = self.token.clone();
        let reply = self
            .web
            .request(method, path, body, token.as_deref())
            .await?;
        self.last_rid = reply.header("x-request-id").map(str::to_string);
        self.remember_refresh(&reply);
        Ok(answer(reply))
    }

    async fn guest(&mut self) -> io::Result<()> {
        if self.ws.is_none() && self.token.is_none() {
            if let Ok(ws) = connect_with(&self.dialer, &self.addr, "", 1).await {
                self.ws = Some(ws);
            }
        }
        Ok(())
    }

    async fn call(&mut self, method: &str, data: Value) -> io::Result<Answer> {
        let answer = self.exchange(method, &data).await?;
        let Answer::Failed(code) = &answer else {
            return Ok(answer);
        };
        if code.starts_with("ошибка (unauthorized)") && self.token.is_none() {
            return Ok(Answer::Failed(SIGN_IN.to_string()));
        }
        if code.starts_with("ошибка (expired)") && self.renew().await? {
            return self.exchange(method, &data).await;
        }
        Ok(answer)
    }

    async fn exchange(&mut self, method: &str, data: &Value) -> io::Result<Answer> {
        let mut aside = Vec::new();
        let answer = self.exchange_aside(method, data, &mut aside).await?;
        for raw in aside {
            self.show(&raw).await?;
        }
        Ok(answer)
    }

    async fn exchange_aside(
        &mut self,
        method: &str,
        data: &Value,
        aside: &mut Vec<String>,
    ) -> io::Result<Answer> {
        self.guest().await?;
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
                aside.push(raw);
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                aside.push(raw);
                continue;
            }
            self.last_rid = value
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(error) = value.get("error") {
                return Ok(Answer::Failed(failure(error, None)));
            }
            return Ok(Answer::Ok(value.get("ok").cloned().unwrap_or(Value::Null)));
        }
    }
}

fn fresh_key() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("cli-{nanos:x}-{}", SERIAL.fetch_add(1, Ordering::Relaxed))
}

fn draft(body: &str) -> Value {
    let mut words = Vec::new();
    let mut tags = Vec::new();
    let mut links = Vec::new();
    for word in body.split_whitespace() {
        if let Some(tag) = word.strip_prefix('#').filter(|tag| !tag.is_empty()) {
            tags.push(tag.to_string());
        } else if word.starts_with("http://") || word.starts_with("https://") {
            links.push(json!({ "url": word }));
        } else {
            words.push(word);
        }
    }
    json!({ "text": words.join(" "), "tags": tags, "links": links })
}

fn answer(reply: Reply) -> Answer {
    let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    if (200..300).contains(&reply.status) {
        return Answer::Ok(value);
    }
    match value.get("error") {
        Some(error) if error.is_object() => Answer::Failed(failure(error, Some(reply.status))),
        _ => Answer::Failed(format!("ошибка HTTP {}: {}", reply.status, reply.body)),
    }
}

const SERVER_SIDE: [&str; 3] = ["internal", "unavailable", "timeout"];

fn failure(error: &Value, status: Option<u16>) -> String {
    let code = text(error, "code").unwrap_or("?");
    let kind = match status {
        Some(status) => format!("{code}, HTTP {status}"),
        None => code.to_string(),
    };
    let mut line = format!(
        "ошибка ({kind}): {}",
        text(error, "message").unwrap_or_default()
    );
    let details = error.get("details").and_then(Value::as_array);
    for detail in details.into_iter().flatten() {
        let field = text(detail, "field").filter(|field| !field.is_empty());
        line.push_str(&format!(
            "\n  {}: {}",
            field.unwrap_or("тело"),
            text(detail, "code").unwrap_or("?")
        ));
        if let Some(message) = text(detail, "message").filter(|message| !message.is_empty()) {
            line.push_str(&format!(" ({message})"));
        }
    }
    if let Some(rid) = text(error, "request_id").filter(|_| SERVER_SIDE.contains(&code)) {
        line.push_str(&format!("\n  номер запроса для поддержки: {rid}"));
    }
    line
}
