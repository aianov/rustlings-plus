use std::time::Duration;

use api_gateway::Config;
use contracts::{Changed, PostView, Push};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::support::{Gateway, ok, within};

#[tokio::test]
async fn a_deleted_account_takes_its_posts_out_of_every_feed() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    for text in ["первый", "второй"] {
        let _ = ok!(writer, "posts.create", format!(r#"{{"text": "{text}"}}"#) => PostView);
    }
    stack
        .feed_until(&mut reader, "оба поста alice у bob", |page| {
            page.posts.len() == 2
        })
        .await;
    let (mut keys, stdin) = tokio::io::duplex(4096);
    let (stdout, screen) = tokio::io::duplex(1 << 16);
    let http = stack.http.to_string();
    let client =
        tokio::spawn(async move { social_cli::run(&http, BufReader::new(stdin), stdout).await });
    let mut screen = BufReader::new(screen).lines();
    keys.write_all(b"login alice correct-horse\ndelete account\nquit\n")
        .await
        .expect("ввод");
    let mut seen = Vec::new();
    while let Ok(Some(line)) = within("вывод клиента", screen.next_line()).await {
        seen.push(line);
    }
    within("клиент вышел", client)
        .await
        .expect("задача клиента")
        .expect("сеанс клиента");
    assert!(
        seen.iter()
            .any(|line| line.starts_with("аккаунт удаляется")),
        "клиент сразу получил ответ, сага идёт в фоне: {seen:?}"
    );
    assert!(
        writer.closed().await,
        "заморозка гасит сессии: сокет alice закрыт"
    );
    stack
        .feed_until(
            &mut reader,
            "постов удалённой alice в ленте нет",
            |page| page.posts.is_empty(),
        )
        .await;
    let gone = reader.fails("users.find", r#"{"username": "alice"}"#).await;
    assert_eq!(gone.code, "not_found", "профиля alice больше нет: {gone:?}");
    within("учётная запись alice стёрта", async {
        loop {
            let login = stack
                .web()
                .request(
                    "POST",
                    "/auth/login",
                    Some(r#"{"username": "alice", "password": "correct-horse"}"#),
                    None,
                )
                .await
                .expect("ответ");
            if login.status == 401 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    stack.stop().await;
}

#[tokio::test]
async fn the_digest_arrives_as_a_push() {
    let stack = Gateway::with_digest(Config::default(), Duration::from_millis(500)).await;
    let (alice, mut phone) = stack.member("alice").await;
    let (_, mut fan) = stack.member("bob").await;
    let _ = ok!(fan, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let mut seen = Vec::new();
    let digest = within("сводка пришла пушем", async {
        loop {
            let push = phone.push().await;
            if let Push::Digest { .. } = push {
                return push;
            }
            seen.push(push);
        }
    })
    .await;
    assert_eq!(
        digest,
        Push::Digest {
            to: alice.id,
            unread: 1
        },
        "сводка о непрочитанной подписке: {seen:?}"
    );
    stack.stop().await;
}
