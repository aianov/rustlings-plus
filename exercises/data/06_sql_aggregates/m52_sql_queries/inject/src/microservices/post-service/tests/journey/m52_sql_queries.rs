use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::common::now_micros;
use post_service::repository::{PostQueries, PostRepository};
use post_service::{Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_queries_{name}_{}", std::process::id())).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m52"), now_micros())
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: vec![post_service::Tag::try_from("sql").expect("тег")],
        links: Vec::new(),
    }
}

#[tokio::test]
async fn hydration_marks_likes_and_hides_authors() {
    let config = database("hydrate");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool, 4).await.expect("репозиторий");
    let (alice, bob) = (UserId(1), UserId(2));
    let first = posts
        .insert(alice, String::from("alice"), draft("первый"), &cause())
        .await
        .expect("пост");
    let second = posts
        .insert(alice, String::from("alice"), draft("второй"), &cause())
        .await
        .expect("пост");
    posts
        .like_if_absent(bob, first.id, &cause())
        .await
        .expect("лайк")
        .expect("пост есть");
    let seen = posts
        .by_ids(Some(bob), vec![second.id, first.id], Vec::new())
        .await;
    let marks: Vec<(String, bool)> = seen
        .iter()
        .map(|(post, liked)| (post.text.clone(), *liked))
        .collect();
    assert_eq!(
        marks,
        [
            (String::from("второй"), false),
            (String::from("первый"), true)
        ],
        "порядок как в запросе, «ты лайкнул» из LEFT JOIN"
    );
    assert_eq!(seen[0].0.tags, ["sql"]);
    assert!(
        posts
            .by_ids(Some(bob), vec![first.id], vec![alice])
            .await
            .is_empty(),
        "скрытый автор отсекается анти-join"
    );
    assert_eq!(posts.author_totals(alice).await, (2, 1));
    assert_eq!(
        posts.liked(bob, vec![first.id, second.id]).await,
        [first.id]
    );
    let days = posts.daily(3, now_micros()).await;
    assert_eq!(days.len(), 3, "пустые дни тоже есть: generate_series");
    assert_eq!((days[0].1, days[0].2), (0, 0));
    assert_eq!((days[2].1, days[2].2), (2, 1));
    let _ = drop_schema(&config).await;
}
