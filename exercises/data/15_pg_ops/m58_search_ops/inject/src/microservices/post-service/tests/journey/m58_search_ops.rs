use contracts::{Cause, Origin, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, connect, drop_schema, migrate};
use post_service::common::now_micros;
use post_service::repository::{FullText, PostRepository};
use post_service::{Draft, MIGRATIONS, PgPosts, PostText};

fn database(name: &str) -> DbConfig {
    let url = std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string());
    DbConfig::new(&url, &format!("t_fts_{name}_{}", std::process::id())).pool(2)
}

fn cause() -> Cause {
    Cause::new(&Origin::new("m58"), now_micros())
}

fn draft(text: &str) -> Draft {
    Draft {
        text: PostText::try_from(text).expect("текст"),
        tags: Vec::new(),
        links: Vec::new(),
    }
}

#[tokio::test]
async fn full_text_ranks_highlights_and_trends_refresh() {
    let config = database("find");
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    migrate(&pool, &MIGRATIONS).await.expect("миграции");
    let posts = PgPosts::open(pool.clone(), 10).await.expect("репозиторий");
    let (alice, bob) = (UserId(1), UserId(2));
    let cats = posts
        .insert(
            alice,
            String::from("alice"),
            draft("Кошки любят спать на солнце, кошки ленивые"),
            &cause(),
        )
        .await
        .expect("пост");
    let dog = posts
        .insert(
            alice,
            String::from("alice"),
            draft("Собака спит у кошки под боком"),
            &cause(),
        )
        .await
        .expect("пост");
    posts
        .insert(
            bob,
            String::from("bob"),
            draft("Про погоду и ничего больше"),
            &cause(),
        )
        .await
        .expect("пост");
    let hits = posts.find(String::from("кошка"), 10).await.expect("поиск");
    let ids: Vec<_> = hits.iter().map(|hit| hit.post.id).collect();
    assert_eq!(
        ids,
        [cats.id, dog.id],
        "словоформы и ранг: где кошек больше, тот выше"
    );
    assert!(
        hits[0].headline.contains("«Кошки»"),
        "подсветка: {}",
        hits[0].headline
    );
    let excluded = posts
        .find(String::from("кошка -собака"), 10)
        .await
        .expect("поиск");
    assert_eq!(
        excluded.iter().map(|hit| hit.post.id).collect::<Vec<_>>(),
        [cats.id],
        "websearch_to_tsquery понимает минус"
    );
    posts
        .like_if_absent(bob, dog.id, &cause())
        .await
        .expect("лайк")
        .expect("пост есть");
    assert!(
        posts.trends(10).await.is_empty(),
        "представление ещё не пересчитано"
    );
    posts.refresh_trends().await.expect("REFRESH CONCURRENTLY");
    assert_eq!(posts.trends(10).await, [(dog.id, 1)]);
    let _ = drop_schema(&config).await;
}
