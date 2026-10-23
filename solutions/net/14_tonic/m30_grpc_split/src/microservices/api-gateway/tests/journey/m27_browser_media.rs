use std::net::SocketAddr;
use std::path::PathBuf;

use api_gateway::Config;
use contracts::{Login, UserView, decode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::support::{Gateway, within};

const APP: &str = "https://app.example";

#[derive(Debug)]
struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.head.split("\r\n").skip(1).find_map(|line| {
            let (own, value) = line.split_once(':')?;
            own.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.head
            .split("\r\n")
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .filter(|(own, _)| own.trim().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim())
            .collect()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

struct Wire {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Wire {
    async fn open(addr: SocketAddr) -> Wire {
        let stream = within("подключение к HTTP", TcpStream::connect(addr))
            .await
            .expect("HTTP-порт шлюза принимает соединения");
        Wire {
            stream,
            buf: Vec::new(),
        }
    }

    async fn fill(&mut self) -> bool {
        let mut chunk = [0u8; 8192];
        match within("ответ HTTP", self.stream.read(&mut chunk)).await {
            Ok(0) | Err(..) => false,
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                true
            }
        }
    }

    async fn call(
        &mut self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Reply {
        let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: test\r\n");
        for (name, value) in headers {
            raw.push_str(&format!("{name}: {value}\r\n"));
        }
        if !body.is_empty() || method == "PUT" || method == "POST" {
            raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        raw.push_str("\r\n");
        let mut bytes = raw.into_bytes();
        bytes.extend_from_slice(body);
        self.stream.write_all(&bytes).await.expect("запрос уходит");
        let end = loop {
            if let Some(end) = self.buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break end;
            }
            assert!(
                self.fill().await,
                "шлюз закрыл соединение, не ответив на {method} {path}"
            );
        };
        let head =
            String::from_utf8(self.buf.drain(..end + 4).collect()).expect("заголовки в UTF-8");
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or_else(|| panic!("кривая строка статуса: {head:?}"));
        let mut reply = Reply {
            status,
            head,
            body: Vec::new(),
        };
        let length = match reply.header("content-length") {
            Some(value) if method != "HEAD" && status != 304 => {
                value.parse().expect("Content-Length числом")
            }
            _ => 0,
        };
        while self.buf.len() < length {
            assert!(self.fill().await, "шлюз закрыл соединение посреди тела");
        }
        reply.body = self.buf.drain(..length).collect();
        reply
    }
}

fn cors_config() -> Config {
    Config {
        cors_origins: vec![APP.to_string()],
        ..Config::default()
    }
}

#[tokio::test]
async fn cors_for_the_allowed_front_only() {
    let gateway = Gateway::start(cors_config()).await;
    let mut wire = Wire::open(gateway.http).await;
    let preflight = wire
        .call(
            "OPTIONS",
            "/users/me",
            &[
                ("Origin", APP),
                ("Access-Control-Request-Method", "GET"),
                ("Access-Control-Request-Headers", "authorization"),
            ],
            b"",
        )
        .await;
    assert_eq!(
        preflight.status, 204,
        "предзапрос к закрытому /users/me отвечает CORS-слой снаружи авторизации, без 401: {preflight:?}"
    );
    assert_eq!(preflight.header("access-control-allow-origin"), Some(APP));
    assert_eq!(
        preflight.header("access-control-allow-credentials"),
        Some("true")
    );
    let evil = wire
        .call(
            "OPTIONS",
            "/users/me",
            &[
                ("Origin", "https://evil.example"),
                ("Access-Control-Request-Method", "GET"),
            ],
            b"",
        )
        .await;
    assert_eq!(evil.status, 403, "чужой origin не получает разрешения");
    let simple = wire.call("GET", "/health", &[("Origin", APP)], b"").await;
    assert_eq!(
        (simple.status, simple.header("access-control-allow-origin")),
        (200, Some(APP)),
        "простой запрос с разрешённого origin"
    );
    assert!(
        simple
            .all("vary")
            .iter()
            .any(|value| value.to_ascii_lowercase().contains("origin")),
        "ответ зависит от Origin, кешам нужен Vary: Origin"
    );
    let other = wire
        .call("GET", "/health", &[("Origin", "https://evil.example")], b"")
        .await;
    assert!(
        other.header("access-control-allow-origin").is_none(),
        "чужой сайт ответ не прочитает"
    );
    let missing = wire
        .call("GET", "/users/nobody", &[("Origin", APP)], b"")
        .await;
    assert_eq!(
        (
            missing.status,
            missing.header("access-control-allow-origin")
        ),
        (404, Some(APP)),
        "ошибки тоже читаемы фронтом"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn login_sets_a_session_cookie_and_logout_kills_it() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let mut auth = Wire::open(gateway.auth).await;
    let mut wire = Wire::open(gateway.http).await;
    let login = auth
        .call(
            "POST",
            "/auth/login",
            &[("Content-Type", "application/json")],
            br#"{"username": "alice"}"#,
        )
        .await;
    assert_eq!(login.status, 200);
    let token = decode::<Login>(&login.text())
        .expect("ответ входа разбирается")
        .token;
    let cookie = login
        .header("set-cookie")
        .expect("вход ставит cookie")
        .to_string();
    assert!(
        cookie.starts_with(&format!("session={token};"))
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Lax")
            && cookie.contains("Path=/"),
        "токен ещё и в cookie: HttpOnly (скрипт страницы его не прочитает), SameSite=Lax, Path=/: {cookie}"
    );
    let session = format!("session={token}");
    let me = wire
        .call("GET", "/users/me", &[("Cookie", &session)], b"")
        .await;
    assert_eq!(
        me.status, 200,
        "браузер ходит с cookie, без заголовка Authorization: {me:?}"
    );
    let user: UserView = decode(&me.text()).expect("профиль разбирается");
    assert_eq!(user.username, "alice");

    let logout = auth
        .call("POST", "/auth/logout", &[("Cookie", &session)], b"")
        .await;
    assert_eq!(logout.status, 204, "выход: {logout:?}");
    let erased = logout.header("set-cookie").expect("выход стирает cookie");
    assert!(
        erased.starts_with("session=;") && erased.contains("Max-Age=0"),
        "cookie стирается через Max-Age=0: {erased}"
    );
    let after = wire
        .call("GET", "/users/me", &[("Cookie", &session)], b"")
        .await;
    assert_eq!(
        after.status, 401,
        "после выхода токен из cookie не работает"
    );
    let bearer = format!("Bearer {token}");
    let revoked = wire
        .call("GET", "/users/me", &[("Authorization", &bearer)], b"")
        .await;
    assert_eq!(
        revoked.status, 401,
        "токен отозван, а не только cookie стёрта"
    );
    let refused = gateway.refused(&format!("/ws?token={token}")).await;
    assert!(
        refused.contains("401"),
        "отозванный токен не открывает и WebSocket: {refused}"
    );
    gateway.stop().await;
}

#[tokio::test]
async fn profile_answers_304_when_nothing_changed() {
    let gateway = Gateway::start(Config::default()).await;
    gateway.register("alice").await;
    let mut wire = Wire::open(gateway.http).await;
    let first = wire.call("GET", "/users/alice", &[], b"").await;
    let tag = first
        .header("etag")
        .expect("у профиля есть ETag")
        .to_string();
    let again = wire
        .call("GET", "/users/alice", &[("If-None-Match", &tag)], b"")
        .await;
    assert_eq!(
        (again.status, again.body.len()),
        (304, 0),
        "тот же ETag: 304 без тела"
    );
    let other = wire
        .call(
            "GET",
            "/users/alice",
            &[("If-None-Match", "\"stale\"")],
            b"",
        )
        .await;
    assert_eq!(other.status, 200, "чужой ETag: полный ответ");
    gateway.stop().await;
}

fn picture(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 251) as u8).collect()
}

#[tokio::test]
async fn avatars_upload_and_ranges() {
    let gateway = Gateway::start(Config {
        avatar_max: 1000,
        ..Config::default()
    })
    .await;
    let alice = gateway.register("alice").await;
    let login = gateway.login("alice").await;
    let bearer = format!("Bearer {}", login.token);
    let auth = [
        ("Authorization", bearer.as_str()),
        ("Content-Type", "image/png"),
    ];
    let mut wire = Wire::open(gateway.http).await;
    let png = picture(300);

    let anonymous = wire
        .call(
            "PUT",
            "/users/me/avatar",
            &[("Content-Type", "image/png")],
            &png,
        )
        .await;
    assert_eq!(
        anonymous.status, 401,
        "загрузка аватара только со своим токеном"
    );
    let text = wire
        .call(
            "PUT",
            "/users/me/avatar",
            &[("Authorization", &bearer), ("Content-Type", "text/plain")],
            b"hello",
        )
        .await;
    assert_eq!(text.status, 415, "аватар это картинка");
    let huge = wire
        .call("PUT", "/users/me/avatar", &auth, &picture(5000))
        .await;
    assert_eq!(huge.status, 413, "аватар больше avatar_max это 413");
    let created = wire.call("PUT", "/users/me/avatar", &auth, &png).await;
    assert_eq!(
        created.status, 201,
        "после 413 соединение живо: тело дочитано, и следующий запрос не путается: {created:?}"
    );
    let replaced = wire.call("PUT", "/users/me/avatar", &auth, &png).await;
    assert_eq!(replaced.status, 204, "повторная загрузка заменяет аватар");

    let path = format!("/avatars/{}", alice.id);
    let full = wire.call("GET", &path, &[], b"").await;
    assert_eq!((full.status, &full.body), (200, &png), "аватар целиком");
    assert_eq!(full.header("content-type"), Some("image/png"));
    assert_eq!(full.header("accept-ranges"), Some("bytes"));
    let tag = full
        .header("etag")
        .expect("у аватара есть ETag")
        .to_string();
    let cached = wire
        .call("GET", &path, &[("If-None-Match", &tag)], b"")
        .await;
    assert_eq!(
        (cached.status, cached.body.len()),
        (304, 0),
        "не изменился: 304"
    );
    let tail = wire
        .call("GET", &path, &[("Range", "bytes=100-")], b"")
        .await;
    assert_eq!(
        (tail.status, tail.header("content-range"), &tail.body[..]),
        (206, Some("bytes 100-299/300"), &png[100..]),
        "докачка с сотого байта: 206 и Content-Range"
    );
    let last = wire
        .call("GET", &path, &[("Range", "bytes=-10")], b"")
        .await;
    assert_eq!(&last.body[..], &png[290..], "последние 10 байт");
    let beyond = wire
        .call("GET", &path, &[("Range", "bytes=999-")], b"")
        .await;
    assert_eq!(
        (beyond.status, beyond.header("content-range")),
        (416, Some("bytes */300")),
        "за концом файла 416"
    );
    let head = wire.call("HEAD", &path, &[], b"").await;
    assert_eq!(
        (head.status, head.header("content-length"), head.body.len()),
        (200, Some("300"), 0),
        "HEAD с длиной и без тела"
    );
    let numeric = wire
        .call("GET", &format!("/avatars/{}", alice.id.0), &[], b"")
        .await;
    assert_eq!(numeric.status, 200, "id числом тоже годится");
    assert_eq!(
        wire.call("GET", "/avatars/u_99", &[], b"").await.status,
        404
    );
    let removed = wire
        .call(
            "DELETE",
            "/users/me/avatar",
            &[("Authorization", &bearer)],
            b"",
        )
        .await;
    assert_eq!(removed.status, 204, "аватар удаляется");
    assert_eq!(
        wire.call("GET", &path, &[], b"").await.status,
        404,
        "удалённого аватара нет"
    );
    gateway.stop().await;
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("m27");
    std::fs::create_dir_all(&dir).expect("папка для файлов теста создаётся");
    dir.join(name)
}

#[tokio::test]
async fn course_client_uploads_and_resumes_downloads() {
    let gateway = Gateway::start(Config::default()).await;
    let source = scratch("avatar.png");
    let target = scratch("alice.avatar");
    let png = picture(5000);
    std::fs::write(&source, &png).expect("картинка пишется");
    let _ = std::fs::remove_file(&target);
    let http = gateway.http.to_string();
    let auth = gateway.auth.to_string();
    let script = format!(
        "register alice\nlogin alice\navatar set {}\navatar get alice {}\nquit\n",
        source.display(),
        target.display()
    );
    let mut screen = Vec::new();
    within(
        "сеанс клиента",
        social_cli::run(&http, &auth, script.as_bytes(), &mut screen),
    )
    .await
    .expect("сеанс проходит");
    let screen = String::from_utf8(screen).expect("клиент пишет UTF-8");
    assert!(
        screen.contains("аватар загружен: 5000 байт") && screen.contains("скачано 5000 байт"),
        "загрузка и скачивание:\n{screen}"
    );
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .expect("скачанный файл открывается");
    file.set_len(1000).expect("файл обрезается");
    drop(file);
    let mut screen = Vec::new();
    let script = format!("avatar get alice {}\nquit\n", target.display());
    within(
        "сеанс клиента",
        social_cli::run(&http, &auth, script.as_bytes(), &mut screen),
    )
    .await
    .expect("сеанс проходит");
    let screen = String::from_utf8(screen).expect("клиент пишет UTF-8");
    assert!(
        screen.contains("докачано 4000 байт с позиции 1000"),
        "клиент докачивает по Range только хвост:\n{screen}"
    );
    assert_eq!(
        std::fs::read(&target).expect("файл читается"),
        png,
        "после докачки файл целый"
    );
    gateway.stop().await;
}
