use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::common::Link;
use post_service::repository::{DraftRepository, OutboxRepository, PostRepository};
use post_service::{Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_posts_{name}_{}", std::process::id())).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m51"), 1_000)
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: vec![post_service::Tag::try_from("rust").expect("тег")],
        links: vec![Link {
            url: String::from("https://example.com"),
            title: Some(String::from("Пример")),
        }],
    }
}

#[tokio::test]
async fn posts_and_their_events_commit_together() {
    let config = database("live");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool.clone(), 3).await.expect("репозиторий");
    let (alice, bob) = (UserId(1), UserId(2));
    let first = posts
        .insert(alice, String::from("alice"), draft("привет"), &cause())
        .await
        .expect("пост");
    assert_eq!(first.tags, ["rust"]);
    let stored = posts.get(first.id).await.expect("читается");
    assert_eq!(stored.links[0].title.as_deref(), Some("Пример"));
    let liked = posts
        .like_if_absent(bob, first.id, &cause())
        .await
        .expect("лайк")
        .expect("пост есть");
    assert!(liked.changed);
    let again = posts
        .like_if_absent(bob, first.id, &cause())
        .await
        .expect("повтор")
        .expect("пост есть");
    assert!(!again.changed);
    assert_eq!(posts.get(first.id).await.map(|post| post.likes), Some(1));
    let comment = posts
        .comment(
            bob,
            first.id,
            post_service::CommentText::try_from("класс").expect("текст"),
            &cause(),
        )
        .await
        .expect("комментарий")
        .expect("пост есть");
    assert_eq!(posts.comment_owners(comment).await, Some((bob, alice)));
    let kinds: Vec<String> = posts
        .unpublished(10)
        .await
        .into_iter()
        .map(|row| row.event.kind)
        .collect();
    assert_eq!(
        kinds,
        ["post.created", "post.liked", "post.commented"],
        "пост и событие пишутся одной транзакцией"
    );
    let draft_id = posts
        .save_draft(alice, String::from("черновик"), &cause())
        .await
        .expect("черновик");
    assert_eq!(
        posts.draft(draft_id).await.map(|(owner, _)| owner),
        Some(alice)
    );
    assert!(posts.delete(first.id, &cause()).await.expect("удаление"));
    assert!(posts.get(first.id).await.is_none());
    let reopened = PgPosts::open(pool.clone(), 3)
        .await
        .expect("после перезапуска");
    let second = reopened
        .insert(alice, String::from("alice"), draft("ещё"), &cause())
        .await
        .expect("пост");
    assert!(
        second.id.0 > first.id.0,
        "генератор продолжает за последним id из базы"
    );
    assert_eq!(
        reopened.by_authors(vec![alice], 0, 10).await.len(),
        1,
        "удалённый не виден"
    );
    pool.close().await;
    drop_schema(&config).await.expect("схема убрана");
}
