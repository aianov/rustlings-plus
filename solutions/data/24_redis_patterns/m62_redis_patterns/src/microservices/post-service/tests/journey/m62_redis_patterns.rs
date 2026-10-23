use std::sync::Arc;

use contracts::{Cause, Origin, PostId, UserId};
use db_engine::{DEFAULT_DB_URL, DbConfig, PG_URL_ENV, apply, connect, drop_schema};
use post_service::client::{RedisLikes, pending_key};
use post_service::common::now_micros;
use post_service::repository::PostRepository;
use post_service::{Draft, MIGRATIONS, PgPosts, PostText};
use resp_client::{Client, RedisConfig, cmd, key};

#[tokio::test]
async fn likes_pile_up_in_redis_and_flush_by_rename() {
    let config = DbConfig::new(
        &std::env::var(PG_URL_ENV).unwrap_or_else(|_| DEFAULT_DB_URL.to_string()),
        &format!("t_likes_{}", std::process::id()),
    )
    .pool(2);
    let _ = drop_schema(&config).await;
    let pool = connect(&config)
        .await
        .expect("Postgres курса на 127.0.0.1:55432");
    apply(&pool, &MIGRATIONS).await.expect("миграции");
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    redis.del(&[pending_key()]).await.expect("чисто");
    let likes = Arc::new(RedisLikes::new(
        redis.clone(),
        &format!("t{}", std::process::id()),
    ));
    let posts = PgPosts::open(pool.clone(), 11)
        .await
        .expect("репозиторий")
        .with_pending(likes.clone());
    let cause = || Cause::new(&Origin::new("m62"), now_micros());
    let post = posts
        .insert(
            UserId(1),
            String::from("alice"),
            Draft {
                text: PostText::try_from("лайкайте").expect("текст"),
                tags: Vec::new(),
                links: Vec::new(),
            },
            &cause(),
        )
        .await
        .expect("пост");
    for user in 2..5 {
        posts
            .like_if_absent(UserId(user), post.id, &cause())
            .await
            .expect("лайк")
            .expect("пост есть");
    }
    let repeat = posts
        .like_if_absent(UserId(2), post.id, &cause())
        .await
        .expect("повтор")
        .expect("пост есть");
    assert!(!repeat.changed, "лайк по-прежнему идемпотентен");
    assert_eq!(
        posts.get(post.id).await.map(|post| post.likes),
        Some(0),
        "счётчик ждёт сброса"
    );
    let left = key(
        "posts",
        "likes-flushing",
        format!("crashed-{}", std::process::id()),
    );
    redis
        .query(cmd("HINCRBY").arg(&left).arg(post.id.0).arg(10))
        .await
        .expect("брошенный рабочий ключ");
    let moved = likes.flush(&posts).await.expect("сброс");
    assert!(
        moved >= 13,
        "RENAME рабочего ключа и подбор брошенного: {moved}"
    );
    assert_eq!(posts.get(post.id).await.map(|post| post.likes), Some(13));
    let _ = PostId(0);
    let _ = drop_schema(&config).await;
}
