use api_gateway::Config;
use contracts::{Changed, PostView};
use service_runtime::CancellationToken;
use tokio::net::TcpListener;

use crate::support::{Gateway, nats, ok};

#[tokio::test]
async fn the_feed_comes_from_feed_service_and_follows_a_rename() {
    let stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let first = ok!(writer, "posts.create", r#"{"text": "старый пост"}"# => PostView);
    let page = stack
        .feed_until(&mut reader, "пост alice в ленте bob", |page| {
            page.posts.iter().any(|post| post.id == first.id)
        })
        .await;
    assert_eq!(page.posts[0].author_name, "alice");
    let login = stack.login("alice").await;
    let renamed = stack
        .web()
        .request(
            "PATCH",
            "/users/me",
            Some(r#"{"display_name": "Алиса"}"#),
            Some(&login.token),
        )
        .await
        .expect("PATCH /users/me");
    assert_eq!(renamed.status, 200, "{renamed:?}");
    let page = stack
        .feed_until(&mut reader, "новое имя alice", |page| {
            page.posts.iter().any(|post| post.author_name == "Алиса")
        })
        .await;
    assert_eq!(
        page.posts
            .iter()
            .map(|post| (post.id, post.author_name.as_str()))
            .collect::<Vec<_>>(),
        [(first.id, "Алиса")],
        "лента собрана feed-service из событий: новое имя видно и в старом посте"
    );
    stack.stop().await;
}

#[tokio::test]
async fn a_restarted_feed_service_rebuilds_the_feed() {
    let mut stack = Gateway::start(Config::default()).await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", format!(r#"{{"target": "{}"}}"#, alice.id) => Changed);
    let post = ok!(writer, "posts.create", r#"{"text": "до перезапуска"}"# => PostView);
    stack
        .feed_until(&mut reader, "пост до перезапуска", |page| {
            !page.posts.is_empty()
        })
        .await;
    stack.halt("feed-service").await;
    let listener = TcpListener::bind(&stack.services.feed)
        .await
        .expect("адрес feed-service свободен после остановки");
    let config = feed_service::Config {
        nats: nats("feed-service"),
        topics: stack.topics.clone(),
        ..feed_service::Config::default()
    };
    let stop = CancellationToken::new();
    let revived = tokio::spawn(feed_service::run(listener, config, stop.clone()));
    let page = stack
        .feed_until(
            &mut reader,
            "лента после перезапуска feed-service",
            |page| page.posts.iter().any(|seen| seen.id == post.id),
        )
        .await;
    assert_eq!(
        page.posts.len(),
        1,
        "пустой feed-service прочитал stream с начала: пост на месте и не задвоен"
    );
    stop.cancel();
    revived
        .await
        .expect("задача feed-service")
        .expect("feed-service остановился");
    stack.stop().await;
}
