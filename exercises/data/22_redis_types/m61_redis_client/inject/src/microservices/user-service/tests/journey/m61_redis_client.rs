use contracts::UserId;
use resp_client::{Client, RedisConfig};
use user_service::client::{RedisGuests, guests_key};
use user_service::service::{GUESTS_KEPT, Guests};

#[tokio::test]
async fn recent_guests_are_a_short_fresh_list() {
    let redis = Client::connect(RedisConfig::from_env());
    redis.ping().await.expect("Redis курса на 127.0.0.1:56379");
    let guests = RedisGuests::new(redis.clone());
    let owner = UserId(u64::from(std::process::id()) * 10);
    redis.del(&[guests_key(owner)]).await.expect("чисто");
    for guest in 1..=12 {
        guests.visited(owner, UserId(guest)).await;
    }
    guests.visited(owner, UserId(5)).await;
    let recent = guests.recent(owner, GUESTS_KEPT).await;
    assert_eq!(recent.len(), GUESTS_KEPT, "LTRIM держит десять");
    assert_eq!(
        recent[0],
        UserId(5),
        "повторный гость поднялся наверх, без дубля"
    );
    assert_eq!(
        recent.iter().filter(|guest| **guest == UserId(5)).count(),
        1
    );
    assert!(!recent.contains(&UserId(1)), "самые старые ушли");
    redis.del(&[guests_key(owner)]).await.expect("уборка");
}
