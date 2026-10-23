use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use api_gateway::Config;
use auth_service::Lifetimes;
use contracts::{Changed, ErrorBody, Outcome, PostView, decode};
use social_cli::run_with;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, duplex};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::support::{DEADLINE, Gateway, ok, within};

fn quick(access: u64) -> auth_service::Config {
    auth_service::Config {
        lifetimes: Lifetimes {
            access,
            leeway: 0,
            ..Lifetimes::default()
        },
        moderators: vec!["moder".to_string()],
        ..auth_service::Config::default()
    }
}

#[tokio::test]
async fn authors_edit_and_moderators_delete_over_websocket() {
    let stack = Gateway::with_auth(Config::default(), quick(900)).await;
    let (_, mut alice) = stack.member("alice").await;
    let (_, mut bob) = stack.member("bobby").await;
    let (_, mut moder) = stack.member("moder").await;
    let post = ok!(alice, "posts.create", r#"{"text": "черновик"}"# => PostView);
    let edit = format!(r#"{{"post": "{}", "text": "чужая правка"}}"#, post.id);
    let refused = bob.fails("posts.edit", &edit).await;
    assert_eq!(refused.code, "forbidden", "{refused:?}");
    let mine = format!(r#"{{"post": "{}", "text": "исправлено"}}"#, post.id);
    let edited = ok!(alice, "posts.edit", &mine => PostView);
    assert_eq!(edited.text, "исправлено");
    let target = format!(r#"{{"post": "{}"}}"#, post.id);
    let denied = bob.fails("posts.delete", &target).await;
    assert_eq!(
        denied.code, "forbidden",
        "обычный пользователь чужое не удаляет"
    );
    let removed = ok!(moder, "posts.delete", &target => Changed);
    assert!(
        removed.changed,
        "роль moderator приехала из токена через шлюз в x-user-role, и post-service её проверил"
    );
    stack.stop().await;
}

#[tokio::test]
async fn an_expired_access_token_has_its_own_code() {
    let stack = Gateway::with_auth(Config::default(), quick(1)).await;
    stack.register("alice").await;
    let login = stack.login("alice").await;
    assert_eq!(login.expires_in, 1);
    sleep(Duration::from_millis(1_200)).await;
    let me = stack
        .web()
        .request("GET", "/users/me", None, Some(&login.token))
        .await
        .expect("ответ");
    let body: ErrorBody = match decode::<Outcome<()>>(&me.body) {
        Ok(Outcome::Error(body)) => body,
        other => panic!("ждали ошибку: {other:?}"),
    };
    assert_eq!(
        (me.status, body.code.as_str()),
        (401, "expired"),
        "просроченный токен это не «войди заново», а «обнови по refresh»"
    );
    let refused = stack.refused(&format!("/ws?token={}", login.token)).await;
    assert!(refused.contains("401"), "{refused}");
    stack.stop().await;
}

struct Window {
    keys: DuplexStream,
    screen: Arc<Mutex<String>>,
    session: JoinHandle<()>,
    reader: JoinHandle<()>,
}

impl Window {
    fn open(stack: &Gateway) -> Window {
        let (keys, typed) = duplex(4096);
        let (shown, mut glass) = duplex(64 * 1024);
        let (http, dialer) = (stack.http.to_string(), stack.dialer.clone());
        let session = tokio::spawn(async move {
            let _ = run_with(&http, dialer, BufReader::new(typed), shown).await;
        });
        let screen = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&screen);
        let reader = tokio::spawn(async move {
            let mut chunk = [0u8; 4096];
            while let Ok(read) = glass.read(&mut chunk).await {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&chunk[..read]).to_string();
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push_str(&text);
            }
        });
        Window {
            keys,
            screen,
            session,
            reader,
        }
    }

    fn text(&self) -> String {
        self.screen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn type_line(&mut self, line: &str) {
        self.keys
            .write_all(format!("{line}\n").as_bytes())
            .await
            .expect("клиент читает ввод");
    }

    async fn wait_for(&self, needle: &str, times: usize) {
        within(&format!("на экране «{needle}»"), async {
            while self.text().matches(needle).count() < times {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
    }

    async fn close(mut self) -> String {
        self.type_line("quit").await;
        let _ = tokio::time::timeout(DEADLINE, self.session).await;
        let _ = tokio::time::timeout(DEADLINE, self.reader).await;
        self.screen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[tokio::test]
async fn the_client_renews_its_token_and_logout_all_closes_the_other_window() {
    let stack = Gateway::with_auth(Config::default(), quick(1)).await;
    let mut laptop = Window::open(&stack);
    laptop.type_line("register alice correct-horse").await;
    laptop.wait_for("зарегистрирован @alice", 1).await;
    laptop.type_line("login alice correct-horse").await;
    laptop.wait_for("привет, @alice", 1).await;
    sleep(Duration::from_millis(1_300)).await;
    laptop.type_line("me").await;
    laptop.wait_for("ты @alice", 1).await;
    let mut phone = Window::open(&stack);
    phone.type_line("login alice correct-horse").await;
    phone.wait_for("привет, @alice", 1).await;
    laptop.type_line("logout all").await;
    laptop.wait_for("вышли на всех устройствах", 1).await;
    phone.type_line("me").await;
    phone.wait_for("сессия закрыта", 1).await;
    let laptop = laptop.close().await;
    let phone = phone.close().await;
    assert!(
        laptop.contains("токен обновлён по refresh"),
        "access прожил секунду, и клиент сам обменял refresh из cookie:\n{laptop}"
    );
    assert!(
        phone.contains("сессия закрыта: войди заново") && !phone.contains("ты @alice"),
        "после «выйти везде» второе окно не продлило сессию:\n{phone}"
    );
    stack.stop().await;
}
