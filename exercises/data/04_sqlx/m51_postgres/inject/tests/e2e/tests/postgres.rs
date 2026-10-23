use contracts::{Changed, PostView, decode};
use db_engine::{DEFAULT_DB_URL, PG_URL_ENV};
use e2e::{Database, Stack, StackConfig};
use test_support::{unique_root, user};

fn database() -> Database {
    Database {
        url: std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string()),
        prefix: unique_root("pg").to_ascii_lowercase(),
    }
}

fn config(database: &Database) -> StackConfig {
    StackConfig {
        database: Some(database.clone()),
        ..StackConfig::default()
    }
}

#[tokio::test]
async fn users_posts_and_likes_survive_a_restart() {
    let database = database();
    let stack = Stack::start_with(config(&database)).await;
    let alice = user().named("alice");
    let bob = user().named("bob");
    let (_, mut writer) = stack.member(&alice).await;
    let (_, mut fan) = stack.member(&bob).await;
    let post: PostView = writer
        .ok("posts.create", r#"{"text": "переживу перезапуск"}"#)
        .await;
    let liked: Changed = fan
        .ok("posts.like", &format!(r#"{{"post": "{}"}}"#, post.id))
        .await;
    assert!(liked.changed);
    stack.stop().await;
    let again = Stack::start_with(config(&database)).await;
    let login = again.login(&alice).await;
    assert!(!login.token.is_empty(), "учётка alice в Postgres");
    let found = again
        .request("GET", &format!("/users/{}", alice.username), None, None)
        .await;
    assert_eq!(found.status, 200, "профиль alice в Postgres: {found:?}");
    let shown = again
        .request("GET", &format!("/posts/{}", post.id), None, None)
        .await;
    assert_eq!(shown.status, 200, "{shown:?}");
    let shown: PostView = decode(&shown.body).expect("пост разбирается");
    assert_eq!(
        (shown.text.as_str(), shown.likes),
        ("переживу перезапуск", 1),
        "пост и лайк на месте"
    );
    again.stop().await;
    database.drop().await;
}
