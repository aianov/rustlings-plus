use std::sync::Arc;

use contracts::{Fact, Followed, Origin, ROOT_ENV, UserId, Username};
use nats_engine::NATS_URL_ENV;
use service_runtime::Source;
use user_service::repository::OutboxRepository;
use user_service::{Config, MemoryAvatars, MemoryUsers, UserError, UserService};

fn name(raw: &str) -> Username {
    Username::try_from(raw).expect("ник")
}

#[tokio::test]
async fn a_follow_becomes_a_fact_once_and_only_when_something_changed() {
    let users = Arc::new(MemoryUsers::new());
    let service = UserService::new(users.clone(), Arc::new(MemoryAvatars::default()));
    let origin = Origin::new("e856e880e9a85e6d").traced(Some("trace-7"));
    let alice = service
        .register(name("alice"), &origin)
        .await
        .expect("alice")
        .id;
    let bob = service
        .register(name("bob"), &origin)
        .await
        .expect("bob")
        .id;
    let before = users.unpublished(100).await.len();
    assert!(matches!(
        service.follow(bob, alice, &origin).await,
        Ok(true)
    ));
    assert!(matches!(
        service.follow(bob, alice, &origin).await,
        Ok(false)
    ));
    assert!(matches!(
        service.follow(alice, alice, &origin).await,
        Err(UserError::SelfFollow)
    ));
    assert!(matches!(
        service.follow(bob, UserId(99), &origin).await,
        Err(UserError::UnknownUser(_))
    ));
    let rows = users.unpublished(100).await;
    assert_eq!(
        rows.len() - before,
        1,
        "повтор, подписка на себя и на несуществующего ничего не меняют и событий не дают"
    );
    let row = rows.last().expect("строка outbox");
    assert_eq!(
        row.event.fact(),
        Ok(Some(Fact::Followed(Followed {
            follower_id: bob,
            followee_id: alice
        }))),
        "подписка записана фактом user.followed"
    );
    assert_eq!(
        (row.request_id.as_str(), row.trace_id.as_deref()),
        ("e856e880e9a85e6d", Some("trace-7")),
        "номер запроса и трасса лежат рядом с событием до самой публикации"
    );
}

#[test]
fn the_broker_and_the_root_come_from_the_environment() {
    let config = Config::from_source(&Source::from_pairs([
        (NATS_URL_ENV, "nats://127.0.0.1:4999"),
        (ROOT_ENV, "social_dev"),
    ]))
    .expect("годный конфиг");
    assert_eq!(
        config.nats.map(|nats| (nats.url, nats.name)),
        Some((
            String::from("nats://127.0.0.1:4999"),
            Some(String::from("user-service"))
        )),
        "адрес из NATS_URL, имя соединения это имя сервиса"
    );
    assert_eq!(config.topics.root(), "social_dev");
    let quiet = Config::from_source(&Source::from_pairs([(ROOT_ENV, "social")]))
        .expect("без NATS_URL сервис работает, события копятся в outbox");
    assert_eq!(quiet.nats, None);
    let crooked = Config::from_source(&Source::from_pairs([(ROOT_ENV, "social.>")]))
        .expect_err("корень с точкой и подстановкой");
    assert_eq!(crooked.keys(), [ROOT_ENV], "{crooked}");
}
