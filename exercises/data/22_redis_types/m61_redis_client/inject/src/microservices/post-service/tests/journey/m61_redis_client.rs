use contracts::{PostId, UserId};
use post_service::client::{RedisHeat, tags_key, views_key};
use post_service::service::Heat;
use resp_client::{Client, RedisConfig};

const HOUR: u64 = 60 * 60 * 1_000_000;

#[tokio::test]
async fn unique_views_and_hourly_tags_live_in_redis() {
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let heat = RedisHeat::new(redis.clone());
    let base = u64::from(std::process::id()) * 1_000;
    let (post, other) = (PostId(base + 1), PostId(base + 2));
    let now = (u64::from(std::process::id()) + 400_000) * HOUR;
    let mut keys = vec![views_key(post), views_key(other)];
    for hour in 0..30 {
        keys.push(tags_key(now / HOUR - hour));
    }
    redis.del(&keys).await.expect("чисто");
    for viewer in [1, 2, 3, 2, 1, 4] {
        heat.view(post, UserId(viewer)).await;
    }
    let last = heat.view(post, UserId(4)).await;
    assert_eq!(last, 4, "HyperLogLog считает уникальных");
    assert_eq!(heat.views(vec![post, other]).await, [4, 0]);
    heat.tagged(vec![String::from("rust"), String::from("sql")], now)
        .await;
    heat.tagged(vec![String::from("rust")], now - HOUR).await;
    heat.tagged(vec![String::from("go")], now - 2 * HOUR).await;
    heat.tagged(vec![String::from("old")], now - 30 * HOUR)
        .await;
    let top = heat.top_tags(3, now).await;
    assert_eq!(top[0], (String::from("rust"), 2), "сумма по часам");
    assert_eq!(top.len(), 3);
    assert!(
        !top.iter().any(|(tag, _)| tag == "old"),
        "старше суток не считается"
    );
    redis.del(&keys).await.expect("уборка");
}
