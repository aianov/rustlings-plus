use std::time::Duration;

use api_gateway::{Config, RateLimits};
use contracts::{Changed, ErrorCode, FeedPage, PostView};
use social_cli::Web;

use crate::support::{Gateway, ok, within};

fn target(id: impl std::fmt::Display) -> String {
    format!(r#"{{"target": "{id}"}}"#)
}

#[tokio::test]
async fn the_same_key_makes_one_post() {
    let stack = Gateway::start(Config::default()).await;
    let (_, mut phone) = stack.member("alice").await;
    let mut laptop = stack.device("alice").await;
    let body = r#"{"text": "один раз", "idempotency_key": "k-1"}"#;
    let first = phone.send("posts.create", body).await;
    let second = laptop.send("posts.create", body).await;
    let from_phone = phone.reply(first).await;
    let from_laptop = laptop.reply(second).await;
    let id = |raw: &str| -> PostView {
        match contracts::decode::<contracts::Response<PostView>>(raw) {
            Ok(contracts::Response {
                outcome: contracts::Outcome::Ok(post),
                ..
            }) => post,
            other => panic!("ждали пост: {other:?}"),
        }
    };
    let (a, b) = (id(&from_phone), id(&from_laptop));
    assert_eq!(
        a.id, b.id,
        "два одновременных запроса с одним ключом это один пост"
    );
    let again = ok!(phone, "posts.create", body.to_string() => PostView);
    assert_eq!(again, a, "повтор отдаёт сохранённый ответ");
    let other = ok!(phone, "posts.create", r#"{"text": "один раз", "idempotency_key": "k-2"}"#.to_string() => PostView);
    assert_ne!(other.id, a.id, "другой ключ это другой пост");
    let plain = ok!(phone, "posts.create", r#"{"text": "без ключа"}"#.to_string() => PostView);
    assert_ne!(plain.id, other.id);
    stack.stop().await;
}

#[tokio::test]
async fn each_user_has_a_token_bucket() {
    let stack = Gateway::start(Config {
        limits: RateLimits {
            user_rate: 1,
            user_burst: 3,
            ..RateLimits::default()
        },
        ..Config::default()
    })
    .await;
    let (alice, mut device) = stack.member("alice").await;
    for _ in 0..3 {
        let _ = ok!(device, "users.find", format!(r#"{{"username": "{}"}}"#, alice.username) => contracts::UserView);
    }
    let refused = device
        .fails(
            "users.find",
            &format!(r#"{{"username": "{}"}}"#, alice.username),
        )
        .await;
    assert_eq!(refused.known(), Some(ErrorCode::Exhausted), "{refused:?}");
    let (_, mut other) = stack.member("bob").await;
    let _ = ok!(other, "users.find", r#"{"username": "bob"}"#.to_string() => contracts::UserView);
    stack.stop().await;
}

#[tokio::test]
async fn an_address_has_a_token_bucket_too() {
    let stack = Gateway::start(Config {
        limits: RateLimits {
            ip_rate: 1,
            ip_burst: 2,
            ..RateLimits::default()
        },
        ..Config::default()
    })
    .await;
    let mut web = Web::new(&stack.http.to_string());
    for _ in 0..2 {
        let reply = within("профиль", web.request("GET", "/users/nobody", None, None))
            .await
            .expect("шлюз отвечает");
        assert_eq!(reply.status, 404);
    }
    let limited = within("профиль", web.request("GET", "/users/nobody", None, None))
        .await
        .expect("шлюз отвечает");
    assert_eq!(limited.status, 429, "{limited:?}");
    assert_eq!(limited.header("retry-after"), Some("1"));
    let health = within("здоровье", web.request("GET", "/health", None, None))
        .await
        .expect("шлюз отвечает");
    assert_eq!(health.status, 200, "проверка здоровья не тратит жетоны");
    stack.stop().await;
}

#[tokio::test]
async fn overload_is_shed_fast_with_retry_after() {
    let stack = Gateway::start(Config {
        limits: RateLimits {
            shed_inflight: 0,
            ..RateLimits::default()
        },
        ..Config::default()
    })
    .await;
    let reply = within(
        "сброс",
        Web::new(&stack.http.to_string()).request("GET", "/users/alice", None, None),
    )
    .await
    .expect("шлюз отвечает");
    assert_eq!(reply.status, 503, "{reply:?}");
    assert_eq!(reply.header("retry-after"), Some("1"));
    stack.stop().await;
}

#[tokio::test]
async fn the_feed_survives_a_dead_user_service_with_stale_profiles() {
    let mut stack = Gateway::start(Config {
        profile_fresh: Duration::from_millis(1),
        ..Config::default()
    })
    .await;
    let (alice, mut writer) = stack.member("alice").await;
    let (_, mut reader) = stack.member("bob").await;
    let _ = ok!(reader, "users.follow", target(alice.id) => Changed);
    let post = ok!(writer, "posts.create", r#"{"text": "переживём"}"#.to_string() => PostView);
    let page = stack
        .feed_until(&mut reader, "пост alice в ленте bob", |page| {
            page.posts.iter().any(|seen| seen.id == post.id)
        })
        .await;
    assert!(!page.stale);
    assert!(
        page.authors.iter().any(|author| author.id == alice.id),
        "{page:?}"
    );
    stack.halt("user-service").await;
    let page = ok!(reader, "posts.feed", "{}".to_string() => FeedPage);
    assert!(
        page.posts.iter().any(|seen| seen.id == post.id),
        "лента открывается без user-service"
    );
    assert!(page.stale, "с пометкой, что профили могут быть устаревшими");
    assert!(
        page.authors.iter().any(|author| author.id == alice.id),
        "профиль alice из устаревшего кэша"
    );
    stack.stop().await;
}
