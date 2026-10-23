use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use auth_service::client::RedisActivity;
use auth_service::service::{Activity, ThrottleKey};
use resp_client::{Client, RedisConfig};

#[tokio::test]
async fn failed_logins_and_sliding_sessions_live_in_redis() {
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let activity = RedisActivity::new(
        redis.clone(),
        Duration::from_secs(60),
        Duration::from_secs(2),
    );
    let id = std::process::id();
    let account = format!("t{id}");
    let keys = vec![
        ThrottleKey::Account(account.clone()),
        ThrottleKey::Address(IpAddr::V4(Ipv4Addr::new(10, 0, (id % 250) as u8, 1))),
    ];
    activity.cleared(account.clone()).await;
    activity.failed(keys.clone()).await;
    activity.failed(vec![keys[0].clone()]).await;
    assert_eq!(
        activity.strikes(keys.clone()).await,
        2,
        "счётчик на любом экземпляре"
    );
    activity.cleared(account.clone()).await;
    assert_eq!(activity.strikes(vec![keys[0].clone()]).await, 0);
    let user = u64::from(id);
    activity.opened(user, format!("s{id}a")).await;
    activity.opened(user, format!("s{id}b")).await;
    assert!(
        activity.touched(format!("s{id}a")).await,
        "активная сессия продлевается"
    );
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert!(activity.touched(format!("s{id}a")).await, "скользящий срок");
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    assert!(
        activity.touched(format!("s{id}a")).await,
        "трогали: ещё жива"
    );
    assert!(
        !activity.touched(format!("s{id}b")).await,
        "без активности сессия истекла"
    );
    assert_eq!(activity.closed_all(user).await, 2);
    assert!(
        !activity.touched(format!("s{id}a")).await,
        "выход на всех устройствах"
    );
}
