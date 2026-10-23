use std::time::Duration;

use api_gateway::common::{RateLimits, RedisPresence, RedisProfiles, SharedLimits};
use contracts::{UserId, UserView};
use resp_client::{Client, RedisConfig};

fn limits(burst: u32) -> RateLimits {
    RateLimits {
        user_rate: 1,
        user_burst: burst,
        ..RateLimits::default()
    }
}

#[tokio::test]
async fn gateways_share_limits_presence_and_profile_cache() {
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let base = u64::from(std::process::id()) * 100;
    let first = SharedLimits::new(redis.clone(), limits(3));
    let second = SharedLimits::new(redis.dedicated(), limits(3));
    let user = UserId(base + 1);
    assert!(first.take_user(user).await.is_ok());
    assert!(second.take_user(user).await.is_ok());
    assert!(first.take_user(user).await.is_ok());
    let refused = second.take_user(user).await;
    assert!(refused.is_err(), "ведро общее на все шлюзы");
    assert!(refused.err().is_some_and(|wait| wait > Duration::ZERO));
    let ttl = Duration::from_secs(30);
    let north = RedisPresence::new(redis.clone(), ttl, "north");
    let south = RedisPresence::new(redis.clone(), ttl, "south");
    let (alice, bob) = (UserId(base + 2), UserId(base + 3));
    north.touch(alice).await;
    south.touch(alice).await;
    north.release(alice).await;
    assert!(north.online(alice).await, "чужое присутствие не снимается");
    south.release(alice).await;
    assert!(!south.online(alice).await);
    north.touch(bob).await;
    assert_eq!(
        north.online_many(&[alice, bob]).await,
        [false, true],
        "один конвейер"
    );
    north.release(bob).await;
    let cache = RedisProfiles::new(redis.clone(), Duration::from_secs(60));
    let carol = UserId(base + 4);
    cache.invalidate(carol).await;
    assert_eq!(cache.claim(&[carol]).await, [carol]);
    assert!(
        cache.claim(&[carol]).await.is_empty(),
        "замок SET NX PX один"
    );
    cache
        .store(&[UserView::named(carol, "carol")], &[carol])
        .await;
    assert_eq!(
        cache.claim(&[carol]).await,
        [carol],
        "замок снят после записи"
    );
    let hit = cache.cached(&[carol]).await;
    assert_eq!(
        hit.get(&carol).map(|view| view.username.as_str()),
        Some("carol")
    );
    cache.invalidate(carol).await;
    assert!(
        cache.cached(&[carol]).await.is_empty(),
        "сброс после события"
    );
}
