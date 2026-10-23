use std::sync::Arc;

use contracts::{PostId, UserId};
use feed_service::common::{Change, Consumed};
use feed_service::repository::{FeedRepository, ScyllaFeed};
use resp_client::{Client, RedisConfig, key};
use store_engine::{StoreConfig, connect, drop_keyspace};

fn posted(post: u64, author: u64, at: u64) -> Change {
    Change::Posted {
        post: PostId(post),
        author: UserId(author),
        author_name: format!("u{author}"),
        text: format!("пост {post}"),
        tags: Vec::new(),
        links: Vec::new(),
        at,
    }
}

#[tokio::test]
async fn feeds_live_in_scylla_with_a_redis_cache() {
    let id = std::process::id();
    let config = StoreConfig::from_env(&format!("t_feed_{id}"));
    let _ = drop_keyspace(&config).await;
    let session = connect(&config)
        .await
        .expect("Scylla курса на 127.0.0.1:59042");
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let (alice, bob, carol) = (
        UserId(u64::from(id) * 10 + 1),
        UserId(u64::from(id) * 10 + 2),
        UserId(u64::from(id) * 10 + 3),
    );
    redis
        .del(&[key("feed", "user", bob.0), key("feed", "user", alice.0)])
        .await
        .expect("чисто");
    let feed: Arc<dyn FeedRepository> = Arc::new(
        ScyllaFeed::open(session, Some(redis.clone()))
            .await
            .expect("схема ленты"),
    );
    let follow = Change::Followed {
        follower: bob,
        followee: alice,
    };
    assert_eq!(
        feed.apply(format!("u_{}", bob.0), 1, follow.clone()).await,
        Consumed::Applied
    );
    assert_eq!(
        feed.apply(format!("u_{}", bob.0), 1, follow).await,
        Consumed::Duplicate,
        "повтор события отсекается"
    );
    let base = u64::from(id) * 1_000;
    for n in 1..=3 {
        feed.apply(
            format!("p_{}", base + n),
            1,
            posted(base + n, alice.0, n * 10),
        )
        .await;
    }
    feed.apply(format!("p_{}", base + 9), 1, posted(base + 9, carol.0, 99))
        .await;
    let (page, next) = feed.after(bob, None, 2).await;
    let ids: Vec<u64> = page.iter().map(|item| item.post.0).collect();
    assert_eq!(
        ids,
        [base + 3, base + 2],
        "строка на подписчика, свежие первыми, чужие не попали"
    );
    let cached = redis
        .lrange(&key("feed", "user", bob.0), 0, -1)
        .await
        .expect("кэш");
    assert_eq!(cached.len(), 3, "кэш ленты в Redis: LPUSH и LTRIM");
    let (rest, _) = feed.after(bob, next, 10).await;
    assert_eq!(
        rest.iter().map(|item| item.post.0).collect::<Vec<_>>(),
        [base + 1]
    );
    feed.apply(
        format!("p_{}", base + 3),
        2,
        Change::Deleted {
            post: PostId(base + 3),
        },
    )
    .await;
    let (page, _) = feed.after(bob, None, 5).await;
    assert!(
        !page.iter().any(|item| item.post.0 == base + 3),
        "удалённый пост выпал"
    );
    let stale = feed
        .apply(
            format!("p_{}", base + 3),
            2,
            Change::Edited {
                post: PostId(base + 3),
                text: String::from("x"),
            },
        )
        .await;
    assert_eq!(
        stale,
        Consumed::Duplicate,
        "версия события не новее: отбрасывается"
    );
    redis
        .del(&[key("feed", "user", bob.0), key("feed", "user", alice.0)])
        .await
        .expect("уборка");
    drop_keyspace(&config).await.expect("keyspace убран");
}
