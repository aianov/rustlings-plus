use std::sync::Arc;

use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::common::now_micros;
use post_service::repository::{PinQueries, Pinned, PostRepository};
use post_service::{CommentText, Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_pins_{name}_{}", std::process::id())).pool(8)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m54"), now_micros())
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

#[tokio::test]
async fn pins_never_exceed_three_even_in_a_race() {
    let config = database("race");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    apply(&pool, &MIGRATIONS).await.expect("повтор миграций");
    let posts = Arc::new(PgPosts::open(pool.clone(), 6).await.expect("репозиторий"));
    let (alice, bob) = (UserId(1), UserId(2));
    let mut ids = Vec::new();
    for n in 0..6 {
        let post = posts
            .insert(
                alice,
                String::from("alice"),
                draft(&format!("пост {n}")),
                &cause(),
            )
            .await
            .expect("пост");
        ids.push(post.id);
    }
    let mut racers = Vec::new();
    for id in ids.clone() {
        let posts = Arc::clone(&posts);
        racers.push(tokio::spawn(
            async move { posts.pin(alice, id, true).await },
        ));
    }
    let mut won = 0;
    let mut full = 0;
    for racer in racers {
        match racer.await.expect("задача").expect("запрос") {
            Pinned::Changed(true) => won += 1,
            Pinned::Full => full += 1,
            other => panic!("неожиданный исход {other:?}"),
        }
    }
    assert_eq!(
        (won, full),
        (3, 3),
        "замок строки автора не даёт write skew"
    );
    assert_eq!(posts.pinned(alice).await.len(), 3);
    assert_eq!(
        posts.pin(bob, ids[0], true).await.expect("запрос"),
        Pinned::NotAuthor
    );
    let pinned = posts.pinned(alice).await;
    assert_eq!(
        posts.pin(alice, pinned[0], false).await.expect("открепить"),
        Pinned::Changed(true)
    );
    let free = ids
        .iter()
        .find(|id| !pinned.contains(id))
        .copied()
        .expect("свободный");
    assert_eq!(
        posts.pin(alice, free, true).await.expect("закрепить"),
        Pinned::Changed(true),
        "после открепления место освободилось"
    );
    let comment = posts
        .comment(
            bob,
            ids[0],
            CommentText::try_from("лайкни").expect("текст"),
            &cause(),
        )
        .await
        .expect("комментарий")
        .expect("пост есть");
    let first = posts
        .like_comment(alice, comment, now_micros())
        .await
        .expect("лайк");
    let again = posts
        .like_comment(alice, comment, now_micros())
        .await
        .expect("повтор");
    assert_eq!(
        (first, again),
        (Some((true, 1)), Some((false, 1))),
        "ON CONFLICT"
    );
    let broken = sqlx::query("UPDATE posts SET status = 'weird' WHERE id = $1")
        .bind(i64::try_from(ids[1].0).expect("id"))
        .execute(&pool)
        .await;
    let code = broken.err().and_then(|error| {
        error
            .as_database_error()
            .and_then(|db| db.code().map(|c| c.to_string()))
    });
    assert_eq!(code.as_deref(), Some("23514"), "статус держит CHECK");
    let _ = drop_schema(&config).await;
}
